//! Ready-made scenes for the default look: plain functions returning
//! [`Scene`]s, composed with Bevy's [`bsn!`] macro.
//!
//! ```
//! use bevy::prelude::*;
//! use bevy_noodle::{prelude::*, style::kit};
//! const NUMBER: PortType = PortType::named("number");
//! const BLUE: Color = Color::srgb(0.3, 0.6, 1.0);
//! # let mut world = World::new();
//! # let mut commands = world.commands();
//! let canvas = commands.spawn_scene(kit::canvas()).id();
//! commands.spawn_scene(bsn! {
//!     @kit::node(Vec2::new(40.0, 40.0))
//!     // Patch single fields; the others keep the kit's values.
//!     Node { min_width: px(220) }
//!     ChildOf(canvas)
//!     Children [
//!         @kit::title("Add")
//!         --
//!         @kit::row("a", Port::input(NUMBER), BLUE)
//!         --
//!         @kit::row("sum", Port::output(NUMBER), BLUE)
//!     ]
//! });
//! ```
//!
//! A node is a padded column: its title, then rows. Any other content, such
//! as feathers controls, can go between them and lines up with the rows.

use bevy::picking::Pickable;
use bevy::picking::cursor::EntityCursor;
use bevy::prelude::*;
use bevy::window::SystemCursorIcon;

use super::{
    CanvasGrid, EdgeStyle, FocusOutline, PortColor, PortHighlight, SelectedBorderColor,
    SelectionBoxStyle,
};
use crate::components::{GraphNode, NodeCanvas, NodePosition, Port, PortDirection};
use crate::interaction::CanvasInteraction;

const BORDER: f32 = 1.5;
/// Horizontal padding inside a node.
const PADDING: f32 = 10.0;
const PORT_RADIUS: f32 = 6.0;

/// An interactive canvas filling its parent, with the whole default look.
///
/// It clips its content, which also lets Bevy skip drawing nodes and edges
/// that are out of view.
pub fn canvas() -> impl Scene {
    bsn! {
        NodeCanvas
        CanvasInteraction
        Node { width: percent(100), height: percent(100), overflow: Overflow::clip() }
        EdgeStyle
        CanvasGrid
        SelectionBoxStyle
        FocusOutline
    }
}

/// A node frame at `position` whose border follows selection: a padded
/// column of its title and rows, given as its `Children`.
pub fn node(position: Vec2) -> impl Scene {
    bsn! {
        GraphNode
        NodePosition(position)
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: px(6),
            min_width: px(160),
            padding: UiRect::new(px(PADDING), px(PADDING), px(0), px(8)),
            border: UiRect::all(px(BORDER)),
            border_radius: BorderRadius::all(px(8)),
        }
        BackgroundColor(Color::srgb_u8(44, 46, 52))
        SelectedBorderColor
        BoxShadow::new(Color::srgba(0.0, 0.0, 0.0, 0.45), px(0), px(4), px(0), px(14))
        // Controls inside keep their own cursors.
        EntityCursor::System(SystemCursorIcon::Grab)
    }
}

/// A title bar spanning the node's width.
pub fn title(text: impl Into<String>) -> impl Scene {
    let text = text.into();
    bsn! {
        Text(text)
        TextFont { font_size: FontSize::Px(14.0) }
        Node {
            padding: UiRect::axes(px(PADDING), px(5)),
            // Out over the node's padding, to its border.
            margin: UiRect::new(px(-PADDING), px(-PADDING), px(0), px(2)),
            border_radius: BorderRadius::top(px(6.5)),
        }
        BackgroundColor(Color::srgb_u8(60, 63, 71))
    }
}

/// A port dot centered on the node's border, on the side its direction faces.
pub fn port(port: Port, color: Color) -> impl Scene {
    // Centered on the frame's border; rows reach its inner edge.
    let inset = px(-(BORDER + PORT_RADIUS));
    let (left, right) = match port.direction {
        PortDirection::Input => (inset, Val::Auto),
        PortDirection::Output => (Val::Auto, inset),
    };
    bsn! {
        port
        Node {
            position_type: PositionType::Absolute,
            width: px(PORT_RADIUS * 2.0),
            height: px(PORT_RADIUS * 2.0),
            top: percent(50),
            left: left,
            right: right,
            margin: UiRect::top(px(-PORT_RADIUS)),
            border: UiRect::all(px(1.5)),
            border_radius: BorderRadius::MAX,
        }
        PortColor(color)
        PortHighlight
        BackgroundColor(color)
        BorderColor::all(color)
        EntityCursor::System(SystemCursorIcon::Crosshair)
    }
}

/// A row with a label and `port`, both on the side its direction faces:
/// `row("a", Port::input(NUMBER), BLUE)`.
pub fn row(label: impl Into<String>, kind: Port, color: Color) -> impl Scene {
    let label = label.into();
    let justify = match kind.direction {
        PortDirection::Input => JustifyContent::FlexStart,
        PortDirection::Output => JustifyContent::FlexEnd,
    };
    bsn! {
        Node {
            justify_content: justify,
            align_items: AlignItems::Center,
            min_height: px(22),
            // Out over the node's padding, so the port reaches its border.
            margin: UiRect::horizontal(px(-PADDING)),
            padding: UiRect::horizontal(px(PADDING)),
        }
        Children [
            Text(label)
            TextFont { font_size: FontSize::Px(13.0) }
            TextColor(Color::srgb_u8(214, 218, 224))
            Pickable { should_block_lower: false, is_hoverable: false }
            --
            @port(kind, color)
        ]
    }
}

/// A row holding only a port: no label, so no text to lay out. For dense or
/// very large graphs.
pub fn dot(kind: Port, color: Color) -> impl Scene {
    bsn! {
        Node { min_height: px(16), margin: UiRect::horizontal(px(-PADDING)) }
        Children [ @port(kind, color) ]
    }
}

#[cfg(test)]
#[allow(clippy::disallowed_methods, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::{GraphEdit, GraphQuery, GraphWorldExt, NoodleCorePlugin, PortType};
    use bevy::ecs::system::RunSystemOnce;
    use bevy::scene::ScenePlugin;

    const NUM: PortType = PortType::named("num");

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin::default(),
            ScenePlugin,
            NoodleCorePlugin,
        ));
        app
    }

    #[test]
    fn a_field_override_keeps_the_other_defaults() {
        let mut app = app();
        let w = app.world_mut();
        let plain = w.spawn_scene(node(Vec2::ZERO)).unwrap().id();
        let scene = bsn! { @node(Vec2::ZERO) Node { min_width: px(240) } };
        let wide = w.spawn_scene(scene).unwrap().id();
        let (plain, wide) = (w.get::<Node>(plain).unwrap(), w.get::<Node>(wide).unwrap());
        let expected = Node {
            min_width: px(240),
            ..plain.clone()
        };
        assert_eq!(*wide, expected);
    }

    #[test]
    fn scene_nodes_join_the_graph() {
        let mut app = app();
        let w = app.world_mut();
        let canvas = w.spawn_scene(canvas()).unwrap().id();
        let mut spawn = |kind: Port| {
            let scene = bsn! { @node(Vec2::ZERO) ChildOf(canvas) Children [ @row("p", kind, Color::WHITE) ] };
            w.spawn_scene(scene).unwrap().id()
        };
        let (a, b) = (spawn(Port::output(NUM)), spawn(Port::input(NUM)));
        app.update();
        let w = app.world_mut();
        let ports = w.run_system_once(move |graph: GraphQuery| {
            assert_eq!(graph.nodes_in(canvas).count(), 2);
            (graph.outputs_of(a).next(), graph.inputs_of(b).next())
        });
        let Ok((Some(from), Some(to))) = ports else {
            panic!("ports not found: {ports:?}");
        };
        assert!(
            w.graph_edit(canvas, GraphEdit::Connect { from, to })
                .is_ok()
        );
    }
}
