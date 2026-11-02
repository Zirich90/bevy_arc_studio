# bevy_noodle

[![crates.io](https://img.shields.io/crates/v/bevy_noodle.svg)](https://crates.io/crates/bevy_noodle)
[![docs.rs](https://docs.rs/bevy_noodle/badge.svg)](https://docs.rs/bevy_noodle)
[![CI](https://github.com/gold-silver-copper/bevy_noodle/actions/workflows/ci.yml/badge.svg)](https://github.com/gold-silver-copper/bevy_noodle/actions/workflows/ci.yml)

A headless node graph library for [Bevy](https://bevyengine.org) UI.
**You build and style the nodes; bevy_noodle handles the graph.**

![A wire snapping onto a port](https://raw.githubusercontent.com/gold-silver-copper/bevy_noodle/main/docs/connecting.png)

- **Just entities.** Nodes and ports are your UI entities with a marker
  component; edges are Bevy relationships.
- **Draws nothing by default.** Draw edges from `EdgeGeometry` yourself, or
  enable the `default_style` look.
- **Any number of graphs**, side by side or nested inside nodes.
- **One edit pipeline** for UI and code, with observers that can veto or
  rewrite any edit.

Requires Bevy **0.20** and Rust **1.97.1** or newer.

## Examples

```sh
cargo run --example <name> --all-features
```

<table>
<tr>
<td colspan="2"><img src="https://raw.githubusercontent.com/gold-silver-copper/bevy_noodle/main/docs/shader_world.png" alt="shader_world"><br><b>shader_world</b>: a node graph that writes a shader, raymarching a landscape you fly over (use <code>--release</code>)</td>
</tr>
<tr>
<td width="50%"><img src="https://raw.githubusercontent.com/gold-silver-copper/bevy_noodle/main/docs/scene_builder_3d.png" alt="scene_builder_3d"><br><b>scene_builder_3d</b>: build a live 3D scene from a graph</td>
<td width="50%"><img src="https://raw.githubusercontent.com/gold-silver-copper/bevy_noodle/main/docs/edge_styles.png" alt="edge_styles"><br><b>edge_styles</b>: gradients, dashes and animated flow</td>
</tr>
<tr>
<td><img src="https://raw.githubusercontent.com/gold-silver-copper/bevy_noodle/main/docs/subgraph.png" alt="subgraph"><br><b>subgraph</b>: graphs nested inside nodes</td>
<td><img src="https://raw.githubusercontent.com/gold-silver-copper/bevy_noodle/main/docs/styled.png" alt="styled"><br><b>styled</b>: the default look with feathers controls</td>
</tr>
<tr>
<td><img src="https://raw.githubusercontent.com/gold-silver-copper/bevy_noodle/main/docs/math_graph.png" alt="math_graph"><br><b>math_graph</b>: a live calculator that rejects cycles</td>
<td><img src="https://raw.githubusercontent.com/gold-silver-copper/bevy_noodle/main/docs/editor.png" alt="editor"><br><b>editor</b>: undo/redo, copy/paste, edge selection</td>
</tr>
<tr>
<td><img src="https://raw.githubusercontent.com/gold-silver-copper/bevy_noodle/main/docs/type_conversion.png" alt="type_conversion"><br><b>type_conversion</b>: auto-inserted converter nodes</td>
<td><img src="https://raw.githubusercontent.com/gold-silver-copper/bevy_noodle/main/docs/comment_frames.png" alt="comment_frames"><br><b>comment_frames</b>: frames that carry their nodes</td>
</tr>
<tr>
<td><img src="https://raw.githubusercontent.com/gold-silver-copper/bevy_noodle/main/docs/reroute.png" alt="reroute"><br><b>reroute</b>: reroute dots on edges</td>
<td><img src="https://raw.githubusercontent.com/gold-silver-copper/bevy_noodle/main/docs/minimap.png" alt="minimap"><br><b>minimap</b>: a synced overview you can drag</td>
</tr>
<tr>
<td><img src="https://raw.githubusercontent.com/gold-silver-copper/bevy_noodle/main/docs/auto_layout.png" alt="auto_layout"><br><b>auto_layout</b>: layered layout as undoable edits</td>
<td><img src="https://raw.githubusercontent.com/gold-silver-copper/bevy_noodle/main/docs/keyboard.png" alt="keyboard"><br><b>keyboard</b>: Tab, arrows and Space to connect</td>
</tr>
<tr>
<td><img src="https://raw.githubusercontent.com/gold-silver-copper/bevy_noodle/main/docs/save_load.png" alt="save_load"><br><b>save_load</b>: save and load graphs as RON</td>
<td><img src="https://raw.githubusercontent.com/gold-silver-copper/bevy_noodle/main/docs/stress.png" alt="stress"><br><b>stress</b>: hundreds of nodes rewired every frame (use <code>--release</code>)</td>
</tr>
<tr>
<td><img src="https://raw.githubusercontent.com/gold-silver-copper/bevy_noodle/main/docs/minimal.png" alt="minimal"><br><b>minimal</b>: plain Bevy UI, edges drawn as gizmos</td>
<td><img src="https://raw.githubusercontent.com/gold-silver-copper/bevy_noodle/main/docs/geometry_nodes.png" alt="geometry_nodes"><br><b>geometry_nodes</b>: Blender-style point nodes, including a world query</td>
</tr>
</table>

## Quick start

Add these dependencies to your application's `Cargo.toml`:

```toml
[dependencies]
bevy = "0.20"
bevy_noodle = { version = "0.5", features = ["default_style"] }
```

Put this in `src/main.rs` and run `cargo run`:

```no_run
use bevy::picking::cursor::CursorIconPlugin;
use bevy::prelude::*;
use bevy_noodle::{prelude::*, style::kit};

const NUMBER: PortType = PortType::named("number");
const BLUE: Color = Color::srgb(0.3, 0.6, 1.0);

fn main() {
    App::new()
        // Hover and drag cursors. Apps using Feathers get it from Feathers.
        .add_plugins((DefaultPlugins, CursorIconPlugin))
        .add_plugins((NoodlePlugins, NoodleDefaultStylePlugin))
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    let canvas = commands.spawn_scene(kit::canvas()).id();
    commands.spawn_scene(bsn! {
        @kit::node(Vec2::new(80.0, 120.0)) ChildOf(canvas)
        Children [ @kit::title("Source") -- @kit::row("number", Port::output(NUMBER), BLUE) ]
    });
    commands.spawn_scene(bsn! {
        @kit::node(Vec2::new(420.0, 120.0)) ChildOf(canvas)
        Children [ @kit::title("Sink") -- @kit::row("number", Port::input(NUMBER), BLUE) ]
    });
}
```

Drag from the Source's blue port to the Sink's blue port to connect them.
Drag a node to move it. The grid and wires come from `default_style` and
`NoodleDefaultStylePlugin`; `kit::canvas()` enables them on this canvas.

The `kit` functions return scenes for Bevy's `bsn!` macro. Patch single
fields over their defaults, such as `@kit::node(at) Node { min_width: px(240) }`,
and put your own content, such as feathers controls, among a node's
`Children`; it lines up with the rows. They are optional: nodes are plain Bevy UI entities with
`GraphNode` and `Port` components, built any way you like
(`examples/minimal.rs` uses no styling at all).

From this repository, run the same example with:

```sh
cargo run --example quick_start --features default_style
```

For custom rendering, omit `default_style` and follow `examples/minimal.rs`.

## How it works

```text
NodeCanvas              one graph and its viewport
└── CanvasContent       spawned by the canvas; pans and zooms (CanvasView), holds the nodes
    └── GraphNode       your UI node
        └── … Port      your UI node marking a connection point

Edge                    relates an output port to an input port; has EdgeGeometry.
                        No parent: its ports decide which graph it is in.
```

- **Read** with the `GraphQuery` system param (`nodes_in`, `edges_of`,
  `peers_of`, `source_of`, …).
- **Write** with `commands.graph_edit(canvas, GraphEdit::Connect { from, to })`;
  also `Disconnect`, `MoveNodes` and `Delete`. Nodes are just spawned, as
  children of the canvas.
- **Select** with `commands.select(canvas, items, mode)`; selection is Bevy's
  `Selected` component, not an edit.
- **Validate** connections in `On<ConnectionCheck>` observers: `allow()` or
  `reject()`, overriding the built-in type rules. Dragged wires ask them too.
- **Redirect** edits in `On<EditRequested>` observers: rewrite or `reject()`
  them, and do side effects there.
- **React** to `EditApplied`: its `GraphChange` says what changed.
- **Style** from `Selected`, the dragged wire's `WireCandidates` and `WireTarget`, and `SelectionBox`.
- **Controls inside nodes** (sliders, text fields, menus) keep their own
  presses and drags.

### Evaluating a graph is your app's job

bevy_noodle stores and edits the graph; it never runs it. There is no
execution engine, no value type, and no built-in node that queries the
world or iterates a collection. Your app decides what its nodes mean,
usually with a component on each node, and evaluates them in ordinary Bevy
systems that walk `GraphQuery`, on whatever schedule fits: every frame, or
on `EditApplied`. `source_of(node, i)` finds the output feeding a node's
input, the usual first step of evaluating it.

- `math_graph`, `styled` and `type_conversion` compute values through the
  graph; `math_graph` also rejects cycles in a `ConnectionCheck` observer.
- `scene_builder_3d` builds 3D entities from the graph after every edit.
- `shader_world` compiles the graph into a WGSL shader: each node is one
  line of code, and rewiring regenerates a raymarched landscape.
- `geometry_nodes` builds Blender-style geometry nodes in about 100 lines
  of node code: wires carry lists of points, one node queries entities
  already in the world, others change or filter every point, and the result
  is re-evaluated every frame.

## Features

| Feature | Adds |
|---|---|
| `default_style` | Wire shader (`EdgeStyle`), `CanvasGrid`, selection box, port highlights, and `style::kit` node scenes (enables Bevy's `bevy_scene`) |
| `scene` | `DynamicWorld` snapshots for undo, copy/paste and save/load |

Keyboard use is a plugin: add `NoodleKeyboardPlugin` and `CanvasKeyboard`.

## Release checks

CI checks each feature combination both as a library (without the examples'
extra Bevy features) and with Clippy across all targets. It runs tests,
compiles documentation and verifies the release package. Library build jobs
cover Linux, Windows and macOS, plus Rust 1.97.1 on Linux.

Before releasing, run `tools/audit_inputs/run.sh` on a Linux desktop to
exercise the examples' text fields at several zoom levels. It saves results,
screenshots of failures, and process logs under `target/audit_inputs/out`.
Also check dragging, connecting, keyboard navigation, nested canvases and
save/load in the graphical examples on the platforms you intend to support;
library build checks do not exercise windows or GPU rendering.

Set the changelog's release date and commit the final changes. After CI
passes, run `cargo publish --dry-run --locked --all-features` on that clean
commit, then publish and tag that same reviewed revision.

## License

MIT
