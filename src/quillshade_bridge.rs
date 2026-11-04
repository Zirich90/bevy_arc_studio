//! Studio-owned adapter. Generated Quillshade files are never edited here.
use crate::{
    editor_registry::{EditorAppExt, PanelLocation},
    operators::{self, Target},
    world_io::SceneComponentAppExt,
};
use bevy::pbr::Material;
use bevy::prelude::*;
use bevy_egui::egui;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Reflect, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum MaterialValue {
    Float(f32),
    Vector([f32; 4]),
}

#[derive(Reflect, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MaterialParameter {
    pub name: String,
    pub value: MaterialValue,
}

#[derive(Component, Reflect, Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[reflect(Component, Default, Clone)]
pub struct QuillshadeBinding {
    pub material: String,
    #[serde(default)]
    #[reflect(default)]
    pub layout: String,
    #[serde(default)]
    #[reflect(default)]
    pub parameters: Vec<MaterialParameter>,
}

#[derive(Clone)]
pub struct QuillshadeMaterialDef {
    pub id: &'static str,
    pub layout: &'static str,
    pub defaults: Vec<MaterialParameter>,
    pub apply: Arc<dyn Fn(&mut World, Entity, &QuillshadeBinding) -> Result<(), String> + Send + Sync>,
    pub remove: fn(&mut World, Entity),
    pub inspect: Arc<dyn Fn(&World, Entity) -> Option<serde_json::Value> + Send + Sync>,
}

/// Read only the generated uniform fields; padding and textures are not numeric overrides.
pub fn parameters(value: &dyn PartialReflect) -> Vec<MaterialParameter> {
    let bevy::reflect::ReflectRef::Struct(fields) = value.reflect_ref() else { return Vec::new() };
    (0..fields.field_len()).filter_map(|i| {
        let name = fields.name_at(i)?;
        if !name.starts_with("inp_") { return None; }
        let field = fields.field_at(i)?;
        let value = if let Some(v) = field.try_downcast_ref::<f32>() { MaterialValue::Float(*v) }
            else if let Some(v) = field.try_downcast_ref::<Vec4>() { MaterialValue::Vector(v.to_array()) }
            else { return None; };
        Some(MaterialParameter { name: name.into(), value })
    }).collect()
}

fn override_parameters(value: &mut dyn PartialReflect, overrides: &[MaterialParameter]) -> Result<(), String> {
    let bevy::reflect::ReflectMut::Struct(fields) = value.reflect_mut() else { return Err("missing uniform struct".into()) };
    let mut names = std::collections::BTreeSet::new();
    for p in overrides {
        if !p.name.starts_with("inp_") || !names.insert(&p.name) { return Err("invalid or duplicate uniform".into()); }
        let field = fields.field_mut(&p.name).ok_or_else(|| format!("unknown uniform {}", p.name))?;
        match &p.value {
            MaterialValue::Float(v) if v.is_finite() => *field.try_downcast_mut::<f32>().ok_or("uniform type mismatch")? = *v,
            MaterialValue::Vector(v) if v.iter().all(|v| v.is_finite()) => *field.try_downcast_mut::<Vec4>().ok_or("uniform type mismatch")? = Vec4::from_array(*v),
            _ => return Err("non-finite uniform".into()),
        }
    }
    Ok(())
}

type Uniforms<M> = fn(&mut M) -> Option<&mut dyn PartialReflect>;

fn instance<M: Material + Clone>(w: &mut World, e: Entity, base: &Handle<M>, binding: &QuillshadeBinding, uniforms: Uniforms<M>) -> Result<(), String> {
    let mut value = w.resource::<Assets<M>>().get(base).ok_or("material is still loading")?.clone();
    if !binding.parameters.is_empty() {
        override_parameters(uniforms(&mut value).ok_or("material has no numeric uniforms")?, &binding.parameters)?;
    }
    if w.get::<crate::terrain_surfaces::TerrainLayer>(e).is_some() {
        crate::terrain_surfaces::instance(w, e, value);
    } else {
        let handle = w.resource_mut::<Assets<M>>().add(value);
        w.entity_mut(e).insert(MeshMaterial3d(handle));
    }
    Ok(())
}

pub fn register<M: Material + Clone>(app: &mut App, id: &'static str, layout: &'static str, uniforms: Uniforms<M>, defaults: Vec<MaterialParameter>) where M::Data: PartialEq + Eq + std::hash::Hash + Copy {
    app.add_plugins(bevy::pbr::MaterialPlugin::<crate::terrain_surfaces::LayerMaterial<M>>::default());
    let def = QuillshadeMaterialDef {
        id,
        layout,
        defaults,
        apply: Arc::new(move |w, e, binding| {
            if (!binding.layout.is_empty() || !binding.parameters.is_empty()) && binding.layout != layout {
                return Err("material layout changed; reset overrides after regeneration".into());
            }
            let handle = w
                .resource::<quillshade::material::BakedMaterials>()
                .get::<M>(id)
                .ok_or("material type mismatch")?;
            instance(w, e, &handle, binding, uniforms)
        }),
        remove: |w, e| {
            w.entity_mut(e).remove::<MeshMaterial3d<M>>();
            w.entity_mut(e).remove::<MeshMaterial3d<crate::terrain_surfaces::LayerMaterial<M>>>();
        },
        inspect: Arc::new(move |w, e| {
            if let Some(handle) = w.get::<MeshMaterial3d<crate::terrain_surfaces::LayerMaterial<M>>>(e) {
                let mut value = w.resource::<Assets<crate::terrain_surfaces::LayerMaterial<M>>>().get(&handle.0)?.base.clone();
                return Some(serde_json::json!({"handle":format!("{:?}", handle.0.id()), "parameters":uniforms(&mut value).map(|v| parameters(v)).unwrap_or_default()}));
            }
            let handle = &w.get::<MeshMaterial3d<M>>(e)?.0;
            let mut value = w.resource::<Assets<M>>().get(handle)?.clone();
            Some(serde_json::json!({"handle":format!("{:?}", handle.id()), "parameters":uniforms(&mut value).map(|v| parameters(v)).unwrap_or_default()}))
        }),
    };
    app.world_mut()
        .resource_mut::<crate::editor_registry::EditorRegistry>()
        .quillshade_materials
        .push(def);
    app.add_systems(PostUpdate, move |mut events: MessageReader<AssetEvent<M>>, baked: Res<quillshade::material::BakedMaterials>, mut applied: Query<&mut PreviousMaterial>| {
        let Some(base) = baked.get::<M>(id) else { return; };
        if events.read().any(|e| matches!(e, AssetEvent::Modified { id } | AssetEvent::Added { id } if *id == base.id())) {
            for mut previous in &mut applied {
                if previous.applied.material == id { previous.applied.material.clear(); }
            }
        }
    });
}

#[derive(Component)]
struct PreviousMaterial {
    original: Option<Handle<StandardMaterial>>,
    applied: QuillshadeBinding,
}

#[derive(Resource, Default)]
pub struct QuillshadeEnvironment(pub bool);

pub fn environment(w: &mut World, path: &str) -> Result<String, String> {
    crate::environment::set(w, Some(path.into()), 0.0)
}

pub fn assign(w: &mut World, target: &Target, material: &str) -> Result<String, String> {
    if !w
        .resource::<crate::editor_registry::EditorRegistry>()
        .quillshade_materials
        .iter()
        .any(|m| m.id == material)
    {
        return Err(format!("unknown 3D Quillshade material: {material}"));
    }
    write_binding(w, target, QuillshadeBinding { material: material.into(), ..default() })
}

pub fn write_binding(w: &mut World, target: &Target, binding: QuillshadeBinding) -> Result<String, String> {
    let e = operators::resolve(w, target)?;
    if let Some(result) = write_member_binding(w, e, Some(&binding)) { return result; }
    operators::add_component(w, target, "QuillshadeBinding", Some(&serde_json::to_string(&binding).map_err(|e| e.to_string())?))
}

pub fn write_member_binding(w: &mut World, e: Entity, binding: Option<&QuillshadeBinding>) -> Option<Result<String, String>> {
    let member = w.get::<crate::prefab_variants::PrefabMember>(e)?.clone();
    Some((|| {
        let patches: serde_json::Value = serde_json::from_str(w.get::<crate::prefab_variants::PrefabOverrides>(member.root).map_or("{}", |p| &p.json)).map_err(|e| e.to_string())?;
        let mut components = patches.get(&member.key).and_then(|p| p.get("components")).cloned().unwrap_or_else(|| serde_json::json!({}));
        components[QuillshadeBinding::type_path()] = serde_json::to_value(binding).map_err(|e| e.to_string())?;
        crate::prefab_variants::set_override(w, member.root, &member.key, "components", &components.to_string())
    })())
}

pub fn set_parameters(w: &mut World, target: &Target, layout: &str, parameters: Vec<MaterialParameter>) -> Result<String, String> {
    let e = operators::resolve(w, target)?;
    let mut binding = w.get::<QuillshadeBinding>(e).cloned().ok_or("assign a Quillshade material first")?;
    binding.layout = layout.into();
    binding.parameters = parameters;
    validate_binding(w, &binding)?;
    write_binding(w, target, binding)
}

pub fn validate_binding(w: &World, binding: &QuillshadeBinding) -> Result<(), String> {
    let registry = w.resource::<crate::editor_registry::EditorRegistry>();
    let def = registry.quillshade_materials.iter().find(|d| d.id == binding.material).ok_or("unknown material")?;
    if (!binding.layout.is_empty() || !binding.parameters.is_empty()) && binding.layout != def.layout { return Err("material layout mismatch".into()); }
    let mut names = std::collections::BTreeSet::new();
    for parameter in &binding.parameters {
        if !names.insert(&parameter.name) { return Err("duplicate uniform".into()); }
        let field = def.defaults.iter().find(|p| p.name == parameter.name).ok_or("unknown uniform")?;
        match (&field.value, &parameter.value) {
            (MaterialValue::Float(_), MaterialValue::Float(v)) if v.is_finite() => (),
            (MaterialValue::Vector(_), MaterialValue::Vector(v)) if v.iter().all(|v| v.is_finite()) => (),
            _ => return Err("invalid uniform type or value".into()),
        }
    }
    Ok(())
}

pub fn material_instances(w: &mut World) -> Vec<serde_json::Value> {
    let defs = w.resource::<crate::editor_registry::EditorRegistry>().quillshade_materials.clone();
    let meshes: Vec<_> = w.query_filtered::<Entity, With<Mesh3d>>().iter(w).collect();
    let mut result = Vec::new();
    for e in meshes {
        for def in &defs {
            if let Some(value) = (def.inspect)(w, e) {
                result.push(serde_json::json!({"entity":e.to_bits(), "name":w.get::<Name>(e).map(Name::as_str), "material":def.id, "instance":value}));
            }
        }
    }
    result
}

fn load(mut loader: quillshade::material::BakedMaterialsLoader) {
    loader.load();
}

fn binding_inspector(ui: &mut egui::Ui, value: &mut dyn bevy::reflect::PartialReflect) -> bool {
    let Some(binding) = value.try_downcast_mut::<QuillshadeBinding>() else {
        return false;
    };
    let old_material = binding.material.clone();
    let mut changed = false;
    egui::ComboBox::from_label("Generated material")
        .selected_text(&binding.material)
        .show_ui(ui, |ui| {
            for id in crate::quillshade_catalog::MATERIAL_IDS {
                changed |= ui
                    .selectable_value(&mut binding.material, (*id).to_string(), *id)
                    .changed();
            }
        });
    if old_material != binding.material {
        binding.layout.clear();
        binding.parameters.clear();
    }
    if let Some((layout, defaults)) = crate::quillshade_catalog::uniforms(&binding.material) {
        if ui.button("Add / reset numeric parameters").clicked() {
            binding.layout = layout.into();
            binding.parameters = defaults;
            changed = true;
        }
        if !binding.parameters.is_empty() && binding.layout != layout {
            ui.colored_label(egui::Color32::YELLOW, "Layout changed. Reset numeric parameters.");
        } else {
            for p in &mut binding.parameters {
                ui.horizontal(|ui| {
                    ui.label(&p.name);
                    match &mut p.value {
                        MaterialValue::Float(v) => changed |= ui.add(egui::DragValue::new(v).speed(0.01)).changed(),
                        MaterialValue::Vector(values) => {
                            for v in values { changed |= ui.add(egui::DragValue::new(v).speed(0.01)).changed(); }
                        }
                    }
                });
            }
            if !binding.parameters.is_empty() && ui.button("Clear overrides (follow source)").clicked() {
                binding.parameters.clear();
                binding.layout.clear();
                changed = true;
            }
        }
    }
    changed
}

// Reconcile from persisted bindings after load, undo and PIE rollback, including late GLTF children.
fn reconcile(w: &mut World) {
    let defs = w
        .resource::<crate::editor_registry::EditorRegistry>()
        .quillshade_materials
        .clone();
    let meshes: Vec<Entity> = w.query_filtered::<Entity, With<Mesh3d>>().iter(w).collect();
    for e in meshes {
        let mut cursor = Some(e);
        let mut binding = None;
        while let Some(at) = cursor {
            if let Some(b) = w.get::<QuillshadeBinding>(at) {
                binding = Some(b.clone());
                break;
            }
            cursor = w.get::<ChildOf>(at).map(|p| p.parent());
        }
        let def = binding
            .as_ref()
            .and_then(|b| defs.iter().find(|d| d.id == b.material));
        if let Some(def) = def {
            let binding = binding.as_ref().unwrap();
            if w.get::<PreviousMaterial>(e)
                .is_some_and(|p| &p.applied == binding)
            {
                continue;
            }
            if w.get::<PreviousMaterial>(e).is_none() {
                let old = w
                    .get::<MeshMaterial3d<StandardMaterial>>(e)
                    .map(|m| m.0.clone());
                w.entity_mut(e).insert(PreviousMaterial {
                    original: old,
                    applied: QuillshadeBinding::default(),
                });
            }
            // Keep the last valid render material when a persisted override is invalid or still loading.
            if (def.apply)(w, e, binding).is_err() { continue; }
            w.entity_mut(e).remove::<MeshMaterial3d<StandardMaterial>>();
            for d in &defs {
                if d.id != def.id { (d.remove)(w, e); }
            }
            w.get_mut::<PreviousMaterial>(e).unwrap().applied = binding.clone();
        } else if let Some(old) = w.entity_mut(e).take::<PreviousMaterial>() {
            for d in &defs {
                (d.remove)(w, e);
            }
            if let Some(h) = old.original {
                w.entity_mut(e).insert(MeshMaterial3d(h));
            }
        }
    }
}

fn panel(ui: &mut egui::Ui, d: &mut crate::ui::UiData, _: &crate::ui_inspector::Filter) {
    ui.label("Quillshade · generated materials");
    for m in d.reg.quillshade_materials.clone() {
        if ui.button(m.id).clicked() {
            if let Some(e) = d.sel.primary {
                d.queue.push(
                    crate::mcp_commands::StudioCommand::AssignQuillshadeMaterial {
                        target: Target::Entity(e),
                        material: m.id.into(),
                    },
                );
            }
        }
    }
    ui.weak("Select a scene object, tile or prefab root. Parameters: Quillshade RON; binding: Reflect inspector.");
    ui.separator();
    for slot in 0..=crate::arc_core::SPLAT_LAYERS {
        let layer = slot.checked_sub(1);
        let current = layer.map_or_else(|| d.terrain_surfaces.master.clone(), |i| d.terrain_surfaces.layers[i].clone());
        let name = layer.map_or("Terrain master", |i| ["Sand", "Gravel", "Grass", "Moss", "Rock", "Snow"][i]);
        ui.push_id(("terrain_surface", slot), |ui| {
            ui.collapsing(name, |ui| {
                let mut binding = current.clone().unwrap_or_else(|| QuillshadeBinding { material: "env/terrain_layers".into(), ..default() });
                let changed = binding_inspector(ui, &mut binding);
                if changed || ui.button("Apply surface material").clicked() {
                    d.queue.push(crate::mcp_commands::StudioCommand::SetTerrainMaterial { layer, binding: Some(binding) });
                }
                if current.is_some() && ui.button("Reset surface slot").clicked() {
                    d.queue.push(crate::mcp_commands::StudioCommand::SetTerrainMaterial { layer, binding: None });
                }
            });
        });
    }
    ui.separator();
    let seed = d.water.selected.and_then(|i| d.water.items.get(i)).map(|b| b.seed);
    ui.label(if seed.is_some() { "Selected water basin material" } else { "Global water material" });
    let current = if let Some(i) = d.water.selected { d.water.items.get(i).and_then(|b| b.material.clone()) } else { d.water.material.clone() };
    let mut binding = current.clone().unwrap_or_else(|| QuillshadeBinding { material: "env/water".into(), ..default() });
    let mut apply = binding_inspector(ui, &mut binding);
    apply |= ui.button("Apply to water surface").clicked();
    if apply { d.queue.push(crate::mcp_commands::StudioCommand::SetWaterMaterial { seed, binding: Some(binding) }); }
    if current.is_some() && ui.button("Restore standard water").clicked() {
        d.queue.push(crate::mcp_commands::StudioCommand::SetWaterMaterial { seed, binding: None });
    }
    if ui.button("Restore studio environment").clicked() {
        d.queue.push(crate::mcp_commands::StudioCommand::TransitionEnvironment { preset: None, duration: 0.0 });
    }
    for preset in quillshade::light::Preset::ALL {
        if ui.button(format!("Transition to {} (2 s)", preset.as_str())).clicked() {
            d.queue.push(crate::mcp_commands::StudioCommand::TransitionEnvironment { preset: Some(preset.as_str().into()), duration: 2.0 });
        }
        if ui
            .button(format!("Environment: {}", preset.as_str()))
            .clicked()
        {
            d.queue.push(
                crate::mcp_commands::StudioCommand::SetQuillshadeEnvironment {
                    preset: preset.as_str().into(),
                },
            );
        }
    }
}

pub struct QuillshadeBridgePlugin;
impl Plugin for QuillshadeBridgePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((quillshade::QuillshadePlugin, crate::terrain_surfaces::TerrainSurfacesPlugin))
            .init_resource::<QuillshadeEnvironment>()
            .add_plugins(crate::environment::EnvironmentPlugin)
            .register_type::<QuillshadeBinding>()
            .register_scene_component::<QuillshadeBinding>()
            .register_panel(
                "quillshade",
                "Quillshade",
                PanelLocation::RightInspector,
                panel,
            )
            .add_systems(Startup, load)
            .add_systems(
                PostUpdate,
                reconcile.before(bevy::transform::TransformSystems::Propagate),
            );
        crate::quillshade_catalog::register(app);
        app.register_inspector_override::<QuillshadeBinding>(binding_inspector);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quillshade::material::materials::dir_fx::mat_hologram::{HologramMaterial, HologramParams};

    #[test]
    fn instances_are_independent_and_rebase_without_losing_overrides() {
        let mut w = World::new();
        w.insert_resource(Assets::<HologramMaterial>::default());
        let base = w.resource_mut::<Assets<HologramMaterial>>().add(HologramMaterial { params: HologramParams::default() });
        let a = w.spawn_empty().id();
        let b = w.spawn_empty().id();
        let binding = QuillshadeBinding { material: "fx/hologram".into(), parameters: vec![MaterialParameter { name: "inp_scan_speed".into(), value: MaterialValue::Float(0.9) }], ..default() };
        instance(&mut w, a, &base, &binding, |m| Some(&mut m.params)).unwrap();
        instance(&mut w, b, &base, &QuillshadeBinding::default(), |m| Some(&mut m.params)).unwrap();
        let ah = w.get::<MeshMaterial3d<HologramMaterial>>(a).unwrap().0.clone();
        let bh = w.get::<MeshMaterial3d<HologramMaterial>>(b).unwrap().0.clone();
        assert_ne!(ah, bh);
        assert_ne!(ah, base);
        assert_eq!(w.resource::<Assets<HologramMaterial>>().get(&base).unwrap().params.inp_scan_speed, 0.5);
        assert_eq!(w.resource::<Assets<HologramMaterial>>().get(&ah).unwrap().params.inp_scan_speed, 0.9);
        w.resource_mut::<Assets<HologramMaterial>>().get_mut(&base).unwrap().params.inp_color = Vec4::ONE;
        instance(&mut w, a, &base, &binding, |m| Some(&mut m.params)).unwrap();
        let assets = w.resource::<Assets<HologramMaterial>>();
        let updated = assets.get(&w.get::<MeshMaterial3d<HologramMaterial>>(a).unwrap().0).unwrap();
        assert_eq!(updated.params.inp_color, Vec4::ONE);
        assert_eq!(updated.params.inp_scan_speed, 0.9);
        assert_ne!(assets.get(&bh).unwrap().params.inp_color, Vec4::ONE);
    }

    #[test]
    fn uniform_overrides_reject_padding_unknown_types_and_nonfinite_values() {
        let mut params = HologramParams::default();
        assert_eq!(parameters(&params).len(), 2);
        for (name, value) in [
            ("_pad0", MaterialValue::Float(1.0)),
            ("unknown", MaterialValue::Float(1.0)),
            ("inp_color", MaterialValue::Float(1.0)),
            ("inp_scan_speed", MaterialValue::Float(f32::NAN)),
            ("inp_color", MaterialValue::Vector([f32::INFINITY; 4])),
        ] {
            assert!(override_parameters(&mut params, &[MaterialParameter { name: name.into(), value }]).is_err());
        }
        let p = MaterialParameter { name: "inp_scan_speed".into(), value: MaterialValue::Float(1.0) };
        assert!(override_parameters(&mut params, &[p.clone(), p]).is_err());
        let legacy: QuillshadeBinding = serde_json::from_str(r#"{"material":"fx/hologram"}"#).unwrap();
        assert!(legacy.parameters.is_empty());
        assert!(legacy.layout.is_empty());
    }
}
