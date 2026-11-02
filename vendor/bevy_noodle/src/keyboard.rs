//! Keyboard use, opt-in per canvas with [`CanvasKeyboard`].
//!
//! Built on `bevy_input_focus`: the canvas is a [`TabGroup`], its nodes and
//! ports get a [`TabIndex`] so Tab and Shift+Tab move focus between them, and
//! keys reach the focused entity as [`FocusedInput`] events. No key does
//! anything on a canvas without [`CanvasKeyboard`].

use bevy::input::ButtonState;
use bevy::input::keyboard::KeyboardInput;
use bevy::input_focus::tab_navigation::{TabGroup, TabIndex, TabNavigationPlugin};
use bevy::input_focus::{FocusGained, FocusedInput};
use bevy::prelude::*;
use bevy::ui::ComputedNode;

use crate::components::*;
use crate::edit::{EditOrigin, GraphCommandsExt, GraphEdit, SelectMode};
use crate::interaction::{WireCandidates, WireTarget, additive_keys};
use crate::query::GraphQuery;

/// Keyboard handling for canvases with [`CanvasKeyboard`]. Adds Bevy's
/// [`TabNavigationPlugin`] if the app has not.
pub struct NoodleKeyboardPlugin;

impl Plugin for NoodleKeyboardPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<TabNavigationPlugin>() {
            app.add_plugins(TabNavigationPlugin);
        }
        app.init_resource::<ButtonInput<KeyCode>>()
            .add_observer(on_key)
            .add_observer(snap_on_focus)
            .add_systems(PostUpdate, make_focusable);
    }
}

/// Turns on keyboard use for a canvas. Every key setting is a list: any of
/// its keys works, and an empty list unbinds it.
#[derive(Component, Reflect, Clone, Debug)]
#[reflect(Component, Default)]
#[require(TabGroup)]
pub struct CanvasKeyboard {
    /// Selects the focused node; with an additive key held, toggles it.
    pub select: Vec<KeyCode>,
    /// On a port: starts a connection, or completes one started elsewhere.
    pub connect: Vec<KeyCode>,
    /// Drops a connection being made.
    pub cancel: Vec<KeyCode>,
    /// Move the selection (or the focused node) left.
    pub move_left: Vec<KeyCode>,
    /// Move it right.
    pub move_right: Vec<KeyCode>,
    /// Move it up.
    pub move_up: Vec<KeyCode>,
    /// Move it down.
    pub move_down: Vec<KeyCode>,
    /// How far one key press moves, in graph units.
    pub step: f32,
    /// Held keys making selection additive (the same as
    /// [`CanvasInteraction`](crate::CanvasInteraction)'s by default).
    pub additive_keys: Vec<KeyCode>,
    /// Held with a move key: pan the view instead, by `pan_step`.
    pub pan_modifiers: Vec<KeyCode>,
    /// How far one key press pans, in canvas pixels.
    pub pan_step: f32,
    /// Zoom in around the canvas centre, by `zoom_step`, within the
    /// [`CanvasView`] limits.
    pub zoom_in: Vec<KeyCode>,
    /// Zoom out the same way.
    pub zoom_out: Vec<KeyCode>,
    /// The zoom factor of one key press.
    pub zoom_step: f32,
}

impl Default for CanvasKeyboard {
    fn default() -> Self {
        use KeyCode::*;
        Self {
            select: vec![Enter, NumpadEnter],
            connect: vec![Space],
            cancel: vec![Escape],
            move_left: vec![ArrowLeft],
            move_right: vec![ArrowRight],
            move_up: vec![ArrowUp],
            move_down: vec![ArrowDown],
            step: 10.0,
            additive_keys: additive_keys(),
            pan_modifiers: vec![ControlLeft, ControlRight, SuperLeft, SuperRight],
            pan_step: 60.0,
            zoom_in: vec![Equal, NumpadAdd],
            zoom_out: vec![Minus, NumpadSubtract],
            zoom_step: 1.2,
        }
    }
}

/// Nodes and ports of keyboard canvases become tabbable.
fn make_focusable(
    mut commands: Commands,
    graph: GraphQuery,
    children: Query<&Children>,
    keyboards: Query<(), With<CanvasKeyboard>>,
    new_canvases: Query<Entity, Added<CanvasKeyboard>>,
    new: Query<Entity, Or<(Added<GraphNode>, Added<Port>)>>,
    items: Query<(), (Or<(With<GraphNode>, With<Port>)>, Without<TabIndex>)>,
) {
    let inside = new_canvases
        .iter()
        .flat_map(|c| children.iter_descendants(c));
    for entity in new.iter().chain(inside) {
        let keyboard = graph
            .canvas_of(entity)
            .is_some_and(|c| keyboards.contains(c));
        if items.contains(entity) && keyboard {
            commands.entity(entity).try_insert(TabIndex(0));
        }
    }
}

#[allow(clippy::too_many_arguments, reason = "system parameters")]
fn on_key(
    mut input: On<FocusedInput<KeyboardInput>>,
    graph: GraphQuery,
    keyboards: Query<&CanvasKeyboard>,
    keys: Res<ButtonInput<KeyCode>>,
    wires: Query<&PendingWire>,
    anchors: Query<&PortAnchor>,
    mut views: Query<(&mut CanvasView, &ComputedNode)>,
    mut commands: Commands,
) {
    // Act once, where the key was pressed (it then bubbles up to the window).
    let target = input.event_target();
    let key = &input.input;
    if key.state != ButtonState::Pressed || target != input.original_event_target() {
        return;
    }
    let canvas = graph.canvas_of(target);
    let Some((canvas, settings)) = canvas.and_then(|c| Some((c, keyboards.get(c).ok()?))) else {
        return;
    };
    let code = key.key_code;
    let is = |keys: &[KeyCode]| keys.contains(&code);
    let node = graph.node_of(target).filter(|n| *n == target);
    // Keys typed into a control inside a node (a text field) are its own.
    if target != canvas && node.is_none() && graph.port(target).is_none() {
        return;
    }
    let origin = EditOrigin::Interaction;
    let wire = graph.wire_of(canvas);
    let s = settings;
    let moves = [&s.move_left, &s.move_right, &s.move_up, &s.move_down];
    let directions = [Vec2::NEG_X, Vec2::X, Vec2::NEG_Y, Vec2::Y];
    let direction = moves
        .into_iter()
        .zip(directions)
        .find_map(|(keys, direction)| is(keys).then_some(direction));
    let panning = keys.any_pressed(settings.pan_modifiers.iter().copied());
    let zoom = match () {
        _ if is(&s.zoom_in) => Some(s.zoom_step),
        _ if is(&s.zoom_out) => Some(1.0 / s.zoom_step),
        _ => None,
    };
    if let (Ok((mut view, computed)), true) = (
        views.get_mut(canvas),
        (panning && direction.is_some()) || zoom.is_some(),
    ) {
        if let Some(direction) = direction.filter(|_| panning) {
            // The view moves the way the key points, so the graph moves back.
            view.pan -= direction * settings.pan_step;
        }
        if let Some(factor) = zoom {
            let centre = computed.size() * computed.inverse_scale_factor() / 2.0;
            view.zoom_around(centre, factor);
        }
    } else if is(&settings.connect) && graph.port(target).is_some() {
        match wire {
            Some(entity) => {
                let from = wires.get(entity).ok().map(|w| w.from);
                if let Some(from) = from.filter(|from| *from != target) {
                    let edit = GraphEdit::Connect { from, to: target };
                    commands.graph_edit_with_origin(canvas, edit, origin);
                }
                commands.entity(entity).try_despawn();
            }
            None => {
                let pointer = anchors.get(target).ok().and_then(|a| a.position);
                let (from, pointer) = (target, pointer.unwrap_or_default());
                commands.spawn((PendingWire { from, pointer }, WireOf(canvas)));
                input.propagate(false);
                return;
            }
        }
    } else if let (Some(wire), true) = (wire, is(&settings.cancel)) {
        commands.entity(wire).try_despawn();
    } else if let (Some(node), true) = (node, is(&settings.select)) {
        let additive = keys.any_pressed(settings.additive_keys.iter().copied());
        let mode = if additive {
            SelectMode::Toggle
        } else {
            SelectMode::Replace
        };
        commands.select(canvas, vec![node], mode);
    } else if let (Some(node), Some(direction)) = (node, direction) {
        let delta = direction * settings.step;
        let edit = GraphEdit::move_nodes(graph.selection_with(node), delta);
        commands.graph_edit_with_origin(canvas, edit, origin);
    } else {
        return;
    }
    input.propagate(false);
}

/// While a connection is being made, focusing a compatible port snaps to it.
fn snap_on_focus(
    gained: On<FocusGained>,
    graph: GraphQuery,
    mut wires: Query<(&mut WireTarget, Option<&WireCandidates>)>,
) {
    // Act once, on the focused entity (the event then bubbles up).
    let port = gained.original_event_target();
    if gained.event_target() != port {
        return;
    }
    let wire = graph.canvas_of(port).and_then(|c| graph.wire_of(c));
    if let Some((mut target, candidates)) = wire.and_then(|w| wires.get_mut(w).ok()) {
        let fits = candidates.is_some_and(|c| c.contains(&port));
        target.set_if_neq(WireTarget(fits.then_some(port)));
    }
}
