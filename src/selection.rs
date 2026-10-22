//! Object selection (doc 03): mesh-picking click → SceneObject root, 2D marquee, glowing AABB brackets.

use std::collections::HashSet;

use bevy::gizmos::config::GizmoConfigStore;
use bevy::gizmos::prelude::*;
use bevy::picking::mesh_picking::{MeshPickingCamera, MeshPickingPlugin, MeshPickingSettings};
use bevy::picking::pointer::PointerButton;
use bevy::picking::prelude::*;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};

use crate::arc_core::*;
use crate::camera::EditorCamera;
use crate::gizmo_3d::{GizmoState, gizmo_system};
use crate::props::SceneObject;
use crate::studio::StudioState;

const MARQUEE_THRESHOLD: f32 = 4.0;

/// Soft-deleted object (hidden, kept so undo can bring it back).
#[derive(Component)]
pub struct Deleted;

/// Transform scale at spawn; AABB half extents are expressed at this scale.
#[derive(Component, Clone, Copy)]
pub struct SpawnScale(pub Vec3);

#[derive(Resource, Default)]
pub struct EditorSelection {
    pub primary: Option<Entity>,
    pub selected: HashSet<Entity>,
    pub marquee: Option<(Vec2, Vec2)>,
    press: Option<Vec2>,
    hit_object: bool,
}

impl EditorSelection {
    pub fn clear(&mut self) {
        self.primary = None;
        self.selected.clear();
    }
    pub fn set_one(&mut self, e: Entity) {
        self.selected.clear();
        self.selected.insert(e);
        self.primary = Some(e);
    }
    pub fn add(&mut self, e: Entity) {
        self.selected.insert(e);
        self.primary.get_or_insert(e);
    }
    fn set_all(&mut self, set: HashSet<Entity>) {
        if !self.primary.is_some_and(|p| set.contains(&p)) {
            self.primary = set.iter().copied().min_by_key(|e| e.to_bits());
        }
        self.selected = set;
    }
}

#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct SelectionGizmos;

/// Oriented box of an object: (centre, half extents, rotation). The pivot sits on the object's base.
pub fn object_box(tf: &Transform, so: &SceneObject, base: Option<&SpawnScale>) -> (Vec3, Vec3, Quat) {
    let ratio = base.map_or(Vec3::ONE, |b| tf.scale / b.0.max(Vec3::splat(1e-4)));
    let half = so.aabb_half_extents * ratio.abs();
    (tf.translation + tf.rotation * Vec3::new(0.0, half.y, 0.0), half, tf.rotation)
}

/// 12 wireframe edges of an oriented box.
pub fn draw_box<T: GizmoConfigGroup>(g: &mut Gizmos<T>, c: Vec3, h: Vec3, q: Quat, color: Color) {
    let corner = |s: u32| c + q * (Vec3::new(if s & 1 == 0 { -1.0 } else { 1.0 }, if s & 2 == 0 { -1.0 } else { 1.0 }, if s & 4 == 0 { -1.0 } else { 1.0 }) * h);
    for s in 0..8u32 {
        for bit in [1u32, 2, 4] {
            if s & bit == 0 {
                g.line(corner(s), corner(s | bit), color);
            }
        }
    }
}

pub struct SelectionPlugin;

impl Plugin for SelectionPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MeshPickingPlugin)
            .insert_resource(MeshPickingSettings { require_markers: true, ..default() })
            .init_resource::<EditorSelection>()
            .init_gizmo_group::<SelectionGizmos>()
            .add_observer(on_click)
            .add_systems(Startup, |mut store: ResMut<GizmoConfigStore>| {
                let (cfg, _) = store.config_mut::<SelectionGizmos>();
                cfg.depth_bias = -0.2;
                cfg.line.width = 3.0;
            })
            .add_systems(Update, (mark_pickable, init_spawn_scale, prune_selection, env_select, selection_input.after(gizmo_system), draw_selection).chain())
            .add_systems(EguiPrimaryContextPass, draw_marquee);
    }
}

fn mark_pickable(
    mut commands: Commands,
    new: Query<Entity, Added<Mesh3d>>,
    parents: Query<&ChildOf>,
    objs: Query<(), With<SceneObject>>,
    cams: Query<Entity, (With<EditorCamera>, Without<MeshPickingCamera>)>,
) {
    for c in &cams {
        commands.entity(c).insert(MeshPickingCamera);
    }
    for e in &new {
        let mut cur = e;
        loop {
            if objs.contains(cur) {
                commands.entity(e).insert(Pickable::default());
                break;
            }
            match parents.get(cur) {
                Ok(p) => cur = p.parent(),
                Err(_) => break,
            }
        }
    }
}

fn init_spawn_scale(mut commands: Commands, q: Query<(Entity, &Transform), (With<SceneObject>, Without<SpawnScale>)>) {
    for (e, tf) in &q {
        commands.entity(e).insert(SpawnScale(tf.scale));
    }
}

pub fn prune_selection(mut sel: ResMut<EditorSelection>, alive: Query<(), (With<SceneObject>, Without<Deleted>)>) {
    if sel.selected.iter().any(|e| !alive.contains(*e)) {
        sel.selected.retain(|e| alive.contains(*e));
    }
    if sel.primary.is_some_and(|p| !sel.selected.contains(&p)) {
        sel.primary = sel.selected.iter().copied().min_by_key(|e| e.to_bits());
    }
}

/// Dev aid: `ARC_SELECT=<Name>` selects the first object with that name once it exists.
fn env_select(mut sel: ResMut<EditorSelection>, mut done: Local<bool>, q: Query<(Entity, &SceneObject), Without<Deleted>>) {
    if *done {
        return;
    }
    let Ok(name) = std::env::var("ARC_SELECT") else {
        *done = true;
        return;
    };
    if let Some((e, _)) = q.iter().find(|(_, so)| so.name == name) {
        sel.set_one(e);
        *done = true;
    }
}

#[allow(clippy::too_many_arguments)]
fn on_click(
    mut ev: On<PointerClick>,
    keys: Res<ButtonInput<KeyCode>>,
    mut sel: ResMut<EditorSelection>,
    gizmo: Res<GizmoState>,
    objs: Query<(), (With<SceneObject>, Without<Deleted>)>,
    parents: Query<&ChildOf>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut egui: EguiContexts,
    studio: Res<StudioState>,
) {
    ev.propagate(false);
    let blocked = egui.ctx_mut().map(|c| c.is_pointer_over_egui()).unwrap_or(false) || !studio.gizmo_active();
    let (Some(p0), Some(cur)) = (sel.press, window.cursor_position()) else { return };
    if ev.button != PointerButton::Primary || blocked || gizmo.busy() || p0.distance(cur) > MARQUEE_THRESHOLD {
        return;
    }
    let mut e = ev.original_event_target();
    let root = loop {
        if objs.contains(e) {
            break Some(e);
        }
        match parents.get(e) {
            Ok(p) => e = p.parent(),
            Err(_) => break None,
        }
    };
    let Some(root) = root else { return };
    sel.hit_object = true;
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let ctrl = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    if shift || ctrl {
        if sel.selected.contains(&root) {
            sel.selected.remove(&root);
            if sel.primary == Some(root) {
                sel.primary = sel.selected.iter().copied().min_by_key(|e| e.to_bits());
            }
        } else {
            sel.selected.insert(root);
            sel.primary = Some(root);
        }
    } else {
        sel.set_one(root);
    }
}

#[allow(clippy::too_many_arguments)]
pub fn selection_input(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut sel: ResMut<EditorSelection>,
    gizmo: Res<GizmoState>,
    mut egui: EguiContexts,
    cam: Single<(&Camera, &GlobalTransform), With<EditorCamera>>,
    objs: Query<(Entity, &Transform, &SceneObject, Option<&SpawnScale>), Without<Deleted>>,
    studio: Res<StudioState>,
) {
    let blocked = egui.ctx_mut().map(|c| c.is_pointer_over_egui() || c.egui_wants_pointer_input()).unwrap_or(false) || !studio.gizmo_active();
    let alt = keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]);
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let ctrl = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    let cursor = window.cursor_position();

    if mouse.just_pressed(MouseButton::Left) && !alt && !blocked && !gizmo.busy() {
        sel.press = cursor;
        sel.hit_object = false;
    }
    if mouse.pressed(MouseButton::Left) {
        if let (Some(p0), Some(c)) = (sel.press, cursor) {
            if p0.distance(c) > MARQUEE_THRESHOLD || sel.marquee.is_some() {
                sel.marquee = Some((p0, c));
            }
        }
    }
    if !mouse.just_released(MouseButton::Left) {
        return;
    }
    let marquee = sel.marquee.take();
    if sel.press.take().is_none() {
        return;
    }
    if let Some((a, b)) = marquee {
        let rect = normalize_rect(a, b);
        let (camera, gtf) = *cam;
        let inside: HashSet<Entity> = objs
            .iter()
            .filter(|(_, tf, so, base)| {
                let (c, _, _) = object_box(tf, so, *base);
                camera.world_to_viewport(gtf, c).is_ok_and(|p| rect_contains(rect, p))
            })
            .map(|(e, ..)| e)
            .collect();
        let next = combine_selection(&sel.selected, &inside, shift, ctrl);
        sel.set_all(next);
    } else if !sel.hit_object && !shift && !ctrl {
        sel.clear();
    }
}

fn draw_selection(sel: Res<EditorSelection>, objs: Query<(&Transform, &SceneObject, Option<&SpawnScale>), Without<Deleted>>, mut gizmos: Gizmos<SelectionGizmos>) {
    for e in &sel.selected {
        let Ok((tf, so, base)) = objs.get(*e) else { continue };
        let (c, h, q) = object_box(tf, so, base);
        let (dim, bright) = if sel.primary == Some(*e) {
            (Color::linear_rgb(0.15, 0.7, 1.0), Color::linear_rgb(0.5, 3.0, 4.5))
        } else {
            (Color::linear_rgb(0.1, 0.4, 0.6), Color::linear_rgb(0.3, 1.8, 2.8))
        };
        draw_box(&mut gizmos, c, h, q, dim);
        for s in 0..8u32 {
            let sg = Vec3::new(if s & 1 == 0 { -1.0 } else { 1.0 }, if s & 2 == 0 { -1.0 } else { 1.0 }, if s & 4 == 0 { -1.0 } else { 1.0 });
            let corner = c + q * (sg * h);
            for axis in 0..3 {
                let mut dir = Vec3::ZERO;
                dir[axis] = -sg[axis];
                let len = (h[axis] * 0.5).min(1.6);
                gizmos.line(corner, corner + q * dir * len, bright);
            }
        }
    }
}

fn draw_marquee(mut contexts: EguiContexts, sel: Res<EditorSelection>) {
    let Some((a, b)) = sel.marquee else { return };
    let Ok(ctx) = contexts.ctx_mut() else { return };
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("marquee")));
    let rect = egui::Rect::from_two_pos(egui::pos2(a.x, a.y), egui::pos2(b.x, b.y));
    painter.rect_filled(rect, 0.0, egui::Color32::from_rgba_unmultiplied(80, 170, 255, 40));
    painter.rect_stroke(rect, 0.0, egui::Stroke::new(1.5, egui::Color32::from_rgb(120, 200, 255)), egui::StrokeKind::Inside);
}
