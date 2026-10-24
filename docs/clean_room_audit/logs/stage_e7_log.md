# Stage E7 log — Pilot tactical RPG scene (PF2e, free movement in feet, ActionGate, combat UI)

- Timestamp: 2026-10-24T12:13+10:00
- Prompt: `docs/clean_room_audit/prompts/stage_e7_prompt.md`
- New: `src/rules/encounter.rs` (pure: Threat/GateStop, `stride_triggers`, `GatedMove`, `attack`, `basic_save_damage`), `src/encounter.rs` (Combat resource, initiative, 3-action turns, Stride/Step/Strike/Arcane Bolt/Fireball/Raise Shield/Interact, enemy AI, route preview gizmos, `CombatInput` BRP order queue), `src/ui_combat.rs` (Turn Order Bar, Tactical Hotbar, Combat Log, route label, Victory/Defeat banner), `assets/actors/{hero_wizard,dungeon_guard}.ron`.
- Changed: `Cargo.toml` (`default = ["pf2e"]`), `actors/mod.rs` (CharacterSheet + reach/Reactive Strike/shield/spell DC, composite 3D kits, green Party / red Enemy rings, 4 spawner items), `movement.rs` (`CombatLock`, nav fixes), `lib.rs`, `ui.rs`, tests.
- Archetypes: Hero Fighter 32hp AC19 +10 1d8+4 RS; Hero Wizard 20hp AC16 DC18 Fireball/Arcane Bolt; Goblin Warrior 16hp AC16 +7 1d6+2; Dungeon Guard 28hp AC18 reach 5 ft RS.
- ActionGate: leaving a Reactive-Strike reach stops the Stride at the exit point, enemy rolls d20+mod vs AC (4 degrees, nat 20/1 shift); crit hit interrupts, 0 HP stops; Step 5 ft never provokes; Manipulate (spell, door) in reach provokes and a crit disrupts.
- Fixed while testing: `CharacterSheet` was silently missing from `PlayModeSnapshot` (`[i32;N]` fields have no field-wise `reflect_clone`; now `#[reflect(Clone)]`) so HP would not have rolled back; nav raster never rebuilt with actors present (walking actors dirtied it every frame; water-level NaN kept resetting the debounce).
- `cargo check --message-format=short -q`: 0 errors (also `--no-default-features`).
- `cargo test --message-format=short -q`: 101 passed, 0 failed (new: degrees/nat shift, gate triggers, crit interrupt, Step bypass, Down verdict, ECS reaction/provoke, 4 archetype specs, Stop revives killed actor + restores HP/positions).
- Live run (dx12, two rooms + doorway, 2 heroes vs goblin + guard): initiative, door opened via Interact, Raise Shield (+2 AC tag), Stride with route preview in ft, Reactive Strike on Stride and on Arcane Bolt (crit -> hero down, spell disrupted), Frightened on unnerved enemies, Victory banner.
- Stop check (via BRP query): all four HP back to max (killed goblin/guard alive again), positions and rotations original, door closed, `Stop: scene restored`.
- Screenshots: `docs/screenshots/e7_tactical_combat.png` (final), `e7_gate_preview.png` (route 15 ft + "Reactive Strike: Dungeon Guard"), `e7_after_stop.png`, `e7_play_start.png`.
- Not verified / limits: Faction variants stay `Player`/`Hostile` (labelled Party/Enemy); actors are not path obstacles; a saved scene with level tiles reloads without tile components (LoadScene, seen during the run, not investigated); Asset Browser spawner entries registered but not visually checked; hotbar glyphs need a Windows symbol font (falls back to suit glyphs).
- Verdict: **PASS**
