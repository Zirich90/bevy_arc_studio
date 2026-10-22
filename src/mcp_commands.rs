//! Two-way command bus: the UI and external AI agents (BRP / MCP) push `StudioCommand`s into `StudioCommandQueue`.
//!
//! Agent usage (bevy_remote): `world.mutate_resources` / `world.insert_resources` on
//! `bevy_arc_studio::mcp_commands::StudioCommandQueue` with e.g.
//! `{"commands":[{"ApplyPreset":"MistyDuskLake"},{"SpawnObject":{"kind":"pine","x":10.0,"z":-4.0,"yaw":0.5}}],"log":[]}`;
//! results are appended to `log` the next frame.

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use serde::{Deserialize, Serialize};

use crate::arc_core::*;
use crate::camera::EditorCamera;
use crate::gizmo_3d::{ObjState, SceneHistory};
use crate::props::{SceneObject, UserProp, assets_dir};
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
    SetTab(InspectorTab),
    SetBrush { kind: BrushKind, radius: f32, strength: f32 },
    /// kind: pine | boulder | campfire | pier | lantern | model:<file.glb>
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
        app.init_resource::<StudioCommandQueue>()
            .register_type::<StudioCommandQueue>()
            .register_type::<StudioCommand>()
            .register_type::<QualityPreset>()
            .register_type::<WorldPreset>()
            .add_systems(Update, execute_commands.before(process_history));
    }
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
    mut req: ResMut<HistoryRequest>,
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
            StudioCommand::SetMode(m) => {
                studio.mode = m;
                Ok(format!("mode {m:?}"))
            }
            StudioCommand::SetTab(t) => {
                studio.tab = t;
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
                    if !assets_dir().join("models").join(file).exists() {
                        return Err(format!("model not found: assets/models/{file}"));
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
                req.undo = true;
                Ok("undo".into())
            }
            StudioCommand::Redo => {
                req.redo = true;
                Ok("redo".into())
            }
            StudioCommand::Screenshot { path } => {
                if let Some(dir) = std::path::Path::new(&path).parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                spawner.commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path.clone()));
                Ok(format!("screenshot -> {path}"))
            }
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
        let line = match res {
            Ok(s) => s,
            Err(e) => format!("ERR {e}"),
        };
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
            ],
            log: vec![],
        };
        let json = serde_json::to_string(&q).unwrap();
        let back: StudioCommandQueue = serde_json::from_str(&json).unwrap();
        assert_eq!(back.commands, q.commands);
    }
}
