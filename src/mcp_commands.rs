//! Two-way command bus: the UI and external AI agents (BRP / MCP) push `StudioCommand`s into `StudioCommandQueue`.
//!
//! Agent usage (bevy_remote): `world.mutate_resources` / `world.insert_resources` on
//! `bevy_arc_studio::mcp_commands::StudioCommandQueue` with e.g.
//! `{"commands":[{"ApplyPreset":"MistyDuskLake"},{"SpawnObject":{"kind":"pine","x":10.0,"z":-4.0,"yaw":0.5}}],"log":[]}`;
//! results are appended to `log` the next frame.

use std::sync::Arc;

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use serde::{Deserialize, Serialize};

use crate::arc_core::*;
use crate::camera::EditorCamera;
use crate::gizmo_3d::{GizmoMode, GizmoState, ObjState, SceneHistory};
use crate::level_builder::{self as lb, TileKind, WallStyle};
use crate::movement;
use crate::operators::{self, CommandResults, DEFERRED, Target};
use crate::props::{SceneObject, UserProp, assets_dir, model_asset_path};
use crate::selection::{Deleted, EditorSelection, object_box};
use crate::studio::*;
use crate::terrain_brush::{BrushKind, BrushState, height_dab, union_rect};
use crate::terrain_render::{PendingHeights, TerrainGrid, TerrainMesh, update_terrain_region};
use crate::world_io::{SceneIo, SceneIoQueue, is_reflect_path};

#[derive(Reflect, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum StudioCommand {
    ApplyPreset(WorldPreset),
    SetQuality(QualityPreset),
    SetTime(f32),
    SetMode(StudioMode),
    /// Play-in-Editor: `Playing` snapshots the scene, `Edit` (Stop) rolls it back, `Paused` freezes game systems.
    SetPlayMode { mode: StudioMode },
    SetTab(InspectorTab),
    SetBrush { kind: BrushKind, radius: f32, strength: f32 },
    /// kind: pine | boulder | campfire | pier | lantern | model:<file.glb in assets/models | path/under/assets.glb>
    SpawnObject { kind: String, x: f32, z: f32, yaw: f32 },
    /// Hold a height brush at (x, z) for `seconds` (dab rate 30 Hz).
    Sculpt { kind: BrushKind, x: f32, z: f32, radius: f32, strength: f32, seconds: f32 },
    /// Select the newest object with this `Name` (e.g. "CampfireSite", "PineTree").
    Select { name: String },
    DeleteSelected,
    SetCameraStyle(CamStyle),
    SetCamera { pivot: [f32; 3], yaw: f32, pitch: f32, distance: f32 },
    FocusSelected,
    Undo,
    Redo,
    Screenshot { path: String },
    SaveScene { path: String },
    LoadScene { path: String },
    /// Spawn a test character on walkable ground nearest to (x, z); it becomes the active actor.
    SpawnActor { x: f32, z: f32 },
    /// Result line: `{"distance_meters","distance_feet","cost_feet","is_reachable"}`.
    QueryPath { from: Vec3, to: Vec3 },
    MoveActor { entity: Entity, to: Vec3 },
    /// Gizmo tool and snapping; `None` leaves a field unchanged.
    SetGizmo { mode: Option<GizmoMode>, snap_ground: Option<bool>, align_normal: Option<bool> },
    /// Select by `SceneObject.id` (see ListObjects); `additive` keeps the current selection.
    SelectIds { ids: Vec<u32>, additive: bool },
    /// Edit a transform (target: `id` or the primary selection); rot_deg = (x, y, z) Euler YXZ; undoable.
    SetTransform { id: Option<u32>, pos: Option<Vec3>, rot_deg: Option<Vec3>, scale: Option<Vec3>, drop_to_ground: bool },
    /// Copy the selected editor-spawned props, offset by `offset` (x, z).
    Duplicate { offset: [f32; 2] },
    /// Set a WorldSpec field by reflect path (e.g. "lake_radius", "fog_enabled"); bool as 0/1; undoable.
    SetSpec { field: String, value: f64 },
    /// Result line: JSON array `[{"id","name","kind","pos":[x,y,z]}]` (max 300).
    ListObjects,
    /// Result data: JSON schema of every operator (`{"operators":[{name,undoable,kind,params}]}`).
    Describe,
    /// Result data: selection, per-name counts, and (up to `limit`, default 100, name contains `filter`) hierarchy paths, kinds, bounds, component names.
    DescribeScene { filter: Option<String>, limit: Option<u32> },
    /// Patch fields of an existing component; `value` is JSON in reflect-serde form (partial objects merge); undoable.
    SetComponent { target: Target, component: String, value: String },
    /// Insert a component by type name (short or full path); `value` JSON or Default; undoable.
    AddComponent { target: Target, component: String, value: Option<String> },
    RemoveComponent { target: Target, component: String },
    /// Move `child` under `parent` (None = root), keeping world position; undoable.
    Reparent { child: Target, parent: Option<Target> },
    /// Modular building: floor over the rectangle `min..=max` (grid cells, any corner order), pillars on corners, walls on the perimeter; one undo step.
    BuildRoom { min: [i32; 2], max: [i32; 2], floor: i32, style: WallStyle },
    /// Row of walls from `from` along the dominant axis to `to`; one undo step.
    BuildWallLine { from: [i32; 2], to: [i32; 2], floor: i32, style: WallStyle },
    /// One tile at grid cell `pos = [x, storey, z]`. Replaces the tile in the same slot (wall→doorway); `facing`/`style` None inherit the wall there.
    PlaceLevelTile { kind: TileKind, pos: [i32; 3], facing: Option<u8>, style: Option<WallStyle> },
    /// Brush stroke over many cells (floor paint, or `erase` = remove the wall layer / floor of each cell); one undo step.
    PaintLevelTiles { kind: TileKind, cells: Vec<[i32; 3]>, style: WallStyle, erase: bool },
    /// Open/close (`open: None` toggles) the door in a cell; locked doors refuse to open; undoable.
    ToggleDoor { pos: [i32; 3], open: Option<bool> },
    /// Snap grid and storey: `None` leaves a field unchanged. `floor_level` lifts the building plane to `Y = floor_level * floor_height`.
    SetGrid { enabled: Option<bool>, cell_size: Option<f32>, floor_level: Option<i32>, floor_height: Option<f32>, angle_snap_deg: Option<f32> },
    /// Own water level for the basin at world (x, z): fills a moat or drains a pond without touching the global level. `level: None` removes that body's override; undoable.
    SetWaterBody { x: f32, z: f32, level: Option<f32> },
    /// Bind (`enabled`) or unbind the building plane from the terrain. Unbound: storey 0 is `Y = 0`. `site` [x, z] re-grounds on the terrain there.
    SetGroundSnap { enabled: bool, site: Option<[f32; 2]> },
    /// Save the selected objects (with descendants) as `assets/prefabs/<name>.prefab.json`; `path` may be a bare name. Not a scene edit.
    SaveSelectionAsPrefab { path: String },
    /// Instance a prefab at `position` (grid / ground snapped per settings) turned by `rotation_y_deg`; one undo step.
    SpawnPrefab { path: String, position: Vec3, rotation_y_deg: f32 },
    /// Switch a module (its `module.json` found in `modules/`) on or off. Not undoable.
    SetModule { id: String, enabled: bool },
    /// Re-read the `modules/` folder for module files.
    RescanModules,
    /// Copy a `module.json` (or a module folder) from `path` into `modules/` and rescan.
    ConnectModule { path: String },
    /// Command contributed by a plugin through [`CustomCommands`]; `args` is a JSON document the handler parses itself.
    Custom { name: String, args: String },
}

pub type CustomCommandFn = Arc<dyn Fn(&mut World, &str) -> Result<String, String> + Send + Sync>;

/// Plugin-contributed `StudioCommand::Custom` handlers, looked up by `name` (use `"<plugin>.<verb>"`).
/// Handlers of an inactive module (see `modules`) are not found.
#[derive(Resource, Default, Clone)]
pub struct CustomCommands {
    entries: Vec<(String, CustomCommandFn, Option<String>)>,
    pub scope: Option<String>,
    pub inactive: std::collections::HashSet<String>,
}

impl CustomCommands {
    pub fn register(&mut self, name: &str, f: CustomCommandFn) {
        match self.entries.iter_mut().find(|(n, ..)| n == name) {
            Some(e) => (e.1, e.2) = (f, self.scope.clone()),
            None => self.entries.push((name.to_string(), f, self.scope.clone())),
        }
    }

    pub fn get(&self, name: &str) -> Option<CustomCommandFn> {
        self.entries.iter().find(|(n, _, o)| n == name && o.as_ref().is_none_or(|o| !self.inactive.contains(o))).map(|(_, f, _)| f.clone())
    }
}

pub trait CustomCommandAppExt {
    fn register_custom_command(&mut self, name: &str, f: impl Fn(&mut World, &str) -> Result<String, String> + Send + Sync + 'static) -> &mut Self;
}

impl CustomCommandAppExt for App {
    fn register_custom_command(&mut self, name: &str, f: impl Fn(&mut World, &str) -> Result<String, String> + Send + Sync + 'static) -> &mut Self {
        self.world_mut().get_resource_or_insert_with(CustomCommands::default).register(name, Arc::new(f));
        self
    }
}

#[derive(Resource, Reflect, Default, Serialize, Deserialize)]
#[reflect(Resource)]
pub struct StudioCommandQueue {
    pub commands: Vec<StudioCommand>,
    /// Result lines of executed commands (newest last, capped).
    pub log: Vec<String>,
}

impl StudioCommandQueue {
    pub fn push(&mut self, c: StudioCommand) {
        self.commands.push(c);
    }
}

#[derive(Serialize, Deserialize)]
struct SavedObject {
    kind: String,
    translation: [f32; 3],
    rotation: [f32; 4],
    scale: [f32; 3],
}

#[derive(Serialize, Deserialize)]
struct SceneFile {
    version: u32,
    spec: WorldSpec,
    heights: Vec<f32>,
    objects: Vec<SavedObject>,
}

pub struct McpCommandsPlugin;

impl Plugin for McpCommandsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(operators::OperatorsPlugin)
            .init_resource::<StudioCommandQueue>()
            .init_resource::<CustomCommands>()
            .register_type::<StudioCommandQueue>()
            .register_type::<StudioCommand>()
            .register_type::<QualityPreset>()
            .register_type::<WorldPreset>()
            .add_systems(Update, execute_commands.before(process_history));
    }
}

/// Run `f` with world access after this system; its result line lands in the command log (`ERR ` prefix on failure).
fn queue_world_cmd(commands: &mut Commands, op: &'static str, f: impl FnOnce(&mut World) -> Result<String, String> + Send + 'static) -> Result<String, String> {
    commands.queue(move |w: &mut World| {
        let res = f(w);
        operators::push_result(w, op, res);
    });
    Ok(DEFERRED.into())
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct Extra<'w> {
    gz: ResMut<'w, GizmoState>,
    req: ResMut<'w, HistoryRequest>,
    results: ResMut<'w, CommandResults>,
    custom: Res<'w, CustomCommands>,
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn execute_commands(
    mut queue: ResMut<StudioCommandQueue>,
    mut spec: ResMut<WorldSpec>,
    mut wh: ResMut<UndoHistory<WorldSpec>>,
    mut sh: ResMut<SceneHistory>,
    mut th: ResMut<TerrainHistory>,
    mut grid: ResMut<TerrainGrid>,
    mut pending: ResMut<PendingHeights>,
    mut studio: ResMut<StudioState>,
    mut brush: ResMut<BrushState>,
    mut sel: ResMut<EditorSelection>,
    mut extra: Extra,
    mut cam: Query<(&mut EditorCamera, &Projection)>,
    terrain: Query<&Mesh3d, With<TerrainMesh>>,
    objs: Query<(Entity, &Transform, &SceneObject, Option<&UserProp>), Without<Deleted>>,
    users: Query<(Entity, &Transform, &UserProp), Without<Deleted>>,
    mut spawner: Spawner,
) {
    if queue.commands.is_empty() {
        return;
    }
    let cmds = std::mem::take(&mut queue.commands);
    for c in cmds {
        let res: Result<String, String> = (|| match c.clone() {
            StudioCommand::ApplyPreset(p) => {
                let next = apply_preset(&spec, p);
                wh.record(format!("Preset {p:?}"), spec.clone(), next.clone());
                *spec = next;
                Ok(format!("preset {p:?}"))
            }
            StudioCommand::SetQuality(q) => {
                let mut next = spec.clone();
                next.quality_preset = q;
                wh.record("Quality", spec.clone(), next.clone());
                *spec = next;
                Ok(format!("quality {q:?}"))
            }
            StudioCommand::SetTime(t) => {
                let mut next = spec.clone();
                next.time_of_day = t.rem_euclid(24.0);
                wh.record("Time of day", spec.clone(), next.clone());
                *spec = next;
                Ok(format!("time {t:.2}"))
            }
            StudioCommand::SetPlayMode { mode } => queue_world_cmd(&mut spawner.commands, "SetPlayMode", move |w| crate::play_mode::cmd_set_play_mode(w, mode)),
            StudioCommand::SetMode(m) => {
                studio.mode = m;
                Ok(format!("mode {m:?}"))
            }
            StudioCommand::SetTab(t) => {
                studio.tab = t.panel_id().to_string();
                Ok(format!("tab {t:?}"))
            }
            StudioCommand::SetBrush { kind, radius, strength } => {
                brush.kind = kind;
                brush.radius = radius.clamp(2.0, 45.0);
                brush.strength = strength.clamp(0.1, 30.0);
                Ok(format!("brush {kind:?}"))
            }
            StudioCommand::SpawnObject { kind, x, z, yaw } => {
                if let Some(file) = kind.strip_prefix("model:") {
                    if !assets_dir().join(model_asset_path(file)).exists() {
                        return Err(format!("model not found: assets/{}", model_asset_path(file)));
                    }
                }
                let half = grid.0.size * 0.5;
                let (x, z) = (x.clamp(-half, half), z.clamp(-half, half));
                let (e, tf) = spawner.spawn(&grid.0, &spec, &kind, x, z, yaw).ok_or_else(|| format!("cannot spawn '{kind}'"))?;
                sh.record(format!("Spawn {kind}"), vec![ObjState { entity: e, tf, alive: false }], vec![ObjState { entity: e, tf, alive: true }]);
                sel.set_one(e);
                Ok(format!("spawned {kind} at ({x:.1}, {z:.1})"))
            }
            StudioCommand::Sculpt { kind, x, z, radius, strength, seconds } => {
                let mode = kind.mode().ok_or("Sculpt needs Raise|Lower|Smooth|Flatten")?;
                let before = grid.0.heights.clone();
                let target = grid.0.sample_height(x, z);
                let mut rect = None;
                for _ in 0..((seconds.clamp(0.0, 20.0) * 30.0).ceil() as usize).max(1) {
                    rect = Some(union_rect(rect, height_dab(&mut grid.0, mode, x, z, radius.clamp(2.0, 45.0), strength, 1.0 / 30.0, target, spec.max_height)));
                }
                if let (Some(rect), Ok(m)) = (rect, terrain.single()) {
                    if let Some(mut mesh) = spawner.meshes.get_mut(&m.0) {
                        update_terrain_region(&mut mesh, &grid.0, &spec, rect);
                    }
                }
                th.record("Sculpt", before, grid.0.heights.clone());
                Ok(format!("sculpt {kind:?} at ({x:.1}, {z:.1}) r={radius}"))
            }
            StudioCommand::Select { name } => {
                let best = objs.iter().filter(|(_, _, so, _)| so.name == name).max_by_key(|(_, _, so, _)| so.id).map(|(e, ..)| e);
                let e = best.ok_or(format!("no object named '{name}'"))?;
                sel.set_one(e);
                Ok(format!("selected {name}"))
            }
            StudioCommand::DeleteSelected => {
                let (mut b, mut a) = (Vec::new(), Vec::new());
                for e in sel.selected.iter() {
                    if let Ok((_, tf, ..)) = objs.get(*e) {
                        b.push(ObjState { entity: *e, tf: *tf, alive: true });
                        a.push(ObjState { entity: *e, tf: *tf, alive: false });
                        spawner.commands.entity(*e).insert((Deleted, Visibility::Hidden));
                    }
                }
                let n = a.len();
                sh.record("Delete", b, a);
                sel.clear();
                Ok(format!("deleted {n}"))
            }
            StudioCommand::SetCameraStyle(style) => {
                let (mut ec, _) = cam.single_mut().map_err(|e| e.to_string())?;
                let eye = ec.eye();
                let fwd = ec.forward();
                match style {
                    CamStyle::Fly => {
                        ec.distance = 1.0;
                        ec.pivot = eye + fwd;
                    }
                    CamStyle::Orbit => {
                        ec.distance = 35.0;
                        let mut p = eye + fwd * 35.0;
                        p.y = p.y.max(grid.0.sample_height(p.x, p.z));
                        ec.pivot = p;
                    }
                }
                ec.focus = None;
                studio.cam_style = style;
                Ok(format!("camera {style:?}"))
            }
            StudioCommand::SetCamera { pivot, yaw, pitch, distance } => {
                let (mut ec, _) = cam.single_mut().map_err(|e| e.to_string())?;
                (ec.pivot, ec.yaw, ec.pitch, ec.distance, ec.focus) = (Vec3::from(pivot), yaw, pitch, distance.max(0.1), None);
                Ok("camera set".into())
            }
            StudioCommand::FocusSelected => {
                let (mut ec, proj) = cam.single_mut().map_err(|e| e.to_string())?;
                let fov = if let Projection::Perspective(p) = proj { p.fov } else { 1.0 };
                let p = sel.primary.and_then(|e| objs.get(e).ok());
                let (c, r) = match p {
                    Some((_, tf, so, _)) => {
                        let (c, h, _) = object_box(tf, so, None);
                        (c, h.length().max(1.0) * 1.2)
                    }
                    None => (ec.pivot, 6.0),
                };
                ec.focus_on(c, r, fov);
                Ok("focus".into())
            }
            StudioCommand::Undo => {
                extra.req.undo = true;
                Ok("undo".into())
            }
            StudioCommand::Redo => {
                extra.req.redo = true;
                Ok("redo".into())
            }
            StudioCommand::Screenshot { path } => {
                if let Some(dir) = std::path::Path::new(&path).parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                spawner.commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path.clone()));
                Ok(format!("screenshot -> {path}"))
            }
            StudioCommand::SpawnActor { x, z } => {
                queue_world_cmd(&mut spawner.commands, "SpawnActor", move |w| movement::cmd_spawn_actor(w, x, z))
            }
            StudioCommand::QueryPath { from, to } => {
                queue_world_cmd(&mut spawner.commands, "QueryPath", move |w| movement::cmd_query_path(w, from, to))
            }
            StudioCommand::MoveActor { entity, to } => {
                queue_world_cmd(&mut spawner.commands, "MoveActor", move |w| movement::cmd_move_actor(w, entity, to))
            }
            StudioCommand::SetModule { id, enabled } => queue_world_cmd(&mut spawner.commands, "SetModule", move |w| crate::modules::set_module(w, &id, enabled)),
            StudioCommand::RescanModules => queue_world_cmd(&mut spawner.commands, "RescanModules", crate::modules::rescan),
            StudioCommand::ConnectModule { path } => queue_world_cmd(&mut spawner.commands, "ConnectModule", move |w| crate::modules::connect_module(w, &path)),
            StudioCommand::Custom { name, args } => {
                let f = extra.custom.get(&name).ok_or_else(|| format!("unknown custom command '{name}'"))?;
                queue_world_cmd(&mut spawner.commands, "Custom", move |w| f(w, &args))
            }
            StudioCommand::SetGizmo { mode, snap_ground, align_normal } => {
                if let Some(m) = mode {
                    extra.gz.mode = m;
                }
                extra.gz.snap_ground = snap_ground.unwrap_or(extra.gz.snap_ground);
                extra.gz.align_normal = align_normal.unwrap_or(extra.gz.align_normal);
                Ok(format!("gizmo {:?} snap={} align={}", extra.gz.mode, extra.gz.snap_ground, extra.gz.align_normal))
            }
            StudioCommand::SelectIds { ids, additive } => {
                let mut set: Vec<Entity> = if additive { sel.selected.iter().copied().collect() } else { Vec::new() };
                set.extend(objs.iter().filter(|(_, _, so, _)| ids.contains(&so.id)).map(|(e, ..)| e));
                let _ = *set.first().ok_or("no object with these ids")?;
                sel.clear();
                for e in set {
                    sel.add(e);
                }
                                Ok(format!("selected {} objects", sel.selected.len()))
            }
            StudioCommand::SetTransform { id, pos, rot_deg, scale, drop_to_ground } => {
                let e = match id {
                    Some(id) => objs.iter().find(|(_, _, so, _)| so.id == id).map(|(e, ..)| e).ok_or(format!("no object id {id}"))?,
                    None => sel.primary.ok_or("nothing selected")?,
                };
                let (_, tf, ..) = objs.get(e).map_err(|_| "object not editable".to_string())?;
                let mut next = *tf;
                if let Some(p) = pos {
                    next.translation = p;
                }
                if let Some(r) = rot_deg {
                    next.rotation = Quat::from_euler(EulerRot::YXZ, r.y.to_radians(), r.x.to_radians(), r.z.to_radians());
                }
                if let Some(s) = scale {
                    next.scale = s.max(Vec3::splat(0.05));
                }
                if drop_to_ground {
                    next.translation.y = grid.0.sample_height(next.translation.x, next.translation.z);
                }
                sh.record("MCP transform", vec![ObjState { entity: e, tf: *tf, alive: true }], vec![ObjState { entity: e, tf: next, alive: true }]);
                spawner.commands.entity(e).insert(next);
                Ok(format!("transform set at ({:.1}, {:.1}, {:.1})", next.translation.x, next.translation.y, next.translation.z))
            }
            StudioCommand::Duplicate { offset } => {
                let src: Vec<(Transform, String)> = sel.selected.iter().filter_map(|e| objs.get(*e).ok()).filter_map(|(_, tf, _, up)| up.map(|u| (*tf, u.kind.clone()))).collect();
                if src.is_empty() {
                    return Err("nothing duplicable selected (editor-spawned props only)".into());
                }
                sel.clear();
                for (tf, kind) in &src {
                    let (x, z) = (tf.translation.x + offset[0], tf.translation.z + offset[1]);
                    let yaw = tf.rotation.to_euler(EulerRot::YXZ).0;
                    if let Some((e, t)) = spawner.spawn(&grid.0, &spec, kind, x, z, yaw) {
                        let t = Transform { scale: tf.scale, rotation: tf.rotation, ..t };
                        spawner.commands.entity(e).insert(t);
                        sh.record(format!("Duplicate {kind}"), vec![ObjState { entity: e, tf: t, alive: false }], vec![ObjState { entity: e, tf: t, alive: true }]);
                        sel.add(e);
                    }
                }
                Ok(format!("duplicated {}", src.len()))
            }
            StudioCommand::SetSpec { field, value } => {
                use bevy::reflect::GetPath;
                let mut next = spec.clone();
                let slot = next.reflect_path_mut(field.as_str()).map_err(|e| format!("{field}: {e}"))?;
                if let Some(x) = slot.try_downcast_mut::<f32>() {
                    *x = value as f32;
                } else if let Some(x) = slot.try_downcast_mut::<f64>() {
                    *x = value;
                } else if let Some(x) = slot.try_downcast_mut::<bool>() {
                    *x = value != 0.0;
                } else if let Some(x) = slot.try_downcast_mut::<u32>() {
                    *x = value.max(0.0) as u32;
                } else if let Some(x) = slot.try_downcast_mut::<usize>() {
                    *x = value.max(0.0) as usize;
                } else if let Some(x) = slot.try_downcast_mut::<i32>() {
                    *x = value as i32;
                } else {
                    return Err(format!("{field}: unsupported type"));
                }
                next.sanitize();
                wh.record(format!("Spec {field}"), spec.clone(), next.clone());
                *spec = next;
                Ok(format!("spec {field} = {value}"))
            }
            StudioCommand::ListObjects => {
                let list: Vec<_> = objs
                    .iter()
                    .take(300)
                    .map(|(_, tf, so, up)| serde_json::json!({"id": so.id, "name": so.name, "kind": up.map(|u| u.kind.as_str()), "pos": tf.translation.to_array()}))
                    .collect();
                Ok(serde_json::Value::Array(list).to_string())
            }
            StudioCommand::Describe => queue_world_cmd(&mut spawner.commands, "Describe", |w| operators::describe_operators(&w.resource::<AppTypeRegistry>().read())),
            StudioCommand::DescribeScene { filter, limit } => {
                queue_world_cmd(&mut spawner.commands, "DescribeScene", move |w| operators::describe_scene(w, filter.as_deref(), limit.unwrap_or(100) as usize))
            }
            StudioCommand::SetComponent { target, component, value } => {
                queue_world_cmd(&mut spawner.commands, "SetComponent", move |w| operators::set_component(w, &target, &component, &value))
            }
            StudioCommand::AddComponent { target, component, value } => {
                queue_world_cmd(&mut spawner.commands, "AddComponent", move |w| operators::add_component(w, &target, &component, value.as_deref()))
            }
            StudioCommand::RemoveComponent { target, component } => {
                queue_world_cmd(&mut spawner.commands, "RemoveComponent", move |w| operators::remove_component(w, &target, &component))
            }
            StudioCommand::Reparent { child, parent } => {
                queue_world_cmd(&mut spawner.commands, "Reparent", move |w| operators::reparent(w, &child, parent.as_ref()))
            }
            StudioCommand::BuildRoom { min, max, floor, style } => queue_world_cmd(&mut spawner.commands, "BuildRoom", move |w| lb::cmd_build_room(w, min, max, floor, style)),
            StudioCommand::BuildWallLine { from, to, floor, style } => queue_world_cmd(&mut spawner.commands, "BuildWallLine", move |w| lb::cmd_build_wall_line(w, from, to, floor, style)),
            StudioCommand::PlaceLevelTile { kind, pos, facing, style } => queue_world_cmd(&mut spawner.commands, "PlaceLevelTile", move |w| lb::cmd_place_tile(w, kind, pos, facing, style)),
            StudioCommand::PaintLevelTiles { kind, cells, style, erase } => queue_world_cmd(&mut spawner.commands, "PaintLevelTiles", move |w| lb::cmd_paint_tiles(w, kind, cells, style, erase)),
            StudioCommand::ToggleDoor { pos, open } => queue_world_cmd(&mut spawner.commands, "ToggleDoor", move |w| lb::cmd_toggle_door(w, pos, open)),
            StudioCommand::SaveSelectionAsPrefab { path } => {
                let selected: Vec<Entity> = sel.selected.iter().copied().collect();
                queue_world_cmd(&mut spawner.commands, "SaveSelectionAsPrefab", move |w| crate::prefabs::cmd_save_selection(w, &path, selected))
            }
            StudioCommand::SpawnPrefab { path, position, rotation_y_deg } => {
                queue_world_cmd(&mut spawner.commands, "SpawnPrefab", move |w| crate::prefabs::cmd_spawn_prefab(w, &path, position, rotation_y_deg))
            }
            StudioCommand::SetGrid { enabled, cell_size, floor_level, floor_height, angle_snap_deg } => {
                queue_world_cmd(&mut spawner.commands, "SetGrid", move |w| lb::cmd_set_grid(w, enabled, cell_size, floor_level, floor_height, angle_snap_deg))
            }
            StudioCommand::SetWaterBody { x, z, level } => queue_world_cmd(&mut spawner.commands, "SetWaterBody", move |w| crate::water::cmd_set_water_body(w, x, z, level)),
            StudioCommand::SetGroundSnap { enabled, site } => queue_world_cmd(&mut spawner.commands, "SetGroundSnap", move |w| lb::cmd_set_ground_snap(w, enabled, site)),
            StudioCommand::SaveScene { path } if is_reflect_path(&path) => {
                spawner.commands.queue(move |w: &mut World| w.resource_mut::<SceneIoQueue>().0.push(SceneIo::Save(path)));
                Ok("save queued (reflect scene)".into())
            }
            StudioCommand::LoadScene { path } if is_reflect_path(&path) => {
                spawner.commands.queue(move |w: &mut World| w.resource_mut::<SceneIoQueue>().0.push(SceneIo::Load(path)));
                Ok("load queued (reflect scene)".into())
            }
            StudioCommand::SaveScene { path } => {
                let objects = users
                    .iter()
                    .map(|(_, tf, u)| SavedObject { kind: u.kind.clone(), translation: tf.translation.to_array(), rotation: tf.rotation.to_array(), scale: tf.scale.to_array() })
                    .collect();
                let file = SceneFile { version: 1, spec: spec.clone(), heights: grid.0.heights.clone(), objects };
                if let Some(dir) = std::path::Path::new(&path).parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                let json = serde_json::to_string(&file).map_err(|e| e.to_string())?;
                std::fs::write(&path, json).map_err(|e| format!("{path}: {e}"))?;
                Ok(format!("saved {path} ({} objects)", file.objects.len()))
            }
            StudioCommand::LoadScene { path } => {
                let json = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
                let file: SceneFile = serde_json::from_str(&json).map_err(|e| format!("{path}: {e}"))?;
                for (e, ..) in &users {
                    spawner.commands.entity(e).despawn();
                }
                sel.clear();
                *spec = file.spec;
                spec.sanitize();
                pending.0 = Some(file.heights);
                let n = file.objects.len();
                for o in file.objects {
                    let pos = Vec3::from(o.translation);
                    let yaw = Quat::from_array(o.rotation).to_euler(EulerRot::YXZ).0;
                    if let Some((e, _)) = spawner.spawn(&grid.0, &spec, &o.kind, pos.x, pos.z, yaw) {
                        spawner.commands.entity(e).insert(Transform { translation: pos, rotation: Quat::from_array(o.rotation), scale: Vec3::from(o.scale) });
                    }
                }
                Ok(format!("loaded {path} ({n} objects)"))
            }
        })();
        if matches!(&res, Ok(s) if s == DEFERRED) {
            continue;
        }
        let line = extra.results.push(&operators::op_name(&c), res);
        studio.status = line.clone();
        queue.log.push(line);
    }
    let excess = queue.log.len().saturating_sub(24);
    queue.log.drain(..excess);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_roundtrip_json() {
        let q = StudioCommandQueue {
            commands: vec![
                StudioCommand::ApplyPreset(WorldPreset::MistyDuskLake),
                StudioCommand::SpawnObject { kind: "pine".into(), x: 1.0, z: -2.0, yaw: 0.5 },
                StudioCommand::Sculpt { kind: BrushKind::Raise, x: 0.0, z: 0.0, radius: 10.0, strength: 5.0, seconds: 1.0 },
                StudioCommand::SetGizmo { mode: Some(GizmoMode::Rotate), snap_ground: Some(true), align_normal: None },
                StudioCommand::SetTransform { id: Some(3), pos: Some(Vec3::new(1.0, 2.0, 3.0)), rot_deg: None, scale: None, drop_to_ground: true },
                StudioCommand::SetSpec { field: "lake_radius".into(), value: 20.0 },
                StudioCommand::SelectIds { ids: vec![1, 2], additive: false },
                StudioCommand::Duplicate { offset: [2.0, 0.0] },
                StudioCommand::ListObjects,
                StudioCommand::SetPlayMode { mode: StudioMode::Paused },
                StudioCommand::Describe,
                StudioCommand::DescribeScene { filter: Some("Pine".into()), limit: None },
                StudioCommand::SetComponent { target: Target::Path("Camp/Tent#1".into()), component: "Name".into(), value: "\"x\"".into() },
                StudioCommand::AddComponent { target: Target::Id(4), component: "Visibility".into(), value: None },
                StudioCommand::RemoveComponent { target: Target::Name("Pine".into()), component: "Visibility".into() },
                StudioCommand::Reparent { child: Target::Id(1), parent: None },
                StudioCommand::SaveSelectionAsPrefab { path: "tavern_corner".into() },
                StudioCommand::SpawnPrefab { path: "tavern_corner".into(), position: Vec3::new(4.0, 0.0, -2.0), rotation_y_deg: 90.0 },
            ],
            log: vec![],
        };
        let json = serde_json::to_string(&q).unwrap();
        let back: StudioCommandQueue = serde_json::from_str(&json).unwrap();
        assert_eq!(back.commands, q.commands);
    }
}
