//! Cinematic showcase: snowy castle, duelling knights, live landscape morph and a time-of-day sweep.
//! Deterministic offline capture: `showcase record <dir>` steps the clock by 1/fps and writes one PNG per frame.

pub mod knight;

use std::f32::consts::{PI, TAU};
use std::time::Duration;

use bevy::camera::visibility::NoFrustumCulling;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::gltf::GltfMaterialName;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use bevy::time::TimeUpdateStrategy;
use bevy::world_serialization::WorldInstanceReady;
use noise::{NoiseFn, Perlin};
use rand::{Rng, SeedableRng, rngs::StdRng};

use crate::arc_core::{WorldSpec, smoothstep};
use crate::lighting::NightLight;
use crate::terrain_render::{TerrainGrid, TerrainMesh, TerrainVersion};
use knight::{Duel, KnightKit, Team};

pub const PLATEAU: f32 = 20.0;
pub const DURATION: f32 = 14.0;
const CELL: f32 = 5.0;
const PLATEAU_CENTER: Vec2 = Vec2::new(0.0, 18.0);

#[derive(Resource, Default)]
pub struct Clock {
    pub t: f32,
    pub frame: u32,
}

#[derive(Resource)]
pub struct Duels(pub Vec<Duel>);

#[derive(Clone)]
pub enum Mode {
    Live,
    Shot { t: f32, out: String, cam: Option<[f32; 6]> },
    Record { dir: String, fps: u32 },
}

#[derive(Resource)]
pub struct ShowMode(pub Mode);

#[derive(Resource, Default)]
struct Progress {
    expected: u32,
    ready: u32,
    warm: u32,
    shot_frame: Option<u32>,
    saved: u32,
    requested: u32,
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Skin {
    Castle,
    Nature,
}

#[derive(Resource)]
struct WinterMaterial(Handle<StandardMaterial>);

#[derive(Component)]
struct Follow {
    x: f32,
    z: f32,
    scale: f32,
    sink: f32,
}

#[derive(Component)]
struct Flake {
    seed: Vec3,
    size: f32,
}

#[derive(Component)]
struct Spark {
    duel: usize,
    k: u32,
}

#[derive(Component)]
struct ClashLight(usize);

#[derive(Resource)]
struct FlakeMaterial(Handle<StandardMaterial>);

pub struct ShowcasePlugin(pub Mode);

pub fn showcase_spec() -> WorldSpec {
    // the studio's camp clearing is the castle plateau: water floor + 1.5 pins it at PLATEAU, the lake sits beside it
    let mut s = WorldSpec {
        base_height: 16.0,
        clearing_radius: 30.0,
        lake_radius: 5.0,
        lake_depth: 0.0,
        lake_center: [0.0, PLATEAU_CENTER.y],
        camp_center: [1.0, PLATEAU_CENTER.y],
        erosion_droplets: 0,
        thermal_iterations: 0,
        resolution: 401,
        size: 400.0,
        forest_density: 0.0,
        boulder_density: 0.0,
        grass_density: 0.0,
        water_level: PLATEAU - 1.5,
        time_of_day: 7.0,
        auto_sun: false,
        fog_color: [0.62, 0.70, 0.84],
        fog_density: 0.0024,
        god_rays: std::env::var("ARC_GODRAYS").is_ok(),
        shadows: std::env::var("ARC_NOSHADOW").is_err(),
        ..WorldSpec::default()
    };
    // move the lake west so the camp (clearing centre) lands exactly on the plateau centre
    let off = s.camp_pos().x;
    s.lake_center[0] -= off;
    s.camp_center[0] -= off;
    s
}

impl Plugin for ShowcasePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(showcase_spec())
            .insert_resource(crate::lighting::NightBoost(3.5))
            .insert_resource(ShowMode(self.0.clone()))
            .init_resource::<Clock>()
            .init_resource::<Progress>()
            .insert_resource(Duels(vec![
                Duel { center: Vec2::new(0.0, 31.0), yaw: 0.2, period: 4.4, phase: 0.2, style: 0 },
                Duel { center: Vec2::new(-17.0, 39.0), yaw: 1.0, period: 3.6, phase: 1.3, style: 1 },
                Duel { center: Vec2::new(16.0, 40.0), yaw: -1.25, period: 4.0, phase: 2.3, style: 2 },
                Duel { center: Vec2::new(-8.0, 24.0), yaw: 0.35, period: 3.3, phase: 0.6, style: 1 },
            ]))
            .add_observer(on_world_ready)
            .add_systems(Startup, (build_scene, build_flakes, build_far_ground))
            .add_systems(
                Update,
                (drive_clock, drive_time_of_day, drive_terrain_spec, fix_atmosphere, recolor_terrain, follow_terrain, knight::animate_knights, animate_sparks, animate_flakes, capture)
                    .chain(),
            )
            .add_systems(PostUpdate, drive_camera.before(TransformSystems::Propagate));
        if !matches!(self.0, Mode::Live) {
            app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 30.0)));
        }
    }
}

// ───────────────────────────── Scene ─────────────────────────────

fn hash(n: u32) -> f32 {
    ((n as f32 * 12.9898).sin() * 43758.547).fract().abs()
}

#[allow(clippy::too_many_arguments)]
fn build_scene(
    mut commands: Commands,
    server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut progress: ResMut<Progress>,
    duels: Res<Duels>,
) {
    // winter palette shared by every castle piece
    let tex: Handle<Image> = server.load("showcase/winter_colormap.png");
    let winter = mats.add(StandardMaterial { base_color_texture: Some(tex), perceptual_roughness: 0.82, metallic: 0.0, reflectance: 0.3, ..default() });
    commands.insert_resource(WinterMaterial(winter));

    let load = |commands: &mut Commands, progress: &mut Progress, path: &str, skin: Skin, tf: Transform| -> Entity {
        progress.expected += 1;
        commands.spawn((WorldAssetRoot(server.load(GltfAssetLabel::Scene(0).from_asset(path.to_string()))), skin, tf)).id()
    };

    // ── castle ──
    let s = CELL;
    let at = |cx: f32, cz: f32, y: f32| Vec3::new(cx * s, PLATEAU + y, cz * s);
    let piece = |name: &str| format!("showcase/castle/{name}.glb");
    let spawn_castle = |commands: &mut Commands, progress: &mut Progress, name: &str, p: Vec3, yaw: f32, scale: f32| {
        let sy = if name.starts_with("wall") { scale * 1.4 } else { scale };
        load(commands, progress, &piece(name), Skin::Castle, Transform::from_translation(p).with_rotation(Quat::from_rotation_y(yaw)).with_scale(Vec3::new(scale, sy, scale)));
    };

    // corner towers
    for (cx, cz) in [(-3.0, -3.0), (3.0, -3.0), (-3.0, 3.0), (3.0, 3.0)] {
        let mut y = 0.0;
        for (name, h) in [("tower-square-base", 1.01), ("tower-square-mid-windows", 1.01), ("tower-square-top", 0.3), ("tower-square-top-roof-high", 1.35)] {
            spawn_castle(&mut commands, &mut progress, name, at(cx, cz, y * s), 0.0, s);
            y += h;
        }
        load(&mut commands, &mut progress, &piece("flag-pennant"), Skin::Castle, Transform::from_translation(at(cx, cz, y * s - 0.2 * s)).with_scale(Vec3::splat(s * 1.4)));
    }
    // curtain walls (run along local X); gate gap in the south wall
    for k in -2..=2 {
        let kf = k as f32;
        spawn_castle(&mut commands, &mut progress, "wall", at(kf, -3.0, 0.0), 0.0, s);
        spawn_castle(&mut commands, &mut progress, if k == 0 { "wall-doorway" } else { "wall" }, at(kf, 3.0, 0.0), 0.0, s);
        spawn_castle(&mut commands, &mut progress, "wall", at(-3.0, kf, 0.0), PI * 0.5, s);
        spawn_castle(&mut commands, &mut progress, "wall", at(3.0, kf, 0.0), PI * 0.5, s);
    }
    // central keep
    let ks = 6.8;
    let mut y = 0.0;
    for (name, h) in [("tower-square-base", 1.01), ("tower-square-mid-windows", 1.01), ("tower-square-mid", 1.01), ("tower-square-mid-windows", 1.01), ("tower-square-top", 0.3), ("tower-square-top-roof-high", 1.35)] {
        spawn_castle(&mut commands, &mut progress, name, Vec3::new(0.0, PLATEAU + y * ks, -4.0), 0.0, ks);
        y += h;
    }
    load(&mut commands, &mut progress, &piece("flag"), Skin::Castle, Transform::from_translation(Vec3::new(0.0, PLATEAU + (y - 0.2) * ks, -4.0)).with_scale(Vec3::splat(ks * 1.2)));
    // banners on the gate towers, siege engines in the field
    for sx in [-1.0, 1.0] {
        load(&mut commands, &mut progress, &piece("flag-banner-long"), Skin::Castle, Transform::from_translation(Vec3::new(sx * 6.0, PLATEAU + 3.6, 3.0 * s + 2.7)).with_rotation(Quat::from_rotation_y(PI * 0.5)).with_scale(Vec3::splat(2.4)));
    }
    for (name, x, z, yaw, sc) in [("siege-catapult", -46.0, 22.0, 0.9, 5.0), ("siege-trebuchet", 46.0, 14.0, -2.2, 5.0), ("siege-ballista", 40.0, 50.0, 2.6, 4.5), ("siege-ballista", -50.0, 44.0, 0.3, 4.5)] {
        load(&mut commands, &mut progress, &piece(name), Skin::Castle, Transform::from_xyz(x, PLATEAU + 0.6, z).with_rotation(Quat::from_rotation_y(yaw)).with_scale(Vec3::splat(sc)));
    }

    // ── forest and rocks that ride on the morphing terrain ──
    let pines = ["tree_pineTallA", "tree_pineTallB", "tree_pineTallC", "tree_pineTallD", "tree_pineTallA_detailed", "tree_pineTallB_detailed", "tree_pineTallC_detailed", "tree_pineDefaultA", "tree_pineDefaultB", "tree_pineRoundA", "tree_pineRoundB", "tree_pineRoundC"];
    let rocks = ["stone_largeA", "stone_largeB", "stone_largeC", "stone_largeD", "rock_largeA", "rock_largeC", "stone_tallA", "stone_tallC"];
    let mut rng = StdRng::seed_from_u64(11);
    let mut placed: Vec<Vec2> = vec![];
    let mut tries = 0;
    let cam_path: Vec<Vec2> = (0..=56).map(|i| { let e = cam_at(i as f32 * 0.25).0; Vec2::new(e.x, e.z) }).collect();
    while placed.len() < 420 && tries < 20000 {
        tries += 1;
        let a = rng.r#gen::<f32>() * TAU;
        let r = 52.0 + rng.r#gen::<f32>().powf(0.7) * 135.0;
        let p = PLATEAU_CENTER + Vec2::new(a.cos(), a.sin()) * r;
        if p.x.abs() > 190.0 || p.y.abs() > 190.0 || placed.iter().any(|q| q.distance(p) < 6.5) || cam_path.iter().any(|q| q.distance(p) < 15.0) {
            continue;
        }
        placed.push(p);
        let sc = 6.5 + rng.r#gen::<f32>() * 5.5;
        let name = pines[rng.gen_range(0..pines.len())];
        let e = load(&mut commands, &mut progress, &format!("showcase/nature/{name}.glb"), Skin::Nature, Transform::from_xyz(p.x, PLATEAU, p.y).with_rotation(Quat::from_rotation_y(rng.r#gen::<f32>() * TAU)).with_scale(Vec3::splat(sc)));
        commands.entity(e).insert(Follow { x: p.x, z: p.y, scale: sc, sink: 0.25 });
    }
    // a handful of trees and rocks on the plateau rim for framing
    for (x, z, sc) in [(-40.0, 30.0, 9.0), (43.0, 36.0, 8.0), (-34.0, 6.0, 10.0), (38.0, 2.0, 9.5), (-30.0, 52.0, 8.5), (30.0, 56.0, 9.0), (50.0, 24.0, 8.0)] {
        let name = pines[((x + z) as usize) % pines.len()];
        let e = load(&mut commands, &mut progress, &format!("showcase/nature/{name}.glb"), Skin::Nature, Transform::from_xyz(x, PLATEAU, z).with_scale(Vec3::splat(sc)));
        commands.entity(e).insert(Follow { x, z, scale: sc, sink: 0.25 });
    }
    for i in 0..70u32 {
        let a = hash(i) * TAU;
        let r = 42.0 + hash(i + 99) * 130.0;
        let p = PLATEAU_CENTER + Vec2::new(a.cos(), a.sin()) * r;
        if p.x.abs() > 185.0 || p.y.abs() > 185.0 {
            continue;
        }
        let sc = 4.0 + hash(i + 7) * 6.0;
        let name = rocks[i as usize % rocks.len()];
        let e = load(&mut commands, &mut progress, &format!("showcase/nature/{name}.glb"), Skin::Nature, Transform::from_xyz(p.x, PLATEAU, p.y).with_rotation(Quat::from_rotation_y(hash(i + 3) * TAU)).with_scale(Vec3::splat(sc)));
        commands.entity(e).insert(Follow { x: p.x, z: p.y, scale: sc, sink: 0.2 });
    }

    // ── braziers around the arena (lit at night) ──
    let metal = mats.add(StandardMaterial { base_color: Color::srgb(0.12, 0.12, 0.14), metallic: 0.8, perceptual_roughness: 0.5, ..default() });
    let flame = mats.add(StandardMaterial { base_color: Color::srgb(1.0, 0.55, 0.12), emissive: LinearRgba::rgb(30.0, 9.0, 1.2), unlit: true, ..default() });
    let pole = meshes.add(Cylinder::new(0.08, 2.0));
    let bowl = meshes.add(Cone::new(0.42, 0.45));
    let fl = meshes.add(Sphere::new(0.3).mesh().uv(12, 8));
    for (x, z) in [(-12.0, 33.0), (12.0, 33.0), (-5.0, 46.0), (7.0, 20.0), (-24.0, 28.0), (25.0, 30.0)] {
        commands
            .spawn((Name::new("Brazier"), Transform::from_xyz(x, PLATEAU, z), Visibility::default()))
            .with_children(|c| {
                c.spawn((Mesh3d(pole.clone()), MeshMaterial3d(metal.clone()), Transform::from_xyz(0.0, 1.0, 0.0)));
                c.spawn((Mesh3d(bowl.clone()), MeshMaterial3d(metal.clone()), Transform::from_xyz(0.0, 2.1, 0.0).with_rotation(Quat::from_rotation_x(PI))));
                c.spawn((Mesh3d(fl.clone()), MeshMaterial3d(flame.clone()), Transform::from_xyz(0.0, 2.45, 0.0).with_scale(Vec3::new(1.0, 1.5, 1.0)), NotShadowCaster));
                c.spawn((PointLight { intensity: 0.0, range: 28.0, color: Color::srgb(1.0, 0.62, 0.3), shadow_maps_enabled: false, ..default() }, NightLight { base: 420_000.0 }, Transform::from_xyz(0.0, 2.9, 0.0)));
            });
    }

    // ── knights ──
    let kit = KnightKit::new(&mut meshes, &mut mats);
    for (di, _) in duels.0.iter().enumerate() {
        knight::spawn_knight(&mut commands, &kit, Team::Blue, di, 0, di as f32 * 1.7);
        knight::spawn_knight(&mut commands, &kit, Team::Red, di, 1, di as f32 * 1.7 + 0.9);
    }
    // sparks and flash lights per duel
    let spark_mesh = meshes.add(Sphere::new(0.035).mesh().uv(6, 4));
    let spark_mat = mats.add(StandardMaterial { base_color: Color::srgb(1.0, 0.8, 0.35), emissive: LinearRgba::rgb(60.0, 32.0, 6.0), unlit: true, ..default() });
    for di in 0..duels.0.len() {
        for k in 0..18u32 {
            commands.spawn((Spark { duel: di, k }, Mesh3d(spark_mesh.clone()), MeshMaterial3d(spark_mat.clone()), Transform::default(), Visibility::Hidden, NotShadowCaster));
        }
        commands.spawn((ClashLight(di), PointLight { intensity: 0.0, range: 14.0, color: Color::srgb(1.0, 0.8, 0.5), shadow_maps_enabled: false, ..default() }, Transform::default()));
    }
}

fn on_world_ready(
    ev: On<WorldInstanceReady>,
    skins: Query<&Skin>,
    children: Query<&Children>,
    mut handles: Query<(&mut MeshMaterial3d<StandardMaterial>, Option<&GltfMaterialName>)>,
    winter: Option<Res<WinterMaterial>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut progress: ResMut<Progress>,
    mut commands: Commands,
) {
    progress.ready += 1;
    let Ok(&skin) = skins.get(ev.entity) else { return };
    let Some(winter) = winter else { return };
    for d in children.iter_descendants(ev.entity) {
        let Ok((mut h, name)) = handles.get_mut(d) else { continue };
        match skin {
            Skin::Castle => h.0 = winter.0.clone(),
            Skin::Nature => {
                let n = name.map(|n| n.0.as_str()).unwrap_or("");
                let (c, rough) = match n {
                    s if s.starts_with("leafs") => (Color::srgb(0.20, 0.34, 0.30), 0.9),
                    s if s.starts_with("wood") => (Color::srgb(0.16, 0.11, 0.09), 0.9),
                    "stone" => (Color::srgb(0.46, 0.49, 0.56), 0.85),
                    "grass" => (Color::srgb(0.88, 0.93, 1.0), 0.8),
                    "dirt" => (Color::srgb(0.38, 0.38, 0.42), 0.9),
                    _ => (Color::srgb(0.7, 0.72, 0.78), 0.85),
                };
                h.0 = mats.add(StandardMaterial { base_color: c, perceptual_roughness: rough, metallic: 0.0, reflectance: 0.25, ..default() });
            }
        }
        let _ = &mut commands;
    }
}

// ───────────────────────────── Timeline ─────────────────────────────

fn tod(t: f32) -> f32 {
    const K: [(f32, f32); 7] = [(0.0, 6.5), (3.0, 9.3), (5.6, 12.4), (8.4, 16.9), (10.4, 18.9), (12.2, 20.6), (14.0, 23.4)];
    for w in K.windows(2) {
        if t <= w[1].0 {
            let k = smoothstep(w[0].0, w[1].0, t);
            let lin = (t - w[0].0) / (w[1].0 - w[0].0);
            return w[0].1 + (w[1].1 - w[0].1) * (0.5 * k + 0.5 * lin);
        }
    }
    K[K.len() - 1].1
}

fn drive_clock(mut clock: ResMut<Clock>, mode: Res<ShowMode>, time: Res<Time>, mut progress: ResMut<Progress>) {
    let ready = progress.ready >= progress.expected;
    progress.warm = if ready { progress.warm + 1 } else { 0 };
    match &mode.0 {
        Mode::Live => clock.t = time.elapsed_secs().rem_euclid(DURATION),
        Mode::Shot { t, .. } => clock.t = *t,
        Mode::Record { fps, .. } => {
            if let Some(f) = progress.shot_frame {
                clock.t = f as f32 / *fps as f32;
            } else {
                clock.t = 0.0;
            }
        }
    }
    clock.frame += 1;
}

fn drive_time_of_day(clock: Res<Clock>, mut spec: ResMut<WorldSpec>, cam: Query<&bevy::camera::Exposure>) {
    if std::env::var("ARC_LOG").is_ok() {
        info!("t={:.2} tod={:.2} elev={:.3} ev={:?}", clock.t, spec.time_of_day, crate::lighting::sun_direction(spec.time_of_day).y, cam.iter().map(|e| e.ev100).collect::<Vec<_>>());
    }
    let v = tod(clock.t);
    if (spec.time_of_day - v).abs() > 1e-4 {
        spec.time_of_day = v;
    }
    // slightly deeper, colder haze at night so the sky stays moody
    spec.auto_sun = false;
}

// ───────────────────────────── Terrain (studio generator, driven through WorldSpec) ─────────────────────────────

/// The terrain sliders of the inspector over time; the studio regenerates the heightfield from them.
fn drive_terrain_spec(clock: Res<Clock>, mut spec: ResMut<WorldSpec>) {
    let (s1, s2) = (smoothstep(2.6, 6.6, clock.t), smoothstep(7.6, 11.8, clock.t));
    let want = [30.0 + 100.0 * s1 - 45.0 * s2, 70.0 + 120.0 * s1 - 60.0 * s2, 3.0 + 0.8 * s1 - 0.5 * s2, 0.08 + 0.17 * s1 - 0.07 * s2];
    let cur = [spec.mountain_height, spec.max_height, spec.noise_frequency, spec.warp_amount];
    if cur.iter().zip(&want).any(|(a, b)| (a - b).abs() > 1e-4) {
        [spec.mountain_height, spec.max_height, spec.noise_frequency, spec.warp_amount] = want;
    }
}

fn fbm(p: &Perlin, x: f64, z: f64, oct: u32) -> f32 {
    let (mut s, mut a, mut f, mut norm) = (0.0, 1.0, 1.0, 0.0);
    for _ in 0..oct {
        s += a * p.get([x * f, z * f]);
        norm += a;
        a *= 0.5;
        f *= 2.03;
    }
    (s / norm) as f32
}

/// Snow palette over the studio's own terrain mesh (positions come from the generator untouched).
fn recolor_terrain(
    mut commands: Commands,
    version: Res<TerrainVersion>,
    grid: Option<Res<TerrainGrid>>,
    q: Query<(Entity, &Mesh3d, Has<NoFrustumCulling>), With<TerrainMesh>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut seen: Local<Option<u32>>,
) {
    let (Some(grid), Ok((e, handle, culled))) = (grid, q.single()) else { return };
    if !culled {
        commands.entity(e).insert(NoFrustumCulling);
    }
    if *seen == Some(version.0) {
        return;
    }
    *seen = Some(version.0);
    let g = &grid.0;
    let pj = Perlin::new(5);
    let mut col = Vec::with_capacity(g.n * g.n);
    for j in 0..g.n {
        for i in 0..g.n {
            let (x, z) = g.grid_to_world(i, j);
            let nv = g.sample_normal(x, z);
            let jt = 0.5 * fbm(&pj, x as f64 / 5.0, z as f64 / 5.0, 3) + 0.5;
            let rock = smoothstep(0.30, 0.52, 1.0 - nv.y + (jt - 0.5) * 0.12);
            let snow = Vec3::new(0.70, 0.75, 0.86) * (0.93 + 0.12 * jt);
            let stone = Vec3::new(0.085, 0.09, 0.105) * (0.7 + 0.6 * jt);
            // trampled battleground around the arena
            let tr = (1.0 - smoothstep(9.0, 30.0, (Vec2::new(x, z) - Vec2::new(0.0, 33.0)).length())) * 0.4;
            let base = snow.lerp(Vec3::new(0.50, 0.52, 0.60), tr * (0.6 + 0.4 * jt)).lerp(stone, rock);
            col.push([base.x, base.y, base.z, 1.0]);
        }
    }
    if let Some(mut m) = meshes.get_mut(&handle.0) {
        m.insert_attribute(Mesh::ATTRIBUTE_COLOR, VertexAttributeValues::Float32x4(col));
    }
}

fn follow_terrain(grid: Option<Res<TerrainGrid>>, mut q: Query<(&Follow, &mut Transform)>) {
    let Some(grid) = grid else { return };
    let g = &grid.0;
    for (f, mut tf) in &mut q {
        let h = g.sample_height(f.x, f.z);
        // steep ground: shrink the prop away instead of letting it hang off a cliff
        let hx = (g.sample_height(f.x + 2.5, f.z) - g.sample_height(f.x - 2.5, f.z)).abs() / 5.0;
        let hz = (g.sample_height(f.x, f.z + 2.5) - g.sample_height(f.x, f.z - 2.5)).abs() / 5.0;
        let k = 1.0 - smoothstep(0.55, 1.1, hx.max(hz));
        tf.translation.y = h - f.sink * f.scale * 0.35;
        tf.scale = Vec3::splat(f.scale * k.max(0.001));
    }
}

fn build_far_ground(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let m = mats.add(StandardMaterial { base_color: Color::linear_rgb(0.66, 0.71, 0.82), perceptual_roughness: 0.9, reflectance: 0.2, ..default() });
    commands.spawn((Name::new("FarGround"), Mesh3d(meshes.add(Plane3d::default().mesh().size(9000.0, 9000.0))), MeshMaterial3d(m), Transform::from_xyz(0.0, PLATEAU - 12.2, 0.0), NotShadowCaster));
}

fn fix_atmosphere(mut q: Query<&mut bevy::light::Atmosphere>, cam: Query<Entity, (With<Camera3d>, Without<bevy::anti_alias::smaa::Smaa>)>, mut commands: Commands, mut done: Local<bool>) {
    for e in &cam {
        commands.entity(e).insert(bevy::anti_alias::smaa::Smaa::default());
    }
    if *done {
        return;
    }
    for mut a in &mut q {
        a.ground_albedo = Vec3::splat(0.8);
        *done = true;
    }
}

// ───────────────────────────── Sparks / snow ─────────────────────────────

fn animate_sparks(
    clock: Res<Clock>,
    duels: Res<Duels>,
    mut sparks: Query<(&Spark, &mut Transform, &mut Visibility), Without<ClashLight>>,
    mut lights: Query<(&ClashLight, &mut Transform, &mut PointLight), Without<Spark>>,
) {
    let t = clock.t;
    let origin = |di: usize| -> Option<(Vec3, f32)> {
        let d = &duels.0[di];
        let ti = d.last_impact(t)?;
        let _ = d;
        Some((knight::clash_point(&duels.0, di, ti), t - ti))
    };
    for (s, mut tf, mut vis) in &mut sparks {
        let Some((o, tau)) = origin(s.duel) else { *vis = Visibility::Hidden; continue };
        if tau > 0.55 {
            *vis = Visibility::Hidden;
            continue;
        }
        let (a, b, c) = (hash(s.k * 3 + 1), hash(s.k * 3 + 2), hash(s.k * 3 + 3));
        let ang = a * TAU;
        let v = Vec3::new(ang.cos() * (1.5 + 3.5 * b), 1.0 + 3.5 * c, ang.sin() * (1.5 + 3.5 * b));
        tf.translation = o + v * tau + Vec3::new(0.0, -9.0 * tau * tau * 0.5, 0.0);
        tf.scale = Vec3::splat((1.0 - tau / 0.55).max(0.05) * (0.6 + c));
        *vis = Visibility::Visible;
    }
    for (l, mut tf, mut pl) in &mut lights {
        if let Some((o, tau)) = origin(l.0) {
            tf.translation = o;
            pl.intensity = 900_000.0 * (-tau * 11.0).exp();
        }
    }
}

fn build_flakes(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let m = mats.add(StandardMaterial { base_color: Color::WHITE, unlit: true, ..default() });
    commands.insert_resource(FlakeMaterial(m.clone()));
    let mesh = meshes.add(Sphere::new(1.0).mesh().uv(10, 7));
    for i in 0..3600u32 {
        let seed = Vec3::new(hash(i * 3 + 1), hash(i * 3 + 2), hash(i * 3 + 3));
        let size = 0.03 + 0.05 * hash(i + 50_000).powf(2.0);
        commands.spawn((Flake { seed, size }, Mesh3d(mesh.clone()), MeshMaterial3d(m.clone()), Transform::from_scale(Vec3::splat(size)), NotShadowCaster, NotShadowReceiver));
    }
}

fn animate_flakes(clock: Res<Clock>, spec: Res<WorldSpec>, cam: Query<&Transform, (With<Camera3d>, Without<Flake>)>, mut q: Query<(&Flake, &mut Transform)>, fm: Res<FlakeMaterial>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let Ok(ct) = cam.single() else { return };
    let t = clock.t;
    let centre = ct.translation + ct.forward() * 22.0;
    let b = Vec3::new(64.0, 34.0, 64.0);
    for (f, mut tf) in &mut q {
        let sp = 0.7 + 0.6 * f.seed.x;
        let fall = t * (2.0 + 1.5 * f.seed.y) * sp;
        let wind = t * (2.2 + f.seed.z) * sp;
        let fr = |v: f32| v - v.floor();
        let x = fr(f.seed.x + wind / b.x + 0.02 * (t * 1.3 + f.seed.z * 9.0).sin());
        let y = fr(f.seed.y - fall / b.y);
        let z = fr(f.seed.z + wind * 0.3 / b.z);
        let wrap = |v: f32, c: f32, s: f32| c + (fr((v - (c / s)).rem_euclid(1.0)) - 0.5) * s;
        tf.translation = Vec3::new(wrap(x, centre.x, b.x), wrap(y, centre.y, b.y), wrap(z, centre.z, b.z)) + Vec3::new(0.35 * (t * 2.0 + f.seed.y * 20.0).sin(), 0.0, 0.3 * (t * 1.6 + f.seed.x * 20.0).cos());
        tf.scale = Vec3::splat(f.size);
    }
    let d = crate::lighting::daylight(spec.time_of_day);
    if let Some(mut m) = mats.get_mut(&fm.0) {
        let elev = crate::lighting::sun_direction(spec.time_of_day).y;
        let v = 0.30 + 0.70 * smoothstep(-0.1, 0.3, elev);
        m.base_color = Color::linear_rgb(v * (0.8 + 0.15 * d), v * (0.9 + 0.05 * d), v * 1.1);
        let _ = d;
    }
}

// ───────────────────────────── Camera & capture ─────────────────────────────

/// (time, eye, target, vertical fov)
const CAM: [(f32, [f32; 3], [f32; 3], f32); 6] = [
    (0.0, [-34.0, 34.0, 76.0], [0.0, 25.0, 22.0], 0.85),
    (3.0, [-11.5, 23.0, 31.8], [0.5, 22.0, 31.0], 0.60),
    (6.0, [-2.0, 23.4, 21.0], [0.3, 22.2, 31.5], 0.62),
    (9.0, [-32.0, 30.0, 52.0], [0.0, 26.0, 22.0], 0.78),
    (11.5, [-52.0, 50.0, 52.0], [0.0, 30.0, 14.0], 0.8),
    (14.0, [-30.0, 70.0, 100.0], [0.0, 30.0, 10.0], 0.85),
];

fn catmull(p0: Vec3, p1: Vec3, p2: Vec3, p3: Vec3, t: f32) -> Vec3 {
    0.5 * ((2.0 * p1) + (-p0 + p2) * t + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t * t + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t * t * t)
}

fn cam_at(t: f32) -> (Vec3, Vec3, f32) {
    let t = t.clamp(0.0, CAM[CAM.len() - 1].0);
    let mut i = 0;
    while i + 2 < CAM.len() && t > CAM[i + 1].0 {
        i += 1;
    }
    let lin = ((t - CAM[i].0) / (CAM[i + 1].0 - CAM[i].0)).clamp(0.0, 1.0);
    let u = smoothstep(0.0, 1.0, lin) * 0.4 + 0.6 * lin;
    let k = |j: i32| CAM[(j.clamp(0, CAM.len() as i32 - 1)) as usize];
    let (a, b, c, d) = (k(i as i32 - 1), k(i as i32), k(i as i32 + 1), k(i as i32 + 2));
    let eye = catmull(Vec3::from(a.1), Vec3::from(b.1), Vec3::from(c.1), Vec3::from(d.1), u);
    let tgt = catmull(Vec3::from(a.2), Vec3::from(b.2), Vec3::from(c.2), Vec3::from(d.2), u);
    (eye, tgt, b.3 + (c.3 - b.3) * smoothstep(0.0, 1.0, lin))
}

fn drive_camera(clock: Res<Clock>, mode: Res<ShowMode>, terrain: Option<Res<TerrainGrid>>, mut q: Query<(&mut Transform, &mut Projection), With<Camera3d>>) {
    let Ok((mut tf, mut proj)) = q.single_mut() else { return };
    let (mut eye, tgt, fov) = match &mode.0 {
        Mode::Shot { cam: Some(c), .. } => (Vec3::new(c[0], c[1], c[2]), Vec3::new(c[3], c[4], c[5]), 0.78),
        _ => cam_at(clock.t),
    };
    if let Some(st) = terrain {
        eye.y = eye.y.max(st.0.sample_height(eye.x, eye.z) + 3.0);
    }
    *tf = Transform::from_translation(eye).looking_at(tgt, Vec3::Y);
    if let Projection::Perspective(p) = &mut *proj {
        p.fov = fov;
        p.near = 0.1;
        p.far = 2500.0;
    }
}

fn capture(mut commands: Commands, mode: Res<ShowMode>, clock: Res<Clock>, mut progress: ResMut<Progress>, mut exit: MessageWriter<AppExit>) {
    let warm_needed: u32 = std::env::var("ARC_WARM").ok().and_then(|s| s.parse().ok()).unwrap_or(150);
    match &mode.0 {
        Mode::Live => {}
        Mode::Shot { out, .. } => {
            if progress.warm == warm_needed && progress.shot_frame.is_none() {
                progress.shot_frame = Some(clock.frame);
                commands.spawn(Screenshot::primary_window()).observe(save_to_disk(out.clone())).observe(|_: On<ScreenshotCaptured>, mut p: ResMut<Progress>| p.saved += 1);
            }
            if progress.saved > 0 {
                exit.write(AppExit::Success);
            }
        }
        Mode::Record { dir, fps } => {
            let env = |k: &str, d: u32| std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
            let (step, start) = (env("ARC_STEP", 1).max(1), env("ARC_START", 0));
            let total = (DURATION * *fps as f32) as u32;
            let count = (total.saturating_sub(start)).div_ceil(step);
            if progress.shot_frame.is_none() && progress.warm >= warm_needed {
                progress.shot_frame = Some(start);
            }
            if let Some(f) = progress.shot_frame {
                if f < total && progress.requested < count {
                    let path = format!("{dir}/frame_{f:04}.png");
                    commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path)).observe(|_: On<ScreenshotCaptured>, mut p: ResMut<Progress>| p.saved += 1);
                    progress.requested += 1;
                    progress.shot_frame = Some(f + step);
                }
                if progress.saved >= count {
                    exit.write(AppExit::Success);
                }
            }
        }
    }
}
