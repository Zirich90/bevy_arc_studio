//! arc_movement ECS layer: nav-raster upkeep, kinematic mover (terrain-snapped, no physics), actor separation,
//! BG3-style click-to-move preview (Play mode) and the world-level helpers behind `QueryPath` / `MoveActor` / `SpawnActor`.

use bevy::gizmos::config::GizmoConfigStore;
use bevy::gizmos::prelude::*;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};

use crate::arc_core::*;
use crate::camera::EditorCamera;
use crate::level_builder::{Door, GridSettings, LevelTile, tile_footprint};
use crate::navigation::*;
use crate::props::{ObjectCategory, SceneObject};
use crate::selection::Deleted;
use crate::studio::{StudioMode, StudioState};
use crate::terrain_render::{TerrainGrid, TerrainVersion};

/// PF2e base Speed for one Stride action.
pub const BASE_SPEED_FEET: f32 = 25.0;
/// A full Speed worth of walking takes this many seconds.
const SECONDS_PER_STRIDE: f32 = 3.0;
const CORNER_RADIUS: f32 = 0.15;
const PREVIEW_STEP: f32 = 0.5;
const PREVIEW_LIFT: f32 = 0.12;
const REBUILD_DEBOUNCE: f32 = 0.25;

#[derive(Component, Reflect, Clone, Debug)]
#[reflect(Component)]
#[require(Kinematics)]
pub struct Mover {
    /// Base Speed per Stride action in feet.
    pub speed_feet: f32,
    pub radius: f32,
}

impl Default for Mover {
    fn default() -> Self {
        Self { speed_feet: BASE_SPEED_FEET, radius: AGENT_RADIUS }
    }
}

#[derive(Component, Reflect, Clone, Debug, Default)]
#[reflect(Component)]
pub struct WaypointsPath {
    pub points: Vec<Vec3>,
    pub current_idx: usize,
    pub total_feet: f32,
}

#[derive(Component, Default)]
pub struct Kinematics {
    vel: Vec2,
}

/// Actor that receives click-to-move orders.
#[derive(Resource, Default)]
pub struct ActiveActor(pub Option<Entity>);

/// Set while an encounter owns the Play-mode input (click-to-move and door clicks are routed through its action economy).
#[derive(Resource, Default)]
pub struct CombatLock(pub bool);

#[derive(Resource)]
pub struct NavMap {
    pub grid: Option<NavGrid>,
    dirty: bool,
    changed_at: f32,
    water: f32,
    /// Bumped on every rebuild so cached previews know to recompute.
    pub revision: u32,
}

impl NavMap {
    /// Raster is out of date (an edit is waiting out the rebuild debounce).
    pub fn is_stale(&self) -> bool {
        self.dirty || self.grid.is_none()
    }
}

impl Default for NavMap {
    fn default() -> Self {
        Self { grid: None, dirty: true, changed_at: f32::MIN, water: f32::NAN, revision: 0 }
    }
}

#[derive(Default)]
pub struct Preview {
    pub points: Vec<(Vec3, bool)>,
    pub report: PathReport,
    pub path: Option<NavPath>,
    pub cursor: Vec2,
    pub hit: Vec3,
    from: Vec2,
    revision: u32,
    pub rough: bool,
}

impl Default for PathReport {
    fn default() -> Self {
        PathReport::UNREACHABLE
    }
}

#[derive(Resource, Default)]
pub struct PathPreview(pub Option<Preview>);

#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct PathGizmos;

pub struct MovementPlugin;

impl Plugin for MovementPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NavMap>()
            .init_resource::<CombatLock>()
            .init_resource::<ActiveActor>()
            .init_resource::<PathPreview>()
            .register_type::<Mover>()
            .register_type::<WaypointsPath>()
            .init_gizmo_group::<PathGizmos>()
            .add_systems(Startup, |mut store: ResMut<GizmoConfigStore>| {
                let (cfg, _) = store.config_mut::<PathGizmos>();
                cfg.depth_bias = -0.3;
                cfg.line.width = 5.0;
            })
            .add_systems(Update, (refresh_nav, play_input, move_actors.run_if(|s: Res<StudioState>| s.mode != StudioMode::Paused), draw_paths).chain())
            .add_systems(EguiPrimaryContextPass, draw_path_label);
    }
}

// ───────────────────────────── Nav raster upkeep ─────────────────────────────

/// Ground footprint blocking movement, or `None` for walkable props (grass, reeds, pier deck).
pub fn footprint_of(tf: &Transform, so: &SceneObject) -> Option<Footprint> {
    let c = Vec2::new(tf.translation.x, tf.translation.z);
    let h = so.aabb_half_extents;
    let yaw = yaw_of(tf.rotation);
    match so.category {
        ObjectCategory::Vegetation => (h.y > 1.5).then(|| Footprint::Circle { c, r: (h.x.max(h.z) * 0.22).max(0.25) }),
        ObjectCategory::Rock => Some(Footprint::Circle { c, r: h.x.max(h.z) * 0.85 }),
        ObjectCategory::Actor | ObjectCategory::Trigger | ObjectCategory::Level => None,
        ObjectCategory::Camp => Some(Footprint::Circle { c, r: 1.2 }),
        ObjectCategory::Structure if so.name.contains("Pier") => None,
        ObjectCategory::Structure => Some(Footprint::Circle { c, r: h.x.max(h.z) * 0.6 }),
        ObjectCategory::Model => Some(Footprint::Box { c, half: Vec2::new(h.x, h.z) * 0.5 * Vec2::new(tf.scale.x, tf.scale.z), yaw }),
    }
}

type TileQuery<'w, 's> = Query<'w, 's, (&'static LevelTile, Option<&'static Door>, &'static Transform), Without<Deleted>>;

fn rebuild(nav: &mut NavMap, grid: &HeightfieldGrid, water: f32, objs: &Query<(&Transform, &SceneObject), Without<Deleted>>, tiles: &TileQuery, cell_size: f32) {
    let mut fps: Vec<Footprint> = objs.iter().filter_map(|(tf, so)| footprint_of(tf, so)).collect();
    fps.extend(tiles.iter().filter_map(|(t, d, tf)| tile_footprint(t, d, tf, cell_size)));
    nav.grid = Some(NavGrid::build(grid, water, &fps, AGENT_RADIUS));
    nav.dirty = false;
    nav.water = water;
    nav.revision += 1;
}

#[allow(clippy::type_complexity)]
fn refresh_nav(
    time: Res<Time>,
    version: Res<TerrainVersion>,
    grid: Res<TerrainGrid>,
    spec: Res<WorldSpec>,
    // walking actors write their Transform every frame and never block the raster, so they must not keep it dirty
    changed: Query<(), (With<SceneObject>, Without<Mover>, Or<(Added<SceneObject>, Changed<Transform>, Added<Deleted>)>)>,
    objs: Query<(&Transform, &SceneObject), Without<Deleted>>,
    tiles: TileQuery,
    doors: Query<(), Changed<Door>>,
    gset: Res<GridSettings>,
    mut undeleted: RemovedComponents<Deleted>,
    mut gone: RemovedComponents<SceneObject>,
    mut nav: ResMut<NavMap>,
) {
    let any = !changed.is_empty() || !doors.is_empty() || undeleted.read().count() > 0 || gone.read().count() > 0;
    if any || version.is_changed() || grid.is_changed() {
        nav.dirty = true;
        nav.changed_at = time.elapsed_secs();
    } else if !nav.dirty && (nav.water.is_nan() || spec.water_level != nav.water) {
        // a standing difference (never-built raster, new water level) must not keep restarting the debounce
        nav.dirty = true;
        nav.changed_at = time.elapsed_secs();
    }
    if nav.dirty && time.elapsed_secs() - nav.changed_at >= REBUILD_DEBOUNCE {
        rebuild(&mut nav, &grid.0, spec.water_level, &objs, &tiles, gset.cell_size);
    }
}

/// Immediate rebuild (used by commands so queries never see a stale raster).
fn force_rebuild(grid: Res<TerrainGrid>, spec: Res<WorldSpec>, objs: Query<(&Transform, &SceneObject), Without<Deleted>>, tiles: TileQuery, gset: Res<GridSettings>, mut nav: ResMut<NavMap>) {
    if nav.dirty || nav.grid.is_none() {
        rebuild(&mut nav, &grid.0, spec.water_level, &objs, &tiles, gset.cell_size);
    }
}

// ───────────────────────────── Kinematic controller ─────────────────────────────

fn xz(p: Vec3) -> Vec2 {
    Vec2::new(p.x, p.z)
}

fn move_actors(
    time: Res<Time>,
    nav: Res<NavMap>,
    grid: Res<TerrainGrid>,
    units: Res<UnitScale>,
    mut commands: Commands,
    mut q: Query<(Entity, &Mover, &mut Transform, &mut Kinematics, Option<&mut WaypointsPath>)>,
) {
    let dt = time.delta_secs().min(0.1);
    let Some(navgrid) = nav.grid.as_ref() else { return };
    for (e, mv, mut tf, mut kin, path) in &mut q {
        let mut pos = xz(tf.translation);
        let mut desired = Vec2::ZERO;
        if let Some(mut path) = path {
            while path.current_idx < path.points.len() {
                let last = path.current_idx + 1 == path.points.len();
                let target = xz(path.points[path.current_idx]);
                let reach = if last { 0.04 } else { CORNER_RADIUS };
                if pos.distance(target) <= reach {
                    path.current_idx += 1;
                } else {
                    let pace = units.feet_to_world(mv.speed_feet) / SECONDS_PER_STRIDE;
                    let rough = if navgrid.at(pos) == NavCell::Difficult { 1.0 / DIFFICULT_COST } else { 1.0 };
                    let to_go = pos.distance(target);
                    // slow into the final point so the actor settles instead of orbiting it
                    let ease = if last { (to_go / 0.8).clamp(0.25, 1.0) } else { 1.0 };
                    desired = (target - pos) / to_go * pace * rough * ease;
                    break;
                }
            }
            if path.current_idx >= path.points.len() {
                commands.entity(e).remove::<WaypointsPath>();
                kin.vel = Vec2::ZERO;
            }
        }
        kin.vel = kin.vel.lerp(desired, 1.0 - (-dt / 0.08).exp());
        let step = kin.vel * dt;
        let next = pos + step;
        if navgrid.at(next) != NavCell::Blocked {
            pos = next;
        } else {
            kin.vel = Vec2::ZERO;
        }
        let ground = Vec3::new(pos.x, grid.0.sample_height(pos.x, pos.y), pos.y);
        if tf.translation != ground {
            tf.translation = ground;
        }
        if kin.vel.length_squared() > 0.04 {
            let want = Quat::from_rotation_y((-kin.vel.x).atan2(-kin.vel.y));
            tf.rotation = tf.rotation.slerp(want, 1.0 - (-dt / 0.07).exp());
        }
    }

    // local avoidance: positional push-apart (equal weights) that never pushes into blocked cells
    let mut pairs = q.iter_combinations_mut();
    while let Some([(_, a, mut ta, ..), (_, b, mut tb, ..)]) = pairs.fetch_next() {
        let (pa, pb) = (xz(ta.translation), xz(tb.translation));
        let min = a.radius + b.radius;
        let d = pa.distance(pb);
        if d >= min {
            continue;
        }
        let dir = if d > 1e-4 { (pa - pb) / d } else { Vec2::X };
        let push = dir * (min - d) * 0.5;
        for (tf, p, s) in [(&mut ta, pa, push), (&mut tb, pb, -push)] {
            let np = p + s;
            if navgrid.at(np) != NavCell::Blocked {
                tf.translation = Vec3::new(np.x, grid.0.sample_height(np.x, np.y), np.y);
            }
        }
    }
}

// ───────────────────────────── Path helpers ─────────────────────────────

/// Terrain-hugging samples along the polyline; flag marks difficult ground.
pub(crate) fn sample_polyline(nav: &NavGrid, grid: &HeightfieldGrid, path: &NavPath, lift: f32) -> Vec<(Vec3, bool)> {
    let mut out = Vec::new();
    let lift_pt = |p: Vec2| Vec3::new(p.x, grid.sample_height(p.x, p.y) + lift, p.y);
    for w in path.points.windows(2) {
        let n = ((w[0].distance(w[1]) / PREVIEW_STEP).ceil() as usize).max(1);
        for k in 0..n {
            let p = w[0].lerp(w[1], k as f32 / n as f32);
            out.push((lift_pt(p), nav.at(w[0].lerp(w[1], (k as f32 + 0.5) / n as f32)) == NavCell::Difficult));
        }
    }
    if let Some(l) = path.points.last() {
        out.push((lift_pt(*l), false));
    }
    out
}

pub(crate) fn ray_point_distance(ray: &Ray3d, p: Vec3) -> f32 {
    let t = (p - ray.origin).dot(*ray.direction).max(0.0);
    p.distance(ray.origin + *ray.direction * t)
}

fn waypoints_from(path: &NavPath, grid: &HeightfieldGrid, units: &UnitScale) -> WaypointsPath {
    let points: Vec<Vec3> = path.points.iter().map(|p| Vec3::new(p.x, grid.sample_height(p.x, p.y), p.y)).collect();
    // first point is where the actor already stands
    WaypointsPath { points, current_idx: 1, total_feet: units.world_to_feet(path.length) }
}

// ───────────────────────────── Play-mode input ─────────────────────────────

#[allow(clippy::too_many_arguments)]
fn play_input(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    studio: Res<StudioState>,
    nav: Res<NavMap>,
    grid: Res<TerrainGrid>,
    units: Res<UnitScale>,
    window: Single<&Window, With<PrimaryWindow>>,
    cam: Single<(&Camera, &GlobalTransform), With<EditorCamera>>,
    movers: Query<(Entity, &Transform), With<Mover>>,
    mut egui: EguiContexts,
    mut active: ResMut<ActiveActor>,
    mut preview: ResMut<PathPreview>,
    mut commands: Commands,
    mut doors: Query<(&LevelTile, &mut Door), Without<Deleted>>,
    gset: Res<GridSettings>,
    lock: Res<CombatLock>,
) {
    if active.0.is_none_or(|e| movers.get(e).is_err()) {
        active.0 = movers.iter().next().map(|(e, _)| e);
    }
    if lock.0 {
        preview.0 = None;
        return;
    }
    let blocked = egui.ctx_mut().map(|c| c.is_pointer_over_egui() || c.egui_wants_pointer_input()).unwrap_or(false);
    let alt = keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]);
    let cursor = window.cursor_position();
    let (Some(cursor), Some(navgrid), false, StudioMode::Playing) = (cursor, nav.grid.as_ref(), blocked, studio.mode) else {
        preview.0 = None;
        return;
    };
    let (camera, gtf) = *cam;
    let Ok(ray) = camera.viewport_to_world(gtf, cursor) else {
        preview.0 = None;
        return;
    };
    let clicked = mouse.just_pressed(MouseButton::Left) && !alt;

    // click on a door opens / closes it (locked doors stay shut)
    if clicked {
        let (o, d) = (ray.origin, *ray.direction);
        let cs = gset.cell_size;
        let hit = doors
            .iter()
            .enumerate()
            .filter_map(|(i, (t, _))| {
                let c = gset.cell_center(t.grid_pos);
                let (lo, hi) = (c - Vec3::new(cs * 0.5, 0.0, cs * 0.5), c + Vec3::new(cs * 0.5, gset.floor_height, cs * 0.5));
                crate::level_builder::ray_aabb(o, d, lo, hi).map(|dist| (i, dist))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((i, _)) = hit {
            if let Some((_, mut door)) = doors.iter_mut().nth(i) {
                if !door.is_open && door.is_locked {
                    return;
                }
                door.is_open = !door.is_open;
                return;
            }
        }
    }

    // click on an actor selects it
    if clicked {
        let pick = movers
            .iter()
            .map(|(e, tf)| (e, ray_point_distance(&ray, tf.translation + Vec3::Y * 0.8)))
            .filter(|(_, d)| *d < 0.9)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((e, _)) = pick {
            active.0 = Some(e);
            return;
        }
    }

    let (Some(actor), Some(hit)) = (active.0, grid.0.raycast(ray.origin, *ray.direction)) else {
        preview.0 = None;
        return;
    };
    let Ok((_, atf)) = movers.get(actor) else { return };
    let from = xz(atf.translation);
    let to = Vec2::new(hit.x, hit.z);

    let stale = preview.0.as_ref().is_none_or(|p| p.revision != nav.revision || p.from.distance(from) > 0.25 || xz(p.hit).distance(to) > 0.25);
    if stale {
        let path = navgrid.find_path(from, to);
        preview.0 = Some(match path {
            Some(p) => {
                let pts = sample_polyline(navgrid, &grid.0, &p, PREVIEW_LIFT);
                let report = p.report(&units);
                let rough = report.cost_feet > report.distance_feet + 0.5;
                Preview { points: pts, report, path: Some(p), cursor, hit, from, revision: nav.revision, rough }
            }
            None => Preview { cursor, hit, from, revision: nav.revision, ..default() },
        });
    } else if let Some(p) = preview.0.as_mut() {
        p.cursor = cursor;
    }

    if clicked {
        if let Some(path) = preview.0.as_ref().and_then(|p| p.path.as_ref()) {
            commands.entity(actor).insert(waypoints_from(path, &grid.0, &units));
        }
    }
}

// ───────────────────────────── Drawing ─────────────────────────────

const GLOW_GREEN: Color = Color::srgb(0.25, 1.0, 0.45);
const GLOW_YELLOW: Color = Color::srgb(1.0, 0.85, 0.2);
const OVER_RED: Color = Color::srgb(1.0, 0.3, 0.25);

pub(crate) fn ring(g: &mut Gizmos<PathGizmos>, grid: &HeightfieldGrid, c: Vec2, r: f32, color: Color) {
    let at = |a: f32| {
        let p = c + Vec2::from_angle(a) * r;
        Vec3::new(p.x, grid.sample_height(p.x, p.y) + PREVIEW_LIFT, p.y)
    };
    const N: usize = 28;
    for i in 0..N {
        g.line(at(i as f32 / N as f32 * std::f32::consts::TAU), at((i + 1) as f32 / N as f32 * std::f32::consts::TAU), color);
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_paths(
    studio: Res<StudioState>,
    grid: Res<TerrainGrid>,
    active: Res<ActiveActor>,
    preview: Res<PathPreview>,
    nav: Res<NavMap>,
    lock: Res<CombatLock>,
    movers: Query<(Entity, &Transform, &Mover, Option<&WaypointsPath>)>,
    mut g: Gizmos<PathGizmos>,
) {
    let lift = |p: Vec3| Vec3::new(p.x, grid.0.sample_height(p.x, p.z) + PREVIEW_LIFT, p.z);
    for (e, tf, mv, wp) in &movers {
        if studio.mode == StudioMode::Playing && active.0 == Some(e) && !lock.0 {
            ring(&mut g, &grid.0, xz(tf.translation), mv.radius + 0.25, GLOW_GREEN);
        }
        // remaining route of a walking actor
        if let (Some(wp), Some(navgrid)) = (wp, nav.grid.as_ref()) {
            let pts: Vec<Vec2> = std::iter::once(xz(tf.translation)).chain(wp.points[wp.current_idx.min(wp.points.len())..].iter().map(|p| xz(*p))).collect();
            if pts.len() > 1 {
                let route = NavPath { length: 0.0, cost: 0.0, points: pts };
                let s = sample_polyline(navgrid, &grid.0, &route, PREVIEW_LIFT);
                for w in s.windows(2) {
                    g.line(w[0].0, w[1].0, GLOW_GREEN.with_alpha(0.55));
                }
            }
        }
    }
    let Some(p) = preview.0.as_ref().filter(|_| studio.mode == StudioMode::Playing) else { return };
    if p.path.is_none() {
        ring(&mut g, &grid.0, Vec2::new(p.hit.x, p.hit.z), 0.45, OVER_RED);
        return;
    }
    let over = p.report.cost_feet > BASE_SPEED_FEET + 0.5;
    for w in p.points.windows(2) {
        let col = if w[0].1 || over { GLOW_YELLOW } else { GLOW_GREEN };
        g.line(w[0].0, w[1].0, col);
        g.line(lift(w[0].0) + Vec3::Y * 0.04, lift(w[1].0) + Vec3::Y * 0.04, col.with_alpha(0.5));
    }
    let end = p.points.last().map_or(p.hit, |x| x.0);
    ring(&mut g, &grid.0, Vec2::new(end.x, end.z), 0.45, if p.rough || over { GLOW_YELLOW } else { GLOW_GREEN });
}

fn draw_path_label(mut contexts: EguiContexts, studio: Res<StudioState>, preview: Res<PathPreview>) {
    let Some(p) = preview.0.as_ref().filter(|_| studio.mode == StudioMode::Playing) else { return };
    let Ok(ctx) = contexts.ctx_mut() else { return };
    let (text, color) = if p.path.is_none() {
        ("No path".to_string(), egui::Color32::from_rgb(255, 90, 80))
    } else {
        let over = p.report.cost_feet > BASE_SPEED_FEET + 0.5;
        let warn = over || p.rough;
        (format!("Path: {:.0} ft", p.report.cost_feet), if warn { egui::Color32::from_rgb(255, 214, 64) } else { egui::Color32::from_rgb(90, 255, 140) })
    };
    let frame = egui::Frame::new().fill(egui::Color32::from_rgba_unmultiplied(8, 11, 16, 220)).stroke(egui::Stroke::new(1.0, color)).corner_radius(7).inner_margin(egui::Margin::symmetric(9, 5));
    egui::Area::new(egui::Id::new("path_label")).fixed_pos(egui::pos2(p.cursor.x + 18.0, p.cursor.y - 34.0)).order(egui::Order::Tooltip).interactable(false).show(ctx, |ui| {
        frame.show(ui, |ui| {
            ui.label(egui::RichText::new(text).monospace().strong().size(14.0).color(color));
        });
    });
}

// ───────────────────────────── Command backends (run with &mut World) ─────────────────────────────

fn ready(w: &mut World) {
    let _ = w.run_system_cached(force_rebuild);
}

fn plan(w: &mut World, from: Vec2, to: Vec2) -> Result<Option<NavPath>, String> {
    ready(w);
    let nav = w.resource::<NavMap>();
    Ok(nav.grid.as_ref().ok_or("navigation raster not ready")?.find_path(from, to))
}

pub fn cmd_query_path(w: &mut World, from: Vec3, to: Vec3) -> Result<String, String> {
    let path = plan(w, xz(from), xz(to))?;
    let report = path.map_or(PathReport::UNREACHABLE, |p| p.report(w.resource::<UnitScale>()));
    serde_json::to_string(&report).map_err(|e| e.to_string())
}

pub fn cmd_move_actor(w: &mut World, entity: Entity, to: Vec3) -> Result<String, String> {
    let from = w.get::<Transform>(entity).filter(|_| w.get::<Mover>(entity).is_some()).map(|t| xz(t.translation)).ok_or("entity is not a Mover")?;
    let path = plan(w, from, xz(to))?.ok_or("destination unreachable")?;
    let units = *w.resource::<UnitScale>();
    let wp = waypoints_from(&path, &w.resource::<TerrainGrid>().0, &units);
    let report = path.report(&units);
    w.entity_mut(entity).insert(wp);
    w.resource_mut::<ActiveActor>().0 = Some(entity);
    Ok(format!("move {entity} {:.1} ft (cost {:.1} ft)", report.distance_feet, report.cost_feet))
}

pub fn cmd_spawn_actor(w: &mut World, x: f32, z: f32) -> Result<String, String> {
    ready(w);
    let (spot, h) = {
        let nav = w.resource::<NavMap>().grid.as_ref().ok_or("navigation raster not ready")?.nearest_free(Vec2::new(x, z)).ok_or("no walkable ground near spawn point")?;
        (nav, w.resource::<TerrainGrid>().0.sample_height(nav.x, nav.y))
    };
    let body = w.resource_mut::<Assets<Mesh>>().add(Capsule3d::new(0.3, 1.0));
    let nose = w.resource_mut::<Assets<Mesh>>().add(Cuboid::new(0.16, 0.16, 0.32));
    let (mat_body, mat_nose) = {
        let mut mats = w.resource_mut::<Assets<StandardMaterial>>();
        (
            mats.add(StandardMaterial { base_color: Color::srgb(0.95, 0.45, 0.12), perceptual_roughness: 0.6, ..default() }),
            mats.add(StandardMaterial { base_color: Color::srgb(0.98, 0.95, 0.85), perceptual_roughness: 0.5, ..default() }),
        )
    };
    let e = w
        .spawn((Name::new("TestActor"), Mover::default(), Transform::from_xyz(spot.x, h, spot.y), Visibility::default()))
        .with_children(|c| {
            c.spawn((Name::new("ActorBody"), Mesh3d(body), MeshMaterial3d(mat_body), Transform::from_xyz(0.0, 0.8, 0.0)));
            c.spawn((Name::new("ActorNose"), Mesh3d(nose), MeshMaterial3d(mat_nose), Transform::from_xyz(0.0, 1.25, -0.3)));
        })
        .id();
    w.resource_mut::<ActiveActor>().0 = Some(e);
    Ok(format!("spawned actor {e} at ({:.1}, {:.1})", spot.x, spot.y))
}
