//! Reroute dots: tiny nodes with one input and one output of any type, for
//! routing edges around others. Right-click an edge to insert one where you
//! clicked (edges get pointer events like any UI entity); drag dots around.
//!
//! ```sh
//! cargo run --example reroute --features default_style
//! ```

use bevy::picking::cursor::CursorIconPlugin;
use bevy::prelude::*;
use bevy_noodle::prelude::*;
use bevy_noodle::style::kit;

const NUMBER: PortType = PortType::named("number");
const BLUE: Color = Color::srgb(0.25, 0.52, 0.9);

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, CursorIconPlugin))
        .add_plugins((NoodlePlugins, NoodleDefaultStylePlugin))
        .insert_resource(ClearColor(Color::srgb_u8(24, 25, 29)))
        .add_systems(Startup, setup)
        .add_observer(reroute_on_right_click)
        .run();
}

/// A pill with an input on its left and an output on its right.
fn reroute(at: Vec2, canvas: Entity) -> impl Scene {
    let at = at - Vec2::new(13.0, 7.0);
    let (input, output) = (Port::input(PortType::ANY), Port::output(PortType::ANY));
    bsn! {
        GraphNode
        NodePosition(at)
        Node { width: px(26), height: px(14), border_radius: BorderRadius::MAX }
        BackgroundColor(Color::srgb_u8(60, 63, 71))
        ChildOf(canvas)
        Children [ @kit::port(input, BLUE) -- @kit::port(output, BLUE) ]
    }
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    let canvas = commands.spawn_scene(kit::canvas()).id();
    let number = commands
        .spawn_scene(bsn! {
            @kit::node(Vec2::new(60.0, 300.0)) ChildOf(canvas)
            Children [ @kit::title("Number") -- @kit::row("value", Port::output(NUMBER), BLUE) ]
        })
        .id();
    // An obstacle the edge is routed around.
    commands.spawn_scene(bsn! {
        @kit::node(Vec2::new(420.0, 230.0)) ChildOf(canvas)
        Children [
            @kit::title("Obstacle") -- @kit::row("x", Port::input(NUMBER), BLUE)
            -- @kit::row("y", Port::output(NUMBER), BLUE)
        ]
    });
    let display = commands
        .spawn_scene(bsn! {
            @kit::node(Vec2::new(900.0, 300.0)) ChildOf(canvas)
            Children [ @kit::title("Display") -- @kit::row("value", Port::input(NUMBER), BLUE) ]
        })
        .id();
    let [first, second] = [Vec2::new(330.0, 150.0), Vec2::new(720.0, 150.0)]
        .map(|at| commands.spawn_scene(reroute(at, canvas)).id());
    let chain = [number, first, second, display];
    commands.queue(move |world: &mut World| {
        for (from, to) in chain.into_iter().zip(chain.into_iter().skip(1)) {
            connect(world, canvas, from, to);
        }
    });
}

/// Connects the first output of `from` to the first input of `to`.
fn connect(world: &mut World, canvas: Entity, from: Entity, to: Entity) {
    let ports = |In((a, b)): In<(Entity, Entity)>, graph: GraphQuery| {
        Some((graph.outputs_of(a).next()?, graph.inputs_of(b).next()?))
    };
    if let Ok(Some((from, to))) = world.run_system_cached_with(ports, (from, to)) {
        world
            .graph_edit(canvas, GraphEdit::Connect { from, to })
            .ok();
    }
}

/// Right-click on an edge splits it with a reroute dot.
fn reroute_on_right_click(
    click: On<PointerClick>,
    graph: GraphQuery,
    views: Query<&CanvasView>,
    mut commands: Commands,
) {
    // Act once, on the edge itself (the click then bubbles up).
    let edge = click.original_event_target();
    let (Some(PortPair { output, input }), Some(canvas), true) = (
        graph.edge_ports(edge),
        graph.canvas_of(edge),
        click.event_target() == edge,
    ) else {
        return;
    };
    let (Ok(view), PointerButton::Secondary) = (views.get(canvas), click.button) else {
        return;
    };
    // The canvas fills the window here, so window and canvas coordinates match.
    let at = view.canvas_to_graph(click.pointer.position);
    let dot = commands.spawn_scene(reroute(at, canvas)).id();
    commands.queue(move |world: &mut World| {
        let ports = |In(dot), graph: GraphQuery| {
            Some((graph.inputs_of(dot).next()?, graph.outputs_of(dot).next()?))
        };
        let Ok(Some((dot_in, dot_out))) = world.run_system_cached_with(ports, dot) else {
            return;
        };
        world
            .graph_edit(canvas, GraphEdit::Disconnect { edge })
            .ok();
        for (from, to) in [(output, dot_in), (dot_out, input)] {
            world
                .graph_edit(canvas, GraphEdit::Connect { from, to })
                .ok();
        }
    });
}
