// Infinite background grid for the node graph canvas.
import bevy_ui_render::ui_vertex_output::UiVertexOutput;

struct Grid {
    background: vec4<f32>,
    minor: vec4<f32>,
    major: vec4<f32>,
    // xy: pan, z: zoom, w: grid spacing in graph units.
    view: vec4<f32>,
    // xy: canvas size in logical pixels, z: major line interval.
    extent: vec4<f32>,
}
@group(1) @binding(0) var<uniform> grid: Grid;

// Coverage of a 1px line repeating every `spacing` screen pixels.
fn grid_line(x: f32, spacing: f32, pixel: f32) -> f32 {
    let distance = abs(x - spacing * round(x / spacing));
    return 1.0 - smoothstep(0.5 * pixel, 1.5 * pixel, distance);
}

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let p = in.uv * grid.extent.xy;
    let local = p - grid.view.xy;
    let pixel = max(length(fwidth(p)) * 0.7071, 1e-4);

    let spacing = grid.view.w * grid.view.z;
    let major_spacing = spacing * max(grid.extent.z, 1.0);

    // Fade minor lines out as they get too dense to read.
    let minor_fade = smoothstep(4.0, 10.0, spacing);
    let minor_cov = max(grid_line(local.x, spacing, pixel), grid_line(local.y, spacing, pixel)) * minor_fade;
    let major_cov = max(grid_line(local.x, major_spacing, pixel), grid_line(local.y, major_spacing, pixel));

    // Composite lines over the (possibly transparent) background.
    var color = grid.background;
    color = over(color, vec4<f32>(grid.minor.rgb, grid.minor.a * minor_cov));
    color = over(color, vec4<f32>(grid.major.rgb, grid.major.a * major_cov));
    return color;
}

// Non-premultiplied "source over destination".
fn over(dst: vec4<f32>, src: vec4<f32>) -> vec4<f32> {
    let a = src.a + dst.a * (1.0 - src.a);
    let rgb = (src.rgb * src.a + dst.rgb * dst.a * (1.0 - src.a)) / max(a, 1e-5);
    return vec4<f32>(rgb, a);
}
