# Stage 09 — arc_actors (PF2e actors, factions, trigger volumes; feature `pf2e`)

- Date: 2026-10-22
- Files: `src/actors/{mod,triggers,tests}.rs`, `assets/actors/{hero_fighter,goblin_warrior}.ron`, `Cargo.toml` (optional `ron` dep, `pf2e = ["dep:ron"]`; already in Bevy's tree),
  `src/lib.rs`, `src/props.rs` (kinds `actor:<template>`, `trigger`; categories Actor/Trigger), `src/movement.rs` (no nav footprint), `src/mcp_commands.rs`, `src/ui.rs`, `src/ui_inspector.rs`, `docs/clean_room_audit/prompts/stage_09_prompt.md`.
- Components (Reflect, registered + saved via `register_scene_component`): `Actor`, `CharacterSheet` (converts to `rules::Combatant`), `Faction`, `Interactable`, `TriggerVolume`. Actors are `SceneObject`s, so selection, gizmo, soft-delete undo and `.scn.json` round-trip work unchanged.
- Spawn: template → capsule actor with `Mover`, placed via `sample_height`, faction ring under it. MCP: `SpawnActorFromTemplate { template, pos, faction }` (undoable); UI buttons `+ Spawn Fighter / Goblin / Trigger Volume`.
- Triggers: editor Gizmo cylinder outline + clickable post; Play mode emits `TriggerEnterEvent` / `TriggerExitEvent` (Messages) for `Faction::Player` actors only.
- Inspector: Actor id/template, Faction chips (editable), CharacterSheet summary, TriggerVolume r/h/event_id (editable).
- `cargo check --features pf2e --message-format=short -q`: 0 errors; default-feature check also clean.
- `cargo test --features pf2e --message-format=short -q`: 45 lib + 2 other passed (8 new: templates, spawn from RON, damage/strike, enter/exit, hostile/editor ignored, deleted actor exit, scn.json round-trip, geometry).
- Screenshots (BRP): `docs/screenshots/stage_09_actors_spawn.png`, `stage_09_actors_spawn_b.png` — Fighter + 2 Goblins at the campfire, trigger outline, selection box, inspector shows Actor/Faction/CharacterSheet. Actors stand on terrain, faction rings visible, no layout bugs.
- Not verified live: click-select of an actor with the mouse, gizmo drag + Ctrl+Z of an actor, Play-mode trigger events (covered by unit tests; undo reuses existing SceneHistory path).
- Verdict: PASS
