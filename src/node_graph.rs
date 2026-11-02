//! Native Noodle canvas over a stable, reflected StudioGraph document.
use crate::{
    arc_core::{StudioGraph, StudioGraphEdge},
    editor_registry::{EditorAppExt, PanelLocation},
    operators,
};
use bevy::prelude::*;
use bevy_egui::egui;
use bevy_noodle::{prelude::*, style::kit};

const REPLAY: EditOrigin = EditOrigin::Custom(0x415243);
#[derive(Component, Clone, Default)]
struct StableNode(u32);
#[derive(Resource, Default)]
struct GraphView {
    root: Option<Entity>,
    rendered: StudioGraph,
}

pub fn replace(w: &mut World, graph: StudioGraph) -> Result<String, String> {
    graph.validate()?;
    let before = operators::snap_resource::<StudioGraph>(w);
    w.insert_resource(graph);
    let after = operators::snap_resource::<StudioGraph>(w);
    operators::record_edit(w, "Edit node graph", before, after, false);
    Ok("graph updated".into())
}

fn node_id(w: &World, mut e: Entity) -> Option<u32> {
    loop {
        if let Some(n) = w.get::<StableNode>(e) {
            return Some(n.0);
        }
        e = w.get::<ChildOf>(e)?.parent();
    }
}

fn read_canvas(w: &mut World) -> StudioGraph {
    let nodes = w
        .query::<(&StableNode, &NodePosition)>()
        .iter(w)
        .map(|(id, p)| (id.0, p.0.to_array()))
        .collect::<Vec<_>>();
    let old = w.resource::<StudioGraph>();
    let mut result = old.clone();
    result.nodes.retain_mut(|n| {
        if let Some((_, p)) = nodes.iter().find(|(id, _)| *id == n.id) {
            n.position = *p;
            true
        } else {
            false
        }
    });
    let edges = w
        .query::<(&EdgeSource, &EdgeTarget)>()
        .iter(w)
        .map(|(s, t)| (s.0, t.0))
        .collect::<Vec<_>>();
    result.edges = edges
        .into_iter()
        .filter_map(|(a, b)| {
            Some(StudioGraphEdge {
                from: node_id(w, a)?,
                to: node_id(w, b)?,
            })
        })
        .collect();
    result.edges.sort_by_key(|e| (e.from, e.to));
    result
}

fn applied(event: On<EditApplied>, mut commands: Commands) {
    if event.origin == REPLAY || event.change.is_drag_step() {
        return;
    }
    commands.queue(|w: &mut World| {
        let graph = read_canvas(w);
        if graph.validate().is_ok() {
            let _ = replace(w, graph.clone());
            w.resource_mut::<GraphView>().rendered = graph;
        }
    });
}

fn connection(
    mut event: On<ConnectionCheck>,
    graph: Res<StudioGraph>,
    topology: GraphQuery,
    ids: Query<&StableNode>,
) {
    let (Some(a), Some(b)) = (
        topology.node_of(event.ports.output),
        topology.node_of(event.ports.input),
    ) else {
        event.reject();
        return;
    };
    let (Ok(a), Ok(b)) = (ids.get(a), ids.get(b)) else {
        event.reject();
        return;
    };
    let mut next = graph.clone();
    next.edges.retain(|e| e.to != b.0);
    next.edges.push(StudioGraphEdge { from: a.0, to: b.0 });
    if next.validate().is_err() {
        event.reject();
    }
}

fn delete_selection(
    keys: Res<ButtonInput<KeyCode>>,
    studio: Res<crate::studio::StudioState>,
    view: Res<GraphView>,
    graph: GraphQuery,
    mut contexts: bevy_egui::EguiContexts,
    mut commands: Commands,
) {
    if studio.tool != "node_graph"
        || !keys.just_pressed(KeyCode::Delete)
        || contexts
            .ctx_mut()
            .is_ok_and(|c| c.egui_wants_keyboard_input())
    {
        return;
    }
    if let Some(root) = view.root {
        let items: Vec<_> = graph.selected_in(root).collect();
        if !items.is_empty() {
            commands.graph_edit(root, GraphEdit::Delete { items });
        }
    }
}

fn sync_canvas(w: &mut World) {
    let open = w.resource::<crate::studio::StudioState>().tool == "node_graph";
    let doc = w.resource::<StudioGraph>().clone();
    if doc.validate().is_err() {
        return;
    }
    let mut view = w.remove_resource::<GraphView>().unwrap_or_default();
    if !open || doc != view.rendered {
        if let Some(e) = view.root.take() {
            let _ = w.despawn(e);
        }
    }
    if open && view.root.is_none() {
        let root = w.commands().spawn_scene(bsn! {
            @kit::canvas()
            Node { position_type: PositionType::Absolute, left: px(285), top: px(115), width: percent(57), height: percent(68) }
            GlobalZIndex(30)
            BackgroundColor(Color::srgb(0.055, 0.065, 0.08))
        }).id();
        w.flush();
        w.entity_mut(root).observe(applied).observe(connection);
        for n in &doc.nodes {
            let title = format!("{}: {:?}", n.id, n.kind);
            let ty = PortType::named(if n.kind.event_pipeline() {
                "world_effect"
            } else {
                "world_state"
            });
            let id = n.id;
            let node = w
                .commands()
                .spawn_scene(bsn! {
                    @kit::node(Vec2::from_array(n.position)) ChildOf(root) StableNode(id)
                    Children [ @kit::title(title) ]
                })
                .id();
            if !n.kind.source() {
                w.commands().spawn_scene(bsn! { @kit::row("in", Port::input(ty), Color::srgb(0.3,0.8,0.7)) ChildOf(node) });
            }
            w.commands().spawn_scene(
                bsn! { @kit::row("out", Port::output(ty), Color::srgb(0.3,0.8,0.7)) ChildOf(node) },
            );
        }
        w.flush();
        let ports: Vec<_> = w
            .query::<(Entity, &Port)>()
            .iter(w)
            .map(|(e, p)| (e, p.direction))
            .collect();
        for e in &doc.edges {
            let find = |id, direction| {
                ports
                    .iter()
                    .find(|(p, d)| *d == direction && node_id(w, *p) == Some(id))
                    .map(|(p, _)| *p)
            };
            if let (Some(from), Some(to)) = (
                find(e.from, PortDirection::Output),
                find(e.to, PortDirection::Input),
            ) {
                let _ = w.graph_edit_with_origin(root, GraphEdit::Connect { from, to }, REPLAY);
            }
        }
        view.root = Some(root);
        view.rendered = doc;
    }
    w.insert_resource(view);
}

fn panel(ui: &mut egui::Ui, d: &mut crate::ui::UiData, _: &crate::ui_inspector::Filter) {
    use crate::mcp_commands::StudioCommand as Cmd;
    let open = d.studio.tool == "node_graph";
    if ui
        .button(if open {
            "Close native canvas"
        } else {
            "Open native canvas"
        })
        .clicked()
    {
        d.studio.tool = if open { "select" } else { "node_graph" }.into();
    }
    for (label, events) in [
        ("World / multiphysics graph", false),
        ("PF2e / environment effects graph", true),
    ] {
        if ui.button(label).clicked() {
            d.queue.push(Cmd::ReplaceStudioGraph {
                ron: ron::to_string(&StudioGraph::template(events)).unwrap(),
            });
        }
    }
    ui.weak("Drag nodes and ports. Ctrl+Z / Ctrl+Y use Studio history. Domain evaluators are separate runtime adapters.");
}

pub struct NodeGraphPlugin;
impl Plugin for NodeGraphPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((NoodlePlugins, NoodleDefaultStylePlugin))
            .insert_resource(StudioGraph::template(false))
            .init_resource::<GraphView>()
            .register_type::<StudioGraph>()
            .register_panel(
                "node_graph",
                "Node Graph",
                PanelLocation::RightInspector,
                panel,
            )
            .add_systems(PreUpdate, sync_canvas)
            .add_systems(Update, delete_selection);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn noodle_drag_records_only_final_step() {
        let mut app = App::new();
        let mut graph = StudioGraph::template(false);
        graph.nodes.truncate(2);
        graph.edges.clear();
        app.add_plugins((MinimalPlugins, NoodleCorePlugin))
            .register_type::<StudioGraph>()
            .insert_resource(graph.clone())
            .init_resource::<GraphView>()
            .init_resource::<operators::ComponentHistory>();
        let w = app.world_mut();
        let canvas = w.spawn((NodeCanvas, Node::default())).observe(applied).id();
        let ids: Vec<_> = graph
            .nodes
            .iter()
            .map(|n| {
                w.spawn((
                    GraphNode,
                    NodePosition(Vec2::from_array(n.position)),
                    StableNode(n.id),
                    ChildOf(canvas),
                ))
                .id()
            })
            .collect();
        for (delta, total, is_final) in [(4.0, 4.0, false), (1.0, 5.0, true)] {
            w.graph_edit(
                canvas,
                GraphEdit::MoveNodes {
                    nodes: vec![ids[0]],
                    delta: Vec2::new(delta, 0.0),
                    drag: Some(DragProgress {
                        total: Vec2::new(total, 0.0),
                        is_final,
                    }),
                },
            )
            .unwrap();
            w.flush();
            assert_eq!(
                w.resource::<operators::ComponentHistory>().undo_len(),
                usize::from(is_final)
            );
        }
        assert_eq!(
            w.resource::<StudioGraph>().nodes[0].position[0],
            graph.nodes[0].position[0] + 5.0
        );
        let undo = w
            .resource_mut::<operators::ComponentHistory>()
            .undo()
            .unwrap();
        operators::apply_snap(w, &undo);
        assert_eq!(*w.resource::<StudioGraph>(), graph);
    }
    #[test]
    fn graph_replace_is_atomic_and_replays_without_new_history() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .register_type::<StudioGraph>()
            .insert_resource(StudioGraph::template(false))
            .init_resource::<operators::ComponentHistory>();
        let w = app.world_mut();
        let initial = w.resource::<StudioGraph>().clone();
        let next = StudioGraph::template(true);
        replace(w, next.clone()).unwrap();
        assert_eq!(w.resource::<operators::ComponentHistory>().undo_len(), 1);
        let mut invalid = next.clone();
        invalid.edges.push(invalid.edges[0]);
        assert!(replace(w, invalid).is_err());
        assert_eq!(*w.resource::<StudioGraph>(), next);
        let undo = w
            .resource_mut::<operators::ComponentHistory>()
            .undo()
            .unwrap();
        operators::apply_snap(w, &undo);
        assert_eq!(*w.resource::<StudioGraph>(), initial);
        let redo = w
            .resource_mut::<operators::ComponentHistory>()
            .redo()
            .unwrap();
        operators::apply_snap(w, &redo);
        assert_eq!(*w.resource::<StudioGraph>(), next);
        assert_eq!(w.resource::<operators::ComponentHistory>().undo_len(), 1);
    }
}
