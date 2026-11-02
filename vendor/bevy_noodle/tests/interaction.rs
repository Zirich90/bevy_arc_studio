//! Pointer interaction, headless: pointer events are triggered directly, as
//! `bevy_picking` would.

// Test helpers may panic: a panic is a failed test.
#![allow(
    clippy::disallowed_methods,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use bevy::camera::NormalizedRenderTarget;
use bevy::ecs::entity::EntityHashSet;
use bevy::input::gestures::PinchGesture;
use bevy::input_focus::tab_navigation::TabIndex;
use bevy::picking::backend::HitData;
use bevy::picking::hover::HoverMap;
use bevy::picking::pointer::{Location, PointerId, PointerLocation};
use bevy::prelude::*;
use bevy::ui::{ComputedNode, InteractionDisabled, Selected, UiGlobalTransform, UiScale};
use bevy_noodle::prelude::*;
use bevy_noodle::{DragProgress, WireCandidates, WireTarget};

const NUM: PortType = PortType::named("num");

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, bevy::input::InputPlugin, NoodlePlugins));
    // Pointer events bubble through `PointerTraversal`, which reads `Window`.
    app.world_mut().register_component::<Window>();
    app
}

fn graph(world: &mut World) -> (Entity, Entity) {
    let canvas = world
        .spawn((NodeCanvas, CanvasInteraction::default(), laid_out()))
        .id();
    world.flush();
    (canvas, world.get::<Children>(canvas).unwrap()[0])
}

/// A UI node as layout leaves it: 400 by 300 pixels at the window's origin.
fn laid_out() -> (Node, ComputedNode) {
    let size = Vec2::new(400.0, 300.0);
    let computed = ComputedNode { size, ..default() };
    (Node::default(), computed)
}

/// One node per port in `content`; returns the ports.
fn one_node_each<const N: usize>(w: &mut World, content: Entity, ports: [Port; N]) -> [Entity; N] {
    ports.map(|port| {
        let node = w.spawn((GraphNode, Node::default(), ChildOf(content))).id();
        w.spawn((port, Node::default(), ChildOf(node))).id()
    })
}

/// Whether a dragged wire may connect to `port`.
fn candidate(w: &mut World, port: Entity) -> bool {
    w.query::<&WireCandidates>()
        .iter(w)
        .any(|c| c.contains(&port))
}

fn hit() -> HitData {
    HitData::new(Entity::PLACEHOLDER, 0.0, None, None)
}

/// A pointer event's own fields; `pointer` adds the target and the pointer.
trait Fields {
    fn trigger(self, world: &mut World, entity: Entity, pointer: Pointer);
}

macro_rules! fields {
    ($name:ident => $event:ident { $($field:ident: $ty:ty),* }) => {
        struct $name { $($field: $ty),* }
        impl Fields for $name {
            fn trigger(self, world: &mut World, entity: Entity, pointer: Pointer) {
                world.trigger($event { entity, pointer, $($field: self.$field),* });
            }
        }
    };
}

fields!(Press => PointerPress { button: PointerButton, hit: HitData, count: u8 });
fields!(DragStart => PointerDragStart { button: PointerButton, hit: HitData });
fields!(Drag => PointerDrag { button: PointerButton, distance: Vec2, delta: Vec2 });
fields!(DragEnd => PointerDragEnd { button: PointerButton, distance: Vec2 });
fields!(Cancel => PointerCancel { hit: HitData });

/// Triggers a pointer event on `target` (bubbling up), then applies its edits.
fn pointer(world: &mut World, target: Entity, fields: impl Fields) {
    pointer_at(world, target, Vec2::ZERO, fields);
}

/// [`pointer`] with the pointer at window `position`.
fn pointer_at(world: &mut World, target: Entity, position: Vec2, fields: impl Fields) {
    let target_none = NormalizedRenderTarget::None {
        width: 1,
        height: 1,
    };
    let location = Location {
        target: target_none,
        position,
    };
    fields.trigger(world, target, Pointer::new(PointerId::Mouse, location));
    world.flush();
}

fn drag(world: &mut World, target: Entity, by: Vec2) {
    let button = PointerButton::Primary;
    pointer(world, target, DragStart { button, hit: hit() });
    pointer(
        world,
        target,
        Drag {
            button,
            distance: by,
            delta: by,
        },
    );
    pointer(
        world,
        target,
        DragEnd {
            button,
            distance: by,
        },
    );
}

#[test]
fn nodes_with_a_drag_handle_move_only_from_it() {
    let mut app = app();
    let w = app.world_mut();
    let (_, content) = graph(w);
    let node = w
        .spawn((
            GraphNode,
            NodePosition::default(),
            Node::default(),
            ChildOf(content),
        ))
        .id();
    let handle = w
        .spawn((NodeDragHandle, Node::default(), ChildOf(node)))
        .id();
    let body = w.spawn((Node::default(), ChildOf(node))).id();

    drag(w, body, Vec2::new(10.0, 0.0));
    assert_eq!(w.get::<NodePosition>(node).unwrap().0, Vec2::ZERO);
    drag(w, handle, Vec2::new(10.0, 0.0));
    assert_eq!(w.get::<NodePosition>(node).unwrap().0, Vec2::new(10.0, 0.0));
}

#[test]
fn drags_stream_steps_and_end_with_their_total() {
    let mut app = app();
    app.add_message::<EditApplied>();
    let w = app.world_mut();
    let (_, content) = graph(w);
    let node = w
        .spawn((
            GraphNode,
            NodePosition::default(),
            Node::default(),
            ChildOf(content),
        ))
        .id();
    drag(w, node, Vec2::new(10.0, 4.0));
    let moves: Vec<_> = w
        .resource_mut::<Messages<EditApplied>>()
        .drain()
        .filter_map(|e| match e.change {
            GraphChange::Moved { drag, .. } => Some((e.change.is_drag_step(), drag)),
            _ => None,
        })
        .collect();
    let progress = |is_final| {
        Some(DragProgress {
            total: Vec2::new(10.0, 4.0),
            is_final,
        })
    };
    assert_eq!(moves, [(true, progress(false)), (false, progress(true))]);
}

#[test]
fn dragging_from_a_port_snaps_to_the_hovered_port_and_connects() {
    let mut app = app();
    let w = app.world_mut();
    let (_, content) = graph(w);
    let ports = one_node_each(w, content, [Port::output(NUM), Port::input(NUM)]);
    let button = PointerButton::Primary;
    pointer(w, ports[0], DragStart { button, hit: hit() });
    assert_eq!(w.query::<&PendingWire>().iter(w).count(), 1);

    // bevy_picking keeps hovering during a drag; the wire snaps to what it hovers.
    let hovered = [(ports[1], hit())].into_iter().collect();
    w.resource_mut::<HoverMap>()
        .insert(PointerId::Mouse, hovered);
    pointer(
        w,
        ports[0],
        Drag {
            button,
            distance: Vec2::X,
            delta: Vec2::X,
        },
    );
    let target = *w.query::<&WireTarget>().single(w).unwrap();
    assert_eq!(target, WireTarget(Some(ports[1])));

    pointer(
        w,
        ports[0],
        DragEnd {
            button,
            distance: Vec2::X,
        },
    );
    let edges: Vec<_> = w.query::<&EdgeTarget>().iter(w).map(|t| t.0).collect();
    assert_eq!(edges, [ports[1]]);
    assert_eq!(w.query::<&PendingWire>().iter(w).count(), 0);
}

#[test]
fn panning_works_from_an_edge() {
    let mut app = app();
    let w = app.world_mut();
    let (canvas, content) = graph(w);
    let [from, to] = one_node_each(w, content, [Port::output(NUM), Port::input(NUM)]);
    let edit = GraphEdit::Connect { from, to };
    let Ok(GraphChange::Connected { edge, .. }) = w.graph_edit(canvas, edit) else {
        panic!("connected");
    };
    // Edges have no parent, so their events never bubble to the canvas.
    let button = PointerButton::Middle;
    let by = Vec2::new(30.0, -10.0);
    pointer(w, edge, DragStart { button, hit: hit() });
    let (distance, delta) = (by, by);
    pointer(
        w,
        edge,
        Drag {
            button,
            distance,
            delta,
        },
    );
    let pan = w.get::<CanvasView>(canvas).unwrap().pan;
    assert!((pan - by).length() < 1e-3, "{pan}");
}

#[test]
fn pinching_zooms_the_innermost_canvas_under_the_mouse() {
    let mut app = app();
    let w = app.world_mut();
    let (outer, content) = graph(w);
    let node = w.spawn((GraphNode, Node::default(), ChildOf(content))).id();
    let inner = w
        .spawn((
            NodeCanvas,
            CanvasInteraction::default(),
            laid_out(),
            ChildOf(node),
        ))
        .id();
    let location = Location {
        target: NormalizedRenderTarget::None {
            width: 1,
            height: 1,
        },
        position: Vec2::ZERO,
    };
    w.spawn((PointerId::Mouse, PointerLocation::new(location)));
    let hovered = [(inner, hit())].into_iter().collect();
    w.resource_mut::<HoverMap>()
        .insert(PointerId::Mouse, hovered);
    w.write_message(PinchGesture(0.5));
    app.update();
    let zoom = |app: &App, canvas| app.world().get::<CanvasView>(canvas).unwrap().zoom;
    assert_eq!((zoom(&app, inner), zoom(&app, outer)), (1.5, 1.0));

    // Not while interaction is disabled.
    app.world_mut()
        .entity_mut(inner)
        .insert(InteractionDisabled);
    app.world_mut().write_message(PinchGesture(0.5));
    app.update();
    assert_eq!(zoom(&app, inner), 1.5);
}

#[test]
fn dragged_wires_snap_to_ports_observers_allow() {
    let mut app = app();
    const TEXT: PortType = PortType::named("text");
    app.add_observer(|mut check: On<ConnectionCheck>| {
        if check.refused == Some(bevy_noodle::RejectReason::IncompatibleTypes) {
            check.allow();
        }
    });
    let w = app.world_mut();
    let (_, content) = graph(w);
    let ports = one_node_each(w, content, [Port::output(NUM), Port::input(TEXT)]);
    let button = PointerButton::Primary;
    pointer(w, ports[0], DragStart { button, hit: hit() });
    assert!(candidate(w, ports[1]), "the observer allows it");
    let hovered = [(ports[1], hit())].into_iter().collect();
    w.resource_mut::<HoverMap>()
        .insert(PointerId::Mouse, hovered);
    let (distance, delta) = (Vec2::X, Vec2::X);
    pointer(
        w,
        ports[0],
        Drag {
            button,
            distance,
            delta,
        },
    );
    pointer(w, ports[0], DragEnd { button, distance });
    let targets: Vec<_> = w.query::<&EdgeTarget>().iter(w).map(|t| t.0).collect();
    assert_eq!(targets, [ports[1]]);
}

/// Feeds the edge picking backend a UI hit on `top` at `at`, and returns the
/// edge it reports, if any.
fn pick(app: &mut App, top: Entity, at: Vec2) -> Option<Entity> {
    use bevy::picking::backend::PointerHits;
    let w = app.world_mut();
    let camera = w.spawn(Camera::default()).id();
    let location = Location {
        target: NormalizedRenderTarget::None {
            width: 1,
            height: 1,
        },
        position: at,
    };
    w.spawn((PointerId::Mouse, PointerLocation::new(location)));
    let hit = HitData::new(camera, 0.0, None, None);
    w.write_message(PointerHits::new(PointerId::Mouse, vec![(top, hit)], 0.5));
    app.update();
    let messages = app.world().resource::<Messages<PointerHits>>();
    let mut cursor = messages.get_cursor();
    let picks: Vec<Entity> = cursor
        .read(messages)
        .flat_map(|h| h.picks.iter().map(|(e, _)| *e))
        .collect();
    // The latest hit: the previous frame's messages are still buffered.
    picks
        .into_iter()
        .rev()
        .find(|e| app.world().get::<Edge>(*e).is_some())
}

#[test]
fn outer_edges_are_pickable_over_nested_canvases() {
    let mut app = app();
    let w = app.world_mut();
    let (_, outer) = graph(w);
    let group = w.spawn((GraphNode, Node::default(), ChildOf(outer))).id();
    let inner_canvas = w
        .spawn((
            NodeCanvas,
            CanvasInteraction::default(),
            Node::default(),
            ChildOf(group),
        ))
        .id();
    w.flush();
    let inner = w.get::<Children>(inner_canvas).unwrap()[0];
    // An edge of `content` running along y = 0, from x = 0 to x = 100.
    let edge_in = |w: &mut World, content: Entity| {
        let ports = one_node_each(w, content, [Port::output(NUM), Port::input(NUM)]);
        let points = [
            Vec2::ZERO,
            Vec2::new(30.0, 0.0),
            Vec2::new(70.0, 0.0),
            Vec2::new(100.0, 0.0),
        ];
        let hitbox = EdgeHitbox {
            points,
            radius: 4.0,
            below_nodes: false,
        };
        w.spawn((Edge, EdgeSource(ports[0]), EdgeTarget(ports[1]), hitbox))
            .id()
    };
    let outer_edge = edge_in(w, outer);
    // Over the nested canvas, where only the outer edge passes.
    assert_eq!(
        pick(&mut app, inner_canvas, Vec2::new(50.0, 1.0)),
        Some(outer_edge)
    );
    // Where an inner edge passes too, the inner one wins.
    let w = app.world_mut();
    let inner_edge = edge_in(w, inner);
    assert_eq!(
        pick(&mut app, inner_canvas, Vec2::new(50.0, 1.0)),
        Some(inner_edge)
    );
}

#[test]
fn edges_above_nodes_do_not_intercept_controls_or_their_children() {
    for target_index in 0..3 {
        let mut app = app();
        let w = app.world_mut();
        let (_, content) = graph(w);
        let ports = one_node_each(w, content, [Port::output(NUM), Port::input(NUM)]);
        let node = w.get::<ChildOf>(ports[0]).unwrap().parent();
        // Keyboard-enabled graph nodes are tabbable; that must not stop
        // edges from being picked over their ordinary backgrounds.
        w.entity_mut(node).insert(TabIndex(0));
        let control = w.spawn((TabIndex(0), Node::default(), ChildOf(node))).id();
        let child = w.spawn((Node::default(), ChildOf(control))).id();
        let edge = w
            .spawn((
                Edge,
                EdgeSource(ports[0]),
                EdgeTarget(ports[1]),
                EdgeHitbox {
                    points: [
                        Vec2::ZERO,
                        Vec2::new(30.0, 0.0),
                        Vec2::new(70.0, 0.0),
                        Vec2::new(100.0, 0.0),
                    ],
                    radius: 4.0,
                    below_nodes: false,
                },
            ))
            .id();
        let target = [node, control, child][target_index];
        let expected = (target == node).then_some(edge);
        assert_eq!(pick(&mut app, target, Vec2::new(50.0, 1.0)), expected);
    }
}

#[test]
fn controls_inside_nodes_keep_their_presses_and_drags() {
    let mut app = app();
    let w = app.world_mut();
    let (_, content) = graph(w);
    let node = w
        .spawn((
            GraphNode,
            NodePosition::default(),
            Node::default(),
            ChildOf(content),
        ))
        .id();
    // A focusable control, such as a slider or text field, and a label.
    let control = w.spawn((TabIndex(0), Node::default(), ChildOf(node))).id();
    let thumb = w.spawn((Node::default(), ChildOf(control))).id();
    let label = w.spawn((Text::new("Value"), ChildOf(node))).id();

    let button = PointerButton::Primary;
    pointer(
        w,
        thumb,
        Press {
            button,
            hit: hit(),
            count: 1,
        },
    );
    drag(w, thumb, Vec2::new(10.0, 0.0));
    assert_eq!(w.get::<NodePosition>(node).unwrap().0, Vec2::ZERO);
    assert!(w.get::<Selected>(node).is_none());

    pointer(
        w,
        label,
        Press {
            button,
            hit: hit(),
            count: 1,
        },
    );
    drag(w, label, Vec2::new(10.0, 0.0));
    assert_eq!(w.get::<NodePosition>(node).unwrap().0, Vec2::new(10.0, 0.0));
    assert!(w.get::<Selected>(node).is_some());
}

#[test]
fn wires_mark_candidates_of_their_canvas_while_they_exist() {
    let mut app = app();
    let w = app.world_mut();
    let [(first, fits, other), (_, second_fits, _)] = [(); 2].map(|()| {
        let (canvas, content) = graph(w);
        let ports = [Port::output(NUM), Port::input(NUM), Port::output(NUM)];
        let [from, fits, other] = one_node_each(w, content, ports);
        let pointer = Vec2::ZERO;
        let wire = w.spawn((PendingWire { from, pointer }, WireOf(canvas)));
        (wire.id(), fits, other)
    });
    w.flush();
    assert!(candidate(w, fits));
    assert!(!candidate(w, other));
    w.despawn(first);
    w.flush();
    assert!(!candidate(w, fits));
    assert!(candidate(w, second_fits), "another canvas keeps its marks");
}

#[test]
fn dragging_off_a_connected_input_picks_up_its_wire() {
    let mut app = app();
    let w = app.world_mut();
    let (canvas, content) = graph(w);
    let [output, input] = one_node_each(w, content, [Port::output(NUM), Port::input(NUM)]);
    w.graph_edit(
        canvas,
        GraphEdit::Connect {
            from: output,
            to: input,
        },
    )
    .unwrap();
    let button = PointerButton::Primary;
    pointer(w, input, DragStart { button, hit: hit() });
    let wire = *w.query::<&PendingWire>().single(w).unwrap();
    assert_eq!(wire.from, output, "the wire hangs off the output now");
    assert_eq!(w.query::<&Edge>().iter(w).count(), 0);
    assert!(candidate(w, input));

    let distance = Vec2::X;
    pointer(w, input, DragEnd { button, distance });
    assert_eq!(w.query::<&PendingWire>().iter(w).count(), 0);
    assert_eq!(w.query::<&Edge>().iter(w).count(), 0, "dropped on nothing");
    assert!(!candidate(w, input));
}

#[test]
fn pressing_raises_nodes_without_reordering_them() {
    let mut app = app();
    let w = app.world_mut();
    let (_, content) = graph(w);
    let spawn = |w: &mut World, z: i32| {
        let node = (GraphNode, Node::default(), ZIndex(z), ChildOf(content));
        w.spawn(node).id()
    };
    let (a, b, frame) = (spawn(w, 0), spawn(w, 0), spawn(w, -1));
    let order = w.get::<Children>(content).unwrap().to_vec();
    let press = |w: &mut World, node| {
        let button = PointerButton::Primary;
        let (hit, count) = (hit(), 1);
        pointer(w, node, Press { button, hit, count });
    };
    let z = |w: &World, node| w.get::<ZIndex>(node).unwrap().0;
    press(w, a);
    assert!(z(w, a) > z(w, b));
    press(w, b);
    assert!(z(w, b) > z(w, a));
    press(w, b);
    assert_eq!(z(w, b), 2, "already on top");
    press(w, frame);
    assert_eq!(z(w, frame), -1, "negative ZIndex stays under");
    assert_eq!(w.get::<Children>(content).unwrap().to_vec(), order);
}

#[test]
fn wires_dropped_on_empty_canvas_are_reported() {
    let mut app = app();
    #[derive(Resource, Default)]
    struct Dropped(Vec<Entity>);
    app.init_resource::<Dropped>();
    app.add_observer(|d: On<WireDropped>, mut dropped: ResMut<Dropped>| dropped.0.push(d.from));
    let w = app.world_mut();
    let (canvas, content) = graph(w);
    let [out] = one_node_each(w, content, [Port::output(NUM)]);
    drag(w, out, Vec2::new(40.0, 0.0));
    assert_eq!(w.resource::<Dropped>().0, [out]);
    let messages: Vec<_> = w.resource_mut::<Messages<WireDropped>>().drain().collect();
    assert_eq!((messages.len(), messages[0].canvas), (1, canvas));
}

#[test]
fn drags_follow_the_pointer_through_an_outer_zoom() {
    let mut app = app();
    let w = app.world_mut();
    let (canvas, content) = graph(w);
    // As inside an outer canvas zoomed in twice.
    w.entity_mut(canvas)
        .insert(UiGlobalTransform::from_scale(Vec2::splat(2.0)));
    let node = w
        .spawn((
            GraphNode,
            NodePosition::default(),
            Node::default(),
            ChildOf(content),
        ))
        .id();
    drag(w, node, Vec2::new(10.0, 0.0));
    assert_eq!(w.get::<NodePosition>(node).unwrap().0, Vec2::new(5.0, 0.0));
}

#[test]
fn wires_follow_the_pointer_under_ui_scale() {
    let mut app = app();
    let w = app.world_mut();
    w.insert_resource(UiScale(2.0));
    let (canvas, content) = graph(w);
    // Layout under `UiScale` 2: physical sizes, the top left at the origin.
    let size = Vec2::new(400.0, 300.0);
    let computed = ComputedNode {
        size,
        inverse_scale_factor: 0.5,
        ..default()
    };
    let at_origin = UiGlobalTransform::from_translation(size / 2.0);
    w.entity_mut(canvas).insert((computed, at_origin));
    let [port] = one_node_each(w, content, [Port::output(NUM)]);
    let button = PointerButton::Primary;
    pointer_at(
        w,
        port,
        Vec2::new(100.0, 50.0),
        DragStart { button, hit: hit() },
    );
    let wire = w.query::<&PendingWire>().single(w).unwrap();
    assert!((wire.pointer - Vec2::new(50.0, 25.0)).length() < 1e-3);
}

#[test]
fn cancelled_pointers_drop_their_wire() {
    let mut app = app();
    let w = app.world_mut();
    let (_, content) = graph(w);
    let [port] = one_node_each(w, content, [Port::output(NUM)]);
    let button = PointerButton::Primary;
    pointer(w, port, DragStart { button, hit: hit() });
    assert_eq!(w.query::<&PendingWire>().iter(w).count(), 1);
    pointer(w, port, Cancel { hit: hit() });
    assert_eq!(w.query::<&PendingWire>().iter(w).count(), 0);
}

#[test]
fn wires_go_with_their_port() {
    let mut app = app();
    let w = app.world_mut();
    let (_, content) = graph(w);
    let [port] = one_node_each(w, content, [Port::output(NUM)]);
    let button = PointerButton::Primary;
    pointer(w, port, DragStart { button, hit: hit() });
    let node = w.get::<ChildOf>(port).unwrap().parent();
    w.despawn(node);
    app.update();
    let w = app.world_mut();
    assert_eq!(w.query::<&PendingWire>().iter(w).count(), 0);
}

#[test]
fn additive_box_selection_shrinks_back_to_what_was_selected() {
    let mut app = app();
    let w = app.world_mut();
    let (canvas, content) = graph(w);
    let size = ComputedNode {
        size: Vec2::splat(10.0),
        ..default()
    };
    let [a, b] = [0.0, 100.0].map(|x| {
        let at = NodePosition(Vec2::new(x, 0.0));
        w.spawn((GraphNode, at, Node::default(), size, ChildOf(content)))
            .id()
    });
    w.select(canvas, vec![a], SelectMode::Replace);
    w.resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::ShiftLeft);
    // The canvas is centred on the window's origin: window = local - (200, 150).
    let window = |local: Vec2| local - Vec2::new(200.0, 150.0);
    let start = Vec2::new(-10.0, -10.0);
    let button = PointerButton::Primary;
    pointer_at(w, canvas, window(start), DragStart { button, hit: hit() });
    let selected = |w: &mut World, to: Vec2| {
        let (distance, delta) = (to - start, to - start);
        pointer_at(
            w,
            canvas,
            window(to),
            Drag {
                button,
                distance,
                delta,
            },
        );
        let mut selected = w.query_filtered::<Entity, With<Selected>>();
        selected.iter(w).collect::<EntityHashSet>()
    };
    assert_eq!(
        selected(w, Vec2::new(120.0, 20.0)),
        EntityHashSet::from_iter([a, b])
    );
    assert_eq!(
        selected(w, Vec2::new(50.0, 20.0)),
        EntityHashSet::from_iter([a])
    );
}
