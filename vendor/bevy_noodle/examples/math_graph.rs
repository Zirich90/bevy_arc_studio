//! A live calculator: your own components on nodes, Bevy's text input inside
//! a node, evaluation through `GraphQuery`, and a `ConnectionCheck` observer
//! rejecting connections that would create a cycle.
//!
//! ```sh
//! cargo run --example math_graph --features default_style
//! ```

use bevy::input_focus::tab_navigation::TabIndex;
use bevy::picking::cursor::CursorIconPlugin;
use bevy::prelude::*;
use bevy::text::{EditableText, EditableTextFilter, LineBreak, TextCursorStyle};
use bevy::ui_widgets::TextInput;
use bevy_noodle::prelude::*;
use bevy_noodle::style::kit;

const NUMBER: PortType = PortType::named("number");
const BLUE: Color = Color::srgb(0.25, 0.52, 0.9);

/// What a node computes: your data, not the library's.
#[derive(Component, Clone, Copy, PartialEq, Debug)]
enum MathOp {
    Number,
    Add,
    Multiply,
    Display,
}

/// The value typed into a Number node.
#[derive(Component, Clone, Copy, PartialEq, Default)]
struct NumberValue(f64);

/// The text field of a Number node, and the result text of a Display node.
#[derive(Component, Clone, Default)]
struct NumberField;
#[derive(Component, Clone, Default)]
struct ResultText;

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, CursorIconPlugin))
        .add_plugins((NoodlePlugins, NoodleDefaultStylePlugin))
        .insert_resource(ClearColor(Color::srgb_u8(24, 25, 29)))
        .add_systems(Startup, setup)
        .add_systems(Update, (read_number_fields, evaluate).chain())
        .add_observer(reject_cycles)
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    let canvas = commands.spawn_scene(kit::canvas()).id();
    let nodes = [
        (MathOp::Number, 60.0, 60.0, 2.0),
        (MathOp::Number, 60.0, 220.0, 3.5),
        (MathOp::Number, 60.0, 380.0, 4.0),
        (MathOp::Add, 320.0, 120.0, 0.0),
        (MathOp::Multiply, 560.0, 220.0, 0.0),
        (MathOp::Display, 800.0, 240.0, 0.0),
    ]
    .map(|(op, x, y, value)| spawn_node(&mut commands, canvas, Vec2::new(x, y), op, value));
    commands.queue(move |world: &mut World| {
        _ = world.run_system_cached_with(wire_demo, (canvas, nodes))
    });
}

fn wire_demo(
    In((canvas, n)): In<(Entity, [Entity; 6])>,
    graph: GraphQuery,
    mut commands: Commands,
) {
    let [a, b, c, add, multiply, display] = n;
    for (from, to) in [
        (a, add),
        (b, add),
        (add, multiply),
        (c, multiply),
        (multiply, display),
    ] {
        if let (Some(from), Some(to)) = (graph.outputs_of(from).next(), graph.inputs_of(to).next())
        {
            commands.graph_edit(canvas, GraphEdit::Connect { from, to });
        }
    }
}

/// A kit frame plus our own widgets and components.
fn spawn_node(commands: &mut Commands, canvas: Entity, at: Vec2, op: MathOp, value: f64) -> Entity {
    let title = format!("{op:?}");
    let rows: Box<dyn SceneList> = match op {
        MathOp::Number => {
            let mut text = EditableText::new(format!("{value}"));
            text.visible_width = Some(8.0);
            let filter = EditableTextFilter::new(|c| c.is_ascii_digit() || matches!(c, '.' | '-'));
            Box::new(bsn_list! {
                NumberField
                // A focusable control: presses and drags in it are its own.
                TabIndex(0)
                Node { padding: UiRect::axes(px(6), px(3)), border: UiRect::all(px(1)) }
                // The widget behavior; `EditableText` is only its state.
                TextInput
                text
                filter
                TextLayout::linebreak(LineBreak::NoWrap)
                // Without a cursor style, Bevy draws no cursor or selection.
                TextCursorStyle {
                    color: Color::WHITE,
                    selection_color: {BLUE.with_alpha(0.45)},
                    unfocused_selection_color: Color::NONE,
                }
                BackgroundColor(Color::srgb_u8(28, 29, 33))
                BorderColor::all(Color::srgb_u8(70, 73, 81))
                --
                @kit::row("value", Port::output(NUMBER), BLUE)
            })
        }
        // One port taking any number of inputs.
        MathOp::Add | MathOp::Multiply => {
            let values = Port::input(NUMBER).with_capacity(Capacity::Unlimited);
            Box::new(bsn_list! {
                @kit::row("values", values, BLUE) -- @kit::row("result", Port::output(NUMBER), BLUE)
            })
        }
        MathOp::Display => Box::new(bsn_list! {
            @kit::row("value", Port::input(NUMBER), BLUE)
            --
            ResultText
            Text("–")
            TextFont { font_size: FontSize::Px(20.0) }
            TextColor(Color::srgb(0.6, 0.9, 0.65))
        }),
    };
    let scene = bsn! {
        @kit::node(at) op NumberValue(value) ChildOf(canvas)
        Children [ @kit::title(title) -- {rows} ]
    };
    commands.spawn_scene(scene).id()
}

fn read_number_fields(
    fields: Query<(Entity, &EditableText), With<NumberField>>,
    graph: GraphQuery,
    mut values: Query<&mut NumberValue>,
) {
    for (field, text) in &fields {
        if let (Ok(parsed), Some(mut value)) = (
            text.value().to_string().trim().parse(),
            graph.node_of(field).and_then(|n| values.get_mut(n).ok()),
        ) {
            value.set_if_neq(NumberValue(parsed));
        }
    }
}

/// Recomputes every Display when the graph or a number changes.
fn evaluate(
    mut edits: MessageReader<EditApplied>,
    changed: Query<(), Changed<NumberValue>>,
    graph: GraphQuery,
    ops: Query<(&MathOp, Option<&NumberValue>)>,
    mut results: Query<(Entity, &mut Text), With<ResultText>>,
) {
    if edits.read().count() == 0 && changed.is_empty() {
        return;
    }
    for (result, mut text) in &mut results {
        let node = graph.node_of(result);
        let shown = node
            .and_then(|n| value_of(n, &graph, &ops, 0))
            .map_or("–".into(), |v| v.to_string());
        text.set_if_neq(Text(shown));
    }
}

/// A node's value from its upstream nodes; `None` if a Display is unconnected.
fn value_of(
    node: Entity,
    graph: &GraphQuery,
    ops: &Query<(&MathOp, Option<&NumberValue>)>,
    depth: usize,
) -> Option<f64> {
    let (op, number) = ops.get(node).ok().filter(|_| depth < 64)?;
    let mut inputs = graph
        .inputs_of(node)
        .flat_map(|i| graph.peers_of(i))
        .filter_map(|p| graph.node_of(p));
    Some(match op {
        MathOp::Number => number.map_or(0.0, |n| n.0),
        MathOp::Add => inputs
            .filter_map(|n| value_of(n, graph, ops, depth + 1))
            .sum(),
        MathOp::Multiply => inputs
            .filter_map(|n| value_of(n, graph, ops, depth + 1))
            .product(),
        MathOp::Display => value_of(inputs.next()?, graph, ops, depth + 1)?,
    })
}

/// Rules are observers: refuse connections that would make a cycle. Dragged
/// wires ask too, so they do not snap where a cycle would form.
fn reject_cycles(mut check: On<ConnectionCheck>, graph: GraphQuery) {
    let ports = check.ports;
    let (Some(source), Some(target)) = (graph.node_of(ports.output), graph.node_of(ports.input))
    else {
        return;
    };
    // A cycle forms if `source` is already downstream of `target`.
    let mut stack = vec![target];
    while let Some(node) = stack.pop() {
        if node == source {
            return check.reject();
        }
        stack.extend(
            graph
                .outputs_of(node)
                .flat_map(|o| graph.peers_of(o))
                .filter_map(|p| graph.node_of(p)),
        );
    }
}
