//! Terrain mesh: eroded heightfield → positions, smooth normals, splatmap vertex colours; live rebuild.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;

use crate::arc_core::*;

#[derive(Resource)]
pub struct TerrainGrid(pub HeightfieldGrid);

/// Incremented every time the terrain geometry is rebuilt; dependants (water, props) watch it.
#[derive(Resource, Default)]
pub struct TerrainVersion(pub u32);

#[derive(Component)]
pub struct TerrainMesh;

/// Heights to load into the grid after the next (re)generation (scene load).
#[derive(Resource, Default)]
pub struct PendingHeights(pub Option<Vec<f32>>);

/// The WorldSpec fields that change geometry (everything else is purely visual / lighting).
#[derive(Resource, Clone, PartialEq)]
struct GeoKey {
    terrain: (u32, u32, [u32; 7], u32, u32),
    lake: ([f32; 2], [f32; 2], [f32; 4]),
    biome: [f32; 4],
}

impl GeoKey {
    fn of(s: &WorldSpec) -> Self {
        Self {
            terrain: (
                s.seed,
                s.resolution,
                [s.size.to_bits(), s.base_height.to_bits(), s.mountain_height.to_bits(), s.max_height.to_bits(), s.noise_frequency.to_bits(), s.warp_amount.to_bits(), s.shore_foam.to_bits()],
                s.erosion_droplets,
                s.thermal_iterations,
            ),
            lake: (s.lake_center, s.camp_pos().to_array(), [s.lake_radius, s.lake_depth, s.water_level, s.clearing_radius]),
            biome: [s.forest_density, s.boulder_density, s.grass_density, 0.0],
        }
    }
}

pub struct TerrainRenderPlugin;

impl Plugin for TerrainRenderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TerrainVersion>().init_resource::<PendingHeights>().add_systems(Startup, setup_terrain).add_systems(Update, rebuild_on_spec_change);
    }
}

pub fn generate_world(spec: &WorldSpec) -> HeightfieldGrid {
    let mut grid = HeightfieldGrid::generate(spec);
    grid.erode_hydraulic(spec.erosion_droplets, spec.seed as u64);
    grid.erode_thermal(spec.thermal_iterations, 35.0);
    grid.clamp_heights(spec.max_height);
    grid
}

/// Cheap deterministic value noise used to break up flat vertex colours.
fn colour_jitter(x: f32, z: f32) -> f32 {
    let h = |a: f32, b: f32| ((a * 127.1 + b * 311.7).sin() * 43758.547).fract();
    let (fx, fz) = (x.floor(), z.floor());
    let (u, v) = ((x - fx) * (x - fx) * (3.0 - 2.0 * (x - fx)), (z - fz) * (z - fz) * (3.0 - 2.0 * (z - fz)));
    let a = h(fx, fz) * (1.0 - u) + h(fx + 1.0, fz) * u;
    let b = h(fx, fz + 1.0) * (1.0 - u) + h(fx + 1.0, fz + 1.0) * u;
    a * (1.0 - v) + b * v
}

pub fn build_terrain_mesh(g: &HeightfieldGrid, splats: &[Splat]) -> Mesh {
    let n = g.n;
    let mut pos = Vec::with_capacity(n * n);
    let mut nrm = Vec::with_capacity(n * n);
    let mut col = Vec::with_capacity(n * n);
    let mut uv = Vec::with_capacity(n * n);
    for j in 0..n {
        for i in 0..n {
            let (x, z) = g.grid_to_world(i, j);
            pos.push([x, g.heights[g.idx(i, j)], z]);
            nrm.push(g.sample_normal(x, z).to_array());
            let s = &splats[g.idx(i, j)];
            // two-octave tint variation so large splat areas do not look flat
            let t = 0.82 + 0.36 * (0.65 * colour_jitter(x * 0.35, z * 0.35) + 0.35 * colour_jitter(x * 1.9, z * 1.9));
            col.push([(s.albedo[0] * t).min(1.0), (s.albedo[1] * t).min(1.0), (s.albedo[2] * t).min(1.0), 1.0]);
            uv.push([i as f32 / (n - 1) as f32, j as f32 / (n - 1) as f32]);
        }
    }
    let mut idx = Vec::with_capacity((n - 1) * (n - 1) * 6);
    for j in 0..n - 1 {
        for i in 0..n - 1 {
            let (a, b, c, d) = ((j * n + i) as u32, (j * n + i + 1) as u32, ((j + 1) * n + i) as u32, ((j + 1) * n + i + 1) as u32);
            idx.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nrm)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col)
        .with_inserted_indices(Indices::U32(idx))
}

fn setup_terrain(mut commands: Commands, spec: Res<WorldSpec>, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let grid = generate_world(&spec);
    let splats = grid.build_splatmap(&spec);
    commands.spawn((
        Name::new("Terrain"),
        TerrainMesh,
        Mesh3d(meshes.add(build_terrain_mesh(&grid, &splats))),
        MeshMaterial3d(mats.add(StandardMaterial { base_color: Color::WHITE, perceptual_roughness: 0.88, reflectance: 0.2, ..default() })),
    ));
    commands.insert_resource(TerrainGrid(grid));
    commands.insert_resource(GeoKey::of(&spec));
}

/// Re-evaluate vertex data inside a dirty grid rectangle (+1 cell margin for normals / slope).
pub fn update_terrain_region(mesh: &mut Mesh, g: &HeightfieldGrid, spec: &WorldSpec, rect: (usize, usize, usize, usize)) {
    let n = g.n;
    let (i0, j0) = (rect.0.saturating_sub(1), rect.1.saturating_sub(1));
    let (i1, j1) = ((rect.2 + 1).min(n - 1), (rect.3 + 1).min(n - 1));
    let mut cells = Vec::with_capacity((i1 - i0 + 1) * (j1 - j0 + 1));
    for j in j0..=j1 {
        for i in i0..=i1 {
            let (x, z) = g.grid_to_world(i, j);
            let s = g.splat_at(i, j, spec);
            let t = 0.82 + 0.36 * (0.65 * colour_jitter(x * 0.35, z * 0.35) + 0.35 * colour_jitter(x * 1.9, z * 1.9));
            cells.push((g.idx(i, j), [x, g.heights[g.idx(i, j)], z], g.sample_normal(x, z).to_array(), [(s.albedo[0] * t).min(1.0), (s.albedo[1] * t).min(1.0), (s.albedo[2] * t).min(1.0), 1.0]));
        }
    }
    if let Some(VertexAttributeValues::Float32x3(p)) = mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION) {
        for c in &cells {
            p[c.0] = c.1;
        }
    }
    if let Some(VertexAttributeValues::Float32x3(p)) = mesh.attribute_mut(Mesh::ATTRIBUTE_NORMAL) {
        for c in &cells {
            p[c.0] = c.2;
        }
    }
    if let Some(VertexAttributeValues::Float32x4(p)) = mesh.attribute_mut(Mesh::ATTRIBUTE_COLOR) {
        for c in &cells {
            p[c.0] = c.3;
        }
    }
}

/// Geometry-relevant spec change (or pending scene heights) → regenerate heightfield + mesh; props/water follow via `TerrainVersion`.
fn rebuild_on_spec_change(
    spec: Res<WorldSpec>,
    mut key: ResMut<GeoKey>,
    mut pending: ResMut<PendingHeights>,
    mut grid: ResMut<TerrainGrid>,
    mut version: ResMut<TerrainVersion>,
    terrain: Query<&Mesh3d, With<TerrainMesh>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let has_pending = pending.0.is_some();
    if !spec.is_changed() && !has_pending {
        return;
    }
    let new = GeoKey::of(&spec);
    if new == *key && !has_pending {
        return;
    }
    let terrain_changed = new.terrain != key.terrain || new.lake != key.lake;
    *key = new;
    if terrain_changed {
        grid.0 = generate_world(&spec);
    }
    if let Some(h) = pending.0.take() {
        if h.len() == grid.0.heights.len() {
            grid.0.heights = h;
        }
    }
    if terrain_changed || has_pending {
        if let Ok(h) = terrain.single() {
            let splats = grid.0.build_splatmap(&spec);
            let _ = meshes.insert(&h.0, build_terrain_mesh(&grid.0, &splats));
        }
    }
    version.0 += 1;
}
