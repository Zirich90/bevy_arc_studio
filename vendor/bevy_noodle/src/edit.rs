//! Changing the graph. Every change, from interaction or code, runs through
//! one pipeline: validation, with [`ConnectionCheck`] for connections
//! (failures trigger [`EditRejected`]), then [`EditRequested`] (observers may
//! rewrite or reject it), then the change, then [`EditApplied`].

use bevy::prelude::*;
use bevy::ui::Selected;

use crate::components::*;
use crate::query::{Connection, GraphQuery};

/// A change to a graph.
#[derive(Clone, Debug, PartialEq, Reflect)]
pub enum GraphEdit {
    /// Connect two ports (either order; normalized to output → input).
    Connect {
        /// One port.
        from: Entity,
        /// The other port.
        to: Entity,
    },
    /// Remove an edge.
    Disconnect {
        /// The edge.
        edge: Entity,
    },
    /// Move nodes by `delta` (graph units). Build one with
    /// [`GraphEdit::move_nodes`]; pointer drags add their [`DragProgress`].
    MoveNodes {
        /// The nodes (others are ignored).
        nodes: Vec<Entity>,
        /// The movement.
        delta: Vec2,
        /// Set while a drag streams its steps; `None` for a complete move.
        drag: Option<DragProgress>,
    },
    /// Despawn nodes with their ports and edges, and disconnect edges: a
    /// whole selection can be deleted at once.
    Delete {
        /// Nodes and edges (others are ignored).
        items: Vec<Entity>,
    },
}

impl GraphEdit {
    /// A complete move of `nodes` by `delta`.
    pub fn move_nodes(nodes: Vec<Entity>, delta: Vec2) -> Self {
        Self::MoveNodes {
            nodes,
            delta,
            drag: None,
        }
    }
}

/// What an applied edit changed: reported by [`EditApplied`] and returned by
/// [`GraphWorldExt::graph_edit`].
#[derive(Clone, Debug, PartialEq, Reflect)]
pub enum GraphChange {
    /// A new edge connects `ports`.
    Connected {
        /// The new edge.
        edge: Entity,
        /// Its ports.
        ports: PortPair,
    },
    /// An edge is gone. Its `ports` are kept, so the change can be replayed
    /// or inverted.
    Disconnected {
        /// The removed edge.
        edge: Entity,
        /// The ports it connected.
        ports: PortPair,
    },
    /// Nodes moved by `delta` (graph units).
    Moved {
        /// The nodes.
        nodes: Vec<Entity>,
        /// The movement.
        delta: Vec2,
        /// Set while a drag streams its steps; `None` for a complete move.
        drag: Option<DragProgress>,
    },
    /// Nodes (with their ports) and edges were despawned, after their edges
    /// were reported [`Disconnected`](Self::Disconnected).
    Deleted {
        /// The nodes and edges.
        items: Vec<Entity>,
    },
}

impl GraphChange {
    /// Whether this is a step of a drag that has not ended: an undo stack
    /// records the drag's last step instead.
    pub fn is_drag_step(&self) -> bool {
        matches!(self, Self::Moved { drag: Some(drag), .. } if !drag.is_final)
    }
}

/// How far a drag that streams [`GraphEdit::MoveNodes`] steps has gone.
#[derive(Clone, Copy, Debug, Default, PartialEq, Reflect)]
pub struct DragProgress {
    /// The whole drag's movement so far.
    pub total: Vec2,
    /// Whether this step ends the drag.
    pub is_final: bool,
}

/// How [`GraphCommandsExt::select`] combines its items with the current selection.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub enum SelectMode {
    /// Select exactly these.
    #[default]
    Replace,
    /// Add these to the selection.
    Add,
    /// Remove these from the selection.
    Remove,
    /// Flip each of these.
    Toggle,
}

/// Where an edit came from, so undo stacks and netcode can skip echoes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Reflect)]
pub enum EditOrigin {
    /// Called from code with [`GraphCommandsExt::graph_edit`] or [`GraphWorldExt::graph_edit`].
    #[default]
    Code,
    /// Made by pointer interaction ([`CanvasInteraction`](crate::CanvasInteraction)).
    Interaction,
    /// Yours, e.g. for edits replayed from the network.
    Custom(u64),
}

/// Triggered on the canvas whenever a connection is considered: before a
/// [`GraphEdit::Connect`] applies, and to preview one (the ports a dragged
/// wire may snap to, [`GraphWorldExt::preview_connection`]). `refused` holds
/// the built-in rules' verdict (e.g. [`RejectReason::IncompatibleTypes`]),
/// which observers may [`allow`](Self::allow) or [`reject`](Self::reject).
///
/// Put connection rules here, so previews and edits agree. It can run for
/// every port of a graph at once: decide from the graph, and change nothing
/// (do that in [`EditRequested`]). Connections that cannot exist at all
/// (missing ports, ports of one node, two inputs) never get here.
#[derive(EntityEvent, Clone, Debug)]
pub struct ConnectionCheck {
    /// The canvas.
    #[event_target]
    pub canvas: Entity,
    /// The ports.
    pub ports: PortPair,
    /// Why the connection is refused, if it is.
    pub refused: Option<RejectReason>,
}

impl ConnectionCheck {
    /// Refuse the connection: it reports [`RejectReason::Rejected`].
    pub fn reject(&mut self) {
        self.refused = Some(RejectReason::Rejected);
    }

    /// Allow the connection even if the built-in rules refuse it.
    pub fn allow(&mut self) {
        self.refused = None;
    }
}

/// Triggered on the canvas before an edit applies, once it passed the
/// built-in rules and (for a connection) [`ConnectionCheck`]. Observers may
/// change `edit` (a changed connection is checked again) or
/// [`reject`](Self::reject) it, e.g. to do something else instead: side
/// effects belong here. Never triggered for previews.
#[derive(EntityEvent, Clone, Debug)]
pub struct EditRequested {
    /// The canvas edited.
    #[event_target]
    pub canvas: Entity,
    /// The edit, which observers may change.
    pub edit: GraphEdit,
    /// Where it came from.
    pub origin: EditOrigin,
    /// Whether an observer rejected it.
    pub rejected: bool,
}

impl EditRequested {
    /// Refuse the edit: it reports [`RejectReason::Rejected`].
    pub fn reject(&mut self) {
        self.rejected = true;
    }
}

/// Triggered on the canvas, and written as a message, after an edit applied.
#[derive(EntityEvent, Message, Clone, Debug)]
pub struct EditApplied {
    /// The canvas edited.
    #[event_target]
    pub canvas: Entity,
    /// What changed.
    pub change: GraphChange,
    /// Where it came from.
    pub origin: EditOrigin,
}

/// Triggered on the canvas, and written as a message, when an edit was refused.
#[derive(EntityEvent, Message, Clone, Debug)]
pub struct EditRejected {
    /// The canvas.
    #[event_target]
    pub canvas: Entity,
    /// The edit as requested.
    pub edit: GraphEdit,
    /// Why it was refused.
    pub reason: RejectReason,
}

/// Why an edit was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Reflect)]
pub enum RejectReason {
    /// The canvas, port or edge does not exist (or is not one).
    InvalidEntity,
    /// An entity belongs to another canvas.
    NotInCanvas,
    /// A port is not inside a [`GraphNode`].
    NotInNode,
    /// Both ports are on the same node.
    SameNode,
    /// Both ports are inputs, or both outputs.
    SameDirection,
    /// The port types do not accept each other.
    IncompatibleTypes,
    /// The ports are connected already.
    AlreadyConnected,
    /// The port is full and its [`Capacity`](crate::Capacity) refuses more.
    PortFull,
    /// Nothing to do (e.g. no nodes of this canvas listed).
    Empty,
    /// A [`ConnectionCheck`] or [`EditRequested`] observer rejected it.
    Rejected,
}

/// What applying an edit returns: what it changed, or why it was refused.
pub type EditResult = Result<GraphChange, RejectReason>;

/// Queue graph edits from [`Commands`].
pub trait GraphCommandsExt {
    /// Queue `edit` with an [`EditOrigin`].
    fn graph_edit_with_origin(&mut self, canvas: Entity, edit: GraphEdit, origin: EditOrigin);

    /// Queue `edit`, with [`EditOrigin::Code`].
    fn graph_edit(&mut self, canvas: Entity, edit: GraphEdit) {
        self.graph_edit_with_origin(canvas, edit, EditOrigin::Code);
    }

    /// Queue a selection change (see [`GraphWorldExt::select`]).
    fn select(&mut self, canvas: Entity, items: Vec<Entity>, mode: SelectMode);
}

impl GraphCommandsExt for Commands<'_, '_> {
    fn graph_edit_with_origin(&mut self, canvas: Entity, edit: GraphEdit, origin: EditOrigin) {
        self.queue(move |world: &mut World| _ = world.graph_edit_with_origin(canvas, edit, origin));
    }

    fn select(&mut self, canvas: Entity, items: Vec<Entity>, mode: SelectMode) {
        self.queue(move |world: &mut World| world.select(canvas, items, mode));
    }
}

/// Apply graph edits immediately.
pub trait GraphWorldExt {
    /// Apply `edit` now, with an [`EditOrigin`].
    fn graph_edit_with_origin(
        &mut self,
        canvas: Entity,
        edit: GraphEdit,
        origin: EditOrigin,
    ) -> EditResult;

    /// Apply `edit` now, with [`EditOrigin::Code`].
    fn graph_edit(&mut self, canvas: Entity, edit: GraphEdit) -> EditResult {
        self.graph_edit_with_origin(canvas, edit, EditOrigin::Code)
    }

    /// Whether ports `a` and `b` (either order) of `canvas` may connect, by
    /// the built-in rules and [`ConnectionCheck`] observers, without
    /// changing anything.
    fn preview_connection(
        &mut self,
        canvas: Entity,
        a: Entity,
        b: Entity,
    ) -> Result<Connection, RejectReason>;

    /// Changes which nodes and edges of `canvas` carry [`Selected`], combining
    /// `items` (others are ignored) with the selection by `mode`. Selection
    /// is not an edit: react to it with `On<Add, Selected>` and
    /// `On<Remove, Selected>` observers, or `Has<Selected>`.
    fn select(&mut self, canvas: Entity, items: Vec<Entity>, mode: SelectMode);
}

impl GraphWorldExt for World {
    fn graph_edit_with_origin(
        &mut self,
        canvas: Entity,
        edit: GraphEdit,
        origin: EditOrigin,
    ) -> EditResult {
        let result = run(self, canvas, edit.clone(), origin);
        if let (Err(reason), Ok(_)) = (&result, self.get_entity(canvas)) {
            let event = EditRejected {
                canvas,
                edit,
                reason: *reason,
            };
            self.trigger(event.clone());
            self.write_message(event);
        }
        result
    }

    fn preview_connection(
        &mut self,
        canvas: Entity,
        a: Entity,
        b: Entity,
    ) -> Result<Connection, RejectReason> {
        let check = |In((canvas, a, b)), graph: GraphQuery| graph.check_connection(canvas, a, b);
        let connection = self
            .run_system_cached_with(check, (canvas, a, b))
            .map_err(|_| RejectReason::InvalidEntity)??;
        match ask(self, canvas, &connection) {
            Some(reason) => Err(reason),
            None => Ok(Connection {
                refused: None,
                ..connection
            }),
        }
    }

    fn select(&mut self, canvas: Entity, items: Vec<Entity>, mode: SelectMode) {
        let changes = self.run_system_cached_with(plan_selection, (canvas, items, mode));
        for (item, on) in changes.unwrap_or_default() {
            let Ok(mut item) = self.get_entity_mut(item) else {
                continue;
            };
            match on {
                true => _ = item.insert(Selected),
                false => _ = item.remove::<Selected>(),
            }
        }
    }
}

/// The selection changes `items` make with `mode`: `(item, selected)`.
fn plan_selection(
    In((canvas, mut items, mode)): In<(Entity, Vec<Entity>, SelectMode)>,
    graph: GraphQuery,
) -> Vec<(Entity, bool)> {
    let item = |e: &Entity| {
        let node_or_edge = graph.node_of(*e) == Some(*e) || graph.edge_ports(*e).is_some();
        node_or_edge && graph.canvas_of(*e) == Some(canvas)
    };
    items.retain(item);
    // Only what is selected now or listed can change.
    let mut candidates: Vec<Entity> = graph.selected_in(canvas).collect();
    candidates.extend(items.iter().copied());
    candidates.sort();
    candidates.dedup();
    let mut changes = Vec::new();
    for item in candidates {
        let (listed, on) = (items.contains(&item), graph.is_selected(item));
        let want = match mode {
            SelectMode::Replace => listed,
            SelectMode::Add => on || listed,
            SelectMode::Remove => on && !listed,
            SelectMode::Toggle => on != listed,
        };
        if want != on {
            changes.push((item, want));
        }
    }
    changes
}

/// The verdict on a connection, after [`ConnectionCheck`] observers.
pub(crate) fn ask(
    world: &mut World,
    canvas: Entity,
    connection: &Connection,
) -> Option<RejectReason> {
    let mut check = ConnectionCheck {
        canvas,
        ports: connection.ports,
        refused: connection.refused,
    };
    world.trigger_ref(&mut check);
    check.refused
}

/// A validated edit: the edges it removes first (each reported as a
/// disconnect), then what it does.
struct Plan(Vec<(Entity, PortPair)>, Step);

enum Step {
    Connect(Connection),
    Disconnect(Entity, PortPair),
    Move(Vec<Entity>, Vec2, Option<DragProgress>),
    Delete(Vec<Entity>),
}

fn run(world: &mut World, canvas: Entity, edit: GraphEdit, origin: EditOrigin) -> EditResult {
    // The plan, and the edit normalized.
    let plan = |world: &mut World, edit| {
        let mut input = (canvas, edit);
        let plan = world
            .run_system_cached_with(plan_edit, &mut input)
            .map_err(|_| RejectReason::InvalidEntity)??;
        Ok((plan, input.1))
    };
    // A plan whose connection observers allow.
    let checked = |world: &mut World, edit| match plan(world, edit)? {
        (Plan(_, Step::Connect(connection)), _)
            if let Some(reason) = ask(world, canvas, &connection) =>
        {
            Err(reason)
        }
        planned => Ok(planned),
    };
    // Checked before `EditRequested`, so observers see the normalized,
    // allowed edit.
    let (_, edit) = checked(world, edit)?;
    let mut request = EditRequested {
        canvas,
        edit: edit.clone(),
        origin,
        rejected: false,
    };
    world.trigger_ref(&mut request);
    if request.rejected {
        return Err(RejectReason::Rejected);
    }
    // Planned again, as observers may have changed the world; a changed edit
    // is checked again, an unchanged one keeps its verdict.
    let (Plan(removed, step), _) = if request.edit == edit {
        plan(world, edit)?
    } else {
        checked(world, request.edit)?
    };
    let applied = |world: &mut World, change: GraphChange| {
        let event = EditApplied {
            canvas,
            change,
            origin,
        };
        world.trigger(event.clone());
        world.write_message(event.clone());
        event.change
    };
    for (edge, ports) in removed {
        world.despawn(edge);
        applied(world, GraphChange::Disconnected { edge, ports });
    }
    let change = match step {
        Step::Connect(connection) => {
            let ports = connection.ports;
            let edge = world.spawn((Edge, EdgeSource(ports.output), EdgeTarget(ports.input)));
            GraphChange::Connected {
                edge: edge.id(),
                ports,
            }
        }
        Step::Disconnect(edge, ports) => {
            world.despawn(edge);
            GraphChange::Disconnected { edge, ports }
        }
        Step::Move(nodes, delta, drag) => {
            for node in &nodes {
                if let Some(mut position) = world.get_mut::<NodePosition>(*node) {
                    position.0 += delta;
                }
            }
            GraphChange::Moved { nodes, delta, drag }
        }
        Step::Delete(items) => {
            // Listed edges are already gone.
            items.iter().for_each(|e| _ = world.try_despawn(*e));
            GraphChange::Deleted { items }
        }
    };
    Ok(applied(world, change))
}

fn plan_edit(
    InMut((canvas, edit)): InMut<(Entity, GraphEdit)>,
    graph: GraphQuery,
) -> Result<Plan, RejectReason> {
    let canvas = *canvas;
    if graph.canvas_of(canvas) != Some(canvas) {
        return Err(RejectReason::InvalidEntity);
    }
    // Each check looks only at the entities listed, so edits cost the same in
    // a graph of any size.
    let here = |e: &Entity| graph.canvas_of(*e) == Some(canvas);
    let mine = |e: &Entity| graph.node_of(*e) == Some(*e) && here(e);
    let edge = |e: &Entity| graph.edge_ports(*e).is_some() && here(e);
    let listed = |items: &Vec<Entity>| match items.is_empty() {
        true => Err(RejectReason::Empty),
        false => Ok(items.clone()),
    };
    let mut removed = Vec::new();
    let step = match edit {
        GraphEdit::Connect { from, to } => {
            let connection = graph.check_connection(canvas, *from, *to)?;
            (*from, *to) = (connection.ports.output, connection.ports.input);
            removed = connection.replaces.clone();
            Step::Connect(connection)
        }
        GraphEdit::Disconnect { edge } => match graph.edge_ports(*edge) {
            None => return Err(RejectReason::InvalidEntity),
            Some(_) if !here(edge) => return Err(RejectReason::NotInCanvas),
            Some(ports) => Step::Disconnect(*edge, ports),
        },
        GraphEdit::MoveNodes { nodes, delta, drag } => {
            nodes.retain(mine);
            Step::Move(listed(nodes)?, *delta, *drag)
        }
        GraphEdit::Delete { items } => {
            // Nodes go with their edges (and those of nodes inside them);
            // listed edges go too.
            items.retain(|e| mine(e) || edge(e));
            items.sort();
            items.dedup();
            removed = items
                .iter()
                .flat_map(|n| graph.ports_within(*n))
                .flat_map(|p| graph.edges_of(p))
                .collect();
            removed.extend(items.iter().filter(|e| edge(e)));
            removed.sort();
            removed.dedup();
            Step::Delete(listed(items)?)
        }
    };
    let removed = removed
        .into_iter()
        .filter_map(|e| Some((e, graph.edge_ports(e)?)));
    Ok(Plan(removed.collect(), step))
}
