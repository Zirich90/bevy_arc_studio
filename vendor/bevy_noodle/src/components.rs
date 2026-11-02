//! The components that make up a graph. You insert [`NodeCanvas`],
//! [`CanvasContent`], [`GraphNode`] and [`Port`]; the library manages edges.

use std::num::NonZeroU32;

use bevy::curve::cubic_splines::CubicSegment;
use bevy::ecs::lifecycle::HookContext;
use bevy::ecs::world::DeferredWorld;
use bevy::picking::Pickable;
use bevy::prelude::*;
use bevy::ui::Selectable;

/// A graph and its viewport. Adds no background; it spawns its
/// [`CanvasContent`] child, which holds the nodes. Spawn nodes as children of
/// the canvas (they move into the content) or of the content.
#[derive(Component, Reflect, Debug, Default, Clone, Copy)]
#[reflect(Component, Default)]
#[require(CanvasView)]
#[component(on_add = canvas_added)]
pub struct NodeCanvas;

fn canvas_added(mut world: DeferredWorld, context: HookContext) {
    let canvas = context.entity;
    world
        .commands()
        .queue(move |world: &mut World| _ = ensure_content(world, canvas));
}

/// The [`CanvasContent`] among a canvas's children.
pub(crate) fn content(world: &World, canvas: Entity) -> Option<Entity> {
    let children = world.get::<Children>(canvas)?;
    children
        .iter()
        .find(|c| world.get::<CanvasContent>(*c).is_some())
}

/// Gives `canvas` a content, unless it has one.
pub(crate) fn ensure_content(world: &mut World, canvas: Entity) -> Option<Entity> {
    world.get::<NodeCanvas>(canvas)?;
    let content = content(world, canvas);
    Some(content.unwrap_or_else(|| world.spawn((CanvasContent, ChildOf(canvas))).id()))
}

/// The canvas camera: `pan` is where the graph origin appears (canvas-local
/// logical pixels) and `zoom` scales graph space, within `min_zoom` and
/// `max_zoom` when zoomed with [`zoom_around`](Self::zoom_around) (as the
/// pointer and keyboard do).
#[derive(Component, Reflect, Debug, Clone, Copy, PartialEq)]
#[reflect(Component, Default)]
pub struct CanvasView {
    /// Where the graph origin appears, in canvas-local logical pixels.
    pub pan: Vec2,
    /// Scale of graph space: 1 is one logical pixel per graph unit.
    pub zoom: f32,
    /// Smallest zoom.
    pub min_zoom: f32,
    /// Largest zoom.
    pub max_zoom: f32,
}

impl Default for CanvasView {
    fn default() -> Self {
        Self {
            pan: Vec2::ZERO,
            zoom: 1.0,
            min_zoom: 0.1,
            max_zoom: 4.0,
        }
    }
}

impl CanvasView {
    /// Graph space → canvas-local logical pixels.
    pub fn graph_to_canvas(&self, point: Vec2) -> Vec2 {
        self.pan + point * self.zoom
    }

    /// Canvas-local logical pixels → graph space.
    pub fn canvas_to_graph(&self, point: Vec2) -> Vec2 {
        (point - self.pan) / self.zoom.max(f32::EPSILON)
    }

    /// Scales the zoom by `factor`, within the limits, keeping the graph
    /// point under the canvas-local `anchor` in place.
    pub fn zoom_around(&mut self, anchor: Vec2, factor: f32) {
        let fixed = self.canvas_to_graph(anchor);
        // Not `clamp`, which panics on inverted limits.
        self.zoom = (self.zoom * factor).max(self.min_zoom).min(self.max_zoom);
        self.pan = anchor - fixed * self.zoom;
    }
}

/// The child of a [`NodeCanvas`] holding its nodes, spawned by the canvas. Its
/// `UiTransform` follows [`CanvasView`]; it is otherwise an invisible,
/// zero-size container. One spawned as a canvas child by hand (or from a
/// snapshot) replaces the canvas's own while that one is empty.
#[derive(Component, Reflect, Debug, Default, Clone, Copy)]
#[reflect(Component, Default)]
#[require(Node = content_node(), UiTransform, Pickable = Pickable::IGNORE)]
#[component(on_add = content_added)]
pub struct CanvasContent;

fn content_added(mut world: DeferredWorld, context: HookContext) {
    let content = context.entity;
    world
        .commands()
        .queue(move |world: &mut World| link_content(world, content));
}

/// A content added beside its canvas's own replaces it while that one is
/// empty, so a canvas normally has one.
pub(crate) fn link_content(world: &mut World, content: Entity) {
    let Some(canvas) = world.get::<ChildOf>(content).map(ChildOf::parent) else {
        return;
    };
    if world.get::<NodeCanvas>(canvas).is_none() {
        return;
    }
    let siblings = world.get::<Children>(canvas).map(|c| c.to_vec());
    let empty = |e: Entity| world.get::<Children>(e).is_none_or(|c| c.is_empty());
    let replaced: Vec<_> = siblings
        .into_iter()
        .flatten()
        .filter(|c| *c != content && world.get::<CanvasContent>(*c).is_some() && empty(*c))
        .collect();
    for old in replaced {
        world.despawn(old);
    }
}

// Zero-size at the canvas origin, so zoom pivots on the graph origin even when
// nodes are laid out in flow.
fn content_node() -> Node {
    let zero = Val::Px(0.0);
    Node {
        position_type: PositionType::Absolute,
        left: zero,
        top: zero,
        width: zero,
        height: zero,
        ..default()
    }
}

/// Marks the root UI entity of a node. Style it however you like. A node
/// spawned as a child of a [`NodeCanvas`] moves into its [`CanvasContent`].
/// Nodes are Bevy `Selectable`s: with an `AccessibilityNode`, their
/// `Selected` state reaches screen readers.
#[derive(Component, Reflect, Debug, Default, Clone, Copy)]
#[reflect(Component, Default)]
#[require(Selectable)]
pub struct GraphNode;

/// A node put under a canvas moves into the canvas's content. An observer,
/// not a hook: it runs after `ChildOf`'s hooks, so their queued commands
/// (giving the canvas its `Children`) apply before the move.
pub(crate) fn adopt_nodes(
    insert: On<Insert<(GraphNode, ChildOf)>>,
    nodes: Query<&ChildOf, With<GraphNode>>,
    canvases: Query<(), With<NodeCanvas>>,
    mut commands: Commands,
) {
    let node = insert.entity;
    if nodes.get(node).is_ok_and(|p| canvases.contains(p.parent())) {
        commands.queue(move |world: &mut World| {
            let canvas = world.get::<ChildOf>(node).map(ChildOf::parent);
            if let Some(content) = canvas.and_then(|c| ensure_content(world, c))
                && let Ok(mut node) = world.get_entity_mut(node)
            {
                node.insert(ChildOf(content));
            }
        });
    }
}

/// Optional position of a [`GraphNode`]'s top-left corner in graph space. With
/// it, the library writes the node's `position_type`/`left`/`top`; without it,
/// your layout places the node.
#[derive(Component, Reflect, Debug, Default, Clone, Copy, PartialEq, Deref, DerefMut)]
#[reflect(Component, Default)]
pub struct NodePosition(pub Vec2);

/// If a node contains one, only the handle starts node drags.
#[derive(Component, Reflect, Debug, Default, Clone, Copy)]
#[reflect(Component, Default)]
pub struct NodeDragHandle;

/// Which way data flows through a [`Port`].
#[derive(Reflect, Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PortDirection {
    /// Edges arrive here.
    #[default]
    Input,
    /// Edges leave from here.
    Output,
}

/// What a port carries. Ports connect when types are equal or either is
/// [`PortType::ANY`]; add finer rules with a [`ConnectionCheck`](crate::ConnectionCheck) observer.
/// Types compare by a hash of their name; the name is kept for `Debug`
/// (except in types loaded from a snapshot, which show the hash).
#[derive(Reflect, Clone, Copy)]
#[reflect(Debug, PartialEq, Hash, Default)]
pub struct PortType {
    id: u64,
    #[reflect(ignore)]
    name: &'static str,
}

impl PortType {
    /// Connects to every type.
    pub const ANY: PortType = PortType { id: 0, name: "any" };

    /// A type identified by name (FNV-1a; never `ANY`).
    pub const fn named(name: &'static str) -> Self {
        let (mut bytes, mut hash) = (name.as_bytes(), 0xcbf2_9ce4_8422_2325_u64);
        while let [byte, rest @ ..] = bytes {
            hash = (hash ^ *byte as u64).wrapping_mul(0x0100_0000_01b3);
            bytes = rest;
        }
        let id = if hash == 0 { 1 } else { hash };
        PortType { id, name }
    }

    /// The hash types compare by.
    pub const fn id(self) -> u64 {
        self.id
    }

    /// Whether a port of this type may connect to one of `other`.
    pub fn accepts(self, other: PortType) -> bool {
        self == other || self == Self::ANY || other == Self::ANY
    }
}

impl Default for PortType {
    fn default() -> Self {
        Self::ANY
    }
}

impl PartialEq for PortType {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for PortType {}

impl std::hash::Hash for PortType {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl std::fmt::Debug for PortType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.name {
            "" => write!(f, "PortType(#{:016x})", self.id),
            name => write!(f, "PortType({name:?})"),
        }
    }
}

/// Marks a connection point: any UI entity inside a [`GraphNode`].
#[derive(Component, Reflect, Debug, Clone, Copy, PartialEq)]
#[reflect(Component)]
#[require(PortAnchor)]
pub struct Port {
    /// Input or output.
    pub direction: PortDirection,
    /// What the port carries.
    pub port_type: PortType,
    /// How many edges it holds, and what a new one does when it is full.
    pub capacity: Capacity,
}

/// How many edges a [`Port`] holds, and what connecting to it when full does.
#[derive(Reflect, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Capacity {
    /// Any number of edges.
    Unlimited,
    /// At most this many; more connections are refused
    /// ([`RejectReason::PortFull`](crate::RejectReason::PortFull)). If an
    /// observer allows one anyway, the port keeps all its edges.
    Refuse(NonZeroU32),
    /// At most this many; the oldest edges make room for a new one.
    Replace(NonZeroU32),
}

impl Port {
    /// An input holding one connection, which a new one replaces.
    pub const fn input(port_type: PortType) -> Self {
        Self {
            direction: PortDirection::Input,
            port_type,
            capacity: Capacity::Replace(NonZeroU32::MIN),
        }
    }

    /// An output holding any number of connections.
    pub const fn output(port_type: PortType) -> Self {
        Self {
            direction: PortDirection::Output,
            port_type,
            capacity: Capacity::Unlimited,
        }
    }

    /// The same port with another [`Capacity`].
    pub const fn with_capacity(mut self, capacity: Capacity) -> Self {
        self.capacity = capacity;
        self
    }

    pub(crate) fn tangent(&self, custom: Option<&PortTangent>) -> Vec2 {
        custom.map(|t| t.0).unwrap_or(match self.direction {
            PortDirection::Output => Vec2::X,
            PortDirection::Input => Vec2::NEG_X,
        })
    }
}

/// Overrides the direction wires leave or enter a port (graph space).
#[derive(Component, Reflect, Debug, Clone, Copy, PartialEq, Deref)]
#[reflect(Component)]
pub struct PortTangent(pub Vec2);

/// Where a port is, measured after layout (graph space). The library keeps
/// it up to date; read it here or with [`GraphQuery::port_position`](crate::GraphQuery::port_position).
#[derive(Component, Reflect, Debug, Default, Clone, Copy, PartialEq)]
#[reflect(Component, Default)]
pub struct PortAnchor {
    #[entities]
    pub(crate) node: Option<Entity>,
    pub(crate) offset: Vec2,
    pub(crate) position: Option<Vec2>,
}

impl PortAnchor {
    /// The [`GraphNode`] the port belongs to.
    pub fn node(&self) -> Option<Entity> {
        self.node
    }

    /// Center relative to the node's [`NodePosition`].
    pub fn offset(&self) -> Vec2 {
        self.offset
    }

    /// Center in graph space at the last layout; `None` until laid out.
    pub fn position(&self) -> Option<Vec2> {
        self.position
    }
}

/// A connection: an entity without a parent, in the graph of its ports and
/// despawned with either port. Pointer events on it bubble to the window,
/// not to the canvas; observe them on the edge or app-wide.
#[derive(Component, Reflect, Debug, Default, Clone, Copy)]
#[reflect(Component, Default)]
#[require(Selectable)]
pub struct Edge;

/// The output port an [`Edge`] starts at.
#[derive(Component, Reflect, Debug, Clone, Copy, PartialEq, Eq)]
#[reflect(Component)]
#[relationship(relationship_target = OutgoingEdges)]
pub struct EdgeSource(pub Entity);

/// The input port an [`Edge`] ends at.
#[derive(Component, Reflect, Debug, Clone, Copy, PartialEq, Eq)]
#[reflect(Component)]
#[relationship(relationship_target = IncomingEdges)]
pub struct EdgeTarget(pub Entity);

/// The two ports of a connection, output → input.
#[derive(Reflect, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PortPair {
    /// Where the edge starts.
    pub output: Entity,
    /// Where the edge ends.
    pub input: Entity,
}

impl PortPair {
    /// The pair `output` → `input`.
    pub const fn new(output: Entity, input: Entity) -> Self {
        Self { output, input }
    }

    /// The port across from `port` (the output if `port` is not the output).
    pub fn other(&self, port: Entity) -> Entity {
        if port == self.output {
            self.input
        } else {
            self.output
        }
    }
}

/// Edges leaving a port, maintained by Bevy.
#[derive(Component, Reflect, Debug, Default, Clone, PartialEq, Deref)]
#[reflect(Component, Default)]
#[relationship_target(relationship = EdgeSource, linked_spawn)]
pub struct OutgoingEdges(Vec<Entity>);

/// Edges arriving at a port, maintained by Bevy.
#[derive(Component, Reflect, Debug, Default, Clone, PartialEq, Deref)]
#[reflect(Component, Default)]
#[relationship_target(relationship = EdgeTarget, linked_spawn)]
pub struct IncomingEdges(Vec<Entity>);

/// Makes an [`Edge`] pickable: it gets `Pointer` events (and can be selected)
/// when the pointer is within `radius` of the cubic Bézier `points` (graph
/// space). The default style keeps this in sync with what it draws; set it
/// yourself for edges you draw.
#[derive(Component, Reflect, Debug, Clone, Copy, PartialEq)]
#[reflect(Component)]
pub struct EdgeHitbox {
    /// The curve's control points.
    pub points: [Vec2; 4],
    /// How far from the curve the pointer still hits it.
    pub radius: f32,
    /// Drawn under nodes, so only pickable over empty canvas.
    pub below_nodes: bool,
}

impl EdgeHitbox {
    /// The curve, as Bevy's [`CubicSegment`].
    pub fn curve(&self) -> CubicSegment<Vec2> {
        CubicSegment::new_bezier(self.points)
    }

    /// Distance from `point` to the curve, sampled like the default wire shader.
    pub fn distance(&self, point: Vec2) -> f32 {
        let samples: Vec<Vec2> = self.curve().iter_positions(32).collect();
        let ends = samples.iter().zip(samples.iter().skip(1));
        let segments = ends.map(|(&a, &b)| Segment2d::new(a, b));
        segments
            .map(|s| s.closest_point(point).distance(point))
            .fold(f32::MAX, f32::min)
    }
}

/// Where an [`Edge`] or [`PendingWire`] runs, output → input, in graph space.
/// Draw edges from it however you like. It is there only while both ends
/// are laid out.
#[derive(Component, Reflect, Debug, Clone, Copy, PartialEq)]
#[reflect(Component)]
pub struct EdgeGeometry {
    /// Output end.
    pub start: Vec2,
    /// Input end.
    pub end: Vec2,
    /// Points away from the output's node.
    pub start_tangent: Vec2,
    /// Points away from the input's node.
    pub end_tangent: Vec2,
    /// The port at the output end (none at a dragged wire's free end).
    pub output: Option<Entity>,
    /// The port at the input end (none at a dragged wire's free end).
    pub input: Option<Entity>,
}

impl EdgeGeometry {
    /// A geometry between two `(position, tangent)` ends, without ports.
    pub fn between((start, start_tangent): (Vec2, Vec2), (end, end_tangent): (Vec2, Vec2)) -> Self {
        Self {
            start,
            end,
            start_tangent,
            end_tangent,
            output: None,
            input: None,
        }
    }

    /// The same geometry, at these ports.
    pub fn with_ports(mut self, output: Option<Entity>, input: Option<Entity>) -> Self {
        (self.output, self.input) = (output, input);
        self
    }

    /// Cubic Bézier control points; `curvature` 0.5 is a good default. The
    /// handles reach `curvature` times the distance between the ends,
    /// clamped to 30–240 graph units, so short wires still curve and long
    /// ones do not balloon.
    pub fn bezier(&self, curvature: f32) -> [Vec2; 4] {
        let handle = (self.end.distance(self.start) * curvature).clamp(30.0, 240.0);
        [
            self.start,
            self.start + self.start_tangent * handle,
            self.end + self.end_tangent * handle,
            self.end,
        ]
    }
}

/// The wire being dragged, spawned with its canvas as `(PendingWire { from,
/// pointer }, WireOf(canvas))`: its own entity with an [`EdgeGeometry`], so
/// edge renderers draw it like any edge. Spawning one gives it the ports it
/// may connect to ([`WireCandidates`](crate::WireCandidates), asking
/// [`ConnectionCheck`](crate::ConnectionCheck) observers) and the one it
/// snaps to ([`WireTarget`](crate::WireTarget)).
#[derive(Component, Reflect, Debug, Clone, Copy, PartialEq)]
#[reflect(Component)]
#[require(crate::WireTarget)]
#[component(on_add = wire_added)]
pub struct PendingWire {
    /// The port the drag started at.
    #[entities]
    pub from: Entity,
    /// Pointer position in graph space.
    pub pointer: Vec2,
}

fn wire_added(mut world: DeferredWorld, context: HookContext) {
    let wire = context.entity;
    world
        .commands()
        .queue(move |world: &mut World| crate::interaction::mark_candidates(world, wire));
}

/// On a [`PendingWire`]: the canvas it is dragged in. A wire taken out of its
/// canvas, or replaced by another one there, is despawned.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Deref)]
#[relationship(relationship_target = DraggedWire)]
#[component(on_remove = wire_removed)]
pub struct WireOf(pub Entity);

fn wire_removed(mut world: DeferredWorld, context: HookContext) {
    world.commands().entity(context.entity).try_despawn();
}

/// On a [`NodeCanvas`]: its [`PendingWire`], maintained by Bevy.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Deref)]
#[relationship_target(relationship = WireOf, linked_spawn)]
pub struct DraggedWire(Entity);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_types() {
        const NUMBER: PortType = PortType::named("number");
        assert_eq!(NUMBER, PortType::named("number"));
        assert_ne!(NUMBER, PortType::named("text"));
        assert!(NUMBER.accepts(PortType::ANY) && !NUMBER.accepts(PortType::named("text")));
        assert_eq!(format!("{NUMBER:?}"), r#"PortType("number")"#);
    }

    #[test]
    fn zoom_keeps_anchor_fixed() {
        let mut view = CanvasView::default();
        let anchor = Vec2::new(300.0, 200.0);
        let before = view.canvas_to_graph(anchor);
        view.zoom_around(anchor, 1.7);
        assert!((view.canvas_to_graph(anchor) - before).length() < 1e-3);
        view.zoom_around(anchor, 100.0);
        assert_eq!(view.zoom, view.max_zoom);
    }

    #[test]
    fn inverted_zoom_limits_do_not_panic() {
        let mut view = CanvasView {
            min_zoom: 2.0,
            max_zoom: 1.0,
            ..default()
        };
        view.zoom_around(Vec2::ZERO, 1.5);
        assert!(view.zoom.is_finite());
    }
}
