# Stage E3 log — EditorRegistry + WorldEnvironmentPlugin

- Timestamp: 2026-10-24
- New: `src/editor_registry.rs` (EditorRegistry, EditorAppExt, EditorRegistryPlugin), `src/world_environment.rs` (WorldEnvironmentPlugin).
- Changed: `ui_inspector.rs` (registry-driven tabs, spawner list, docks; Global/Brushes moved out), `ui_reflect.rs` (`[+ Add Component]`, `[×]`, inspector overrides), `ui_toolbar.rs` (registered tool chips), `studio.rs` (`tab`/`tool` are registry ids), `terrain_brush.rs`, `terrain_render.rs`, `arc_core.rs` (`terrain_enabled`, serde-default), `world_io.rs` (scene components also listed in registry), `actors/mod.rs` (pf2e spawners), `lib.rs`.
- API: register_panel / register_tool (+ update system gated on active tool) / register_scene_component / register_inspector_override / register_spawner_item. Core inspector has no hard-coded world tabs; `SetTab` BRP command kept (maps to panel ids). Tool↔panel sync, hotkey B for Terrain Brush.
- Empty Grid Scene: `WorldSpec.terrain_enabled=false` → flat grid at Y=0, terrain/water hidden, no scatter, Infinite Grid gizmo + axes.
- Add/Remove component go through `AddComponent`/`RemoveComponent` commands (undoable).
- `cargo check --message-format=short -q`: 0 errors (also `--features pf2e`). `cargo test --message-format=short -q`: all pass (incl. 7 registry + 2 world_environment tests).
- Screenshots: `docs/screenshots/e3_registry_inspector.png` (Duck selected, Components list, `+ Add Component`, `×` buttons, Terrain Brush (B) tool chip), `docs/screenshots/e3_empty_grid.png` (Empty Grid Scene).
- Not verified: "build without world plugin" acceptance from the plan; dropdown contents of `+ Add Component` (menu not opened in screenshot).
- Verdict: PASS
