//! Right inspector (~360 px): live search + tabs for every `EditorRegistry` panel (Global Settings and Terrain Brushes come from
//! `WorldEnvironmentPlugin`); the core contributes the Objects & Spawner panel with the universal component inspector.

use std::ops::RangeInclusive;

use bevy::prelude::*;
use bevy_egui::egui;

use crate::arc_core::*;
use crate::gizmo_3d::ObjState;
use crate::mcp_commands::StudioCommand as Cmd;
use crate::props::assets_dir;
use crate::editor_registry::PanelLocation;
use crate::studio::StudioState;
use crate::ui::*;

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
    if !(f.ok(title) || labels.iter().any(|l| f.ok(l))) {
        return;
    }
    let mut h = egui::CollapsingHeader::new(egui::RichText::new(title).strong().color(TEAL)).default_open(true);
    if !f.0.is_empty() {
        h = h.open(Some(true));
    }
    h.show(ui, body);
}

pub fn srow<N: egui::emath::Numeric>(ui: &mut egui::Ui, f: &Filter, ev: &mut Ev, label: &str, v: &mut N, range: RangeInclusive<N>) {
    if f.ok(label) {
        ev.note(&ui.add(egui::Slider::new(v, range).text(label)));
    }
}

pub fn crow(ui: &mut egui::Ui, f: &Filter, ev: &mut Ev, label: &str, v: &mut bool) {
    if f.ok(label) {
        ev.note(&ui.checkbox(v, label));
    }
}

pub fn draw(ui: &mut egui::Ui, d: &mut UiData) {
    let frame = egui::Frame::new().fill(PANEL_BG).stroke(egui::Stroke::new(1.0, BORDER)).inner_margin(egui::Margin::same(10));
    egui::Panel::right("inspector").exact_size(360.0).resizable(false).frame(frame).show(ui, |ui| {
        let mut q = d.studio.search.clone();
        ui.add(egui::TextEdit::singleline(&mut q).hint_text("Search parameters or objects...").desired_width(f32::INFINITY));
        if q != d.studio.search {
            d.studio.search = q;
        }
        let panels = d.reg.panels(PanelLocation::RightInspector);
        let Some(current) = panels.iter().find(|p| p.id == d.studio.tab).or(panels.first()).cloned() else {
            ui.weak("No inspector panels registered.");
            return;
        };
        ui.horizontal_wrapped(|ui| {
            for p in &panels {
                if chip(ui, current.id == p.id, p.title).clicked() {
                    d.studio.tab = p.id.to_string();
                }
            }
        });
        ui.separator();
        egui::ScrollArea::vertical().id_salt("inspector_scroll").auto_shrink([false, false]).show(ui, |ui| {
            let f = Filter(d.studio.search.to_lowercase());
            (current.render)(ui, d, &f);
        });
    });
}

/// Registered panels of the left / bottom docks (nothing is drawn while a dock has no panels).
pub fn draw_docks(ui: &mut egui::Ui, d: &mut UiData) {
    let frame = || egui::Frame::new().fill(PANEL_BG).stroke(egui::Stroke::new(1.0, BORDER)).inner_margin(egui::Margin::same(8));
    for (loc, name) in [(PanelLocation::Left, "registry_left"), (PanelLocation::Bottom, "registry_bottom")] {
        let panels = d.reg.panels(loc);
        if panels.is_empty() {
            continue;
        }
        let f = Filter(d.studio.search.to_lowercase());
        let body = |ui: &mut egui::Ui, d: &mut UiData| {
            for p in &panels {
                egui::CollapsingHeader::new(egui::RichText::new(p.title).strong().color(TEAL)).id_salt(p.id).default_open(true).show(ui, |ui| (p.render)(ui, d, &f));
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
    ui.label(egui::RichText::new("Spawn at scene center").strong().color(TEAL));
    let (sx, sz) = spawn_point(d);
    for cat in d.reg.spawner_categories() {
        if cat != d.reg.spawner_items()[0].category {
            ui.label(egui::RichText::new(cat).strong().color(TEAL));
        }
        let items: Vec<_> = d.reg.spawner_items().into_iter().filter(|i| i.category == cat).cloned().collect();
        ui.horizontal_wrapped(|ui| {
            for it in items {
                if chip(ui, false, &format!("+ {}", it.label)).clicked() {
                    (it.spawn)(&mut d.queue, sx, sz);
                }
            }
        });
    }
    ui.horizontal(|ui| {
        let mut name = d.studio.glb_name.clone();
        ui.add(egui::TextEdit::singleline(&mut name).hint_text("model.glb in assets/models/").desired_width(190.0));
        if name != d.studio.glb_name {
            d.studio.glb_name = name.clone();
        }
        if chip(ui, false, "Import .glb").clicked() && !name.trim().is_empty() {
            let mut file = name.trim().to_string();
            if !file.to_lowercase().ends_with(".glb") && !file.to_lowercase().ends_with(".gltf") {
                file.push_str(".glb");
            }
            d.queue.push(Cmd::SpawnObject { kind: format!("model:{file}"), x: sx, z: sz, yaw: 0.0 });
        }
    });
    let avail: Vec<String> = std::fs::read_dir(assets_dir().join("models")).map(|rd| rd.flatten().filter_map(|e| e.file_name().into_string().ok()).filter(|n| n.to_lowercase().ends_with(".glb") || n.to_lowercase().ends_with(".gltf")).collect()).unwrap_or_default();
    if avail.is_empty() {
        ui.weak("assets/models/ is empty - drop .glb files there");
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
        ui.label(egui::RichText::new("Scene objects").strong().color(TEAL));
        ui.checkbox(&mut hs, "hide grass/reeds");
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
    ui.weak(format!("{} shown", rows.len()));
    let row_h = ui.spacing().interact_size.y + ui.spacing().item_spacing.y;
    egui::ScrollArea::vertical().id_salt("object_list").max_height(row_h * 7.0).auto_shrink([false, true]).show_rows(ui, row_h, rows.len(), |ui, range| {
        for i in range {
            let (e, id, name, user) = &rows[i];
            let label = format!("#{id}  {name}{}", if *user { "  (user)" } else { "" });
            if ui.selectable_label(d.sel.selected.contains(e), label).clicked() {
                d.sel.set_one(*e);
            }
        }
    });
    ui.separator();

    // details of the primary selection
    let Some(e) = d.sel.primary else {
        ui.weak("Click an object in the viewport or in the list.");
        return;
    };
    let Ok((_, mut tf, so, _)) = d.objs.get_mut(e) else { return };
    ui.label(egui::RichText::new(format!("{}  #{}", so.name, so.id)).strong().size(14.0));
    let so_id = so.id;
    let before_tf = *tf;
    let mut pos = tf.translation.to_array();
    let (ry, rx, rz) = tf.rotation.to_euler(EulerRot::YXZ);
    let mut rot = [rx.to_degrees(), ry.to_degrees(), rz.to_degrees()];
    let mut scl = tf.scale.to_array();
    let mut ev = Ev::default();
    let mut vec3 = |ui: &mut egui::Ui, title: &str, v: &mut [f32; 3], speed: f64, suffix: &str, min: f32| {
        ui.horizontal(|ui| {
            ui.add_sized([62.0, 18.0], egui::Label::new(title));
            for (x, n) in v.iter_mut().zip(["X", "Y", "Z"]) {
                ev.note(&ui.add(egui::DragValue::new(x).speed(speed).prefix(format!("{n} ")).suffix(suffix).range(min..=f32::MAX).max_decimals(2)));
            }
        });
    };
    vec3(ui, "Position", &mut pos, 0.1, "", f32::MIN);
    vec3(ui, "Rotation", &mut rot, 1.0, "°", f32::MIN);
    vec3(ui, "Scale", &mut scl, 0.02, "", 0.05);
    let ground_y = d.grid.0.sample_height(pos[0], pos[2]);
    let drop = ui.add(egui::Button::new("Drop to Ground")).clicked();
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
        if let Ok((mut pl, nl)) = d.lights.get_mut(le) {
            ui.separator();
            ui.label(egui::RichText::new("PointLight").strong().color(TEAL));
            let c = pl.color.to_srgba();
            let mut rgb = [c.red, c.green, c.blue];
            ui.horizontal(|ui| {
                if egui::color_picker::color_edit_button_rgb(ui, &mut rgb).changed() {
                    pl.color = Color::srgb(rgb[0], rgb[1], rgb[2]);
                }
                ui.label("color");
            });
            let mut range = pl.range;
            if ui.add(egui::Slider::new(&mut range, 1.0..=120.0).text("radius (range)")).changed() {
                pl.range = range;
            }
            let mut src = pl.radius;
            if ui.add(egui::Slider::new(&mut src, 0.0..=2.0).text("source radius")).changed() {
                pl.radius = src;
            }
            match nl {
                Some(mut nl) => {
                    let mut base = nl.base;
                    if ui.add(egui::Slider::new(&mut base, 0.0..=400_000.0).logarithmic(true).text("intensity (night)")).changed() {
                        nl.base = base;
                    }
                }
                None => {
                    let mut i = pl.intensity;
                    if ui.add(egui::Slider::new(&mut i, 0.0..=400_000.0).logarithmic(true).text("intensity")).changed() {
                        pl.intensity = i;
                    }
                }
            }
        }
    }
}

#[allow(dead_code)]
fn _keep(_: &StudioState) {}
