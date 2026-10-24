//! Runtime-switchable modules (E9): a module is a game-rules plugin compiled into the app whose **module file**
//! (`modules/<id>/module.json`) is found on disk. A module whose file the editor sees is already connected; the **Modules**
//! tab (or `StudioCommand::SetModule`) only switches it on/off, without a restart. `Rescan` re-reads the folder, and
//! `ConnectModule { path }` / the *Add* field copies a `module.json` (or a module folder) from anywhere into `modules/`.
//!
//! A module registers its code with [`AppModuleExt::register_module`]; everything it adds to `EditorRegistry`,
//! `SpawnKinds` and `CustomCommands` while its `build` closure runs is tagged with the module id and hidden while the
//! module is not active. Its own systems gate on [`module_active`]. On/off state persists in `modules/state.json`.
//! Switching a module off keeps its entities and components in the scene as inert data.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy_egui::egui;
use serde::{Deserialize, Serialize};

use crate::editor_registry::{EditorAppExt, EditorRegistry, PanelDef, PanelLocation};
use crate::mcp_commands::{CustomCommands, StudioCommand};
use crate::props::SpawnKinds;
use crate::ui::{AMBER, TEAL, UiData, chip};
use crate::ui_inspector::Filter;

pub const MODULES_PANEL: &str = "modules";
pub const MANIFEST: &str = "module.json";
const STATE_FILE: &str = "state.json";

/// Folder scanned for module files: the first existing `modules/` among `BEVY_ASSET_ROOT`, the working directory and its
/// parents (so tests inside `crates/*` find the workspace's), the executable's folder and `CARGO_MANIFEST_DIR`;
/// `./modules` when none exists yet.
pub fn modules_dir() -> PathBuf {
    let var = |k: &str| std::env::var(k).ok().map(PathBuf::from);
    let cwd = std::env::current_dir().ok();
    let exe = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf));
    let roots = var("BEVY_ASSET_ROOT")
        .into_iter()
        .chain(cwd.iter().flat_map(|c| c.ancestors().take(4).map(Path::to_path_buf).collect::<Vec<_>>()))
        .chain(exe)
        .chain(var("CARGO_MANIFEST_DIR"));
    roots.map(|r| r.join("modules")).find(|p| p.is_dir()).unwrap_or_else(|| cwd.unwrap_or_default().join("modules"))
}

/// Code side of a module, registered by its plugin. `name`/`version`/`description` are fallbacks for the manifest.
#[derive(Clone)]
pub struct ModuleDef {
    pub id: &'static str,
    pub name: &'static str,
    pub version: &'static str,
    pub description: &'static str,
    /// Runs when the module becomes active (file found and on), also for modules active at startup.
    pub on_activate: fn(&mut World),
    /// Runs when the module stops being active; put the world back into a state the editor can use alone.
    pub on_deactivate: fn(&mut World),
}

impl ModuleDef {
    pub fn new(id: &'static str, name: &'static str) -> Self {
        Self { id, name, version: "", description: "", on_activate: |_| {}, on_deactivate: |_| {} }
    }
    pub fn version(mut self, v: &'static str) -> Self {
        self.version = v;
        self
    }
    pub fn description(mut self, d: &'static str) -> Self {
        self.description = d;
        self
    }
    pub fn on_activate(mut self, f: fn(&mut World)) -> Self {
        self.on_activate = f;
        self
    }
    pub fn on_deactivate(mut self, f: fn(&mut World)) -> Self {
        self.on_deactivate = f;
        self
    }
}

/// Contents of `modules/<id>/module.json`. `id` must match the id of a [`ModuleDef`] compiled into the app.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Manifest {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Clone, Debug)]
pub struct FoundModule {
    pub manifest: Manifest,
    pub dir: PathBuf,
}

#[derive(Serialize, Deserialize, Default)]
struct StateFile {
    enabled: BTreeMap<String, bool>,
}

#[derive(Resource, Default)]
pub struct ModuleRegistry {
    defs: Vec<ModuleDef>,
    found: BTreeMap<String, FoundModule>,
    enabled: BTreeMap<String, bool>,
    /// Folder holding the module files; `None` = not scanned (tests without disk).
    pub dir: Option<PathBuf>,
}

impl ModuleRegistry {
    pub fn defs(&self) -> &[ModuleDef] {
        &self.defs
    }

    pub fn def(&self, id: &str) -> Option<&ModuleDef> {
        self.defs.iter().find(|d| d.id == id)
    }

    /// Modules whose file was found, sorted by id (they may lack code in this build).
    pub fn found(&self) -> impl Iterator<Item = &FoundModule> {
        self.found.values()
    }

    pub fn is_enabled(&self, id: &str) -> bool {
        self.enabled.get(id).copied().unwrap_or(true)
    }

    /// Code compiled in, file found and switched on.
    pub fn is_active(&self, id: &str) -> bool {
        self.def(id).is_some() && self.found.contains_key(id) && self.is_enabled(id)
    }

    fn active_ids(&self) -> HashSet<String> {
        self.defs.iter().filter(|d| self.is_active(d.id)).map(|d| d.id.to_string()).collect()
    }

    fn inactive_ids(&self) -> HashSet<String> {
        self.defs.iter().filter(|d| !self.is_active(d.id)).map(|d| d.id.to_string()).collect()
    }

    pub fn state_json(&self) -> String {
        serde_json::to_string_pretty(&StateFile { enabled: self.enabled.clone() }).unwrap_or_default()
    }

    pub fn load_state_json(&mut self, json: &str) -> Result<(), String> {
        self.enabled = serde_json::from_str::<StateFile>(json).map_err(|e| e.to_string())?.enabled;
        Ok(())
    }

    /// Re-read `dir`: every subfolder with a valid `module.json` is a found module.
    fn scan(&mut self) {
        self.found.clear();
        let Some(dir) = self.dir.clone() else { return };
        let Ok(rd) = std::fs::read_dir(&dir) else { return };
        for e in rd.flatten().filter(|e| e.path().is_dir()) {
            let Ok(text) = std::fs::read_to_string(e.path().join(MANIFEST)) else { continue };
            match serde_json::from_str::<Manifest>(&text) {
                Ok(m) if valid_id(&m.id) => {
                    self.found.insert(m.id.clone(), FoundModule { manifest: m, dir: e.path() });
                }
                Ok(m) => warn!("module file in {} ignored: bad id '{}'", e.path().display(), m.id),
                Err(err) => warn!("module file in {} ignored: {err}", e.path().display()),
            }
        }
    }

    fn save_state(&self) {
        if let Some(dir) = &self.dir {
            let _ = std::fs::create_dir_all(dir);
            let _ = std::fs::write(dir.join(STATE_FILE), self.state_json());
        }
    }
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Run condition for a module's own systems: true while the module is found and on.
pub fn module_active(id: &'static str) -> impl FnMut(Option<Res<ModuleRegistry>>) -> bool + Clone {
    move |m| m.is_some_and(|m| m.is_active(id))
}

/// Push the set of inactive module ids into every registry that filters by owner.
fn sync_gates(w: &mut World) {
    let Some(inactive) = w.get_resource::<ModuleRegistry>().map(ModuleRegistry::inactive_ids) else { return };
    if let Some(mut r) = w.get_resource_mut::<EditorRegistry>() {
        r.inactive = inactive.clone();
    }
    if let Some(mut r) = w.get_resource_mut::<SpawnKinds>() {
        r.inactive = inactive.clone();
    }
    if let Some(mut r) = w.get_resource_mut::<CustomCommands>() {
        r.inactive = inactive;
    }
}

/// Re-scan the module folder, apply the registry gates and run the hooks of modules whose activity changed.
pub fn rescan(w: &mut World) -> Result<String, String> {
    let before = w.get_resource::<ModuleRegistry>().ok_or("module registry missing")?.active_ids();
    w.resource_mut::<ModuleRegistry>().scan();
    sync_gates(w);
    let (after, defs) = {
        let r = w.resource::<ModuleRegistry>();
        (r.active_ids(), r.defs.clone())
    };
    for d in defs {
        match (before.contains(d.id), after.contains(d.id)) {
            (false, true) => (d.on_activate)(w),
            (true, false) => (d.on_deactivate)(w),
            _ => {}
        }
    }
    let r = w.resource::<ModuleRegistry>();
    Ok(format!("modules: {} found, {} active", r.found.len(), after.len()))
}

/// Switch a found module on/off. Runs the activate/deactivate hook when activity changes.
pub fn set_module(w: &mut World, id: &str, enabled: bool) -> Result<String, String> {
    let (def, before) = {
        let reg = w.get_resource::<ModuleRegistry>().ok_or("module registry missing")?;
        if !reg.found.contains_key(id) {
            return Err(format!("no module file for '{id}' in the modules folder"));
        }
        (reg.def(id).ok_or_else(|| format!("module '{id}' has no code in this build"))?.clone(), reg.is_active(id))
    };
    {
        let mut reg = w.resource_mut::<ModuleRegistry>();
        reg.enabled.insert(id.to_string(), enabled);
        reg.save_state();
    }
    sync_gates(w);
    match (before, enabled) {
        (false, true) => (def.on_activate)(w),
        (true, false) => (def.on_deactivate)(w),
        _ => {}
    }
    Ok(format!("module {}: {}", def.name, if enabled { "on" } else { "off" }))
}

/// "Connect" a module file from disk: `path` is a `module.json` or a folder containing one. A file is copied to
/// `modules/<id>/module.json`, a folder is copied recursively to `modules/<id>/`; then the folder is re-scanned.
pub fn connect_module(w: &mut World, path: &str) -> Result<String, String> {
    let dir = w.get_resource::<ModuleRegistry>().ok_or("module registry missing")?.dir.clone().ok_or("modules folder unknown")?;
    let src = PathBuf::from(path.trim().trim_matches('"'));
    let (manifest_path, folder) = if src.is_dir() { (src.join(MANIFEST), Some(src.clone())) } else { (src.clone(), None) };
    let text = std::fs::read_to_string(&manifest_path).map_err(|e| format!("{}: {e}", manifest_path.display()))?;
    let m: Manifest = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", manifest_path.display()))?;
    if !valid_id(&m.id) {
        return Err(format!("bad module id '{}'", m.id));
    }
    let dest = dir.join(&m.id);
    let same = |a: &Path, b: &Path| a.canonicalize().ok().zip(b.canonicalize().ok()).is_some_and(|(a, b)| a == b);
    if !same(&manifest_path.parent().unwrap_or(Path::new(".")).join(""), &dest) {
        std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
        match folder {
            Some(f) => copy_dir(&f, &dest).map_err(|e| e.to_string())?,
            None => {
                std::fs::copy(&manifest_path, dest.join(MANIFEST)).map_err(|e| e.to_string())?;
            }
        }
    }
    let msg = rescan(w)?;
    let r = w.resource::<ModuleRegistry>();
    let code = if r.def(&m.id).is_some() { "" } else { " (no code for it in this build)" };
    Ok(format!("module file '{}' connected{code}; {msg}", m.id))
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let t = to.join(e.file_name());
        if e.path().is_dir() {
            copy_dir(&e.path(), &t)?;
        } else {
            std::fs::copy(e.path(), t)?;
        }
    }
    Ok(())
}

pub trait AppModuleExt {
    /// Register the code of `def` and run `build` with every registry contribution tagged as owned by the module.
    fn register_module(&mut self, def: ModuleDef, build: impl FnOnce(&mut App)) -> &mut Self;
}

impl AppModuleExt for App {
    fn register_module(&mut self, def: ModuleDef, build: impl FnOnce(&mut App)) -> &mut Self {
        let id = def.id;
        {
            let mut reg = self.world_mut().get_resource_or_insert_with(ModuleRegistry::default);
            match reg.defs.iter_mut().find(|d| d.id == id) {
                Some(d) => *d = def,
                None => reg.defs.push(def),
            }
        }
        set_scope(self, Some(id.to_string()));
        build(self);
        set_scope(self, None);
        self
    }
}

fn set_scope(app: &mut App, scope: Option<String>) {
    let w = app.world_mut();
    w.get_resource_or_insert_with(EditorRegistry::default).scope = scope.clone();
    w.get_resource_or_insert_with(SpawnKinds::default).scope = scope.clone();
    w.get_resource_or_insert_with(CustomCommands::default).scope = scope;
}

/// Scan `modules/`, load the saved on/off state and activate what is on before the first frame.
fn init_modules(w: &mut World) {
    {
        let mut reg = w.resource_mut::<ModuleRegistry>();
        if reg.dir.is_none() {
            reg.dir = Some(modules_dir());
        }
        if let Some(text) = reg.dir.as_ref().and_then(|d| std::fs::read_to_string(d.join(STATE_FILE)).ok()) {
            if let Err(e) = reg.load_state_json(&text) {
                warn!("modules/{STATE_FILE} ignored: {e}");
            }
        }
    }
    let _ = rescan(w);
}

pub fn modules_tab(ui: &mut egui::Ui, d: &mut UiData, _f: &Filter) {
    ui.label(egui::RichText::new("Modules").strong().color(TEAL));
    ui.label("A module whose module.json is in the modules folder is connected; switch it on or off here.");
    let dir = d.modules.dir.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
    ui.horizontal_wrapped(|ui| {
        ui.weak(format!("Folder: {dir}"));
        if chip(ui, false, "Rescan").on_hover_text("Look for module files again").clicked() {
            d.queue.push(StudioCommand::RescanModules);
        }
    });
    let id = ui.make_persistent_id("module_add_path");
    let mut path: String = ui.data_mut(|m| m.get_temp(id)).unwrap_or_default();
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut path).hint_text("path to module.json or a module folder").desired_width(190.0));
        if chip(ui, false, "Add").on_hover_text("Copy the module file into the modules folder").clicked() && !path.trim().is_empty() {
            d.queue.push(StudioCommand::ConnectModule { path: path.trim().to_string() });
            path.clear();
        }
    });
    ui.data_mut(|m| m.insert_temp(id, path));
    ui.separator();
    let found: Vec<FoundModule> = d.modules.found().cloned().collect();
    if found.is_empty() {
        ui.label("No module files found.");
    }
    for fm in found {
        let m = &fm.manifest;
        let def = d.modules.def(&m.id);
        let on = d.modules.is_enabled(&m.id);
        let name = if !m.name.is_empty() { m.name.clone() } else { def.map_or(m.id.clone(), |d| d.name.to_string()) };
        let version = if !m.version.is_empty() { m.version.clone() } else { def.map_or(String::new(), |d| d.version.to_string()) };
        let description = if !m.description.is_empty() { m.description.clone() } else { def.map_or(String::new(), |d| d.description.to_string()) };
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(name).strong());
            if !version.is_empty() {
                ui.label(format!("v{version}"));
            }
            let (txt, col) = match (def.is_some(), on) {
                (false, _) => ("no code in this build", AMBER),
                (true, true) => ("on", TEAL),
                (true, false) => ("off", AMBER),
            };
            ui.label(egui::RichText::new(txt).color(col));
        });
        if !description.is_empty() {
            ui.label(description);
        }
        if def.is_some() && chip(ui, on, if on { "On" } else { "Off" }).on_hover_text("Switch the module on/off").clicked() {
            d.queue.push(StudioCommand::SetModule { id: m.id.clone(), enabled: !on });
        }
        ui.separator();
    }
}

pub struct ModulesPlugin;

impl Plugin for ModulesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ModuleRegistry>()
            .register_panel_def(PanelDef::new(MODULES_PANEL, "Modules", PanelLocation::RightInspector, modules_tab).order(900))
            .add_systems(Startup, init_modules);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp_commands::CustomCommandAppExt;

    #[derive(Resource, Default)]
    struct Hooks(Vec<&'static str>);

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("arc_modules_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn write_module(dir: &Path, id: &str) {
        let d = dir.join(id);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join(MANIFEST), format!(r#"{{"id":"{id}","name":"Mod","version":"1"}}"#)).unwrap();
    }

    fn app(dir: &Path) -> App {
        let mut app = App::new();
        app.init_resource::<Hooks>().init_resource::<EditorRegistry>().init_resource::<SpawnKinds>().init_resource::<CustomCommands>().init_resource::<ModuleRegistry>();
        app.world_mut().resource_mut::<ModuleRegistry>().dir = Some(dir.to_path_buf());
        let def = ModuleDef::new("m", "Mod").on_activate(|w| w.resource_mut::<Hooks>().0.push("on")).on_deactivate(|w| w.resource_mut::<Hooks>().0.push("off"));
        app.register_module(def, |app| {
            app.register_custom_command("m.ping", |_, _| Ok("pong".into()));
        });
        rescan(app.world_mut()).unwrap();
        app
    }

    fn has_cmd(app: &App) -> bool {
        app.world().resource::<CustomCommands>().get("m.ping").is_some()
    }

    #[test]
    fn module_file_on_disk_connects_and_switch_toggles() {
        let dir = tmp("toggle");
        let mut app = app(&dir);
        assert!(!has_cmd(&app), "no module file: hidden");
        assert!(set_module(app.world_mut(), "m", true).is_err());

        write_module(&dir, "m");
        rescan(app.world_mut()).unwrap();
        assert!(has_cmd(&app), "file found: connected and on by default");
        set_module(app.world_mut(), "m", false).unwrap();
        assert!(!has_cmd(&app));
        set_module(app.world_mut(), "m", true).unwrap();
        set_module(app.world_mut(), "m", true).unwrap();
        std::fs::remove_dir_all(dir.join("m")).unwrap();
        rescan(app.world_mut()).unwrap();
        assert!(!has_cmd(&app), "file removed: gone again");
        assert_eq!(app.world().resource::<Hooks>().0, ["on", "off", "on", "off"]);
    }

    #[test]
    fn manifest_without_code_is_listed_but_not_switchable() {
        let dir = tmp("nocode");
        write_module(&dir, "ghost");
        let mut app = app(&dir);
        assert_eq!(app.world().resource::<ModuleRegistry>().found().count(), 1);
        assert!(set_module(app.world_mut(), "ghost", true).unwrap_err().contains("no code"));
    }

    #[test]
    fn connect_copies_a_file_or_folder_into_the_modules_folder() {
        let dir = tmp("connect");
        let src = tmp("connect_src");
        write_module(&src, "m");
        std::fs::write(src.join("m").join("extra.txt"), "x").unwrap();
        let mut app = app(&dir);
        connect_module(app.world_mut(), src.join("m").to_str().unwrap()).unwrap();
        assert!(has_cmd(&app));
        assert!(dir.join("m").join("extra.txt").is_file(), "folder copied with its assets");

        set_module(app.world_mut(), "m", false).unwrap();
        let single = tmp("connect_file");
        write_module(&single, "m");
        connect_module(app.world_mut(), single.join("m").join(MANIFEST).to_str().unwrap()).unwrap();
        assert!(!has_cmd(&app), "re-adding keeps the saved off state");
        assert!(connect_module(app.world_mut(), "no/such/file.json").is_err());
    }

    #[test]
    fn on_off_state_roundtrips_through_json() {
        let dir = tmp("state");
        let mut app = app(&dir);
        write_module(&dir, "m");
        rescan(app.world_mut()).unwrap();
        set_module(app.world_mut(), "m", false).unwrap();
        let json = app.world().resource::<ModuleRegistry>().state_json();
        let mut reg = ModuleRegistry::default();
        reg.load_state_json(&json).unwrap();
        assert!(!reg.is_enabled("m") && reg.is_enabled("other"));
        assert!(dir.join(STATE_FILE).is_file());
    }
}
