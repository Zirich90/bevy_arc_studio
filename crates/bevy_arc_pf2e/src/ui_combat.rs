//! CRPG-style tactical UI shown in Play mode (feature `pf2e`): Turn Order Bar (top), Tactical Hotbar (bottom centre),
//! Combat Log (bottom right) and the floating route label. Pure egui over the [`Combat`] resource.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};

use crate::actors::{CharacterSheet, Faction};
use crate::encounter::{ActionMode, Combat, LogKind, Req, condition_tags};
use bevy_arc_studio::studio::{StudioMode, StudioState};
use bevy_arc_studio::ui::{AMBER, BORDER, EMERALD, TEAL, chip, studio_ui_system};

/// Action-point glyphs; the symbol font is loaded at runtime when the OS has one.
#[derive(Resource)]
struct Glyphs {
    full: char,
    empty: char,
}

impl Default for Glyphs {
    fn default() -> Self {
        Self { full: '♦', empty: '♢' }
    }
}

fn install_font(mut contexts: EguiContexts, mut done: Local<bool>, mut glyphs: ResMut<Glyphs>) {
    if *done {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else { return };
    *done = true;
    for file in ["seguisym.ttf", "DejaVuSans.ttf", "Arialuni.ttf"] {
        if let Ok(bytes) = std::fs::read(std::path::Path::new("C:/Windows/Fonts").join(file)) {
            let fam = |family| egui::epaint::text::InsertFontFamily { family, priority: egui::epaint::text::FontPriority::Lowest };
            ctx.add_font(egui::epaint::text::FontInsert::new("arc_symbols", egui::FontData::from_owned(bytes), vec![fam(egui::FontFamily::Proportional), fam(egui::FontFamily::Monospace)]));
            *glyphs = Glyphs { full: '◆', empty: '◇' };
            return;
        }
    }
}

const PARTY: egui::Color32 = egui::Color32::from_rgb(90, 230, 130);
const ENEMY: egui::Color32 = egui::Color32::from_rgb(255, 110, 100);
const GOLD: egui::Color32 = egui::Color32::from_rgb(255, 205, 80);
const DIM: egui::Color32 = egui::Color32::from_gray(130);

fn frame() -> egui::Frame {
    egui::Frame::new().fill(egui::Color32::from_rgba_unmultiplied(8, 11, 16, 225)).stroke(egui::Stroke::new(1.0, BORDER)).corner_radius(10).inner_margin(egui::Margin::same(8))
}

fn faction_color(f: Faction) -> egui::Color32 {
    if f == Faction::Player { PARTY } else { ENEMY }
}

fn hp_bar(ui: &mut egui::Ui, hp: i32, max: i32, width: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 9.0), egui::Sense::hover());
    let frac = if max > 0 { (hp.max(0) as f32 / max as f32).clamp(0.0, 1.0) } else { 0.0 };
    let col = if frac > 0.6 { EMERALD } else if frac > 0.3 { AMBER } else { ENEMY };
    ui.painter().rect_filled(rect, 3.0, egui::Color32::from_gray(38));
    ui.painter().rect_filled(egui::Rect::from_min_size(rect.min, egui::vec2(rect.width() * frac, rect.height())), 3.0, col);
}

fn turn_bar(ctx: &egui::Context, combat: &Combat, sheets: &Query<(&CharacterSheet, &Faction)>, top: f32) {
    egui::Area::new(egui::Id::new("combat_turn_bar")).anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, top + 8.0)).order(egui::Order::Foreground).show(ctx, |ui| {
        frame().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("ROUND").size(10.0).color(DIM));
                    ui.label(egui::RichText::new(combat.round.to_string()).size(22.0).strong().color(GOLD));
                });
                for (i, s) in combat.slots.iter().enumerate() {
                    let Ok((sheet, _)) = sheets.get(s.entity) else { continue };
                    let down = sheet.hp <= 0;
                    let current = combat.active && i == combat.turn;
                    let stroke = if current { egui::Stroke::new(2.0, GOLD) } else { egui::Stroke::new(1.0, faction_color(s.faction).gamma_multiply(0.45)) };
                    let fill = if current { egui::Color32::from_rgba_unmultiplied(70, 56, 16, 200) } else { egui::Color32::from_rgba_unmultiplied(255, 255, 255, 10) };
                    egui::Frame::new().fill(fill).stroke(stroke).corner_radius(7).inner_margin(egui::Margin::symmetric(7, 5)).show(ui, |ui| ui.vertical(|ui| {
                        ui.set_width(112.0);
                        ui.horizontal(|ui| {
                            let c = if down { DIM } else { faction_color(s.faction) };
                            ui.label(egui::RichText::new(&s.name).strong().size(12.0).color(c));
                        });
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(format!("Init {}", s.initiative)).size(10.0).color(DIM));
                            if down {
                                ui.label(egui::RichText::new("DOWN").size(10.0).strong().color(ENEMY));
                            }
                        });
                        hp_bar(ui, sheet.hp, sheet.hp_max, 98.0);
                        ui.label(egui::RichText::new(format!("HP {}/{}   AC {}", sheet.hp.max(0), sheet.hp_max, sheet.ac + if s.shield { 2 } else { 0 })).size(10.5).monospace());
                        for tag in condition_tags(s) {
                            let c = if tag.starts_with("Shield") { TEAL } else { AMBER };
                            ui.label(egui::RichText::new(tag).size(9.5).color(c));
                        }
                    }));
                }
            });
        });
    });
}

fn pips(g: &Glyphs, actions: u8) -> String {
    (0..3).map(|i| if i < actions { g.full } else { g.empty }).collect()
}

fn hotbar(ctx: &egui::Context, g: &Glyphs, combat: &mut Combat, sheets: &Query<(&CharacterSheet, &Faction)>, playing: bool) {
    let Some(cur) = combat.current().cloned() else { return };
    let Ok((sheet, _)) = sheets.get(cur.entity) else { return };
    let mine = playing && cur.faction == Faction::Player && !combat.is_moving();
    egui::Area::new(egui::Id::new("combat_hotbar")).anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(-170.0, -12.0)).order(egui::Order::Foreground).show(ctx, |ui| {
        frame().show(ui, |ui| {
            ui.set_width(700.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(&cur.name).strong().size(14.0).color(faction_color(cur.faction)));
                ui.label(egui::RichText::new(pips(g, cur.actions)).size(20.0).color(GOLD));
                let rc = if cur.reaction { TEAL } else { egui::Color32::from_gray(80) };
                ui.label(egui::RichText::new("↺").size(20.0).color(rc)).on_hover_text("Reaction");
                if cur.faction != Faction::Player {
                    ui.label(egui::RichText::new("enemy turn…").italics().color(DIM));
                }
            });
            ui.horizontal_wrapped(|ui| {
                let (d1, d2) = (g.full.to_string(), format!("{0}{0}", g.full));
                let mut btn = |ui: &mut egui::Ui, label: String, mode: Option<ActionMode>, cost: u8, usable: bool, tip: &str| {
                    let enabled = mine && usable && cur.actions >= cost;
                    let on = mode.is_some_and(|m| combat.mode == m);
                    let r = ui.add_enabled_ui(enabled, |ui| chip(ui, on, &label)).inner.on_hover_text(tip);
                    if r.clicked() {
                        if let Some(m) = mode {
                            combat.mode = if on { ActionMode::None } else { m };
                        }
                        return true;
                    }
                    false
                };
                btn(ui, format!("{d1} Stride"), Some(ActionMode::Stride), 1, true, "Move up to your Speed; leaving a threatened area provokes Reactive Strike");
                btn(ui, format!("{d1} Step 5ft"), Some(ActionMode::Step), 1, true, "Move 5 ft without provoking");
                btn(ui, format!("{d1} Strike"), Some(ActionMode::Strike), 1, true, "Melee attack, -5 MAP on the second");
                if btn(ui, format!("{d1} Raise Shield"), None, 1, sheet.has_shield, "+2 AC until your next turn") {
                    combat.requests.push(Req::RaiseShield);
                }
                if sheet.spell_dc > 0 {
                    btn(ui, format!("{d1} Arcane Bolt"), Some(ActionMode::ArcaneBolt), 1, true, "30 ft ranged spell attack, 2d4");
                    btn(ui, format!("{d2} Cast Fireball (15ft burst)"), Some(ActionMode::Fireball), 2, true, "60 ft, 6d6 basic Reflex save");
                }
                btn(ui, format!("{d1} Interact / Open Door"), Some(ActionMode::Interact), 1, true, "Open or close a door within 10 ft");
                if mine && ui.add(egui::Button::new(egui::RichText::new("End Turn (Space)").strong().color(egui::Color32::from_rgb(20, 14, 2))).fill(GOLD)).clicked() {
                    combat.requests.push(Req::EndTurn);
                }
            });
        });
    });
}

fn log_panel(ctx: &egui::Context, combat: &Combat) {
    egui::Area::new(egui::Id::new("combat_log")).anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-12.0, -12.0)).order(egui::Order::Foreground).show(ctx, |ui| {
        frame().show(ui, |ui| {
            ui.set_width(440.0);
            ui.label(egui::RichText::new("COMBAT LOG").size(10.5).strong().color(TEAL));
            egui::ScrollArea::vertical().max_height(200.0).min_scrolled_height(200.0).stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
                for l in &combat.log {
                    let c = match l.kind {
                        LogKind::Info => egui::Color32::from_gray(190),
                        LogKind::Turn => GOLD,
                        LogKind::Hit => egui::Color32::from_rgb(255, 160, 90),
                        LogKind::Crit => egui::Color32::from_rgb(255, 90, 90),
                        LogKind::Miss => egui::Color32::from_gray(140),
                        LogKind::Gate => egui::Color32::from_rgb(120, 200, 255),
                    };
                    ui.label(egui::RichText::new(&l.text).size(11.0).color(c));
                }
            });
        });
    });
}

fn route_label(ctx: &egui::Context, combat: &Combat) {
    let Some(plan) = combat.plan.as_ref().filter(|_| combat.player_turn()) else { return };
    let col = if plan.ok { PARTY } else { ENEMY };
    egui::Area::new(egui::Id::new("combat_route_label")).fixed_pos(egui::pos2(combat.cursor.x + 18.0, combat.cursor.y - 40.0)).order(egui::Order::Tooltip).interactable(false).show(ctx, |ui| {
        frame().stroke(egui::Stroke::new(1.0, col)).show(ui, |ui| {
            ui.set_min_width(150.0);
            ui.label(egui::RichText::new(format!("{:.0} ft", plan.feet)).monospace().strong().size(15.0).color(col));
            if !plan.note.is_empty() {
                ui.label(egui::RichText::new(&plan.note).size(11.0).color(AMBER));
            }
            for (_, name) in &plan.gates {
                ui.label(egui::RichText::new(format!("! Reactive Strike: {name}")).size(11.0).color(egui::Color32::from_rgb(255, 160, 60)));
            }
        });
    });
}

fn banner(ctx: &egui::Context, combat: &Combat, top: f32) {
    let Some(w) = combat.concluded else { return };
    let (text, c) = if w == Faction::Player { ("VICTORY", PARTY) } else { ("DEFEAT", ENEMY) };
    egui::Area::new(egui::Id::new("combat_banner")).anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, top + 8.0)).order(egui::Order::Foreground).show(ctx, |ui| {
        frame().show(ui, |ui| ui.label(egui::RichText::new(text).size(26.0).strong().color(c)));
    });
}

fn combat_ui(mut contexts: EguiContexts, glyphs: Res<Glyphs>, studio: Res<StudioState>, mut combat: ResMut<Combat>, sheets: Query<(&CharacterSheet, &Faction)>) {
    if !studio.mode.in_session() {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else { return };
    let playing = studio.mode == StudioMode::Playing;
    if combat.active {
        turn_bar(ctx, &combat, &sheets, studio.toolbar_h);
        hotbar(ctx, &glyphs, &mut combat, &sheets, playing);
        route_label(ctx, &combat);
    } else {
        banner(ctx, &combat, studio.toolbar_h);
    }
    if !combat.log.is_empty() {
        log_panel(ctx, &combat);
    }
}

pub struct CombatUiPlugin;

impl Plugin for CombatUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Glyphs>().add_systems(EguiPrimaryContextPass, (install_font, combat_ui.after(studio_ui_system)));
    }
}
