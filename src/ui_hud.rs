//! Top-left telemetry + quick toggles (visible in Editor and Play mode).

use bevy::diagnostic::{EntityCountDiagnosticsPlugin, FrameTimeDiagnosticsPlugin};
use bevy_egui::egui;

use crate::arc_core::*;
use crate::mcp_commands::StudioCommand as Cmd;
use crate::studio::StudioMode;
use crate::ui::*;

fn metric(ui: &mut egui::Ui, name: &str, value: String) {
    ui.label(egui::RichText::new(name).size(10.5).color(egui::Color32::from_gray(140)));
    ui.label(egui::RichText::new(value).size(12.5).monospace().color(egui::Color32::from_gray(230)));
}

pub fn draw(ctx: &egui::Context, d: &mut UiData) {
    let fps = d.diag.get(&FrameTimeDiagnosticsPlugin::FPS).and_then(|x| x.smoothed()).unwrap_or(0.0);
    let ms = d.diag.get(&FrameTimeDiagnosticsPlugin::FRAME_TIME).and_then(|x| x.smoothed()).unwrap_or(0.0);
    let ents = d.diag.get(&EntityCountDiagnosticsPlugin::ENTITY_COUNT).and_then(|x| x.value()).unwrap_or(0.0);
    let props = d.objs.iter().count();
    let lights = d.light_count.iter().count();
    let left = if d.studio.mode != StudioMode::Playing { crate::ui_outliner::OUTLINER_W + 16.0 } else { 10.0 };
    let pos = egui::pos2(left, d.studio.toolbar_h + 10.0);
    let frame = egui::Frame::new().fill(egui::Color32::from_rgba_unmultiplied(8, 11, 16, 205)).stroke(egui::Stroke::new(1.0, BORDER)).corner_radius(10).inner_margin(egui::Margin::same(10));
    egui::Area::new(egui::Id::new("studio_hud")).fixed_pos(pos).order(egui::Order::Foreground).show(ctx, |ui| {
        frame.show(ui, |ui| {
            ui.set_width(318.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("ARCSTUDIO").strong().size(12.0).color(TEAL));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if d.studio.mode == StudioMode::Playing && chip(ui, false, "Stop (Tab)").clicked() {
                        d.studio.mode = StudioMode::Edit;
                    }
                });
            });
            egui::Grid::new("hud_metrics").num_columns(10).spacing([6.0, 0.0]).show(ui, |ui| {
                metric(ui, "FPS", format!("{fps:.0}"));
                metric(ui, "MS", format!("{ms:.1}"));
                metric(ui, "ENT", format!("{ents:.0}"));
                metric(ui, "PROPS", format!("{props}"));
                metric(ui, "LIGHTS", format!("{lights}"));
                ui.end_row();
            });
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Quality").size(11.5).color(egui::Color32::from_gray(150)));
                for (label, q) in [("Low", QualityPreset::Low), ("Med", QualityPreset::Medium), ("High", QualityPreset::High), ("Ultra", QualityPreset::Ultra)] {
                    if chip(ui, d.spec.quality_preset == q, label).clicked() {
                        d.queue.push(Cmd::SetQuality(q));
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(format!("Time: {}", hhmm(d.spec.time_of_day))).monospace().size(12.5));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if chip(ui, d.spec.auto_sun, "Auto Sun").clicked() {
                        let (before, mut next) = ((*d.spec).clone(), (*d.spec).clone());
                        next.auto_sun = !next.auto_sun;
                        d.wh.record("Auto Sun", before, next.clone());
                        *d.spec = next;
                    }
                    if chip(ui, false, "+").clicked() {
                        d.spec.sun_speed = (d.spec.sun_speed * 1.5).min(6.0);
                    }
                    ui.label(egui::RichText::new(format!("x{:.1}", d.spec.sun_speed / 0.15)).monospace().size(11.5));
                    if chip(ui, false, "-").clicked() {
                        d.spec.sun_speed = (d.spec.sun_speed / 1.5).max(0.01);
                    }
                });
            });
            let mut t = d.spec.time_of_day;
            ui.spacing_mut().slider_width = 292.0;
            let r = ui.add(egui::Slider::new(&mut t, 0.0..=24.0).show_value(false));
            let mut ev = Ev::default();
            ev.note(&r);
            let mut next = (*d.spec).clone();
            next.time_of_day = t;
            let mut cur = (*d.spec).clone();
            if ev.changed || ev.finish {
                track_ev(&ev, &mut d.wh, "Time of day", &mut cur, next);
                if ev.changed {
                    *d.spec = cur;
                }
            }
            let s = d.spec.clone();
            let items = [
                ("Water", d.studio.show_water),
                ("Reflections", s.reflections),
                ("Fog", s.fog_enabled),
                ("God Rays", s.god_rays),
                ("Grass", d.studio.show_grass),
                ("Shadows", s.shadows),
            ];
            let mut clicked = None;
            ui.horizontal_wrapped(|ui| {
                for (k, (label, on)) in items.iter().enumerate() {
                    if chip(ui, *on, label).clicked() {
                        clicked = Some(k);
                    }
                }
            });
            if let Some(k) = clicked {
                match k {
                    0 => d.studio.show_water = !d.studio.show_water,
                    4 => d.studio.show_grass = !d.studio.show_grass,
                    _ => {
                        let mut next = s.clone();
                        match k {
                            1 => next.reflections = !next.reflections,
                            2 => next.fog_enabled = !next.fog_enabled,
                            3 => next.god_rays = !next.god_rays,
                            _ => next.shadows = !next.shadows,
                        }
                        d.wh.record(format!("Toggle {}", items[k].0), s, next.clone());
                        *d.spec = next;
                    }
                }
            }
        });
    });
}
