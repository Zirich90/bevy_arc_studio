//! Startup scene: sun/moon, fog volume and editor camera (terrain, water and props live in their own modules).

use bevy::light::atmosphere::ScatteringMedium;
use bevy::light::{Atmosphere, FogVolume, SunDisk, VolumetricLight};
use bevy::prelude::*;

use crate::arc_core::*;
use crate::camera::EditorCamera;
use crate::lighting::{self, Moon, Sun};
pub use crate::terrain_render::TerrainGrid;

pub struct ScenePlugin;

impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostStartup, setup_scene);
    }
}

fn setup_scene(
    mut commands: Commands,
    spec: Res<WorldSpec>,
    grid: Res<TerrainGrid>,
    mut media: ResMut<Assets<ScatteringMedium>>,
) {
    let grid = &grid.0;
    commands.spawn((
        Name::new("Sun"),
        Sun,
        DirectionalLight { shadow_maps_enabled: spec.shadows, illuminance: 100_000.0, ..default() },
        SunDisk::EARTH,
        VolumetricLight,
        lighting::cascades(),
        Transform::default().looking_to(Vec3::NEG_Y, Vec3::X),
    ));
    commands.spawn((
        Name::new("Moon"),
        Moon,
        DirectionalLight { color: Color::srgb(0.55, 0.65, 1.0), illuminance: 0.0, shadow_maps_enabled: false, ..default() },
        Transform::default().looking_to(Vec3::NEG_Y, Vec3::X),
    ));
    commands.spawn((
        Name::new("FogVolume"),
        FogVolume { density_factor: 0.02, scattering: 0.4, absorption: 0.05, ..default() },
        Transform::from_xyz(0.0, 40.0, 0.0).with_scale(Vec3::new(grid.size, 120.0, grid.size)),
    ));

    let cp = spec.camp_pos();
    let camp = Vec3::new(cp.x, grid.sample_height(cp.x, cp.y), cp.y);
    let mut cam = EditorCamera { pivot: camp + Vec3::new(-14.0, 2.0, 8.0), distance: 34.0, yaw: -1.2, pitch: 0.14, ..default() };
    // dev aid: ARC_CAM="px,py,pz,yaw,pitch,distance"
    if let Some(v) = std::env::var("ARC_CAM").ok().map(|s| s.split(',').filter_map(|t| t.trim().parse::<f32>().ok()).collect::<Vec<_>>()).filter(|v| v.len() == 6) {
        (cam.pivot, cam.yaw, cam.pitch, cam.distance) = (Vec3::new(v[0], v[1], v[2]), v[3], v[4], v[5]);
        // keep the eye above the terrain
        let fwd = Vec3::new(cam.pitch.cos() * cam.yaw.sin(), -cam.pitch.sin(), cam.pitch.cos() * cam.yaw.cos());
        let eye = cam.pivot - fwd * cam.distance;
        let min_y = grid.sample_height(eye.x, eye.z) + 3.0;
        info!("ARC_CAM eye {eye:?}, ground {:.1}", min_y - 3.0);
        cam.pivot.y += (min_y - eye.y).max(0.0);
    }
    let medium = media.add(ScatteringMedium::earth(256, 256));
    let atmosphere = Atmosphere::earth(medium);
    commands.spawn((Name::new("Planet"), Transform::from_xyz(0.0, -atmosphere.inner_radius, 0.0), atmosphere));
    commands.spawn((
        Name::new("EditorCamera"),
        Camera3d::default(),
        cam,
        Transform::default(),
        lighting::camera_stack(&spec),
    ));
}
