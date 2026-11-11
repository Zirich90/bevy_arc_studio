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
/// Brand kit: #14C7B3 accent, #0F1416 chrome, #1E2326 panels, #344146 borders, #E6ECEB / #A8B3B1 text.
pub const TEAL: egui::Color32 = egui::Color32::from_rgb(20, 199, 179);
pub const CHROME_BG: egui::Color32 = egui::Color32::from_rgb(15, 20, 22);
pub const PANEL_BG: egui::Color32 = egui::Color32::from_rgb(30, 35, 38);
pub const BORDER: egui::Color32 = egui::Color32::from_rgb(52, 65, 70);
pub const TEXT: egui::Color32 = egui::Color32::from_rgb(230, 236, 235);
pub const MUTED: egui::Color32 = egui::Color32::from_rgb(168, 179, 177);
pub const AXIS_X: egui::Color32 = egui::Color32::from_rgb(232, 106, 106);
pub const AXIS_Y: egui::Color32 = egui::Color32::from_rgb(126, 205, 126);
pub const AXIS_Z: egui::Color32 = egui::Color32::from_rgb(108, 158, 235);

pub(crate) mod layout;
pub(crate) mod i18n;
pub(crate) mod icons;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<layout::LayoutState>().init_resource::<ui_inspector::InspectorDraft>().add_systems(EguiPrimaryContextPass, studio_ui_system);
    }
}

#[derive(SystemParam)]
pub struct UiData<'w, 's> {
    pub(crate) layout: ResMut<'w, layout::LayoutState>,
    pub graph: Res<'w, crate::arc_core::StudioGraph>,
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
    pub modules: Res<'w, crate::modules::ModuleRegistry>,
    pub prefab_roots: Query<'w, 's, (&'static crate::prefabs::PrefabInstance, Option<&'static crate::prefab_variants::PrefabOverrides>)>,
    pub matlib: Res<'w, crate::materials::MaterialLibrary>,
    pub thumbs: ResMut<'w, crate::thumbnails::Thumbnails>,
    pub gset: ResMut<'w, crate::level_builder::GridSettings>,
    pub water: ResMut<'w, crate::water::WaterBodies>,
    pub terrain_surfaces: Res<'w, crate::terrain_surfaces::TerrainSurfaces>,
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
        s.visuals.override_text_color = Some(TEXT);
        s.visuals.extreme_bg_color = CHROME_BG;
        s.visuals.faint_bg_color = PANEL_BG;
        s.visuals.panel_fill = PANEL_BG;
        s.visuals.window_fill = PANEL_BG;
        s.visuals.selection.bg_fill = egui::Color32::from_rgb(23, 107, 99);
        s.visuals.selection.stroke = egui::Stroke::new(1.0, TEAL);
        s.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(5);
        s.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(5);
        s.visuals.widgets.active.corner_radius = egui::CornerRadius::same(5);
        s.visuals.widgets.inactive.weak_bg_fill = egui::Color32::from_rgb(47, 59, 64);
        s.visuals.widgets.hovered.weak_bg_fill = egui::Color32::from_rgb(58, 76, 80);
        s.visuals.slider_trailing_fill = true;
        s.spacing.item_spacing = egui::vec2(6.0, 5.0);
        s.spacing.button_padding = egui::vec2(9.0, 5.0);
        s.spacing.interact_size.y = 30.0;
        s.spacing.slider_width = 140.0;
    });
}

pub(crate) fn icon_chip(ui: &mut egui::Ui, on: bool, kind: icons::Icon, label: &str, tooltip: &str) -> egui::Response {
    let font = egui::FontId::proportional(12.5);
    let width = ui.painter().layout_no_wrap(label.to_owned(), font.clone(), TEXT).size().x;
    let size = egui::vec2(width + 32.0, 28.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let visuals = ui.style().interact(&response);
    ui.painter().rect_filled(rect, 6.0, if on { TEAL } else { visuals.weak_bg_fill });
    if response.hovered() && !on {
        ui.painter().rect_stroke(rect, 6.0, egui::Stroke::new(1.0, BORDER), egui::StrokeKind::Middle);
    }
    let color = if on { CHROME_BG } else { visuals.fg_stroke.color };
    let galley = ui.painter().layout_no_wrap(label.to_owned(), font, color);
    let icon_rect = egui::Rect::from_min_size(rect.left_top() + egui::vec2(8.0, (rect.height() - 14.0) / 2.0), egui::vec2(14.0, 14.0));
    icons::draw(ui.painter(), icon_rect, kind, color);
    ui.painter().galley(rect.left_top() + egui::vec2(26.0, (rect.height() - galley.size().y) / 2.0), galley, color);
    response.on_hover_text(tooltip)
}

pub fn studio_ui_system(mut contexts: EguiContexts, mut d: UiData, mut outliner: Local<ui_outliner::OutlinerState>, mut styled: Local<bool>, keys: Res<ButtonInput<KeyCode>>) {
    let Ok(ctx) = contexts.ctx_mut() else { return };
    if !*styled {
        apply_style(ctx);
        *styled = true;
    }
    if !ctx.input(|input| input.pointer.any_down() || input.pointer.any_released()) {
        d.layout.apply_pending();
    }
    let play = d.studio.mode == StudioMode::Playing;
    let mut ui = egui::Ui::new(ctx.clone(), "viewport".into(), egui::UiBuilder::new().layer_id(egui::LayerId::background()).max_rect(ctx.viewport_rect()));
    let top = ui_toolbar::draw(&mut ui, &mut d);
    if d.layout.active() != layout::Layout::Focus || d.studio.show_stats {
        ui_hud::status_bar(&mut ui, &d);
    }
    if !play {
        if d.layout.active() == layout::Layout::Focus && !d.studio.mode.in_session() {
            layout::focus_dock(&mut ui, &mut d);
        }
        if d.layout.visible(layout::Panel::Inspector) {
            ui_inspector::draw(&mut ui, &mut d);
        }
        if d.layout.visible(layout::Panel::SceneTree) {
            ui_outliner::draw(&mut ui, &mut d, &mut outliner);
        }
        if d.layout.visible(layout::Panel::Assets) {
            ui_assets::draw(&mut ui, &mut d);
        } else if d.layout.active() != layout::Layout::Focus {
            ui_assets::collapsed_strip(&mut ui, &mut d);
        }
        ui_inspector::draw_docks(&mut ui, &mut d);
        ui_hud::viewport_chips(&mut ui, &mut d);
    }
    // Panels use a manually constructed root Ui; egui's run_ui viewport bookkeeping does not see them.
    ctx.data_mut(|data| data.insert_temp(egui::Id::new("studio_viewport"), ui.available_rect_before_wrap()));
    if (d.studio.toolbar_h - top).abs() > 0.5 {
        d.studio.toolbar_h = top;
    }
    layout::palette(ctx, &mut d, &keys);
    d.layout.save_if_dirty(ctx);
}

pub(crate) fn pointer_over_ui(ctx: &egui::Context) -> bool {
    ctx.egui_is_using_pointer() || ctx.is_pointer_over_egui() || ctx.data(|data| data.get_temp::<egui::Rect>(egui::Id::new("studio_viewport")))
        .is_some_and(|rect| ctx.pointer_latest_pos().is_some_and(|pos| !rect.contains(pos)))
}

#[cfg(test)]
mod pointer_tests {
    use super::*;

    #[test]
    fn manual_panels_block_pointer_outside_viewport() {
        let ctx = egui::Context::default();
        ctx.data_mut(|data| data.insert_temp(egui::Id::new("studio_viewport"), egui::Rect::from_min_max(egui::pos2(260.0, 100.0), egui::pos2(1240.0, 720.0))));
        for (pos, blocked) in [(egui::pos2(1400.0, 600.0), true), (egui::pos2(800.0, 400.0), false), (egui::pos2(800.0, 50.0), true)] {
            ctx.run_ui(egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 900.0))),
                events: vec![egui::Event::PointerMoved(pos)], ..Default::default()
            }, |_| {}).drop_without_applying_deltas();
            assert_eq!(pointer_over_ui(&ctx), blocked);
        }
    }
}

// ───────────────────────────── shared widgets ─────────────────────────────

/// Toggle-style button: teal when `on`.
pub fn chip(ui: &mut egui::Ui, on: bool, text: &str) -> egui::Response {
    let text = egui::RichText::new(text).size(12.5);
    ui.add(egui::Button::selectable(on, text))
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
