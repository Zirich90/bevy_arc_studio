//! Keyboard use, headless: keys go through Bevy's input focus dispatch, as in
//! an app.

// Test helpers may panic: a panic is a failed test.
#![allow(
    clippy::disallowed_methods,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input_focus::tab_navigation::{TabGroup, TabIndex};
use bevy::input_focus::{FocusCause, InputDispatchPlugin, InputFocus, InputFocusPlugin};
use bevy::prelude::*;
use bevy::ui::Selected;
use bevy::window::PrimaryWindow;
use bevy_noodle::prelude::*;

const NUM: PortType = PortType::named("num");

struct Graph {
    canvas: Entity,
    nodes: [Entity; 2],
    /// Node 0's output and node 1's input.
    ports: [Entity; 2],
}

fn app() -> (App, Graph) {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        bevy::input::InputPlugin,
        InputFocusPlugin,
        InputDispatchPlugin,
        NoodlePlugins,
        NoodleKeyboardPlugin,
    ));
    let w = app.world_mut();
    w.spawn((Window::default(), PrimaryWindow));
    let canvas = w
        .spawn((NodeCanvas, CanvasKeyboard::default(), Node::default()))
        .id();
    // Nodes spawned under the canvas move into its content.
    let spawn = |w: &mut World, port: Port| {
        let node = (GraphNode, NodePosition::default(), Node::default());
        let node = w.spawn((node, ChildOf(canvas))).id();
        (node, w.spawn((port, Node::default(), ChildOf(node))).id())
    };
    let (a, out) = spawn(w, Port::output(NUM));
    let (b, inp) = spawn(w, Port::input(NUM));
    app.update();
    let graph = Graph {
        canvas,
        nodes: [a, b],
        ports: [out, inp],
    };
    (app, graph)
}

/// Focuses `entity`, presses `key` there, and runs a frame.
fn press(app: &mut App, entity: Entity, key: KeyCode) {
    let w = app.world_mut();
    w.resource_mut::<InputFocus>()
        .set(entity, FocusCause::Navigated);
    let window = w
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(w)
        .unwrap();
    for state in [ButtonState::Pressed, ButtonState::Released] {
        let logical_key = Key::Unidentified(bevy::input::keyboard::NativeKey::Unidentified);
        w.write_message(KeyboardInput {
            key_code: key,
            logical_key,
            state,
            text: None,
            repeat: false,
            window,
        });
    }
    app.update();
}

#[test]
fn nodes_and_ports_become_tabbable() {
    let (app, g) = app();
    let w = app.world();
    assert!(w.get::<TabGroup>(g.canvas).is_some());
    for entity in g.nodes.into_iter().chain(g.ports) {
        assert!(w.get::<TabIndex>(entity).is_some());
    }
}

#[test]
fn enter_selects_and_arrows_move() {
    let (mut app, g) = app();
    press(&mut app, g.nodes[0], KeyCode::Enter);
    assert!(app.world().get::<Selected>(g.nodes[0]).is_some());
    press(&mut app, g.nodes[0], KeyCode::ArrowRight);
    press(&mut app, g.nodes[0], KeyCode::ArrowDown);
    let position = app.world().get::<NodePosition>(g.nodes[0]).unwrap().0;
    assert_eq!(position, Vec2::new(10.0, 10.0));
    assert_eq!(
        app.world().get::<NodePosition>(g.nodes[1]).unwrap().0,
        Vec2::ZERO
    );
}

#[test]
fn space_on_two_ports_connects_them() {
    let (mut app, g) = app();
    press(&mut app, g.ports[0], KeyCode::Space);
    // Focusing a compatible port snaps the wire to it.
    let w = app.world_mut();
    assert_eq!(w.query::<&PendingWire>().iter(w).count(), 1);
    w.resource_mut::<InputFocus>()
        .set(g.ports[1], FocusCause::Navigated);
    app.update();
    let w = app.world_mut();
    let target = *w.query::<&WireTarget>().single(w).unwrap();
    assert_eq!(target, WireTarget(Some(g.ports[1])));

    press(&mut app, g.ports[1], KeyCode::Space);
    let w = app.world_mut();
    let targets: Vec<_> = w.query::<&EdgeTarget>().iter(w).map(|t| t.0).collect();
    assert_eq!(targets, [g.ports[1]]);
    assert_eq!(w.query::<&PendingWire>().iter(w).count(), 0);
}

#[test]
fn escape_cancels_a_connection() {
    let (mut app, g) = app();
    press(&mut app, g.ports[0], KeyCode::Space);
    press(&mut app, g.ports[0], KeyCode::Escape);
    let w = app.world_mut();
    assert_eq!(w.query::<&PendingWire>().iter(w).count(), 0);
    assert_eq!(w.query::<&Edge>().iter(w).count(), 0);
}

/// Holds `key` down (or lets it go) for the next frames.
fn hold(app: &mut App, key: KeyCode, state: ButtonState) {
    let w = app.world_mut();
    let window = w
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(w)
        .unwrap();
    let logical_key = Key::Unidentified(bevy::input::keyboard::NativeKey::Unidentified);
    let (text, repeat) = (None, false);
    w.write_message(KeyboardInput {
        key_code: key,
        logical_key,
        state,
        text,
        repeat,
        window,
    });
    app.update();
}

#[test]
fn modified_arrows_pan_and_plus_minus_zoom() {
    let (mut app, g) = app();
    hold(&mut app, KeyCode::ControlLeft, ButtonState::Pressed);
    press(&mut app, g.nodes[0], KeyCode::ArrowLeft);
    hold(&mut app, KeyCode::ControlLeft, ButtonState::Released);
    let view = *app.world().get::<CanvasView>(g.canvas).unwrap();
    assert_eq!(view.pan, Vec2::new(60.0, 0.0), "the view moved left");
    assert_eq!(
        app.world().get::<NodePosition>(g.nodes[0]).unwrap().0,
        Vec2::ZERO
    );
    press(&mut app, g.nodes[0], KeyCode::Equal);
    assert!((app.world().get::<CanvasView>(g.canvas).unwrap().zoom - 1.2).abs() < 1e-5);
    press(&mut app, g.nodes[0], KeyCode::Minus);
    assert!((app.world().get::<CanvasView>(g.canvas).unwrap().zoom - 1.0).abs() < 1e-5);
}

#[test]
fn keyboard_zoom_stays_within_the_view_limits() {
    let (mut app, g) = app();
    app.world_mut()
        .get_mut::<CanvasView>(g.canvas)
        .unwrap()
        .max_zoom = 1.1;
    press(&mut app, g.nodes[0], KeyCode::Equal);
    assert_eq!(app.world().get::<CanvasView>(g.canvas).unwrap().zoom, 1.1);
}

#[test]
fn keys_are_lists_and_empty_lists_unbind() {
    let (mut app, g) = app();
    let mut keyboard = app.world_mut().get_mut::<CanvasKeyboard>(g.canvas).unwrap();
    keyboard.select.clear();
    keyboard.move_right = vec![KeyCode::KeyD];
    press(&mut app, g.nodes[0], KeyCode::Enter);
    assert!(app.world().get::<Selected>(g.nodes[0]).is_none());
    press(&mut app, g.nodes[0], KeyCode::KeyD);
    press(&mut app, g.nodes[0], KeyCode::ArrowRight);
    let position = app.world().get::<NodePosition>(g.nodes[0]).unwrap().0;
    assert_eq!(position, Vec2::new(10.0, 0.0));
}

#[test]
fn keys_typed_into_controls_inside_nodes_are_theirs() {
    let (mut app, g) = app();
    let field = (Node::default(), TabIndex(0), ChildOf(g.nodes[0]));
    let field = app.world_mut().spawn(field).id();
    press(&mut app, field, KeyCode::Minus);
    let view = app.world().get::<CanvasView>(g.canvas).unwrap();
    assert_eq!(view.zoom, 1.0);
}
