//! arc_actors (stage 09, feature `pf2e`): reflect components for NPCs/heroes, `.ron` actor templates,
//! faction markers and the inspector section. Trigger volumes live in [`triggers`].
//!
//! Actors are ordinary editor objects (`SceneObject` + `UserProp { kind: "actor:<template>" }`), so selection,
//! gizmo, soft-delete undo and `.scn.json` save/load work without special cases; the reflect components below
//! are registered with the scene component registry and restored on load.

pub mod triggers;

use bevy::prelude::*;
use bevy_egui::egui;
use serde::{Deserialize, Serialize};

use crate::movement::Mover;
use crate::props::{ObjectCategory, SceneObject, assets_dir};
use crate::rules::{Abilities, Combatant, CheckResult, Conditions, Dice, Hp, Weapon};
use crate::rules::actions::{MultipleAttackPenalty, TurnActionEconomy};
use crate::ui::{TEAL, UiData, chip};
use crate::world_io::SceneComponentAppExt;

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

    pub fn label(self) -> &'static str {
        match self {
            Faction::Player => "Player",
            Faction::Hostile => "Hostile",
            Faction::Neutral => "Neutral",
        }
    }

    fn disc_color(self) -> Color {
        match self {
            Faction::Player => Color::srgb(0.2, 0.75, 1.0),
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
    /// File stem in `assets/actors/`.
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
#[reflect(Component)]
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
}

impl CharacterSheet {
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
}

impl ActorTemplate {
    pub fn parse(text: &str) -> Result<Self, String> {
        ron::from_str(text).map_err(|e| e.to_string())
    }

    /// Loads `assets/actors/<name>.ron`; `name` is a bare file stem.
    pub fn load(name: &str) -> Result<Self, String> {
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
            return Err(format!("invalid actor template name '{name}'"));
        }
        let path = assets_dir().join("actors").join(format!("{name}.ron"));
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::parse(&text).map_err(|e| format!("{}: {e}", path.display()))
    }
}

/// Stems of `assets/actors/*.ron`.
pub fn available_templates() -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(assets_dir().join("actors"))
        .map(|rd| rd.flatten().filter_map(|e| e.file_name().into_string().ok()).filter_map(|n| n.strip_suffix(".ron").map(str::to_string)).collect())
        .unwrap_or_default();
    v.sort();
    v
}

// ───────────────────────────── Spawning ─────────────────────────────

/// Marks the coloured ring under an actor that shows its faction.
#[derive(Component)]
struct FactionDisc;

/// Spawn an actor root with body meshes. `tf` must already stand on the terrain.
pub fn spawn_actor(commands: &mut Commands, meshes: &mut Assets<Mesh>, mats: &mut Assets<StandardMaterial>, mut so: SceneObject, template: &str, t: &ActorTemplate, tf: Transform) -> Entity {
    let s = t.scale;
    so.category = ObjectCategory::Actor;
    so.aabb_half_extents = Vec3::new(0.4, 0.9, 0.4) * s;
    let body = meshes.add(Capsule3d::new(0.3, 1.0));
    let nose = meshes.add(Cuboid::new(0.16, 0.16, 0.32));
    let disc = meshes.add(Annulus::new(0.5, 0.65));
    let (r, g, b) = t.color;
    let mat_body = mats.add(StandardMaterial { base_color: Color::srgb(r, g, b), perceptual_roughness: 0.6, ..default() });
    let mat_nose = mats.add(StandardMaterial { base_color: Color::srgb(0.98, 0.95, 0.85), perceptual_roughness: 0.5, ..default() });
    let mat_disc = mats.add(StandardMaterial { base_color: t.faction.disc_color(), unlit: true, double_sided: true, cull_mode: None, ..default() });
    let actor = Actor { name: t.name.clone(), id: so.id, template: template.to_string() };
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
            c.spawn((Name::new("ActorBody"), Mesh3d(body), MeshMaterial3d(mat_body), Transform::from_xyz(0.0, 0.8 * s, 0.0).with_scale(Vec3::splat(s))));
            c.spawn((Name::new("ActorNose"), Mesh3d(nose), MeshMaterial3d(mat_nose), Transform::from_xyz(0.0, 1.25 * s, -0.3 * s).with_scale(Vec3::splat(s))));
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

/// Right-inspector section for the selected entity's pf2e components.
pub fn inspector(ui: &mut egui::Ui, d: &mut UiData, e: Entity) {
    let Ok((actor, sheet, fac, inter, trig)) = d.rpg.get_mut(e) else { return };
    if actor.is_none() && trig.is_none() {
        return;
    }
    ui.separator();
    if let Some(a) = actor {
        ui.label(egui::RichText::new("Actor").strong().color(TEAL));
        ui.label(format!("{}   id {}   template {}", a.name, a.id, a.template));
    }
    if let Some(mut f) = fac {
        ui.horizontal(|ui| {
            ui.label("Faction");
            for v in Faction::ALL {
                if chip(ui, *f == v, v.label()).clicked() {
                    f.set_if_neq(v);
                }
            }
        });
    }
    if let Some(s) = sheet {
        ui.label(egui::RichText::new("CharacterSheet").strong().color(TEAL));
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
    }
    if let Some(i) = inter {
        ui.label(format!("Interactable: {:?}", i.kind));
    }
    if let Some(mut tv) = trig {
        ui.label(egui::RichText::new("TriggerVolume").strong().color(TEAL));
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
        tv.set_if_neq(next);
    }
}

#[cfg(test)]
mod tests;
