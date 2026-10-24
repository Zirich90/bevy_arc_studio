# Stage E8 log — PF2e decoupled from the editor into its own crate

- Timestamp: 2026-10-24
- Prompt: `docs/clean_room_audit/prompts/stage_e8_prompt.md`
- Editor extension points (new): `props::SpawnKinds` + `SpawnCtx` + `register_spawn_kind` (exact / `prefix:` keys), `mcp_commands::CustomCommands` + `StudioCommand::Custom { name, args }` + `register_custom_command`; actor/trigger inspector now `register_inspector_override` (Faction, CharacterSheet, TriggerVolume), so `UiData.rpg` and the `ui_inspector` hook are gone. `CombatLock` -> `MoveInputLock`. `StudioCommand::SpawnActorFromTemplate` -> `Custom "pf2e.spawn_actor"`.
- Removed from the editor crate: every `#[cfg(feature = "pf2e")]`, the `pf2e` feature, the `ron` dependency, and `rules/`, `actors/`, `encounter.rs`, `ui_combat.rs` (moved with `git mv` to `crates/bevy_arc_pf2e`, assets/actors moved with them; a game's own `assets/actors` still overrides).
- Workspace: root package `bevy_arc_studio` (universal editor, `main.rs` has no PF2e) + member `bevy_arc_pf2e` (`Pf2ePlugin`). Root has a dev-dependency on the plugin crate (cycle allowed) only so `examples/pf2e_studio.rs` can combine both; `run_pf2e.bat/.sh` launch it.
- Visibility changes in the editor for the plugin: `ray_aabb`, `ray_point_distance`, `ring`, `sample_polyline`, `studio_ui_system` made `pub`.
- Test moved: Play→Stop actor rollback is now `crates/bevy_arc_pf2e/tests/play_rollback.rs`.
- `cargo check --workspace --examples --message-format=short -q`: 0 errors. `cargo test --workspace --message-format=short -q`: all pass (editor 45 + 1, pf2e 55 + 2).
- Live run (`cargo run --example pf2e_studio`): `Custom pf2e.spawn_actor` x2 and `SpawnObject trigger` via BRP spawned Hero Fighter, Goblin Warrior and a trigger volume (command log OK); screenshot `docs/screenshots/e8_pf2e_plugin.png` (outliner lists all three, trigger outline and gizmo render).
- Not verified: the editor-only binary (`cargo run`) was not launched; actor inspector overrides (Faction chips, CharacterSheet summary, TriggerVolume drag) were not looked at in the UI; combat Play mode not re-run after the move (code unchanged apart from paths and `MoveInputLock`). `ObjectCategory::Actor/Trigger` stay as generic categories.
- Verdict: **PASS** (with the gaps above)
