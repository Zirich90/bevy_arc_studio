//! UI materials for the default look: anti-aliased Bézier wires and an
//! infinite grid.
//!
//! Bevy UI only draws rectangles, so both are fragment shaders running on a
//! UI node: one node per wire covering the curve's bounding box, and one node
//! filling the canvas for the grid. Both anti-alias in screen space, so they
//! stay crisp at any zoom.

use bevy::asset::embedded_asset;
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;

pub(crate) struct MaterialsPlugin;

impl Plugin for MaterialsPlugin {
    // `embedded_asset!` needs the `AssetPlugin`, as the material plugins do.
    #[expect(clippy::disallowed_methods)]
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "wire.wesl");
        embedded_asset!(app, "grid.wesl");
        app.add_plugins((
            UiMaterialPlugin::<WireMaterial>::default(),
            UiMaterialPlugin::<GridMaterial>::default(),
        ));
    }
}

/// A cubic Bézier stroke. One uniform, so each wire needs one GPU buffer.
#[derive(Asset, TypePath, AsBindGroup, Clone, Debug, PartialEq)]
pub(crate) struct WireMaterial {
    #[uniform(0)]
    pub wire: Wire,
}

/// Coordinates are in the node's local logical pixels; colors are linear RGBA.
#[derive(ShaderType, Clone, Debug, PartialEq)]
pub(crate) struct Wire {
    /// Colors at the start and end.
    pub start_color: Vec4,
    pub end_color: Vec4,
    /// Control points 0 and 1, then 2 and 3.
    pub p0p1: Vec4,
    pub p2p3: Vec4,
    /// x: stroke width, yz: node size in logical pixels.
    pub params: Vec4,
    /// x: dash length (0 = solid), y: gap length, z: flow speed (pixels/s).
    pub pattern: Vec4,
}

impl UiMaterial for WireMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://bevy_noodle/style/wire.wesl".into()
    }
}

/// An infinite, pannable, zoomable grid.
#[derive(Asset, TypePath, AsBindGroup, Clone, Debug, PartialEq)]
pub(crate) struct GridMaterial {
    #[uniform(0)]
    pub grid: Grid,
}

#[derive(ShaderType, Clone, Debug, PartialEq)]
pub(crate) struct Grid {
    pub background: Vec4,
    pub minor: Vec4,
    pub major: Vec4,
    /// xy: pan, z: zoom, w: grid spacing in graph units.
    pub view: Vec4,
    /// xy: canvas size in logical pixels, z: major line interval.
    pub extent: Vec4,
}

impl UiMaterial for GridMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://bevy_noodle/style/grid.wesl".into()
    }
}

/// A wire material for `points` (graph space), plus the node rect it needs.
pub(crate) fn wire_material(
    points: [Vec2; 4],
    [start, end]: [Color; 2],
    width: f32,
    pattern: Vec3,
) -> (Rect, WireMaterial) {
    // Conservative bounds: the curve stays inside the hull of its control points.
    let padding = Vec2::splat(width + 2.0);
    let min = points.iter().copied().fold(Vec2::MAX, Vec2::min) - padding;
    let max = points.iter().copied().fold(Vec2::MIN, Vec2::max) + padding;
    let size = (max - min).max(Vec2::ONE);
    let local = points.map(|point| point - min);
    (
        Rect::from_corners(min, min + size),
        WireMaterial {
            wire: Wire {
                start_color: start.to_linear().to_vec4(),
                end_color: end.to_linear().to_vec4(),
                p0p1: Vec4::new(local[0].x, local[0].y, local[1].x, local[1].y),
                p2p3: Vec4::new(local[2].x, local[2].y, local[3].x, local[3].y),
                params: Vec4::new(width, size.x, size.y, 0.0),
                pattern: pattern.extend(0.0),
            },
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::EdgeGeometry;
    use bevy::curve::cubic_splines::CubicSegment;

    #[test]
    fn wire_bounds_contain_the_curve() {
        let ends = (
            (Vec2::ZERO, Vec2::X),
            (Vec2::new(300.0, 120.0), Vec2::NEG_X),
        );
        let geometry = EdgeGeometry::between(ends.0, ends.1);
        let points = geometry.bezier(0.5);
        let (rect, _) = wire_material(points, [Color::WHITE; 2], 3.0, Vec3::ZERO);
        let curve = CubicSegment::new_bezier(points);
        for point in curve.iter_positions(20) {
            assert!(rect.contains(point));
        }
    }
}
