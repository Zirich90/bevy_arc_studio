//! Headless check: `assets/models/Duck.glb` loads through the AssetServer and snaps onto the heightfield.

use std::time::{Duration, Instant};

use bevy::asset::AssetPlugin;
use bevy::gltf::GltfPlugin;
use bevy::image::{CompressedImageFormats, ImageLoader};
use bevy::mesh::MeshPlugin;
use bevy::prelude::*;
use bevy::world_serialization::WorldSerializationPlugin;
use bevy_arc_studio::arc_core::{HeightfieldGrid, WorldSpec};
use bevy_arc_studio::props::assets_dir;

const MODEL: &str = "models/Duck.glb";

#[test]
fn glb_file_present_and_valid() {
    let bytes = std::fs::read(assets_dir().join(MODEL)).expect("Duck.glb missing");
    assert_eq!(&bytes[..4], b"glTF");
    assert!(bytes.len() < 1_000_000, "model should stay small");
}

#[test]
fn duck_loads_and_spawns_on_terrain() {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin { file_path: assets_dir().to_string_lossy().into_owned(), ..default() },
        MeshPlugin,
        ImagePlugin::default(),
        WorldSerializationPlugin,
        GltfPlugin::default(),
    ))
    .init_asset::<StandardMaterial>()
    .register_asset_loader(ImageLoader::new(CompressedImageFormats::NONE));

    app.finish();
    app.cleanup();
    let server = app.world().resource::<AssetServer>().clone();
    let scene: Handle<WorldAsset> = server.load(GltfAssetLabel::Scene(0).from_asset(MODEL));

    let start = Instant::now();
    loop {
        app.update();
        if server.is_loaded_with_dependencies(&scene) {
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(20), "load timeout: {:?} deps={:?}", server.load_state(&scene), server.dependency_load_state(&scene));
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(app.world().resource::<Assets<WorldAsset>>().get(&scene).is_some());

    // ground snap, same formula as props::sync_props
    let spec = WorldSpec::default();
    let grid = HeightfieldGrid::generate(&spec);
    let camp = spec.camp_pos();
    let p = camp + Vec2::new(0.6_f32.cos(), 0.6_f32.sin()) * 9.0;
    let ground = grid.sample_height(p.x, p.y);
    let e = app.world_mut().spawn((WorldAssetRoot(scene), Transform::from_xyz(p.x, ground, p.y))).id();
    for _ in 0..3 {
        app.update();
    }
    let tf = app.world().get::<Transform>(e).unwrap();
    assert!((tf.translation.y - grid.sample_height(tf.translation.x, tf.translation.z)).abs() < 1e-4);
    // model must have been instantiated as children of the root
    let mut q = app.world_mut().query::<&ChildOf>();
    assert!(q.iter(app.world()).any(|c| c.parent() == e), "scene was not instantiated under the root");
}
