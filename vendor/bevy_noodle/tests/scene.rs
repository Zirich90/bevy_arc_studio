//! Snapshots (feature `scene`): headless, no rendering.

// Test helpers may panic: a panic is a failed test.
#![allow(
    clippy::disallowed_methods,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use bevy::prelude::*;
use bevy_noodle::prelude::*;
use bevy_noodle::{OutgoingEdges, scene};

const NUM: PortType = PortType::named("num");

#[derive(Component, Reflect, Clone, Copy, PartialEq, Debug)]
#[reflect(Component)]
struct Payload(f32);

fn graph(world: &mut World) -> (Entity, Entity, [Entity; 2]) {
    let canvas = world.spawn((NodeCanvas, Node::default())).id();
    let content = content_of(world, canvas);
    let ports = [Port::output(NUM), Port::input(NUM)].map(|port| {
        let node = world
            .spawn((
                GraphNode,
                NodePosition(Vec2::X),
                Payload(2.5),
                ChildOf(content),
            ))
            .id();
        world.spawn((port, Node::default(), ChildOf(node))).id()
    });
    (canvas, content, ports)
}

/// The content a canvas spawned for itself.
fn content_of(world: &mut World, canvas: Entity) -> Entity {
    world.flush();
    world.get::<Children>(canvas).unwrap()[0]
}

/// Snapshots here hold no asset handles.
struct NoAssets;

impl bevy::asset::LoadFromPath for NoAssets {
    fn load_from_path_erased(
        &mut self,
        _: std::any::TypeId,
        path: bevy::asset::AssetPath<'static>,
    ) -> bevy::asset::UntypedHandle {
        panic!("no asset expected: {path}")
    }
}

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, NoodleCorePlugin));
    app
}

#[test]
fn applied_edits_report_their_ports() {
    let mut app = app();
    app.add_message::<EditApplied>();
    let w = app.world_mut();
    let (canvas, _, [out, inp]) = graph(w);
    let edit = GraphEdit::Connect { from: inp, to: out };
    let Ok(GraphChange::Connected { edge, ports }) = w.graph_edit(canvas, edit) else {
        panic!("connected");
    };
    assert_eq!(ports, PortPair::new(out, inp));
    w.graph_edit(canvas, GraphEdit::Disconnect { edge })
        .unwrap();
    let changes: Vec<_> = w
        .resource_mut::<Messages<EditApplied>>()
        .drain()
        .map(|e| e.change)
        .collect();
    use GraphChange::*;
    assert_eq!(
        changes,
        [Connected { edge, ports }, Disconnected { edge, ports }]
    );
}

#[test]
fn snapshot_round_trip() {
    let mut app = app();
    let w = app.world_mut();
    let (canvas, content, [out, inp]) = graph(w);
    w.graph_edit(canvas, GraphEdit::Connect { from: out, to: inp })
        .unwrap();
    // A control the app rebuilds from `Payload` stays out, with its children.
    let node = w.get::<ChildOf>(out).unwrap().parent();
    let control = w.spawn((scene::Transient, ChildOf(node))).id();
    w.spawn(ChildOf(control));
    let saved = w.snapshot(canvas).unwrap();
    assert_eq!(saved.entities.len(), 5, "two nodes, two ports and an edge");

    // Change everything, then restore.
    let nodes: Vec<_> = w.get::<Children>(content).unwrap().to_vec();
    w.graph_edit(canvas, GraphEdit::Delete { items: nodes })
        .unwrap();
    assert!(w.get::<Children>(content).is_none_or(|c| c.is_empty()));
    let map = w.restore_snapshot(canvas, &saved).unwrap();
    app.update();

    let w = app.world_mut();
    let children = w.get::<Children>(content).unwrap().to_vec();
    assert_eq!(children.len(), 2, "two nodes");
    let mut edges = w.query_filtered::<(&EdgeSource, &EdgeTarget), With<Edge>>();
    let (source, target) = edges.single(w).unwrap();
    assert_eq!((source.0, target.0), (map[&out], map[&inp]));
    assert_eq!(w.get::<OutgoingEdges>(map[&out]).map(|e| e.len()), Some(1));
    let node = w.get::<ChildOf>(map[&inp]).unwrap().parent();
    assert_eq!(w.get::<Payload>(node), Some(&Payload(2.5)));
    // The left-out control leaves no trace in its node's children.
    let node = w.get::<ChildOf>(map[&out]).unwrap().parent();
    assert_eq!(w.get::<Children>(node).unwrap().to_vec(), [map[&out]]);
}

#[test]
fn snapshot_serializes_to_ron() {
    let mut app = app();
    let w = app.world_mut();
    let (canvas, _, [out, inp]) = graph(w);
    w.graph_edit(canvas, GraphEdit::Connect { from: out, to: inp })
        .unwrap();
    let saved = w.snapshot(canvas).unwrap();
    let ron = saved
        .serialize(&w.resource::<AppTypeRegistry>().read())
        .unwrap();
    assert!(
        ron.contains("Payload") && ron.contains("EdgeSource"),
        "{ron}"
    );
    // And back: ports keep their types (compared by hash; the name stays out).
    let loaded = {
        use serde::de::DeserializeSeed;
        let registry = w.resource::<AppTypeRegistry>().read();
        bevy::world_serialization::serde::WorldDeserializer {
            type_registry: &registry,
            load_from_path: &mut NoAssets,
        }
        .deserialize(&mut ron::Deserializer::from_str(&ron).unwrap())
        .unwrap()
    };
    let map = w.restore_snapshot(canvas, &loaded).unwrap();
    assert_eq!(w.get::<Port>(map[&out]).unwrap().port_type, NUM);
    let edit = GraphEdit::Connect {
        from: map[&out],
        to: map[&inp],
    };
    assert_eq!(
        w.graph_edit(canvas, edit),
        Err(bevy_noodle::RejectReason::AlreadyConnected)
    );
}

#[test]
fn copy_some_nodes_and_paste_them_anywhere() {
    let mut app = app();
    let w = app.world_mut();
    let (canvas, content, [out, inp]) = graph(w);
    // A third node also fed by `out`, left out of the copy.
    let third = w
        .spawn((GraphNode, NodePosition::default(), ChildOf(content)))
        .id();
    let other_in = w
        .spawn((Port::input(NUM), Node::default(), ChildOf(third)))
        .id();
    for to in [inp, other_in] {
        w.graph_edit(canvas, GraphEdit::Connect { from: out, to })
            .unwrap();
    }
    let node_of = |w: &World, port| w.get::<ChildOf>(port).unwrap().parent();
    let copied = w.snapshot_nodes(&[node_of(w, out), node_of(w, inp)]);
    assert_eq!(copied.entities.len(), 5, "two nodes, two ports, one edge");

    // Paste into the same graph: the copy is wired only to itself.
    let map = w.insert_snapshot(canvas, &copied).unwrap();
    let (new_out, new_in) = (map[&out], map[&inp]);
    assert_eq!(w.get::<OutgoingEdges>(out).unwrap().len(), 2);
    let new_edges: Vec<_> = w.get::<OutgoingEdges>(new_out).unwrap().iter().collect();
    assert_eq!(new_edges.len(), 1);
    assert_eq!(w.get::<EdgeTarget>(new_edges[0]).unwrap().0, new_in);
    assert_eq!(
        w.get::<ChildOf>(node_of(w, new_out)).unwrap().parent(),
        content
    );

    // And into another graph.
    let other = w.spawn((NodeCanvas, Node::default())).id();
    let other_content = content_of(w, other);
    let map = w.insert_snapshot(other, &copied).unwrap();
    let edge = w.get::<OutgoingEdges>(map[&out]).unwrap()[0];
    assert!(
        w.get::<ChildOf>(edge).is_none(),
        "edges stay out of the hierarchy"
    );
    assert_eq!(w.get::<Children>(other_content).unwrap().len(), 2);
}

#[test]
fn nested_canvases_restore_with_one_content_each() {
    let mut app = app();
    let w = app.world_mut();
    let (canvas, content, _) = graph(w);
    let group = w
        .spawn((GraphNode, NodePosition::default(), ChildOf(content)))
        .id();
    let inner = w.spawn((NodeCanvas, Node::default(), ChildOf(group))).id();
    let inner_content = content_of(w, inner);
    w.spawn((
        GraphNode,
        NodePosition::default(),
        Payload(7.0),
        ChildOf(inner),
    ));
    w.flush();
    assert_eq!(w.get::<Children>(inner_content).map(|c| c.len()), Some(1));
    let saved = w.snapshot(canvas).unwrap();
    let map = w.restore_snapshot(canvas, &saved).unwrap();
    app.update();

    let w = app.world_mut();
    let inner = map[&inner];
    let contents: Vec<_> = w
        .get::<Children>(inner)
        .unwrap()
        .iter()
        .filter(|c| w.get::<CanvasContent>(*c).is_some())
        .collect();
    assert_eq!(contents.len(), 1, "the snapshot's content, not a new one");
    let nodes = w.get::<Children>(contents[0]).unwrap();
    assert_eq!(w.get::<Payload>(nodes[0]), Some(&Payload(7.0)));
}

#[test]
fn failed_snapshot_writes_keep_the_graph_and_remove_partial_entities() {
    use bevy::ecs::entity::EntityHashSet;
    use bevy::reflect::structs::DynamicStruct;
    use bevy::world_serialization::WorldInstanceSpawnError;

    for restore in [false, true] {
        let mut app = app();
        let w = app.world_mut();
        let (canvas, content, [out, inp]) = graph(w);
        w.graph_edit(canvas, GraphEdit::Connect { from: out, to: inp })
            .unwrap();
        let node = w.get::<ChildOf>(out).unwrap().parent();
        let control = w.spawn((scene::Transient, ChildOf(node))).id();
        let inner = w.spawn((NodeCanvas, ChildOf(node))).id();
        content_of(w, inner);
        let mut saved = w.snapshot(canvas).unwrap();
        // Fail after valid entities (including a canvas with queued hooks)
        // have been written, rather than during RON deserialization.
        saved
            .entities
            .last_mut()
            .unwrap()
            .components
            .push(Box::new(DynamicStruct::default()));
        let before: EntityHashSet = w.query::<Entity>().iter(w).collect();
        let children = w.get::<Children>(content).unwrap().to_vec();

        let result = if restore {
            w.restore_snapshot(canvas, &saved)
        } else {
            w.insert_snapshot(canvas, &saved)
        };
        assert!(matches!(
            result,
            Err(WorldInstanceSpawnError::NoRepresentedType { .. })
        ));
        w.flush();
        let after: EntityHashSet = w.query::<Entity>().iter(w).collect();
        assert_eq!(
            after, before,
            "restore={restore}: no leaked or lost entities"
        );
        assert_eq!(w.get::<Children>(content).unwrap().to_vec(), children);
        assert!(w.get_entity(control).is_ok());
        assert_eq!(w.get::<OutgoingEdges>(out).unwrap().len(), 1);
    }
}

#[test]
fn restore_replaces_existing_nodes_edges_and_transient_children() {
    let mut app = app();
    let w = app.world_mut();
    let (canvas, content, [out, inp]) = graph(w);
    let GraphChange::Connected { edge, .. } = w
        .graph_edit(canvas, GraphEdit::Connect { from: out, to: inp })
        .unwrap()
    else {
        panic!("connected");
    };
    let node = w.get::<ChildOf>(out).unwrap().parent();
    let control = w.spawn((scene::Transient, ChildOf(node))).id();
    let saved = w.snapshot(canvas).unwrap();
    let map = w.restore_snapshot(canvas, &saved).unwrap();
    for old in [node, out, inp, edge, control] {
        assert!(w.get_entity(old).is_err());
    }
    assert_eq!(w.get::<Children>(content).unwrap().len(), 2);
    assert_eq!(w.get::<EdgeSource>(map[&edge]).unwrap().0, map[&out]);
    w.restore_snapshot(canvas, &default()).unwrap();
    assert!(w.get::<Children>(content).is_none_or(|c| c.is_empty()));
    assert_eq!(w.query::<&Edge>().iter(w).count(), 0);
}

#[test]
fn inserting_without_a_type_registry_returns_an_error_without_leaking() {
    let mut source = app();
    let (canvas, _, _) = graph(source.world_mut());
    let saved = source.world().snapshot(canvas).unwrap();
    let mut destination = World::new();
    let (canvas, content, _) = graph(&mut destination);
    let before = destination.query::<Entity>().iter(&destination).count();
    assert!(destination.insert_snapshot(canvas, &saved).is_err());
    assert_eq!(
        destination.query::<Entity>().iter(&destination).count(),
        before
    );
    assert_eq!(destination.get::<Children>(content).unwrap().len(), 2);
}
