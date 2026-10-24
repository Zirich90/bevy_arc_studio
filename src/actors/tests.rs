use bevy::ecs::reflect::AppTypeRegistry;
use bevy::reflect::FromReflect;
use bevy::ecs::system::RunSystemOnce;

use super::triggers::*;
use super::*;
use crate::arc_core::{UnitScale, WorldSpec};
use crate::props::UserProp;
use crate::selection::Deleted;
use crate::studio::{StudioMode, StudioState};
use crate::world_io::{SceneComponentRegistry, build_dynamic_world, dynamic_world_from_json, dynamic_world_to_json};

fn base_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins).add_plugins(AssetPlugin::default()).init_asset::<Mesh>().init_asset::<StandardMaterial>();
    app.init_resource::<StudioState>();
    app
}

fn so(id: u32, name: &str) -> SceneObject {
    SceneObject { id, name: name.into(), category: ObjectCategory::Actor, aabb_half_extents: Vec3::ONE }
}

fn spawn_from_template(app: &mut App, template: &'static str, id: u32, pos: Vec3) -> Entity {
    app.world_mut()
        .run_system_once(move |mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>| {
            let t = ActorTemplate::load(template).expect("template");
            spawn_actor(&mut commands, &mut meshes, &mut mats, so(id, &t.name), template, &t, Transform::from_translation(pos))
        })
        .unwrap()
}

#[test]
fn templates_match_spec() {
    let f = ActorTemplate::load("hero_fighter").unwrap();
    assert_eq!((f.level, f.hp, f.ac, f.speed_feet, f.faction), (2, 32, 19, 25.0, Faction::Player));
    assert_eq!((f.weapon.name.as_str(), f.weapon.dice, f.weapon.sides, f.weapon.damage_type, f.weapon.agile), ("Longsword", 1, 8, DamageType::Slashing, false));
    let g = ActorTemplate::load("goblin_warrior").unwrap();
    assert_eq!((g.level, g.hp, g.ac, g.speed_feet, g.faction), (1, 16, 16, 25.0, Faction::Hostile));
    assert_eq!((g.weapon.name.as_str(), g.weapon.dice, g.weapon.sides, g.weapon.damage_type, g.weapon.agile), ("Dagger", 1, 6, DamageType::Piercing, true));
    assert!(ActorTemplate::load("../Cargo").is_err());
    assert!(available_templates().contains(&"hero_fighter".to_string()));
}

#[test]
fn four_pf2e_archetypes_match_spec() {
    // (template, hp, ac, speed, attack, dice, sides, reactive strike, spell dc, faction)
    let want = [
        ("hero_fighter", 32, 19, 10, 1, 8, true, 0, Faction::Player),
        ("hero_wizard", 20, 16, 6, 1, 4, false, 18, Faction::Player),
        ("goblin_warrior", 16, 16, 7, 1, 6, false, 0, Faction::Hostile),
        ("dungeon_guard", 28, 18, 9, 1, 8, true, 0, Faction::Hostile),
    ];
    for (name, hp, ac, atk, n, sides, rs, dc, fac) in want {
        let t = ActorTemplate::load(name).unwrap();
        let s = CharacterSheet::from_template(&t);
        assert_eq!((s.hp, s.ac, s.speed_feet, s.attack_bonus, s.weapon.dice, s.weapon.sides), (hp, ac, 25.0, atk, n, sides), "{name}");
        assert_eq!((s.reactive_strike, s.spell_dc, t.faction), (rs, dc, fac), "{name}");
        assert_eq!(s.reach(), 5.0);
        assert!(available_templates().contains(&name.to_string()));
    }
    // strength modifiers behind the 1d8+4 / 1d6+2 damage lines
    assert_eq!(ActorTemplate::load("hero_fighter").unwrap().abilities.str_, 18);
    assert_eq!(ActorTemplate::load("goblin_warrior").unwrap().abilities.str_, 14);
}

#[test]
fn composite_bodies_have_weapons_and_faction_rings() {
    let mut app = base_app();
    for (i, t) in ["hero_fighter", "hero_wizard", "goblin_warrior", "dungeon_guard"].into_iter().enumerate() {
        let e = spawn_from_template(&mut app, t, i as u32 + 1, Vec3::ZERO);
        let kids = app.world().get::<Children>(e).map(|c| c.len()).unwrap_or(0);
        assert!(kids >= 7, "{t} has {kids} parts");
    }
}

#[test]
fn spawn_fighter_from_ron_template() {
    let mut app = base_app();
    let e = spawn_from_template(&mut app, "hero_fighter", 7, Vec3::new(1.0, 2.0, 3.0));
    let w = app.world();
    let er = w.entity(e);
    let actor = er.get::<Actor>().unwrap();
    assert_eq!((actor.id, actor.template.as_str(), actor.name.as_str()), (7, "hero_fighter", "Hero Fighter"));
    let sheet = er.get::<CharacterSheet>().unwrap();
    assert_eq!((sheet.hp, sheet.hp_max, sheet.ac, sheet.speed_feet), (32, 32, 19, 25.0));
    assert_eq!(*er.get::<Faction>().unwrap(), Faction::Player);
    let mover = er.get::<Mover>().expect("Mover");
    assert_eq!(mover.speed_feet, 25.0);
    assert_eq!(er.get::<Transform>().unwrap().translation, Vec3::new(1.0, 2.0, 3.0));
    assert_eq!(er.get::<SceneObject>().unwrap().category, ObjectCategory::Actor);
}

#[test]
fn damage_and_strikes_hurt_the_sheet() {
    let mut app = base_app();
    let f = spawn_from_template(&mut app, "hero_fighter", 1, Vec3::ZERO);
    let mut sheet = app.world().get::<CharacterSheet>(f).unwrap().clone();
    sheet.take_damage(8, false);
    assert_eq!((sheet.hp, sheet.dying), (24, 0));
    sheet.take_damage(30, false);
    assert_eq!((sheet.hp, sheet.dying), (0, 1));
    sheet.heal(5);
    assert_eq!((sheet.hp, sheet.dying, sheet.wounded), (5, 0, 1));

    // a goblin dagger strike (seeded) wounds the fighter
    let goblin = ActorTemplate::load("goblin_warrior").unwrap();
    let attacker = CharacterSheet::from_template(&goblin);
    let mut victim = CharacterSheet::from_template(&ActorTemplate::load("hero_fighter").unwrap());
    let mut dice = Dice::new(3);
    let mut dealt = 0;
    for _ in 0..30 {
        if let Some((_, d)) = attacker.strike(&mut victim, &mut dice) {
            dealt += d;
        }
    }
    assert!(dealt > 0 && victim.hp < victim.hp_max);
    assert_eq!(victim.hp, (32 - dealt).max(0));
}

fn trigger_app() -> App {
    let mut app = base_app();
    app.add_message::<TriggerEnterEvent>().add_message::<TriggerExitEvent>().add_systems(Update, track_triggers);
    app
}

fn drain<M: Message + Clone>(app: &mut App) -> Vec<M> {
    app.world_mut().resource_mut::<Messages<M>>().drain().collect()
}

#[test]
fn fighter_enters_and_leaves_goblin_trigger_zone() {
    let mut app = trigger_app();
    let goblin_pos = Vec3::new(10.0, 1.0, 10.0);
    let _goblin = spawn_from_template(&mut app, "goblin_warrior", 2, goblin_pos);
    let trigger = app
        .world_mut()
        .run_system_once(move |mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>| {
            spawn_trigger(&mut commands, &mut meshes, &mut mats, so(3, "TriggerVolume"), Transform::from_translation(goblin_pos), TriggerVolume::new(4.0, 3.0, "goblin_camp"))
        })
        .unwrap();
    let fighter = spawn_from_template(&mut app, "hero_fighter", 1, Vec3::new(0.0, 1.0, 0.0));

    app.world_mut().resource_mut::<StudioState>().mode = StudioMode::Playing;
    app.update();
    assert!(drain::<TriggerEnterEvent>(&mut app).is_empty());

    app.world_mut().get_mut::<Transform>(fighter).unwrap().translation = goblin_pos + Vec3::new(2.0, 0.0, 1.0);
    app.update();
    assert_eq!(drain::<TriggerEnterEvent>(&mut app), vec![TriggerEnterEvent { trigger, actor: fighter, event_id: "goblin_camp".into() }]);
    app.update();
    assert!(drain::<TriggerEnterEvent>(&mut app).is_empty(), "no repeat while inside");

    app.world_mut().get_mut::<Transform>(fighter).unwrap().translation = goblin_pos + Vec3::new(9.0, 0.0, 0.0);
    app.update();
    assert_eq!(drain::<TriggerExitEvent>(&mut app).len(), 1);
}

#[test]
fn hostile_and_editor_mode_do_not_trigger() {
    let mut app = trigger_app();
    let pos = Vec3::new(5.0, 0.0, 5.0);
    app.world_mut().spawn((Transform::from_translation(pos), TriggerVolume::new(3.0, 3.0, "z")));
    let goblin = spawn_from_template(&mut app, "goblin_warrior", 2, pos);
    let fighter = spawn_from_template(&mut app, "hero_fighter", 1, pos);
    app.update(); // editor mode
    assert!(drain::<TriggerEnterEvent>(&mut app).is_empty());
    app.world_mut().resource_mut::<StudioState>().mode = StudioMode::Playing;
    app.update();
    let evs = drain::<TriggerEnterEvent>(&mut app);
    assert_eq!(evs.len(), 1);
    assert_eq!(evs[0].actor, fighter);
    assert_ne!(evs[0].actor, goblin);
}

#[test]
fn deleted_actor_leaves_zone() {
    let mut app = trigger_app();
    let pos = Vec3::ZERO;
    app.world_mut().spawn((Transform::from_translation(pos), TriggerVolume::new(3.0, 3.0, "z")));
    let fighter = spawn_from_template(&mut app, "hero_fighter", 1, pos);
    app.world_mut().resource_mut::<StudioState>().mode = StudioMode::Playing;
    app.update();
    assert_eq!(drain::<TriggerEnterEvent>(&mut app).len(), 1);
    app.world_mut().entity_mut(fighter).insert(Deleted);
    app.update();
    assert_eq!(drain::<TriggerExitEvent>(&mut app).len(), 1);
}

#[test]
fn pf2e_components_roundtrip_through_scn_json() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins).add_plugins(AssetPlugin::default());
    app.init_resource::<SceneComponentRegistry>().register_type::<WorldSpec>().register_type::<UnitScale>().register_type::<UserProp>();
    app.init_resource::<WorldSpec>().init_resource::<UnitScale>();
    register_types(&mut app);

    let sheet = CharacterSheet::from_template(&ActorTemplate::load("hero_fighter").unwrap());
    let mut hurt = sheet.clone();
    hurt.take_damage(6, false);
    let e = app
        .world_mut()
        .spawn((
            Transform::from_xyz(1.0, 2.0, 3.0),
            UserProp { kind: "actor:hero_fighter".into() },
            Actor { name: "Hero Fighter".into(), id: 9, template: "hero_fighter".into() },
            hurt.clone(),
            Faction::Player,
        ))
        .id();
    let t = app.world_mut().spawn((Transform::default(), UserProp { kind: "trigger".into() }, TriggerVolume::new(2.5, 4.0, "gate"), Interactable { kind: InteractionKind::Trigger })).id();

    let dw = build_dynamic_world(app.world(), [e, t].into_iter());
    assert_eq!(dw.entities.len(), 2);
    let registry = app.world().resource::<AppTypeRegistry>().clone();
    let json = dynamic_world_to_json(&dw, &registry.read()).unwrap();
    assert!(json.contains("CharacterSheet") && json.contains("TriggerVolume"));

    let mut server = app.world().resource::<AssetServer>().clone();
    let back = dynamic_world_from_json(&json, &registry.read(), &mut server).unwrap();
    let all: Vec<_> = back.entities.iter().flat_map(|e| e.components.iter()).collect();
    assert!(all.iter().any(|c| CharacterSheet::from_reflect(c.as_ref()) == Some(hurt.clone())));
    assert!(all.iter().any(|c| Faction::from_reflect(c.as_ref()) == Some(Faction::Player)));
    assert!(all.iter().any(|c| TriggerVolume::from_reflect(c.as_ref()) == Some(TriggerVolume::new(2.5, 4.0, "gate"))));
}

#[test]
fn trigger_volume_geometry() {
    let v = TriggerVolume::new(3.0, 2.0, "x");
    let o = Vec3::new(1.0, 5.0, 1.0);
    assert!(v.contains(o, o + Vec3::new(2.9, 0.0, 0.0)));
    assert!(!v.contains(o, o + Vec3::new(3.1, 0.0, 0.0)));
    assert!(!v.contains(o, o + Vec3::new(0.0, 2.5, 0.0)));
    assert!(v.contains(o, o + Vec3::new(0.0, -0.3, 0.0)));
}
