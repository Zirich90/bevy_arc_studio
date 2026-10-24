# Stage E5 log — Asset Browser, Prefabs, tile grouping

- Timestamp: 2026-10-24T11:08+10:00
- Files: `src/prefabs.rs` (new), `src/ui_assets.rs` (new), `src/ui_outliner.rs`, `src/level_builder.rs`, `src/mcp_commands.rs`, `src/operators.rs`, `src/props.rs`, `src/ui.rs`, `src/ui_toolbar.rs`, `src/ui_inspector.rs`, `src/lib.rs`
- Scenetree: `LevelTile` siblings fold into `▸ Floor L0 ×N / Wall L0 ×N / Pillar L1 ×N` groups (storey + kind), under the existing `Level Building / Floor N` nodes.
- `ToggleDoor` is now one undo step (ComponentHistory snapshot of `Door`); no-op toggles record nothing. Added to the undoable operator list.
- Prefabs: `assets/prefabs/*.prefab.json`; `PrefabInstance { prefab_path }` (Reflect + Serialize + Deserialize, saved in scenes). Commands `SaveSelectionAsPrefab { path }` (selection captured at issue time, not undoable: file write) and `SpawnPrefab { path, position, rotation_y_deg }` (one `SceneHistory` step, grid / ground snap, tiles re-snapped and turned in 90° steps, doors keep state). Free objects (models, props, lights) become children of the instance root; a scene file stores only the root and re-instances the members on load. Tiles stay world-space grid cells (their transform is owned by the tile system), so they are not children of the root.
- Model paths: `model:<file>` = `assets/models/<file>`, `model:showcase/castle/gate.glb` = path under `assets/`.
- Asset Browser: bottom panel (toolbar `Assets` toggle, name filter) with tabs All / Models / Prefabs / Scenes / Procedural & Level Kit, auto-rescan of `assets/` every 1.5 s, `[+ Save Selected as Prefab]`, `[Load]` / `[Save Current]`, "Click to place" cursor (Esc / RMB stops).
- cargo check --message-format=short -q: 0 errors, 0 warnings.
- cargo test --message-format=short -q: 52 + 2 passed, 0 failed (new: prefab pivot/relative coords, quarter-turn tile placement, path normalisation, save→spawn(+offset)→single-step undo, ToggleDoor undo, asset directory scan).
- Live run: tavern built via BRP, 24 tile ids selected → `assets/prefabs/tavern_corner.prefab.json`; `yard_props.prefab.json` (Duck, tower, lantern); both spawned as second instances (`yard_props` root shows its 3 children in the Scenetree).
- Screenshot: `docs/screenshots/e5_asset_browser.png` — collapsed `Floor L0 ×81 / Pillar L0 ×7 / Wall L0 ×27`, Asset Browser with `tavern_corner` / `yard_props` prefab cards + `.glb` cards, spawned prefab instances in the viewport. No layout bugs seen.
- Known gaps: `SaveSelectionAsPrefab` is not undoable; moving a prefab root does not move its tiles; per-instance overrides are not stored (E5 plan item); nested prefabs rejected.
- Verdict: **PASS**
