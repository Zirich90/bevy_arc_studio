# 05. Transactional Undo/Redo & MCP command API

## 1. Undo history
```
UndoHistory { undo: Vec<Entry>, redo: Vec<Entry>, open: Option<Txn>, limit: 200 }
Entry { label, before: State, after: State }          ; State = delta (component values / height-patch rect)
```
State machine:
| state | event | next | action |
|---|---|---|---|
| Idle | BeginTransaction(label, target) | Open | `open = {label, before = snapshot(target)}` |
| Open | UpdatePreview(value) | Open | apply live to world; **no** stack push |
| Open | CommitTransaction | Idle | `after = snapshot(target)`; if `after==before` discard, else push to `undo`, clear `redo`, trim to `limit` |
| Open | Cancel | Idle | restore `before` |
| Idle | Undo | Idle | pop `undo`, apply `before`, push to `redo` |
| Idle | Redo | Idle | pop `redo`, apply `after`, push to `undo` |
Rules: Undo/Redo during Open ⇒ Cancel first. Slider drag, gizmo drag, brush stroke each = one Entry. Terrain entries store only the dirty rectangle (`before`/`after` height patches).

## 2. StudioCommandQueue (JSON via BRP)
Resource `StudioCommandQueue { pending: Vec<StudioCommand> }`, written with `world.insert_resources`/`mutate_resources` over bevy_remote (port 15702), drained once per frame. Each command is externally tagged: `{"cmd": "<name>", ...args}`.
| cmd | args | effect |
|---|---|---|
| `generate_world` | `seed:u32, size:f32, resolution:u32, params?:{lake_radius,lake_depth,…}` | rebuild terrain |
| `erode` | `droplets:u32, thermal_iters:u32` | run erosion |
| `brush` | `mode:"raise"\|"lower"\|"smooth"\|"flatten", pos:[x,z], radius, strength` | one stroke dab (own txn) |
| `scatter` | `kind:"tree"\|"rock", radius_min:f32, count_max:u32, region?:[x,z,r]` | Poisson-disk placement |
| `spawn` | `kind, pos:[x,y,z], rot?:[x,y,z,w], scale?:f32, snap?:bool` | add object |
| `select` | `ids:[u64], mode:"set"\|"add"\|"toggle"` | selection |
| `transform` | `ids, translate?, rotate?, scale?` | wrapped in txn |
| `delete` | `ids` | remove, undoable |
| `camera` | `mode:"orbit"\|"focus"\|"set", pivot?, yaw?, pitch?, distance?` | camera control |
| `tool` | `"select"\|"move"\|"rotate"\|"brush"\|"scatter"` | active tool |
| `begin_txn` / `update_txn` / `commit_txn` / `cancel_txn` | `label` / `value` | explicit transaction |
| `undo` / `redo` | – | history |
| `screenshot` | `path` | handled by brp_extras/screenshot |
| `save` / `load` | `path` | WorldSpec JSON round trip |
Result: `StudioStatus { last_cmd_id, ok:bool, error:Option<String>, selection:Vec<u64>, undo_len, redo_len }` readable via `world.get_resources`. Unknown/invalid commands are dropped and reported in `error`; never panic.
