//! Authoritative CPU field state; scene, Undo and PIE restore the same data.
use bevy::prelude::*;
use crate::{arc_core::*, terrain_render::{TerrainGrid, TerrainMesh, TerrainVersion, build_terrain_mesh}};

#[derive(Message, Clone, Copy, Debug)]
pub struct WorldEffect {
    pub id: u64,
    pub session_id: u64,
    pub tick: u64,
    pub source: u64,
    pub position_m: [f64; 2],
    pub radius_m: f64,
    pub energy_j: f64,
}

#[derive(Resource)]
pub(crate) struct DirtyTerrain {
    pub revision: u32,
    pub min: Vec2,
    pub max: Vec2,
}

#[derive(Resource, Reflect, Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[reflect(Resource, Clone, Default)]
pub struct SimulationState {
    pub fields: Option<GraphFields>,
    pub source: Option<WorldSpec>,
    pub graph_active: bool,
    pub tick: u64,
    #[serde(default)]
    #[reflect(default)]
    pub physics: Option<PhysicsGrid>,
    #[serde(default)]
    #[reflect(default)]
    pub last_effect: Option<(u64, u64)>,
    #[serde(default)]
    #[reflect(default)]
    pub effect_cursor: u64,
    #[serde(default)]
    #[reflect(default)]
    pub biology: Option<[f64; 3]>,
}
impl SimulationState {
    pub(crate) fn water_surface_at(&self, x: f32, z: f32) -> Option<f32> {
        let fields = self.fields.as_ref().filter(|_| self.graph_active)?;
        let (i, j) = fields.grid.nearest_cell(x, z);
        let depth = fields.water_m[fields.grid.idx(i, j)] as f32;
        Some(fields.grid.sample_height(x, z) + depth)
    }

    pub fn validate(&self) -> Result<(),String> {
        if let Some([growth, min_water, optimum]) = self.biology {
            if !(0.0..=1.0).contains(&growth)
                || !(f64::from(0.0001_f32)..=100.0).contains(&min_water)
                || !(-50.0..=100.0).contains(&optimum)
            {
                return Err("invalid simulation biology parameters".into());
            }
        }
        let Some(f)=&self.fields else { return if self.graph_active { Err("active simulation has no fields".into()) } else { Ok(()) }; };
        let n=f.grid.n;
        if !(5..=513).contains(&n) || !f.grid.size.is_finite() || !(1.0..=4096.0).contains(&f.grid.size) {return Err("invalid simulation dimensions".into());}
        let count=n*n;
        if [f.grid.heights.len(),f.grid.erosion_mask.len(),f.temperature_c.len(),f.water_m.len(),f.biomass.len()].iter().any(|x|*x!=count) {return Err("simulation field dimensions differ".into());}
        if f.grid.heights.iter().chain(&f.grid.erosion_mask).any(|x|!x.is_finite()) || f.temperature_c.iter().any(|x|!x.is_finite() || *x < -273.15) || f.water_m.iter().any(|x|!x.is_finite() || *x < 0.0) || f.biomass.iter().any(|x|!x.is_finite() || !(0.0..=1.0).contains(x)) {return Err("invalid simulation field values".into());}
        if f.scatter.len()>5000 || f.scatter.iter().any(|x|!x.pos.is_finite() || !x.rot.is_finite() || !x.scale.is_finite() || x.scale<=0.0) {return Err("invalid simulation scatter".into());}
        let source=self.source.as_ref().ok_or("simulation source spec missing")?;
        if source.resolution as usize!=n || source.size!=f.grid.size {return Err("simulation and source dimensions differ".into());}
        if let Some(physics) = &self.physics {
            if !physics.sunlight_j.is_finite() || physics.sunlight_j < 0.0
                || !physics.external_heat_j.is_finite() || physics.external_heat_j < 0.0
                || physics.cells.len() != count || physics.cells.iter().any(|cell| {
                [cell.water_kg, cell.oil_kg, cell.lava_kg, cell.biomass_kg, cell.ash_kg, cell.sediment_kg]
                    .into_iter().any(|mass| !mass.is_finite() || mass < 0.0)
                    || !cell.energy_j.is_finite()
                    || cell.fluid_velocity_mps.iter().flatten().any(|speed| !speed.is_finite())
                    || cell.fusion_override_j_per_kg.is_some_and(|fusion| !(1.0..=1e7).contains(&fusion))
            }) {
                return Err("invalid physical cell inventory".into());
            }
        }
        Ok(())
    }
    fn baseline(w: &World) -> Self {
        let spec=w.resource::<WorldSpec>().clone();
        let current=&w.resource::<TerrainGrid>().0;
        let grid=if crate::terrain_render::geometry_is_current(w) && current.n==spec.resolution as usize && current.size==spec.size {current.clone()} else {crate::terrain_render::generate_world(&spec)};
        let count=grid.heights.len();
        let water_m=grid.heights.iter().map(|h|(spec.water_level as f64-*h as f64).max(0.0)).collect();
        Self {fields:Some(GraphFields {grid,temperature_c:vec![15.0;count],water_m,biomass:vec![0.0;count],scatter:vec![]}),source:Some(spec),..default()}
    }
}

pub fn evaluate(w: &mut World, target: Option<u32>) -> Result<String,String> {
    let mut spec=w.resource::<WorldSpec>().clone();
    let enable_terrain=!spec.terrain_enabled; spec.terrain_enabled=true;
    let (fields, biology)=evaluate_world_graph_with_biology(w.resource::<StudioGraph>(),&spec,target)?;
    let state=SimulationState {fields:Some(fields),source:Some(spec.clone()),graph_active:true,biology,..default()};
    state.validate()?;
    // Initialize the pre-graph fields before snapshotting, so Undo restores actual terrain too.
    if w.resource::<SimulationState>().fields.is_none() || w.resource::<SimulationState>().source.as_ref().is_some_and(|s|!crate::terrain_render::geometry_matches(s,w.resource::<WorldSpec>())) {let baseline=SimulationState::baseline(w);w.insert_resource(baseline);}
    let mut before=crate::operators::snap_resource::<SimulationState>(w);
    if enable_terrain {before.also.push(crate::operators::snap_resource::<WorldSpec>(w)); w.insert_resource(spec);}
    w.insert_resource(state);
    let mut after=crate::operators::snap_resource::<SimulationState>(w);
    if enable_terrain {after.also.push(crate::operators::snap_resource::<WorldSpec>(w));}
    crate::operators::record_edit(w,"Evaluate world graph",before,after,false);
    Ok("CPU graph applied".into())
}

pub fn describe(w: &mut World) -> serde_json::Value {
    let handles:Vec<_>=w.query_filtered::<&Mesh3d,With<TerrainMesh>>().iter(w).map(|h|h.0.clone()).collect();
    let pines=w.query::<&crate::props::SceneObject>().iter(w).filter(|o|o.name=="PineTree").count();
    let state=w.resource::<SimulationState>();
    let Some(f)=&state.fields else {return serde_json::json!({"active":false});};
    let range=|v:&[f64]| v.iter().fold([f64::INFINITY,f64::NEG_INFINITY],|[a,b],x|[a.min(*x),b.max(*x)]);
    let height_fingerprint = f.grid.heights.iter().fold(0xcbf29ce484222325u64, |hash, height| {
        (hash ^ u64::from(height.to_bits())).wrapping_mul(0x100000001b3)
    });
    let physical_totals = state.physics.as_ref().map(|physics| serde_json::json!({
        "mass_kg": physics.cells.iter().map(|cell| cell.mass_kg()).sum::<f64>(),
        "energy_j": physics.cells.iter().map(|cell| cell.total_energy_j()).sum::<f64>(),
        "oil_kg": physics.cells.iter().map(|cell| cell.oil_kg).sum::<f64>(),
        "lava_kg": physics.cells.iter().map(|cell| cell.lava_kg).sum::<f64>(),
        "melt_snow_cells": physics.cells.iter().filter(|cell| cell.fusion_override_j_per_kg.is_some()).count(),
        "ash_kg": physics.cells.iter().map(|cell| cell.ash_kg).sum::<f64>(),
        "ice_kg": physics.cells.iter().map(|cell| {
            let spacing = f64::from(f.grid.size) / (f.grid.n - 1) as f64;
            cell.phase(spacing * spacing * 100_000.0).ice_kg
        }).sum::<f64>(),
        "sunlight_j": physics.sunlight_j,
        "external_heat_j": physics.external_heat_j,
    }));
    let mesh_matches=!handles.is_empty() && handles.iter().all(|h|w.resource::<Assets<Mesh>>().get(h).and_then(|m|m.attribute(Mesh::ATTRIBUTE_POSITION)).is_some_and(|v|match v {bevy::mesh::VertexAttributeValues::Float32x3(p)=>p.len()==f.grid.heights.len() && p.iter().zip(&f.grid.heights).all(|(p,h)|p[1]==*h),_=>false}));
    serde_json::json!({"physical_totals":physical_totals,"validation_error":state.validate().err(),"last_effect":state.last_effect,"fingerprint":format!("{:016x}",f.fingerprint()),"terrain_matches":f.grid==w.resource::<TerrainGrid>().0,"mesh_matches":mesh_matches,"pine_entities":pines,"terrain_revision":w.resource::<TerrainVersion>().0,"active":state.graph_active,"tick":state.tick,"cells":f.grid.heights.len(),"scatter":f.scatter.len(),"water_m3":f.water_m.iter().sum::<f64>()*(f.grid.size as f64/(f.grid.n-1) as f64).powi(2),"temperature_c":range(&f.temperature_c),"biomass":range(&f.biomass),"height_fingerprint":format!("{height_fingerprint:016x}"),"height_sample":f.grid.heights.iter().step_by((f.grid.heights.len()/8).max(1)).take(8).collect::<Vec<_>>()})
}

/// Runs after ordinary terrain regeneration and before props/water consume its revision.
pub fn sync(w: &mut World, mut last: Local<(bool, Vec<ScatterItem>, Vec<u8>, u64)>) {
    let state_changed=w.is_resource_changed::<SimulationState>() || w.resource::<SimulationState>().tick != last.3;
    let grid_changed=w.is_resource_changed::<TerrainGrid>();
    let state=w.resource::<SimulationState>();
    if state.validate().is_err() {return;}
    let released=state.source.as_ref().is_some_and(|s| !crate::terrain_render::geometry_matches(s,w.resource::<WorldSpec>()));
    if released {w.insert_resource(SimulationState::default());}
    if !state_changed && !grid_changed && !released {return;}
    // Manual sculpting changes CPU geometry too; keep other simulation fields intact.
    if !state_changed && !released {
        let actual=&w.resource::<TerrainGrid>().0;
        if w.resource::<SimulationState>().fields.as_ref().is_some_and(|f|f.grid.heights!=actual.heights) {
            let actual=actual.clone();
            if let Some(f)=w.resource_mut::<SimulationState>().fields.as_mut() {f.grid=actual;}
        }
    }
    let state=w.resource::<SimulationState>();
    let active=state.graph_active;
    let tick=state.tick;
    // read-only pass: compare against the live terrain without cloning it every tick
    let items=state.fields.as_ref().filter(|_|active).map(|f|f.scatter.as_slice()).unwrap_or(&[]);
    let depths: Vec<u8> = state.fields.as_ref().filter(|_| active).map(|fields| {
        fields.water_m.iter().map(|&depth| {
            u8::from(depth > 0.0) + u8::from(depth > f64::from(crate::navigation::WADE_DEPTH))
        }).collect()
    }).unwrap_or_default();
    let mut changed = last.0 != active || last.1.as_slice() != items || last.2 != depths;
    let previous = &w.resource::<TerrainGrid>().0;
    let mut region=None;
    let mut replacement=None;
    if let Some(f)=state.fields.as_ref() {
        let grid=&f.grid;
        if last.0 == active && last.2.len() == depths.len()
            && previous.n == grid.n && previous.size == grid.size
        {
            let spacing = grid.size / (grid.n - 1) as f32;
            let mut min = Vec2::splat(f32::INFINITY);
            let mut max = Vec2::splat(f32::NEG_INFINITY);
            for (index, (before, after)) in previous.heights.iter().zip(&grid.heights).enumerate() {
                if before != after || last.2.get(index) != depths.get(index) {
                    let point = Vec2::new((index % grid.n) as f32, (index / grid.n) as f32)
                        * spacing - Vec2::splat(grid.size * 0.5);
                    min = min.min(point);
                    max = max.max(point);
                }
            }
            if min.is_finite() {
                region=Some((min - Vec2::splat(spacing), max + Vec2::splat(spacing)));
            }
        }
        if grid != previous {
            replacement=Some((build_terrain_mesh(grid,&grid.build_splatmap(w.resource::<WorldSpec>())), grid.clone()));
            changed=true;
        }
    }
    *last = (active, items.to_vec(), depths, tick);
    if let Some((min,max))=region {
        w.insert_resource(DirtyTerrain {
            revision: w.resource::<TerrainVersion>().0.wrapping_add(1),
            min,
            max,
        });
    }
    if let Some((mesh,grid))=replacement {
        let handles:Vec<_>=w.query_filtered::<&Mesh3d,With<TerrainMesh>>().iter(w).map(|h|h.0.clone()).collect();
        for h in handles {let _=w.resource_mut::<Assets<Mesh>>().insert(&h,mesh.clone());}
        w.resource_mut::<TerrainGrid>().0=grid;
    }
    if changed {w.resource_mut::<TerrainVersion>().0=w.resource::<TerrainVersion>().0.wrapping_add(1);}
}

fn step_physics(time: Res<Time<Fixed>>, mut state: ResMut<SimulationState>, studio: Res<crate::studio::StudioState>, graph: Res<StudioGraph>, session: Res<crate::play_mode::PlayModeSnapshot>, mut effects: MessageReader<WorldEffect>) {
    if studio.mode != crate::studio::StudioMode::Playing || !state.graph_active {
        effects.clear();
        return;
    }
    let SimulationState { fields, physics, tick, last_effect, effect_cursor, biology, .. } = &mut *state;
    if let Some(fields) = fields {
        let physics = physics.get_or_insert_with(|| PhysicsGrid::from_graph(fields));
        for effect in effects.read() {
            if effect.session_id != session.session_id || effect.tick > *tick || effect.id <= *effect_cursor {
                continue;
            }
            *effect_cursor = effect.id;
            let default_graph;
            let event_graph = if graph.nodes.iter().any(|node| node.kind == StudioNodeKind::Fireball) {
                &*graph
            } else {
                default_graph = StudioGraph::template(true);
                &default_graph
            };
            let Some(root) = event_graph.nodes.iter().find(|node| node.kind == StudioNodeKind::Fireball) else {
                continue;
            };
            let NodeParameters::Fireball { energy_j, radius_m } = root.params else {
                continue;
            };
            // Graph values scale the cast payload relative to the template.
            let NodeParameters::Fireball { energy_j: base_energy, radius_m: base_radius } =
                NodeParameters::for_kind(StudioNodeKind::Fireball)
            else {
                unreachable!();
            };
            let mut reachable = std::collections::BTreeSet::from([root.id]);
            while event_graph.edges.iter().fold(false, |changed, edge| {
                let inserted = reachable.contains(&edge.from) && reachable.insert(edge.to);
                changed || inserted
            }) {}
            let affected = physics.deposit_heat(
                &fields.grid,
                effect.position_m,
                effect.radius_m * f64::from(radius_m) / f64::from(base_radius),
                effect.energy_j * f64::from(energy_j) / f64::from(base_energy),
            );
            if !affected.is_empty() {
                *last_effect = Some((effect.source, effect.tick));
                let spacing = fields.grid.size as f64 / (fields.grid.n - 1) as f64;
                for node in event_graph.nodes.iter().filter(|node| reachable.contains(&node.id)) {
                    match node.params {
                        NodeParameters::IgniteOil { ignition_c } => {
                            for &index in &affected {
                                let cell = &mut physics.cells[index];
                                cell.burn_oil(spacing * spacing * 100_000.0, ignition_c as f64, cell.oil_kg);
                            }
                        }
                        NodeParameters::MeltSnow { latent_heat_j_per_kg } => {
                            for &index in &affected {
                                physics.cells[index].fusion_override_j_per_kg =
                                    Some(f64::from(latent_heat_j_per_kg));
                            }
                        }
                        NodeParameters::Granular { talus_degrees, iterations } => {
                            physics.relax_sediment_region(
                                &mut fields.grid, talus_degrees as f64, iterations, Some(&affected),
                            );
                        }
                        _ => {}
                    }
                }
            }
        }
        physics.step(fields, *biology, time.delta_secs_f64());
        *tick = tick.saturating_add(1);
    }
}

pub struct SimulationPlugin;
impl Plugin for SimulationPlugin {
    fn build(&self, app:&mut App) {
        app.init_resource::<SimulationState>().register_type::<SimulationState>().add_systems(Update,sync.after(crate::terrain_render::rebuild_on_spec_change).after(crate::world_io::load_scene).before(crate::props::sync_props));
        app.add_message::<WorldEffect>().add_systems(FixedUpdate, step_physics);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn graph_apply_undo_redo_restore_real_mesh_and_fields() {
        let mut app=App::new(); app.add_plugins(MinimalPlugins).init_resource::<crate::operators::ComponentHistory>().register_type::<SimulationState>().register_type::<WorldSpec>().init_resource::<SimulationState>().init_resource::<Assets<Mesh>>().init_resource::<TerrainVersion>().add_systems(Update,sync);
        let spec=WorldSpec {terrain_enabled:false,resolution:17,size:32.0,erosion_droplets:0,thermal_iterations:0,..default()};
        let baseline=HeightfieldGrid::new(17,32.0); let original=baseline.heights.clone();
        let mesh=build_terrain_mesh(&baseline,&baseline.build_splatmap(&spec));
        let h=app.world_mut().resource_mut::<Assets<Mesh>>().add(mesh);
        app.world_mut().spawn((TerrainMesh,Mesh3d(h)));
        let mut graph=StudioGraph::template(false); graph.nodes.truncate(1); graph.edges.clear(); graph.nodes[0].params=NodeParameters::Noise {seed:99,frequency:3.0,warp:0.12};
        app.insert_resource(spec).insert_resource(TerrainGrid(baseline)).insert_resource(graph); app.update();
        evaluate(app.world_mut(),None).unwrap(); app.update();
        assert!(app.world().resource::<WorldSpec>().terrain_enabled);
        let generated=app.world().resource::<TerrainGrid>().0.heights.clone(); assert_ne!(generated,original);
        assert!(describe(app.world_mut())["mesh_matches"].as_bool().unwrap());
        let undo=app.world_mut().resource_mut::<crate::operators::ComponentHistory>().undo().unwrap();
        crate::operators::apply_snap(app.world_mut(),&undo); app.update();
        assert_eq!(app.world().resource::<TerrainGrid>().0.heights,original);
        assert!(!app.world().resource::<SimulationState>().graph_active);
        assert!(!app.world().resource::<WorldSpec>().terrain_enabled);
        let redo=app.world_mut().resource_mut::<crate::operators::ComponentHistory>().redo().unwrap();
        crate::operators::apply_snap(app.world_mut(),&redo); app.update();
        assert_eq!(app.world().resource::<TerrainGrid>().0.heights,generated);
        let state=app.world().resource::<SimulationState>();
        assert_eq!(serde_json::from_str::<SimulationState>(&serde_json::to_string(state).unwrap()).unwrap(),*state);
    }
    #[test]
    fn invalid_field_state_is_rejected() {
        let mut w=World::new();w.insert_resource(WorldSpec {resolution:17,size:32.0,..default()});w.insert_resource(TerrainGrid(HeightfieldGrid::new(17,32.0)));
        let mut state=SimulationState::baseline(&w);assert!(state.validate().is_ok());
        state.fields.as_mut().unwrap().water_m[0]=-1.0;assert!(state.validate().is_err());
    }
}
