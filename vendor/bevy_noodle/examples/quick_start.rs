//! Two nodes with styled ports and wires. Drag from the source port to the sink.
//! Run with `cargo run --example quick_start --features default_style`.

use bevy::picking::cursor::CursorIconPlugin;
use bevy::prelude::*;
use bevy_noodle::{prelude::*, style::kit};

const NUMBER: PortType = PortType::named("number");
const BLUE: Color = Color::srgb(0.3, 0.6, 1.0);

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, CursorIconPlugin))
        .add_plugins((NoodlePlugins, NoodleDefaultStylePlugin))
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    let canvas = commands.spawn_scene(kit::canvas()).id();
    commands.spawn_scene(bsn! {
        @kit::node(Vec2::new(80.0, 120.0)) ChildOf(canvas)
        Children [ @kit::title("Source") -- @kit::row("number", Port::output(NUMBER), BLUE) ]
    });
    commands.spawn_scene(bsn! {
        @kit::node(Vec2::new(420.0, 120.0)) ChildOf(canvas)
        Children [ @kit::title("Sink") -- @kit::row("number", Port::input(NUMBER), BLUE) ]
    });
}
