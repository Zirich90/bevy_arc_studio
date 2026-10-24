//! Left outliner (E2): tree of scene objects via `ChildOf`, click / ctrl+click selection, double-click rename, drag-and-drop reparent.
//! Every edit goes through `StudioCommand` (`SetComponent`, `Reparent`), so it is undoable and scriptable like the rest.

use std::collections::HashMap;

use bevy::prelude::*;
use bevy_egui::egui;

use crate::mcp_commands::StudioCommand as Cmd;
use crate::operators::Target;
use crate::ui::*;

pub const OUTLINER_W: f32 = 260.0;

/// Outliner-local UI state.
#[derive(Resource, Default)]
pub struct OutlinerState {
    pub search: String,
    /// (entity, text buffer) of the row being renamed.
    renaming: Option<(Entity, String)>,
}

struct Node {
    name: String,
    id: u32,
    kids: Vec<Entity>,
}

fn drag_payload_over_root(ui: &egui::Ui) -> bool {
    egui::DragAndDrop::has_payload_of_type::<Entity>(ui.ctx())
}

pub fn draw(ui: &mut egui::Ui, d: &mut UiData, st: &mut OutlinerState) {
    // flat snapshot: objects, their object-parent, children sorted by id
    let mut nodes: HashMap<Entity, Node> = d.objs.iter().map(|(e, _, so, _)| (e, Node { name: so.name.clone(), id: so.id, kids: Vec::new() })).collect();
    let mut roots: Vec<Entity> = Vec::new();
    let mut order: Vec<(Entity, u32)> = nodes.iter().map(|(e, n)| (*e, n.id)).collect();
    order.sort_by_key(|(_, id)| *id);
    for (e, _) in &order {
        let parent = d.parents.get(*e).ok().map(|p| p.parent()).filter(|p| nodes.contains_key(p));
        match parent {
            Some(p) => nodes.get_mut(&p).unwrap().kids.push(*e),
            None => roots.push(*e),
        }
    }

    let frame = egui::Frame::new().fill(PANEL_BG).stroke(egui::Stroke::new(1.0, BORDER)).inner_margin(egui::Margin::same(8));
    egui::Panel::left("outliner").exact_size(OUTLINER_W).resizable(false).frame(frame).show(ui, |ui| {
        ui.label(egui::RichText::new("SCENETREE").strong().size(12.0).color(TEAL));
        ui.add(egui::TextEdit::singleline(&mut st.search).hint_text("Filter by name...").desired_width(f32::INFINITY));
        ui.label(egui::RichText::new(format!("{} objects", nodes.len())).size(10.5).color(egui::Color32::from_gray(130)));
        ui.separator();

        // drop target: scene root
        let root_row = ui.add(egui::Label::new(egui::RichText::new("▣ Scene (root)").color(egui::Color32::from_gray(170))).sense(egui::Sense::hover()));
        if drag_payload_over_root(ui) && root_row.dnd_hover_payload::<Entity>().is_some() {
            ui.painter().rect_stroke(root_row.rect.expand(2.0), 3.0, egui::Stroke::new(1.5, TEAL), egui::StrokeKind::Outside);
        }
        if let Some(child) = root_row.dnd_release_payload::<Entity>() {
            if d.parents.get(*child).is_ok() {
                d.queue.push(Cmd::Reparent { child: Target::Entity(*child), parent: None });
            }
        }

        let filter = st.search.to_lowercase();
        let mut acts = Acts::default();
        egui::ScrollArea::vertical().id_salt("outliner_scroll").auto_shrink([false, false]).show(ui, |ui| {
            if filter.is_empty() {
                // same-named roots collapse into one "Name ×N" group (scatter forests have thousands)
                let mut groups: Vec<(&str, Vec<Entity>)> = Vec::new();
                for r in &roots {
                    let name = nodes[r].name.as_str();
                    match groups.iter_mut().find(|(n, _)| *n == name) {
                        Some((_, v)) => v.push(*r),
                        None => groups.push((name, vec![*r])),
                    }
                }
                for (name, members) in groups {
                    if members.len() == 1 {
                        row(ui, members[0], &nodes, d, st, &mut acts);
                    } else {
                        egui::CollapsingHeader::new(format!("{name} ×{}", members.len())).id_salt(("outliner_group", name)).default_open(false).show(ui, |ui| {
                            for m in members {
                                row(ui, m, &nodes, d, st, &mut acts);
                            }
                        });
                    }
                }
            } else {
                for (e, _) in &order {
                    if nodes[e].name.to_lowercase().contains(&filter) {
                        let n = &nodes[e];
                        leaf(ui, *e, &n.name, &format!("{} #{}", n.name, n.id), d, st, &mut acts);
                    }
                }
            }
        });
        if let Some((child, parent)) = acts.reparent {
            if child != parent && d.parents.get(child).ok().map(|p| p.parent()) != Some(parent) {
                d.queue.push(Cmd::Reparent { child: Target::Entity(child), parent: Some(Target::Entity(parent)) });
            }
        }
        if let Some((e, name)) = acts.rename {
            d.queue.push(Cmd::SetComponent { target: Target::Entity(e), component: "SceneObject".into(), value: serde_json::json!({ "name": name }).to_string() });
        }
    });
}

#[derive(Default)]
struct Acts {
    reparent: Option<(Entity, Entity)>,
    rename: Option<(Entity, String)>,
}

fn row(ui: &mut egui::Ui, e: Entity, nodes: &HashMap<Entity, Node>, d: &mut UiData, st: &mut OutlinerState, acts: &mut Acts) {
    let n = &nodes[&e];
    let shown = format!("{} #{}", n.name, n.id);
    if n.kids.is_empty() {
        leaf(ui, e, &n.name, &shown, d, st, acts);
        return;
    }
    let id = ui.make_persistent_id(("outliner", e));
    egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true)
        .show_header(ui, |ui| leaf(ui, e, &n.name, &shown, d, st, acts))
        .body(|ui| {
            for k in &n.kids {
                row(ui, *k, nodes, d, st, acts);
            }
        });
}

/// `name` is the real object name (rename buffer, change detection); `shown` is the row text.
fn leaf(ui: &mut egui::Ui, e: Entity, name: &str, shown: &str, d: &mut UiData, st: &mut OutlinerState, acts: &mut Acts) {
    if let Some((re, buf)) = st.renaming.as_mut() {
        if *re == e {
            let r = ui.add(egui::TextEdit::singleline(buf).desired_width(f32::INFINITY));
            if !r.has_focus() && !r.lost_focus() {
                r.request_focus();
            }
            let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
            let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
            if r.lost_focus() && enter && !buf.trim().is_empty() && buf.as_str() != name {
                acts.rename = Some((e, buf.trim().to_string()));
            }
            if r.lost_focus() || esc {
                st.renaming = None;
            }
            return;
        }
    }
    let selected = d.sel.selected.contains(&e);
    let primary = d.sel.primary == Some(e);
    let text = egui::RichText::new(shown).color(if primary { TEAL } else { egui::Color32::from_gray(220) });
    let r = ui.add(egui::Button::selectable(selected, text).sense(egui::Sense::click_and_drag()));
    if r.drag_started() {
        egui::DragAndDrop::set_payload(ui.ctx(), e);
    }
    if r.dnd_hover_payload::<Entity>().is_some_and(|p| *p != e) {
        ui.painter().rect_stroke(r.rect, 3.0, egui::Stroke::new(1.5, TEAL), egui::StrokeKind::Inside);
    }
    if let Some(child) = r.dnd_release_payload::<Entity>() {
        if *child != e {
            acts.reparent = Some((*child, e));
        }
    }
    if r.double_clicked() {
        st.renaming = Some((e, name.to_string()));
    } else if r.clicked() {
        let (shift, ctrl) = ui.input(|i| (i.modifiers.shift, i.modifiers.command));
        if shift || ctrl {
            if selected {
                d.sel.selected.remove(&e);
                if d.sel.primary == Some(e) {
                    d.sel.primary = d.sel.selected.iter().copied().next();
                }
            } else {
                d.sel.selected.insert(e);
                d.sel.primary = Some(e);
            }
        } else {
            d.sel.set_one(e);
        }
    }
}
