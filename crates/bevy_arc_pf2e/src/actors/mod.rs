//! arc_actors (stage 09, feature `pf2e`): reflect components for NPCs/heroes, `.ron` actor templates,
//! faction markers and the inspector section. Trigger volumes live in [`triggers`].
//!
//! Actors are ordinary editor objects (`SceneObject` + `UserProp { kind: "actor:<template>" }`), so selection,
//! gizmo, soft-delete undo and `.scn.json` save/load work without special cases; the reflect components below
//! are registered with the scene component registry and restored on load.

pub mod triggers;

use bevy::ecs::system::SystemState;
use bevy::prelude::*;
use bevy::reflect::PartialReflect;
use bevy_arc_studio::arc_core::WorldSpec;
use bevy_arc_studio::editor_registry::{EditorAppExt, spawn_kind};
use bevy_arc_studio::gizmo_3d::{ObjState, SceneHistory};
use bevy_arc_studio::mcp_commands::CustomCommandAppExt;
use bevy_arc_studio::props::SpawnKindAppExt;
use bevy_arc_studio::selection::EditorSelection;
use bevy_arc_studio::studio::Spawner;
use bevy_arc_studio::terrain_render::TerrainGrid;
use bevy_egui::egui;
use serde::{Deserialize, Serialize};

use bevy_arc_studio::movement::Mover;
use bevy_arc_studio::props::{ObjectCategory, SceneObject, assets_dir};
use crate::rules::{Abilities, Combatant, CheckResult, Conditions, Dice, Hp, Weapon};
use crate::rules::actions::{MultipleAttackPenalty, TurnActionEconomy};
use bevy_arc_studio::ui::chip;
use bevy_arc_studio::world_io::SceneComponentAppExt;

// ───────────────────────────── Components ─────────────────────────────

#[derive(Component, Reflect, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[reflect(Component)]
pub enum Faction {
    Player,
    Hostile,
    #[default]
    Neutral,
}

impl Faction {
    pub const ALL: [Faction; 3] = [Faction::Player, Faction::Hostile, Faction::Neutral];

    /// `Player` is the Party, `Hostile` the Enemy side of an encounter.
    pub fn label(self) -> &'static str {
        match self {
            Faction::Player => "Party",
            Faction::Hostile => "Enemy",
            Faction::Neutral => "Neutral",
        }
    }

    fn disc_color(self) -> Color {
        match self {
            Faction::Player => Color::srgb(0.2, 0.95, 0.35),
            Faction::Hostile => Color::srgb(0.95, 0.2, 0.18),
            Faction::Neutral => Color::srgb(0.95, 0.8, 0.2),
        }
    }
}

#[derive(Component, Reflect, Clone, Debug, Default, PartialEq)]
#[reflect(Component)]
pub struct Actor {
    pub name: String,
    /// Mirrors `SceneObject::id` (kept in sync, so duplicates and reloads stay unique).
    pub id: u32,
    /// File stem in the actors folder (see `actors_dir`).
    pub template: String,
}

#[derive(Reflect, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DamageType {
    #[default]
    Slashing,
    Piercing,
    Bludgeoning,
}

#[derive(Reflect, Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct WeaponSheet {
    pub name: String,
    pub dice: u32,
    pub sides: u32,
    pub damage_type: DamageType,
    pub agile: bool,
}

/// Rules-facing stats of one actor; converts to/from `rules::Combatant` for strikes and damage.
#[derive(Component, Reflect, Clone, Debug, Default, PartialEq)]
/// `Clone` is reflected explicitly: the `[i32; N]` fields have no field-wise `reflect_clone`, which would silently drop the
/// sheet from the play-mode snapshot.
#[reflect(Component, Default, Clone)]
pub struct CharacterSheet {
    pub level: i32,
    /// Str, Dex, Con, Int, Wis, Cha.
    pub abilities: [i32; 6],
    pub hp_max: i32,
    pub hp: i32,
    pub dying: u8,
    pub wounded: u8,
    pub ac: i32,
    /// Fortitude, Reflex, Will totals.
    pub saves: [i32; 3],
    pub speed_feet: f32,
    pub attack_bonus: i32,
    pub weapon: WeaponSheet,
    /// Melee reach in feet (below 5 reads as 5).
    pub reach_ft: f32,
    pub reactive_strike: bool,
    pub has_shield: bool,
    /// Spell DC; 0 for non-casters.
    pub spell_dc: i32,
}

impl CharacterSheet {
    pub fn reach(&self) -> f32 {
        self.reach_ft.max(5.0)
    }

    pub fn from_template(t: &ActorTemplate) -> Self {
        let a = &t.abilities;
        Self {
            level: t.level,
            abilities: [a.str_, a.dex, a.con, a.int, a.wis, a.cha],
            hp_max: t.hp,
            hp: t.hp,
            dying: 0,
            wounded: 0,
            ac: t.ac,
            saves: [t.saves.fortitude, t.saves.reflex, t.saves.will],
            speed_feet: t.speed_feet,
            attack_bonus: t.attack_bonus,
            weapon: t.weapon.clone(),
            reach_ft: t.reach_ft,
            reactive_strike: t.reactive_strike,
            has_shield: t.has_shield,
            spell_dc: t.spell_dc,
        }
    }

    pub fn rules_hp(&self) -> Hp {
        Hp { max: self.hp_max, current: self.hp, dying: self.dying, wounded: self.wounded }
    }

    fn set_rules_hp(&mut self, h: Hp) {
        (self.hp_max, self.hp, self.dying, self.wounded) = (h.max, h.current, h.dying, h.wounded);
    }

    pub fn take_damage(&mut self, amount: i32, critical: bool) {
        let mut h = self.rules_hp();
        h.take_damage(amount, critical);
        self.set_rules_hp(h);
    }

    pub fn heal(&mut self, amount: i32) {
        let mut h = self.rules_hp();
        h.heal(amount);
        self.set_rules_hp(h);
    }

    pub fn is_dead(&self) -> bool {
        self.rules_hp().is_dead()
    }

    pub fn to_combatant(&self) -> Combatant {
        let w = &self.weapon;
        Combatant {
            name: "Actor",
            level: self.level,
            abilities: Abilities { scores: self.abilities },
            hp: self.rules_hp(),
            ac: self.ac,
            saves: self.saves,
            speed_ft: self.speed_feet,
            attack_bonus: self.attack_bonus,
            weapon: Weapon { name: "Weapon", dice: w.dice, sides: w.sides, agile: w.agile },
            conditions: Conditions::default(),
            economy: TurnActionEconomy::default(),
            map: MultipleAttackPenalty::default(),
        }
    }

    /// One Strike (rules::Combatant::strike); damage lands on `target`. `None` when the attacker has no action left.
    pub fn strike(&self, target: &mut CharacterSheet, dice: &mut Dice) -> Option<(CheckResult, i32)> {
        let (mut a, mut t) = (self.to_combatant(), target.to_combatant());
        let out = a.strike(&mut t, dice)?;
        target.set_rules_hp(t.hp);
        Some(out)
    }
}

#[derive(Reflect, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum InteractionKind {
    #[default]
    Loot,
    Talk,
    Trigger,
}

#[derive(Component, Reflect, Clone, Debug, Default, PartialEq)]
#[reflect(Component)]
pub struct Interactable {
    pub kind: InteractionKind,
}

// ───────────────────────────── Templates (.ron) ─────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AbilityScores {
    #[serde(rename = "str")]
    pub str_: i32,
    pub dex: i32,
    pub con: i32,
    pub int: i32,
    pub wis: i32,
    pub cha: i32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SaveBonuses {
    pub fortitude: i32,
    pub reflex: i32,
    pub will: i32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ActorTemplate {
    pub name: String,
    pub level: i32,
    pub faction: Faction,
    pub abilities: AbilityScores,
    pub hp: i32,
    pub ac: i32,
    pub saves: SaveBonuses,
    pub speed_feet: f32,
    pub attack_bonus: i32,
    pub weapon: WeaponSheet,
    /// Body colour (sRGB).
    pub color: (f32, f32, f32),
    /// Visual scale of the body (not the transform).
    pub scale: f32,
    /// Body kit: `fighter`, `wizard`, `goblin`, `guard` (anything else: plain capsule).
    #[serde(default)]
    pub kit: String,
    #[serde(default)]
    pub reach_ft: f32,
    #[serde(default)]
    pub reactive_strike: bool,
    #[serde(default)]
    pub has_shield: bool,
    #[serde(default)]
    pub spell_dc: i32,
}

impl ActorTemplate {
    pub fn parse(text: &str) -> Result<Self, String> {
        ron::from_str(text).map_err(|e| e.to_string())
    }

    /// Loads `<actors_dir>/<name>.ron`; `name` is a bare file stem.
    pub fn load(name: &str) -> Result<Self, String> {
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
            return Err(format!("invalid actor template name '{name}'"));
        }
        let path = actors_dir().join(format!("{name}.ron"));
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::parse(&text).map_err(|e| format!("{}: {e}", path.display()))
    }
}

/// Folder with the actor templates: a game's own `assets/actors` wins over the ones shipped with this crate.
pub fn actors_dir() -> std::path::PathBuf {
    let game = assets_dir().join("actors");
    if game.is_dir() { game } else { std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/actors") }
}

/// Stems of `*.ron` in [`actors_dir`].
pub fn available_templates() -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(actors_dir())
        .map(|rd| rd.flatten().filter_map(|e| e.file_name().into_string().ok()).filter_map(|n| n.strip_suffix(".ron").map(str::to_string)).collect())
        .unwrap_or_default();
    v.sort();
    v
}

// ───────────────────────────── Spawning ─────────────────────────────

/// Marks the coloured ring under an actor that shows its faction.
#[derive(Component)]
struct FactionDisc;

/// One mesh piece of an actor body, in the actor's local frame (before the template scale).
struct Part {
    name: &'static str,
    mesh: Mesh,
    color: Color,
    metal: bool,
    glow: bool,
    tf: Transform,
}

fn part(name: &'static str, mesh: impl Into<Mesh>, color: (f32, f32, f32), tf: Transform) -> Part {
    Part { name, mesh: mesh.into(), color: Color::srgb(color.0, color.1, color.2), metal: false, glow: false, tf }
}

fn metal(mut p: Part) -> Part {
    p.metal = true;
    p
}

const STEEL: (f32, f32, f32) = (0.72, 0.75, 0.8);
const WOOD: (f32, f32, f32) = (0.45, 0.3, 0.16);

/// Composite fighters: body, head and a kit with weapon / shield / staff. Forward is -Z.
fn kit_parts(kit: &str, body: (f32, f32, f32)) -> Vec<Part> {
    let skin = (0.9, 0.72, 0.58);
    let at = |x: f32, y: f32, z: f32| Transform::from_xyz(x, y, z);
    let mut v = vec![part("ActorBody", Capsule3d::new(0.3, 0.8), body, at(0.0, 0.8, 0.0))];
    match kit {
        "fighter" => {
            v.push(part("Head", Sphere::new(0.2), skin, at(0.0, 1.55, 0.0)));
            v.push(metal(part("Helmet", Cylinder::new(0.215, 0.2), STEEL, at(0.0, 1.62, 0.0))));
            v.push(metal(part("Sword", Cuboid::new(0.07, 0.06, 1.0), STEEL, at(0.42, 1.0, -0.5))));
            v.push(part("Crossguard", Cuboid::new(0.3, 0.06, 0.06), (0.8, 0.65, 0.2), at(0.42, 1.0, -0.02)));
            v.push(metal(part("Shield", Cuboid::new(0.1, 0.7, 0.55), (0.2, 0.3, 0.7), at(-0.45, 1.0, -0.1))));
            v.push(part("ShieldBoss", Sphere::new(0.1), (0.85, 0.7, 0.25), at(-0.52, 1.0, -0.1)));
        }
        "wizard" => {
            v.push(part("Robe", Cone { radius: 0.42, height: 1.1 }, body, at(0.0, 0.55, 0.0)));
            v.push(part("Head", Sphere::new(0.19), skin, at(0.0, 1.5, 0.0)));
            v.push(part("Hat", Cone { radius: 0.3, height: 0.6 }, (0.35, 0.2, 0.6), at(0.0, 1.9, 0.0)));
            v.push(part("HatBrim", Cylinder::new(0.34, 0.03), (0.3, 0.17, 0.5), at(0.0, 1.62, 0.0)));
            v.push(part("Staff", Cylinder::new(0.035, 1.9), WOOD, at(0.5, 0.95, -0.2)));
            let mut orb = part("StaffOrb", Sphere::new(0.11), (1.0, 0.55, 0.15), at(0.5, 1.95, -0.2));
            orb.glow = true;
            v.push(orb);
        }
        "goblin" => {
            v.push(part("Head", Sphere::new(0.26), (0.4, 0.78, 0.28), at(0.0, 1.45, 0.0)));
            v.push(part("EarL", Cuboid::new(0.35, 0.05, 0.14), (0.4, 0.78, 0.28), at(-0.38, 1.5, 0.0)));
            v.push(part("EarR", Cuboid::new(0.35, 0.05, 0.14), (0.4, 0.78, 0.28), at(0.38, 1.5, 0.0)));
            v.push(metal(part("Dagger", Cuboid::new(0.05, 0.05, 0.55), STEEL, at(0.38, 0.95, -0.35))));
            v.push(part("Hide", Cuboid::new(0.7, 0.3, 0.5), (0.4, 0.28, 0.15), at(0.0, 0.6, 0.0)));
        }
        "guard" => {
            v.push(part("Head", Sphere::new(0.2), skin, at(0.0, 1.55, 0.0)));
            v.push(metal(part("Helmet", Sphere::new(0.24), (0.5, 0.52, 0.56), at(0.0, 1.6, 0.0))));
            v.push(part("Crest", Cuboid::new(0.05, 0.18, 0.4), (0.8, 0.15, 0.12), at(0.0, 1.88, 0.0)));
            v.push(part("Pauldrons", Cuboid::new(0.85, 0.16, 0.4), (0.45, 0.47, 0.5), at(0.0, 1.3, 0.0)));
            v.push(part("SpearShaft", Cylinder::new(0.03, 2.2), WOOD, at(0.46, 1.1, -0.2)));
            v.push(metal(part("SpearTip", Cone { radius: 0.07, height: 0.3 }, STEEL, at(0.46, 2.35, -0.2))));
            v.push(metal(part("TowerShield", Cuboid::new(0.1, 0.95, 0.6), (0.55, 0.12, 0.1), at(-0.46, 1.0, -0.1))));
        }
        _ => {
            v.push(part("ActorNose", Cuboid::new(0.16, 0.16, 0.32), (0.98, 0.95, 0.85), at(0.0, 1.45, -0.3)));
        }
    }
    v
}

/// Spawn an actor root with body meshes. `tf` must already stand on the terrain.
pub fn spawn_actor(commands: &mut Commands, meshes: &mut Assets<Mesh>, mats: &mut Assets<StandardMaterial>, mut so: SceneObject, template: &str, t: &ActorTemplate, tf: Transform) -> Entity {
    let s = t.scale;
    so.category = ObjectCategory::Actor;
    so.aabb_half_extents = Vec3::new(0.4, 0.9, 0.4) * s;
    let disc = meshes.add(Annulus::new(0.5, 0.68));
    let mat_disc = mats.add(StandardMaterial { base_color: t.faction.disc_color(), unlit: true, double_sided: true, cull_mode: None, ..default() });
    let actor = Actor { name: t.name.clone(), id: so.id, template: template.to_string() };
    let handles: Vec<_> = kit_parts(&t.kit, t.color)
        .into_iter()
        .map(|p| {
            let mat = StandardMaterial {
                base_color: p.color,
                perceptual_roughness: if p.metal { 0.35 } else { 0.7 },
                metallic: if p.metal { 0.8 } else { 0.0 },
                emissive: if p.glow { LinearRgba::rgb(4.0, 1.8, 0.4) } else { LinearRgba::BLACK },
                ..default()
            };
            (p.name, meshes.add(p.mesh), mats.add(mat), p.tf)
        })
        .collect();
    commands
        .spawn((
            Name::new(t.name.clone()),
            so,
            actor,
            CharacterSheet::from_template(t),
            t.faction,
            Mover { speed_feet: t.speed_feet, ..default() },
            tf,
            Visibility::default(),
        ))
        .with_children(|c| {
            for (name, mesh, mat, mut ptf) in handles {
                ptf.translation *= s;
                ptf.scale = Vec3::splat(s);
                c.spawn((Name::new(name), Mesh3d(mesh), MeshMaterial3d(mat), ptf));
            }
            c.spawn((
                Name::new("FactionDisc"),
                FactionDisc,
                Mesh3d(disc),
                MeshMaterial3d(mat_disc),
                Transform::from_xyz(0.0, 0.05, 0.0).with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)).with_scale(Vec3::splat(s)),
            ));
        })
        .id()
}

// ───────────────────────────── Plugin & systems ─────────────────────────────

pub struct ActorsPlugin;

impl Plugin for ActorsPlugin {
    fn build(&self, app: &mut App) {
        register_types(app);
        app.register_spawn_kind("actor:", |sc, kind| {
            let name = &kind["actor:".len()..];
            let t = ActorTemplate::load(name).ok()?;
            let so = sc.scene_object(&t.name, ObjectCategory::Actor, Vec3::splat(1.0));
            Some(spawn_actor(sc.commands, sc.meshes, sc.mats, so, name, &t, sc.tf))
        })
        .register_spawn_kind("trigger", |sc, _| {
            let so = sc.scene_object("TriggerVolume", ObjectCategory::Trigger, Vec3::splat(1.0));
            Some(triggers::spawn_trigger(sc.commands, sc.meshes, sc.mats, so, sc.tf, Default::default()))
        })
        .register_inspector_override::<Faction>(faction_inspector)
        .register_inspector_override::<CharacterSheet>(sheet_inspector)
        .register_inspector_override::<triggers::TriggerVolume>(trigger_inspector)
        .register_custom_command("pf2e.spawn_actor", cmd_spawn_actor)
        .register_spawner_item("RPG (pf2e)", "Hero Fighter", spawn_kind("actor:hero_fighter"))
            .register_spawner_item("RPG (pf2e)", "Hero Wizard", spawn_kind("actor:hero_wizard"))
            .register_spawner_item("RPG (pf2e)", "Goblin Warrior", spawn_kind("actor:goblin_warrior"))
            .register_spawner_item("RPG (pf2e)", "Dungeon Guard", spawn_kind("actor:dungeon_guard"))
            .register_spawner_item("RPG (pf2e)", "Trigger Volume", spawn_kind("trigger"));
        app.add_plugins(triggers::TriggersPlugin).add_systems(Update, (sync_actor_ids, sync_faction_disc));
    }
}

/// Reflect registration + `.scn.json` persistence for every pf2e component.
pub fn register_types(app: &mut App) {
    app.register_type::<DamageType>()
        .register_type::<WeaponSheet>()
        .register_type::<InteractionKind>()
        .register_type::<triggers::TriggerShape>()
        .register_scene_component::<Actor>()
        .register_scene_component::<CharacterSheet>()
        .register_scene_component::<Faction>()
        .register_scene_component::<Interactable>()
        .register_scene_component::<triggers::TriggerVolume>();
}

fn sync_actor_ids(mut q: Query<(&SceneObject, &mut Actor), Changed<SceneObject>>) {
    for (so, mut a) in &mut q {
        a.set_if_neq(Actor { id: so.id, ..a.clone() });
    }
}

fn sync_faction_disc(changed: Query<(&Faction, &Children), Changed<Faction>>, discs: Query<&MeshMaterial3d<StandardMaterial>, With<FactionDisc>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    for (f, kids) in &changed {
        for k in kids.iter() {
            if let Some(mut m) = discs.get(k).ok().and_then(|h| mats.get_mut(&h.0)) {
                m.base_color = f.disc_color();
            }
        }
    }
}

// ───────────────────────────── Inspector ─────────────────────────────

/// `EditorRegistry` inspector override for [`Faction`]: one chip per variant.
fn faction_inspector(ui: &mut egui::Ui, v: &mut dyn PartialReflect) -> bool {
    let Some(f) = v.try_downcast_mut::<Faction>() else { return false };
    let mut changed = false;
    ui.horizontal(|ui| {
        for opt in Faction::ALL {
            if chip(ui, *f == opt, opt.label()).clicked() && *f != opt {
                *f = opt;
                changed = true;
            }
        }
    });
    changed
}

/// Read-only summary of a [`CharacterSheet`].
fn sheet_inspector(ui: &mut egui::Ui, v: &mut dyn PartialReflect) -> bool {
    let Some(s) = v.try_downcast_ref::<CharacterSheet>() else { return false };
    let hp_color = if s.hp <= 0 { egui::Color32::from_rgb(255, 90, 80) } else { egui::Color32::from_rgb(120, 230, 150) };
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(format!("HP {}/{}", s.hp, s.hp_max)).strong().color(hp_color));
        ui.label(format!("AC {}", s.ac));
        ui.label(format!("Speed {} ft", s.speed_feet));
        ui.label(format!("Lv {}", s.level));
    });
    if s.dying > 0 || s.wounded > 0 {
        ui.label(format!("dying {}  wounded {}", s.dying, s.wounded));
    }
    let [st, dx, co, it, wi, ch] = s.abilities;
    ui.label(format!("Str {st}  Dex {dx}  Con {co}  Int {it}  Wis {wi}  Cha {ch}"));
    ui.label(format!("Fort {:+}  Ref {:+}  Will {:+}", s.saves[0], s.saves[1], s.saves[2]));
    let w = &s.weapon;
    ui.label(format!("{} {:+}  {}d{} {:?}{}", w.name, s.attack_bonus, w.dice, w.sides, w.damage_type, if w.agile { "  agile" } else { "" }));
    ui.label(format!(
        "Reach {} ft{}{}{}",
        s.reach(),
        if s.reactive_strike { "  Reactive Strike" } else { "" },
        if s.has_shield { "  Shield" } else { "" },
        if s.spell_dc > 0 { format!("  Spell DC {}", s.spell_dc) } else { String::new() }
    ));
    false
}

fn trigger_inspector(ui: &mut egui::Ui, v: &mut dyn PartialReflect) -> bool {
    let Some(tv) = v.try_downcast_mut::<triggers::TriggerVolume>() else { return false };
    let triggers::TriggerShape::Cylinder { mut radius, mut height } = tv.shape;
    let mut id = tv.event_id.clone();
    ui.horizontal(|ui| {
        ui.label("Cylinder");
        ui.add(egui::DragValue::new(&mut radius).speed(0.05).prefix("r ").range(0.25..=60.0));
        ui.add(egui::DragValue::new(&mut height).speed(0.05).prefix("h ").range(0.25..=60.0));
    });
    ui.horizontal(|ui| {
        ui.label("event_id");
        ui.add(egui::TextEdit::singleline(&mut id).desired_width(150.0));
    });
    let next = triggers::TriggerVolume { shape: triggers::TriggerShape::Cylinder { radius, height }, event_id: id };
    if *tv == next {
        return false;
    }
    *tv = next;
    true
}

// ───────────────────────────── Agent command ─────────────────────────────

/// `StudioCommand::Custom { name: "pf2e.spawn_actor", args: {"template":"goblin_warrior","pos":[x,y,z],"faction":"Hostile"} }`:
/// spawns `actors/<template>.ron` on the terrain below `pos` (y ignored); `faction` overrides the template's. One undo step.
fn cmd_spawn_actor(w: &mut World, args: &str) -> Result<String, String> {
    #[derive(Deserialize)]
    struct Args {
        template: String,
        pos: [f32; 3],
        #[serde(default)]
        faction: Option<Faction>,
    }
    let a: Args = serde_json::from_str(args).map_err(|e| format!("pf2e.spawn_actor args: {e}"))?;
    ActorTemplate::load(&a.template)?;
    let mut state = SystemState::<(Spawner, Res<TerrainGrid>, Res<WorldSpec>, ResMut<SceneHistory>, ResMut<EditorSelection>)>::new(w);
    let msg = {
        let (mut spawner, grid, spec, mut sh, mut sel) = state.get_mut(w).map_err(|e| e.to_string())?;
        let half = grid.0.size * 0.5;
        let (x, z) = (a.pos[0].clamp(-half, half), a.pos[2].clamp(-half, half));
        let kind = format!("actor:{}", a.template);
        let (e, tf) = spawner.spawn(&grid.0, &spec, &kind, x, z, 0.0).ok_or_else(|| format!("cannot spawn '{kind}'"))?;
        if let Some(f) = a.faction {
            spawner.commands.entity(e).insert(f);
        }
        sh.record(format!("Spawn {}", a.template), vec![ObjState { entity: e, tf, alive: false }], vec![ObjState { entity: e, tf, alive: true }]);
        sel.set_one(e);
        format!("spawned actor {} at ({x:.1}, {z:.1})", a.template)
    };
    state.apply(w);
    Ok(msg)
}

#[cfg(test)]
mod tests;
