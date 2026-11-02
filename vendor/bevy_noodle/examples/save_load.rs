//! Saving a graph to a RON file and loading it back (feature `scene`).
//!
//! A snapshot is a Bevy `DynamicWorld` of everything in the canvas:
//! nodes, ports, edges, and your own reflected components, entity references
//! included (`Shows` below points at a text entity and survives the trip).
//!
//! A snapshot keeps every UI detail, so it is large. For a small file, save
//! the graph model instead (M, and O to open it): what each node is, where,
//! and how they connect; loading rebuilds the UI with ordinary spawns and
//! connects.
//!
//! Type into a Number's field to change its value; the Sum updates live, and
//! both kinds of save keep it. Fields are `Transient`: a field is
//! rebuilt for every node that gets a `Value`, loaded ones included.
//!
//! S saves a snapshot, L loads it, M saves the model, O opens it, N clears
//! (not while typing in a field). Right-click adds a number, Delete removes
//! the selection.
//!
//! ```sh
//! cargo run --example save_load --features default_style,scene
//! ```

use std::path::PathBuf;

use bevy::feathers::FeathersPlugins;
use bevy::feathers::controls::{FeathersNumberInput, NumberInputValue};
use bevy::feathers::dark_theme::create_dark_theme;
use bevy::feathers::theme::UiTheme;
use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use bevy::text::EditableText;
use bevy::ui_widgets::ValueChange;
use bevy::world_serialization::serde::WorldDeserializer;
use bevy_noodle::prelude::*;
use bevy_noodle::style::kit;
use serde::de::DeserializeSeed;

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
        .add_systems(Update, (keys.run_if(not_typing), add_fields, evaluate))
        .add_observer(add_on_right_click)
        .add_observer(edit_number)
        .run();
}

/// A number node's value. Reflected, so it is saved.
#[derive(Component, Reflect, Clone, Default)]
#[reflect(Component)]
struct Value(f32);

/// A sum node's result text: an entity reference, remapped on load.
#[derive(Component, Reflect)]
#[reflect(Component)]
struct Shows(#[entities] Entity);

#[derive(Resource)]
struct Graph(Entity);

#[derive(Component)]
struct Status;

/// The graph as plain data.
#[derive(serde::Serialize, serde::Deserialize)]
struct Model {
    /// Each node, with its top-left corner.
    nodes: Vec<(Kind, [f32; 2])>,
    /// `[from node, output, to node, input]`, by index.
    edges: Vec<[usize; 4]>,
}

#[derive(serde::Serialize, serde::Deserialize)]
enum Kind {
    Number(f32),
    Sum,
}

fn model_file() -> PathBuf {
    std::env::temp_dir().join("bevy_noodle_graph.model.ron")
}

fn file() -> PathBuf {
    std::env::temp_dir().join("bevy_noodle_graph.scn.ron")
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    let canvas = commands.spawn_scene(kit::canvas()).id();
    commands.insert_resource(Graph(canvas));
    let numbers = [(2.0, 100.0), (3.5, 230.0), (4.0, 360.0)]
        .map(|(value, y)| spawn_number(&mut commands, canvas, value, Vec2::new(80.0, y)));
    let sum = spawn_sum(&mut commands, canvas, Vec2::new(420.0, 210.0));
    commands.queue(move |world: &mut World| {
        let ports = |In((numbers, sum)): In<([Entity; 3], Entity)>, graph: GraphQuery| {
            let to = graph.inputs_of(sum).next();
            let ports = numbers.map(|n| Some((graph.outputs_of(n).next()?, to?)));
            ports.into_iter().flatten().collect::<Vec<_>>()
        };
        let pairs = world.run_system_cached_with(ports, (numbers, sum));
        for (from, to) in pairs.unwrap_or_default() {
            world
                .graph_edit(canvas, GraphEdit::Connect { from, to })
                .ok();
        }
    });
    commands.spawn((
        Text::new(format!(
            "S/L: save/load snapshot | M/O: save/open model | N: clear    {}",
            std::env::temp_dir().display()
        )),
        TextFont::from_font_size(14.0),
        TextColor(Color::srgb_u8(170, 175, 185)),
        Status,
        Node {
            position_type: PositionType::Absolute,
            left: px(12),
            bottom: px(10),
            ..default()
        },
    ));
}

fn spawn_number(commands: &mut Commands, canvas: Entity, value: f32, at: Vec2) -> Entity {
    let scene = bsn! {
        @kit::node(at) Value(value) ChildOf(canvas)
        Children [ @kit::title("Number") -- @kit::row("value", Port::output(NUMBER), BLUE) ]
    };
    commands.spawn_scene(scene).id()
}

/// Every node that gets a `Value` (spawned or loaded) gets a field under its
/// title showing it.
fn add_fields(nodes: Query<(Entity, &Value), Added<Value>>, mut commands: Commands) {
    for (node, value) in &nodes {
        let value = value.0;
        let field = commands
            .spawn_scene(bsn! { @FeathersNumberInput NumberInputValue::F32({value}) Transient })
            .id();
        commands.entity(node).insert_child(1, field);
    }
}

/// A field's edit sets its node's value.
fn edit_number(change: On<ValueChange<f32>>, graph: GraphQuery, mut values: Query<&mut Value>) {
    if let Some(mut value) = graph
        .node_of(change.source)
        .and_then(|n| values.get_mut(n).ok())
    {
        value.0 = change.value;
    }
}

/// Shortcuts are off while a text field has the focus.
fn not_typing(focus: Res<InputFocus>, fields: Query<(), With<EditableText>>) -> bool {
    focus.get().is_none_or(|f| !fields.contains(f))
}

fn spawn_sum(commands: &mut Commands, canvas: Entity, at: Vec2) -> Entity {
    let inputs = Port::input(NUMBER).with_capacity(Capacity::Unlimited);
    let scene = bsn! {
        @kit::node(at) ChildOf(canvas)
        Children [ @kit::title("Sum") -- @kit::row("values", inputs, BLUE) ]
    };
    let node = commands.spawn_scene(scene).id();
    let result = commands
        .spawn((
            Text::new("= ?"),
            TextColor(ORANGE),
            TextFont::from_font_size(22.0),
            ChildOf(node),
        ))
        .id();
    commands.entity(node).try_insert(Shows(result));
    node
}

/// Sum nodes show the total of the numbers wired into them.
fn evaluate(
    graph: GraphQuery,
    sums: Query<(Entity, &Shows)>,
    values: Query<&Value>,
    mut texts: Query<&mut Text>,
) {
    for (node, shows) in &sums {
        let total: f32 = graph
            .inputs_of(node)
            .flat_map(|input| graph.peers_of(input))
            .filter_map(|output| values.get(graph.node_of(output)?).ok())
            .map(|value| value.0)
            .sum();
        if let Ok(mut text) = texts.get_mut(shows.0) {
            text.set_if_neq(Text(format!("= {total}")));
        }
    }
}

fn keys(
    keys: Res<ButtonInput<KeyCode>>,
    graph: GraphQuery,
    canvas: Res<Graph>,
    mut commands: Commands,
) {
    if keys.just_pressed(KeyCode::Delete) || keys.just_pressed(KeyCode::Backspace) {
        let items = graph.selected_in(canvas.0).collect();
        commands.graph_edit(canvas.0, GraphEdit::Delete { items });
    }
    let action: fn(&mut World, Entity) -> Result<String> = match () {
        _ if keys.just_pressed(KeyCode::KeyS) => save,
        _ if keys.just_pressed(KeyCode::KeyL) => load,
        _ if keys.just_pressed(KeyCode::KeyN) => clear,
        _ if keys.just_pressed(KeyCode::KeyM) => save_model,
        _ if keys.just_pressed(KeyCode::KeyO) => open_model,
        _ => return,
    };
    let canvas = canvas.0;
    commands.queue(move |world: &mut World| {
        let message = action(world, canvas).unwrap_or_else(|error| format!("failed: {error}"));
        info!("{message}");
        let mut status = world.query_filtered::<&mut Text, With<Status>>();
        if let Ok(mut status) = status.single_mut(world) {
            status.0 = message;
        }
    });
}

fn save(world: &mut World, canvas: Entity) -> Result<String> {
    let snapshot = world.snapshot(canvas).ok_or("not a canvas")?;
    let ron = snapshot.serialize(
        &world
            .get_resource::<AppTypeRegistry>()
            .ok_or("no registry")?
            .read(),
    )?;
    std::fs::write(file(), &ron)?;
    let (entities, bytes) = (snapshot.entities.len(), ron.len());
    Ok(format!(
        "saved a snapshot: {entities} entities, {bytes} bytes"
    ))
}

fn load(world: &mut World, canvas: Entity) -> Result<String> {
    let ron = std::fs::read_to_string(file())?;
    let registry = world
        .get_resource::<AppTypeRegistry>()
        .ok_or("no registry")?
        .clone();
    // Asset handles in the file (fonts, images) load through the asset server.
    let mut assets = world
        .get_resource::<AssetServer>()
        .ok_or("no asset server")?
        .clone();
    let snapshot = WorldDeserializer {
        type_registry: &registry.read(),
        load_from_path: &mut assets,
    }
    .deserialize(&mut ron::Deserializer::from_str(&ron)?)?;
    world.restore_snapshot(canvas, &snapshot)?;
    Ok(format!("loaded {} entities", snapshot.entities.len()))
}

fn clear(world: &mut World, canvas: Entity) -> Result<String> {
    world.restore_snapshot(canvas, &default())?;
    Ok("cleared".into())
}

fn save_model(world: &mut World, canvas: Entity) -> Result<String> {
    let read =
        |In(canvas), graph: GraphQuery, values: Query<&Value>, positions: Query<&NodePosition>| {
            let nodes: Vec<Entity> = graph.nodes_in(canvas).collect();
            let index = |n| nodes.iter().position(|m| *m == n);
            let kind = |n| values.get(n).map_or(Kind::Sum, |v| Kind::Number(v.0));
            let at = |n| positions.get(n).map_or([0.0; 2], |p| p.0.to_array());
            let edges = graph.edges_in(canvas).filter_map(|edge| {
                let PortPair { output, input } = graph.edge_ports(edge)?;
                let (from, to) = (graph.node_of(output)?, graph.node_of(input)?);
                let output = graph.outputs_of(from).position(|p| p == output)?;
                let input = graph.inputs_of(to).position(|p| p == input)?;
                Some([index(from)?, output, index(to)?, input])
            });
            let edges = edges.collect();
            Model {
                nodes: nodes.iter().map(|n| (kind(*n), at(*n))).collect(),
                edges,
            }
        };
    let model = world.run_system_cached_with(read, canvas)?;
    let ron = ron::ser::to_string_pretty(&model, default())?;
    std::fs::write(model_file(), &ron)?;
    Ok(format!(
        "saved the model: {} nodes, {} bytes",
        model.nodes.len(),
        ron.len()
    ))
}

fn open_model(world: &mut World, canvas: Entity) -> Result<String> {
    let model: Model = ron::from_str(&std::fs::read_to_string(model_file())?)?;
    clear(world, canvas)?;
    let mut commands = world.commands();
    let nodes: Vec<Entity> = (model.nodes.iter())
        .map(|(kind, [x, y])| match kind {
            Kind::Number(value) => spawn_number(&mut commands, canvas, *value, Vec2::new(*x, *y)),
            Kind::Sum => spawn_sum(&mut commands, canvas, Vec2::new(*x, *y)),
        })
        .collect();
    world.flush();
    for [from, output, to, input] in &model.edges {
        let ports = |In((a, b)): In<(Entity, Entity)>, graph: GraphQuery| {
            let outputs: Vec<Entity> = graph.outputs_of(a).collect();
            (outputs, graph.inputs_of(b).collect::<Vec<_>>())
        };
        let ends = nodes.get(*from).zip(nodes.get(*to));
        let (&a, &b) = ends.ok_or("an edge names a missing node")?;
        let (outputs, inputs) = world.run_system_cached_with(ports, (a, b))?;
        let ends = outputs.get(*output).zip(inputs.get(*input));
        let (&from, &to) = ends.ok_or("an edge names a missing port")?;
        world
            .graph_edit(canvas, GraphEdit::Connect { from, to })
            .ok();
    }
    Ok(format!("opened the model: {} nodes", nodes.len()))
}

fn add_on_right_click(
    click: On<PointerClick>,
    graph: GraphQuery,
    views: Query<&CanvasView>,
    values: Query<(), With<Value>>,
    mut commands: Commands,
) {
    let canvas = click.event_target();
    let Ok(view) = views.get(canvas) else {
        return;
    };
    if click.button == PointerButton::Secondary
        && graph.node_of(click.original_event_target()).is_none()
    {
        let at = view.canvas_to_graph(click.pointer.position);
        let value = (values.iter().count() % 9 + 1) as f32;
        spawn_number(&mut commands, canvas, value, at);
    }
}
