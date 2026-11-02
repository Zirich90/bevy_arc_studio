//! Keeping layout and graph geometry in sync. Before layout: node positions,
//! the canvas view and edge geometry (from node positions plus measured port
//! offsets, so edges follow dragged nodes without lag). After layout: ports
//! are measured.

use bevy::prelude::*;
use bevy::ui::{ComputedNode, ui_transform::UiGlobalTransform};

use crate::components::*;
use crate::edit::{GraphCommandsExt, GraphEdit};
use crate::interaction::WireTarget;
use crate::query::GraphQuery;

pub(crate) fn sync_layout(
    mut nodes: Query<(&NodePosition, &mut Node), Changed<NodePosition>>,
    canvases: Query<(&CanvasView, &Children)>,
    mut contents: Query<&mut UiTransform, With<CanvasContent>>,
) {
    for (position, mut node) in &mut nodes {
        node.position_type = PositionType::Absolute;
        node.left = Val::Px(position.x);
        node.top = Val::Px(position.y);
    }
    for (view, children) in &canvases {
        let mut contents = contents.iter_many_mut(children).matched();
        while let Some(mut transform) = contents.fetch_next() {
            let wanted = UiTransform {
                translation: Val2::px(view.pan.x, view.pan.y),
                scale: Vec2::splat(view.zoom),
                ..default()
            };
            transform.set_if_neq(wanted);
        }
    }
}

/// A port's position (graph space) and wire tangent, once laid out.
pub(crate) fn endpoint(
    port: Entity,
    ports: &Query<(&Port, &PortAnchor, Option<&PortTangent>)>,
    nodes: &Query<&NodePosition>,
) -> Option<(Vec2, Vec2)> {
    let (port, anchor, tangent) = ports.get(port).ok()?;
    let positioned = anchor.position?;
    let at = anchor
        .node
        .and_then(|n| nodes.get(n).ok())
        .map_or(positioned, |p| p.0 + anchor.offset);
    Some((at, port.tangent(tangent)))
}

pub(crate) fn update_edge_geometry(
    changed: Query<(), Or<(Changed<NodePosition>, Changed<PortAnchor>, Added<Edge>)>>,
    edges: Query<(Entity, &EdgeSource, &EdgeTarget, Option<&EdgeGeometry>)>,
    wires: Query<(Entity, &PendingWire, Option<&EdgeGeometry>)>,
    targets: Query<&WireTarget>,
    ports: Query<(&Port, &PortAnchor, Option<&PortTangent>)>,
    nodes: Query<&NodePosition>,
    mut commands: Commands,
) {
    let end = |port| endpoint(port, &ports, &nodes);
    // A wire ends with its port: its drag end goes to an entity that is gone.
    for (wire, ..) in wires.iter().filter(|(_, w, _)| !ports.contains(w.from)) {
        commands.entity(wire).try_despawn();
    }
    // Inserted while both ends are laid out, removed otherwise.
    let mut set = |entity, current: Option<&EdgeGeometry>, wanted: Option<EdgeGeometry>| match (
        current, wanted,
    ) {
        (Some(current), Some(wanted)) if *current == wanted => {}
        (_, Some(wanted)) => _ = commands.entity(entity).try_insert(wanted),
        (Some(_), None) => _ = commands.entity(entity).try_remove::<EdgeGeometry>(),
        (None, None) => {}
    };
    for (edge, source, target, current) in edges.iter().filter(|_| !changed.is_empty()) {
        let ends = end(source.0).zip(end(target.0));
        let geometry = ends
            .map(|(a, b)| EdgeGeometry::between(a, b).with_ports(Some(source.0), Some(target.0)));
        set(edge, current, geometry);
    }
    // The dragged wire runs output → input; the pointer stands in for the free end.
    for (entity, wire, current) in &wires {
        let target = targets.get(entity).ok().and_then(|t| t.0);
        let geometry = end(wire.from).map(|fixed| {
            let free = target.and_then(end).unwrap_or((wire.pointer, -fixed.1));
            let from_output = ports
                .get(wire.from)
                .is_ok_and(|(p, ..)| p.direction == PortDirection::Output);
            let (from, to) = (Some(wire.from), target);
            if from_output {
                EdgeGeometry::between(fixed, free).with_ports(from, to)
            } else {
                EdgeGeometry::between(free, fixed).with_ports(to, from)
            }
        });
        set(entity, current, geometry);
    }
}

/// After layout: each port's center in graph space, and relative to its node.
pub(crate) fn measure_ports(
    mut ports: Query<
        (Entity, &mut PortAnchor, &UiGlobalTransform, &ComputedNode),
        (With<Port>, Changed<UiGlobalTransform>),
    >,
    parents: Query<&ChildOf>,
    graph_nodes: Query<Option<&NodePosition>, With<GraphNode>>,
    contents: Query<&UiGlobalTransform, With<CanvasContent>>,
) {
    for (entity, mut anchor, transform, computed) in &mut ports {
        let node = parents
            .iter_ancestors(entity)
            .find(|e| graph_nodes.contains(*e));
        let content = parents
            .iter_ancestors(entity)
            .find_map(|e| contents.get(e).ok());
        // The center in content space, which is graph space once unscaled.
        let position = content
            .and_then(|content| content.try_inverse())
            .filter(|_| computed.size() != Vec2::ZERO)
            .map(|inverse| {
                inverse.transform_point2(transform.translation) * computed.inverse_scale_factor()
            });
        let origin = node
            .and_then(|n| graph_nodes.get(n).ok().flatten())
            .map_or(Vec2::ZERO, |p| p.0);
        let offset = position.unwrap_or_default() - origin;
        anchor.set_if_neq(PortAnchor {
            node,
            offset,
            position,
        });
    }
}

/// After re-parenting (or unparenting), edges whose ends ended up in
/// different graphs are disconnected. Checked once per frame, so moving both ends one after the
/// other keeps the edge (it belongs to whichever graph its ports are in).
pub(crate) fn drop_split_edges(
    moved: Query<Entity, Changed<ChildOf>>,
    mut unparented: RemovedComponents<ChildOf>,
    graph: GraphQuery,
    children: Query<&Children>,
    mut commands: Commands,
) {
    let subtrees = moved
        .iter()
        .chain(unparented.read())
        .flat_map(|e| std::iter::once(e).chain(children.iter_descendants(e)));
    let mut edges: Vec<_> = subtrees.flat_map(|e| graph.edges_of(e)).collect();
    edges.sort();
    edges.dedup();
    for edge in edges {
        let Some(ends) = graph.edge_ports(edge) else {
            continue;
        };
        match (graph.canvas_of(ends.output), graph.canvas_of(ends.input)) {
            (Some(canvas), other) if other != Some(canvas) => {
                commands.graph_edit(canvas, GraphEdit::Disconnect { edge });
            }
            (None, _) => commands.entity(edge).try_despawn(),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::{GraphChange, GraphWorldExt};

    #[test]
    fn edges_have_geometry_only_while_laid_out() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, crate::NoodleCorePlugin));
        let w = app.world_mut();
        let canvas = w.spawn((NodeCanvas, Node::default())).id();
        let num = PortType::named("num");
        let [out, inp] = [Port::output(num), Port::input(num)].map(|port| {
            let node = (GraphNode, NodePosition::default(), ChildOf(canvas));
            let node = w.spawn(node).id();
            w.spawn((port, ChildOf(node))).id()
        });
        w.flush();
        let edit = GraphEdit::Connect { from: out, to: inp };
        let Ok(GraphChange::Connected { edge, .. }) = w.graph_edit(canvas, edit) else {
            panic!("connected");
        };
        app.update();
        assert!(app.world().get::<EdgeGeometry>(edge).is_none());

        let measure = |app: &mut App, x: Option<f32>| {
            for (port, offset) in [(out, 0.0), (inp, 100.0)] {
                let mut anchor = app.world_mut().get_mut::<PortAnchor>(port).unwrap();
                anchor.offset = Vec2::new(offset, 0.0);
                anchor.position = x.map(|x| Vec2::new(x + offset, 0.0));
            }
            app.update();
        };
        measure(&mut app, Some(0.0));
        let geometry = *app.world().get::<EdgeGeometry>(edge).unwrap();
        assert_eq!((geometry.start.x, geometry.end.x), (0.0, 100.0));
        assert_eq!((geometry.output, geometry.input), (Some(out), Some(inp)));
        let mut state = bevy::ecs::system::SystemState::<GraphQuery>::new(app.world_mut());
        let graph = state.get(app.world()).unwrap();
        assert_eq!(graph.port_position(inp), Some(Vec2::new(100.0, 0.0)));
        measure(&mut app, None);
        assert!(app.world().get::<EdgeGeometry>(edge).is_none());
    }
}
