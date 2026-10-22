//! Right inspector (~360 px): live search + tabs Global Settings / Objects & Spawner / Terrain Brushes.

use std::ops::RangeInclusive;

use bevy::prelude::*;
use bevy_egui::egui;

use crate::arc_core::*;
use crate::gizmo_3d::ObjState;
use crate::mcp_commands::StudioCommand as Cmd;
use crate::props::assets_dir;
use crate::studio::{InspectorTab, StudioState};
use crate::terrain_brush::BrushKind;
use crate::ui::*;

/// Pending (not yet released) edit of geometry-affecting WorldSpec fields — terrain regenerates once, on release.
#[derive(Resource, Default)]
pub struct InspectorDraft(pub Option<WorldSpec>);

struct Filter(String);

impl Filter {
    fn ok(&self, label: &str) -> bool {
        self.0.is_empty() || label.to_lowercase().contains(&self.0)
    }
}

fn section(ui: &mut egui::Ui, f: &Filter, title: &str, labels: &[&str], body: impl FnOnce(&mut egui::Ui)) {
    if !(f.ok(title) || labels.iter().any(|l| f.ok(l))) {
        return;
    }
    let mut h = egui::CollapsingHeader::new(egui::RichText::new(title).strong().color(TEAL)).default_open(true);
    if !f.0.is_empty() {
        h = h.open(Some(true));
    }
    h.show(ui, body);
}

fn srow<N: egui::emath::Numeric>(ui: &mut egui::Ui, f: &Filter, ev: &mut Ev, label: &str, v: &mut N, range: RangeInclusive<N>) {
    if f.ok(label) {
        ev.note(&ui.add(egui::Slider::new(v, range).text(label)));
    }
}

fn crow(ui: &mut egui::Ui, f: &Filter, ev: &mut Ev, label: &str, v: &mut bool) {
    if f.ok(label) {
        ev.note(&ui.checkbox(v, label));
    }
}

fn merge_geo(dst: &mut WorldSpec, s: &WorldSpec) {
    dst.seed = s.seed;
    dst.size = s.size;
    dst.resolution = s.resolution;
    dst.base_height = s.base_height;
    dst.mountain_height = s.mountain_height;
    dst.max_height = s.max_height;
    dst.noise_frequency = s.noise_frequency;
    dst.warp_amount = s.warp_amount;
    dst.erosion_droplets = s.erosion_droplets;
    dst.thermal_iterations = s.thermal_iterations;
    dst.lake_center = s.lake_center;
    dst.lake_radius = s.lake_radius;
    dst.lake_depth = s.lake_depth;
    dst.water_level = s.water_level;
    dst.shore_foam = s.shore_foam;
    dst.camp_center = s.camp_center;
    dst.clearing_radius = s.clearing_radius;
    dst.forest_density = s.forest_density;
    dst.boulder_density = s.boulder_density;
    dst.grass_density = s.grass_density;
}

pub fn draw(ui: &mut egui::Ui, d: &mut UiData) {
    let frame = egui::Frame::new().fill(PANEL_BG).stroke(egui::Stroke::new(1.0, BORDER)).inner_margin(egui::Margin::same(10));
    egui::Panel::right("inspector").exact_size(360.0).resizable(false).frame(frame).show(ui, |ui| {
        let mut q = d.studio.search.clone();
        ui.add(egui::TextEdit::singleline(&mut q).hint_text("Search parameters or objects...").desired_width(f32::INFINITY));
        if q != d.studio.search {
            d.studio.search = q;
        }
        let tab = d.studio.tab;
        ui.horizontal(|ui| {
            for (label, t) in [("Global Settings", InspectorTab::Global), ("Objects & Spawner", InspectorTab::Objects), ("Terrain Brushes", InspectorTab::Brushes)] {
                if chip(ui, tab == t, label).clicked() {
                    d.studio.tab = t;
                }
            }
        });
        ui.separator();
        egui::ScrollArea::vertical().id_salt("inspector_scroll").auto_shrink([false, false]).show(ui, |ui| {
            let f = Filter(d.studio.search.to_lowercase());
            match d.studio.tab {
                InspectorTab::Global => global_tab(ui, d, &f),
                InspectorTab::Objects => objects_tab(ui, d, &f),
                InspectorTab::Brushes => brushes_tab(ui, d, &f),
            }
        });
    });
}

// ───────────────────────────── Global Settings ─────────────────────────────

fn global_tab(ui: &mut egui::Ui, d: &mut UiData, f: &Filter) {
    let mut geo = d.draft.0.clone().unwrap_or_else(|| (*d.spec).clone());
    let mut live = (*d.spec).clone();
    let (mut gev, mut lev) = (Ev::default(), Ev::default());

    section(ui, f, "Terrain & Erosion", &["seed", "size", "resolution", "base height", "mountain height", "max height", "noise frequency", "domain warp", "erosion droplets", "thermal iterations"], |ui| {
        if f.ok("seed") {
            ui.horizontal(|ui| {
                ui.label("seed");
                gev.note(&ui.add(egui::DragValue::new(&mut geo.seed).speed(1.0)));
            });
        }
        srow(ui, f, &mut gev, "size", &mut geo.size, 64.0..=1024.0);
        srow(ui, f, &mut gev, "resolution", &mut geo.resolution, 65..=513);
        srow(ui, f, &mut gev, "base height", &mut geo.base_height, 0.0..=60.0);
        srow(ui, f, &mut gev, "mountain height", &mut geo.mountain_height, 0.0..=150.0);
        srow(ui, f, &mut gev, "max height", &mut geo.max_height, 20.0..=200.0);
        srow(ui, f, &mut gev, "noise frequency", &mut geo.noise_frequency, 0.5..=8.0);
        srow(ui, f, &mut gev, "domain warp", &mut geo.warp_amount, 0.0..=0.5);
        srow(ui, f, &mut gev, "erosion droplets", &mut geo.erosion_droplets, 0..=150_000);
        srow(ui, f, &mut gev, "thermal iterations", &mut geo.thermal_iterations, 0..=80);
    });
    section(ui, f, "Lake & Water", &["lake x", "lake z", "lake radius", "lake depth", "water level", "shore foam", "camp x", "camp z", "clearing radius"], |ui| {
        srow(ui, f, &mut gev, "lake x", &mut geo.lake_center[0], -120.0..=120.0);
        srow(ui, f, &mut gev, "lake z", &mut geo.lake_center[1], -120.0..=120.0);
        srow(ui, f, &mut gev, "lake radius", &mut geo.lake_radius, 5.0..=80.0);
        srow(ui, f, &mut gev, "lake depth", &mut geo.lake_depth, 0.0..=15.0);
        srow(ui, f, &mut gev, "water level", &mut geo.water_level, 0.0..=30.0);
        srow(ui, f, &mut gev, "shore foam", &mut geo.shore_foam, 0.0..=1.0);
        srow(ui, f, &mut gev, "camp x", &mut geo.camp_center[0], -120.0..=120.0);
        srow(ui, f, &mut gev, "camp z", &mut geo.camp_center[1], -120.0..=120.0);
        srow(ui, f, &mut gev, "clearing radius", &mut geo.clearing_radius, 3.0..=30.0);
    });
    section(ui, f, "Sun & Atmosphere", &["time of day", "auto sun", "sun speed", "sun intensity", "shadows", "bloom", "reflections"], |ui| {
        if f.ok("time of day") {
            let r = ui.add(egui::Slider::new(&mut live.time_of_day, 0.0..=24.0).text("time of day").custom_formatter(|v, _| hhmm(v as f32)));
            lev.note(&r);
        }
        crow(ui, f, &mut lev, "auto sun", &mut live.auto_sun);
        srow(ui, f, &mut lev, "sun speed", &mut live.sun_speed, 0.01..=2.0);
        srow(ui, f, &mut lev, "sun intensity", &mut live.sun_intensity, 0.0..=2.0);
        crow(ui, f, &mut lev, "shadows", &mut live.shadows);
        crow(ui, f, &mut lev, "bloom", &mut live.bloom);
        crow(ui, f, &mut lev, "reflections", &mut live.reflections);
    });
    section(ui, f, "Fog & God Rays", &["fog", "fog density", "fog color", "god rays"], |ui| {
        crow(ui, f, &mut lev, "fog", &mut live.fog_enabled);
        if f.ok("fog density") {
            lev.note(&ui.add(egui::Slider::new(&mut live.fog_density, 0.0..=0.03).text("fog density").fixed_decimals(4)));
        }
        if f.ok("fog color") {
            ui.horizontal(|ui| {
                let mut c = live.fog_color;
                let r = egui::color_picker::color_edit_button_rgb(ui, &mut c);
                ui.label("fog color");
                if r.changed() {
                    live.fog_color = c;
                }
                lev.note(&r);
            });
        }
        crow(ui, f, &mut lev, "god rays", &mut live.god_rays);
    });
    section(ui, f, "Biome Foliage", &["forest density", "boulder density", "grass density"], |ui| {
        srow(ui, f, &mut gev, "forest density", &mut geo.forest_density, 0.0..=1.0);
        srow(ui, f, &mut gev, "boulder density", &mut geo.boulder_density, 0.0..=1.0);
        srow(ui, f, &mut gev, "grass density", &mut geo.grass_density, 0.0..=1.0);
    });

    // live (cheap) fields: transactional preview, one undo entry per drag
    if lev.changed || lev.finish {
        let mut cur = (*d.spec).clone();
        track_ev(&lev, &mut d.wh, "World settings", &mut cur, live);
        if lev.changed {
            *d.spec = cur;
        }
    }
    // geometry fields: draft until release, then one regeneration + one undo entry
    if gev.changed {
        d.draft.0 = Some(geo);
    }
    if d.draft.0.is_some() {
        ui.colored_label(egui::Color32::from_rgb(240, 190, 80), "Terrain edit pending: release the slider to regenerate (sculpting is reset).");
    }
    if gev.finish {
        if let Some(g) = d.draft.0.take() {
            let before = (*d.spec).clone();
            let mut next = before.clone();
            merge_geo(&mut next, &g);
            next.sanitize();
            if d.wh.record("Terrain / world", before, next.clone()) {
                *d.spec = next;
            }
        }
    }
}

// ───────────────────────────── Objects & Spawner ─────────────────────────────

fn spawn_point(d: &UiData) -> (f32, f32) {
    let half = d.grid.0.size * 0.5 - 4.0;
    match d.cam.single() {
        Ok(c) => {
            let p = if d.studio.cam_style == crate::studio::CamStyle::Fly { c.eye() + c.forward() * 12.0 } else { c.pivot };
            (p.x.clamp(-half, half), p.z.clamp(-half, half))
        }
        Err(_) => (0.0, 0.0),
    }
}

fn objects_tab(ui: &mut egui::Ui, d: &mut UiData, f: &Filter) {
    ui.label(egui::RichText::new("Spawn at scene center").strong().color(TEAL));
    let (sx, sz) = spawn_point(d);
    ui.horizontal_wrapped(|ui| {
        for (label, kind) in [("+ Pine", "pine"), ("+ Boulder", "boulder"), ("+ Campfire", "campfire"), ("+ Pier", "pier"), ("+ Lantern", "lantern")] {
            if chip(ui, false, label).clicked() {
                d.queue.push(Cmd::SpawnObject { kind: kind.into(), x: sx, z: sz, yaw: 0.0 });
            }
        }
    });
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

// ───────────────────────────── Terrain Brushes ─────────────────────────────

fn brushes_tab(ui: &mut egui::Ui, d: &mut UiData, f: &Filter) {
    ui.label(egui::RichText::new("Brush").strong().color(TEAL));
    ui.horizontal_wrapped(|ui| {
        for k in BrushKind::ALL {
            if f.ok(k.label()) && chip(ui, d.brush.kind == k, k.label()).clicked() {
                d.brush.kind = k;
            }
        }
    });
    let (mut r, mut s) = (d.brush.radius, d.brush.strength);
    if f.ok("brush radius") && ui.add(egui::Slider::new(&mut r, 2.0..=45.0).text("Brush Radius (m)")).changed() {
        d.brush.radius = r;
    }
    if f.ok("strength") && ui.add(egui::Slider::new(&mut s, 0.5..=20.0).text("Strength")).changed() {
        d.brush.strength = s;
    }
    ui.separator();
    let help = match d.brush.kind {
        BrushKind::Raise => "Hold LMB on the terrain: raises ground (m/s = Strength).",
        BrushKind::Lower => "Hold LMB: digs the ground.",
        BrushKind::Smooth => "Hold LMB: relaxes bumps inside the ring.",
        BrushKind::Flatten => "Hold LMB: levels to the height where the stroke began.",
        BrushKind::PaintPines => "Hold LMB: scatters pines (one undo step per stroke).",
        BrushKind::EraseProps => "Hold LMB: removes every prop inside the ring.",
    };
    ui.label(help);
    ui.weak("RMB+WASD fly · MMB / Alt+LMB orbit · wheel zoom");
    match d.brush.hover {
        Some(h) => ui.weak(format!("cursor: x {:.1}  y {:.1}  z {:.1}", h.x, h.y, h.z)),
        None => ui.weak("cursor: -"),
    };
    ui.weak(format!("sculpt steps: {} undo / {} redo", d.th.undo_len(), d.th.redo_len()));
}

#[allow(dead_code)]
fn _keep(_: &StudioState) {}
