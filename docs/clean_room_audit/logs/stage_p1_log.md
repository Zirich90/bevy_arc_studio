# Phase 1 log (1.1 undo gaps, 1.2 scene format v3)
- Date: 2026-10-27
- Files: operators.rs (CompSnap.resource + snap_resource, UNDOABLE list), water.rs (undoable cmd_set_water_body, pick via command, reflect(Clone)), world_ui: world_environment.rs (water UI via commands), world_io.rs (SceneMeta, scene_version, ChildOf save/relink, v3), mcp_commands.rs (docs), play_mode.rs (CompSnap field).
- cargo check --message-format=short -q --tests: clean. cargo test: lib 69 + 45 passed, 0 failed.
- New tests: water_body_edits_undo_and_redo, hierarchy_and_version_are_saved, scene_without_meta_is_v2_and_newer_is_rejected.
- Limits: ChildOf of level tiles is not restored on load; a parent outside the saved set leaves local transform as world.
- Runtime (BRP): SetWaterBody undo/redo ok (Undo 2/0 in panel); save stamps v3; load of scene with ChildOf restored PineTree under CampfireSite ("loaded ... (v3, 2 objects)").
- Screenshots: docs/screenshots/p1_water_raised2.png (flooded lake, Body 1 (-20,14) level 12.0), docs/screenshots/p1_hierarchy_loaded.png (post-load UI, no layout bugs; viewport is under the level-12 flood so grey).
- Verdict: PASS.
