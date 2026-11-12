//! Top-left telemetry + quick toggles (visible in Editor and Play mode).

use bevy::diagnostic::{EntityCountDiagnosticsPlugin, FrameTimeDiagnosticsPlugin};
use bevy_egui::egui;

use crate::arc_core::*;
use crate::mcp_commands::StudioCommand as Cmd;
use crate::ui::*;

pub fn draw(ui: &mut egui::Ui, d: &UiData) {
    let fps = d.diag.get(&FrameTimeDiagnosticsPlugin::FPS).and_then(|x| x.smoothed()).unwrap_or(0.0);
    let ms = d.diag.get(&FrameTimeDiagnosticsPlugin::FRAME_TIME).and_then(|x| x.smoothed()).unwrap_or(0.0);
    let ents = d.diag.get(&EntityCountDiagnosticsPlugin::ENTITY_COUNT).and_then(|x| x.value()).unwrap_or(0.0);
    let props = d.objs.iter().count();
    let lights = d.light_count.iter().count();
    ui.weak(egui::RichText::new(format!("{fps:.0} FPS  MS {ms:.1}")).monospace().size(11.5))
        .on_hover_text(format!("{ents:.0} {} · {props} {} · {lights} {}", i18n::tr("entities", "сущностей"), i18n::tr("props", "объектов"), i18n::tr("lights", "источников света")));
}

/// Slim full-width status bar pinned to the window bottom, concept style: context left, telemetry right.
pub fn status_bar(ui: &mut egui::Ui, d: &UiData) {
    use crate::ui::i18n::tr;
    let frame = egui::Frame::new().fill(CHROME_BG).stroke(egui::Stroke::new(1.0, BORDER)).inner_margin(egui::Margin::symmetric(10, 3));
    egui::Panel::bottom("status_bar").frame(frame).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("●").size(8.0).color(TEAL));
            let weak = |t: String| egui::RichText::new(t).size(11.5);
            match d.layout.active() {
                crate::ui::layout::Layout::Focus => {
                    ui.weak(weak(tr("FOCUS MODE", "РЕЖИМ ФОКУСА").into()));
                    ui.weak(weak("•".into()));
                    let tool = d.reg.tools().into_iter().find(|t| t.id == d.studio.tool).map(|t| crate::ui::i18n::label(t.label).to_string()).unwrap_or_else(|| tr("Select", "Выбор").into());
                    ui.weak(weak(format!("{} {}", tr("Tool:", "Инструмент:"), tool)));
                    ui.weak(weak("•".into()));
                    ui.weak(weak("Ctrl+Shift+2".into()));
                }
                crate::ui::layout::Layout::Smart => {
                    ui.weak(weak(tr("SMART WORKSPACES", "СМАРТ-ПРОСТРАНСТВА").into()));
                    ui.weak(weak("•".into()));
                    ui.weak(weak(crate::ui::layout::workspace_label(d.layout.workspace()).into()));
                    ui.weak(weak("•".into()));
                    ui.weak(weak("Ctrl+Shift+3".into()));
                }
                crate::ui::layout::Layout::Classic => {
                    let name = std::path::Path::new(&d.studio.scene_path).file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| tr("Untitled scene", "Безымянная сцена").into());
                    ui.weak(weak(format!("{} {}", tr("Scene:", "Сцена:"), name)));
                    ui.weak(weak("•".into()));
                    ui.weak(weak(format!("{} {}", d.objs.iter().count(), tr("objects", "объектов"))));
                    if !d.studio.status.is_empty() {
                        ui.weak(weak("•".into()));
                        ui.weak(weak(crate::ui::i18n::shown(&d.studio.status).to_string()));
                    }
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                draw(ui, d);
            });
        });
    });
}

/// Floating viewport chips per concept: perspective left, time/camera right, active brush top-left in Focus.
pub(crate) fn viewport_chips(ui: &mut egui::Ui, d: &mut UiData) {
    use crate::ui::i18n::tr;
    use crate::ui::icons::{self, Icon};
    let focus = d.layout.active() == crate::ui::layout::Layout::Focus;
    let bottom = if focus { if d.studio.show_stats { 40.0 } else { 16.0 } } else { 34.0 };
    let frame = || egui::Frame::new().fill(CHROME_BG).stroke(egui::Stroke::new(1.0, BORDER)).corner_radius(6).inner_margin(egui::Margin::symmetric(8, 4));
    egui::Area::new(egui::Id::new("chip_perspective")).anchor(egui::Align2::LEFT_BOTTOM, [10.0, -bottom]).show(ui.ctx(), |ui| {
        frame().show(ui, |ui| {
            ui.horizontal(|ui| {
                icons::paint(ui, Icon::Cube, 13.0, TEXT);
                ui.weak(egui::RichText::new(tr("Perspective", "Перспектива")).size(11.5));
            });
        });
    });
    egui::Area::new(egui::Id::new("chip_viewport_right")).anchor(egui::Align2::RIGHT_BOTTOM, [-10.0, -bottom]).show(ui.ctx(), |ui| {
        frame().show(ui, |ui| {
            if focus {
                let cam = match d.studio.cam_style {
                    crate::studio::CamStyle::Orbit => tr("Orbit Cam", "Орбитальная"),
                    crate::studio::CamStyle::Fly => tr("Fly Cam", "Свободная"),
                };
                ui.horizontal(|ui| {
                    icons::paint(ui, Icon::Camera, 13.0, TEXT);
                    ui.menu_button(cam, |ui| {
                        if ui.button(tr("Orbit Camera", "Орбитальная камера")).clicked() {
                            d.queue.push(Cmd::SetCameraStyle(crate::studio::CamStyle::Orbit));
                            ui.close();
                        }
                        if ui.button(tr("Fly Camera", "Свободная камера")).clicked() {
                            d.queue.push(Cmd::SetCameraStyle(crate::studio::CamStyle::Fly));
                            ui.close();
                        }
                    });
                });
            } else {
                ui.horizontal(|ui| {
                    icons::paint(ui, Icon::Sun, 13.0, TEXT);
                    ui.weak(egui::RichText::new(hhmm(d.spec.time_of_day)).monospace().size(11.5));
                });
            }
        });
    });
    if focus && d.studio.tool == "terrain_brush" {
        egui::Area::new(egui::Id::new("chip_brush_active")).anchor(egui::Align2::LEFT_TOP, [58.0, d.studio.toolbar_h + 12.0]).show(ui.ctx(), |ui| {
            frame().show(ui, |ui| {
                ui.horizontal(|ui| {
                    icons::paint(ui, Icon::Brush, 13.0, TEXT);
                    ui.weak(egui::RichText::new(tr("Terrain brush", "Кисть рельефа")).size(11.5));
                    ui.label(egui::RichText::new("●").size(8.0).color(TEAL));
                    ui.weak(egui::RichText::new(tr("Active", "Активна")).size(11.5));
                });
            });
        });
    }
}

pub(crate) fn controls(ui: &mut egui::Ui, d: &mut UiData) {
    use crate::ui::i18n::tr;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(tr("Quality", "Качество")).size(11.5).color(egui::Color32::from_gray(150)));
        for (label, ru, q) in [("Low", "Низкое", QualityPreset::Low), ("Med", "Среднее", QualityPreset::Medium), ("High", "Высокое", QualityPreset::High), ("Ultra", "Ультра", QualityPreset::Ultra)] {
            if chip(ui, d.spec.quality_preset == q, tr(label, ru)).clicked() {
                d.queue.push(Cmd::SetQuality(q));
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(format!("{} {}", tr("Time:", "Время:"), hhmm(d.spec.time_of_day))).monospace().size(12.5));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if chip(ui, d.spec.auto_sun, tr("Auto Sun", "Авто-солнце")).clicked() {
                d.queue.push(Cmd::SetSpec { field: "auto_sun".into(), value: (!d.spec.auto_sun) as u8 as f64 });
            }
            if chip(ui, false, "+").clicked() {
                d.queue.push(Cmd::SetSpec { field: "sun_speed".into(), value: (d.spec.sun_speed * 1.5).min(6.0) as f64 });
            }
            ui.label(egui::RichText::new(format!("x{:.1}", d.spec.sun_speed / 0.15)).monospace().size(11.5));
            if chip(ui, false, "-").clicked() {
                d.queue.push(Cmd::SetSpec { field: "sun_speed".into(), value: (d.spec.sun_speed / 1.5).max(0.01) as f64 });
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
        ("Water", "Вода", d.studio.show_water),
        ("Reflections", "Отражения", s.reflections),
        ("Fog", "Туман", s.fog_enabled),
        ("God Rays", "Лучи", s.god_rays),
        ("Grass", "Трава", d.studio.show_grass),
        ("Shadows", "Тени", s.shadows),
    ];
    let mut clicked = None;
    ui.horizontal_wrapped(|ui| {
        for (k, (label, ru, on)) in items.iter().enumerate() {
            if chip(ui, *on, tr(label, ru)).clicked() {
                clicked = Some(k);
            }
        }
    });
    if let Some(k) = clicked {
        match k {
            0 => d.queue.push(Cmd::SetView { show_water: Some(!d.studio.show_water), show_grass: None }),
            4 => d.queue.push(Cmd::SetView { show_water: None, show_grass: Some(!d.studio.show_grass) }),
            _ => {
                let (field, on) = match k {
                    1 => ("reflections", s.reflections),
                    2 => ("fog_enabled", s.fog_enabled),
                    3 => ("god_rays", s.god_rays),
                    _ => ("shadows", s.shadows),
                };
                d.queue.push(Cmd::SetSpec { field: field.into(), value: (!on) as u8 as f64 });
            }
        }
    }
}
