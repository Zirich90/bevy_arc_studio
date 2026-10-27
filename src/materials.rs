//! Material editor (phase 2): `MaterialOverride` on an object tints every `StandardMaterial` below it without touching the
//! materials other instances share; the library (`assets/materials/*.mat.json`) is hot-reloaded and re-applied by preset name.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::SystemTime;

use bevy::prelude::*;
use bevy_egui::egui;
use serde::{Deserialize, Serialize};

use crate::editor_registry::{EditorAppExt, EditorRegistry, PanelLocation};
use crate::mcp_commands::StudioCommand;
use crate::operators::{Target, add_component};
use crate::props::assets_dir;
use crate::ui::{TEAL, UiData, chip};
use crate::ui_inspector::Filter;
use crate::world_io::SceneComponentAppExt;

/// Editable PBR parameters of an object; `preset` names the library file it follows ("" = standalone).
#[derive(Component, Reflect, Serialize, Deserialize, Clone, PartialEq, Debug)]
#[reflect(Component, Default, Clone)]
pub struct MaterialOverride {
    pub preset: String,
    /// sRGB + alpha.
    pub base_color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
    pub reflectance: f32,
    /// Linear RGB.
    pub emissive: [f32; 3],
}

impl Default for MaterialOverride {
    fn default() -> Self {
        Self::from_material(&StandardMaterial::default())
    }
}

impl MaterialOverride {
    pub fn from_material(m: &StandardMaterial) -> Self {
        let c = m.base_color.to_srgba();
        Self {
            preset: String::new(),
            base_color: [c.red, c.green, c.blue, c.alpha],
            metallic: m.metallic,
            roughness: m.perceptual_roughness,
            reflectance: m.reflectance,
            emissive: [m.emissive.red, m.emissive.green, m.emissive.blue],
        }
    }

    pub fn apply(&self, m: &mut StandardMaterial) {
        let [r, g, b, a] = self.base_color;
        m.base_color = Color::srgba(r, g, b, a);
        m.metallic = self.metallic.clamp(0.0, 1.0);
        m.perceptual_roughness = self.roughness.clamp(0.089, 1.0);
        m.reflectance = self.reflectance.clamp(0.0, 1.0);
        m.emissive = LinearRgba::new(self.emissive[0].max(0.0), self.emissive[1].max(0.0), self.emissive[2].max(0.0), 1.0);
        if matches!(m.alpha_mode, AlphaMode::Opaque | AlphaMode::Blend) {
            m.alpha_mode = if a < 0.999 { AlphaMode::Blend } else { AlphaMode::Opaque };
        }
    }

    fn same_values(&self, o: &Self) -> bool {
        self.base_color == o.base_color && self.metallic == o.metallic && self.roughness == o.roughness && self.reflectance == o.reflectance && self.emissive == o.emissive
    }
}

/// The material a mesh had before it was overridden, and the private copy that carries the override.
/// Deliberately not `Clone`: a cloned object builds its own copy instead of sharing the source's.
#[derive(Component)]
struct OverrideMaterials {
    original: Handle<StandardMaterial>,
    own: Handle<StandardMaterial>,
}

/// Library presets found in `assets/materials`, with file times for hot-reload.
#[derive(Resource, Default)]
pub struct MaterialLibrary {
    pub dir: PathBuf,
    pub names: Vec<String>,
    stamps: HashMap<String, SystemTime>,
    timer: f32,
}

fn preset_path(dir: &std::path::Path, name: &str) -> Option<PathBuf> {
    let ok = !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-');
    ok.then(|| dir.join(format!("{name}.mat.json")))
}

/// Writes the override as a library file.
pub fn save_preset(dir: &std::path::Path, name: &str, m: &MaterialOverride) -> Result<(), String> {
    let path = preset_path(dir, name).ok_or("material name: letters, digits, _ and - only")?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut m = m.clone();
    m.preset = name.to_string();
    std::fs::write(path, serde_json::to_string_pretty(&m).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

pub fn load_preset(dir: &std::path::Path, name: &str) -> Result<MaterialOverride, String> {
    let path = preset_path(dir, name).ok_or("bad material name")?;
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut m: MaterialOverride = serde_json::from_str(&text).map_err(|e| format!("{name}: {e}"))?;
    m.preset = name.to_string();
    Ok(m)
}

fn scan_library(dir: &std::path::Path) -> HashMap<String, SystemTime> {
    let mut out = HashMap::new();
    let Ok(rd) = std::fs::read_dir(dir) else { return out };
    for e in rd.flatten() {
        let fname = e.file_name().to_string_lossy().to_string();
        if let Some(name) = fname.strip_suffix(".mat.json") {
            out.insert(name.to_string(), e.metadata().and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH));
        }
    }
    out
}

/// Polls the library twice a second; a changed file overwrites the values of every object that follows that preset.
fn hot_reload(time: Res<Time>, mut lib: ResMut<MaterialLibrary>, mut users: Query<&mut MaterialOverride>) {
    lib.timer -= time.delta_secs();
    if lib.timer > 0.0 {
        return;
    }
    lib.timer = 0.5;
    let found = scan_library(&lib.dir);
    for (name, stamp) in &found {
        if lib.stamps.get(name) == Some(stamp) {
            continue;
        }
        if lib.stamps.contains_key(name)
            && let Ok(fresh) = load_preset(&lib.dir, name)
        {
            for mut u in users.iter_mut().filter(|u| u.preset == *name) {
                if !u.same_values(&fresh) {
                    *u = fresh.clone();
                }
            }
        }
    }
    let mut names: Vec<String> = found.keys().cloned().collect();
    names.sort();
    lib.names = names;
    lib.stamps = found;
}

/// Gives every mesh below an overridden object its own material built from the original plus the override values.
fn apply_overrides(
    mut commands: Commands,
    roots: Query<(Entity, &MaterialOverride, Ref<MaterialOverride>)>,
    kids: Query<&Children>,
    meshes: Query<(&MeshMaterial3d<StandardMaterial>, Option<&OverrideMaterials>)>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    for (root, m, tracked) in &roots {
        let targets = std::iter::once(root).chain(kids.iter_descendants(root));
        for e in targets {
            let Ok((handle, own)) = meshes.get(e) else { continue };
            match own {
                Some(o) if handle.0 == o.own => {
                    if tracked.is_changed()
                        && let Some(mut asset) = mats.get_mut(&o.own)
                    {
                        m.apply(&mut *asset);
                    }
                }
                _ => {
                    // first time, or the mesh was cloned and still points at the source's private copy
                    let mut asset = mats.get(&handle.0).cloned().unwrap_or_default();
                    m.apply(&mut asset);
                    let own = mats.add(asset);
                    commands.entity(e).insert((MeshMaterial3d(own.clone()), OverrideMaterials { original: handle.0.clone(), own }));
                }
            }
        }
    }
}

/// Puts the original materials back when the override is removed (undo of "add" included).
fn restore_removed(mut commands: Commands, mut removed: RemovedComponents<MaterialOverride>, kids: Query<&Children>, meshes: Query<&OverrideMaterials>) {
    for root in removed.read() {
        for e in std::iter::once(root).chain(kids.iter_descendants(root)) {
            if let Ok(o) = meshes.get(e) {
                commands.entity(e).insert(MeshMaterial3d(o.original.clone())).remove::<OverrideMaterials>();
            }
        }
    }
}

/// First `StandardMaterial` at or below `root`, used to seed a fresh override with what the object looks like now.
fn first_material(w: &mut World, root: Entity) -> Option<StandardMaterial> {
    let mut stack = vec![root];
    while let Some(e) = stack.pop() {
        if let Some(h) = w.get::<MeshMaterial3d<StandardMaterial>>(e)
            && let Some(m) = w.resource::<Assets<StandardMaterial>>().get(&h.0)
        {
            return Some(m.clone());
        }
        if let Some(c) = w.get::<Children>(e) {
            stack.extend(c.iter());
        }
    }
    None
}

/// Commands behind `EditMaterial` / `ApplyMaterial` / `SaveMaterial` (all undoable through the component history).
pub fn edit_material(w: &mut World, target: Entity) -> Result<String, String> {
    let m = first_material(w, target).ok_or("object has no standard material")?;
    let json = serde_json::to_string(&MaterialOverride::from_material(&m)).map_err(|e| e.to_string())?;
    add_component(w, &Target::Entity(target), "MaterialOverride", Some(&json))
}

pub fn apply_material(w: &mut World, target: Entity, name: &str) -> Result<String, String> {
    let dir = w.resource::<MaterialLibrary>().dir.clone();
    let json = serde_json::to_string(&load_preset(&dir, name)?).map_err(|e| e.to_string())?;
    add_component(w, &Target::Entity(target), "MaterialOverride", Some(&json))?;
    Ok(format!("material '{name}' applied"))
}

pub fn save_material(w: &mut World, target: Entity, name: &str) -> Result<String, String> {
    let m = w.get::<MaterialOverride>(target).cloned().ok_or("object has no MaterialOverride (EditMaterial first)")?;
    let dir = w.resource::<MaterialLibrary>().dir.clone();
    save_preset(&dir, name, &m)?;
    w.entity_mut(target).insert(MaterialOverride { preset: name.to_string(), ..m });
    Ok(format!("material '{name}' saved"))
}

/// Colour pickers and sliders for `MaterialOverride` in the component inspector.
fn inspector(ui: &mut egui::Ui, v: &mut dyn PartialReflect) -> bool {
    let Some(m) = v.try_downcast_mut::<MaterialOverride>() else { return false };
    let mut changed = false;
    if !m.preset.is_empty() {
        ui.weak(format!("follows library: {}", m.preset));
    }
    ui.horizontal(|ui| {
        ui.label("base color");
        let mut c = egui::Color32::from_rgba_unmultiplied(
            (m.base_color[0] * 255.0).round() as u8,
            (m.base_color[1] * 255.0).round() as u8,
            (m.base_color[2] * 255.0).round() as u8,
            (m.base_color[3] * 255.0).round() as u8,
        );
        if ui.color_edit_button_srgba(&mut c).changed() {
            let [r, g, b, a] = c.to_array();
            m.base_color = [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a as f32 / 255.0];
            changed = true;
        }
    });
    changed |= ui.add(egui::Slider::new(&mut m.metallic, 0.0..=1.0).text("metallic")).changed();
    changed |= ui.add(egui::Slider::new(&mut m.roughness, 0.089..=1.0).text("roughness")).changed();
    changed |= ui.add(egui::Slider::new(&mut m.reflectance, 0.0..=1.0).text("reflectance")).changed();
    ui.horizontal(|ui| {
        ui.label("emissive");
        let mut e = m.emissive;
        if ui.color_edit_button_rgb(&mut e).changed() {
            m.emissive = e;
            changed = true;
        }
        changed |= ui.add(egui::DragValue::new(&mut m.emissive[0]).speed(0.05).prefix("r ")).changed();
        changed |= ui.add(egui::DragValue::new(&mut m.emissive[1]).speed(0.05).prefix("g ")).changed();
        changed |= ui.add(egui::DragValue::new(&mut m.emissive[2]).speed(0.05).prefix("b ")).changed();
    });
    changed
}

fn materials_tab(ui: &mut egui::Ui, d: &mut UiData, _f: &Filter) {
    ui.label(egui::RichText::new("Materials").strong().color(TEAL));
    let Some(sel) = d.sel.primary else {
        ui.weak("Select an object to edit its material.");
        return;
    };
    let target = Target::Entity(sel);
    ui.weak("Edit the values in Objects & Spawner → MaterialOverride; the object's own materials stay untouched for other instances.");
    if chip(ui, false, "Edit material of selection").on_hover_text("Adds a MaterialOverride seeded from the current look").clicked() {
        d.queue.push(StudioCommand::EditMaterial { target: target.clone() });
    }
    let id = ui.make_persistent_id("material_save_name");
    let mut name: String = ui.data_mut(|m| m.get_temp(id)).unwrap_or_default();
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut name).hint_text("preset name").desired_width(140.0));
        if chip(ui, false, "Save as preset").clicked() {
            d.queue.push(StudioCommand::SaveMaterial { target: target.clone(), name: name.clone() });
        }
    });
    ui.data_mut(|m| m.insert_temp(id, name));
    ui.label(egui::RichText::new("Library").strong().color(TEAL));
    if d.matlib.names.is_empty() {
        ui.weak(format!("No presets yet ({})", d.matlib.dir.display()));
    }
    ui.horizontal_wrapped(|ui| {
        for n in d.matlib.names.clone() {
            if chip(ui, false, &n).on_hover_text("Apply to the selection").clicked() {
                d.queue.push(StudioCommand::ApplyMaterial { target: target.clone(), name: n });
            }
        }
    });
}

pub struct MaterialsPlugin;

impl Plugin for MaterialsPlugin {
    fn build(&self, app: &mut App) {
        let dir = assets_dir().join("materials");
        app.register_type::<MaterialOverride>()
            .insert_resource(MaterialLibrary { dir, ..default() })
            .register_scene_component::<MaterialOverride>()
            .register_panel_def(crate::editor_registry::PanelDef::new("materials", "Materials", PanelLocation::RightInspector, materials_tab).order(40))
            .add_systems(Update, (hot_reload, apply_overrides, restore_removed).chain());
        app.world_mut().resource_mut::<EditorRegistry>().register_inspector_override::<MaterialOverride>(inspector);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_round_trips_and_applies() {
        let mut o = MaterialOverride::from_material(&StandardMaterial::default());
        o.metallic = 0.7;
        o.roughness = 0.3;
        o.base_color = [1.0, 0.0, 0.0, 0.5];
        let mut m = StandardMaterial::default();
        o.apply(&mut m);
        assert_eq!((m.metallic, m.perceptual_roughness), (0.7, 0.3));
        assert!(matches!(m.alpha_mode, AlphaMode::Blend));
        let back: MaterialOverride = serde_json::from_str(&serde_json::to_string(&o).unwrap()).unwrap();
        assert_eq!(back, o);
    }

    #[test]
    fn preset_files_save_load_and_reject_bad_names() {
        let dir = std::env::temp_dir().join(format!("arc_mat_test_{}", std::process::id()));
        let o = MaterialOverride { roughness: 0.2, ..default() };
        save_preset(&dir, "gold-1", &o).unwrap();
        let l = load_preset(&dir, "gold-1").unwrap();
        assert_eq!((l.roughness, l.preset.as_str()), (0.2, "gold-1"));
        assert!(save_preset(&dir, "../evil", &o).is_err());
        assert_eq!(scan_library(&dir).len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn override_gives_each_object_its_own_material_and_restores() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default())).init_asset::<StandardMaterial>().add_systems(Update, (apply_overrides, restore_removed).chain());
        let shared = app.world_mut().resource_mut::<Assets<StandardMaterial>>().add(StandardMaterial::default());
        let a = app.world_mut().spawn(MeshMaterial3d(shared.clone())).id();
        let b = app.world_mut().spawn(MeshMaterial3d(shared.clone())).id();
        app.world_mut().entity_mut(a).insert(MaterialOverride { metallic: 1.0, ..default() });
        app.update();
        let ha = app.world().get::<MeshMaterial3d<StandardMaterial>>(a).unwrap().0.clone();
        assert_ne!(ha, shared, "override gets a private copy");
        assert_eq!(app.world().get::<MeshMaterial3d<StandardMaterial>>(b).unwrap().0, shared, "other instance untouched");
        assert_eq!(app.world().resource::<Assets<StandardMaterial>>().get(&ha).unwrap().metallic, 1.0);
        app.world_mut().entity_mut(a).insert(MaterialOverride { metallic: 0.25, ..default() });
        app.update();
        assert_eq!(app.world().resource::<Assets<StandardMaterial>>().get(&ha).unwrap().metallic, 0.25);
        app.world_mut().entity_mut(a).remove::<MaterialOverride>();
        app.update();
        assert_eq!(app.world().get::<MeshMaterial3d<StandardMaterial>>(a).unwrap().0, shared, "original restored");
    }
}
