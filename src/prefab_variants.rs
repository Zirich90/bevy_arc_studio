//! Linked prefab variants and undoable instance overrides. Files store only a base path and changed fields.
use crate::{
    operators::{self, Target},
    prefabs::{self, PrefabFile, PrefabInstance},
    world_io::SceneComponentAppExt,
};
use bevy::{
    prelude::*,
    reflect::serde::{TypedReflectDeserializer, TypedReflectSerializer},
};
use bevy_egui::egui;
use serde::{Deserialize, Serialize, de::DeserializeSeed};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use bevy::ecs::entity::{EntityMapper, MapEntities};

type Patches = BTreeMap<String, Value>;
/// Persistent source identity survives rename, reorder and source-scene reloads.
#[derive(Component, Reflect, Clone, Debug, Default, PartialEq)]
#[reflect(Component, Default, Clone)]
pub struct PrefabSourceKey {
    pub id: String,
}
#[derive(Component, Reflect, Clone, Debug, PartialEq)]
#[reflect(Component, Default, Clone)]
pub struct PrefabOverrides {
    pub json: String,
}
impl Default for PrefabOverrides {
    fn default() -> Self {
        Self { json: "{}".into() }
    }
}
#[derive(Component, Clone, Debug)]
#[component(map_entities)]
pub struct PrefabMember {
    pub root: Entity,
    pub key: String,
}
impl MapEntities for PrefabMember {
    fn map_entities<M: EntityMapper>(&mut self, mapper: &mut M) {
        self.root = mapper.get_mapped(self.root);
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Variant {
    version: u32,
    name: String,
    base: String,
    #[serde(default)]
    overrides: Patches,
}
#[derive(Component)]
struct HiddenByPrefabRoot;
#[derive(Resource, Default)]
struct Applied {
    timer: f32,
    roots: BTreeMap<Entity, String>,
    components: BTreeMap<Entity, BTreeSet<String>>,
}

fn safe_rel(path: &str) -> Result<String, String> {
    let rel = prefabs::normalize_rel(path);
    if !rel.starts_with("prefabs/")
        || rel
            .split('/')
            .any(|x| x == ".." || x.is_empty() || x.contains(':'))
    {
        return Err("prefab path must stay under assets/prefabs".into());
    }
    Ok(rel)
}
fn merge(base: &mut Value, patch: &Value) {
    if let (Some(a), Some(b)) = (base.as_object_mut(), patch.as_object()) {
        for (key, value) in b {
            merge(a.entry(key.clone()).or_insert(Value::Null), value);
        }
    } else {
        *base = patch.clone();
    }
}
fn patch_file(mut file: PrefabFile, patches: &Patches) -> Result<PrefabFile, String> {
    for (key, patch) in patches {
        let member = file
            .entities
            .iter_mut()
            .find(|m| &m.id == key)
            .ok_or_else(|| format!("missing prefab member '{key}'"))?;
        let fields = patch
            .as_object()
            .ok_or("member override must be an object")?;
        for field in fields.keys() {
            if !["name", "pos", "rot", "scale", "door", "light", "components"]
                .contains(&field.as_str())
            {
                return Err(format!("field '{field}' cannot be overridden"));
            }
        }
        let mut value = serde_json::to_value(&*member).map_err(|e| e.to_string())?;
        merge(&mut value, patch);
        *member = serde_json::from_value(value).map_err(|e| e.to_string())?;
        if member.tile.is_some()
            && fields
                .keys()
                .any(|k| ["pos", "rot", "scale"].contains(&k.as_str()))
        {
            return Err(
                "grid tile transforms follow the prefab root; edit the root transform".into(),
            );
        }
        let q = Quat::from_array(member.rot);
        if !Vec3::from(member.pos).is_finite()
            || !Vec3::from(member.scale).is_finite()
            || Vec3::from(member.scale).abs().min_element() < 0.00001
            || !q.is_finite()
            || q.length_squared() < 0.00001
        {
            return Err("invalid prefab transform".into());
        }
        member.rot = q.normalize().to_array();
    }
    Ok(file)
}
pub fn load_resolved(root: &Path, path: &str) -> Result<PrefabFile, String> {
    fn visit(root: &Path, path: &str, seen: &mut BTreeSet<String>) -> Result<PrefabFile, String> {
        let rel = safe_rel(path)?;
        if seen.len() >= 32 || !seen.insert(rel.clone()) {
            return Err("prefab inheritance cycle or depth > 32".into());
        }
        let raw = std::fs::read_to_string(root.join(&rel)).map_err(|e| format!("{rel}: {e}"))?;
        let value: Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
        let result = if value.get("base").is_some() {
            let v: Variant = serde_json::from_value(value).map_err(|e| e.to_string())?;
            if v.version != 2 {
                return Err("unsupported prefab variant version".into());
            }
            let mut f = patch_file(visit(root, &v.base, seen)?, &v.overrides)?;
            f.name = v.name;
            f
        } else {
            let mut f: PrefabFile = serde_json::from_value(value).map_err(|e| e.to_string())?;
            if f.version != prefabs::PREFAB_VERSION {
                return Err("unsupported prefab version".into());
            }
            let mut keys = BTreeSet::new();
            let count = f.entities.len();
            for (i, m) in f.entities.iter_mut().enumerate() {
                if m.id.is_empty() {
                    m.id = i.to_string();
                }
                if !keys.insert(m.id.clone()) {
                    return Err("duplicate prefab member id".into());
                }
                if m.parent.is_some_and(|p| p >= count || p == i) {
                    return Err("invalid prefab parent".into());
                }
            }
            for i in 0..f.entities.len() {
                let mut ancestry = BTreeSet::new();
                let mut at = Some(i);
                while let Some(p) = at {
                    if !ancestry.insert(p) {
                        return Err("prefab parent cycle".into());
                    }
                    at = f.entities[p].parent;
                }
            }
            f
        };
        seen.remove(&rel);
        Ok(result)
    }
    visit(root, path, &mut BTreeSet::new())
}
fn registration<'a>(
    reg: &'a bevy::reflect::TypeRegistry,
    name: &str,
) -> Result<&'a bevy::reflect::TypeRegistration, String> {
    reg.get_with_type_path(name)
        .or_else(|| reg.get_with_short_type_path(name))
        .ok_or_else(|| format!("unknown component {name}"))
}
fn component_allowed(w: &World, id: std::any::TypeId) -> bool {
    w.get_resource::<crate::world_io::SceneComponentRegistry>()
        .is_some_and(|r| r.contains(id))
        && ![
            std::any::TypeId::of::<PrefabInstance>(),
            std::any::TypeId::of::<PrefabOverrides>(),
            std::any::TypeId::of::<PrefabSourceKey>(),
            std::any::TypeId::of::<ChildOf>(),
            std::any::TypeId::of::<Transform>(),
        ]
        .contains(&id)
}
pub fn capture_components(w: &World, e: Entity) -> Result<BTreeMap<String, Value>, String> {
    let Some(registry) = w.get_resource::<AppTypeRegistry>() else {
        return Ok(default());
    };
    let registry = registry.read();
    let mut values = BTreeMap::new();
    if let Some(types) = w.get_resource::<crate::world_io::SceneComponentRegistry>() {
        for id in types.ids() {
            if !component_allowed(w, *id)
                || [
                    std::any::TypeId::of::<Name>(),
                    std::any::TypeId::of::<crate::level_builder::LevelTile>(),
                    std::any::TypeId::of::<crate::level_builder::Door>(),
                    std::any::TypeId::of::<PointLight>(),
                ]
                .contains(id)
            {
                continue;
            }
            if let Some(rc) = registry.get_type_data::<ReflectComponent>(*id) {
                if let Some(v) = w.get_entity(e).ok().and_then(|er| rc.reflect(er)) {
                    values.insert(
                        v.reflect_type_path().to_string(),
                        serde_json::to_value(TypedReflectSerializer::new(
                            v.as_partial_reflect(),
                            &registry,
                        ))
                        .map_err(|e| e.to_string())?,
                    );
                }
            }
        }
    }
    Ok(values)
}
pub fn validate_components(w: &World, f: &PrefabFile) -> Result<(), String> {
    if f.entities.iter().all(|m| m.components.is_empty()) {
        return Ok(());
    }
    let registry = w.resource::<AppTypeRegistry>().read();
    for member in &f.entities {
        for (name, value) in &member.components {
            let r = registration(&registry, name)?;
            if !component_allowed(w, r.type_id()) {
                return Err(format!("component {name} is not a prefab scene component"));
            }
            if !value.is_null() {
                TypedReflectDeserializer::new(r, &registry)
                    .deserialize(value.clone())
                    .map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}
pub fn apply_components(
    w: &mut World,
    e: Entity,
    values: &BTreeMap<String, Value>,
) -> Result<(), String> {
    if values.is_empty() {
        return Ok(());
    }
    let reg = w.resource::<AppTypeRegistry>().clone();
    let registry = reg.read();
    for (name, value) in values {
        let r = registration(&registry, name)?;
        if !component_allowed(w, r.type_id()) {
            return Err(format!("component {name} is not a prefab scene component"));
        }
        let rc = r.data::<ReflectComponent>().ok_or("not a component")?;
        if value.is_null() {
            rc.remove(&mut w.entity_mut(e));
        } else {
            let v = TypedReflectDeserializer::new(r, &registry)
                .deserialize(value.clone())
                .map_err(|e| e.to_string())?;
            rc.insert(&mut w.entity_mut(e), v.as_ref(), &registry);
        }
    }
    Ok(())
}
fn effective(w: &World, e: Entity, root: &Path) -> Result<PrefabFile, String> {
    let inst = w
        .get::<PrefabInstance>(e)
        .ok_or("select a prefab instance root")?;
    let patches: Patches = serde_json::from_str(
        w.get::<PrefabOverrides>(e)
            .map_or("{}", |p| p.json.as_str()),
    )
    .map_err(|e| e.to_string())?;
    patch_file(load_resolved(root, &inst.prefab_path)?, &patches)
}
/// Null resets a field (or an entire member when field is empty) to the inherited value.
pub fn set_override(
    w: &mut World,
    e: Entity,
    member: &str,
    field: &str,
    value: &str,
) -> Result<String, String> {
    set_override_at(w, e, member, field, value, &crate::props::assets_dir())
}
fn set_override_at(
    w: &mut World,
    e: Entity,
    member: &str,
    field: &str,
    value: &str,
    assets: &Path,
) -> Result<String, String> {
    let mut patches: Patches = serde_json::from_str(
        w.get::<PrefabOverrides>(e)
            .map_or("{}", |p| p.json.as_str()),
    )
    .map_err(|e| e.to_string())?;
    let v: Value = serde_json::from_str(value).map_err(|e| e.to_string())?;
    if field.is_empty() && v.is_null() {
        patches.remove(member);
    } else {
        let p = patches
            .entry(member.into())
            .or_insert_with(|| serde_json::json!({}));
        if v.is_null() {
            p.as_object_mut().ok_or("invalid override")?.remove(field);
        } else {
            p[field] = v;
        }
        if p.as_object().is_some_and(|p| p.is_empty()) {
            patches.remove(member);
        }
    }
    let base = w
        .get::<PrefabInstance>(e)
        .ok_or("select a prefab instance root")?;
    let f = patch_file(load_resolved(assets, &base.prefab_path)?, &patches)?;
    validate_components(w, &f)?;
    let before = operators::snap(w, e, std::any::TypeId::of::<PrefabOverrides>());
    w.entity_mut(e).insert(PrefabOverrides {
        json: serde_json::to_string(&patches).map_err(|e| e.to_string())?,
    });
    let after = operators::snap(w, e, std::any::TypeId::of::<PrefabOverrides>());
    operators::record_edit(w, "Prefab override", before, after, false);
    Ok(format!("prefab override {member}.{field}"))
}
pub fn save_variant(w: &World, e: Entity, path: &str) -> Result<String, String> {
    let root = crate::props::assets_dir();
    let inst = w
        .get::<PrefabInstance>(e)
        .ok_or("select a prefab instance root")?;
    let rel = safe_rel(path)?;
    let base = safe_rel(&inst.prefab_path)?;
    if rel == base || root.join(&rel).exists() {
        return Err("choose a new variant path; existing prefabs are not overwritten".into());
    }
    let resolved = effective(w, e, &root)?;
    validate_components(w, &resolved)?;
    let patches = serde_json::from_str(
        w.get::<PrefabOverrides>(e)
            .map_or("{}", |p| p.json.as_str()),
    )
    .map_err(|e| e.to_string())?;
    let file = Variant {
        version: 2,
        name: rel
            .rsplit('/')
            .next()
            .unwrap()
            .trim_end_matches(prefabs::PREFAB_EXT)
            .into(),
        base,
        overrides: patches,
    };
    let dest = root.join(&rel);
    std::fs::create_dir_all(dest.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(
        dest,
        serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(format!("saved linked variant {rel}"))
}
fn sync(w: &mut World) {
    sync_at(w, &crate::props::assets_dir());
}
fn sync_at(w: &mut World, assets: &Path) {
    let dt = w.resource::<Time>().delta_secs();
    let mut state = w.remove_resource::<Applied>().unwrap_or_default();
    state.timer += dt;
    let poll = state.timer >= 0.5;
    if poll {
        state.timer = 0.0;
    }
    let roots: Vec<_> = w.query_filtered::<(Entity, &PrefabInstance, Option<&PrefabOverrides>), Without<crate::selection::Deleted>>().iter(w)
        .map(|(e, i, o)| (e, i.prefab_path.clone(), o.map_or("{}", |o| o.json.as_str()).to_string())).collect();
    for (root, path, json) in roots {
        let request = format!("{path}:{json}");
        if !poll
            && state
                .roots
                .get(&root)
                .is_some_and(|s| s.starts_with(&format!("{request}\n")))
        {
            continue;
        }
        let f = match effective(w, root, assets) {
            Ok(f) => f,
            Err(e) => {
                if poll {
                    warn!("prefab: {e}");
                }
                continue;
            }
        };
        let fingerprint = format!(
            "{request}\n{}",
            serde_json::to_string(&f).unwrap_or_default()
        );
        // Wait for deferred hydration before caching the file fingerprint.
        let members: Vec<_> = w
            .query::<(Entity, &PrefabMember)>()
            .iter(w)
            .filter(|(_, m)| m.root == root)
            .map(|(e, m)| (e, m.key.clone()))
            .collect();
        if w.get::<crate::prefabs::PrefabHydrated>(root).is_none()
            || state.roots.get(&root) == Some(&fingerprint)
        {
            continue;
        }
        if let Err(e) = validate_components(w, &f) {
            warn!("prefab: {e}");
            continue;
        }
        let previous: Option<PrefabFile> = state
            .roots
            .get(&root)
            .and_then(|s| s.split_once('\n'))
            .and_then(|(_, json)| serde_json::from_str(json).ok());
        for (e, key) in &members {
            let new = f.entities.iter().find(|m| &m.id == key);
            let old = previous
                .as_ref()
                .and_then(|f| f.entities.iter().find(|m| &m.id == key));
            let changed_kind = old
                .zip(new)
                .is_some_and(|(a, b)| a.kind != b.kind || a.tile.is_some() != b.tile.is_some());
            if (new.is_none() || changed_kind) && w.get_entity(*e).is_ok() {
                w.entity_mut(*e).despawn();
            }
        }
        prefabs::spawn_members(w, root, &f);
        prefabs::spawn_tiles(w, root, &f);
        let members: Vec<_> = w
            .query::<(Entity, &PrefabMember)>()
            .iter(w)
            .filter(|(_, m)| m.root == root)
            .map(|(e, m)| (e, m.key.clone()))
            .collect();
        for (e, key) in members {
            let Some(m) = f.entities.iter().find(|m| m.id == key) else {
                continue;
            };
            let is_tile = w.get::<crate::prefabs::PrefabTile>(e).is_some();
            if !is_tile {
                w.entity_mut(e).insert(Transform {
                    translation: m.pos.into(),
                    rotation: Quat::from_array(m.rot),
                    scale: m.scale.into(),
                });
            }
            w.entity_mut(e).insert(Name::new(m.name.clone()));
            if let Some(mut so) = w.get_mut::<crate::props::SceneObject>(e) {
                so.name = m.name.clone();
            }
            if let Some(d) = m.door {
                w.entity_mut(e).insert(d);
            } else {
                w.entity_mut(e).remove::<crate::level_builder::Door>();
            }
            if let Some(l) = &m.light {
                w.entity_mut(e).insert(PointLight {
                    color: Color::srgba(l.color[0], l.color[1], l.color[2], l.color[3]),
                    intensity: l.intensity,
                    range: l.range,
                    ..default()
                });
            }
            if m.light.is_none() {
                w.entity_mut(e).remove::<PointLight>();
            }
            if let Some(old) = state.components.get(&e) {
                let removed: BTreeMap<_, _> = old
                    .iter()
                    .filter(|n| !m.components.contains_key(*n))
                    .map(|n| (n.clone(), Value::Null))
                    .collect();
                let _ = apply_components(w, e, &removed);
            }
            if let Err(err) = apply_components(w, e, &m.components) {
                warn!("prefab: {err}");
            }
            state
                .components
                .insert(e, m.components.keys().cloned().collect());
        }
        state.roots.insert(root, fingerprint);
    }
    // Grid members are not hierarchy children: mirror the root's soft-delete state for Undo/Redo.
    let tile_visibility: Vec<_> = w
        .query::<(Entity, &crate::prefabs::PrefabTile)>()
        .iter(w)
        .map(|(e, m)| (e, w.get::<crate::selection::Deleted>(m.root).is_some()))
        .collect();
    for (e, hidden) in tile_visibility {
        if hidden {
            w.entity_mut(e).insert((
                HiddenByPrefabRoot,
                crate::selection::Deleted,
                Visibility::Hidden,
            ));
        } else if w.get::<HiddenByPrefabRoot>(e).is_some() {
            w.entity_mut(e)
                .remove::<(HiddenByPrefabRoot, crate::selection::Deleted)>();
            w.entity_mut(e).insert(Visibility::Inherited);
        }
    }
    let orphans: Vec<_> = w
        .query::<(Entity, &PrefabMember)>()
        .iter(w)
        .filter(|(_, m)| w.get_entity(m.root).is_err())
        .map(|(e, _)| e)
        .collect();
    for e in orphans {
        w.entity_mut(e).despawn();
    }
    state.roots.retain(|e, _| w.get_entity(*e).is_ok());
    state.components.retain(|e, _| w.get_entity(*e).is_ok());
    w.insert_resource(state);
}
fn panel(ui: &mut egui::Ui, d: &mut crate::ui::UiData, _: &crate::ui_inspector::Filter) {
    use crate::mcp_commands::StudioCommand as Cmd;
    ui.heading("Prefab variants");
    let Some(root) = d.sel.primary else {
        ui.label("Select a prefab root.");
        return;
    };
    let Ok((inst, overrides)) = d.prefab_roots.get(root) else {
        ui.label("Select a prefab root in the outliner.");
        return;
    };
    ui.label(format!("Base: {}", inst.prefab_path));
    let mut patches: Patches =
        serde_json::from_str(overrides.map_or("{}", |o| o.json.as_str())).unwrap_or_default();
    let file = match load_resolved(&crate::props::assets_dir(), &inst.prefab_path) {
        Ok(f) => f,
        Err(e) => {
            ui.colored_label(crate::ui::AMBER, e);
            return;
        }
    };
    let target = Target::Entity(root);
    ui.weak("Overrides follow base updates. Reset restores the inherited value.");
    let name_id = ui.make_persistent_id("variant_name");
    let mut name: String = ui.data_mut(|m| m.get_temp(name_id)).unwrap_or_default();
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut name)
                .hint_text("new variant name")
                .desired_width(165.0),
        );
        if ui.button("Save variant").clicked() {
            d.queue.push(Cmd::SavePrefabVariant {
                target: target.clone(),
                path: name.clone(),
            });
        }
    });
    ui.data_mut(|m| m.insert_temp(name_id, name));
    for m in &file.entities {
        let patch = patches
            .remove(&m.id)
            .unwrap_or_else(|| serde_json::json!({}));
        ui.push_id(&m.id, |ui| {
            egui::CollapsingHeader::new(format!(
                "{} [{}]{}",
                m.name,
                m.id,
                if patch.as_object().is_some_and(|p| !p.is_empty()) {
                    " *"
                } else {
                    ""
                }
            ))
            .default_open(true)
            .show(ui, |ui| {
                let inherited = serde_json::to_value(m).unwrap();
                for field in ["name", "pos", "rot", "scale", "door", "light", "components"] {
                    if m.tile.is_some() && ["pos", "rot", "scale"].contains(&field) {
                        continue;
                    }
                    let changed = patch.get(field).is_some();
                    let id = ui.make_persistent_id((field, root));
                    let current = patch.get(field).unwrap_or(&inherited[field]).to_string();
                    let (mut value, previous): (String, String) = ui
                        .data_mut(|s| s.get_temp(id))
                        .unwrap_or_else(|| (current.clone(), current.clone()));
                    if previous != current {
                        value = current.clone();
                    }
                    ui.horizontal(|ui| {
                        ui.label(if changed {
                            format!("{field} *")
                        } else {
                            field.into()
                        });
                        if ui
                            .add_enabled(changed, egui::Button::new("Reset"))
                            .clicked()
                        {
                            d.queue.push(Cmd::SetPrefabOverride {
                                target: target.clone(),
                                member: m.id.clone(),
                                field: field.into(),
                                value: "null".into(),
                            });
                            value = inherited[field].to_string();
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut value).desired_width(235.0));
                        if ui.small_button("Apply").clicked() {
                            d.queue.push(Cmd::SetPrefabOverride {
                                target: target.clone(),
                                member: m.id.clone(),
                                field: field.into(),
                                value: value.clone(),
                            });
                        }
                    });
                    ui.data_mut(|s| s.insert_temp(id, (value, current)));
                }
            });
        });
    }
}
pub struct PrefabVariantsPlugin;
impl Plugin for PrefabVariantsPlugin {
    fn build(&self, app: &mut App) {
        use crate::editor_registry::{EditorAppExt, PanelDef, PanelLocation};
        app.register_scene_component::<PrefabSourceKey>()
            .register_scene_component::<PrefabOverrides>()
            .init_resource::<Applied>()
            .register_panel_def(
                PanelDef::new(
                    "prefab_variants",
                    "Prefabs",
                    PanelLocation::RightInspector,
                    panel,
                )
                .order(45),
            )
            .add_systems(Update, sync.after(crate::studio::process_history));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn dir(tag: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("arc_variant_{tag}_{}", std::process::id()));
        std::fs::create_dir_all(p.join("prefabs")).unwrap();
        p
    }
    fn base(root: &Path) {
        std::fs::write(root.join("prefabs/base.prefab.json"),r#"{"version":1,"name":"base","cell_size":2.0,"entities":[{"id":"a","parent":null,"kind":"boulder","name":"Rock","pos":[0,0,0],"rot":[0,0,0,1],"scale":[1,1,1]}]}"#).unwrap();
    }
    #[test]
    fn linked_tiles_are_root_owned_and_follow_delete_redo() {
        let root = dir("tiles");
        let mut w = World::new();
        w.init_resource::<Time>();
        w.init_resource::<crate::level_builder::GridSettings>();
        w.init_resource::<crate::props::NextObjectId>();
        w.init_resource::<crate::gizmo_3d::SceneHistory>();
        w.init_resource::<crate::selection::EditorSelection>();
        let file = prefabs::build_prefab(
            "tile",
            vec![prefabs::Member {
                parent: None,
                kind: "level".into(),
                name: "Floor".into(),
                tf: Transform::default(),
                tile: Some(crate::level_builder::LevelTile::default()),
                door: None,
                light: None,
            }],
            w.resource::<crate::level_builder::GridSettings>(),
        )
        .unwrap();
        std::fs::write(
            root.join("prefabs/tile.prefab.json"),
            serde_json::to_string(&file).unwrap(),
        )
        .unwrap();
        prefabs::spawn_prefab(&mut w, &root, "tile", Vec3::ZERO, 0.).unwrap();
        let instance = w
            .query_filtered::<Entity, With<PrefabInstance>>()
            .single(&w)
            .unwrap();
        let tile = w
            .query_filtered::<Entity, With<PrefabMember>>()
            .single(&w)
            .unwrap();
        assert!(
            w.get::<crate::props::UserProp>(tile).is_none(),
            "only the root is persisted"
        );
        w.entity_mut(instance).insert(crate::selection::Deleted);
        sync_at(&mut w, &root);
        assert!(w.get::<crate::selection::Deleted>(tile).is_some());
        w.entity_mut(instance).remove::<crate::selection::Deleted>();
        sync_at(&mut w, &root);
        assert!(w.get::<crate::selection::Deleted>(tile).is_none());
        assert_eq!(w.get::<Visibility>(tile), Some(&Visibility::Inherited));
        let _ = std::fs::remove_dir_all(root);
    }
    #[test]
    fn live_refresh_keeps_entity_ids_and_instance_overrides_isolated() {
        let root = dir("live");
        base(&root);
        let mut w = World::new();
        w.init_resource::<Time>();
        w.init_resource::<crate::level_builder::GridSettings>();
        let a = w
            .spawn((
                PrefabInstance {
                    prefab_path: "base".into(),
                    ..default()
                },
                PrefabOverrides {
                    json: r#"{"a":{"scale":[3,3,3]}}"#.into(),
                },
                crate::prefabs::PrefabHydrated,
                Transform::default(),
            ))
            .id();
        let b = w
            .spawn((
                PrefabInstance {
                    prefab_path: "base".into(),
                    ..default()
                },
                crate::prefabs::PrefabHydrated,
                Transform::default(),
            ))
            .id();
        let member_a = w
            .spawn((
                PrefabMember {
                    root: a,
                    key: "a".into(),
                },
                Transform::default(),
            ))
            .id();
        let member_b = w
            .spawn((
                PrefabMember {
                    root: b,
                    key: "a".into(),
                },
                Transform::default(),
            ))
            .id();
        sync_at(&mut w, &root);
        assert_eq!(w.get::<Transform>(member_a).unwrap().scale, Vec3::splat(3.));
        assert_eq!(w.get::<Transform>(member_b).unwrap().scale, Vec3::ONE);
        let mut file = load_resolved(&root, "base").unwrap();
        file.entities[0].pos = [4., 5., 6.];
        file.entities[0].scale = [2.; 3];
        std::fs::write(
            root.join("prefabs/base.prefab.json"),
            serde_json::to_string(&file).unwrap(),
        )
        .unwrap();
        w.resource_mut::<Applied>().timer = 1.;
        sync_at(&mut w, &root);
        assert_eq!(
            w.get::<Transform>(member_a).unwrap().translation,
            Vec3::new(4., 5., 6.)
        );
        assert_eq!(w.get::<Transform>(member_a).unwrap().scale, Vec3::splat(3.));
        assert_eq!(w.get::<Transform>(member_b).unwrap().scale, Vec3::splat(2.));
        w.entity_mut(a).insert(PrefabOverrides::default());
        sync_at(&mut w, &root);
        assert_eq!(w.get::<Transform>(member_a).unwrap().scale, Vec3::splat(2.));
        let _ = std::fs::remove_dir_all(root);
    }
    #[test]
    fn variants_inherit_nested_fields_and_follow_base_updates() {
        let root = dir("inherit");
        base(&root);
        std::fs::write(
            root.join("prefabs/blue.prefab.json"),
            r#"{"version":2,"name":"blue","base":"base","overrides":{"a":{"scale":[2,2,2]}}}"#,
        )
        .unwrap();
        std::fs::write(
            root.join("prefabs/large.prefab.json"),
            r#"{"version":2,"name":"large","base":"blue","overrides":{"a":{"name":"Large rock"}}}"#,
        )
        .unwrap();
        let first = load_resolved(&root, "large").unwrap();
        assert_eq!(first.entities[0].scale, [2.0; 3]);
        assert_eq!(first.entities[0].name, "Large rock");
        let mut b = load_resolved(&root, "base").unwrap();
        b.entities[0].pos = [3., 4., 5.];
        b.entities[0].scale = [9.; 3];
        std::fs::write(
            root.join("prefabs/base.prefab.json"),
            serde_json::to_string(&b).unwrap(),
        )
        .unwrap();
        let updated = load_resolved(&root, "large").unwrap();
        assert_eq!(updated.entities[0].pos, [3., 4., 5.]);
        assert_eq!(updated.entities[0].scale, [2.; 3]);
        let _ = std::fs::remove_dir_all(root);
    }
    #[test]
    fn cycles_paths_unknown_members_and_structural_overrides_fail() {
        let root = dir("bad");
        base(&root);
        for bad in ["../base", "prefabs/../base", "C:/base"] {
            assert!(load_resolved(&root, bad).is_err());
        }
        std::fs::write(
            root.join("prefabs/cycle.prefab.json"),
            r#"{"version":2,"name":"cycle","base":"cycle","overrides":{}}"#,
        )
        .unwrap();
        assert!(load_resolved(&root, "cycle").unwrap_err().contains("cycle"));
        let f = load_resolved(&root, "base").unwrap();
        for patch in [
            serde_json::json!({"missing":{"name":"bad"}}),
            serde_json::json!({"a":{"parent":0}}),
            serde_json::json!({"a":{"scale":[0,1,1]}}),
        ] {
            assert!(patch_file(f.clone(), &serde_json::from_value(patch).unwrap()).is_err());
        }
        let _ = std::fs::remove_dir_all(root);
    }
    #[test]
    fn override_reset_undo_redo_and_reflect_scene_roundtrip() {
        let root = dir("undo");
        base(&root);
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_resource::<crate::world_io::SceneComponentRegistry>()
            .init_resource::<operators::ComponentHistory>()
            .init_resource::<operators::CompClock>()
            .register_type::<crate::props::UserProp>()
            .register_type::<Transform>()
            .register_scene_component::<PrefabInstance>()
            .register_scene_component::<PrefabOverrides>();
        let w = app.world_mut();
        let e = w
            .spawn((
                PrefabInstance {
                    prefab_path: "prefabs/base.prefab.json".into(),
                    ..default()
                },
                PrefabOverrides::default(),
                Transform::default(),
                crate::props::UserProp {
                    kind: "prefab".into(),
                },
            ))
            .id();
        set_override_at(w, e, "a", "scale", "[2,3,4]", &root).unwrap();
        assert_eq!(
            effective(w, e, &root).unwrap().entities[0].scale,
            [2., 3., 4.]
        );
        set_override_at(w, e, "a", "scale", "null", &root).unwrap();
        assert_eq!(effective(w, e, &root).unwrap().entities[0].scale, [1.; 3]);
        let snap = w
            .resource_mut::<operators::ComponentHistory>()
            .undo()
            .unwrap();
        operators::apply_snap(w, &snap);
        assert_eq!(
            effective(w, e, &root).unwrap().entities[0].scale,
            [2., 3., 4.]
        );
        let registry = w.resource::<AppTypeRegistry>().clone();
        // The same reflected payload used by scene serialization retains both base and overrides.
        let reg = registry.read();
        let value = w.get::<PrefabOverrides>(e).unwrap();
        let json = serde_json::to_value(TypedReflectSerializer::new(value, &reg)).unwrap();
        let r = reg.get(std::any::TypeId::of::<PrefabOverrides>()).unwrap();
        let back = TypedReflectDeserializer::new(r, &reg)
            .deserialize(json)
            .unwrap();
        assert_eq!(
            PrefabOverrides::from_reflect(back.as_ref()).unwrap(),
            *value
        );
        let dw = crate::world_io::build_dynamic_world(w, std::iter::once(e));
        let json = crate::world_io::dynamic_world_to_json(&dw, &reg).unwrap();
        let mut server = w.resource::<AssetServer>().clone();
        let restored = crate::world_io::dynamic_world_from_json(&json, &reg, &mut server).unwrap();
        assert!(restored.entities[0].components.iter().any(|c| {
            PrefabOverrides::from_reflect(c.as_ref()).is_some_and(|p| p.json.contains("scale"))
        }));
        assert!(restored.entities[0].components.iter().any(|c| {
            PrefabInstance::from_reflect(c.as_ref())
                .is_some_and(|p| p.prefab_path == "prefabs/base.prefab.json")
        }));
        let (kind, _, _, extras) = crate::world_io::scene_object_parts(
            &restored.entities[0].components,
            w.resource::<crate::world_io::SceneComponentRegistry>(),
        )
        .unwrap();
        assert_eq!(kind, "prefab");
        assert!(extras.iter().any(|c| c.represents::<PrefabInstance>()));
        assert!(extras.iter().any(|c| c.represents::<PrefabOverrides>()));
        drop(reg);
        let snap = w
            .resource_mut::<operators::ComponentHistory>()
            .redo()
            .unwrap();
        operators::apply_snap(w, &snap);
        assert_eq!(effective(w, e, &root).unwrap().entities[0].scale, [1.; 3]);
        let _ = std::fs::remove_dir_all(root);
    }
    #[test]
    fn components_merge_by_field_and_stable_keys_survive_reorder() {
        let root = dir("keys");
        base(&root);
        let mut f = load_resolved(&root, "base").unwrap();
        f.entities[0].components.insert(
            "MaterialOverride".into(),
            serde_json::json!({"metallic":0.2,"roughness":0.8}),
        );
        let mut second = f.entities[0].clone();
        second.id = "b".into();
        f.entities.insert(0, second);
        let patches = serde_json::from_value(
            serde_json::json!({"a":{"components":{"MaterialOverride":{"metallic":0.9}}}}),
        )
        .unwrap();
        let f = patch_file(f, &patches).unwrap();
        assert_eq!(
            f.entities[1].components["MaterialOverride"]["roughness"],
            0.8
        );
        assert_eq!(
            f.entities[1].components["MaterialOverride"]["metallic"],
            0.9
        );
        assert_eq!(
            f.entities[0].components["MaterialOverride"]["metallic"],
            0.2
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
