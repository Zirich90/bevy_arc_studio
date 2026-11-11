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
use crate::asset_meta::{self, AssetMeta};
use crate::level_builder::{GridSettings, WallStyle, demo_tavern_commands};
use crate::mcp_commands::{StudioCommand as Cmd, StudioCommandQueue};
use crate::props::assets_dir;
use crate::terrain_render::TerrainGrid;
use crate::thumbnails::Thumbnails;
use crate::ui::*;
use crate::ui::i18n::tr;
use crate::ui::icons::{self, Icon};
use crate::ui_inspector::spawn_point;

const RESCAN: Duration = Duration::from_millis(1500);
const CARD: egui::Vec2 = egui::vec2(150.0, 66.0);
const BADGE: f32 = 54.0;

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
        app.init_resource::<AssetBrowserState>()
            .init_resource::<AssetLoadStats>()
            .register_type::<AssetLoadStats>()
            .add_systems(Update, (placement_system, count_asset_loads));
    }
}

/// Loader-event counters; switching layouts must leave these unchanged (no asset reload).
#[derive(Resource, Reflect, Default)]
#[reflect(Resource)]
pub struct AssetLoadStats {
    pub added: u32,
    pub modified: u32,
    pub loaded: u32,
}

fn count_asset_loads(
    mut stats: ResMut<AssetLoadStats>,
    mut mesh: MessageReader<AssetEvent<bevy::render::mesh::Mesh>>,
    mut image: MessageReader<AssetEvent<bevy::image::Image>>,
    mut material: MessageReader<AssetEvent<bevy::pbr::StandardMaterial>>,
    mut gltf: MessageReader<AssetEvent<bevy::gltf::Gltf>>,
) {
    let stats = &mut *stats;
    tally(stats, mesh.read());
    tally(stats, image.read());
    tally(stats, material.read());
    tally(stats, gltf.read());
}

fn tally<'a, A: bevy::asset::Asset>(stats: &mut AssetLoadStats, events: impl Iterator<Item = &'a AssetEvent<A>>) {
    for ev in events {
        match ev {
            AssetEvent::Added { .. } => stats.added += 1,
            AssetEvent::Modified { .. } => stats.modified += 1,
            AssetEvent::LoadedWithDependencies { .. } => stats.loaded += 1,
            _ => {}
        }
    }
}

// ───────────────────────────── UI ─────────────────────────────

fn card(ui: &mut egui::Ui, icon: &str, tint: egui::Color32, title: &str, sub: &str, active: bool, thumb: Option<(&mut Thumbnails, &str)>) -> egui::Response {
    let (rect, r) = ui.allocate_exact_size(CARD, egui::Sense::click());
    let fill = if active { egui::Color32::from_rgb(18, 62, 58) } else if r.hovered() { egui::Color32::from_rgb(42, 50, 54) } else { CHROME_BG };
    let tex = thumb.and_then(|(t, key)| if ui.is_rect_visible(rect) { t.request(key) } else { None });
    let p = ui.painter();
    p.rect_filled(rect, 5.0, fill);
    p.rect_stroke(rect, 5.0, egui::Stroke::new(1.0, if active { TEAL } else { BORDER }), egui::StrokeKind::Inside);
    let badge = egui::Rect::from_min_size(rect.min + egui::vec2(6.0, 6.0), egui::vec2(BADGE, BADGE));
    if let Some(id) = tex {
        p.image(id, badge, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
    } else {
        p.rect_filled(badge, 4.0, tint.gamma_multiply(0.30));
        p.text(badge.center(), egui::Align2::CENTER_CENTER, icon, egui::FontId::proportional(13.0), tint);
    }
    let wrap = rect.width() - BADGE - 20.0;
    let line = |text: &str, size: f32, color: egui::Color32| {
        let mut job = egui::text::LayoutJob::single_section(text.to_string(), egui::TextFormat { font_id: egui::FontId::proportional(size), color, ..Default::default() });
        job.wrap = egui::text::TextWrapping::truncate_at_width(wrap);
        ui.painter().layout_job(job)
    };
    let (g1, g2) = (line(title, 12.0, TEXT), line(sub, 10.0, MUTED));
    let x = rect.min.x + BADGE + 14.0;
    ui.painter().galley(egui::pos2(x, rect.min.y + 16.0), g1, egui::Color32::WHITE);
    ui.painter().galley(egui::pos2(x, rect.min.y + 36.0), g2, egui::Color32::WHITE);
    r.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Right-click menu of a model card: import settings stored in `<model>.arcmeta.json` and applied on spawn.
fn import_menu(ui: &mut egui::Ui, rel: &str) {
    let id = egui::Id::new(("arcmeta", rel));
    let mut m: AssetMeta = ui.data_mut(|d| d.get_temp(id)).unwrap_or_else(|| asset_meta::load(rel));
    ui.label(egui::RichText::new(format!("{} - {rel}", tr("Import settings", "Настройки импорта"))).size(11.0).color(TEAL));
    ui.horizontal(|ui| {
        ui.label(tr("Scale", "Масштаб"));
        ui.add(egui::DragValue::new(&mut m.scale).speed(0.01).range(0.001..=1000.0));
        ui.label(tr("Yaw", "Поворот"));
        ui.add(egui::DragValue::new(&mut m.yaw_deg).speed(1.0).suffix(" deg"));
        ui.label(tr("Y offset", "Смещение Y"));
        ui.add(egui::DragValue::new(&mut m.y_offset).speed(0.01));
    });
    let saved = asset_meta::load(rel);
    ui.horizontal(|ui| {
        if ui.add_enabled(m != saved, egui::Button::new(tr("Save", "Сохранить"))).on_hover_text(tr("Writes the .arcmeta.json sidecar; new spawns of this model use it", "Записывает .arcmeta.json; новые спавны модели используют его")).clicked() {
            let _ = asset_meta::save(rel, &m);
        }
        if ui.button(tr("Reset", "Сбросить")).clicked() {
            m = AssetMeta::default();
            let _ = asset_meta::save(rel, &m);
        }
    });
    ui.data_mut(|d| d.insert_temp(id, m));
}

fn model_cards(ui: &mut egui::Ui, d: &mut UiData, list: &[AssetEntry], sx: f32, sz: f32) {
    for e in list {
        let kind = model_kind(&e.rel);
        let armed = d.assets.placing == Some(Placing::Model(kind.clone()));
        let sub = if asset_meta::load(&e.rel).is_default() { e.group.clone() } else { format!("{} - {}", e.group, tr("custom import", "свой импорт")) };
        let resp = card(ui, "GLB", egui::Color32::from_rgb(120, 170, 255), &e.name, &sub, armed, Some((&mut d.thumbs, &e.rel))).on_hover_text(format!("{}\n{}", e.name, e.rel));
        resp.context_menu(|ui| import_menu(ui, &e.rel));
        if resp.clicked() {
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
        if card(ui, "PFB", egui::Color32::from_rgb(255, 190, 90), &e.name, tr("prefab - click to spawn", "префаб - клик для спавна"), armed, None).on_hover_text(format!("{}\n{}", e.name, e.rel)).clicked() {
            if d.assets.click_place {
                d.assets.placing = Some(Placing::Prefab(e.rel.clone()));
            } else {
                d.queue.push(Cmd::SpawnPrefab { path: e.rel.clone(), position: center, rotation_y_deg: 0.0 });
            }
        }
    }
}

pub fn draw(ui: &mut egui::Ui, d: &mut UiData) {
    if d.assets.scanned.is_none_or(|t| t.elapsed() > RESCAN) {
        let assets = assets_dir();
        let project = assets.parent().map(Path::to_path_buf).unwrap_or_default();
        d.assets.index = scan_assets(&assets, &project);
        d.assets.scanned = Some(Instant::now());
    }
    let frame = egui::Frame::new().fill(PANEL_BG).stroke(egui::Stroke::new(1.0, BORDER)).inner_margin(egui::Margin::same(8));
    let (sx, sz) = spawn_point(d);
    let center = Vec3::new(sx, 0.0, sz);
    let max_height = (ui.available_height() * 0.55).max(76.0);
    let panel_id = d.layout.asset_panel_id();
    let panel = egui::Panel::bottom(panel_id).default_size(d.layout.asset_height().clamp(76.0, max_height)).size_range(76.0..=max_height).resizable(true).frame(frame).show(ui, |ui| {
        let counts = [d.assets.index.models.len(), d.assets.index.prefabs.len(), d.assets.index.scenes.len()];
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(tr("ASSET BROWSER", "БРАУЗЕР АССЕТОВ")).strong().size(12.0).color(TEAL));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if icons::button(ui, false, Icon::Chevron, tr("Collapse the Asset Browser dock (reopen from Window → Asset Browser)", "Свернуть браузер ассетов (вернуть через Окно → Браузер ассетов)"), 20.0).clicked() {
                    d.layout.toggle(crate::ui::layout::Panel::Assets);
                }
                ui.add(egui::TextEdit::singleline(&mut d.assets.search).hint_text(tr("Search file name...", "Поиск по имени файла...")).desired_width(160.0));
                icons::paint(ui, Icon::Search, 13.0, MUTED);
            });
        });
        ui.horizontal_wrapped(|ui| {
            for (tab, kind, name) in [
                (Tab::All, Icon::Objects, tr("All", "Все").to_string()),
                (Tab::Models, Icon::Cube, format!("{} (.glb / .gltf) {}", tr("Models", "Модели"), counts[0])),
                (Tab::Prefabs, Icon::Folder, format!("{} (.prefab.json) {}", tr("Prefabs", "Префабы"), counts[1])),
                (Tab::Scenes, Icon::Save, format!("{} (.scn.json) {}", tr("Scenes", "Сцены"), counts[2])),
                (Tab::Kit, Icon::Grid, tr("Procedural & Level Kit", "Процедурный набор и уровни").to_string()),
            ] {
                if icon_chip(ui, d.assets.tab == tab, kind, &name, &name).clicked() {
                    d.assets.tab = tab;
                }
            }
            vsep(ui);
            if icon_chip(ui, d.assets.click_place, Icon::Select, tr("Click to place", "Клик для размещения"), tr("Armed: a card sticks to the cursor; click terrain / grid to place, Esc or right click to stop", "Режим ожидания: карточка следует за курсором; клик по рельефу / сетке размещает её, Esc или правый клик — отмена")).clicked() {
                d.assets.click_place = !d.assets.click_place;
                d.assets.placing = None;
            }
            if let Some(p) = &d.assets.placing {
                ui.label(egui::RichText::new(format!("{} {}", tr("placing", "размещение"), match p { Placing::Model(k) | Placing::Prefab(k) => k })).size(11.0).color(TEAL));
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
                        ui.weak(tr("No models match - drop .glb files into assets/models/ or assets/showcase/", "Модели не найдены - положите .glb в assets/models/ или assets/showcase/"));
                    }
                });
            }
            Tab::Prefabs => {
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut d.assets.prefab_name).hint_text(tr("prefab name", "имя префаба")).desired_width(150.0));
                    let n = d.sel.selected.len();
                    ui.add_enabled_ui(n > 0 && !d.assets.prefab_name.trim().is_empty(), |ui| {
                        if icon_chip(ui, false, Icon::Save, tr("Save Selected as Prefab", "Сохранить выделенное как префаб"), tr("Saves the selection to assets/prefabs/<name>.prefab.json", "Сохраняет выделение в assets/prefabs/<имя>.prefab.json")).clicked() {
                            d.queue.push(Cmd::SaveSelectionAsPrefab { path: d.assets.prefab_name.trim().to_string() });
                            d.assets.scanned = None;
                        }
                    });
                    ui.weak(format!("{}: {n}", tr("Selected", "Выделено")));
                });
                let list: Vec<_> = d.assets.index.prefabs.iter().filter(|e| hit(e)).cloned().collect();
                egui::ScrollArea::vertical().id_salt("assets_prefabs").auto_shrink([false, false]).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| prefab_cards(ui, d, &list, center));
                    if list.is_empty() {
                        ui.weak(tr("No prefabs yet - select objects and save them as a prefab", "Префабов пока нет - выделите объекты и сохраните их как префаб"));
                    }
                });
            }
            Tab::Scenes => {
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut d.assets.scene_path).desired_width(240.0));
                    if icon_chip(ui, false, Icon::Save, tr("Save Current", "Сохранить текущую"), tr("Write the scene to the path above", "Записать сцену по указанному пути")).clicked() {
                        d.queue.push(Cmd::SaveScene { path: d.assets.scene_path.trim().to_string() });
                        d.assets.scanned = None;
                    }
                });
                let list: Vec<_> = d.assets.index.scenes.iter().filter(|e| hit(e)).cloned().collect();
                egui::ScrollArea::vertical().id_salt("assets_scenes").auto_shrink([false, false]).show(ui, |ui| {
                    for e in &list {
                        ui.horizontal(|ui| {
                            if icon_chip(ui, false, Icon::Folder, tr("Load", "Загрузить"), &e.rel).clicked() {
                                d.queue.push(Cmd::LoadScene { path: e.rel.clone() });
                            }
                            if ui.small_button(tr("use name", "взять имя")).clicked() {
                                d.assets.scene_path = e.rel.clone();
                            }
                            ui.label(egui::RichText::new(&e.rel).color(TEXT));
                        });
                    }
                    if list.is_empty() {
                        ui.weak(tr("No scenes - Save Current writes a .scn.json into scenes/", "Сцен нет - «Сохранить текущую» создаёт .scn.json в scenes/"));
                    }
                });
            }
            Tab::Kit => {
                let items: Vec<_> = d.reg.spawner_items().into_iter().filter(|i| filter.is_empty() || i.label.to_lowercase().contains(&filter)).cloned().collect();
                let level = d.gset.floor_level;
                let cell = IVec2::new((sx / d.gset.cell_size).round() as i32, (sz / d.gset.cell_size).round() as i32);
                egui::ScrollArea::vertical().id_salt("assets_kit").auto_shrink([false, false]).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for it in &items {
                            if card(ui, "OBJ", egui::Color32::from_rgb(120, 220, 150), crate::ui::i18n::label(it.label), crate::ui::i18n::label(it.category), false, None).clicked() {
                                (it.spawn)(&mut d.queue, sx, sz);
                            }
                        }
                        for style in WallStyle::ALL {
                            if card(ui, "ROOM", egui::Color32::from_rgb(230, 130, 170), crate::ui::i18n::label(style.label()), tr("5 x 4 room at view center", "комната 5 x 4 в центре вида"), false, None).clicked() {
                                let (min, max) = ([cell.x - 2, cell.y - 2], [cell.x + 2, cell.y + 1]);
                                d.queue.push(Cmd::SetGrid { enabled: Some(true), cell_size: None, floor_level: None, floor_height: None, angle_snap_deg: None });
                                d.queue.push(Cmd::BuildRoom { min, max, floor: level, style });
                            }
                        }
                        if card(ui, "DEMO", egui::Color32::from_rgb(255, 150, 90), tr("Demo Tavern", "Демо-таверна"), tr("two rooms, door, stairs", "две комнаты, дверь, лестница"), false, None).clicked() {
                            for c in demo_tavern_commands(WallStyle::WoodTavern) {
                                d.queue.push(c);
                            }
                        }
                        if card(ui, "SKY", egui::Color32::from_rgb(160, 160, 255), tr("Misty Dusk", "Туманный закат"), tr("world preset", "пресет мира"), false, None).clicked() {
                            d.queue.push(Cmd::ApplyPreset(WorldPreset::MistyDuskLake));
                        }
                    });
                });
            }
        }
    });
    if ui.ctx().read_response(panel_id.with("__resize")).is_some_and(|handle| handle.drag_stopped() || handle.double_clicked()) {
        d.layout.set_asset_height(panel.response.rect.height());
    }
}

/// Compact reopen strip shown in Classic when the Asset Browser dock is hidden (keeps a predictable bottom anchor).
pub fn collapsed_strip(ui: &mut egui::Ui, d: &mut UiData) {
    let frame = egui::Frame::new().fill(PANEL_BG).stroke(egui::Stroke::new(1.0, BORDER)).inner_margin(egui::Margin::symmetric(8, 4));
    egui::Panel::bottom("asset_browser_strip").frame(frame).show(ui, |ui| {
        ui.horizontal(|ui| {
            icons::paint(ui, Icon::Folder, 14.0, TEAL);
            ui.label(egui::RichText::new(tr("ASSET BROWSER", "БРАУЗЕР АССЕТОВ")).strong().size(11.5).color(TEAL));
            ui.weak(egui::RichText::new(tr("collapsed", "свёрнуто")).size(11.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if icons::button(ui, false, Icon::Chevron, tr("Reopen the Asset Browser dock", "Развернуть браузер ассетов"), 20.0).clicked() {
                    d.layout.toggle(crate::ui::layout::Panel::Assets);
                }
            });
        });
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
    let over_ui = egui.ctx_mut().map(|c| crate::ui::pointer_over_ui(c) || c.egui_wants_pointer_input()).unwrap_or(false);
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
