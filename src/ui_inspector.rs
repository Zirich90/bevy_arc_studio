//! Right inspector (~360 px): live search + tabs for every `EditorRegistry` panel (Global Settings and Terrain Brushes come from
//! `WorldEnvironmentPlugin`); the core contributes the Objects & Spawner panel with the universal component inspector.

use std::ops::RangeInclusive;

use bevy::prelude::*;
use bevy_egui::egui;

use crate::arc_core::*;
use crate::gizmo_3d::ObjState;
use crate::lighting::NightLight;
use crate::mcp_commands::StudioCommand as Cmd;
use crate::props::assets_dir;
use crate::editor_registry::PanelLocation;
use crate::studio::StudioState;
use crate::ui::*;
use crate::ui::i18n::{self, label, tr};
use crate::ui::icons::{self, Icon};

/// Pending (not yet released) edit of geometry-affecting WorldSpec fields — terrain regenerates once, on release.
#[derive(Resource, Default)]
pub struct InspectorDraft(pub Option<WorldSpec>);

pub struct Filter(pub String);

impl Filter {
    pub fn ok(&self, label: &str) -> bool {
        self.0.is_empty() || label.to_lowercase().contains(&self.0)
    }
}

pub fn section(ui: &mut egui::Ui, f: &Filter, title: &str, labels: &[&str], body: impl FnOnce(&mut egui::Ui)) {
    let shown = i18n::shown(title);
    if !(f.ok(title) || f.ok(shown) || labels.iter().any(|l| f.ok(l) || f.ok(i18n::shown(l)))) {
        return;
    }
    let mut h = egui::CollapsingHeader::new(egui::RichText::new(shown).strong().color(TEAL)).default_open(true);
    if !f.0.is_empty() {
        h = h.open(Some(true));
    }
    h.show(ui, body);
}

pub fn srow<N: egui::emath::Numeric>(ui: &mut egui::Ui, f: &Filter, ev: &mut Ev, label: &str, v: &mut N, range: RangeInclusive<N>) {
    let shown = i18n::shown(label);
    if f.ok(label) || f.ok(shown) {
        ui.horizontal(|ui| {
            ui.add_sized([96.0, 18.0], egui::Label::new(egui::RichText::new(shown).size(12.0)).truncate())
                .on_hover_text(label);
            ev.note(&ui.add(egui::Slider::new(v, range.clone()).show_value(false)));
            ev.note(&ui.add(egui::DragValue::new(v).range(range).max_decimals(2)));
        });
    }
}

pub fn crow(ui: &mut egui::Ui, f: &Filter, ev: &mut Ev, label: &str, v: &mut bool) {
    let shown = i18n::shown(label);
    if f.ok(label) || f.ok(shown) {
        ev.note(&ui.checkbox(v, shown));
    }
}

pub fn draw(ui: &mut egui::Ui, d: &mut UiData) {
    let frame = egui::Frame::new().fill(d.layout.panel_fill()).stroke(egui::Stroke::new(1.0, BORDER)).inner_margin(egui::Margin::same(10));
    let width = (ui.ctx().viewport_rect().width() * 0.36).clamp(116.0, 360.0);
    if d.layout.active() == crate::ui::layout::Layout::Focus {
        let mut open = true;
        let x = (ui.ctx().viewport_rect().width() - width - 16.0).max(16.0);
        egui::Window::new("Inspector")
            .id(egui::Id::new("focus_inspector"))
            .default_pos(egui::pos2(x, d.studio.toolbar_h + 12.0))
            .default_width(width)
            .max_height((ui.available_height() * 0.75).max(100.0))
            .collapsible(false)
            .open(&mut open)
            .frame(frame)
            .show(ui.ctx(), |ui| inspector_content(ui, d));
        if !open {
            d.layout.toggle(crate::ui::layout::Panel::Inspector);
        }
    } else {
        egui::Panel::right("inspector").exact_size(width).resizable(false).frame(frame)
            .show(ui, |ui| inspector_content(ui, d));
    }
}

fn inspector_content(ui: &mut egui::Ui, d: &mut UiData) {
    if d.layout.active() != crate::ui::layout::Layout::Focus {
        ui.label(egui::RichText::new(tr("INSPECTOR", "ИНСПЕКТОР")).strong().color(TEAL));
    }
    let mut q = d.studio.search.clone();
    ui.horizontal(|ui| {
        icons::paint(ui, icons::Icon::Search, 14.0, egui::Color32::from_gray(150));
        ui.add(egui::TextEdit::singleline(&mut q).hint_text(tr("Search parameters or objects...", "Поиск параметров или объектов...")).desired_width(f32::INFINITY));
    });
    if q != d.studio.search {
        d.studio.search = q;
    }
    let panels = d.reg.panels(PanelLocation::RightInspector);
    use crate::ui::layout::{Layout, Workspace};
    let mode = d.layout.active();
    let focus = mode == Layout::Focus;
    let workspace = d.layout.workspace();
    if mode == Layout::Smart {
        smart_workspace(ui, d, workspace);
        return;
    }
    let tabs = || panels.iter().filter(|p| mode != Layout::Smart || Workspace::for_panel(p.id) == workspace);
    let context = if focus {
        let tool_panel = d.reg.tool(&d.studio.tool).and_then(|tool| tool.panel);
        let water = d.water.picking || d.water.selected.is_some();
        d.layout.inspector_context(tool_panel.unwrap_or(if water { "global" } else { "objects" }))
    } else {
        d.layout.classic_context(&d.studio.tab)
    };
    let Some(current) = panels.iter().find(|p| p.id == context).or_else(|| tabs().next()).cloned() else {
        ui.weak(tr("No inspector panels registered.", "Панели инспектора не зарегистрированы."));
        return;
    };
    if focus {
        ui.horizontal(|ui| {
            icons::paint(ui, icons::panel(current.id), 16.0, TEAL);
            ui.heading(label(current.title));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let pinned = d.layout.pinned(crate::ui::layout::Panel::Inspector);
                if icon_chip(ui, pinned, Icon::Pin, tr("Pin", "Пин"), tr("Pin this context: keeps the panel content when the active tool changes", "Закрепить контекст: содержимое панели не меняется при смене инструмента")).clicked() {
                    d.layout.toggle_pin(crate::ui::layout::Panel::Inspector);
                }
            });
        });
    } else {
        ui.horizontal(|ui| {
            const PRIMARY: [&str; 3] = ["global", "terrain_settings", "objects"];
            for p in tabs().into_iter().filter(|p| PRIMARY.contains(&p.id)) {
                let text = match p.id {
                    "global" => tr("Global", "Общие"),
                    "terrain_settings" => tr("Terrain", "Ландшафт"),
                    _ => tr("Objects", "Объекты"),
                };
                if icon_chip(ui, current.id == p.id, icons::panel(p.id), text, label(p.title)).clicked() {
                    d.layout.show_classic_panel(p.id);
                    d.studio.tab = p.id.to_string();
                }
            }
            let more = if PRIMARY.contains(&current.id) { tr("More", "Ещё") } else { label(current.title) };
            ui.menu_button(more, |ui| {
                for p in tabs().into_iter().filter(|p| !PRIMARY.contains(&p.id)) {
                    if ui.selectable_label(current.id == p.id, label(p.title)).clicked() {
                        d.layout.show_classic_panel(p.id);
                        d.studio.tab = p.id.to_string();
                        ui.close();
                    }
                }
            });
        });
    }
    ui.separator();
    egui::ScrollArea::vertical().id_salt("inspector_scroll").auto_shrink([false, false]).show(ui, |ui| {
        let f = Filter(d.studio.search.to_lowercase());
        (current.render)(ui, d, &f);
    });
}

/// Smart Workspaces: the right panel is a task-specific composition of registry panels (not one universal tab).
fn smart_workspace(ui: &mut egui::Ui, d: &mut UiData, workspace: crate::ui::layout::Workspace) {
    use crate::editor_registry::TOOL_SELECT;
    use crate::ui::layout::Workspace;
    use crate::world_environment::TOOL_TERRAIN_BRUSH;
    let (title, kind) = match workspace {
        Workspace::Scene => (tr("Object Properties", "Свойства объекта"), Icon::Objects),
        Workspace::Terrain => (tr("Terrain Parameters", "Параметры ландшафта"), Icon::Mountain),
        Workspace::World => (tr("Environment Settings", "Настройки окружения"), Icon::Sun),
        Workspace::Build => (tr("Level Builder", "Конструктор уровней"), Icon::Cube),
        Workspace::Modules => (tr("Modules", "Модули"), Icon::Gear),
    };
    ui.horizontal(|ui| {
        icons::paint(ui, kind, 16.0, TEAL);
        ui.heading(title);
    });
    let panels: Vec<_> = d.reg.panels(PanelLocation::RightInspector).into_iter().filter(|p| Workspace::for_panel(p.id) == workspace).collect();
    let primary = d.layout.smart_context(panels.first().map(|p| p.id));
    if workspace == Workspace::Terrain {
        let active = d.studio.tool == TOOL_TERRAIN_BRUSH;
        ui.horizontal(|ui| {
            if icon_chip(ui, active, Icon::Brush, if active { tr("Brush active", "Кисть активна") } else { tr("Activate brush", "Включить кисть") },
                tr("Arm the Terrain Brush explicitly (opening this workspace never auto-activates it)", "Явное включение кисти рельефа (открытие пространства не активирует её автоматически)"))
                .clicked()
            {
                d.reg.activate_tool(&mut d.studio, if active { TOOL_SELECT } else { TOOL_TERRAIN_BRUSH });
            }
            ui.weak(tr("sculpt / paint / erase", "лепка / покраска / стирание"));
        });
    }
    ui.separator();
    let f = Filter(d.studio.search.to_lowercase());
    egui::ScrollArea::vertical().id_salt("smart_workspace").auto_shrink([false, false]).show(ui, |ui| {
        for p in &panels {
            let open = Some(p.id) == primary.as_deref() || !matches!(p.id, "node_graph" | "quillshade" | "materials" | "terrain_layers");
            egui::CollapsingHeader::new(egui::RichText::new(label(p.title)).strong().color(TEAL)).id_salt(p.id).default_open(open).show(ui, |ui| (p.render)(ui, d, &f));
        }
        if workspace == Workspace::World {
            quality_section(ui, d, &f);
        }
        if panels.is_empty() && workspace != Workspace::World {
            ui.weak(tr("No panels available in this workspace.", "В этом рабочем пространстве нет панелей."));
        }
    });
}

fn quality_section(ui: &mut egui::Ui, d: &mut UiData, f: &Filter) {
    section(ui, f, tr("Quality", "Качество"), &["quality", "качество", "low", "medium", "high", "ultra"], |ui| {
        ui.horizontal_wrapped(|ui| {
            for (name, q) in [(tr("Low", "Низкое"), QualityPreset::Low), (tr("Medium", "Среднее"), QualityPreset::Medium), (tr("High", "Высокое"), QualityPreset::High), (tr("Ultra", "Ультра"), QualityPreset::Ultra)] {
                if chip(ui, d.spec.quality_preset == q, name).clicked() {
                    d.queue.push(Cmd::SetQuality(q));
                }
            }
        });
    });
}

/// Registered panels of the left / bottom docks (nothing is drawn while a dock has no panels).
pub fn draw_docks(ui: &mut egui::Ui, d: &mut UiData) {
    let fill = d.layout.panel_fill();
    let frame = || egui::Frame::new().fill(fill).stroke(egui::Stroke::new(1.0, BORDER)).inner_margin(egui::Margin::same(8));
    for (loc, name) in [(PanelLocation::Left, "registry_left"), (PanelLocation::Bottom, "registry_bottom")] {
        let panels = d.reg.panels(loc);
        if panels.is_empty() {
            continue;
        }
        let f = Filter(d.studio.search.to_lowercase());
        let body = |ui: &mut egui::Ui, d: &mut UiData| {
            for p in &panels {
                egui::CollapsingHeader::new(egui::RichText::new(label(p.title)).strong().color(TEAL)).id_salt(p.id).default_open(true).show(ui, |ui| (p.render)(ui, d, &f));
            }
        };
        if loc == PanelLocation::Left {
            egui::Panel::left(name).frame(frame()).show(ui, |ui| body(ui, d));
        } else {
            egui::Panel::bottom(name).frame(frame()).show(ui, |ui| body(ui, d));
        }
    }
}

// ───────────────────────────── Objects & Spawner ─────────────────────────────

pub(crate) fn spawn_point(d: &UiData) -> (f32, f32) {
    let half = d.grid.0.size * 0.5 - 4.0;
    match d.cam.single() {
        Ok(c) => {
            let p = if d.studio.cam_style == crate::studio::CamStyle::Fly { c.eye() + c.forward() * 12.0 } else { c.pivot };
            (p.x.clamp(-half, half), p.z.clamp(-half, half))
        }
        Err(_) => (0.0, 0.0),
    }
}

pub fn objects_tab(ui: &mut egui::Ui, d: &mut UiData, f: &Filter) {
    ui.label(egui::RichText::new(tr("Spawn at scene center", "Спавн в центре сцены")).strong().color(TEAL));
    let (sx, sz) = spawn_point(d);
    for cat in d.reg.spawner_categories() {
        if cat != d.reg.spawner_items()[0].category {
            ui.label(egui::RichText::new(label(cat)).strong().color(TEAL));
        }
        let items: Vec<_> = d.reg.spawner_items().into_iter().filter(|i| i.category == cat).cloned().collect();
        ui.horizontal_wrapped(|ui| {
            for it in items {
                if chip(ui, false, &format!("+ {}", label(it.label))).clicked() {
                    (it.spawn)(&mut d.queue, sx, sz);
                }
            }
        });
    }
    ui.horizontal(|ui| {
        let mut name = d.studio.glb_name.clone();
        ui.add(egui::TextEdit::singleline(&mut name).hint_text(tr("model.glb in assets/models/", "model.glb в assets/models/")).desired_width(190.0));
        if name != d.studio.glb_name {
            d.studio.glb_name = name.clone();
        }
        if chip(ui, false, tr("Import .glb", "Импорт .glb")).clicked() && !name.trim().is_empty() {
            let mut file = name.trim().to_string();
            if !file.to_lowercase().ends_with(".glb") && !file.to_lowercase().ends_with(".gltf") {
                file.push_str(".glb");
            }
            d.queue.push(Cmd::SpawnObject { kind: format!("model:{file}"), x: sx, z: sz, yaw: 0.0 });
        }
    });
    let avail: Vec<String> = std::fs::read_dir(assets_dir().join("models")).map(|rd| rd.flatten().filter_map(|e| e.file_name().into_string().ok()).filter(|n| n.to_lowercase().ends_with(".glb") || n.to_lowercase().ends_with(".gltf")).collect()).unwrap_or_default();
    if avail.is_empty() {
        ui.weak(tr("assets/models/ is empty - drop .glb files there", "assets/models/ пуста - положите туда .glb файлы"));
    } else {
        ui.horizontal_wrapped(|ui| {
            for a in &avail {
                if ui.small_button(a).clicked() {
                    d.queue.push(Cmd::SpawnObject { kind: format!("model:{a}"), x: sx, z: sz, yaw: 0.0 });
                }
            }
        });
    }
    ui.separator();

    // object list
    let hide = d.studio.hide_scatter;
    let mut hs = hide;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(tr("Scene objects", "Объекты сцены")).strong().color(TEAL));
        ui.checkbox(&mut hs, tr("hide grass/reeds", "скрыть траву/камыш"));
    });
    if hs != hide {
        d.studio.hide_scatter = hs;
    }
    let mut rows: Vec<(Entity, u32, String, bool)> = d
        .objs
        .iter()
        .filter(|(_, _, so, u)| !(hide && u.is_none() && matches!(so.name.as_str(), "GrassClump" | "Reeds")) && (f.0.is_empty() || f.ok(&so.name) || so.id.to_string().contains(&f.0)))
        .map(|(e, _, so, u)| (e, so.id, so.name.clone(), u.is_some()))
        .collect();
    rows.sort_by(|a, b| b.3.cmp(&a.3).then_with(|| if a.3 { b.1.cmp(&a.1) } else { a.2.cmp(&b.2).then(a.1.cmp(&b.1)) }));
    ui.weak(format!("{}: {}", tr("Shown", "Показано"), rows.len()));
    let row_h = ui.spacing().interact_size.y + ui.spacing().item_spacing.y;
    egui::ScrollArea::vertical().id_salt("object_list").max_height(row_h * 7.0).auto_shrink([false, true]).show_rows(ui, row_h, rows.len(), |ui, range| {
        for i in range {
            let (e, id, name, user) = &rows[i];
            let label = format!("#{id}  {name}{}", if *user { format!("  {}", tr("(user)", "(пользователь)")) } else { String::new() });
            if ui.selectable_label(d.sel.selected.contains(e), label).clicked() {
                d.sel.set_one(*e);
            }
        }
    });
    ui.separator();

    // details of the primary selection
    let Some(e) = d.sel.primary else {
        ui.weak(tr("Click an object in the viewport or in the list.", "Кликните объект во вьюпорте или в списке."));
        return;
    };
    let Ok((_, mut tf, so, _)) = d.objs.get_mut(e) else { return };
    egui::Frame::new().fill(CHROME_BG).corner_radius(6.0).inner_margin(egui::Margin::symmetric(8, 6)).show(ui, |ui| {
        ui.horizontal(|ui| {
            icons::paint(ui, Icon::Objects, 16.0, TEAL);
            ui.label(egui::RichText::new(&so.name).strong().size(14.0));
            ui.weak(egui::RichText::new(format!("#{}", so.id)).monospace().size(11.5));
        });
    });
    let so_id = so.id;
    let before_tf = *tf;
    let mut pos = tf.translation.to_array();
    let (ry, rx, rz) = tf.rotation.to_euler(EulerRot::YXZ);
    let mut rot = [rx.to_degrees(), ry.to_degrees(), rz.to_degrees()];
    let mut scl = tf.scale.to_array();
    let mut ev = Ev::default();
    ui.label(egui::RichText::new(tr("Transform", "Трансформация")).strong().size(12.0).color(TEAL));
    let mut vec3 = |ui: &mut egui::Ui, title: &str, v: &mut [f32; 3], speed: f64, suffix: &str, min: f32| {
        ui.horizontal(|ui| {
            ui.add_sized([62.0, 18.0], egui::Label::new(egui::RichText::new(title).size(12.0)));
            for (x, (n, c)) in v.iter_mut().zip([("X", AXIS_X), ("Y", AXIS_Y), ("Z", AXIS_Z)]) {
                ui.label(egui::RichText::new(n).monospace().size(11.5).color(c));
                ev.note(&ui.add(egui::DragValue::new(x).speed(speed).suffix(suffix).range(min..=f32::MAX).max_decimals(2)));
            }
        });
    };
    vec3(ui, tr("Position", "Положение"), &mut pos, 0.1, "", f32::MIN);
    vec3(ui, tr("Rotation", "Вращение"), &mut rot, 1.0, "°", f32::MIN);
    vec3(ui, tr("Scale", "Масштаб"), &mut scl, 0.02, "", 0.05);
    let ground_y = d.grid.0.sample_height(pos[0], pos[2]);
    let drop = icon_chip(ui, false, Icon::Mountain, tr("Drop to Ground", "Опустить на землю"), tr("Snap the object to the terrain height", "Поставить объект на высоту рельефа")).clicked();
    if drop {
        pos[1] = ground_y;
        ev.changed = true;
        ev.finish = true;
    }
    if ev.changed {
        let next_tf = Transform { translation: Vec3::from(pos), rotation: Quat::from_euler(EulerRot::YXZ, rot[1].to_radians(), rot[0].to_radians(), rot[2].to_radians()), scale: Vec3::from(scl) };
        let mut cur = vec![ObjState { entity: e, tf: before_tf, alive: true }];
        track_ev(&ev, &mut d.sh, if drop { "Drop to ground" } else { "Inspector edit" }, &mut cur, vec![ObjState { entity: e, tf: next_tf, alive: true }]);
        *tf = cur[0].tf;
    } else if ev.finish {
        let mut cur = vec![ObjState { entity: e, tf: before_tf, alive: true }];
        let same = cur.clone();
        track_ev(&ev, &mut d.sh, "Inspector edit", &mut cur, same);
    }

    match crate::ui_reflect::draw(ui, &mut d.rview, &mut d.redits, &d.reg, &f.0) {
        Some(crate::ui_reflect::CompAction::Add(name)) => d.queue.push(Cmd::AddComponent { target: crate::operators::Target::Id(so_id), component: name, value: None }),
        Some(crate::ui_reflect::CompAction::Remove(name)) => d.queue.push(Cmd::RemoveComponent { target: crate::operators::Target::Id(so_id), component: name }),
        None => {}
    }

    // PointLight child
    let mut stack = vec![e];
    let mut light = None;
    while let Some(n) = stack.pop() {
        if d.lights.contains(n) {
            light = Some(n);
            break;
        }
        if let Ok(ch) = d.children.get(n) {
            stack.extend(ch.iter());
        }
    }
    if let Some(le) = light {
        if let Ok((pl, nl)) = d.lights.get(le) {
            ui.separator();
            ui.label(egui::RichText::new("PointLight").strong().color(TEAL));
            let mut new_pl = pl.clone();
            let mut pl_changed = false;
            let c = pl.color.to_srgba();
            let mut rgb = [c.red, c.green, c.blue];
            ui.horizontal(|ui| {
                if egui::color_picker::color_edit_button_rgb(ui, &mut rgb).changed() {
                    new_pl.color = Color::srgb(rgb[0], rgb[1], rgb[2]);
                    pl_changed = true;
                }
                ui.label(tr("color", "цвет"));
            });
            let mut range = pl.range;
            if ui.add(egui::Slider::new(&mut range, 1.0..=120.0).text(tr("radius (range)", "радиус (дальность)"))).changed() {
                new_pl.range = range;
                pl_changed = true;
            }
            let mut src = pl.radius;
            if ui.add(egui::Slider::new(&mut src, 0.0..=2.0).text(tr("source radius", "радиус источника"))).changed() {
                new_pl.radius = src;
                pl_changed = true;
            }
            match nl {
                Some(nl) => {
                    let mut base = nl.base;
                    if ui.add(egui::Slider::new(&mut base, 0.0..=400_000.0).logarithmic(true).text(tr("intensity (night)", "яркость (ночь)"))).changed() {
                        let mut new_nl = nl.clone();
                        new_nl.base = base;
                        d.redits.0.push((le, std::any::TypeId::of::<NightLight>(), Box::new(new_nl)));
                    }
                }
                None => {
                    let mut i = pl.intensity;
                    if ui.add(egui::Slider::new(&mut i, 0.0..=400_000.0).logarithmic(true).text(tr("intensity", "яркость"))).changed() {
                        new_pl.intensity = i;
                        pl_changed = true;
                    }
                }
            }
            if pl_changed {
                d.redits.0.push((le, std::any::TypeId::of::<PointLight>(), Box::new(new_pl)));
            }
        }
    }
}

#[allow(dead_code)]
fn _keep(_: &StudioState) {}
