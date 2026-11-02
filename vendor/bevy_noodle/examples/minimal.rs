//! No default style: plain Bevy UI nodes, edges drawn with gizmos from
//! `EdgeGeometry`, selection shown by reacting to `Selected`.
//!
//! ```sh
//! cargo run --example minimal
//! ```

use bevy::prelude::*;
use bevy::ui::Selected;
use bevy_noodle::prelude::*;

const NUMBER: PortType = PortType::named("number");

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, NoodlePlugins))
        .add_systems(Startup, setup)
        .add_systems(Update, (draw_edges, show_selection))
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    // No background: whatever is behind the canvas shows through.
    let canvas = commands
        .spawn((
            NodeCanvas,
            CanvasInteraction::default(),
            Node {
                width: percent(100),
                height: percent(100),
                ..default()
            },
        ))
        .id();
    for (title, position, port) in [
        ("Source", Vec2::new(80.0, 120.0), Port::output(NUMBER)),
        ("Source", Vec2::new(80.0, 300.0), Port::output(NUMBER)),
        (
            "Sink",
            Vec2::new(420.0, 200.0),
            Port::input(NUMBER).with_capacity(Capacity::Unlimited),
        ),
    ] {
        let side = if port.direction == PortDirection::Input {
            AlignSelf::FlexStart
        } else {
            AlignSelf::FlexEnd
        };
        commands.spawn((
            GraphNode,
            NodePosition(position),
            ChildOf(canvas),
            Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(10)),
                row_gap: px(8),
                min_width: px(120),
                border: UiRect::all(px(2)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.16, 0.18, 0.22)),
            BorderColor::all(Color::srgb(0.3, 0.32, 0.38)),
            children![
                (Text::new(title), Pickable::IGNORE),
                (
                    port,
                    Node {
                        width: px(14),
                        height: px(14),
                        align_self: side,
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.4, 0.7, 1.0))
                ),
            ],
        ));
    }
}

/// No built-in renderer: `EdgeGeometry` (graph space) is on every laid-out
/// edge and on the wire being dragged. Here it becomes gizmo curves; the canvas fills the
/// viewport, so canvas-local positions are viewport positions.
fn draw_edges(
    mut gizmos: Gizmos,
    edges: Query<&EdgeGeometry>,
    view: Single<&CanvasView>,
    camera: Single<(&Camera, &GlobalTransform)>,
) {
    let (camera, transform) = *camera;
    for geometry in &edges {
        let to_world = |p| {
            camera
                .viewport_to_world_2d(transform, view.graph_to_canvas(p))
                .unwrap_or_default()
        };
        let Ok(curve) = CubicBezier::new([geometry.bezier(0.5).map(to_world)]).to_curve() else {
            continue;
        };
        gizmos.linestrip_2d(curve.iter_positions(32), Color::srgb(0.4, 0.7, 1.0));
    }
}

fn show_selection(mut nodes: Query<(Has<Selected>, &mut BorderColor), With<GraphNode>>) {
    for (selected, mut border) in &mut nodes {
        border.set_if_neq(BorderColor::all(if selected {
            Color::srgb(1.0, 0.8, 0.3)
        } else {
            Color::srgb(0.3, 0.32, 0.38)
        }));
    }
}
