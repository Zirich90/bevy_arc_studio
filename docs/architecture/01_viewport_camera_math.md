# 01. Viewport camera math

Conventions: right-handed, Y up. `eye`, `pivot`, `distance`, `fov_y` (rad), `H` viewport height (px). `forward = normalize(pivot - eye)`, `right = normalize(forward × Y)`, `up = right × forward`.

## 1. Orbit
State: `yaw`, `pitch`, `pivot`, `distance`.
```
yaw   += -dx * k_rot                 ; k_rot ≈ 0.3°/px (in radians: 0.0052)
pitch  = clamp(pitch - dy * k_rot, -89°, +89°)
forward = (cos(pitch)·sin(yaw), -sin(pitch), cos(pitch)·cos(yaw))   ; yaw=0 looks toward +Z
eye     = pivot - forward * distance
```
Orientation = look-at(eye → pivot, Y). Clamp prevents gimbal flip at ±90°.

## 2. Depth-synchronised pan (1:1)
World units per pixel at the pivot plane: `u = 2 · distance · tan(fov_y/2) / H`.
```
delta_world = (-right * dx + up * dy) * u
pivot += delta_world ; eye += delta_world
```
(dx,dy in px, screen y grows downward — sign above already accounts for it.)

## 3. Zoom
Exponential: `distance *= exp(-scroll * zoom_rate)`; `zoom_rate ≈ 0.15` per wheel notch (Shift ×3 alt). Clamp `distance ∈ [0.1, 5000]`.
Cursor-directed zoom: cast ray `r(t)=o+t·d` through cursor, hit point `P` on heightfield (doc 03); if hit:
```
eye'   = eye + (P - eye) * (1 - exp(-scroll * zoom_rate))
pivot' = eye' + forward * |P - eye'|          ; keeps look direction
```
else fall back to pivot-directed zoom. Trackpad/drag zoom: `scale = 1 + delta * 0.015`, with `|delta| ≥ 1e-4`.

## 4. Free flight (RMB held)
Look: same as orbit rotation but about `eye` (pivot recomputed after).
Move: `wish = forward*(W−S) + right*(D−A) + Y*(E−Q)`, `normalize_or_zero`.
Base speed `v0 = 9 m/s` (default); wheel during flight: `v0 *= exp(scroll * 0.1)`, clamp `[0.1, 500]`.
Shift acceleration (non-linear ramp): while Shift held, speed target
```
v_target(t+dt) = v_target(t) * a^dt        ; a = accel_param + 1  (offset avoids a<1), v_target(0)=v0
```
Shift released: `v_target = v0`.
Smoothing / velocity damping:
```
vel  = lerp(vel, wish * v_target, 1 - exp(-dt / tau))     ; tau ≈ 0.08 s
eye += vel * dt
```
On exit: `pivot = eye + forward * distance`.

## 5. Focus (F)
Given AABB of selection with centre `c`, half-extents `h`: `radius = |h|`.
```
target_distance = max(radius / sin(fov_y/2), 2.0)
target_pivot    = c
```
Smoothing per frame: `s = 1 - exp(-dt / 0.12)`; `pivot = lerp(pivot, target_pivot, s)`; `distance = lerp(distance, target_distance, s)`; stop when `|Δpivot| < 1e-3` and `|Δdistance| < 1e-3`.
