# 04. Procedural terrain, erosion, splatmap, brushes

Grid `N×N`, world size `S`, cell `c=S/(N-1)`. `smoothstep(a,b,x)=t²(3-2t)`, `t=clamp((x-a)/(b-a),0,1)`.

## 1. Valley height field
Coordinates `p=(x,z)/S ∈ [-0.5,0.5]²`, `r=|p|·2`.
fBm with analytic derivatives: `fbm(p)=Σ_{i<O} g^i · n(f^i p)`, `O=6, f=2.0, g=0.5`; derivative `∇fbm` accumulated as `Σ g^i f^i ∇n`.
Domain warp: `q = p + A · (fbm(p+o1), fbm(p+o2))` with `A≈0.12`; derivative-damped octaves: `amp_i /= 1 + |∇sum|²`.
```
base   = 0.5 + 0.5*fbm(q)
ring   = smoothstep(0.55, 0.95, r)                 ; mountain ring at map edge
h      = base*H_base + ring*H_mount*(0.6+0.4*fbm(2q))
lake   : d=|p - lake_center|*S ; bowl = 1 - smoothstep(0, lake_radius, d)
         h -= bowl*lake_depth                       ; smooth bowl
shelf  : shallow = smoothstep(lake_radius*1.0, lake_radius*1.35, d); h = lerp(h_lake_floor+0.3, h, shallow)
camp   : w = 1 - smoothstep(camp_r, camp_r*1.6, |p - camp_center|*S) ; h = lerp(h, h_camp, w)   ; flat clearing
path   : centreline x_c(z)= x0 + A_p sin(k z + φ) ; dd=|x - x_c|; w = 1 - smoothstep(wp, 2wp, dd); h = lerp(h, h_path, 0.7w)
```
Heights are clamped `[0, H_max]`.

## 2. Droplet hydraulic erosion (per droplet)
Params: inertia `ι=0.05`, capacity `K_c=4`, min slope `0.01`, erode `K_e=0.3`, deposit `K_d=0.3`, evaporate `K_v=0.01`, gravity `g=4`, life 30 steps.
State `pos, dir, speed=1, water=1, sediment=0`.
```
grad = bilinear gradient of h at pos
dir  = dir*ι - grad*(1-ι) ; normalise ; pos += dir
Δh   = h_new - h_old
cap  = max(-Δh, min_slope) * speed * water * K_c
if sediment > cap or Δh > 0:
     dep = Δh>0 ? min(Δh, sediment) : (sediment-cap)*K_d ; sediment -= dep ; h[pos] += dep (bilinear weights)
else:
     ero = min((cap-sediment)*K_e, -Δh) ; h[pos] -= ero (brush radius 3) ; sediment += ero
speed = sqrt(max(speed² + Δh*g, 0)) ; water *= (1-K_v)
```
Also store `erosion_mask[cell] += ero` (for moss).

## 3. Thermal (talus) erosion
Per iteration for each cell and its 8 neighbours: `Δ = h_c - h_n`, `slope = Δ/dist`. If `slope > tan(talus_angle)` (`talus=35°`): move `m = 0.5·(Δ - dist·tan(talus))·k` (`k≈0.5`) from c to n. 20–40 iterations.

## 4. 5-layer biome splatmap
Inputs: height `h`, normal `n`, `slope = 1 - n.y`, `d_lake` (signed distance to water level), `moss_mask` (erosion deposit / concavity), `h_n = h/H_max`.
Raw weights:
```
w_sand  = (1 - smoothstep(0.0, 0.6, d_lake)) * (d_lake>-0.3)
w_shore = smoothstep(0.0, 0.3, d_lake) * (1 - smoothstep(0.6, 1.4, d_lake))   ; gravel + foam fringe
w_rock  = smoothstep(0.32, 0.42, slope)                                        ; basalt when 1-n.y > 0.32
w_snow  = smoothstep(0.78, 0.9, h_n + 0.05*noise) * (1 - w_rock*0.7)
w_moss  = clamp(moss_mask,0,1) * (1 - w_rock)
w_grass = 1 - max(others)  ; remainder
```
Normalise: `w_i /= Σw`. Output RGBA8 (+ 5th channel in second map).
PBR (colour linear, roughness `ρ`, reflectance `F`):
| layer | albedo | ρ | F |
|---|---|---|---|
| wet sand | (0.36,0.30,0.20) | 0.35 | 0.45 |
| gravel + shore_foam | (0.52,0.50,0.46) → foam (0.9,0.92,0.92) at shore_foam=1-smoothstep(0,0.15,d) | 0.55 | 0.35 |
| lush grass | (0.16,0.32,0.08) | 0.85 | 0.25 |
| dark moss | (0.07,0.14,0.06) | 0.80 | 0.30 |
| basalt rock | (0.20,0.20,0.22) | 0.75 | 0.30 |
| snow | (0.93,0.95,1.0) | 0.45 | 0.5 |
Blend: `albedo=Σ w_i a_i`, `ρ=Σ w_i ρ_i`.

## 5. Brushes
Radial profile `f(d)=smoothstep(R, 0, d)` ⇒ `1-smoothstep(0,R,d)` (zero at rim), strength `s`, time `dt`.
- Raise: `h += s·f·dt` ; Lower: `h -= s·f·dt`
- Smooth: `h += (mean_{3×3}(h) - h)·s·f` 
- Flatten: `h += (h_target - h)·s·f`, `h_target` = height at stroke start.

## 6. Poisson-disk placement (Bridson)
Min distance `r`, cell `r/√2`, `k=30` candidates in annulus `[r,2r]` around active sample; accept if no neighbour within `r` (check 5×5 grid cells). Per-type spacing: reject if `dist < max(r_a, r_b)` between differing kinds. Filters: slope `< 0.35`, `h > water+0.2`, density mask by splat weight (grass for trees, rock for stones). Erase: remove instances within brush radius.
