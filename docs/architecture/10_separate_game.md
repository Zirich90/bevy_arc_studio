# E5/E6: separate game and scene transitions

The existing F5 Play-in-Editor remains available. The `Separate game` window launches a saved `.scn.json` in another instance of the application, using BRP port 15704. Save edits before launching. The editor world and its Undo history remain independent.

Expand the window, select a saved scene and Launch. Pause/Resume controls the child through BRP. The live entity tree refreshes every second; select an entity, enter its full reflected component path and use Read/Apply. Component values use BRP JSON. Stop terminates the owned child; closing the editor also terminates it.

`Custom game.start` args: `{"path":"scenes/behavior_pilot.scn.json","port":15704}`.
`Custom game.stop` args: `{}`.
`Custom scene.switch` args: `{"path":"scenes/other.scn.json"}`. Run this command on the game endpoint. It leaves Play, loads the scene using the existing DynamicWorld v3 loader, then resumes Play only on success. Pause timers and session state are reset on transition.

Scene assets and prefab overrides continue to use DynamicWorld v3 and the registered component registry. No dependency versions changed.

Verification script: `scripts/verify_game_process_brp.ps1`. Acceptance passed: 138 Rust tests, one ignored doctest; BRP scenario and UI Pause/Apply/Stop verified. See docs/clean_room_audit/logs/stage_e5_e6_completion_log.md.
