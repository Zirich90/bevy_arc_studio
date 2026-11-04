I2.03 water substage — 2026-11-03, Asia/Vladivostok.
Prompt: docs/clean_room_audit/prompts/stage_i2_03_prompt.md.
Water mesh split into global and independently owned basin draw sections.
Triangles filtered by basin ownership; each section uses its own surface level.
Lowered basin navigation surface agrees with rendered water level.
WaterBodies and WaterBody persist optional typed QuillshadeBinding.
SetWaterMaterial validates schema and records resource Undo history.
Water rebuilds preserve persisted bindings; DescribeIntegration reports actual sections.
PIE snapshot includes water levels and surface material bindings.
UI exposes binding, numeric parameters and standard-water reset.
cargo check --message-format=short -q -j 1: PASS.
cargo test --message-format=short -q -j 1 -p bevy_arc_studio --lib: 99 PASS.
Updated water scene roundtrip with numeric binding values: targeted test PASS.
scripts/verify_water_materials_brp.ps1: PASS, Vulkan PID 15992 / BRP 15702.
Runtime verified actual generated instances, Undo/Redo, PIE and scene roundtrip.
Fixture corrected to stable UI-supported 65×65 grid; earlier 33×33 image rejected.
BRP screenshot docs/screenshots/i2_water_sections.png captured and visually inspected.
Both isolated basin surfaces visible; no bridging or unintended shared material observed.
Verdict: PASS for water substage only; terrain master/layer slots and full I2 remain active.
No commit/push; generated output untouched.
