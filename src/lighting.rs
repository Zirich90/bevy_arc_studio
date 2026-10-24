//! AAA atmosphere: physical sky, sun/moon cycle, volumetric fog, SSR, bloom, night fires.

use bevy::camera::Exposure;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::{
    AtmosphereEnvironmentMapLight, CascadeShadowConfig, CascadeShadowConfigBuilder, DirectionalLightShadowMap, FogVolume, GlobalAmbientLight, VolumetricFog,
    VolumetricLight,
};
use bevy::pbr::{AtmosphereSettings, DistanceFog, FogFalloff, ScreenSpaceReflections};
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;

use crate::arc_core::{QualityPreset, WorldSpec, smoothstep};

#[derive(Component)]
pub struct Sun;
#[derive(Component)]
pub struct Moon;
/// Point light that is lit between dusk and dawn (19:30–06:30).
#[derive(Component, Clone)]
pub struct NightLight {
    pub base: f32,
}

/// Multiplier for moonlight and night ambient (1.0 = studio default).
#[derive(Resource)]
pub struct NightBoost(pub f32);

impl Default for NightBoost {
    fn default() -> Self {
        Self(1.0)
    }
}

const SUN_AZIMUTH: f32 = 0.6;
const EV_DAY: f32 = 13.5;
const EV_NIGHT: f32 = 5.5;

/// Unit vector pointing from the scene towards the sun.
pub fn sun_direction(t: f32) -> Vec3 {
    let a = (t - 6.0) / 24.0 * std::f32::consts::TAU;
    Vec3::new(a.cos() * SUN_AZIMUTH.cos(), a.sin(), a.cos() * SUN_AZIMUTH.sin()).normalize()
}

/// 0 = night, 1 = full day (smooth through twilight).
pub fn daylight(t: f32) -> f32 {
    smoothstep(-0.12, 0.28, sun_direction(t).y)
}

/// 1 when night lights should burn: 19:30 → 06:30 with 30 min fades.
pub fn night_factor(t: f32) -> f32 {
    let t = t.rem_euclid(24.0);
    if t >= 12.0 { smoothstep(19.25, 19.75, t) } else { 1.0 - smoothstep(6.25, 6.75, t) }
}

pub fn cascades() -> CascadeShadowConfig {
    CascadeShadowConfigBuilder { num_cascades: 4, minimum_distance: 0.5, maximum_distance: 220.0, first_cascade_far_bound: 12.0, overlap_proportion: 0.3 }.build()
}

fn fog_component(spec: &WorldSpec) -> DistanceFog {
    DistanceFog {
        color: Color::linear_rgb(spec.fog_color[0], spec.fog_color[1], spec.fog_color[2]),
        directional_light_color: Color::srgba(1.0, 0.75, 0.45, 0.4),
        directional_light_exponent: 30.0,
        falloff: FogFalloff::Exponential { density: spec.fog_density },
    }
}

/// All render components on the 3D camera.
/// The `Atmosphere` is its own entity (planet centre in world space); cameras only carry settings.
pub fn camera_stack(spec: &WorldSpec) -> impl Bundle {
    (
        Msaa::Off,
        Tonemapping::TonyMcMapface,
        Exposure { ev100: EV_DAY },
        AtmosphereSettings::default(),
        AtmosphereEnvironmentMapLight::default(),
        (
            Bloom::NATURAL,
            ScreenSpaceReflections::default(),
            VolumetricFog { ambient_intensity: 0.0, step_count: 48, ..default() },
            fog_component(spec),
        ),
    )
}

pub struct LightingPlugin;

impl Plugin for LightingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NightBoost>().insert_resource(GlobalAmbientLight { color: Color::srgb(0.45, 0.55, 0.9), brightness: 0.0, ..default() })
            .insert_resource(DirectionalLightShadowMap { size: 4096 })
            .add_systems(Update, (advance_clock, apply_time_of_day, apply_toggles, flicker_night_lights).chain());
    }
}

fn advance_clock(mut spec: ResMut<WorldSpec>, time: Res<Time>) {
    if spec.auto_sun {
        spec.time_of_day = (spec.time_of_day + time.delta_secs() * spec.sun_speed).rem_euclid(24.0);
    }
}

#[allow(clippy::type_complexity)]
fn apply_time_of_day(
    spec: Res<WorldSpec>,
    mut sun: Query<(&mut Transform, &mut DirectionalLight), (With<Sun>, Without<Moon>)>,
    mut moon: Query<(&mut Transform, &mut DirectionalLight), (With<Moon>, Without<Sun>)>,
    mut cam: Query<(&mut Exposure, &mut DistanceFog)>,
    mut vol: Query<&mut FogVolume>,
    mut ambient: ResMut<GlobalAmbientLight>,
    boost: Res<NightBoost>,
) {
    let t = spec.time_of_day;
    let dir = sun_direction(t);
    let d = daylight(t);
    let elev = dir.y;
    // exposure follows the log of the available light (sun + twilight sky) so dusk never over-exposes
    let lux = 130_000.0 * spec.sun_intensity * smoothstep(-0.04, 0.12, elev) + 3000.0 * smoothstep(-0.35, 0.05, elev) + 100.0;
    let frac = ((lux / 100.0).log10() / 1300.0_f32.log10()).clamp(0.0, 1.0);
    let ev = EV_NIGHT + (EV_DAY - EV_NIGHT) * frac;
    // extra moonlight only once the sky is really dark, so twilight exposure does not blow it out
    let nb = 1.0 + (boost.0 - 1.0) * (1.0 - frac).powf(1.4);
    if let Ok((mut tf, mut l)) = sun.single_mut() {
        *tf = Transform::default().looking_to(-dir, Vec3::Y);
        // low sun: longer air path → dimmer and warmer light
        let low = smoothstep(0.0, 0.4, elev);
        l.illuminance = 130_000.0 * spec.sun_intensity * smoothstep(-0.04, 0.12, elev) * (0.3 + 0.7 * low);
        l.color = Color::srgb(1.0, 0.68 + 0.32 * low, 0.42 + 0.58 * low);
        l.shadow_maps_enabled = spec.shadows && elev > -0.03;
    }
    if let Ok((mut tf, mut l)) = moon.single_mut() {
        let mdir = Vec3::new(-dir.x, dir.y.abs().max(0.35), -dir.z).normalize();
        *tf = Transform::default().looking_to(-mdir, Vec3::Y);
        l.illuminance = 22.0 * nb * (1.0 - d);
        l.shadow_maps_enabled = spec.shadows && d < 0.3;
    }
    ambient.brightness = 14.0 * nb * (1.0 - d) + 450.0 * d;
    for mut v in &mut vol {
        v.density_factor = if spec.fog_enabled { spec.fog_density * 1.6 } else { 0.0 };
    }
    for (mut exp, mut fog) in &mut cam {
        exp.ev100 = ev;
        let vis = smoothstep(-0.25, 0.3, elev);
        let k = 0.015 + 0.985 * vis;
        let dusk = (1.0 - (elev.abs() * 4.0).min(1.0)) * vis; // warm tint near the horizon
        let c = Vec3::from(spec.fog_color) * k + Vec3::new(0.35, 0.12, -0.05) * dusk * 0.5;
        fog.color = Color::linear_rgb(c.x.max(0.0), c.y.max(0.0), c.z.max(0.0));
        fog.directional_light_color = Color::srgba(1.0, 0.75, 0.45, 0.4 * smoothstep(0.0, 0.2, elev));
        fog.falloff = FogFalloff::Exponential { density: if spec.fog_enabled { spec.fog_density } else { 0.0 } };
    }
}

/// Feature toggles & quality preset → add/remove camera components.
fn apply_toggles(
    spec: Res<WorldSpec>,
    mut commands: Commands,
    cam: Query<Entity, With<Camera3d>>,
    sun: Query<Entity, With<Sun>>,
    mut shadow_map: ResMut<DirectionalLightShadowMap>,
    mut last: Local<Option<(bool, bool, bool, QualityPreset)>>,
) {
    // the time-of-day slider mutates WorldSpec every frame; only react to real toggle changes
    let key = (spec.bloom, spec.reflections, spec.god_rays, spec.quality_preset);
    if *last == Some(key) {
        return;
    }
    *last = Some(key);
    let size = match spec.quality_preset {
        QualityPreset::Low => 1024,
        QualityPreset::Medium => 2048,
        QualityPreset::High => 4096,
        QualityPreset::Ultra => 8192,
    };
    if shadow_map.size != size {
        shadow_map.size = size;
    }
    for e in &cam {
        let mut c = commands.entity(e);
        if spec.bloom { c.insert(Bloom::NATURAL); } else { c.remove::<Bloom>(); }
        if spec.reflections { c.insert(ScreenSpaceReflections::default()); } else { c.remove::<ScreenSpaceReflections>(); }
        if spec.god_rays { c.insert(VolumetricFog { ambient_intensity: 0.0, step_count: 48, ..default() }); } else { c.remove::<VolumetricFog>(); }
    }
    for e in &sun {
        if spec.god_rays { commands.entity(e).insert(VolumetricLight); } else { commands.entity(e).remove::<VolumetricLight>(); }
    }
}

fn flicker_night_lights(spec: Res<WorldSpec>, time: Res<Time>, mut q: Query<(&NightLight, &mut PointLight)>) {
    let n = night_factor(spec.time_of_day);
    let t = time.elapsed_secs();
    let f = 1.0 + 0.10 * (t * 17.0).sin() * (t * 5.3).cos() + 0.05 * (t * 41.0).sin();
    for (nl, mut l) in &mut q {
        l.intensity = nl.base * n * f;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn night_window_matches_spec() {
        assert_eq!(night_factor(12.0), 0.0);
        assert_eq!(night_factor(19.0), 0.0);
        assert_eq!(night_factor(20.0), 1.0);
        assert_eq!(night_factor(3.0), 1.0);
        assert_eq!(night_factor(7.0), 0.0);
        assert!(sun_direction(12.0).y > 0.99 && sun_direction(0.0).y < -0.99);
        assert!(daylight(12.0) > 0.99 && daylight(0.0) < 0.01);
    }
}
