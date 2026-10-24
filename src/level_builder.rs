//! `LevelBuilderPlugin` (E4): modular level construction on a snap grid — floors, walls, doorways with swinging doors, arches,
//! pillars, stairs and torches in three architectural styles. Everything goes through `StudioCommand` (`BuildRoom`, `BuildWallLine`,
//! `PlaceLevelTile`, `PaintLevelTiles`, `ToggleDoor`) so a mouse drag and an AI agent produce identical, single-step undoable edits.
//!
//! Model: a tile occupies one grid cell (`cell_size` m) centred on `grid_pos * cell_size`; `grid_pos.y` is the storey and lifts the
//! cell to `Y = y * floor_height`. Walls run along the local X axis (`facing` 0/2 → X, 1/3 → Z) and face ±local Z.

use std::f32::consts::FRAC_PI_2;

use bevy::gizmos::prelude::*;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{EguiContexts, egui};
use serde::{Deserialize, Serialize};

use crate::arc_core::yaw_of;
use crate::camera::EditorCamera;
use crate::editor_registry::{EditorAppExt, PanelDef, PanelLocation, ToolDef};
use crate::gizmo_3d::{ObjState, SceneHistory, gizmo_system};
use crate::mcp_commands::{StudioCommand, StudioCommandQueue};
use crate::navigation::Footprint;
use crate::props::{NextObjectId, ObjectCategory, SceneObject, UserProp};
use crate::selection::Deleted;
use crate::ui::{UiData, chip, TEAL};
use crate::ui_inspector::Filter;
use crate::world_io::SceneComponentAppExt;

pub const TOOL_LEVEL_BUILDER: &str = "level_builder";
/// Wall thickness in metres.
pub const WALL_THICKNESS: f32 = 0.4;
/// Clear width of a doorway / arch opening.
pub const DOOR_WIDTH: f32 = 1.2;
const DOOR_HEIGHT: f32 = 2.4;
const MAX_BUILD_CELLS: i64 = 4096;

// ───────────────────────────── Data ─────────────────────────────

#[derive(Reflect, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TileKind {
    #[default]
    Floor,
    Wall,
    Doorway,
    Arch,
    Pillar,
    Stairs,
    Roof,
    /// Wall-mounted torch with a warm `PointLight` (`facing` = side of the wall it hangs on).
    Torch,
}

impl TileKind {
    /// Tiles in one slot replace each other when placed in the same cell.
    fn slot(self) -> u8 {
        match self {
            Self::Floor | Self::Stairs | Self::Roof => 0,
            Self::Wall | Self::Doorway | Self::Arch | Self::Pillar => 1,
            Self::Torch => 2,
        }
    }
}

#[derive(Reflect, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum WallStyle {
    #[default]
    StoneDungeon,
    WoodTavern,
    CastleBrick,
}

impl WallStyle {
    pub const ALL: [WallStyle; 3] = [Self::StoneDungeon, Self::WoodTavern, Self::CastleBrick];
    pub fn label(self) -> &'static str {
        match self {
            Self::StoneDungeon => "Stone Dungeon",
            Self::WoodTavern => "Wood Tavern",
            Self::CastleBrick => "Castle Brick",
        }
    }
}

#[derive(Component, Reflect, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[reflect(Component, Default)]
pub struct LevelTile {
    pub kind: TileKind,
    pub grid_pos: IVec3,
    pub facing: u8,
    pub style: WallStyle,
}

#[derive(Component, Reflect, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[reflect(Component, Default)]
pub struct Door {
    pub is_open: bool,
    pub is_locked: bool,
}

/// Snap-grid settings shared by the builder tool and the transform gizmo.
#[derive(Resource, Reflect, Clone, Debug, PartialEq)]
#[reflect(Resource)]
pub struct GridSettings {
    pub enabled: bool,
    pub cell_size: f32,
    pub angle_snap_deg: f32,
    pub floor_level: i32,
    pub floor_height: f32,
}

impl Default for GridSettings {
    fn default() -> Self {
        Self { enabled: false, cell_size: 2.0, angle_snap_deg: 15.0, floor_level: 0, floor_height: 3.5 }
    }
}

impl GridSettings {
    /// Y of the building / grid plane of the current storey.
    pub fn plane_y(&self) -> f32 {
        self.floor_level as f32 * self.floor_height
    }
    pub fn snap(&self, v: f32) -> f32 {
        (v / self.cell_size).round() * self.cell_size
    }
    pub fn cell_center(&self, p: IVec3) -> Vec3 {
        Vec3::new(p.x as f32 * self.cell_size, p.y as f32 * self.floor_height, p.z as f32 * self.cell_size)
    }
    pub fn world_to_cell(&self, p: Vec3, level: i32) -> IVec3 {
        IVec3::new((p.x / self.cell_size).round() as i32, level, (p.z / self.cell_size).round() as i32)
    }
    /// Move-gizmo step: a cell, or half a cell while holding Ctrl.
    pub fn move_step(&self, fine: bool) -> Option<f32> {
        self.enabled.then(|| if fine { self.cell_size * 0.5 } else { self.cell_size })
    }
    pub fn rot_step_rad(&self) -> Option<f32> {
        self.enabled.then(|| self.angle_snap_deg.max(1.0).to_radians())
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum BuildMode {
    #[default]
    RoomBox,
    WallLine,
    FloorBrush,
    Eraser,
    Door,
    Arch,
    Stairs,
    Pillar,
    Torch,
}

impl BuildMode {
    pub const ALL: [BuildMode; 9] = [Self::RoomBox, Self::WallLine, Self::FloorBrush, Self::Eraser, Self::Door, Self::Arch, Self::Stairs, Self::Pillar, Self::Torch];
    pub fn label(self) -> &'static str {
        match self {
            Self::RoomBox => "Room Box",
            Self::WallLine => "Wall Line",
            Self::FloorBrush => "Floor Brush",
            Self::Eraser => "Eraser",
            Self::Door => "Place Door",
            Self::Arch => "Place Arch",
            Self::Stairs => "Place Stairs",
            Self::Pillar => "Place Pillar",
            Self::Torch => "Place Torch",
        }
    }
    fn place_kind(self) -> Option<TileKind> {
        match self {
            Self::Door => Some(TileKind::Doorway),
            Self::Arch => Some(TileKind::Arch),
            Self::Stairs => Some(TileKind::Stairs),
            Self::Pillar => Some(TileKind::Pillar),
            Self::Torch => Some(TileKind::Torch),
            _ => None,
        }
    }
}

#[derive(Resource, Default)]
pub struct LevelBuilderState {
    pub mode: BuildMode,
    pub style: WallStyle,
    /// Facing used by stairs / doors placed on empty cells (walls and torches derive it from the wall).
    pub facing: u8,
    anchor: Option<IVec3>,
    hover: Option<IVec3>,
    hover_side: Option<u8>,
    stroke: Vec<IVec3>,
}

// ───────────────────────────── Pure generation ─────────────────────────────

fn ordered(a: i32, b: i32) -> (i32, i32) {
    (a.min(b), a.max(b))
}

/// Floor under the whole rectangle, pillars on its corners, walls on the rest of the perimeter.
pub fn room_tiles(a: IVec2, b: IVec2, level: i32, style: WallStyle) -> Result<Vec<LevelTile>, String> {
    let ((x0, x1), (z0, z1)) = (ordered(a.x, b.x), ordered(a.y, b.y));
    let cells = (x1 - x0 + 1) as i64 * (z1 - z0 + 1) as i64;
    if cells > MAX_BUILD_CELLS {
        return Err(format!("room too large ({cells} cells, max {MAX_BUILD_CELLS})"));
    }
    let tile = |kind, x, z, facing| LevelTile { kind, grid_pos: IVec3::new(x, level, z), facing, style };
    let mut out = Vec::with_capacity(cells as usize * 2);
    for z in z0..=z1 {
        for x in x0..=x1 {
            out.push(tile(TileKind::Floor, x, z, 0));
        }
    }
    for z in z0..=z1 {
        for x in x0..=x1 {
            let (ex, ez) = (x == x0 || x == x1, z == z0 || z == z1);
            match (ex, ez) {
                (true, true) => out.push(tile(TileKind::Pillar, x, z, 0)),
                (false, true) => out.push(tile(TileKind::Wall, x, z, 0)),
                (true, false) => out.push(tile(TileKind::Wall, x, z, 1)),
                _ => {}
            }
        }
    }
    Ok(out)
}

/// Straight run of walls from `a` along the dominant axis to `b`'s coordinate on that axis.
pub fn wall_line_tiles(a: IVec2, b: IVec2, level: i32, style: WallStyle) -> Result<Vec<LevelTile>, String> {
    let (dx, dz) = ((b.x - a.x).abs(), (b.y - a.y).abs());
    let along_x = dx >= dz;
    let n = if along_x { dx } else { dz } + 1;
    if n as i64 > MAX_BUILD_CELLS {
        return Err("wall line too long".into());
    }
    let step = if along_x { IVec2::new(if b.x < a.x { -1 } else { 1 }, 0) } else { IVec2::new(0, if b.y < a.y { -1 } else { 1 }) };
    Ok((0..n)
        .map(|i| {
            let p = a + step * i;
            LevelTile { kind: TileKind::Wall, grid_pos: IVec3::new(p.x, level, p.y), facing: if along_x { 0 } else { 1 }, style }
        })
        .collect())
}

pub fn tile_rotation(facing: u8) -> Quat {
    Quat::from_rotation_y(facing as f32 * FRAC_PI_2)
}

pub fn tile_transform(t: &LevelTile, g: &GridSettings) -> Transform {
    Transform { translation: g.cell_center(t.grid_pos), rotation: tile_rotation(t.facing), scale: Vec3::ONE }
}

/// Navigation footprint (level 0 only: the raster is 2D). Closed doors and walls block; open doors, arches and floors do not.
pub fn tile_footprint(t: &LevelTile, door: Option<&Door>, tf: &Transform, cell_size: f32) -> Option<Footprint> {
    if t.grid_pos.y != 0 {
        return None;
    }
    let c = Vec2::new(tf.translation.x, tf.translation.z);
    let wall = Footprint::Box { c, half: Vec2::new(cell_size * 0.5, WALL_THICKNESS * 0.5), yaw: yaw_of(tf.rotation) };
    match t.kind {
        TileKind::Wall => Some(wall),
        TileKind::Doorway => (!door.is_some_and(|d| d.is_open)).then_some(wall),
        TileKind::Pillar => Some(Footprint::Circle { c, r: 0.32 }),
        _ => None,
    }
}

// ───────────────────────────── Part generator (pure geometry) ─────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Shape {
    Cube,
    Cyl,
    Sphere,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mat {
    Wall,
    Trim,
    Floor,
    Joint,
    Wood,
    Iron,
    Flame,
}

#[derive(Clone, Debug)]
pub struct Part {
    pub shape: Shape,
    pub mat: Mat,
    pub tf: Transform,
    /// Part of the swinging door leaf (positioned relative to the hinge).
    pub leaf: bool,
    pub light: bool,
}

struct Parts(Vec<Part>);

impl Parts {
    fn add(&mut self, shape: Shape, mat: Mat, center: Vec3, size: Vec3) {
        self.0.push(Part { shape, mat, tf: Transform { translation: center, scale: size, ..default() }, leaf: false, light: false });
    }
    /// Axis-aligned box from `min` to `max`.
    fn cube(&mut self, mat: Mat, min: Vec3, max: Vec3) {
        self.add(Shape::Cube, mat, (min + max) * 0.5, max - min);
    }
    fn cyl(&mut self, mat: Mat, center: Vec3, r: f32, h: f32) {
        self.add(Shape::Cyl, mat, center, Vec3::new(r, h, r));
    }
}

/// Wall slab between `x0..x1` and `y0..y1` with the style's surface detail (`top` = reaches the wall crown).
fn wall_block(p: &mut Parts, style: WallStyle, x0: f32, x1: f32, y0: f32, y1: f32, top: bool, bottom: bool) {
    let t = WALL_THICKNESS;
    let (hz, w) = (t * 0.5, x1 - x0);
    if w <= 0.01 || y1 - y0 <= 0.01 {
        return;
    }
    let v = Vec3::new;
    match style {
        WallStyle::StoneDungeon => {
            p.cube(Mat::Wall, v(x0, y0, -hz), v(x1, y1, hz));
            // ashlar courses: thin dark mortar bands
            let mut y = y0 + 0.62;
            while y < y1 - 0.2 {
                p.cube(Mat::Joint, v(x0, y, -hz - 0.012), v(x1, y + 0.035, hz + 0.012));
                y += 0.62;
            }
            if bottom {
                p.cube(Mat::Trim, v(x0, y0, -hz - 0.06), v(x1, y0 + 0.28, hz + 0.06));
            }
            if top {
                p.cube(Mat::Trim, v(x0 - 0.0, y1 - 0.22, -hz - 0.1), v(x1, y1, hz + 0.1));
            }
        }
        WallStyle::WoodTavern => {
            p.cube(Mat::Trim, v(x0, y0, -hz), v(x1, y1, hz));
            let n = ((w / 0.26).round() as i32).max(1);
            let pw = w / n as f32;
            for i in 0..n {
                let (a, b) = (x0 + pw * i as f32 + 0.012, x0 + pw * (i + 1) as f32 - 0.012);
                p.cube(Mat::Wall, v(a, y0, -hz - 0.02), v(b, y1, hz + 0.02));
            }
            if bottom {
                p.cube(Mat::Trim, v(x0, y0, -hz - 0.07), v(x1, y0 + 0.22, hz + 0.07));
            }
            if top {
                p.cube(Mat::Trim, v(x0, y1 - 0.2, -hz - 0.1), v(x1, y1, hz + 0.1));
            }
            p.cube(Mat::Trim, v(x0, (y0 + y1) * 0.5 - 0.06, -hz - 0.05), v(x1, (y0 + y1) * 0.5 + 0.06, hz + 0.05));
        }
        WallStyle::CastleBrick => {
            p.cube(Mat::Wall, v(x0, y0, -hz), v(x1, y1, hz));
            let mut y = y0 + 0.24;
            while y < y1 - 0.1 {
                p.cube(Mat::Joint, v(x0, y, -hz - 0.01), v(x1, y + 0.022, hz + 0.01));
                y += 0.24;
            }
            if bottom {
                p.cube(Mat::Trim, v(x0, y0, -hz - 0.07), v(x1, y0 + 0.4, hz + 0.07));
            }
            if top {
                p.cube(Mat::Trim, v(x0, y1 - 0.3, -hz - 0.12), v(x1, y1, hz + 0.12));
                // crenellations
                let n = ((w / 0.5).floor() as i32).max(1);
                for i in (0..n).step_by(2) {
                    let a = x0 + w / n as f32 * i as f32;
                    p.cube(Mat::Trim, v(a + 0.02, y1, -hz - 0.04), v(a + w / n as f32 - 0.02, y1 + 0.28, hz + 0.04));
                }
            }
        }
    }
}

fn opening_frame(p: &mut Parts, style: WallStyle, cs: f32, h: f32, width: f32, height: f32) {
    let (hw, t) = (cs * 0.5, WALL_THICKNESS);
    let ow = width * 0.5;
    wall_block(p, style, -hw, -ow, 0.0, h, true, true);
    wall_block(p, style, ow, hw, 0.0, h, true, true);
    wall_block(p, style, -ow, ow, height, h, true, false);
    // timber / stone frame around the opening
    let frame = if style == WallStyle::StoneDungeon { Mat::Trim } else { Mat::Wood };
    let v = Vec3::new;
    p.cube(frame, v(-ow - 0.1, 0.0, -t * 0.5 - 0.05), v(-ow, height + 0.1, t * 0.5 + 0.05));
    p.cube(frame, v(ow, 0.0, -t * 0.5 - 0.05), v(ow + 0.1, height + 0.1, t * 0.5 + 0.05));
    p.cube(frame, v(-ow - 0.1, height, -t * 0.5 - 0.05), v(ow + 0.1, height + 0.12, t * 0.5 + 0.05));
    p.cube(Mat::Trim, v(-ow, -0.02, -t * 0.5), v(ow, 0.03, t * 0.5)); // threshold
}

/// All parts of a tile in tile space (floor top = Y 0). `hinge` is the door pivot when the tile has a leaf.
pub fn tile_parts(t: &LevelTile, cs: f32, fh: f32) -> (Vec<Part>, Option<Vec3>) {
    let mut p = Parts(Vec::new());
    let v = Vec3::new;
    let mut hinge = None;
    let hw = cs * 0.5;
    match t.kind {
        TileKind::Floor => {
            p.cube(Mat::Joint, v(-hw, -0.24, -hw), v(hw, -0.02, hw));
            let (n, thick) = match t.style {
                WallStyle::StoneDungeon => (2, 0.06),
                WallStyle::WoodTavern => ((cs / 0.3).round() as i32, 0.04),
                WallStyle::CastleBrick => (4, 0.05),
            };
            let gap = 0.025;
            if t.style == WallStyle::WoodTavern {
                let bw = cs / n as f32;
                for i in 0..n {
                    let (a, b) = (-hw + bw * i as f32 + gap * 0.5, -hw + bw * (i + 1) as f32 - gap * 0.5);
                    p.cube(Mat::Floor, v(-hw + 0.01, -0.02, a), v(hw - 0.01, thick - 0.02, b));
                }
            } else {
                let sw = cs / n as f32;
                for i in 0..n {
                    for j in 0..n {
                        let (ax, bx) = (-hw + sw * i as f32 + gap, -hw + sw * (i + 1) as f32 - gap);
                        let (az, bz) = (-hw + sw * j as f32 + gap, -hw + sw * (j + 1) as f32 - gap);
                        p.cube(Mat::Floor, v(ax, -0.02, az), v(bx, thick - 0.02, bz));
                    }
                }
            }
        }
        TileKind::Wall => wall_block(&mut p, t.style, -hw, hw, 0.0, fh, true, true),
        TileKind::Doorway => {
            opening_frame(&mut p, t.style, cs, fh, DOOR_WIDTH, DOOR_HEIGHT);
            let (ow, lh) = (DOOR_WIDTH * 0.5 - 0.03, DOOR_HEIGHT - 0.05);
            hinge = Some(v(-ow, 0.0, 0.0));
            let mut leaf = Parts(Vec::new());
            leaf.cube(Mat::Wood, v(-ow, 0.02, -0.045), v(ow, lh, 0.045));
            for i in 1..5 {
                let x = -ow + (2.0 * ow) * i as f32 / 5.0;
                leaf.cube(Mat::Trim, v(x - 0.006, 0.02, -0.05), v(x + 0.006, lh, 0.05)); // plank seams
            }
            for y in [0.35, lh - 0.4] {
                leaf.cube(Mat::Iron, v(-ow, y, 0.045), v(ow * 0.8, y + 0.1, 0.066)); // forged straps
                leaf.cube(Mat::Iron, v(-ow - 0.02, y - 0.02, -0.07), v(-ow + 0.13, y + 0.12, 0.07)); // hinge knuckles
            }
            leaf.add(Shape::Cyl, Mat::Iron, v(ow - 0.14, 1.05, 0.085), v(0.05, 0.02, 0.05));
            leaf.0.last_mut().unwrap().tf.rotation = Quat::from_rotation_x(FRAC_PI_2);
            leaf.cube(Mat::Iron, v(ow - 0.2, 1.0, -0.07), v(ow - 0.08, 1.1, 0.07));
            for mut part in leaf.0 {
                part.leaf = true;
                p.0.push(part);
            }
        }
        TileKind::Arch => {
            opening_frame(&mut p, t.style, cs, fh, DOOR_WIDTH + 0.4, DOOR_HEIGHT + 0.3);
            p.cube(Mat::Trim, v(-0.14, DOOR_HEIGHT + 0.2, -WALL_THICKNESS * 0.5 - 0.1), v(0.14, DOOR_HEIGHT + 0.42, WALL_THICKNESS * 0.5 + 0.1)); // keystone
        }
        TileKind::Pillar => {
            let sq = t.style == WallStyle::WoodTavern;
            let shaft_h = fh - 0.55;
            p.cube(Mat::Trim, v(-0.38, 0.0, -0.38), v(0.38, 0.28, 0.38));
            if sq {
                p.cube(Mat::Wood, v(-0.26, 0.28, -0.26), v(0.26, 0.28 + shaft_h, 0.26));
            } else {
                p.cyl(Mat::Wall, v(0.0, 0.28 + shaft_h * 0.5, 0.0), 0.28, shaft_h);
                for k in 0..3 {
                    let y = 0.28 + shaft_h * (k as f32 + 1.0) / 4.0;
                    p.cyl(Mat::Trim, v(0.0, y, 0.0), 0.3, 0.07);
                }
            }
            p.cube(Mat::Trim, v(-0.4, fh - 0.27, -0.4), v(0.4, fh, 0.4));
        }
        TileKind::Stairs => {
            // flight of 14 steps over two cells, climbing along local +X
            let n = 14;
            let (run, rise) = (cs * 2.0 / n as f32, fh / n as f32);
            let w = cs * 0.92;
            for i in 0..n {
                let x0 = -hw + run * i as f32;
                let top = rise * (i + 1) as f32;
                p.cube(Mat::Floor, v(x0, 0.0, -w * 0.5), v(x0 + run + 0.01, top, w * 0.5));
                p.cube(Mat::Joint, v(x0 + run - 0.012, top - rise, -w * 0.5 - 0.01), v(x0 + run + 0.01, top, w * 0.5 + 0.01));
            }
            for s in [-1.0, 1.0] {
                let zc = s * (w * 0.5 + 0.06);
                p.cube(Mat::Trim, v(-hw, 0.0, zc - 0.06), v(hw * 3.0, 0.55, zc + 0.06));
            }
        }
        TileKind::Roof => {
            p.cube(Mat::Trim, v(-hw, fh, -hw), v(hw, fh + 0.22, hw));
            p.cube(Mat::Joint, v(-hw + 0.04, fh + 0.22, -hw + 0.04), v(hw - 0.04, fh + 0.28, hw - 0.04));
        }
        TileKind::Torch => {
            let z = WALL_THICKNESS * 0.5;
            p.cube(Mat::Iron, v(-0.05, 1.55, z), v(0.05, 2.05, z + 0.03)); // wall plate
            p.cube(Mat::Iron, v(-0.025, 1.9, z), v(0.025, 1.95, z + 0.3)); // arm
            p.cyl(Mat::Wood, v(0.0, 1.9, z + 0.28), 0.04, 0.5);
            p.cyl(Mat::Iron, v(0.0, 2.12, z + 0.28), 0.075, 0.1); // cup
            p.add(Shape::Sphere, Mat::Flame, v(0.0, 2.27, z + 0.28), v(0.075, 0.13, 0.075));
            p.0.push(Part { shape: Shape::Sphere, mat: Mat::Flame, tf: Transform::from_xyz(0.0, 2.3, z + 0.4), leaf: false, light: true });
        }
    }
    (p.0, hinge)
}

// ───────────────────────────── World edits (single undo step) ─────────────────────────────

#[derive(Clone, Debug)]
pub enum Edit {
    Place(LevelTile),
    /// Remove tiles in a cell; `None` removes the wall layer (incl. torches) or, when empty, the floor.
    Erase { pos: IVec3, kind: Option<TileKind> },
}

type Live = Vec<(Entity, LevelTile, Transform)>;

fn live_tiles(w: &mut World) -> Live {
    w.query_filtered::<(Entity, &LevelTile, &Transform), Without<Deleted>>().iter(w).map(|(e, t, tf)| (e, *t, *tf)).collect()
}

fn same_spot(a: &LevelTile, b: &LevelTile) -> bool {
    a.grid_pos == b.grid_pos && a.kind.slot() == b.kind.slot() && (a.kind != TileKind::Torch || a.facing == b.facing)
}

fn spawn_tile(w: &mut World, t: LevelTile, g: &GridSettings) -> (Entity, Transform) {
    let id = {
        let mut n = w.resource_mut::<NextObjectId>();
        n.0 += 1;
        n.0
    };
    let tf = tile_transform(&t, g);
    let so = SceneObject { id, name: tile_name(&t), category: ObjectCategory::Level, aabb_half_extents: Vec3::new(g.cell_size * 0.5, g.floor_height * 0.5, g.cell_size * 0.5) };
    let mut e = w.spawn((Name::new(so.name.clone()), so, UserProp { kind: "level".into() }, t, tf, Visibility::default()));
    if t.kind == TileKind::Doorway {
        e.insert(Door::default());
    }
    (e.id(), tf)
}

fn tile_name(t: &LevelTile) -> String {
    format!("{:?} L{} ({}, {})", t.kind, t.grid_pos.y, t.grid_pos.x, t.grid_pos.z)
}

/// Apply edits as ONE `SceneHistory` entry. Returns a summary line.
pub fn apply_edits(w: &mut World, label: &str, edits: Vec<Edit>) -> Result<String, String> {
    let g = w.resource::<GridSettings>().clone();
    let mut live = live_tiles(w);
    let (mut before, mut after) = (Vec::new(), Vec::new());
    let (mut added, mut removed) = (0, 0);
    let hide = |w: &mut World, e: Entity, tf: Transform, before: &mut Vec<ObjState>, after: &mut Vec<ObjState>| {
        before.push(ObjState { entity: e, tf, alive: true });
        after.push(ObjState { entity: e, tf, alive: false });
        w.entity_mut(e).insert((Deleted, Visibility::Hidden));
    };
    for edit in edits {
        match edit {
            Edit::Place(t) => {
                if live.iter().any(|(_, o, _)| *o == t) {
                    continue;
                }
                let old: Vec<_> = live.iter().filter(|(_, o, _)| same_spot(o, &t)).map(|(e, _, tf)| (*e, *tf)).collect();
                for (e, tf) in old {
                    hide(w, e, tf, &mut before, &mut after);
                    removed += 1;
                }
                live.retain(|(_, o, _)| !same_spot(o, &t));
                let (e, tf) = spawn_tile(w, t, &g);
                before.push(ObjState { entity: e, tf, alive: false });
                after.push(ObjState { entity: e, tf, alive: true });
                live.push((e, t, tf));
                added += 1;
            }
            Edit::Erase { pos, kind } => {
                let in_cell = |o: &LevelTile| o.grid_pos == pos;
                let pick: Vec<(Entity, Transform)> = match kind {
                    Some(k) => live.iter().filter(|(_, o, _)| in_cell(o) && o.kind == k).map(|(e, _, tf)| (*e, *tf)).collect(),
                    None => {
                        let walls: Vec<_> = live.iter().filter(|(_, o, _)| in_cell(o) && o.kind.slot() != 0).map(|(e, _, tf)| (*e, *tf)).collect();
                        if walls.is_empty() { live.iter().filter(|(_, o, _)| in_cell(o)).map(|(e, _, tf)| (*e, *tf)).collect() } else { walls }
                    }
                };
                for (e, tf) in pick {
                    hide(w, e, tf, &mut before, &mut after);
                    live.retain(|(o, ..)| *o != e);
                    removed += 1;
                }
            }
        }
    }
    if before.is_empty() {
        return Err("nothing to change".into());
    }
    w.resource_mut::<SceneHistory>().record(label, before, after);
    Ok(format!("{label}: +{added} tiles, -{removed} replaced/erased"))
}

pub fn cmd_build_room(w: &mut World, min: [i32; 2], max: [i32; 2], floor: i32, style: WallStyle) -> Result<String, String> {
    let tiles = room_tiles(IVec2::from(min), IVec2::from(max), floor, style)?;
    apply_edits(w, "Build Room", tiles.into_iter().map(Edit::Place).collect())
}

pub fn cmd_build_wall_line(w: &mut World, from: [i32; 2], to: [i32; 2], floor: i32, style: WallStyle) -> Result<String, String> {
    let tiles = wall_line_tiles(IVec2::from(from), IVec2::from(to), floor, style)?;
    apply_edits(w, "Build Wall Line", tiles.into_iter().map(Edit::Place).collect())
}

/// `facing: None` inherits the wall in that cell (doors, arches, torches), else 0.
pub fn cmd_place_tile(w: &mut World, kind: TileKind, pos: [i32; 3], facing: Option<u8>, style: Option<WallStyle>) -> Result<String, String> {
    let pos = IVec3::from(pos);
    let wall = live_tiles(w).into_iter().map(|(_, t, _)| t).find(|t| t.grid_pos == pos && t.kind.slot() == 1);
    let facing = facing.unwrap_or_else(|| wall.map_or(0, |t| t.facing)) % 4;
    let style = style.or(wall.map(|t| t.style)).unwrap_or_default();
    let name = format!("Place {kind:?}");
    apply_edits(w, &name, vec![Edit::Place(LevelTile { kind, grid_pos: pos, facing, style })])
}

/// Brush stroke: paint `kind` (or erase when `erase`) over many cells as one undo step.
pub fn cmd_paint_tiles(w: &mut World, kind: TileKind, cells: Vec<[i32; 3]>, style: WallStyle, erase: bool) -> Result<String, String> {
    if cells.len() as i64 > MAX_BUILD_CELLS {
        return Err("stroke too large".into());
    }
    let edits = cells
        .into_iter()
        .map(|c| {
            let pos = IVec3::from(c);
            if erase { Edit::Erase { pos, kind: None } } else { Edit::Place(LevelTile { kind, grid_pos: pos, facing: 0, style }) }
        })
        .collect();
    apply_edits(w, if erase { "Erase Tiles" } else { "Paint Tiles" }, edits)
}

/// Open / close / toggle (`open: None`) the door in a cell. Not part of undo history; locked doors refuse to open.
pub fn cmd_toggle_door(w: &mut World, pos: [i32; 3], open: Option<bool>) -> Result<String, String> {
    let pos = IVec3::from(pos);
    let found = w.query_filtered::<(Entity, &LevelTile), (With<Door>, Without<Deleted>)>().iter(w).find(|(_, t)| t.grid_pos == pos && t.kind == TileKind::Doorway).map(|(e, _)| e);
    let e = found.ok_or_else(|| format!("no door at ({}, {}, {})", pos.x, pos.y, pos.z))?;
    let mut door = w.get_mut::<Door>(e).ok_or("door component missing")?;
    let want = open.unwrap_or(!door.is_open);
    if want && door.is_locked {
        return Err("door is locked".into());
    }
    door.is_open = want;
    Ok(format!("door at ({}, {}, {}) {}", pos.x, pos.y, pos.z, if want { "open" } else { "closed" }))
}

pub fn cmd_set_grid(w: &mut World, enabled: Option<bool>, cell_size: Option<f32>, floor_level: Option<i32>, floor_height: Option<f32>, angle_snap_deg: Option<f32>) -> Result<String, String> {
    let mut g = w.resource_mut::<GridSettings>();
    g.enabled = enabled.unwrap_or(g.enabled);
    g.cell_size = cell_size.map_or(g.cell_size, |c| c.clamp(0.5, 16.0));
    g.floor_level = floor_level.map_or(g.floor_level, |l| l.clamp(-8, 32));
    g.floor_height = floor_height.map_or(g.floor_height, |h| h.clamp(1.5, 12.0));
    g.angle_snap_deg = angle_snap_deg.map_or(g.angle_snap_deg, |a| a.clamp(1.0, 90.0));
    Ok(format!("grid snap={} cell={} floor={} (Y={:.2}) floor_height={}", g.enabled, g.cell_size, g.floor_level, g.plane_y(), g.floor_height))
}

/// Two adjoining rooms with a doorway, columns, torches and a stair to a second storey (acceptance scene / "Build demo" button).
pub fn demo_tavern_commands(style: WallStyle) -> Vec<StudioCommand> {
    use StudioCommand::*;
    let place = |kind, pos: [i32; 3], facing, style| PlaceLevelTile { kind, pos, facing, style: Some(style) };
    vec![
        SetGrid { enabled: Some(true), cell_size: Some(2.0), floor_level: Some(0), floor_height: Some(3.5), angle_snap_deg: None },
        BuildRoom { min: [-5, -3], max: [0, 2], floor: 0, style },
        BuildRoom { min: [0, -3], max: [5, 2], floor: 0, style: WallStyle::CastleBrick },
        place(TileKind::Doorway, [0, 0, 0], Some(1), style),
        place(TileKind::Pillar, [-3, 0, -1], None, style),
        place(TileKind::Pillar, [-3, 0, 1], None, style),
        place(TileKind::Pillar, [3, 0, -1], None, WallStyle::CastleBrick),
        place(TileKind::Pillar, [3, 0, 1], None, WallStyle::CastleBrick),
        place(TileKind::Torch, [-5, 0, 0], Some(1), style),
        place(TileKind::Torch, [-5, 0, -2], Some(1), style),
        place(TileKind::Torch, [-1, 0, -3], Some(0), style),
        place(TileKind::Torch, [5, 0, 0], Some(3), WallStyle::CastleBrick),
        place(TileKind::Torch, [2, 0, 2], Some(2), WallStyle::CastleBrick),
        place(TileKind::Torch, [0, 0, -2], Some(3), style),
        place(TileKind::Stairs, [1, 0, -2], Some(0), WallStyle::CastleBrick),
        SetGrid { enabled: None, cell_size: None, floor_level: Some(1), floor_height: None, angle_snap_deg: None },
        BuildRoom { min: [3, -3], max: [5, 0], floor: 1, style: WallStyle::CastleBrick },
        ToggleDoor { pos: [0, 0, 0], open: Some(true) },
    ]
}

// ───────────────────────────── Visuals ─────────────────────────────

struct StyleMats {
    m: [Handle<StandardMaterial>; 7],
}

#[derive(Resource)]
pub struct LevelKit {
    cube: Handle<Mesh>,
    cyl: Handle<Mesh>,
    sphere: Handle<Mesh>,
    styles: [StyleMats; 3],
}

impl LevelKit {
    fn mat(&self, s: WallStyle, m: Mat) -> Handle<StandardMaterial> {
        self.styles[s as usize].m[m as usize].clone()
    }
}

#[derive(Component)]
struct TileVisual;

/// Pivot of a door leaf; rotates towards 90° when the owning `Door` is open.
#[derive(Component)]
struct DoorHinge {
    tile: Entity,
}

fn build_kit(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let mut m = |c: (f32, f32, f32), rough: f32, metal: f32| {
        mats.add(StandardMaterial { base_color: Color::srgb(c.0, c.1, c.2), perceptual_roughness: rough, metallic: metal, ..default() })
    };
    let iron = m((0.07, 0.07, 0.08), 0.55, 0.85);
    let mut flame_mat = StandardMaterial { base_color: Color::srgb(1.0, 0.72, 0.25), emissive: LinearRgba::rgb(12.0, 5.0, 1.0), unlit: false, ..default() };
    flame_mat.perceptual_roughness = 1.0;
    let flame = mats.add(flame_mat);
    // order matches `Mat`: Wall, Trim, Floor, Joint, Wood, Iron, Flame
    let mut set = |wall, trim, floor, joint, wood, rw: f32| {
        let mut mm = |c: (f32, f32, f32), r: f32| mats.add(StandardMaterial { base_color: Color::srgb(c.0, c.1, c.2), perceptual_roughness: r, ..default() });
        StyleMats { m: [mm(wall, rw), mm(trim, rw), mm(floor, rw), mm(joint, 1.0), mm(wood, 0.8), iron.clone(), flame.clone()] }
    };
    let styles = [
        set((0.40, 0.41, 0.43), (0.25, 0.26, 0.28), (0.34, 0.35, 0.37), (0.11, 0.115, 0.125), (0.30, 0.19, 0.10), 0.95),
        set((0.52, 0.34, 0.19), (0.27, 0.16, 0.08), (0.45, 0.30, 0.16), (0.14, 0.09, 0.05), (0.33, 0.21, 0.11), 0.85),
        set((0.58, 0.31, 0.24), (0.66, 0.60, 0.50), (0.52, 0.49, 0.44), (0.30, 0.22, 0.18), (0.30, 0.19, 0.10), 0.9),
    ];
    commands.insert_resource(LevelKit { cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)), cyl: meshes.add(Cylinder::new(1.0, 1.0)), sphere: meshes.add(Sphere::new(1.0)), styles });
}

#[allow(clippy::type_complexity)]
fn attach_visuals(
    mut commands: Commands,
    kit: Option<Res<LevelKit>>,
    g: Res<GridSettings>,
    mut q: Query<(Entity, &LevelTile, Option<&Door>, &mut Transform, &mut SceneObject, Option<&mut Name>, Option<&Children>), Changed<LevelTile>>,
    old: Query<(), With<TileVisual>>,
) {
    let Some(kit) = kit else { return };
    for (e, tile, door, mut tf, mut so, name, children) in &mut q {
        for c in children.into_iter().flatten().copied().filter(|c| old.contains(*c)) {
            commands.entity(c).despawn();
        }
        *tf = tile_transform(tile, &g);
        so.name = tile_name(tile);
        so.aabb_half_extents = Vec3::new(g.cell_size * 0.5, g.floor_height * 0.5, g.cell_size * 0.5);
        if let Some(mut n) = name {
            *n = Name::new(so.name.clone());
        }
        let (parts, hinge) = tile_parts(tile, g.cell_size, g.floor_height);
        let open = door.is_some_and(|d| d.is_open);
        let pivot = hinge.map(|h| {
            let rot = if open { Quat::from_rotation_y(-FRAC_PI_2) } else { Quat::IDENTITY };
            commands.spawn((TileVisual, DoorHinge { tile: e }, Transform { translation: h, rotation: rot, ..default() }, Visibility::default(), ChildOf(e))).id()
        });
        for part in parts {
            let mesh = match part.shape {
                Shape::Cube => kit.cube.clone(),
                Shape::Cyl => kit.cyl.clone(),
                Shape::Sphere => kit.sphere.clone(),
            };
            if part.light {
                commands.spawn((
                    TileVisual,
                    PointLight { color: Color::srgb(1.0, 0.62, 0.28), intensity: 55_000.0, range: 15.0, radius: 0.06, shadow_maps_enabled: false, ..default() },
                    part.tf,
                    ChildOf(e),
                ));
                continue;
            }
            let (parent, mut ptf) = match (part.leaf, pivot) {
                (true, Some(p)) => (p, part.tf),
                _ => (e, part.tf),
            };
            if part.leaf {
                if let Some(h) = hinge {
                    ptf.translation -= h;
                }
            }
            let mut ent = commands.spawn((TileVisual, Mesh3d(mesh), MeshMaterial3d(kit.mat(tile.style, part.mat)), ptf, ChildOf(parent)));
            if part.mat == Mat::Flame {
                ent.insert(bevy::light::NotShadowCaster);
            }
        }
    }
}

fn animate_doors(time: Res<Time>, doors: Query<&Door>, mut hinges: Query<(&DoorHinge, &mut Transform)>) {
    let k = 1.0 - (-time.delta_secs() * 7.0).exp();
    for (h, mut tf) in &mut hinges {
        let Ok(d) = doors.get(h.tile) else { continue };
        let target = if d.is_open { Quat::from_rotation_y(-FRAC_PI_2) } else { Quat::IDENTITY };
        if tf.rotation.angle_between(target) > 1e-3 {
            tf.rotation = tf.rotation.slerp(target, k);
        }
    }
}

// ───────────────────────────── Plugin ─────────────────────────────

pub struct LevelBuilderPlugin;

impl Plugin for LevelBuilderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GridSettings>()
            .init_resource::<LevelBuilderState>()
            .register_type::<GridSettings>()
            .register_type::<TileKind>()
            .register_type::<WallStyle>()
            .register_scene_component::<LevelTile>()
            .register_scene_component::<Door>()
            .register_panel_def(PanelDef::new("level_builder", "Level Builder", PanelLocation::RightInspector, level_panel).order(30))
            .register_tool(
                ToolDef::new(TOOL_LEVEL_BUILDER, "Level Builder").hotkey(KeyCode::KeyL).panel("level_builder").hint("Rooms, walls, doors, stairs on a snap grid"),
                level_tool_system.after(gizmo_system),
            )
            .add_systems(Startup, build_kit)
            .add_systems(Update, (attach_visuals, animate_doors).chain());
    }
}

// ───────────────────────────── Interactive tool ─────────────────────────────

fn ray_aabb(o: Vec3, d: Vec3, min: Vec3, max: Vec3) -> Option<f32> {
    let inv = Vec3::ONE / d;
    let (t1, t2) = ((min - o) * inv, (max - o) * inv);
    let (tn, tf) = (t1.min(t2).max_element(), t1.max(t2).min_element());
    (tf >= tn.max(0.0)).then_some(tn.max(0.0))
}

#[allow(clippy::too_many_arguments)]
fn level_tool_system(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut egui: EguiContexts,
    cam: Single<(&Camera, &GlobalTransform, &EditorCamera)>,
    g: Res<GridSettings>,
    mut st: ResMut<LevelBuilderState>,
    tiles: Query<(&LevelTile, &Transform), Without<Deleted>>,
    mut queue: ResMut<StudioCommandQueue>,
    mut gizmos: Gizmos,
) {
    let (camera, gtf, ecam) = *cam;
    let over_ui = egui.ctx_mut().map(|c| c.is_pointer_over_egui() || c.egui_wants_pointer_input()).unwrap_or(false);
    let typing = egui.ctx_mut().map(|c| c.egui_wants_keyboard_input()).unwrap_or(false);
    let alt = keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]);
    if !typing && keys.just_pressed(KeyCode::KeyT) {
        st.facing = (st.facing + 1) % 4;
    }
    if keys.just_pressed(KeyCode::Escape) {
        st.anchor = None;
        st.stroke.clear();
    }

    let ray = window.cursor_position().and_then(|c| camera.viewport_to_world(gtf, c).ok());
    let mode = st.mode;
    let level = g.floor_level;
    let (cs, fh, py) = (g.cell_size, g.floor_height, g.plane_y());
    st.hover = None;
    let mut hit_point = None;
    if let (Some(r), false, false) = (ray, over_ui, ecam.mode.is_some() || alt) {
        let (o, d) = (r.origin, *r.direction);
        // wall-targeting modes pick the wall box under the cursor first, then fall back to the storey plane
        if matches!(mode, BuildMode::Door | BuildMode::Arch | BuildMode::Torch) {
            let best = tiles
                .iter()
                .filter(|(t, _)| t.kind.slot() == 1 && t.kind != TileKind::Pillar)
                .filter_map(|(t, tf)| {
                    let c = tf.translation + Vec3::Y * fh * 0.5;
                    let half = tf.rotation * Vec3::new(cs * 0.5, fh * 0.5, WALL_THICKNESS * 0.5);
                    let half = half.abs();
                    ray_aabb(o, d, c - half, c + half).map(|dist| (dist, t.grid_pos, tf.translation, tf.rotation))
                })
                .min_by(|a, b| a.0.total_cmp(&b.0));
            if let Some((dist, pos, center, rot)) = best {
                let p = o + d * dist;
                st.hover = Some(pos);
                hit_point = Some(p);
                let n = rot * Vec3::Z;
                st.hover_side = Some(if (p - center).dot(n) >= 0.0 { 0 } else { 2 });
            }
        }
        if st.hover.is_none() && d.y.abs() > 1e-5 {
            let t = (py - o.y) / d.y;
            if t > 0.0 && t < 4000.0 {
                let p = o + d * t;
                st.hover = Some(g.world_to_cell(p, level));
                hit_point = Some(p);
                st.hover_side = None;
            }
        }
    }

    draw_level_grid(&mut gizmos, &g, st.hover, hit_point.unwrap_or(ecam.pivot));
    let Some(hover) = st.hover else {
        if mouse.just_released(MouseButton::Left) {
            st.anchor = None;
            st.stroke.clear();
        }
        return;
    };
    let style = st.style;
    let c3 = |p: IVec3| g.cell_center(p);
    let ok = Color::srgb(0.25, 1.0, 0.75);
    let bad = Color::srgb(1.0, 0.45, 0.3);

    match mode {
        BuildMode::RoomBox | BuildMode::WallLine => {
            if mouse.just_pressed(MouseButton::Left) {
                st.anchor = Some(hover);
            }
            if let Some(a) = st.anchor {
                let (a2, b2) = (IVec2::new(a.x, a.z), IVec2::new(hover.x, hover.z));
                if mode == BuildMode::RoomBox {
                    let ((x0, x1), (z0, z1)) = (ordered(a2.x, b2.x), ordered(a2.y, b2.y));
                    let size = Vec3::new((x1 - x0 + 1) as f32 * cs, fh, (z1 - z0 + 1) as f32 * cs);
                    let center = Vec3::new((x0 + x1) as f32 * 0.5 * cs, py + fh * 0.5, (z0 + z1) as f32 * 0.5 * cs);
                    gizmos.cube(Transform::from_translation(center).with_scale(size), ok);
                    gizmos.cube(Transform::from_translation(center - Vec3::Y * (fh * 0.5 - 0.02)).with_scale(Vec3::new(size.x, 0.04, size.z)), ok);
                } else if let Ok(line) = wall_line_tiles(a2, b2, level, style) {
                    for t in &line {
                        let c = c3(t.grid_pos) + Vec3::Y * fh * 0.5;
                        let rot = tile_rotation(t.facing);
                        gizmos.cube(Transform { translation: c, rotation: rot, scale: Vec3::new(cs, fh, WALL_THICKNESS) }, ok);
                    }
                }
                if mouse.just_released(MouseButton::Left) {
                    let cmd = if mode == BuildMode::RoomBox {
                        StudioCommand::BuildRoom { min: [a2.x, a2.y], max: [b2.x, b2.y], floor: level, style }
                    } else {
                        StudioCommand::BuildWallLine { from: [a2.x, a2.y], to: [b2.x, b2.y], floor: level, style }
                    };
                    queue.push(cmd);
                    st.anchor = None;
                }
            }
        }
        BuildMode::FloorBrush | BuildMode::Eraser => {
            let col = if mode == BuildMode::Eraser { bad } else { ok };
            if mouse.just_pressed(MouseButton::Left) {
                st.stroke.clear();
            }
            if mouse.pressed(MouseButton::Left) && st.stroke.last() != Some(&hover) && !st.stroke.contains(&hover) {
                st.stroke.push(hover);
            }
            for p in st.stroke.iter().chain(std::iter::once(&hover)) {
                gizmos.cube(Transform::from_translation(c3(*p) + Vec3::Y * 0.05).with_scale(Vec3::new(cs, 0.1, cs)), col);
            }
            if mouse.just_released(MouseButton::Left) && !st.stroke.is_empty() {
                let cells = st.stroke.drain(..).map(|p| p.to_array()).collect();
                queue.push(StudioCommand::PaintLevelTiles { kind: TileKind::Floor, cells, style, erase: mode == BuildMode::Eraser });
            }
        }
        _ => {
            let Some(kind) = mode.place_kind() else { return };
            let wall_mode = matches!(kind, TileKind::Doorway | TileKind::Arch | TileKind::Torch);
            let on_wall = tiles.iter().any(|(t, _)| t.grid_pos == hover && t.kind.slot() == 1 && t.kind != TileKind::Pillar);
            let facing = match (kind, st.hover_side, on_wall) {
                (TileKind::Torch, Some(side), true) => tiles.iter().find(|(t, _)| t.grid_pos == hover && t.kind.slot() == 1).map(|(t, _)| (t.facing + side) % 4),
                (TileKind::Torch, ..) => Some(st.facing),
                (TileKind::Doorway | TileKind::Arch, _, true) => None,
                _ => Some(st.facing),
            };
            let valid = !wall_mode || on_wall || kind != TileKind::Torch;
            let col = if valid { ok } else { bad };
            let rot = tile_rotation(facing.unwrap_or(st.facing));
            let size = if kind == TileKind::Stairs { Vec3::new(cs * 2.0, fh, cs) } else { Vec3::new(cs, fh, cs) };
            let shift = if kind == TileKind::Stairs { rot * Vec3::X * cs * 0.5 } else { Vec3::ZERO };
            gizmos.cube(Transform { translation: c3(hover) + shift + Vec3::Y * fh * 0.5, rotation: rot, scale: size }, col);
            if mouse.just_pressed(MouseButton::Left) && valid {
                queue.push(StudioCommand::PlaceLevelTile { kind, pos: hover.to_array(), facing, style: if on_wall { None } else { Some(style) } });
            }
        }
    }
}

/// Storey-plane grid around `focus` (cells of `cell_size`, raised to `floor_level`) plus the hovered cell.
fn draw_level_grid(gizmos: &mut Gizmos, g: &GridSettings, hover: Option<IVec3>, focus: Vec3) {
    let (cs, y) = (g.cell_size, g.plane_y() + 0.015);
    let (cx, cz) = ((focus.x / cs).round() as i32, (focus.z / cs).round() as i32);
    let r = 14;
    let col = Color::srgba(0.35, 0.85, 0.80, 0.22);
    let strong = Color::srgba(0.35, 0.95, 0.85, 0.45);
    for i in -r..=r {
        let (x, z) = ((cx + i) as f32 * cs - cs * 0.5, (cz + i) as f32 * cs - cs * 0.5);
        let c = if (cx + i) % 5 == 0 { strong } else { col };
        gizmos.line(Vec3::new(x, y, (cz - r) as f32 * cs - cs * 0.5), Vec3::new(x, y, (cz + r) as f32 * cs - cs * 0.5), c);
        let c = if (cz + i) % 5 == 0 { strong } else { col };
        gizmos.line(Vec3::new((cx - r) as f32 * cs - cs * 0.5, y, z), Vec3::new((cx + r) as f32 * cs - cs * 0.5, y, z), c);
    }
    if let Some(h) = hover {
        let p = g.cell_center(h);
        gizmos.cube(Transform::from_translation(Vec3::new(p.x, y + 0.02, p.z)).with_scale(Vec3::new(cs, 0.04, cs)), Color::srgb(1.0, 0.85, 0.3));
    }
}

// ───────────────────────────── Panel ─────────────────────────────

fn level_panel(ui: &mut egui::Ui, d: &mut UiData, f: &Filter) {
    let _ = f;
    ui.label(egui::RichText::new("Grid & Storey").strong().color(TEAL));
    ui.checkbox(&mut d.gset.enabled, "Grid Snap (move: cell, Ctrl = ½ cell; rotate: angle step)");
    ui.add(egui::Slider::new(&mut d.gset.cell_size, 0.5..=8.0).text("cell size"));
    ui.add(egui::Slider::new(&mut d.gset.angle_snap_deg, 1.0..=90.0).text("angle snap°"));
    ui.add(egui::Slider::new(&mut d.gset.floor_height, 2.0..=8.0).text("floor height"));
    ui.horizontal(|ui| {
        ui.label("Floor level");
        if ui.button("−").clicked() {
            d.gset.floor_level = (d.gset.floor_level - 1).max(-8);
        }
        ui.label(egui::RichText::new(format!("{}  (Y = {:.1})", d.gset.floor_level, d.gset.plane_y())).strong());
        if ui.button("+").clicked() {
            d.gset.floor_level = (d.gset.floor_level + 1).min(32);
        }
    });
    ui.separator();
    ui.label(egui::RichText::new("Mode  (hotkey L)").strong().color(TEAL));
    ui.horizontal_wrapped(|ui| {
        for m in BuildMode::ALL {
            if chip(ui, d.lbs.mode == m, m.label()).clicked() {
                d.lbs.mode = m;
                d.lbs.anchor = None;
            }
        }
    });
    ui.label(egui::RichText::new("Style").strong().color(TEAL));
    ui.horizontal_wrapped(|ui| {
        for s in WallStyle::ALL {
            if chip(ui, d.lbs.style == s, s.label()).clicked() {
                d.lbs.style = s;
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label("Facing (T)");
        for i in 0..4u8 {
            if chip(ui, d.lbs.facing == i, ["+Z / E", "+X", "−Z", "−X"][i as usize]).clicked() {
                d.lbs.facing = i;
            }
        }
    });
    ui.small("Room Box / Wall Line: drag on the grid. Floor Brush / Eraser: paint cells. Door / Arch / Torch: click a wall. Esc cancels.");
    ui.separator();
    ui.label(egui::RichText::new("Doors").strong().color(TEAL));
    ui.small("Toggle `is_open` on a Door in the Objects inspector, or use the ToggleDoor command.");
    ui.separator();
    if ui.button("Build demo tavern (2 rooms, door, stairs, torches)").clicked() {
        let style = d.lbs.style;
        for c in demo_tavern_commands(style) {
            d.queue.push(c);
        }
    }
}

// ───────────────────────────── Tests ─────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gizmo_3d::SceneHistory;
    use crate::navigation::{NavCell, NavGrid};
    use bevy::ecs::system::RunSystemOnce;

    fn app() -> App {
        let mut app = App::new();
        app.init_resource::<GridSettings>().init_resource::<SceneHistory>().init_resource::<NextObjectId>();
        app
    }

    fn live_count(app: &mut App) -> usize {
        app.world_mut().query_filtered::<(), (With<LevelTile>, Without<Deleted>)>().iter(app.world()).count()
    }

    fn undo(app: &mut App) {
        let states = app.world_mut().resource_mut::<SceneHistory>().undo().expect("one history entry");
        app.world_mut()
            .run_system_once(move |mut q: Query<(&mut Transform, &mut Visibility)>, mut c: Commands| crate::gizmo_3d::apply_states(&states, &mut q, &mut c))
            .unwrap();
    }

    #[test]
    fn room_box_builds_floor_walls_pillars_and_undoes_in_one_step() {
        let mut app = app();
        let n = room_tiles(IVec2::new(-2, -1), IVec2::new(2, 1), 0, WallStyle::StoneDungeon).unwrap().len();
        // 5x3 floor + perimeter of 12 cells (4 pillars + 8 walls)
        assert_eq!(n, 15 + 12);
        cmd_build_room(app.world_mut(), [2, 1], [-2, -1], 0, WallStyle::StoneDungeon).unwrap();
        assert_eq!(live_count(&mut app), 27);
        assert_eq!(app.world().resource::<SceneHistory>().undo_len(), 1, "whole room = one transaction");
        undo(&mut app);
        assert_eq!(live_count(&mut app), 0);
        let states = app.world_mut().resource_mut::<SceneHistory>().redo().unwrap();
        assert_eq!(states.iter().filter(|s| s.alive).count(), 27);
    }

    #[test]
    fn placing_replaces_in_slot_and_is_idempotent() {
        let mut app = app();
        cmd_build_wall_line(app.world_mut(), [0, 0], [3, 0], 0, WallStyle::WoodTavern).unwrap();
        assert_eq!(live_count(&mut app), 4);
        cmd_place_tile(app.world_mut(), TileKind::Doorway, [1, 0, 0], None, None).unwrap();
        assert_eq!(live_count(&mut app), 4, "wall replaced by doorway");
        let kinds: Vec<_> = app.world_mut().query_filtered::<&LevelTile, Without<Deleted>>().iter(app.world()).map(|t| t.kind).collect();
        assert_eq!(kinds.iter().filter(|k| **k == TileKind::Doorway).count(), 1);
        assert!(cmd_place_tile(app.world_mut(), TileKind::Doorway, [1, 0, 0], None, None).is_err(), "identical tile = no-op");
        undo(&mut app);
        assert_eq!(app.world_mut().query_filtered::<&LevelTile, Without<Deleted>>().iter(app.world()).filter(|t| t.kind == TileKind::Wall).count(), 4);
    }

    #[test]
    fn door_toggles_navigation_and_respects_lock() {
        let mut app = app();
        cmd_build_wall_line(app.world_mut(), [-4, 0], [4, 0], 0, WallStyle::StoneDungeon).unwrap();
        cmd_place_tile(app.world_mut(), TileKind::Doorway, [0, 0, 0], None, None).unwrap();
        let cs = 2.0;
        let raster = |app: &mut App| {
            let mut g = NavGrid::new(40.0, 0.5);
            let fps: Vec<_> = app
                .world_mut()
                .query_filtered::<(&LevelTile, Option<&Door>, &Transform), Without<Deleted>>()
                .iter(app.world())
                .filter_map(|(t, d, tf)| tile_footprint(t, d, tf, cs))
                .collect();
            for fp in &fps {
                g.block(fp, 0.35);
            }
            g
        };
        let (a, b) = (Vec2::new(0.0, -3.0), Vec2::new(0.0, 3.0));
        let wall_only = Vec2::new(4.0, 0.0);
        let closed = raster(&mut app);
        assert_eq!(closed.at(Vec2::new(0.0, 0.0)), NavCell::Blocked, "closed door blocks");
        assert_eq!(closed.at(wall_only), NavCell::Blocked);
        assert!(closed.find_path(a, b).is_none_or(|p| p.length > 12.0), "no way through a closed door");
        cmd_toggle_door(app.world_mut(), [0, 0, 0], Some(true)).unwrap();
        let open = raster(&mut app);
        assert_ne!(open.at(Vec2::new(0.0, 0.0)), NavCell::Blocked, "open door passable");
        assert_eq!(open.at(wall_only), NavCell::Blocked, "walls stay solid");
        let path = open.find_path(a, b).expect("path through open door");
        assert!(path.length < 8.0, "goes through the doorway, got {}", path.length);
        // locked doors refuse to open
        cmd_toggle_door(app.world_mut(), [0, 0, 0], Some(false)).unwrap();
        let e = app.world_mut().query_filtered::<Entity, With<Door>>().single(app.world()).unwrap();
        app.world_mut().get_mut::<Door>(e).unwrap().is_locked = true;
        assert!(cmd_toggle_door(app.world_mut(), [0, 0, 0], Some(true)).is_err());
        assert!(cmd_toggle_door(app.world_mut(), [9, 0, 9], None).is_err());
    }

    #[test]
    fn pillars_block_arches_and_floors_do_not_and_upper_floors_are_ignored() {
        let g = GridSettings::default();
        let tf = |p: IVec3| tile_transform(&LevelTile { grid_pos: p, ..default() }, &g);
        let t = |kind, y| LevelTile { kind, grid_pos: IVec3::new(1, y, 1), ..default() };
        assert!(tile_footprint(&t(TileKind::Pillar, 0), None, &tf(IVec3::new(1, 0, 1)), 2.0).is_some());
        assert!(tile_footprint(&t(TileKind::Arch, 0), None, &tf(IVec3::new(1, 0, 1)), 2.0).is_none());
        assert!(tile_footprint(&t(TileKind::Floor, 0), None, &tf(IVec3::new(1, 0, 1)), 2.0).is_none());
        assert!(tile_footprint(&t(TileKind::Wall, 1), None, &tf(IVec3::new(1, 1, 1)), 2.0).is_none());
    }

    #[test]
    fn grid_settings_defaults_and_floor_plane() {
        let mut g = GridSettings::default();
        assert_eq!((g.cell_size, g.floor_height, g.angle_snap_deg), (2.0, 3.5, 15.0));
        g.floor_level = 2;
        assert_eq!(g.plane_y(), 7.0);
        assert_eq!(g.snap(2.9), 2.0);
        assert_eq!(g.snap(3.1), 4.0);
        assert_eq!(g.world_to_cell(Vec3::new(4.9, 0.0, -2.2), 2), IVec3::new(2, 2, -1));
        assert_eq!(g.move_step(false), None);
        g.enabled = true;
        assert_eq!((g.move_step(false), g.move_step(true)), (Some(2.0), Some(1.0)));
    }

    #[test]
    fn every_kind_and_style_has_geometry_and_door_has_leaf() {
        for style in WallStyle::ALL {
            for kind in [TileKind::Floor, TileKind::Wall, TileKind::Doorway, TileKind::Arch, TileKind::Pillar, TileKind::Stairs, TileKind::Roof, TileKind::Torch] {
                let (parts, hinge) = tile_parts(&LevelTile { kind, style, ..default() }, 2.0, 3.5);
                assert!(!parts.is_empty(), "{kind:?}/{style:?}");
                assert_eq!(hinge.is_some(), kind == TileKind::Doorway);
                assert_eq!(parts.iter().any(|p| p.leaf), kind == TileKind::Doorway);
                assert_eq!(parts.iter().any(|p| p.light), kind == TileKind::Torch);
            }
        }
    }

    #[test]
    fn demo_commands_roundtrip_json() {
        let cmds = demo_tavern_commands(WallStyle::StoneDungeon);
        let json = serde_json::to_string(&cmds).unwrap();
        let back: Vec<StudioCommand> = serde_json::from_str(&json).unwrap();
        assert_eq!(back, cmds);
    }

    #[test]
    fn wall_line_follows_dominant_axis_both_directions() {
        let l = wall_line_tiles(IVec2::new(3, 0), IVec2::new(0, 1), 0, WallStyle::StoneDungeon).unwrap();
        assert_eq!(l.iter().map(|t| t.grid_pos.x).collect::<Vec<_>>(), [3, 2, 1, 0]);
        assert!(l.iter().all(|t| t.facing == 0 && t.grid_pos.z == 0));
        let l = wall_line_tiles(IVec2::new(0, 2), IVec2::new(1, -1), 0, WallStyle::StoneDungeon).unwrap();
        assert_eq!(l.iter().map(|t| t.grid_pos.z).collect::<Vec<_>>(), [2, 1, 0, -1]);
        assert!(l.iter().all(|t| t.facing == 1));
        assert!(room_tiles(IVec2::ZERO, IVec2::splat(100), 0, WallStyle::StoneDungeon).is_err());
    }
}
