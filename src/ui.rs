//! Studio GUI shell: one egui pass in `EguiPrimaryContextPass` drawing toolbar, inspector and HUD, plus shared widgets.

use bevy::diagnostic::DiagnosticsStore;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};

use crate::arc_core::*;
use crate::camera::EditorCamera;
use crate::gizmo_3d::{GizmoState, SceneHistory};
use crate::lighting::NightLight;
use crate::mcp_commands::StudioCommandQueue;
use crate::props::{ModelLibrary, SceneObject, UserProp};
use crate::selection::{Deleted, EditorSelection};
use crate::studio::*;
use crate::terrain_brush::BrushState;
use crate::terrain_render::TerrainGrid;
use crate::{ui_assets, ui_hud, ui_inspector, ui_outliner, ui_toolbar};

pub const EMERALD: egui::Color32 = egui::Color32::from_rgb(52, 211, 120);
pub const AMBER: egui::Color32 = egui::Color32::from_rgb(245, 176, 40);
pub const TEAL: egui::Color32 = egui::Color32::from_rgb(38, 198, 180);
pub const PANEL_BG: egui::Color32 = egui::Color32::from_rgba_premultiplied(11, 14, 19, 224);
pub const BORDER: egui::Color32 = egui::Color32::from_rgba_premultiplied(30, 34, 40, 90);

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ui_inspector::InspectorDraft>().add_systems(EguiPrimaryContextPass, studio_ui_system);
    }
}

#[derive(SystemParam)]
pub struct UiData<'w, 's> {
    pub spec: ResMut<'w, WorldSpec>,
    pub wh: ResMut<'w, UndoHistory<WorldSpec>>,
    pub sh: ResMut<'w, SceneHistory>,
    pub studio: ResMut<'w, StudioState>,
    pub brush: ResMut<'w, BrushState>,
    pub queue: ResMut<'w, StudioCommandQueue>,
    pub sel: ResMut<'w, EditorSelection>,
    pub gz: ResMut<'w, GizmoState>,
    pub draft: ResMut<'w, ui_inspector::InspectorDraft>,
    pub rview: ResMut<'w, crate::ui_reflect::ReflectView>,
    pub redits: ResMut<'w, crate::ui_reflect::ReflectEdits>,
    pub reg: Res<'w, crate::editor_registry::EditorRegistry>,
    pub gset: ResMut<'w, crate::level_builder::GridSettings>,
    pub lbs: ResMut<'w, crate::level_builder::LevelBuilderState>,
    pub order: Res<'w, UndoOrder>,
    pub th: Res<'w, TerrainHistory>,
    pub grid: Res<'w, TerrainGrid>,
    pub lib: Res<'w, ModelLibrary>,
    pub diag: Res<'w, DiagnosticsStore>,
    pub cam: Query<'w, 's, &'static EditorCamera>,
    pub objs: Query<'w, 's, (Entity, &'static mut Transform, &'static SceneObject, Option<&'static UserProp>), Without<Deleted>>,
    pub assets: ResMut<'w, crate::ui_assets::AssetBrowserState>,
    pub tiles: Query<'w, 's, &'static crate::level_builder::LevelTile>,
    pub children: Query<'w, 's, &'static Children>,
    pub parents: Query<'w, 's, &'static ChildOf>,
    pub lights: Query<'w, 's, (&'static mut PointLight, Option<&'static mut NightLight>)>,
    pub light_count: Query<'w, 's, (), Or<(With<PointLight>, With<DirectionalLight>)>>,
}

fn apply_style(ctx: &egui::Context) {
    ctx.global_style_mut(|s| {
        s.visuals = egui::Visuals::dark();
        s.visuals.panel_fill = PANEL_BG;
        s.visuals.window_fill = PANEL_BG;
        s.visuals.selection.bg_fill = egui::Color32::from_rgb(24, 120, 110);
        s.visuals.selection.stroke = egui::Stroke::new(1.0, TEAL);
        s.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(5);
        s.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(5);
        s.visuals.widgets.active.corner_radius = egui::CornerRadius::same(5);
        s.visuals.widgets.inactive.weak_bg_fill = egui::Color32::from_rgba_unmultiplied(255, 255, 255, 14);
        s.visuals.widgets.hovered.weak_bg_fill = egui::Color32::from_rgba_unmultiplied(38, 198, 180, 50);
        s.visuals.slider_trailing_fill = true;
        s.spacing.item_spacing = egui::vec2(6.0, 5.0);
        s.spacing.button_padding = egui::vec2(8.0, 3.0);
        s.spacing.slider_width = 140.0;
    });
}

pub fn studio_ui_system(mut contexts: EguiContexts, mut d: UiData, mut outliner: Local<ui_outliner::OutlinerState>, mut styled: Local<bool>) {
    let Ok(ctx) = contexts.ctx_mut() else { return };
    if !*styled {
        apply_style(ctx);
        *styled = true;
    }
    let play = d.studio.mode == StudioMode::Playing;
    let mut ui = egui::Ui::new(ctx.clone(), "viewport".into(), egui::UiBuilder::new().layer_id(egui::LayerId::background()).max_rect(ctx.viewport_rect()));
    let top = ui_toolbar::draw(&mut ui, &mut d);
    if !play {
        ui_inspector::draw(&mut ui, &mut d);
        ui_outliner::draw(&mut ui, &mut d, &mut outliner);
        ui_assets::draw(&mut ui, &mut d);
        ui_inspector::draw_docks(&mut ui, &mut d);
    }
    if (d.studio.toolbar_h - top).abs() > 0.5 {
        d.studio.toolbar_h = top;
    }
    ui_hud::draw(ctx, &mut d);
}

// ───────────────────────────── shared widgets ─────────────────────────────

/// Toggle-style button: teal when `on`.
pub fn chip(ui: &mut egui::Ui, on: bool, text: &str) -> egui::Response {
    let color = if on { egui::Color32::from_rgb(4, 22, 20) } else { egui::Color32::from_gray(208) };
    let mut b = egui::Button::new(egui::RichText::new(text).size(12.5).color(color));
    b = b.fill(if on { TEAL } else { egui::Color32::from_rgba_unmultiplied(255, 255, 255, 16) });
    ui.add(b)
}

pub fn vsep(ui: &mut egui::Ui) {
    ui.add(egui::Separator::default().vertical().spacing(8.0));
}

/// Collected widget events of one edit group (sliders, drags, checkboxes).
#[derive(Default)]
pub struct Ev {
    pub changed: bool,
    pub finish: bool,
}

impl Ev {
    pub fn note(&mut self, r: &egui::Response) {
        self.changed |= r.changed();
        self.finish |= r.drag_stopped() || (r.changed() && !r.dragged());
    }
}

/// Begin → UpdatePreview* → Commit: one undo entry per slider drag / click / typed value.
pub fn track_ev<S: Clone + PartialEq + Send + Sync + 'static>(ev: &Ev, hist: &mut UndoHistory<S>, label: &str, cur: &mut S, next: S) {
    if ev.changed {
        if !hist.is_open() {
            hist.begin(label, cur.clone());
        }
        hist.update_preview(next.clone());
        *cur = next;
    }
    if ev.finish && hist.is_open() {
        hist.commit(cur.clone());
    }
}

pub fn hhmm(t: f32) -> String {
    let m = (t.rem_euclid(24.0) * 60.0).round() as u32 % 1440;
    format!("{:02}:{:02}", m / 60, m % 60)
}
