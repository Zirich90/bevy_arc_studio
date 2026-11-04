//! Persisted environment intent and reversible ownership of upstream lighting.
use bevy::{prelude::*, camera::{Exposure, Hdr}, core_pipeline::{Skybox, tonemapping::{Tonemapping, DebandDither}}, light::{EnvironmentMapLight, GeneratedEnvironmentMapLight, GlobalAmbientLight, AtmosphereEnvironmentMapLight, VolumetricFog}, pbr::{DistanceFog, AtmosphereSettings}, render::view::ColorGrading, ecs::system::RunSystemOnce};
use quillshade::light::{self, EnvironmentValues, BakedLightManifest, LightRuntime, skybox::BlendedSkybox};
use std::sync::Arc;

#[derive(Resource, Reflect, Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[reflect(Resource, Default, Clone)]
pub struct Environment {
    pub preset: Option<String>,
    pub duration: f32,
    pub elapsed: f32,
    /// Serialized upstream value type avoids imposing Reflect on generated types.
    pub from_json: String,
}

impl Environment {
    pub fn validate(&self) -> Result<(), String> {
        if !self.duration.is_finite() || !self.elapsed.is_finite() || !(0.0..=86400.0).contains(&self.duration) || !(0.0..=self.duration).contains(&self.elapsed) {
            return Err("invalid environment transition clock".into());
        }
        if self.preset.as_ref().is_some_and(|p| light::Preset::from_path(p).is_none()) { return Err("unknown Quillshade environment".into()); }
        if !self.from_json.is_empty() {
            if self.from_json.len() > 1_000_000 { return Err("environment snapshot exceeds limit".into()); }
            serde_json::from_str::<EnvironmentValues>(&self.from_json).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

type Restore = Arc<dyn Fn(&mut World) + Send + Sync>;
#[derive(Resource, Default)]
struct Ownership { restore: Vec<Restore>, cameras: Vec<Entity>, applied: Option<Environment> }

fn component<T: Component + Clone>(w: &World, e: Entity) -> Restore {
    let value = w.get::<T>(e).cloned();
    Arc::new(move |w| {
        if let Ok(mut entity) = w.get_entity_mut(e) {
            if let Some(value) = &value { entity.insert(value.clone()); } else { entity.remove::<T>(); }
        }
    })
}
fn resource<T: Resource + Clone>(w: &World) -> Restore {
    let value = w.get_resource::<T>().cloned();
    Arc::new(move |w| { if let Some(v) = &value { w.insert_resource(v.clone()); } else { w.remove_resource::<T>(); } })
}
fn capture_camera(w: &mut World, e: Entity, owner: &mut Ownership) {
    macro_rules! keep { ($($t:ty),*) => { $(owner.restore.push(component::<$t>(w,e));)* }; }
    keep!(Hdr, Exposure, Tonemapping, DebandDither, ColorGrading, BlendedSkybox, EnvironmentMapLight, GeneratedEnvironmentMapLight, DistanceFog, Skybox, AtmosphereSettings, AtmosphereEnvironmentMapLight, VolumetricFog);
    w.entity_mut(e).remove::<(Skybox, AtmosphereSettings, AtmosphereEnvironmentMapLight, VolumetricFog)>();
    owner.cameras.push(e);
}

pub fn set(w: &mut World, preset: Option<String>, duration: f32) -> Result<String, String> {
    if !duration.is_finite() || !(0.0..=86400.0).contains(&duration) { return Err("invalid transition duration".into()); }
    if preset.as_ref().is_some_and(|p| light::Preset::from_path(p).is_none()) { return Err("unknown Quillshade environment".into()); }
    let from_json = if duration > 0.0 && preset.is_some() {
        let values = w.run_system_once(|light: light::Light| light.current_values().cloned()).map_err(|e| e.to_string())?;
        values.map(|v| serde_json::to_string(&v).unwrap()).unwrap_or_default()
    } else { String::new() };
    let before = crate::operators::snap_resource::<Environment>(w);
    w.insert_resource(Environment { preset, duration, elapsed: 0.0, from_json });
    let after = crate::operators::snap_resource::<Environment>(w);
    crate::operators::record_edit(w, "Environment", before, after, false);
    Ok("environment updated".into())
}

fn reconcile(w: &mut World) {
    let desired = w.resource::<Environment>().clone();
    if desired.validate().is_err() { return; }
    let mut owner = w.remove_resource::<Ownership>().unwrap_or_default();
    if desired.preset.is_none() {
        if owner.applied.is_some() {
            let lights: Vec<_> = w.query_filtered::<Entity, With<light::ManagedDirectionalLight>>().iter(w).collect();
            for e in lights { w.despawn(e); }
            for restore in owner.restore.drain(..) { restore(w); }
            owner.cameras.clear(); owner.applied = None;
            w.resource_mut::<crate::quillshade_bridge::QuillshadeEnvironment>().0 = false;
        }
        w.insert_resource(owner); return;
    }
    let Some(preset) = desired.preset.as_deref().and_then(light::Preset::from_path) else { w.insert_resource(owner); return; };
    if owner.applied.is_none() {
        owner.restore.push(resource::<ClearColor>(w));
        owner.restore.push(resource::<GlobalAmbientLight>(w));
        owner.restore.push(resource::<LightRuntime>(w));
        if let Some(state) = w.get_resource::<State<light::State>>().map(|s| *s.get()) {
            owner.restore.push(Arc::new(move |w| { w.insert_resource(State::new(state)); }));
        }
        owner.restore.push(resource::<NextState<light::State>>(w));
        let lights: Vec<_> = w.query_filtered::<Entity, Or<(With<crate::lighting::Sun>, With<crate::lighting::Moon>)>>().iter(w).collect();
        for e in lights {
            owner.restore.push(component::<DirectionalLight>(w,e));
            owner.restore.push(component::<Transform>(w,e));
            if let Some(mut l) = w.get_mut::<DirectionalLight>(e) { l.illuminance = 0.0; }
        }
    }
    let cameras: Vec<_> = w.query_filtered::<Entity, With<Camera3d>>().iter(w).filter(|e| !owner.cameras.contains(e)).collect();
    for e in cameras { capture_camera(w,e,&mut owner); }
    w.resource_mut::<crate::quillshade_bridge::QuillshadeEnvironment>().0 = true;
    let changed = owner.applied.as_ref() != Some(&desired);
    let mut next = desired.clone();
    let paused = w.get_resource::<crate::studio::StudioState>().is_some_and(|s| s.mode == crate::studio::StudioMode::Paused);
    if !paused { next.elapsed = (next.elapsed + w.resource::<Time>().delta_secs()).min(next.duration); }
    let transitioning = !next.from_json.is_empty() && next.elapsed < next.duration;
    if changed || transitioning || owner.applied.as_ref().is_some_and(|a| a.elapsed < a.duration && next.elapsed >= next.duration) {
        let mut manifest = w.get_resource::<Assets<BakedLightManifest>>().and_then(|a| a.iter().next().map(|(_,m)| m.clone())).unwrap_or_else(|| BakedLightManifest::parse(light::COMPILED_MANIFEST).unwrap());
        if let Some(target) = manifest.environments.get(desired.preset.as_ref().unwrap()).cloned() {
            if transitioning {
                if let Ok(from) = serde_json::from_str::<EnvironmentValues>(&next.from_json) {
                    let t = next.elapsed / next.duration;
                    manifest.environments.insert(desired.preset.clone().unwrap(), EnvironmentValues::interpolate(&from,&target,t,t));
                }
            }
            let server = w.get_resource::<AssetServer>().cloned();
            w.resource_mut::<LightRuntime>().apply_manifest_data(&manifest, server.as_ref());
            if changed { let _ = w.run_system_once(move |mut light: light::Light| light.set_immediate(preset)); }
        }
    }
    owner.applied = Some(next.clone());
    if next != desired { w.insert_resource(next); }
    w.insert_resource(owner);
}

pub fn describe(w: &mut World) -> serde_json::Value {
    let managed = w.query_filtered::<Entity, With<light::ManagedDirectionalLight>>().iter(w).count();
    let cameras: Vec<_> = w.query_filtered::<Entity, With<Camera3d>>().iter(w).collect();
    let cameras: Vec<_> = cameras.into_iter().map(|e| serde_json::json!({
        "entity": e.to_bits(), "hdr": w.get::<Hdr>(e).is_some(),
        "exposure": w.get::<Exposure>(e).map(|v| v.ev100),
        "skybox": w.get::<Skybox>(e).is_some(), "blended_skybox": w.get::<BlendedSkybox>(e).is_some(),
        "environment_map": w.get::<EnvironmentMapLight>(e).is_some(),
        "generated_environment_map": w.get::<GeneratedEnvironmentMapLight>(e).is_some(),
        "atmosphere_settings": w.get::<AtmosphereSettings>(e).is_some(),
        "atmosphere_ibl": w.get::<AtmosphereEnvironmentMapLight>(e).is_some(),
        "volumetric_fog": w.get::<VolumetricFog>(e).is_some(),
    })).collect();
    let values = w.run_system_once(|light: light::Light| light.current_values().cloned()).ok().flatten();
    serde_json::json!({"managed_lights":managed,"cameras":cameras,"values":values})
}

pub struct EnvironmentPlugin;
impl Plugin for EnvironmentPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Environment>().init_resource::<Ownership>().register_type::<Environment>().add_systems(First, reconcile);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn world() -> World {
        let mut w = World::new();
        w.init_resource::<Environment>(); w.init_resource::<Ownership>(); w.init_resource::<Time>();
        w.init_resource::<crate::quillshade_bridge::QuillshadeEnvironment>();
        w.init_resource::<LightRuntime>(); w.init_resource::<NextState<light::State>>();
        w.init_resource::<Messages<light::LightTransitionStarted>>(); w.init_resource::<Messages<light::LightTransitionFinished>>();
        w.insert_resource(State::new(light::State::None));
        w.insert_resource(ClearColor(Color::srgb(0.1,0.2,0.3)));
        w.insert_resource(GlobalAmbientLight::default()); w
    }
    #[test]
    fn release_restores_camera_globals_and_removes_owned_lights() {
        let mut w = world();
        let camera = w.spawn((Camera3d::default(), Exposure { ev100: 8.5 }, Tonemapping::None, AtmosphereSettings::default())).id();
        let sun = w.spawn((crate::lighting::Sun, DirectionalLight { illuminance: 1234.0, ..default() }, Transform::default())).id();
        let baseline = w.resource::<ClearColor>().0;
        let had_hdr = w.get::<Hdr>(camera).is_some();
        w.resource_mut::<Environment>().preset = Some("day".into()); reconcile(&mut w);
        assert_eq!(w.get::<DirectionalLight>(sun).unwrap().illuminance,0.0);
        assert!(w.get::<AtmosphereSettings>(camera).is_none());
        w.entity_mut(camera).insert((Hdr,Exposure {ev100: 15.0}, BlendedSkybox::default()));
        w.resource_mut::<ClearColor>().0 = Color::BLACK;
        let managed = w.spawn(light::ManagedDirectionalLight {slot: 0}).id();
        w.insert_resource(Environment::default()); reconcile(&mut w);
        assert_eq!(w.get::<Exposure>(camera).unwrap().ev100,8.5);
        assert_eq!(w.get::<Hdr>(camera).is_some(),had_hdr); assert!(w.get::<BlendedSkybox>(camera).is_none());
        assert!(w.get::<AtmosphereSettings>(camera).is_some());
        assert_eq!(w.resource::<ClearColor>().0,baseline);
        assert_eq!(w.get::<DirectionalLight>(sun).unwrap().illuminance,1234.0);
        assert!(w.get_entity(managed).is_err());
    }
    #[test]
    fn transition_resumes_serialized_progress() {
        let mut w = world();
        w.resource_mut::<Environment>().preset = Some("day".into()); reconcile(&mut w);
        let from = w.run_system_once(|light: light::Light| light.current_values().cloned().unwrap()).unwrap();
        let env = Environment { preset: Some("night".into()), duration: 20.0, elapsed: 8.0, from_json: serde_json::to_string(&from).unwrap() };
        let saved = serde_json::to_string(&env).unwrap();
        w.insert_resource(serde_json::from_str::<Environment>(&saved).unwrap()); reconcile(&mut w);
        let got = w.run_system_once(|light: light::Light| light.current_values().cloned().unwrap()).unwrap();
        let target = BakedLightManifest::parse(light::COMPILED_MANIFEST).unwrap().environments["night"].clone();
        assert_eq!(got,EnvironmentValues::interpolate(&from,&target,0.4,0.4));
        assert_eq!(w.resource::<Environment>().elapsed,8.0);
    }
}
