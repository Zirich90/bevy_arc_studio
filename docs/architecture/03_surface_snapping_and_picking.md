# 03. Surface snapping & picking

## 1. Ray–heightfield intersection
Field `h(x,z)` (bilinear). Ray `R(t)=o+t·d`. Define `f(t) = R_y(t) - h(R_x(t), R_z(t))`; surface hit when `f` crosses 0 from `+` to `−`.
Adaptive march:
```
t = t_enter (ray ∩ field AABB) ; dt = cell_size * 0.5
loop: if f(t) > 0: dt = clamp(f(t) * 0.5 + cell_size*0.25, cell_size*0.25, 8*cell_size); t_prev = t; t += dt
      else break
      stop at t > t_exit → miss
```
Refine: bracket `[t_lo=t_prev (f>0), t_hi=t (f≤0)]`, 10 bisections:
```
t_mid = (t_lo + t_hi)/2 ; f(t_mid) > 0 ? t_lo = t_mid : t_hi = t_mid
```
Stop early if `|t_hi - t_lo| < 0.0005`. Hit = `R((t_lo+t_hi)/2)`.

## 2. Normal & snapping
Central differences: `dx = (h(x+e,z)-h(x-e,z))/(2e)`, `dz` likewise ⇒ `n = normalize(-dx, 1, -dz)`, `e = cell_size`.
Placement: `y = h(x,z) + aabb_bottom_offset` (`aabb_bottom_offset = -aabb.min_y_local * scale_y`).
Alignment (optional, strength `w∈[0,1]`): `n' = normalize(lerp(Y, n, w))`; `q = from_rotation_arc(Y, n')`; final rotation `q · Rot_Y(yaw_random)`.

## 3. Marquee selection
Rect `[x0,y0,x1,y1]` in pixels (normalised so x0<x1,y0<y1), drag threshold 4 px (below ⇒ point pick).
Per object centre `W`: clip `c = VP · (W,1)`; require `c.w > 0`; `ndc = c.xyz/c.w`; pixel `p = ((ndc.x+1)/2·Wpx, (1-ndc.y)/2·Hpx)`. Selected iff `p ∈ rect`.
Result set `S_rect`, previous `S_prev`:
| modifier | result |
|---|---|
| none | `S_rect` |
| Shift | `S_prev ∪ S_rect` |
| Ctrl | `S_prev △ S_rect` (symmetric difference, toggle) |
Ctrl has priority over Shift. Point pick (click) uses nearest hit by ray distance; Shift-click on already-selected active item removes it, otherwise adds.
