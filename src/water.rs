//! Lake surface: depth-tinted translucent PBR mesh with animated Gerstner-like ripples.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use bevy::window::PrimaryWindow;
use bevy_egui::EguiContexts;

use crate::arc_core::*;
use crate::camera::EditorCamera;
use crate::terrain_render::{TerrainGrid, TerrainVersion};

/// A water body with its own surface level: the terrain basin that contains `seed` (world x, z), flooded to `level`
/// without touching the global `WorldSpec::water_level`. A level above the rim spills into whatever lies beyond it.
#[derive(Reflect, Clone, Debug, PartialEq)]
pub struct WaterBody {
    pub seed: [f32; 2],
    pub level: f32,
}

#[derive(Resource, Reflect, Default, Clone, Debug)]
#[reflect(Resource, Default)]
pub struct WaterBodies {
    pub items: Vec<WaterBody>,
    pub selected: Option<usize>,
    /// Next left click on the terrain selects (or creates) the body under it.
    pub picking: bool,
}

const NO_BODY: u16 = u16::MAX;

/// Owner body of every grid cell (`NO_BODY` = plain global water): 4-connected cells below `max(level, global)` from each seed.
pub fn body_owners(grid: &HeightfieldGrid, global: f32, bodies: &[WaterBody]) -> Vec<u16> {
    let mut owner = vec![NO_BODY; grid.n * grid.n];
    let m = grid.n as i32 - 1;
    let mut stack = Vec::new();
    for (b, body) in bodies.iter().enumerate() {
        let lim = body.level.max(global);
        let (gx, gz) = grid.world_to_grid(body.seed[0], body.seed[1]);
        let (si, sj) = ((gx.round() as i32).clamp(0, m), (gz.round() as i32).clamp(0, m));
        let ok = |o: &[u16], i: i32, j: i32| (0..=m).contains(&i) && (0..=m).contains(&j) && o[grid.idx(i as usize, j as usize)] == NO_BODY && grid.heights[grid.idx(i as usize, j as usize)] < lim;
        if !ok(&owner, si, sj) {
            continue;
        }
        owner[grid.idx(si as usize, sj as usize)] = b as u16;
        stack.push((si, sj));
        while let Some((i, j)) = stack.pop() {
            for (di, dj) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (ni, nj) = (i + di, j + dj);
                if ok(&owner, ni, nj) {
                    owner[grid.idx(ni as usize, nj as usize)] = b as u16;
                    stack.push((ni, nj));
                }
            }
        }
    }
    owner
}

/// Index of the body whose flooded basin covers world point (x, z).
pub fn body_at(grid: &HeightfieldGrid, global: f32, bodies: &[WaterBody], x: f32, z: f32) -> Option<usize> {
    let m = grid.n as i32 - 1;
    let (gx, gz) = grid.world_to_grid(x, z);
    let o = body_owners(grid, global, bodies)[grid.idx((gx.round() as i32).clamp(0, m) as usize, (gz.round() as i32).clamp(0, m) as usize)];
    (o != NO_BODY).then_some(o as usize)
}

/// Surface height at world (x, z): the owning body's level, else the global one (what navigation wades through).
pub fn surface_fn<'a>(grid: &'a HeightfieldGrid, global: f32, bodies: &'a [WaterBody]) -> impl Fn(f32, f32) -> f32 + 'a {
    let owners = if bodies.is_empty() { Vec::new() } else { body_owners(grid, global, bodies) };
    move |x, z| {
        let m = grid.n as i32 - 1;
        let (gx, gz) = grid.world_to_grid(x, z);
        let o = owners.get(grid.idx((gx.round() as i32).clamp(0, m) as usize, (gz.round() as i32).clamp(0, m) as usize)).copied().unwrap_or(NO_BODY);
        if o == NO_BODY { global } else { bodies[o as usize].level.max(global) }
    }
}

#[derive(Component)]
pub struct Water {
    nx: usize,
    nz: usize,
    origin: Vec2,
    cell: f32,
    /// Per-vertex surface offset from the global water level (non-zero inside bodies with their own level).
    base: Vec<f32>,
}

pub struct WaterPlugin;

impl Plugin for WaterPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WaterBodies>()
            .register_type::<WaterBodies>()
            .register_type::<WaterBody>()
            .add_systems(PostStartup, spawn_water)
            .add_systems(Update, (pick_water_body, rebuild_water, animate_water).chain());
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

fn build_water_mesh(spec: &WorldSpec, grid: &HeightfieldGrid, bodies: &[WaterBody]) -> (Mesh, Water) {
    let owner = body_owners(grid, spec.water_level, bodies);
    let level_of = |o: u16| if o == NO_BODY { spec.water_level } else { bodies[o as usize].level };
    let m = grid.n as i32 - 1;
    let level_at = |x: f32, z: f32| {
        let (gx, gz) = grid.world_to_grid(x, z);
        level_of(owner[grid.idx((gx.round() as i32).clamp(0, m) as usize, (gz.round() as i32).clamp(0, m) as usize)])
    };
    // bounding box of every cell that lies below its waterline, padded
    let (mut lo, mut hi) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
    for j in 0..grid.n {
        for i in 0..grid.n {
            if grid.heights[grid.idx(i, j)] < level_of(owner[grid.idx(i, j)]) + 0.3 {
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
    let mut base = Vec::with_capacity(nx * nz);
    for j in 0..nz {
        for i in 0..nx {
            let (x, z) = (lo.x + i as f32 * cell, lo.y + j as f32 * cell);
            let lvl = level_at(x, z);
            pos.push([x, lvl - spec.water_level, z]);
            base.push(lvl - spec.water_level);
            uv.push([i as f32 / nx as f32, j as f32 / nz as f32]);
            // depth gradient: shallow turquoise → deep blue, transparent on land
            let depth = lvl - grid.sample_height(x, z);
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
    (mesh, Water { nx, nz, origin: lo, cell, base })
}

fn spawn_water(mut commands: Commands, spec: Res<WorldSpec>, grid: Res<TerrainGrid>, bodies: Res<WaterBodies>, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let mat = mats.add(StandardMaterial {
        base_color: Color::WHITE, // tinted by per-vertex depth colour
        perceptual_roughness: 0.06,
        reflectance: 0.9,
        metallic: 0.0,
        opaque_render_method: bevy::material::OpaqueRendererMethod::Deferred, // required for ScreenSpaceReflections
        alpha_mode: AlphaMode::Opaque, // opaque so ScreenSpaceReflections applies; depth tint stands in for translucency
        ..default()
    });
    let (mesh, water) = build_water_mesh(&spec, &grid.0, &bodies.items);
    commands.spawn((Name::new("Water"), water, Mesh3d(meshes.add(mesh)), MeshMaterial3d(mat), Transform::from_xyz(0.0, spec.water_level, 0.0)));
}

/// Rebuild on a new terrain version, or (throttled) after sculpting changed the heights so dug basins fill and raised ones drain.
fn rebuild_water(
    time: Res<Time>,
    version: Res<TerrainVersion>,
    mut seen: Local<u32>,
    mut sculpt_dirty: Local<bool>,
    mut last: Local<f32>,
    mut snap: Local<Vec<WaterBody>>,
    spec: Res<WorldSpec>,
    grid: Res<TerrainGrid>,
    bodies: Res<WaterBodies>,
    mut q: Query<(&mut Water, &Mesh3d, &mut Transform)>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    // the UI holds the resource mutably, so is_changed() fires every frame it is open: compare the data instead
    let moved = *snap != bodies.items;
    if moved {
        snap.clone_from(&bodies.items);
    }
    *sculpt_dirty |= grid.is_changed() || moved;
    let sculpt_due = *sculpt_dirty && time.elapsed_secs() - *last >= 0.25;
    if *seen == version.0 && !sculpt_due {
        return;
    }
    *seen = version.0;
    *sculpt_dirty = false;
    *last = time.elapsed_secs();
    for (mut w, h, mut tf) in &mut q {
        let (mut mesh, nw) = build_water_mesh(&spec, &grid.0, &bodies.items);
        ripple(&mut mesh, &nw, time.elapsed_secs());
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
        ripple(&mut mesh, w, t);
    }
}

/// Write the animated surface into `mesh`; a freshly rebuilt mesh gets it too, so a rebuild never flashes a flat surface.
fn ripple(mesh: &mut Mesh, w: &Water, t: f32) {
    let mut pos = Vec::with_capacity(w.nx * w.nz);
    let mut nrm = Vec::with_capacity(w.nx * w.nz);
    for j in 0..w.nz {
        for i in 0..w.nx {
            let (x, z) = (w.origin.x + i as f32 * w.cell, w.origin.y + j as f32 * w.cell);
            let (y, dx, dz) = wave(x, z, t);
            pos.push([x, y + w.base[j * w.nx + i], z]);
            nrm.push(Vec3::new(-dx, 1.0, -dz).normalize().to_array());
        }
    }
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nrm);
}

/// Give the basin at world (x, z) its own water level (creating the body if needed); `None` removes the body again.
pub fn cmd_set_water_body(w: &mut World, x: f32, z: f32, level: Option<f32>) -> Result<String, String> {
    let (global, grid) = (w.resource::<WorldSpec>().water_level, w.resource::<TerrainGrid>().0.clone());
    let mut bodies = w.resource_mut::<WaterBodies>();
    let hit = body_at(&grid, global, &bodies.items, x, z);
    match (hit, level) {
        (Some(i), Some(l)) => bodies.items[i].level = l,
        (Some(i), None) => {
            bodies.items.remove(i);
            bodies.selected = None;
        }
        (None, Some(l)) => {
            bodies.items.push(WaterBody { seed: [x, z], level: l });
            bodies.selected = Some(bodies.items.len() - 1);
        }
        (None, None) => return Err("no water body there".into()),
    }
    Ok(format!("{} water bodies", bodies.items.len()))
}

/// While `picking`: a left click on the terrain selects the body under the cursor, or creates one there. Esc cancels. Also marks the selected body.
fn pick_water_body(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut egui: EguiContexts,
    cam: Single<(&Camera, &GlobalTransform), With<EditorCamera>>,
    spec: Res<WorldSpec>,
    grid: Res<TerrainGrid>,
    mut bodies: ResMut<WaterBodies>,
    mut gizmos: Gizmos,
) {
    if let Some(b) = bodies.selected.and_then(|i| bodies.items.get(i)) {
        let c = Vec3::new(b.seed[0], b.level + 0.15, b.seed[1]);
        let col = Color::srgb(1.0, 0.85, 0.3);
        gizmos.circle(Isometry3d::new(c, Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)), 2.5, col);
        gizmos.line(c, c + Vec3::Y * 6.0, col);
    }
    if !bodies.picking {
        return;
    }
    if keys.just_pressed(KeyCode::Escape) {
        bodies.picking = false;
        return;
    }
    let over_ui = egui.ctx_mut().map(|c| c.is_pointer_over_egui() || c.egui_wants_pointer_input()).unwrap_or(false);
    if !mouse.just_pressed(MouseButton::Left) || over_ui || !spec.terrain_enabled {
        return;
    }
    let (camera, gtf) = *cam;
    let Some(ray) = window.cursor_position().and_then(|c| camera.viewport_to_world(gtf, c).ok()) else { return };
    let Some(p) = grid.0.raycast(ray.origin, *ray.direction) else { return };
    bodies.picking = false;
    if let Some(i) = body_at(&grid.0, spec.water_level, &bodies.items, p.x, p.z) {
        bodies.selected = Some(i);
        return;
    }
    let h = grid.0.sample_height(p.x, p.z);
    let level = if h < spec.water_level { spec.water_level } else { h + 0.5 };
    bodies.items.push(WaterBody { seed: [p.x, p.z], level });
    bodies.selected = Some(bodies.items.len() - 1);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Flat 10 m plateau with a 4x4-cell moat dug to 7 m and a separate lake pit dug to 2 m.
    fn grid() -> HeightfieldGrid {
        let mut g = HeightfieldGrid::new(33, 32.0);
        g.heights.iter_mut().for_each(|h| *h = 10.0);
        for j in 4..8 {
            for i in 4..8 {
                let k = g.idx(i, j);
                g.heights[k] = 7.0;
            }
        }
        for j in 20..26 {
            for i in 20..26 {
                let k = g.idx(i, j);
                g.heights[k] = 2.0;
            }
        }
        g
    }

    #[test]
    fn body_floods_only_its_own_basin() {
        let g = grid();
        let (mx, mz) = g.grid_to_world(5, 5);
        let bodies = [WaterBody { seed: [mx, mz], level: 9.0 }];
        let o = body_owners(&g, 6.0, &bodies);
        assert_eq!(o.iter().filter(|&&b| b == 0).count(), 16, "moat basin only");
        assert_eq!(o[g.idx(22, 22)], NO_BODY, "the lake pit keeps the global level");
        assert_eq!(body_at(&g, 6.0, &bodies, mx, mz), Some(0));
    }

    #[test]
    fn level_above_the_rim_spills_over() {
        let g = grid();
        let (mx, mz) = g.grid_to_world(5, 5);
        let o = body_owners(&g, 6.0, &[WaterBody { seed: [mx, mz], level: 11.0 }]);
        assert!(o.iter().filter(|&&b| b == 0).count() > 16);
    }

    #[test]
    fn nav_wades_only_through_a_filled_moat() {
        use crate::navigation::{NavCell, NavGrid};
        let g = grid();
        let (mx, mz) = g.grid_to_world(5, 5);
        let bodies = [WaterBody { seed: [mx, mz], level: 9.0 }];
        let at = surface_fn(&g, 6.0, &bodies);
        assert_eq!((at(mx, mz), at(0.0, 0.0)), (9.0, 6.0));
        let dry = NavGrid::build(&g, 6.0, &[], 0.35);
        let wet = NavGrid::build_with(&g, &at, &[], 0.35);
        assert_eq!(dry.at(Vec2::new(mx, mz)), NavCell::Free);
        assert_ne!(wet.at(Vec2::new(mx, mz)), NavCell::Free);
        assert_eq!(wet.at(Vec2::ZERO), dry.at(Vec2::ZERO));
    }

    #[test]
    fn seed_on_dry_land_owns_nothing() {
        let g = grid();
        let o = body_owners(&g, 6.0, &[WaterBody { seed: [0.0, 0.0], level: 6.0 }]);
        assert!(o.iter().all(|&b| b == NO_BODY));
    }
}
