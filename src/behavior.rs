//! Phase 3.1: scene-authored, one-shot timer controlling its owner's visibility.
use std::collections::HashMap;

use bevy::prelude::*;
use crate::studio::{StudioMode, StudioState};
use crate::world_io::SceneComponentAppExt;
use crate::mcp_commands::CustomCommandAppExt;

/// Named scene event; repeated emissions reapply the configured action.
#[derive(Component, Reflect, Clone, Debug, Default)]
#[reflect(Component, Default, Clone)]
pub struct SignalVisibility {
    pub signal: String,
    pub visible: bool,
}

#[derive(Resource, Default)]
struct PendingSignals(Vec<String>);

pub(crate) fn reset_session(world: &mut World) {
    if let Some(mut runtime) = world.get_resource_mut::<BehaviorRuntime>() { runtime.0.clear(); }
    if let Some(mut pending) = world.get_resource_mut::<PendingSignals>() { pending.0.clear(); }
}

/// Runtime events are accepted only while Playing and never enter editor history.
pub fn emit_signal(world: &mut World, name: &str) -> Result<String, String> {
    if world.resource::<StudioState>().mode != StudioMode::Playing {
        return Err("behavior signals require Playing mode".into());
    }
    if name.trim().is_empty() || name.len() > 256 {
        return Err("signal name must contain 1..256 bytes and non-whitespace text".into());
    }
    let mut pending = world.resource_mut::<PendingSignals>();
    if pending.0.len() >= 1024 { return Err("behavior signal queue is full".into()); }
    pending.0.push(name.to_owned());
    Ok(format!("signal queued: {name}"))
}

fn dispatch_signals(
    state: Res<StudioState>, mut pending: ResMut<PendingSignals>,
    mut objects: Query<(&SignalVisibility, &mut Visibility), Without<crate::selection::Deleted>>,
) {
    if state.mode == StudioMode::Edit { pending.0.clear(); return; }
    if state.mode != StudioMode::Playing { return; }
    for name in pending.0.drain(..) {
        for (behavior, mut visibility) in &mut objects {
            if behavior.signal == name {
                *visibility = if behavior.visible { Visibility::Inherited } else { Visibility::Hidden };
            }
        }
    }
}

#[derive(Component, Reflect, Clone, Debug)]
#[reflect(Component, Default, Clone)]
pub struct TimedVisibility {
    pub delay_seconds: f32,
    pub visible: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn stop_restores_visibility_and_resets_runtime_without_edit_frame() {
        use crate::{arc_core::WorldSpec, props::{SceneObject, ObjectCategory}};
        let mut app = App::new();
        app.init_resource::<StudioState>().init_resource::<Time>().init_resource::<WorldSpec>()
            .add_plugins((BehaviorPlugin, crate::play_mode::PlayModePlugin))
            .register_type::<Visibility>().register_type::<Transform>();
        let e = app.world_mut().spawn((SceneObject { id: 1, name: "pilot".into(), category: ObjectCategory::Structure, aabb_half_extents: Vec3::ONE }, TimedVisibility { delay_seconds: 0.0, visible: false }, Visibility::Inherited, Transform::default())).id();
        crate::play_mode::cmd_set_play_mode(app.world_mut(), StudioMode::Playing).unwrap();
        app.world_mut().run_schedule(PostUpdate);
        assert_eq!(*app.world().get::<Visibility>(e).unwrap(), Visibility::Hidden);
        crate::play_mode::cmd_set_play_mode(app.world_mut(), StudioMode::Edit).unwrap();
        assert_eq!(*app.world().get::<Visibility>(e).unwrap(), Visibility::Inherited);
        crate::play_mode::cmd_set_play_mode(app.world_mut(), StudioMode::Playing).unwrap();
        app.world_mut().run_schedule(PostUpdate);
        assert_eq!(*app.world().get::<Visibility>(e).unwrap(), Visibility::Hidden);
    }

    #[test]
    fn authoring_undo_and_prefab_component_roundtrip() {
        use crate::operators::*;
        let mut app = App::new();
        app.init_resource::<Time>().init_resource::<ComponentHistory>().init_resource::<CompClock>().add_plugins(BehaviorPlugin);
        let w = app.world_mut();
        let e = w.spawn_empty().id();
        add_component(w, &Target::Entity(e), "TimedVisibility", None).unwrap();
        set_component(w, &Target::Entity(e), "TimedVisibility", r#"{"delay_seconds":3.0}"#).unwrap();
        let components = crate::prefab_variants::capture_components(w, e).unwrap();
        let clone = w.spawn_empty().id();
        crate::prefab_variants::apply_components(w, clone, &components).unwrap();
        assert_eq!(w.get::<TimedVisibility>(clone).unwrap().delay_seconds, 3.0);
        let undo = w.resource_mut::<ComponentHistory>().undo().unwrap();
        apply_snap(w, &undo);
        assert_eq!(w.get::<TimedVisibility>(e).unwrap().delay_seconds, 1.0);
        let undo = w.resource_mut::<ComponentHistory>().undo().unwrap();
        apply_snap(w, &undo);
        assert!(w.get::<TimedVisibility>(e).is_none());
    }

    #[test]
    fn behavior_scene_serialization_roundtrip() {
        use crate::world_io::*;
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default(), BehaviorPlugin));
        let e = app.world_mut().spawn((TimedVisibility::default(), SignalVisibility { signal: "demo.hide".into(), visible: false })).id();
        let scene = build_dynamic_world(app.world(), std::iter::once(e));
        let registry = app.world().resource::<AppTypeRegistry>().clone();
        let json = dynamic_world_to_json(&scene, &registry.read()).unwrap();
        let mut server = app.world().resource::<AssetServer>().clone();
        let back = dynamic_world_from_json(&json, &registry.read(), &mut server).unwrap();
        assert!(back.entities[0].components.iter().any(|c| TimedVisibility::from_reflect(c.as_ref()).is_some()));
        assert!(back.entities[0].components.iter().any(|c| SignalVisibility::from_reflect(c.as_ref()).is_some_and(|b| b.signal == "demo.hide")));
    }

    #[test]
    fn named_signal_is_scoped_validated_and_consumed() {
        let mut app = App::new();
        app.init_resource::<StudioState>().init_resource::<Time>().add_plugins(BehaviorPlugin);
        let a = app.world_mut().spawn((SignalVisibility { signal: "door.open".into(), visible: false }, Visibility::Inherited)).id();
        let b = app.world_mut().spawn((SignalVisibility { signal: "other".into(), visible: false }, Visibility::Inherited)).id();
        assert!(emit_signal(app.world_mut(), "door.open").is_err());
        app.world_mut().resource_mut::<StudioState>().mode = StudioMode::Playing;
        assert!(emit_signal(app.world_mut(), " ").is_err());
        emit_signal(app.world_mut(), "door.open").unwrap();
        app.world_mut().run_schedule(PostUpdate);
        assert_eq!(*app.world().get::<Visibility>(a).unwrap(), Visibility::Hidden);
        assert_eq!(*app.world().get::<Visibility>(b).unwrap(), Visibility::Inherited);
        assert!(app.world().resource::<PendingSignals>().0.is_empty());
    }

    #[test]
    fn timer_pauses_fires_once_and_restarts_in_new_session() {
        let mut app = App::new();
        app.init_resource::<StudioState>().init_resource::<Time>().add_plugins(BehaviorPlugin);
        let e = app.world_mut().spawn((TimedVisibility::default(), Visibility::Inherited)).id();
        app.world_mut().resource_mut::<StudioState>().mode = StudioMode::Playing;
        let step = |app: &mut App, seconds| {
            app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs_f32(seconds));
            app.world_mut().run_schedule(PostUpdate);
        };
        step(&mut app, 0.5);
        assert_eq!(*app.world().get::<Visibility>(e).unwrap(), Visibility::Inherited);
        app.world_mut().resource_mut::<StudioState>().mode = StudioMode::Paused;
        step(&mut app, 5.0);
        assert_eq!(*app.world().get::<Visibility>(e).unwrap(), Visibility::Inherited);
        app.world_mut().resource_mut::<StudioState>().mode = StudioMode::Playing;
        step(&mut app, 0.5);
        assert_eq!(*app.world().get::<Visibility>(e).unwrap(), Visibility::Hidden);
        *app.world_mut().get_mut::<Visibility>(e).unwrap() = Visibility::Inherited;
        step(&mut app, 2.0);
        assert_eq!(*app.world().get::<Visibility>(e).unwrap(), Visibility::Inherited);
        app.world_mut().resource_mut::<StudioState>().mode = StudioMode::Edit;
        step(&mut app, 0.0);
        app.world_mut().resource_mut::<StudioState>().mode = StudioMode::Playing;
        step(&mut app, 1.0);
        assert_eq!(*app.world().get::<Visibility>(e).unwrap(), Visibility::Hidden);
    }
}

impl Default for TimedVisibility {
    fn default() -> Self { Self { delay_seconds: 1.0, visible: false } }
}

#[derive(Resource, Default)]
struct BehaviorRuntime(HashMap<Entity, (f32, bool)>);

pub struct BehaviorPlugin;
impl Plugin for BehaviorPlugin {
    fn build(&self, app: &mut App) {
        app.register_scene_component::<TimedVisibility>()
            .register_scene_component::<SignalVisibility>()
            .init_resource::<PendingSignals>()
            .register_custom_command("behavior.emit", |world, args| {
                #[derive(serde::Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Args { signal: String }
                let args: Args = serde_json::from_str(args).map_err(|e| e.to_string())?;
                emit_signal(world, &args.signal)
            })
            .init_resource::<BehaviorRuntime>()
            .add_systems(PostUpdate, (tick, dispatch_signals).chain());
    }
}

fn tick(
    state: Res<StudioState>, time: Res<Time>, mut runtime: ResMut<BehaviorRuntime>,
    mut objects: Query<(Entity, &TimedVisibility, &mut Visibility), Without<crate::selection::Deleted>>,
) {
    if state.mode == StudioMode::Edit { runtime.0.clear(); return; }
    if state.mode != StudioMode::Playing { return; }
    runtime.0.retain(|entity, _| objects.contains(*entity));
    for (entity, behavior, mut visibility) in &mut objects {
        if !behavior.delay_seconds.is_finite() || behavior.delay_seconds < 0.0 { continue; }
        let (elapsed, fired) = runtime.0.entry(entity).or_default();
        if *fired { continue; }
        *elapsed += time.delta_secs();
        if *elapsed >= behavior.delay_seconds {
            *visibility = if behavior.visible { Visibility::Inherited } else { Visibility::Hidden };
            *fired = true;
        }
    }
}
