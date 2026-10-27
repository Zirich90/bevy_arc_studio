//! Industrial 3D gizmo (doc 02/03): Move / Rotate / Scale, surface snapping, duplicate / delete, transactional undo.

use std::f32::consts::TAU;

use bevy::gizmos::config::GizmoConfigStore;
use bevy::gizmos::prelude::*;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::EguiContexts;

use crate::arc_core::*;
use crate::camera::EditorCamera;
use crate::props::SceneObject;
use crate::selection::{Deleted, EditorSelection, draw_box, prune_selection};
use crate::studio::StudioState;
use crate::terrain_render::TerrainGrid;

// geometry in units of the constant-screen-size `handle_size`
const AX_LEN: f32 = 1.2;
const CONE_LEN: f32 = 0.3;
const CONE_R: f32 = 0.09;
const PLANE_A: f32 = 0.35;
const PLANE_B: f32 = 0.7;
const RING_R: f32 = 1.0;
const SCALE_LEN: f32 = 1.1;
const CUBE: f32 = 0.13;
const PICK_PX: f32 = 9.0;
/// Visual multiplier on top of `calc_handle_size` (keeps constant pixel size, just larger).
const HANDLE_SCALE: f32 = 2.0;
const MOVE_SNAP: f32 = 0.5;
const ROT_SNAP_DEG: f32 = 15.0;

#[derive(Reflect, serde::Serialize, serde::Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum GizmoMode {
    Select,
    #[default]
    Move,
    Rotate,
    Scale,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum GizmoSpace {
    #[default]
    World,
    Local,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Handle {
    Axis(usize),
    Plane(usize),
    Center,
    Ring(usize),
    ScaleAxis(usize),
    ScaleAll,
}

/// Undo snapshot of one object: transform plus soft-delete flag.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ObjState {
    pub entity: Entity,
    pub tf: Transform,
    pub alive: bool,
}

pub type SceneHistory = UndoHistory<Vec<ObjState>>;

struct Drag {
    handle: Handle,
    starts: Vec<(Entity, Transform)>,
    origin: Vec3,
    axes: [Vec3; 3],
    normal: Vec3,
    hs: f32,
    grab_t: f32,
    grab_pt: Vec3,
    grab_ground: Option<Vec3>,
    v_start: Vec3,
    tangent_mode: bool,
    tangent: Vec2,
    acc: f32,
    prev_raw: Option<f32>,
    last_mouse: Vec2,
    mouse0: Vec2,
    p0: Vec2,
    last_delta: Vec3,
    last: Vec<ObjState>,
}

#[derive(Resource)]
pub struct GizmoState {
    pub mode: GizmoMode,
    pub space: GizmoSpace,
    pub snap_ground: bool,
    pub align_normal: bool,
    pub align_strength: f32,
    hover: Option<Handle>,
    drag: Option<Drag>,
}

impl Default for GizmoState {
    fn default() -> Self {
        Self { mode: GizmoMode::Move, space: GizmoSpace::World, snap_ground: false, align_normal: false, align_strength: 1.0, hover: None, drag: None }
    }
}

impl GizmoState {
    /// True while the pointer is on a handle or a drag is running (selection must not react).
    pub fn busy(&self) -> bool {
        self.drag.is_some() || self.hover.is_some()
    }
}

#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct HandleGizmos;

pub struct Gizmo3dPlugin;

impl Plugin for Gizmo3dPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GizmoState>()
            .init_resource::<SceneHistory>()
            .init_resource::<ObjectClipboard>()
            .add_systems(Update, refresh_cloned_ids)
            .init_gizmo_group::<HandleGizmos>()
            .add_systems(Startup, setup_gizmo)
            .add_systems(Update, (gizmo_hotkeys, gizmo_system).chain())
            .add_systems(Update, scene_edit_hotkeys.after(gizmo_system).before(prune_selection));
    }
}

fn setup_gizmo(mut store: ResMut<GizmoConfigStore>, mut gz: ResMut<GizmoState>) {
    let (cfg, _) = store.config_mut::<HandleGizmos>();
    cfg.depth_bias = -1.0;
    cfg.line.width = 3.0;
    // dev aid: ARC_GIZMO=select|move|rotate|scale [,local] [,snap] [,align]
    if let Ok(v) = std::env::var("ARC_GIZMO") {
        for t in v.split(',') {
            match t.trim() {
                "select" => gz.mode = GizmoMode::Select,
                "move" => gz.mode = GizmoMode::Move,
                "rotate" => gz.mode = GizmoMode::Rotate,
                "scale" => gz.mode = GizmoMode::Scale,
                "local" => gz.space = GizmoSpace::Local,
                "snap" => gz.snap_ground = true,
                "align" => gz.align_normal = true,
                _ => {}
            }
        }
    }
}

// ───────────────────────────── view helpers ─────────────────────────────

struct View<'a> {
    camera: &'a Camera,
    gtf: &'a GlobalTransform,
    dir: Vec3,
    eye: Vec3,
}

impl View<'_> {
    fn proj(&self, p: Vec3) -> Option<Vec2> {
        self.camera.world_to_viewport(self.gtf, p).ok()
    }
}

struct Frame {
    o: Vec3,
    axes: [Vec3; 3],
    hs: f32,
    eye: Vec3,
}

fn plane_uv(i: usize) -> (usize, usize) {
    match i {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    }
}

fn plane_corners(f: &Frame, i: usize) -> [Vec3; 4] {
    let (u, v) = plane_uv(i);
    let p = |a: f32, b: f32| f.o + (f.axes[u] * a + f.axes[v] * b) * f.hs;
    [p(PLANE_A, PLANE_A), p(PLANE_B, PLANE_A), p(PLANE_B, PLANE_B), p(PLANE_A, PLANE_B)]
}

fn ring_points(f: &Frame, i: usize, n: usize) -> Vec<Vec3> {
    let (e1, e2) = (f.axes[(i + 1) % 3], f.axes[(i + 2) % 3]);
    (0..=n)
        .map(|k| {
            let a = k as f32 / n as f32 * TAU;
            f.o + (e1 * a.cos() + e2 * a.sin()) * f.hs * RING_R
        })
        .collect()
}

fn axis_t_screen(view: &View, o: Vec3, a: Vec3, hs: f32, cursor: Vec2) -> Option<f32> {
    let (p0, p1) = (view.proj(o)?, view.proj(o + a * hs)?);
    let d = p1 - p0;
    let l2 = d.length_squared();
    (l2 > 1e-6).then(|| (cursor - p0).dot(d) / l2 * hs)
}

/// Axis parameter of the cursor ray; screen-space fallback inside the dead zone.
fn axis_t(view: &View, o: Vec3, a: Vec3, hs: f32, ray: Ray3d, cursor: Vec2) -> Option<f32> {
    axis_hit_param(o, a, view.dir, ray.origin, *ray.direction).or_else(|| axis_t_screen(view, o, a, hs, cursor))
}

fn pick(view: &View, cursor: Vec2, mode: GizmoMode, f: &Frame) -> Option<Handle> {
    let axis_pick = |len: f32, make: fn(usize) -> Handle| -> Option<Handle> {
        let p0 = view.proj(f.o)?;
        (0..3)
            .filter_map(|i| {
                let p1 = view.proj(f.o + f.axes[i] * len * f.hs)?;
                let d = dist_point_segment(cursor, p0, p1);
                (d < PICK_PX).then_some((d, make(i)))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, h)| h)
    };
    let centre = view.proj(f.o).is_some_and(|p| p.distance(cursor) < PICK_PX + 2.0);
    match mode {
        GizmoMode::Move => {
            if centre {
                return Some(Handle::Center);
            }
            axis_pick(AX_LEN, Handle::Axis).or_else(|| {
                (0..3).find(|&i| {
                    let c = plane_corners(f, i).map(|p| view.proj(p));
                    matches!(c, [Some(a), Some(b), Some(c), Some(d)] if point_in_quad(cursor, [a, b, c, d]))
                })
                .map(Handle::Plane)
            })
        }
        GizmoMode::Rotate => (0..3)
            .filter_map(|i| {
                let pts: Vec<Vec2> = ring_points(f, i, 64).into_iter().filter_map(|p| view.proj(p)).collect();
                let d = pts.windows(2).map(|w| dist_point_segment(cursor, w[0], w[1])).fold(f32::MAX, f32::min);
                (d < PICK_PX).then_some((d, Handle::Ring(i)))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, h)| h),
        GizmoMode::Scale => {
            if centre {
                return Some(Handle::ScaleAll);
            }
            axis_pick(SCALE_LEN, Handle::ScaleAxis)
        }
        GizmoMode::Select => None,
    }
}

// ───────────────────────────── hotkeys ─────────────────────────────

fn gizmo_hotkeys(keys: Res<ButtonInput<KeyCode>>, mouse: Res<ButtonInput<MouseButton>>, mut gz: ResMut<GizmoState>, mut egui: EguiContexts) {
    let typing = egui.ctx_mut().map(|c| c.egui_wants_keyboard_input()).unwrap_or(false);
    // RMB held = fly camera (WASD/QE belong to the camera)
    if typing || mouse.pressed(MouseButton::Right) || gz.drag.is_some() || keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]) {
        return;
    }
    for (k, m) in [(KeyCode::KeyQ, GizmoMode::Select), (KeyCode::KeyW, GizmoMode::Move), (KeyCode::KeyE, GizmoMode::Rotate), (KeyCode::KeyR, GizmoMode::Scale)] {
        if keys.just_pressed(k) {
            gz.mode = m;
        }
    }
    if keys.just_pressed(KeyCode::KeyX) {
        gz.space = if gz.space == GizmoSpace::World { GizmoSpace::Local } else { GizmoSpace::World };
    }
}

// ───────────────────────────── drag ─────────────────────────────

fn states_of(list: &[(Entity, Transform)]) -> Vec<ObjState> {
    list.iter().map(|(e, t)| ObjState { entity: *e, tf: *t, alive: true }).collect()
}

fn start_drag(handle: Handle, view: &View, f: &Frame, ray: Ray3d, cursor: Vec2, starts: Vec<(Entity, Transform)>, grid: &HeightfieldGrid) -> Option<Drag> {
    let (ro, rd) = (ray.origin, *ray.direction);
    let mut d = Drag {
        handle,
        starts,
        origin: f.o,
        axes: f.axes,
        normal: Vec3::Y,
        hs: f.hs,
        grab_t: 0.0,
        grab_pt: f.o,
        grab_ground: grid.raycast(ro, rd),
        v_start: Vec3::X,
        tangent_mode: false,
        tangent: Vec2::X,
        acc: 0.0,
        prev_raw: None,
        last_mouse: cursor,
        mouse0: cursor,
        p0: view.proj(f.o)?,
        last_delta: Vec3::ZERO,
        last: Vec::new(),
    };
    match handle {
        Handle::Axis(i) | Handle::ScaleAxis(i) => d.grab_t = axis_t(view, f.o, f.axes[i], f.hs, ray, cursor)?,
        Handle::Plane(_) | Handle::Center => {
            d.normal = if let Handle::Plane(i) = handle { f.axes[i] } else { -view.dir };
            d.grab_pt = plane_drag_point(f.o, d.normal, view.dir, ro, rd)?;
        }
        Handle::Ring(i) => {
            let a = f.axes[i];
            if let Some(p) = plane_drag_point(f.o, a, view.dir, ro, rd) {
                d.v_start = (p - f.o).normalize_or_zero();
            }
            d.tangent_mode = a.dot(view.dir).abs() < 0.08 || d.v_start == Vec3::ZERO;
            if d.tangent_mode {
                let pts = ring_points(f, i, 64);
                let near = (0..pts.len() - 1)
                    .filter_map(|k| Some((dist_point_segment(cursor, view.proj(pts[k])?, view.proj(pts[k + 1])?), k)))
                    .min_by(|a, b| a.0.total_cmp(&b.0))
                    .map(|(_, k)| k)
                    .unwrap_or(0);
                if let (Some(p0), Some(p1)) = (view.proj(pts[near]), view.proj(pts[near + 1])) {
                    d.tangent = (p1 - p0).normalize_or_zero();
                }
            }
        }
        Handle::ScaleAll => {}
    }
    Some(d)
}

/// `snap`: level-builder grid steps `(move step in metres, rotate step in radians)`; they replace the Ctrl step while the grid is on.
#[allow(clippy::too_many_arguments)]
fn drag_update(d: &mut Drag, view: &View, ray: Ray3d, cursor: Vec2, ctrl: bool, snap_ground: bool, align: Option<f32>, grid: &HeightfieldGrid, snap: (Option<f32>, Option<f32>)) -> Vec<(Entity, Transform)> {
    let (ro, rd) = (ray.origin, *ray.direction);
    let o = d.origin;
    let mut out = Vec::with_capacity(d.starts.len());
    match d.handle {
        Handle::Axis(_) | Handle::Plane(_) | Handle::Center => {
            let mut vertical = false;
            match d.handle {
                Handle::Axis(i) => {
                    let a = d.axes[i];
                    vertical = a.y.abs() > 0.7;
                    let step = ctrl.then_some(MOVE_SNAP);
                    let t = axis_drag_delta(o, a, view.dir, ro, rd, d.grab_t, step).or_else(|| {
                        axis_t_screen(view, o, a, d.hs, cursor).map(|t| {
                            let t = t - d.grab_t;
                            step.map_or(t, |s| (t / s).round() * s)
                        })
                    });
                    if let Some(t) = t {
                        d.last_delta = a * t;
                    }
                }
                _ => {
                    // grazing planes keep the previous delta
                    if let Some(p) = plane_drag_point(o, d.normal, view.dir, ro, rd) {
                        d.last_delta = p - d.grab_pt;
                    }
                    if snap_ground {
                        if let (Some(h), Some(g0)) = (grid.raycast(ro, rd), d.grab_ground) {
                            d.last_delta = Vec3::new(h.x - g0.x, 0.0, h.z - g0.z);
                        }
                    }
                }
            }
            for (e, s) in &d.starts {
                let mut tf = *s;
                tf.translation += d.last_delta;
                if let Some(step) = snap.0 {
                    // only axes the drag actually moved are rounded, so a pure X drag never shifts Z
                    for c in [0, 2] {
                        if d.last_delta[c].abs() > 1e-4 {
                            tf.translation[c] = (tf.translation[c] / step).round() * step;
                        }
                    }
                }
                if snap_ground && !vertical {
                    let (x, z) = (tf.translation.x, tf.translation.z);
                    tf.translation.y = grid.sample_height(x, z);
                    if let Some(w) = align {
                        tf.rotation = surface_align_rotation(grid.sample_normal(x, z), yaw_of(s.rotation), w);
                    }
                }
                out.push((*e, tf));
            }
        }
        Handle::Ring(i) => {
            let a = d.axes[i];
            if d.tangent_mode {
                d.acc += (cursor - d.last_mouse).dot(d.tangent) * 0.5_f32.to_radians();
            } else if let Some(raw) = disc_rotation_angle(o, a, view.dir, ro, rd, d.v_start, None) {
                if let Some(pr) = d.prev_raw {
                    d.acc += wrap_angle(raw - pr);
                }
                d.prev_raw = Some(raw);
            }
            let angle = if let Some(st) = snap.1 {
                (d.acc / st).round() * st
            } else if ctrl {
                let st = ROT_SNAP_DEG.to_radians();
                (d.acc / st).round() * st
            } else {
                d.acc
            };
            let q = Quat::from_axis_angle(a, angle);
            for (e, s) in &d.starts {
                let tf = Transform { translation: o + q * (s.translation - o), rotation: (q * s.rotation).normalize(), scale: s.scale };
                out.push((*e, tf));
            }
        }
        Handle::ScaleAxis(_) | Handle::ScaleAll => {
            let (axis, f) = match d.handle {
                Handle::ScaleAxis(i) => (Some(i), axis_t(view, o, d.axes[i], d.hs, ray, cursor).map_or(1.0, |t| scale_factor(t, d.grab_t))),
                _ => (None, scale_factor(cursor.distance(d.p0), d.mouse0.distance(d.p0))),
            };
            let mut k = Vec3::ONE;
            match axis {
                Some(i) => k[i] = f,
                None => k = Vec3::splat(f),
            }
            for (e, s) in &d.starts {
                out.push((*e, Transform { scale: s.scale * k, ..*s }));
            }
        }
    }
    d.last_mouse = cursor;
    out
}

// ───────────────────────────── drawing ─────────────────────────────

const LIT: (f32, f32, f32) = (2.6, 2.2, 0.5);

/// Axis colour; `lit` = hovered/dragged, `dim` = another handle has focus, `a` scales alpha.
fn axis_color(i: usize, lit: bool, dim: bool, a: f32) -> Color {
    let (r, g, b) = if lit {
        LIT
    } else {
        match i {
            0 => (2.2, 0.35, 0.35),
            1 => (0.45, 2.0, 0.45),
            2 => (0.45, 0.95, 2.4),
            _ => (2.0, 2.0, 2.0),
        }
    };
    let alpha = if lit { 1.0 } else if dim { 0.3 } else { 0.9 } * a;
    Color::linear_rgba(r, g, b, alpha)
}

fn draw_gizmo(g: &mut Gizmos<HandleGizmos>, mode: GizmoMode, f: &Frame, hover: Option<Handle>, drag: Option<&Drag>) {
    let active = drag.map(|d| d.handle);
    let focus = active.or(hover);
    let lit = |h: Handle| focus == Some(h);
    let dim = focus.is_some();
    let col = |i: usize, h: Handle, a: f32| axis_color(i, lit(h), dim && !lit(h), a);
    let o = f.o;
    let hs = f.hs;
    let to_eye = (f.eye - o).normalize_or_zero();
    match mode {
        GizmoMode::Move => {
            const N: usize = 12;
            for i in 0..3 {
                let a = f.axes[i];
                let c = col(i, Handle::Axis(i), 1.0);
                let base = o + a * (AX_LEN - CONE_LEN) * hs;
                let tip = o + a * AX_LEN * hs;
                g.line(o + a * 0.16 * hs, base, c);
                let (u, v) = (f.axes[(i + 1) % 3], f.axes[(i + 2) % 3]);
                let rim: Vec<Vec3> = (0..=N).map(|k| { let t = k as f32 / N as f32 * TAU; base + (u * t.cos() + v * t.sin()) * CONE_R * hs }).collect();
                for p in rim.iter().take(N).step_by(3) {
                    g.line(tip, *p, c);
                }
                g.linestrip(rim, c);
                // plane handle: thin outline plus one diagonal
                let q = plane_corners(f, i);
                let pc = col(i, Handle::Plane(i), 0.8);
                g.linestrip([q[0], q[1], q[2], q[3], q[0]], pc);
                if lit(Handle::Plane(i)) {
                    g.line(q[0], q[2], pc);
                    g.line(q[1], q[3], pc);
                }
            }
            g.sphere(o, 0.06 * hs, axis_color(3, lit(Handle::Center), dim && !lit(Handle::Center), 1.0));
        }
        GizmoMode::Rotate => {
            for i in 0..3 {
                let h = Handle::Ring(i);
                let pts = ring_points(f, i, 64);
                // far half of the ring fades so the front arc reads clearly
                for w in pts.windows(2) {
                    let front = ((w[0] + w[1]) * 0.5 - o).dot(to_eye) >= 0.0;
                    g.line(w[0], w[1], col(i, h, if front { 1.0 } else { 0.35 }));
                }
                if let Some(d) = drag.filter(|d| d.handle == h && !d.tangent_mode) {
                    let w = f.axes[i].cross(d.v_start);
                    let n = 24;
                    let arc: Vec<Vec3> = (0..=n).map(|k| { let t = d.acc * k as f32 / n as f32; o + (d.v_start * t.cos() + w * t.sin()) * hs * RING_R }).collect();
                    let fill = Color::linear_rgba(LIT.0, LIT.1, LIT.2, 0.45);
                    for p in arc.iter().step_by(3) {
                        g.line(o, *p, fill);
                    }
                    g.linestrip(arc, axis_color(i, true, false, 1.0));
                }
            }
            g.sphere(o, 0.04 * hs, axis_color(3, false, dim, 1.0));
        }
        GizmoMode::Scale => {
            let rot = Quat::from_mat3(&Mat3::from_cols(f.axes[0], f.axes[1], f.axes[2]));
            for i in 0..3 {
                let c = col(i, Handle::ScaleAxis(i), 1.0);
                let end = o + f.axes[i] * SCALE_LEN * hs;
                g.line(o + f.axes[i] * 0.16 * hs, end, c);
                draw_box(g, end, Vec3::splat(CUBE * hs), rot, c);
            }
            draw_box(g, o, Vec3::splat(CUBE * 0.8 * hs), rot, axis_color(3, lit(Handle::ScaleAll), dim && !lit(Handle::ScaleAll), 1.0));
        }
        GizmoMode::Select => {}
    }
}

// ───────────────────────────── main system ─────────────────────────────

#[allow(clippy::too_many_arguments)]
pub fn gizmo_system(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut egui: EguiContexts,
    cam: Single<(&Camera, &GlobalTransform, &Projection, &EditorCamera)>,
    terrain: Res<TerrainGrid>,
    sel: Res<EditorSelection>,
    mut gz: ResMut<GizmoState>,
    mut hist: ResMut<SceneHistory>,
    mut objs: Query<(&mut Transform, &SceneObject), Without<Deleted>>,
    mut gizmos: Gizmos<HandleGizmos>,
    studio: Res<StudioState>,
    gset: Res<crate::level_builder::GridSettings>,
) {
    let blocked = egui.ctx_mut().map(|c| c.is_pointer_over_egui() || c.egui_wants_pointer_input()).unwrap_or(false);
    let (camera, gtf, proj, ecam) = *cam;
    let fov = if let Projection::Perspective(p) = proj { p.fov } else { 1.0 };
    let view = View { camera, gtf, dir: *gtf.forward(), eye: gtf.translation() };
    let alt = keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]);
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let ctrl = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    let grid = &terrain.0;

    let primary = sel.primary.filter(|e| objs.contains(*e));
    let Some(primary) = primary.filter(|_| gz.mode != GizmoMode::Select && studio.gizmo_active()) else {
        if let Some(d) = gz.drag.take() {
            for (e, t) in &d.starts {
                if let Ok((mut tf, _)) = objs.get_mut(*e) {
                    *tf = *t;
                }
            }
            hist.cancel();
        }
        gz.hover = None;
        return;
    };
    let (origin, prot) = objs.get(primary).map(|(t, _)| (t.translation, t.rotation)).unwrap_or_default();
    let space = if gz.mode == GizmoMode::Scale { GizmoSpace::Local } else { gz.space };
    let axes = match space {
        GizmoSpace::World => [Vec3::X, Vec3::Y, Vec3::Z],
        GizmoSpace::Local => [prot * Vec3::X, prot * Vec3::Y, prot * Vec3::Z],
    };
    let hs = (calc_handle_size(view.eye, origin, view.dir, fov) * HANDLE_SCALE).max(1e-3);
    let frame = Frame { o: origin, axes, hs, eye: view.eye };
    let cursor = window.cursor_position();
    let ray = cursor.and_then(|c| camera.viewport_to_world(gtf, c).ok());
    let cam_busy = ecam.mode.is_some() || alt;

    if gz.drag.is_none() {
        gz.hover = match (cursor, blocked || cam_busy) {
            (Some(c), false) => pick(&view, c, gz.mode, &frame),
            _ => None,
        };
        if mouse.just_pressed(MouseButton::Left) {
            if let (Some(h), Some(c), Some(r)) = (gz.hover, cursor, ray) {
                let starts: Vec<(Entity, Transform)> = sel.selected.iter().filter_map(|e| objs.get(*e).ok().map(|(t, _)| (*e, *t))).collect();
                if let Some(d) = start_drag(h, &view, &frame, r, c, starts, grid) {
                    let label = match gz.mode {
                        GizmoMode::Rotate => "Rotate",
                        GizmoMode::Scale => "Scale",
                        _ => "Move",
                    };
                    hist.begin(label, states_of(&d.starts));
                    gz.drag = Some(d);
                }
            }
        }
    }

    if let Some(mut d) = gz.drag.take() {
        if keys.just_pressed(KeyCode::Escape) {
            for (e, t) in &d.starts {
                if let Ok((mut tf, _)) = objs.get_mut(*e) {
                    *tf = *t;
                }
            }
            hist.cancel();
        } else {
            if let (Some(c), Some(r)) = (cursor, ray) {
                let align = gz.align_normal.then_some(gz.align_strength);
                let res = drag_update(&mut d, &view, r, c, ctrl, shift || gz.snap_ground, align, grid, (gset.move_step(ctrl), gset.rot_step_rad()));
                for (e, t) in &res {
                    if let Ok((mut tf, _)) = objs.get_mut(*e) {
                        *tf = *t;
                    }
                }
                d.last = states_of(&res);
                hist.update_preview(d.last.clone());
            }
            if mouse.pressed(MouseButton::Left) {
                gz.drag = Some(d);
            } else {
                let fin = if d.last.is_empty() { states_of(&d.starts) } else { d.last.clone() };
                hist.commit(fin);
            }
        }
    }

    let hover = gz.hover;
    let mode = gz.mode;
    draw_gizmo(&mut gizmos, mode, &frame, hover, gz.drag.as_ref());
}

// ───────────────────────────── duplicate / delete / undo ─────────────────────────────

pub fn apply_states(states: &[ObjState], q: &mut Query<(&mut Transform, &mut Visibility)>, commands: &mut Commands) {
    for s in states {
        let Ok((mut tf, mut vis)) = q.get_mut(s.entity) else { continue };
        *tf = s.tf;
        *vis = if s.alive { Visibility::Inherited } else { Visibility::Hidden };
        if s.alive {
            commands.entity(s.entity).try_remove::<Deleted>();
        } else {
            commands.entity(s.entity).try_insert(Deleted);
        }
    }
}

/// Objects copied with Ctrl+C / `Copy`; `pastes` staggers repeated pastes so they do not stack.
#[derive(Resource, Default)]
pub struct ObjectClipboard {
    pub sources: Vec<Entity>,
    pub pastes: u32,
}

/// On a freshly cloned root: its cloned descendants still carry the source ids.
#[derive(Component)]
pub struct FreshIds;

/// Selected entities with no selected ancestor (their descendants travel with them).
pub fn top_level(sel: &EditorSelection, parents: &Query<&ChildOf>) -> Vec<Entity> {
    let mut v: Vec<Entity> = sel
        .selected
        .iter()
        .copied()
        .filter(|e| {
            let mut cur = *e;
            while let Ok(p) = parents.get(cur) {
                cur = p.parent();
                if sel.selected.contains(&cur) {
                    return false;
                }
            }
            true
        })
        .collect();
    v.sort_by_key(|e| e.to_bits());
    v
}

/// Clone `sources` with all descendants, shifted by `stagger` times their footprint. Returns (before, after, new roots).
pub fn clone_objects(
    commands: &mut Commands,
    grid: &HeightfieldGrid,
    sources: &[(Entity, Transform, SceneObject, bool)],
    stagger: f32,
    next_id: &mut u32,
) -> (Vec<ObjState>, Vec<ObjState>, Vec<Entity>) {
    let (mut before, mut after, mut fresh) = (Vec::new(), Vec::new(), Vec::new());
    for (src, tf, so, is_child) in sources {
        let off = Vec3::new((so.aabb_half_extents.x * 2.0 + 1.0).clamp(1.5, 6.0) * stagger, 0.0, 0.0);
        let mut ntf = *tf;
        ntf.translation += off;
        if !is_child {
            let lift = tf.translation.y - grid.sample_height(tf.translation.x, tf.translation.z);
            ntf.translation.y = grid.sample_height(ntf.translation.x, ntf.translation.z) + lift;
        }
        *next_id += 1;
        let mut dup = so.clone();
        dup.id = *next_id;
        let id = commands
            .entity(*src)
            .clone_and_spawn_with_opt_out(|b| {
                b.linked_cloning(true);
            })
            .insert((ntf, dup, FreshIds))
            .id();
        before.push(ObjState { entity: id, tf: ntf, alive: false });
        after.push(ObjState { entity: id, tf: ntf, alive: true });
        fresh.push(id);
    }
    (before, after, fresh)
}

/// Gives cloned descendants unique object ids (the clone copies the source's).
fn refresh_cloned_ids(mut commands: Commands, roots: Query<Entity, With<FreshIds>>, kids: Query<&Children>, mut objs: Query<&mut SceneObject>) {
    if roots.is_empty() {
        return;
    }
    let mut next = objs.iter().map(|o| o.id).max().unwrap_or(0);
    for r in &roots {
        for d in kids.iter_descendants(r) {
            if let Ok(mut so) = objs.get_mut(d) {
                next += 1;
                so.id = next;
            }
        }
        commands.entity(r).remove::<FreshIds>();
    }
}

/// Live, editable roots among `ids`, ready for `clone_objects`.
pub fn clone_sources(ids: &[Entity], q: &Query<(&mut Transform, &mut Visibility)>, all: &Query<&SceneObject>, deleted: &Query<(), With<Deleted>>, parents: &Query<&ChildOf>) -> Vec<(Entity, Transform, SceneObject, bool)> {
    ids.iter()
        .filter(|e| !deleted.contains(**e))
        .filter_map(|e| Some((*e, *q.get(*e).ok()?.0, all.get(*e).ok()?.clone(), parents.contains(*e))))
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn scene_edit_hotkeys(
    keys: Res<ButtonInput<KeyCode>>,
    mut egui: EguiContexts,
    mut commands: Commands,
    mut sel: ResMut<EditorSelection>,
    mut hist: ResMut<SceneHistory>,
    mut clip: ResMut<ObjectClipboard>,
    gz: Res<GizmoState>,
    terrain: Res<TerrainGrid>,
    mut q: Query<(&mut Transform, &mut Visibility)>,
    all: Query<&SceneObject>,
    deleted: Query<(), With<Deleted>>,
    parents: Query<&ChildOf>,
) {
    if egui.ctx_mut().map(|c| c.egui_wants_keyboard_input()).unwrap_or(false) || gz.drag.is_some() {
        return;
    }
    let ctrl = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);

    if ctrl && keys.just_pressed(KeyCode::KeyC) && !sel.selected.is_empty() {
        clip.sources = top_level(&sel, &parents);
        clip.pastes = 0;
    }

    let paste = ctrl && keys.just_pressed(KeyCode::KeyV) && !clip.sources.is_empty();
    let dup = ctrl && keys.just_pressed(KeyCode::KeyD) && !sel.selected.is_empty();
    if paste || dup {
        let ids = if paste { clip.sources.clone() } else { top_level(&sel, &parents) };
        let stagger = if paste {
            clip.pastes += 1;
            clip.pastes as f32
        } else {
            1.0
        };
        let srcs = clone_sources(&ids, &q, &all, &deleted, &parents);
        let mut next_id = all.iter().map(|o| o.id).max().unwrap_or(0);
        let (before, after, fresh) = clone_objects(&mut commands, &terrain.0, &srcs, stagger, &mut next_id);
        if let Some(first) = fresh.first().copied() {
            hist.record(if paste { "Paste" } else { "Duplicate" }, before, after);
            sel.selected = fresh.into_iter().collect();
            sel.primary = Some(first);
        }
    }

    if keys.just_pressed(KeyCode::Delete) && !sel.selected.is_empty() {
        let (mut before, mut after) = (Vec::new(), Vec::new());
        for e in sel.selected.iter().copied() {
            let Ok((tf, _)) = q.get(e) else { continue };
            before.push(ObjState { entity: e, tf: *tf, alive: true });
            after.push(ObjState { entity: e, tf: *tf, alive: false });
        }
        if hist.record("Delete", before, after.clone()) {
            apply_states(&after, &mut q, &mut commands);
            sel.clear();
        }
    }
}

#[cfg(test)]
mod clone_tests {
    use super::*;
    use crate::props::ObjectCategory;

    fn obj(id: u32) -> SceneObject {
        SceneObject { id, name: format!("o{id}"), category: ObjectCategory::Model, aabb_half_extents: Vec3::ONE }
    }

    #[derive(Resource, Default)]
    struct Fresh(Vec<Entity>);

    fn run_clone(mut commands: Commands, q: Query<(Entity, &Transform, &SceneObject, Has<ChildOf>)>, mut fresh: ResMut<Fresh>) {
        let grid = HeightfieldGrid::new(5, 10.0);
        let srcs: Vec<_> = q.iter().filter(|o| !o.3).map(|(e, t, s, c)| (e, *t, s.clone(), c)).collect();
        let mut next = 2;
        fresh.0 = clone_objects(&mut commands, &grid, &srcs, 1.0, &mut next).2;
    }

    #[test]
    fn clone_carries_descendants_with_unique_ids() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).init_resource::<Fresh>().add_systems(Update, refresh_cloned_ids);
        let parent = app.world_mut().spawn((Transform::default(), obj(1))).id();
        app.world_mut().spawn((Transform::from_xyz(1.0, 0.0, 0.0), obj(2), ChildOf(parent)));
        { let id = app.world_mut().register_system(run_clone); app.world_mut().run_system(id).unwrap(); }
        app.update();
        app.update();
        let w = app.world_mut();
        let mut ids: Vec<u32> = w.query::<&SceneObject>().iter(w).map(|o| o.id).collect();
        assert_eq!(ids.len(), 4, "parent + child, cloned");
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 4, "ids are unique");
        let root = app.world().resource::<Fresh>().0[0];
        assert_eq!(app.world().get::<Children>(root).map(|c| c.len()), Some(1));
        assert!(app.world().get::<FreshIds>(root).is_none());
    }
}
