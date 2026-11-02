//! A stress test that drives itself: it keeps a graph of a few hundred nodes
//! alive by spawning, wiring, moving, rewiring, selecting and deleting nodes
//! every frame through the normal edit pipeline, while the camera drifts and
//! zooms. An overlay shows the frame rate, graph size and edits per second.
//!
//! Space pauses, Up/Down double or halve the target node count, C hands the
//! camera back to you (then pan and zoom as usual), and T switches new nodes
//! to unlabelled ports (`kit::dot`): less text to lay out. Build with `--release`
//! for meaningful numbers. For per-system timings, run it with Bevy's
//! `trace_tracy` feature and connect the Tracy profiler.
//!
//! ```sh
//! cargo run --release --example stress --features default_style
//! ```

use bevy::diagnostic::{
    DiagnosticPath, DiagnosticsStore, EntityCountDiagnosticsPlugin, FrameTimeDiagnosticsPlugin,
};
use bevy::picking::cursor::CursorIconPlugin;
use bevy::prelude::*;
use bevy::ui::Selected;
use bevy_noodle::prelude::*;
use bevy_noodle::style::kit;

const NUMBER: PortType = PortType::named("number");
const TEXT: PortType = PortType::named("text");
const BLUE: Color = Color::srgb(0.25, 0.52, 0.9);
const GREEN: Color = Color::srgb(0.45, 0.8, 0.5);
/// The graph-space area nodes are spawned in.
const WORLD: Vec2 = Vec2::new(5000.0, 3200.0);

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            CursorIconPlugin,
            FrameTimeDiagnosticsPlugin::default(),
            EntityCountDiagnosticsPlugin::default(),
            NoodlePlugins,
            NoodleDefaultStylePlugin,
        ))
        .insert_resource(ClearColor(Color::srgb_u8(24, 25, 29)))
        .add_systems(Startup, setup)
        .add_systems(Update, (keys, churn, drive_camera, overlay).chain())
        .add_observer(|_: On<EditApplied>, mut stress: ResMut<Stress>| stress.edits += 1)
        .run();
}

#[derive(Resource)]
struct Stress {
    canvas: Entity,
    /// Nodes to keep alive.
    target: usize,
    paused: bool,
    auto_camera: bool,
    /// Whether new nodes get port labels.
    labels: bool,
    /// Edits applied since the overlay last updated.
    edits: u32,
    rng: u64,
}

impl Stress {
    /// xorshift64: deterministic, so every run does the same things.
    fn next(&mut self) -> u64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        self.rng
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }

    fn unit(&mut self) -> f32 {
        (self.next() % 10_000) as f32 / 10_000.0
    }

    fn pick<T: Copy>(&mut self, items: &[T]) -> Option<T> {
        items.get(self.below(items.len())).copied()
    }
}

#[derive(Component)]
struct Overlay;

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    // Solid wires carry pulses, so the wire shader works every frame too.
    let style = EdgeStyle {
        flow_speed: 120.0,
        ..default()
    };
    let canvas = commands.spawn_scene(kit::canvas()).try_insert(style).id();
    commands.insert_resource(Stress {
        canvas,
        target: 400,
        paused: false,
        auto_camera: true,
        labels: true,
        edits: 0,
        rng: 0x9E37_79B9_7F4A_7C15,
    });
    commands.spawn((
        Text::default(),
        TextFont::from_font_size(15.0),
        TextColor(Color::WHITE),
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.85)),
        // Above the canvas, whatever order the roots end up in.
        GlobalZIndex(1),
        Overlay,
        Node {
            position_type: PositionType::Absolute,
            left: px(10),
            top: px(10),
            padding: UiRect::all(px(8)),
            ..default()
        },
    ));
}

fn keys(keys: Res<ButtonInput<KeyCode>>, mut stress: ResMut<Stress>) {
    if keys.just_pressed(KeyCode::Space) {
        stress.paused = !stress.paused;
    }
    if keys.just_pressed(KeyCode::ArrowUp) {
        stress.target = (stress.target * 2).min(10_000);
    }
    if keys.just_pressed(KeyCode::ArrowDown) {
        stress.target = (stress.target / 2).max(10);
    }
    if keys.just_pressed(KeyCode::KeyC) {
        stress.auto_camera = !stress.auto_camera;
    }
    if keys.just_pressed(KeyCode::KeyT) {
        stress.labels = !stress.labels;
    }
}

/// One frame of scripted editing.
fn churn(
    mut commands: Commands,
    mut stress: ResMut<Stress>,
    nodes: Query<Entity, With<GraphNode>>,
    ports: Query<(Entity, &Port)>,
    edges: Query<Entity, With<Edge>>,
) {
    if stress.paused {
        return;
    }
    let nodes: Vec<Entity> = nodes.iter().collect();
    let outputs: Vec<Entity> = ports
        .iter()
        .filter(|(_, p)| p.direction == PortDirection::Output)
        .map(|(e, _)| e)
        .collect();
    let inputs: Vec<Entity> = ports
        .iter()
        .filter(|(_, p)| p.direction == PortDirection::Input)
        .map(|(e, _)| e)
        .collect();
    let edges: Vec<Entity> = edges.iter().collect();
    let canvas = stress.canvas;

    // Grow toward the target a few dozen nodes per frame, or shrink past it.
    let missing = stress.target.saturating_sub(nodes.len()).min(30);
    for _ in 0..missing {
        let at = Vec2::new(stress.unit(), stress.unit()) * WORLD;
        let kind = stress.below(3);
        let (canvas, labels) = (stress.canvas, stress.labels);
        spawn_node(&mut commands, canvas, kind, at, labels);
    }
    let excess = nodes.len().saturating_sub(stress.target).min(30);
    // Once grown, keep deleting a few nodes so they get replaced.
    let deletions = if missing == 0 { excess + 2 } else { 0 };
    let doomed: Vec<Entity> = (0..deletions).filter_map(|_| stress.pick(&nodes)).collect();
    if !doomed.is_empty() {
        commands.graph_edit(canvas, GraphEdit::Delete { items: doomed });
    }

    // Wiring: random pairs, many of them rejected (types, same node, full).
    for _ in 0..12 {
        if let (Some(from), Some(to)) = (stress.pick(&outputs), stress.pick(&inputs)) {
            commands.graph_edit(canvas, GraphEdit::Connect { from, to });
        }
    }
    for _ in 0..3 {
        if let Some(edge) = stress.pick(&edges) {
            commands.graph_edit(canvas, GraphEdit::Disconnect { edge });
        }
    }

    // Moving: a few nodes nudged as if dragged.
    for _ in 0..8 {
        if let Some(node) = stress.pick(&nodes) {
            let delta = (Vec2::new(stress.unit(), stress.unit()) - 0.5) * 60.0;
            commands.graph_edit(canvas, GraphEdit::move_nodes(vec![node], delta));
        }
    }

    // Selecting: now and then a random handful.
    if stress.below(20) == 0 {
        let picked = (0..10).filter_map(|_| stress.pick(&nodes)).collect();
        commands.select(canvas, picked, SelectMode::Replace);
    }
}

/// Sources, operations and sinks over two port types, so some wiring fails.
/// Without labels, ports are bare dots: less text for Bevy to lay out.
fn spawn_node(commands: &mut Commands, canvas: Entity, kind: usize, at: Vec2, labels: bool) {
    let (i, o) = (Port::input, Port::output);
    let (title, ports): (&str, &[_]) = match kind {
        0 => (
            "Source",
            &[("number", o(NUMBER), BLUE), ("text", o(TEXT), GREEN)],
        ),
        1 => (
            "Operation",
            &[
                ("a", i(NUMBER), BLUE),
                ("b", i(NUMBER), BLUE),
                ("out", o(NUMBER), BLUE),
            ],
        ),
        _ => (
            "Sink",
            &[("number", i(NUMBER), BLUE), ("text", i(TEXT), GREEN)],
        ),
    };
    let rows: Vec<Box<dyn Scene>> = ports
        .iter()
        .map(|&(label, port, color)| -> Box<dyn Scene> {
            match labels {
                true => Box::new(kit::row(label, port, color)),
                false => Box::new(kit::dot(port, color)),
            }
        })
        .collect();
    commands.spawn_scene(
        bsn! { @kit::node(at) ChildOf(canvas) Children [ @kit::title(title) -- {rows} ] },
    );
}

/// Slowly drifts and breathes over the whole area.
fn drive_camera(
    time: Res<Time>,
    stress: Res<Stress>,
    mut canvases: Query<(&mut CanvasView, &ComputedNode)>,
) {
    let Ok((mut view, computed)) = canvases.get_mut(stress.canvas) else {
        return;
    };
    if !stress.auto_camera {
        return;
    }
    let t = time.elapsed_secs();
    let size = computed.size() * computed.inverse_scale_factor();
    let fit = (size / WORLD).min_element().max(0.05);
    view.zoom = fit * (1.6 + 0.6 * (t * 0.23).sin());
    let focus = WORLD * (0.5 + 0.3 * Vec2::new((t * 0.11).sin(), (t * 0.17).cos()));
    view.pan = size / 2.0 - focus * view.zoom;
}

fn overlay(
    time: Res<Time>,
    mut since: Local<f32>,
    mut stress: ResMut<Stress>,
    diagnostics: Res<DiagnosticsStore>,
    nodes: Query<Has<Selected>, With<GraphNode>>,
    edges: Query<(), With<Edge>>,
    mut text: Single<&mut Text, With<Overlay>>,
) {
    *since += time.delta_secs();
    if *since < 0.5 {
        return;
    }
    // Bevy's own diagnostics: frame rate and the world's entity count.
    let value = |path: DiagnosticPath| {
        let diagnostic = diagnostics.get(&path);
        diagnostic.and_then(|d| d.smoothed()).unwrap_or_default()
    };
    let fps = value(FrameTimeDiagnosticsPlugin::FPS);
    let entities = value(EntityCountDiagnosticsPlugin::ENTITY_COUNT);
    let edits = stress.edits as f32 / *since;
    (stress.edits, *since) = (0, 0.0);
    let state = if stress.paused { "paused" } else { "running" };
    text.0 = format!(
        "{fps:.0} fps | {entities:.0} entities | {} nodes (target {}) | {} edges | {} selected | {edits:.0} edits/s | {state}\n\
         Space: pause | Up/Down: target x2 / /2 | C: camera {} | T: port labels {}",
        nodes.iter().count(),
        stress.target,
        edges.iter().count(),
        nodes.iter().filter(|selected| *selected).count(),
        if stress.auto_camera { "auto" } else { "yours" },
        if stress.labels { "on" } else { "off" },
    );
}
