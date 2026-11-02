//! The optional default look: kit node scenes, Bézier wires, a grid and a selection
//! box, with Bevy's feathers controls inside the nodes. Type numbers into the
//! Number nodes and a template into Format; Print shows the result live.
//! Right-click adds a node; dropping a wire on empty canvas adds a node that
//! accepts it; Delete removes the selection. All of that is plain app code
//! below, so bind it however you like.
//!
//! ```sh
//! cargo run --example styled --features default_style
//! ```

use bevy::feathers::FeathersPlugins;
use bevy::feathers::controls::{
    FeathersNumberInput, FeathersTextInput, FeathersTextInputContainer, NumberInputValue,
};
use bevy::feathers::dark_theme::create_dark_theme;
use bevy::feathers::theme::UiTheme;
use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use bevy::text::{EditableText, TextEditChange};
use bevy::ui_widgets::ValueChange;
use bevy_noodle::prelude::*;
use bevy_noodle::style::{SelectionBoxStyle, kit};

mod feathers_fixes;
use feathers_fixes::FeathersFixesPlugin;

const NUMBER: PortType = PortType::named("number");
const TEXT: PortType = PortType::named("text");
const BLUE: Color = Color::srgb(0.25, 0.52, 0.9);
const GREEN: Color = Color::srgb(0.45, 0.8, 0.5);

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, FeathersPlugins, FeathersFixesPlugin))
        .add_plugins((NoodlePlugins, NoodleDefaultStylePlugin))
        .insert_resource(UiTheme(create_dark_theme()))
        .insert_resource(ClearColor(Color::srgb_u8(24, 25, 29)))
        .add_systems(Startup, setup)
        .add_systems(Update, (delete_selection.run_if(not_typing), evaluate))
        .add_observer(add_on_right_click)
        .add_observer(add_on_wire_drop)
        .add_observer(edit_number)
        .add_observer(edit_template)
        .run();
}

/// The node kinds of this example.
#[derive(Component, Clone, Copy)]
enum Kind {
    Number,
    Add,
    Format,
    Print,
}

/// A Number node's value, edited by its field.
#[derive(Component, Clone, Default)]
struct Value(f32);

/// A Format node's template, edited by its field: `{}` becomes the number.
#[derive(Component, Clone, Default)]
struct Template(String);

/// The text where a Print node shows its input.
#[derive(Component, Clone, Default)]
struct Shows;

fn spawn(commands: &mut Commands, canvas: Entity, kind: Kind, at: Vec2) -> Entity {
    let node = match kind {
        Kind::Number => commands.spawn_scene(bsn! {
            @kit::node(at) kind Value(1.5) ChildOf(canvas)
            Children [
                @kit::title("Number")
                --
                @FeathersNumberInput NumberInputValue::F32(1.5)
                --
                @kit::row("value", Port::output(NUMBER), BLUE)
            ]
        }),
        Kind::Add => commands.spawn_scene(bsn! {
            @kit::node(at) kind ChildOf(canvas)
            Children [
                @kit::title("Add") -- @kit::row("a", Port::input(NUMBER), BLUE)
                -- @kit::row("b", Port::input(NUMBER), BLUE) -- @kit::row("sum", Port::output(NUMBER), BLUE)
            ]
        }),
        Kind::Format => commands.spawn_scene(bsn! {
            // A field-level override: wider than the kit's node, every
            // other `Node` field as the kit sets it.
            @kit::node(at) Node { min_width: px(220) }
            kind Template("sum: {}") ChildOf(canvas)
            Children [
                @kit::title("Format")
                --
                @FeathersTextInputContainer Children [ @FeathersTextInput EditableText::new("sum: {}") ]
                --
                @kit::row("number", Port::input(NUMBER), BLUE)
                --
                @kit::row("text", Port::output(TEXT), GREEN)
            ]
        }),
        Kind::Print => commands.spawn_scene(bsn! {
            @kit::node(at) kind ChildOf(canvas)
            Children [
                @kit::title("Print") -- @kit::row("text", Port::input(TEXT), GREEN)
                -- Shows Text TextFont { font_size: FontSize::Px(15.0) }
            ]
        }),
    };
    node.id()
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    let canvas = commands
        .spawn((
            NodeCanvas,
            CanvasInteraction::default(),
            // Clipping also lets Bevy skip drawing what is out of view.
            Node {
                width: percent(100),
                height: percent(100),
                overflow: Overflow::clip(),
                ..default()
            },
            // Each piece of the default look is opted into here.
            EdgeStyle::default(),
            CanvasGrid::default(),
            SelectionBoxStyle::default(),
        ))
        .id();
    let nodes = [
        (Kind::Number, 60.0, 80.0),
        (Kind::Number, 60.0, 260.0),
        (Kind::Add, 320.0, 150.0),
        (Kind::Format, 560.0, 150.0),
        (Kind::Print, 820.0, 180.0),
    ]
    .map(|(kind, x, y)| spawn(&mut commands, canvas, kind, Vec2::new(x, y)));
    commands.queue(move |world: &mut World| {
        _ = world.run_system_cached_with(connect_demo, (canvas, nodes))
    });
}

fn connect_demo(
    In((canvas, n)): In<(Entity, [Entity; 5])>,
    graph: GraphQuery,
    mut commands: Commands,
) {
    let [a, b, add, format, print] = n;
    for (from, to, input) in [
        (a, add, 0),
        (b, add, 1),
        (add, format, 0),
        (format, print, 0),
    ] {
        let output = graph.outputs_of(from).next();
        if let (Some(from), Some(to)) = (output, graph.inputs_of(to).nth(input)) {
            commands.graph_edit(canvas, GraphEdit::Connect { from, to });
        }
    }
}

/// A Number field's edit sets its node's value.
fn edit_number(change: On<ValueChange<f32>>, graph: GraphQuery, mut values: Query<&mut Value>) {
    if let Some(mut value) = graph
        .node_of(change.source)
        .and_then(|n| values.get_mut(n).ok())
    {
        value.0 = change.value;
    }
}

/// A Format field's edit sets its node's template.
fn edit_template(
    change: On<TextEditChange>,
    graph: GraphQuery,
    fields: Query<&EditableText>,
    mut templates: Query<&mut Template>,
) {
    let field = change.event_target();
    let node = graph.node_of(field);
    if let (Ok(text), Some(mut template)) = (
        fields.get(field),
        node.and_then(|n| templates.get_mut(n).ok()),
    ) {
        template.0 = text.value().to_string();
    }
}

/// What flows along a wire.
enum Data {
    Number(f32),
    Text(String),
}

type Kinds = (
    &'static Kind,
    Option<&'static Value>,
    Option<&'static Template>,
);

/// What `node` outputs, following its inputs back through the graph.
fn output(node: Entity, graph: &GraphQuery, kinds: &Query<Kinds>, depth: u8) -> Option<Data> {
    let input = |i: usize| {
        let peer = graph.source_of(node, i)?;
        // Wires can form a loop; give up on a deep chain.
        output(graph.node_of(peer)?, graph, kinds, depth.checked_sub(1)?)
    };
    let number = |i| match input(i) {
        Some(Data::Number(n)) => n,
        _ => 0.0,
    };
    let (kind, value, template) = kinds.get(node).ok()?;
    Some(match kind {
        Kind::Number => Data::Number(value?.0),
        Kind::Add => Data::Number(number(0) + number(1)),
        Kind::Format => Data::Text(template?.0.replace("{}", &number(0).to_string())),
        Kind::Print => return None,
    })
}

/// Shows each Print node's input, every frame.
fn evaluate(
    graph: GraphQuery,
    kinds: Query<Kinds>,
    mut shown: Query<(&mut Text, &ChildOf), With<Shows>>,
) {
    for (mut text, parent) in &mut shown {
        let peer = graph.source_of(parent.parent(), 0);
        let data = peer.and_then(|p| output(graph.node_of(p)?, &graph, &kinds, 32));
        let shown = match data {
            Some(Data::Text(text)) => text,
            Some(Data::Number(n)) => n.to_string(),
            None => "(not connected)".into(),
        };
        text.set_if_neq(Text(shown));
    }
}

/// Right-click on empty canvas adds a Number node there.
fn add_on_right_click(
    click: On<PointerClick>,
    graph: GraphQuery,
    views: Query<&CanvasView>,
    mut commands: Commands,
) {
    let canvas = click.event_target();
    let Ok(view) = views.get(canvas) else {
        return;
    };
    if click.button == PointerButton::Secondary
        && graph.node_of(click.original_event_target()).is_none()
    {
        // The canvas fills the window here, so window and canvas coordinates match.
        let at = view.canvas_to_graph(click.pointer.position);
        spawn(&mut commands, canvas, Kind::Number, at);
    }
}

/// A wire dropped on empty canvas gets a node that accepts it, connected.
fn add_on_wire_drop(dropped: On<WireDropped>, graph: GraphQuery, mut commands: Commands) {
    let Some(port) = graph.port(dropped.from) else {
        return;
    };
    let kind = match (port.direction, port.port_type == TEXT) {
        (PortDirection::Output, true) => Kind::Print,
        (PortDirection::Output, false) => Kind::Format,
        (PortDirection::Input, true) => Kind::Format,
        (PortDirection::Input, false) => Kind::Number,
    };
    let (canvas, from) = (dropped.canvas, dropped.from);
    let node = spawn(&mut commands, canvas, kind, dropped.position);
    commands.queue(move |world: &mut World| {
        let fits = |In((canvas, from, node)): In<(Entity, Entity, Entity)>, g: GraphQuery| {
            g.ports_of(node).find(|to| {
                g.check_connection(canvas, from, *to)
                    .is_ok_and(|c| c.allowed())
            })
        };
        if let Ok(Some(to)) = world.run_system_cached_with(fits, (canvas, from, node)) {
            world
                .graph_edit(canvas, GraphEdit::Connect { from, to })
                .ok();
        }
    });
}

/// Shortcuts are off while a text field has the focus.
fn not_typing(focus: Res<InputFocus>, fields: Query<(), With<EditableText>>) -> bool {
    focus.get().is_none_or(|f| !fields.contains(f))
}

/// A key binding is just a system triggering an edit.
fn delete_selection(
    keys: Res<ButtonInput<KeyCode>>,
    graph: GraphQuery,
    canvases: Query<Entity, With<NodeCanvas>>,
    mut commands: Commands,
) {
    if keys.just_pressed(KeyCode::Delete) {
        for canvas in &canvases {
            let items = graph.selected_in(canvas).collect();
            commands.graph_edit(canvas, GraphEdit::Delete { items });
        }
    }
}
