//! Play-in-Editor (E6): `Edit -> Playing/Paused -> Edit` with a full scene rollback on Stop.
//!
//! Entering a session (`StudioState.mode` leaves `Edit`) captures a `PlayModeSnapshot`: `WorldSpec`, terrain heights, the editor camera,
//! the four undo histories and, for every `SceneObject`, its `Transform`, `Visibility`, parent, soft-delete flag and all components
//! registered with `register_scene_component` (`LevelTile`, `Door`, `PrefabInstance`, `Actor`, `CharacterSheet`, ...). Returning to
//! `Edit` writes all of it back: objects spawned during the session (any `SceneObject`, or anything tagged `PlayEphemeral`) are
//! despawned, transient walking state is dropped and the histories are restored, so nothing done while playing leaks into the editor
//! or its undo stack. Limit: an object hard-despawned (not soft-`Deleted`) during play cannot be brought back; it is counted in the
//! Stop status.

use std::any::TypeId;
use std::collections::HashSet;

use bevy::prelude::*;

use crate::arc_core::{UndoHistory, WorldSpec};
use crate::camera::EditorCamera;
use crate::gizmo_3d::SceneHistory;
use crate::movement::WaypointsPath;
use crate::operators::{CompSnap, ComponentHistory, apply_snap, snap};
use crate::props::SceneObject;
use crate::selection::{Deleted, EditorSelection};
use crate::studio::{StudioMode, StudioState, TerrainHistory, UndoOrder};
use crate::terrain_render::TerrainGrid;
use crate::world_io::SceneComponentRegistry;

/// Tag for gameplay effects spawned during a session that must vanish on Stop.
#[derive(Component)]
pub struct PlayEphemeral;

#[derive(Clone)]
pub struct EntitySnap {
    pub entity: Entity,
    pub parent: Option<Entity>,
    pub deleted: bool,
    pub comps: Vec<CompSnap>,
}

#[derive(Resource, Default)]
pub struct PlayModeSnapshot {
    pub active: bool,
    /// Mode the transition logic last acted on.
    seen: StudioMode,
    pub spec: Option<WorldSpec>,
    pub camera: Vec<CompSnap>,
    pub ents: Vec<EntitySnap>,
    pub heights: Vec<f32>,
    world_hist: Option<UndoHistory<WorldSpec>>,
    scene_hist: Option<SceneHistory>,
    terrain_hist: Option<TerrainHistory>,
    comp_hist: Option<ComponentHistory>,
    order: Option<UndoOrder>,
}

fn tracked_types(w: &World) -> Vec<TypeId> {
    let mut ids = vec![TypeId::of::<Transform>(), TypeId::of::<Visibility>()];
    if let Some(r) = w.get_resource::<SceneComponentRegistry>() {
        ids.extend(r.ids().iter().copied());
    }
    ids
}

/// Serialise the scene into `PlayModeSnapshot` (Edit -> Playing).
pub fn capture(w: &mut World) {
    let ids = tracked_types(w);
    let ents: Vec<Entity> = w.query_filtered::<Entity, With<SceneObject>>().iter(w).collect();
    let ents = ents
        .into_iter()
        .map(|e| EntitySnap {
            entity: e,
            parent: w.get::<ChildOf>(e).map(|c| c.parent()),
            deleted: w.get::<Deleted>(e).is_some(),
            comps: ids.iter().map(|t| snap(w, e, *t)).filter(|s| s.value.is_some()).collect(),
        })
        .collect();
    let cam: Vec<Entity> = w.query_filtered::<Entity, With<EditorCamera>>().iter(w).collect();
    let cam_ids = [TypeId::of::<Transform>(), TypeId::of::<EditorCamera>()];
    let camera = cam.iter().flat_map(|e| cam_ids.iter().map(|t| snap(w, *e, *t)).collect::<Vec<_>>()).collect();
    let spec = w.get_resource::<WorldSpec>().cloned();
    let heights = w.get_resource::<TerrainGrid>().map(|g| g.0.heights.clone()).unwrap_or_default();
    let snapshot = PlayModeSnapshot {
        active: true,
        seen: w.resource::<StudioState>().mode,
        spec,
        camera,
        ents,
        heights,
        world_hist: w.get_resource::<UndoHistory<WorldSpec>>().cloned(),
        scene_hist: w.get_resource::<SceneHistory>().cloned(),
        terrain_hist: w.get_resource::<TerrainHistory>().cloned(),
        comp_hist: w.get_resource::<ComponentHistory>().cloned(),
        order: w.get_resource::<UndoOrder>().cloned(),
    };
    if let Some(mut sel) = w.get_resource_mut::<EditorSelection>() {
        sel.clear();
    }
    w.insert_resource(snapshot);
}

/// Roll the scene back to the snapshot (Playing/Paused -> Edit); returns a status line. No-op without an active snapshot.
pub fn restore(w: &mut World) -> String {
    let mut s = match w.get_resource_mut::<PlayModeSnapshot>() {
        Some(mut s) if s.active => std::mem::take(&mut *s),
        _ => return "Stop: no snapshot".into(),
    };
    let ids = tracked_types(w);
    let known: HashSet<Entity> = s.ents.iter().map(|e| e.entity).collect();

    // spawned during play
    let strays: Vec<Entity> = {
        let mut q = w.query_filtered::<Entity, Or<(With<SceneObject>, With<PlayEphemeral>)>>();
        q.iter(w).filter(|e| !known.contains(e)).collect()
    };
    for e in &strays {
        if let Ok(em) = w.get_entity_mut(*e) {
            em.despawn();
        }
    }
    // alive at Play but gone for good
    let lost = s.ents.iter().filter(|e| w.get_entity(e.entity).is_err()).count();

    for es in &s.ents {
        if w.get_entity(es.entity).is_err() {
            continue;
        }
        for t in &ids {
            let value = es.comps.iter().find(|c| c.type_id == *t).cloned();
            apply_snap(w, &value.unwrap_or(CompSnap { entity: es.entity, type_id: *t, value: None, also: Vec::new() }));
        }
        let cur = w.get::<ChildOf>(es.entity).map(|c| c.parent());
        let mut em = w.entity_mut(es.entity);
        if es.deleted {
            em.insert(Deleted);
        } else {
            em.remove::<Deleted>();
        }
        em.remove::<WaypointsPath>();
        match es.parent {
            Some(p) if cur != Some(p) => {
                em.insert(ChildOf(p));
            }
            None if cur.is_some() => {
                em.remove::<ChildOf>();
            }
            _ => {}
        }
    }
    for c in &s.camera {
        apply_snap(w, c);
    }
    if let (Some(spec), Some(mut cur)) = (s.spec.take(), w.get_resource_mut::<WorldSpec>()) {
        if *cur != spec {
            *cur = spec;
        }
    }
    if !s.heights.is_empty() {
        if let Some(mut g) = w.get_resource_mut::<TerrainGrid>() {
            if g.0.heights != s.heights {
                g.0.heights = std::mem::take(&mut s.heights);
            }
        }
    }
    if let Some(h) = s.world_hist.take() {
        w.insert_resource(h);
    }
    if let Some(h) = s.scene_hist.take() {
        w.insert_resource(h);
    }
    if let Some(h) = s.terrain_hist.take() {
        w.insert_resource(h);
    }
    if let Some(h) = s.comp_hist.take() {
        w.insert_resource(h);
    }
    if let Some(h) = s.order.take() {
        w.insert_resource(h);
    }
    if let Some(mut sel) = w.get_resource_mut::<EditorSelection>() {
        sel.clear();
    }
    let extra = if lost > 0 { format!(", {lost} hard-deleted not recoverable") } else { String::new() };
    format!("Stop: scene restored ({} spawned removed{extra})", strays.len())
}

/// Run the Edit <-> session transition if `StudioState.mode` changed since the last call; returns the status line.
pub fn apply_transition(world: &mut World) -> Option<String> {
    let now = world.resource::<StudioState>().mode;
    let was = world.resource::<PlayModeSnapshot>().seen;
    if now == was {
        return None;
    }
    let line = match (was, now) {
        (StudioMode::Edit, m) if m.in_session() => {
            capture(world);
            format!("Play: scene snapshot taken ({m:?})")
        }
        (w, StudioMode::Edit) if w.in_session() => restore(world),
        (_, m) => format!("mode {m:?}"),
    };
    world.resource_mut::<PlayModeSnapshot>().seen = now;
    world.resource_mut::<StudioState>().status = line.clone();
    Some(line)
}

/// `SetPlayMode` operator: switch mode and apply the snapshot / rollback immediately.
pub fn cmd_set_play_mode(w: &mut World, mode: StudioMode) -> Result<String, String> {
    w.resource_mut::<StudioState>().mode = mode;
    Ok(apply_transition(w).unwrap_or_else(|| format!("mode {mode:?} (unchanged)")))
}

fn transitions(world: &mut World) {
    apply_transition(world);
}

pub struct PlayModePlugin;

impl Plugin for PlayModePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayModeSnapshot>().add_systems(PreUpdate, transitions);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gizmo_3d::ObjState;
    use crate::level_builder::{Door, LevelTile, TileKind, WallStyle};
    use crate::props::ObjectCategory;
    use crate::world_io::SceneComponentAppExt;

    fn so(id: u32) -> SceneObject {
        SceneObject { id, name: format!("o{id}"), category: ObjectCategory::Structure, aabb_half_extents: Vec3::ONE }
    }

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.register_type::<Transform>().register_type::<Visibility>();
        app.register_scene_component::<Door>().register_scene_component::<LevelTile>();
        app.init_resource::<StudioState>()
            .init_resource::<EditorSelection>()
            .init_resource::<WorldSpec>()
            .init_resource::<UndoHistory<WorldSpec>>()
            .init_resource::<SceneHistory>()
            .init_resource::<TerrainHistory>()
            .init_resource::<ComponentHistory>()
            .init_resource::<UndoOrder>()
            .add_plugins(PlayModePlugin);
        app
    }

    fn set_mode(app: &mut App, m: StudioMode) {
        app.world_mut().resource_mut::<StudioState>().mode = m;
        app.update();
    }

    #[test]
    fn play_modify_stop_restores_scene_and_keeps_undo() {
        let mut app = app();
        let w = app.world_mut();
        let tile = LevelTile { kind: TileKind::Doorway, grid_pos: IVec3::new(2, 0, 1), facing: 0, style: WallStyle::WoodTavern };
        let door = w.spawn((so(1), Transform::from_xyz(4.0, 0.0, 2.0), tile, Door::default())).id();
        let prop = w.spawn((so(2), Transform::from_xyz(1.0, 2.0, 3.0))).id();
        let doomed = w.spawn((so(3), Transform::default())).id();
        let step = |e, x: f32| vec![ObjState { entity: e, tf: Transform::from_xyz(x, 0.0, 0.0), alive: true }];
        w.resource_mut::<SceneHistory>().record("edit", step(prop, 0.0), step(prop, 1.0));
        w.resource_mut::<EditorSelection>().set_one(prop);
        app.update();

        set_mode(&mut app, StudioMode::Playing);
        assert!(app.world().resource::<PlayModeSnapshot>().active);
        assert!(app.world().resource::<EditorSelection>().selected.is_empty(), "selection cleared on Play");

        // play: open the door, move a prop, delete one, spawn a combat effect and a stray object, edit history, change spec
        let w = app.world_mut();
        w.get_mut::<Door>(door).unwrap().is_open = true;
        w.get_mut::<Transform>(prop).unwrap().translation = Vec3::new(9.0, 9.0, 9.0);
        w.entity_mut(doomed).insert(Deleted);
        let fx = w.spawn(PlayEphemeral).id();
        let stray = w.spawn((so(99), Transform::default())).id();
        w.resource_mut::<SceneHistory>().record("play edit", step(prop, 1.0), step(prop, 2.0));
        w.resource_mut::<WorldSpec>().time_of_day = 0.123;
        app.update();
        set_mode(&mut app, StudioMode::Paused);
        assert!(app.world().resource::<PlayModeSnapshot>().active, "Playing -> Paused keeps the snapshot");
        assert!(app.world().get::<Door>(door).unwrap().is_open, "Pause does not roll back");

        set_mode(&mut app, StudioMode::Edit);
        let w = app.world();
        assert!(!w.get::<Door>(door).unwrap().is_open);
        assert_eq!(w.get::<LevelTile>(door), Some(&tile));
        assert_eq!(w.get::<Transform>(prop).unwrap().translation, Vec3::new(1.0, 2.0, 3.0));
        assert!(w.get::<Deleted>(doomed).is_none());
        assert!(w.get_entity(fx).is_err() && w.get_entity(stray).is_err());
        assert_eq!(w.resource::<SceneHistory>().undo_len(), 1, "UndoHistory is exactly as before Play");
        assert_ne!(w.resource::<WorldSpec>().time_of_day, 0.123);
        assert!(!w.resource::<PlayModeSnapshot>().active);
        assert!(w.resource::<StudioState>().status.starts_with("Stop: scene restored"));
    }

    #[test]
    fn set_play_mode_command_snapshots_and_rolls_back_immediately() {
        let mut app = app();
        let w = app.world_mut();
        let door = w.spawn((so(1), Transform::default(), Door::default())).id();
        assert!(cmd_set_play_mode(w, StudioMode::Playing).unwrap().starts_with("Play:"));
        w.get_mut::<Door>(door).unwrap().is_open = true;
        assert!(cmd_set_play_mode(w, StudioMode::Paused).unwrap().starts_with("mode Paused"));
        assert!(cmd_set_play_mode(w, StudioMode::Edit).unwrap().starts_with("Stop: scene restored"));
        assert!(!w.get::<Door>(door).unwrap().is_open);
        assert!(cmd_set_play_mode(w, StudioMode::Edit).unwrap().contains("unchanged"));
    }

    #[test]
    fn stop_without_play_is_a_noop() {
        let mut app = app();
        set_mode(&mut app, StudioMode::Edit);
        assert_eq!(restore(app.world_mut()), "Stop: no snapshot");
    }
}
