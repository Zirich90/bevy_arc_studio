//! Cinematic showcase runner.
//!   showcase live                         real-time preview (loops)
//!   showcase shot <t> <out.png> [ex,ey,ez,tx,ty,tz]
//!   showcase record <dir> [fps]           deterministic frame dump (1920x1080)
use bevy::prelude::*;
use bevy::window::PresentMode;
use bevy_arc_studio::showcase::{Mode, ShowcasePlugin};
use bevy_arc_studio::{prelude::BevyArcStudioPlugin, props};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = match args.first().map(String::as_str) {
        Some("shot") => Mode::Shot {
            t: args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0.0),
            out: args.get(2).cloned().unwrap_or_else(|| "shot.png".into()),
            cam: args.get(3).map(|s| s.split(',').filter_map(|v| v.trim().parse::<f32>().ok()).collect::<Vec<_>>()).filter(|v| v.len() == 6).map(|v| [v[0], v[1], v[2], v[3], v[4], v[5]]),
        },
        Some("record") => Mode::Record { dir: args.get(1).cloned().unwrap_or_else(|| "frames".into()), fps: args.get(2).and_then(|s| s.parse().ok()).unwrap_or(30) },
        _ => Mode::Live,
    };
    let (w, h) = if matches!(mode, Mode::Live) { (1600, 900) } else { (1920, 1080) };
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window { title: "ArcStudio Showcase".into(), resolution: (w, h).into(), resizable: false, present_mode: PresentMode::AutoNoVsync, ..default() }),
                    ..default()
                })
                .set(AssetPlugin { file_path: props::assets_dir().to_string_lossy().into_owned(), ..default() }),
        )
        .add_plugins((ShowcasePlugin(mode), BevyArcStudioPlugin::default()))
        .run();
}
