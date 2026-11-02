//! Automatic layout: press L to lay the graph out in layers (each node one
//! layer right of its deepest input, ordered within a layer by where its
//! inputs are), S to scramble it again, F to pan and zoom so it all fits. Every node moves through one
//! `MoveNodes` edit, so an undo stack (see the `editor` example) can undo it.
//!
//! ```sh
//! cargo run --example auto_layout --features default_style
//! ```

use std::collections::HashMap;

use bevy::picking::cursor::CursorIconPlugin;
use bevy::prelude::*;
use bevy_noodle::prelude::*;
use bevy_noodle::style::kit;

const NUMBER: PortType = PortType::named("number");
const BLUE: Color = Color::srgb(0.25, 0.52, 0.9);
const SPACING: Vec2 = Vec2::new(205.0, 120.0);

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, CursorIconPlugin))
        .add_plugins((NoodlePlugins, NoodleDefaultStylePlugin))
        .insert_resource(ClearColor(Color::srgb_u8(24, 25, 29)))
        .add_systems(Startup, setup)
        .add_systems(Update, (keys, frame_all))
        .run();
}

#[derive(Resource)]
struct Graph(Entity);

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    let canvas = commands.spawn_scene(kit::canvas()).id();
    commands.insert_resource(Graph(canvas));
    // (title, inputs), deliberately out of order.
    let specs = [
        ("Sum", 2),
        ("Value A", 0),
        ("Scale", 1),
        ("Display", 1),
        ("Value B", 0),
        ("Offset", 2),
        ("Value C", 0),
        ("Clamp", 1),
    ];
    let nodes = specs.map(|(title, inputs)| {
        let at = scrambled(title);
        let rows: Vec<_> = ["a", "b"]
            .into_iter()
            .take(inputs)
            .map(|name| kit::row(name, Port::input(NUMBER), BLUE))
            .chain([kit::row("out", Port::output(NUMBER), BLUE)])
            .collect();
        let scene =
            bsn! { @kit::node(at) ChildOf(canvas) Children [ @kit::title(title) -- {rows} ] };
        commands.spawn_scene(scene).id()
    });
    let [sum, a, scale, display, b, offset, c, clamp] = nodes;
    // (from, to, input).
    let wires = [
        (a, scale, 0),
        (scale, sum, 0),
        (b, sum, 1),
        (sum, offset, 0),
        (c, offset, 1),
        (offset, clamp, 0),
        (clamp, display, 0),
    ];
    commands.queue(move |world: &mut World| {
        let ports = |In((a, b, i)): In<(Entity, Entity, usize)>, graph: GraphQuery| {
            Some((graph.outputs_of(a).next()?, graph.inputs_of(b).nth(i)?))
        };
        for wire in wires {
            let Ok(Some((from, to))) = world.run_system_cached_with(ports, wire) else {
                continue;
            };
            world
                .graph_edit(canvas, GraphEdit::Connect { from, to })
                .ok();
        }
    });
    commands.spawn((
        Text::new("L: lay out | S: scramble | F: frame all"),
        TextFont::from_font_size(14.0),
        TextColor(Color::srgb_u8(170, 175, 185)),
        Node {
            position_type: PositionType::Absolute,
            left: px(12),
            bottom: px(10),
            ..default()
        },
    ));
}

/// A messy but repeatable position for a title.
fn scrambled(title: &str) -> Vec2 {
    // FNV-1a, so similar titles land far apart.
    let hash = title.bytes().fold(0x811c_9dc5_u32, |h, b| {
        (h ^ b as u32).wrapping_mul(0x0100_0193)
    });
    Vec2::new(
        (hash % 1000) as f32 + 40.0,
        ((hash >> 12) % 540) as f32 + 40.0,
    )
}

fn keys(
    keys: Res<ButtonInput<KeyCode>>,
    graph_entity: Res<Graph>,
    graph: GraphQuery,
    nodes: Query<&NodePosition>,
    texts: Query<&Text>,
    children: Query<&Children>,
    mut commands: Commands,
) {
    let canvas = graph_entity.0;
    let targets: HashMap<Entity, Vec2> = if keys.just_pressed(KeyCode::KeyL) {
        layout(&graph, canvas)
    } else if keys.just_pressed(KeyCode::KeyS) {
        let title = |n: Entity| {
            let kids = children.get(n).into_iter().flatten();
            kids.filter_map(|c| texts.get(*c).ok())
                .next()
                .map(|t| t.0.clone())
        };
        let nodes = graph.nodes_in(canvas);
        nodes
            .map(|n| (n, scrambled(&title(n).unwrap_or_default())))
            .collect()
    } else {
        return;
    };
    // One edit per node, so each move is undoable like a drag.
    for (node, target) in targets {
        if let Ok(position) = nodes.get(node) {
            let delta = target - position.0;
            commands.graph_edit(canvas, GraphEdit::move_nodes(vec![node], delta));
        }
    }
}

/// F: pans and zooms so every node fits, with a margin.
fn frame_all(
    keys: Res<ButtonInput<KeyCode>>,
    graph_entity: Res<Graph>,
    graph: GraphQuery,
    mut views: Query<(&mut CanvasView, &ComputedNode)>,
    nodes: Query<(&NodePosition, &ComputedNode)>,
) {
    let canvas = graph_entity.0;
    let (true, Ok((mut view, computed))) =
        (keys.just_pressed(KeyCode::KeyF), views.get_mut(canvas))
    else {
        return;
    };
    let size = |c: &ComputedNode| c.size() * c.inverse_scale_factor();
    let placed = graph.nodes_in(canvas);
    let bounds = placed
        .filter_map(|n| nodes.get(n).ok())
        .map(|(p, c)| Rect::from_corners(p.0, p.0 + size(c)))
        .reduce(|a, b| a.union(b));
    if let Some(bounds) = bounds {
        let fit = (size(computed) - 80.0).max(Vec2::ONE) / bounds.size().max(Vec2::ONE);
        view.zoom = fit.min_element().clamp(0.1, 1.0);
        view.pan = size(computed) / 2.0 - bounds.center() * view.zoom;
    }
}

/// Layered layout: layer = longest path from a source; within a layer, nodes
/// sit in the average order of their inputs (one barycenter pass).
fn layout(graph: &GraphQuery, canvas: Entity) -> HashMap<Entity, Vec2> {
    let nodes: Vec<Entity> = graph.nodes_in(canvas).collect();
    let inputs_of = |n: Entity| -> Vec<Entity> {
        let ports = graph.inputs_of(n).flat_map(|p| graph.peers_of(p));
        ports.filter_map(|p| graph.node_of(p)).collect()
    };
    let mut layer: HashMap<Entity, usize> = nodes.iter().map(|n| (*n, 0)).collect();
    // Relax until stable (at most one pass per node, as the graph has no cycles).
    for _ in 0..nodes.len() {
        for node in &nodes {
            let deepest = inputs_of(*node)
                .iter()
                .filter_map(|i| layer.get(i).map(|l| l + 1))
                .max()
                .unwrap_or(0);
            layer.insert(*node, deepest);
        }
    }
    let layers = layer.values().max().map_or(0, |l| l + 1);
    let mut order: HashMap<Entity, f32> = HashMap::new();
    let mut targets = HashMap::new();
    for l in 0..layers {
        let mut column: Vec<Entity> = nodes
            .iter()
            .copied()
            .filter(|n| layer.get(n) == Some(&l))
            .collect();
        let center = |n: &Entity| {
            let inputs = inputs_of(*n);
            let sum: f32 = inputs.iter().filter_map(|i| order.get(i)).sum();
            if inputs.is_empty() {
                0.0
            } else {
                sum / inputs.len() as f32
            }
        };
        column.sort_by(|a, b| center(a).total_cmp(&center(b)));
        for (i, node) in column.into_iter().enumerate() {
            order.insert(node, i as f32);
            targets.insert(
                node,
                Vec2::new(40.0, 60.0) + Vec2::new(l as f32, i as f32) * SPACING,
            );
        }
    }
    targets
}
