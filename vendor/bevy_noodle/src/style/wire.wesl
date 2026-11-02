// Anti-aliased cubic Bézier stroke drawn inside a UI node, with an optional
// color gradient, dashes, and flow animated by the global time.
import bevy_render::globals::Globals;
import bevy_ui_render::ui_vertex_output::UiVertexOutput;

@group(0) @binding(1) var<uniform> globals: Globals;
struct Wire {
    // Colors at the start (output) and end (input) of the wire.
    start_color: vec4<f32>,
    end_color: vec4<f32>,
    p0p1: vec4<f32>,
    p2p3: vec4<f32>,
    // x: stroke width, yz: node size in logical pixels.
    params: vec4<f32>,
    // x: dash length (0 = solid), y: gap length, z: flow speed in pixels per second.
    pattern: vec4<f32>,
}
@group(1) @binding(0) var<uniform> wire: Wire;

const SAMPLES: i32 = 32;
const PULSE_SPACING: f32 = 96.0;

fn bezier(p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>, p3: vec2<f32>, t: f32) -> vec2<f32> {
    let u = 1.0 - t;
    return p0 * (u * u * u) + p1 * (3.0 * u * u * t) + p2 * (3.0 * u * t * t) + p3 * (t * t * t);
}

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let p = in.uv * wire.params.yz;
    let p0 = wire.p0p1.xy;
    let p1 = wire.p0p1.zw;
    let p2 = wire.p2p3.xy;
    let p3 = wire.p2p3.zw;

    // Distance to the curve, and the arc length at the closest point.
    var distance = 1e9;
    var along = 0.0;
    var total = 0.0;
    var previous = p0;
    for (var i = 1; i <= SAMPLES; i++) {
        let current = bezier(p0, p1, p2, p3, f32(i) / f32(SAMPLES));
        let ba = current - previous;
        let pa = p - previous;
        let h = clamp(dot(pa, ba) / max(dot(ba, ba), 1e-6), 0.0, 1.0);
        let d = length(pa - ba * h);
        let segment = length(ba);
        if d < distance {
            distance = d;
            along = total + segment * h;
        }
        total += segment;
        previous = current;
    }

    // Width of one screen pixel in local units, so edges stay crisp when zoomed.
    let pixel = max(length(fwidth(p)) * 0.7071, 1e-4);
    let half_width = max(wire.params.x * 0.5, pixel * 0.5);
    var coverage = 1.0 - smoothstep(half_width - pixel, half_width + pixel, distance);

    var color = mix(wire.start_color, wire.end_color, clamp(along / max(total, 1e-4), 0.0, 1.0));
    let phase = along - globals.time * wire.pattern.z;
    if wire.pattern.x > 0.0 {
        let period = wire.pattern.x + wire.pattern.y;
        let offset = phase - period * floor(phase / period);
        coverage *= clamp(min(offset, wire.pattern.x - offset) / pixel + 0.5, 0.0, 1.0);
    } else if wire.pattern.z != 0.0 {
        // Solid wires carry pulses instead.
        let wave = 0.5 + 0.5 * cos(phase * 6.2831853 / PULSE_SPACING);
        color = vec4<f32>(mix(color.rgb, vec3<f32>(1.0), 0.7 * pow(wave, 16.0)), color.a);
    }
    return vec4<f32>(color.rgb, color.a * coverage);
}
