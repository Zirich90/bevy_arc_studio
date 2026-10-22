//! Pure math core: world passport, heightfield, erosion, splatmap, scatter, gizmo math, undo history.
//! No dependency on render ECS systems; unit-testable.

use std::collections::HashSet;
use std::hash::Hash;

use bevy::prelude::{EulerRot, Quat, Reflect, ReflectResource, Resource, Vec2, Vec3};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};

pub const SPLAT_LAYERS: usize = 6; // sand, gravel/shore, grass, moss, rock, snow

pub fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

// ───────────────────────────── WorldSpec ─────────────────────────────

#[derive(Reflect, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum QualityPreset {
    Low,
    #[default]
    Medium,
    High,
    Ultra,
}

#[derive(Reflect, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum WorldPreset {
    MistyDuskLake,
    SunnyAlpineNoon,
    MidnightCampfire,
    GoldenAutumnDawn,
}

#[derive(Resource, Reflect, Serialize, Deserialize, Clone, PartialEq, Debug)]
#[reflect(Resource)]
pub struct WorldSpec {
    // terrain
    pub seed: u32,
    pub size: f32,
    pub resolution: u32,
    pub base_height: f32,
    pub mountain_height: f32,
    pub max_height: f32,
    pub noise_frequency: f32,
    pub warp_amount: f32,
    // erosion
    pub erosion_droplets: u32,
    pub thermal_iterations: u32,
    // lake
    pub lake_center: [f32; 2],
    pub lake_radius: f32,
    pub lake_depth: f32,
    pub water_level: f32,
    pub shore_foam: f32,
    // camp
    pub camp_center: [f32; 2],
    pub clearing_radius: f32,
    // atmosphere
    pub time_of_day: f32,
    pub auto_sun: bool,
    pub sun_speed: f32, // hours per second while auto_sun
    pub sun_intensity: f32,
    pub fog_enabled: bool,
    pub fog_color: [f32; 3],
    pub fog_density: f32,
    pub god_rays: bool,
    pub bloom: bool,
    pub reflections: bool,
    pub shadows: bool,
    pub quality_preset: QualityPreset,
    // biome
    pub forest_density: f32,
    pub boulder_density: f32,
    pub grass_density: f32,
}

impl Default for WorldSpec {
    fn default() -> Self {
        Self {
            seed: 1337,
            size: 256.0,
            resolution: 257,
            base_height: 18.0,
            mountain_height: 60.0,
            max_height: 90.0,
            noise_frequency: 3.0,
            warp_amount: 0.12,
            erosion_droplets: 60_000,
            thermal_iterations: 24,
            lake_center: [-20.0, 14.0],
            lake_radius: 32.0,
            lake_depth: 5.0,
            water_level: 6.0,
            shore_foam: 0.8,
            camp_center: [38.0, -22.0],
            clearing_radius: 9.0,
            time_of_day: 17.5,
            auto_sun: false, // true = time_of_day advances automatically
            sun_speed: 0.15,
            sun_intensity: 1.0,
            fog_enabled: true,
            fog_color: [0.62, 0.68, 0.78],
            fog_density: 0.004,
            god_rays: true,
            bloom: true,
            reflections: true,
            shadows: true,
            quality_preset: QualityPreset::High,
            forest_density: 0.6,
            boulder_density: 0.4,
            grass_density: 0.5,
        }
    }
}

impl WorldSpec {
    pub fn from_preset(p: WorldPreset) -> Self {
        let mut s = Self::default();
        match p {
            WorldPreset::MistyDuskLake => {
                s.time_of_day = 20.8;
                s.fog_density = 0.007;
                s.fog_color = [0.52, 0.56, 0.68];
                s.sun_intensity = 0.7;
                s.forest_density = 0.7;
                s.shore_foam = 0.6;
            }
            WorldPreset::SunnyAlpineNoon => {
                s.time_of_day = 13.5;
                s.fog_density = 0.0018;
                s.fog_color = [0.70, 0.80, 0.95];
                s.mountain_height = 75.0;
                s.forest_density = 0.4;
                s.grass_density = 0.7;
            }
            WorldPreset::MidnightCampfire => {
                s.time_of_day = 23.5;
                s.fog_density = 0.006;
                s.fog_color = [0.03, 0.04, 0.09];
                s.sun_intensity = 0.4;
                s.god_rays = false;
                s.forest_density = 0.8;
            }
            WorldPreset::GoldenAutumnDawn => {
                s.time_of_day = 6.8;
                s.fog_density = 0.007;
                s.fog_color = [0.95, 0.72, 0.50];
                s.sun_intensity = 0.9;
                s.boulder_density = 0.5;
            }
        }
        s
    }

    /// Clamp all fields to legal ranges (used after UI / BRP edits).
    pub fn sanitize(&mut self) {
        self.time_of_day = self.time_of_day.rem_euclid(24.0);
        self.resolution = self.resolution.clamp(33, 1025);
        self.size = self.size.clamp(32.0, 4096.0);
        for v in [&mut self.forest_density, &mut self.boulder_density, &mut self.grass_density, &mut self.shore_foam] {
            *v = v.clamp(0.0, 1.0);
        }
    }
}

// ───────────────────────────── Heightfield ─────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BrushMode {
    Raise,
    Lower,
    Smooth,
    Flatten,
}

#[derive(Clone, Copy, Debug)]
pub struct Splat {
    pub weights: [f32; SPLAT_LAYERS],
    pub albedo: [f32; 3],
    pub roughness: f32,
}

#[derive(Clone, Debug)]
pub struct HeightfieldGrid {
    pub n: usize,
    pub size: f32,
    pub heights: Vec<f32>,
    pub erosion_mask: Vec<f32>,
}

/// Perlin fBm (value ≈ [-1,1]).
/// Lake radius along direction `rel` (from the lake centre), distorted by multi-octave noise (±35 %).
fn lake_shore_radius(np: &noise::Perlin, spec: &WorldSpec, rel: Vec2) -> f32 {
    let dir = rel.try_normalize().unwrap_or(Vec2::X);
    let n = fbm(np, dir.x as f64 * 1.6 + 40.0, dir.y as f64 * 1.6 + 40.0, 3);
    spec.lake_radius * (1.0 + n * 0.35 * 1.6).clamp(0.6, 1.5)
}

impl WorldSpec {
    /// Camp position: on the shore of the lake, in the direction of `camp_center` seen from `lake_center`.
    pub fn camp_pos(&self) -> Vec2 {
        let lake = Vec2::from(self.lake_center);
        let dir = (Vec2::from(self.camp_center) - lake).try_normalize().unwrap_or(Vec2::X);
        let np = noise::Perlin::new(self.seed);
        lake + dir * (lake_shore_radius(&np, self, dir) * 1.5 + self.clearing_radius * 0.6)
    }
}

fn fbm(noise: &noise::Perlin, x: f64, y: f64, octaves: u32) -> f32 {
    use noise::NoiseFn;
    let (mut sum, mut amp, mut freq, mut norm) = (0.0, 1.0, 1.0, 0.0);
    for _ in 0..octaves {
        sum += amp * noise.get([x * freq, y * freq]);
        norm += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    (sum / norm) as f32
}

const LAYER_ALBEDO: [[f32; 3]; SPLAT_LAYERS] = [
    [0.36, 0.30, 0.20], // wet sand
    [0.52, 0.50, 0.46], // gravel
    [0.16, 0.32, 0.08], // grass
    [0.07, 0.14, 0.06], // moss
    [0.20, 0.20, 0.22], // basalt
    [0.93, 0.95, 1.00], // snow
];
const LAYER_ROUGH: [f32; SPLAT_LAYERS] = [0.35, 0.55, 0.85, 0.80, 0.75, 0.45];

impl HeightfieldGrid {
    pub fn new(n: usize, size: f32) -> Self {
        Self { n, size, heights: vec![0.0; n * n], erosion_mask: vec![0.0; n * n] }
    }

    pub fn cell(&self) -> f32 {
        self.size / (self.n as f32 - 1.0)
    }

    #[inline]
    pub fn idx(&self, i: usize, j: usize) -> usize {
        j * self.n + i
    }

    pub fn world_to_grid(&self, x: f32, z: f32) -> (f32, f32) {
        ((x / self.size + 0.5) * (self.n as f32 - 1.0), (z / self.size + 0.5) * (self.n as f32 - 1.0))
    }

    pub fn grid_to_world(&self, i: usize, j: usize) -> (f32, f32) {
        let k = self.size / (self.n as f32 - 1.0);
        (i as f32 * k - self.size * 0.5, j as f32 * k - self.size * 0.5)
    }

    /// Domain-warped fBm valley with lake bowl, camp clearing and path.
    pub fn generate(spec: &WorldSpec) -> Self {
        let n = spec.resolution as usize;
        let mut g = Self::new(n, spec.size);
        let np = noise::Perlin::new(spec.seed);
        let (o1, o2) = (17.3_f64, 91.7_f64);
        let s = spec.size;
        let lake_c = Vec2::from(spec.lake_center);
        let camp_c = spec.camp_pos();
        let lake_floor = spec.water_level - spec.lake_depth;
        // camp height: raw terrain height at the camp centre, lifted above water
        let raw = |x: f32, z: f32| -> f32 {
            let p = Vec2::new(x, z) / s;
            let f = spec.noise_frequency as f64;
            let w = |ox: f64| fbm(&np, (p.x as f64) * f + ox, (p.y as f64) * f + ox * 0.5, 4);
            let q = p + spec.warp_amount * Vec2::new(w(o1), w(o2));
            let r = p.length() * 2.0;
            let qf = |m: f64| fbm(&np, (q.x as f64) * f * m, (q.y as f64) * f * m, 6);
            let base = 0.5 + 0.5 * qf(1.0);
            let ring = smoothstep(0.55, 0.95, r);
            base * spec.base_height + ring * spec.mountain_height * (0.6 + 0.4 * qf(2.0))
        };
        let h_camp = raw(camp_c.x, camp_c.y).max(spec.water_level + 1.5).min(spec.base_height + 6.0);
        let (px0, pamp, pk, pphi) = (camp_c.x, 14.0, 0.035, 0.7);
        for j in 0..n {
            for i in 0..n {
                let (x, z) = g.grid_to_world(i, j);
                let pos = Vec2::new(x, z);
                let mut h = raw(x, z);
                // organic shoreline: distance rescaled by the angular radius distortion
                let rel = pos - lake_c;
                let d = rel.length() * spec.lake_radius / lake_shore_radius(&np, spec, rel);
                // everywhere outside the lake stays above the waterline (smooth max, no second lake)
                let floor = spec.water_level + 1.0;
                let k = 1.5;
                h = floor + k * (1.0 + ((h - floor) / k).exp()).ln();
                let bowl = 1.0 - smoothstep(0.0, spec.lake_radius, d);
                h -= bowl * spec.lake_depth;
                let shallow = smoothstep(spec.lake_radius, spec.lake_radius * 1.35, d);
                h = lerp(lake_floor + 0.3, h, shallow);
                // path from the camp, sinusoidal centreline in x over z
                let xc = px0 + pamp * (pk * z + pphi).sin();
                let wp = 2.0;
                let wpath = (1.0 - smoothstep(wp, 2.0 * wp, (x - xc).abs())) * (1.0 - smoothstep(0.0, 70.0, (z - camp_c.y).abs()));
                h = lerp(h, h_camp, 0.7 * wpath);
                // camp clearing (applied last so it stays flat)
                let w = 1.0 - smoothstep(spec.clearing_radius, spec.clearing_radius * 1.6, pos.distance(camp_c));
                h = lerp(h, h_camp, w);
                let idx = g.idx(i, j);
                g.heights[idx] = h.clamp(0.0, spec.max_height);
            }
        }
        g
    }

    pub fn clamp_heights(&mut self, max_h: f32) {
        for h in &mut self.heights {
            *h = h.clamp(0.0, max_h);
        }
    }

    // ── sampling ──

    fn h_at(&self, i: i32, j: i32) -> f32 {
        let m = self.n as i32 - 1;
        self.heights[self.idx(i.clamp(0, m) as usize, j.clamp(0, m) as usize)]
    }

    /// Bilinear height in grid coordinates.
    pub fn height_grid(&self, gx: f32, gz: f32) -> f32 {
        let m = self.n as f32 - 1.0;
        let (gx, gz) = (gx.clamp(0.0, m), gz.clamp(0.0, m));
        let (i, j) = (gx.floor() as i32, gz.floor() as i32);
        let (fx, fz) = (gx - i as f32, gz - j as f32);
        let a = lerp(self.h_at(i, j), self.h_at(i + 1, j), fx);
        let b = lerp(self.h_at(i, j + 1), self.h_at(i + 1, j + 1), fx);
        lerp(a, b, fz)
    }

    pub fn sample_height(&self, x: f32, z: f32) -> f32 {
        let (gx, gz) = self.world_to_grid(x, z);
        self.height_grid(gx, gz)
    }

    pub fn sample_normal(&self, x: f32, z: f32) -> Vec3 {
        let e = self.cell();
        let dx = self.sample_height(x + e, z) - self.sample_height(x - e, z);
        let dz = self.sample_height(x, z + e) - self.sample_height(x, z - e);
        Vec3::new(-dx / (2.0 * e), 1.0, -dz / (2.0 * e)).normalize()
    }

    fn grad_cells(&self, gx: f32, gz: f32) -> (f32, f32) {
        let (i, j) = (gx.floor() as i32, gz.floor() as i32);
        let (fx, fz) = (gx - i as f32, gz - j as f32);
        let (h00, h10, h01, h11) = (self.h_at(i, j), self.h_at(i + 1, j), self.h_at(i, j + 1), self.h_at(i + 1, j + 1));
        (lerp(h10 - h00, h11 - h01, fz), lerp(h01 - h00, h11 - h10, fx))
    }

    // ── erosion ──

    pub fn erode_hydraulic(&mut self, droplets: u32, seed: u64) {
        const INERTIA: f32 = 0.05;
        const K_C: f32 = 4.0;
        const MIN_SLOPE: f32 = 0.01;
        const K_E: f32 = 0.3;
        const K_D: f32 = 0.3;
        const K_V: f32 = 0.01;
        const G: f32 = 4.0;
        const LIFE: u32 = 30;
        const R: i32 = 3;
        let mut rng = StdRng::seed_from_u64(seed);
        let lim = self.n as f32 - 3.0;
        for _ in 0..droplets {
            let (mut px, mut pz) = (rng.gen_range(1.0..lim), rng.gen_range(1.0..lim));
            let (mut dx, mut dz, mut speed, mut water, mut sed) = (0.0_f32, 0.0_f32, 1.0_f32, 1.0_f32, 0.0_f32);
            for _ in 0..LIFE {
                let h_old = self.height_grid(px, pz);
                let (gx, gz) = self.grad_cells(px, pz);
                dx = dx * INERTIA - gx * (1.0 - INERTIA);
                dz = dz * INERTIA - gz * (1.0 - INERTIA);
                let len = (dx * dx + dz * dz).sqrt();
                if len < 1e-6 {
                    let a = rng.gen_range(0.0..std::f32::consts::TAU);
                    dx = a.cos();
                    dz = a.sin();
                } else {
                    dx /= len;
                    dz /= len;
                }
                let (ox, oz) = (px, pz);
                px += dx;
                pz += dz;
                if px < 1.0 || pz < 1.0 || px > lim || pz > lim {
                    break;
                }
                let dh = self.height_grid(px, pz) - h_old;
                let cap = (-dh).max(MIN_SLOPE) * speed * water * K_C;
                if sed > cap || dh > 0.0 {
                    let dep = if dh > 0.0 { dh.min(sed) } else { (sed - cap) * K_D };
                    sed -= dep;
                    let (i, j) = (ox.floor() as usize, oz.floor() as usize);
                    let (fx, fz) = (ox - i as f32, oz - j as f32);
                    let n = self.n;
                    self.heights[j * n + i] += dep * (1.0 - fx) * (1.0 - fz);
                    self.heights[j * n + i + 1] += dep * fx * (1.0 - fz);
                    self.heights[(j + 1) * n + i] += dep * (1.0 - fx) * fz;
                    self.heights[(j + 1) * n + i + 1] += dep * fx * fz;
                } else {
                    let ero = ((cap - sed) * K_E).min(-dh);
                    let (ci, cj) = (ox.round() as i32, oz.round() as i32);
                    let mut wsum = 0.0;
                    for b in -R..=R {
                        for a in -R..=R {
                            wsum += (1.0 - ((a * a + b * b) as f32).sqrt() / R as f32).max(0.0);
                        }
                    }
                    let n = self.n as i32;
                    for b in -R..=R {
                        for a in -R..=R {
                            let w = (1.0 - ((a * a + b * b) as f32).sqrt() / R as f32).max(0.0) / wsum;
                            let (ii, jj) = (ci + a, cj + b);
                            if w > 0.0 && ii >= 0 && jj >= 0 && ii < n && jj < n {
                                let k = (jj * n + ii) as usize;
                                self.heights[k] -= ero * w;
                                self.erosion_mask[k] += ero * w;
                            }
                        }
                    }
                    sed += ero;
                }
                speed = (speed * speed + dh * G).max(0.0).sqrt();
                water *= 1.0 - K_V;
            }
        }
    }

    pub fn erode_thermal(&mut self, iterations: u32, talus_deg: f32) {
        let n = self.n;
        let c = self.cell();
        let tan_t = talus_deg.to_radians().tan();
        for _ in 0..iterations {
            let mut delta = vec![0.0_f32; n * n];
            for j in 0..n {
                for i in 0..n {
                    let hc = self.heights[j * n + i];
                    for (di, dj) in [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)] {
                        let (ii, jj) = (i as i32 + di, j as i32 + dj);
                        if ii < 0 || jj < 0 || ii >= n as i32 || jj >= n as i32 {
                            continue;
                        }
                        let k = jj as usize * n + ii as usize;
                        let dist = c * ((di * di + dj * dj) as f32).sqrt();
                        let dh = hc - self.heights[k];
                        if dh / dist > tan_t {
                            let m = 0.5 * (dh - dist * tan_t) * 0.5;
                            delta[j * n + i] -= m * 0.5; // 8 neighbours share the flow
                            delta[k] += m * 0.5;
                        }
                    }
                }
            }
            for (h, d) in self.heights.iter_mut().zip(delta) {
                *h += d;
            }
        }
    }

    // ── splatmap ──

    pub fn splat_weights(&self, i: usize, j: usize, spec: &WorldSpec) -> [f32; SPLAT_LAYERS] {
        let (x, z) = self.grid_to_world(i, j);
        let h = self.heights[self.idx(i, j)];
        let nrm = self.sample_normal(x, z);
        let slope = 1.0 - nrm.y;
        let d = h - spec.water_level;
        let h_n = h / spec.max_height;
        let moss_mask = (self.erosion_mask[self.idx(i, j)] * 1.5).clamp(0.0, 1.0);
        let w_sand = (1.0 - smoothstep(0.0, 0.6, d)) * if d > -0.3 { 1.0 } else { 0.0 };
        let w_shore = smoothstep(0.0, 0.3, d) * (1.0 - smoothstep(0.6, 1.4, d));
        let w_rock = smoothstep(0.32, 0.42, slope);
        let w_snow = smoothstep(0.78, 0.9, h_n + 0.05) * (1.0 - w_rock * 0.7);
        let w_moss = moss_mask * (1.0 - w_rock);
        let max_o = w_sand.max(w_shore).max(w_rock).max(w_snow).max(w_moss);
        let w_grass = 1.0 - max_o;
        let mut w = [w_sand, w_shore, w_grass, w_moss, w_rock, w_snow];
        let sum: f32 = w.iter().sum::<f32>().max(1e-6);
        for v in &mut w {
            *v /= sum;
        }
        w
    }

    pub fn splat_at(&self, i: usize, j: usize, spec: &WorldSpec) -> Splat {
        let weights = self.splat_weights(i, j, spec);
        let d = self.heights[self.idx(i, j)] - spec.water_level;
        let mut albedo = [0.0; 3];
        let mut roughness = 0.0;
        for (l, w) in weights.iter().enumerate() {
            let mut a = LAYER_ALBEDO[l];
            if l == 1 {
                let foam = (1.0 - smoothstep(0.0, 0.15, d.abs())) * spec.shore_foam;
                for c in 0..3 {
                    a[c] = lerp(a[c], [0.90, 0.92, 0.92][c], foam);
                }
            }
            for c in 0..3 {
                albedo[c] += w * a[c];
            }
            roughness += w * LAYER_ROUGH[l];
        }
        Splat { weights, albedo, roughness }
    }

    pub fn build_splatmap(&self, spec: &WorldSpec) -> Vec<Splat> {
        let mut out = Vec::with_capacity(self.n * self.n);
        for j in 0..self.n {
            for i in 0..self.n {
                out.push(self.splat_at(i, j, spec));
            }
        }
        out
    }

    fn nearest_cell(&self, x: f32, z: f32) -> (usize, usize) {
        let (gx, gz) = self.world_to_grid(x, z);
        let m = self.n as f32 - 1.0;
        (gx.round().clamp(0.0, m) as usize, gz.round().clamp(0.0, m) as usize)
    }

    // ── brushes ──

    /// Apply one dab. Returns dirty rectangle (i0, j0, i1, j1) inclusive.
    pub fn apply_brush(&mut self, mode: BrushMode, x: f32, z: f32, radius: f32, strength: f32, dt: f32, flatten_target: f32) -> (usize, usize, usize, usize) {
        let (cx, cz) = self.world_to_grid(x, z);
        let rg = radius / self.cell();
        let m = self.n as i32 - 1;
        let i0 = ((cx - rg).floor() as i32).clamp(0, m);
        let i1 = ((cx + rg).ceil() as i32).clamp(0, m);
        let j0 = ((cz - rg).floor() as i32).clamp(0, m);
        let j1 = ((cz + rg).ceil() as i32).clamp(0, m);
        let mut updates = Vec::new();
        for j in j0..=j1 {
            for i in i0..=i1 {
                let (wx, wz) = self.grid_to_world(i as usize, j as usize);
                let d = ((wx - x).powi(2) + (wz - z).powi(2)).sqrt();
                let f = 1.0 - smoothstep(0.0, radius, d);
                if f <= 0.0 {
                    continue;
                }
                let h = self.h_at(i, j);
                let nh = match mode {
                    BrushMode::Raise => h + strength * f * dt,
                    BrushMode::Lower => h - strength * f * dt,
                    BrushMode::Smooth => {
                        let mut s = 0.0;
                        for b in -1..=1 {
                            for a in -1..=1 {
                                s += self.h_at(i + a, j + b);
                            }
                        }
                        h + (s / 9.0 - h) * (strength * f).min(1.0)
                    }
                    BrushMode::Flatten => h + (flatten_target - h) * (strength * f).min(1.0),
                };
                updates.push((self.idx(i as usize, j as usize), nh));
            }
        }
        for (k, v) in updates {
            self.heights[k] = v;
        }
        (i0 as usize, j0 as usize, i1 as usize, j1 as usize)
    }

    // ── raycast ──

    /// March along the ray then refine with 10 bisection steps. Returns world hit point.
    pub fn raycast(&self, origin: Vec3, dir: Vec3) -> Option<Vec3> {
        let dir = dir.normalize_or_zero();
        if dir == Vec3::ZERO {
            return None;
        }
        let half = self.size * 0.5;
        let step = self.cell() * 0.5;
        let t_max = self.size * 3.0;
        let inside = |p: Vec3| p.x.abs() <= half && p.z.abs() <= half;
        let above = |p: Vec3| p.y - self.sample_height(p.x, p.z);
        let mut t_prev = 0.0;
        let mut prev_above: Option<bool> = None;
        let mut t = 0.0;
        while t <= t_max {
            let p = origin + dir * t;
            if inside(p) {
                let a = above(p) > 0.0;
                if let Some(pa) = prev_above {
                    if pa && !a {
                        let (mut lo, mut hi) = (t_prev, t);
                        for _ in 0..10 {
                            let mid = 0.5 * (lo + hi);
                            if above(origin + dir * mid) > 0.0 {
                                lo = mid;
                            } else {
                                hi = mid;
                            }
                        }
                        let q = origin + dir * (0.5 * (lo + hi));
                        return Some(Vec3::new(q.x, self.sample_height(q.x, q.z), q.z));
                    }
                } else if !a {
                    return None; // starts below ground
                }
                prev_above = Some(a);
            } else {
                prev_above = None;
            }
            t_prev = t;
            t += step;
        }
        None
    }
}

// ───────────────────────────── Scatter ─────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScatterKind {
    Pine,
    Boulder,
    Grass,
    Reed,
    /// Wooden pier: `pos` = shore end (deck height), local +X runs towards the lake, `scale` = length in metres.
    WoodenPier,
    /// Fire ring, logs, benches and tent around the camp centre.
    CampfireSite,
}

#[derive(Clone, Copy, Debug)]
pub struct ScatterItem {
    pub kind: ScatterKind,
    pub pos: Vec3,
    pub rot: Quat,
    pub scale: f32,
}

/// Bridson Poisson-disk sampling over a square [-half, half]². `accept` filters candidates.
pub fn poisson_disk(rng: &mut StdRng, half: f32, r: f32, k: u32, max_pts: usize, accept: &mut dyn FnMut(Vec2, &mut StdRng) -> bool) -> Vec<Vec2> {
    let cell = r / std::f32::consts::SQRT_2;
    let gn = ((2.0 * half / cell).ceil() as usize).max(1);
    let mut grid = vec![-1_i32; gn * gn];
    let to_cell = |p: Vec2| (((p.x + half) / cell) as usize).min(gn - 1);
    let to_cell_z = |p: Vec2| (((p.y + half) / cell) as usize).min(gn - 1);
    let mut pts: Vec<Vec2> = Vec::new();
    let mut active: Vec<usize> = Vec::new();
    let mut tries = 0;
    while pts.is_empty() && tries < 200 {
        tries += 1;
        let p = Vec2::new(rng.gen_range(-half..half), rng.gen_range(-half..half));
        if accept(p, rng) {
            grid[to_cell_z(p) * gn + to_cell(p)] = 0;
            pts.push(p);
            active.push(0);
        }
    }
    while !active.is_empty() && pts.len() < max_pts {
        let ai = rng.gen_range(0..active.len());
        let base = pts[active[ai]];
        let mut found = false;
        for _ in 0..k {
            let ang = rng.gen_range(0.0..std::f32::consts::TAU);
            let rad = rng.gen_range(r..2.0 * r);
            let c = base + Vec2::new(ang.cos(), ang.sin()) * rad;
            if c.x.abs() >= half || c.y.abs() >= half {
                continue;
            }
            let (ci, cj) = (to_cell(c) as i32, to_cell_z(c) as i32);
            let mut ok = true;
            'n: for b in (cj - 2).max(0)..=(cj + 2).min(gn as i32 - 1) {
                for a in (ci - 2).max(0)..=(ci + 2).min(gn as i32 - 1) {
                    let g = grid[b as usize * gn + a as usize];
                    if g >= 0 && pts[g as usize].distance(c) < r {
                        ok = false;
                        break 'n;
                    }
                }
            }
            if ok && accept(c, rng) {
                grid[cj as usize * gn + ci as usize] = pts.len() as i32;
                active.push(pts.len());
                pts.push(c);
                found = true;
                break;
            }
        }
        if !found {
            active.swap_remove(ai);
        }
    }
    pts
}

fn ground_rot(normal: Vec3, yaw: f32, tilt: f32) -> Quat {
    Quat::from_rotation_arc(Vec3::Y, Vec3::Y.lerp(normal, tilt).normalize()) * Quat::from_rotation_y(yaw)
}

pub fn generate_scatter_plan(spec: &WorldSpec, grid: &HeightfieldGrid) -> Vec<ScatterItem> {
    let mut rng = StdRng::seed_from_u64(spec.seed as u64 ^ 0x9E37_79B9);
    let half = grid.size * 0.5 - 2.0;
    let camp = spec.camp_pos();
    let lake = Vec2::from(spec.lake_center);
    let mut items: Vec<ScatterItem> = Vec::new();

    let slope_ok = |x: f32, z: f32| 1.0 - grid.sample_normal(x, z).y < 0.35;
    let weights_at = |p: Vec2| {
        let (i, j) = grid.nearest_cell(p.x, p.y);
        grid.splat_weights(i, j, spec)
    };

    // pines: grass/moss density mask, away from camp and water
    let r_pine = lerp(14.0, 4.5, spec.forest_density);
    let mut pines: Vec<Vec2> = Vec::new();
    if spec.forest_density > 0.0 {
        pines = poisson_disk(&mut rng, half, r_pine, 30, 2500, &mut |p, rng| {
            let h = grid.sample_height(p.x, p.y);
            if !slope_ok(p.x, p.y) || h < spec.water_level + 0.2 || p.distance(camp) < spec.clearing_radius * 1.5 {
                return false;
            }
            if p.distance(lake) < spec.lake_radius * 1.2 {
                return false;
            }
            let w = weights_at(p);
            rng.gen_range(0.0..1.0) < (w[2] + w[3]).clamp(0.05, 1.0)
        });
        for p in &pines {
            let n = grid.sample_normal(p.x, p.y);
            let s = rng.gen_range(0.8..1.5);
            items.push(ScatterItem { kind: ScatterKind::Pine, pos: Vec3::new(p.x, grid.sample_height(p.x, p.y), p.y), rot: ground_rot(n, rng.gen_range(0.0..std::f32::consts::TAU), 0.15), scale: s });
        }
    }

    // boulders: prefer rock weight, spacing against pines
    if spec.boulder_density > 0.0 {
        let r_b = lerp(22.0, 8.0, spec.boulder_density);
        let pts = poisson_disk(&mut rng, half, r_b, 30, 400, &mut |p, rng| {
            let h = grid.sample_height(p.x, p.y);
            if h < spec.water_level - 0.3 || p.distance(camp) < spec.clearing_radius * 1.2 {
                return false;
            }
            if pines.iter().any(|q| q.distance(p) < r_b.max(r_pine) * 0.35) {
                return false;
            }
            let w = weights_at(p);
            rng.gen_range(0.0..1.0) < 0.2 + w[4]
        });
        for p in pts {
            let n = grid.sample_normal(p.x, p.y);
            items.push(ScatterItem { kind: ScatterKind::Boulder, pos: Vec3::new(p.x, grid.sample_height(p.x, p.y) - 0.15, p.y), rot: ground_rot(n, rng.gen_range(0.0..std::f32::consts::TAU), 0.6), scale: rng.gen_range(0.6..2.4) });
        }
    }

    // grass tufts
    if spec.grass_density > 0.0 {
        let r_g = lerp(4.0, 1.4, spec.grass_density);
        let pts = poisson_disk(&mut rng, half, r_g, 20, 4000, &mut |p, rng| {
            let h = grid.sample_height(p.x, p.y);
            if h < spec.water_level + 0.6 || !slope_ok(p.x, p.y) {
                return false;
            }
            let w = weights_at(p);
            rng.gen_range(0.0..1.0) < w[2]
        });
        for p in pts {
            let n = grid.sample_normal(p.x, p.y);
            items.push(ScatterItem { kind: ScatterKind::Grass, pos: Vec3::new(p.x, grid.sample_height(p.x, p.y), p.y), rot: ground_rot(n, rng.gen_range(0.0..std::f32::consts::TAU), 0.5), scale: rng.gen_range(0.6..1.3) });
        }
    }

    // reeds along the shoreline
    let pts = poisson_disk(&mut rng, half, 1.6, 20, 600, &mut |p, _| {
        let d = grid.sample_height(p.x, p.y) - spec.water_level;
        (-0.3..0.5).contains(&d)
    });
    for p in pts {
        items.push(ScatterItem { kind: ScatterKind::Reed, pos: Vec3::new(p.x, grid.sample_height(p.x, p.y), p.y), rot: Quat::from_rotation_y(rng.gen_range(0.0..std::f32::consts::TAU)), scale: rng.gen_range(0.8..1.6) });
    }

    // pier: from the camp-side shore towards the lake centre until the water is deep enough
    let to_camp = (camp - lake).normalize_or_zero();
    let dir = if to_camp == Vec2::ZERO { Vec2::X } else { to_camp };
    let mut shore: Option<f32> = None;
    let mut s = spec.lake_radius * 1.6;
    let mut end = None;
    while s > 0.0 {
        let p = lake + dir * s;
        let d = grid.sample_height(p.x, p.y) - spec.water_level;
        if shore.is_none() && d < 0.35 {
            shore = Some(s);
        }
        if let Some(s0) = shore {
            if d < -2.6 || s0 - s > 20.0 {
                end = Some(s);
                break;
            }
        }
        s -= 0.5;
    }
    if let (Some(s0), Some(s1)) = (shore, end) {
        let p = lake + dir * (s0 + 1.2); // start slightly on land
        let yaw = dir.y.atan2(-dir.x);
        items.push(ScatterItem { kind: ScatterKind::WoodenPier, pos: Vec3::new(p.x, spec.water_level + 0.55, p.y), rot: Quat::from_rotation_y(yaw), scale: (s0 - s1 + 1.2).max(10.0) });
    }

    // camp site (one composite prefab)
    let cy = grid.sample_height(camp.x, camp.y);
    items.push(ScatterItem { kind: ScatterKind::CampfireSite, pos: Vec3::new(camp.x, cy, camp.y), rot: Quat::IDENTITY, scale: 1.0 });
    items
}

// ───────────────────────────── Gizmo math (doc 02) ─────────────────────────────

/// Constant screen-space handle size.
pub fn calc_handle_size(eye: Vec3, origin: Vec3, cam_forward: Vec3, fov_y: f32) -> f32 {
    (eye - origin).dot(cam_forward).abs() * (fov_y * 0.5).tan() * 0.16
}

fn ray_plane(o: Vec3, d: Vec3, plane_o: Vec3, n: Vec3) -> Option<f32> {
    let denom = d.dot(n);
    if denom.abs() < 1e-8 {
        return None;
    }
    let s = (plane_o - o).dot(n) / denom;
    (s > 0.0).then_some(s)
}

/// Parameter along `axis` where the ray meets the camera-facing helper plane. `None` in the dead zone.
pub fn axis_hit_param(origin: Vec3, axis: Vec3, view_dir: Vec3, ray_o: Vec3, ray_d: Vec3) -> Option<f32> {
    if axis.dot(view_dir).abs() > 0.96 {
        return None;
    }
    let n = axis.cross(view_dir).cross(axis).normalize_or_zero();
    if n == Vec3::ZERO {
        return None;
    }
    let s = ray_plane(ray_o, ray_d, origin, n)?;
    Some((ray_o + ray_d * s - origin).dot(axis))
}

/// New distance along the axis relative to drag start: `t_hit_curr - grab_offset`.
pub fn axis_drag_delta(origin: Vec3, axis: Vec3, view_dir: Vec3, ray_o: Vec3, ray_d: Vec3, grab_offset: f32, snap: Option<f32>) -> Option<f32> {
    let t = axis_hit_param(origin, axis, view_dir, ray_o, ray_d)? - grab_offset;
    Some(match snap {
        Some(step) if step > 0.0 => (t / step).round() * step,
        _ => t,
    })
}

/// Point where the ray meets the drag plane (normal `n`). Rejects grazing planes.
pub fn plane_drag_point(origin: Vec3, normal: Vec3, view_dir: Vec3, ray_o: Vec3, ray_d: Vec3) -> Option<Vec3> {
    if normal.dot(view_dir).abs() < 0.08 {
        return None;
    }
    let s = ray_plane(ray_o, ray_d, origin, normal)?;
    Some(ray_o + ray_d * s)
}

/// Signed angle swept around `axis` from `v_start` (unit, in ring plane) to the current ray hit.
pub fn disc_rotation_angle(center: Vec3, axis: Vec3, view_dir: Vec3, ray_o: Vec3, ray_d: Vec3, v_start: Vec3, snap_deg: Option<f32>) -> Option<f32> {
    if axis.dot(view_dir).abs() < 0.08 {
        return None; // edge-on: caller uses tangent drag
    }
    let s = ray_plane(ray_o, ray_d, center, axis)?;
    let v = (ray_o + ray_d * s - center).normalize_or_zero();
    if v == Vec3::ZERO {
        return None;
    }
    let mut a = axis.dot(v_start.cross(v)).atan2(v_start.dot(v));
    if let Some(step) = snap_deg {
        let st = step.to_radians();
        a = (a / st).round() * st;
    }
    Some(a)
}

/// Wrap an angle into (-π, π].
pub fn wrap_angle(a: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    let w = (a + PI).rem_euclid(TAU) - PI;
    if w <= -PI { PI } else { w }
}


// ───────────────────────────── Selection & snapping (doc 03) ─────────────────────────────

/// Normalised marquee rectangle (min, max) in pixels.
pub fn normalize_rect(a: Vec2, b: Vec2) -> (Vec2, Vec2) {
    (a.min(b), a.max(b))
}

pub fn rect_contains(r: (Vec2, Vec2), p: Vec2) -> bool {
    p.x >= r.0.x && p.x <= r.1.x && p.y >= r.0.y && p.y <= r.1.y
}

/// Marquee result: none → rect, Shift → union, Ctrl → symmetric difference (Ctrl wins over Shift).
pub fn combine_selection<T: Clone + Eq + Hash>(prev: &HashSet<T>, rect: &HashSet<T>, shift: bool, ctrl: bool) -> HashSet<T> {
    if ctrl {
        prev.symmetric_difference(rect).cloned().collect()
    } else if shift {
        prev.union(rect).cloned().collect()
    } else {
        rect.clone()
    }
}

/// Final rotation `q = from_rotation_arc(Y, normalize(lerp(Y, n, w))) * Rot_Y(yaw)`.
pub fn surface_align_rotation(normal: Vec3, yaw: f32, weight: f32) -> Quat {
    let n = Vec3::Y.lerp(normal, weight.clamp(0.0, 1.0)).normalize_or_zero();
    let n = if n == Vec3::ZERO { Vec3::Y } else { n };
    Quat::from_rotation_arc(Vec3::Y, n) * Quat::from_rotation_y(yaw)
}

pub fn yaw_of(q: Quat) -> f32 {
    q.to_euler(EulerRot::YXZ).0
}

/// Scale-gizmo ratio of the current axis parameter to the grab parameter, kept positive and bounded.
pub fn scale_factor(t_curr: f32, t_grab: f32) -> f32 {
    if t_grab.abs() < 1e-4 {
        return 1.0;
    }
    (t_curr / t_grab).clamp(0.05, 50.0)
}

/// Distance (px) from point `p` to segment `a`–`b`.
pub fn dist_point_segment(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let l2 = ab.length_squared();
    let t = if l2 < 1e-6 { 0.0 } else { ((p - a).dot(ab) / l2).clamp(0.0, 1.0) };
    p.distance(a + ab * t)
}

/// Point inside a convex quad (any winding).
pub fn point_in_quad(p: Vec2, q: [Vec2; 4]) -> bool {
    let mut sign = 0.0_f32;
    for i in 0..4 {
        let (a, b) = (q[i], q[(i + 1) % 4]);
        let c = (b - a).perp_dot(p - a);
        if c.abs() < 1e-6 {
            continue;
        }
        if sign == 0.0 {
            sign = c.signum();
        } else if sign != c.signum() {
            return false;
        }
    }
    true
}

// ───────────────────────────── Undo history (doc 05) ─────────────────────────────

#[derive(Clone, Debug)]
pub struct UndoEntry<S> {
    pub label: String,
    pub before: S,
    pub after: S,
}

#[derive(Clone, Debug)]
struct OpenTxn<S> {
    label: String,
    before: S,
    preview: S,
}

/// Transactional history: Begin → UpdatePreview* → Commit collapses into one entry.
#[derive(Resource, Debug)]
pub struct UndoHistory<S: Clone + PartialEq + Send + Sync + 'static> {
    undo: Vec<UndoEntry<S>>,
    redo: Vec<UndoEntry<S>>,
    open: Option<OpenTxn<S>>,
    pub limit: usize,
}

impl<S: Clone + PartialEq + Send + Sync + 'static> Default for UndoHistory<S> {
    fn default() -> Self {
        Self { undo: Vec::new(), redo: Vec::new(), open: None, limit: 200 }
    }
}

impl<S: Clone + PartialEq + Send + Sync + 'static> UndoHistory<S> {
    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }
    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }
    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }
    pub fn preview(&self) -> Option<&S> {
        self.open.as_ref().map(|t| &t.preview)
    }

    /// Begin (or restart) a transaction with a snapshot of the target.
    pub fn begin(&mut self, label: impl Into<String>, before: S) {
        self.cancel();
        self.open = Some(OpenTxn { label: label.into(), preview: before.clone(), before });
    }

    /// Live preview value; never touches the stacks. Returns false if no transaction is open.
    pub fn update_preview(&mut self, value: S) -> bool {
        match &mut self.open {
            Some(t) => {
                t.preview = value;
                true
            }
            None => false,
        }
    }

    /// Commit with the final state. Returns true if an entry was pushed (false when nothing changed).
    pub fn commit(&mut self, after: S) -> bool {
        let Some(t) = self.open.take() else { return false };
        if t.before == after {
            return false;
        }
        self.undo.push(UndoEntry { label: t.label, before: t.before, after });
        self.redo.clear();
        if self.undo.len() > self.limit {
            let extra = self.undo.len() - self.limit;
            self.undo.drain(..extra);
        }
        true
    }

    /// Abort the open transaction; returns the snapshot to restore.
    pub fn cancel(&mut self) -> Option<S> {
        self.open.take().map(|t| t.before)
    }

    /// One-shot transaction (begin + commit).
    pub fn record(&mut self, label: impl Into<String>, before: S, after: S) -> bool {
        self.begin(label, before);
        self.commit(after)
    }

    /// Pop the latest entry; returns the state to apply. An open transaction is cancelled first.
    pub fn undo(&mut self) -> Option<S> {
        self.cancel();
        let e = self.undo.pop()?;
        let s = e.before.clone();
        self.redo.push(e);
        Some(s)
    }

    pub fn redo(&mut self) -> Option<S> {
        self.cancel();
        let e = self.redo.pop()?;
        let s = e.after.clone();
        self.undo.push(e);
        Some(s)
    }
}

// ───────────────────────────── Unit scale (feet ↔ world) ─────────────────────────────

/// Rules are expressed in feet; the world is in metres. `world_per_foot` links them
/// (`REAL` = exact conversion, `GAME` = 1 ft is 1 world unit, so 5 ft = 5 m).
#[derive(Resource, Reflect, Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
#[reflect(Resource)]
pub struct UnitScale {
    pub world_per_foot: f32,
}

impl UnitScale {
    pub const REAL: Self = Self { world_per_foot: 0.3048 };
    pub const GAME: Self = Self { world_per_foot: 1.0 };

    pub fn feet_to_world(&self, feet: f32) -> f32 {
        feet * self.world_per_foot
    }

    pub fn world_to_feet(&self, world: f32) -> f32 {
        world / self.world_per_foot
    }
}

impl Default for UnitScale {
    fn default() -> Self {
        Self::GAME
    }
}

// ───────────────────────────── Tests ─────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_scale_modes() {
        assert!((UnitScale::REAL.feet_to_world(5.0) - 1.524).abs() < 1e-5);
        assert_eq!(UnitScale::GAME.feet_to_world(5.0), 5.0);
        for s in [UnitScale::REAL, UnitScale::GAME] {
            assert!((s.world_to_feet(s.feet_to_world(30.0)) - 30.0).abs() < 1e-4);
        }
    }

    fn small_spec() -> WorldSpec {
        WorldSpec { resolution: 129, erosion_droplets: 0, thermal_iterations: 0, ..WorldSpec::default() }
    }

    #[test]
    fn erosion_changes_terrain_and_thermal_relaxes_slopes() {
        let spec = small_spec();
        let base = HeightfieldGrid::generate(&spec);
        let mut g = base.clone();
        g.erode_hydraulic(20_000, 7);
        g.clamp_heights(spec.max_height);
        assert!(g.erosion_mask.iter().sum::<f32>() > 0.0);
        assert!(g.heights.iter().zip(&base.heights).any(|(a, b)| (a - b).abs() > 1e-3));
        assert!(g.heights.iter().all(|h| h.is_finite() && *h >= 0.0));
        let max_slope = |g: &HeightfieldGrid| {
            let n = g.n;
            let mut m = 0.0_f32;
            for j in 0..n {
                for i in 0..n - 1 {
                    m = m.max((g.heights[j * n + i + 1] - g.heights[j * n + i]).abs() / g.cell());
                }
            }
            m
        };
        let mut t = base.clone();
        t.erode_thermal(30, 35.0);
        assert!(max_slope(&t) <= max_slope(&base) + 1e-4);
    }

    #[test]
    fn raycast_hits_terrain_and_splat_normalised() {
        let spec = small_spec();
        let g = HeightfieldGrid::generate(&spec);
        let hit = g.raycast(Vec3::new(10.0, 300.0, -5.0), Vec3::new(0.2, -1.0, 0.1)).expect("hit");
        assert!((hit.y - g.sample_height(hit.x, hit.z)).abs() < 0.05);
        assert!(g.raycast(Vec3::new(0.0, 300.0, 0.0), Vec3::Y).is_none());
        for (i, j) in [(10, 10), (64, 64), (100, 30)] {
            let s = g.splat_at(i, j, &spec);
            assert!((s.weights.iter().sum::<f32>() - 1.0).abs() < 1e-4);
        }
        let plan = generate_scatter_plan(&spec, &g);
        assert!(plan.iter().any(|i| i.kind == ScatterKind::Pine) && plan.iter().any(|i| i.kind == ScatterKind::CampfireSite));
    }

    #[test]
    fn gizmo_trigonometry() {
        let eye = Vec3::new(0.0, 0.0, 10.0);
        let hs = calc_handle_size(eye, Vec3::ZERO, Vec3::NEG_Z, 1.0);
        assert!((hs - 10.0 * 0.5_f32.tan() * 0.16).abs() < 1e-5);
        // axis X drag: ray straight down -Z at x=3 → delta 3 minus grab offset 1
        let d = axis_drag_delta(Vec3::ZERO, Vec3::X, Vec3::NEG_Z, Vec3::new(3.0, 0.0, 10.0), Vec3::NEG_Z, 1.0, None).unwrap();
        assert!((d - 2.0).abs() < 1e-4);
        assert!(axis_drag_delta(Vec3::ZERO, Vec3::Z, Vec3::NEG_Z, eye, Vec3::NEG_Z, 0.0, None).is_none());
        let p = plane_drag_point(Vec3::ZERO, Vec3::Z, Vec3::NEG_Z, Vec3::new(2.0, -1.0, 10.0), Vec3::NEG_Z).unwrap();
        assert!((p - Vec3::new(2.0, -1.0, 0.0)).length() < 1e-4);
        // disc: start on +X, ray hits +Y → +90° about Z
        let a = disc_rotation_angle(Vec3::ZERO, Vec3::Z, Vec3::NEG_Z, Vec3::new(0.0, 4.0, 10.0), Vec3::NEG_Z, Vec3::X, None).unwrap();
        assert!((a - std::f32::consts::FRAC_PI_2).abs() < 1e-4);
        assert!((wrap_angle(3.0 * std::f32::consts::PI) - std::f32::consts::PI).abs() < 1e-4);
    }

    #[test]
    fn selection_and_snap_math() {
        let prev: HashSet<u32> = [1, 2].into();
        let rect: HashSet<u32> = [2, 3].into();
        assert_eq!(combine_selection(&prev, &rect, false, false), rect);
        assert_eq!(combine_selection(&prev, &rect, true, false), [1, 2, 3].into());
        assert_eq!(combine_selection(&prev, &rect, true, true), [1, 3].into());
        let r = normalize_rect(Vec2::new(10.0, 40.0), Vec2::new(2.0, 5.0));
        assert!(rect_contains(r, Vec2::new(5.0, 20.0)) && !rect_contains(r, Vec2::new(11.0, 20.0)));
        let q = surface_align_rotation(Vec3::new(1.0, 1.0, 0.0).normalize(), 0.7, 1.0);
        assert!((q * Vec3::Y - Vec3::new(1.0, 1.0, 0.0).normalize()).length() < 1e-4);
        assert!((yaw_of(Quat::from_rotation_y(0.7)) - 0.7).abs() < 1e-5);
        assert!((dist_point_segment(Vec2::new(5.0, 3.0), Vec2::ZERO, Vec2::new(10.0, 0.0)) - 3.0).abs() < 1e-5);
        assert!(point_in_quad(Vec2::splat(0.5), [Vec2::ZERO, Vec2::X, Vec2::ONE, Vec2::Y]) && !point_in_quad(Vec2::splat(2.0), [Vec2::ZERO, Vec2::X, Vec2::ONE, Vec2::Y]));
        assert!((scale_factor(2.0, 1.0) - 2.0).abs() < 1e-6);
    }

    #[test]
    fn undo_transaction_collapses_drag() {
        let mut h: UndoHistory<f32> = UndoHistory::default();
        h.begin("slider", 1.0);
        for v in [1.1, 1.5, 2.0, 3.0] {
            assert!(h.update_preview(v));
        }
        assert_eq!(h.undo_len(), 0);
        assert!(h.commit(3.0));
        assert_eq!(h.undo_len(), 1);
        assert_eq!(h.undo(), Some(1.0));
        assert_eq!(h.redo(), Some(3.0));
        // no-op transaction is discarded
        h.begin("noop", 3.0);
        assert!(!h.commit(3.0));
        assert_eq!(h.undo_len(), 1);
    }

    #[test]
    fn undo_cancel_limit_and_redo_clear() {
        let mut h: UndoHistory<i32> = UndoHistory { limit: 3, ..Default::default() };
        for k in 0..5 {
            h.record("t", k, k + 1);
        }
        assert_eq!(h.undo_len(), 3);
        h.begin("open", 5);
        h.update_preview(99);
        assert_eq!(h.undo(), Some(4)); // cancels open txn, then undoes top entry
        assert!(!h.is_open());
        assert_eq!(h.redo_len(), 1);
        h.record("new", 4, 10);
        assert_eq!(h.redo_len(), 0);
        let spec = WorldSpec::from_preset(WorldPreset::MidnightCampfire);
        let json = serde_json::to_string(&spec).unwrap();
        assert_eq!(serde_json::from_str::<WorldSpec>(&json).unwrap(), spec);
    }
}
