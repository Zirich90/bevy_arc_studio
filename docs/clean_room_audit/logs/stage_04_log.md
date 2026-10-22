# Stage 04 log — Super-GUI, brushes, MCP command bus

- Timestamp: 2026-10-22
- New files: `src/studio.rs` (mode/tab state, unified undo order World/Scene/Terrain, view toggles), `src/terrain_brush.rs`, `src/mcp_commands.rs`, `src/ui_hud.rs`, `src/ui_toolbar.rs`, `src/ui_inspector.rs`; `src/ui.rs` rewritten as the single `EguiPrimaryContextPass` shell.
- Changed: `props.rs` (`UserProp`, `spawn_user_prop`, lantern), `terrain_render.rs` (`update_terrain_region`, `PendingHeights`), `gizmo_3d.rs`/`selection.rs` (gated by `StudioState`, undo hotkeys moved to unified history), `arc_core.rs` (`WorldSpec.sun_speed`), `main.rs`.
- HUD: FPS / ms / entities / props / lights, quality presets, time slider + speed + Auto Sun, six teal toggles.
- Toolbar: Editor/Play (Tab), tools, snap/align, Orbit/Fly/Focus, Undo/Redo (Ctrl+Z/Y, global order), 4 presets, Save/Load Scene JSON.
- Inspector 360 px: live search; Global Settings (5 sections; live fields = begin/preview/commit, geometry fields commit on release), Objects & Spawner (5 spawn buttons, .glb import, filtered list, transform/Drop to Ground/PointLight), Terrain Brushes (6 brushes, radius 2..45, strength, 3D ring via Gizmos, live LMB sculpt, one undo entry per stroke).
- `StudioCommandQueue` (Resource+Reflect+Serialize): ApplyPreset, SetQuality, SetTime, SetMode/Tab/Brush, SpawnObject, Sculpt, Select, Delete, Camera, Focus, Undo/Redo, Screenshot, Save/LoadScene; results in `log`.
- `cargo check --message-format=short -q`: 0 errors. `cargo test --message-format=short -q`: 9 passed.
- Screenshots (viewed): `docs/screenshots/stage_04_editor_global.png`, `stage_04_editor_objects.png`, `stage_04_terrain_brush.png`.
- Verified live via BRP: command bus (preset, select, tab, brush, undo), real LMB sculpt (2 undo steps, rolled back by command Undo).
- Known limits: Play mode hides panels/gizmos (no OS fullscreen); light edits and props not re-seated after sculpt are outside undo; selection gizmo lines draw over the translucent inspector; `.glb` folder empty so import untested; Save/Load not exercised in this run.
- Verdict: PASS
