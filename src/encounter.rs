//! E7 tactical encounter (feature `pf2e`): initiative, 3-action turns, free-movement Stride with the Reactive Strike
//! `ActionGate`, Strike / Arcane Bolt / Fireball / Raise Shield / Interact and a simple enemy AI.
//!
//! Combat state lives in the runtime-only [`Combat`] resource. Everything that changes the scene (HP in `CharacterSheet`,
//! `Transform`, `Door`) is a registered scene component, so `PlayModeSnapshot` rolls it back on Stop; `Combat` is reset
//! when the editor returns to `Edit`.

use std::time::{SystemTime, UNIX_EPOCH};

use bevy::gizmos::prelude::*;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::EguiContexts;

use crate::actors::{Actor, CharacterSheet, Faction};
use crate::arc_core::{HeightfieldGrid, UnitScale};
use crate::camera::EditorCamera;
use crate::level_builder::{Door, GridSettings, LevelTile};
use crate::movement::{ActiveActor, CombatLock, NavMap, PathGizmos, WaypointsPath, ray_point_distance, ring, sample_polyline};
use crate::navigation::{NavGrid, NavPath};
use crate::rules::actions::MultipleAttackPenalty;
use crate::rules::check::{CheckResult, DegreeOfSuccess, Dice};
use crate::rules::conditions::Conditions;
use crate::rules::encounter::*;
use crate::selection::Deleted;
use crate::studio::{StudioMode, StudioState};
use crate::terrain_render::TerrainGrid;

const ACTIONS_PER_TURN: u8 = 3;
const STEP_FT: f32 = 5.0;
const BOLT_RANGE_FT: f32 = 30.0;
const FIREBALL_RANGE_FT: f32 = 60.0;
const FIREBALL_RADIUS_FT: f32 = 15.0;
const INTERACT_RANGE_FT: f32 = 10.0;
/// Radius slack so melee between two touching bodies counts as in reach.
const BODY: f32 = 0.35;
const AI_PAUSE: f32 = 0.55;
const SEGMENT_TIMEOUT: f32 = 7.0;

// ───────────────────────────── State ─────────────────────────────

#[derive(Reflect, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ActionMode {
    #[default]
    None,
    Stride,
    Step,
    Strike,
    ArcaneBolt,
    Fireball,
    Interact,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LogKind {
    Info,
    Turn,
    Hit,
    Crit,
    Miss,
    Gate,
}

#[derive(Clone, Debug)]
pub struct LogLine {
    pub kind: LogKind,
    pub text: String,
}

/// Scripted player input (BRP / AI agents): each order acts like the equivalent click and runs on the active hero's turn.
#[derive(Reflect, Clone, Debug, PartialEq)]
pub enum CombatOrder {
    /// Pick the action the next ground click applies to.
    Mode(ActionMode),
    Stride { x: f32, z: f32 },
    Step { x: f32, z: f32 },
    Strike { target: Entity },
    ArcaneBolt { target: Entity },
    Fireball { x: f32, z: f32 },
    /// Open / close the door nearest to this ground point.
    Interact { x: f32, z: f32 },
    RaiseShield,
    EndTurn,
}

/// Queue of [`CombatOrder`]s plus an optional ground point that stands in for the mouse (route preview without a cursor).
#[derive(Resource, Reflect, Default, Clone, Debug)]
#[reflect(Resource)]
pub struct CombatInput {
    pub hover: Option<Vec2>,
    pub orders: Vec<CombatOrder>,
}

/// UI -> encounter requests handled at the start of the player's update.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Req {
    RaiseShield,
    EndTurn,
}

#[derive(Clone, Debug)]
pub struct Slot {
    pub entity: Entity,
    pub name: String,
    pub faction: Faction,
    pub initiative: i32,
    pub actions: u8,
    pub reaction: bool,
    pub map: u8,
    pub shield: bool,
    pub cond: Conditions,
}

/// Hover preview of a Stride / Step.
#[derive(Clone, Debug, Default)]
pub struct Plan {
    pub path: Vec<Vec2>,
    pub feet: f32,
    pub ok: bool,
    pub note: String,
    pub hit: Vec2,
    /// Reaction trigger points with the provoked enemy's name.
    pub gates: Vec<(Vec2, String)>,
}

struct MoveJob {
    entity: Entity,
    path: Vec<Vec2>,
    gm: GatedMove,
    issued: bool,
    timer: f32,
}

#[derive(Resource)]
pub struct Combat {
    pub active: bool,
    started: bool,
    /// Winning side once the encounter is over.
    pub concluded: Option<Faction>,
    pub round: u32,
    pub slots: Vec<Slot>,
    pub turn: usize,
    pub mode: ActionMode,
    pub log: Vec<LogLine>,
    pub plan: Option<Plan>,
    pub cursor: Vec2,
    pub hover: Option<Entity>,
    pub burst: Option<Vec2>,
    pub requests: Vec<Req>,
    pub seed: Option<u64>,
    dice: Dice,
    moving: Option<MoveJob>,
    ai_wait: f32,
}

impl Default for Combat {
    fn default() -> Self {
        Self {
            active: false,
            started: false,
            concluded: None,
            round: 0,
            slots: Vec::new(),
            turn: 0,
            mode: ActionMode::None,
            log: Vec::new(),
            plan: None,
            cursor: Vec2::ZERO,
            hover: None,
            burst: None,
            requests: Vec::new(),
            seed: None,
            dice: Dice::new(1),
            moving: None,
            ai_wait: 0.0,
        }
    }
}

impl Combat {
    pub fn say(&mut self, kind: LogKind, text: impl Into<String>) {
        self.log.push(LogLine { kind, text: text.into() });
        if self.log.len() > 300 {
            self.log.drain(..100);
        }
    }

    pub fn idx(&self, e: Entity) -> Option<usize> {
        self.slots.iter().position(|s| s.entity == e)
    }

    pub fn current(&self) -> Option<&Slot> {
        self.active.then(|| self.slots.get(self.turn)).flatten()
    }

    pub fn player_turn(&self) -> bool {
        self.current().is_some_and(|s| s.faction == Faction::Player) && self.moving.is_none()
    }

    pub fn is_moving(&self) -> bool {
        self.moving.is_some()
    }
}

type Actors<'w, 's> = Query<'w, 's, (Entity, &'static Actor, &'static mut CharacterSheet, &'static Faction, &'static mut Transform), Without<Deleted>>;

// ───────────────────────────── Pure helpers ─────────────────────────────

pub fn degree_label(d: DegreeOfSuccess) -> &'static str {
    match d {
        DegreeOfSuccess::CriticalSuccess => "CRITICAL SUCCESS",
        DegreeOfSuccess::Success => "Success",
        DegreeOfSuccess::Failure => "Failure",
        DegreeOfSuccess::CriticalFailure => "CRITICAL FAILURE",
    }
}

/// `Roll d20(17) + 10 = 27 vs AC 18`
pub fn roll_text(c: &CheckResult, modifier: i32, dc_name: &str) -> String {
    let sign = if modifier < 0 { '-' } else { '+' };
    format!("Roll d20({}) {sign} {} = {} vs {dc_name} {}", c.natural, modifier.abs(), c.total, c.dc)
}

pub fn condition_tags(s: &Slot) -> Vec<String> {
    let mut v = Vec::new();
    if s.shield {
        v.push("Shield Raised +2 AC".to_string());
    }
    if s.cond.off_guard {
        v.push("Off-Guard -2 AC".to_string());
    }
    if s.cond.frightened > 0 {
        v.push(format!("Frightened {}", s.cond.frightened));
    }
    v
}

fn str_mod(s: &CharacterSheet) -> i32 {
    (s.abilities[0] - 10).div_euclid(2)
}

fn effective_ac(slot: &Slot, s: &CharacterSheet) -> i32 {
    s.ac + if slot.shield { 2 } else { 0 } + slot.cond.ac_penalty() + slot.cond.check_penalty()
}

fn p2(tf: &Transform) -> Vec2 {
    Vec2::new(tf.translation.x, tf.translation.z)
}

fn reach_world(s: &CharacterSheet, units: &UnitScale) -> f32 {
    units.feet_to_world(s.reach()) + BODY
}

fn lay_down(tf: &mut Transform) {
    tf.rotation = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
}

/// Route of a Stride / Step: truncated to the action's range, with the Reactive Strike gates along it.
pub fn plan_move(nav: &NavGrid, units: &UnitScale, from: Vec2, to: Vec2, speed_ft: f32, step: bool, threats: &[(Threat, String)]) -> Plan {
    let mut plan = Plan { hit: to, ..default() };
    let Some(found) = nav.find_path(from, to) else {
        plan.note = "No path".into();
        return plan;
    };
    let budget = units.feet_to_world(if step { STEP_FT } else { speed_ft });
    let len = path_length(&found.points);
    if step && len > budget + 0.05 {
        plan.path = slice_path(&found.points, 0.0, budget);
        plan.feet = STEP_FT;
        plan.note = "Step is limited to 5 ft".into();
        return plan;
    }
    plan.path = if len > budget { slice_path(&found.points, 0.0, budget) } else { found.points };
    plan.feet = units.world_to_feet(path_length(&plan.path));
    plan.ok = plan.path.len() > 1;
    if len > budget + 0.05 {
        plan.note = format!("Max {} ft (route {:.0} ft)", speed_ft as i32, units.world_to_feet(len));
    }
    let ts: Vec<Threat> = threats.iter().map(|(t, _)| *t).collect();
    for g in stride_triggers(&plan.path, &ts, step) {
        let name = threats.iter().find(|(t, _)| t.id == g.threat).map(|(_, n)| n.clone()).unwrap_or_default();
        plan.gates.push((point_at(&plan.path, g.dist), name));
    }
    plan
}

// ───────────────────────────── Encounter logic (ECS-level helpers) ─────────────────────────────

fn alive(actors: &Actors, f: Faction) -> usize {
    actors.iter().filter(|(_, _, s, fac, _)| **fac == f && s.hp > 0).count()
}

fn start_combat(combat: &mut Combat, actors: &Actors) {
    if alive(actors, Faction::Player) == 0 || alive(actors, Faction::Hostile) == 0 {
        return;
    }
    let seed = combat.seed.unwrap_or_else(|| SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(7));
    combat.dice = Dice::new(seed);
    let mut slots: Vec<Slot> = actors
        .iter()
        .filter(|(_, _, s, f, _)| s.hp > 0 && **f != Faction::Neutral)
        .map(|(e, a, s, f, _)| {
            let dex = (s.abilities[1] - 10).div_euclid(2);
            Slot { entity: e, name: a.name.clone(), faction: *f, initiative: 0, actions: ACTIONS_PER_TURN, reaction: true, map: 0, shield: false, cond: Conditions::default() }.with_init(dex + s.level)
        })
        .collect();
    for s in &mut slots {
        s.initiative += combat.dice.d20() as i32;
    }
    slots.sort_by(|a, b| b.initiative.cmp(&a.initiative).then((a.faction != Faction::Player).cmp(&(b.faction != Faction::Player))).then(a.name.cmp(&b.name)));
    let order = slots.iter().map(|s| format!("{} {}", s.name, s.initiative)).collect::<Vec<_>>().join(", ");
    combat.slots = slots;
    combat.active = true;
    combat.started = true;
    combat.round = 1;
    combat.turn = 0;
    combat.say(LogKind::Info, format!("Encounter! Initiative: {order}"));
    begin_turn(combat);
}

impl Slot {
    fn with_init(mut self, bonus: i32) -> Self {
        self.initiative = bonus;
        self
    }
}

fn begin_turn(combat: &mut Combat) {
    let turn = combat.turn;
    let round = combat.round;
    let s = &mut combat.slots[turn];
    s.actions = ACTIONS_PER_TURN;
    s.reaction = true;
    s.map = 0;
    s.shield = false;
    let (name, faction) = (s.name.clone(), s.faction);
    combat.mode = if faction == Faction::Player { ActionMode::Stride } else { ActionMode::None };
    combat.plan = None;
    combat.ai_wait = AI_PAUSE;
    combat.say(LogKind::Turn, format!("— Round {round}: {name}'s turn —"));
}

fn end_turn(combat: &mut Combat, actors: &Actors) {
    if !combat.active {
        return;
    }
    if let Some(s) = combat.slots.get_mut(combat.turn) {
        s.cond.end_turn();
        s.cond.off_guard = false;
    }
    for _ in 0..combat.slots.len() {
        combat.turn += 1;
        if combat.turn >= combat.slots.len() {
            combat.turn = 0;
            combat.round += 1;
        }
        let e = combat.slots[combat.turn].entity;
        if actors.get(e).is_ok_and(|(_, _, s, _, _)| s.hp > 0) {
            break;
        }
    }
    begin_turn(combat);
}

fn check_end(combat: &mut Combat, actors: &Actors) {
    if !combat.active {
        return;
    }
    let winner = if alive(actors, Faction::Hostile) == 0 {
        Faction::Player
    } else if alive(actors, Faction::Player) == 0 {
        Faction::Hostile
    } else {
        return;
    };
    combat.active = false;
    combat.concluded = Some(winner);
    combat.moving = None;
    combat.mode = ActionMode::None;
    combat.plan = None;
    let msg = if winner == Faction::Player { "VICTORY! All enemies are down." } else { "DEFEAT. The party has fallen." };
    combat.say(LogKind::Info, msg);
}

/// Pose a fallen actor and unnerve its allies (monsters only).
fn after_down(combat: &mut Combat, actors: &mut Actors, e: Entity) {
    let Ok((_, a, _, f, mut tf)) = actors.get_mut(e) else { return };
    let (name, faction) = (a.name.clone(), *f);
    lay_down(&mut tf);
    combat.say(LogKind::Crit, format!("{name} is down!"));
    if faction == Faction::Hostile {
        let allies: Vec<Entity> = actors.iter().filter(|(x, _, s, f, _)| *x != e && **f == Faction::Hostile && s.hp > 0).map(|(x, ..)| x).collect();
        for x in allies {
            if let Some(i) = combat.idx(x) {
                combat.slots[i].cond.frightened = combat.slots[i].cond.frightened.max(1);
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Atk {
    Strike,
    Bolt,
    Reactive,
}

/// One attack roll with damage applied to `tgt`. Returns the outcome and the target's HP afterwards.
fn do_attack(combat: &mut Combat, actors: &mut Actors, att: Entity, tgt: Entity, kind: Atk) -> Option<(AttackOutcome, i32)> {
    let (ai, ti) = (combat.idx(att)?, combat.idx(tgt)?);
    let (an, bonus, agile, dice_n, dice_s, dmg_bonus, dc_bolt) = {
        let (_, a, s, ..) = actors.get(att).ok()?;
        (a.name.clone(), s.attack_bonus, s.weapon.agile, s.weapon.dice, s.weapon.sides, str_mod(&s).max(0), s.spell_dc)
    };
    let (tn, ac) = {
        let (_, a, s, ..) = actors.get(tgt).ok()?;
        (a.name.clone(), effective_ac(&combat.slots[ti], &s))
    };
    let cond = combat.slots[ai].cond;
    let map = MultipleAttackPenalty { attacks_made: combat.slots[ai].map }.penalty(agile);
    let (label, modifier, n, sides, db) = match kind {
        Atk::Strike => ("Strike", bonus + map + cond.attack_penalty() + cond.check_penalty(), dice_n, dice_s, dmg_bonus),
        Atk::Bolt => ("Arcane Bolt", dc_bolt - 10 + map + cond.check_penalty(), 2, 4, 0),
        Atk::Reactive => ("Reactive Strike", bonus + cond.check_penalty(), dice_n, dice_s, dmg_bonus),
    };
    if kind != Atk::Reactive {
        combat.slots[ai].map = combat.slots[ai].map.saturating_add(1);
    }
    let out = attack(&mut combat.dice, modifier, ac, n, sides, db);
    let hp_after = {
        let (_, _, mut s, ..) = actors.get_mut(tgt).ok()?;
        if out.damage > 0 {
            s.take_damage(out.damage, out.crit());
        }
        s.hp
    };
    let dmg = if out.hit() { format!(" {} dmg", out.damage) } else { String::new() };
    let k = if out.crit() {
        LogKind::Crit
    } else if out.hit() {
        LogKind::Hit
    } else {
        LogKind::Miss
    };
    let k = if kind == Atk::Reactive { LogKind::Gate } else { k };
    let head = if kind == Atk::Reactive { "↺ ".to_string() } else { String::new() };
    combat.say(k, format!("{head}{an} {label} on {tn}: {} -> {}!{dmg}", roll_text(&out.check, modifier, "AC"), degree_label(out.check.degree)));
    if hp_after <= 0 {
        after_down(combat, actors, tgt);
    }
    Some((out, hp_after))
}

fn threats_against(combat: &Combat, actors: &Actors, units: &UnitScale, mover: Entity) -> Vec<(Threat, String)> {
    let Some(mf) = actors.get(mover).ok().map(|(_, _, _, f, _)| *f) else { return Vec::new() };
    combat
        .slots
        .iter()
        .enumerate()
        .filter(|(_, s)| s.entity != mover && s.faction != mf && s.reaction)
        .filter_map(|(i, s)| {
            let (_, a, sh, _, tf) = actors.get(s.entity).ok()?;
            (sh.hp > 0 && sh.reactive_strike).then(|| (Threat { id: i, pos: p2(&tf), reach: reach_world(&sh, units) }, a.name.clone()))
        })
        .collect()
}

/// Reactive Strike by slot `threat` on `mover`; the gate verdict.
fn reaction(combat: &mut Combat, actors: &mut Actors, threat: usize, mover: Entity) -> Verdict {
    let Some(slot) = combat.slots.get(threat) else { return Verdict::Continue };
    if !slot.reaction {
        return Verdict::Continue;
    }
    let striker = slot.entity;
    if actors.get(striker).is_ok_and(|(_, _, s, ..)| s.hp <= 0) {
        return Verdict::Continue;
    }
    combat.slots[threat].reaction = false;
    match do_attack(combat, actors, striker, mover, Atk::Reactive) {
        Some((out, hp)) => {
            let v = verdict(&out, hp);
            if v == Verdict::Interrupt {
                combat.say(LogKind::Gate, "Critical Reactive Strike: the action is interrupted!");
            }
            v
        }
        None => Verdict::Continue,
    }
}

/// A Manipulate action (spell, door) in an enemy's reach provokes Reactive Strikes. `true` when the action is disrupted.
fn provoke(combat: &mut Combat, actors: &mut Actors, units: &UnitScale, who: Entity) -> bool {
    let threats = threats_against(combat, actors, units, who);
    let Some(pos) = actors.get(who).ok().map(|(.., tf)| p2(&tf)) else { return false };
    let ts: Vec<Threat> = threats.iter().map(|(t, _)| *t).collect();
    for id in manipulate_triggers(pos, &ts) {
        combat.say(LogKind::Gate, "ActionGate: a Manipulate action provokes a reaction");
        if reaction(combat, actors, id, who) != Verdict::Continue {
            return true;
        }
    }
    false
}

fn spend(combat: &mut Combat, e: Entity, n: u8) -> Result<(), String> {
    let i = combat.idx(e).ok_or("not in the encounter")?;
    if combat.slots[i].actions < n {
        return Err(format!("Not enough actions ({n} needed)"));
    }
    combat.slots[i].actions -= n;
    Ok(())
}

fn begin_move(combat: &mut Combat, actors: &Actors, who: Entity, path: Vec<Vec2>, step: bool, units: &UnitScale) -> Result<(), String> {
    if path.len() < 2 {
        return Err("Nowhere to go".into());
    }
    spend(combat, who, 1)?;
    let threats: Vec<Threat> = threats_against(combat, actors, units, who).into_iter().map(|(t, _)| t).collect();
    let stops = stride_triggers(&path, &threats, step);
    let name = combat.idx(who).map(|i| combat.slots[i].name.clone()).unwrap_or_default();
    combat.say(LogKind::Info, format!("{name} {} {:.0} ft", if step { "Steps" } else { "Strides" }, units.world_to_feet(path_length(&path))));
    combat.moving = Some(MoveJob { entity: who, gm: GatedMove::new(path_length(&path), stops), path, issued: false, timer: 0.0 });
    Ok(())
}

fn act_attack(combat: &mut Combat, actors: &mut Actors, units: &UnitScale, att: Entity, tgt: Entity, kind: Atk) -> Result<(), String> {
    let (reach, ap, spell) = {
        let (_, _, s, ..) = actors.get(att).map_err(|_| "no attacker")?;
        (if kind == Atk::Bolt { units.feet_to_world(BOLT_RANGE_FT) } else { reach_world(&s, units) }, p2(actors.get(att).map_err(|_| "no attacker")?.4), s.spell_dc)
    };
    if kind == Atk::Bolt && spell == 0 {
        return Err("Only casters can cast Arcane Bolt".into());
    }
    if actors.get(tgt).is_ok_and(|(_, _, s, ..)| s.hp <= 0) {
        return Err("Target is already down".into());
    }
    let tp = p2(actors.get(tgt).map_err(|_| "no target")?.4);
    if ap.distance(tp) > reach {
        return Err(format!("Target is out of range ({:.0} ft away)", units.world_to_feet(ap.distance(tp))));
    }
    spend(combat, att, 1)?;
    if kind == Atk::Bolt && provoke(combat, actors, units, att) {
        combat.say(LogKind::Gate, "Arcane Bolt is disrupted!");
        check_end(combat, actors);
        return Ok(());
    }
    do_attack(combat, actors, att, tgt, kind);
    check_end(combat, actors);
    Ok(())
}

fn act_fireball(combat: &mut Combat, actors: &mut Actors, units: &UnitScale, caster: Entity, center: Vec2) -> Result<(), String> {
    let (dc, cn, cp) = {
        let (_, a, s, _, tf) = actors.get(caster).map_err(|_| "no caster")?;
        (s.spell_dc, a.name.clone(), p2(tf))
    };
    if dc == 0 {
        return Err("Only casters can cast Fireball".into());
    }
    if cp.distance(center) > units.feet_to_world(FIREBALL_RANGE_FT) {
        return Err("Fireball: target point is beyond 60 ft".into());
    }
    spend(combat, caster, 2)?;
    if provoke(combat, actors, units, caster) {
        combat.say(LogKind::Gate, "Fireball is disrupted!");
        check_end(combat, actors);
        return Ok(());
    }
    let rolled = combat.dice.roll(6, 6);
    combat.say(LogKind::Info, format!("{cn} casts Fireball (15 ft burst, DC {dc}): 6d6 = {rolled}"));
    let radius = units.feet_to_world(FIREBALL_RADIUS_FT);
    let victims: Vec<Entity> = actors.iter().filter(|(_, _, s, _, tf)| s.hp > 0 && p2(tf).distance(center) <= radius).map(|(e, ..)| e).collect();
    for v in victims {
        let Some(i) = combat.idx(v) else { continue };
        let (vn, reflex) = {
            let (_, a, s, ..) = actors.get(v).unwrap();
            (a.name.clone(), s.saves[1])
        };
        let modifier = reflex + combat.slots[i].cond.check_penalty();
        let c = combat.dice.check(modifier, dc);
        let dmg = basic_save_damage(c.degree, rolled);
        if c.degree == DegreeOfSuccess::CriticalFailure {
            combat.slots[i].cond.off_guard = true;
        }
        let hp_after = {
            let (_, _, mut s, ..) = actors.get_mut(v).unwrap();
            if dmg > 0 {
                s.take_damage(dmg, c.degree == DegreeOfSuccess::CriticalFailure);
            }
            s.hp
        };
        let kind = match c.degree {
            DegreeOfSuccess::CriticalFailure => LogKind::Crit,
            DegreeOfSuccess::Failure => LogKind::Hit,
            _ => LogKind::Miss,
        };
        combat.say(kind, format!("{vn} Reflex save: {} -> {}! {dmg} dmg", roll_text(&c, modifier, "DC"), degree_label(c.degree)));
        if hp_after <= 0 {
            after_down(combat, actors, v);
        }
    }
    check_end(combat, actors);
    Ok(())
}

// ───────────────────────────── Systems ─────────────────────────────

fn playing(s: Res<StudioState>) -> bool {
    s.mode == StudioMode::Playing
}

fn lifecycle(studio: Res<StudioState>, mut combat: ResMut<Combat>, mut lock: ResMut<CombatLock>, actors: Actors) {
    if studio.mode == StudioMode::Edit {
        if combat.started || !combat.log.is_empty() {
            *combat = Combat::default();
        }
    } else if studio.mode == StudioMode::Playing && !combat.started && combat.concluded.is_none() {
        start_combat(&mut combat, &actors);
    }
    if lock.0 != combat.active {
        lock.0 = combat.active;
    }
}

fn move_driver(
    time: Res<Time>,
    nav: Res<NavMap>,
    grid: Res<TerrainGrid>,
    units: Res<UnitScale>,
    mut combat: ResMut<Combat>,
    mut active: ResMut<ActiveActor>,
    mut actors: Actors,
    paths: Query<(), With<WaypointsPath>>,
    mut commands: Commands,
) {
    if !combat.active {
        return;
    }
    let dt = time.delta_secs().min(0.1);
    combat.ai_wait = (combat.ai_wait - dt).max(0.0);
    let cur = combat.slots[combat.turn].entity;
    if active.0 != Some(cur) {
        active.0 = Some(cur);
    }
    let Some(mut job) = combat.moving.take() else { return };
    job.timer += dt;
    if !job.issued {
        let (from, to) = (job.gm.reached, job.gm.next_target());
        let seg = slice_path(&job.path, from, to);
        let points: Vec<Vec3> = seg.iter().map(|p| Vec3::new(p.x, grid.0.sample_height(p.x, p.y), p.y)).collect();
        commands.entity(job.entity).insert(WaypointsPath { points, current_idx: 1, total_feet: units.world_to_feet(to - from) });
        job.issued = true;
        job.timer = 0.0;
        combat.moving = Some(job);
        return;
    }
    let walking = paths.get(job.entity).is_ok();
    let _ = &nav;
    if walking && job.timer < SEGMENT_TIMEOUT {
        combat.moving = Some(job);
        return;
    }
    if walking {
        commands.entity(job.entity).remove::<WaypointsPath>();
    }
    job.issued = false;
    let who = job.entity;
    let mut gm = job.gm.clone();
    gm.arrive(|stop| reaction(&mut combat, &mut actors, stop.threat, who));
    job.gm = gm;
    check_end(&mut combat, &actors);
    if job.gm.finished || !combat.active {
        if combat.active {
            combat.ai_wait = AI_PAUSE * 0.6;
        }
        return;
    }
    combat.moving = Some(job);
}

#[allow(clippy::too_many_arguments)]
fn player_input(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    nav: Res<NavMap>,
    grid: Res<TerrainGrid>,
    units: Res<UnitScale>,
    gset: Res<GridSettings>,
    window: Single<&Window, With<PrimaryWindow>>,
    cam: Single<(&Camera, &GlobalTransform), With<EditorCamera>>,
    mut egui: EguiContexts,
    mut combat: ResMut<Combat>,
    mut input: ResMut<CombatInput>,
    mut actors: Actors,
    mut doors: Query<(&LevelTile, &mut Door), Without<Deleted>>,
) {
    if !combat.active || combat.moving.is_some() || nav.is_stale() {
        combat.plan = None;
        return;
    }
    let (kb, blocked) = egui.ctx_mut().map(|c| (c.egui_wants_keyboard_input(), c.is_pointer_over_egui() || c.egui_wants_pointer_input())).unwrap_or((false, false));
    let Some(cur) = combat.current().cloned() else { return };
    if actors.get(cur.entity).is_ok_and(|(_, _, s, ..)| s.hp <= 0) {
        // dropped mid-turn (e.g. by a Reactive Strike): the turn is over
        combat.requests.clear();
        end_turn(&mut combat, &actors);
        return;
    }
    let is_player = cur.faction == Faction::Player;
    for r in std::mem::take(&mut combat.requests) {
        if !is_player {
            continue;
        }
        match r {
            Req::EndTurn => end_turn(&mut combat, &actors),
            Req::RaiseShield => {
                let has = actors.get(cur.entity).is_ok_and(|(_, _, s, ..)| s.has_shield);
                match (has, spend(&mut combat, cur.entity, 1)) {
                    (false, _) => combat.say(LogKind::Info, format!("{} has no shield", cur.name)),
                    (_, Err(e)) => combat.say(LogKind::Info, e),
                    _ => {
                        let t = combat.turn;
                        combat.slots[t].shield = true;
                        combat.say(LogKind::Info, format!("{} raises a shield (+2 AC)", cur.name));
                    }
                }
            }
        }
    }
    let Some(cur) = combat.current().cloned() else { return };
    if !is_player || combat.slots.get(combat.turn).is_none_or(|s| s.entity != cur.entity) {
        combat.plan = None;
        return;
    }
    if !kb {
        if keys.just_pressed(KeyCode::Space) {
            end_turn(&mut combat, &actors);
            return;
        }
        if keys.just_pressed(KeyCode::Escape) {
            combat.mode = ActionMode::None;
        }
    }
    combat.plan = None;
    combat.hover = None;
    combat.burst = None;
    let (camera, gtf) = *cam;
    // one scripted order per frame acts like a click at that point
    let order = (!input.orders.is_empty()).then(|| input.orders.remove(0));
    let mut forced: Option<(ActionMode, Vec2)> = None;
    match order {
        Some(CombatOrder::Mode(m)) => combat.mode = m,
        Some(CombatOrder::EndTurn) => {
            end_turn(&mut combat, &actors);
            return;
        }
        Some(CombatOrder::RaiseShield) => combat.requests.push(Req::RaiseShield),
        Some(CombatOrder::Strike { target }) => {
            if let Err(e) = act_attack(&mut combat, &mut actors, &units, cur.entity, target, Atk::Strike) {
                combat.say(LogKind::Info, e);
            }
            return;
        }
        Some(CombatOrder::ArcaneBolt { target }) => {
            if let Err(e) = act_attack(&mut combat, &mut actors, &units, cur.entity, target, Atk::Bolt) {
                combat.say(LogKind::Info, e);
            }
            return;
        }
        Some(CombatOrder::Stride { x, z }) => forced = Some((ActionMode::Stride, Vec2::new(x, z))),
        Some(CombatOrder::Step { x, z }) => forced = Some((ActionMode::Step, Vec2::new(x, z))),
        Some(CombatOrder::Fireball { x, z }) => forced = Some((ActionMode::Fireball, Vec2::new(x, z))),
        Some(CombatOrder::Interact { x, z }) => forced = Some((ActionMode::Interact, Vec2::new(x, z))),
        None => {}
    }
    let mouse_hit = if blocked {
        None
    } else {
        window.cursor_position().and_then(|c| camera.viewport_to_world(gtf, c).ok().and_then(|ray| grid.0.raycast(ray.origin, *ray.direction).map(|h| (ray, c, Vec2::new(h.x, h.z)))))
    };
    let screen_of = |p: Vec2| camera.world_to_viewport(gtf, Vec3::new(p.x, grid.0.sample_height(p.x, p.y), p.y)).unwrap_or(Vec2::new(800.0, 450.0));
    let (ray, cursor, ground, clicked) = match (forced, mouse_hit, input.hover) {
        (Some((_, p)), _, _) => (None, screen_of(p), p, true),
        (None, Some((r, c, g)), _) => (Some(r), c, g, mouse.just_pressed(MouseButton::Left) && !keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight])),
        (None, None, Some(h)) => (None, screen_of(h), h, false),
        _ => return,
    };
    if let Some((m, _)) = forced {
        combat.mode = m;
    }
    combat.cursor = cursor;
    let Ok((_, _, sheet, _, tf)) = actors.get(cur.entity) else { return };
    let (from, speed) = (p2(tf), sheet.speed_feet);
    let under_cursor = actors
        .iter()
        .filter(|(_, _, s, ..)| s.hp > 0)
        .map(|(e, _, _, _, t)| (e, ray.as_ref().map_or_else(|| p2(t).distance(ground), |r| ray_point_distance(r, t.translation + Vec3::Y * 0.8))))
        .filter(|(_, d)| *d < 0.9)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(e, _)| e);
    let mut msg: Option<String> = None;
    match combat.mode {
        ActionMode::Stride | ActionMode::Step => {
            let Some(navgrid) = nav.grid.as_ref() else { return };
            let step = combat.mode == ActionMode::Step;
            let threats = threats_against(&combat, &actors, &units, cur.entity);
            let plan = plan_move(navgrid, &units, from, ground, speed, step, &threats);
            if clicked {
                if plan.ok {
                    if let Err(e) = begin_move(&mut combat, &actors, cur.entity, plan.path.clone(), step, &units) {
                        msg = Some(e);
                    }
                } else {
                    msg = Some(if plan.note.is_empty() { "Cannot move there".into() } else { plan.note.clone() });
                }
            }
            combat.plan = Some(plan);
        }
        ActionMode::Strike | ActionMode::ArcaneBolt => {
            let enemy = under_cursor.filter(|e| actors.get(*e).is_ok_and(|(_, _, _, f, _)| *f != cur.faction));
            combat.hover = enemy;
            if let (true, Some(t)) = (clicked, enemy) {
                let kind = if combat.mode == ActionMode::Strike { Atk::Strike } else { Atk::Bolt };
                if let Err(e) = act_attack(&mut combat, &mut actors, &units, cur.entity, t, kind) {
                    msg = Some(e);
                }
            }
        }
        ActionMode::Fireball => {
            combat.burst = Some(ground);
            if clicked {
                if let Err(e) = act_fireball(&mut combat, &mut actors, &units, cur.entity, ground) {
                    msg = Some(e);
                }
            }
        }
        ActionMode::Interact => {
            if clicked {
                let cs = gset.cell_size;
                let found = doors
                    .iter()
                    .enumerate()
                    .filter_map(|(i, (t, _))| {
                        let c = gset.cell_center(t.grid_pos);
                        let dist = match &ray {
                            Some(r) => {
                                let (lo, hi) = (c - Vec3::new(cs * 0.5, 0.0, cs * 0.5), c + Vec3::new(cs * 0.5, gset.floor_height, cs * 0.5));
                                crate::level_builder::ray_aabb(r.origin, *r.direction, lo, hi)
                            }
                            None => Some(Vec2::new(c.x, c.z).distance(ground)).filter(|d| *d <= cs),
                        };
                        dist.map(|dist| (i, dist, Vec2::new(c.x, c.z)))
                    })
                    .min_by(|a, b| a.1.total_cmp(&b.1));
                match found {
                    None => msg = Some("Click a door to interact".into()),
                    Some((_, _, c)) if from.distance(c) > units.feet_to_world(INTERACT_RANGE_FT) + cs * 0.5 => msg = Some("Door is too far (10 ft)".into()),
                    Some((i, ..)) => {
                        let locked = doors.iter().nth(i).is_some_and(|(_, d)| !d.is_open && d.is_locked);
                        if locked {
                            msg = Some("The door is locked".into());
                        } else if let Err(e) = spend(&mut combat, cur.entity, 1) {
                            msg = Some(e);
                        } else if provoke(&mut combat, &mut actors, &units, cur.entity) {
                            combat.say(LogKind::Gate, format!("{} fails to work the door", cur.name));
                            check_end(&mut combat, &actors);
                        } else if let Some((_, mut door)) = doors.iter_mut().nth(i) {
                            door.is_open = !door.is_open;
                            let state = if door.is_open { "opens" } else { "closes" };
                            combat.say(LogKind::Info, format!("{} {state} the door", cur.name));
                        }
                    }
                }
            }
        }
        ActionMode::None => {}
    }
    if let Some(m) = msg {
        combat.say(LogKind::Info, m);
    }
}

fn enemy_ai(nav: Res<NavMap>, units: Res<UnitScale>, mut combat: ResMut<Combat>, mut actors: Actors) {
    if !combat.active || combat.moving.is_some() || combat.ai_wait > 0.0 || nav.is_stale() {
        return;
    }
    let Some(cur) = combat.current().cloned() else { return };
    if cur.faction != Faction::Hostile {
        return;
    }
    let (from, speed, reach) = match actors.get(cur.entity) {
        Ok((_, _, s, _, tf)) if s.hp > 0 => (p2(&tf), s.speed_feet, reach_world(&s, &units)),
        _ => {
            end_turn(&mut combat, &actors);
            return;
        }
    };
    if cur.actions == 0 {
        end_turn(&mut combat, &actors);
        return;
    }
    // nearest living hero by route length
    let Some(navgrid) = nav.grid.as_ref() else { return };
    let heroes: Vec<(Entity, Vec2)> = actors.iter().filter(|(_, _, s, f, _)| **f == Faction::Player && s.hp > 0).map(|(e, _, _, _, t)| (e, p2(t))).collect();
    let best = heroes
        .iter()
        .filter_map(|(e, p)| navgrid.find_path(from, *p).map(|path| (*e, *p, path_length(&path.points), path.points)))
        .min_by(|a, b| a.2.total_cmp(&b.2));
    let Some((target, tpos, len, points)) = best else {
        combat.say(LogKind::Info, format!("{} cannot reach any target", cur.name));
        end_turn(&mut combat, &actors);
        return;
    };
    if from.distance(tpos) <= reach {
        let _ = act_attack(&mut combat, &mut actors, &units, cur.entity, target, Atk::Strike);
        combat.ai_wait = AI_PAUSE;
        return;
    }
    let budget = units.feet_to_world(speed);
    let stop = (len - reach * 0.75).min(budget);
    if stop < 0.2 {
        end_turn(&mut combat, &actors);
        return;
    }
    let path = slice_path(&points, 0.0, stop);
    if let Err(e) = begin_move(&mut combat, &actors, cur.entity, path, false, &units) {
        combat.say(LogKind::Info, e);
        end_turn(&mut combat, &actors);
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_combat(
    combat: Res<Combat>,
    grid: Res<TerrainGrid>,
    nav: Res<NavMap>,
    units: Res<UnitScale>,
    actors: Query<(Entity, &CharacterSheet, &Faction, &Transform), Without<Deleted>>,
    mut g: Gizmos<PathGizmos>,
) {
    if !combat.active {
        return;
    }
    let Some(cur) = combat.current() else { return };
    let hg: &HeightfieldGrid = &grid.0;
    let red = Color::srgb(1.0, 0.3, 0.25);
    let orange = Color::srgb(1.0, 0.62, 0.15);
    let green = Color::srgb(0.25, 1.0, 0.45);
    // threat circles of every Reactive Strike enemy of the active creature
    for (_, s, f, tf) in &actors {
        if s.hp > 0 && *f != cur.faction && s.reactive_strike {
            ring(&mut g, hg, p2(tf), reach_world(s, &units), red.with_alpha(0.7));
        }
    }
    let Ok((_, sheet, _, atf)) = actors.get(cur.entity) else { return };
    ring(&mut g, hg, p2(atf), 0.62, Color::srgb(1.0, 0.8, 0.25));
    if cur.faction == Faction::Player {
        if let Some(plan) = &combat.plan {
            if let Some(navgrid) = nav.grid.as_ref() {
                if plan.path.len() > 1 {
                    let route = NavPath { length: 0.0, cost: 0.0, points: plan.path.clone() };
                    let col = if plan.ok { green } else { red };
                    for w in sample_polyline(navgrid, hg, &route, 0.12).windows(2) {
                        g.line(w[0].0, w[1].0, col);
                    }
                    if let Some(end) = plan.path.last() {
                        ring(&mut g, hg, *end, 0.45, col);
                    }
                }
            }
            for (p, _) in &plan.gates {
                ring(&mut g, hg, *p, 0.3, orange);
                ring(&mut g, hg, *p, 0.18, orange);
            }
        }
        match combat.mode {
            ActionMode::Strike => ring(&mut g, hg, p2(atf), reach_world(sheet, &units), green.with_alpha(0.5)),
            ActionMode::ArcaneBolt => ring(&mut g, hg, p2(atf), units.feet_to_world(BOLT_RANGE_FT), Color::srgb(0.6, 0.5, 1.0).with_alpha(0.5)),
            ActionMode::Fireball => {
                ring(&mut g, hg, p2(atf), units.feet_to_world(FIREBALL_RANGE_FT), orange.with_alpha(0.3));
                if let Some(c) = combat.burst {
                    ring(&mut g, hg, c, units.feet_to_world(FIREBALL_RADIUS_FT), orange);
                    ring(&mut g, hg, c, units.feet_to_world(FIREBALL_RADIUS_FT) * 0.97, orange.with_alpha(0.4));
                }
            }
            _ => {}
        }
        if let Some(t) = combat.hover.and_then(|e| actors.get(e).ok()) {
            ring(&mut g, hg, p2(t.3), 0.8, red);
        }
    }
}

pub struct EncounterPlugin;

impl Plugin for EncounterPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Combat>()
            .init_resource::<CombatInput>()
            .register_type::<CombatInput>()
            .register_type::<CombatOrder>()
            .register_type::<ActionMode>()
            .add_systems(Startup, |mut u: ResMut<UnitScale>| *u = UnitScale::REAL)
            .add_systems(Update, (lifecycle, move_driver.run_if(playing), player_input.run_if(playing), enemy_ai.run_if(playing), draw_combat.run_if(playing)).chain());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::navigation::Footprint;

    fn nav_with_wall() -> NavGrid {
        let grid = HeightfieldGrid::new(81, 40.0);
        NavGrid::build(&grid, -100.0, &[Footprint::Box { c: Vec2::ZERO, half: Vec2::new(0.1, 4.0), yaw: 0.0 }], 0.35)
    }

    #[test]
    fn roll_text_matches_log_format() {
        let c = CheckResult { natural: 17, total: 27, dc: 18, degree: DegreeOfSuccess::CriticalSuccess };
        assert_eq!(roll_text(&c, 10, "AC"), "Roll d20(17) + 10 = 27 vs AC 18");
        assert_eq!(roll_text(&CheckResult { natural: 3, total: 1, dc: 15, degree: DegreeOfSuccess::CriticalFailure }, -2, "DC"), "Roll d20(3) - 2 = 1 vs DC 15");
    }

    #[test]
    fn condition_tags_list_active_states() {
        let mut s = Slot { entity: Entity::PLACEHOLDER, name: "X".into(), faction: Faction::Player, initiative: 0, actions: 3, reaction: true, map: 0, shield: true, cond: Conditions::default() };
        s.cond.frightened = 1;
        s.cond.off_guard = true;
        assert_eq!(condition_tags(&s), ["Shield Raised +2 AC", "Off-Guard -2 AC", "Frightened 1"]);
    }

    #[test]
    fn effective_ac_counts_shield_and_conditions() {
        let sheet = CharacterSheet { ac: 19, ..default() };
        let mut s = Slot { entity: Entity::PLACEHOLDER, name: "X".into(), faction: Faction::Player, initiative: 0, actions: 3, reaction: true, map: 0, shield: true, cond: Conditions::default() };
        assert_eq!(effective_ac(&s, &sheet), 21);
        s.cond.off_guard = true;
        s.cond.frightened = 1;
        assert_eq!(effective_ac(&s, &sheet), 18);
    }

    #[test]
    fn stride_plan_marks_reactive_strike_gate_and_step_does_not() {
        let nav = nav_with_wall();
        let units = UnitScale::REAL;
        let guard = Threat { id: 3, pos: Vec2::new(-5.0, 6.0), reach: 1.9 };
        let from = Vec2::new(-5.0, 5.0);
        let to = Vec2::new(-5.0, 1.0);
        let stride = plan_move(&nav, &units, from, to, 25.0, false, &[(guard, "Dungeon Guard".into())]);
        assert!(stride.ok);
        assert_eq!(stride.gates.len(), 1);
        assert_eq!(stride.gates[0].1, "Dungeon Guard");
        let step = plan_move(&nav, &units, from, from + Vec2::new(0.0, -1.0), 25.0, true, &[(guard, "Dungeon Guard".into())]);
        assert!(step.ok && step.gates.is_empty());
        let long_step = plan_move(&nav, &units, from, to, 25.0, true, &[]);
        assert!(!long_step.ok);
    }

    #[test]
    fn stride_is_truncated_to_speed() {
        let nav = nav_with_wall();
        let p = plan_move(&nav, &UnitScale::REAL, Vec2::new(-15.0, 0.0), Vec2::new(-15.0, 15.0), 25.0, false, &[]);
        assert!(p.ok && (p.feet - 25.0).abs() < 0.5, "{}", p.feet);
    }

    fn actor_world() -> (World, Entity, Entity) {
        use bevy::ecs::system::RunSystemOnce;
        let mut w = World::new();
        let mut spawn = |name: &str, f: Faction, pos: Vec3, sheet: CharacterSheet| w.spawn((Actor { name: name.into(), id: 0, template: String::new() }, sheet, f, Transform::from_translation(pos))).id();
        let guard = spawn("Guard", Faction::Hostile, Vec3::new(0.0, 0.0, 0.0), CharacterSheet { hp: 28, hp_max: 28, ac: 18, attack_bonus: 100, reactive_strike: true, abilities: [10; 6], weapon: crate::actors::WeaponSheet { dice: 1, sides: 1, ..default() }, ..default() });
        let hero = spawn("Hero", Faction::Player, Vec3::new(1.0, 0.0, 0.0), CharacterSheet { hp: 40, hp_max: 40, ac: 10, abilities: [10; 6], ..default() });
        let mut c = Combat::default();
        let slot = |e, name: &str, f| Slot { entity: e, name: name.into(), faction: f, initiative: 0, actions: 3, reaction: true, map: 0, shield: false, cond: Conditions::default() };
        c.slots = vec![slot(guard, "Guard", Faction::Hostile), slot(hero, "Hero", Faction::Player)];
        c.active = true;
        c.started = true;
        w.insert_resource(c);
        w.run_system_once(|| {}).unwrap();
        (w, guard, hero)
    }

    #[test]
    fn reaction_gate_interrupts_on_critical_and_spends_the_reaction() {
        use bevy::ecs::system::RunSystemOnce;
        let mut interrupts = 0;
        for seed in 0..40u64 {
            let (mut w, _guard, hero) = actor_world();
            w.resource_mut::<Combat>().dice = Dice::new(seed);
            let v = w
                .run_system_once(move |mut c: ResMut<Combat>, mut a: Actors| {
                    let first = reaction(&mut c, &mut a, 0, hero);
                    let again = reaction(&mut c, &mut a, 0, hero);
                    assert_eq!(again, Verdict::Continue, "one reaction per round");
                    first
                })
                .unwrap();
            assert!(!w.resource::<Combat>().slots[0].reaction);
            assert!(matches!(v, Verdict::Interrupt | Verdict::Continue), "{v:?}");
            interrupts += (v == Verdict::Interrupt) as u32;
            assert!(w.resource::<Combat>().log.iter().any(|l| l.kind == LogKind::Gate && l.text.contains("Reactive Strike")));
        }
        assert!(interrupts >= 30, "a +100 Reactive Strike crits unless the d20 is a natural 1: {interrupts}/40");
    }

    #[test]
    fn reaction_that_drops_the_mover_returns_down_and_lays_it_flat() {
        use bevy::ecs::system::RunSystemOnce;
        let (mut w, _guard, hero) = actor_world();
        w.get_mut::<CharacterSheet>(hero).unwrap().hp = 1;
        let v = w.run_system_once(move |mut c: ResMut<Combat>, mut a: Actors| reaction(&mut c, &mut a, 0, hero)).unwrap();
        assert_eq!(v, Verdict::Down);
        assert!(w.get::<CharacterSheet>(hero).unwrap().hp <= 0);
        assert_ne!(w.get::<Transform>(hero).unwrap().rotation, Quat::IDENTITY);
        w.run_system_once(|mut c: ResMut<Combat>, a: Actors| check_end(&mut c, &a)).unwrap();
        let c = w.resource::<Combat>();
        assert!(!c.active);
        assert_eq!(c.concluded, Some(Faction::Hostile));
    }

    #[test]
    fn manipulate_in_reach_provokes_and_out_of_reach_does_not() {
        use bevy::ecs::system::RunSystemOnce;
        let units = UnitScale::REAL;
        let (mut w, _guard, hero) = actor_world();
        let disrupted = w.run_system_once(move |mut c: ResMut<Combat>, mut a: Actors| provoke(&mut c, &mut a, &units, hero)).unwrap();
        let _ = disrupted;
        assert!(!w.resource::<Combat>().slots[0].reaction, "guard reacted to the cast next to it");
        let (mut w, _guard, hero) = actor_world();
        w.get_mut::<Transform>(hero).unwrap().translation = Vec3::new(9.0, 0.0, 0.0);
        let disrupted = w.run_system_once(move |mut c: ResMut<Combat>, mut a: Actors| provoke(&mut c, &mut a, &units, hero)).unwrap();
        assert!(!disrupted && w.resource::<Combat>().slots[0].reaction);
    }

    #[test]
    fn route_goes_around_closed_wall() {
        let nav = nav_with_wall();
        let p = plan_move(&nav, &UnitScale::REAL, Vec2::new(-1.0, 0.0), Vec2::new(1.0, 0.0), 100.0, false, &[]);
        assert!(p.ok);
        assert!(path_length(&p.path) > 8.0, "detours around the 8 m wall: {}", path_length(&p.path));
    }
}
