# 02. 3D gizmo projection & drag math

Ray: `R(t) = o + t·d`, `|d|=1`. `view_dir` = camera forward. `cam_forward` = same.

## 1. Constant screen size
```
handle_size = |(eye - origin) · cam_forward| * tan(fov_y/2) * 0.16
```
All gizmo geometry (arrow length, ring radius, plane quad offset) is `handle_size * k`. Pick tolerance on axes ≈ `0.1 * handle_size`.

## 2. 1D axis drag (unit axis `a`, origin `O`)
Helper plane through `O`: `n = normalize_or_zero((a × view_dir) × a)` (plane contains `a`, faces camera as much as possible).
Parameter along axis of a ray hit:
```
t_hit = (o + s·d - O) · a ,  s = ((O - o)·n) / (d·n)
```
Dead zone: if `|a · view_dir| > 0.96` (looking along the axis, `n≈0`), use screen-space fallback: project `O` and `O+a` to pixels `p0,p1`; `t = ((mouse - p0)·(p1-p0)) / |p1-p0|² * 1` (world units per unit of that projected segment).
Grab offset at click: `grab_offset = t_hit_initial - t_origin` (with `t_origin = 0`). During drag:
```
new_pos = O_start + a * (t_hit_curr - grab_offset)
```
Optional snap: `round(value/step)*step`.

## 3. 2D plane drag (plane normal `n`, plane axes `u,v`)
Intersection: `s = ((O - o)·n) / (d·n)`, valid if `s > 0`.
Reject grazing: if `|n · view_dir| < 0.08` ⇒ no update (keep last position).
```
grab = P_hit_initial - O_start
new_pos = P_hit_curr - grab          ; planes XY (n=Z), XZ (n=Y), YZ (n=X)
```

## 4. Rotation disc (axis `a`, centre `O`)
Ring plane: normal `a`, through `O`. Hit point `P` ⇒ `v = normalize(P - O)`.
Degenerate: if `|a · view_dir| > 0.999`, ring is face-on: use the same plane intersection (valid). If edge-on (`|a · view_dir| < 0.08`) use tangent drag: `angle += (mouse_delta · screen_tangent) * 0.5°/px`.
```
angle = atan2( a · (v_start × v_curr),  v_start · v_curr )
```
Ctrl held: `angle = round(angle / 15°) * 15°`.
Accumulated rotation: `q = Quat(a, angle) * q_start` (no wrap loss: sum signed deltas between frames, wrap each into `(-π, π]`).
Grab zone: ring radius ± `0.3 * handle_size * 0.1`.
