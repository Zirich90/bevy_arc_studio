//! A minimal, headless node graph library for Bevy UI: you build and style the
//! nodes and edges, it handles the graph.
//!
//! ```text
//! NodeCanvas              your UI node: one graph and its viewport (no background)
//! └── CanvasContent       spawned by the canvas; pans and zooms, holds the nodes
//!     └── GraphNode       your UI node, styled however you like
//!         └── … Port      your UI node marking a connection point
//!
//! Edge                    spawned on connect, outside the hierarchy: its
//!                         EdgeSource/EdgeTarget ports decide its graph
//! ```
//!
//! Spawn nodes as children of a canvas; they move into its content. Any
//! number of canvases can coexist or nest; every lookup resolves to the
//! nearest one. Read graphs with [`GraphQuery`], change them with
//! [`GraphCommandsExt::graph_edit`], and select with
//! [`GraphCommandsExt::select`]. Interaction is opt-in per canvas with
//! [`CanvasInteraction`]. The core draws nothing: draw edges from
//! [`EdgeGeometry`], or enable the `default_style` feature. The `scene`
//! feature adds graph snapshots (undo, save and load).
//!
//! Evaluating a graph is the app's job: Noodle has no execution engine and no
//! built-in nodes. Give nodes your own components and walk [`GraphQuery`] in
//! ordinary systems; [`GraphQuery::source_of`] finds what feeds an input.
//!
//! Events follow one rule. What observers answer is only triggered on the
//! canvas: [`ConnectionCheck`] (connection rules, also asked by previews) and
//! [`EditRequested`] (rewrite or veto an edit about to apply). What happened
//! is triggered on the canvas and also written as a message, for observers
//! and systems alike: [`EditApplied`], [`EditRejected`] and [`WireDropped`].
//! Selection is Bevy's `Selected` component: observe its `Add` and `Remove`.

// Bevy's `AsBindGroup` derive trips a recursion lint on recent compilers.
#![recursion_limit = "256"]
#![allow(clippy::type_complexity)]
#![warn(missing_docs)]

mod components;
mod edit;
mod geometry;
mod interaction;
mod keyboard;
mod query;
// Compile the README's quick start alongside the API examples.
#[cfg(all(doctest, feature = "default_style"))]
#[doc = include_str!("../README.md")]
pub struct Readme;
#[cfg(feature = "scene")]
pub mod scene;
#[cfg(feature = "default_style")]
pub mod style;

use bevy::app::PluginGroupBuilder;
use bevy::prelude::*;
use bevy::ui::UiSystems;

pub use components::*;
pub use edit::*;
pub use interaction::*;
pub use keyboard::*;
pub use query::{Connection, GraphQuery};

/// System sets, all in `PostUpdate`.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum NoodleSystems {
    /// Before UI layout: node positions, canvas view, edge geometry.
    Sync,
    /// After `Sync`, before layout: drawing (put your edge renderers here).
    /// Ports are measured after layout, in Bevy's `UiSystems::PostLayout`.
    Render,
}

/// [`NoodleCorePlugin`] + [`NoodleInteractionPlugin`].
pub struct NoodlePlugins;

impl PluginGroup for NoodlePlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(NoodleCorePlugin)
            .add(NoodleInteractionPlugin)
    }
}

/// Graph components, the edit pipeline and geometry.
pub struct NoodleCorePlugin;

impl Plugin for NoodleCorePlugin {
    fn build(&self, app: &mut App) {
        use geometry::*;
        app.add_message::<EditApplied>()
            .add_message::<EditRejected>()
            .add_observer(components::adopt_nodes)
            .configure_sets(
                PostUpdate,
                (NoodleSystems::Sync, NoodleSystems::Render)
                    .chain()
                    .before(UiSystems::Prepare),
            )
            .add_systems(
                PostUpdate,
                (drop_split_edges, sync_layout, update_edge_geometry)
                    .chain()
                    .in_set(NoodleSystems::Sync),
            )
            .add_systems(PostUpdate, measure_ports.in_set(UiSystems::PostLayout));
    }
}

/// Everything most apps need: `use bevy_noodle::prelude::*;`. The rest
/// (relationship targets, measured anchors) is at the crate root.
pub mod prelude {
    #[cfg(feature = "scene")]
    pub use crate::scene::{SnapshotWorldExt, Transient};
    #[cfg(feature = "default_style")]
    pub use crate::style::{
        CanvasGrid, EdgeStyle, FocusOutline, NoodleDefaultStylePlugin, PortColor, PortHighlight,
        SelectedBorderColor, SelectionBoxStyle,
    };
    pub use crate::{
        CanvasContent, CanvasInteraction, CanvasKeyboard, CanvasView, Capacity, Connection,
        ConnectionCheck, DragProgress, Edge, EdgeGeometry, EdgeHitbox, EdgeSource, EdgeTarget,
        EditApplied, EditOrigin, EditRejected, EditRequested, GraphChange, GraphCommandsExt,
        GraphEdit, GraphNode, GraphQuery, GraphWorldExt, NodeCanvas, NodeDragHandle, NodePosition,
        NoodleCorePlugin, NoodleInteractionPlugin, NoodleKeyboardPlugin, NoodlePlugins,
        NoodleSystems, PendingWire, Port, PortDirection, PortPair, PortTangent, PortType,
        RejectReason, ScrollMode, SelectMode, SelectionBox, WireCandidates, WireDropped, WireOf,
        WireTarget,
    };
}
