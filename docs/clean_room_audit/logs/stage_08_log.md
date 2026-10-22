# Stage 08 — arc_rules (PF2e, feature `pf2e`)

- Date: 2026-10-22
- Files: `Cargo.toml` (`[features] default=[], pf2e=[]`), `src/lib.rs` (`#[cfg(feature = "pf2e")] pub mod rules;`),
  `src/rules/{mod,check,stats,conditions,actions,templates}.rs`, `docs/clean_room_audit/prompts/stage_08_prompt.md`.
- Isolation: `src/rules` uses only `rand` (StdRng::seed_from_u64); no Bevy imports. Only `src/lib.rs` references the module, behind the cfg.
- Implemented: d20 checks with 4 degrees + Nat 20/Nat 1 shift; 6 abilities, proficiency ranks; Hp with Dying 1..4 / Wounded;
  Frightened (decays at end of turn), Off-Guard, Prone; TurnActionEconomy (3 actions + reaction), MAP (0/-5/-10, agile 0/-4/-8), Stride (<= Speed, 1 action);
  templates Fighter L1 (AC 18, HP 20, longsword 1d8), Goblin Warrior (AC 16, HP 6, dagger 1d4 agile).
- `cargo check --message-format=short -q` (default features): clean, exit 0.
- `cargo test --features pf2e --message-format=short -q`: 37 passed (rules) + 2 other, 0 failed.
- Screenshot: N/A — pure logic module, no rendering change.
- Verdict: PASS
