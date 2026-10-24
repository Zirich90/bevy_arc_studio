//! Prefabs (E5): a saved group of editor objects (`assets/prefabs/<name>.prefab.json`) that can be stamped into the scene again.
//!
//! Format: entities relative to a pivot (centre of the selection's footprint, snapped to a grid cell, at the lowest point). Free
//! objects keep local transforms and hierarchy; `LevelTile`s store cell offsets, so a spawn re-snaps them to the grid and rotates
//! them in quarter turns. A spawned instance is a root `PrefabInstance` node holding the free objects as children; tiles stay
//! world-space grid cells (the tile system owns their transforms) and are grouped by the Scenetree like any other tile.
//! `SaveSelectionAsPrefab` / `SpawnPrefab` are `StudioCommand`s; a spawn is one `SceneHistory` step.

use std::f32::consts::FRAC_PI_2;
use std::path::{Path, PathBuf};

use bevy::ecs::system::SystemState;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::arc_core::WorldSpec;
use crate::gizmo_3d::{GizmoState, ObjState, SceneHistory};
use crate::level_builder::{self as lb, Door, Edit, GridSettings, LevelTile, TileKind};
use crate::props::{NextObjectId, ObjectCategory, SceneObject, UserProp, assets_dir};
use crate::selection::{Deleted, EditorSelection};
use crate::studio::Spawner;
use crate::terrain_render::TerrainGrid;
use crate::world_io::SceneComponentAppExt;

pub const PREFAB_EXT: &str = ".prefab.json";
pub const PREFAB_VERSION: u32 = 1;

/// Root node of a spawned prefab; `prefab_path` is relative to `assets/` (e.g. `prefabs/tavern_corner.prefab.json`).
#[derive(Component, Reflect, Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[reflect(Component, Default)]
pub struct PrefabInstance {
    pub prefab_path: String,
}

/// Runtime marker: the free objects of this instance exist (absent right after a scene load, which triggers re-instancing).
#[derive(Component)]
struct PrefabHydrated;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PrefabLight {
    pub color: [f32; 4],
    pub intensity: f32,
    pub range: f32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PrefabEntity {
    /// Index into `entities` of the parent member (free objects only).
    pub parent: Option<usize>,
    /// `UserProp.kind`: pine | boulder | campfire | pier | lantern | `model:<path .glb>` | `level`.
    pub kind: String,
    pub name: String,
    pub pos: [f32; 3],
    pub rot: [f32; 4],
    pub scale: [f32; 3],
    /// `grid_pos` is the cell offset from the pivot cell (y = storey above the lowest storey).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tile: Option<LevelTile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub door: Option<Door>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub light: Option<PrefabLight>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PrefabFile {
    pub version: u32,
    pub name: String,
    pub cell_size: f32,
    pub entities: Vec<PrefabEntity>,
}

/// One source object of `build_prefab`: world transform for roots, local transform for members with a `parent`.
#[derive(Clone, Debug)]
pub struct Member {
    pub parent: Option<usize>,
    pub kind: String,
    pub name: String,
    pub tf: Transform,
    pub tile: Option<LevelTile>,
    pub door: Option<Door>,
    pub light: Option<PrefabLight>,
}

// ───────────────────────────── Pure core ─────────────────────────────

/// Pivot and relative coordinates; `Err` when there is nothing to save.
pub fn build_prefab(name: &str, members: Vec<Member>, g: &GridSettings) -> Result<PrefabFile, String> {
    if members.is_empty() {
        return Err("nothing to save: select objects first".into());
    }
    let cs = g.cell_size;
    let (mut lo, mut hi) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
    let (mut base_y, mut min_level) = (f32::MAX, i32::MAX);
    for m in members.iter().filter(|m| m.parent.is_none() || m.tile.is_some()) {
        let p = match &m.tile {
            Some(t) => {
                min_level = min_level.min(t.grid_pos.y);
                g.cell_center(t.grid_pos)
            }
            None => {
                base_y = base_y.min(m.tf.translation.y);
                m.tf.translation
            }
        };
        lo = lo.min(Vec2::new(p.x, p.z));
        hi = hi.max(Vec2::new(p.x, p.z));
    }
    let mid = (lo + hi) * 0.5;
    let pivot_cell = IVec2::new((mid.x / cs).round() as i32, (mid.y / cs).round() as i32);
    let pivot = Vec3::new(pivot_cell.x as f32 * cs, if base_y == f32::MAX { 0.0 } else { base_y }, pivot_cell.y as f32 * cs);
    let min_level = if min_level == i32::MAX { 0 } else { min_level };
    let entities = members
        .into_iter()
        .map(|m| {
            let tile = m.tile.map(|t| LevelTile { grid_pos: IVec3::new(t.grid_pos.x - pivot_cell.x, t.grid_pos.y - min_level, t.grid_pos.z - pivot_cell.y), ..t });
            let (pos, parent) = match (&tile, m.parent) {
                (Some(_), _) => (Vec3::ZERO, None),
                (None, None) => (m.tf.translation - pivot, None),
                (None, p) => (m.tf.translation, p),
            };
            let (rot, scale) = if tile.is_some() { (Quat::IDENTITY, Vec3::ONE) } else { (m.tf.rotation, m.tf.scale) };
            PrefabEntity { parent, kind: m.kind, name: m.name, pos: pos.to_array(), rot: rot.to_array(), scale: scale.to_array(), tile, door: m.door, light: m.light }
        })
        .collect();
    Ok(PrefabFile { version: PREFAB_VERSION, name: name.to_string(), cell_size: cs, entities })
}

/// Number of 90° turns closest to `yaw_deg` (0..4).
pub fn quarter_turns(yaw_deg: f32) -> u8 {
    ((yaw_deg / 90.0).round() as i32).rem_euclid(4) as u8
}

/// Re-anchor a saved tile at `base` (pivot cell on the target storey) turned by `quarter` 90° steps about the pivot.
pub fn place_tile(t: &LevelTile, base: IVec3, quarter: u8) -> LevelTile {
    let r = Quat::from_rotation_y(quarter as f32 * FRAC_PI_2) * Vec3::new(t.grid_pos.x as f32, 0.0, t.grid_pos.z as f32);
    LevelTile { grid_pos: IVec3::new(base.x + r.x.round() as i32, base.y + t.grid_pos.y, base.z + r.z.round() as i32), facing: (t.facing + quarter) % 4, ..*t }
}

/// `tavern` / `prefabs/tavern` / `assets/prefabs/tavern.prefab.json` → `prefabs/tavern.prefab.json` (relative to `assets/`).
pub fn normalize_rel(path: &str) -> String {
    let p = path.trim().replace('\\', "/");
    let p = p.strip_prefix("assets/").unwrap_or(&p).to_string();
    let p = if p.contains('/') { p } else { format!("prefabs/{p}") };
    if p.ends_with(PREFAB_EXT) { p } else { format!("{p}{PREFAB_EXT}") }
}

fn half_extents(f: &PrefabFile) -> Vec3 {
    f.entities.iter().fold(Vec3::splat(1.0), |h, e| {
        let p = if let Some(t) = &e.tile { Vec3::new(t.grid_pos.x as f32 * f.cell_size, 0.0, t.grid_pos.z as f32 * f.cell_size) } else { Vec3::from(e.pos) };
        h.max(p.abs() + Vec3::new(1.0, 2.0, 1.0))
    })
}

// ───────────────────────────── World operations ─────────────────────────────

fn load_file(root: &Path, rel: &str) -> Result<PrefabFile, String> {
    let fs = root.join(rel);
    let json = std::fs::read_to_string(&fs).map_err(|e| format!("{}: {e}", fs.display()))?;
    serde_json::from_str(&json).map_err(|e| format!("{rel}: {e}"))
}

fn next_id(w: &mut World) -> u32 {
    let mut n = w.resource_mut::<NextObjectId>();
    n.0 += 1;
    n.0
}

/// Save the current selection (descendants included) as `<root>/<path>`; returns a result line.
pub fn save_selection(w: &mut World, root: &Path, path: &str) -> Result<String, String> {
    let selected = w.resource::<EditorSelection>().selected.iter().copied().collect();
    save_entities(w, root, path, selected)
}

/// Same as `save_selection` for an explicit entity set (the command bus captures the selection when the command is issued).
pub fn save_entities(w: &mut World, root: &Path, path: &str, selected: Vec<Entity>) -> Result<String, String> {
    let rel = normalize_rel(path);
    let name = rel.rsplit('/').next().and_then(|f| f.strip_suffix(PREFAB_EXT)).unwrap_or("prefab").to_string();
    let mut items: Vec<Entity> = Vec::new();
    let mut stack = selected;
    while let Some(e) = stack.pop() {
        if items.contains(&e) || w.get::<Deleted>(e).is_some() {
            continue;
        }
        stack.extend(w.get::<Children>(e).map(|c| c.iter().collect::<Vec<_>>()).unwrap_or_default());
        if w.get::<SceneObject>(e).is_some() && w.get::<UserProp>(e).is_some() {
            if w.get::<PrefabInstance>(e).is_some() {
                return Err("prefab instances cannot be nested in a prefab; save its contents instead".into());
            }
            items.push(e);
        }
    }
    items.sort_by_key(|e| w.get::<SceneObject>(*e).map_or(0, |s| s.id));
    let parent_of = |w: &World, e: Entity| -> Option<usize> {
        let mut cur = w.get::<ChildOf>(e).map(|c| c.parent());
        while let Some(p) = cur {
            if let Some(i) = items.iter().position(|x| *x == p) {
                return Some(i);
            }
            cur = w.get::<ChildOf>(p).map(|c| c.parent());
        }
        None
    };
    let members: Vec<Member> = items
        .iter()
        .map(|&e| Member {
            parent: parent_of(w, e),
            kind: w.get::<UserProp>(e).map(|u| u.kind.clone()).unwrap_or_default(),
            name: w.get::<SceneObject>(e).map(|s| s.name.clone()).unwrap_or_default(),
            tf: w.get::<Transform>(e).copied().unwrap_or_default(),
            tile: w.get::<LevelTile>(e).copied(),
            door: w.get::<Door>(e).copied(),
            light: w.get::<PointLight>(e).map(|l| PrefabLight { color: l.color.to_srgba().to_f32_array(), intensity: l.intensity, range: l.range }),
        })
        .collect();
    let g = w.resource::<GridSettings>().clone();
    let file = build_prefab(&name, members, &g)?;
    let fs = root.join(&rel);
    if let Some(dir) = fs.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let json = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
    std::fs::write(&fs, json).map_err(|e| format!("{}: {e}", fs.display()))?;
    Ok(format!("saved prefab {rel} ({} objects)", file.entities.len()))
}

/// Spawn the free (non-tile) members of `file` as children of `root`; returns the new entities.
fn spawn_members(w: &mut World, root: Entity, file: &PrefabFile) -> Vec<Entity> {
    let free: Vec<usize> = (0..file.entities.len()).filter(|i| file.entities[*i].tile.is_none()).collect();
    if free.is_empty() {
        return Vec::new();
    }
    let mut made: Vec<Option<Entity>> = vec![None; file.entities.len()];
    let mut ss = SystemState::<(Spawner, Res<TerrainGrid>, Res<WorldSpec>)>::new(w);
    {
        let Ok((mut sp, grid, spec)) = ss.get_mut(w) else { return Vec::new() };
        for &i in &free {
            made[i] = sp.spawn(&grid.0, &spec, &file.entities[i].kind, 0.0, 0.0, 0.0).map(|(e, _)| e);
        }
    }
    ss.apply(w);
    let mut out = Vec::new();
    for &i in &free {
        let (Some(e), pe) = (made[i], &file.entities[i]) else { continue };
        let parent = pe.parent.and_then(|p| made.get(p).copied().flatten()).unwrap_or(root);
        let tf = Transform { translation: Vec3::from(pe.pos), rotation: Quat::from_array(pe.rot), scale: Vec3::from(pe.scale) };
        let mut em = w.entity_mut(e);
        em.insert((tf, ChildOf(parent), Name::new(pe.name.clone())));
        // members belong to the instance; only the root is saved with the scene and re-instances them on load
        em.remove::<UserProp>();
        if let Some(mut so) = em.get_mut::<SceneObject>() {
            so.name = pe.name.clone();
        }
        if let Some(l) = &pe.light {
            em.insert(PointLight { color: Color::srgba(l.color[0], l.color[1], l.color[2], l.color[3]), intensity: l.intensity, range: l.range, ..default() });
        }
        out.push(e);
    }
    out
}

/// Instantiate `path` at `pos` turned by `yaw_deg` as ONE `SceneHistory` step and select the new root.
pub fn spawn_prefab(w: &mut World, root: &Path, path: &str, pos: Vec3, yaw_deg: f32) -> Result<String, String> {
    let rel = normalize_rel(path);
    let file = load_file(root, &rel)?;
    let g = w.resource::<GridSettings>().clone();
    let has_tiles = file.entities.iter().any(|e| e.tile.is_some());
    let snap_ground = w.get_resource::<GizmoState>().is_none_or(|s| s.snap_ground);
    let (x, z) = if g.enabled || has_tiles { (g.snap(pos.x), g.snap(pos.z)) } else { (pos.x, pos.z) };
    let y = match w.get_resource::<TerrainGrid>() {
        Some(t) if snap_ground => t.0.sample_height(x, z),
        _ => pos.y,
    };
    let quarter = quarter_turns(yaw_deg);
    let root_tf = Transform { translation: Vec3::new(x, y, z), rotation: Quat::from_rotation_y(yaw_deg.to_radians()), scale: Vec3::ONE };

    let id = next_id(w);
    let so = SceneObject { id, name: file.name.clone(), category: ObjectCategory::Structure, aabb_half_extents: half_extents(&file) };
    let root_e = w.spawn((Name::new(file.name.clone()), so, UserProp { kind: "prefab".into() }, PrefabInstance { prefab_path: rel.clone() }, PrefabHydrated, root_tf, Visibility::default())).id();
    let mut spawned = vec![(root_e, root_tf)];
    for e in spawn_members(w, root_e, &file) {
        spawned.push((e, w.get::<Transform>(e).copied().unwrap_or_default()));
    }

    // tiles: re-snapped onto the grid, one batch so replaced tiles undo together with the rest
    let base = g.world_to_cell(Vec3::new(x, 0.0, z), g.floor_level);
    let placed: Vec<LevelTile> = file.entities.iter().filter_map(|e| e.tile.as_ref()).map(|t| place_tile(t, base, quarter)).collect();
    let (mut before, mut after, added, _) = lb::apply_edits_raw(w, placed.iter().map(|t| Edit::Place(*t)).collect());
    for (pe, t) in file.entities.iter().filter(|e| e.tile.is_some()).zip(&placed) {
        if let (Some(d), TileKind::Doorway) = (pe.door, t.kind) {
            let found = w.query_filtered::<(Entity, &LevelTile), Without<Deleted>>().iter(w).find(|(_, o)| *o == t).map(|(e, _)| e);
            if let Some(e) = found {
                w.entity_mut(e).insert(d);
            }
        }
    }
    for (e, tf) in &spawned {
        before.push(ObjState { entity: *e, tf: *tf, alive: false });
        after.push(ObjState { entity: *e, tf: *tf, alive: true });
    }
    let n = spawned.len() - 1;
    w.resource_mut::<SceneHistory>().record(format!("Spawn Prefab {}", file.name), before, after);
    w.resource_mut::<EditorSelection>().set_one(root_e);
    Ok(format!("spawned prefab {rel} at ({x:.1}, {y:.1}, {z:.1}) yaw {yaw_deg:.0}: {n} objects, {added} tiles"))
}

/// Re-create the members of prefab roots that came from a scene file (the file stores only the root).
#[allow(clippy::type_complexity)]
fn rehydrate(mut commands: Commands, roots: Query<(Entity, &PrefabInstance), (Without<PrefabHydrated>, Without<Deleted>)>) {
    for (e, inst) in &roots {
        let rel = inst.prefab_path.clone();
        commands.entity(e).insert(PrefabHydrated);
        commands.queue(move |w: &mut World| match load_file(&assets_dir(), &rel) {
            Ok(file) => {
                if let Some(mut so) = w.get_mut::<SceneObject>(e) {
                    so.name = file.name.clone();
                }
                w.entity_mut(e).insert(Name::new(file.name.clone()));
                spawn_members(w, e, &file);
            }
            Err(err) => warn!("prefab {rel}: {err}"),
        });
    }
}

pub fn cmd_save_selection(w: &mut World, path: &str, selected: Vec<Entity>) -> Result<String, String> {
    save_entities(w, &assets_dir(), path, selected)
}

pub fn cmd_spawn_prefab(w: &mut World, path: &str, pos: Vec3, yaw_deg: f32) -> Result<String, String> {
    spawn_prefab(w, &assets_dir(), path, pos, yaw_deg)
}

/// Absolute path of `assets/` for display and scanning.
pub fn prefab_dir() -> PathBuf {
    assets_dir().join("prefabs")
}

pub struct PrefabsPlugin;

impl Plugin for PrefabsPlugin {
    fn build(&self, app: &mut App) {
        app.register_scene_component::<PrefabInstance>().add_systems(Update, rehydrate);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level_builder::WallStyle;

    fn tile(kind: TileKind, p: [i32; 3], facing: u8) -> LevelTile {
        LevelTile { kind, grid_pos: IVec3::from(p), facing, style: WallStyle::WoodTavern }
    }

    fn member(t: LevelTile) -> Member {
        Member { parent: None, kind: "level".into(), name: "t".into(), tf: Transform::from_translation(GridSettings::default().cell_center(t.grid_pos)), tile: Some(t), door: None, light: None }
    }

    #[test]
    fn build_makes_coordinates_relative_to_pivot() {
        let g = GridSettings::default();
        let tiles = vec![member(tile(TileKind::Floor, [4, 1, 6], 0)), member(tile(TileKind::Wall, [6, 2, 6], 1)), member(tile(TileKind::Floor, [5, 1, 8], 0))];
        let f = build_prefab("x", tiles, &g).unwrap();
        // footprint x 4..6, z 6..8 → pivot cell (5, 7); lowest storey 1
        let ps: Vec<_> = f.entities.iter().map(|e| e.tile.unwrap().grid_pos.to_array()).collect();
        assert_eq!(ps, [[-1, 0, -1], [1, 1, -1], [0, 0, 1]]);
        assert!(build_prefab("x", vec![], &g).is_err());
    }

    #[test]
    fn free_objects_are_relative_and_json_roundtrips() {
        let g = GridSettings::default();
        let m = |x: f32, y: f32, z: f32, parent| Member { parent, kind: "model:Duck.glb".into(), name: "Duck".into(), tf: Transform::from_xyz(x, y, z), tile: None, door: None, light: None };
        let f = build_prefab("duck", vec![m(10.0, 3.0, 20.0, None), m(14.0, 5.0, 20.0, None), m(0.0, 1.0, 0.0, Some(0))], &g).unwrap();
        assert_eq!(f.entities[0].pos, [-2.0, 0.0, 0.0]); // pivot (12, 3, 20)
        assert_eq!(f.entities[1].pos, [2.0, 2.0, 0.0]);
        assert_eq!((f.entities[2].parent, f.entities[2].pos), (Some(0), [0.0, 1.0, 0.0]));
        let back: PrefabFile = serde_json::from_str(&serde_json::to_string(&f).unwrap()).unwrap();
        assert_eq!(back, f);
    }

    #[test]
    fn tiles_rotate_about_the_pivot_in_quarter_turns() {
        let t = tile(TileKind::Wall, [2, 0, 0], 0);
        let base = IVec3::new(10, 1, 10);
        assert_eq!(place_tile(&t, base, 0).grid_pos, IVec3::new(12, 1, 10));
        assert_eq!(place_tile(&t, base, 1), LevelTile { grid_pos: IVec3::new(10, 1, 8), facing: 1, ..t });
        assert_eq!(place_tile(&t, base, 2).grid_pos, IVec3::new(8, 1, 10));
        assert_eq!(quarter_turns(-90.0), 3);
        assert_eq!(quarter_turns(450.0), 1);
    }

    #[test]
    fn rel_paths_are_normalised() {
        for p in ["tavern", "prefabs/tavern", "assets/prefabs/tavern.prefab.json", "assets\\prefabs\\tavern"] {
            assert_eq!(normalize_rel(p), "prefabs/tavern.prefab.json", "{p}");
        }
    }

    fn world() -> World {
        let mut w = World::new();
        w.insert_resource(GridSettings::default());
        w.insert_resource(NextObjectId::default());
        w.init_resource::<SceneHistory>();
        w.init_resource::<EditorSelection>();
        w
    }

    #[test]
    fn save_group_spawn_offset_and_undo_in_one_step() {
        let dir = std::env::temp_dir().join(format!("arc_prefab_{}", std::process::id()));
        let mut w = world();
        lb::cmd_build_room(&mut w, [0, 0], [2, 1], 0, WallStyle::WoodTavern).unwrap();
        lb::cmd_place_tile(&mut w, TileKind::Doorway, [1, 0, 0], Some(0), None).unwrap();
        let tiles: Vec<Entity> = w.query_filtered::<Entity, With<LevelTile>>().iter(&w).collect();
        let n = tiles.len();
        let live: Vec<Entity> = tiles.into_iter().filter(|e| w.get::<Deleted>(*e).is_none()).collect();
        assert!(n >= live.len() && live.len() > 8);
        w.resource_mut::<EditorSelection>().selected.extend(live.iter().copied());

        let msg = save_selection(&mut w, &dir, "corner").unwrap();
        assert!(msg.contains("prefabs/corner.prefab.json"), "{msg}");
        let file = load_file(&dir, "prefabs/corner.prefab.json").unwrap();
        assert_eq!(file.entities.len(), live.len());
        assert!(file.entities.iter().any(|e| e.door.is_some()));

        let steps = w.resource::<SceneHistory>().undo_len();
        spawn_prefab(&mut w, &dir, "corner", Vec3::new(20.0, 0.0, 0.0), 0.0).unwrap();
        assert_eq!(w.resource::<SceneHistory>().undo_len(), steps + 1, "one undo step");
        let now: Vec<LevelTile> = w.query_filtered::<&LevelTile, Without<Deleted>>().iter(&w).copied().collect();
        assert_eq!(now.len(), live.len() * 2);
        // the room spans cells x 0..=2 (pivot cell 1); the copy is centred on cell 10 (20 m / 2 m), so it spans 9..=11
        assert_eq!(now.iter().filter(|t| t.grid_pos.x >= 9).count(), live.len());
        let roots = w.query::<&PrefabInstance>().iter(&w).count();
        assert_eq!(roots, 1);

        let undo = w.resource_mut::<SceneHistory>().undo().unwrap();
        assert!(undo.iter().all(|s| !s.alive));
        assert!(undo.len() > live.len());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
