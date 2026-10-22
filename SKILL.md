---
name: bevy-arc-studio
description: Control a running Bevy ArcStudio instance (terrain, lighting, props, camera, undo, screenshots) over BRP / bevy_brp_mcp.
---

# Bevy ArcStudio — agent skill

ArcStudio exposes a command bus as the reflected resource
`bevy_arc_studio::mcp_commands::StudioCommandQueue`. Push `StudioCommand`s into
`commands`; they run next frame and results are appended to `log`.
BRP listens on `http://127.0.0.1:15702` (`bevy_brp_extras`).

## 1. Setup (Claude Code / Cursor)

Install the MCP server once: `cargo install bevy_brp_mcp`. Then register it, e.g. `.mcp.json`:

```json
{ "mcpServers": { "bevy-brp": { "command": "bevy_brp_mcp", "args": [] } } }
```

Launch the studio with `run.bat` / `./run.sh` (`cargo run`; `brp_launch` of the bare binary can fail
because of `dynamic_linking`), then check with `brp_status`.

## 2. Send commands (JSON-RPC, `world.insert_resources`)

```json
{"jsonrpc":"2.0","id":1,"method":"world.insert_resources","params":{
  "resource":"bevy_arc_studio::mcp_commands::StudioCommandQueue",
  "value":{"commands":[
    {"ApplyPreset":"MistyDuskLake"},
    {"SpawnObject":{"kind":"pine","x":10.0,"z":-4.0,"yaw":0.5}}
  ],"log":[]}}}
```

Read results:

```json
{"jsonrpc":"2.0","id":2,"method":"world.get_resources","params":{"resource":"bevy_arc_studio::mcp_commands::StudioCommandQueue"}}
```

## 3. Command reference

| Command | Payload |
|---|---|
| `ApplyPreset` | `"MistyDuskLake"` \| `"SunnyAlpineNoon"` \| `"MidnightCampfire"` \| `"GoldenAutumnDawn"` |
| `SetQuality` / `SetTime` | `QualityPreset` / hour `f32` |
| `SetMode` / `SetTab` | `StudioMode` / `InspectorTab` |
| `SetBrush` | `{kind, radius, strength}` |
| `SpawnObject` | `{kind: pine\|boulder\|campfire\|pier\|lantern\|model:<file.glb>, x, z, yaw}` |
| `Sculpt` | `{kind, x, z, radius, strength, seconds}` |
| `Select` | `{name}` — newest object with that `Name` |
| `DeleteSelected`, `FocusSelected`, `Undo`, `Redo` | unit variants |
| `SetCameraStyle` | `CamStyle` |
| `SetCamera` | `{pivot:[x,y,z], yaw, pitch, distance}` |
| `SaveScene` / `LoadScene` | `{path}` (JSON) |
| `Screenshot` | `{path}` |

## 4. Screenshots

Use MCP tool `brp_extras_screenshot` or JSON-RPC:

```json
{"jsonrpc":"2.0","id":3,"method":"brp_extras/screenshot","params":{"path":"E:/bevy_arc_studio/docs/screenshots/shot.png"}}
```

Wait ~1 s after the last command (terrain/shadows settle), then **look at the image** before declaring success.

## 5. Rules for agents

- Use absolute paths for screenshots and scenes.
- Each stroke / edit is one undo entry; roll back mistakes with `Undo`.
- Inspect `log` for errors after every batch; never assume a command succeeded.
- Raw entity access (`world.query`, `world.mutate_components`) works on `SceneObject`, `Transform`, `Name`.
