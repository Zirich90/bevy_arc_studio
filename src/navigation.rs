//! Pure navigation core (no ECS): 0.5 m traversability raster, Theta* any-angle search and funnel (string-pulling) smoothing.
//! Distances are horizontal (XZ) metres; feet come from `UnitScale`. Difficult terrain costs ×2 (PF2e).

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use bevy::math::Vec2;
use serde::{Deserialize, Serialize};

use crate::arc_core::{HeightfieldGrid, UnitScale};

pub const NAV_CELL: f32 = 0.5;
pub const AGENT_RADIUS: f32 = 0.35;
pub const MAX_SLOPE_DEG: f32 = 45.0;
/// Slopes between this and `MAX_SLOPE_DEG` are scrambling terrain (difficult).
pub const DIFFICULT_SLOPE_DEG: f32 = 30.0;
pub const DIFFICULT_COST: f32 = 2.0;
/// Water shallower than this (below the surface) is wading depth; deeper is impassable.
pub const WADE_DEPTH: f32 = 0.5;
const MAX_EXPANSIONS: usize = 60_000;
/// Theta* grandparent shortcuts are tried only up to this length (keeps the search cheap; the funnel pass restores long legs).
const MAX_SHORTCUT: f32 = 20.0;
const SNAP_RADIUS_CELLS: i32 = 8;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum NavCell {
    #[default]
    Free,
    /// `DifficultTerrain`: ankle-deep water, mud, scrambling slopes. Movement cost ×2.
    Difficult,
    Blocked,
}

impl NavCell {
    #[inline]
    fn cost(self) -> f32 {
        if self == NavCell::Difficult { DIFFICULT_COST } else { 1.0 }
    }
}

/// Ground-plane footprint of a prop (world XZ).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Footprint {
    Circle { c: Vec2, r: f32 },
    /// Oriented rectangle, `yaw` rotates about +Y as in `Quat::from_rotation_y`.
    Box { c: Vec2, half: Vec2, yaw: f32 },
}

impl Footprint {
    fn reach(&self) -> f32 {
        match *self {
            Footprint::Circle { r, .. } => r,
            Footprint::Box { half, .. } => half.length(),
        }
    }
    fn center(&self) -> Vec2 {
        match *self {
            Footprint::Circle { c, .. } | Footprint::Box { c, .. } => c,
        }
    }
    fn contains(&self, p: Vec2, inflate: f32) -> bool {
        match *self {
            Footprint::Circle { c, r } => c.distance_squared(p) <= (r + inflate) * (r + inflate),
            Footprint::Box { c, half, yaw } => {
                let (s, co) = yaw.sin_cos();
                let d = p - c;
                let (lx, lz) = (d.x * co - d.y * s, d.x * s + d.y * co);
                lx.abs() <= half.x + inflate && lz.abs() <= half.y + inflate
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct NavGrid {
    pub n: usize,
    pub size: f32,
    pub cell: f32,
    cells: Vec<NavCell>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NavPath {
    pub points: Vec<Vec2>,
    /// Horizontal length in metres.
    pub length: f32,
    /// Length weighted by terrain cost (metre-equivalents).
    pub cost: f32,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct PathReport {
    pub distance_meters: f32,
    pub distance_feet: f32,
    pub cost_feet: f32,
    pub is_reachable: bool,
}

impl PathReport {
    pub const UNREACHABLE: Self = Self { distance_meters: 0.0, distance_feet: 0.0, cost_feet: 0.0, is_reachable: false };
}

impl NavPath {
    pub fn report(&self, units: &UnitScale) -> PathReport {
        PathReport { distance_meters: self.length, distance_feet: units.world_to_feet(self.length), cost_feet: units.world_to_feet(self.cost), is_reachable: true }
    }
}

#[derive(PartialEq)]
struct Node {
    f: f32,
    idx: u32,
}
impl Eq for Node {}
impl PartialOrd for Node {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Node {
    fn cmp(&self, o: &Self) -> Ordering {
        o.f.partial_cmp(&self.f).unwrap_or(Ordering::Equal) // min-heap
    }
}

impl NavGrid {
    pub fn new(size: f32, cell: f32) -> Self {
        let n = (size / cell).ceil() as usize;
        Self { n, size, cell, cells: vec![NavCell::Free; n * n] }
    }

    /// Terrain pass (slope, water) then props inflated by the agent radius.
    pub fn build(grid: &HeightfieldGrid, water_level: f32, obstacles: &[Footprint], agent_radius: f32) -> Self {
        let mut g = Self::new(grid.size, NAV_CELL);
        let (cos_block, cos_diff) = (MAX_SLOPE_DEG.to_radians().cos(), DIFFICULT_SLOPE_DEG.to_radians().cos());
        for j in 0..g.n {
            for i in 0..g.n {
                let c = g.cell_center(i, j);
                let h = grid.sample_height(c.x, c.y);
                let ny = grid.sample_normal(c.x, c.y).y;
                g.cells[j * g.n + i] = if ny < cos_block || h < water_level - WADE_DEPTH {
                    NavCell::Blocked
                } else if h < water_level || ny < cos_diff {
                    NavCell::Difficult
                } else {
                    NavCell::Free
                };
            }
        }
        for fp in obstacles {
            g.block(fp, agent_radius);
        }
        g
    }

    /// Mark every cell touched by `fp` grown by `inflate` (plus a quarter cell so thin props never slip between samples).
    pub fn block(&mut self, fp: &Footprint, inflate: f32) {
        let inflate = inflate + self.cell * 0.25;
        let (c, reach) = (fp.center(), fp.reach() + inflate);
        let (Some(lo), Some(hi)) = (self.clamped_cell(c - Vec2::splat(reach)), self.clamped_cell(c + Vec2::splat(reach))) else { return };
        for j in lo.1..=hi.1 {
            for i in lo.0..=hi.0 {
                if fp.contains(self.cell_center(i, j), inflate) {
                    self.cells[j * self.n + i] = NavCell::Blocked;
                }
            }
        }
    }

    pub fn set(&mut self, i: usize, j: usize, c: NavCell) {
        self.cells[j * self.n + i] = c;
    }

    fn clamped_cell(&self, p: Vec2) -> Option<(usize, usize)> {
        let m = self.n as f32 - 1.0;
        let f = |v: f32| ((v + self.size * 0.5) / self.cell).floor().clamp(0.0, m) as usize;
        Some((f(p.x), f(p.y)))
    }

    pub fn world_to_cell(&self, p: Vec2) -> Option<(usize, usize)> {
        let (fx, fz) = ((p.x + self.size * 0.5) / self.cell, (p.y + self.size * 0.5) / self.cell);
        (fx >= 0.0 && fz >= 0.0 && (fx as usize) < self.n && (fz as usize) < self.n).then(|| (fx as usize, fz as usize))
    }

    pub fn cell_center(&self, i: usize, j: usize) -> Vec2 {
        Vec2::new((i as f32 + 0.5) * self.cell - self.size * 0.5, (j as f32 + 0.5) * self.cell - self.size * 0.5)
    }

    /// Out of bounds counts as blocked.
    pub fn at(&self, p: Vec2) -> NavCell {
        self.world_to_cell(p).map_or(NavCell::Blocked, |(i, j)| self.cells[j * self.n + i])
    }

    /// Nearest non-blocked cell centre to `p` (`p` itself when already walkable).
    pub fn nearest_free(&self, p: Vec2) -> Option<Vec2> {
        if self.at(p) != NavCell::Blocked {
            return Some(p);
        }
        let (ci, cj) = self.clamped_cell(p)?;
        let mut best: Option<(f32, Vec2)> = None;
        for dj in -SNAP_RADIUS_CELLS..=SNAP_RADIUS_CELLS {
            for di in -SNAP_RADIUS_CELLS..=SNAP_RADIUS_CELLS {
                let (i, j) = (ci as i32 + di, cj as i32 + dj);
                if i < 0 || j < 0 || i as usize >= self.n || j as usize >= self.n {
                    continue;
                }
                if self.cells[j as usize * self.n + i as usize] != NavCell::Blocked {
                    let c = self.cell_center(i as usize, j as usize);
                    let d = c.distance_squared(p);
                    if best.is_none_or(|(bd, _)| d < bd) {
                        best = Some((d, c));
                    }
                }
            }
        }
        best.map(|(_, c)| c)
    }

    /// Straight-line walk from `a` to `b`: `None` when it crosses a blocked cell, else the terrain-weighted length.
    pub fn line_cost(&self, a: Vec2, b: Vec2) -> Option<f32> {
        let len = a.distance(b);
        let steps = ((len / (self.cell * 0.5)).ceil() as usize).max(1);
        let seg = len / steps as f32;
        let mut cost = 0.0;
        for k in 0..=steps {
            let c = self.at(a.lerp(b, k as f32 / steps as f32));
            if c == NavCell::Blocked {
                return None;
            }
            if k < steps {
                let mid = self.at(a.lerp(b, (k as f32 + 0.5) / steps as f32));
                cost += seg * mid.cost();
            }
        }
        Some(cost)
    }

    pub fn line_of_sight(&self, a: Vec2, b: Vec2) -> bool {
        self.line_cost(a, b).is_some()
    }

    /// Theta* from `from` to `to`, then funnel smoothing. `None` when the goal cannot be reached.
    pub fn find_path(&self, from: Vec2, to: Vec2) -> Option<NavPath> {
        let s = self.nearest_free(from)?;
        let t = self.nearest_free(to)?;
        let straight = s.distance(t);
        if let Some(c) = self.line_cost(s, t) {
            if c <= straight + 1e-3 {
                return Some(Self::finish(vec![s, t], c));
            }
        }
        let (si, sj) = self.world_to_cell(s)?;
        let (ti, tj) = self.world_to_cell(t)?;
        let (start, goal) = ((sj * self.n + si) as u32, (tj * self.n + ti) as u32);
        let pos = |idx: u32| -> Vec2 {
            if idx == start {
                s
            } else if idx == goal {
                t
            } else {
                self.cell_center(idx as usize % self.n, idx as usize / self.n)
            }
        };
        let mut g = vec![f32::INFINITY; self.n * self.n];
        let mut parent = vec![u32::MAX; self.n * self.n];
        let mut closed = vec![false; self.n * self.n];
        let mut open = BinaryHeap::new();
        g[start as usize] = 0.0;
        parent[start as usize] = start;
        open.push(Node { f: s.distance(t), idx: start });
        let mut expansions = 0;
        let mut found = false;
        while let Some(Node { idx, .. }) = open.pop() {
            if closed[idx as usize] {
                continue;
            }
            closed[idx as usize] = true;
            if idx == goal {
                found = true;
                break;
            }
            expansions += 1;
            if expansions > MAX_EXPANSIONS {
                break;
            }
            let (ci, cj) = (idx as usize % self.n, idx as usize / self.n);
            let cur = pos(idx);
            let par = parent[idx as usize];
            for (di, dj) in [(-1i32, -1i32), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)] {
                let (ni, nj) = (ci as i32 + di, cj as i32 + dj);
                if ni < 0 || nj < 0 || ni as usize >= self.n || nj as usize >= self.n {
                    continue;
                }
                let nidx = (nj as usize * self.n + ni as usize) as u32;
                if closed[nidx as usize] || self.cells[nidx as usize] == NavCell::Blocked {
                    continue;
                }
                if di != 0 && dj != 0 && (self.cells[cj * self.n + ni as usize] == NavCell::Blocked || self.cells[nj as usize * self.n + ci] == NavCell::Blocked) {
                    continue;
                }
                let np = pos(nidx);
                // Theta*: try the grandparent first (any-angle shortcut), else step from the current node.
                // Theta*: also try the grandparent (any-angle shortcut); with weighted terrain keep whichever is cheaper.
                let step = self.line_cost(cur, np).map(|c| (g[idx as usize] + c, idx));
                let short = if par != idx && pos(par).distance(np) <= MAX_SHORTCUT { self.line_cost(pos(par), np).map(|c| (g[par as usize] + c, par)) } else { None };
                let Some((cand, via)) = [step, short].into_iter().flatten().min_by(|a, b| a.0.total_cmp(&b.0)) else { continue };
                if cand < g[nidx as usize] {
                    g[nidx as usize] = cand;
                    parent[nidx as usize] = via;
                    open.push(Node { f: cand + np.distance(t), idx: nidx });
                }
            }
        }
        if !found {
            return None;
        }
        let mut pts = vec![t];
        let mut cur = goal;
        while cur != start {
            cur = parent[cur as usize];
            pts.push(pos(cur));
        }
        pts.reverse();
        let pts = self.funnel(pts);
        let cost = pts.windows(2).map(|w| self.line_cost(w[0], w[1]).unwrap_or(f32::INFINITY)).sum();
        Some(Self::finish(pts, cost))
    }

    fn finish(points: Vec<Vec2>, cost: f32) -> NavPath {
        let length = points.windows(2).map(|w| w[0].distance(w[1])).sum();
        NavPath { points, length, cost }
    }

    /// Funnel / string-pulling: from each anchor jump to the farthest visible waypoint whose direct cost does not exceed the replaced detour.
    pub fn funnel(&self, pts: Vec<Vec2>) -> Vec<Vec2> {
        if pts.len() < 3 {
            return pts;
        }
        let seg: Vec<f32> = pts.windows(2).map(|w| self.line_cost(w[0], w[1]).unwrap_or(f32::INFINITY)).collect();
        let mut out = vec![pts[0]];
        let mut i = 0;
        while i + 1 < pts.len() {
            let mut next = i + 1;
            for j in (i + 2..pts.len()).rev() {
                let orig: f32 = seg[i..j].iter().sum();
                if self.line_cost(pts[i], pts[j]).is_some_and(|c| c <= orig + 0.15) {
                    next = j;
                    break;
                }
            }
            out.push(pts[next]);
            i = next;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_field() -> NavGrid {
        NavGrid::new(64.0, NAV_CELL)
    }

    fn polyline_ok(nav: &NavGrid, p: &NavPath) -> bool {
        p.points.windows(2).all(|w| nav.line_of_sight(w[0], w[1]))
    }

    #[test]
    fn open_field_is_one_straight_segment() {
        let nav = open_field();
        let p = nav.find_path(Vec2::new(-20.0, -7.0), Vec2::new(18.0, 11.0)).unwrap();
        assert_eq!(p.points.len(), 2);
        assert!((p.length - Vec2::new(38.0, 18.0).length()).abs() < 1e-3);
        assert!((p.cost - p.length).abs() < 0.05);
    }

    #[test]
    fn walks_around_boulder_by_shortest_taut_route() {
        let mut nav = open_field();
        nav.block(&Footprint::Circle { c: Vec2::ZERO, r: 3.0 }, AGENT_RADIUS);
        let (a, b) = (Vec2::new(-10.0, 0.0), Vec2::new(10.0, 0.0));
        let p = nav.find_path(a, b).unwrap();
        assert!(polyline_ok(&nav, &p));
        assert!(p.points.len() <= 5, "zigzag: {:?}", p.points);
        // ideal route around the inflated disc: two tangents + arc
        let r = 3.0 + AGENT_RADIUS + NAV_CELL * 0.25;
        let tangent = (100.0_f32 - r * r).sqrt();
        let ideal = 2.0 * tangent + r * (std::f32::consts::PI - 2.0 * (r / 10.0).acos());
        assert!(p.length > 20.0 && p.length < ideal * 1.04, "len {} ideal {ideal}", p.length);
    }

    #[test]
    fn difficult_terrain_costs_double() {
        let mut nav = open_field();
        for j in 0..nav.n {
            for i in 0..nav.n {
                if nav.cell_center(i, j).x.abs() < 2.0 {
                    nav.set(i, j, NavCell::Difficult);
                }
            }
        }
        let p = nav.find_path(Vec2::new(-10.0, 0.0), Vec2::new(10.0, 0.0)).unwrap();
        assert_eq!(p.points.len(), 2);
        // 4 m of rough ground counted twice: cost = length + 4
        assert!((p.cost - (p.length + 4.0)).abs() < 0.3, "cost {} len {}", p.cost, p.length);
        let r = p.report(&UnitScale::REAL);
        assert!(r.cost_feet > r.distance_feet && (r.distance_feet - 20.0 / 0.3048).abs() < 0.01);
    }

    #[test]
    fn prefers_cheap_detour_over_wide_mud() {
        let mut nav = open_field();
        // mud belt 10 m wide, x in [-5,5], z in [-4,30]; clear ground beyond z<-4
        for j in 0..nav.n {
            for i in 0..nav.n {
                let c = nav.cell_center(i, j);
                if c.x.abs() < 5.0 && c.y > -4.0 {
                    nav.set(i, j, NavCell::Difficult);
                }
            }
        }
        let p = nav.find_path(Vec2::new(-10.0, 0.0), Vec2::new(10.0, 0.0)).unwrap();
        assert!(p.cost < 20.0 + 10.0 - 1.0, "should dodge mud, cost {}", p.cost);
        assert!(polyline_ok(&nav, &p));
    }

    #[test]
    fn walled_off_goal_is_unreachable() {
        let mut nav = open_field();
        nav.block(&Footprint::Box { c: Vec2::ZERO, half: Vec2::new(0.5, 40.0), yaw: 0.0 }, 0.0);
        assert!(nav.find_path(Vec2::new(-10.0, 0.0), Vec2::new(10.0, 0.0)).is_none());
    }

    #[test]
    fn goal_inside_obstacle_snaps_to_edge() {
        let mut nav = open_field();
        nav.block(&Footprint::Circle { c: Vec2::ZERO, r: 2.0 }, AGENT_RADIUS);
        let p = nav.find_path(Vec2::new(-10.0, 0.0), Vec2::new(0.2, 0.0)).unwrap();
        let end = *p.points.last().unwrap();
        assert!(nav.at(end) != NavCell::Blocked && end.length() < 5.0);
    }

    #[test]
    fn rotated_box_blocks_its_corners() {
        let mut nav = open_field();
        nav.block(&Footprint::Box { c: Vec2::ZERO, half: Vec2::new(4.0, 0.5), yaw: std::f32::consts::FRAC_PI_2 }, 0.0);
        // yaw 90° turns the long axis onto Z
        assert_eq!(nav.at(Vec2::new(0.0, 3.5)), NavCell::Blocked);
        assert_eq!(nav.at(Vec2::new(3.5, 0.0)), NavCell::Free);
    }

    #[test]
    fn terrain_classification_slope_and_water() {
        let n = 65;
        let mut h = HeightfieldGrid::new(n, 32.0);
        let cell = h.cell();
        for j in 0..n {
            for i in 0..n {
                let (x, _) = h.grid_to_world(i, j);
                // x < -8: pit at -1 (deep), x in [-8, 8]: flat 0 (ankle water), x > 8: 60° ramp
                let k = h.idx(i, j);
                h.heights[k] = if x < -8.0 { -1.0 } else if x > 8.0 { (x - 8.0) * 1.8 } else { 0.0 };
            }
        }
        let _ = cell;
        let nav = NavGrid::build(&h, 0.3, &[], AGENT_RADIUS);
        assert_eq!(nav.at(Vec2::new(-12.0, 0.0)), NavCell::Blocked, "deep water");
        assert_eq!(nav.at(Vec2::new(0.0, 0.0)), NavCell::Difficult, "wading");
        assert_eq!(nav.at(Vec2::new(12.0, 0.0)), NavCell::Blocked, "steep");
        let dry = NavGrid::build(&h, -0.1, &[], AGENT_RADIUS);
        assert_eq!(dry.at(Vec2::new(0.0, 0.0)), NavCell::Free);
    }
}
