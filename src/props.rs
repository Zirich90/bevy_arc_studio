//! Procedural prop forge (pines, boulders, reeds, grass, pier, campsite) + auto-loading of `assets/models/*.glb`.

use std::f32::consts::{FRAC_PI_2, TAU};
use std::sync::Arc;

use bevy::asset::RenderAssetUsages;
use bevy::light::{NotShadowCaster, VolumetricLight};
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use noise::NoiseFn;
use serde::{Deserialize, Serialize};

use crate::arc_core::*;
use crate::lighting::NightLight;
use crate::terrain_render::{TerrainGrid, TerrainVersion};

// ───────────────────────────── Components ─────────────────────────────

#[derive(Reflect, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ObjectCategory {
    #[default]
    Vegetation,
    Rock,
    Structure,
    Camp,
    Model,
    /// Game character (NPC / hero) provided by a rules plugin.
    Actor,
    /// Gameplay trigger volume provided by a rules plugin.
    Trigger,
    /// Modular level tile (`LevelTile`); navigation footprints come from the tile, not the category.
    Level,
}

/// Selectable root of every spawned object (stage 3 picking / inspector hook).
#[derive(Component, Reflect, Serialize, Deserialize, Clone, Debug)]
#[reflect(Component)]
pub struct SceneObject {
    pub id: u32,
    pub name: String,
    pub category: ObjectCategory,
    pub aabb_half_extents: Vec3,
}

/// Marks everything the props system owns and may despawn on terrain rebuild.
#[derive(Component, Clone)]
pub struct PropRoot;

#[derive(Component, Clone)]
struct Flame {
    phase: f32,
    base: Vec3,
}

#[derive(Component, Clone)]
struct Spark {
    age: f32,
    life: f32,
    vel: Vec3,
    origin: Vec3,
    seed: f32,
}

#[derive(Component, Clone)]
struct Bob {
    phase: f32,
    base_y: f32,
    yaw: f32,
}

// ───────────────────────────── Resources ─────────────────────────────

struct Palette {
    veg: Handle<StandardMaterial>,
    rock: Handle<StandardMaterial>,
    wood: Handle<StandardMaterial>,
    blade: Handle<StandardMaterial>,
    canvas: Handle<StandardMaterial>,
    lantern_glass: Handle<StandardMaterial>,
    flame: [Handle<StandardMaterial>; 3],
    ember: Handle<StandardMaterial>,
    spark: Handle<StandardMaterial>,
}

#[derive(Resource)]
pub struct PropAssets {
    pines: Vec<Handle<Mesh>>,
    rocks: Vec<Handle<Mesh>>,
    grass: Vec<Handle<Mesh>>,
    reeds: Vec<Handle<Mesh>>,
    tent: Handle<Mesh>,
    flame_cone: Handle<Mesh>,
    spark_mesh: Handle<Mesh>,
    mats: Palette,
}

/// `.glb` files discovered in `assets/models/`.
#[derive(Resource, Default)]
pub struct ModelLibrary {
    pub models: Vec<(String, Handle<WorldAsset>)>,
}

/// Context handed to a [`SpawnKinds`] handler: where to put the object and the asset stores to build it from.
pub struct SpawnCtx<'a, 'w, 's> {
    pub commands: &'a mut Commands<'w, 's>,
    pub meshes: &'a mut Assets<Mesh>,
    pub mats: &'a mut Assets<StandardMaterial>,
    pub server: &'a AssetServer,
    /// Spawn transform (terrain point + yaw).
    pub tf: Transform,
    /// Id reserved for this object's `SceneObject`.
    pub id: u32,
}

impl SpawnCtx<'_, '_, '_> {
    /// Selectable root component for the object this handler spawns.
    pub fn scene_object(&self, name: &str, category: ObjectCategory, half: Vec3) -> SceneObject {
        SceneObject { id: self.id, name: name.to_string(), category, aabb_half_extents: half }
    }
}

pub type SpawnHandler = Arc<dyn Fn(&mut SpawnCtx<'_, '_, '_>, &str) -> Option<Entity> + Send + Sync>;

/// Spawn handlers for `UserProp::kind` strings the editor does not know itself (`SpawnObject { kind }`).
/// A key ending in `:` matches by prefix (`"actor:"` handles `"actor:hero_fighter"`), any other key matches exactly.
/// Handlers of an inactive module (see `modules`) are skipped.
#[derive(Resource, Default, Clone)]
pub struct SpawnKinds {
    entries: Vec<(&'static str, SpawnHandler, Option<String>)>,
    pub scope: Option<String>,
    pub inactive: std::collections::HashSet<String>,
}

impl SpawnKinds {
    pub fn register(&mut self, key: &'static str, handler: SpawnHandler) {
        match self.entries.iter_mut().find(|(k, ..)| *k == key) {
            Some(e) => (e.1, e.2) = (handler, self.scope.clone()),
            None => self.entries.push((key, handler, self.scope.clone())),
        }
    }

    pub fn find(&self, kind: &str) -> Option<SpawnHandler> {
        self.entries
            .iter()
            .find(|(k, _, o)| (if k.ends_with(':') { kind.starts_with(k) } else { kind == *k }) && o.as_ref().is_none_or(|o| !self.inactive.contains(o)))
            .map(|(_, h, _)| h.clone())
    }
}

pub trait SpawnKindAppExt {
    fn register_spawn_kind(&mut self, key: &'static str, handler: impl Fn(&mut SpawnCtx<'_, '_, '_>, &str) -> Option<Entity> + Send + Sync + 'static) -> &mut Self;
}

impl SpawnKindAppExt for App {
    fn register_spawn_kind(&mut self, key: &'static str, handler: impl Fn(&mut SpawnCtx<'_, '_, '_>, &str) -> Option<Entity> + Send + Sync + 'static) -> &mut Self {
        self.world_mut().get_resource_or_insert_with(SpawnKinds::default).register(key, Arc::new(handler));
        self
    }
}

pub struct PropsPlugin;

impl Plugin for PropsPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<SceneObject>()
            .register_type::<ObjectCategory>()
            .init_resource::<ModelLibrary>()
            .init_resource::<SpawnKinds>()
            .init_resource::<NextObjectId>()
            .register_type::<UserProp>()
            .add_observer(strip_model_cameras)
            .add_systems(Startup, (build_assets, load_models).chain())
            .add_systems(Update, (sync_props, animate_flames, animate_sparks, animate_bobbing));
    }
}

/// glTF models may embed cameras; only the editor camera may render to the window.
/// An observer strips them at insertion, before the first extract sees two active cameras.
fn strip_model_cameras(add: On<Add<Camera>>, editor: Query<(), With<crate::camera::EditorCamera>>, mut commands: Commands) {
    if !editor.contains(add.entity) {
        commands.entity(add.entity).remove::<(Camera, Camera3d)>();
    }
}

// ───────────────────────────── Mesh helpers ─────────────────────────────

fn hash01(n: u32) -> f32 {
    let mut x = n.wrapping_mul(0x9E37_79B1) ^ 0x85EB_CA6B;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    x = x.wrapping_mul(0x297A_2D39);
    x ^= x >> 15;
    (x & 0xFFFFFF) as f32 / 0x1000000 as f32
}

fn rgb(r: f32, g: f32, b: f32) -> [f32; 4] {
    [r, g, b, 1.0]
}

fn mulc(c: [f32; 4], k: f32) -> [f32; 4] {
    [c[0] * k, c[1] * k, c[2] * k, c[3]]
}

fn mixc(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    let t = t.clamp(0.0, 1.0);
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t, 1.0]
}

fn cyl(r: f32, h: f32, res: u32) -> Mesh {
    Cylinder::new(r, h).mesh().resolution(res).into()
}
fn cone(r: f32, h: f32, res: u32) -> Mesh {
    Cone::new(r, h).mesh().resolution(res).into()
}
fn cube(x: f32, y: f32, z: f32) -> Mesh {
    Cuboid::new(x, y, z).into()
}

/// Accumulates transformed, vertex-coloured primitives into one mesh.
#[derive(Default)]
struct MeshAcc {
    pos: Vec<[f32; 3]>,
    nrm: Vec<[f32; 3]>,
    col: Vec<[f32; 4]>,
    idx: Vec<u32>,
}

impl MeshAcc {
    fn push(&mut self, mesh: &Mesh, tf: Transform, warp: impl Fn(Vec3) -> Vec3, color: impl Fn(Vec3, [f32; 4]) -> [f32; 4]) {
        let p = mesh.attribute(Mesh::ATTRIBUTE_POSITION).and_then(|a| a.as_float3()).expect("positions");
        let n = mesh.attribute(Mesh::ATTRIBUTE_NORMAL).and_then(|a| a.as_float3()).expect("normals");
        let c = match mesh.attribute(Mesh::ATTRIBUTE_COLOR) {
            Some(VertexAttributeValues::Float32x4(v)) => Some(v.as_slice()),
            _ => None,
        };
        let base = self.pos.len() as u32;
        for k in 0..p.len() {
            let lp = Vec3::from(p[k]);
            self.pos.push(tf.transform_point(warp(lp)).to_array());
            self.nrm.push((tf.rotation * (Vec3::from(n[k]) / tf.scale)).normalize_or_zero().to_array());
            self.col.push(color(lp, c.map_or([1.0; 4], |c| c[k])));
        }
        match mesh.indices() {
            Some(i) => self.idx.extend(i.iter().map(|i| base + i as u32)),
            None => self.idx.extend((0..p.len() as u32).map(|i| base + i)),
        }
    }

    fn plain(&mut self, mesh: &Mesh, tf: Transform, col: [f32; 4]) {
        self.push(mesh, tf, |p| p, |_, _| col);
    }

    fn build(self) -> Mesh {
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.pos)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.nrm)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.col)
            .with_inserted_indices(Indices::U32(self.idx))
    }
}

/// Five-tier pine: bark trunk + ragged needle tiers with per-variant tone and scale.
fn pine_mesh(variant: u32) -> Mesh {
    let mut acc = MeshAcc::default();
    let v = variant * 17 + 3;
    let bark = rgb(0.075 + 0.02 * hash01(v), 0.040, 0.022);
    acc.push(&cyl(0.26, 3.4, 8), Transform::from_xyz(0.0, 1.7, 0.0), |p| p * Vec3::new(1.0 - 0.35 * (p.y + 1.7) / 3.4, 1.0, 1.0 - 0.35 * (p.y + 1.7) / 3.4), |p, _| mulc(bark, 0.7 + 0.5 * (p.y + 1.7) / 3.4));
    let hue = hash01(v + 1);
    let dark = rgb(0.014 + 0.012 * hue, 0.075 + 0.04 * hue, 0.03);
    let light = rgb(0.045 + 0.03 * hue, 0.17 + 0.05 * hue, 0.05);
    for k in 0..5u32 {
        let t = k as f32 / 4.0;
        let radius = (2.15 - 1.35 * t) * (0.92 + 0.16 * hash01(v + 10 + k));
        let height = 2.6 - 0.8 * t;
        let base_y = 1.7 + k as f32 * 1.12;
        let tier_tone = 0.8 + 0.4 * hash01(v + 20 + k);
        let seed = (v + k) as f32;
        let h = height;
        acc.push(
            &cone(radius, height, 14),
            Transform::from_xyz(0.0, base_y + height * 0.5, 0.0).with_rotation(Quat::from_rotation_y(hash01(v + 30 + k) * TAU)),
            move |p| {
                // ragged skirt: radial wobble strongest at the base ring
                let w = (1.0 - (p.y + h * 0.5) / h).clamp(0.0, 1.0);
                let a = p.z.atan2(p.x);
                let f = 1.0 + 0.22 * w * ((a * 5.0 + seed).sin() * 0.6 + (a * 11.0 + seed * 2.3).sin() * 0.4);
                Vec3::new(p.x * f, p.y - 0.12 * w * (a * 7.0 + seed).sin().abs(), p.z * f)
            },
            move |p, _| mixc(mulc(dark, tier_tone), mulc(light, tier_tone), (p.y + h * 0.5) / h * 0.9 + 0.1),
        );
    }
    acc.plain(&cone(0.16, 1.1, 6), Transform::from_xyz(0.0, 1.7 + 4.0 * 1.12 + 2.0, 0.0), dark);
    acc.build()
}

/// Faceted noise-deformed boulder; upward faces turn to moss.
fn rock_mesh(seed: u32) -> Mesh {
    let mut m: Mesh = Sphere::new(1.0).mesh().ico(2).expect("ico");
    m.duplicate_vertices();
    let perlin = noise::Perlin::new(seed);
    let s = seed as f64 * 3.7;
    if let Some(VertexAttributeValues::Float32x3(pos)) = m.attribute_mut(Mesh::ATTRIBUTE_POSITION) {
        for p in pos.iter_mut() {
            let d = Vec3::from(*p).normalize();
            let q = [d.x as f64 * 1.4 + s, d.y as f64 * 1.4, d.z as f64 * 1.4];
            let q2 = [d.x as f64 * 3.7 + s, d.y as f64 * 3.7, d.z as f64 * 3.7];
            let r = 1.0 + 0.30 * perlin.get(q) as f32 + 0.09 * perlin.get(q2) as f32;
            let v = d * r * Vec3::new(1.2, 0.85, 1.0);
            *p = [v.x, v.y.max(-0.55), v.z];
        }
    }
    m.compute_flat_normals();
    let nrm: Vec<[f32; 3]> = m.attribute(Mesh::ATTRIBUTE_NORMAL).and_then(|a| a.as_float3()).unwrap().to_vec();
    let pos: Vec<[f32; 3]> = m.attribute(Mesh::ATTRIBUTE_POSITION).and_then(|a| a.as_float3()).unwrap().to_vec();
    let mut col = Vec::with_capacity(pos.len());
    for (k, n) in nrm.iter().enumerate() {
        let p = pos[k];
        let n_i = perlin.get([p[0] as f64 * 2.3 + s, p[1] as f64 * 2.3, p[2] as f64 * 2.3]) as f32;
        let moss = ((n[1] + 0.25 * n_i - 0.38) / 0.35).clamp(0.0, 1.0);
        let tone = 0.75 + 0.35 * (0.5 + 0.5 * n_i);
        let stone = rgb(0.14 * tone, 0.14 * tone, 0.15 * tone);
        let green = rgb(0.02 + 0.012 * n_i.abs(), 0.065 + 0.03 * n_i.abs(), 0.022);
        col.push(mixc(stone, green, moss));
    }
    m.insert_attribute(Mesh::ATTRIBUTE_COLOR, col);
    m
}

/// Clump of curved grass/reed blades (one mesh, vertex-coloured base→tip).
fn blade_clump(seed: u32, count: u32, height: f32, width: f32, base: [f32; 4], tip: [f32; 4]) -> Mesh {
    let (mut pos, mut nrm, mut col, mut idx) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for b in 0..count {
        let r = |k: u32| hash01(seed * 1000 + b * 13 + k);
        let (a, dist) = (r(0) * TAU, r(1).sqrt() * 0.22);
        let origin = Vec3::new(a.cos() * dist, 0.0, a.sin() * dist);
        let lean = r(2) * TAU;
        let bend = Vec3::new(lean.cos(), 0.0, lean.sin()) * (0.18 + 0.45 * r(3)) * height;
        let h = height * (0.65 + 0.5 * r(4));
        let side = Vec3::new(-lean.sin(), 0.0, lean.cos()) * width * 0.5;
        let facing = Vec3::new(lean.cos(), 0.0, lean.sin());
        let normal = (facing * 0.45 + Vec3::Y * 0.9).normalize().to_array();
        let base_i = pos.len() as u32;
        let rings = [(0.0_f32, 1.0_f32), (0.5, 0.75), (1.0, 0.0)];
        for (k, (t, w)) in rings.iter().enumerate() {
            let centre = origin + Vec3::Y * h * *t + bend * t * t;
            let c = mixc(mulc(base, 0.7 + 0.5 * r(5)), tip, *t);
            if k == 2 {
                pos.push(centre.to_array());
                nrm.push(normal);
                col.push(c);
            } else {
                pos.push((centre - side * *w).to_array());
                pos.push((centre + side * *w).to_array());
                nrm.extend([normal, normal]);
                col.extend([c, c]);
            }
        }
        idx.extend_from_slice(&[base_i, base_i + 1, base_i + 3, base_i, base_i + 3, base_i + 2, base_i + 2, base_i + 3, base_i + 4]);
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nrm)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col)
        .with_inserted_indices(Indices::U32(idx))
}

/// A-frame tent with a darker entrance wall.
fn tent_mesh() -> Mesh {
    let (hx, hz, hy) = (1.45, 1.15, 1.55);
    let b = [Vec3::new(-hx, 0.0, -hz), Vec3::new(hx, 0.0, -hz), Vec3::new(hx, 0.0, hz), Vec3::new(-hx, 0.0, hz)];
    let r = [Vec3::new(-hx, hy, 0.0), Vec3::new(hx, hy, 0.0)];
    let canvas = rgb(0.30, 0.17, 0.065);
    let tris: [([Vec3; 3], [f32; 4]); 6] = [
        ([b[0], b[1], r[1]], canvas),
        ([b[0], r[1], r[0]], canvas),
        ([b[2], b[3], r[0]], mulc(canvas, 0.9)),
        ([b[2], r[0], r[1]], mulc(canvas, 0.9)),
        ([b[3], b[0], r[0]], mulc(canvas, 0.7)),
        ([b[1], b[2], r[1]], rgb(0.025, 0.018, 0.012)),
    ];
    let mut pos = Vec::new();
    let mut col = Vec::new();
    for (t, c) in tris {
        for v in t {
            pos.push(v.to_array());
            col.push(c);
        }
    }
    let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col);
    m.compute_flat_normals();
    m
}

// ───────────────────────────── Setup ─────────────────────────────

fn build_assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let mut vc = |rough: f32, double: bool| {
        mats.add(StandardMaterial { base_color: Color::WHITE, perceptual_roughness: rough, reflectance: 0.2, double_sided: double, cull_mode: if double { None } else { Some(bevy::render::render_resource::Face::Back) }, ..default() })
    };
    let (veg, rock, wood, blade, canvas) = (vc(0.9, false), vc(0.85, false), vc(0.8, false), vc(0.7, true), vc(0.9, true));
    let mut glow = |emissive: LinearRgba, alpha: AlphaMode| mats.add(StandardMaterial { base_color: Color::srgba(0.0, 0.0, 0.0, 0.9), emissive, alpha_mode: alpha, ..default() });
    let flame = [glow(LinearRgba::rgb(9.0, 1.4, 0.15), AlphaMode::Add), glow(LinearRgba::rgb(22.0, 7.0, 0.5), AlphaMode::Add), glow(LinearRgba::rgb(40.0, 26.0, 5.0), AlphaMode::Add)];
    let ember = glow(LinearRgba::rgb(7.0, 1.3, 0.15), AlphaMode::Opaque);
    let spark = glow(LinearRgba::rgb(30.0, 12.0, 1.5), AlphaMode::Opaque);
    let lantern_glass = glow(LinearRgba::rgb(14.0, 7.0, 1.6), AlphaMode::Opaque);

    let a = PropAssets {
        pines: (0..4).map(|i| meshes.add(pine_mesh(i))).collect(),
        rocks: (0..5).map(|i| meshes.add(rock_mesh(11 + i * 7))).collect(),
        grass: (0..3).map(|i| meshes.add(blade_clump(i + 1, 16, 0.55, 0.06, rgb(0.03, 0.10, 0.02), rgb(0.16, 0.34, 0.07)))).collect(),
        reeds: (0..3).map(|i| meshes.add(blade_clump(i + 40, 10, 1.9, 0.07, rgb(0.07, 0.16, 0.04), rgb(0.34, 0.31, 0.12)))).collect(),
        tent: meshes.add(tent_mesh()),
        flame_cone: meshes.add(cone(1.0, 1.0, 10)),
        spark_mesh: meshes.add(Sphere::new(0.028).mesh().ico(1).expect("ico")),
        mats: Palette { veg, rock, wood, blade, canvas, lantern_glass, flame, ember, spark },
    };
    commands.insert_resource(a);
}

pub fn assets_dir() -> std::path::PathBuf {
    let env = ["BEVY_ASSET_ROOT", "CARGO_MANIFEST_DIR"].into_iter().filter_map(|k| std::env::var(k).ok()).map(|p| std::path::PathBuf::from(p).join("assets"));
    let cwd = std::env::current_dir().ok().map(|p| p.join("assets"));
    let exe = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.join("assets")));
    env.chain(cwd).chain(exe.clone()).find(|p| p.is_dir()).or(exe).unwrap_or_else(|| "assets".into())
}

/// `Duck.glb` → `models/Duck.glb`; a path with a folder (`showcase/castle/gate.glb`) is relative to `assets/`.
pub fn model_asset_path(file: &str) -> String {
    if file.contains('/') { file.to_string() } else { format!("models/{file}") }
}

/// Discover `assets/models/*.glb` / `.gltf` and request the first scene of each through the AssetServer.
fn load_models(server: Res<AssetServer>, mut lib: ResMut<ModelLibrary>) {
    let Ok(rd) = std::fs::read_dir(assets_dir().join("models")) else { return };
    let mut files: Vec<String> = rd
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| matches!(n.rsplit('.').next().map(str::to_ascii_lowercase).as_deref(), Some("glb" | "gltf")))
        .collect();
    files.sort();
    for f in files {
        let handle = server.load(GltfAssetLabel::Scene(0).from_asset(format!("models/{f}")));
        lib.models.push((f.rsplit_once('.').map_or(f.clone(), |(s, _)| s.to_string()), handle));
    }
}

// ───────────────────────────── Spawning ─────────────────────────────

struct Ctx<'a> {
    grid: &'a HeightfieldGrid,
    spec: &'a WorldSpec,
    next_id: u32,
}

impl Ctx<'_> {
    fn obj(&mut self, name: &str, category: ObjectCategory, half: Vec3) -> (PropRoot, SceneObject) {
        self.next_id += 1;
        (PropRoot, SceneObject { id: self.next_id, name: name.to_string(), category, aabb_half_extents: half })
    }
}

#[allow(clippy::too_many_arguments)]
fn sync_props(
    mut commands: Commands,
    version: Res<TerrainVersion>,
    mut seen: Local<Option<u32>>,
    spec: Res<WorldSpec>,
    grid: Res<TerrainGrid>,
    assets: Option<Res<PropAssets>>,
    lib: Res<ModelLibrary>,
    old: Query<Entity, With<PropRoot>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    let Some(assets) = assets else { return };
    if *seen == Some(version.0) {
        return;
    }
    *seen = Some(version.0);
    for e in &old {
        commands.entity(e).despawn();
    }
    let mut cx = Ctx { grid: &grid.0, spec: &spec, next_id: 0 };
    for (n, it) in generate_scatter_plan(&spec, &grid.0).iter().enumerate() {
        let tf = Transform { translation: it.pos, rotation: it.rot, scale: Vec3::splat(it.scale) };
        let n = n as u32;
        match it.kind {
            ScatterKind::Pine => {
                let o = cx.obj("PineTree", ObjectCategory::Vegetation, Vec3::new(2.0, 4.2, 2.0) * it.scale);
                let h = assets.pines[(hash01(n) * assets.pines.len() as f32) as usize % assets.pines.len()].clone();
                commands.spawn((Name::new("PineTree"), o, Mesh3d(h), MeshMaterial3d(assets.mats.veg.clone()), tf));
            }
            ScatterKind::Boulder => {
                let o = cx.obj("Boulder", ObjectCategory::Rock, Vec3::new(1.2, 0.8, 1.0) * it.scale);
                let h = assets.rocks[(hash01(n + 7) * assets.rocks.len() as f32) as usize % assets.rocks.len()].clone();
                commands.spawn((Name::new("Boulder"), o, Mesh3d(h), MeshMaterial3d(assets.mats.rock.clone()), tf));
            }
            ScatterKind::Grass => {
                let o = cx.obj("GrassClump", ObjectCategory::Vegetation, Vec3::new(0.3, 0.35, 0.3) * it.scale);
                let h = assets.grass[n as usize % assets.grass.len()].clone();
                commands.spawn((Name::new("GrassClump"), o, Mesh3d(h), MeshMaterial3d(assets.mats.blade.clone()), tf, NotShadowCaster));
            }
            ScatterKind::Reed => {
                let o = cx.obj("Reeds", ObjectCategory::Vegetation, Vec3::new(0.35, 1.0, 0.35) * it.scale);
                let h = assets.reeds[n as usize % assets.reeds.len()].clone();
                commands.spawn((Name::new("Reeds"), o, Mesh3d(h), MeshMaterial3d(assets.mats.blade.clone()), tf, NotShadowCaster));
            }
            ScatterKind::WoodenPier => {
                spawn_pier(&mut commands, &mut cx, &assets, &mut meshes, it);
            }
            ScatterKind::CampfireSite => {
                spawn_camp(&mut commands, &mut cx, &assets, &mut meshes, &mut mats, it);
            }
        }
    }
    // external models: ring around the camp, ground-snapped
    let camp = spec.camp_pos();
    for (i, (name, scene)) in lib.models.iter().enumerate() {
        let a = 0.6 + i as f32 * 0.9;
        let p = camp + Vec2::new(a.cos(), a.sin()) * 9.0;
        let o = cx.obj(name, ObjectCategory::Model, Vec3::splat(1.0));
        commands.spawn((Name::new(name.clone()), o, WorldAssetRoot(scene.clone()), Transform::from_xyz(p.x, grid.0.sample_height(p.x, p.y), p.y)));
    }
}

fn mul_tf(pos: Vec3, rot: Quat) -> Transform {
    Transform { translation: pos, rotation: rot, scale: Vec3::ONE }
}

fn spawn_pier(commands: &mut Commands, cx: &mut Ctx, assets: &PropAssets, meshes: &mut Assets<Mesh>, it: &ScatterItem) -> Entity {
    let len = it.scale;
    let root = it.pos;
    let rot = it.rot;
    let ground_rel = |x: f32, z: f32| {
        let w = root + rot * Vec3::new(x, 0.0, z);
        cx.grid.sample_height(w.x, w.z) - root.y
    };
    let mut wood = MeshAcc::default();
    let plank_m = cube(0.46, 0.09, 2.1);
    let mut x = 0.0;
    let mut k = 0;
    while x + 0.46 <= len {
        let tone = 0.75 + 0.5 * hash01(k + 900);
        let c = rgb(0.17 * tone, 0.105 * tone, 0.055 * tone);
        wood.plain(&plank_m, Transform::from_xyz(x + 0.25, 0.0, 0.0).with_rotation(Quat::from_rotation_y((hash01(k + 5) - 0.5) * 0.02)), c);
        x += 0.5;
        k += 1;
    }
    let beam = rgb(0.10, 0.06, 0.035);
    for z in [-0.8, 0.8] {
        wood.plain(&cube(len, 0.14, 0.16), Transform::from_xyz(len * 0.5, -0.14, z), beam);
    }
    let mut px = 0.6;
    let mut pk = 0;
    while px < len {
        for z in [-0.95, 0.95] {
            let g = ground_rel(px, z) - 0.5;
            let top = 0.6;
            let h = top - g;
            let tall = if px + 2.4 >= len { 0.0 } else { 0.0 };
            let _ = tall;
            wood.push(
                &cyl(0.12, h, 8),
                Transform::from_xyz(px, g + h * 0.5, z),
                |p| p,
                move |p, _| {
                    let y = p.y + h * 0.5 + g; // relative to deck
                    if y < -0.7 { mixc(rgb(0.04, 0.07, 0.035), rgb(0.07, 0.045, 0.03), (y + 1.5).clamp(0.0, 1.0)) } else { rgb(0.085, 0.052, 0.03) }
                },
            );
        }
        px += 2.4;
        pk += 1;
    }
    let _ = pk;
    for z in [-0.95, 0.95] {
        let start = 1.6;
        wood.plain(&cube(len - start, 0.07, 0.07), Transform::from_xyz(start + (len - start) * 0.5, 0.52, z), rgb(0.12, 0.07, 0.04));
    }
    // lantern post at the far end
    let (lx, lz) = (len - 0.35, -0.95);
    wood.plain(&cyl(0.05, 2.4, 6), Transform::from_xyz(lx, 1.2, lz), rgb(0.07, 0.045, 0.03));
    wood.plain(&cube(0.05, 0.05, 0.7), Transform::from_xyz(lx, 2.38, lz + 0.35), rgb(0.07, 0.045, 0.03));
    let lant_pos = Vec3::new(lx, 2.12, lz + 0.62);
    wood.plain(&cube(0.012, 0.2, 0.012), Transform::from_xyz(lant_pos.x, lant_pos.y + 0.2, lant_pos.z), rgb(0.03, 0.03, 0.03));

    // boat: hull, rim, seat, oars
    let mut boat = MeshAcc::default();
    let hull = rgb(0.075, 0.05, 0.03);
    let paint = rgb(0.08, 0.05, 0.11);
    boat.plain(&cube(2.7, 0.08, 0.95), Transform::from_xyz(0.0, 0.0, 0.0), hull);
    for s in [-1.0, 1.0] {
        boat.plain(&cube(2.7, 0.36, 0.07), Transform::from_xyz(0.0, 0.2, s * 0.46).with_rotation(Quat::from_rotation_x(-s * 0.12)), paint);
        boat.plain(&cube(0.95, 0.36, 0.07), Transform::from_xyz(1.55, 0.2, s * 0.26).with_rotation(Quat::from_rotation_y(s * 0.52)), paint);
    }
    boat.plain(&cube(0.07, 0.36, 0.95), Transform::from_xyz(-1.35, 0.2, 0.0), paint);
    boat.plain(&cube(0.35, 0.06, 0.92), Transform::from_xyz(0.1, 0.3, 0.0), hull);
    boat.plain(&cyl(0.025, 1.9, 5), Transform::from_xyz(-0.4, 0.38, 0.0).with_rotation(Quat::from_rotation_x(FRAC_PI_2) * Quat::from_rotation_z(0.25)), rgb(0.14, 0.09, 0.05));

    let half = Vec3::new(len * 0.5, 1.4, 1.4);
    let o = cx.obj("WoodenPier", ObjectCategory::Structure, half);
    let boat_x = (len * 0.55).max(2.0);
    let lantern_light = (
        NightLight { base: 26_000.0 },
        PointLight { color: Color::srgb(1.0, 0.68, 0.32), intensity: 0.0, range: 24.0, radius: 0.08, shadow_maps_enabled: false, ..default() },
        Transform::from_translation(lant_pos),
    );
    commands
        .spawn((Name::new("WoodenPier"), o, mul_tf(root, rot), Visibility::default()))
        .with_children(|c| {
            c.spawn((Name::new("PierDeck"), Mesh3d(meshes.add(wood.build())), MeshMaterial3d(assets.mats.wood.clone()), Transform::default()));
            c.spawn((
                Name::new("RowBoat"),
                Mesh3d(meshes.add(boat.build())),
                MeshMaterial3d(assets.mats.wood.clone()),
                Transform::from_xyz(boat_x, -0.52, 2.4).with_rotation(Quat::from_rotation_y(0.18)),
                Bob { phase: hash01(77) * TAU, base_y: -0.52, yaw: 0.18 },
            ));
            c.spawn((Name::new("PierLanternGlass"), Mesh3d(meshes.add(cube(0.18, 0.26, 0.18))), MeshMaterial3d(assets.mats.lantern_glass.clone()), Transform::from_translation(lant_pos), NotShadowCaster));
            c.spawn(lantern_light);
        })
        .id()
}

fn spawn_camp(commands: &mut Commands, cx: &mut Ctx, assets: &PropAssets, meshes: &mut Assets<Mesh>, _mats: &mut Assets<StandardMaterial>, it: &ScatterItem) -> Entity {
    let root = it.pos;
    let g = |x: f32, z: f32| cx.grid.sample_height(root.x + x, root.z + z) - root.y;
    let mut acc = MeshAcc::default();
    let rock_m = &assets.rocks[0];
    let _ = rock_m;
    let stone_mesh = rock_mesh(5);
    // stone ring
    for k in 0..11u32 {
        let a = k as f32 / 11.0 * TAU;
        let (x, z) = (a.cos() * 0.95, a.sin() * 0.95);
        let s = 0.26 + 0.1 * hash01(k + 300);
        acc.push(&stone_mesh, Transform::from_xyz(x, g(x, z) + 0.08, z).with_rotation(Quat::from_rotation_y(hash01(k + 310) * TAU)).with_scale(Vec3::new(s * 1.2, s * 0.8, s)), |p| p, |_, c| c);
    }
    // charred logs piled like a teepee
    for k in 0..6u32 {
        let a = k as f32 / 6.0 * TAU + 0.3;
        let radial = Vec3::new(a.cos(), 0.0, a.sin());
        let rot = Quat::from_rotation_y(-a) * Quat::from_rotation_z(1.0);
        acc.push(&cyl(0.075, 1.15, 7), Transform::from_translation(radial * 0.32 + Vec3::Y * 0.3).with_rotation(rot), |p| p, |p, _| mixc(rgb(0.07, 0.04, 0.025), rgb(0.012, 0.01, 0.01), (p.y + 0.57) * 1.0));
    }
    // log benches
    for (k, a) in [0.5_f32, 2.35, 4.05].into_iter().enumerate() {
        let (x, z) = (a.cos() * 2.9, a.sin() * 2.9);
        let rot = Quat::from_rotation_y(-(a + FRAC_PI_2)) * Quat::from_rotation_z(FRAC_PI_2);
        let bark = rgb(0.055 + 0.01 * k as f32, 0.04, 0.026);
        acc.push(&cyl(0.22, 2.1, 9), Transform::from_xyz(x, g(x, z) + 0.2, z).with_rotation(rot), |p| p, move |p, _| mulc(bark, 0.8 + 0.6 * (p.x / 0.22).abs() * 0.0 + 0.2 * (p.y * 7.0).sin().abs()));
    }
    // firewood stack by the tent
    for (k, (dx, dy)) in [(-0.2, 0.0), (0.0, 0.0), (0.2, 0.0), (-0.1, 0.17), (0.1, 0.17), (0.0, 0.34)].into_iter().enumerate() {
        let (x, z) = (-2.6 + dx, 4.8);
        acc.plain(&cyl(0.09, 1.0, 7), Transform::from_xyz(x, g(x, z) + 0.1 + dy, z).with_rotation(Quat::from_rotation_x(FRAC_PI_2)), rgb(0.14 - 0.01 * k as f32, 0.08, 0.04));
    }
    // backpack & crate
    let (bx, bz) = (-2.4, 2.2);
    acc.plain(&cube(0.5, 0.65, 0.34), Transform::from_xyz(bx, g(bx, bz) + 0.33, bz).with_rotation(Quat::from_rotation_y(0.7)), rgb(0.03, 0.07, 0.04));
    acc.plain(&cube(0.38, 0.22, 0.36), Transform::from_xyz(bx - 0.4, g(bx, bz) + 0.11, bz + 0.5).with_rotation(Quat::from_rotation_y(0.3)), rgb(0.12, 0.075, 0.04));

    let tent_off = Vec2::new(-4.8, 3.2);
    let flames = [(0.36_f32, 1.05_f32, 0usize, Vec3::ZERO), (0.26, 0.8, 1, Vec3::ZERO), (0.15, 0.5, 2, Vec3::ZERO), (0.14, 0.55, 0, Vec3::new(0.2, 0.0, 0.1)), (0.12, 0.5, 1, Vec3::new(-0.18, 0.0, 0.12)), (0.12, 0.45, 0, Vec3::new(0.05, 0.0, -0.2))];

    let o = cx.obj("CampfireSite", ObjectCategory::Camp, Vec3::new(5.5, 1.6, 5.5));
    let tent_y = g(tent_off.x, tent_off.y);
    let static_mesh = meshes.add(acc.build());
    commands
        .spawn((Name::new("CampfireSite"), o, mul_tf(root, it.rot), Visibility::default()))
        .with_children(|c| {
            c.spawn((Name::new("CampProps"), Mesh3d(static_mesh), MeshMaterial3d(assets.mats.rock.clone()), Transform::default()));
            c.spawn((Name::new("Tent"), Mesh3d(assets.tent.clone()), MeshMaterial3d(assets.mats.canvas.clone()), Transform::from_xyz(tent_off.x, tent_y, tent_off.y).with_rotation(Quat::from_rotation_y(0.9))));
            c.spawn((Name::new("EmberBed"), Mesh3d(meshes.add(Sphere::new(0.5).mesh().ico(2).expect("ico"))), MeshMaterial3d(assets.mats.ember.clone()), Transform::from_xyz(0.0, 0.08, 0.0).with_scale(Vec3::new(1.0, 0.28, 1.0)), NotShadowCaster));
            for (k, (r, h, layer, off)) in flames.iter().enumerate() {
                let base = Vec3::new(*r, *h, *r);
                c.spawn((
                    Name::new("Flame"),
                    Flame { phase: k as f32 * 1.7, base },
                    Mesh3d(assets.flame_cone.clone()),
                    MeshMaterial3d(assets.mats.flame[*layer].clone()),
                    Transform::from_translation(Vec3::new(0.0, 0.25 + h * 0.5, 0.0) + *off).with_scale(base),
                    NotShadowCaster,
                ));
            }
            for k in 0..26u32 {
                let life = 1.1 + 1.8 * hash01(k + 500);
                c.spawn((
                    Name::new("Spark"),
                    Spark { age: hash01(k + 520) * life, life, vel: Vec3::new((hash01(k + 540) - 0.5) * 0.6, 1.2 + 1.6 * hash01(k + 560), (hash01(k + 580) - 0.5) * 0.6), origin: Vec3::new((hash01(k + 600) - 0.5) * 0.4, 0.5, (hash01(k + 620) - 0.5) * 0.4), seed: hash01(k + 640) * TAU },
                    Mesh3d(assets.spark_mesh.clone()),
                    MeshMaterial3d(assets.mats.spark.clone()),
                    Transform::default(),
                    NotShadowCaster,
                ));
            }
            c.spawn((
                Name::new("CampfireLight"),
                NightLight { base: 70_000.0 },
                PointLight { color: Color::srgb(1.0, 0.52, 0.2), intensity: 0.0, range: 45.0, radius: 0.35, shadow_maps_enabled: cx.spec.shadows, ..default() },
                VolumetricLight,
                Transform::from_xyz(0.0, 0.95, 0.0),
            ));
        })
        .id()
}

// ───────────────────────────── User-spawned props ─────────────────────────────

/// Marks editor-spawned roots (kept across terrain rebuilds, saved in scene JSON). `kind` = pine|boulder|campfire|pier|lantern|model:<file>.
#[derive(Component, Reflect, Serialize, Deserialize, Clone, Debug, PartialEq)]
#[reflect(Component)]
pub struct UserProp {
    pub kind: String,
}

/// Monotonic id source for editor-spawned objects.
#[derive(Resource)]
pub struct NextObjectId(pub u32);

impl Default for NextObjectId {
    fn default() -> Self {
        Self(1_000_000)
    }
}

fn spawn_lantern(commands: &mut Commands, cx: &mut Ctx, assets: &PropAssets, meshes: &mut Assets<Mesh>, tf: Transform) -> Entity {
    let dark = rgb(0.07, 0.045, 0.03);
    let mut acc = MeshAcc::default();
    acc.plain(&cyl(0.05, 2.4, 6), Transform::from_xyz(0.0, 1.2, 0.0), dark);
    acc.plain(&cube(0.05, 0.05, 0.7), Transform::from_xyz(0.0, 2.38, 0.35), dark);
    acc.plain(&cube(0.012, 0.2, 0.012), Transform::from_xyz(0.0, 2.32, 0.62), rgb(0.03, 0.03, 0.03));
    let lp = Vec3::new(0.0, 2.12, 0.62);
    let o = cx.obj("Lantern", ObjectCategory::Structure, Vec3::new(0.4, 1.3, 0.7));
    commands
        .spawn((Name::new("Lantern"), o, tf, Visibility::default()))
        .with_children(|c| {
            c.spawn((Name::new("LanternPost"), Mesh3d(meshes.add(acc.build())), MeshMaterial3d(assets.mats.wood.clone()), Transform::default()));
            c.spawn((Name::new("LanternGlass"), Mesh3d(meshes.add(cube(0.18, 0.26, 0.18))), MeshMaterial3d(assets.mats.lantern_glass.clone()), Transform::from_translation(lp), NotShadowCaster));
            c.spawn((
                Name::new("LanternLight"),
                NightLight { base: 26_000.0 },
                PointLight { color: Color::srgb(1.0, 0.68, 0.32), intensity: 0.0, range: 24.0, radius: 0.08, shadow_maps_enabled: false, ..default() },
                Transform::from_translation(lp),
            ));
        })
        .id()
}

/// Spawn one editor object on the terrain. Returns the root and its spawn transform.
#[allow(clippy::too_many_arguments)]
pub fn spawn_user_prop(
    commands: &mut Commands,
    assets: &PropAssets,
    kind: &str,
    pos: Vec3,
    yaw: f32,
    id: u32,
    spec: &WorldSpec,
    grid: &HeightfieldGrid,
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<StandardMaterial>,
    server: &AssetServer,
    kinds: &SpawnKinds,
) -> Option<(Entity, Transform)> {
    let mut cx = Ctx { grid, spec, next_id: id - 1 };
    let rot = Quat::from_rotation_y(yaw);
    let seed = id.wrapping_mul(2654435761);
    let var = hash01(seed);
    let scale = 0.9 + 0.5 * hash01(seed ^ 0x55);
    let (e, tf) = match kind {
        "pine" => {
            let tf = Transform { translation: pos, rotation: rot, scale: Vec3::splat(scale) };
            let o = cx.obj("PineTree", ObjectCategory::Vegetation, Vec3::new(2.0, 4.2, 2.0) * scale);
            let h = assets.pines[(var * assets.pines.len() as f32) as usize % assets.pines.len()].clone();
            (commands.spawn((Name::new("PineTree"), o, Mesh3d(h), MeshMaterial3d(assets.mats.veg.clone()), tf)).id(), tf)
        }
        "boulder" => {
            let tf = Transform { translation: pos, rotation: rot, scale: Vec3::splat(scale * 1.4) };
            let o = cx.obj("Boulder", ObjectCategory::Rock, Vec3::new(1.2, 0.8, 1.0) * scale * 1.4);
            let h = assets.rocks[(var * assets.rocks.len() as f32) as usize % assets.rocks.len()].clone();
            (commands.spawn((Name::new("Boulder"), o, Mesh3d(h), MeshMaterial3d(assets.mats.rock.clone()), tf)).id(), tf)
        }
        "campfire" => {
            let it = ScatterItem { kind: ScatterKind::CampfireSite, pos, rot, scale: 1.0 };
            (spawn_camp(commands, &mut cx, assets, meshes, mats, &it), mul_tf(pos, rot))
        }
        "pier" => {
            let it = ScatterItem { kind: ScatterKind::WoodenPier, pos, rot, scale: 10.0 };
            (spawn_pier(commands, &mut cx, assets, meshes, &it), mul_tf(pos, rot))
        }
        "lantern" => {
            let tf = mul_tf(pos, rot);
            (spawn_lantern(commands, &mut cx, assets, meshes, tf), tf)
        }
        "level" => {
            // bare root: `LevelTile` (restored from a scene file or inserted by the builder) drives the visuals
            let tf = mul_tf(pos, rot);
            let o = cx.obj("LevelTile", ObjectCategory::Level, Vec3::new(1.0, 1.75, 1.0));
            (commands.spawn((Name::new("LevelTile"), o, tf, Visibility::default())).id(), tf)
        }
        "prefab" => {
            // bare root: `PrefabInstance` (set by SpawnPrefab or restored from a scene file) re-creates the members
            let tf = mul_tf(pos, rot);
            let o = cx.obj("Prefab", ObjectCategory::Structure, Vec3::splat(2.0));
            (commands.spawn((Name::new("Prefab"), o, tf, Visibility::default())).id(), tf)
        }
        k if k.starts_with("model:") => {
            let file = &k[6..];
            let leaf = file.rsplit('/').next().unwrap_or(file);
            let stem = leaf.rsplit_once('.').map_or(leaf, |(s, _)| s).to_string();
            let tf = mul_tf(pos, rot);
            let o = cx.obj(&stem, ObjectCategory::Model, Vec3::splat(1.0));
            let scene = server.load(GltfAssetLabel::Scene(0).from_asset(model_asset_path(file)));
            (commands.spawn((Name::new(stem), o, WorldAssetRoot(scene), tf)).id(), tf)
        }
        k => {
            let handler = kinds.find(k)?;
            let tf = mul_tf(pos, rot);
            let mut sc = SpawnCtx { commands: &mut *commands, meshes: &mut *meshes, mats: &mut *mats, server, tf, id };
            (handler(&mut sc, k)?, tf)
        }
    };
    commands.entity(e).remove::<PropRoot>().insert(UserProp { kind: kind.to_string() });
    Some((e, tf))
}

// ───────────────────────────── Animation ─────────────────────────────

fn animate_flames(time: Res<Time>, mut q: Query<(&Flame, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (f, mut tf) in &mut q {
        let p = f.phase;
        let flick = 1.0 + 0.22 * (t * 11.0 + p).sin() * (t * 4.3 + p * 2.0).cos() + 0.10 * (t * 27.0 + p).sin();
        tf.scale = Vec3::new(f.base.x * (1.0 + 0.12 * (t * 8.0 + p).cos()), f.base.y * flick, f.base.z * (1.0 + 0.12 * (t * 9.0 + p).sin()));
        tf.translation.y = 0.25 + tf.scale.y * 0.5;
        tf.rotation = Quat::from_rotation_z(0.10 * (t * 3.1 + p).sin()) * Quat::from_rotation_x(0.10 * (t * 2.6 + p * 1.3).cos());
    }
}

fn animate_sparks(time: Res<Time>, mut q: Query<(&mut Spark, &mut Transform)>) {
    let dt = time.delta_secs();
    for (mut s, mut tf) in &mut q {
        s.age += dt;
        if s.age >= s.life {
            s.age = 0.0;
            s.seed = rand::random::<f32>() * TAU;
            s.vel = Vec3::new((rand::random::<f32>() - 0.5) * 0.7, 1.2 + 1.8 * rand::random::<f32>(), (rand::random::<f32>() - 0.5) * 0.7);
        }
        let t = s.age;
        let sway = Vec3::new((t * 4.0 + s.seed).sin(), 0.0, (t * 3.3 + s.seed).cos()) * 0.12 * t;
        tf.translation = s.origin + s.vel * t - Vec3::Y * 0.15 * t * t + sway;
        let k = 1.0 - (t / s.life).powi(2);
        tf.scale = Vec3::splat(k.max(0.001));
    }
}

fn animate_bobbing(time: Res<Time>, mut q: Query<(&Bob, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (b, mut tf) in &mut q {
        tf.translation.y = b.base_y + 0.025 * (t * 1.3 + b.phase).sin();
        tf.rotation = Quat::from_rotation_y(b.yaw) * Quat::from_rotation_z(0.025 * (t * 1.1 + b.phase).sin()) * Quat::from_rotation_x(0.02 * (t * 0.9 + b.phase).cos());
    }
}
