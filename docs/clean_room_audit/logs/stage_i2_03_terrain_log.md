I2.03 terrain substage — 2026-11-03, Asia/Vladivostok.
Prompt: docs/clean_room_audit/prompts/stage_i2_03_prompt.md.
TerrainSurfaces persists master and six independent layer bindings.
Layer sections retain actual generated shaders, UVs, parameters and private handles.
Pure arc_core::splat_intervals maps six normalized weights to disjoint coverage intervals.
Studio LayerExtension enables stochastic coverage in loaded WESL assets only.
Generated Rust/WESL/baked/source RON files have no final diff.
SetTerrainMaterial records resource Undo; world_io and PIE snapshot include all slots.
cargo check --message-format=short -q -j 1: PASS including final capability flag.
Library tests: 101 PASS; updated terrain scene roundtrip targeted test PASS (102 total discovered).
Updated PIE rollback test separately PASS; shader adapter and interval partition tests PASS.
verify_terrain_materials_brp.ps1 -HotReload: PASS for six actual instances and isolated numeric overrides.
BRP Undo/Redo, master, six layers, scene roundtrip and PIE rollback PASS.
Source RON color change rebased all layer instances; speed overrides retained; source restored through backend.
Viewed BRP screenshots: i2_terrain_layers.png, i2_terrain_master_comparison.png.
Viewed BRP screenshots: i2_terrain_zero_weight.png, i2_terrain_weighted_layer.png.
Zero-weight layer leaves plateau unchanged; nonzero grass weight renders expected shader/UV pattern.
Base material pattern also appears without layer sections; stochastic edge coverage is intentional.
Final runtime logs: no ERROR/WARN shader/GPU failures; studio shut down cleanly.
Verdict: PASS surface substage; environment, CPU evaluator/multiphysics/PF2e/navigation and Graph UX remain active.
