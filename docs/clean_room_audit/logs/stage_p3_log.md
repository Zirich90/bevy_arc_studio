# Phase 3 acceptance
Timestamp: 2026-11-02, Asia/Vladivostok.
Scope: 3.1 TimedVisibility; 3.2 SignalVisibility + inspector/BRP; 3.3 pilot and acceptance.
Implementation: src/behavior.rs; registered in src/lib.rs; session reset in src/play_mode.rs.
UI: src/ui_toolbar.rs exposes signal emission; reflected inspector edits behavior with Undo.
Input fix: src/ui.rs records viewport bounds; camera, picking, gizmo, brush, movement, level and asset tools respect panels.
Pilot: assets/prefabs/behavior_pilot.prefab.json; scenes/behavior_pilot.scn.json + .heights.
Cargo check: cargo check --message-format=short -q -j 1 — PASS (session 82240).
Cargo tests: cargo test --message-format=short -q -j 1 --workspace — PASS (session 22704): 136 passed, 1 doctest ignored.
BRP verification: scripts/verify_behavior_brp.ps1; scene save/load, linked prefab, component edit/Undo, Pause, timer, signal, Stop — PASS; DX12 additionally asserts changed prefab delay survives scene roundtrip.
Vulkan: AMD Radeon RX 9060 XT / driver 26.8.1; BRP 15703; complete scenario PASS, stage_p3_vulkan_brp.txt.
DX12: same adapter / driver 32.0.31041.1004; BRP 15702; complete scenario PASS, stage_p3_dx12_brp.txt.
Viewed BRP screenshots: docs/screenshots/p3_vulkan_pilot_edit.png, p3_vulkan_signal.png, p3_vulkan_inspector.png.
Viewed BRP screenshots: docs/screenshots/p3_dx12_pilot_edit.png, p3_dx12_signal.png.
Visual verdict: both renderers show the pilot, hidden state and intact UI; no observed layout or rendering defects.
UI acceptance: Play/Emit hide both objects; Stop restores them; inspector checkbox edit and toolbar Undo verified.
BRP transport: sandbox-launched server was unreachable; elevated cargo run resolved it, without firewall changes.
Graph: cache predates behavior.rs and UI edits; UiData node L35 verified; current sources used, no freshness claim.
Diff check PASS. Runtime restored to Edit and shutdown requested through BRP. No commit/push performed.
Oracle verdict: PASS. Phase 3.1–3.3 complete; node graph UI remains a later decision.
