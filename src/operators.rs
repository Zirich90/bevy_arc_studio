//! Operators: every `StudioCommand` variant is a named operator. The registry is derived from the enum's Reflect info
//! (no hand-written schema to drift), results are typed (`CommandResults`, readable via BRP) and component edits share one undo stack.

use std::any::TypeId;

use bevy::prelude::*;
use bevy::reflect::serde::{TypedReflectDeserializer, TypedReflectSerializer};
use bevy::reflect::TypeRegistry;
use bevy::reflect::enums::VariantInfo;
use serde::de::DeserializeSeed;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::arc_core::UndoHistory;
use crate::mcp_commands::StudioCommand;
use crate::props::{SceneObject, UserProp};
use crate::selection::{Deleted, EditorSelection, object_box};
use crate::studio::StudioState;

// ───────────────────────────── Typed results ─────────────────────────────

/// Outcome of one executed command. `data` is a JSON document for query-style operators, empty otherwise.
#[derive(Reflect, Serialize, Deserialize, Clone, Debug, Default)]
pub struct CommandResult {
    pub seq: u64,
    pub op: String,
    pub ok: bool,
    pub message: String,
    pub data: String,
}

#[derive(Resource, Reflect, Default, Serialize, Deserialize)]
#[reflect(Resource)]
pub struct CommandResults {
    pub items: Vec<CommandResult>,
    pub next_seq: u64,
}

/// Returned by an operator that finishes later (world-level closure) and reports its own result.
pub const DEFERRED: &str = "\u{0}deferred";

impl CommandResults {
    /// Record a result; returns the one-line status text. `Ok` strings starting with `{`/`[` are JSON payloads.
    pub fn push(&mut self, op: &str, res: Result<String, String>) -> String {
        let (ok, text) = match res {
            Ok(s) => (true, s),
            Err(e) => (false, e),
        };
        let (message, data) = if ok && (text.starts_with('{') || text.starts_with('[')) { ("ok".to_string(), text) } else { (text, String::new()) };
        let line = if ok { message.clone() } else { format!("ERR {message}") };
        self.next_seq += 1;
        self.items.push(CommandResult { seq: self.next_seq, op: op.to_string(), ok, message, data });
        let excess = self.items.len().saturating_sub(64);
        self.items.drain(..excess);
        line
    }
}

/// Operator name = variant name of the command.
pub fn op_name(c: &StudioCommand) -> String {
    let d = format!("{c:?}");
    d.split(|ch: char| !ch.is_alphanumeric()).next().unwrap_or_default().to_string()
}

/// Publish the result of a deferred (world-level) operator.
pub fn push_result(w: &mut World, op: &str, res: Result<String, String>) {
    let line = w.resource_mut::<CommandResults>().push(op, res);
    w.resource_mut::<StudioState>().status = line.clone();
    w.resource_mut::<crate::mcp_commands::StudioCommandQueue>().log.push(line);
}

// ───────────────────────────── Registry ─────────────────────────────

const UNDOABLE: &[&str] = &[
    "ApplyPreset", "SetQuality", "SetTime", "SpawnObject", "Sculpt", "DeleteSelected", "SetTransform", "Duplicate", "Paste", "EditMaterial", "ApplyMaterial", "SetSpec", "SetComponent",
    "AddComponent", "RemoveComponent", "Reparent", "ToggleDoor", "SetWaterBody", "BuildRoom", "BuildWallLine", "PlaceLevelTile", "PaintLevelTiles", "ToggleDoor", "SpawnPrefab",
];

pub fn is_undoable(op: &str) -> bool {
    UNDOABLE.contains(&op)
}

fn type_schema(short: &str) -> Value {
    if let Some(inner) = short.strip_prefix("Option<").and_then(|s| s.strip_suffix('>')) {
        return type_schema(inner);
    }
    if let Some(inner) = short.strip_prefix("Vec<").and_then(|s| s.strip_suffix('>')) {
        return json!({"type": "array", "items": type_schema(inner)});
    }
    match short {
        "f32" | "f64" => json!({"type": "number"}),
        "u8" | "u16" | "u32" | "u64" | "usize" | "i32" | "i64" => json!({"type": "integer"}),
        "bool" => json!({"type": "boolean"}),
        "String" => json!({"type": "string"}),
        "Vec3" => json!({"type": "array", "items": {"type": "number"}, "minItems": 3, "maxItems": 3}),
        other => json!({"type": "reflect", "type_path": other}),
    }
}

/// JSON schema of every operator, built from `StudioCommand`'s reflect info.
pub fn describe_operators(reg: &TypeRegistry) -> Result<String, String> {
    let info = reg.get(TypeId::of::<StudioCommand>()).ok_or("StudioCommand not registered")?.type_info();
    let en = info.as_enum().map_err(|e| e.to_string())?;
    let ops: Vec<Value> = en
        .iter()
        .map(|v| {
            let (mut props, mut required) = (serde_json::Map::new(), Vec::new());
            let kind = match v {
                VariantInfo::Struct(s) => {
                    for f in s.iter() {
                        let short = f.type_path_table().short_path();
                        if !short.starts_with("Option<") {
                            required.push(f.name().to_string());
                        }
                        props.insert(f.name().to_string(), type_schema(short));
                    }
                    "struct"
                }
                VariantInfo::Tuple(t) => {
                    for (i, f) in t.iter().enumerate() {
                        required.push(i.to_string());
                        props.insert(i.to_string(), type_schema(f.type_path_table().short_path()));
                    }
                    "tuple"
                }
                VariantInfo::Unit(_) => "unit",
            };
            json!({
                "name": v.name(),
                "undoable": is_undoable(v.name()),
                "kind": kind,
                "params": {"type": "object", "properties": props, "required": required},
            })
        })
        .collect();
    Ok(json!({"operators": ops}).to_string())
}

// ───────────────────────────── Addressing ─────────────────────────────

/// How a command names an object: `Id` is `SceneObject.id` (changes when terrain is rebuilt); `Name` the newest object with that
/// name; `Path` a `/`-separated hierarchy path from `DescribeScene` (`Camp/Tent#1`, `#k` picks among equal siblings).
#[derive(Reflect, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum Target {
    Id(u32),
    Name(String),
    Path(String),
    Entity(Entity),
}

fn label(w: &World, e: Entity) -> Option<String> {
    let er = w.get_entity(e).ok()?;
    er.get::<Name>().map(|n| n.as_str().to_string()).or_else(|| er.get::<SceneObject>().map(|s| s.name.clone()))
}

fn sort_key(w: &World, e: Entity) -> (u32, u64) {
    (w.get::<SceneObject>(e).map(|s| s.id).unwrap_or(u32::MAX), e.to_bits())
}

fn live(w: &World, e: Entity) -> bool {
    w.get_entity(e).is_ok() && w.get::<Deleted>(e).is_none()
}

fn children_of(w: &mut World, parent: Option<Entity>) -> Vec<Entity> {
    let all: Vec<Entity> = match parent {
        Some(p) => w.get::<Children>(p).map(|c| c.to_vec()).unwrap_or_default(),
        None => w.query_filtered::<Entity, Without<ChildOf>>().iter(w).collect(),
    };
    all.into_iter().filter(|e| live(w, *e) && label(w, *e).is_some()).collect()
}

fn same_label_siblings(w: &mut World, parent: Option<Entity>, lab: &str) -> Vec<Entity> {
    let mut v: Vec<Entity> = children_of(w, parent).into_iter().filter(|e| label(w, *e).as_deref() == Some(lab)).collect();
    v.sort_by_key(|e| sort_key(w, *e));
    v
}

pub fn path_of(w: &mut World, e: Entity) -> String {
    let (mut segs, mut cur) = (Vec::new(), e);
    loop {
        let lab = label(w, cur).unwrap_or_else(|| format!("entity{}", cur.to_bits()));
        let parent = w.get::<ChildOf>(cur).map(|c| c.parent());
        let sibs = same_label_siblings(w, parent, &lab);
        if sibs.len() > 1 {
            segs.push(format!("{lab}#{}", sibs.iter().position(|s| *s == cur).unwrap_or(0)));
        } else {
            segs.push(lab);
        }
        match parent {
            Some(p) => cur = p,
            None => break,
        }
    }
    segs.reverse();
    segs.join("/")
}

pub fn resolve(w: &mut World, t: &Target) -> Result<Entity, String> {
    match t {
        Target::Entity(e) => live(w, *e).then_some(*e).ok_or_else(|| format!("entity {e:?} is gone")),
        Target::Id(id) => {
            let mut q = w.query_filtered::<(Entity, &SceneObject), Without<Deleted>>();
            q.iter(w).find(|(_, so)| so.id == *id).map(|(e, _)| e).ok_or(format!("no object id {id}"))
        }
        Target::Name(n) => {
            let mut q = w.query_filtered::<(Entity, &SceneObject), Without<Deleted>>();
            let by_obj = q.iter(w).filter(|(_, so)| &so.name == n).max_by_key(|(_, so)| so.id).map(|(e, _)| e);
            if let Some(e) = by_obj {
                return Ok(e);
            }
            let mut q = w.query_filtered::<(Entity, &Name), Without<Deleted>>();
            q.iter(w).filter(|(_, nm)| nm.as_str() == n).map(|(e, _)| e).max_by_key(|e| e.to_bits()).ok_or(format!("no object named '{n}'"))
        }
        Target::Path(p) => {
            let mut cur: Option<Entity> = None;
            for seg in p.split('/').filter(|s| !s.is_empty()) {
                let (name, k) = match seg.rsplit_once('#') {
                    Some((n, k)) if k.parse::<usize>().is_ok() => (n, k.parse::<usize>().unwrap_or(0)),
                    _ => (seg, 0),
                };
                let c = same_label_siblings(w, cur, name);
                cur = Some(*c.get(k).ok_or(format!("path segment '{seg}' not found"))?);
            }
            cur.ok_or_else(|| "empty path".to_string())
        }
    }
}

// ───────────────────────────── Component history ─────────────────────────────

/// One component of one entity; `value: None` means the component is absent.
pub type ResourceApply = fn(&mut World, &dyn PartialReflect);

pub struct CompSnap {
    pub entity: Entity,
    pub type_id: TypeId,
    pub value: Option<Box<dyn Reflect>>,
    /// Set for a resource snapshot (`entity` is then a placeholder): writes `value` back into the world.
    pub resource: Option<ResourceApply>,
    /// Components restored together with this one in a single undo step (e.g. `ChildOf` + `Transform` of a reparent).
    pub also: Vec<CompSnap>,
}

impl Clone for CompSnap {
    fn clone(&self) -> Self {
        Self { entity: self.entity, type_id: self.type_id, value: self.value.as_ref().and_then(|v| v.reflect_clone().ok()), resource: self.resource, also: self.also.clone() }
    }
}

impl PartialEq for CompSnap {
    fn eq(&self, o: &Self) -> bool {
        self.entity == o.entity
            && self.type_id == o.type_id
            && self.resource.is_some() == o.resource.is_some()
            && self.also == o.also
            && match (&self.value, &o.value) {
                (None, None) => true,
                (Some(a), Some(b)) => a.reflect_partial_eq(b.as_partial_reflect()).unwrap_or(false),
                _ => false,
            }
    }
}

pub type ComponentHistory = UndoHistory<CompSnap>;

/// Inspector drags produce one edit per frame; edits to the same component within `COALESCE_SECS` form one undo entry.
#[derive(Resource, Default)]
pub struct CompClock {
    pub last_edit: f64,
    /// Undo/redo request held back one frame while an open edit is committed first.
    pub replay: (bool, bool),
}

pub const COALESCE_SECS: f64 = 0.4;

pub fn snap(w: &World, e: Entity, tid: TypeId) -> CompSnap {
    let reg = w.resource::<AppTypeRegistry>().read();
    let value = reg
        .get_type_data::<ReflectComponent>(tid)
        .and_then(|rc| w.get_entity(e).ok().and_then(|er| rc.reflect(er)))
        .and_then(|r| r.reflect_clone().ok());
    CompSnap { entity: e, type_id: tid, value, resource: None, also: Vec::new() }
}

/// Snapshot of a reflected resource (`#[reflect(Resource)]`), for undoable resource edits.
pub fn snap_resource<R: Resource + Reflect + FromReflect>(w: &World) -> CompSnap {
    let value = w.get_resource::<R>().map(|r| r.reflect_clone().ok()).flatten();
    let apply: ResourceApply = |w, v| {
        if let Some(r) = R::from_reflect(v) {
            w.insert_resource(r);
        }
    };
    CompSnap { entity: Entity::PLACEHOLDER, type_id: TypeId::of::<R>(), value, resource: Some(apply), also: Vec::new() }
}

pub fn apply_snap(w: &mut World, s: &CompSnap) {
    apply_one(w, s);
    for a in &s.also {
        apply_one(w, a);
    }
}

fn apply_one(w: &mut World, s: &CompSnap) {
    let registry = w.resource::<AppTypeRegistry>().clone();
    let reg = registry.read();
    if let Some(apply) = s.resource {
        if let Some(v) = &s.value {
            apply(w, v.as_partial_reflect());
        }
        return;
    }
    let Some(rc) = reg.get_type_data::<ReflectComponent>(s.type_id) else { return };
    let Ok(mut em) = w.get_entity_mut(s.entity) else { return };
    match &s.value {
        Some(v) => rc.insert(&mut em, v.as_partial_reflect(), &reg),
        None => {
            rc.remove(&mut em);
        }
    }
}

/// Commit the open (coalescing) edit, if any.
pub fn flush_comp_history(h: &mut ComponentHistory) {
    if let Some(p) = h.preview().cloned() {
        h.commit(p);
    }
}

/// Record a component edit. `coalesce` merges consecutive edits of the same component (inspector drags).
pub fn record_edit(w: &mut World, label: &str, before: CompSnap, after: CompSnap, coalesce: bool) {
    let now = w.resource::<Time>().elapsed_secs_f64();
    let mut h = w.resource_mut::<ComponentHistory>();
    if coalesce {
        let same = h.preview().is_some_and(|p| p.entity == before.entity && p.type_id == before.type_id);
        if !same {
            flush_comp_history(&mut h);
            h.begin(label, before);
        }
        h.update_preview(after);
        w.resource_mut::<CompClock>().last_edit = now;
    } else {
        flush_comp_history(&mut h);
        h.record(label, before, after);
    }
}

// ───────────────────────────── Component operators ─────────────────────────────

fn find_component(reg: &TypeRegistry, name: &str) -> Result<TypeId, String> {
    reg.get_with_type_path(name)
        .or_else(|| reg.get_with_short_type_path(name))
        .filter(|r| r.data::<ReflectComponent>().is_some())
        .map(|r| r.type_id())
        .ok_or_else(|| format!("unknown or ambiguous component '{name}' (use the full type path)"))
}

fn merge(base: &mut Value, patch: Value) {
    match (base, patch) {
        (Value::Object(b), Value::Object(p)) => {
            for (k, v) in p {
                merge(b.entry(k).or_insert(Value::Null), v);
            }
        }
        (b, p) => *b = p,
    }
}

fn parse_json(s: &str) -> Result<Value, String> {
    serde_json::from_str(s).map_err(|e| format!("value is not JSON: {e}"))
}

/// Patch fields of an existing component: `value` is JSON in reflect-serde form, merged over the current value.
pub fn set_component(w: &mut World, target: &Target, component: &str, value: &str) -> Result<String, String> {
    let e = resolve(w, target)?;
    let registry = w.resource::<AppTypeRegistry>().clone();
    let reg = registry.read();
    let tid = find_component(&reg, component)?;
    let before = snap(w, e, tid);
    let cur = before.value.as_ref().ok_or(format!("entity has no {component}"))?;
    let mut doc = serde_json::to_value(TypedReflectSerializer::new(cur.as_partial_reflect(), &reg)).map_err(|e| e.to_string())?;
    merge(&mut doc, parse_json(value)?);
    let json = doc.to_string();
    let reg_entry = reg.get(tid).ok_or("type vanished")?;
    let new = TypedReflectDeserializer::new(reg_entry, &reg).deserialize(&mut serde_json::Deserializer::from_str(&json)).map_err(|e| e.to_string())?;
    let rc = reg.get_type_data::<ReflectComponent>(tid).ok_or("not a component")?;
    rc.insert(&mut w.get_entity_mut(e).map_err(|e| e.to_string())?, new.as_ref(), &reg);
    drop(reg);
    let after = snap(w, e, tid);
    record_edit(w, "Set component", before, after, false);
    Ok(format!("set {component}"))
}

pub fn add_component(w: &mut World, target: &Target, component: &str, value: Option<&str>) -> Result<String, String> {
    let e = resolve(w, target)?;
    let registry = w.resource::<AppTypeRegistry>().clone();
    let reg = registry.read();
    let tid = find_component(&reg, component)?;
    let before = snap(w, e, tid);
    let rc = reg.get_type_data::<ReflectComponent>(tid).ok_or("not a component")?;
    let new: Box<dyn PartialReflect> = match value {
        Some(json) => {
            let reg_entry = reg.get(tid).ok_or("type vanished")?;
            TypedReflectDeserializer::new(reg_entry, &reg).deserialize(&mut serde_json::Deserializer::from_str(json)).map_err(|e| e.to_string())?
        }
        None => {
            if let Some(d) = reg.get_type_data::<ReflectDefault>(tid) {
                d.default().into_partial_reflect()
            } else if let Some(f) = reg.get_type_data::<ReflectFromWorld>(tid) {
                f.from_world(w).into_partial_reflect()
            } else {
                return Err(format!("{component} has no Default; pass `value`"));
            }
        }
    };
    rc.insert(&mut w.get_entity_mut(e).map_err(|e| e.to_string())?, new.as_ref(), &reg);
    drop(reg);
    let after = snap(w, e, tid);
    record_edit(w, "Add component", before, after, false);
    Ok(format!("added {component}"))
}

pub fn remove_component(w: &mut World, target: &Target, component: &str) -> Result<String, String> {
    let e = resolve(w, target)?;
    let registry = w.resource::<AppTypeRegistry>().clone();
    let reg = registry.read();
    let tid = find_component(&reg, component)?;
    let before = snap(w, e, tid);
    if before.value.is_none() {
        return Err(format!("entity has no {component}"));
    }
    let rc = reg.get_type_data::<ReflectComponent>(tid).ok_or("not a component")?;
    rc.remove(&mut w.get_entity_mut(e).map_err(|e| e.to_string())?);
    drop(reg);
    let after = snap(w, e, tid);
    record_edit(w, "Remove component", before, after, false);
    Ok(format!("removed {component}"))
}

fn hierarchy_snap(w: &World, e: Entity) -> CompSnap {
    let mut s = snap(w, e, TypeId::of::<ChildOf>());
    s.also.push(snap(w, e, TypeId::of::<Transform>()));
    s
}

/// Move `child` under `parent` (or to the root), keeping its world transform. One undo step restores parent and local transform.
pub fn reparent(w: &mut World, child: &Target, parent: Option<&Target>) -> Result<String, String> {
    let c = resolve(w, child)?;
    let before = hierarchy_snap(w, c);
    let p = parent.map(|t| resolve(w, t)).transpose()?;
    if let Some(p) = p {
        let mut anc = Some(p);
        while let Some(a) = anc {
            if a == c {
                return Err("cannot parent an object under its own descendant".into());
            }
            anc = w.get::<ChildOf>(a).map(|x| x.parent());
        }
    }
    let world_m = w.get::<GlobalTransform>(c).map(|g| g.to_matrix());
    let mut em = w.get_entity_mut(c).map_err(|e| e.to_string())?;
    match p {
        Some(p) => {
            em.insert(ChildOf(p));
        }
        None => {
            em.remove::<ChildOf>();
        }
    }
    if let Some(m) = world_m {
        let local = match p.and_then(|p| w.get::<GlobalTransform>(p).map(|g| g.to_matrix())) {
            Some(pm) => pm.inverse() * m,
            None => m,
        };
        w.entity_mut(c).insert(Transform::from_matrix(local));
    }
    let after = hierarchy_snap(w, c);
    record_edit(w, "Reparent", before, after, false);
    let path = path_of(w, c);
    Ok(format!("reparented -> {path}"))
}

// ───────────────────────────── DescribeScene ─────────────────────────────

pub fn describe_scene(w: &mut World, filter: Option<&str>, limit: usize) -> Result<String, String> {
    let registry = w.resource::<AppTypeRegistry>().clone();
    let (sel_all, sel_primary) = {
        let s = w.resource::<EditorSelection>();
        (s.selected.iter().copied().collect::<Vec<_>>(), s.primary)
    };
    let mut q = w.query_filtered::<(Entity, &Transform, &SceneObject, Option<&UserProp>), Without<Deleted>>();
    let mut rows: Vec<(Entity, Transform, SceneObject, Option<String>)> = q.iter(w).map(|(e, t, s, u)| (e, *t, s.clone(), u.map(|u| u.kind.clone()))).collect();
    rows.sort_by_key(|r| r.2.id);
    let total = rows.len();
    let mut counts = std::collections::BTreeMap::<String, usize>::new();
    for r in &rows {
        *counts.entry(r.2.name.clone()).or_default() += 1;
    }
    if let Some(f) = filter {
        let f = f.to_lowercase();
        rows.retain(|r| r.2.name.to_lowercase().contains(&f));
    }
    let matched = rows.len();
    rows.truncate(limit.min(1000));
    let mut entities = Vec::with_capacity(rows.len());
    for (e, tf, so, kind) in rows {
        let (center, half, _) = object_box(&tf, &so, None);
        let comps: Vec<String> = {
            let reg = registry.read();
            let mut v: Vec<String> = w
                .get_entity(e)
                .map(|er| {
                    er.archetype()
                        .iter_components()
                        .filter_map(|c| w.components().get_info(c).and_then(|i| i.type_id()))
                        .filter(|t| reg.get_type_data::<ReflectComponent>(*t).is_some())
                        .filter_map(|t| reg.get(t).map(|r| r.type_info().type_path_table().short_path().to_string()))
                        .collect()
                })
                .unwrap_or_default();
            v.sort();
            v
        };
        let parent = w.get::<ChildOf>(e).map(|c| c.parent());
        let children: Vec<Entity> = w.get::<Children>(e).map(|c| c.to_vec()).unwrap_or_default();
        entities.push(json!({
            "entity": e.to_bits(),
            "id": so.id,
            "name": so.name,
            "path": path_of(w, e),
            "parent": parent.map(|p| path_of(w, p)),
            "children": children.iter().filter_map(|c| label(w, *c)).collect::<Vec<_>>(),
            "kind": kind,
            "pos": tf.translation.to_array(),
            "bounds": {"center": center.to_array(), "half": half.to_array()},
            "components": comps,
        }));
    }
    let live_sel: Vec<Entity> = sel_all.into_iter().filter(|e| live(w, *e)).collect();
    let selected: Vec<Value> = live_sel.into_iter().map(|e| Value::String(path_of(w, e))).collect();
    let primary = sel_primary.filter(|e| live(w, *e)).map(|e| path_of(w, e));
    Ok(json!({"total": total, "matched": matched, "counts": counts, "selected": selected, "primary": primary, "entities": entities}).to_string())
}

pub struct OperatorsPlugin;

impl Plugin for OperatorsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CommandResults>()
            .init_resource::<ComponentHistory>()
            .init_resource::<CompClock>()
            .register_type::<CommandResults>()
            .register_type::<CommandResult>()
            .register_type::<Target>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Component, Reflect, Default, Clone, PartialEq, Debug)]
    #[reflect(Component, Default)]
    struct OpProbe {
        hp: i32,
        speed: f32,
    }

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).init_resource::<ComponentHistory>().init_resource::<CompClock>().register_type::<OpProbe>();
        app
    }

    #[test]
    fn results_split_json_payload_and_errors() {
        let mut r = CommandResults::default();
        assert_eq!(r.push("A", Ok("done".into())), "done");
        assert_eq!(r.push("B", Ok("[1,2]".into())), "ok");
        assert_eq!(r.push("C", Err("boom".into())), "ERR boom");
        assert_eq!(r.items[1].data, "[1,2]");
        assert!(!r.items[2].ok && r.items[2].seq == 3);
    }

    #[test]
    fn op_name_is_variant_name() {
        assert_eq!(op_name(&StudioCommand::Undo), "Undo");
        assert_eq!(op_name(&StudioCommand::SetTime(1.0)), "SetTime");
        assert_eq!(op_name(&StudioCommand::SelectIds { ids: vec![], additive: false }), "SelectIds");
    }

    #[test]
    fn component_ops_roundtrip_with_undo() {
        let mut app = app();
        let w = app.world_mut();
        let e = w.spawn(Name::new("Hero")).id();
        let t = Target::Name("Hero".into());
        add_component(w, &t, "OpProbe", Some(r#"{"hp":5,"speed":1.5}"#)).unwrap();
        assert_eq!(w.get::<OpProbe>(e).unwrap().hp, 5);
        set_component(w, &t, "OpProbe", r#"{"hp":9}"#).unwrap();
        let p = w.get::<OpProbe>(e).unwrap();
        assert_eq!((p.hp, p.speed), (9, 1.5));
        // undo set -> hp 5, undo add -> component gone
        let s = w.resource_mut::<ComponentHistory>().undo().unwrap();
        apply_snap(w, &s);
        assert_eq!(w.get::<OpProbe>(e).unwrap().hp, 5);
        let s = w.resource_mut::<ComponentHistory>().undo().unwrap();
        apply_snap(w, &s);
        assert!(w.get::<OpProbe>(e).is_none());
        let s = w.resource_mut::<ComponentHistory>().redo().unwrap();
        apply_snap(w, &s);
        assert_eq!(w.get::<OpProbe>(e).unwrap().hp, 5);
        remove_component(w, &t, "OpProbe").unwrap();
        assert!(w.get::<OpProbe>(e).is_none());
    }

    #[test]
    fn paths_resolve_and_reparent_keeps_world_pos() {
        let mut app = app();
        let w = app.world_mut();
        let root = w.spawn((Name::new("Camp"), Transform::from_xyz(10.0, 0.0, 0.0), GlobalTransform::from(Transform::from_xyz(10.0, 0.0, 0.0)))).id();
        let a = w.spawn((Name::new("Tent"), Transform::from_xyz(1.0, 0.0, 0.0), GlobalTransform::from(Transform::from_xyz(1.0, 0.0, 0.0)))).id();
        let b = w.spawn(Name::new("Tent")).id();
        reparent(w, &Target::Entity(a), Some(&Target::Entity(root))).unwrap();
        reparent(w, &Target::Entity(b), Some(&Target::Entity(root))).unwrap();
        assert!((w.get::<Transform>(a).unwrap().translation.x - (-9.0)).abs() < 1e-4);
        let pa = path_of(w, a);
        assert!(pa.starts_with("Camp/Tent#"), "{pa}");
        assert_eq!(resolve(w, &Target::Path(pa)).unwrap(), a);
        assert!(reparent(w, &Target::Entity(root), Some(&Target::Entity(a))).is_err());
    }

    #[test]
    fn reparent_undo_restores_parent_and_local_transform() {
        let mut app = app();
        app.register_type::<ChildOf>().register_type::<Transform>();
        let w = app.world_mut();
        let tf = Transform::from_xyz(10.0, 0.0, 0.0);
        let root = w.spawn((Name::new("Camp"), tf, GlobalTransform::from(tf))).id();
        let tf = Transform::from_xyz(1.0, 0.0, 0.0);
        let a = w.spawn((Name::new("Tent"), tf, GlobalTransform::from(tf))).id();
        reparent(w, &Target::Entity(a), Some(&Target::Entity(root))).unwrap();
        assert_eq!(w.get::<ChildOf>(a).map(|c| c.parent()), Some(root));
        assert!((w.get::<Transform>(a).unwrap().translation.x + 9.0).abs() < 1e-4);
        let s = w.resource_mut::<ComponentHistory>().undo().unwrap();
        apply_snap(w, &s);
        assert!(w.get::<ChildOf>(a).is_none());
        assert!((w.get::<Transform>(a).unwrap().translation.x - 1.0).abs() < 1e-4);
        let s = w.resource_mut::<ComponentHistory>().redo().unwrap();
        apply_snap(w, &s);
        assert_eq!(w.get::<ChildOf>(a).map(|c| c.parent()), Some(root));
    }
}
