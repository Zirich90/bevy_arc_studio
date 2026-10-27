pub mod arc_core;
pub mod camera;
pub mod editor_registry;
pub mod gizmo_3d;
pub mod level_builder;
pub mod lighting;
pub mod materials;
pub mod props;
pub mod scene;
pub mod showcase;
pub mod selection;
pub mod terrain_render;
pub mod ui;
pub mod water;
pub mod mcp_commands;
pub mod operators;
pub mod play_mode;
pub mod prefabs;
pub mod modules;
pub mod movement;
pub mod navigation;
pub mod studio;
pub mod terrain_brush;
pub mod ui_assets;
pub mod ui_hud;
pub mod ui_inspector;
pub mod ui_outliner;
pub mod ui_reflect;
pub mod ui_toolbar;
pub mod world_environment;
pub mod world_io;

pub mod prelude {
    pub use crate::BevyArcStudioPlugin;
    pub use crate::arc_core::{HeightfieldGrid, UndoHistory, UnitScale, WorldSpec};
    pub use crate::mcp_commands::StudioCommandQueue;
    pub use crate::props::SceneObject;
    pub use crate::world_io::SceneComponentAppExt;
}

use bevy::diagnostic::{EntityCountDiagnosticsPlugin, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;

/// One-line integration: `app.add_plugins(BevyArcStudioPlugin::default());`
/// Requires `DefaultPlugins` to be added first.
#[derive(Default)]
pub struct BevyArcStudioPlugin {
    /// Starting world; `None` uses `WorldSpec::default()` (or one already inserted).
    pub spec: Option<arc_core::WorldSpec>,
}

impl BevyArcStudioPlugin {
    pub fn with_spec(spec: arc_core::WorldSpec) -> Self {
        Self { spec: Some(spec) }
    }
}

impl Plugin for BevyArcStudioPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<bevy_egui::EguiPlugin>() {
            app.add_plugins(bevy_egui::EguiPlugin::default());
        }
        if !app.is_plugin_added::<FrameTimeDiagnosticsPlugin>() {
            app.add_plugins(FrameTimeDiagnosticsPlugin::default());
        }
        if !app.is_plugin_added::<EntityCountDiagnosticsPlugin>() {
            app.add_plugins(EntityCountDiagnosticsPlugin::default());
        }
        if !app.is_plugin_added::<bevy_brp_extras::BrpExtrasPlugin>() {
            app.add_plugins(bevy_brp_extras::BrpExtrasPlugin::default());
        }
        if let Some(spec) = &self.spec {
            app.insert_resource(spec.clone());
        } else {
            app.init_resource::<arc_core::WorldSpec>();
        }
        app.init_resource::<arc_core::UndoHistory<arc_core::WorldSpec>>()
            .register_type::<arc_core::WorldSpec>()
            .register_type::<camera::EditorCamera>()
            .add_plugins((editor_registry::EditorRegistryPlugin, modules::ModulesPlugin, world_environment::WorldEnvironmentPlugin, level_builder::LevelBuilderPlugin, prefabs::PrefabsPlugin, ui_assets::AssetBrowserPlugin, materials::MaterialsPlugin))
            .add_plugins((
                scene::ScenePlugin,
                camera::EditorCameraPlugin,
                ui::UiPlugin,
                ui_reflect::ReflectInspectorPlugin,
                selection::SelectionPlugin,
                gizmo_3d::Gizmo3dPlugin,
                studio::StudioPlugin,
                mcp_commands::McpCommandsPlugin,
                movement::MovementPlugin,
                world_io::WorldIoPlugin,
            ));
    }
}
