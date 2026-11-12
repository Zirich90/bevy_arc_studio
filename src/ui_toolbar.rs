//! Top toolbar: mode switch, tools, camera, history, scene presets, scene JSON.

use bevy_egui::egui;

use crate::arc_core::WorldPreset;
use crate::editor_registry::TOOL_SELECT;
use crate::gizmo_3d::GizmoMode;
use crate::mcp_commands::StudioCommand as Cmd;
use crate::studio::{CamStyle, StudioMode};
use crate::ui::i18n::{self, tr};
use crate::ui::icons::{self, Icon};
use crate::ui::*;

const SCENE_PATH_WINDOW: &str = "studio_scene_path_window";

fn scene_name(d: &UiData) -> String {
    std::path::Path::new(&d.studio.scene_path)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| tr("Untitled scene", "Безымянная сцена").into())
}

fn logo(ui: &mut egui::Ui) {
    icons::paint(ui, Icon::Logo, 18.0, TEAL);
    ui.label(egui::RichText::new("ARC").strong().color(TEAL).size(13.0));
    ui.label(egui::RichText::new("STUDIO").strong().color(TEXT).size(13.0));
    ui.add_space(6.0);
}

fn transport(ui: &mut egui::Ui, d: &mut UiData) {
    let mode = d.studio.mode;
    egui::Frame::new().fill(egui::Color32::from_rgb(30, 36, 40)).corner_radius(6.0).inner_margin(egui::Margin::same(2)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            if icons::button(ui, mode == StudioMode::Playing, Icon::Play, tr("Play (F5)", "Пуск (F5)"), 26.0).clicked() {
                d.studio.mode = StudioMode::Playing;
            }
            if icons::button(ui, mode == StudioMode::Paused, Icon::Pause, tr("Pause (F6)", "Пауза (F6)"), 26.0).clicked() && mode.in_session() {
                d.studio.mode = if mode == StudioMode::Paused { StudioMode::Playing } else { StudioMode::Paused };
            }
            if icons::button(ui, false, Icon::Stop, tr("Stop (Shift+F5)", "Стоп (Shift+F5)"), 26.0).clicked() {
                d.studio.mode = StudioMode::Edit;
            }
        });
    });
}

fn settings_menu(ui: &mut egui::Ui, d: &mut UiData) {
    icons::paint(ui, Icon::Gear, 14.0, ui.style().visuals.text_color());
    ui.menu_button(tr("Settings", "Настройки"), |ui| {
        ui.menu_button(tr("Environment & Quality", "Окружение и качество"), |ui| crate::ui_hud::controls(ui, d));
        ui.menu_button(tr("Language", "Язык"), |ui| {
            if ui.selectable_label(d.layout.lang() == crate::ui::i18n::Lang::En, "English").clicked() {
                d.layout.set_lang(crate::ui::i18n::Lang::En);
                ui.close();
            }
            if ui.selectable_label(d.layout.lang() == crate::ui::i18n::Lang::Ru, "Русский").clicked() {
                d.layout.set_lang(crate::ui::i18n::Lang::Ru);
                ui.close();
            }
        });
        let mut opacity = d.layout.panel_opacity();
        if ui.add(egui::Slider::new(&mut opacity, 0..=100).text(tr("Panel opacity", "Непрозрачность панелей")).suffix("%")).changed() {
            d.layout.set_panel_opacity(opacity);
        }
        ui.separator();
        if ui.button(tr("Separate Game", "Отдельная игра")).clicked() {
            crate::game_process::toggle_panel(ui.ctx());
            ui.close();
        }
    });
}

fn scene_menu(ui: &mut egui::Ui, d: &mut UiData) {
    ui.menu_button(format!("{} {}", tr("Scene:", "Сцена:"), scene_name(d)), |ui| {
        if ui.button(tr("Scene Path…", "Путь сцены…")).clicked() {
            ui.data_mut(|data| data.insert_temp(egui::Id::new(SCENE_PATH_WINDOW), true));
            ui.close();
        }
        let path = d.studio.scene_path.clone();
        action(ui, d, tr("Save Scene", "Сохранить сцену"), Cmd::SaveScene { path: path.clone() });
        action(ui, d, tr("Load Scene", "Загрузить сцену"), Cmd::LoadScene { path });
    });
}

/// Draws the panel; returns its bottom edge (screen y) so the HUD can sit underneath.
pub fn draw(ui: &mut egui::Ui, d: &mut UiData) -> f32 {
    let mode = d.studio.mode;
    let (accent, banner) = match mode {
        StudioMode::Playing => (EMERALD, Some(tr("PLAY MODE ACTIVE (Changes will revert on Stop)", "РЕЖИМ ИГРЫ АКТИВЕН (изменения откатятся при Stop)"))),
        StudioMode::Paused => (AMBER, Some(tr("PAUSED - PLAY MODE ACTIVE (Changes will revert on Stop)", "ПАУЗА - РЕЖИМ ИГРЫ АКТИВЕН (изменения откатятся при Stop)"))),
        StudioMode::Edit => (BORDER, None),
    };
    let frame = egui::Frame::new().fill(CHROME_BG).stroke(egui::Stroke::new(if banner.is_some() { 2.5 } else { 1.0 }, accent)).inner_margin(egui::Margin::symmetric(10, 6));
    let out = egui::Panel::top("toolbar").frame(frame).show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            logo(ui);
            if !mode.in_session() {
                if d.layout.active() == crate::ui::layout::Layout::Focus {
                    scene_menu(ui, d);
                } else {
                    menus(ui, d);
                }
                let layout = d.layout.active().label();
                ui.menu_button(layout, |ui| crate::ui::layout::menu(ui, &mut d.layout));
            }
            vsep(ui);
            transport(ui, d);
            if mode == StudioMode::Playing && ui.button(tr("Stop (Tab)", "Стоп (Tab)")).clicked() {
                d.studio.mode = StudioMode::Edit;
            }
            if !mode.in_session() {
                if d.layout.active() == crate::ui::layout::Layout::Focus {
                    if icon_chip(ui, false, Icon::Search, tr("Search command  Ctrl+K", "Поиск команд  Ctrl+K"), tr("Command Palette (Ctrl+K)", "Палитра команд (Ctrl+K)")).clicked() {
                        crate::ui::layout::open_palette(&mut d.layout);
                    }
                } else if ui.available_width() >= 120.0 && icon_chip(ui, false, Icon::Save, tr("Save Scene", "Сохранить сцену"), tr("Save Scene", "Сохранить сцену")).clicked() {
                    d.queue.push(Cmd::SaveScene { path: d.studio.scene_path.clone() });
                }
                settings_menu(ui, d);
            }
            if let Some(b) = banner {
                ui.add_space(12.0);
                ui.label(egui::RichText::new(b).strong().color(accent).size(12.5));
            }
        });
        if mode.in_session() {
            ui.horizontal(|ui| {
                ui.label(tr("Behavior signal", "Сигнал поведения"));
                let id = ui.id().with("behavior_signal");
                let mut signal = ui.data_mut(|data| data.get_temp::<String>(id)).unwrap_or_else(|| "demo.hide".into());
                ui.add(egui::TextEdit::singleline(&mut signal).desired_width(180.0));
                if ui.add_enabled(mode == StudioMode::Playing && !signal.trim().is_empty(), egui::Button::new(tr("Emit", "Отправить"))).clicked() {
                    d.queue.push(Cmd::Custom { name: "behavior.emit".into(), args: serde_json::json!({"signal": signal}).to_string() });
                }
                ui.data_mut(|data| data.insert_temp(id, signal));
            });
            return;
        }
        match d.layout.active() {
            crate::ui::layout::Layout::Smart => {
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    crate::ui::layout::workspaces(ui, &mut d.layout);
                    vsep(ui);
                    ui.menu_button(tr("Tools", "Инструменты"), |ui| tools(ui, d, true));
                    if chip(ui, d.gz.snap_ground, tr("Snap", "Привязка")).on_hover_text(tr("Snap to Ground", "Привязка к земле")).clicked() {
                        d.queue.push(Cmd::SetGizmo { mode: None, snap_ground: Some(!d.gz.snap_ground), align_normal: None });
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.weak(format!("{} {}", tr("Workspace:", "Рабочее пространство:"), crate::ui::layout::workspace_label(d.layout.workspace())));
                    });
                });
            }
            crate::ui::layout::Layout::Classic => {
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    tools(ui, d, false);
                    vsep(ui);
                    ui.menu_button(tr("Snap", "Привязка"), |ui| {
                        if ui.selectable_label(d.gz.snap_ground, tr("Snap to Ground", "Привязка к земле")).clicked() {
                            d.queue.push(Cmd::SetGizmo { mode: None, snap_ground: Some(!d.gz.snap_ground), align_normal: None });
                        }
                        if ui.selectable_label(d.gz.align_normal, tr("Align to Normal", "Выровнять по нормали")).clicked() {
                            d.queue.push(Cmd::SetGizmo { mode: None, snap_ground: None, align_normal: Some(!d.gz.align_normal) });
                        }
                    });
                    let cam = match d.studio.cam_style {
                        CamStyle::Orbit => tr("Orbit Camera", "Орбитальная камера"),
                        CamStyle::Fly => tr("Fly Camera", "Свободная камера"),
                    };
                    ui.menu_button(cam, |ui| {
                        action(ui, d, tr("Orbit Camera", "Орбитальная камера"), Cmd::SetCameraStyle(CamStyle::Orbit));
                        action(ui, d, tr("Fly Camera", "Свободная камера"), Cmd::SetCameraStyle(CamStyle::Fly));
                        action(ui, d, tr("Focus Selected (F)", "Фокус на выбранном (F)"), Cmd::FocusSelected);
                    });
                    ui.menu_button(tr("Lighting", "Освещение"), |ui| crate::ui_hud::controls(ui, d));
                });
            }
            crate::ui::layout::Layout::Focus => {}
        }
    });
    if !mode.in_session() {
        let id = egui::Id::new(SCENE_PATH_WINDOW);
        let mut open = ui.data(|data| data.get_temp::<bool>(id)).unwrap_or_default();
        egui::Window::new(tr("Scene File", "Файл сцены")).id(id).open(&mut open).collapsible(false).show(ui.ctx(), |ui| {
            ui.label(tr("Scene path", "Путь сцены"));
            ui.add(egui::TextEdit::singleline(&mut d.studio.scene_path).desired_width(320.0));
        });
        ui.data_mut(|data| data.insert_temp(id, open));
    }
    out.response.rect.bottom()
}

pub(crate) fn tools(ui: &mut egui::Ui, d: &mut UiData, compact: bool) {
    for (label, ru, kind, m) in [
        ("Select (Q)", "Выбрать (Q)", Icon::Select, GizmoMode::Select),
        ("Move (W)", "Перемещение (W)", Icon::Move, GizmoMode::Move),
        ("Rotate (E)", "Вращение (E)", Icon::Rotate, GizmoMode::Rotate),
        ("Scale (R)", "Масштаб (R)", Icon::Scale, GizmoMode::Scale),
    ] {
        let on = d.studio.tool == TOOL_SELECT && d.gz.mode == m;
        let response = if compact { icons::button(ui, on, kind, tr(label, ru), 30.0) } else { icon_chip(ui, on, kind, tr(label, ru), tr(label, ru)) };
        if response.clicked() {
            d.queue.push(Cmd::SetGizmo { mode: Some(m), snap_ground: None, align_normal: None });
            d.queue.push(Cmd::SetTool { id: TOOL_SELECT.into() });
        }
    }
    for t in d.reg.tools().into_iter().filter(|t| t.id != TOOL_SELECT).cloned().collect::<Vec<_>>() {
        let name = i18n::label(t.label);
        let shown = t.hotkey.map_or_else(|| name.to_string(), |k| format!("{name} ({})", key_label(k)));
        let kind = match t.id {
            "terrain_brush" => Icon::Brush,
            "level_builder" => Icon::Cube,
            _ => Icon::Objects,
        };
        let tip = format!("{shown} · {}", i18n::label(t.hint));
        let response = if compact { icons::button(ui, d.studio.tool == t.id, kind, &tip, 30.0) } else { icon_chip(ui, d.studio.tool == t.id, kind, &shown, &tip) };
        if response.clicked() {
            d.queue.push(Cmd::SetTool { id: t.id.to_string() });
        }
    }
    if compact {
        if icons::button(ui, d.gz.snap_ground, Icon::Grid, tr("Snap to Ground", "Привязка к земле"), 30.0).clicked() {
            d.queue.push(Cmd::SetGizmo { mode: None, snap_ground: Some(!d.gz.snap_ground), align_normal: None });
        }
        if icons::button(ui, d.gz.align_normal, Icon::GridPlus, tr("Align to Normal", "Выровнять по нормали"), 30.0).clicked() {
            d.queue.push(Cmd::SetGizmo { mode: None, snap_ground: None, align_normal: Some(!d.gz.align_normal) });
        }
    } else {
        if chip(ui, d.gz.snap_ground, tr("Snap to Ground", "Привязка к земле")).on_hover_text(tr("Snap to Ground", "Привязка к земле")).clicked() {
            d.queue.push(Cmd::SetGizmo { mode: None, snap_ground: Some(!d.gz.snap_ground), align_normal: None });
        }
        if chip(ui, d.gz.align_normal, tr("Align to Normal", "Выровнять по нормали")).on_hover_text(tr("Align to Normal", "Выровнять по нормали")).clicked() {
            d.queue.push(Cmd::SetGizmo { mode: None, snap_ground: None, align_normal: Some(!d.gz.align_normal) });
        }
    }
}

/// Focus mode: one uniform square button per tool in a vertical column (icon + tooltip + active highlight).
pub(crate) fn tools_vertical(ui: &mut egui::Ui, d: &mut UiData) {
    ui.vertical(|ui| {
        ui.set_max_width(38.0);
        ui.spacing_mut().item_spacing.y = 4.0;
        for (label, ru, kind, m) in [
            ("Select (Q)", "Выбрать (Q)", Icon::Select, GizmoMode::Select),
            ("Move (W)", "Перемещение (W)", Icon::Move, GizmoMode::Move),
            ("Rotate (E)", "Вращение (E)", Icon::Rotate, GizmoMode::Rotate),
            ("Scale (R)", "Масштаб (R)", Icon::Scale, GizmoMode::Scale),
        ] {
            if icons::button(ui, d.studio.tool == TOOL_SELECT && d.gz.mode == m, kind, tr(label, ru), 34.0).clicked() {
                d.queue.push(Cmd::SetGizmo { mode: Some(m), snap_ground: None, align_normal: None });
                d.queue.push(Cmd::SetTool { id: TOOL_SELECT.into() });
            }
        }
        for t in d.reg.tools().into_iter().filter(|t| t.id != TOOL_SELECT).cloned().collect::<Vec<_>>() {
            let name = i18n::label(t.label);
            let shown = t.hotkey.map_or_else(|| name.to_string(), |k| format!("{name} ({})", key_label(k)));
            let kind = match t.id {
                "terrain_brush" => Icon::Brush,
                "level_builder" => Icon::Cube,
                _ => Icon::Objects,
            };
            if icons::button(ui, d.studio.tool == t.id, kind, &format!("{shown} · {}", i18n::label(t.hint)), 34.0).clicked() {
                d.queue.push(Cmd::SetTool { id: t.id.to_string() });
            }
        }
        ui.separator();
        if icons::button(ui, d.gz.snap_ground, Icon::Grid, tr("Snap to Ground", "Привязка к земле"), 34.0).clicked() {
            d.queue.push(Cmd::SetGizmo { mode: None, snap_ground: Some(!d.gz.snap_ground), align_normal: None });
        }
        if icons::button(ui, d.gz.align_normal, Icon::GridPlus, tr("Align to Normal", "Выровнять по нормали"), 34.0).clicked() {
            d.queue.push(Cmd::SetGizmo { mode: None, snap_ground: None, align_normal: Some(!d.gz.align_normal) });
        }
    });
}

fn menus(ui: &mut egui::Ui, d: &mut UiData) {
    ui.menu_button(tr("File", "Файл"), |ui| {
        if ui.button(tr("Scene Path…", "Путь сцены…")).clicked() {
            ui.data_mut(|data| data.insert_temp(egui::Id::new(SCENE_PATH_WINDOW), true));
            ui.close();
        }
        let path = d.studio.scene_path.clone();
        action(ui, d, tr("Save Scene", "Сохранить сцену"), Cmd::SaveScene { path: path.clone() });
        action(ui, d, tr("Load Scene", "Загрузить сцену"), Cmd::LoadScene { path });
        ui.separator();
        for (label, ru, preset) in [
            ("Misty Dusk", "Туманный закат", WorldPreset::MistyDuskLake),
            ("Sunny Noon", "Солнечный полдень", WorldPreset::SunnyAlpineNoon),
            ("Midnight Camp", "Ночной лагерь", WorldPreset::MidnightCampfire),
            ("Golden Dawn", "Золотой рассвет", WorldPreset::GoldenAutumnDawn),
        ] {
            action(ui, d, tr(label, ru), Cmd::ApplyPreset(preset));
        }
    });
    ui.menu_button(tr("Edit", "Правка"), |ui| {
        ui.add_enabled_ui(!d.order.stack.is_empty(), |ui| action(ui, d, tr("Undo (Ctrl+Z)", "Отменить (Ctrl+Z)"), Cmd::Undo));
        ui.add_enabled_ui(!d.order.redo.is_empty(), |ui| action(ui, d, tr("Redo (Ctrl+Y)", "Вернуть (Ctrl+Y)"), Cmd::Redo));
    });
    ui.menu_button(tr("View", "Вид"), |ui| {
        if ui.button(tr("Command Palette (Ctrl+K)", "Палитра команд (Ctrl+K)")).clicked() {
            crate::ui::layout::open_palette(&mut d.layout);
            ui.close();
        }
        ui.menu_button(tr("Interface Layout", "Раскладка интерфейса"), |ui| crate::ui::layout::menu(ui, &mut d.layout));
        ui.menu_button(tr("Environment & Quality", "Окружение и качество"), |ui| crate::ui_hud::controls(ui, d));
        action(ui, d, tr("Orbit Camera", "Орбитальная камера"), Cmd::SetCameraStyle(CamStyle::Orbit));
        action(ui, d, tr("Fly Camera", "Свободная камера"), Cmd::SetCameraStyle(CamStyle::Fly));
        action(ui, d, tr("Focus Selected (F)", "Фокус на выбранном (F)"), Cmd::FocusSelected);
    });
    ui.menu_button(tr("Window", "Окно"), |ui| {
        crate::ui::layout::panels(ui, &mut d.layout);
        ui.separator();
        for panel in d.reg.panels(crate::editor_registry::PanelLocation::RightInspector) {
            if ui.button(panel.title).clicked() {
                crate::ui::layout::show_registered_panel(d, panel.id);
                ui.close();
            }
        }
    });
    ui.menu_button(tr("Help", "Справка"), |ui| {
        ui.label(tr("Q / W / E / R — Select / Move / Rotate / Scale", "Q / W / E / R — Выбор / Перемещение / Вращение / Масштаб"));
        ui.label(tr("F — Focus Selected", "F — Фокус на выбранном"));
        ui.label(tr("F5 / F6 / Shift+F5 — Play / Pause / Stop", "F5 / F6 / Shift+F5 — Пуск / Пауза / Стоп"));
    });
}

fn action(ui: &mut egui::Ui, d: &mut UiData, label: &str, command: Cmd) {
    if ui.button(label).clicked() {
        d.queue.push(command);
        ui.close();
    }
}

fn key_label(k: bevy::input::keyboard::KeyCode) -> String {
    format!("{k:?}").trim_start_matches("Key").to_string()
}