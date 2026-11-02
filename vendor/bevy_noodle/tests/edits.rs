//! Headless tests: no window, no rendering.

// Test helpers may panic: a panic is a failed test.
#![allow(
    clippy::disallowed_methods,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use std::num::NonZeroU32;

use bevy::ecs::system::SystemState;
use bevy::prelude::*;
use bevy::ui::Selected;
use bevy_noodle::prelude::*;
use bevy_noodle::{EditRejected, IncomingEdges, OutgoingEdges, RejectReason};

const NUM: PortType = PortType::named("num");
const TEXT: PortType = PortType::named("text");
const TWO: NonZeroU32 = NonZeroU32::new(2).unwrap();

#[derive(Resource, Default)]
struct Log(Vec<String>);

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, NoodleCorePlugin))
        .init_resource::<Log>();
    app.add_observer(|e: On<EditApplied>, mut log: ResMut<Log>| {
        log.0.push(format!("applied {}", kind(&e.change)))
    });
    app.add_observer(|e: On<EditRejected>, mut log: ResMut<Log>| {
        log.0.push(format!("rejected {:?}", e.reason))
    });
    app
}

fn kind(change: &GraphChange) -> &'static str {
    match change {
        GraphChange::Connected { .. } => "connect",
        GraphChange::Disconnected { .. } => "disconnect",
        GraphChange::Moved { .. } => "move",
        GraphChange::Deleted { .. } => "delete",
    }
}

/// Connects two ports of canvas `c`, returning the new edge.
fn connect(w: &mut World, c: Entity, from: Entity, to: Entity) -> Entity {
    match w.graph_edit(c, GraphEdit::Connect { from, to }) {
        Ok(GraphChange::Connected { edge, .. }) => edge,
        other => panic!("not connected: {other:?}"),
    }
}

/// A canvas with its content, as children of `parent` if given.
fn canvas(world: &mut World, parent: Option<Entity>) -> (Entity, Entity) {
    let canvas = world.spawn((NodeCanvas, Node::default())).id();
    if let Some(parent) = parent {
        world.entity_mut(canvas).insert(ChildOf(parent));
    }
    world.flush();
    (canvas, world.get::<Children>(canvas).unwrap()[0])
}

/// A node in `content` with ports nested one level, like real UI.
fn node(world: &mut World, content: Entity, ports: &[Port]) -> (Entity, Vec<Entity>) {
    let node = world
        .spawn((
            GraphNode,
            NodePosition::default(),
            Node::default(),
            ChildOf(content),
        ))
        .id();
    let row = world.spawn((Node::default(), ChildOf(node))).id();
    (
        node,
        ports
            .iter()
            .map(|p| world.spawn((*p, Node::default(), ChildOf(row))).id())
            .collect(),
    )
}

fn log(app: &mut App) -> Vec<String> {
    std::mem::take(&mut app.world_mut().resource_mut::<Log>().0)
}

fn query<T>(world: &mut World, f: impl FnOnce(&GraphQuery) -> T) -> T {
    let mut state: SystemState<GraphQuery> = SystemState::new(world);
    f(&state.get(world).unwrap())
}

#[test]
fn connect_normalizes_and_relates_ports() {
    let mut app = app();
    let w = app.world_mut();
    let (c, content) = canvas(w, None);
    let (_, a) = node(w, content, &[Port::output(NUM)]);
    let (_, b) = node(w, content, &[Port::input(NUM)]);
    let edge = connect(w, c, b[0], a[0]);
    assert_eq!(w.get::<EdgeSource>(edge).unwrap().0, a[0]);
    assert_eq!(**w.get::<OutgoingEdges>(a[0]).unwrap(), vec![edge]);
    assert_eq!(
        query(w, |g| (
            g.peers_of(b[0]).collect::<Vec<_>>(),
            g.canvas_of(edge)
        )),
        (vec![a[0]], Some(c))
    );
    assert_eq!(log(&mut app), ["applied connect"]);
}

#[test]
fn invalid_connections_are_rejected() {
    let mut app = app();
    let w = app.world_mut();
    let (c, content) = canvas(w, None);
    let (_, a) = node(w, content, &[Port::output(NUM), Port::input(NUM)]);
    let (_, b) = node(
        w,
        content,
        &[Port::input(NUM), Port::input(TEXT), Port::output(NUM)],
    );
    let mut connect = |from, to| w.graph_edit(c, GraphEdit::Connect { from, to }).err();
    assert_eq!(connect(a[0], a[1]), Some(RejectReason::SameNode));
    assert_eq!(connect(a[0], b[1]), Some(RejectReason::IncompatibleTypes));
    assert_eq!(connect(a[0], b[2]), Some(RejectReason::SameDirection));
    assert_eq!(connect(a[0], b[0]), None);
    assert_eq!(connect(a[0], b[0]), Some(RejectReason::AlreadyConnected));
    let rejected = log(&mut app)
        .into_iter()
        .filter(|l| l.starts_with("rejected"))
        .count();
    assert_eq!(rejected, 4);
    let messages = app
        .world_mut()
        .resource_mut::<Messages<EditRejected>>()
        .drain()
        .count();
    assert_eq!(messages, 4, "also written as messages");
}

#[test]
fn single_inputs_swap_and_wide_inputs_fill_up() {
    let mut app = app();
    let w = app.world_mut();
    let (c, content) = canvas(w, None);
    let outs: Vec<Entity> = (0..3)
        .map(|_| node(w, content, &[Port::output(NUM)]).1[0])
        .collect();
    let (_, single) = node(w, content, &[Port::input(NUM)]);
    let wide = Port::input(NUM).with_capacity(Capacity::Refuse(TWO));
    let (_, wide) = node(w, content, &[wide]);
    for out in &outs[..2] {
        w.graph_edit(
            c,
            GraphEdit::Connect {
                from: *out,
                to: single[0],
            },
        )
        .unwrap();
        w.graph_edit(
            c,
            GraphEdit::Connect {
                from: *out,
                to: wide[0],
            },
        )
        .unwrap();
    }
    assert_eq!(
        query(w, |g| g.peers_of(single[0]).collect::<Vec<_>>()),
        vec![outs[1]]
    );
    assert_eq!(
        w.graph_edit(
            c,
            GraphEdit::Connect {
                from: outs[2],
                to: wide[0]
            }
        ),
        Err(RejectReason::PortFull)
    );
    assert_eq!(
        log(&mut app)
            .iter()
            .filter(|l| *l == "applied disconnect")
            .count(),
        1
    );
}

#[test]
fn observers_can_reject_and_rewrite() {
    let mut app = app();
    let w = app.world_mut();
    let (c, content) = canvas(w, None);
    let (_, a) = node(w, content, &[Port::output(NUM)]);
    let (n, b) = node(w, content, &[Port::input(NUM)]);
    let blocked = b[0];
    w.add_observer(move |mut r: On<EditRequested>| match &mut r.edit {
        GraphEdit::Connect { to, .. } if *to == blocked => r.reject(),
        GraphEdit::MoveNodes { delta, .. } => *delta = (*delta / 10.0).round() * 10.0,
        _ => {}
    });
    assert_eq!(
        w.graph_edit(
            c,
            GraphEdit::Connect {
                from: a[0],
                to: b[0]
            }
        ),
        Err(RejectReason::Rejected)
    );
    let edit = GraphEdit::move_nodes(vec![n], Vec2::new(14.0, 26.0));
    w.graph_edit(c, edit).unwrap();
    assert_eq!(w.get::<NodePosition>(n).unwrap().0, Vec2::new(10.0, 30.0));
}

#[test]
fn deleting_nodes_or_ports_removes_edges() {
    let mut app = app();
    let w = app.world_mut();
    let (c, content) = canvas(w, None);
    let (na, a) = node(w, content, &[Port::output(NUM)]);
    let (_, b) = node(w, content, &[Port::input(NUM), Port::output(NUM)]);
    let (_, d) = node(w, content, &[Port::input(NUM)]);
    w.graph_edit(
        c,
        GraphEdit::Connect {
            from: a[0],
            to: b[0],
        },
    )
    .unwrap();
    let e2 = connect(w, c, b[1], d[0]);
    log(&mut app);
    let w = app.world_mut();
    w.graph_edit(c, GraphEdit::Delete { items: vec![na] })
        .unwrap();
    assert!(w.get::<IncomingEdges>(b[0]).is_none());
    assert_eq!(log(&mut app), ["applied disconnect", "applied delete"]);
    app.world_mut().despawn(d[0]);
    assert!(app.world().get_entity(e2).is_err());
}

#[test]
fn deleting_a_node_reports_the_edges_of_nodes_inside_it() {
    let mut app = app();
    let w = app.world_mut();
    let (c, content) = canvas(w, None);
    let (group, _) = node(w, content, &[]);
    let (_, inner) = node(w, group, &[Port::output(NUM)]);
    let (_, d) = node(w, content, &[Port::input(NUM)]);
    connect(w, c, inner[0], d[0]);
    log(&mut app);
    let delete = GraphEdit::Delete { items: vec![group] };
    app.world_mut().graph_edit(c, delete).unwrap();
    assert_eq!(log(&mut app), ["applied disconnect", "applied delete"]);
}

#[test]
fn selection_modes() {
    let mut app = app();
    let w = app.world_mut();
    let (c, content) = canvas(w, None);
    let n: Vec<Entity> = (0..3).map(|_| node(w, content, &[]).0).collect();
    let mut select = |nodes: &[usize], mode| {
        w.select(c, nodes.iter().map(|i| n[*i]).collect(), mode);
        n.iter()
            .map(|e| w.get::<Selected>(*e).is_some())
            .collect::<Vec<_>>()
    };
    assert_eq!(select(&[0, 1], SelectMode::Replace), [true, true, false]);
    assert_eq!(select(&[1, 2], SelectMode::Toggle), [true, false, true]);
    assert_eq!(select(&[0], SelectMode::Remove), [false, false, true]);
    assert_eq!(select(&[1], SelectMode::Add), [false, true, true]);
    assert!(log(&mut app).is_empty(), "selection is not an edit");

    // Other graphs' entities are ignored, and stay selected.
    let w = app.world_mut();
    let (other, other_content) = canvas(w, None);
    let (stranger, _) = node(w, other_content, &[]);
    w.select(other, vec![stranger], SelectMode::Replace);
    w.select(c, vec![stranger, n[0]], SelectMode::Replace);
    assert!(w.get::<Selected>(stranger).is_some());
    let mut selected = query(w, |g| g.selected_in(c).collect::<Vec<_>>());
    selected.sort();
    assert_eq!(selected, [n[0]]);
    assert_eq!(query(w, |g| g.selection_with(n[0])), [n[0]]);
    assert_eq!(query(w, |g| g.selection_with(n[2])), [n[2]]);
}

#[test]
fn source_of_follows_an_input_back_to_its_output() {
    let mut app = app();
    let w = app.world_mut();
    let (c, content) = canvas(w, None);
    let (_, a) = node(w, content, &[Port::output(NUM)]);
    let (b, inputs) = node(w, content, &[Port::input(NUM), Port::input(NUM)]);
    connect(w, c, a[0], inputs[1]);
    assert_eq!(
        query(w, |g| (0..3).map(|i| g.source_of(b, i)).collect::<Vec<_>>()),
        [None, Some(a[0]), None]
    );
}

#[test]
fn graphs_are_independent_and_can_nest() {
    let mut app = app();
    let w = app.world_mut();
    let (c1, content1) = canvas(w, None);
    let (c2, content2) = canvas(w, None);
    let (n1, p1) = node(w, content1, &[Port::output(NUM)]);
    let (_, p2) = node(w, content2, &[Port::input(NUM)]);
    // A third graph nested inside a node of the first.
    let (c3, content3) = canvas(w, Some(n1));
    let (n3, p3) = node(w, content3, &[Port::input(NUM)]);
    assert_eq!(
        w.graph_edit(
            c1,
            GraphEdit::Connect {
                from: p1[0],
                to: p2[0]
            }
        ),
        Err(RejectReason::NotInCanvas)
    );
    assert_eq!(
        w.graph_edit(
            c1,
            GraphEdit::Connect {
                from: p1[0],
                to: p3[0]
            }
        ),
        Err(RejectReason::NotInCanvas)
    );
    let (nodes1, ports1, canvas3) = query(w, |g| {
        (
            g.nodes_in(c1).collect::<Vec<_>>(),
            g.ports_of(n1).collect::<Vec<_>>(),
            g.canvas_of(n3),
        )
    });
    assert_eq!((nodes1, ports1, canvas3), (vec![n1], p1.clone(), Some(c3)));
    assert_eq!(
        w.graph_edit(c2, GraphEdit::Delete { items: vec![n1] }),
        Err(RejectReason::Empty)
    );
}

#[test]
fn reparenting_into_another_graph_drops_crossing_edges() {
    let mut app = app();
    let w = app.world_mut();
    let (c1, content1) = canvas(w, None);
    let (_, content2) = canvas(w, None);
    let (_, a) = node(w, content1, &[Port::output(NUM)]);
    let (nb, b) = node(w, content1, &[Port::input(NUM)]);
    let edge = connect(w, c1, a[0], b[0]);
    w.entity_mut(nb).insert(ChildOf(content2));
    app.update();
    assert!(app.world().get_entity(edge).is_err());
    assert!(log(&mut app).contains(&"applied disconnect".to_string()));
}

#[test]
fn unparenting_a_node_disconnects_its_edges() {
    let mut app = app();
    let w = app.world_mut();
    let (c, content) = canvas(w, None);
    let (_, a) = node(w, content, &[Port::output(NUM)]);
    let (nb, b) = node(w, content, &[Port::input(NUM)]);
    let edge = connect(w, c, a[0], b[0]);
    app.update();
    app.world_mut().entity_mut(nb).remove::<ChildOf>();
    app.update();
    assert!(app.world().get_entity(edge).is_err());
}

#[test]
fn edges_follow_their_ports_into_another_graph() {
    let mut app = app();
    let w = app.world_mut();
    let (c1, content1) = canvas(w, None);
    let (c2, content2) = canvas(w, None);
    let (na, a) = node(w, content1, &[Port::output(NUM)]);
    let (nb, b) = node(w, content1, &[Port::input(NUM)]);
    let edge = connect(w, c1, a[0], b[0]);
    let edges = |w: &mut World, c| query(w, |g| g.edges_in(c).collect::<Vec<_>>());
    assert_eq!(edges(w, c1), [edge]);
    w.entity_mut(na).insert(ChildOf(content2));
    w.entity_mut(nb).insert(ChildOf(content2));
    app.update();
    let w = app.world_mut();
    assert_eq!((edges(w, c1), edges(w, c2)), (vec![], vec![edge]));
}

#[test]
fn commands_apply_on_flush_and_types_are_auto_registered() {
    let mut app = app();
    let w = app.world_mut();
    let (c, content) = canvas(w, None);
    let (n, _) = node(w, content, &[]);
    w.commands()
        .graph_edit(c, GraphEdit::move_nodes(vec![n], Vec2::X));
    w.flush();
    assert_eq!(w.get::<NodePosition>(n).unwrap().0, Vec2::X);
    let registry = w.resource::<AppTypeRegistry>().read();
    for id in [
        std::any::TypeId::of::<Port>(),
        std::any::TypeId::of::<EdgeGeometry>(),
        std::any::TypeId::of::<CanvasInteraction>(),
    ] {
        assert!(registry.get(id).is_some());
    }
}

#[test]
fn edges_can_be_selected_and_deleted_with_nodes() {
    let mut app = app();
    let w = app.world_mut();
    let (c, content) = canvas(w, None);
    let (_, a) = node(w, content, &[Port::output(NUM)]);
    let (nb, b) = node(w, content, &[Port::input(NUM)]);
    let (nc, cc) = node(w, content, &[Port::input(NUM), Port::output(NUM)]);
    let e1 = connect(w, c, a[0], b[0]);
    let e2 = connect(w, c, a[0], cc[0]);
    let select = |w: &mut World, items, mode| w.select(c, items, mode);
    select(w, vec![e1, nb], SelectMode::Replace);
    assert!(w.get::<Selected>(e1).is_some() && w.get::<Selected>(nb).is_some());
    select(w, vec![nc], SelectMode::Replace);
    assert!(w.get::<Selected>(e1).is_none(), "replace clears edges too");
    log(&mut app);

    // Deleting a selection of one edge and one node.
    let w = app.world_mut();
    w.graph_edit(
        c,
        GraphEdit::Delete {
            items: vec![e2, nb],
        },
    )
    .unwrap();
    assert!(w.get_entity(e1).is_err() && w.get_entity(e2).is_err());
    assert!(w.get_entity(nb).is_err() && w.get_entity(nc).is_ok());
    let log = log(&mut app);
    assert_eq!(log.iter().filter(|l| *l == "applied disconnect").count(), 2);
}

/// A canvas with one node per port.
fn graph(world: &mut World, ports: &[Port]) -> (Entity, Vec<Entity>) {
    let (canvas, content) = canvas(world, None);
    let ports = ports.iter().map(|p| node(world, content, &[*p]).1[0]);
    (canvas, ports.collect())
}

/// An observer allowing connections refused for `reason`.
fn allow(reason: RejectReason) -> impl Fn(On<ConnectionCheck>) {
    move |mut check: On<ConnectionCheck>| {
        if check.refused == Some(reason) {
            check.allow();
        }
    }
}

#[test]
fn observers_may_allow_or_refuse_what_the_rules_decide() {
    let mut app = app();
    app.add_observer(allow(RejectReason::IncompatibleTypes));
    let w = app.world_mut();
    let (canvas, p) = graph(w, &[Port::output(NUM), Port::input(TEXT)]);
    let (from, to) = (p[0], p[1]);
    // Inputs may come first; the refusal is the observers' to override.
    let check = query(w, |g| g.check_connection(canvas, to, from)).unwrap();
    assert_eq!(check.ports, PortPair::new(from, to));
    assert_eq!(check.refused, Some(RejectReason::IncompatibleTypes));
    assert!(check.replaces.is_empty() && !check.allowed());
    connect(w, canvas, from, to);

    app.add_observer(|mut request: On<EditRequested>| request.reject());
    let w = app.world_mut();
    let (canvas, p) = graph(w, &[Port::output(NUM), Port::input(NUM)]);
    let (from, to) = (p[0], p[1]);
    let refused = w.graph_edit(canvas, GraphEdit::Connect { from, to });
    assert_eq!(refused, Err(RejectReason::Rejected));
}

#[test]
fn an_allowed_full_port_keeps_all_its_edges() {
    let mut app = app();
    let w = app.world_mut();
    let wide = Port::input(NUM).with_capacity(Capacity::Refuse(TWO));
    let out = Port::output(NUM);
    let (canvas, p) = graph(w, &[out, out, out, wide]);
    let to = p[3];
    for from in [p[0], p[1]] {
        w.graph_edit(canvas, GraphEdit::Connect { from, to })
            .unwrap();
    }
    let third = GraphEdit::Connect { from: p[2], to };
    assert_eq!(
        w.graph_edit(canvas, third.clone()),
        Err(RejectReason::PortFull)
    );
    app.add_observer(allow(RejectReason::PortFull));
    let w = app.world_mut();
    w.graph_edit(canvas, third).unwrap();
    assert_eq!(w.get::<IncomingEdges>(to).map(|e| e.len()), Some(3));
}

#[test]
fn previews_ask_connection_checks_but_change_nothing() {
    let mut app = app();
    #[derive(Resource, Default)]
    struct Asked {
        checks: u32,
        requests: u32,
    }
    app.init_resource::<Asked>();
    app.add_observer(|_: On<ConnectionCheck>, mut asked: ResMut<Asked>| asked.checks += 1);
    app.add_observer(|_: On<EditRequested>, mut asked: ResMut<Asked>| asked.requests += 1);
    app.add_observer(allow(RejectReason::IncompatibleTypes));
    // Structurally impossible connections never reach observers, even allowing ones.
    app.add_observer(|mut check: On<ConnectionCheck>| check.allow());
    let w = app.world_mut();
    let (canvas, p) = graph(
        w,
        &[Port::output(NUM), Port::input(TEXT), Port::output(NUM)],
    );
    let (from, to) = (p[0], p[1]);
    let preview = w.preview_connection(canvas, to, from).unwrap();
    assert_eq!(preview.ports, PortPair::new(from, to));
    assert!(preview.allowed());
    let asked = w.resource::<Asked>();
    assert_eq!(
        (asked.checks, asked.requests),
        (1, 0),
        "no EditRequested in previews"
    );
    assert_eq!(w.query::<&Edge>().iter(w).count(), 0);
    assert!(log(&mut app).is_empty(), "no applied or rejected events");
    let w = app.world_mut();
    let same = GraphEdit::Connect { from, to: p[2] };
    assert_eq!(w.graph_edit(canvas, same), Err(RejectReason::SameDirection));
    assert_eq!(
        w.preview_connection(canvas, from, p[2]),
        Err(RejectReason::SameDirection)
    );
}

#[test]
fn previews_and_edits_follow_the_same_rules() {
    let mut app = app();
    // Refuse anything into the second input.
    let w = app.world_mut();
    let (canvas, p) = graph(w, &[Port::output(NUM), Port::input(NUM), Port::input(NUM)]);
    let blocked = p[2];
    app.add_observer(move |mut check: On<ConnectionCheck>| {
        if check.ports.input == blocked {
            check.reject();
        }
    });
    let w = app.world_mut();
    let refused = Err(RejectReason::Rejected);
    assert_eq!(
        w.preview_connection(canvas, p[0], blocked).map(|_| ()),
        refused
    );
    let edit = GraphEdit::Connect {
        from: p[0],
        to: blocked,
    };
    assert_eq!(w.graph_edit(canvas, edit).map(|_| ()), refused);
    // An edit rewritten to the blocked port is checked again.
    app.add_observer(move |mut request: On<EditRequested>| {
        if let GraphEdit::Connect { to, .. } = &mut request.edit {
            *to = blocked;
        }
    });
    let w = app.world_mut();
    let edit = GraphEdit::Connect {
        from: p[0],
        to: p[1],
    };
    assert_eq!(w.graph_edit(canvas, edit).map(|_| ()), refused);
    assert_eq!(w.query::<&Edge>().iter(w).count(), 0);
}

#[test]
fn canvases_spawn_their_content_and_adopt_nodes() {
    let mut app = app();
    let w = app.world_mut();
    let canvas = w.spawn((NodeCanvas, Node::default())).id();
    let node = w.spawn((GraphNode, ChildOf(canvas))).id();
    w.flush();
    let content = query(w, |g| g.content_of(canvas)).unwrap();
    assert!(w.get::<CanvasContent>(content).is_some());
    assert_eq!(w.get::<ChildOf>(node).map(ChildOf::parent), Some(content));
    assert_eq!(w.get::<Children>(canvas).unwrap().to_vec(), [content]);
    // Moved under a canvas later, a node goes into the content too.
    let late = w.spawn((GraphNode, Node::default())).id();
    w.entity_mut(late).insert(ChildOf(canvas));
    app.update();
    let w = app.world_mut();
    assert_eq!(w.get::<ChildOf>(late).map(ChildOf::parent), Some(content));
    let contents = w.query::<&CanvasContent>().iter(w).count();
    assert_eq!(contents, 1);
}

#[test]
fn a_content_spawned_by_hand_replaces_the_empty_one() {
    let mut app = app();
    let w = app.world_mut();
    let canvas = w.spawn((NodeCanvas, Node::default())).id();
    let mine = w.spawn((CanvasContent, ChildOf(canvas))).id();
    w.flush();
    assert_eq!(query(w, |g| g.content_of(canvas)), Some(mine));
    assert_eq!(w.query::<&CanvasContent>().iter(w).count(), 1);
}

#[test]
fn full_replacing_ports_drop_their_oldest_edges() {
    let mut app = app();
    let w = app.world_mut();
    let two = Port::input(NUM).with_capacity(Capacity::Replace(TWO));
    let out = Port::output(NUM);
    let (canvas, p) = graph(w, &[out, out, out, two]);
    let to = p[3];
    let edges: Vec<Entity> = p[..3]
        .iter()
        .map(|from| connect(w, canvas, *from, to))
        .collect();
    assert!(w.get_entity(edges[0]).is_err(), "the oldest made room");
    assert_eq!(
        query(w, |g| g.peers_of(to).collect::<Vec<_>>()),
        vec![p[1], p[2]]
    );
}
