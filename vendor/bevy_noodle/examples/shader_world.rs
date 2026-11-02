//! A node graph that writes a shader: wire noise, warps and colors into
//! World, and a raymarched landscape you fly over is generated from them on
//! the GPU. Every edit to the graph compiles a new shader; sliders are
//! uniforms, so turning them reshapes the world live.
//!
//! The nodes are this file's own `Op`: each one is a line of WGSL. Ports are
//! typed (positions, numbers, colors), so bevy_noodle's connection rules
//! refuse wiring a color into a height. Compiling walks back from World
//! through `GraphQuery::source_of`, emitting one `let` per node; an
//! unconnected input takes a default, and a cycle is cut where it closes.
//! It runs only when an edit applies; time and the sliders are uniforms,
//! updated every frame.
//!
//! ```sh
//! cargo run --example shader_world --features default_style --release
//! ```

use std::collections::{HashMap, HashSet};

use bevy::asset::uuid_handle;
use bevy::feathers::FeathersPlugins;
use bevy::feathers::controls::FeathersSlider;
use bevy::feathers::dark_theme::create_dark_theme;
use bevy::feathers::theme::UiTheme;
use bevy::prelude::*;
use bevy::render::RenderPlugin;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;
use bevy::ui_widgets::{SliderPrecision, SliderStep, SliderValue, ValueChange, slider_self_update};
use bevy_noodle::{prelude::*, style::kit};

const VEC2: PortType = PortType::named("position");
const FLOAT: PortType = PortType::named("number");
const COLOR: PortType = PortType::named("color");
const TEMPLATE: &str = include_str!("shaders/world.wesl");
const SHADER: Handle<Shader> = uuid_handle!("5be1d8a4-0c34-4f0e-9a49-6a3f8f2c7d11");

/// A node: one WGSL expression, with its number set by a slider.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
enum Op {
    Position,
    Time,
    Scale(f32),
    Warp,
    Hills,
    Ridges,
    Add,
    Amplify(f32),
    Terrace(f32),
    Biome,
    Palette,
    World,
}

impl Op {
    /// Inputs: label, type, and the WGSL used when unconnected.
    fn inputs(self) -> &'static [(&'static str, PortType, &'static str)] {
        match self {
            Op::Position | Op::Time => &[],
            Op::Scale(_) | Op::Hills | Op::Ridges => &[("position", VEC2, "p")],
            Op::Warp => &[("position", VEC2, "p"), ("amount", FLOAT, "0.6")],
            Op::Add => &[("a", FLOAT, "0.0"), ("b", FLOAT, "0.0")],
            Op::Amplify(_) | Op::Terrace(_) => &[("height", FLOAT, "0.0")],
            Op::Biome | Op::Palette => &[("height", FLOAT, "h")],
            Op::World => &[("height", FLOAT, "0.3"), ("color", COLOR, "biome(h)")],
        }
    }

    fn output(self) -> Option<PortType> {
        match self {
            Op::Position | Op::Scale(_) | Op::Warp => Some(VEC2),
            Op::Biome | Op::Palette => Some(COLOR),
            Op::World => None,
            _ => Some(FLOAT),
        }
    }

    /// The slider's number, its range, and the uniform it drives.
    fn number(self) -> Option<(f32, f32, f32, &'static str)> {
        match self {
            Op::Scale(k) => Some((k, 0.05, 1.0, "world.params.x")),
            Op::Amplify(k) => Some((k, 0.0, 5.0, "world.params.y")),
            Op::Terrace(k) => Some((k, 1.0, 10.0, "world.params.z")),
            _ => None,
        }
    }

    fn with_number(self, k: f32) -> Self {
        match self {
            Op::Scale(_) => Op::Scale(k),
            Op::Amplify(_) => Op::Amplify(k),
            Op::Terrace(_) => Op::Terrace(k),
            op => op,
        }
    }

    /// The WGSL expression for this op's output, given its input values.
    fn wgsl(self, args: &[String]) -> Option<String> {
        let a = |i: usize| args.get(i).map_or("0.0", String::as_str);
        let k = self.number().map_or("1.0", |(.., uniform)| uniform);
        Some(match self {
            Op::Position => "p".into(),
            Op::Time => "world.time_aspect.x".into(),
            Op::Scale(_) | Op::Amplify(_) => format!("{} * {k}", a(0)),
            Op::Warp => {
                let (q, s) = (a(0), a(1));
                format!("{q} + {s} * vec2<f32>(fbm({q} + 3.1), fbm({q} - 1.7))")
            }
            Op::Hills => format!("fbm({})", a(0)),
            Op::Ridges => format!("ridged({})", a(0)),
            Op::Add => format!("{} + {}", a(0), a(1)),
            Op::Terrace(_) => format!("round({} * {k}) / {k}", a(0)),
            Op::Biome => format!("biome({})", a(0)),
            Op::Palette => format!("palette({})", a(0)),
            Op::World => return None,
        })
    }
}

/// Writes one shader function's body, one `let` per node reached.
struct Codegen<'a, 'w, 's> {
    graph: &'a GraphQuery<'w, 's>,
    ops: &'a Query<'w, 's, (Entity, &'static Op)>,
    lines: Vec<String>,
    names: HashMap<Entity, String>,
    visiting: HashSet<Entity>,
}

impl<'a, 'w, 's> Codegen<'a, 'w, 's> {
    fn new(graph: &'a GraphQuery<'w, 's>, ops: &'a Query<'w, 's, (Entity, &'static Op)>) -> Self {
        let (lines, names, visiting) = default();
        Self {
            graph,
            ops,
            lines,
            names,
            visiting,
        }
    }

    /// The value of `node`'s input `i`, or its default.
    fn input(&mut self, node: Entity, i: usize, default: &str) -> String {
        let from = self
            .graph
            .source_of(node, i)
            .and_then(|o| self.graph.node_of(o));
        from.and_then(|from| self.value(from))
            .unwrap_or_else(|| default.into())
    }

    /// The name holding `node`'s output; `None` if it has none or closes a cycle.
    fn value(&mut self, node: Entity) -> Option<String> {
        if let Some(name) = self.names.get(&node) {
            return Some(name.clone());
        }
        let (_, op) = self.ops.get(node).ok()?;
        if !self.visiting.insert(node) {
            return None;
        }
        let args: Vec<String> = op
            .inputs()
            .iter()
            .enumerate()
            .map(|(i, (_, _, default))| self.input(node, i, default))
            .collect();
        self.visiting.remove(&node);
        let name = format!("v{}", self.lines.len());
        self.lines
            .push(format!("    let {name} = {};", op.wgsl(&args)?));
        self.names.insert(node, name.clone());
        Some(name)
    }

    /// The function body returning World's input `i`.
    fn body(mut self, world: Option<Entity>, i: usize, default: &str) -> String {
        let value = match world {
            Some(world) => self.input(world, i, default),
            None => default.into(),
        };
        self.lines.push(format!("    return {value};"));
        self.lines.join("\n")
    }
}

/// The landscape's material: the generated shader plus the time.
#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
struct WorldMaterial {
    #[uniform(0)]
    world: WorldUniform,
}

#[derive(ShaderType, Clone, Debug)]
struct WorldUniform {
    time_aspect: Vec4,
    /// The sliders: Scale, Amplify and Terrace.
    params: Vec4,
}

impl UiMaterial for WorldMaterial {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(SHADER)
    }
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(RenderPlugin {
            // Compile a rewired graph's shader in the same frame, so the
            // landscape never blinks out while it builds.
            synchronous_pipeline_compilation: true,
            ..default()
        }))
        .add_plugins(FeathersPlugins)
        .add_plugins((NoodlePlugins, NoodleDefaultStylePlugin))
        .add_plugins(UiMaterialPlugin::<WorldMaterial>::default())
        .insert_resource(UiTheme(create_dark_theme()))
        .add_systems(Startup, setup)
        .add_systems(Update, (compile, tick))
        .add_observer(set_number)
        .run();
}

fn setup(mut commands: Commands, mut materials: ResMut<Assets<WorldMaterial>>) {
    commands.spawn(Camera2d);
    let material = materials.add(WorldMaterial {
        world: WorldUniform {
            time_aspect: Vec4::ZERO,
            params: Vec4::ONE,
        },
    });
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            ..default()
        },
        MaterialNode(material),
        GlobalZIndex(-1),
    ));
    let canvas = commands
        .spawn_scene(bsn! {
            @kit::canvas()
            Node { width: px(620), border: UiRect::right(px(1)) }
            CanvasGrid { background: Color::srgba(0.07, 0.08, 0.1, 0.78) }
            BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.1))
        })
        .id();
    let [position, scale, warp, ridges, amplify, biome, world, ..] = [
        (Op::Position, 12.0, 40.0),
        (Op::Scale(0.22), 132.0, 40.0),
        (Op::Warp, 252.0, 40.0),
        (Op::Ridges, 372.0, 40.0),
        (Op::Amplify(2.6), 492.0, 40.0),
        (Op::Biome, 372.0, 240.0),
        (Op::World, 492.0, 190.0),
        // Spare nodes to wire in.
        (Op::Time, 12.0, 240.0),
        (Op::Hills, 132.0, 240.0),
        (Op::Terrace(4.0), 252.0, 240.0),
        (Op::Add, 252.0, 380.0),
        (Op::Palette, 372.0, 380.0),
    ]
    .map(|(op, x, y)| spawn(&mut commands, canvas, op, Vec2::new(x, y)));
    let links = [
        (position, scale, 0),
        (scale, warp, 0),
        (warp, ridges, 0),
        (ridges, amplify, 0),
        (amplify, world, 0),
        (biome, world, 1),
    ];
    commands.queue(move |w: &mut World| {
        for link in links {
            let ports = |In((a, b, i)): In<(Entity, Entity, usize)>, g: GraphQuery| {
                Some((g.outputs_of(a).next()?, g.inputs_of(b).nth(i)?))
            };
            if let Ok(Some((from, to))) = w.run_system_cached_with(ports, link) {
                w.graph_edit(canvas, GraphEdit::Connect { from, to }).ok();
            }
        }
    });
}

fn spawn(commands: &mut Commands, canvas: Entity, op: Op, at: Vec2) -> Entity {
    let title = match op {
        Op::World => "World".to_string(),
        op => format!("{op:?}")
            .split('(')
            .next()
            .unwrap_or_default()
            .to_string(),
    };
    let mut rows: Vec<Box<dyn Scene>> = Vec::new();
    if let Some((value, min, max, _)) = op.number() {
        rows.push(Box::new(bsn! {
            @FeathersSlider { @min: min, @max: max }
            SliderValue(value)
            SliderStep(0.05)
            SliderPrecision(2)
            on(slider_self_update)
        }));
    }
    for (label, port_type, _) in op.inputs() {
        rows.push(Box::new(kit::row(
            *label,
            Port::input(*port_type),
            color(*port_type),
        )));
    }
    if let Some(port_type) = op.output() {
        rows.push(Box::new(kit::row(
            "out",
            Port::output(port_type),
            color(port_type),
        )));
    }
    let scene = bsn! {
        // Narrower than the kit's default, so five columns fit the panel.
        @kit::node(at) Node { min_width: px(110) }
        op
        ChildOf(canvas)
        Children [ @kit::title(title) -- {rows} ]
    };
    commands.spawn_scene(scene).id()
}

fn color(port_type: PortType) -> Color {
    if port_type == VEC2 {
        Color::srgb(0.95, 0.6, 0.25)
    } else if port_type == COLOR {
        Color::srgb(0.9, 0.45, 0.75)
    } else {
        Color::srgb(0.3, 0.75, 0.85)
    }
}

/// Builds the shader from the graph after an edit.
fn compile(
    mut edits: MessageReader<EditApplied>,
    graph: GraphQuery,
    ops: Query<(Entity, &'static Op)>,
    mut shaders: ResMut<Assets<Shader>>,
) {
    if edits.read().count() == 0 {
        return;
    }
    let world = ops.iter().find(|(_, op)| **op == Op::World).map(|(e, _)| e);
    let height = Codegen::new(&graph, &ops).body(world, 0, "0.3");
    let tint = Codegen::new(&graph, &ops).body(world, 1, "biome(h)");
    let source = TEMPLATE
        .replace("    // HEIGHT", &height)
        .replace("    // TINT", &tint);
    shaders
        .insert(&SHADER, Shader::from_wesl(source, "shader_world.wesl"))
        .ok();
}

/// A slider sets its node's number.
fn set_number(change: On<ValueChange<f32>>, graph: GraphQuery, mut ops: Query<&mut Op>) {
    if let Some(mut op) = graph
        .node_of(change.source)
        .and_then(|n| ops.get_mut(n).ok())
    {
        let new = op.with_number(change.value);
        op.set_if_neq(new);
    }
}

/// Time, aspect ratio and the sliders, every frame.
fn tick(
    time: Res<Time>,
    window: Single<&Window>,
    ops: Query<&Op>,
    nodes: Query<&MaterialNode<WorldMaterial>>,
    mut materials: ResMut<Assets<WorldMaterial>>,
) {
    let aspect = window.width() / window.height();
    let mut params = Vec4::ONE;
    for op in &ops {
        match op {
            Op::Scale(k) => params.x = *k,
            Op::Amplify(k) => params.y = *k,
            Op::Terrace(k) => params.z = *k,
            _ => {}
        }
    }
    for node in &nodes {
        if let Some(mut material) = materials.get_mut(&node.0) {
            material.world.time_aspect = Vec4::new(time.elapsed_secs(), aspect, 0.0, 0.0);
            material.world.params = params;
        }
    }
}
