# Phase 2.6 — linked prefab variants
Timestamp: 2026-11-01 UTC.
Verdict: PASS.
Files: prefab_variants.rs, prefabs.rs, world_io.rs, mcp_commands.rs, ui.rs, operators.rs, lib.rs.
Assets: boulder_pair base and blue linked variant; runtime test asset removed.
Format: v2 base reference and member overrides; legacy v1 bases supported.
Stable source IDs, recursive inheritance, cycle/path/transform validation.
Overrides: transform, name, doors, lights and registered reflected scene components.
Reset and changes use unified Undo/Redo; asset creation is not undoable.
Live refresh reconciles members; roots serialize once and hydrate children/grid tiles.
Fixed scene loader represented-type dispatch; regression test covers preserved prefab components.
Check: cargo check --message-format=short -q --tests -j 2 — PASS.
Tests: cargo test --message-format=short -q -j 2 — 130 passed, one doctest ignored.
Runtime: override/reset/Undo/Redo, variant save and actual v3 scene reload verified through BRP.
Screenshot: docs/screenshots/p2_6_prefab_variants.png — captured through BRP and viewed.
Visual: blue/red members, inherited fields and scale override visible; panel layout checked.
Renderer: native DX12 verified; Vulkan swapchain failed on this host.
Limits: JSON field editor; hierarchy/kind/grid topology authored in base; generic inspector does not author overrides.
Shared memory: Obsidian junction, guarded graph export, three Python tests passed; AST coverage remains partial.
