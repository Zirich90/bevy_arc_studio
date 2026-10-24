# Stage E8 prompt — decouple PF2e from the editor

User request (2026-10-24): verify the Pathfinder module is a separable plugin, not sewn into the editor ("we are building a universal tool"); then do the refactor: first add extension points to the editor, then move PF2e into its own crate.

Plan:
1. Editor extension points: `SpawnKinds` registry (spawn handlers by kind/prefix), `CustomCommands` registry + `StudioCommand::Custom { name, args }`, actor/trigger inspector moved to `register_inspector_override`, `CombatLock` renamed to generic `MoveInputLock`.
2. Remove every `#[cfg(feature = "pf2e")]` and every `crate::actors/rules/encounter` reference from the editor crate.
3. New workspace member `crates/bevy_arc_pf2e` (rules, actors, encounter, ui_combat, RON assets loader) depending on `bevy_arc_studio`; editor stays universal. `examples/pf2e_studio.rs` combines both.
4. `cargo check`/`cargo test` for both crates, live run + BRP screenshot, README/SKILL/log update.
