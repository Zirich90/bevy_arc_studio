# I2.01 — Ultra allocation guard and initial integration acceptance
Timestamp: 2026-11-02T23:51:00Z.
Prompt: docs/clean_room_audit/prompts/stage_i2_01_prompt.md.
Skills: caveman, graphify, ponytail full; no agents dispatched.
Graph cache lacks node_graph/quillshade integration nodes; current source and SHA-256 checked.
Observed failure: Vulkan DeviceLost, then invalid buffers and poisoned GPU cluster mutex.
Ultra requested 8192² shadows: four Depth32Float cascades consume 1 GiB per directional light.
Change: src/lighting.rs caps High/Ultra shadows at 4096; Low/Medium remain 1024/2048.
Limit: Ultra currently shares High shadow resolution; adaptive GPU memory budgeting remains open.
Regression test: quality_switches_keep_shadow_allocations_bounded PASS.
Verification script: scripts/verify_quality_brp.ps1; repeated Low/Ultra/Medium/Ultra/High/Ultra and restore.
cargo check --message-format=short -q -j 1: PASS.
cargo test --message-format=short -q -j 1: 143 PASS, 1 doctest ignored.
Runtime: repeated quality switches PASS on DX12 and Vulkan; original driver failure not reproduced after cap.
BRP integration: 11 materials, assign/Undo/Redo, graph Undo/Redo, PIE and scene roundtrip PASS on DX12.
Native Vulkan input: drag delta [40,20], Delete, Undo/Redo, disconnect and reconnect verified against StudioGraph.
Reviewed BRP PNGs: docs/screenshots/i2_ultra_dx12_scene.png, i2_ultra_safe.png, i2_world_native_verified.png.
Verdict: Ultra mitigation PASS; full I2 IN PROGRESS. Graph edge/title overlap remains visible; Hot Reload and event-mode acceptance pending.
LIVE: Vulkan editor PID 18164, BRP 15702; stdout/stderr target/i2_vulkan_*.log. No commit or push.
