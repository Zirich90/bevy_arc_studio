//! Comment frames: a frame is just a node drawn under the others, and an
//! `EditApplied` observer moves the nodes inside it along with it. The extra
//! move carries its own `EditOrigin`, so it does not trigger itself and an
//! undo stack can tell the two apart.
//!
//! A frame's title is a text field (Bevy's `EditableText`): click it and
//! type. It has a `TabIndex`, so pressing or dragging in it edits the text
//! instead of moving the frame.
//!
//! ```sh
//! cargo run --example comment_frames --features default_style
//! ```

use bevy::input_focus::tab_navigation::TabIndex;
use bevy::picking::cursor::CursorIconPlugin;
use bevy::prelude::*;
use bevy::text::{EditableText, LineBreak, TextCursorStyle};
use bevy::ui_widgets::TextInput;
use bevy_noodle::prelude::*;
use bevy_noodle::style::kit;

const NUMBER: PortType = PortType::named("number");
const BLUE: Color = Color::srgb(0.25, 0.52, 0.9);
/// Marks the moves frames make.
const CARRIED: EditOrigin = EditOrigin::Custom(1);

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, CursorIconPlugin))
        .add_plugins((NoodlePlugins, NoodleDefaultStylePlugin))
        .insert_resource(ClearColor(Color::srgb_u8(24, 25, 29)))
        .add_systems(Startup, setup)
        .add_observer(carry_contents)
        .run();
}

#[derive(Component)]
struct Frame;

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    let canvas = commands.spawn_scene(kit::canvas()).id();
    for (title, at, size, color) in [
        (
            "Inputs",
            Vec2::new(40.0, 60.0),
            Vec2::new(260.0, 330.0),
            (0.35, 0.55, 0.95),
        ),
        (
            "Math",
            Vec2::new(360.0, 120.0),
            Vec2::new(260.0, 200.0),
            (0.95, 0.7, 0.3),
        ),
    ] {
        let (r, g, b) = color;
        commands.spawn((
            Frame,
            GraphNode,
            NodePosition(at),
            // Under the nodes: raising leaves a negative `ZIndex` alone.
            ZIndex(-1),
            Node {
                width: px(size.x),
                height: px(size.y),
                padding: UiRect::all(px(8)),
                border: UiRect::all(px(1.5)),
                border_radius: BorderRadius::all(px(10)),
                ..default()
            },
            BackgroundColor(Color::srgba(r, g, b, 0.08)),
            BorderColor::all(Color::srgba(r, g, b, 0.5)),
            ChildOf(canvas),
            children![(
                EditableText::new(title),
                TextInput,
                TabIndex(0),
                Node {
                    width: percent(100),
                    align_self: AlignSelf::FlexStart,
                    ..default()
                },
                TextLayout::linebreak(LineBreak::NoWrap),
                TextFont::from_font_size(16.0),
                TextColor(Color::srgb(r, g, b)),
                // Without a cursor style, Bevy draws no cursor or selection.
                TextCursorStyle {
                    color: Color::srgb(r, g, b),
                    selection_color: Color::srgba(r, g, b, 0.3),
                    unfocused_selection_color: Color::NONE,
                    ..default()
                },
            )],
        ));
    }
    for (name, y) in [("First", 110.0), ("Second", 250.0)] {
        commands.spawn_scene(bsn! {
            @kit::node(Vec2::new(80.0, y)) ChildOf(canvas)
            Children [ @kit::title(name) -- @kit::row("value", Port::output(NUMBER), BLUE) ]
        });
    }
    commands.spawn_scene(bsn! {
        @kit::node(Vec2::new(410.0, 170.0)) ChildOf(canvas)
        Children [
            @kit::title("Add") -- @kit::row("a", Port::input(NUMBER), BLUE)
            -- @kit::row("b", Port::input(NUMBER), BLUE) -- @kit::row("sum", Port::output(NUMBER), BLUE)
        ]
    });
    commands.spawn_scene(bsn! {
        @kit::node(Vec2::new(720.0, 200.0)) ChildOf(canvas)
        Children [ @kit::title("Outside") -- @kit::row("value", Port::input(NUMBER), BLUE) ]
    });
}

/// When a frame moves, the nodes that were inside it move too.
fn carry_contents(
    applied: On<EditApplied>,
    frames: Query<(&NodePosition, &ComputedNode), With<Frame>>,
    nodes: Query<(Entity, &NodePosition, &ComputedNode), Without<Frame>>,
    graph: GraphQuery,
    mut commands: Commands,
) {
    let GraphChange::Moved {
        nodes: moved,
        delta,
        drag,
    } = &applied.change
    else {
        return;
    };
    if applied.origin == CARRIED {
        return;
    }
    let rect = |position: Vec2, computed: &ComputedNode| {
        Rect::from_corners(
            position,
            position + computed.size() * computed.inverse_scale_factor(),
        )
    };
    let mut carried = Vec::new();
    for (frame, computed) in moved.iter().filter_map(|f| frames.get(*f).ok()) {
        // Where the frame was before this step.
        let before = rect(frame.0 - *delta, computed);
        for (node, position, computed) in &nodes {
            let inside = before.contains(rect(position.0, computed).min)
                && before.contains(rect(position.0, computed).max);
            let here = graph.canvas_of(node) == Some(applied.canvas);
            if inside && here && !moved.contains(&node) && !carried.contains(&node) {
                carried.push(node);
            }
        }
    }
    if !carried.is_empty() {
        // Carried along the same drag, so undo still sees one gesture.
        let edit = GraphEdit::MoveNodes {
            nodes: carried,
            delta: *delta,
            drag: *drag,
        };
        commands.graph_edit_with_origin(applied.canvas, edit, CARRIED);
    }
}
