//! Bottom `Asset Browser` (E5): `.glb` models, prefabs, scenes and the procedural / level kit as clickable cards.
//! Cards only push `StudioCommand`s (`SpawnObject`, `SpawnPrefab`, `SaveSelectionAsPrefab`, `LoadScene`, `BuildRoom`), so
//! everything here is undoable and scriptable like the rest. With "Click to place" a card arms a placement cursor instead.

use std::f32::consts::FRAC_PI_2;
use std::path::Path;
use std::time::{Duration, Instant};

use bevy::gizmos::prelude::*;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{EguiContexts, egui};

use crate::arc_core::WorldPreset;
use crate::level_builder::{GridSettings, WallStyle, demo_tavern_commands};
use crate::mcp_commands::{StudioCommand as Cmd, StudioCommandQueue};
use crate::props::assets_dir;
use crate::terrain_render::TerrainGrid;
use crate::ui::*;
use crate::ui_inspector::spawn_point;

const RESCAN: Duration = Duration::from_millis(1500);
const CARD: egui::Vec2 = egui::vec2(132.0, 54.0);

// ───────────────────────────── Scanning ─────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub struct AssetEntry {
    /// Path relative to the scanned base (`assets/` for models and prefabs, the project folder for scenes).
    pub rel: String,
    /// File name without extension.
    pub name: String,
    /// Containing folder (`castle`, `models`, ...).
    pub group: String,
}

#[derive(Default, Clone, Debug)]
pub struct AssetIndex {
    pub models: Vec<AssetEntry>,
    pub prefabs: Vec<AssetEntry>,
    pub scenes: Vec<AssetEntry>,
}

fn walk(dir: &Path, base: &Path, exts: &[&str], out: &mut Vec<AssetEntry>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, base, exts, out);
            continue;
        }
        let file = e.file_name().to_string_lossy().to_string();
        let lower = file.to_ascii_lowercase();
        let Some(ext) = exts.iter().find(|x| lower.ends_with(**x)) else { continue };
        let rel = p.strip_prefix(base).unwrap_or(&p).to_string_lossy().replace('\\', "/");
        let group = p.parent().and_then(|d| d.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        out.push(AssetEntry { rel, name: file[..file.len() - ext.len()].to_string(), group });
    }
}

/// Scan `assets/models`, `assets/showcase` (.glb / .gltf), `assets/prefabs` (.prefab.json) and `scenes/`, `assets/scenes/` (.scn.json).
pub fn scan_assets(assets: &Path, project: &Path) -> AssetIndex {
    let mut ix = AssetIndex::default();
    for d in ["models", "showcase"] {
        walk(&assets.join(d), assets, &[".glb", ".gltf"], &mut ix.models);
    }
    walk(&assets.join("prefabs"), assets, &[crate::prefabs::PREFAB_EXT], &mut ix.prefabs);
    walk(&project.join("scenes"), project, &[".scn.json"], &mut ix.scenes);
    walk(&assets.join("scenes"), project, &[".scn.json"], &mut ix.scenes);
    for list in [&mut ix.models, &mut ix.prefabs, &mut ix.scenes] {
        list.sort_by(|a, b| (&a.group, &a.name).cmp(&(&b.group, &b.name)));
        list.dedup_by(|a, b| a.rel == b.rel);
    }
    ix
}

/// `SpawnObject` kind of a model found under `assets/`: `models/Duck.glb` → `model:Duck.glb`, anything else keeps its folder.
pub fn model_kind(rel: &str) -> String {
    match rel.strip_prefix("models/") {
        Some(f) if !f.contains('/') => format!("model:{f}"),
        _ => format!("model:{rel}"),
    }
}

// ───────────────────────────── State ─────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum Tab {
    #[default]
    All,
    Models,
    Prefabs,
    Scenes,
    Kit,
}

/// What a click in the viewport places while "Click to place" is armed.
#[derive(Clone, Debug, PartialEq)]
pub enum Placing {
    Model(String),
    Prefab(String),
}

#[derive(Resource)]
pub struct AssetBrowserState {
    pub open: bool,
    pub click_place: bool,
    pub placing: Option<Placing>,
    tab: Tab,
    search: String,
    index: AssetIndex,
    scanned: Option<Instant>,
    prefab_name: String,
    scene_path: String,
}

impl Default for AssetBrowserState {
    fn default() -> Self {
        Self {
            open: true,
            click_place: false,
            placing: None,
            tab: Tab::All,
            search: String::new(),
            index: AssetIndex::default(),
            scanned: None,
            prefab_name: "tavern_corner".into(),
            scene_path: "scenes/arc_scene.scn.json".into(),
        }
    }
}

pub struct AssetBrowserPlugin;

impl Plugin for AssetBrowserPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AssetBrowserState>().add_systems(Update, placement_system);
    }
}

// ───────────────────────────── UI ─────────────────────────────

fn card(ui: &mut egui::Ui, icon: &str, tint: egui::Color32, title: &str, sub: &str, active: bool) -> egui::Response {
    let (rect, r) = ui.allocate_exact_size(CARD, egui::Sense::click());
    let fill = if active { egui::Color32::from_rgb(20, 70, 66) } else if r.hovered() { egui::Color32::from_gray(48) } else { egui::Color32::from_gray(30) };
    let p = ui.painter();
    p.rect_filled(rect, 5.0, fill);
    p.rect_stroke(rect, 5.0, egui::Stroke::new(1.0, if active { TEAL } else { egui::Color32::from_gray(58) }), egui::StrokeKind::Inside);
    let badge = egui::Rect::from_min_size(rect.min + egui::vec2(6.0, 7.0), egui::vec2(34.0, 40.0));
    p.rect_filled(badge, 4.0, tint.gamma_multiply(0.30));
    p.text(badge.center(), egui::Align2::CENTER_CENTER, icon, egui::FontId::proportional(13.0), tint);
    let wrap = rect.width() - 52.0;
    let line = |text: &str, size: f32, color: egui::Color32| {
        let mut job = egui::text::LayoutJob::single_section(text.to_string(), egui::TextFormat { font_id: egui::FontId::proportional(size), color, ..Default::default() });
        job.wrap = egui::text::TextWrapping::truncate_at_width(wrap);
        ui.painter().layout_job(job)
    };
    let (g1, g2) = (line(title, 12.0, egui::Color32::from_gray(228)), line(sub, 10.0, egui::Color32::from_gray(135)));
    let x = rect.min.x + 46.0;
    ui.painter().galley(egui::pos2(x, rect.min.y + 11.0), g1, egui::Color32::WHITE);
    ui.painter().galley(egui::pos2(x, rect.min.y + 29.0), g2, egui::Color32::WHITE);
    r.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn model_cards(ui: &mut egui::Ui, d: &mut UiData, list: &[AssetEntry], sx: f32, sz: f32) {
    for e in list {
        let kind = model_kind(&e.rel);
        let armed = d.assets.placing == Some(Placing::Model(kind.clone()));
        if card(ui, "GLB", egui::Color32::from_rgb(120, 170, 255), &e.name, &e.group, armed).on_hover_text(&e.rel).clicked() {
            if d.assets.click_place {
                d.assets.placing = Some(Placing::Model(kind));
            } else {
                d.queue.push(Cmd::SpawnObject { kind, x: sx, z: sz, yaw: 0.0 });
            }
        }
    }
}

fn prefab_cards(ui: &mut egui::Ui, d: &mut UiData, list: &[AssetEntry], center: Vec3) {
    for e in list {
        let armed = d.assets.placing == Some(Placing::Prefab(e.rel.clone()));
        if card(ui, "PFB", egui::Color32::from_rgb(255, 190, 90), &e.name, "prefab - click to spawn", armed).on_hover_text(&e.rel).clicked() {
            if d.assets.click_place {
                d.assets.placing = Some(Placing::Prefab(e.rel.clone()));
            } else {
                d.queue.push(Cmd::SpawnPrefab { path: e.rel.clone(), position: center, rotation_y_deg: 0.0 });
            }
        }
    }
}

pub fn draw(ui: &mut egui::Ui, d: &mut UiData) {
    if !d.assets.open {
        return;
    }
    if d.assets.scanned.is_none_or(|t| t.elapsed() > RESCAN) {
        let assets = assets_dir();
        let project = assets.parent().map(Path::to_path_buf).unwrap_or_default();
        d.assets.index = scan_assets(&assets, &project);
        d.assets.scanned = Some(Instant::now());
    }
    let frame = egui::Frame::new().fill(PANEL_BG).stroke(egui::Stroke::new(1.0, BORDER)).inner_margin(egui::Margin::same(8));
    let (sx, sz) = spawn_point(d);
    let center = Vec3::new(sx, 0.0, sz);
    egui::Panel::bottom("asset_browser").exact_size(176.0).resizable(false).frame(frame).show(ui, |ui| {
        let counts = [d.assets.index.models.len(), d.assets.index.prefabs.len(), d.assets.index.scenes.len()];
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new("ASSET BROWSER").strong().size(12.0).color(TEAL));
            if ui.small_button("Hide").on_hover_text("Hide (Assets button in the toolbar)").clicked() {
                d.assets.open = false;
            }
            vsep(ui);
            for (tab, label) in [
                (Tab::All, "All".to_string()),
                (Tab::Models, format!("Models (.glb / .gltf) {}", counts[0])),
                (Tab::Prefabs, format!("Prefabs (.prefab.json) {}", counts[1])),
                (Tab::Scenes, format!("Scenes (.scn.json) {}", counts[2])),
                (Tab::Kit, "Procedural & Level Kit".to_string()),
            ] {
                if chip(ui, d.assets.tab == tab, &label).clicked() {
                    d.assets.tab = tab;
                }
            }
            ui.add(egui::TextEdit::singleline(&mut d.assets.search).hint_text("Search file name...").desired_width(120.0));
            if chip(ui, d.assets.click_place, "Click to place").on_hover_text("Armed: a card sticks to the cursor; click terrain / grid to place, Esc or right click to stop").clicked() {
                d.assets.click_place = !d.assets.click_place;
                d.assets.placing = None;
            }
            if let Some(p) = &d.assets.placing {
                ui.label(egui::RichText::new(format!("placing {}", match p { Placing::Model(k) | Placing::Prefab(k) => k })).size(11.0).color(TEAL));
            }
        });
        ui.separator();
        let filter = d.assets.search.to_lowercase();
        let hit = |e: &AssetEntry| filter.is_empty() || e.name.to_lowercase().contains(&filter) || e.rel.to_lowercase().contains(&filter);
        match d.assets.tab {
            Tab::All => {
                let (prefabs, models): (Vec<_>, Vec<_>) = (d.assets.index.prefabs.iter().filter(|e| hit(e)).cloned().collect(), d.assets.index.models.iter().filter(|e| hit(e)).cloned().collect());
                egui::ScrollArea::vertical().id_salt("assets_all").auto_shrink([false, false]).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        prefab_cards(ui, d, &prefabs, center);
                        model_cards(ui, d, &models, sx, sz);
                    });
                });
            }
            Tab::Models => {
                let list: Vec<_> = d.assets.index.models.iter().filter(|e| hit(e)).cloned().collect();
                egui::ScrollArea::vertical().id_salt("assets_models").auto_shrink([false, false]).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| model_cards(ui, d, &list, sx, sz));
                    if list.is_empty() {
                        ui.weak("No models match - drop .glb files into assets/models/ or assets/showcase/");
                    }
                });
            }
            Tab::Prefabs => {
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut d.assets.prefab_name).hint_text("prefab name").desired_width(150.0));
                    let n = d.sel.selected.len();
                    ui.add_enabled_ui(n > 0 && !d.assets.prefab_name.trim().is_empty(), |ui| {
                        if chip(ui, false, "[+ Save Selected as Prefab]").on_hover_text("Saves the selection to assets/prefabs/<name>.prefab.json").clicked() {
                            d.queue.push(Cmd::SaveSelectionAsPrefab { path: d.assets.prefab_name.trim().to_string() });
                            d.assets.scanned = None;
                        }
                    });
                    ui.weak(format!("{n} selected"));
                });
                let list: Vec<_> = d.assets.index.prefabs.iter().filter(|e| hit(e)).cloned().collect();
                egui::ScrollArea::vertical().id_salt("assets_prefabs").auto_shrink([false, false]).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| prefab_cards(ui, d, &list, center));
                    if list.is_empty() {
                        ui.weak("No prefabs yet - select objects and press [+ Save Selected as Prefab]");
                    }
                });
            }
            Tab::Scenes => {
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut d.assets.scene_path).desired_width(240.0));
                    if chip(ui, false, "[Save Current]").clicked() {
                        d.queue.push(Cmd::SaveScene { path: d.assets.scene_path.trim().to_string() });
                        d.assets.scanned = None;
                    }
                });
                let list: Vec<_> = d.assets.index.scenes.iter().filter(|e| hit(e)).cloned().collect();
                egui::ScrollArea::vertical().id_salt("assets_scenes").auto_shrink([false, false]).show(ui, |ui| {
                    for e in &list {
                        ui.horizontal(|ui| {
                            if chip(ui, false, "[Load]").clicked() {
                                d.queue.push(Cmd::LoadScene { path: e.rel.clone() });
                            }
                            if ui.small_button("use name").clicked() {
                                d.assets.scene_path = e.rel.clone();
                            }
                            ui.label(egui::RichText::new(&e.rel).color(egui::Color32::from_gray(210)));
                        });
                    }
                    if list.is_empty() {
                        ui.weak("No scenes - Save Current writes a .scn.json into scenes/");
                    }
                });
            }
            Tab::Kit => {
                let items: Vec<_> = d.reg.spawner_items().iter().filter(|i| filter.is_empty() || i.label.to_lowercase().contains(&filter)).cloned().collect();
                let level = d.gset.floor_level;
                let cell = IVec2::new((sx / d.gset.cell_size).round() as i32, (sz / d.gset.cell_size).round() as i32);
                egui::ScrollArea::vertical().id_salt("assets_kit").auto_shrink([false, false]).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for it in &items {
                            if card(ui, "OBJ", egui::Color32::from_rgb(120, 220, 150), it.label, it.category, false).clicked() {
                                (it.spawn)(&mut d.queue, sx, sz);
                            }
                        }
                        for style in WallStyle::ALL {
                            if card(ui, "ROOM", egui::Color32::from_rgb(230, 130, 170), style.label(), "5 x 4 room at view center", false).clicked() {
                                let (min, max) = ([cell.x - 2, cell.y - 2], [cell.x + 2, cell.y + 1]);
                                d.queue.push(Cmd::SetGrid { enabled: Some(true), cell_size: None, floor_level: None, floor_height: None, angle_snap_deg: None });
                                d.queue.push(Cmd::BuildRoom { min, max, floor: level, style });
                            }
                        }
                        if card(ui, "DEMO", egui::Color32::from_rgb(255, 150, 90), "Demo Tavern", "two rooms, door, stairs", false).clicked() {
                            for c in demo_tavern_commands(WallStyle::WoodTavern) {
                                d.queue.push(c);
                            }
                        }
                        if card(ui, "SKY", egui::Color32::from_rgb(160, 160, 255), "Misty Dusk", "world preset", false).clicked() {
                            d.queue.push(Cmd::ApplyPreset(WorldPreset::MistyDuskLake));
                        }
                    });
                });
            }
        }
    });
}

// ───────────────────────────── Click-to-place ─────────────────────────────

#[allow(clippy::too_many_arguments)]
fn placement_system(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut egui: EguiContexts,
    cam: Single<(&Camera, &GlobalTransform)>,
    terrain: Res<TerrainGrid>,
    g: Res<GridSettings>,
    mut st: ResMut<AssetBrowserState>,
    mut queue: ResMut<StudioCommandQueue>,
    mut gizmos: Gizmos,
) {
    let Some(placing) = st.placing.clone() else { return };
    if keys.just_pressed(KeyCode::Escape) || mouse.just_pressed(MouseButton::Right) {
        st.placing = None;
        return;
    }
    let over_ui = egui.ctx_mut().map(|c| c.is_pointer_over_egui() || c.egui_wants_pointer_input()).unwrap_or(false);
    let (camera, gtf) = *cam;
    let ray = window.cursor_position().and_then(|c| camera.viewport_to_world(gtf, c).ok());
    let (Some(r), false) = (ray, over_ui) else { return };
    let Some(mut p) = terrain.0.raycast(r.origin, *r.direction) else { return };
    if g.enabled {
        p = Vec3::new(g.snap(p.x), p.y, g.snap(p.z));
    }
    let ring = Isometry3d::new(p + Vec3::Y * 0.08, Quat::from_rotation_x(FRAC_PI_2));
    gizmos.circle(ring, 1.1, Color::srgb(0.15, 0.9, 0.8));
    gizmos.circle(ring, 0.25, Color::srgb(0.15, 0.9, 0.8));
    if mouse.just_pressed(MouseButton::Left) {
        queue.push(match placing {
            Placing::Model(kind) => Cmd::SpawnObject { kind, x: p.x, z: p.z, yaw: 0.0 },
            Placing::Prefab(path) => Cmd::SpawnPrefab { path, position: p, rotation_y_deg: 0.0 },
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_models_prefabs_and_scenes() {
        let root = std::env::temp_dir().join(format!("arc_assets_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let a = root.join("assets");
        for f in ["models/Duck.glb", "showcase/castle/gate.glb", "showcase/nature/tree.GLTF", "showcase/readme.txt", "prefabs/tavern_corner.prefab.json", "prefabs/x.json", "assets_scenes/none.scn.json", "scenes/a.scn.json", "assets/scenes/b.scn.json"] {
            let p = if f.starts_with("assets/") { root.join(f) } else if f.starts_with("scenes") || f.starts_with("assets_") { root.join(f) } else { a.join(f) };
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, b"x").unwrap();
        }
        let ix = scan_assets(&a, &root);
        let models: Vec<_> = ix.models.iter().map(|e| e.rel.as_str()).collect();
        assert_eq!(models, ["showcase/castle/gate.glb", "models/Duck.glb", "showcase/nature/tree.GLTF"]);
        assert_eq!(ix.models[0].group, "castle");
        assert_eq!(ix.models[0].name, "gate");
        assert_eq!(ix.prefabs.len(), 1);
        assert_eq!(ix.prefabs[0].name, "tavern_corner");
        let scenes: Vec<_> = ix.scenes.iter().map(|e| e.rel.as_str()).collect();
        assert_eq!(scenes, ["scenes/a.scn.json", "assets/scenes/b.scn.json"]);
        assert_eq!(model_kind("models/Duck.glb"), "model:Duck.glb");
        assert_eq!(model_kind("showcase/castle/gate.glb"), "model:showcase/castle/gate.glb");
        let _ = std::fs::remove_dir_all(&root);
    }
}
