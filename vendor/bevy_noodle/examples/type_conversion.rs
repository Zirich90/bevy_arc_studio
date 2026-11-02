//! Automatic type conversion. The built-in rules refuse an int output on a
//! float input (`IncompatibleTypes`); a `ConnectionCheck` observer allows it,
//! so dragged wires snap there, and an `EditRequested` observer answers the
//! real edit with an "int to float" converter node wired in between. Drag from "Int" to "a" or "b" to see it. Type into the Int and
//! Float fields: the product follows live, through the converter.
//!
//! ```sh
//! cargo run --example type_conversion --features default_style
//! ```

use bevy::feathers::FeathersPlugins;
use bevy::feathers::controls::{FeathersNumberInput, NumberInputValue};
use bevy::feathers::dark_theme::create_dark_theme;
use bevy::feathers::theme::UiTheme;
use bevy::prelude::*;
use bevy::ui_widgets::ValueChange;
use bevy_noodle::prelude::*;
use bevy_noodle::style::kit;

mod feathers_fixes;
use feathers_fixes::FeathersFixesPlugin;

const INT: PortType = PortType::named("int");
const FLOAT: PortType = PortType::named("float");
const GREEN: Color = Color::srgb(0.45, 0.8, 0.5);
const BLUE: Color = Color::srgb(0.25, 0.52, 0.9);

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, FeathersPlugins, FeathersFixesPlugin))
        .add_plugins((NoodlePlugins, NoodleDefaultStylePlugin))
        .insert_resource(UiTheme(create_dark_theme()))
        .insert_resource(ClearColor(Color::srgb_u8(24, 25, 29)))
        .add_systems(Startup, setup)
        .add_systems(Update, (demo_connection, show_product))
        .add_observer(allow_int_to_float)
        .add_observer(convert)
        .add_observer(edit_int)
        .add_observer(edit_float)
        .run();
}

/// What a node computes.
#[derive(Component, Clone, Copy)]
enum Calc {
    Int(i32),
    Float(f32),
    ToFloat,
    Multiply,
}

/// The text where Multiply shows its product.
#[derive(Component, Clone, Default)]
struct Shows;

/// The connection made once laid out, to show the conversion.
#[derive(Resource)]
struct Demo {
    canvas: Entity,
    int: Entity,
    multiply: Entity,
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    let canvas = commands.spawn_scene(kit::canvas()).id();
    // The fields get their values at spawn, so they start with them; the
    // value's variant (`I32`, `F32`) sets what a field edits.
    let int = commands
        .spawn_scene(bsn! {
            @kit::node(Vec2::new(60.0, 100.0)) ~{Calc::Int(7)} ChildOf(canvas)
            Children [
                @kit::title("Int")
                --
                @FeathersNumberInput NumberInputValue::I32(7)
                --
                @kit::row("value", Port::output(INT), GREEN)
            ]
        })
        .id();
    commands.spawn_scene(bsn! {
        @kit::node(Vec2::new(60.0, 330.0)) ~{Calc::Float(2.5)} ChildOf(canvas)
        Children [
            @kit::title("Float")
            --
            @FeathersNumberInput NumberInputValue::F32(2.5)
            --
            @kit::row("value", Port::output(FLOAT), BLUE)
        ]
    });
    let multiply = commands
        .spawn_scene(bsn! {
            @kit::node(Vec2::new(620.0, 200.0)) ~{Calc::Multiply} ChildOf(canvas)
            Children [
                @kit::title("Multiply")
                --
                @kit::row("a", Port::input(FLOAT), BLUE)
                --
                @kit::row("b", Port::input(FLOAT), BLUE)
                --
                @kit::row("product", Port::output(FLOAT), BLUE)
                --
                Shows Text TextFont { font_size: FontSize::Px(15.0) }
            ]
        })
        .id();
    commands.insert_resource(Demo {
        canvas,
        int,
        multiply,
    });
}

/// Once laid out, wires the int straight into a float input.
fn demo_connection(demo: Option<Res<Demo>>, graph: GraphQuery, mut commands: Commands) {
    let Some(demo) = demo else {
        return;
    };
    let (Some(from), Some(to)) = (
        graph.outputs_of(demo.int).next(),
        graph.inputs_of(demo.multiply).next(),
    ) else {
        return;
    };
    if graph.port_position(from).is_some() {
        commands.graph_edit(demo.canvas, GraphEdit::Connect { from, to });
        commands.remove_resource::<Demo>();
    }
}

/// Whether a connection runs from an int output to a float input.
fn int_to_float(graph: &GraphQuery, ports: PortPair) -> bool {
    let port_type = |p| graph.port(p).map(|p| p.port_type);
    (port_type(ports.output), port_type(ports.input)) == (Some(INT), Some(FLOAT))
}

/// The rule: ints may connect to floats.
fn allow_int_to_float(mut check: On<ConnectionCheck>, graph: GraphQuery) {
    if check.refused == Some(RejectReason::IncompatibleTypes) && int_to_float(&graph, check.ports) {
        check.allow();
    }
}

/// The side effect: an int → float connection gets a converter in between.
fn convert(mut request: On<EditRequested>, graph: GraphQuery, mut commands: Commands) {
    // Connections arrive normalized: `from` is the output.
    let GraphEdit::Connect { from, to } = request.edit else {
        return;
    };
    if !int_to_float(&graph, PortPair::new(from, to)) {
        return;
    }
    // The direct connection is refused; a converter goes in between.
    request.reject();
    let canvas = request.canvas;
    let position = |p| graph.port_position(p).unwrap_or_default();
    let at = (position(from) + position(to)) / 2.0 - Vec2::new(80.0, 40.0);
    let converter = commands
        .spawn_scene(bsn! {
            @kit::node(at) ~{Calc::ToFloat} ChildOf(canvas)
            Children [
                @kit::title("int to float")
                --
                @kit::row("int", Port::input(INT), GREEN)
                --
                @kit::row("float", Port::output(FLOAT), BLUE)
            ]
        })
        .id();
    commands.queue(move |world: &mut World| {
        let ports = |In(n), graph: GraphQuery| {
            Some((graph.inputs_of(n).next()?, graph.outputs_of(n).next()?))
        };
        let Ok(Some((converter_in, converter_out))) =
            world.run_system_cached_with(ports, converter)
        else {
            return;
        };
        for (from, to) in [(from, converter_in), (converter_out, to)] {
            world
                .graph_edit(canvas, GraphEdit::Connect { from, to })
                .ok();
        }
    });
}

/// An Int field's edit sets its node's value.
fn edit_int(change: On<ValueChange<i32>>, graph: GraphQuery, mut calcs: Query<&mut Calc>) {
    if let Some(mut calc) = graph
        .node_of(change.source)
        .and_then(|n| calcs.get_mut(n).ok())
    {
        *calc = Calc::Int(change.value);
    }
}

/// A Float field's edit sets its node's value.
fn edit_float(change: On<ValueChange<f32>>, graph: GraphQuery, mut calcs: Query<&mut Calc>) {
    if let Some(mut calc) = graph
        .node_of(change.source)
        .and_then(|n| calcs.get_mut(n).ok())
    {
        *calc = Calc::Float(change.value);
    }
}

/// What a node outputs, following its inputs back through the graph.
fn value(node: Entity, graph: &GraphQuery, calcs: &Query<&Calc>, depth: u8) -> Option<f32> {
    let input = |i: usize| {
        let peer = graph.source_of(node, i)?;
        // Wires can form a loop; give up on a deep chain.
        value(graph.node_of(peer)?, graph, calcs, depth.checked_sub(1)?)
    };
    match calcs.get(node).ok()? {
        Calc::Int(v) => Some(*v as f32),
        Calc::Float(v) => Some(*v),
        Calc::ToFloat => input(0),
        Calc::Multiply => Some(input(0)? * input(1)?),
    }
}

/// Multiply shows its product, every frame.
fn show_product(
    graph: GraphQuery,
    calcs: Query<&Calc>,
    mut shown: Query<(&mut Text, &ChildOf), With<Shows>>,
) {
    for (mut text, node) in &mut shown {
        let product = value(node.parent(), &graph, &calcs, 32);
        let product = product.map_or("= ? (connect a and b)".into(), |p| format!("= {p}"));
        text.set_if_neq(Text(product));
    }
}
