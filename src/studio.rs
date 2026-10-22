//! Studio-wide editor state: mode / tab, unified undo order across world, scene and terrain histories, view toggles.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::EguiContexts;
use serde::{Deserialize, Serialize};

use crate::arc_core::*;
use crate::gizmo_3d::{SceneHistory, apply_states};
use crate::props::{NextObjectId, PropAssets, SceneObject, spawn_user_prop};
use crate::selection::{Deleted, EditorSelection};
use crate::terrain_render::{TerrainGrid, TerrainMesh, build_terrain_mesh};
use crate::water::Water;

#[derive(Reflect, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum StudioMode {
    #[default]
    Editor,
    Play,
}

#[derive(Reflect, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum InspectorTab {
    #[default]
    Global,
    Objects,
    Brushes,
}

#[derive(Reflect, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum CamStyle {
    #[default]
    Orbit,
    Fly,
}

#[derive(Resource, Reflect, Debug)]
#[reflect(Resource)]
pub struct StudioState {
    pub mode: StudioMode,
    pub tab: InspectorTab,
    pub cam_style: CamStyle,
    pub search: String,
    pub hide_scatter: bool,
    pub glb_name: String,
    pub scene_path: String,
    pub show_water: bool,
    pub show_grass: bool,
    pub toolbar_h: f32,
    pub status: String,
}

impl Default for StudioState {
    fn default() -> Self {
        Self {
            mode: StudioMode::Editor,
            tab: InspectorTab::Global,
            cam_style: CamStyle::Orbit,
            search: String::new(),
            hide_scatter: true,
            glb_name: String::new(),
            scene_path: "scenes/arc_scene.json".into(),
            show_water: true,
            show_grass: true,
            toolbar_h: 0.0,
            status: "Ready".into(),
        }
    }
}

impl StudioState {
    /// Gizmo + click selection are live (editor mode, not sculpting).
    pub fn gizmo_active(&self) -> bool {
        self.mode == StudioMode::Editor && self.tab != InspectorTab::Brushes
    }
    pub fn brush_active(&self) -> bool {
        self.mode == StudioMode::Editor && self.tab == InspectorTab::Brushes
    }
}

pub type TerrainHistory = UndoHistory<Vec<f32>>;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HistSrc {
    World,
    Scene,
    Terrain,
}

/// Global chronological order of edits across the three histories (what `Undo` / `Redo` act on next).
#[derive(Resource, Default)]
pub struct UndoOrder {
    pub stack: Vec<HistSrc>,
    pub redo: Vec<HistSrc>,
    last: [(usize, usize); 3],
}

#[derive(Resource, Default)]
pub struct HistoryRequest {
    pub undo: bool,
    pub redo: bool,
}

/// Atmosphere-only preset (terrain stays as generated).
pub fn apply_preset(spec: &WorldSpec, p: WorldPreset) -> WorldSpec {
    let mut next = spec.clone();
    let preset = WorldSpec::from_preset(p);
    next.time_of_day = preset.time_of_day;
    next.fog_color = preset.fog_color;
    next.fog_density = preset.fog_density;
    next.sun_intensity = preset.sun_intensity;
    next.god_rays = preset.god_rays;
    next
}

/// Everything needed to put an editor object on the terrain.
#[derive(SystemParam)]
pub struct Spawner<'w, 's> {
    pub commands: Commands<'w, 's>,
    assets: Option<Res<'w, PropAssets>>,
    pub meshes: ResMut<'w, Assets<Mesh>>,
    mats: ResMut<'w, Assets<StandardMaterial>>,
    server: Res<'w, AssetServer>,
    next: ResMut<'w, NextObjectId>,
}

impl Spawner<'_, '_> {
    pub fn spawn(&mut self, grid: &HeightfieldGrid, spec: &WorldSpec, kind: &str, x: f32, z: f32, yaw: f32) -> Option<(Entity, Transform)> {
        let assets = self.assets.as_deref()?;
        self.next.0 += 1;
        let pos = Vec3::new(x, grid.sample_height(x, z), z);
        spawn_user_prop(&mut self.commands, assets, kind, pos, yaw, self.next.0, spec, grid, &mut self.meshes, &mut self.mats, &self.server)
    }
}

pub struct StudioPlugin;

impl Plugin for StudioPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<StudioState>()
            .init_resource::<TerrainHistory>()
            .init_resource::<UndoOrder>()
            .init_resource::<HistoryRequest>()
            .register_type::<StudioState>()
            .register_type::<StudioMode>()
            .register_type::<InspectorTab>()
            .register_type::<CamStyle>()
            .add_systems(Update, (studio_hotkeys, process_history, track_order, apply_view_toggles).chain());
    }
}

fn studio_hotkeys(keys: Res<ButtonInput<KeyCode>>, mut studio: ResMut<StudioState>, mut req: ResMut<HistoryRequest>, mut sel: ResMut<EditorSelection>, mut egui: EguiContexts) {
    if studio.mode == StudioMode::Play {
        sel.clear();
    }
    if egui.ctx_mut().map(|c| c.egui_wants_keyboard_input()).unwrap_or(false) {
        return;
    }
    let ctrl = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    if keys.just_pressed(KeyCode::Tab) {
        studio.mode = if studio.mode == StudioMode::Editor { StudioMode::Play } else { StudioMode::Editor };
    }
    if ctrl && keys.just_pressed(KeyCode::KeyZ) && !shift {
        req.undo = true;
    } else if ctrl && (keys.just_pressed(KeyCode::KeyY) || (shift && keys.just_pressed(KeyCode::KeyZ))) {
        req.redo = true;
    }
}

#[allow(clippy::too_many_arguments)]
pub fn process_history(
    mut req: ResMut<HistoryRequest>,
    order: Res<UndoOrder>,
    mut spec: ResMut<WorldSpec>,
    mut wh: ResMut<UndoHistory<WorldSpec>>,
    mut sh: ResMut<SceneHistory>,
    mut th: ResMut<TerrainHistory>,
    mut grid: ResMut<TerrainGrid>,
    terrain: Query<&Mesh3d, With<TerrainMesh>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut commands: Commands,
    mut q: Query<(&mut Transform, &mut Visibility)>,
) {
    let (undo, redo) = (req.undo, req.redo);
    if !undo && !redo {
        return;
    }
    *req = HistoryRequest::default();
    let src = if undo { order.stack.last() } else { order.redo.last() };
    let Some(src) = src.copied() else { return };
    match src {
        HistSrc::World => {
            if let Some(s) = if undo { wh.undo() } else { wh.redo() } {
                *spec = s;
            }
        }
        HistSrc::Scene => {
            if let Some(s) = if undo { sh.undo() } else { sh.redo() } {
                apply_states(&s, &mut q, &mut commands);
            }
        }
        HistSrc::Terrain => {
            if let Some(h) = if undo { th.undo() } else { th.redo() } {
                if h.len() == grid.0.heights.len() {
                    grid.0.heights = h;
                    if let Ok(m) = terrain.single() {
                        let splats = grid.0.build_splatmap(&spec);
                        let _ = meshes.insert(&m.0, build_terrain_mesh(&grid.0, &splats));
                    }
                }
            }
        }
    }
}

fn track_order(wh: Res<UndoHistory<WorldSpec>>, sh: Res<SceneHistory>, th: Res<TerrainHistory>, mut o: ResMut<UndoOrder>) {
    let now = [(wh.undo_len(), wh.redo_len()), (sh.undo_len(), sh.redo_len()), (th.undo_len(), th.redo_len())];
    for (k, src) in [HistSrc::World, HistSrc::Scene, HistSrc::Terrain].into_iter().enumerate() {
        let ((u, r), (lu, lr)) = (now[k], o.last[k]);
        if u > lu {
            for _ in lu..u {
                if r < lr {
                    if let Some(p) = o.redo.iter().rposition(|s| *s == src) {
                        o.redo.remove(p);
                    }
                } else {
                    o.redo.retain(|s| *s != src);
                }
                o.stack.push(src);
            }
        } else if u < lu {
            for _ in u..lu {
                if let Some(p) = o.stack.iter().rposition(|s| *s == src) {
                    o.stack.remove(p);
                }
                o.redo.push(src);
            }
        }
        o.last[k] = (u, r);
    }
}

/// Water / grass quick toggles from the HUD.
#[allow(clippy::type_complexity)]
fn apply_view_toggles(
    studio: Res<StudioState>,
    mut water: Query<&mut Visibility, With<Water>>,
    mut props: Query<(&SceneObject, &mut Visibility), (Without<Water>, Without<Deleted>)>,
    added: Query<(), Added<SceneObject>>,
) {
    if !studio.is_changed() && added.is_empty() {
        return;
    }
    let vis = |on: bool| if on { Visibility::Inherited } else { Visibility::Hidden };
    for mut v in &mut water {
        v.set_if_neq(vis(studio.show_water));
    }
    for (so, mut v) in &mut props {
        if matches!(so.name.as_str(), "GrassClump" | "Reeds") {
            v.set_if_neq(vis(studio.show_grass));
        }
    }
}
