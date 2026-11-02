//! Editor commands from snapshots (feature `scene`), in plain app code:
//!
//! - Undo and redo: every applied edit that matters (not selection, and only
//!   the final step of a drag) records a whole-graph snapshot; restoring one
//!   swaps the graph back. Ctrl/Cmd+Z undoes, Ctrl/Cmd+Shift+Z or Ctrl+Y redoes.
//! - Copy, paste and duplicate: Ctrl/Cmd+C snapshots the selected nodes with
//!   the edges between them, Ctrl/Cmd+V inserts a copy, Ctrl/Cmd+D does both.
//! - Click an edge to select it; Delete removes selected nodes and edges.
//!   Right-click adds a node on empty canvas, or removes the edge under it.
//! - Type into a Number's field: Add shows the sum live, and the finished
//!   edit (Enter or leaving the field) is recorded for undo too. Fields are
//!   `Transient`: snapshots keep the reflected `Value`, and a field
//!   is rebuilt for every node that gets one.
//!
//! ```sh
//! cargo run --example editor --features default_style,scene
//! ```

use bevy::feathers::FeathersPlugins;
use bevy::feathers::controls::{FeathersNumberInput, NumberInputValue};
use bevy::feathers::dark_theme::create_dark_theme;
use bevy::feathers::theme::UiTheme;
use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use bevy::text::EditableText;
use bevy::ui::Selected;
use bevy::ui_widgets::ValueChange;
use bevy::world_serialization::DynamicWorld;
use bevy_noodle::prelude::*;
use bevy_noodle::style::kit;

mod feathers_fixes;
use feathers_fixes::FeathersFixesPlugin;

const NUMBER: PortType = PortType::named("number");
const BLUE: Color = Color::srgb(0.25, 0.52, 0.9);

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, FeathersPlugins, FeathersFixesPlugin))
        .add_plugins((NoodlePlugins, NoodleDefaultStylePlugin))
        .insert_resource(UiTheme(create_dark_theme()))
        .insert_resource(ClearColor(Color::srgb_u8(24, 25, 29)))
        .init_resource::<History>()
        .init_resource::<Clipboard>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                record_edits,
                add_fields,
                show_sums,
                (shortcuts, delete_selection).run_if(not_typing),
                show_history.run_if(resource_changed::<History>),
            ),
        )
        .add_observer(add_on_right_click)
        .add_observer(edit_number)
        .run();
}

/// Snapshots of the graph: `current` is what is on screen.
#[derive(Resource, Default)]
struct History {
    undo: Vec<DynamicWorld>,
    redo: Vec<DynamicWorld>,
    current: Option<DynamicWorld>,
}

/// Copied nodes, pasted with an offset.
#[derive(Resource, Default)]
struct Clipboard(Option<DynamicWorld>);

#[derive(Resource)]
struct Graph(Entity);

#[derive(Component)]
struct HistoryText;

/// A Number node's value: reflected, so snapshots keep it.
#[derive(Component, Reflect, Clone, Copy, Default)]
#[reflect(Component)]
struct Value(f32);

/// The text where an Add node shows its sum.
#[derive(Component, Reflect, Clone, Default)]
#[reflect(Component)]
struct Sum;

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    let canvas = commands.spawn_scene(kit::canvas()).id();
    commands.insert_resource(Graph(canvas));
    let a = spawn_number(&mut commands, canvas, Vec2::new(80.0, 120.0), 2.0);
    let b = spawn_number(&mut commands, canvas, Vec2::new(80.0, 300.0), 3.5);
    let add = commands
        .spawn_scene(bsn! {
            @kit::node(Vec2::new(380.0, 190.0)) ChildOf(canvas)
            Children [
                @kit::title("Add") -- @kit::row("a", Port::input(NUMBER), BLUE)
                -- @kit::row("b", Port::input(NUMBER), BLUE) -- @kit::row("sum", Port::output(NUMBER), BLUE)
                -- Sum Text TextFont { font_size: FontSize::Px(15.0) }
            ]
        })
        .id();
    commands.queue(move |world: &mut World| {
        let connect = |In((a, b, add)): In<(Entity, Entity, Entity)>, graph: GraphQuery| {
            [a, b]
                .into_iter()
                .zip(graph.inputs_of(add))
                .filter_map(|(n, to)| Some((graph.outputs_of(n).next()?, to)))
                .collect::<Vec<_>>()
        };
        let pairs = world.run_system_cached_with(connect, (a, b, add));
        for (from, to) in pairs.into_iter().flatten() {
            world
                .graph_edit(canvas, GraphEdit::Connect { from, to })
                .ok();
        }
    });
    commands.spawn((
        Text::default(),
        TextFont::from_font_size(14.0),
        TextColor(Color::srgb_u8(170, 175, 185)),
        HistoryText,
        Node {
            position_type: PositionType::Absolute,
            left: px(12),
            bottom: px(10),
            ..default()
        },
    ));
}

fn spawn_number(commands: &mut Commands, canvas: Entity, at: Vec2, value: f32) -> Entity {
    let scene = bsn! {
        @kit::node(at) Value(value) ChildOf(canvas)
        Children [ @kit::title("Number") -- @kit::row("value", Port::output(NUMBER), BLUE) ]
    };
    commands.spawn_scene(scene).id()
}

/// Every node that gets a `Value` (spawned, restored or pasted) gets a field
/// under its title showing it.
fn add_fields(nodes: Query<(Entity, &Value), Added<Value>>, mut commands: Commands) {
    for (node, value) in &nodes {
        let value = value.0;
        let field = commands
            .spawn_scene(bsn! { @FeathersNumberInput NumberInputValue::F32({value}) Transient })
            .id();
        commands.entity(node).insert_child(1, field);
    }
}

/// Typing sets the value live; the finished edit is recorded for undo.
fn edit_number(
    change: On<ValueChange<f32>>,
    graph: GraphQuery,
    mut values: Query<&mut Value>,
    mut edited: Local<bool>,
    mut commands: Commands,
) {
    let Some(mut value) = graph
        .node_of(change.source)
        .and_then(|n| values.get_mut(n).ok())
    else {
        return;
    };
    *edited |= value.0 != change.value;
    value.0 = change.value;
    if change.is_final && std::mem::take(&mut *edited) {
        commands.queue(record);
    }
}

/// Each Add node shows the sum of what flows into it.
fn show_sums(
    graph: GraphQuery,
    values: Query<&Value>,
    mut sums: Query<(&mut Text, &ChildOf), With<Sum>>,
) {
    for (mut text, node) in &mut sums {
        let sum = sum_of(node.parent(), &graph, &values, 32);
        text.set_if_neq(Text(format!("= {sum}")));
    }
}

/// A node's number: its `Value`, or the sum of its inputs.
fn sum_of(node: Entity, graph: &GraphQuery, values: &Query<&Value>, depth: u8) -> f32 {
    if let Ok(value) = values.get(node) {
        return value.0;
    }
    let Some(depth) = depth.checked_sub(1) else {
        return 0.0; // Wires can form a loop; give up on a deep chain.
    };
    let inputs = graph.inputs_of(node);
    let peers = inputs.flat_map(|p| graph.peers_of(p));
    let nodes = peers.filter_map(|p| graph.node_of(p));
    nodes.fold(0.0, |sum, n| sum + sum_of(n, graph, values, depth))
}

/// Edits worth undoing record a snapshot once they have applied.
fn record_edits(mut applied: MessageReader<EditApplied>, mut commands: Commands) {
    // Read every message (`any` would stop early and leave some for next frame).
    if applied.read().filter(|e| !e.change.is_drag_step()).count() > 0 {
        commands.queue(record);
    }
}

fn record(world: &mut World) {
    let Some(now) = world
        .get_resource::<Graph>()
        .and_then(|g| world.snapshot(g.0))
    else {
        return;
    };
    let mut history = world.get_resource_or_init::<History>();
    if let Some(before) = history.current.replace(now) {
        history.undo.push(before);
    }
    history.redo.clear();
}

fn shortcuts(keys: Res<ButtonInput<KeyCode>>, mut commands: Commands) {
    use KeyCode::*;
    if !keys.any_pressed([ControlLeft, ControlRight, SuperLeft, SuperRight]) {
        return;
    }
    let shift = keys.any_pressed([ShiftLeft, ShiftRight]);
    match () {
        _ if keys.just_pressed(KeyZ) => commands.queue(move |w: &mut World| step(w, shift)),
        _ if keys.just_pressed(KeyY) => commands.queue(|w: &mut World| step(w, true)),
        _ if keys.just_pressed(KeyC) => commands.queue(copy),
        _ if keys.just_pressed(KeyV) => commands.queue(paste),
        _ if keys.just_pressed(KeyD) => commands.queue(|w: &mut World| {
            copy(w);
            paste(w);
        }),
        _ => {}
    }
}

fn copy(world: &mut World) {
    let mut selected = world.query_filtered::<Entity, (With<GraphNode>, With<Selected>)>();
    let nodes: Vec<_> = selected.iter(world).collect();
    if !nodes.is_empty() {
        let copied = world.snapshot_nodes(&nodes);
        world.get_resource_or_init::<Clipboard>().0 = Some(copied);
    }
}

fn paste(world: &mut World) {
    let Some(&Graph(canvas)) = world.get_resource() else {
        return;
    };
    let Some(copied) = world.get_resource_or_init::<Clipboard>().0.take() else {
        return;
    };
    let pasted = match world.insert_snapshot(canvas, &copied) {
        Ok(map) => copied
            .entities
            .iter()
            .filter_map(|e| map.get(&e.entity).copied())
            .collect::<Vec<_>>(),
        Err(error) => return error!("pasting failed: {error}"),
    };
    // Top-level pasted nodes (not ones inside a pasted node) move aside.
    let top_level = |w: &World, e: Entity| {
        w.get::<ChildOf>(e)
            .is_some_and(|p| !pasted.contains(&p.parent()))
    };
    let mut nodes = Vec::new();
    for &entity in &pasted {
        if top_level(world, entity)
            && let Some(mut position) = world.get_mut::<NodePosition>(entity)
        {
            position.0 += Vec2::splat(32.0);
            nodes.push(entity);
        }
    }
    // The next paste lands a step further along.
    let next = world.snapshot_nodes(&nodes);
    world.get_resource_or_init::<Clipboard>().0 = Some(next);
    world.select(canvas, nodes, SelectMode::Replace);
    record(world);
}

/// Move one snapshot between the stacks and put it on screen.
fn step(world: &mut World, redo: bool) {
    let Some(&Graph(canvas)) = world.get_resource() else {
        return;
    };
    world.try_resource_scope(|world, mut history: Mut<History>| {
        let History {
            undo,
            redo: redone,
            current,
        } = &mut *history;
        let (from, to) = if redo { (redone, undo) } else { (undo, redone) };
        let (Some(target), Some(shown)) = (from.pop(), current.take()) else {
            return;
        };
        to.push(shown);
        if let Err(error) = world.restore_snapshot(canvas, &target) {
            error!("restoring a snapshot failed: {error}");
        }
        *current = Some(target);
    });
}

fn show_history(history: Res<History>, mut text: Single<&mut Text, With<HistoryText>>) {
    text.0 = format!(
        "{} undo | {} redo    Ctrl/Cmd + Z: undo, Shift+Z: redo, C: copy, V: paste, D: duplicate | Delete: remove | right-click: add node, remove edge",
        history.undo.len(),
        history.redo.len()
    );
}

fn add_on_right_click(
    click: On<PointerClick>,
    graph: GraphQuery,
    views: Query<&CanvasView>,
    mut commands: Commands,
) {
    let (target, clicked) = (click.event_target(), click.original_event_target());
    if click.button != PointerButton::Secondary {
        return;
    }
    // Edges get pointer events like any UI entity, but have no parent canvas.
    if let (Some(canvas), true) = (graph.canvas_of(target), graph.edge_ports(target).is_some()) {
        commands.graph_edit(canvas, GraphEdit::Disconnect { edge: target });
        return;
    }
    let canvas = target;
    let Ok(view) = views.get(canvas) else {
        return;
    };
    if graph.node_of(clicked).is_none() {
        let at = view.canvas_to_graph(click.pointer.position);
        spawn_number(&mut commands, canvas, at, 0.0);
        // Spawning is not a graph edit, so record it here.
        commands.queue(record);
    }
}

/// Shortcuts are off while a text field has the focus.
fn not_typing(focus: Res<InputFocus>, fields: Query<(), With<EditableText>>) -> bool {
    focus.get().is_none_or(|f| !fields.contains(f))
}

/// Selected nodes and edges.
fn delete_selection(
    keys: Res<ButtonInput<KeyCode>>,
    graph: GraphQuery,
    canvas: Res<Graph>,
    mut commands: Commands,
) {
    if keys.just_pressed(KeyCode::Delete) || keys.just_pressed(KeyCode::Backspace) {
        let items = graph.selected_in(canvas.0).collect();
        commands.graph_edit(canvas.0, GraphEdit::Delete { items });
    }
}
