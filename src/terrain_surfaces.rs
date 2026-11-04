//! Studio-owned terrain sections; generated shader files remain unchanged.
use bevy::{mesh::MeshVertexBufferLayoutRef, pbr::{ExtendedMaterial, Material, MaterialExtension, MaterialExtensionKey, MaterialExtensionPipeline}, prelude::*, render::render_resource::{AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError}, shader::{Shader, Source}};
use crate::{arc_core::*, quillshade_bridge::QuillshadeBinding, terrain_render::{TerrainGrid, TerrainMesh}};

#[derive(Resource, Reflect, Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[reflect(Resource, Default, Clone)]
pub struct TerrainSurfaces {
    pub master: Option<QuillshadeBinding>,
    pub layers: [Option<QuillshadeBinding>; SPLAT_LAYERS],
}

#[derive(Component)]
pub struct TerrainLayer(pub usize);

#[derive(Asset, AsBindGroup, Reflect, Clone, Debug, Default)]
pub struct LayerExtension {}

impl MaterialExtension for LayerExtension {
    fn specialize(_: &MaterialExtensionPipeline, descriptor: &mut RenderPipelineDescriptor, _: &MeshVertexBufferLayoutRef, _: MaterialExtensionKey<Self>) -> Result<(), SpecializedMeshPipelineError> {
        if let Some(fragment) = descriptor.fragment.as_mut() { fragment.shader_defs.push("ARC_SURFACE_LAYER".into()); }
        Ok(())
    }
}

pub type LayerMaterial<M> = ExtendedMaterial<M, LayerExtension>;

/// Apply the same generated material and uniforms, with a disjoint splat coverage interval.
pub fn instance<M: Material + Clone>(w: &mut World, e: Entity, value: M) {
    let handle = w.resource_mut::<Assets<LayerMaterial<M>>>().add(LayerMaterial { base: value, extension: LayerExtension {} });
    w.entity_mut(e).insert(MeshMaterial3d(handle));
}

/// Adapt loaded shader assets in memory. Asset/source files and layout features are untouched.
fn layer_shader(source: &str) -> Option<String> {
    if source.contains("// ARC_SURFACE_LAYER adapter") { return None; }
    let signature = "fn fragment(in: VertexOutput";
    let start = source.find(signature)?;
    let mut result = source.to_owned();
    result.replace_range(start..start + signature.len(), "fn fragment(arc_surface_input: VertexOutput");
    let body = start + result[start..].find('{')? + 1;
    result.insert_str(body, "\n    // ARC_SURFACE_LAYER adapter\n    var in = arc_surface_input;\n    @if(ARC_SURFACE_LAYER) {\n        let arc_pixel = floor(in.position.xy);\n        let arc_rank = fract(sin(dot(arc_pixel, vec2<f32>(12.9898, 78.233))) * 43758.5453);\n        if (arc_rank < in.color.r || arc_rank >= in.color.g) { discard; }\n        in.color = vec4<f32>(1.0);\n    }\n");
    Some(result)
}

fn adapt_shaders(mut events: MessageReader<AssetEvent<Shader>>, mut shaders: ResMut<Assets<Shader>>) {
    let ids: Vec<_> = events.read().filter_map(|e| match e { AssetEvent::Added { id } | AssetEvent::Modified { id } => Some(*id), _ => None }).collect();
    for id in ids {
        let Some(shader) = shaders.get(id) else { continue; };
        if !shader.path.replace('\\', "/").contains("quillshade/material/autogen/") { continue; }
        let Source::Wesl(source) = &shader.source else { continue; };
        let Some(source) = layer_shader(source) else { continue; };
        if let Some(mut shader) = shaders.get_mut(id) { shader.source = Source::Wesl(source.into()); }
    }
}

pub fn set(w: &mut World, layer: Option<usize>, binding: Option<QuillshadeBinding>) -> Result<String, String> {
    if layer.is_some_and(|i| i >= SPLAT_LAYERS) { return Err("terrain layer out of range".into()); }
    if let Some(binding) = &binding { crate::quillshade_bridge::validate_binding(w, binding)?; }
    let before = crate::operators::snap_resource::<TerrainSurfaces>(w);
    let mut surfaces = w.resource_mut::<TerrainSurfaces>();
    if let Some(i) = layer { surfaces.layers[i] = binding; } else { surfaces.master = binding; }
    let after = crate::operators::snap_resource::<TerrainSurfaces>(w);
    crate::operators::record_edit(w, "Terrain surface", before, after, false);
    Ok("terrain surface updated".into())
}

pub fn describe(w: &mut World) -> Vec<serde_json::Value> {
    w.query::<(Entity, &TerrainLayer, &Mesh3d)>().iter(w).map(|(entity, layer, mesh)| serde_json::json!({
        "entity":entity.to_bits(), "layer":layer.0,
        "vertices":w.resource::<Assets<Mesh>>().get(&mesh.0).map_or(0, |m| m.count_vertices())
    })).collect()
}

fn sync(mut commands: Commands, surfaces: Res<TerrainSurfaces>, mut previous: Local<Option<TerrainSurfaces>>, mut splat_spec: Local<Option<[f32; 3]>>, grid: Res<TerrainGrid>, spec: Res<WorldSpec>, master: Query<(Entity, &Mesh3d), With<TerrainMesh>>, sections: Query<Entity, With<TerrainLayer>>, mut meshes: ResMut<Assets<Mesh>>) {
    let changed = previous.as_ref() != Some(&*surfaces);
    let key = [spec.water_level, spec.max_height, u8::from(spec.terrain_enabled) as f32];
    if !changed && !grid.is_changed() && *splat_spec == Some(key) { return; }
    let Ok((entity, handle)) = master.single() else { return; };
    let Some(base) = meshes.get(&handle.0).cloned() else { return; };
    if changed {
        if let Some(binding) = &surfaces.master { commands.entity(entity).insert(binding.clone()); }
        else { commands.entity(entity).remove::<QuillshadeBinding>(); }
    }
    *previous = Some(surfaces.clone());
    *splat_spec = Some(key);
    for entity in &sections { commands.entity(entity).despawn(); }
    for (layer, binding) in surfaces.layers.iter().enumerate() {
        let Some(binding) = binding else { continue; };
        let mut mesh = base.clone();
        let mut coverage = Vec::with_capacity(grid.0.n * grid.0.n);
        for j in 0..grid.0.n { for i in 0..grid.0.n {
            let interval = splat_intervals(grid.0.splat_weights(i, j, &spec))[layer];
            coverage.push([interval[0], interval[1], 0.0, 1.0]);
        } }
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, coverage);
        commands.spawn((Name::new(format!("Terrain layer: {layer}")), TerrainLayer(layer), Mesh3d(meshes.add(mesh)), binding.clone(), if spec.terrain_enabled { Visibility::Visible } else { Visibility::Hidden }, bevy::picking::Pickable::IGNORE, bevy::light::NotShadowCaster));
    }
}

pub struct TerrainSurfacesPlugin;
impl Plugin for TerrainSurfacesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TerrainSurfaces>().register_type::<TerrainSurfaces>()
            .add_systems(PostUpdate, (adapt_shaders, sync).before(bevy::transform::TransformSystems::Propagate));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shader_adapter_is_idempotent_and_preserves_generated_body() {
        let input = "@fragment\nfn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {\n    return base(in, is_front);\n}";
        let output = layer_shader(input).unwrap();
        assert!(output.contains("return base(in, is_front);"));
        assert!(output.contains("arc_rank < in.color.r || arc_rank >= in.color.g"));
        assert!(layer_shader(&output).is_none());
    }
    #[test]
    fn six_weight_intervals_partition_coverage_without_overlapping() {
        let weights = [0.1, 0.05, 0.3, 0.15, 0.2, 0.2];
        let intervals = splat_intervals(weights);
        for (i, [lo, hi]) in intervals.into_iter().enumerate() {
            assert!((hi - lo - weights[i]).abs() < 1e-6);
            if i > 0 { assert_eq!(lo, intervals[i - 1][1]); }
        }
        assert_eq!(intervals[0][0], 0.0);
        assert_eq!(intervals[5][1], 1.0);
    }
}
