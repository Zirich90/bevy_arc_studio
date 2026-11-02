//! Pointer interaction, opt-in per canvas with [`CanvasInteraction`]. Built on
//! `bevy_picking` events (any pointer, any render target) and stateless: every
//! drag is computed from the event's `delta`/`distance`. No keyboard bindings;
//! modifier keys for additive selection and zoom are configurable.
//!
//! Presses and drags that start in a focusable control inside a node (anything
//! with a [`TabIndex`], such as a slider or text field) belong to the control:
//! they don't select, raise or move the node.
//!
//! Edges with an [`EdgeHitbox`] are picked by a small backend running after
//! Bevy's UI backend, so they get `Pointer` events like any UI entity.

use bevy::ecs::entity::EntityHashSet;
use bevy::input::gestures::PinchGesture;
use bevy::input::mouse::MouseScrollUnit;
use bevy::input_focus::tab_navigation::TabIndex;
use bevy::picking::backend::{HitData, PointerHits};
use bevy::picking::hover::{HoverMap, Hovered};
use bevy::picking::pointer::{PointerButton, PointerId, PointerLocation, PointerMap};
use bevy::picking::{Pickable, PickingSystems};
use bevy::prelude::*;
use bevy::ui::picking_backend::ui_picking;
use bevy::ui::{ComputedNode, InteractionDisabled, UiScale, ui_transform::UiGlobalTransform};

use crate::components::*;
use crate::edit::{DragProgress, EditOrigin, GraphCommandsExt, GraphEdit, SelectMode, ask};
use crate::query::GraphQuery;

/// Pointer interaction for canvases with [`CanvasInteraction`], and picking for edges with an [`EdgeHitbox`].
pub struct NoodleInteractionPlugin;

impl Plugin for NoodleInteractionPlugin {
    fn build(&self, app: &mut App) {
        // Bevy's UI and picking plugins add these; without them, nothing happens.
        app.init_resource::<UiScale>()
            .init_resource::<HoverMap>()
            .init_resource::<PointerMap>()
            .init_resource::<ButtonInput<KeyCode>>()
            .add_observer(on_press)
            .add_observer(on_drag_start)
            .add_observer(on_drag)
            .add_observer(on_drag_end)
            .add_observer(on_cancel)
            .add_observer(on_scroll)
            .add_message::<WireDropped>()
            // Registered here too, so apps without a picking backend still run.
            .add_message::<PointerHits>()
            .add_systems(Update, pinch_zoom)
            .add_systems(
                PreUpdate,
                pick_edges.in_set(PickingSystems::Backend).after(ui_picking),
            );
    }
}

/// Turns on pointer interaction for a canvas. Set any button to `None` to
/// disable that interaction. Key settings are lists: any of their keys
/// works, and an empty list unbinds them.
#[derive(Component, Reflect, Clone, Debug)]
#[reflect(Component, Default)]
#[require(Hovered)]
pub struct CanvasInteraction {
    /// Press a node (or a pickable edge) to select it; press empty canvas to
    /// clear; drag on empty canvas to box-select.
    pub select_button: Option<PointerButton>,
    /// Drag nodes (with the selection).
    pub drag_button: Option<PointerButton>,
    /// Drag from a port to connect.
    pub connect_button: Option<PointerButton>,
    /// Drag anywhere to pan.
    pub pan_button: Option<PointerButton>,
    /// Dragging from a connected input picks up its wire.
    pub detach_wires: bool,
    /// Pressing a node raises it above its sibling nodes, with a `ZIndex`.
    /// Nodes with a negative `ZIndex` stay where they are (e.g. frames meant
    /// to stay under the others).
    pub raise_on_press: bool,
    /// Held keys making selection additive.
    pub additive_keys: Vec<KeyCode>,
    /// Held keys making scrolling zoom.
    pub zoom_modifiers: Vec<KeyCode>,
    /// What scrolling does.
    pub scroll: ScrollMode,
    /// Trackpad pinch zooms.
    pub pinch_zoom: bool,
}

impl Default for CanvasInteraction {
    fn default() -> Self {
        use KeyCode::*;
        Self {
            select_button: Some(PointerButton::Primary),
            drag_button: Some(PointerButton::Primary),
            connect_button: Some(PointerButton::Primary),
            pan_button: Some(PointerButton::Middle),
            detach_wires: true,
            raise_on_press: true,
            additive_keys: additive_keys(),
            zoom_modifiers: vec![ControlLeft, ControlRight, SuperLeft, SuperRight],
            scroll: ScrollMode::Auto,
            pinch_zoom: true,
        }
    }
}

/// The default keys making selection additive, for pointer and keyboard.
pub(crate) fn additive_keys() -> Vec<KeyCode> {
    use KeyCode::*;
    vec![
        ShiftLeft,
        ShiftRight,
        ControlLeft,
        ControlRight,
        SuperLeft,
        SuperRight,
    ]
}

/// What scrolling over a canvas does.
#[derive(Reflect, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScrollMode {
    /// Mouse wheels (lines) zoom, trackpads (pixels) pan.
    #[default]
    Auto,
    /// Scrolling zooms.
    Zoom,
    /// Scrolling pans.
    Pan,
    /// Left to the rest of the app.
    None,
}

/// On a canvas during box selection: the box in canvas-local pixels.
#[derive(Component, Reflect, Clone, Copy, Debug, PartialEq, Deref)]
#[reflect(Component)]
pub struct SelectionBox(pub Rect);

/// On a canvas during an additive box selection: what was selected before.
#[derive(Component)]
struct KeptSelection(Vec<Entity>);

/// On a [`PendingWire`]: the ports it may connect to, by the built-in rules
/// and [`ConnectionCheck`](crate::ConnectionCheck) observers. Set by the library.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq, Deref)]
pub struct WireCandidates(EntityHashSet);

/// On a [`PendingWire`]: the candidate port it would connect to if dropped now.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Deref)]
pub struct WireTarget(pub Option<Entity>);

/// Triggered on a canvas, and written as a message, when a wire is dropped
/// away from any port; `position` is in graph space.
#[derive(EntityEvent, Message, Clone, Copy, Debug)]
pub struct WireDropped {
    /// The canvas.
    #[event_target]
    pub canvas: Entity,
    /// The port the wire came from.
    pub from: Entity,
    /// Where it was dropped, in graph space.
    pub position: Vec2,
}

/// What an observer needs about the entity an event reached.
#[derive(bevy::ecs::system::SystemParam)]
struct Ctx<'w, 's> {
    graph: GraphQuery<'w, 's>,
    canvases: Query<
        'w,
        's,
        (
            &'static CanvasInteraction,
            &'static mut CanvasView,
            &'static ComputedNode,
            &'static UiGlobalTransform,
        ),
    >,
    disabled: Query<'w, 's, (), With<InteractionDisabled>>,
    handles: Query<'w, 's, (), With<NodeDragHandle>>,
    controls: Query<'w, 's, (), With<TabIndex>>,
    parents: Query<'w, 's, &'static ChildOf>,
    children: Query<'w, 's, &'static Children>,
    z_indices: Query<'w, 's, &'static ZIndex>,
    keys: Res<'w, ButtonInput<KeyCode>>,
    ui_scale: Res<'w, UiScale>,
    commands: Commands<'w, 's>,
}

#[derive(Clone, Copy, PartialEq)]
enum Hop {
    Port(Entity),
    Node(Entity),
    Edge(Entity),
    Canvas,
}

/// What a drag does.
#[derive(Clone, Copy)]
enum Gesture {
    /// Drags a wire from a port.
    Wire(Entity),
    /// Moves a node (with the selection).
    Move(Entity),
    Pan,
    /// Box-selects on empty canvas.
    Box,
}

impl Ctx<'_, '_> {
    /// What `target` is, and the interactive, enabled canvas it belongs to
    /// with its settings.
    fn hop(&self, target: Entity) -> Option<(Hop, Entity, CanvasInteraction)> {
        let g = &self.graph;
        let hop = match () {
            _ if g.port(target).is_some() => Hop::Port(target),
            _ if g.node_of(target) == Some(target) => Hop::Node(target),
            _ if g.edge_ports(target).is_some() => Hop::Edge(target),
            _ if self.canvases.contains(target) => Hop::Canvas,
            _ => return None,
        };
        let canvas = g.canvas_of(target)?;
        let blocked = [Some(canvas), g.node_of(target), Some(target)]
            .into_iter()
            .flatten()
            .any(|e| self.disabled.contains(e));
        let settings = self.canvases.get(canvas).ok().filter(|_| !blocked)?.0;
        Some((hop, canvas, settings.clone()))
    }

    fn held(&self, keys: &[KeyCode]) -> bool {
        self.keys.any_pressed(keys.iter().copied())
    }

    /// The gesture a drag with `button` makes, from `(target, original)`
    /// targets, on the canvas it belongs to.
    fn gesture(
        &self,
        (target, original): (Entity, Entity),
        button: PointerButton,
    ) -> Option<(Gesture, Entity, CanvasInteraction)> {
        let (hop, canvas, settings) = self.hop(target)?;
        let is = |b: Option<PointerButton>| b == Some(button);
        let gesture = match hop {
            Hop::Port(port) if is(settings.connect_button) => Gesture::Wire(port),
            Hop::Node(node) if is(settings.drag_button) && self.grabs(node, original) => {
                Gesture::Move(node)
            }
            // Edges have no parent: their events don't reach the canvas.
            Hop::Canvas | Hop::Edge(_) if is(settings.pan_button) => Gesture::Pan,
            Hop::Canvas if is(settings.select_button) && self.on_background(canvas, original) => {
                Gesture::Box
            }
            _ => return None,
        };
        Some((gesture, canvas, settings))
    }

    fn view(&self, canvas: Entity) -> Option<CanvasView> {
        self.canvases.get(canvas).ok().map(|c| *c.1)
    }

    /// Window position → canvas-local pixels, through every transform above
    /// the canvas (such as an outer canvas's zoom); `None` before layout.
    fn local(&self, canvas: Entity, position: Vec2) -> Option<Vec2> {
        let (_, _, computed, transform) = self.canvases.get(canvas).ok()?;
        // The node's scale factor is the window's times `UiScale`; window
        // positions are scaled by the window's only.
        let scale = computed.inverse_scale_factor();
        let physical = position / (scale * self.ui_scale.0);
        let normalized = computed.normalize_point(*transform, physical)?;
        Some((normalized + 0.5) * computed.size() * scale)
    }

    /// Window position → graph space.
    fn graph_point(&self, canvas: Entity, position: Vec2) -> Option<Vec2> {
        Some(
            self.view(canvas)?
                .canvas_to_graph(self.local(canvas, position)?),
        )
    }

    /// How far the pointer at window `position` moved in graph space since
    /// it was `back` window pixels back.
    fn graph_delta(&self, canvas: Entity, position: Vec2, back: Vec2) -> Option<Vec2> {
        Some(self.graph_point(canvas, position)? - self.graph_point(canvas, position - back)?)
    }

    /// Moves `node` (with the selection) with the pointer at window
    /// `position`, by window-pixel `delta`, `total` so far.
    fn move_nodes(
        &mut self,
        canvas: Entity,
        node: Entity,
        position: Vec2,
        [delta, total]: [Vec2; 2],
        is_final: bool,
    ) {
        let delta = self.graph_delta(canvas, position, delta);
        let (Some(delta), Some(total)) = (delta, self.graph_delta(canvas, position, total)) else {
            return;
        };
        let nodes = self.graph.selection_with(node);
        let drag = Some(DragProgress { total, is_final });
        self.edit(canvas, GraphEdit::MoveNodes { nodes, delta, drag });
    }

    fn edit(&mut self, canvas: Entity, edit: GraphEdit) {
        self.commands
            .graph_edit_with_origin(canvas, edit, EditOrigin::Interaction);
    }

    fn select(&mut self, canvas: Entity, items: Vec<Entity>, mode: SelectMode) {
        self.commands.select(canvas, items, mode);
    }

    /// Puts `node` above its sibling nodes, unless its `ZIndex` is negative.
    fn raise(&mut self, node: Entity) {
        let z = |e: Entity| self.z_indices.get(e).map_or(0, |z| z.0);
        let Ok(parent) = self.parents.get(node) else {
            return;
        };
        let siblings = self
            .children
            .get(parent.parent())
            .into_iter()
            .flatten()
            .copied();
        let others = siblings.filter(|s| *s != node && self.graph.node_of(*s) == Some(*s));
        let top = others.map(&z).max();
        if z(node) >= 0 && top.is_some_and(|top| z(node) <= top) {
            let above = top.unwrap_or_default().saturating_add(1);
            self.commands.entity(node).try_insert(ZIndex(above));
        }
    }

    /// The entities from `original` up to, not including, `item`.
    fn path(&self, item: Entity, original: Entity) -> impl Iterator<Item = Entity> {
        let ancestors = std::iter::once(original).chain(self.parents.iter_ancestors(original));
        ancestors.take_while(move |e| *e != item)
    }

    /// Whether an event from `original` is for `item`, not a control inside it.
    fn owns(&self, item: Entity, original: Entity) -> bool {
        !self.path(item, original).any(|e| self.controls.contains(e))
    }

    /// Whether a drag from `original` moves `node`: with drag handles, only from one.
    fn grabs(&self, node: Entity, original: Entity) -> bool {
        let handle = |e: Entity| self.handles.contains(e);
        self.owns(node, original)
            && (self.path(node, original).any(handle)
                || !self.children.iter_descendants(node).any(handle))
    }

    /// Whether the event started on empty canvas (not on its nodes or edges).
    fn on_background(&self, canvas: Entity, original: Entity) -> bool {
        let g = &self.graph;
        let own = |e: Entity| g.canvas_of(e) == Some(canvas);
        !(g.edge_ports(original).is_some() && own(original))
            && g.node_of(original).is_none_or(|n| !own(n))
    }
}

fn on_press(mut press: On<PointerPress>, mut ctx: Ctx) {
    let Some((hop, canvas, settings)) = ctx.hop(press.event_target()) else {
        return;
    };
    let selecting = settings.select_button == Some(press.button);
    let additive = ctx.held(&settings.additive_keys);
    match hop {
        Hop::Port(_) if settings.connect_button == Some(press.button) => press.propagate(false),
        Hop::Node(item) | Hop::Edge(item)
            if selecting && ctx.owns(item, press.original_event_target()) =>
        {
            press.propagate(false);
            if additive {
                ctx.select(canvas, vec![item], SelectMode::Toggle);
            } else if !ctx.graph.is_selected(item) {
                ctx.select(canvas, vec![item], SelectMode::Replace);
            }
            if let (Hop::Node(_), true) = (hop, settings.raise_on_press) {
                ctx.raise(item);
            }
        }
        Hop::Canvas if selecting && ctx.on_background(canvas, press.original_event_target()) => {
            press.propagate(false);
            if !additive {
                ctx.select(canvas, Vec::new(), SelectMode::Replace);
            }
        }
        _ => {}
    }
}

fn on_drag_start(mut drag: On<PointerDragStart>, mut ctx: Ctx) {
    let target = (drag.event_target(), drag.original_event_target());
    let Some((gesture, canvas, settings)) = ctx.gesture(target, drag.button) else {
        return;
    };
    drag.propagate(false);
    match gesture {
        Gesture::Wire(port) => {
            // Dragging off a connected input picks up its most recent wire.
            let g = &ctx.graph;
            let is_input = g
                .port(port)
                .is_some_and(|p| p.direction == PortDirection::Input);
            let picked = g
                .edges_of(port)
                .last()
                .and_then(|e| Some((e, g.edge_ports(e)?)));
            let Some(pointer) = ctx.graph_point(canvas, drag.pointer.position) else {
                return;
            };
            let mut from = port;
            if let (true, true, Some((edge, ends))) = (settings.detach_wires, is_input, picked) {
                ctx.edit(canvas, GraphEdit::Disconnect { edge });
                from = ends.output;
            }
            ctx.commands
                .spawn((PendingWire { from, pointer }, WireOf(canvas)));
        }
        Gesture::Move(node) if !ctx.graph.is_selected(node) => {
            ctx.select(canvas, vec![node], SelectMode::Replace);
        }
        Gesture::Box if ctx.held(&settings.additive_keys) => {
            let kept = KeptSelection(ctx.graph.selected_in(canvas).collect());
            ctx.commands.entity(canvas).try_insert(kept);
        }
        _ => {}
    }
}

fn on_drag(
    mut drag: On<PointerDrag>,
    mut ctx: Ctx,
    mut wires: Query<(&mut PendingWire, &mut WireTarget, Option<&WireCandidates>)>,
    nodes: Query<(&NodePosition, &ComputedNode)>,
    kept: Query<&KeptSelection>,
    hovered: Res<HoverMap>,
) {
    let target = (drag.event_target(), drag.original_event_target());
    let Some((gesture, canvas, _)) = ctx.gesture(target, drag.button) else {
        return;
    };
    drag.propagate(false);
    let position = drag.pointer.position;
    match gesture {
        Gesture::Wire(_) => {
            let Some(pointer) = ctx.graph_point(canvas, position) else {
                return;
            };
            // Snap to a port it may connect to under the pointer.
            let under = hovered
                .get(&drag.pointer.id)
                .into_iter()
                .flat_map(|h| h.keys());
            if let Some(wire) = ctx.graph.wire_of(canvas)
                && let Ok((mut pending, mut target, candidates)) = wires.get_mut(wire)
            {
                let fits = |p: &&Entity| candidates.is_some_and(|c| c.contains(*p));
                target.set_if_neq(WireTarget(under.copied().find(|p| fits(&p))));
                pending.pointer = pointer;
            }
        }
        Gesture::Move(node) => {
            ctx.move_nodes(canvas, node, position, [drag.delta, drag.distance], false);
        }
        Gesture::Pan => {
            let now = ctx.local(canvas, position);
            let delta = now.zip(ctx.local(canvas, position - drag.delta));
            if let (Some((now, before)), Ok((_, mut view, ..))) =
                (delta, ctx.canvases.get_mut(canvas))
            {
                view.pan += now - before;
            }
        }
        Gesture::Box => {
            let (Some(start), Some(end), Some(view)) = (
                ctx.local(canvas, position - drag.distance),
                ctx.local(canvas, position),
                ctx.view(canvas),
            ) else {
                return;
            };
            let rect = Rect::from_corners(start, end);
            let area = Rect::from_corners(
                view.canvas_to_graph(rect.min),
                view.canvas_to_graph(rect.max),
            );
            let hits = ctx.graph.nodes_in(canvas).filter(|n| {
                nodes.get(*n).is_ok_and(|(p, c)| {
                    let size = c.size() * c.inverse_scale_factor();
                    !area
                        .intersect(Rect::from_corners(p.0, p.0 + size))
                        .is_empty()
                })
            });
            let kept = kept
                .get(canvas)
                .into_iter()
                .flat_map(|k| k.0.iter().copied());
            ctx.select(canvas, hits.chain(kept).collect(), SelectMode::Replace);
            ctx.commands.entity(canvas).try_insert(SelectionBox(rect));
        }
    }
}

fn on_drag_end(
    mut drag: On<PointerDragEnd>,
    mut ctx: Ctx,
    wires: Query<(Entity, &PendingWire, &WireTarget)>,
) {
    let target = (drag.event_target(), drag.original_event_target());
    let Some((gesture, canvas, _)) = ctx.gesture(target, drag.button) else {
        return;
    };
    drag.propagate(false);
    match gesture {
        Gesture::Wire(_) => {
            let wire = ctx.graph.wire_of(canvas).and_then(|w| wires.get(w).ok());
            let Some((entity, wire, target)) = wire else {
                return;
            };
            ctx.commands.entity(entity).try_despawn();
            let (from, position) = (wire.from, wire.pointer);
            match **target {
                Some(to) => ctx.edit(canvas, GraphEdit::Connect { from, to }),
                None => {
                    let dropped = WireDropped {
                        canvas,
                        from,
                        position,
                    };
                    ctx.commands.trigger(dropped);
                    ctx.commands.write_message(dropped);
                }
            }
        }
        Gesture::Move(node) => {
            let position = drag.pointer.position;
            ctx.move_nodes(canvas, node, position, [Vec2::ZERO, drag.distance], true);
        }
        Gesture::Pan | Gesture::Box => {
            ctx.commands
                .entity(canvas)
                .try_remove::<(SelectionBox, KeptSelection)>();
        }
    }
}

/// A cancelled pointer (a touch the system took over) gets no drag end: the
/// wire or selection box on the canvas it was over goes now.
fn on_cancel(cancel: On<PointerCancel>, mut ctx: Ctx) {
    let Some((_, canvas, _)) = ctx.hop(cancel.event_target()) else {
        return;
    };
    if let Some(wire) = ctx.graph.wire_of(canvas) {
        ctx.commands.entity(wire).try_despawn();
    }
    ctx.commands
        .entity(canvas)
        .try_remove::<(SelectionBox, KeptSelection)>();
}

fn on_scroll(mut scroll: On<PointerScroll>, mut ctx: Ctx) {
    let Some((Hop::Canvas | Hop::Edge(_), canvas, settings)) = ctx.hop(scroll.event_target())
    else {
        return;
    };
    if settings.scroll == ScrollMode::None {
        return;
    }
    scroll.propagate(false);
    // Lines come from mouse wheels, pixels from trackpads.
    let (factor, step) = match scroll.unit {
        MouseScrollUnit::Line => (1.1_f32.powf(scroll.y), 24.0),
        MouseScrollUnit::Pixel => ((scroll.y * 0.01).exp(), 1.0),
    };
    let zoom = ctx.held(&settings.zoom_modifiers)
        || settings.scroll == ScrollMode::Zoom
        || (settings.scroll == ScrollMode::Auto && scroll.unit == MouseScrollUnit::Line);
    let Some(anchor) = ctx.local(canvas, scroll.pointer.position) else {
        return;
    };
    let Ok((_, mut view, ..)) = ctx.canvases.get_mut(canvas) else {
        return;
    };
    if zoom {
        view.zoom_around(anchor, factor);
    } else {
        view.pan += Vec2::new(scroll.x, scroll.y) * step;
    }
}

/// Trackpad pinch zooms the innermost canvas under the mouse, at the pointer.
fn pinch_zoom(
    mut pinches: MessageReader<PinchGesture>,
    hovered: Res<HoverMap>,
    pointers: Res<PointerMap>,
    locations: Query<&PointerLocation>,
    mut ctx: Ctx,
) {
    let magnify: f32 = pinches.read().map(|p| p.0).sum();
    let top = hovered
        .get(&PointerId::Mouse)
        .and_then(|h| h.keys().next().copied());
    let canvas = top.and_then(|top| ctx.graph.canvas_of(top));
    let canvas = canvas.filter(|c| !ctx.disabled.contains(*c));
    let mouse = pointers
        .get_entity(PointerId::Mouse)
        .and_then(|e| locations.get(e).ok()?.location());
    let (Some(canvas), Some(location), true) = (canvas, mouse, magnify != 0.0) else {
        return;
    };
    let Some(settings) = ctx.canvases.get(canvas).ok().map(|c| c.0.clone()) else {
        return;
    };
    if settings.pinch_zoom {
        let anchor = ctx.local(canvas, location.position);
        if let (Some(anchor), Ok((_, mut view, ..))) = (anchor, ctx.canvases.get_mut(canvas)) {
            view.zoom_around(anchor, 1.0 + magnify);
        }
    }
}

/// A picking backend for edges with an [`EdgeHitbox`], running after Bevy's
/// UI backend: the nearest edge under the pointer is hit where its canvas is
/// the topmost UI (or, for edges above nodes, one of its nodes, but not a
/// port or a focusable control), so overlays, clipping and controls keep
/// working. The hit shares the UI's layer, on top.
#[allow(clippy::too_many_arguments, reason = "system parameters")]
fn pick_edges(
    mut messages: ParamSet<(MessageReader<PointerHits>, MessageWriter<PointerHits>)>,
    pointers: Res<PointerMap>,
    locations: Query<&PointerLocation>,
    cameras: Query<&Camera>,
    pickables: Query<&Pickable>,
    contents: Query<(&ComputedNode, &UiGlobalTransform), With<CanvasContent>>,
    hitboxes: Query<(Entity, &EdgeHitbox)>,
    controls: Query<(), With<TabIndex>>,
    parents: Query<&ChildOf>,
    graph: GraphQuery,
) {
    /// Minimum pick radius, in logical pixels.
    const MIN_RADIUS: f32 = 4.0;
    let mut hits = Vec::new();
    for ui in messages.p0().read() {
        // The topmost entity taking part in picking (skipping last frame's edge hits).
        let Some((top, data)) = ui.picks.iter().find(|(e, _)| {
            pickables
                .get(*e)
                .map_or(true, |p| p.is_hoverable || p.should_block_lower)
        }) else {
            continue;
        };
        let canvas = graph
            .canvas_of(*top)
            .filter(|_| graph.edge_ports(*top).is_none());
        let location = pointers
            .get_entity(ui.pointer)
            .and_then(|e| locations.get(e).ok()?.location());
        let (Some(canvas), Some(location), Ok(camera), None) =
            (canvas, location, cameras.get(data.camera), graph.port(*top))
        else {
            continue;
        };
        // Controls keep their UI hits even when a wire is drawn over them.
        // Stop at the graph node: keyboard navigation makes nodes tabbable
        // too, but their bodies must still allow edge picking.
        let node = graph.node_of(*top);
        let over_control = std::iter::once(*top)
            .chain(parents.iter_ancestors(*top))
            .take_while(|e| Some(*e) != node && *e != canvas)
            .any(|e| controls.contains(e));
        if over_control {
            continue;
        }
        let mut point = location.position * camera.target_scaling_factor().unwrap_or(1.0);
        point -= camera
            .physical_viewport_rect()
            .map_or(Vec2::ZERO, |v| v.min.as_vec2());
        // The nearest edge of `canvas` under the pointer, if any.
        let nearest = |canvas: Entity, below_too: bool| {
            let (computed, transform) = contents.get(graph.content_of(canvas)?).ok()?;
            let local =
                transform.inverse().transform_point2(point) * computed.inverse_scale_factor();
            let min_radius = MIN_RADIUS / transform.matrix2.x_axis.length().max(1e-6);
            let edges = hitboxes.iter().filter(|(edge, h)| {
                (below_too || !h.below_nodes) && graph.canvas_of(*edge) == Some(canvas)
            });
            let distances =
                edges.map(|(edge, h)| (edge, h.distance(local) / h.radius.max(min_radius)));
            let (edge, _) = distances
                .filter(|(_, d)| *d <= 1.0)
                .min_by(|a, b| a.1.total_cmp(&b.1))?;
            Some((edge, local))
        };
        // The canvas under the pointer first, then the canvases around it,
        // where the pointer is over the node holding the inner one: there
        // only edges drawn above nodes count.
        let over_node = graph
            .node_of(*top)
            .is_some_and(|n| graph.canvas_of(n) == Some(canvas));
        let outer = |c: Entity| graph.node_of(c).and_then(|n| graph.canvas_of(n));
        let mut found = nearest(canvas, !over_node);
        let mut current = outer(canvas);
        while let (None, Some(c)) = (found, current) {
            found = nearest(c, false);
            current = outer(c);
        }
        if let Some((edge, local)) = found {
            let hit = HitData::new(data.camera, -1.0, Some(local.extend(0.0)), None);
            hits.push(PointerHits::new(ui.pointer, vec![(edge, hit)], ui.order));
        }
    }
    messages.p1().write_batch(hits);
}

/// Gives `wire` its [`WireCandidates`]: one pass over the graph for the
/// built-in rules, then
/// [`ConnectionCheck`](crate::ConnectionCheck) observers for the connections
/// that can exist.
pub(crate) fn mark_candidates(world: &mut World, wire: Entity) {
    let canvas = world.get::<WireOf>(wire).map(|c| c.0);
    let Some((canvas, from)) = canvas.zip(world.get::<PendingWire>(wire).map(|w| w.from)) else {
        return;
    };
    let possible = |In((canvas, from)), graph: GraphQuery| {
        let nodes = graph.nodes_in(canvas);
        let ports = nodes.flat_map(|n| graph.ports_of(n));
        let connections = ports.filter_map(|to| graph.check_connection(canvas, from, to).ok());
        connections.collect::<Vec<_>>()
    };
    let Ok(connections) = world.run_system_cached_with(possible, (canvas, from)) else {
        return;
    };
    let mut candidates = EntityHashSet::default();
    for connection in connections {
        if ask(world, canvas, &connection).is_none() {
            candidates.insert(connection.ports.other(from));
        }
    }
    if let Ok(mut wire) = world.get_entity_mut(wire) {
        wire.insert(WireCandidates(candidates));
    }
}
