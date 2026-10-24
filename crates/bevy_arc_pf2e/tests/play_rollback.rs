//! Play → Stop rolls PF2e actor state (HP, position, rotation) back through the editor's snapshot of registered scene components.

use bevy::prelude::*;
use bevy_arc_studio::arc_core::{UndoHistory, WorldSpec};
use bevy_arc_studio::gizmo_3d::SceneHistory;
use bevy_arc_studio::operators::ComponentHistory;
use bevy_arc_studio::play_mode::PlayModePlugin;
use bevy_arc_studio::props::{ObjectCategory, SceneObject};
use bevy_arc_studio::selection::EditorSelection;
use bevy_arc_studio::studio::{StudioMode, StudioState, TerrainHistory, UndoOrder};
use bevy_arc_studio::world_io::SceneComponentAppExt;
use bevy_arc_pf2e::actors::{CharacterSheet, Faction};

fn so(id: u32) -> SceneObject {
    SceneObject { id, name: format!("o{id}"), category: ObjectCategory::Actor, aabb_half_extents: Vec3::ONE }
}

fn set_mode(app: &mut App, m: StudioMode) {
    app.world_mut().resource_mut::<StudioState>().mode = m;
    app.update();
}

#[test]
fn stop_revives_fallen_actors_and_restores_hp_and_positions() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.register_type::<Transform>().register_type::<Visibility>().register_type::<CharacterSheet>().register_type::<Faction>();
    app.register_scene_component::<CharacterSheet>().register_scene_component::<Faction>();
    app.init_resource::<StudioState>()
        .init_resource::<EditorSelection>()
        .init_resource::<WorldSpec>()
        .init_resource::<UndoHistory<WorldSpec>>()
        .init_resource::<SceneHistory>()
        .init_resource::<TerrainHistory>()
        .init_resource::<ComponentHistory>()
        .init_resource::<UndoOrder>()
        .add_plugins(PlayModePlugin);
    let w = app.world_mut();
    let sheet = CharacterSheet { hp: 32, hp_max: 32, ac: 19, ..default() };
    let hero = w.spawn((so(1), Transform::from_xyz(1.0, 0.0, 2.0), sheet.clone(), Faction::Player)).id();
    let goblin = w.spawn((so(2), Transform::from_xyz(5.0, 0.0, 5.0), CharacterSheet { hp: 16, hp_max: 16, ..sheet }, Faction::Hostile)).id();
    app.update();
    set_mode(&mut app, StudioMode::Playing);

    // combat: hero hurt and moved, goblin killed and laid on the floor
    let w = app.world_mut();
    w.get_mut::<CharacterSheet>(hero).unwrap().take_damage(11, false);
    w.get_mut::<Transform>(hero).unwrap().translation = Vec3::new(4.0, 0.0, 4.0);
    {
        let mut g = w.get_mut::<CharacterSheet>(goblin).unwrap();
        g.take_damage(40, true);
        assert!(g.hp <= 0 && g.dying > 0);
    }
    w.get_mut::<Transform>(goblin).unwrap().rotation = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    app.update();

    set_mode(&mut app, StudioMode::Edit);
    let w = app.world();
    let h = w.get::<CharacterSheet>(hero).unwrap();
    assert_eq!((h.hp, h.dying, h.wounded), (32, 0, 0));
    assert_eq!(w.get::<Transform>(hero).unwrap().translation, Vec3::new(1.0, 0.0, 2.0));
    let g = w.get::<CharacterSheet>(goblin).unwrap();
    assert_eq!((g.hp, g.dying), (16, 0), "killed actor is alive again");
    assert!(!g.is_dead());
    assert_eq!(w.get::<Transform>(goblin).unwrap().rotation, Quat::IDENTITY);
}
