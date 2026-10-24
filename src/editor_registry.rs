//! Editor extension registry (E3): plugins contribute panels, viewport tools, scene components, inspector overrides and
//! spawner items through `EditorRegistry`; the core UI iterates the registry instead of hard-coding any of them.

use std::any::TypeId;
use std::sync::Arc;

use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::ScheduleSystem;
use bevy::prelude::*;
use bevy::reflect::{GetTypeRegistration, PartialReflect};
use bevy_egui::egui;

use crate::mcp_commands::StudioCommandQueue;
use crate::world_io::SceneComponentAppExt;
use crate::studio::{StudioMode, StudioState};
use crate::ui::UiData;
use crate::ui_inspector::Filter;

/// Id of the built-in gizmo tool (Select / Move / Rotate / Scale); active whenever no other tool is.
pub const TOOL_SELECT: &str = "select";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PanelLocation {
    RightInspector,
    Left,
    Bottom,
}

pub type PanelFn = fn(&mut egui::Ui, &mut UiData, &Filter);
pub type InspectorFn = fn(&mut egui::Ui, &mut dyn PartialReflect) -> bool;
pub type SpawnFn = Arc<dyn Fn(&mut StudioCommandQueue, f32, f32) + Send + Sync>;

#[derive(Clone)]
pub struct PanelDef {
    pub id: &'static str,
    pub title: &'static str,
    pub location: PanelLocation,
    pub render: PanelFn,
    /// Tabs sort by `(order, registration index)`.
    pub order: i32,
}

impl PanelDef {
    pub fn new(id: &'static str, title: &'static str, location: PanelLocation, render: PanelFn) -> Self {
        Self { id, title, location, render, order: 100 }
    }
    pub fn order(mut self, order: i32) -> Self {
        self.order = order;
        self
    }
}

#[derive(Clone)]
pub struct ToolDef {
    pub id: &'static str,
    pub label: &'static str,
    pub hotkey: Option<KeyCode>,
    pub hint: &'static str,
    /// Inspector panel opened with the tool; opening that panel activates the tool as well.
    pub panel: Option<&'static str>,
    pub on_activate: fn(&mut StudioState),
}

impl ToolDef {
    pub fn new(id: &'static str, label: &'static str) -> Self {
        Self { id, label, hotkey: None, hint: "", panel: None, on_activate: |_| {} }
    }
    pub fn hotkey(mut self, key: KeyCode) -> Self {
        self.hotkey = Some(key);
        self
    }
    pub fn hint(mut self, hint: &'static str) -> Self {
        self.hint = hint;
        self
    }
    pub fn panel(mut self, panel: &'static str) -> Self {
        self.panel = Some(panel);
        self
    }
    pub fn on_activate(mut self, f: fn(&mut StudioState)) -> Self {
        self.on_activate = f;
        self
    }
}

#[derive(Clone)]
pub struct SceneComponentDef {
    pub type_id: TypeId,
    pub name: String,
}

#[derive(Clone)]
pub struct SpawnerItem {
    pub category: &'static str,
    pub label: &'static str,
    pub spawn: SpawnFn,
}

#[derive(Resource, Default)]
pub struct EditorRegistry {
    panels: Vec<PanelDef>,
    tools: Vec<ToolDef>,
    scene_components: Vec<SceneComponentDef>,
    overrides: Vec<(TypeId, InspectorFn)>,
    spawners: Vec<SpawnerItem>,
}

impl EditorRegistry {
    /// Re-registering an id replaces the earlier definition (keeps plugin reloads / tests idempotent).
    pub fn register_panel(&mut self, def: PanelDef) {
        match self.panels.iter_mut().find(|p| p.id == def.id) {
            Some(p) => *p = def,
            None => self.panels.push(def),
        }
    }

    pub fn register_tool(&mut self, def: ToolDef) {
        match self.tools.iter_mut().find(|t| t.id == def.id) {
            Some(t) => *t = def,
            None => self.tools.push(def),
        }
    }

    pub fn register_scene_component<T: Component + Reflect + GetTypeRegistration>(&mut self) {
        let type_id = TypeId::of::<T>();
        if self.scene_components.iter().all(|c| c.type_id != type_id) {
            self.scene_components.push(SceneComponentDef { type_id, name: T::get_type_registration().type_info().type_path_table().short_path().to_string() });
        }
    }

    pub fn register_inspector_override<T: Component + Reflect>(&mut self, f: InspectorFn) {
        let id = TypeId::of::<T>();
        match self.overrides.iter_mut().find(|(t, _)| *t == id) {
            Some(o) => o.1 = f,
            None => self.overrides.push((id, f)),
        }
    }

    pub fn register_spawner_item(&mut self, category: &'static str, label: &'static str, spawn: SpawnFn) {
        if let Some(s) = self.spawners.iter_mut().find(|s| s.category == category && s.label == label) {
            s.spawn = spawn;
        } else {
            self.spawners.push(SpawnerItem { category, label, spawn });
        }
    }

    /// Panels of one location, sorted by `(order, registration index)`.
    pub fn panels(&self, loc: PanelLocation) -> Vec<PanelDef> {
        let mut v: Vec<PanelDef> = self.panels.iter().filter(|p| p.location == loc).cloned().collect();
        v.sort_by_key(|p| p.order); // stable: registration order breaks ties
        v
    }

    pub fn panel(&self, id: &str) -> Option<&PanelDef> {
        self.panels.iter().find(|p| p.id == id)
    }

    pub fn tools(&self) -> &[ToolDef] {
        &self.tools
    }

    pub fn tool(&self, id: &str) -> Option<&ToolDef> {
        self.tools.iter().find(|t| t.id == id)
    }

    pub fn tool_for_panel(&self, panel: &str) -> Option<&ToolDef> {
        self.tools.iter().find(|t| t.panel == Some(panel))
    }

    pub fn scene_components(&self) -> &[SceneComponentDef] {
        &self.scene_components
    }

    pub fn inspector_override(&self, id: TypeId) -> Option<InspectorFn> {
        self.overrides.iter().find(|(t, _)| *t == id).map(|(_, f)| *f)
    }

    pub fn spawner_items(&self) -> &[SpawnerItem] {
        &self.spawners
    }

    /// Spawner categories in first-registration order.
    pub fn spawner_categories(&self) -> Vec<&'static str> {
        let mut out: Vec<&'static str> = Vec::new();
        for s in &self.spawners {
            if !out.contains(&s.category) {
                out.push(s.category);
            }
        }
        out
    }

    /// Make `id` the active tool: opens its bound panel and runs its hook. Unknown ids are ignored.
    pub fn activate_tool(&self, studio: &mut StudioState, id: &str) -> bool {
        let Some(t) = self.tool(id) else { return false };
        studio.tool = t.id.to_string();
        if let Some(p) = t.panel {
            studio.tab = p.to_string();
        }
        (t.on_activate)(studio);
        true
    }
}

/// Spawner helper: `SpawnObject { kind }` at the given scene point.
pub fn spawn_kind(kind: &'static str) -> SpawnFn {
    Arc::new(move |q, x, z| q.push(crate::mcp_commands::StudioCommand::SpawnObject { kind: kind.into(), x, z, yaw: 0.0 }))
}

/// `App`-level façade so plugins can write `app.register_panel(..)` from their `build`.
pub trait EditorAppExt {
    fn register_panel(&mut self, id: &'static str, title: &'static str, location: PanelLocation, render: PanelFn) -> &mut Self;
    fn register_panel_def(&mut self, def: PanelDef) -> &mut Self;
    /// Tool without a per-frame system.
    fn register_tool_def(&mut self, def: ToolDef) -> &mut Self;
    /// Tool whose `update` systems run only while it is the active tool.
    fn register_tool<M>(&mut self, def: ToolDef, update: impl IntoScheduleConfigs<ScheduleSystem, M>) -> &mut Self;
    fn register_inspector_override<T: Component + Reflect>(&mut self, f: InspectorFn) -> &mut Self;
    fn register_spawner_item(&mut self, category: &'static str, label: &'static str, spawn: SpawnFn) -> &mut Self;
}

fn reg_mut(app: &mut App) -> Mut<'_, EditorRegistry> {
    app.world_mut().get_resource_or_insert_with(EditorRegistry::default)
}

impl EditorAppExt for App {
    fn register_panel(&mut self, id: &'static str, title: &'static str, location: PanelLocation, render: PanelFn) -> &mut Self {
        reg_mut(self).register_panel(PanelDef::new(id, title, location, render));
        self
    }

    fn register_panel_def(&mut self, def: PanelDef) -> &mut Self {
        reg_mut(self).register_panel(def);
        self
    }

    fn register_tool_def(&mut self, def: ToolDef) -> &mut Self {
        reg_mut(self).register_tool(def);
        self
    }

    fn register_tool<M>(&mut self, def: ToolDef, update: impl IntoScheduleConfigs<ScheduleSystem, M>) -> &mut Self {
        let id = def.id;
        reg_mut(self).register_tool(def);
        self.add_systems(Update, update.run_if(move |s: Res<StudioState>| s.mode == StudioMode::Edit && s.tool == id));
        self
    }

    fn register_inspector_override<T: Component + Reflect>(&mut self, f: InspectorFn) -> &mut Self {
        reg_mut(self).register_inspector_override::<T>(f);
        self
    }

    fn register_spawner_item(&mut self, category: &'static str, label: &'static str, spawn: SpawnFn) -> &mut Self {
        reg_mut(self).register_spawner_item(category, label, spawn);
        self
    }
}

/// Core contributions: the `select` gizmo tool, panel↔tool sync and tool hotkeys.
pub struct EditorRegistryPlugin;

impl Plugin for EditorRegistryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EditorRegistry>()
            .register_tool_def(ToolDef::new(TOOL_SELECT, "Select").hotkey(KeyCode::KeyQ).hint("Select / Move / Rotate / Scale gizmo"))
            .register_panel_def(PanelDef::new("objects", "Objects & Spawner", PanelLocation::RightInspector, crate::ui_inspector::objects_tab).order(10))
            .register_inspector_override::<Name>(name_override)
            .register_scene_component::<Name>()
            .register_scene_component::<Visibility>()
            .register_scene_component::<PointLight>()
            .add_systems(Update, (sync_tool_with_panel, tool_hotkeys));
    }
}

/// Plain text field for `Name` (the reflected `Cow<str>` + hash pair is not editable generically).
fn name_override(ui: &mut egui::Ui, v: &mut dyn PartialReflect) -> bool {
    let Some(n) = v.try_downcast_mut::<Name>() else { return false };
    let mut s = n.as_str().to_string();
    if ui.text_edit_singleline(&mut s).changed() {
        *n = Name::new(s);
        return true;
    }
    false
}

/// Opening a tool-bound panel (UI tab click or `SetTab`) activates that tool; any other panel drops a panel-bound tool.
fn sync_tool_with_panel(mut studio: ResMut<StudioState>, reg: Res<EditorRegistry>, mut last: Local<String>) {
    if studio.tab == *last {
        return;
    }
    *last = studio.tab.clone();
    let want = reg.tool_for_panel(&studio.tab).map(|t| t.id);
    match want {
        Some(id) if studio.tool != id => {
            let id = id.to_string();
            reg.activate_tool(&mut studio, &id);
        }
        None if reg.tool(&studio.tool).is_some_and(|t| t.panel.is_some()) => studio.tool = TOOL_SELECT.to_string(),
        _ => {}
    }
}

fn tool_hotkeys(keys: Res<ButtonInput<KeyCode>>, mouse: Res<ButtonInput<MouseButton>>, reg: Res<EditorRegistry>, mut studio: ResMut<StudioState>, mut egui: bevy_egui::EguiContexts) {
    if egui.ctx_mut().map(|c| c.egui_wants_keyboard_input()).unwrap_or(false) || mouse.pressed(MouseButton::Right) || keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight, KeyCode::AltLeft, KeyCode::AltRight]) {
        return;
    }
    if studio.mode != StudioMode::Edit {
        return;
    }
    // Q/W/E/R are the gizmo modes (handled in gizmo_3d) and always return to the select tool.
    if keys.any_just_pressed([KeyCode::KeyQ, KeyCode::KeyW, KeyCode::KeyE, KeyCode::KeyR]) && studio.tool != TOOL_SELECT {
        reg.activate_tool(&mut studio, TOOL_SELECT);
        return;
    }
    let hit = reg.tools().iter().find(|t| t.id != TOOL_SELECT && t.hotkey.is_some_and(|k| keys.just_pressed(k))).map(|t| t.id);
    if let Some(id) = hit {
        reg.activate_tool(&mut studio, id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Component, Reflect, Default)]
    #[reflect(Component, Default)]
    struct Probe {
        v: f32,
    }

    fn noop_panel(_: &mut egui::Ui, _: &mut UiData, _: &Filter) {}

    #[test]
    fn panels_sorted_by_order_then_registration() {
        let mut r = EditorRegistry::default();
        r.register_panel(PanelDef::new("b", "B", PanelLocation::RightInspector, noop_panel));
        r.register_panel(PanelDef::new("a", "A", PanelLocation::RightInspector, noop_panel).order(0));
        r.register_panel(PanelDef::new("c", "C", PanelLocation::Bottom, noop_panel));
        let ids: Vec<_> = r.panels(PanelLocation::RightInspector).iter().map(|p| p.id).collect();
        assert_eq!(ids, ["a", "b"]);
        assert_eq!(r.panels(PanelLocation::Bottom).len(), 1);
        assert!(r.panel("c").is_some());
    }

    #[test]
    fn reregistering_replaces_not_duplicates() {
        let mut r = EditorRegistry::default();
        r.register_panel(PanelDef::new("a", "Old", PanelLocation::Left, noop_panel));
        r.register_panel(PanelDef::new("a", "New", PanelLocation::Left, noop_panel));
        assert_eq!(r.panels(PanelLocation::Left)[0].title, "New");
        r.register_tool(ToolDef::new("t", "T"));
        r.register_tool(ToolDef::new("t", "T2"));
        assert_eq!(r.tools().len(), 1);
        assert_eq!(r.tool("t").unwrap().label, "T2");
    }

    #[test]
    fn tool_activation_opens_bound_panel_and_runs_hook() {
        let mut r = EditorRegistry::default();
        r.register_tool(ToolDef::new(TOOL_SELECT, "Select"));
        r.register_tool(ToolDef::new("brush", "Brush").hotkey(KeyCode::KeyB).panel("brushes").on_activate(|s| s.status = "brush on".into()));
        let mut s = StudioState::default();
        assert!(r.activate_tool(&mut s, "brush"));
        assert_eq!((s.tool.as_str(), s.tab.as_str(), s.status.as_str()), ("brush", "brushes", "brush on"));
        assert!(!r.activate_tool(&mut s, "missing"));
        assert_eq!(r.tool_for_panel("brushes").unwrap().id, "brush");
        assert!(r.tool_for_panel("global").is_none());
    }

    #[test]
    fn scene_components_dedup_and_named() {
        let mut r = EditorRegistry::default();
        r.register_scene_component::<Probe>();
        r.register_scene_component::<Probe>();
        assert_eq!(r.scene_components().len(), 1);
        assert_eq!(r.scene_components()[0].name, "Probe");
        assert_eq!(r.scene_components()[0].type_id, TypeId::of::<Probe>());
    }

    #[test]
    fn inspector_override_lookup() {
        let mut r = EditorRegistry::default();
        assert!(r.inspector_override(TypeId::of::<Probe>()).is_none());
        r.register_inspector_override::<Probe>(|_, _| true);
        assert!(r.inspector_override(TypeId::of::<Probe>()).is_some());
    }

    #[test]
    fn spawner_items_grouped_by_category() {
        let mut r = EditorRegistry::default();
        r.register_spawner_item("Props", "Pine", spawn_kind("pine"));
        r.register_spawner_item("RPG", "Goblin", spawn_kind("actor:goblin_warrior"));
        r.register_spawner_item("Props", "Boulder", spawn_kind("boulder"));
        r.register_spawner_item("Props", "Pine", spawn_kind("pine"));
        assert_eq!(r.spawner_categories(), ["Props", "RPG"]);
        assert_eq!(r.spawner_items().len(), 3);
        let mut q = StudioCommandQueue::default();
        (r.spawner_items()[0].spawn)(&mut q, 1.0, 2.0);
        assert_eq!(q.commands.len(), 1);
    }

    #[test]
    fn app_ext_registers_tool_panel_and_runs_update_only_while_active() {
        #[derive(Resource, Default)]
        struct Ticks(u32);
        let mut app = App::new();
        app.init_resource::<Ticks>().init_resource::<StudioState>();
        app.register_panel("p", "P", PanelLocation::RightInspector, noop_panel);
        app.register_tool(ToolDef::new("t", "T"), |mut n: ResMut<Ticks>| n.0 += 1);
        app.update();
        assert_eq!(app.world().resource::<Ticks>().0, 0);
        app.world_mut().resource_mut::<StudioState>().tool = "t".into();
        app.update();
        assert_eq!(app.world().resource::<Ticks>().0, 1);
        let r = app.world().resource::<EditorRegistry>();
        assert!(r.panel("p").is_some() && r.tool("t").is_some());
    }
}
