use bevy::prelude::*;
use bevy_arc_studio::{arc_core::WorldPreset, prelude::*};

fn main() {
    let spec = std::env::args().nth(1).and_then(|a| match a.as_str() {
        "dusk" => Some(WorldPreset::MistyDuskLake),
        "noon" => Some(WorldPreset::SunnyAlpineNoon),
        "night" => Some(WorldPreset::MidnightCampfire),
        "dawn" => Some(WorldPreset::GoldenAutumnDawn),
        _ => None,
    });
    let plugin = spec.map_or_else(BevyArcStudioPlugin::default, |p| BevyArcStudioPlugin::with_spec(WorldSpec::from_preset(p)));
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin { primary_window: Some(Window { title: "Bevy ArcStudio".into(), resolution: (1600, 900).into(), ..default() }), ..default() })
                .set(AssetPlugin { file_path: bevy_arc_studio::props::assets_dir().to_string_lossy().into_owned(), ..default() }),
        )
        .add_plugins(plugin)
        .run();
}
