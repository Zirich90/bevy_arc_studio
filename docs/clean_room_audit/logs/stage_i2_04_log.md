I2.04 environment — PASS (substage only)
Timestamp: 2026-11-03T15:00:00+10:00
Source: src/environment.rs; bridge, lighting, commands, world_io, play_mode, lib.
Environment resource persists preset/duration/elapsed/from-values JSON.
Public upstream interpolation + full-manifest clone; generated files unchanged.
Environment commands use resource Undo/Redo; UI immediate/2s/release actions.
Scene load validates persisted transition; old scenes reset intent.
PIE restores intent and clock together with prior histories.
Pause freezes the environment clock; load resumes the saved fraction.
Ownership captures camera HDR/exposure/tonemap/dither/grading/skyboxes/IBL/fog.
Atmosphere settings/IBL and volumetric fog are suspended while owned.
ClearColor/ambient/runtime/state and ArcStudio Sun/Moon restore on release.
Managed directional lights removed; quality shadow guard remains active.
Tests: 104 library tests PASS; final cargo check --message-format=short -q PASS.
BRP: scripts/verify_environment_brp.ps1 PASS; output target/i2_environment_brp.log.
Initial fixture included transient thumbnail camera; final fixture compares persistent atmosphere camera.
Viewed: docs/screenshots/i2_environment_day.png, i2_environment_night.png, i2_environment_restored.png.
Vulkan clean shutdown PID5128; runtime logs contain no WARN/ERROR/panic/DeviceLost.
Graphify lacks environment/integration nodes; source SHA256 D95ED72EE602C8BAC67C882552D8D9DBE1D7DD4E512E94375EBEACAC38210635.
Full I2 ACTIVE: typed evaluator, CPU multiphysics, PF2e/nav and Graph UX remain; no commit/push.
