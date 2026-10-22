//! Lake surface: depth-tinted translucent PBR mesh with animated Gerstner-like ripples.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use crate::arc_core::*;
use crate::terrain_render::{TerrainGrid, TerrainVersion};

#[derive(Component)]
pub struct Water {
    nx: usize,
    nz: usize,
    origin: Vec2,
    cell: f32,
}

pub struct WaterPlugin;

impl Plugin for WaterPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostStartup, spawn_water).add_systems(Update, (rebuild_water, animate_water).chain());
    }
}

const WAVES: [(Vec2, f32, f32, f32); 4] = [
    // direction, wave number, speed, amplitude
    (Vec2::new(0.8, 0.6), 0.9, 0.9, 0.050),
    (Vec2::new(-0.5, 0.86), 1.5, 1.2, 0.028),
    (Vec2::new(0.2, -0.98), 2.3, 1.6, 0.016),
    (Vec2::new(-0.9, -0.4), 3.4, 2.0, 0.008),
];

/// (height, d/dx, d/dz) of the ripple field.
fn wave(x: f32, z: f32, t: f32) -> (f32, f32, f32) {
    let (mut h, mut dx, mut dz) = (0.0, 0.0, 0.0);
    for (d, k, w, a) in WAVES {
        let d = d.normalize();
        let ph = k * (d.x * x + d.y * z) - w * t;
        h += a * ph.sin();
        let c = a * k * ph.cos();
        dx += c * d.x;
        dz += c * d.y;
    }
    (h, dx, dz)
}

fn build_water_mesh(spec: &WorldSpec, grid: &HeightfieldGrid) -> (Mesh, Water) {
    // bounding box of every cell that lies below the waterline, padded
    let (mut lo, mut hi) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
    for j in 0..grid.n {
        for i in 0..grid.n {
            if grid.heights[grid.idx(i, j)] < spec.water_level + 0.3 {
                let (x, z) = grid.grid_to_world(i, j);
                lo = lo.min(Vec2::new(x, z));
                hi = hi.max(Vec2::new(x, z));
            }
        }
    }
    if lo.x > hi.x {
        lo = Vec2::splat(-1.0);
        hi = Vec2::splat(1.0);
    }
    let (lo, hi) = (lo - Vec2::splat(4.0), hi + Vec2::splat(4.0));
    let cell = ((hi - lo).max_element() / 220.0).max(0.8);
    let nx = ((hi.x - lo.x) / cell).ceil() as usize + 1;
    let nz = ((hi.y - lo.y) / cell).ceil() as usize + 1;

    let mut pos = Vec::with_capacity(nx * nz);
    let mut col = Vec::with_capacity(nx * nz);
    let mut uv = Vec::with_capacity(nx * nz);
    for j in 0..nz {
        for i in 0..nx {
            let (x, z) = (lo.x + i as f32 * cell, lo.y + j as f32 * cell);
            pos.push([x, 0.0, z]);
            uv.push([i as f32 / nx as f32, j as f32 / nz as f32]);
            // depth gradient: shallow turquoise → deep blue, transparent on land
            let depth = spec.water_level - grid.sample_height(x, z);
            let k = smoothstep(0.0, 5.0, depth);
            let a = smoothstep(-0.05, 0.9, depth) * 0.55 + smoothstep(0.9, 3.5, depth) * 0.42;
            let (sh, dp) = (Vec3::new(0.16, 0.50, 0.46), Vec3::new(0.008, 0.075, 0.17));
            let c = sh.lerp(dp, k);
            col.push([c.x, c.y, c.z, a]);
        }
    }
    let mut idx = Vec::with_capacity((nx - 1) * (nz - 1) * 6);
    for j in 0..nz - 1 {
        for i in 0..nx - 1 {
            let (a, b, c, d) = ((j * nx + i) as u32, (j * nx + i + 1) as u32, ((j + 1) * nx + i) as u32, ((j + 1) * nx + i + 1) as u32);
            idx.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }
    let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; nx * nz])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col)
        .with_inserted_indices(Indices::U32(idx));
    (mesh, Water { nx, nz, origin: lo, cell })
}

fn spawn_water(mut commands: Commands, spec: Res<WorldSpec>, grid: Res<TerrainGrid>, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let mat = mats.add(StandardMaterial {
        base_color: Color::WHITE, // tinted by per-vertex depth colour
        perceptual_roughness: 0.06,
        reflectance: 0.9,
        metallic: 0.0,
        opaque_render_method: bevy::material::OpaqueRendererMethod::Deferred, // required for ScreenSpaceReflections
        alpha_mode: AlphaMode::Opaque, // opaque so ScreenSpaceReflections applies; depth tint stands in for translucency
        ..default()
    });
    let (mesh, water) = build_water_mesh(&spec, &grid.0);
    commands.spawn((Name::new("Water"), water, Mesh3d(meshes.add(mesh)), MeshMaterial3d(mat), Transform::from_xyz(0.0, spec.water_level, 0.0)));
}

fn rebuild_water(
    version: Res<TerrainVersion>,
    mut seen: Local<u32>,
    spec: Res<WorldSpec>,
    grid: Res<TerrainGrid>,
    mut q: Query<(&mut Water, &Mesh3d, &mut Transform)>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    if *seen == version.0 {
        return;
    }
    *seen = version.0;
    for (mut w, h, mut tf) in &mut q {
        let (mesh, nw) = build_water_mesh(&spec, &grid.0);
        let _ = meshes.insert(&h.0, mesh);
        *w = nw;
        tf.translation.y = spec.water_level;
    }
}

fn animate_water(time: Res<Time>, q: Query<(&Water, &Mesh3d)>, mut meshes: ResMut<Assets<Mesh>>, mut last: Local<f32>) {
    let t = time.elapsed_secs();
    if t - *last < 1.0 / 30.0 {
        return;
    }
    *last = t;
    for (w, h) in &q {
        let Some(mut mesh) = meshes.get_mut(&h.0) else { continue };
        let mut pos = Vec::with_capacity(w.nx * w.nz);
        let mut nrm = Vec::with_capacity(w.nx * w.nz);
        for j in 0..w.nz {
            for i in 0..w.nx {
                let (x, z) = (w.origin.x + i as f32 * w.cell, w.origin.y + j as f32 * w.cell);
                let (y, dx, dz) = wave(x, z, t);
                pos.push([x, y, z]);
                nrm.push(Vec3::new(-dx, 1.0, -dz).normalize().to_array());
            }
        }
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nrm);
    }
}
