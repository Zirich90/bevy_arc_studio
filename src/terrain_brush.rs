//! Interactive terrain brushes: 3D cursor ring on the relief, live sculpting, pine painting, prop erasing.

use std::f32::consts::TAU;

use bevy::gizmos::prelude::*;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::EguiContexts;
use serde::{Deserialize, Serialize};

use crate::arc_core::*;
use crate::camera::EditorCamera;
use crate::gizmo_3d::{ObjState, SceneHistory};
use crate::props::SceneObject;
use crate::selection::{Deleted, SelectionGizmos};
use crate::studio::{Spawner, StudioState, TerrainHistory};
use crate::terrain_render::{TerrainGrid, TerrainMesh, update_terrain_region};

#[derive(Reflect, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum BrushKind {
    #[default]
    Raise,
    Lower,
    Smooth,
    Flatten,
    PaintPines,
    EraseProps,
}

impl BrushKind {
    pub const ALL: [BrushKind; 6] = [Self::Raise, Self::Lower, Self::Smooth, Self::Flatten, Self::PaintPines, Self::EraseProps];

    pub fn label(self) -> &'static str {
        match self {
            Self::Raise => "Raise",
            Self::Lower => "Lower",
            Self::Smooth => "Smooth",
            Self::Flatten => "Flatten",
            Self::PaintPines => "Paint Pines",
            Self::EraseProps => "Erase Props",
        }
    }

    pub fn mode(self) -> Option<BrushMode> {
        match self {
            Self::Raise => Some(BrushMode::Raise),
            Self::Lower => Some(BrushMode::Lower),
            Self::Smooth => Some(BrushMode::Smooth),
            Self::Flatten => Some(BrushMode::Flatten),
            _ => None,
        }
    }

    fn color(self) -> Color {
        match self {
            Self::Raise => Color::linear_rgb(0.4, 3.2, 1.2),
            Self::Lower => Color::linear_rgb(3.4, 1.2, 0.3),
            Self::Smooth => Color::linear_rgb(0.5, 2.4, 3.4),
            Self::Flatten => Color::linear_rgb(3.0, 2.6, 0.5),
            Self::PaintPines => Color::linear_rgb(0.5, 3.0, 0.5),
            Self::EraseProps => Color::linear_rgb(3.4, 0.4, 0.4),
        }
    }
}

#[derive(Resource, Reflect, Debug)]
#[reflect(Resource)]
pub struct BrushState {
    pub kind: BrushKind,
    pub radius: f32,
    pub strength: f32,
    pub hover: Option<Vec3>,
}

impl Default for BrushState {
    fn default() -> Self {
        Self { kind: BrushKind::Raise, radius: 12.0, strength: 6.0, hover: None }
    }
}

/// One height-brush dab (shared by mouse sculpting and the MCP `Sculpt` command). Returns the dirty rect.
#[allow(clippy::too_many_arguments)]
pub fn height_dab(grid: &mut HeightfieldGrid, mode: BrushMode, x: f32, z: f32, radius: f32, strength: f32, dt: f32, target: f32, max_h: f32) -> (usize, usize, usize, usize) {
    // Raise / Lower integrate strength·dt (m/s); Smooth / Flatten blend by a per-frame factor.
    let s = if matches!(mode, BrushMode::Smooth | BrushMode::Flatten) { strength * dt * 0.8 } else { strength };
    let rect = grid.apply_brush(mode, x, z, radius, s, dt, target);
    for j in rect.1..=rect.3 {
        for i in rect.0..=rect.2 {
            let k = grid.idx(i, j);
            grid.heights[k] = grid.heights[k].clamp(0.0, max_h);
        }
    }
    rect
}

pub fn union_rect(a: Option<(usize, usize, usize, usize)>, b: (usize, usize, usize, usize)) -> (usize, usize, usize, usize) {
    a.map_or(b, |a| (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)))
}

pub struct TerrainBrushPlugin;

impl Plugin for TerrainBrushPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BrushState>().register_type::<BrushState>().register_type::<BrushKind>();
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct Hists<'w> {
    th: ResMut<'w, TerrainHistory>,
    sh: ResMut<'w, SceneHistory>,
}

#[derive(Default)]
pub(crate) struct StrokeData {
    active: bool,
    before: Option<Vec<f32>>,
    target: f32,
    timer: f32,
    placed: Vec<Vec2>,
    spawned: Vec<(Entity, Transform)>,
    erased: Vec<(Entity, Transform)>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn brush_system(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut egui: EguiContexts,
    cam: Single<(&Camera, &GlobalTransform, &EditorCamera)>,
    studio: Res<StudioState>,
    spec: Res<WorldSpec>,
    mut brush: ResMut<BrushState>,
    mut grid: ResMut<TerrainGrid>,
    mut hists: Hists,
    terrain: Query<&Mesh3d, With<TerrainMesh>>,
    objs: Query<(Entity, &Transform, &SceneObject), Without<Deleted>>,
    mut spawner: Spawner,
    mut gizmos: Gizmos<SelectionGizmos>,
    mut st: Local<StrokeData>,
) {
    let active = studio.brush_active();
    let ui_blocked = egui.ctx_mut().map(|c| c.is_pointer_over_egui() || c.egui_wants_pointer_input()).unwrap_or(false);
    let alt = keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]);
    let (camera, gtf, ecam) = *cam;
    let hit = if active && !(ui_blocked && !st.active) && ecam.mode.is_none() && !alt {
        window.cursor_position().and_then(|c| camera.viewport_to_world(gtf, c).ok()).and_then(|r| grid.0.raycast(r.origin, *r.direction))
    } else {
        None
    };
    brush.hover = hit;
    let kind = brush.kind;

    // 3D ring on the relief under the cursor
    if let Some(h) = hit {
        let col = kind.color();
        let seg = 72;
        for (k, (rr, c)) in [(brush.radius, col), (brush.radius * 0.5, col.with_alpha(0.45))].into_iter().enumerate() {
            let pts: Vec<Vec3> = (0..=seg)
                .map(|n| {
                    let a = n as f32 / seg as f32 * TAU;
                    let (x, z) = (h.x + a.cos() * rr, h.z + a.sin() * rr);
                    Vec3::new(x, grid.0.sample_height(x, z) + 0.25 + 0.05 * k as f32, z)
                })
                .collect();
            gizmos.linestrip(pts, c);
        }
        gizmos.line(h, h + Vec3::Y * 2.5, col);
    }

    let pressed = mouse.pressed(MouseButton::Left);
    if active && hit.is_some() && mouse.just_pressed(MouseButton::Left) && !st.active && !ui_blocked {
        *st = StrokeData { active: true, target: hit.map_or(0.0, |h| h.y), ..default() };
        if kind.mode().is_some() {
            st.before = Some(grid.0.heights.clone());
        }
    }

    if st.active && pressed && active {
        if let Some(h) = hit {
            let dt = time.delta_secs().min(0.05);
            if let Some(mode) = kind.mode() {
                let rect = height_dab(&mut grid.0, mode, h.x, h.z, brush.radius, brush.strength, dt, st.target, spec.max_height);
                if let Ok(m) = terrain.single() {
                    if let Some(mut mesh) = spawner.meshes.get_mut(&m.0) {
                        update_terrain_region(&mut mesh, &grid.0, &spec, rect);
                    }
                }
            } else if kind == BrushKind::PaintPines {
                st.timer += dt;
                while st.timer > 0.10 {
                    st.timer -= 0.10;
                    let (a, rr) = (rand::random::<f32>() * TAU, brush.radius * rand::random::<f32>().sqrt());
                    let (x, z) = (h.x + a.cos() * rr, h.z + a.sin() * rr);
                    let spot = Vec2::new(x, z);
                    if grid.0.sample_height(x, z) < spec.water_level + 0.3 || grid.0.sample_normal(x, z).y < 0.75 || st.placed.iter().any(|p| p.distance(spot) < 2.6) {
                        continue;
                    }
                    if let Some(sp) = spawner.spawn(&grid.0, &spec, "pine", x, z, rand::random::<f32>() * TAU) {
                        st.placed.push(spot);
                        st.spawned.push(sp);
                    }
                }
            } else {
                for (e, tf, _) in &objs {
                    if Vec2::new(tf.translation.x - h.x, tf.translation.z - h.z).length() < brush.radius && !st.erased.iter().any(|(x, _)| *x == e) {
                        spawner.commands.entity(e).insert((Deleted, Visibility::Hidden));
                        st.erased.push((e, *tf));
                    }
                }
            }
        }
    }

    if st.active && (!pressed || !active) {
        let s = std::mem::take(&mut *st);
        if let Some(before) = s.before {
            hists.th.record("Sculpt", before, grid.0.heights.clone());
        }
        if !s.spawned.is_empty() {
            let b = s.spawned.iter().map(|(e, tf)| ObjState { entity: *e, tf: *tf, alive: false }).collect();
            let a = s.spawned.iter().map(|(e, tf)| ObjState { entity: *e, tf: *tf, alive: true }).collect();
            hists.sh.record("Paint Pines", b, a);
        }
        if !s.erased.is_empty() {
            let b = s.erased.iter().map(|(e, tf)| ObjState { entity: *e, tf: *tf, alive: true }).collect();
            let a = s.erased.iter().map(|(e, tf)| ObjState { entity: *e, tf: *tf, alive: false }).collect();
            hists.sh.record("Erase Props", b, a);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raise_then_lower_stays_clamped_and_dirty_rect_covers_dab() {
        let mut g = HeightfieldGrid::new(65, 64.0);
        let rect = height_dab(&mut g, BrushMode::Raise, 0.0, 0.0, 6.0, 4.0, 0.5, 0.0, 10.0);
        let c = g.world_to_grid(0.0, 0.0);
        assert!(rect.0 as f32 <= c.0 && rect.2 as f32 >= c.0);
        assert!(g.sample_height(0.0, 0.0) > 1.0);
        for _ in 0..50 {
            height_dab(&mut g, BrushMode::Lower, 0.0, 0.0, 6.0, 40.0, 0.5, 0.0, 10.0);
        }
        assert!(g.heights.iter().all(|h| (0.0..=10.0).contains(h)));
        assert_eq!(union_rect(Some((1, 1, 2, 2)), (0, 3, 1, 5)), (0, 1, 2, 5));
    }
}
