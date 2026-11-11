//! Line pictograms in the ArcStudio brand-kit style, drawn with the egui painter.

use bevy_egui::egui::{self, Color32, Painter, Pos2, Rect, Stroke};

use crate::ui::TEAL;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Icon {
    Logo,
    Select,
    Move,
    Rotate,
    Scale,
    Brush,
    Grid,
    GridPlus,
    Hash,
    Cube,
    Mountain,
    Sun,
    Objects,
    Gear,
    Play,
    Pause,
    Stop,
    Save,
    Search,
    Pin,
    Close,
    Chevron,
    Folder,
    Camera,
    Stats,
}

fn stroke(width: f32, color: Color32) -> Stroke {
    Stroke::new(width, color)
}

/// Draws the pictogram inside `rect` (already padded by the caller).
pub(crate) fn draw(p: &Painter, rect: Rect, kind: Icon, color: Color32) {
    let w = (rect.width().min(rect.height()) * 0.12).max(1.2);
    let s = stroke(w, color);
    let c = rect.center();
    let r = rect.width().min(rect.height()) / 2.0;
    let pt = |x: f32, y: f32| Pos2::new(rect.left() + rect.width() * x, rect.top() + rect.height() * y);
    let line = |a: Pos2, b: Pos2| p.line_segment([a, b], s);
    let poly = |pts: Vec<Pos2>| p.add(egui::Shape::line(pts, s));
    match kind {
        Icon::Logo => {
            let thick = stroke(w * 2.2, color);
            p.add(egui::Shape::line(vec![pt(0.16, 0.86), pt(0.5, 0.12), pt(0.84, 0.86)], thick));
            p.add(egui::Shape::line(vec![pt(0.3, 0.62), pt(0.5, 0.55), pt(0.78, 0.66)], thick));
        }
        Icon::Select => {
            p.add(egui::Shape::convex_polygon(vec![pt(0.24, 0.14), pt(0.24, 0.82), pt(0.42, 0.64), pt(0.56, 0.86), pt(0.66, 0.78), pt(0.52, 0.58), pt(0.76, 0.54)], color, Stroke::NONE));
        }
        Icon::Move => {
            line(pt(0.5, 0.12), pt(0.5, 0.88));
            line(pt(0.12, 0.5), pt(0.88, 0.5));
            for (dx, dy) in [(0.0, -1.0), (0.0, 1.0), (-1.0, 0.0), (1.0, 0.0)] {
                let tip = Pos2::new(c.x + dx * r * 0.88, c.y + dy * r * 0.88);
                let side = Pos2::new(-dy, dx);
                let base = Pos2::new(c.x + dx * r * 0.5, c.y + dy * r * 0.5);
                poly(vec![tip + side.to_vec2() * r * 0.22, base, tip - side.to_vec2() * r * 0.22]);
            }
        }
        Icon::Rotate => {
            let mut pts = Vec::new();
            for i in 0..=20 {
                let a = -0.4 + i as f32 / 20.0 * 4.6;
                pts.push(Pos2::new(c.x + a.cos() * r * 0.72, c.y + a.sin() * r * 0.72));
            }
            let tip = *pts.last().unwrap();
            poly(pts);
            poly(vec![tip + egui::vec2(-r * 0.3, -r * 0.12), tip, tip + egui::vec2(r * 0.02, -r * 0.34)]);
        }
        Icon::Scale => {
            line(pt(0.2, 0.8), pt(0.78, 0.22));
            poly(vec![pt(0.5, 0.2), pt(0.8, 0.2), pt(0.8, 0.5)]);
            p.rect_stroke(Rect::from_min_size(pt(0.14, 0.66), egui::vec2(r * 0.3, r * 0.3)), 1.0, s, egui::StrokeKind::Middle);
        }
        Icon::Brush => {
            let mut top = Vec::new();
            let mut bottom = Vec::new();
            for i in 0..=16 {
                let t = i as f32 / 16.0;
                let x = 0.14 + t * 0.72;
                top.push(pt(x, 0.38 + (t * 6.28).sin() * 0.1));
                bottom.push(pt(x, 0.64 + (t * 6.28 + 1.4).sin() * 0.1));
            }
            poly(top);
            poly(bottom);
        }
        Icon::Grid => {
            for i in 1..=2 {
                let t = i as f32 / 3.0;
                line(pt(t, 0.14), pt(t, 0.86));
                line(pt(0.14, t), pt(0.86, t));
            }
            p.rect_stroke(Rect::from_min_max(pt(0.14, 0.14), pt(0.86, 0.86)), 1.0, s, egui::StrokeKind::Middle);
        }
        Icon::GridPlus => {
            p.rect_stroke(Rect::from_min_max(pt(0.16, 0.16), pt(0.84, 0.84)), 1.0, s, egui::StrokeKind::Middle);
            line(pt(0.5, 0.3), pt(0.5, 0.7));
            line(pt(0.3, 0.5), pt(0.7, 0.5));
        }
        Icon::Hash => {
            line(pt(0.36, 0.14), pt(0.28, 0.86));
            line(pt(0.68, 0.14), pt(0.6, 0.86));
            line(pt(0.16, 0.36), pt(0.86, 0.36));
            line(pt(0.14, 0.64), pt(0.84, 0.64));
        }
        Icon::Cube => {
            let top = pt(0.5, 0.12);
            let l = pt(0.16, 0.32);
            let r = pt(0.84, 0.32);
            let mid = pt(0.5, 0.52);
            let bl = pt(0.16, 0.7);
            let br = pt(0.84, 0.7);
            let bot = pt(0.5, 0.9);
            poly(vec![top, r, br, bot, bl, l, top]);
            poly(vec![l, mid, r]);
            line(mid, bot);
        }
        Icon::Mountain => {
            poly(vec![pt(0.08, 0.8), pt(0.36, 0.28), pt(0.56, 0.62), pt(0.72, 0.4), pt(0.92, 0.8)]);
            line(pt(0.08, 0.8), pt(0.92, 0.8));
        }
        Icon::Sun => {
            p.circle_stroke(c, r * 0.34, s);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::FRAC_PI_4;
                let dir = egui::vec2(a.cos(), a.sin());
                line(c + dir * r * 0.52, c + dir * r * 0.86);
            }
        }
        Icon::Objects => {
            p.rect_stroke(Rect::from_min_max(pt(0.16, 0.16), pt(0.6, 0.6)), 1.0, s, egui::StrokeKind::Middle);
            p.rect_stroke(Rect::from_min_max(pt(0.4, 0.4), pt(0.84, 0.84)), 1.0, s, egui::StrokeKind::Middle);
        }
        Icon::Gear => {
            p.circle_stroke(c, r * 0.3, s);
            for i in 0..6 {
                let a = i as f32 * std::f32::consts::FRAC_PI_3;
                let dir = egui::vec2(a.cos(), a.sin());
                line(c + dir * r * 0.46, c + dir * r * 0.78);
            }
        }
        Icon::Play => {
            p.add(egui::Shape::convex_polygon(vec![pt(0.3, 0.18), pt(0.3, 0.82), pt(0.82, 0.5)], color, Stroke::NONE));
        }
        Icon::Pause => {
            p.rect_filled(Rect::from_min_max(pt(0.28, 0.2), pt(0.42, 0.8)), 1.0, color);
            p.rect_filled(Rect::from_min_max(pt(0.58, 0.2), pt(0.72, 0.8)), 1.0, color);
        }
        Icon::Stop => {
            p.rect_filled(Rect::from_min_max(pt(0.24, 0.24), pt(0.76, 0.76)), 1.5, color);
        }
        Icon::Save => {
            p.rect_stroke(Rect::from_min_max(pt(0.16, 0.16), pt(0.84, 0.84)), 2.0, s, egui::StrokeKind::Middle);
            poly(vec![pt(0.32, 0.16), pt(0.32, 0.4), pt(0.68, 0.4), pt(0.68, 0.16)]);
            p.rect_stroke(Rect::from_min_max(pt(0.32, 0.58), pt(0.68, 0.84)), 1.0, s, egui::StrokeKind::Middle);
        }
        Icon::Search => {
            p.circle_stroke(Pos2::new(c.x - r * 0.14, c.y - r * 0.14), r * 0.44, s);
            line(Pos2::new(c.x + r * 0.2, c.y + r * 0.2), pt(0.84, 0.84));
        }
        Icon::Pin => {
            p.circle_stroke(Pos2::new(c.x, c.y - r * 0.24), r * 0.34, s);
            line(Pos2::new(c.x, c.y + r * 0.1), pt(0.5, 0.88));
        }
        Icon::Close => {
            line(pt(0.24, 0.24), pt(0.76, 0.76));
            line(pt(0.76, 0.24), pt(0.24, 0.76));
        }
        Icon::Chevron => {
            poly(vec![pt(0.26, 0.38), pt(0.5, 0.64), pt(0.74, 0.38)]);
        }
        Icon::Folder => {
            poly(vec![pt(0.12, 0.3), pt(0.4, 0.3), pt(0.48, 0.4), pt(0.88, 0.4), pt(0.88, 0.78), pt(0.12, 0.78), pt(0.12, 0.3)]);
        }
        Icon::Camera => {
            p.circle_stroke(c, r * 0.62, s);
            p.circle_filled(c, r * 0.16, color);
            line(pt(0.5, 0.06), pt(0.5, 0.24));
        }
        Icon::Stats => {
            line(pt(0.14, 0.86), pt(0.86, 0.86));
            p.rect_filled(Rect::from_min_max(pt(0.22, 0.5), pt(0.36, 0.86)), 1.0, color);
            p.rect_filled(Rect::from_min_max(pt(0.44, 0.28), pt(0.58, 0.86)), 1.0, color);
            p.rect_filled(Rect::from_min_max(pt(0.66, 0.62), pt(0.8, 0.86)), 1.0, color);
        }
    }
}

/// Allocates a square pictogram (non-interactive).
pub(crate) fn paint(ui: &mut egui::Ui, kind: Icon, side: f32, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
    draw(ui.painter(), rect.shrink(side * 0.16), kind, color);
}

/// Uniform square icon button with active highlight, brand-kit style.
pub(crate) fn button(ui: &mut egui::Ui, on: bool, kind: Icon, tooltip: &str, side: f32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::click());
    let visuals = ui.style().interact(&response);
    let fill = if on { TEAL } else { visuals.weak_bg_fill };
    ui.painter().rect_filled(rect, 6.0, fill);
    if response.hovered() && !on {
        ui.painter().rect_stroke(rect, 6.0, stroke(1.0, crate::ui::BORDER), egui::StrokeKind::Middle);
    }
    let color = if on { crate::ui::CHROME_BG } else { visuals.fg_stroke.color };
    draw(ui.painter(), rect.shrink(side * 0.24), kind, color);
    response.on_hover_text(tooltip)
}

/// Pictogram of a registry panel id (tabs and collapsing headers).
pub(crate) fn panel(id: &str) -> Icon {
    match id {
        "global" => Icon::Sun,
        "terrain_settings" | "terrain_layers" => Icon::Mountain,
        "brushes" | "quillshade" => Icon::Brush,
        "objects" => Icon::Objects,
        "level_builder" => Icon::Cube,
        "materials" | "prefab_variants" => Icon::Folder,
        "node_graph" => Icon::Hash,
        _ => Icon::Gear,
    }
}