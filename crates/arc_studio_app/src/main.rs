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
    let game_scene = std::env::args().skip_while(|a| a != "--game-scene").nth(1);
    let mut app = App::new();
    app
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin { primary_window: Some(Window { title: "Bevy ArcStudio".into(), resolution: (1600, 900).into(), ..default() }), ..default() })
                .set(AssetPlugin { file_path: bevy_arc_studio::props::assets_dir().to_string_lossy().into_owned(), watch_for_changes_override: Some(true), ..default() }),
        )
        .add_plugins(plugin)
        // optional modules: compiled in, connected from the editor's Modules tab
        .add_plugins(bevy_arc_pf2e::Pf2ePlugin);
    if let Some(path) = game_scene { app.insert_resource(GameBoot(path)).add_systems(PostUpdate, boot_game); }
    app.run();
}

#[derive(Resource)]
struct GameBoot(String);
fn boot_game(world: &mut World) {
    let Some(boot) = world.remove_resource::<GameBoot>() else { return };
    world.resource_mut::<bevy_arc_studio::world_io::ResumeAfterScene>().0 = true;
    world.resource_mut::<bevy_arc_studio::world_io::SceneIoQueue>().0.push(
        bevy_arc_studio::world_io::SceneIo::Load(boot.0));
}
