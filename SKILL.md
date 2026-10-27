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
| `SetPlayMode` | `{mode: "Edit"\|"Playing"\|"Paused"}` — Play snapshots the scene, Edit rolls it back |
| `SetGizmo` | `{mode?, snap_ground?, align_normal?}` (`null` = unchanged) |
| `SelectIds` | `{ids:[u32], additive}` (ids from `ListObjects`) |
| `SetTransform` | `{id?, pos?, rot_deg?, scale?, drop_to_ground}` — undoable |
| `Duplicate` | `{offset:[x,z]}` |
| `Copy` / `Paste` | — (clone selection with descendants; Paste staggers per repeat, undoable) |
| `SetSpec` | `{field, value}` — `WorldSpec` field by reflect path, bool as 0/1 — undoable |
| `ListObjects`, `Describe`, `DescribeScene` | read-only; `Describe` returns the schema of every operator, `DescribeScene` `{filter?, limit?}` the hierarchy |
| `SetComponent` / `AddComponent` / `RemoveComponent` | `{target, component, value}` (reflect-serde JSON string) — undoable |
| `Reparent` | `{child, parent?}` — `null` parent = root; not undoable |
| `SpawnActor` / `QueryPath` / `MoveActor` | test character on the navmesh; `QueryPath {from, to}` returns distance in metres/feet and reachability |
| `BuildRoom` | `{min:[x,z], max:[x,z], floor, style}` — floor, corner pillars, perimeter walls; one undo step |
| `BuildWallLine` | `{from:[x,z], to:[x,z], floor, style}` |
| `PlaceLevelTile` | `{kind, pos:[x,storey,z], facing?, style?}` |
| `PaintLevelTiles` | `{kind, cells:[[x,s,z]], style, erase}` |
| `ToggleDoor` | `{pos:[x,s,z], open?}` — `null` toggles; locked doors refuse |
| `SetGrid` | `{enabled?, cell_size?, floor_level?, floor_height?, angle_snap_deg?}` |
| `SetGroundSnap` | `{enabled, site?:[x,z]}` — bind or unbind the building plane to the terrain |
| `SetWaterBody` | `{x, z, level?}` — own water level of the basin at (x, z) (fills a moat, drains a pond); `null` removes it. Saved in scenes, used by navigation (wading) |
| `SaveSelectionAsPrefab` / `SpawnPrefab` | `{path}` / `{path, position, rotation_y_deg}` |
| `SetModule` / `RescanModules` / `ConnectModule` | see Modules below |
| `Custom` | `{name, args}` — plugin commands, see below |

Props (grass, pines, rocks, reeds) follow the sculpted terrain; grass and pines that end up under water are hidden.

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

## Modules

A module is connected when its `modules/<id>/module.json` is found (on by default). `{"SetModule":{"id":"pf2e","enabled":false}}` switches it off/on, `"RescanModules"` re-reads the folder, `{"ConnectModule":{"path":"D:/mods/foo/module.json"}}` copies a module file or folder into `modules/` and rescans. Module commands, spawn kinds and inspector overrides exist only while it is on. `SetTab` only knows the three built-in tabs; set `StudioState.tab` (`"modules"`) with `world.mutate_resources` to show a module/registry tab.

## Plugin commands

`{"Custom":{"name":"<plugin>.<verb>","args":"<json string>"}}` runs a command registered by a plugin through `CustomCommands`.
With the PF2e module connected: `{"Custom":{"name":"pf2e.spawn_actor","args":"{\"template\":\"goblin_warrior\",\"pos\":[6,0,2],\"faction\":\"Hostile\"}"}}`
(templates: `hero_fighter`, `hero_wizard`, `goblin_warrior`, `dungeon_guard`; `faction` optional). Spawn a trigger volume with `SpawnObject { kind: "trigger" }`.

Tactical combat (PF2e, Play mode) is driven through the reflected resource `bevy_arc_pf2e::encounter::CombatInput`: push into `orders` one of `{"Stride":{"x":..,"z":..}}`, `{"Step":{..}}`, `{"Strike":{"target":<entity>}}`, `{"ArcaneBolt":{"target":..}}`, `{"Fireball":{"x":..,"z":..}}`, `{"Interact":{"x":..,"z":..}}`, `"RaiseShield"`, `"EndTurn"`, `{"Mode":<ActionMode>}`; `hover` stands in for the mouse cursor.
