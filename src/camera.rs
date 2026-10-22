//! Editor camera (doc 01): orbit, depth pan, exponential zoom, fly mode, focus.

use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::EguiContexts;

use crate::scene::TerrainGrid;

const K_ROT: f32 = 0.0052;
const ZOOM_RATE: f32 = 0.15;
const PITCH_LIM: f32 = 89.0 * std::f32::consts::PI / 180.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Reflect)]
pub enum CamMode {
    Orbit,
    Pan,
    Fly,
}

#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct EditorCamera {
    pub yaw: f32,
    pub pitch: f32,
    pub pivot: Vec3,
    pub distance: f32,
    /// Base fly speed v0 (m/s).
    pub fly_speed: f32,
    /// Shift ramp: a = accel + 1.
    pub fly_accel: f32,
    pub v_target: f32,
    pub vel: Vec3,
    pub mode: Option<CamMode>,
    pub focus: Option<(Vec3, f32)>,
}

impl Default for EditorCamera {
    fn default() -> Self {
        Self { yaw: 0.0, pitch: 0.3, pivot: Vec3::ZERO, distance: 30.0, fly_speed: 9.0, fly_accel: 1.5, v_target: 9.0, vel: Vec3::ZERO, mode: None, focus: None }
    }
}

impl EditorCamera {
    pub fn forward(&self) -> Vec3 {
        let (cp, sp) = (self.pitch.cos(), self.pitch.sin());
        Vec3::new(cp * self.yaw.sin(), -sp, cp * self.yaw.cos())
    }
    pub fn eye(&self) -> Vec3 {
        self.pivot - self.forward() * self.distance
    }
    /// Focus on a bounding sphere: distance = max(radius / sin(fov/2), 2).
    pub fn focus_on(&mut self, center: Vec3, radius: f32, fov_y: f32) {
        self.focus = Some((center, (radius / (fov_y * 0.5).sin()).max(2.0)));
    }
}

pub struct EditorCameraPlugin;

impl Plugin for EditorCameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, editor_camera_system);
    }
}

#[allow(clippy::too_many_arguments)]
fn editor_camera_system(
    time: Res<Time>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    terrain: Option<Res<TerrainGrid>>,
    mut egui: EguiContexts,
    window: Single<&Window, With<PrimaryWindow>>,
    mut cam: Single<(&mut EditorCamera, &mut Transform, &Camera, &Projection, &GlobalTransform)>,
) {
    let (ctrl, tf, camera, proj, gtf) = &mut *cam;
    let dt = time.delta_secs().min(0.1);
    let fov_y = if let Projection::Perspective(p) = proj { p.fov } else { 1.0 };
    let blocked = egui.ctx_mut().map(|c| c.egui_wants_pointer_input() || c.is_pointer_over_egui()).unwrap_or(false);
    let alt = keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]);
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let d = motion.delta;

    // mode selection: only starts when the pointer is not over egui; ends on release
    if !blocked {
        if mouse.just_pressed(MouseButton::Right) {
            ctrl.mode = Some(CamMode::Fly);
            ctrl.v_target = ctrl.fly_speed;
            ctrl.vel = Vec3::ZERO;
        } else if mouse.just_pressed(MouseButton::Middle) {
            ctrl.mode = Some(if shift { CamMode::Pan } else { CamMode::Orbit });
        } else if mouse.just_pressed(MouseButton::Left) && alt {
            ctrl.mode = Some(if shift { CamMode::Pan } else { CamMode::Orbit });
        }
    }
    let held = match ctrl.mode {
        Some(CamMode::Fly) => mouse.pressed(MouseButton::Right),
        Some(_) => mouse.pressed(MouseButton::Middle) || mouse.pressed(MouseButton::Left),
        None => false,
    };
    if !held {
        if ctrl.mode == Some(CamMode::Fly) {
            ctrl.pivot = ctrl.eye() + ctrl.forward() * ctrl.distance; // pivot carried along with the eye
        }
        ctrl.mode = None;
        ctrl.vel = Vec3::ZERO;
    }

    let fwd = ctrl.forward();
    let right = fwd.cross(Vec3::Y).normalize_or_zero();
    let up = right.cross(fwd);

    match ctrl.mode {
        Some(CamMode::Orbit) => {
            ctrl.yaw -= d.x * K_ROT;
            ctrl.pitch = (ctrl.pitch - d.y * K_ROT).clamp(-PITCH_LIM, PITCH_LIM);
        }
        Some(CamMode::Pan) => {
            let u = 2.0 * ctrl.distance * (fov_y * 0.5).tan() / window.height();
            ctrl.pivot += (-right * d.x + up * d.y) * u;
        }
        Some(CamMode::Fly) => {
            let eye = ctrl.eye();
            ctrl.yaw -= d.x * K_ROT;
            ctrl.pitch = (ctrl.pitch + d.y * K_ROT).clamp(-PITCH_LIM, PITCH_LIM); // FPS look: mouse up = look up
            let (fwd, right) = (ctrl.forward(), ctrl.forward().cross(Vec3::Y).normalize_or_zero());
            let ax = |p: KeyCode, n: KeyCode| keys.pressed(p) as i32 as f32 - keys.pressed(n) as i32 as f32;
            let wish = (fwd * ax(KeyCode::KeyW, KeyCode::KeyS) + right * ax(KeyCode::KeyD, KeyCode::KeyA) + Vec3::Y * ax(KeyCode::KeyE, KeyCode::KeyQ)).normalize_or_zero();
            if scroll.delta.y != 0.0 {
                ctrl.fly_speed = (ctrl.fly_speed * (scroll.delta.y * 0.1).exp()).clamp(0.1, 500.0);
            }
            ctrl.v_target = if shift { (ctrl.v_target * (ctrl.fly_accel + 1.0).powf(dt)).min(500.0) } else { ctrl.fly_speed };
            let target = wish * ctrl.v_target;
            ctrl.vel = ctrl.vel.lerp(target, 1.0 - (-dt / 0.08).exp());
            let new_eye = eye + ctrl.vel * dt;
            ctrl.pivot = new_eye + ctrl.forward() * ctrl.distance;
        }
        None => {}
    }

    // zoom (wheel), cursor-directed when the cursor ray hits the terrain
    if ctrl.mode != Some(CamMode::Fly) && !blocked && scroll.delta.y != 0.0 {
        let notches = match scroll.unit {
            MouseScrollUnit::Line => scroll.delta.y,
            MouseScrollUnit::Pixel => scroll.delta.y / 40.0,
        } * if shift { 3.0 } else { 1.0 };
        let hit = window
            .cursor_position()
            .and_then(|c| camera.viewport_to_world(*gtf, c).ok())
            .and_then(|ray| terrain.as_ref().and_then(|t| t.0.raycast(ray.origin, *ray.direction)));
        let eye = ctrl.eye();
        match hit {
            Some(p) => {
                let eye2 = eye + (p - eye) * (1.0 - (-notches * ZOOM_RATE).exp());
                ctrl.distance = (p - eye2).length().clamp(0.1, 5000.0);
                ctrl.pivot = eye2 + ctrl.forward() * ctrl.distance;
            }
            None => ctrl.distance = (ctrl.distance * (-notches * ZOOM_RATE).exp()).clamp(0.1, 5000.0),
        }
    }

    // focus (F): selection AABB is not available yet, so focus the point under the cursor (or current pivot)
    if keys.just_pressed(KeyCode::KeyF) && !blocked {
        let hit = window
            .cursor_position()
            .and_then(|c| camera.viewport_to_world(*gtf, c).ok())
            .and_then(|ray| terrain.as_ref().and_then(|t| t.0.raycast(ray.origin, *ray.direction)));
        let c = hit.unwrap_or(ctrl.pivot);
        ctrl.focus_on(c, 4.0, fov_y);
    }
    if let Some((tp, td)) = ctrl.focus {
        let s = 1.0 - (-dt / 0.12).exp();
        ctrl.pivot = ctrl.pivot.lerp(tp, s);
        ctrl.distance += (td - ctrl.distance) * s;
        if ctrl.pivot.distance(tp) < 1e-3 && (ctrl.distance - td).abs() < 1e-3 {
            ctrl.focus = None;
        }
    }

    **tf = Transform::from_translation(ctrl.eye()).looking_at(ctrl.pivot, Vec3::Y);
}
