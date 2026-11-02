//! Graphs of graphs: a Group node holds a whole canvas of its own, with its
//! own view, grid and interaction. Inside, an In node mirrors the group's
//! inputs and an Out node its outputs, so values flow across the boundary.
//!
//! Every `GraphQuery` lookup resolves to the nearest canvas or node, so the
//! inner graph is independent: wires cannot cross it, and dragging, panning
//! and selecting inside it leave the outer graph alone.
//!
//! Type into the Number fields and drag the Scale slider inside the group:
//! Print follows live, through the group.
//!
//! ```sh
//! cargo run --example subgraph --features default_style
//! ```

use bevy::feathers::FeathersPlugins;
use bevy::feathers::controls::{FeathersNumberInput, FeathersSlider, NumberInputValue};
use bevy::feathers::dark_theme::create_dark_theme;
use bevy::feathers::theme::UiTheme;
use bevy::prelude::*;
use bevy::ui_widgets::{SliderPrecision, SliderStep, SliderValue, ValueChange, slider_self_update};
use bevy_noodle::prelude::*;
use bevy_noodle::style::kit;

mod feathers_fixes;
use feathers_fixes::FeathersFixesPlugin;

const NUMBER: PortType = PortType::named("number");
const BLUE: Color = Color::srgb(0.25, 0.52, 0.9);
const ORANGE: Color = Color::srgb(0.95, 0.6, 0.25);

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, FeathersPlugins, FeathersFixesPlugin))
        .add_plugins((NoodlePlugins, NoodleDefaultStylePlugin))
        .insert_resource(UiTheme(create_dark_theme()))
        .insert_resource(ClearColor(Color::srgb_u8(24, 25, 29)))
        .add_systems(Startup, setup)
        .add_systems(Update, show_results)
        .add_observer(edit)
        .run();
}

/// A connection to make: (canvas, from node, output index, to node, input index).
type Wire = (Entity, Entity, usize, Entity, usize);

/// What a node computes.
#[derive(Component, Clone, Copy)]
enum Op {
    Number(f32),
    Scale(f32),
    Add,
    /// A group and the canvas inside it.
    Group(Entity),
    /// Inside a group: its outputs are the group's inputs.
    In,
    /// Inside a group: its inputs are the group's outputs.
    Out,
    /// Shows its input in a text entity.
    Print(Entity),
}

/// A nested canvas: a fixed-size viewport clipping its graph, zoomed out,
/// with a darker grid. Each patch changes only the fields it names.
fn nested() -> impl Scene {
    bsn! {
        @kit::canvas()
        Node { width: px(540), height: px(240), border_radius: BorderRadius::all(px(6)) }
        CanvasView { zoom: 0.7 }
        CanvasGrid { background: Color::srgb_u8(30, 32, 37) }
    }
}

/// A node on `canvas` computing `op`, with `rows` under its frame.
fn node(commands: &mut Commands, canvas: Entity, op: Op, at: Vec2, rows: impl SceneList) -> Entity {
    let scene = bsn! { @kit::node(at) op ChildOf(canvas) Children [ {rows} ] };
    commands.spawn_scene(scene).id()
}

fn number_in(label: &str) -> impl Scene {
    kit::row(label, Port::input(NUMBER), BLUE)
}

fn number_out(label: &str) -> impl Scene {
    kit::row(label, Port::output(NUMBER), BLUE)
}

fn number_field(value: f32) -> impl Scene {
    bsn! { @FeathersNumberInput NumberInputValue::F32({value}) }
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    let outer = commands.spawn_scene(kit::canvas()).id();

    // The group: ports on its frame, a canvas in its body.
    let inner = commands.spawn_scene(nested()).id();
    let group = node(
        &mut commands,
        outer,
        Op::Group(inner),
        Vec2::new(280.0, 90.0),
        bsn_list! { @kit::title("Group") -- @number_in("a") -- @number_in("b") },
    );
    let result_row = commands.spawn_scene(number_out("result")).id();
    commands.entity(group).add_children(&[inner, result_row]);

    let commands = &mut commands;
    let three = node(
        commands,
        outer,
        Op::Number(3.0),
        Vec2::new(40.0, 120.0),
        bsn_list! { @kit::title("Number") -- @number_field(3.0) -- @number_out("value") },
    );
    let four = node(
        commands,
        outer,
        Op::Number(4.0),
        Vec2::new(40.0, 330.0),
        bsn_list! { @kit::title("Number") -- @number_field(4.0) -- @number_out("value") },
    );
    let input = node(
        commands,
        inner,
        Op::In,
        Vec2::new(10.0, 110.0),
        bsn_list! { @kit::title("In") -- @number_out("a") -- @number_out("b") },
    );
    // A slider under the title sets the scale.
    let scale = node(
        commands,
        inner,
        Op::Scale(2.0),
        Vec2::new(200.0, 10.0),
        bsn_list! {
            @kit::title("Scale")
            --
            @FeathersSlider { @max: 4.0 }
            SliderValue(2.0)
            SliderStep(0.25)
            SliderPrecision(2)
            on(slider_self_update)
            --
            @number_in("x") -- @number_out("k x")
        },
    );
    let add = node(
        commands,
        inner,
        Op::Add,
        Vec2::new(390.0, 140.0),
        bsn_list! { @kit::title("Add") -- @number_in("a") -- @number_in("b") -- @number_out("sum") },
    );
    let output = node(
        commands,
        inner,
        Op::Out,
        Vec2::new(600.0, 170.0),
        bsn_list! { @kit::title("Out") -- @number_in("result") },
    );
    let print = node(
        commands,
        outer,
        Op::Print(Entity::PLACEHOLDER),
        Vec2::new(950.0, 280.0),
        bsn_list! { @kit::title("Print") -- @number_in("value") },
    );
    let result = commands
        .spawn((
            Text::new("?"),
            TextColor(ORANGE),
            TextFont::from_font_size(22.0),
            ChildOf(print),
        ))
        .id();
    commands.entity(print).try_insert(Op::Print(result));

    let wires: [Wire; 7] = [
        (outer, three, 0, group, 0),
        (outer, four, 0, group, 1),
        (inner, input, 0, scale, 0),
        (inner, scale, 0, add, 0),
        (inner, input, 1, add, 1),
        (inner, add, 0, output, 0),
        (outer, group, 0, print, 0),
    ];
    commands.queue(move |world: &mut World| {
        let ports = |In(wires): In<[Wire; 7]>, g: GraphQuery| {
            wires
                .into_iter()
                .filter_map(|(canvas, a, i, b, j)| {
                    Some((canvas, g.outputs_of(a).nth(i)?, g.inputs_of(b).nth(j)?))
                })
                .collect::<Vec<_>>()
        };
        let pairs = world.run_system_cached_with(ports, wires);
        for (canvas, from, to) in pairs.into_iter().flatten() {
            world
                .graph_edit(canvas, GraphEdit::Connect { from, to })
                .ok();
        }
    });
}

fn show_results(graph: GraphQuery, ops: Query<(Entity, &Op)>, mut texts: Query<&mut Text>) {
    for (node, op) in &ops {
        if let Op::Print(text) = op
            && let Ok(mut text) = texts.get_mut(*text)
        {
            let value = input_value(&graph, &ops, node, 0, 0);
            let shown = value.map_or("?".into(), |v| v.to_string());
            text.set_if_neq(Text(shown));
        }
    }
}

/// The value arriving at `node`'s input number `index`: `None` if
/// unconnected (or too deep, which also stops cycles).
fn input_value(
    graph: &GraphQuery,
    ops: &Query<(Entity, &Op)>,
    node: Entity,
    index: usize,
    depth: u32,
) -> Option<f32> {
    let output = graph.source_of(node, index)?;
    if depth > 64 {
        return None;
    }
    let node = graph.node_of(output)?;
    let input_of = |node, i| input_value(graph, ops, node, i, depth + 1);
    let index = graph.outputs_of(node).position(|p| p == output)?;
    match ops.get(node).ok()?.1 {
        Op::Number(value) => Some(*value),
        Op::Scale(k) => Some(k * input_of(node, 0)?),
        Op::Add => Some(input_of(node, 0)? + input_of(node, 1)?),
        // A group's output is what reaches the same input of its Out node.
        Op::Group(inner) => {
            let out = graph
                .nodes_in(*inner)
                .find(|n| matches!(ops.get(*n), Ok((_, Op::Out))))?;
            input_of(out, index)
        }
        // An In node's output is what reaches the same input of its group.
        Op::In => {
            let group = graph.node_of(graph.canvas_of(node)?)?;
            input_of(group, index)
        }
        Op::Out | Op::Print(_) => None,
    }
}

/// A field or slider sets its node's number or scale.
fn edit(change: On<ValueChange<f32>>, graph: GraphQuery, mut ops: Query<&mut Op>) {
    let node = graph.node_of(change.source);
    let Some(mut op) = node.and_then(|n| ops.get_mut(n).ok()) else {
        return;
    };
    match *op {
        Op::Number(_) => *op = Op::Number(change.value),
        Op::Scale(_) => *op = Op::Scale(change.value),
        _ => {}
    }
}
