//! Blender-style geometry nodes, built from plain app code: wires carry
//! lists of points, and each node is a few lines turning one list into
//! another. Grid makes points, Trees queries the tree entities already in
//! the world, Wave moves each point, Near keeps the points close to its
//! second input, and Instance puts a cube on every point it gets. Rewire
//! them, and the cubes follow; the trees wander, and the cubes follow those
//! too.
//!
//! bevy_noodle has no built-in nodes: `Geo` and `points` below are the whole
//! node system. It is evaluated every frame, in an ordinary `Update` system,
//! because the wave animates and the trees move. An unconnected input is an
//! empty list, and a walk deeper than [`MAX_DEPTH`] gives nothing, so a
//! cycle can't recurse forever.
//!
//! ```sh
//! cargo run --example geometry_nodes --features default_style
//! ```

use bevy::picking::cursor::CursorIconPlugin;
use bevy::prelude::*;
use bevy_noodle::{prelude::*, style::kit};

const POINTS: PortType = PortType::named("points");
const TEAL: Color = Color::srgb(0.3, 0.8, 0.75);
const SKY: Color = Color::srgb(0.1, 0.11, 0.13);
/// How many nodes deep a walk back through inputs may go.
const MAX_DEPTH: u8 = 16;
/// Grid points per side, and the distance between them.
const GRID: usize = 24;
const SPACING: f32 = 0.5;
/// How close to a tree Near keeps a point.
const RADIUS: f32 = 2.2;

/// What a node does to the points flowing through it.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
enum Geo {
    /// A flat grid of points.
    Grid,
    /// A point under each tree in the world.
    Trees,
    /// Moves each point up and down in a travelling wave.
    Wave,
    /// Keeps the points near any point of its second input.
    Near,
    /// A cube on every point.
    Instance,
}

/// A tree wandering the ground; Trees reads where they are.
#[derive(Component)]
struct Tree(f32);

/// A cube Instance placed.
#[derive(Component)]
struct Cube;

type Cubes<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut Transform,
        &'static mut MeshMaterial3d<StandardMaterial>,
    ),
    (With<Cube>, Without<Tree>),
>;

/// The cube mesh, and cube materials from low to high.
#[derive(Resource)]
struct Look(Handle<Mesh>, Vec<Handle<StandardMaterial>>);

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, CursorIconPlugin))
        .add_plugins((NoodlePlugins, NoodleDefaultStylePlugin))
        .insert_resource(ClearColor(SKY))
        .add_systems(Startup, setup)
        .add_systems(Update, (wander, instance).chain())
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(-5.8, 13.0, 17.0).looking_at(Vec3::new(-5.8, 0.0, 0.0), Vec3::Y),
    ));
    commands.spawn((
        DirectionalLight {
            shadow_maps_enabled: true,
            illuminance: 6000.0,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(200.0, 200.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.16, 0.17, 0.2))),
        Transform::from_xyz(0.0, -0.8, 0.0),
    ));
    let (trunk, leaves) = (
        meshes.add(Cone::new(0.5, 2.6)),
        materials.add(Color::srgb(0.3, 0.7, 0.35)),
    );
    for phase in [0.0, 2.1, 4.2] {
        commands.spawn((
            Tree(phase),
            Mesh3d(trunk.clone()),
            MeshMaterial3d(leaves.clone()),
        ));
    }
    let shades = (0..8).map(|i| materials.add(Color::hsl(200.0 + 18.0 * i as f32, 0.7, 0.6)));
    commands.insert_resource(Look(meshes.add(Cuboid::from_length(0.4)), shades.collect()));

    // The graph: a translucent panel over the 3D view.
    let canvas = commands
        .spawn_scene(bsn! {
            @kit::canvas()
            Node { width: px(600), border: UiRect::right(px(1)) }
            BackgroundColor(Color::srgba(0.07, 0.08, 0.1, 0.82))
            BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.1))
        })
        .id();
    let [grid, trees, wave, near, instance] = [
        (Geo::Grid, 16.0, 40.0),
        (Geo::Trees, 16.0, 200.0),
        (Geo::Wave, 156.0, 40.0),
        (Geo::Near, 296.0, 100.0),
        (Geo::Instance, 436.0, 100.0),
    ]
    .map(|(geo, x, y)| spawn(&mut commands, canvas, geo, Vec2::new(x, y)));
    let links = [
        (grid, wave, 0),
        (wave, near, 0),
        (trees, near, 1),
        (near, instance, 0),
    ];
    commands.queue(move |world: &mut World| {
        for link in links {
            let ports = |In((a, b, i)): In<(Entity, Entity, usize)>, g: GraphQuery| {
                Some((g.outputs_of(a).next()?, g.inputs_of(b).nth(i)?))
            };
            if let Ok(Some((from, to))) = world.run_system_cached_with(ports, link) {
                world
                    .graph_edit(canvas, GraphEdit::Connect { from, to })
                    .ok();
            }
        }
    });
}

fn spawn(commands: &mut Commands, canvas: Entity, geo: Geo, at: Vec2) -> Entity {
    let title = format!("{geo:?}");
    let inputs: &[&str] = match geo {
        Geo::Grid | Geo::Trees => &[],
        Geo::Wave | Geo::Instance => &["points"],
        Geo::Near => &["points", "near"],
    };
    let mut rows: Vec<_> = inputs
        .iter()
        .map(|label| kit::row(*label, Port::input(POINTS), TEAL))
        .collect();
    if geo != Geo::Instance {
        rows.push(kit::row("points", Port::output(POINTS), TEAL));
    }
    let scene = bsn! {
        // Narrower than the kit's default, so four columns fit the panel.
        @kit::node(at) Node { min_width: px(120) }
        geo
        ChildOf(canvas)
        Children [ @kit::title(title) -- {rows} ]
    };
    commands.spawn_scene(scene).id()
}

/// The points leaving `node`, computed from its inputs and the world now.
fn points(
    graph: &GraphQuery,
    geos: &Query<(Entity, &Geo)>,
    trees: &Query<&Transform, With<Tree>>,
    node: Entity,
    time: f32,
    depth: u8,
) -> Vec<Vec3> {
    let input = |i| {
        let from = graph
            .source_of(node, i)
            .and_then(|output| graph.node_of(output));
        from.filter(|_| depth < MAX_DEPTH)
            .map_or_else(Vec::new, |from| {
                points(graph, geos, trees, from, time, depth + 1)
            })
    };
    match geos.get(node).map(|(_, geo)| geo) {
        Ok(Geo::Grid) => {
            let half = (GRID - 1) as f32 * SPACING / 2.0;
            let at = |i: usize| Vec3::new((i % GRID) as f32, 0.0, (i / GRID) as f32) * SPACING;
            (0..GRID * GRID)
                .map(|i| at(i) - Vec3::new(half, 0.0, half))
                .collect()
        }
        Ok(Geo::Trees) => trees.iter().map(|t| t.translation.with_y(0.0)).collect(),
        Ok(Geo::Wave) => input(0)
            .into_iter()
            .map(|p| p.with_y(0.6 * (0.9 * p.x + 0.5 * p.z - 2.5 * time).sin()))
            .collect(),
        Ok(Geo::Near) => {
            let near = input(1);
            let close = |p: &Vec3| near.iter().any(|q| p.xz().distance(q.xz()) < RADIUS);
            input(0).into_iter().filter(close).collect()
        }
        Ok(Geo::Instance) => input(0),
        Err(_) => Vec::new(),
    }
}

/// Puts a cube on every point reaching an Instance node, reusing cubes from
/// the last frame and coloring each by its height.
fn instance(
    mut commands: Commands,
    graph: GraphQuery,
    geos: Query<(Entity, &Geo)>,
    trees: Query<&Transform, With<Tree>>,
    mut cubes: Cubes,
    look: Res<Look>,
    time: Res<Time>,
) {
    let t = time.elapsed_secs();
    let sinks = geos.iter().filter(|(_, geo)| **geo == Geo::Instance);
    let mut placed = sinks.flat_map(|(node, _)| points(&graph, &geos, &trees, node, t, 0));
    let shade = |y: f32| {
        let i = ((y + 0.6) / 1.2 * look.1.len() as f32) as usize;
        look.1.get(i).or(look.1.last()).cloned().unwrap_or_default()
    };
    for (cube, mut transform, mut material) in &mut cubes {
        match placed.next() {
            Some(p) => {
                transform.translation = p;
                material.0 = shade(p.y);
            }
            None => commands.entity(cube).try_despawn(),
        }
    }
    for p in placed {
        commands.spawn((
            Cube,
            Mesh3d(look.0.clone()),
            MeshMaterial3d(shade(p.y)),
            Transform::from_translation(p),
        ));
    }
}

/// Trees drift around in slow loops.
fn wander(mut trees: Query<(&Tree, &mut Transform)>, time: Res<Time>) {
    let t = time.elapsed_secs() * 0.25;
    for (tree, mut transform) in &mut trees {
        let a = t + tree.0;
        transform.translation = Vec3::new(3.5 * a.cos(), 0.6, 2.5 * (1.3 * a).sin());
    }
}
