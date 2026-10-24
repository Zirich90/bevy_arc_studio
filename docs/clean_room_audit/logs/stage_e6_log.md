# Stage E6 log: Play-in-Editor + prefab tile sync
- 2026-10-24
- Files: src/play_mode.rs (new: PlayModeSnapshot, capture/restore, PlayEphemeral, transitions), src/studio.rs (StudioMode Edit/Playing/Paused, F5/F6/Shift+F5, Tab), src/mcp_commands.rs (SetPlayMode), src/movement.rs (door click in Playing, move_actors frozen in Paused), src/ui_toolbar.rs + ui.rs + ui_hud.rs (Play/Pause/Stop block, emerald/amber frame + banner), src/prefabs.rs (PrefabTile link, sync_prefab_tiles), src/arc_core.rs/world_io.rs (Clone for UndoHistory, ids()).
- Snapshot: WorldSpec, terrain heights, editor camera, 4 undo histories + UndoOrder, per-SceneObject Transform/Visibility/parent/Deleted + registered scene components. Stop despawns spawned SceneObject/PlayEphemeral, drops WaypointsPath.
- Limits: hard-despawned objects during play are not recreated (reported in status); PrefabTile link is runtime-only (lost after scene reload).
- cargo check --message-format=short -q: 0 errors; cargo test: all pass (new: play_modify_stop_restores_scene_and_keeps_undo, stop_without_play_is_a_noop, prefab root move/rotate carries tiles).
- Live run: room + doorway; Play (screenshot docs/screenshots/e6_play_mode.png: emerald frame, PLAY MODE ACTIVE banner, door opened, hole in wall visible); Stop -> Door.is_open=false, status "Stop: scene restored", undo 4/0 unchanged (docs/screenshots/e6_after_stop.png, e6_edit_before.png).
- Not exercised live: mouse click on door, Pause with moving actor (covered by code path only), prefab root drag in UI (unit-tested).
- Verdict: PASS
