//! Studio-owned adapter. Generated Quillshade files are never edited here.
use crate::{
    editor_registry::{EditorAppExt, PanelLocation},
    operators::{self, Target},
    world_io::SceneComponentAppExt,
};
use bevy::pbr::Material;
use bevy::prelude::*;
use bevy_egui::egui;

#[derive(Component, Reflect, Clone, Debug, Default, PartialEq)]
#[reflect(Component, Default, Clone)]
pub struct QuillshadeBinding {
    pub material: String,
}

#[derive(Clone)]
pub struct QuillshadeMaterialDef {
    pub id: &'static str,
    pub apply: fn(&mut World, Entity, &str) -> Result<(), String>,
    pub remove: fn(&mut World, Entity),
}

pub fn register<M: Material>(app: &mut App, id: &'static str) {
    let def = QuillshadeMaterialDef {
        id,
        apply: |w, e, id| {
            let handle = w
                .resource::<quillshade::material::BakedMaterials>()
                .get::<M>(id)
                .ok_or("material type mismatch")?;
            w.entity_mut(e).insert(MeshMaterial3d(handle));
            Ok(())
        },
        remove: |w, e| {
            w.entity_mut(e).remove::<MeshMaterial3d<M>>();
        },
    };
    app.world_mut()
        .resource_mut::<crate::editor_registry::EditorRegistry>()
        .quillshade_materials
        .push(def);
}

#[derive(Component)]
struct PreviousMaterial {
    original: Option<Handle<StandardMaterial>>,
    applied: String,
}

#[derive(Resource, Default)]
pub struct QuillshadeEnvironment(pub bool);

pub fn environment(w: &mut World, path: &str) -> Result<String, String> {
    use bevy::ecs::system::RunSystemOnce;
    let preset =
        quillshade::light::Preset::from_path(path).ok_or("unknown Quillshade environment")?;
    w.run_system_once(move |mut light: quillshade::light::Light| {
        light.set_immediate(preset);
    })
    .map_err(|e| e.to_string())?;
    w.resource_mut::<QuillshadeEnvironment>().0 = true;
    let mut lights = w.query_filtered::<&mut DirectionalLight, Or<(With<crate::lighting::Sun>, With<crate::lighting::Moon>)>>();
    for mut light in lights.iter_mut(w) {
        light.illuminance = 0.0;
    }
    Ok(format!("Quillshade environment: {path}"))
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
    let json = serde_json::json!({"material": material}).to_string();
    operators::add_component(w, target, "QuillshadeBinding", Some(&json))
}

fn load(mut loader: quillshade::material::BakedMaterialsLoader) {
    loader.load();
}

fn binding_inspector(ui: &mut egui::Ui, value: &mut dyn bevy::reflect::PartialReflect) -> bool {
    let Some(binding) = value.try_downcast_mut::<QuillshadeBinding>() else {
        return false;
    };
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
                binding = Some(b.material.clone());
                break;
            }
            cursor = w.get::<ChildOf>(at).map(|p| p.parent());
        }
        let def = binding
            .as_ref()
            .and_then(|id| defs.iter().find(|d| d.id == id));
        if let Some(def) = def {
            if w.get::<PreviousMaterial>(e)
                .is_some_and(|p| p.applied == def.id)
            {
                continue;
            }
            if w.get::<PreviousMaterial>(e).is_none() {
                let old = w
                    .get::<MeshMaterial3d<StandardMaterial>>(e)
                    .map(|m| m.0.clone());
                w.entity_mut(e).insert(PreviousMaterial {
                    original: old,
                    applied: String::new(),
                });
            }
            w.entity_mut(e).remove::<MeshMaterial3d<StandardMaterial>>();
            for d in &defs {
                (d.remove)(w, e);
            }
            let _ = (def.apply)(w, e, def.id);
            w.get_mut::<PreviousMaterial>(e).unwrap().applied = def.id.into();
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
    for preset in quillshade::light::Preset::ALL {
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
        app.add_plugins(quillshade::QuillshadePlugin)
            .init_resource::<QuillshadeEnvironment>()
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
