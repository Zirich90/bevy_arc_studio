//! Trigger volumes: editor outline (Gizmos) + Play-mode enter/exit tracking of `Faction::Player` actors.

use std::collections::HashSet;
use std::f32::consts::FRAC_PI_2;

use bevy::gizmos::config::GizmoConfigStore;
use bevy::gizmos::prelude::*;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::{Actor, Faction, InteractionKind, Interactable};
use crate::props::{ObjectCategory, SceneObject};
use crate::selection::{Deleted, EditorSelection};
use crate::studio::{StudioMode, StudioState};

pub const DEFAULT_RADIUS: f32 = 3.0;
pub const DEFAULT_HEIGHT: f32 = 3.0;
/// Actors this far below the trigger's base still count as inside (terrain jitter).
const BASE_SLACK: f32 = 0.5;

#[derive(Reflect, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub enum TriggerShape {
    /// Upright cylinder whose axis starts at the entity position.
    Cylinder { radius: f32, height: f32 },
}

impl Default for TriggerShape {
    fn default() -> Self {
        Self::Cylinder { radius: DEFAULT_RADIUS, height: DEFAULT_HEIGHT }
    }
}

#[derive(Component, Reflect, Clone, Debug, PartialEq, Default)]
#[reflect(Component)]
#[require(TriggerOccupants)]
pub struct TriggerVolume {
    pub shape: TriggerShape,
    pub event_id: String,
}

impl TriggerVolume {
    pub fn new(radius: f32, height: f32, event_id: &str) -> Self {
        Self { shape: TriggerShape::Cylinder { radius, height }, event_id: event_id.to_string() }
    }

    pub fn extents(&self) -> (f32, f32) {
        let TriggerShape::Cylinder { radius, height } = self.shape;
        (radius, height)
    }

    /// `origin` is the trigger's base centre.
    pub fn contains(&self, origin: Vec3, p: Vec3) -> bool {
        let (radius, height) = self.extents();
        let d = p - origin;
        Vec2::new(d.x, d.z).length_squared() <= radius * radius && d.y >= -BASE_SLACK && d.y <= height
    }
}

/// Actors currently inside (runtime only, not saved).
#[derive(Component, Default)]
pub struct TriggerOccupants(pub HashSet<Entity>);

/// Clickable post at a trigger's origin (volumes have no other mesh, so picking needs a handle).
#[derive(Component)]
struct TriggerMarker;

#[derive(Message, Clone, Debug, PartialEq)]
pub struct TriggerEnterEvent {
    pub trigger: Entity,
    pub actor: Entity,
    pub event_id: String,
}

#[derive(Message, Clone, Debug, PartialEq)]
pub struct TriggerExitEvent {
    pub trigger: Entity,
    pub actor: Entity,
    pub event_id: String,
}

#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct TriggerGizmos;

pub struct TriggersPlugin;

impl Plugin for TriggersPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<TriggerEnterEvent>()
            .add_message::<TriggerExitEvent>()
            .init_gizmo_group::<TriggerGizmos>()
            .add_systems(Startup, |mut store: ResMut<GizmoConfigStore>| {
                let (cfg, _) = store.config_mut::<TriggerGizmos>();
                cfg.depth_bias = -0.2;
                cfg.line.width = 3.0;
            })
            .add_systems(Update, (sync_trigger_extents, track_triggers, log_trigger_events, draw_trigger_outlines, marker_visibility).chain());
    }
}

/// Spawn a trigger volume root; `tf` must already stand on the terrain.
pub fn spawn_trigger(commands: &mut Commands, meshes: &mut Assets<Mesh>, mats: &mut Assets<StandardMaterial>, mut so: SceneObject, tf: Transform, volume: TriggerVolume) -> Entity {
    let (r, h) = volume.extents();
    so.category = ObjectCategory::Trigger;
    so.aabb_half_extents = Vec3::new(r, h * 0.5, r);
    let pole = meshes.add(Cuboid::new(0.14, 1.4, 0.14));
    let mat = mats.add(StandardMaterial { base_color: Color::srgb(1.0, 0.62, 0.12), emissive: LinearRgba::new(2.0, 0.9, 0.1, 1.0), unlit: true, ..default() });
    commands
        .spawn((Name::new("TriggerVolume"), so, volume, Interactable { kind: InteractionKind::Trigger }, tf, Visibility::default()))
        .with_children(|c| {
            c.spawn((Name::new("TriggerMarker"), TriggerMarker, Mesh3d(pole), MeshMaterial3d(mat), Transform::from_xyz(0.0, 0.7, 0.0), NotShadowCaster));
        })
        .id()
}

fn sync_trigger_extents(mut q: Query<(&TriggerVolume, &mut SceneObject), Changed<TriggerVolume>>) {
    for (v, mut so) in &mut q {
        let (r, h) = v.extents();
        so.aabb_half_extents = Vec3::new(r, h * 0.5, r);
    }
}

/// Play mode: diff each volume's occupant set against the Player-faction actors inside it.
#[allow(clippy::type_complexity)]
pub fn track_triggers(
    studio: Res<StudioState>,
    mut volumes: Query<(Entity, &Transform, &TriggerVolume, &mut TriggerOccupants), Without<Deleted>>,
    actors: Query<(Entity, &Transform, &Faction), (With<Actor>, Without<Deleted>, Without<TriggerVolume>)>,
    mut enter: MessageWriter<TriggerEnterEvent>,
    mut exit: MessageWriter<TriggerExitEvent>,
) {
    let play = studio.mode == StudioMode::Play;
    for (te, ttf, vol, mut occ) in &mut volumes {
        if !play {
            if !occ.0.is_empty() {
                occ.0.clear();
            }
            continue;
        }
        let now: HashSet<Entity> = actors.iter().filter(|(_, tf, f)| **f == Faction::Player && vol.contains(ttf.translation, tf.translation)).map(|(e, ..)| e).collect();
        let mut entered: Vec<Entity> = now.difference(&occ.0).copied().collect();
        let mut left: Vec<Entity> = occ.0.difference(&now).copied().collect();
        entered.sort();
        left.sort();
        for actor in entered {
            enter.write(TriggerEnterEvent { trigger: te, actor, event_id: vol.event_id.clone() });
        }
        for actor in left {
            exit.write(TriggerExitEvent { trigger: te, actor, event_id: vol.event_id.clone() });
        }
        if now != occ.0 {
            occ.0 = now;
        }
    }
}

fn log_trigger_events(mut enter: MessageReader<TriggerEnterEvent>, mut exit: MessageReader<TriggerExitEvent>, mut studio: ResMut<StudioState>, names: Query<&Name>) {
    let who = |e: Entity| names.get(e).map_or_else(|_| format!("{e}"), |n| n.to_string());
    for ev in enter.read() {
        studio.status = format!("Trigger '{}': {} entered", ev.event_id, who(ev.actor));
    }
    for ev in exit.read() {
        studio.status = format!("Trigger '{}': {} left", ev.event_id, who(ev.actor));
    }
}

/// Editor mode: wire cylinder around every volume (brighter when selected).
fn draw_trigger_outlines(studio: Res<StudioState>, sel: Res<EditorSelection>, volumes: Query<(Entity, &Transform, &TriggerVolume), Without<Deleted>>, mut g: Gizmos<TriggerGizmos>) {
    if studio.mode != StudioMode::Editor {
        return;
    }
    for (e, tf, v) in &volumes {
        let (r, h) = v.extents();
        let color = if sel.selected.contains(&e) { Color::linear_rgb(4.0, 2.2, 0.3) } else { Color::linear_rgb(1.6, 0.8, 0.1) };
        let flat = Quat::from_rotation_x(FRAC_PI_2);
        let base = tf.translation;
        let top = base + Vec3::Y * h;
        g.circle(Isometry3d::new(base, flat), r, color).resolution(48);
        g.circle(Isometry3d::new(top, flat), r, color).resolution(48);
        for k in 0..8 {
            let a = k as f32 / 8.0 * std::f32::consts::TAU;
            let off = Vec3::new(a.cos() * r, 0.0, a.sin() * r);
            g.line(base + off, top + off, color.with_alpha(0.7));
        }
    }
}

fn marker_visibility(studio: Res<StudioState>, mut q: Query<&mut Visibility, With<TriggerMarker>>) {
    let v = if studio.mode == StudioMode::Editor { Visibility::Inherited } else { Visibility::Hidden };
    for mut vis in &mut q {
        vis.set_if_neq(v);
    }
}
