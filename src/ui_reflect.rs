//! Universal Reflect inspector: lists every registered component of the primary selection and edits it without per-type UI code.
//! `sync_reflect` (exclusive, Update) applies queued edits, then re-snapshots the selected entity; the UI works on the snapshot.

use std::any::TypeId;

use bevy::prelude::*;
use bevy::reflect::enums::{DynamicEnum, DynamicVariant, VariantInfo};
use bevy::reflect::{PartialReflect, ReflectMut};
use bevy_egui::egui;

use crate::editor_registry::EditorRegistry;
use crate::operators::{COALESCE_SECS, CompClock, ComponentHistory, flush_comp_history, record_edit, snap};
use crate::selection::EditorSelection;
use crate::studio::HistoryRequest;
use crate::ui::TEAL;

/// Components with dedicated UI or computed every frame.
const HIDDEN: &[&str] = &["Transform", "GlobalTransform", "InheritedVisibility", "ViewVisibility", "ChildOf", "Children"];

pub struct CompView {
    pub name: String,
    pub type_id: TypeId,
    pub value: Box<dyn Reflect>,
}

#[derive(Resource, Default)]
pub struct ReflectView {
    pub entity: Option<Entity>,
    pub comps: Vec<CompView>,
}

#[derive(Resource, Default)]
pub struct ReflectEdits(pub Vec<(Entity, TypeId, Box<dyn Reflect>)>);

pub struct ReflectInspectorPlugin;

impl Plugin for ReflectInspectorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ReflectView>().init_resource::<ReflectEdits>().add_systems(
            Update,
            sync_reflect.after(crate::mcp_commands::execute_commands).before(crate::studio::process_history),
        );
    }
}

fn sync_reflect(world: &mut World) {
    let registry = world.resource::<AppTypeRegistry>().clone();
    // Commit an idle (or about-to-be-undone) coalesced edit; a pending undo/redo waits one frame so the order stack sees it.
    let now = world.resource::<Time>().elapsed_secs_f64();
    let (replay_u, replay_r) = std::mem::take(&mut world.resource_mut::<CompClock>().replay);
    if replay_u || replay_r {
        let mut req = world.resource_mut::<HistoryRequest>();
        (req.undo, req.redo) = (replay_u, replay_r);
    }
    let (req_u, req_r) = {
        let r = world.resource::<HistoryRequest>();
        (r.undo, r.redo)
    };
    if world.resource::<ComponentHistory>().is_open() {
        let idle = now - world.resource::<CompClock>().last_edit > COALESCE_SECS;
        if idle || req_u || req_r {
            flush_comp_history(&mut world.resource_mut::<ComponentHistory>());
            if req_u || req_r {
                *world.resource_mut::<HistoryRequest>() = HistoryRequest::default();
                world.resource_mut::<CompClock>().replay = (req_u, req_r);
            }
        }
    }
    let edits = std::mem::take(&mut world.resource_mut::<ReflectEdits>().0);
    for (e, tid, value) in edits {
        let before = snap(world, e, tid);
        {
            let reg = registry.read();
            let Some(rc) = reg.get_type_data::<ReflectComponent>(tid) else { continue };
            let Ok(em) = world.get_entity_mut(e) else { continue };
            rc.apply(em, value.as_partial_reflect());
        }
        let after = snap(world, e, tid);
        record_edit(world, "Edit component", before, after, true);
    }
    let target = world.resource::<EditorSelection>().primary;
    let mut view = ReflectView::default();
    if let Some(e) = target {
        if let Ok(er) = world.get_entity(e) {
            view.entity = Some(e);
            let reg = registry.read();
            for cid in er.archetype().iter_components() {
                let Some(tid) = world.components().get_info(cid).and_then(|i| i.type_id()) else { continue };
                let Some(rc) = reg.get_type_data::<ReflectComponent>(tid) else { continue };
                let Some(value) = rc.reflect(er).and_then(|r| r.reflect_clone().ok()) else { continue };
                let name = reg.get(tid).map(|t| t.type_info().type_path_table().short_path().to_string()).unwrap_or_default();
                if !HIDDEN.contains(&name.as_str()) {
                    view.comps.push(CompView { name, type_id: tid, value });
                }
            }
            view.comps.sort_by(|a, b| a.name.cmp(&b.name));
        }
    }
    *world.resource_mut::<ReflectView>() = view;
}

/// Component-list action requested from the inspector; the caller turns it into an undoable `StudioCommand`.
pub enum CompAction {
    Add(String),
    Remove(String),
}

/// Components that define the object itself; they have no `[×]`.
const PROTECTED: &[&str] = &["SceneObject", "UserProp"];

pub fn draw(ui: &mut egui::Ui, view: &mut ReflectView, edits: &mut ReflectEdits, reg: &EditorRegistry, filter: &str) -> Option<CompAction> {
    let e = view.entity?;
    let mut action = None;
    ui.separator();
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Components").strong().color(TEAL));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.menu_button("+ Add Component", |ui| {
                let missing: Vec<_> = reg.scene_components().iter().filter(|c| !view.comps.iter().any(|v| v.type_id == c.type_id)).collect();
                if missing.is_empty() {
                    ui.weak("all registered components present");
                }
                for c in missing {
                    if ui.button(&c.name).clicked() {
                        action = Some(CompAction::Add(c.name.clone()));
                        ui.close();
                    }
                }
            });
        });
    });
    for c in view.comps.iter_mut() {
        if !filter.is_empty() && !c.name.to_lowercase().contains(filter) {
            continue;
        }
        let mut changed = false;
        let id = ui.make_persistent_id((e, c.type_id));
        let custom = reg.inspector_override(c.type_id);
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false)
            .show_header(ui, |ui| {
                ui.label(egui::RichText::new(&c.name).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !PROTECTED.contains(&c.name.as_str()) && ui.small_button("×").on_hover_text("Remove component").clicked() {
                        action = Some(CompAction::Remove(c.name.clone()));
                    }
                });
            })
            .body(|ui| {
                changed = match custom {
                    Some(f) => f(ui, c.value.as_partial_reflect_mut()),
                    None => edit(ui, c.value.as_partial_reflect_mut(), "", 0),
                };
            });
        if changed {
            if let Ok(v) = c.value.reflect_clone() {
                edits.0.push((e, c.type_id, v));
            }
        }
    }
    action
}

fn row(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui) -> bool) -> bool {
    ui.horizontal(|ui| {
        if !label.is_empty() {
            ui.add_sized([90.0, 18.0], egui::Label::new(label).truncate());
        }
        add(ui)
    })
    .inner
}

fn drag3(ui: &mut egui::Ui, v: &mut [f32; 3], speed: f64) -> bool {
    let mut ch = false;
    for (x, n) in v.iter_mut().zip(["x", "y", "z"]) {
        ch |= ui.add(egui::DragValue::new(x).speed(speed).prefix(format!("{n} ")).max_decimals(3)).changed();
    }
    ch
}

macro_rules! numeric {
    ($ui:expr, $v:expr, $label:expr, $speed:expr, $($t:ty),+) => {$(
        if let Some(x) = $v.try_downcast_mut::<$t>() {
            return row($ui, $label, |ui| ui.add(egui::DragValue::new(x).speed($speed)).changed());
        }
    )+};
}

/// Draws an editor for `v`; returns true when the user changed it.
fn edit(ui: &mut egui::Ui, v: &mut dyn PartialReflect, label: &str, depth: usize) -> bool {
    if let Some(x) = v.try_downcast_mut::<bool>() {
        return row(ui, label, |ui| ui.checkbox(x, "").changed());
    }
    numeric!(ui, v, label, 0.05, f32, f64);
    numeric!(ui, v, label, 1.0, i8, i16, i32, i64, isize, u8, u16, u32, u64, usize);
    if let Some(s) = v.try_downcast_mut::<String>() {
        return row(ui, label, |ui| ui.text_edit_singleline(s).changed());
    }
    if let Some(x) = v.try_downcast_mut::<Vec3>() {
        return row(ui, label, |ui| {
            let mut a = x.to_array();
            let ch = drag3(ui, &mut a, 0.05);
            *x = Vec3::from(a);
            ch
        });
    }
    if let Some(x) = v.try_downcast_mut::<Vec2>() {
        return row(ui, label, |ui| {
            let ch = ui.add(egui::DragValue::new(&mut x.x).speed(0.05).prefix("x ")).changed();
            ch | ui.add(egui::DragValue::new(&mut x.y).speed(0.05).prefix("y ")).changed()
        });
    }
    if let Some(q) = v.try_downcast_mut::<Quat>() {
        return row(ui, label, |ui| {
            let (y, x, z) = q.to_euler(EulerRot::YXZ);
            let mut a = [x.to_degrees(), y.to_degrees(), z.to_degrees()];
            let ch = drag3(ui, &mut a, 1.0);
            if ch {
                *q = Quat::from_euler(EulerRot::YXZ, a[1].to_radians(), a[0].to_radians(), a[2].to_radians());
            }
            ch
        });
    }
    if let Some(c) = v.try_downcast_mut::<Color>() {
        return row(ui, label, |ui| {
            let s = c.to_srgba();
            let mut rgba = [s.red, s.green, s.blue, s.alpha];
            let ch = ui.color_edit_button_rgba_unmultiplied(&mut rgba).changed();
            if ch {
                *c = Color::srgba(rgba[0], rgba[1], rgba[2], rgba[3]);
            }
            ch
        });
    }
    if let Some(e) = v.try_downcast_mut::<Entity>() {
        return row(ui, label, |ui| {
            ui.label(format!("{e}"));
            false
        });
    }
    if depth > 6 {
        ui.label(format!("{label} …"));
        return false;
    }
    let mut changed = false;
    match v.reflect_mut() {
        ReflectMut::Struct(s) => {
            nested(ui, label, depth, |ui| {
                for i in 0..s.field_len() {
                    let name = s.name_at(i).unwrap_or("?").to_string();
                    if let Some(f) = s.field_at_mut(i) {
                        changed |= edit(ui, f, &name, depth + 1);
                    }
                }
            });
        }
        ReflectMut::TupleStruct(s) => {
            nested(ui, label, depth, |ui| {
                for i in 0..s.field_len() {
                    if let Some(f) = s.field_mut(i) {
                        changed |= edit(ui, f, &i.to_string(), depth + 1);
                    }
                }
            });
        }
        ReflectMut::Tuple(s) => {
            nested(ui, label, depth, |ui| {
                for i in 0..s.field_len() {
                    if let Some(f) = s.field_mut(i) {
                        changed |= edit(ui, f, &i.to_string(), depth + 1);
                    }
                }
            });
        }
        ReflectMut::List(l) => {
            nested(ui, &format!("{label} [{}]", l.len()), depth, |ui| {
                for i in 0..l.len() {
                    if let Some(f) = l.get_mut(i) {
                        changed |= edit(ui, f, &i.to_string(), depth + 1);
                    }
                }
            });
        }
        ReflectMut::Array(l) => {
            nested(ui, &format!("{label} [{}]", l.len()), depth, |ui| {
                for i in 0..l.len() {
                    if let Some(f) = l.get_mut(i) {
                        changed |= edit(ui, f, &i.to_string(), depth + 1);
                    }
                }
            });
        }
        ReflectMut::Enum(en) => {
            let cur = en.variant_name().to_string();
            let units: Vec<String> = en
                .get_represented_enum_info()
                .map(|info| info.iter().filter(|v| matches!(v, VariantInfo::Unit(_))).map(|v| v.name().to_string()).collect())
                .unwrap_or_default();
            let mut pick = cur.clone();
            row(ui, label, |ui| {
                if units.is_empty() {
                    ui.label(&cur);
                } else {
                    egui::ComboBox::from_id_salt((label, depth, "enum")).selected_text(&cur).show_ui(ui, |ui| {
                        for u in &units {
                            ui.selectable_value(&mut pick, u.clone(), u);
                        }
                    });
                }
                false
            });
            if pick != cur {
                en.apply(&DynamicEnum::new(pick, DynamicVariant::Unit));
                changed = true;
            }
            for i in 0..en.field_len() {
                let name = en.name_at(i).map(str::to_string).unwrap_or_else(|| i.to_string());
                if let Some(f) = en.field_at_mut(i) {
                    changed |= edit(ui, f, &name, depth + 1);
                }
            }
        }
        _ => {
            row(ui, label, |ui| {
                ui.weak("(not editable)");
                false
            });
        }
    }
    changed
}

fn nested(ui: &mut egui::Ui, label: &str, depth: usize, body: impl FnOnce(&mut egui::Ui)) {
    if depth == 0 {
        body(ui);
    } else {
        egui::CollapsingHeader::new(label).id_salt((label, depth)).default_open(false).show(ui, body);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Component, Reflect, Default, PartialEq, Debug)]
    #[reflect(Component)]
    struct Probe {
        speed: f32,
        on: bool,
        label: String,
    }

    fn app_with_probe() -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).register_type::<Probe>().init_resource::<EditorSelection>().init_resource::<crate::studio::HistoryRequest>().add_plugins(crate::operators::OperatorsPlugin).add_plugins(ReflectInspectorPlugin);
        let e = app.world_mut().spawn(Probe::default()).id();
        app.world_mut().resource_mut::<EditorSelection>().set_one(e);
        (app, e)
    }

    #[test]
    fn snapshot_edit_roundtrip_applies_to_component() {
        let (mut app, e) = app_with_probe();
        app.update();
        let view = app.world().resource::<ReflectView>();
        assert_eq!(view.entity, Some(e));
        let c = view.comps.iter().find(|c| c.name == "Probe").expect("Probe listed");
        let (tid, mut v) = (c.type_id, c.value.reflect_clone().unwrap());
        if let ReflectMut::Struct(s) = v.reflect_mut() {
            *s.field_mut("speed").unwrap().try_downcast_mut::<f32>().unwrap() = 4.5;
            *s.field_mut("on").unwrap().try_downcast_mut::<bool>().unwrap() = true;
        }
        app.world_mut().resource_mut::<ReflectEdits>().0.push((e, tid, v));
        app.update();
        let p = app.world().get::<Probe>(e).unwrap();
        assert_eq!((p.speed, p.on), (4.5, true));
    }
}
