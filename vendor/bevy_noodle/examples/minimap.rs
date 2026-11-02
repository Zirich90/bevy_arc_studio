//! A minimap: a small second view of the same graph, kept in sync from the
//! nodes' `NodePosition`s and the canvas's `CanvasView`. Click or drag on it
//! to move the main view there.
//!
//! ```sh
//! cargo run --example minimap --features default_style
//! ```

use bevy::picking::Pickable;
use bevy::picking::cursor::CursorIconPlugin;
use bevy::prelude::*;
use bevy::ui::ui_transform::UiGlobalTransform;
use bevy_noodle::prelude::*;
use bevy_noodle::style::kit;

const NUMBER: PortType = PortType::named("number");
const BLUE: Color = Color::srgb(0.25, 0.52, 0.9);
const MAP: Vec2 = Vec2::new(280.0, 170.0);

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, CursorIconPlugin))
        .add_plugins((NoodlePlugins, NoodleDefaultStylePlugin))
        .insert_resource(ClearColor(Color::srgb_u8(24, 25, 29)))
        .add_systems(Startup, setup)
        .add_systems(Update, draw_minimap)
        .run();
}

#[derive(Resource)]
struct Graph(Entity);

#[derive(Component)]
struct Minimap;

/// A minimap rectangle standing for a node.
#[derive(Component)]
struct MiniNode(Entity);

/// The minimap rectangle showing the visible area.
#[derive(Component)]
struct MiniView;

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    let canvas = commands.spawn_scene(kit::canvas()).id();
    commands.insert_resource(Graph(canvas));
    // A long chain, wider than the window.
    let nodes: Vec<Entity> = (0..12)
        .map(|i| {
            let at = Vec2::new(60.0 + 260.0 * i as f32, 120.0 + 180.0 * ((i % 3) as f32));
            let title = format!("Step {}", i + 1);
            commands
                .spawn_scene(bsn! {
                    @kit::node(at) ChildOf(canvas)
                    Children [
                        @kit::title(title) -- @kit::row("in", Port::input(NUMBER), BLUE)
                        -- @kit::row("out", Port::output(NUMBER), BLUE)
                    ]
                })
                .id()
        })
        .collect();
    commands.queue(move |world: &mut World| {
        let ports = |In(nodes): In<Vec<Entity>>, graph: GraphQuery| {
            let pairs = nodes.iter().zip(nodes.iter().skip(1));
            pairs
                .filter_map(|(&a, &b)| {
                    Some((graph.outputs_of(a).next()?, graph.inputs_of(b).next()?))
                })
                .collect::<Vec<_>>()
        };
        let pairs = world
            .run_system_cached_with(ports, nodes)
            .unwrap_or_default();
        for (from, to) in pairs {
            world
                .graph_edit(canvas, GraphEdit::Connect { from, to })
                .ok();
        }
    });
    commands
        .spawn((
            Minimap,
            Node {
                position_type: PositionType::Absolute,
                right: px(12),
                bottom: px(12),
                width: px(MAP.x),
                height: px(MAP.y),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(6)),
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(Color::srgba(0.05, 0.06, 0.08, 0.9)),
            BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.15)),
            GlobalZIndex(1),
            children![(
                MiniView,
                Node {
                    position_type: PositionType::Absolute,
                    border: UiRect::all(px(1.5)),
                    ..default()
                },
                BorderColor::all(Color::srgb_u8(250, 204, 92)),
                Pickable::IGNORE,
            )],
        ))
        .observe(look_here::<PointerPress>)
        .observe(look_here::<PointerDrag>);
}

/// How graph space maps onto the minimap: graph point → minimap pixels.
#[derive(Clone, Copy)]
struct Mapping {
    origin: Vec2,
    scale: f32,
    offset: Vec2,
}

impl Mapping {
    /// Fits every node (with a margin), centered.
    fn new(nodes: &Query<(Entity, &NodePosition, &ComputedNode)>) -> Option<Self> {
        let rects = nodes.iter().map(|(_, p, c)| node_rect(p, c));
        let bounds = rects.reduce(|a, b| a.union(b))?.inflate(80.0);
        let scale = (MAP / bounds.size()).min_element();
        let offset = (MAP - bounds.size() * scale) / 2.0;
        Some(Self {
            origin: bounds.min,
            scale,
            offset,
        })
    }

    fn to_map(self, point: Vec2) -> Vec2 {
        (point - self.origin) * self.scale + self.offset
    }

    fn to_graph(self, point: Vec2) -> Vec2 {
        (point - self.offset) / self.scale + self.origin
    }
}

fn node_rect(position: &NodePosition, computed: &ComputedNode) -> Rect {
    let size = computed.size() * computed.inverse_scale_factor();
    Rect::from_corners(position.0, position.0 + size)
}

fn draw_minimap(
    mut commands: Commands,
    graph: Res<Graph>,
    minimap: Single<Entity, With<Minimap>>,
    views: Query<(&CanvasView, &ComputedNode), Without<GraphNode>>,
    nodes: Query<(Entity, &NodePosition, &ComputedNode)>,
    mut minis: Query<(Entity, &MiniNode, &mut Node), Without<MiniView>>,
    mut view_rect: Single<&mut Node, With<MiniView>>,
) {
    let (Some(map), Ok((view, canvas))) = (Mapping::new(&nodes), views.get(graph.0)) else {
        return;
    };
    let place = |node: &mut Node, rect: Rect| {
        let rect = Rect::from_corners(map.to_map(rect.min), map.to_map(rect.max));
        (node.left, node.top) = (px(rect.min.x), px(rect.min.y));
        (node.width, node.height) = (px(rect.width()), px(rect.height()));
    };
    for (entity, mini, mut node) in &mut minis {
        match nodes.get(mini.0) {
            Ok((_, p, c)) => place(&mut node, node_rect(p, c)),
            Err(_) => commands.entity(entity).try_despawn(),
        }
    }
    for (node, ..) in nodes
        .iter()
        .filter(|(n, ..)| !minis.iter().any(|m| m.1.0 == *n))
    {
        let rect = (
            Node {
                position_type: PositionType::Absolute,
                ..default()
            },
            BackgroundColor(BLUE.with_alpha(0.6)),
        );
        commands.spawn((MiniNode(node), rect, Pickable::IGNORE, ChildOf(*minimap)));
    }
    // The visible area: the canvas's corners in graph space.
    let size = canvas.size() * canvas.inverse_scale_factor();
    place(
        &mut view_rect,
        Rect::from_corners(view.canvas_to_graph(Vec2::ZERO), view.canvas_to_graph(size)),
    );
}

/// Pressing or dragging on the minimap centers the main view there.
fn look_here<E: PointerEvent>(
    event: On<E>,
    graph: Res<Graph>,
    maps: Query<(&ComputedNode, &UiGlobalTransform), With<Minimap>>,
    mut views: Query<(&mut CanvasView, &ComputedNode), Without<Minimap>>,
    nodes: Query<(Entity, &NodePosition, &ComputedNode)>,
) {
    let (Ok((map, transform)), Some(mapping)) =
        (maps.get(event.event_target()), Mapping::new(&nodes))
    else {
        return;
    };
    let scale_factor = map.inverse_scale_factor();
    let Some(normalized) = map.normalize_point(*transform, event.pointer().position / scale_factor)
    else {
        return;
    };
    let target = mapping.to_graph((normalized + 0.5) * MAP);
    if let Ok((mut view, canvas)) = views.get_mut(graph.0) {
        let size = canvas.size() * canvas.inverse_scale_factor();
        view.pan = size / 2.0 - target * view.zoom;
    }
}
