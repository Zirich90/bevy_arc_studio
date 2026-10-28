//! Asset Browser thumbnails: a model is loaded on a private render layer, framed by its bounds and rendered by a
//! temporary camera into a small `Image` that egui shows on the card. One job at a time, only for cards that scroll
//! into view; the world cameras never see the preview (layer `PREVIEW_LAYER`).

use std::collections::{HashMap, VecDeque};

use bevy::asset::LoadState;
use bevy::camera::RenderTarget;
use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::ClearColorConfig;
use bevy::light::DirectionalLight;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::app::{HierarchyPropagatePlugin, Propagate};
use bevy_egui::{EguiTextureHandle, EguiUserTextures, egui};

pub const PREVIEW_LAYER: usize = 7;
pub const THUMB_PX: u32 = 128;
/// Frames the camera keeps rendering after framing, so textures and materials have time to stream in.
const SETTLE_FRAMES: u32 = 30;
/// The first previews wait longer: render pipelines compile asynchronously and draw nothing until ready.
const WARMUP_JOBS: usize = 3;
const WARMUP_FRAMES: u32 = 120;
/// Give up on a model that never produces geometry.
const MAX_FRAMES: u32 = 900;

pub enum Thumb {
    Queued,
    Ready(egui::TextureId),
    Failed,
}

/// Marks the temporary preview camera so editor-wide camera systems leave it alone.
#[derive(Component)]
pub struct ThumbnailCamera;

struct Job {
    key: String,
    root: Entity,
    cam: Entity,
    light: Entity,
    image: Handle<Image>,
    scene: bevy::asset::UntypedAssetId,
    frames: u32,
    settle: Option<u32>,
}

#[derive(Resource, Default)]
pub struct Thumbnails {
    map: HashMap<String, Thumb>,
    queue: VecDeque<String>,
    job: Option<Job>,
    finished: usize,
}

impl Thumbnails {
    /// Texture of a model's thumbnail (path under `assets/`); queues rendering the first time it is asked for.
    pub fn request(&mut self, key: &str) -> Option<egui::TextureId> {
        match self.map.get(key) {
            Some(Thumb::Ready(id)) => Some(*id),
            Some(_) => None,
            None => {
                self.map.insert(key.to_string(), Thumb::Queued);
                self.queue.push_back(key.to_string());
                None
            }
        }
    }

    pub fn ready_count(&self) -> usize {
        self.map.values().filter(|t| matches!(t, Thumb::Ready(_))).count()
    }
}

/// Camera position and clip range that fit a bounding box in view (vertical fov `fov`).
pub fn frame_bounds(min: Vec3, max: Vec3, fov: f32) -> (Vec3, f32) {
    let center = (min + max) * 0.5;
    let radius = ((max - min).length() * 0.5).max(0.01);
    let dist = radius / (fov * 0.5).sin() * 1.08;
    (center + Vec3::new(0.8, 0.55, 1.0).normalize() * dist, dist)
}

fn world_bounds(root: Entity, children: &Query<&Children>, boxes: &Query<(&Aabb, &GlobalTransform)>) -> Option<(Vec3, Vec3)> {
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    let mut any = false;
    for e in children.iter_descendants(root) {
        let Ok((aabb, gtf)) = boxes.get(e) else { continue };
        let (c, h) = (Vec3::from(aabb.center), Vec3::from(aabb.half_extents));
        for i in 0..8u8 {
            let s = Vec3::new(if i & 1 == 0 { -1.0 } else { 1.0 }, if i & 2 == 0 { -1.0 } else { 1.0 }, if i & 4 == 0 { -1.0 } else { 1.0 });
            let p = gtf.transform_point(c + h * s);
            lo = lo.min(p);
            hi = hi.max(p);
        }
        any = true;
    }
    any.then_some((lo, hi))
}

#[allow(clippy::too_many_arguments)]
fn thumbnail_system(
    mut commands: Commands,
    mut th: ResMut<Thumbnails>,
    mut images: ResMut<Assets<Image>>,
    mut egui_tex: ResMut<EguiUserTextures>,
    server: Res<AssetServer>,
    children: Query<&Children>,
    boxes: Query<(&Aabb, &GlobalTransform)>,
    mut cams: Query<&mut Transform>,
) {
    if th.job.is_none() {
        let Some(key) = th.queue.pop_front() else { return };
        let mut img = Image::new_target_texture(THUMB_PX, THUMB_PX, TextureFormat::Rgba8UnormSrgb, None);
        img.texture_descriptor.usage |= bevy::render::render_resource::TextureUsages::COPY_SRC;
        let image = images.add(img);
        let layer = RenderLayers::layer(PREVIEW_LAYER);
        let root = WorldAssetRoot(server.load(GltfAssetLabel::Scene(0).from_asset(key.clone())));
        let scene = root.0.id().untyped();
        let origin = Vec3::new(0.0, -4000.0, 0.0);
        let root = commands.spawn((root, Transform::from_translation(origin), Visibility::default(), Propagate(layer.clone()))).id();
        let cam = commands
            .spawn((
                ThumbnailCamera,
                Camera3d::default(),
                Camera { order: -20, clear_color: ClearColorConfig::Custom(Color::srgb(0.32, 0.36, 0.42)), ..default() },
                RenderTarget::from(image.clone()),
                Transform::from_translation(origin + Vec3::new(0.0, 2.0, 6.0)).looking_at(origin, Vec3::Y),
                layer.clone(),
            ))
            .id();
        let light = commands
            .spawn((DirectionalLight { illuminance: 14_000.0, shadow_maps_enabled: false, ..default() }, Transform::from_translation(origin + Vec3::new(3.0, 6.0, 4.0)).looking_at(origin, Vec3::Y), layer))
            .id();
        th.job = Some(Job { key, root, cam, light, image, scene, frames: 0, settle: None });
        return;
    }
    let th = &mut *th;
    let Some(job) = th.job.as_mut() else { return };
    job.frames += 1;
    let failed = matches!(server.get_load_state(job.scene), Some(LoadState::Failed(_))) || job.frames > MAX_FRAMES;
    if job.settle.is_none() && !failed {
        if let Some((lo, hi)) = world_bounds(job.root, &children, &boxes) {
            let (pos, dist) = frame_bounds(lo, hi, std::f32::consts::FRAC_PI_4);
            if let Ok(mut tf) = cams.get_mut(job.cam) {
                *tf = Transform::from_translation(pos).looking_at((lo + hi) * 0.5, Vec3::Y);
            }
            if let Ok(mut lt) = cams.get_mut(job.light) {
                *lt = Transform::from_translation((lo + hi) * 0.5 + Vec3::new(3.0, 6.0, 4.0).normalize() * dist).looking_at((lo + hi) * 0.5, Vec3::Y);
            }
            job.settle = Some(SETTLE_FRAMES + if th.finished < WARMUP_JOBS { WARMUP_FRAMES } else { 0 });
        }
    }
    let done = failed || job.settle.is_some_and(|n| n == 0);
    if let Some(n) = job.settle.as_mut() {
        *n = n.saturating_sub(1);
    }
    if !done {
        return;
    }
    let job = th.job.take().expect("job checked above");
    th.finished += 1;
    for e in [job.root, job.cam, job.light] {
        commands.entity(e).despawn();
    }
    if failed {
        images.remove(&job.image);
        th.map.insert(job.key, Thumb::Failed);
    } else {
        let id = egui_tex.add_image(EguiTextureHandle::Strong(job.image));
        th.map.insert(job.key, Thumb::Ready(id));
    }
}

pub struct ThumbnailsPlugin;

impl Plugin for ThumbnailsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Thumbnails>().add_plugins(HierarchyPropagatePlugin::<RenderLayers>::new(PostUpdate)).add_systems(Update, thumbnail_system);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framing_keeps_the_box_inside_the_view() {
        let (min, max) = (Vec3::new(-1.0, 0.0, -1.0), Vec3::new(1.0, 2.0, 1.0));
        let fov = std::f32::consts::FRAC_PI_4;
        let (pos, dist) = frame_bounds(min, max, fov);
        let center = (min + max) * 0.5;
        assert!((pos.distance(center) - dist).abs() < 1e-4);
        let radius = (max - min).length() * 0.5;
        assert!(dist * (fov * 0.5).sin() > radius, "bounding sphere must fit the frustum");
        let (_, far) = frame_bounds(min * 10.0, max * 10.0, fov);
        assert!(far > dist * 9.0);
    }

    #[test]
    fn requests_queue_once_per_key() {
        let mut t = Thumbnails::default();
        assert!(t.request("models/a.glb").is_none());
        assert!(t.request("models/a.glb").is_none());
        assert!(t.request("models/b.glb").is_none());
        assert_eq!(t.queue.len(), 2);
        t.map.insert("models/a.glb".into(), Thumb::Ready(egui::TextureId::User(3)));
        assert_eq!(t.request("models/a.glb"), Some(egui::TextureId::User(3)));
        assert_eq!(t.ready_count(), 1);
    }
}
