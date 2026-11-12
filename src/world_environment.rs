//! `WorldEnvironmentPlugin` (E3): the procedural world as an editor module. It owns terrain, water, atmosphere, prop scatter and the
//! terrain brush, and contributes its inspector panels, brush tool, spawner items and the `Empty Grid Scene` grid through `EditorRegistry`.

use std::f32::consts::FRAC_PI_2;

use bevy::gizmos::prelude::*;
use bevy::prelude::*;
use bevy_egui::egui;

use crate::arc_core::*;
use crate::camera::EditorCamera;
use crate::editor_registry::{EditorAppExt, PanelDef, PanelLocation, ToolDef, spawn_kind};
use crate::gizmo_3d::gizmo_system;
use crate::mcp_commands::StudioCommand as Cmd;
use crate::studio::{CamStyle, StudioState};
use crate::terrain_brush::{BrushKind, brush_system};
use crate::ui::*;
use crate::ui::i18n::{self, tr};
use crate::ui_inspector::{Filter, crow, section, srow};

pub const TOOL_TERRAIN_BRUSH: &str = "terrain_brush";

pub struct WorldEnvironmentPlugin;

impl Plugin for WorldEnvironmentPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((crate::terrain_render::TerrainRenderPlugin, crate::water::WaterPlugin, crate::props::PropsPlugin, crate::lighting::LightingPlugin, crate::terrain_brush::TerrainBrushPlugin));
        register_editor_items(app);
    }
}

/// Everything the world module contributes to the editor shell.
fn register_editor_items(app: &mut App) {
    {
        app.register_panel_def(PanelDef::new("global", "Global Settings", PanelLocation::RightInspector, global_tab).order(0))
            .register_panel_def(PanelDef::new("terrain_settings", "Terrain Generation", PanelLocation::RightInspector, terrain_tab).order(5))
            .register_panel_def(PanelDef::new("brushes", "Terrain Brushes", PanelLocation::RightInspector, brushes_tab).order(20))
            .register_tool(
                ToolDef::new(TOOL_TERRAIN_BRUSH, "Terrain Brush").hotkey(KeyCode::KeyB).panel("brushes").hint("Sculpt, paint pines, erase props"),
                brush_system.after(gizmo_system),
            )
            .register_spawner_item("Props", "Pine", spawn_kind("pine"))
            .register_spawner_item("Props", "Boulder", spawn_kind("boulder"))
            .register_spawner_item("Props", "Campfire", spawn_kind("campfire"))
            .register_spawner_item("Props", "Pier", spawn_kind("pier"))
            .register_spawner_item("Props", "Lantern", spawn_kind("lantern"))
            .register_spawner_item("Props", "Duck", spawn_kind("model:Duck.glb"))
            .add_systems(Update, draw_infinite_grid);
    }
}

/// `Empty Grid Scene`: major / minor grid on Y = 0 that follows the view, plus the world axes.
fn draw_infinite_grid(spec: Res<WorldSpec>, studio: Res<StudioState>, cam: Query<&EditorCamera>, mut gizmos: Gizmos) {
    if spec.terrain_enabled {
        return;
    }
    let Ok(c) = cam.single() else { return };
    let at = if studio.cam_style == CamStyle::Fly { c.eye() } else { c.pivot };
    let snap = |v: f32, step: f32| (v / step).round() * step;
    let flat = Quat::from_rotation_x(FRAC_PI_2);
    let minor = Color::srgba(0.55, 0.62, 0.68, 0.16);
    let major = Color::srgba(0.60, 0.72, 0.80, 0.34);
    gizmos.grid(Isometry3d::new(Vec3::new(snap(at.x, 1.0), 0.0, snap(at.z, 1.0)), flat), UVec2::splat(100), Vec2::ONE, minor);
    gizmos.grid(Isometry3d::new(Vec3::new(snap(at.x, 10.0), 0.0, snap(at.z, 10.0)), flat), UVec2::splat(80), Vec2::splat(10.0), major);
    let reach = 400.0;
    gizmos.line(Vec3::new(-reach, 0.01, 0.0) + Vec3::X * at.x.round(), Vec3::new(reach, 0.01, 0.0) + Vec3::X * at.x.round(), Color::srgb(0.85, 0.28, 0.28));
    gizmos.line(Vec3::new(0.0, 0.01, -reach) + Vec3::Z * at.z.round(), Vec3::new(0.0, 0.01, reach) + Vec3::Z * at.z.round(), Color::srgb(0.30, 0.45, 0.90));
}

fn merge_geo(dst: &mut WorldSpec, s: &WorldSpec) {
    dst.terrain_enabled = s.terrain_enabled;
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

// ───────────────────────────── Global Settings ─────────────────────────────

fn global_tab(ui: &mut egui::Ui, d: &mut UiData, f: &Filter) {
    let world_only = d.layout.active() == crate::ui::layout::Layout::Smart;
    environment_tab(ui, d, f, !world_only, true);
}

fn terrain_tab(ui: &mut egui::Ui, d: &mut UiData, f: &Filter) {
    environment_tab(ui, d, f, true, false);
}

fn environment_tab(ui: &mut egui::Ui, d: &mut UiData, f: &Filter, terrain: bool, world: bool) {
    let mut geo = d.draft.0.clone().unwrap_or_else(|| (*d.spec).clone());
    let mut live = (*d.spec).clone();
    let (mut gev, mut lev) = (Ev::default(), Ev::default());

    if terrain {
        section(ui, f, "Terrain & Erosion", &["empty grid scene", "seed", "size", "resolution", "base height", "mountain height", "max height", "noise frequency", "domain warp", "erosion droplets", "thermal iterations"], |ui| {
            let empty_label = tr("empty grid scene (no terrain, lake or props)", "пустая сетка (без рельефа, озера и объектов)");
            if f.ok("empty grid scene") || f.ok(empty_label) {
                let mut empty = !geo.terrain_enabled;
                gev.note(&ui.checkbox(&mut empty, empty_label));
                geo.terrain_enabled = !empty;
            }
            if f.ok("seed") || f.ok(i18n::shown("seed")) {
                ui.horizontal(|ui| {
                    ui.label(i18n::shown("seed"));
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
        section(ui, f, "Biome Foliage", &["forest density", "boulder density", "grass density"], |ui| {
            srow(ui, f, &mut gev, "forest density", &mut geo.forest_density, 0.0..=1.0);
            srow(ui, f, &mut gev, "boulder density", &mut geo.boulder_density, 0.0..=1.0);
            srow(ui, f, &mut gev, "grass density", &mut geo.grass_density, 0.0..=1.0);
        });
    }
    if world {
        section(ui, f, "Lake & Water", &["lake x", "lake z", "lake radius", "lake depth", "water level", "shore foam", "camp x", "camp z", "clearing radius"], |ui| {
            srow(ui, f, &mut gev, "lake x", &mut geo.lake_center[0], -120.0..=120.0);
            srow(ui, f, &mut gev, "lake z", &mut geo.lake_center[1], -120.0..=120.0);
            srow(ui, f, &mut gev, "lake radius", &mut geo.lake_radius, 5.0..=80.0);
            srow(ui, f, &mut gev, "lake depth", &mut geo.lake_depth, 0.0..=15.0);
            srow(ui, f, &mut gev, "water level", &mut geo.water_level, 0.0..=30.0);
            srow(ui, f, &mut gev, "shore foam", &mut geo.shore_foam, 0.0..=1.0);
            water_bodies_ui(ui, &mut d.water, &mut d.queue, geo.water_level);
            srow(ui, f, &mut gev, "camp x", &mut geo.camp_center[0], -120.0..=120.0);
            srow(ui, f, &mut gev, "camp z", &mut geo.camp_center[1], -120.0..=120.0);
            srow(ui, f, &mut gev, "clearing radius", &mut geo.clearing_radius, 3.0..=30.0);
        });
        section(ui, f, "Sun & Atmosphere", &["time of day", "auto sun", "sun speed", "sun intensity", "shadows", "bloom", "reflections"], |ui| {
            if f.ok("time of day") || f.ok(i18n::shown("time of day")) {
                let r = ui.add(egui::Slider::new(&mut live.time_of_day, 0.0..=24.0).text(i18n::shown("time of day")).custom_formatter(|v, _| hhmm(v as f32)));
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
            if f.ok("fog density") || f.ok(i18n::shown("fog density")) {
                lev.note(&ui.add(egui::Slider::new(&mut live.fog_density, 0.0..=0.03).text(i18n::shown("fog density")).fixed_decimals(4)));
            }
            if f.ok("fog color") || f.ok(i18n::shown("fog color")) {
                ui.horizontal(|ui| {
                    let mut c = live.fog_color;
                    let r = egui::color_picker::color_edit_button_rgb(ui, &mut c);
                    ui.label(i18n::shown("fog color"));
                    if r.changed() {
                        live.fog_color = c;
                    }
                    lev.note(&r);
                });
            }
            crow(ui, f, &mut lev, "god rays", &mut live.god_rays);
        });
    }


    // live (cheap) fields: transactional preview, one undo entry per drag
    if lev.changed || lev.finish {
        live.sanitize();
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
        ui.colored_label(AMBER, tr("Terrain edit pending: release the slider to regenerate (sculpting is reset).", "Правка ландшафта ожидает: отпустите ползунок для регенерации (лепка сбрасывается)."));
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

// ───────────────────────────── Terrain Brushes ─────────────────────────────

fn brushes_tab(ui: &mut egui::Ui, d: &mut UiData, f: &Filter) {
    ui.label(egui::RichText::new(i18n::shown("Brush")).strong().color(TEAL));
    ui.horizontal_wrapped(|ui| {
        for k in BrushKind::ALL {
            let name = i18n::label(k.label());
            if (f.ok(k.label()) || f.ok(name)) && chip(ui, d.brush.kind == k, name).clicked() {
                d.queue.push(Cmd::SetBrush { kind: k, radius: d.brush.radius, strength: d.brush.strength });
            }
        }
    });
    let (mut r, mut s) = (d.brush.radius, d.brush.strength);
    if f.ok("brush radius") && ui.add(egui::Slider::new(&mut r, 2.0..=45.0).text(i18n::shown("Brush Radius (m)"))).changed() {
        d.queue.push(Cmd::SetBrush { kind: d.brush.kind, radius: r, strength: d.brush.strength });
    }
    if f.ok("strength") && ui.add(egui::Slider::new(&mut s, 0.5..=20.0).text(i18n::shown("Strength"))).changed() {
        d.queue.push(Cmd::SetBrush { kind: d.brush.kind, radius: d.brush.radius, strength: s });
    }
    ui.separator();
    let help = match d.brush.kind {
        BrushKind::Raise => tr("Hold LMB on the terrain: raises ground (m/s = Strength).", "Удерживайте ЛКМ на рельефе: подъём грунта (м/с = Сила)."),
        BrushKind::Lower => tr("Hold LMB: digs the ground.", "Удерживайте ЛКМ: копает грунт."),
        BrushKind::Smooth => tr("Hold LMB: relaxes bumps inside the ring.", "Удерживайте ЛКМ: сглаживает неровности внутри кольца."),
        BrushKind::Flatten => tr("Hold LMB: levels to the height where the stroke began.", "Удерживайте ЛКМ: выравнивает по высоте начала штриха."),
        BrushKind::PaintPines => tr("Hold LMB: scatters pines (one undo step per stroke).", "Удерживайте ЛКМ: рассаживает сосны (один шаг отмены на штрих)."),
        BrushKind::EraseProps => tr("Hold LMB: removes every prop inside the ring.", "Удерживайте ЛКМ: удаляет все объекты внутри кольца."),
    };
    ui.label(help);
    ui.weak(tr("RMB+WASD fly · MMB / Alt+LMB orbit · wheel zoom", "ПКМ+WASD полёт · СКМ / Alt+ЛКМ орбита · колесо зум"));
    match d.brush.hover {
        Some(h) => ui.weak(format!("{}: x {:.1}  y {:.1}  z {:.1}", tr("cursor", "курсор"), h.x, h.y, h.z)),
        None => ui.weak(format!("{}: -", tr("cursor", "курсор"))),
    };
    ui.weak(format!("{}: {} {} / {} {}", tr("sculpt steps", "шаги лепки"), d.th.undo_len(), tr("undo", "отмена"), d.th.redo_len(), tr("redo", "повтор")));
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor_registry::EditorRegistry;

    #[test]
    fn world_module_registers_panels_tool_and_spawners() {
        let mut app = App::new();
        app.init_resource::<crate::studio::StudioState>();
        register_editor_items(&mut app);
        let r = app.world().resource::<EditorRegistry>();
        let ids: Vec<_> = r.panels(PanelLocation::RightInspector).iter().map(|p| p.id).collect();
        assert_eq!(ids, ["global", "terrain_settings", "brushes"]);
        assert_eq!(r.tool_for_panel("brushes").unwrap().id, TOOL_TERRAIN_BRUSH);
        let labels: Vec<_> = r.spawner_items().iter().map(|i| i.label).collect();
        for want in ["Pine", "Boulder", "Campfire", "Pier", "Duck"] {
            assert!(labels.contains(&want), "missing spawner {want}");
        }
    }

    #[test]
    fn empty_grid_scene_is_flat_without_scatter() {
        let spec = WorldSpec { resolution: 65, erosion_droplets: 0, thermal_iterations: 0, terrain_enabled: false, ..WorldSpec::default() };
        let g = crate::terrain_render::generate_world(&spec);
        assert!(g.heights.iter().all(|h| *h == 0.0));
        assert!(generate_scatter_plan(&spec, &g).is_empty());
        let on = WorldSpec { terrain_enabled: true, ..spec };
        assert!(!generate_scatter_plan(&on, &crate::terrain_render::generate_world(&on)).is_empty());
    }
}

/// Per-body water levels: pick a basin on the terrain and set its level without touching the global one.
fn water_bodies_ui(ui: &mut egui::Ui, w: &mut crate::water::WaterBodies, queue: &mut crate::mcp_commands::StudioCommandQueue, global_level: f32) {
    ui.separator();
    ui.label(egui::RichText::new(tr("Water bodies (own level, global level untouched)", "Водоёмы (свой уровень, общий не меняется)")).strong().color(TEAL));
    ui.horizontal(|ui| {
        if chip(ui, w.picking, if w.picking { tr("Click terrain... (Esc cancels)", "Кликните рельеф... (Esc — отмена)") } else { tr("Pick water body", "Выбрать водоём") }).clicked() {
            w.picking = !w.picking;
        }
        if ui.add_enabled(w.selected.is_some(), egui::Button::new(tr("Reset to global", "Сбросить к общему"))).clicked() {
            if let Some(b) = w.selected.and_then(|i| w.items.get(i)) {
                queue.push(crate::mcp_commands::StudioCommand::SetWaterBody { x: b.seed[0], z: b.seed[1], level: None });
            }
        }
    });
    let mut pick = None;
    for (i, b) in w.items.iter().enumerate() {
        if ui.selectable_label(w.selected == Some(i), format!("{} {}  ({:.0}, {:.0})  {} {:.1}", tr("Body", "Водоём"), i + 1, b.seed[0], b.seed[1], tr("level", "уровень"), b.level)).clicked() {
            pick = Some(i);
        }
    }
    if pick.is_some() {
        w.selected = pick;
    }
    if let Some(i) = w.selected.filter(|&i| i < w.items.len()) {
        let (seed, mut lvl) = (w.items[i].seed, w.items[i].level);
        if ui.add(egui::Slider::new(&mut lvl, 0.0..=40.0).text(tr("body level", "уровень водоёма"))).changed() {
            queue.push(crate::mcp_commands::StudioCommand::SetWaterBody { x: seed[0], z: seed[1], level: Some(lvl) });
        }
        ui.weak(format!("{} {global_level:.1} {}", tr("global level", "общий уровень"), tr("stays as is", "не меняется")));
    }
}
