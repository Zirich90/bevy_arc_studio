use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use bevy::prelude::{ButtonInput, KeyCode, Resource};
use bevy_egui::egui;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Layout {
    #[default]
    Classic,
    Focus,
    Smart,
}

impl Layout {
    const ALL: [Self; 3] = [Self::Classic, Self::Focus, Self::Smart];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Classic => super::i18n::tr("Classic", "Классика"),
            Self::Focus => super::i18n::tr("Focus", "Фокус"),
            Self::Smart => super::i18n::tr("Smart", "Смарт"),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Workspace {
    #[default]
    Scene,
    Terrain,
    World,
    Build,
    Modules,
}

impl Workspace {
    const ALL: [Self; 5] = [Self::Scene, Self::Terrain, Self::World, Self::Build, Self::Modules];

    pub(crate) fn for_panel(id: &str) -> Self {
        match id {
            "objects" | "materials" | "prefab_variants" | "node_graph" | "quillshade" => Self::Scene,
            "terrain_settings" | "terrain_layers" | "brushes" => Self::Terrain,
            "global" => Self::World,
            "level_builder" => Self::Build,
            _ => Self::Modules,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Panel {
    Inspector,
    SceneTree,
    Assets,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct PanelState {
    open: bool,
    pinned: bool,
    context: Option<String>,
}

#[derive(Resource)]
pub(crate) struct LayoutState {
    active: Layout,
    pending: Option<Layout>,
    palette: Palette,
    workspace: Workspace,
    pending_workspace: Option<Workspace>,
    smart_assets_touched: bool,
    focus_override: Option<&'static str>,
    last_focus_context: Option<&'static str>,
    profiles: [[PanelState; 3]; 3],
    asset_heights: [u16; 3],
    lang: super::i18n::Lang,
    saved: Preferences,
    dirty_since: Option<Instant>,
    path: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Preferences {
    version: u8,
    active: Layout,
    workspace: Workspace,
    profiles: [[PanelState; 3]; 3],
    asset_heights: [u16; 3],
    #[serde(default)]
    lang: super::i18n::Lang,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            version: 1,
            active: Layout::Classic,
            workspace: Workspace::Scene,
            profiles: std::array::from_fn(|layout| std::array::from_fn(|_| PanelState {
                open: layout != Layout::Focus as usize,
                pinned: false,
                context: None,
            })),
            asset_heights: [176; 3],
            lang: super::i18n::Lang::En,
        }
    }
}

impl Preferences {
    fn read(path: &Path) -> Self {
        std::fs::read(path).ok()
            .and_then(|bytes| serde_json::from_slice::<Self>(&bytes).ok())
            .filter(|prefs| prefs.version == 1)
            .unwrap_or_default()
    }

    fn write(&self, path: &Path) -> std::io::Result<()> {
        use std::io::Write;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
        let mut file = std::fs::File::create(&temporary)?;
        serde_json::to_writer_pretty(&mut file, self)?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(temporary, path)
    }
}

impl Default for LayoutState {
    fn default() -> Self {
        let path = std::env::var_os("APPDATA").or_else(|| std::env::var_os("XDG_CONFIG_HOME"))
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
            .map(|dir| dir.join("ArcStudio").join("interface.json"));
        let saved = path.as_deref().map_or_else(Preferences::default, Preferences::read);
        super::i18n::set(saved.lang);
        Self {
            active: saved.active,
            pending: None,
            palette: Palette::default(),
            workspace: saved.workspace,
            pending_workspace: None,
            smart_assets_touched: false,
            focus_override: None,
            last_focus_context: None,
            profiles: saved.profiles.clone(),
            asset_heights: saved.asset_heights,
            lang: saved.lang,
            saved,
            dirty_since: None,
            path,
        }
    }
}

impl LayoutState {
    pub(crate) fn asset_panel_id(&self) -> egui::Id {
        asset_panel_id(self.active)
    }

    pub(crate) fn asset_height(&self) -> f32 {
        f32::from(self.asset_heights[self.active as usize])
    }

    pub(crate) fn set_asset_height(&mut self, height: f32) {
        self.asset_heights[self.active as usize] = height.round() as u16;
    }

    pub(crate) fn lang(&self) -> super::i18n::Lang {
        self.lang
    }

    pub(crate) fn set_lang(&mut self, lang: super::i18n::Lang) {
        self.lang = lang;
        super::i18n::set(lang);
    }

    pub(crate) fn save_if_dirty(&mut self, ctx: &egui::Context) {
        let Some(path) = self.path.as_deref() else { return };
        let current = Preferences {
            version: 1,
            active: self.active,
            workspace: self.workspace,
            profiles: self.profiles.clone(),
            asset_heights: self.asset_heights,
            lang: self.lang,
        };
        if current == self.saved {
            self.dirty_since = None;
            return;
        }
        let now = Instant::now();
        let delay = Duration::from_millis(500);
        let since = *self.dirty_since.get_or_insert(now);
        if ctx.input(|input| input.pointer.any_down()) {
            self.dirty_since = Some(now);
            return;
        }
        if let Some(remaining) = (since + delay).checked_duration_since(now) {
            ctx.request_repaint_after(remaining);
            return;
        }
        match current.write(path) {
            Ok(()) => {
                self.saved = current;
                self.dirty_since = None;
            }
            Err(error) => {
                bevy::log::warn!("Could not save interface preferences: {error}");
                let retry_delay = Duration::from_secs(5);
                self.dirty_since = Some(now + (retry_delay - delay));
                ctx.request_repaint_after(retry_delay);
            }
        }
    }

    fn reset(&mut self, all: bool) {
        let defaults = Preferences::default();
        let index = self.active as usize;
        if all {
            self.profiles = defaults.profiles;
            self.asset_heights = defaults.asset_heights;
            self.lang = defaults.lang;
            super::i18n::set(defaults.lang);
            self.pending = Some(defaults.active);
            self.pending_workspace = Some(defaults.workspace);
        } else {
            self.profiles[index] = defaults.profiles[index].clone();
            self.asset_heights[index] = defaults.asset_heights[index];
            if self.active == Layout::Smart {
                self.pending_workspace = Some(defaults.workspace);
            }
        }
        self.focus_override = None;
        self.smart_assets_touched = false;
    }

    pub(crate) fn active(&self) -> Layout {
        self.active
    }

    pub(crate) fn toggle(&mut self, panel: Panel) {
        if self.active == Layout::Smart && matches!(panel, Panel::Assets) {
            self.smart_assets_touched = true;
        }
        let panel = &mut self.profiles[self.active as usize][panel as usize];
        panel.open = !panel.open;
    }

    pub(crate) fn pinned(&self, panel: Panel) -> bool {
        self.profiles[self.active as usize][panel as usize].pinned
    }

    pub(crate) fn toggle_pin(&mut self, panel: Panel) {
        let panel = &mut self.profiles[self.active as usize][panel as usize];
        panel.pinned = !panel.pinned;
    }

    pub(crate) fn classic_context(&mut self, current: &str) -> String {
        let panel = &mut self.profiles[Layout::Classic as usize][Panel::Inspector as usize];
        if !panel.pinned || panel.context.is_none() {
            panel.context = Some(current.to_owned());
        }
        panel.context.clone().unwrap_or_else(|| current.to_owned())
    }

    pub(crate) fn show_classic_panel(&mut self, id: &'static str) {
        self.profiles[Layout::Classic as usize][Panel::Inspector as usize].context = Some(id.into());
        self.open(Panel::Inspector);
    }

    pub(crate) fn inspector_context(&mut self, current: &'static str) -> String {
        if self.last_focus_context.replace(current).is_some_and(|previous| previous != current) {
            self.focus_override = None;
        }
        let current = self.focus_override.unwrap_or(current);
        let panel = &mut self.profiles[self.active as usize][Panel::Inspector as usize];
        if !panel.pinned || panel.context.is_none() {
            panel.context = Some(current.into());
        }
        panel.context.clone().unwrap_or_else(|| current.into())
    }

    pub(crate) fn show_focus_panel(&mut self, id: &'static str) {
        self.focus_override = Some(id);
        self.profiles[self.active as usize][Panel::Inspector as usize].context = Some(id.into());
        self.open(Panel::Inspector);
    }

    pub(crate) fn workspace(&self) -> Workspace {
        self.workspace
    }

    pub(crate) fn smart_context(&mut self, fallback: Option<&'static str>) -> Option<String> {
        let panel = &mut self.profiles[Layout::Smart as usize][Panel::Inspector as usize];
        let keep = panel.context.as_deref().is_some_and(|id| {
            panel.pinned || Workspace::for_panel(id) == self.workspace
        });
        if !keep {
            panel.context = fallback.map(str::to_owned);
        }
        panel.context.clone()
    }

    pub(crate) fn show_smart_panel(&mut self, id: &'static str) {
        self.pending_workspace = Some(Workspace::for_panel(id));
        self.profiles[Layout::Smart as usize][Panel::Inspector as usize].context = Some(id.into());
        self.open(Panel::Inspector);
    }

    pub(crate) fn apply_pending(&mut self) {
        if let Some(layout) = self.pending.take() {
            self.active = layout;
        }
        if let Some(workspace) = self.pending_workspace.take() {
            if self.active == Layout::Smart && workspace != self.workspace && !self.smart_assets_touched {
                let assets = &mut self.profiles[Layout::Smart as usize][Panel::Assets as usize];
                match workspace {
                    Workspace::Terrain if !assets.pinned => assets.open = false,
                    Workspace::Scene | Workspace::Build => assets.open = true,
                    _ => {}
                }
            }
            self.workspace = workspace;
        }
    }

    pub(crate) fn open(&mut self, panel: Panel) {
        if self.active == Layout::Smart && matches!(panel, Panel::Assets) {
            self.smart_assets_touched = true;
        }
        self.profiles[self.active as usize][panel as usize].open = true;
    }

    pub(crate) fn visible(&self, panel: Panel) -> bool {
        let state = &self.profiles[self.active as usize][panel as usize];
        let in_workspace = match panel {
            Panel::Inspector => true,
            Panel::SceneTree => matches!(self.workspace, Workspace::Scene | Workspace::Terrain),
            Panel::Assets => matches!(self.workspace, Workspace::Scene | Workspace::Terrain | Workspace::Build),
        };
        state.open && (self.active != Layout::Smart || in_workspace || state.pinned)
    }
}

fn asset_panel_id(layout: Layout) -> egui::Id {
    egui::Id::new(("asset_browser", layout as usize))
}

pub(crate) fn menu(ui: &mut egui::Ui, state: &mut LayoutState) {
    for layout in Layout::ALL {
        if ui.selectable_label(state.active == layout, layout.label()).clicked() {
            state.pending = Some(layout);
            ui.close();
        }
    }
    ui.separator();
    if ui.button(super::i18n::tr("Reset Current Layout", "Сбросить текущую раскладку")).clicked() {
        state.reset(false);
        ui.ctx().data_mut(|data| data.remove::<egui::PanelState>(state.asset_panel_id()));
        ui.close();
    }
    if ui.button(super::i18n::tr("Reset All Layouts", "Сбросить все раскладки")).clicked() {
        state.reset(true);
        ui.ctx().data_mut(|data| {
            for layout in Layout::ALL {
                data.remove::<egui::PanelState>(asset_panel_id(layout));
            }
        });
        ui.close();
    }
}

pub(crate) fn panels(ui: &mut egui::Ui, state: &mut LayoutState) {
    for (panel, label, ru, _) in PANELS {
        let panel = &mut state.profiles[state.active as usize][panel as usize];
        ui.horizontal(|ui| {
            ui.checkbox(&mut panel.open, super::i18n::tr(label, ru));
            ui.checkbox(&mut panel.pinned, super::i18n::tr("Pin", "Пин"));
        });
    }
}

const PANELS: [(Panel, &str, &str, super::icons::Icon); 3] = [
    (Panel::Assets, "Assets", "Ассеты", super::icons::Icon::Folder),
    (Panel::SceneTree, "Scene Tree", "Объекты", super::icons::Icon::Objects),
    (Panel::Inspector, "Inspector", "Инспектор", super::icons::Icon::Gear),
];

pub(crate) fn focus_dock(ui: &mut egui::Ui, d: &mut super::UiData) {
    let frame = || egui::Frame::new().fill(super::CHROME_BG)
        .stroke(egui::Stroke::new(1.0, super::BORDER))
        .corner_radius(7).inner_margin(egui::Margin::same(6));
    egui::Area::new(egui::Id::new("focus_tools"))
        .anchor(egui::Align2::LEFT_TOP, [10.0, d.studio.toolbar_h + 12.0])
        .show(ui.ctx(), |ui| {
            frame().show(ui, |ui| crate::ui_toolbar::tools_vertical(ui, d));
        });
    let bottom_margin = if d.studio.show_stats { 38.0 } else { 14.0 };
    egui::Area::new(egui::Id::new("focus_dock"))
        .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -bottom_margin])
        .show(ui.ctx(), |ui| {
            frame().show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (panel, label, ru, kind) in PANELS {
                        let open = d.layout.visible(panel);
                        let text = super::i18n::tr(label, ru);
                        if super::icon_chip(ui, open, kind, text, super::i18n::tr(if open { "Click to hide" } else { "Click to show" }, if open { "Клик — скрыть" } else { "Клик — показать" })).clicked() {
                            d.layout.toggle(panel);
                        }
                    }
                    super::vsep(ui);
                    if super::icon_chip(ui, false, super::icons::Icon::Search, super::i18n::tr("Commands", "Команды"), super::i18n::tr("Command Palette (Ctrl+K)", "Палитра команд (Ctrl+K)")).clicked() {
                        open_palette(&mut d.layout);
                    }
                    super::icons::paint(ui, super::icons::Icon::Hash, 14.0, ui.style().visuals.text_color());
                    ui.menu_button(super::i18n::tr("Panels", "Панели"), |ui| {
                        for p in d.reg.panels(crate::editor_registry::PanelLocation::RightInspector) {
                            if ui.button(p.title).clicked() {
                                d.layout.show_focus_panel(p.id);
                                ui.close();
                            }
                        }
                    });
                    super::vsep(ui);
                    if super::icon_chip(ui, d.studio.show_stats, super::icons::Icon::Stats, super::i18n::tr("Stats", "Статистика"), super::i18n::tr("Toggle the bottom status bar", "Показать/скрыть нижнюю строку состояния")).clicked() {
                        d.studio.show_stats = !d.studio.show_stats;
                    }
                });
            });
        });
}

pub(crate) fn workspace_label(workspace: Workspace) -> &'static str {
    match workspace {
        Workspace::Scene => super::i18n::tr("Scene", "Сцена"),
        Workspace::Terrain => super::i18n::tr("Terrain", "Ландшафт"),
        Workspace::World => super::i18n::tr("World", "Мир"),
        Workspace::Build => super::i18n::tr("Build", "Строительство"),
        Workspace::Modules => super::i18n::tr("Modules", "Модули"),
    }
}

pub(crate) fn workspaces(ui: &mut egui::Ui, state: &mut LayoutState) {
    ui.horizontal_wrapped(|ui| {
        for workspace in Workspace::ALL {
            let kind = match workspace {
                Workspace::Scene => super::icons::Icon::Objects,
                Workspace::Terrain => super::icons::Icon::Brush,
                Workspace::World => super::icons::Icon::Sun,
                Workspace::Build => super::icons::Icon::Grid,
                Workspace::Modules => super::icons::Icon::Gear,
            };
            let label = workspace_label(workspace);
            if super::icon_chip(ui, state.workspace == workspace, kind, label, label).clicked() {
                state.pending_workspace = Some(workspace);
            }
        }
    });
}

#[derive(Default)]
struct Palette {
    open: bool,
    query: String,
    selected: usize,
    focus: bool,
}

pub(crate) fn open_palette(state: &mut LayoutState) {
    state.palette = Palette { open: true, focus: true, ..Default::default() };
}

enum PaletteAction {
    Layout(Layout),
    Tool(&'static str),
    Transform(crate::gizmo_3d::GizmoMode),
    Panel(&'static str),
    Command(crate::mcp_commands::StudioCommand),
}

fn layout_shortcut(keys: &ButtonInput<KeyCode>) -> Option<Layout> {
    if !keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight])
        || !keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight])
        || keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight])
    {
        return None;
    }
    [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3].into_iter().zip(Layout::ALL)
        .find_map(|(key, layout)| keys.just_pressed(key).then_some(layout))
}

pub(crate) fn palette(ctx: &egui::Context, d: &mut super::UiData, keys: &ButtonInput<KeyCode>) {
    use crate::mcp_commands::StudioCommand as Cmd;
    use egui::{Key, Modifiers};
    if d.studio.mode.in_session() {
        d.layout.palette.open = false;
        return;
    }
    if !ctx.egui_wants_keyboard_input() {
        if ctx.input_mut(|input| input.consume_key(Modifiers::CTRL, Key::K)) {
            open_palette(&mut d.layout);
        }
        if let Some(layout) = layout_shortcut(keys) {
            d.layout.pending = Some(layout);
        }
    }
    let palette = &mut d.layout.palette;
    if !palette.open {
        return;
    }
    let mut items: Vec<_> = Layout::ALL.into_iter()
        .map(|layout| (format!("{}: {}", super::i18n::tr("Layout", "Раскладка"), layout.label()), PaletteAction::Layout(layout)))
        .collect();
    use crate::editor_registry::TOOL_SELECT;
    use crate::gizmo_3d::GizmoMode;
    let tool = super::i18n::tr("Tool", "Инструмент");
    items.extend([
        (format!("{tool}: {}", super::i18n::tr("Select", "Выбор")), PaletteAction::Transform(GizmoMode::Select)),
        (format!("{tool}: {}", super::i18n::tr("Move", "Перемещение")), PaletteAction::Transform(GizmoMode::Move)),
        (format!("{tool}: {}", super::i18n::tr("Rotate", "Вращение")), PaletteAction::Transform(GizmoMode::Rotate)),
        (format!("{tool}: {}", super::i18n::tr("Scale", "Масштаб")), PaletteAction::Transform(GizmoMode::Scale)),
    ]);
    items.extend(d.reg.tools().into_iter().filter(|tool| tool.id != TOOL_SELECT)
        .map(|t| (format!("{tool}: {}", super::i18n::label(t.label)), PaletteAction::Tool(t.id))));
    items.extend(d.reg.panels(crate::editor_registry::PanelLocation::RightInspector).into_iter()
        .map(|panel| (format!("{}: {}", super::i18n::tr("Panel", "Панель"), super::i18n::label(panel.title)), PaletteAction::Panel(panel.id))));
    items.extend([
        (super::i18n::tr("Focus Selected", "Фокус на выделенном").into(), PaletteAction::Command(Cmd::FocusSelected)),
        (super::i18n::tr("Save Scene", "Сохранить сцену").into(), PaletteAction::Command(Cmd::SaveScene { path: d.studio.scene_path.clone() })),
        (super::i18n::tr("Load Scene", "Загрузить сцену").into(), PaletteAction::Command(Cmd::LoadScene { path: d.studio.scene_path.clone() })),
    ]);

    if !d.order.stack.is_empty() {
        items.push((super::i18n::tr("Undo", "Отменить").into(), PaletteAction::Command(Cmd::Undo)));
    }
    if !d.order.redo.is_empty() {
        items.push((super::i18n::tr("Redo", "Повторить").into(), PaletteAction::Command(Cmd::Redo)));
    }

    let (up, down, accept, cancel) = ctx.input_mut(|input| (
        input.consume_key(Modifiers::NONE, Key::ArrowUp),
        input.consume_key(Modifiers::NONE, Key::ArrowDown),
        input.consume_key(Modifiers::NONE, Key::Enter),
        input.consume_key(Modifiers::NONE, Key::Escape),
    ));
    let mut chosen = None;
    let mut open = !cancel;
    egui::Window::new(super::i18n::tr("Command Palette", "Палитра команд"))
        .anchor(egui::Align2::CENTER_TOP, [0.0, 64.0])
        .collapsible(false).resizable(false).open(&mut open)
        .show(ctx, |ui| {
            let search = ui.add(egui::TextEdit::singleline(&mut palette.query).hint_text(super::i18n::tr("Search commands…", "Поиск команд…")));
            if std::mem::take(&mut palette.focus) {
                search.request_focus();
            }
            let query = palette.query.to_lowercase();
            items.retain(|(label, _)| label.to_lowercase().contains(&query));
            if search.changed() {
                palette.selected = 0;
            }
            if up {
                palette.selected = palette.selected.saturating_sub(1);
            }
            if down {
                palette.selected += 1;
            }
            palette.selected = palette.selected.min(items.len().saturating_sub(1));
            egui::ScrollArea::vertical().max_height(280.0).show(ui, |ui| {
                for (index, (label, _)) in items.iter().enumerate() {
                    let row = ui.selectable_label(index == palette.selected, label);
                    if row.clicked() || (accept && index == palette.selected) {
                        chosen = Some(index);
                    }
                    if index == palette.selected && (up || down) {
                        row.scroll_to_me(None);
                    }
                }
            });
        });
    palette.open = open && chosen.is_none();
    if let Some(index) = chosen {
        match items.swap_remove(index).1 {
            PaletteAction::Layout(layout) => d.layout.pending = Some(layout),
            PaletteAction::Tool(tool) => { d.reg.activate_tool(&mut d.studio, tool); }
            PaletteAction::Transform(mode) => {
                d.gz.mode = mode;
                d.reg.activate_tool(&mut d.studio, TOOL_SELECT);
            }
            PaletteAction::Panel(panel) => show_registered_panel(d, panel),
            PaletteAction::Command(command) => { d.queue.push(command); }
        }
    }
}

pub(crate) fn show_registered_panel(d: &mut super::UiData, panel: &'static str) {
    match d.layout.active() {
        Layout::Focus => d.layout.show_focus_panel(panel),
        Layout::Smart => d.layout.show_smart_panel(panel),
        Layout::Classic => {
            d.layout.show_classic_panel(panel);
            d.studio.tab = panel.into();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferences_roundtrip_and_invalid_file_fallback() {
        let dir = std::env::temp_dir().join(format!("arcstudio-layout-{}", std::process::id()));
        let path = dir.join("interface.json");
        assert_eq!(Preferences::read(&path), Preferences::default());
        let mut prefs = Preferences { active: Layout::Focus, workspace: Workspace::World, ..Preferences::default() };
        let inspector = &mut prefs.profiles[Layout::Focus as usize][Panel::Inspector as usize];
        inspector.pinned = true;
        inspector.context = Some("global".into());
        prefs.asset_heights[Layout::Focus as usize] = 240;
        prefs.write(&path).unwrap();
        assert_eq!(Preferences::read(&path), prefs);
        prefs.active = Layout::Smart;
        prefs.write(&path).unwrap();
        assert_eq!(Preferences::read(&path), prefs);
        std::fs::write(&path, b"{").unwrap();
        assert_eq!(Preferences::read(&path), Preferences::default());
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn language_defaults_to_english_and_persists() {
        let dir = std::env::temp_dir().join(format!("arcstudio-lang-{}", std::process::id()));
        let path = dir.join("interface.json");
        let mut value = serde_json::to_value(Preferences::default()).unwrap();
        value.as_object_mut().unwrap().remove("lang");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, value.to_string()).unwrap();
        assert_eq!(Preferences::read(&path).lang, crate::ui::i18n::Lang::En);
        let prefs = Preferences { lang: crate::ui::i18n::Lang::Ru, ..Preferences::default() };
        prefs.write(&path).unwrap();
        assert_eq!(Preferences::read(&path).lang, crate::ui::i18n::Lang::Ru);
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn focus_open_and_pinned_are_independent_and_reset_is_local() {
        let prefs = Preferences::default();
        let mut state = LayoutState::default();
        state.profiles = prefs.profiles;
        state.active = Layout::Focus;
        let inspector = Panel::Inspector;
        assert!(!state.visible(inspector));
        state.profiles[Layout::Focus as usize][inspector as usize].pinned = true;
        assert!(!state.visible(inspector));
        state.toggle(inspector);
        assert!(state.visible(inspector));
        assert_eq!(state.inspector_context("brushes"), "brushes");
        assert_eq!(state.inspector_context("objects"), "brushes");
        state.toggle(inspector);
        assert!(!state.visible(inspector));
        state.toggle(inspector);
        state.profiles[Layout::Focus as usize][inspector as usize].pinned = false;
        assert_eq!(state.inspector_context("objects"), "objects");
        state.reset(false);
        assert!(!state.visible(inspector));
        state.pending = Some(Layout::Classic);
        state.apply_pending();
        assert!(state.visible(inspector));
    }

    #[test]
    fn smart_context_tracks_workspace_and_preserves_pinned_panel() {
        let mut state = LayoutState::default();
        state.active = Layout::Smart;
        state.workspace = Workspace::Scene;
        state.show_smart_panel("materials");
        state.apply_pending();
        assert_eq!(state.smart_context(Some("objects")).as_deref(), Some("materials"));

        state.profiles[Layout::Smart as usize][Panel::Inspector as usize].pinned = true;
        state.pending_workspace = Some(Workspace::Terrain);
        state.apply_pending();
        assert_eq!(state.smart_context(Some("terrain_settings")).as_deref(), Some("materials"));
        state.profiles[Layout::Smart as usize][Panel::Inspector as usize].pinned = false;
        assert_eq!(state.smart_context(Some("terrain_settings")).as_deref(), Some("terrain_settings"));
    }


    #[test]
    fn smart_remembers_pinned_panel_while_its_module_is_disabled() {
        use crate::editor_registry::{EditorRegistry, PanelDef, PanelLocation};

        let mut registry = EditorRegistry::default();
        registry.scope = Some("test_module".into());
        registry.register_panel(PanelDef::new(
            "module_panel",
            "Module panel",
            PanelLocation::RightInspector,
            |_, _, _| {},
        ));

        let mut state = LayoutState::default();
        state.active = Layout::Smart;
        state.show_smart_panel("module_panel");
        state.apply_pending();
        state.profiles[Layout::Smart as usize][Panel::Inspector as usize].pinned = true;

        registry.inactive.insert("test_module".into());
        assert!(registry.panel("module_panel").is_none());
        assert_eq!(state.smart_context(Some("modules")).as_deref(), Some("module_panel"));

        registry.inactive.remove("test_module");
        let context = state.smart_context(Some("modules")).unwrap();
        assert_eq!(registry.panel(&context).unwrap().id, "module_panel");
    }

    #[test]
    fn smart_docks_follow_workspace_and_open_precedes_pin() {
        let mut state = LayoutState::default();
        state.profiles = Preferences::default().profiles;
        state.active = Layout::Smart;
        state.workspace = Workspace::Terrain;
        assert!(state.visible(Panel::SceneTree));
        assert!(state.visible(Panel::Assets));

        state.pending_workspace = Some(Workspace::World);
        state.apply_pending();
        assert!(!state.visible(Panel::Assets));
        state.profiles[Layout::Smart as usize][Panel::Assets as usize].pinned = true;
        assert!(state.visible(Panel::Assets));
        state.toggle(Panel::Assets);
        assert!(!state.visible(Panel::Assets));

        state.set_asset_height(250.0);
        state.reset(true);
        state.apply_pending();
        assert_eq!(state.active(), Layout::Classic);
        assert_eq!(state.workspace(), Workspace::Scene);
        assert_eq!(state.asset_heights, [176; 3]);
        assert!(state.visible(Panel::Assets));
    }


    #[test]
    fn layout_shortcuts_match_physical_digits_and_ignore_alt_gr() {
        let mut keys = ButtonInput::default();
        keys.press(KeyCode::Digit2);
        assert_eq!(layout_shortcut(&keys), None);
        keys.press(KeyCode::ControlLeft);
        keys.press(KeyCode::ShiftRight);
        assert_eq!(layout_shortcut(&keys), Some(Layout::Focus));
        keys.press(KeyCode::AltRight);
        assert_eq!(layout_shortcut(&keys), None);
        keys.release(KeyCode::AltRight);
        keys.clear_just_pressed(KeyCode::Digit2);
        assert_eq!(layout_shortcut(&keys), None);
    }

}
