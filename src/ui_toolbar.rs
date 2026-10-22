//! Top toolbar: mode switch, tools, camera, history, scene presets, scene JSON.

use bevy_egui::egui;

use crate::arc_core::WorldPreset;
use crate::gizmo_3d::GizmoMode;
use crate::mcp_commands::StudioCommand as Cmd;
use crate::studio::{CamStyle, StudioMode};
use crate::ui::*;

/// Draws the panel; returns its bottom edge (screen y) so the HUD can sit underneath.
pub fn draw(ui: &mut egui::Ui, d: &mut UiData) -> f32 {
    let frame = egui::Frame::new().fill(PANEL_BG).stroke(egui::Stroke::new(1.0, BORDER)).inner_margin(egui::Margin::symmetric(10, 6));
    let out = egui::Panel::top("toolbar").frame(frame).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("ARC STUDIO").strong().color(TEAL).size(13.0));
            vsep(ui);
            if chip(ui, d.studio.mode == StudioMode::Editor, "Editor Mode (Tab)").clicked() {
                d.studio.mode = StudioMode::Editor;
            }
            if chip(ui, d.studio.mode == StudioMode::Play, "Play Mode (Full Screen)").clicked() {
                d.studio.mode = StudioMode::Play;
            }
            vsep(ui);
            for (label, m) in [("Select (Q)", GizmoMode::Select), ("Move (W)", GizmoMode::Move), ("Rotate (E)", GizmoMode::Rotate), ("Scale (R)", GizmoMode::Scale)] {
                if chip(ui, d.gz.mode == m, label).clicked() {
                    d.gz.mode = m;
                }
            }
            if chip(ui, d.gz.snap_ground, "Snap to Ground").clicked() {
                d.gz.snap_ground = !d.gz.snap_ground;
            }
            if chip(ui, d.gz.align_normal, "Align to Normal").clicked() {
                d.gz.align_normal = !d.gz.align_normal;
            }
            vsep(ui);
            if chip(ui, d.studio.cam_style == CamStyle::Orbit, "Orbit Cam").clicked() {
                d.queue.push(Cmd::SetCameraStyle(CamStyle::Orbit));
            }
            if chip(ui, d.studio.cam_style == CamStyle::Fly, "Fly Cam").clicked() {
                d.queue.push(Cmd::SetCameraStyle(CamStyle::Fly));
            }
            if chip(ui, false, "Focus (F)").clicked() {
                d.queue.push(Cmd::FocusSelected);
            }
        });
        ui.horizontal(|ui| {
            ui.add_enabled_ui(!d.order.stack.is_empty(), |ui| {
                if chip(ui, false, "Undo (Ctrl+Z)").clicked() {
                    d.queue.push(Cmd::Undo);
                }
            });
            ui.add_enabled_ui(!d.order.redo.is_empty(), |ui| {
                if chip(ui, false, "Redo (Ctrl+Y)").clicked() {
                    d.queue.push(Cmd::Redo);
                }
            });
            ui.weak(format!("{}/{}", d.order.stack.len(), d.order.redo.len()));
            vsep(ui);
            for (label, p) in [("Misty Dusk", WorldPreset::MistyDuskLake), ("Sunny Noon", WorldPreset::SunnyAlpineNoon), ("Midnight Camp", WorldPreset::MidnightCampfire), ("Golden Dawn", WorldPreset::GoldenAutumnDawn)] {
                if chip(ui, false, label).clicked() {
                    d.queue.push(Cmd::ApplyPreset(p));
                }
            }
            vsep(ui);
            let mut path = d.studio.scene_path.clone();
            ui.add(egui::TextEdit::singleline(&mut path).desired_width(190.0));
            if path != d.studio.scene_path {
                d.studio.scene_path = path.clone();
            }
            if chip(ui, false, "Save Scene").clicked() {
                d.queue.push(Cmd::SaveScene { path: path.clone() });
            }
            if chip(ui, false, "Load Scene").clicked() {
                d.queue.push(Cmd::LoadScene { path });
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.weak(&d.studio.status);
            });
        });
    });
    out.response.rect.bottom()
}
