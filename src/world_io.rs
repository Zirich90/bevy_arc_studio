//! Reflect scene IO (stage 06): `.scn.json` = `DynamicWorld` of the editor objects + world resources,
//! plus a `<file>.heights` sidecar (raw little-endian f32 heightfield).
//!
//! Any `#[derive(Reflect)]` component registered with `App::register_scene_component::<T>()` is saved on
//! editor objects and restored on load without further code (gameplay components of later stages).

use std::any::TypeId;

use bevy::asset::AssetServer;
use bevy::ecs::reflect::{AppTypeRegistry, ReflectComponent};
use bevy::prelude::*;
use bevy::reflect::{FromReflect, GetTypeRegistration, PartialReflect, TypeRegistry};
use bevy::world_serialization::serde::{DynamicWorldSerializer, WorldDeserializer};
use bevy::world_serialization::{DynamicWorld, DynamicWorldBuilder, WorldFilter};
use serde::de::DeserializeSeed;

use crate::arc_core::*;
use crate::mcp_commands::StudioCommandQueue;
use crate::props::UserProp;
use crate::selection::{Deleted, EditorSelection};
use crate::studio::{Spawner, StudioState};
use crate::terrain_render::{PendingHeights, TerrainGrid};

pub const SCENE_FORMAT_VERSION: u32 = 2;

/// True for the reflect format (`*.scn.json`); other `.json` paths use the legacy v1 loader.
pub fn is_reflect_path(path: &str) -> bool {
    path.ends_with(".scn.json")
}

fn heights_path(path: &str) -> String {
    format!("{path}.heights")
}

/// Component types stored on editor objects in addition to `Transform` + `UserProp`.
#[derive(Resource, Default)]
pub struct SceneComponentRegistry {
    ids: Vec<TypeId>,
}

impl SceneComponentRegistry {
    pub fn ids(&self) -> &[TypeId] {
        &self.ids
    }
    pub fn contains(&self, id: TypeId) -> bool {
        self.ids.contains(&id)
    }
}

pub trait SceneComponentAppExt {
    /// Register a reflected component so it is saved/loaded with editor objects.
    fn register_scene_component<T: Component + Reflect + GetTypeRegistration>(&mut self) -> &mut Self;
}

impl SceneComponentAppExt for App {
    fn register_scene_component<T: Component + Reflect + GetTypeRegistration>(&mut self) -> &mut Self {
        self.register_type::<T>();
        self.world_mut().get_resource_or_insert_with(SceneComponentRegistry::default).ids.push(TypeId::of::<T>());
        // also listed in the inspector's [+ Add Component] menu
        self.world_mut().get_resource_or_insert_with(crate::editor_registry::EditorRegistry::default).register_scene_component::<T>();
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SceneIo {
    Save(String),
    Load(String),
}

#[derive(Resource, Default)]
pub struct SceneIoQueue(pub Vec<SceneIo>);

pub struct WorldIoPlugin;

impl Plugin for WorldIoPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SceneIoQueue>()
            .init_resource::<SceneComponentRegistry>()
            .register_type::<UnitScale>()
            .init_resource::<UnitScale>()
            .add_systems(Update, (save_scene, load_scene).chain().after(crate::mcp_commands::execute_commands));
    }
}

fn component_filter(extras: &SceneComponentRegistry) -> WorldFilter {
    let base = WorldFilter::deny_all().allow::<Transform>().allow::<UserProp>();
    extras.ids.iter().fold(base, |f, id| f.allow_by_id(*id))
}

/// Build the serializable scene of the given editor-object entities and the world resources.
pub fn build_dynamic_world(world: &World, entities: impl Iterator<Item = Entity>) -> DynamicWorld {
    let registry = world.resource::<AppTypeRegistry>().clone();
    let registry = registry.read();
    let filter = component_filter(world.resource::<SceneComponentRegistry>());
    DynamicWorldBuilder::from_world(world, &registry)
        .with_component_filter(filter)
        .with_resource_filter(WorldFilter::deny_all().allow::<WorldSpec>().allow::<UnitScale>())
        .extract_entities(entities)
        .remove_empty_entities()
        .extract_resources()
        .build()
}

pub fn dynamic_world_to_json(dw: &DynamicWorld, registry: &TypeRegistry) -> Result<String, String> {
    serde_json::to_string_pretty(&DynamicWorldSerializer::new(dw, registry)).map_err(|e| e.to_string())
}

pub fn dynamic_world_from_json(json: &str, registry: &TypeRegistry, server: &mut AssetServer) -> Result<DynamicWorld, String> {
    let mut de = serde_json::Deserializer::from_str(json);
    WorldDeserializer { type_registry: registry, load_from_path: server }.deserialize(&mut de).map_err(|e| e.to_string())
}

fn report(queue: &mut StudioCommandQueue, studio: &mut StudioState, line: String) {
    studio.status = line.clone();
    queue.log.push(line);
}

fn save_scene(world: &mut World) {
    let reqs: Vec<String> = {
        let mut io = world.resource_mut::<SceneIoQueue>();
        let (saves, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut io.0).into_iter().partition(|r| matches!(r, SceneIo::Save(_)));
        io.0 = rest;
        saves.into_iter().filter_map(|r| if let SceneIo::Save(p) = r { Some(p) } else { None }).collect()
    };
    for path in reqs {
        let res = (|| -> Result<String, String> {
            let ents: Vec<Entity> = world.query_filtered::<Entity, (With<UserProp>, Without<Deleted>)>().iter(world).collect();
            let dw = build_dynamic_world(world, ents.iter().copied());
            let registry = world.resource::<AppTypeRegistry>().clone();
            let json = dynamic_world_to_json(&dw, &registry.read())?;
            if let Some(dir) = std::path::Path::new(&path).parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            std::fs::write(&path, json).map_err(|e| format!("{path}: {e}"))?;
            let heights = &world.resource::<TerrainGrid>().0.heights;
            let bytes: Vec<u8> = heights.iter().flat_map(|h| h.to_le_bytes()).collect();
            std::fs::write(heights_path(&path), bytes).map_err(|e| format!("{}: {e}", heights_path(&path)))?;
            Ok(format!("saved {path} (v{SCENE_FORMAT_VERSION}, {} objects)", dw.entities.len()))
        })();
        let line = res.unwrap_or_else(|e| format!("ERR {e}"));
        let mut q = world.resource_mut::<StudioCommandQueue>();
        q.log.push(line.clone());
        world.resource_mut::<StudioState>().status = line;
    }
}

#[allow(clippy::too_many_arguments)]
fn load_scene(
    mut io: ResMut<SceneIoQueue>,
    mut q: ResMut<StudioCommandQueue>,
    mut studio: ResMut<StudioState>,
    mut spec: ResMut<WorldSpec>,
    mut units: ResMut<UnitScale>,
    mut pending: ResMut<PendingHeights>,
    mut sel: ResMut<EditorSelection>,
    grid: Res<TerrainGrid>,
    registry: Res<AppTypeRegistry>,
    extras: Res<SceneComponentRegistry>,
    server: Res<AssetServer>,
    users: Query<Entity, With<UserProp>>,
    mut spawner: Spawner,
) {
    let (loads, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut io.0).into_iter().partition(|r| matches!(r, SceneIo::Load(_)));
    io.0 = rest;
    for r in loads {
        let SceneIo::Load(path) = r else { continue };
        let res = (|| -> Result<String, String> {
            let json = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
            let mut srv = (*server).clone();
            let reg = registry.read();
            let dw = dynamic_world_from_json(&json, &reg, &mut srv).map_err(|e| format!("{path}: {e}"))?;
            let hbytes = std::fs::read(heights_path(&path)).map_err(|e| format!("{}: {e}", heights_path(&path)))?;
            let heights: Vec<f32> = hbytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();

            for res in &dw.resources {
                if let Some(s) = WorldSpec::from_reflect(res.as_ref()) {
                    *spec = s;
                    spec.sanitize();
                } else if let Some(u) = UnitScale::from_reflect(res.as_ref()) {
                    *units = u;
                }
            }
            for e in &users {
                spawner.commands.entity(e).despawn();
            }
            sel.clear();
            pending.0 = Some(heights);

            let mut n = 0;
            for de in &dw.entities {
                let mut kind = None;
                let mut tf = None;
                let mut rest: Vec<Box<dyn PartialReflect>> = Vec::new();
                for c in &de.components {
                    let tid = c.get_represented_type_info().map(|i| i.type_id());
                    if let Some(u) = UserProp::from_reflect(c.as_ref()) {
                        kind = Some(u.kind);
                    } else if let Some(t) = Transform::from_reflect(c.as_ref()) {
                        tf = Some(t);
                    } else if tid.is_some_and(|t| extras.contains(t)) {
                        if let Ok(b) = c.reflect_clone() { rest.push(b.into_partial_reflect()); }
                    }
                }
                let (Some(kind), Some(tf)) = (kind, tf) else { continue };
                let yaw = tf.rotation.to_euler(EulerRot::YXZ).0;
                let Some((e, _)) = spawner.spawn(&grid.0, &spec, &kind, tf.translation.x, tf.translation.z, yaw) else { continue };
                spawner.commands.entity(e).insert(tf);
                if !rest.is_empty() {
                    spawner.commands.queue(move |world: &mut World| {
                        let registry = world.resource::<AppTypeRegistry>().clone();
                        let registry = registry.read();
                        for c in &rest {
                            let Some(info) = c.get_represented_type_info() else { continue };
                            let Some(rc) = registry.get(info.type_id()).and_then(|r| r.data::<ReflectComponent>()) else { continue };
                            rc.insert(&mut world.entity_mut(e), c.as_ref(), &registry);
                        }
                    });
                }
                n += 1;
            }
            Ok(format!("loaded {path} (v{SCENE_FORMAT_VERSION}, {n} objects)"))
        })();
        report(&mut q, &mut studio, res.unwrap_or_else(|e| format!("ERR {e}")));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Component, Reflect, Default, Clone, PartialEq, Debug)]
    #[reflect(Component)]
    struct Marker {
        hp: i32,
        tag: String,
    }

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_plugins(AssetPlugin::default());
        app.init_resource::<SceneComponentRegistry>()
            .register_type::<WorldSpec>()
            .register_type::<UnitScale>()
            .register_type::<UserProp>()
            .register_scene_component::<Marker>();
        app.init_resource::<WorldSpec>().init_resource::<UnitScale>();
        app
    }

    #[test]
    fn reflect_scene_roundtrip() {
        let mut app = test_app();
        let e = app.world_mut().spawn((Transform::from_xyz(1.0, 2.0, 3.0), UserProp { kind: "pine".into() }, Marker { hp: 7, tag: "npc".into() })).id();
        app.world_mut().resource_mut::<UnitScale>().world_per_foot = 0.3048;

        let dw = build_dynamic_world(app.world(), std::iter::once(e));
        assert_eq!(dw.entities.len(), 1);
        assert_eq!(dw.entities[0].components.len(), 3);
        let registry = app.world().resource::<AppTypeRegistry>().clone();
        let json = dynamic_world_to_json(&dw, &registry.read()).unwrap();

        let mut server = app.world().resource::<AssetServer>().clone();
        let back = dynamic_world_from_json(&json, &registry.read(), &mut server).unwrap();
        assert_eq!(back.entities.len(), 1);
        let found: Vec<_> = back.entities[0].components.iter().filter_map(|c| Marker::from_reflect(c.as_ref())).collect();
        assert_eq!(found, vec![Marker { hp: 7, tag: "npc".into() }]);
        assert!(back.resources.iter().any(|r| UnitScale::from_reflect(r.as_ref()).is_some_and(|u| u.world_per_foot == 0.3048)));
    }

    #[test]
    fn unregistered_components_not_saved() {
        let mut app = test_app();
        let e = app.world_mut().spawn((Transform::default(), UserProp { kind: "pine".into() }, Name::new("x"))).id();
        let dw = build_dynamic_world(app.world(), std::iter::once(e));
        assert_eq!(dw.entities[0].components.len(), 2);
    }

    #[test]
    fn reflect_path_detection() {
        assert!(is_reflect_path("scenes/a.scn.json"));
        assert!(!is_reflect_path("scenes/a.json"));
    }
}
