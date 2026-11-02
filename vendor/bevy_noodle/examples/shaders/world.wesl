// A raymarched landscape for the shader_world example. The node graph
// writes the bodies of `height` and `tint`; everything else is fixed.
import bevy_ui_render::ui_vertex_output::UiVertexOutput;

struct World {
    // x: time in seconds, y: width / height.
    time_aspect: vec4<f32>,
    // The sliders: x Scale, y Amplify, z Terrace.
    params: vec4<f32>,
}
@group(1) @binding(0) var<uniform> world: World;

const SUN: vec3<f32> = vec3<f32>(0.62, 0.45, -0.64);
const SKY_LOW: vec3<f32> = vec3<f32>(0.74, 0.62, 0.62);
const SKY_HIGH: vec3<f32> = vec3<f32>(0.2, 0.3, 0.52);

fn hash(p: vec2<f32>) -> f32 {
    let q = fract(p * vec2<f32>(123.34, 456.21));
    let r = q + dot(q, q + 45.32);
    return fract(r.x * r.y);
}

// Smooth value noise in -1..1.
fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash(i);
    let b = hash(i + vec2<f32>(1.0, 0.0));
    let c = hash(i + vec2<f32>(0.0, 1.0));
    let d = hash(i + vec2<f32>(1.0, 1.0));
    return 2.0 * mix(mix(a, b, u.x), mix(c, d, u.x), u.y) - 1.0;
}

// Rolling hills: five octaves of noise.
fn fbm(p: vec2<f32>) -> f32 {
    var sum = 0.0;
    var amp = 0.5;
    var q = p;
    for (var i = 0; i < 5; i++) {
        sum += amp * noise(q);
        q = mat2x2<f32>(1.6, 1.2, -1.2, 1.6) * q;
        amp *= 0.5;
    }
    return sum;
}

// Sharp mountain ridges.
fn ridged(p: vec2<f32>) -> f32 {
    var sum = 0.0;
    var amp = 0.6;
    var q = p;
    for (var i = 0; i < 5; i++) {
        let n = 1.0 - abs(noise(q));
        sum += amp * n * n;
        q = mat2x2<f32>(1.6, 1.2, -1.2, 1.6) * q;
        amp *= 0.5;
    }
    return sum - 0.4;
}

// Sand, grass, rock and snow by height.
fn biome(h: f32) -> vec3<f32> {
    let sand = vec3<f32>(0.76, 0.7, 0.5);
    let grass = vec3<f32>(0.25, 0.45, 0.2);
    let rock = vec3<f32>(0.42, 0.38, 0.36);
    let snow = vec3<f32>(0.95, 0.96, 1.0);
    var c = mix(sand, grass, smoothstep(0.1, 0.4, h));
    c = mix(c, rock, smoothstep(1.4, 2.2, h));
    return mix(c, snow, smoothstep(2.8, 3.4, h));
}

// A cosine palette: bright bands cycling through hues.
fn palette(t: f32) -> vec3<f32> {
    let phase = vec3<f32>(0.0, 0.33, 0.67);
    return 0.5 + 0.5 * cos(6.2832 * (t + phase));
}

fn height(p: vec2<f32>) -> f32 {
    // HEIGHT
}

fn tint(p: vec2<f32>, h: f32) -> vec3<f32> {
    // TINT
}

// The ground: terrain, or the sea where it dips below zero.
fn ground(p: vec2<f32>) -> f32 {
    return max(height(p), 0.0);
}

fn sky(rd: vec3<f32>) -> vec3<f32> {
    let glow = pow(max(dot(rd, SUN), 0.0), 24.0);
    return mix(SKY_LOW, SKY_HIGH, clamp(rd.y * 2.5, 0.0, 1.0)) + vec3<f32>(1.0, 0.8, 0.6) * glow;
}

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let time = world.time_aspect.x;
    let uv = (in.uv * 2.0 - 1.0) * vec2<f32>(world.time_aspect.y, -1.0);
    // Fly forward, gliding over whatever is below.
    let along = vec2<f32>(0.0, time * 1.5);
    let ro = vec3<f32>(along.x, ground(along) + 2.5, along.y);
    // Look toward the right of the screen, beside the graph panel.
    let rd = normalize(vec3<f32>(uv.x - 0.75, uv.y - 0.35, 1.6));

    var t = 0.3;
    var hit = false;
    for (var i = 0; i < 320; i++) {
        let pos = ro + rd * t;
        let gap = pos.y - ground(pos.xz);
        if gap < 0.002 * t {
            hit = true;
            break;
        }
        t += max(gap * 0.3, 0.005 * t);
        if t > 70.0 {
            break;
        }
    }
    if !hit {
        return vec4<f32>(sky(rd), 1.0);
    }

    let pos = ro + rd * t;
    let h = height(pos.xz);
    var color: vec3<f32>;
    if h < 0.0 {
        // Sea: a deep blue that reflects more sky at grazing angles.
        let fresnel = pow(1.0 - max(-rd.y, 0.0), 4.0);
        color = mix(vec3<f32>(0.05, 0.18, 0.3), sky(reflect(rd, vec3<f32>(0.0, 1.0, 0.0))), fresnel);
    } else {
        let e = 0.02;
        let n = normalize(vec3<f32>(
            height(pos.xz - vec2<f32>(e, 0.0)) - height(pos.xz + vec2<f32>(e, 0.0)),
            2.0 * e,
            height(pos.xz - vec2<f32>(0.0, e)) - height(pos.xz + vec2<f32>(0.0, e)),
        ));
        let light = max(dot(n, SUN), 0.0) + 0.25;
        color = tint(pos.xz, h) * light;
    }
    let fog = 1.0 - exp(-0.0012 * t * t);
    return vec4<f32>(mix(color, sky(rd), fog), 1.0);
}
