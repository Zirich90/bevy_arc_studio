//! A node graph that builds a 3D scene: shapes, colors and motion feed Object
//! nodes, Ring nodes copy objects around a circle, and everything wired into
//! Scene is spawned as real `Mesh3d` entities, rebuilt whenever an edit
//! applies. The graph is a translucent panel over the 3D view, with no window
//! of its own.
//!
//! Every value is a control in its node: a shape dropdown, color pickers,
//! spin and size sliders, a ring count field. Change any of them, or rewire
//! anything, and the scene follows live. Right-click adds a node.
//!
//! ```sh
//! cargo run --example scene_builder_3d --features default_style
//! ```

use std::f32::consts::TAU;

use bevy::feathers::FeathersPlugins;
use bevy::feathers::controls::{
    ColorChannel, ColorPlaneValue, FeathersColorPlane, FeathersColorSlider, FeathersMenu,
    FeathersMenuButton, FeathersMenuItem, FeathersMenuPopup, FeathersNumberInput, FeathersSlider,
    HardLimit, NumberInputValue, SliderBaseColor,
};
use bevy::feathers::dark_theme::create_dark_theme;
use bevy::feathers::theme::{ThemedText, UiTheme};
use bevy::prelude::*;
use bevy::ui_widgets::{Activate, SliderPrecision, SliderValue, ValueChange, slider_self_update};
use bevy_noodle::prelude::*;
use bevy_noodle::style::{SelectedBorderColor, kit};

mod feathers_fixes;
use feathers_fixes::FeathersFixesPlugin;

const SHAPE: PortType = PortType::named("shape");
const PAINT: PortType = PortType::named("paint");
const MOTION: PortType = PortType::named("motion");
const OBJECT: PortType = PortType::named("object");
const GREY: Color = Color::srgb(0.7, 0.72, 0.76);
const PURPLE: Color = Color::srgb(0.66, 0.5, 0.95);
const GREEN: Color = Color::srgb(0.45, 0.8, 0.5);
const ORANGE: Color = Color::srgb(0.95, 0.6, 0.25);
const SKY: Color = Color::srgb(0.1, 0.11, 0.13);

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, FeathersPlugins, FeathersFixesPlugin))
        .add_plugins((NoodlePlugins, NoodleDefaultStylePlugin))
        .insert_resource(UiTheme(create_dark_theme()))
        .insert_resource(ClearColor(SKY))
        .add_systems(Startup, setup)
        .add_systems(Update, (show_paints, rebuild.run_if(graph_changed), spin))
        .add_observer(add_on_right_click)
        .add_observer(choose_shape)
        .add_observer(edit_number)
        .add_observer(edit_count)
        .add_observer(edit_hue)
        .run();
}

#[derive(Clone, Copy, PartialEq, Default)]
enum Shape {
    #[default]
    Cube,
    Sphere,
    Torus,
}

impl Shape {
    const ALL: [Shape; 3] = [Shape::Cube, Shape::Sphere, Shape::Torus];

    fn name(self) -> &'static str {
        match self {
            Shape::Cube => "Cube",
            Shape::Sphere => "Sphere",
            Shape::Torus => "Torus",
        }
    }
}

/// What a node is, with the value its controls edit.
#[derive(Component, Clone, Copy)]
enum Kind {
    Shape(Shape),
    Paint(Hsla),
    /// Turns per second.
    Spin(f32),
    /// Size.
    Object(f32),
    Ring(i32),
    Scene,
}

/// A shape dropdown's item.
#[derive(Component, Clone, Default)]
struct Choice(Shape);

/// A shape dropdown's caption.
#[derive(Component, Clone, Default)]
struct Caption;

/// Marks the 3D entities built from the graph.
#[derive(Component)]
struct Built;

/// Speed and starting rotation, so a rebuild keeps the phase.
#[derive(Component)]
struct Spin(f32, Quat);

#[derive(Resource)]
struct Meshes([Handle<Mesh>; 3]);

impl Meshes {
    fn of(&self, shape: Shape) -> Handle<Mesh> {
        let [cube, sphere, torus] = &self.0;
        match shape {
            Shape::Cube => cube,
            Shape::Sphere => sphere,
            Shape::Torus => torus,
        }
        .clone()
    }
}

/// One object to spawn.
#[derive(Clone)]
struct Instance {
    shape: Shape,
    color: Color,
    spin: Option<f32>,
    transform: Transform,
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(Meshes([
        meshes.add(Cuboid::from_length(1.0)),
        meshes.add(Sphere::new(0.6)),
        meshes.add(Torus::new(0.3, 0.6)),
    ]));
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(-2.6, 4.2, 8.5).looking_at(Vec3::new(-2.6, 0.4, 0.0), Vec3::Y),
        DistanceFog {
            color: SKY,
            falloff: FogFalloff::Linear {
                start: 12.0,
                end: 40.0,
            },
            ..default()
        },
    ));
    commands.spawn((
        DirectionalLight {
            shadow_maps_enabled: true,
            illuminance: 6000.0,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(200.0, 200.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.16, 0.17, 0.2))),
        Transform::from_xyz(0.0, -0.5, 0.0),
    ));

    // The graph panel: a canvas like any other, with a translucent
    // background. Each patch changes only the fields it names.
    let canvas = commands
        .spawn_scene(bsn! {
            @kit::canvas()
            Node { width: px(640), border: UiRect::right(px(1)) }
            BackgroundColor(Color::srgba(0.07, 0.08, 0.1, 0.82))
            BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.1))
        })
        .id();
    let coral = Hsla::hsl(5.0, 0.9, 0.65);
    let teal = Hsla::hsl(180.0, 0.6, 0.45);
    let n = [
        (Kind::Shape(Shape::Cube), 20.0, 20.0),
        (Kind::Shape(Shape::Torus), 20.0, 410.0),
        (Kind::Paint(coral), 20.0, 125.0),
        (Kind::Paint(teal), 20.0, 515.0),
        (Kind::Spin(1.2), 20.0, 305.0),
        (Kind::Object(1.0), 230.0, 40.0),
        (Kind::Object(1.0), 230.0, 410.0),
        (Kind::Ring(8), 440.0, 300.0),
        (Kind::Scene, 440.0, 170.0),
        (Kind::Shape(Shape::Sphere), 440.0, 30.0),
        (Kind::Paint(Hsla::hsl(42.0, 0.85, 0.6)), 440.0, 460.0),
    ]
    .map(|(kind, x, y)| spawn(&mut commands, canvas, kind, Vec2::new(x, y)));
    // (from node, output, to node, input)
    let wires = [
        (n[0], 0, n[5], 0),
        (n[2], 0, n[5], 1),
        (n[4], 0, n[5], 2),
        (n[1], 0, n[6], 0),
        (n[3], 0, n[6], 1),
        (n[4], 0, n[6], 2),
        (n[6], 0, n[7], 0),
        (n[5], 0, n[8], 0),
        (n[7], 0, n[8], 0),
    ];
    commands.queue(move |world: &mut World| {
        let ports = |In(wires): In<[(Entity, usize, Entity, usize); 9]>, g: GraphQuery| {
            wires
                .into_iter()
                .filter_map(|(a, i, b, j)| Some((g.outputs_of(a).nth(i)?, g.inputs_of(b).nth(j)?)))
                .collect::<Vec<_>>()
        };
        let pairs = world.run_system_cached_with(ports, wires);
        for (from, to) in pairs.into_iter().flatten() {
            world
                .graph_edit(canvas, GraphEdit::Connect { from, to })
                .ok();
        }
    });
}

fn spawn(commands: &mut Commands, canvas: Entity, kind: Kind, at: Vec2) -> Entity {
    let (title, inputs, outputs): (_, &[_], &[_]) = match kind {
        Kind::Shape(_) => ("Shape", &[], &[("shape", SHAPE, GREY)]),
        Kind::Paint(_) => ("Paint", &[], &[("color", PAINT, PURPLE)]),
        Kind::Spin(_) => ("Spin", &[], &[("motion", MOTION, GREEN)]),
        Kind::Object(_) => (
            "Object",
            &[
                ("shape", SHAPE, GREY),
                ("color", PAINT, PURPLE),
                ("motion", MOTION, GREEN),
            ],
            &[("object", OBJECT, ORANGE)],
        ),
        Kind::Ring(_) => (
            "Ring",
            &[("object", OBJECT, ORANGE)],
            &[("objects", OBJECT, ORANGE)],
        ),
        Kind::Scene => ("Scene", &[("objects", OBJECT, ORANGE)], &[]),
    };
    let inputs = inputs.iter().map(|&(label, port_type, color)| {
        let port = match kind {
            Kind::Scene => Port::input(port_type).with_capacity(Capacity::Unlimited),
            _ => Port::input(port_type),
        };
        kit::row(label, port, color)
    });
    let outputs = outputs
        .iter()
        .map(|&(label, port_type, color)| kit::row(label, Port::output(port_type), color));
    let rows: Vec<_> = inputs.chain(outputs).collect();
    let controls = controls(kind);
    let scene = bsn! {
        @kit::node(at) kind ChildOf(canvas)
        Children [ @kit::title(title) -- {controls} -- {rows} ]
    };
    commands.spawn_scene(scene).id()
}

/// The controls editing a node's value, under its title.
fn controls(kind: Kind) -> Box<dyn SceneList> {
    let slider = |min: f32, max: f32, value: f32| {
        Box::new(bsn_list! {
            @FeathersSlider { @min: min, @max: max }
            SliderValue({value})
            SliderPrecision(2)
            on(slider_self_update)
        })
    };
    match kind {
        Kind::Shape(shape) => {
            let items = Shape::ALL.map(|choice| {
                let name = choice.name();
                bsn! {
                    @FeathersMenuItem { @caption: bsn! { Text(name) ThemedText } }
                    Choice(choice)
                }
            });
            let name = shape.name();
            let [cube, sphere, torus] = items;
            Box::new(bsn_list! {
                @FeathersMenu
                    Children [
                    @FeathersMenuButton { @caption: bsn! { Text(name) ThemedText Caption } }
                    --
                    @FeathersMenuPopup
                    Children [ {cube} -- {sphere} -- {torus} ]
                ]
            })
        }
        Kind::Paint(color) => {
            let lightness = color.lightness;
            // A hue and saturation plane over a lightness slider.
            Box::new(bsn_list! {
                @FeathersColorPlane::HueSaturation
                Node { min_height: px(70) }
                --
                @FeathersColorSlider { @channel: ColorChannel::HslLightness, @value: lightness }
                    on(slider_self_update)
            })
        }
        Kind::Spin(speed) => slider(-4.0, 4.0, speed),
        Kind::Object(size) => slider(0.25, 2.0, size),
        // Typed or dragged, the count stays within 0 to 64.
        Kind::Ring(count) => Box::new(bsn_list! {
            @FeathersNumberInput
            NumberInputValue::I32({count})
            HardLimit::i32(0..=64)
        }),
        Kind::Scene => Box::new(bsn_list! {}),
    }
}

/// Whether an edit changed connections or nodes (a run condition).
fn graph_changed(
    mut applied: MessageReader<EditApplied>,
    added: Query<(), Added<GraphNode>>,
    edited: Query<(), Changed<Kind>>,
) -> bool {
    let structural = |e: &&EditApplied| !matches!(e.change, GraphChange::Moved { .. });
    // Read every message (`any` would stop early and leave some for next frame).
    applied.read().filter(structural).count() > 0 || !added.is_empty() || !edited.is_empty()
}

/// Rebuild the 3D scene from the graph.
fn rebuild(
    mut commands: Commands,
    graph: GraphQuery,
    kinds: Query<(Entity, &Kind)>,
    built: Query<Entity, With<Built>>,
    meshes: Res<Meshes>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for entity in &built {
        commands.entity(entity).try_despawn();
    }
    let mut items = Vec::new();
    for (node, kind) in &kinds {
        if let Kind::Scene = kind {
            for input in graph.inputs_of(node) {
                for output in graph.peers_of(input) {
                    items.push(objects(&graph, &kinds, output, 0));
                }
            }
        }
    }
    let spacing = 3.2;
    let offset = (items.len() as f32 - 1.0) * spacing / 2.0;
    for (i, item) in items.into_iter().enumerate() {
        let place = Transform::from_xyz(i as f32 * spacing - offset, 0.0, 0.0);
        for instance in item {
            let mesh = meshes.of(instance.shape);
            let transform = place * instance.transform;
            let mut entity = commands.spawn((
                Built,
                Mesh3d(mesh),
                MeshMaterial3d(materials.add(instance.color)),
                transform,
            ));
            if let Some(speed) = instance.spin {
                entity.try_insert(Spin(speed, transform.rotation));
            }
        }
    }
}

/// The objects an output produces.
fn objects(
    graph: &GraphQuery,
    kinds: &Query<(Entity, &Kind)>,
    output: Entity,
    depth: u32,
) -> Vec<Instance> {
    let Some((node, kind)) = graph.node_of(output).and_then(|n| kinds.get(n).ok()) else {
        return Vec::new();
    };
    let input = |i| kinds.get(graph.node_of(graph.source_of(node, i)?)?).ok();
    match kind {
        Kind::Object(size) => {
            let Some((_, Kind::Shape(shape))) = input(0) else {
                return Vec::new();
            };
            let color = match input(1) {
                Some((_, Kind::Paint(color))) => (*color).into(),
                _ => Color::WHITE,
            };
            let spin = match input(2) {
                Some((_, Kind::Spin(speed))) => Some(*speed),
                _ => None,
            };
            vec![Instance {
                shape: *shape,
                color,
                spin,
                transform: Transform::from_scale(Vec3::splat(*size)),
            }]
        }
        Kind::Ring(count) if depth < 8 => {
            let Some(inner) = graph.source_of(node, 0) else {
                return Vec::new();
            };
            let inner = objects(graph, kinds, inner, depth + 1);
            let count = (*count).clamp(0, 64);
            (0..count)
                .flat_map(|k| {
                    let angle = k as f32 / count as f32 * TAU;
                    let around =
                        Transform::from_translation(Quat::from_rotation_y(angle) * Vec3::X * 1.3)
                            .with_scale(Vec3::splat(0.4));
                    inner.iter().map(move |i| Instance {
                        transform: around * i.transform,
                        ..i.clone()
                    })
                })
                .collect()
        }
        _ => Vec::new(),
    }
}

fn spin(time: Res<Time>, mut spinning: Query<(&mut Transform, &Spin)>) {
    for (mut transform, Spin(speed, start)) in &mut spinning {
        transform.rotation = *start * Quat::from_rotation_y(speed * time.elapsed_secs());
    }
}

/// A slider sets its node's lightness, speed or size.
fn edit_number(change: On<ValueChange<f32>>, graph: GraphQuery, mut kinds: Query<&mut Kind>) {
    let Some(mut kind) = graph
        .node_of(change.source)
        .and_then(|n| kinds.get_mut(n).ok())
    else {
        return;
    };
    match &mut *kind {
        Kind::Paint(color) => color.lightness = change.value,
        Kind::Spin(speed) => *speed = change.value,
        Kind::Object(size) => *size = change.value,
        _ => {}
    }
}

/// The color plane sets hue (across) and saturation (down).
fn edit_hue(change: On<ValueChange<Vec2>>, graph: GraphQuery, mut kinds: Query<&mut Kind>) {
    let node = graph.node_of(change.source);
    if let Some(Kind::Paint(color)) = node.and_then(|n| kinds.get_mut(n).ok()).as_deref_mut() {
        color.hue = change.value.x * 360.0;
        color.saturation = 1.0 - change.value.y;
    }
}

/// A ring's count field sets its count.
fn edit_count(change: On<ValueChange<i32>>, graph: GraphQuery, mut kinds: Query<&mut Kind>) {
    let node = graph.node_of(change.source);
    if let Some(Kind::Ring(count)) = node.and_then(|n| kinds.get_mut(n).ok()).as_deref_mut() {
        *count = change.value;
    }
}

/// A dropdown item sets its node's shape and the dropdown's caption.
fn choose_shape(
    activate: On<Activate>,
    choices: Query<&Choice>,
    graph: GraphQuery,
    mut kinds: Query<&mut Kind>,
    mut captions: Query<(Entity, &mut Text), With<Caption>>,
) {
    let (Ok(Choice(shape)), Some(node)) = (
        choices.get(activate.event_target()),
        graph.node_of(activate.event_target()),
    ) else {
        return;
    };
    if let Ok(mut kind) = kinds.get_mut(node) {
        *kind = Kind::Shape(*shape);
    }
    for (caption, mut text) in &mut captions {
        if graph.node_of(caption) == Some(node) {
            text.0 = shape.name().into();
        }
    }
}

/// Paint nodes show their color: the plane's thumb and gradient, the slider's
/// gradient, and the node's border.
fn show_paints(
    paints: Query<(Entity, &Kind, &mut SelectedBorderColor), Changed<Kind>>,
    mut planes: Query<(&ChildOf, &mut ColorPlaneValue)>,
    mut sliders: Query<(&ChildOf, &mut SliderBaseColor)>,
) {
    for (node, kind, mut border) in paints {
        let Kind::Paint(color) = *kind else {
            continue;
        };
        border.normal = color.with_lightness(color.lightness.max(0.25)).into();
        for (parent, mut plane) in &mut planes {
            if parent.parent() == node {
                plane.0 = Vec3::new(color.hue / 360.0, 1.0 - color.saturation, color.lightness);
            }
        }
        for (parent, mut base) in &mut sliders {
            if parent.parent() == node {
                base.0 = color.into();
            }
        }
    }
}

/// Right-click on empty canvas adds the next kind of node from a short list.
fn add_on_right_click(
    click: On<PointerClick>,
    graph: GraphQuery,
    views: Query<&CanvasView>,
    mut next: Local<usize>,
    mut commands: Commands,
) {
    let canvas = click.event_target();
    let Ok(view) = views.get(canvas) else {
        return;
    };
    if click.button == PointerButton::Secondary
        && graph.node_of(click.original_event_target()).is_none()
    {
        let kinds = [
            Kind::Shape(Shape::Sphere),
            Kind::Paint(Hsla::hsl(90.0, 0.7, 0.55)),
            Kind::Object(1.0),
            Kind::Ring(5),
            Kind::Spin(-2.5),
        ];
        let at = view.canvas_to_graph(click.pointer.position);
        if let Some(&kind) = kinds.get(*next % kinds.len()) {
            spawn(&mut commands, canvas, kind, at);
        }
        *next += 1;
    }
}
