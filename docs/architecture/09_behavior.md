# Phase 3: behavior

Scene components `TimedVisibility` and `SignalVisibility` are registered with the existing scene-component registry.
Add/edit/remove them through the reflected inspector or existing AddComponent/SetComponent/RemoveComponent commands; these editor operations use existing Undo history.
Scene serialization and prefab component capture use the same registry. Both component types have serialization tests; the pilot loads a linked prefab through the v3 scene loader.

`TimedVisibility`: delay_seconds (finite, nonnegative), visible. Fires once per Play session.
`SignalVisibility`: signal (exact, case-sensitive name), visible. Every matching emission applies visibility to the owner.
Actions use Hidden or Inherited; Inherited still respects a hidden parent.
Deleted objects are excluded. Timers freeze in Pause. Signals are accepted only in Playing.
Runtime events and elapsed times are transient; Stop restores tracked scene visibility using the existing Play snapshot.
When timer and signal target the same object in one frame, the signal runs last.

BRP command payload in StudioCommandQueue:
`Custom { name: "behavior.emit", args: "{\"signal\":\"door.open\"}" }`.
Signal names must contain non-whitespace text and at most 256 bytes; queue limit is 1024 per frame.

Pilot: `scenes/behavior_pilot.scn.json`, linked to `assets/prefabs/behavior_pilot.prefab.json`. On the empty grid, one boulder hides after five seconds; the second listens to `demo.hide`.
In Edit, select either child in the outliner, open Objects & Spawner, and expand its behavior under Components. In Play, send the default signal with Emit. Stop restores both objects.
For persistent changes to a linked prefab instance, select its root and use Prefabs → member → components (BRP: SetPrefabOverride). Direct child edits are temporary; the linked asset and root overrides define the saved instance. Standalone objects save registered behavior components directly.
Run `pwsh -NoProfile -File scripts/verify_behavior_brp.ps1 -Port 15703` against a running editor to check scene save/load, linked prefab load, component edit/Undo, Pause, timer, signal and Stop.
On this host, launching the editor inside the execution sandbox prevents localhost BRP connections; the authorized elevated `cargo run --message-format=short -q ...` launch responds normally. No firewall settings were changed.
Status: phase 3.1–3.3 accepted on Vulkan and DX12; 136 Rust tests passed, one doctest ignored. BRP screenshots were captured and visually inspected; see `docs/clean_room_audit/logs/stage_p3_log.md`.
