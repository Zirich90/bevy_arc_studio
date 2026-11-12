//! Pure math core: world passport, heightfield, erosion, splatmap, scatter, gizmo math, undo history.
//! No dependency on render ECS systems; unit-testable.

/// Stable, application-owned graph document. Noodle entities are a disposable view of this model.
#[derive(Resource, Reflect, Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[reflect(Resource, Clone)]
pub struct StudioGraph {
    pub version: u32,
    pub nodes: Vec<StudioGraphNode>,
    pub edges: Vec<StudioGraphEdge>,
}

#[derive(Reflect, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum StudioNodeKind {
    Noise, HydraulicErosion, Temperature, FluidFlow, Biology, Scatter,
    Fireball, IgniteOil, MeltSnow, Granular, InvalidateNavigation,
}

impl StudioNodeKind {
    pub fn event_pipeline(self) -> bool {
        matches!(self, Self::Fireball | Self::IgniteOil | Self::MeltSnow | Self::Granular | Self::InvalidateNavigation)
    }
    pub fn source(self) -> bool { matches!(self, Self::Noise | Self::Fireball) }
}

/// Typed node settings; Legacy exists only at the v1 document import boundary.
#[derive(Reflect, Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub enum NodeParameters {
    #[default]
    Legacy,
    Noise { seed: u32, frequency: f32, warp: f32 },
    HydraulicErosion { seed: u32, droplets: u32, thermal_iterations: u32 },
    Temperature { ambient_c: f32, lapse_c_per_m: f32, diffusion: f32 },
    FluidFlow { rain_depth_m: f32, conductivity: f32, iterations: u32 },
    Biology { growth: f32, min_water_m: f32, optimum_c: f32 },
    Scatter { seed: u32, density: f32, spacing_m: f32 },
    Fireball { energy_j: f32, radius_m: f32 },
    IgniteOil { ignition_c: f32 },
    MeltSnow { latent_heat_j_per_kg: f32 },
    Granular { talus_degrees: f32, iterations: u32 },
    InvalidateNavigation,
}
impl NodeParameters {
    pub fn for_kind(kind: StudioNodeKind) -> Self {
        use StudioNodeKind as K;
        match kind {
            K::Noise => Self::Noise { seed:1337, frequency:3.0, warp:0.12 },
            K::HydraulicErosion => Self::HydraulicErosion { seed:1337, droplets:60_000, thermal_iterations:24 },
            K::Temperature => Self::Temperature { ambient_c:15.0, lapse_c_per_m:0.0065, diffusion:0.1 },
            K::FluidFlow => Self::FluidFlow { rain_depth_m:0.05, conductivity:0.2, iterations:8 },
            K::Biology => Self::Biology { growth:0.5, min_water_m:0.01, optimum_c:20.0 },
            K::Scatter => Self::Scatter { seed:1337, density:0.6, spacing_m:4.0 },
            K::Fireball => Self::Fireball { energy_j:1_000_000.0, radius_m:4.572 },
            K::IgniteOil => Self::IgniteOil { ignition_c:300.0 },
            K::MeltSnow => Self::MeltSnow { latent_heat_j_per_kg:334_000.0 },
            K::Granular => Self::Granular { talus_degrees:35.0, iterations:8 },
            K::InvalidateNavigation => Self::InvalidateNavigation,
        }
    }
    pub fn kind(&self) -> Option<StudioNodeKind> {
        use StudioNodeKind as K;
        Some(match self {
            Self::Legacy => return None,
            Self::Noise {..} => K::Noise, Self::HydraulicErosion {..} => K::HydraulicErosion,
            Self::Temperature {..} => K::Temperature, Self::FluidFlow {..} => K::FluidFlow,
            Self::Biology {..} => K::Biology, Self::Scatter {..} => K::Scatter,
            Self::Fireball {..} => K::Fireball, Self::IgniteOil {..} => K::IgniteOil,
            Self::MeltSnow {..} => K::MeltSnow, Self::Granular {..} => K::Granular,
            Self::InvalidateNavigation => K::InvalidateNavigation,
        })
    }
    pub fn validate(&self, kind: StudioNodeKind) -> Result<(), String> {
        if self.kind() != Some(kind) { return Err("node settings do not match kind".into()); }
        let range = |x:f32,a:f32,b:f32| x.is_finite() && (a..=b).contains(&x);
        let valid = match *self {
            Self::Legacy => false,
            Self::Noise {frequency,warp,..} => range(frequency,0.01,64.0) && range(warp,0.0,4.0),
            Self::HydraulicErosion {droplets,thermal_iterations,..} => droplets <= 2_000_000 && thermal_iterations <= 512,
            Self::Temperature {ambient_c,lapse_c_per_m,diffusion} => range(ambient_c,-273.0,5000.0) && range(lapse_c_per_m,0.0,0.1) && range(diffusion,0.0,0.25),
            Self::FluidFlow {rain_depth_m,conductivity,iterations} => range(rain_depth_m,0.0,100.0) && range(conductivity,0.0,0.25) && iterations <= 512,
            Self::Biology {growth,min_water_m,optimum_c} => range(growth,0.0,1.0) && range(min_water_m,0.0001,100.0) && range(optimum_c,-50.0,100.0),
            Self::Scatter {density,spacing_m,..} => range(density,0.0,1.0) && range(spacing_m,0.5,128.0),
            Self::Fireball {energy_j,radius_m} => range(energy_j,0.0,1e12) && range(radius_m,0.01,10_000.0),
            Self::IgniteOil {ignition_c} => range(ignition_c,0.0,2000.0),
            Self::MeltSnow {latent_heat_j_per_kg} => range(latent_heat_j_per_kg,1.0,1e7),
            Self::Granular {talus_degrees,iterations} => range(talus_degrees,0.0,89.0) && iterations <= 512,
            Self::InvalidateNavigation => true,
        };
        if valid { Ok(()) } else { Err("node parameter out of range".into()) }
    }
    fn from_v1(kind: StudioNodeKind, value:f32) -> Self {
        let mut settings = Self::for_kind(kind);
        // v1 value had no evaluator. Map it explicitly to each node's primary parameter.
        match &mut settings {
            Self::Noise { frequency,.. } => *frequency = value,
            Self::HydraulicErosion { droplets,.. } => *droplets = value.max(0.0) as u32,
            Self::Temperature { ambient_c,.. } => *ambient_c = value,
            Self::FluidFlow { conductivity,.. } => *conductivity = value * 0.2,
            Self::Biology { growth,.. } => *growth = value,
            Self::Scatter { density,.. } => *density = value,
            Self::Fireball { energy_j,.. } => *energy_j = value * 1_000_000.0,
            Self::IgniteOil { ignition_c } => *ignition_c = value * 300.0,
            Self::MeltSnow { latent_heat_j_per_kg } => *latent_heat_j_per_kg = value * 334_000.0,
            Self::Granular { talus_degrees,.. } => *talus_degrees = value * 35.0,
            _ => {}
        }
        settings
    }
}

#[derive(Reflect, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct StudioGraphNode {
    pub id: u32,
    pub kind: StudioNodeKind,
    pub position: [f32; 2],
    /// Import-only v1 compatibility value; v2 execution uses params.
    #[serde(default)]
    #[reflect(default)]
    pub value: f32,
    #[serde(default)]
    #[reflect(default)]
    pub params: NodeParameters,
}

#[derive(Reflect, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct StudioGraphEdge { pub from: u32, pub to: u32 }

impl StudioGraph {
    pub fn template(events: bool) -> Self {
        use StudioNodeKind::*;
        let kinds: &[StudioNodeKind] = if events { &[Fireball, IgniteOil, MeltSnow, Granular, InvalidateNavigation] }
            else { &[Noise, HydraulicErosion, Temperature, FluidFlow, Biology, Scatter] };
        Self { version: 2,
            nodes: kinds.iter().enumerate().map(|(i, &kind)| StudioGraphNode {
                id: i as u32 + 1, kind,
                position: if events { [[12.0, 95.0], [225.0, 0.0], [225.0, 95.0], [225.0, 190.0], [438.0, 190.0]][i] }
                    else { [12.0 + (i % 3) as f32 * 213.0, (i / 3) as f32 * 150.0] },
                value: 1.0, params: NodeParameters::for_kind(kind),
            }).collect(),
            edges: if events {
                [(1, 2), (1, 3), (1, 4), (4, 5)].into_iter().map(|(from, to)| StudioGraphEdge { from, to }).collect()
            } else {
                (1..kinds.len() as u32).map(|from| StudioGraphEdge { from, to: from + 1 }).collect()
            },
        }
    }

    pub fn migrate(mut self) -> Result<Self,String> {
        match self.version {
            1 => {
                for node in &mut self.nodes {
                    if !node.value.is_finite() || (node.value < 0.0 && node.kind != StudioNodeKind::Temperature) { return Err("invalid v1 node value".into()); }
                    node.params = NodeParameters::from_v1(node.kind,node.value);
                }
                self.version = 2;
            }
            2 => {},
            _ => return Err("unsupported graph version".into()),
        }
        self.validate()?;
        Ok(self)
    }

    /// Validate the trust boundary before replacing the document or accepting a canvas connection.
    pub fn validate(&self) -> Result<Vec<u32>, String> {
        use std::collections::{BTreeMap, BTreeSet};
        if self.version != 2 { return Err("unsupported graph version".into()); }
        if self.nodes.len() > 512 || self.edges.len() > 1024 { return Err("graph limit exceeded".into()); }
        let mut nodes = BTreeMap::new();
        for n in &self.nodes {
            if !n.value.is_finite() || !n.position.iter().all(|p| p.is_finite()) { return Err("non-finite node data".into()); }
            n.params.validate(n.kind)?;
            if nodes.insert(n.id, n).is_some() { return Err("duplicate node id".into()); }
        }
        let mut input = BTreeSet::new();
        for e in &self.edges {
            let a = nodes.get(&e.from).ok_or("missing source")?;
            let b = nodes.get(&e.to).ok_or("missing target")?;
            if a.kind.event_pipeline() != b.kind.event_pipeline() || b.kind.source() || e.from == e.to {
                return Err("incompatible graph ports".into());
            }
            if !input.insert(e.to) { return Err("input capacity exceeded".into()); }
        }
        let mut order = Vec::new();
        let mut seen = BTreeSet::new();
        while seen.len() < nodes.len() {
            let next = nodes.keys().find(|id| !seen.contains(*id) && self.edges.iter().filter(|e| e.to == **id).all(|e| seen.contains(&e.from)));
            let Some(&id) = next else { return Err("graph cycle".into()); };
            seen.insert(id); order.push(id);
        }
        Ok(order)
    }
}

/// Deterministic CPU world-node result. Water is depth in metres; biomass is coverage [0,1].
#[derive(Reflect, Serialize, Deserialize, Clone, Debug, PartialEq)]
#[reflect(Clone)]
pub struct GraphFields {
    pub grid: HeightfieldGrid,
    pub temperature_c: Vec<f64>,
    pub water_m: Vec<f64>,
    pub biomass: Vec<f64>,
    pub scatter: Vec<ScatterItem>,
}

/// Equilibrium at atmospheric pressure, with enthalpy measured from ice at 0 °C.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaterPhase {
    pub temperature_c: f64,
    pub ice_kg: f64,
    pub liquid_kg: f64,
    pub vapor_kg: f64,
}

impl WaterPhase {
    const FUSION: f64 = 334_000.0;
    const VAPORIZATION: f64 = 2_256_000.0;
    const ICE_CAPACITY: f64 = 2_100.0;
    const LIQUID_CAPACITY: f64 = 4_186.0;
    const VAPOR_CAPACITY: f64 = 2_000.0;

    /// The dry cell heat capacity is positive and includes ground and other matter.
    pub fn equilibrium(mass_kg: f64, energy_j: f64, dry_capacity: f64) -> Self {
        Self::with_fusion(mass_kg, energy_j, dry_capacity, Self::FUSION)
    }

    fn with_fusion(mass_kg: f64, energy_j: f64, dry_capacity: f64, fusion: f64) -> Self {
        let melted = mass_kg * fusion;
        let liquid_capacity = dry_capacity + mass_kg * Self::LIQUID_CAPACITY;
        let boiling = melted + 100.0 * liquid_capacity;
        let vaporized = boiling + mass_kg * Self::VAPORIZATION;
        let (temperature_c, ice_kg, liquid_kg, vapor_kg) = if energy_j < 0.0 {
            (energy_j / (dry_capacity + mass_kg * Self::ICE_CAPACITY), mass_kg, 0.0, 0.0)
        } else if energy_j < melted {
            let liquid = (energy_j / fusion).min(mass_kg);
            (0.0, mass_kg - liquid, liquid, 0.0)
        } else if energy_j < boiling {
            ((energy_j - melted) / liquid_capacity, 0.0, mass_kg, 0.0)
        } else if energy_j < vaporized {
            let vapor = ((energy_j - boiling) / Self::VAPORIZATION).min(mass_kg);
            (100.0, 0.0, mass_kg - vapor, vapor)
        } else {
            let capacity = dry_capacity + mass_kg * Self::VAPOR_CAPACITY;
            (100.0 + (energy_j - vaporized) / capacity, 0.0, 0.0, mass_kg)
        };
        Self { temperature_c, ice_kg, liquid_kg, vapor_kg }
    }

    pub fn enthalpy_j(self, dry_capacity: f64) -> f64 {
        let t = self.temperature_c;
        dry_capacity * t
            + self.ice_kg * Self::ICE_CAPACITY * t
            + self.liquid_kg * (Self::FUSION + Self::LIQUID_CAPACITY * t)
            + self.vapor_kg * (Self::FUSION + 100.0 * Self::LIQUID_CAPACITY
                + Self::VAPORIZATION + Self::VAPOR_CAPACITY * (t - 100.0))
    }
}

#[cfg(test)]
#[test]
fn water_phase_conserves_mass_and_enthalpy() {
    let mass = 2.0;
    let dry_capacity = 1000.0;
    let boiling = mass * 334_000.0 + 100.0 * (dry_capacity + mass * 4186.0);
    for energy in [-50_000.0, 334_000.0, 900_000.0, boiling + 2_256_000.0, 8_000_000.0] {
        let phase = WaterPhase::equilibrium(mass, energy, dry_capacity);
        assert_eq!(phase.ice_kg + phase.liquid_kg + phase.vapor_kg, mass);
        assert!((phase.enthalpy_j(dry_capacity) - energy).abs() < 1e-8);
    }
    let melting = WaterPhase::equilibrium(mass, 334_000.0, dry_capacity);
    assert_eq!((melting.temperature_c, melting.ice_kg, melting.liquid_kg), (0.0, 1.0, 1.0));
    let evaporating = WaterPhase::equilibrium(mass, boiling + 2_256_000.0, dry_capacity);
    assert_eq!((evaporating.temperature_c, evaporating.liquid_kg, evaporating.vapor_kg), (100.0, 1.0, 1.0));
}

/// Transfer heat between two closed cells without crossing thermal equilibrium.
/// Conductance is the energy transferred per degree during this step.
pub fn exchange_cell_heat(
    mass_kg: [f64; 2],
    energy_j: &mut [f64; 2],
    dry_capacity: [f64; 2],
    conductance: f64,
) {
    exchange_heat_with_fusion(mass_kg, energy_j, dry_capacity, conductance, [WaterPhase::FUSION; 2]);
}

fn exchange_heat_with_fusion(
    mass_kg: [f64; 2],
    energy_j: &mut [f64; 2],
    dry_capacity: [f64; 2],
    conductance: f64,
    fusion: [f64; 2],
) {
    let temperature = |i: usize, energy| {
        WaterPhase::with_fusion(mass_kg[i], energy, dry_capacity[i], fusion[i]).temperature_c
    };
    let difference = temperature(0, energy_j[0]) - temperature(1, energy_j[1]);
    let donor = usize::from(difference < 0.0);
    let receiver = 1 - donor;
    let minimum_capacity = |i: usize| dry_capacity[i] + mass_kg[i] * WaterPhase::VAPOR_CAPACITY;
    let limit = 1.0 / (1.0 / minimum_capacity(0) + 1.0 / minimum_capacity(1));
    let transfer = difference.abs() * conductance.clamp(0.0, limit);
    energy_j[donor] -= transfer;
    energy_j[receiver] += transfer;
}

#[test]
fn heat_exchange_conserves_energy_through_melting() {
    let mass = [1.0, 1.0];
    let capacity = [1000.0, 1000.0];
    let mut energy = [800_000.0, -30_000.0];
    let initial_total = energy.iter().sum::<f64>();
    for _ in 0..50 {
        exchange_cell_heat(mass, &mut energy, capacity, 10_000.0);
        let hot = WaterPhase::equilibrium(mass[0], energy[0], capacity[0]);
        let cold = WaterPhase::equilibrium(mass[1], energy[1], capacity[1]);
        assert!(hot.temperature_c >= cold.temperature_c - 1e-10);
        assert!((energy.iter().sum::<f64>() - initial_total).abs() < 1e-8);
    }
    assert_eq!(WaterPhase::equilibrium(mass[1], energy[1], capacity[1]).ice_kg, 0.0);
}

/// Per-cell inventory. Water phases share one mass and are derived from enthalpy.
#[derive(Reflect, Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PhysicsCell {
    pub water_kg: f64,
    pub oil_kg: f64,
    #[serde(default)]
    #[reflect(default)]
    pub lava_kg: f64,
    /// Per-tick outgoing transport velocity in m/s, indexed by water, oil, lava.
    #[serde(default)]
    #[reflect(default)]
    pub fluid_velocity_mps: [[f64; 2]; 3],
    #[serde(default)]
    #[reflect(default)]
    pub fusion_override_j_per_kg: Option<f64>,
    pub biomass_kg: f64,
    pub ash_kg: f64,
    pub sediment_kg: f64,
    pub energy_j: f64,
}

impl PhysicsCell {
    const OIL_CAPACITY: f64 = 2000.0;
    const LAVA_CAPACITY: f64 = 1200.0;
    const LAVA_FLOW_THRESHOLD_C: f64 = 700.0;
    const OIL_HEAT: f64 = 44_000_000.0;
    const BIOMASS_HEAT: f64 = 17_000_000.0;

    pub fn mass_kg(self) -> f64 {
        self.water_kg + self.oil_kg + self.lava_kg + self.biomass_kg + self.ash_kg + self.sediment_kg
    }

    pub fn total_energy_j(self) -> f64 {
        self.energy_j + self.oil_kg * Self::OIL_HEAT + self.biomass_kg * Self::BIOMASS_HEAT
    }

    fn dry_capacity(self, ground_capacity: f64) -> f64 {
        ground_capacity + Self::OIL_CAPACITY * self.oil_kg + 1500.0 * self.biomass_kg
            + Self::LAVA_CAPACITY * self.lava_kg
            + 800.0 * (self.ash_kg + self.sediment_kg)
    }

    fn fusion(self) -> f64 {
        self.fusion_override_j_per_kg.unwrap_or(WaterPhase::FUSION)
    }

    pub fn phase(self, ground_capacity: f64) -> WaterPhase {
        WaterPhase::with_fusion(self.water_kg, self.energy_j, self.dry_capacity(ground_capacity), self.fusion())
    }

    /// Convert substrate to biomass. Return the chemical energy supplied by light.
    pub fn grow_biomass(
        &mut self,
        area: f64,
        growth: f64,
        min_water_m: f64,
        optimum_c: f64,
    ) -> f64 {
        let phase = self.phase(area * 100_000.0);
        let moisture = (phase.liquid_kg / (area * 1000.0 * min_water_m)).min(1.0);
        let warmth = (1.0 - (phase.temperature_c - optimum_c).abs() / 25.0).max(0.0);
        let space = (area - self.biomass_kg).max(0.0);
        let grown = (space * growth * moisture * warmth / 4096.0).min(self.sediment_kg);
        self.sediment_kg -= grown;
        self.biomass_kg += grown;
        grown * Self::BIOMASS_HEAT
    }

    /// Advect available liquid water and its specific enthalpy to the receiver.
    pub fn move_water_to(&mut self, receiver: &mut Self, ground_capacity: f64, requested_kg: f64) {
        let phase = self.phase(ground_capacity);
        let moved = requested_kg.clamp(0.0, phase.liquid_kg);
        let energy = moved * (self.fusion() + WaterPhase::LIQUID_CAPACITY * phase.temperature_c);
        self.water_kg -= moved;
        receiver.water_kg += moved;
        self.energy_j -= energy;
        receiver.energy_j += energy;
    }

    /// Advect oil and its sensible heat. Chemical energy follows the oil mass.
    pub fn move_oil_to(&mut self, receiver: &mut Self, ground_capacity: f64, requested_kg: f64) {
        let moved = requested_kg.clamp(0.0, self.oil_kg);
        let energy = moved * Self::OIL_CAPACITY * self.phase(ground_capacity).temperature_c;
        self.oil_kg -= moved;
        receiver.oil_kg += moved;
        self.energy_j -= energy;
        receiver.energy_j += energy;
    }

    fn move_lava_to(&mut self, receiver: &mut Self, ground_capacity: f64, requested_kg: f64) {
        let temperature = self.phase(ground_capacity).temperature_c;
        if temperature < Self::LAVA_FLOW_THRESHOLD_C {
            return;
        }
        let moved = requested_kg.clamp(0.0, self.lava_kg);
        let energy = moved * Self::LAVA_CAPACITY * temperature;
        self.lava_kg -= moved;
        receiver.lava_kg += moved;
        self.energy_j -= energy;
        receiver.energy_j += energy;
    }

    /// Lumped reaction: ash represents the retained combustion products.
    pub fn burn_oil(&mut self, ground_capacity: f64, ignition_c: f64, budget_kg: f64) {
        if self.oil_kg == 0.0 || self.phase(ground_capacity).temperature_c < ignition_c {
            return;
        }
        let burned = budget_kg.clamp(0.0, self.oil_kg);
        self.oil_kg -= burned;
        self.ash_kg += burned;
        self.energy_j += burned * Self::OIL_HEAT;
    }
}

#[test]
fn oil_reaction_preserves_mass_and_chemical_plus_thermal_energy() {
    let mut cell = PhysicsCell {
        water_kg: 1.0,
        oil_kg: 2.0,
        lava_kg: 0.0,
        fluid_velocity_mps: [[0.0; 2]; 3],
        fusion_override_j_per_kg: None,
        biomass_kg: 0.0,
        ash_kg: 0.0,
        sediment_kg: 0.0,
        energy_j: 0.0,
    };
    let mass = cell.mass_kg();
    cell.burn_oil(1000.0, 300.0, 0.5);
    assert_eq!(cell.oil_kg, 2.0);
    cell.energy_j = 10_000_000.0;
    let total = cell.total_energy_j();
    cell.burn_oil(1000.0, 300.0, 0.5);
    assert_eq!(cell.oil_kg, 1.5);
    assert_eq!(cell.ash_kg, 0.5);
    assert_eq!(cell.mass_kg(), mass);
    assert_eq!(cell.total_energy_j(), total);
}

#[test]
fn melt_snow_fusion_cost_changes_phase_without_adding_energy() {
    let mut cell = PhysicsCell {
        water_kg: 1.0,
        energy_j: 167_000.0,
        ..Default::default()
    };
    assert_eq!(cell.phase(1000.0).ice_kg, 0.5);
    cell.fusion_override_j_per_kg = Some(668_000.0);
    assert_eq!(cell.phase(1000.0).ice_kg, 0.75);
    assert_eq!(cell.mass_kg(), 1.0);
    assert_eq!(cell.total_energy_j(), 167_000.0);
}

#[test]
fn lava_transport_preserves_mass_and_heat_and_stops_when_cold() {
    let capacity = 1000.0;
    let mut donor = PhysicsCell {
        lava_kg: 2.0,
        energy_j: (capacity + 2.0 * PhysicsCell::LAVA_CAPACITY) * 900.0,
        ..Default::default()
    };
    let mut receiver = PhysicsCell::default();
    let energy = donor.total_energy_j();
    donor.move_lava_to(&mut receiver, capacity, 0.5);
    assert_eq!(receiver.lava_kg, 0.5);
    assert_eq!(donor.mass_kg() + receiver.mass_kg(), 2.0);
    assert_eq!(donor.total_energy_j() + receiver.total_energy_j(), energy);
    receiver.move_lava_to(&mut donor, capacity, 0.5);
    assert_eq!(receiver.lava_kg, 0.5);
}

/// Closed grid with a fixed traversal order. Every face is visited once per step.
#[derive(Reflect, Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PhysicsGrid {
    pub cells: Vec<PhysicsCell>,
    #[serde(default)]
    #[reflect(default)]
    pub sunlight_j: f64,
    #[serde(default)]
    #[reflect(default)]
    pub external_heat_j: f64,
}

impl PhysicsGrid {
    /// Relax slopes by moving loose sediment. Bedrock stays fixed.
    pub fn relax_sediment(&mut self, grid: &mut HeightfieldGrid, talus_degrees: f64, iterations: u32) {
        self.relax_sediment_region(grid, talus_degrees, iterations, None);
    }

    pub fn relax_sediment_region(
        &mut self,
        grid: &mut HeightfieldGrid,
        talus_degrees: f64,
        iterations: u32,
        affected: Option<&[usize]>,
    ) {
        let area = Self::cell_area(grid);
        let mass_per_m = 1600.0 * area;
        let limit = area.sqrt() * talus_degrees.to_radians().tan();
        let mut heights: Vec<_> = grid.heights.iter().map(|&h| h as f64).collect();
        let active = affected.map(|indices| {
            let mut mask = vec![false; heights.len()];
            for &index in indices {
                mask[index] = true;
            }
            mask
        });
        for _ in 0..iterations {
            for a in 0..heights.len() {
                let east = (a % grid.n + 1 < grid.n).then_some(a + 1);
                let south = (a + grid.n < heights.len()).then_some(a + grid.n);
                for b in [east, south].into_iter().flatten() {
                    if active.as_ref().is_some_and(|mask| !mask[a] && !mask[b]) {
                        continue;
                    }
                    let (high, low) = if heights[a] >= heights[b] { (a, b) } else { (b, a) };
                    let excess = (heights[high] - heights[low] - limit).max(0.0);
                    let moved = (excess * 0.5 * mass_per_m).min(self.cells[high].sediment_kg);
                    let temperature = self.cells[high].phase(area * 100_000.0).temperature_c;
                    let heat = moved * 800.0 * temperature;
                    self.cells[high].sediment_kg -= moved;
                    self.cells[low].sediment_kg += moved;
                    self.cells[high].energy_j -= heat;
                    self.cells[low].energy_j += heat;
                    heights[high] -= moved / mass_per_m;
                    heights[low] += moved / mass_per_m;
                }
            }
        }
        for (height, next) in grid.heights.iter_mut().zip(heights) {
            *height = next as f32;
        }
    }

    pub fn from_graph(fields: &GraphFields) -> Self {
        let area = Self::cell_area(&fields.grid);
        let cells = fields.water_m.iter().zip(&fields.temperature_c).zip(&fields.biomass)
            .map(|((&depth, &temperature), &biomass)| {
                let water = depth * area * 1000.0;
                let mut cell = PhysicsCell {
                    water_kg: water,
                    oil_kg: 0.0,
                    lava_kg: 0.0,
                    fluid_velocity_mps: [[0.0; 2]; 3],
                    fusion_override_j_per_kg: None,
                    biomass_kg: biomass * area,
                    ash_kg: 0.0,
                    sediment_kg: 1600.0 * area,
                    energy_j: 0.0,
                };
                let phase = WaterPhase {
                    temperature_c: temperature,
                    ice_kg: if temperature < 0.0 { water } else { 0.0 },
                    liquid_kg: if (0.0..100.0).contains(&temperature) { water } else { 0.0 },
                    vapor_kg: if temperature >= 100.0 { water } else { 0.0 },
                };
                cell.energy_j = phase.enthalpy_j(cell.dry_capacity(area * 100_000.0));
                cell
            }).collect();
        Self { cells, sunlight_j: 0.0, external_heat_j: 0.0 }
    }

    fn cell_area(grid: &HeightfieldGrid) -> f64 {
        (grid.size as f64 / (grid.n - 1) as f64).powi(2)
    }

    /// Deposit one pulse over the intersected samples. A pulse missing the grid adds no energy.
    pub fn deposit_heat(&mut self, grid: &HeightfieldGrid, center: [f64; 2], radius: f64, joules: f64) -> Vec<usize> {
        if center.iter().any(|v| !v.is_finite()) || !radius.is_finite() || radius <= 0.0
            || !joules.is_finite() || joules <= 0.0
        {
            return Vec::new();
        }
        let spacing = Self::cell_area(grid).sqrt();
        let half = grid.size as f64 * 0.5;
        let samples: Vec<_> = (0..self.cells.len()).filter_map(|i| {
            let x = (i % grid.n) as f64 * spacing - half - center[0];
            let z = (i / grid.n) as f64 * spacing - half - center[1];
            let weight = 1.0 - x.hypot(z) / radius;
            (weight > 0.0).then_some((i, weight))
        }).collect();
        if samples.is_empty() {
            return Vec::new();
        }
        let weight_sum = samples.iter().map(|(_, w)| w).sum::<f64>();
        for &(i, weight) in &samples {
            self.cells[i].energy_j += joules * (weight / weight_sum);
        }
        self.external_heat_j += joules;
        samples.into_iter().map(|(i, _)| i).collect()
    }

    pub fn step(&mut self, fields: &mut GraphFields, biology: Option<[f64; 3]>, dt_seconds: f64) {
        let grid = &fields.grid;
        let area = Self::cell_area(grid);
        let capacity = area * 100_000.0;
        for cell in &mut self.cells {
            cell.fluid_velocity_mps = [[0.0; 2]; 3];
        }
        let speed_scale = area.sqrt() / dt_seconds;
        for a in 0..self.cells.len() {
            let right = (a % grid.n + 1 < grid.n).then_some(a + 1);
            let down = (a + grid.n < self.cells.len()).then_some(a + grid.n);
            for b in [right, down].into_iter().flatten() {
                let (left, right) = self.cells.split_at_mut(b);
                let (a_cell, b_cell) = (&mut left[a], &mut right[0]);
                let mut energy = [a_cell.energy_j, b_cell.energy_j];
                exchange_heat_with_fusion(
                    [a_cell.water_kg, b_cell.water_kg],
                    &mut energy,
                    [a_cell.dry_capacity(capacity), b_cell.dry_capacity(capacity)],
                    area * 100.0,
                    [a_cell.fusion(), b_cell.fusion()],
                );
                [a_cell.energy_j, b_cell.energy_j] = energy;
                let head = |cell: PhysicsCell, height: f32| {
                    let phase = cell.phase(capacity);
                    height as f64 + phase.ice_kg / (917.0 * area) + phase.liquid_kg / (1000.0 * area)
                };
                let difference = head(*a_cell, grid.heights[a]) - head(*b_cell, grid.heights[b]);
                let water = difference.abs() * 50.0 * area;
                let axis = usize::from(b != a + 1 || grid.n == 1);
                let (donor, receiver, direction) = if difference > 0.0 {
                    (&mut *a_cell, &mut *b_cell, 1.0)
                } else {
                    (&mut *b_cell, &mut *a_cell, -1.0)
                };
                let available = donor.phase(capacity).liquid_kg;
                if available > 0.0 {
                    let before = donor.water_kg;
                    donor.move_water_to(receiver, capacity, water);
                    donor.fluid_velocity_mps[0][axis] +=
                        direction * speed_scale * ((before - donor.water_kg) / available);
                }
                let oil_head = head(*a_cell, grid.heights[a]) - head(*b_cell, grid.heights[b])
                    + (a_cell.oil_kg - b_cell.oil_kg) / (850.0 * area);
                let oil = oil_head.abs() * 42.5 * area;
                let (donor, receiver, direction) = if oil_head > 0.0 {
                    (&mut *a_cell, &mut *b_cell, 1.0)
                } else {
                    (&mut *b_cell, &mut *a_cell, -1.0)
                };
                let available = donor.oil_kg;
                if available > 0.0 {
                    let before = donor.oil_kg;
                    donor.move_oil_to(receiver, capacity, oil);
                    donor.fluid_velocity_mps[1][axis] +=
                        direction * speed_scale * ((before - donor.oil_kg) / available);
                }
                let lava_head = grid.heights[a] as f64 - grid.heights[b] as f64
                    + (a_cell.lava_kg - b_cell.lava_kg) / (2700.0 * area);
                let lava = lava_head.abs() * 2.7 * area;
                let (donor, receiver, direction) = if lava_head > 0.0 {
                    (&mut *a_cell, &mut *b_cell, 1.0)
                } else {
                    (&mut *b_cell, &mut *a_cell, -1.0)
                };
                let available = donor.lava_kg;
                if available > 0.0 {
                    let before = donor.lava_kg;
                    donor.move_lava_to(receiver, capacity, lava);
                    donor.fluid_velocity_mps[2][axis] +=
                        direction * speed_scale * ((before - donor.lava_kg) / available);
                }
            }
        }
        for (i, cell) in self.cells.iter_mut().enumerate() {
            if let Some([growth, min_water, optimum]) = biology {
                self.sunlight_j += cell.grow_biomass(area, growth, min_water, optimum);
            }
            let phase = cell.phase(capacity);
            fields.temperature_c[i] = phase.temperature_c;
            fields.water_m[i] = phase.liquid_kg / (1000.0 * area);
            fields.biomass[i] = cell.biomass_kg / area;
        }
    }
}

#[test]
fn physical_grid_preserves_inventory_and_accounts_for_sunlight() {
    let mut fields = GraphFields {
        grid: HeightfieldGrid::new(5, 4.0),
        temperature_c: vec![20.0; 25],
        water_m: vec![0.1; 25],
        biomass: vec![0.1; 25],
        scatter: vec![],
    };
    fields.grid.heights[0] = 2.0;
    let mut physics = PhysicsGrid::from_graph(&fields);
    physics.cells[0].oil_kg = 2.0;
    let totals = |grid: &PhysicsGrid| {
        (
            grid.cells.iter().map(|cell| cell.mass_kg()).sum::<f64>(),
            grid.cells.iter().map(|cell| cell.total_energy_j()).sum::<f64>() - grid.sunlight_j,
        )
    };
    let before = totals(&physics);
    let mut duplicate = physics.clone();
    let mut duplicate_fields = fields.clone();
    for (grid, output) in [(&mut physics, &mut fields), (&mut duplicate, &mut duplicate_fields)] {
        grid.relax_sediment(&mut output.grid, 30.0, 3);
        for _ in 0..10 {
            grid.step(output, Some([0.5, 0.01, 20.0]), 1.0 / 64.0);
        }
    }
    assert_eq!(physics, duplicate);
    assert_eq!(fields, duplicate_fields);
    assert!(fields.grid.heights[0] < 2.0);
    assert!(physics.cells[0].oil_kg < 2.0);
    assert!(physics.sunlight_j > 0.0);
    let after = totals(&physics);
    assert!((before.0 - after.0).abs() < 1e-8);
    assert!((before.1 - after.1).abs() < 1e-4);
}

impl GraphFields {
    /// Stable byte fingerprint for complete runtime/scene roundtrip verification.
    pub fn fingerprint(&self) -> u64 {
        fn mix(h:&mut u64,value:u64) {for byte in value.to_le_bytes() {*h=(*h ^ byte as u64).wrapping_mul(0x100000001b3);}}
        let mut h=0xcbf29ce484222325;
        mix(&mut h,self.grid.n as u64);mix(&mut h,self.grid.size.to_bits() as u64);
        for value in self.grid.heights.iter().chain(&self.grid.erosion_mask) {mix(&mut h,value.to_bits() as u64);}
        for value in self.temperature_c.iter().chain(&self.water_m).chain(&self.biomass) {mix(&mut h,value.to_bits());}
        mix(&mut h,self.scatter.len() as u64);
        for item in &self.scatter {
            mix(&mut h,item.kind as u64);
            for value in item.pos.to_array().into_iter().chain(item.rot.to_array()).chain([item.scale]) {mix(&mut h,value.to_bits() as u64);}
        }
        h
    }
}

/// Closed-boundary pair fluxes: donor-limited and conservative, with deterministic traversal.
pub fn flow_water(grid: &HeightfieldGrid, water: &mut [f64], conductivity: f64, iterations: u32) {
    for _ in 0..iterations {
        for j in 0..grid.n { for i in 0..grid.n {
            let a = grid.idx(i,j);
            for b in [if i+1<grid.n {Some(a+1)} else {None}, if j+1<grid.n {Some(a+grid.n)} else {None}].into_iter().flatten() {
                let difference = grid.heights[a] as f64 + water[a] - grid.heights[b] as f64 - water[b];
                let (from,to) = if difference >= 0.0 {(a,b)} else {(b,a)};
                let amount = (difference.abs()*conductivity).min(water[from]*0.25);
                water[from] -= amount; water[to] += amount;
            }
        }}
    }
}

pub fn diffuse_temperature(n: usize, temperature: &mut [f64], conductivity: f64) {
    for j in 0..n { for i in 0..n {
        let a=j*n+i;
        for b in [if i+1<n {Some(a+1)} else {None}, if j+1<n {Some(a+n)} else {None}].into_iter().flatten() {
            let flux=(temperature[b]-temperature[a])*conductivity;
            temperature[a]+=flux; temperature[b]-=flux;
        }
    }}
}

/// Evaluate only ancestors of `target`; editor-only disconnected nodes do not run implicitly.
/// A bounded workload prevents imported graphs from allocating one terrain per unbounded node.
pub fn evaluate_world_graph(graph: &StudioGraph, spec: &WorldSpec, target: Option<u32>) -> Result<GraphFields,String> {
    evaluate_world_graph_with_biology(graph, spec, target).map(|(fields, _)| fields)
}

pub(crate) fn evaluate_world_graph_with_biology(
    graph: &StudioGraph,
    spec: &WorldSpec,
    target: Option<u32>,
) -> Result<(GraphFields, Option<[f64; 3]>), String> {
    use std::collections::{BTreeMap,BTreeSet};
    let order=graph.validate()?;
    let target=target.or_else(|| order.last().copied()).ok_or("empty world graph")?;
    if !graph.nodes.iter().any(|n| n.id==target) { return Err("missing evaluation target".into()); }
    let mut needed=BTreeSet::from([target]);
    for &id in order.iter().rev() { if needed.contains(&id) { for e in graph.edges.iter().filter(|e| e.to==id) {needed.insert(e.from);} } }
    if !(5..=513).contains(&spec.resolution) || !spec.size.is_finite() || !(1.0..=4096.0).contains(&spec.size) { return Err("invalid evaluator grid dimensions".into()); }
    let geometry=[spec.base_height,spec.mountain_height,spec.max_height,spec.lake_center[0],spec.lake_center[1],spec.lake_radius,spec.lake_depth,spec.water_level,spec.camp_center[0],spec.camp_center[1],spec.clearing_radius];
    if geometry.iter().any(|x| !x.is_finite() || x.abs()>1e6) || spec.max_height<=0.0 { return Err("invalid evaluator world geometry".into()); }
    let cells=spec.resolution as u64*spec.resolution as u64;
    if cells*needed.len() as u64>4_194_304 { return Err("graph field allocation budget exceeded".into()); }
    let mut droplets=0u64; let mut iterations=0u64;
    for node in graph.nodes.iter().filter(|n|needed.contains(&n.id)) {
        if let NodeParameters::Scatter {spacing_m,..}=node.params {
            let side=(spec.size as f64*std::f64::consts::SQRT_2/spacing_m as f64).ceil();
            if side*side>4_194_304.0 {return Err("scatter allocation budget exceeded".into());}
        }
        match node.params { NodeParameters::HydraulicErosion {droplets:d,thermal_iterations,..} => {droplets+=d as u64;iterations+=thermal_iterations as u64;}, NodeParameters::FluidFlow {iterations:i,..} => iterations+=i as u64, _=>{} }
    }
    if droplets>2_000_000 || cells*iterations>32_000_000 {return Err("graph iteration budget exceeded".into());}
    let mut consumers=BTreeMap::<u32,usize>::new();
    for e in graph.edges.iter().filter(|e|needed.contains(&e.to)) {*consumers.entry(e.from).or_insert(0)+=1;}
    let mut outputs=BTreeMap::<u32, (GraphFields, Option<[f64; 3]>)>::new();
    for id in order.into_iter().filter(|id|needed.contains(id)) {
        let node=graph.nodes.iter().find(|n|n.id==id).unwrap();
        if node.kind.event_pipeline() {return Err("event nodes require an effect, not world evaluation".into());}
        let (mut state, mut biology)=if let NodeParameters::Noise {seed,frequency,warp}=node.params {
            let mut source=spec.clone(); source.seed=seed; source.noise_frequency=frequency; source.warp_amount=warp;
            let grid=HeightfieldGrid::generate(&source);
            let count=grid.heights.len();
            let water_m=grid.heights.iter().map(|h|(spec.water_level as f64-*h as f64).max(0.0)).collect();
            (GraphFields {grid,temperature_c:vec![15.0;count],water_m,biomass:vec![0.0;count],scatter:vec![]}, None)
        } else {
            let input=graph.edges.iter().find(|e|e.to==id).ok_or_else(||format!("node {id} has no input"))?;
            // the last consumer takes the fields by move; fan-out keeps cloning
            let left=consumers.get_mut(&input.from).ok_or("missing evaluated input")?;
            *left-=1;
            if *left==0 {outputs.remove(&input.from).ok_or("missing evaluated input")?} else {outputs.get(&input.from).ok_or("missing evaluated input")?.clone()}
        };
        match node.params {
            NodeParameters::Noise {..} => {},
            NodeParameters::HydraulicErosion {seed,droplets,thermal_iterations} => {
                state.grid.erode_hydraulic(droplets,seed as u64);
                state.grid.erode_thermal(thermal_iterations,35.0);
                state.grid.clamp_heights(spec.max_height);
                for (water,h) in state.water_m.iter_mut().zip(&state.grid.heights) {*water=(spec.water_level as f64-*h as f64).max(0.0);}
            }
            NodeParameters::Temperature {ambient_c,lapse_c_per_m,diffusion} => {
                for (t,h) in state.temperature_c.iter_mut().zip(&state.grid.heights) {*t=(ambient_c as f64-*h as f64*lapse_c_per_m as f64).max(-273.15);}
                diffuse_temperature(state.grid.n,&mut state.temperature_c,diffusion as f64);
            }
            NodeParameters::FluidFlow {rain_depth_m,conductivity,iterations} => {
                for water in &mut state.water_m {*water+=rain_depth_m as f64;}
                flow_water(&state.grid,&mut state.water_m,conductivity as f64,iterations);
            }
            NodeParameters::Biology {growth,min_water_m,optimum_c} => {
                biology = Some([growth.into(), min_water_m.into(), optimum_c.into()]);
                for i in 0..state.biomass.len() {
                    let wet=(state.water_m[i]/min_water_m as f64).clamp(0.0,1.0);
                    let warm=(1.0-(state.temperature_c[i]-optimum_c as f64).abs()/40.0).clamp(0.0,1.0);
                    state.biomass[i]=(state.biomass[i]+growth as f64*wet*warm*(1.0-state.biomass[i])).clamp(0.0,1.0);
                }
            }
            NodeParameters::Scatter {seed,density,spacing_m} => {
                let mut rng=StdRng::seed_from_u64(seed as u64);
                let points=poisson_disk(&mut rng,state.grid.size*0.5,spacing_m,30,5000,&mut |p,rng| {
                    let (i,j)=state.grid.nearest_cell(p.x,p.y); let k=state.grid.idx(i,j);
                    state.water_m[k]<0.2 && rng.gen_range(0.0..1.0)<density as f64*state.biomass[k]
                });
                state.scatter=points.into_iter().map(|p| ScatterItem {kind:ScatterKind::Pine,pos:Vec3::new(p.x,state.grid.sample_height(p.x,p.y),p.y),rot:Quat::from_rotation_y(rng.gen_range(0.0..std::f32::consts::TAU)),scale:rng.gen_range(0.8..1.5)}).collect();
            }
            _=>return Err("unsupported world node settings".into()),
        }
        if state.grid.heights.iter().any(|h|!h.is_finite()) || state.temperature_c.iter().chain(&state.water_m).chain(&state.biomass).any(|x|!x.is_finite()) {return Err("non-finite evaluator output".into());}
        outputs.insert(id,(state,biology));
    }
    outputs.remove(&target).ok_or_else(||"target was not evaluated".into())
}

#[cfg(test)]
mod world_graph_tests {
    use super::*;
    fn fixture() -> (StudioGraph,WorldSpec) {
        let mut graph=StudioGraph::template(false);
        graph.nodes[1].params=NodeParameters::HydraulicErosion {seed:7,droplets:300,thermal_iterations:2};
        let spec=WorldSpec {resolution:17,size:32.0,lake_radius:1.0,lake_center:[100.0,100.0],camp_center:[100.0,100.0],water_level:-100.0,..Default::default()};
        (graph,spec)
    }
    #[test]
    fn v1_import_maps_values_and_rejects_mismatched_v2_settings() {
        let old=r#"(version:1,nodes:[(id:1,kind:Noise,position:(0.0,0.0),value:2.0)],edges:[])"#;
        let graph=ron::from_str::<StudioGraph>(old).unwrap().migrate().unwrap();
        assert_eq!(graph.version,2);
        let mut erosion=StudioGraph::template(false); erosion.version=1; erosion.nodes[1].value=60000.0;
        assert!(matches!(erosion.migrate().unwrap().nodes[1].params,NodeParameters::HydraulicErosion {droplets:60000,..}));
        assert!(matches!(graph.nodes[0].params,NodeParameters::Noise {frequency:2.0,..}));
        assert_eq!(ron::from_str::<StudioGraph>(&ron::to_string(&graph).unwrap()).unwrap(),graph);
        let mut bad=graph; bad.nodes[0].params=NodeParameters::for_kind(StudioNodeKind::Scatter);
        assert!(bad.validate().is_err());
    }
    #[test]
    fn world_nodes_run_deterministically_and_parameters_affect_outputs() {
        let (mut graph,spec)=fixture();
        let a=evaluate_world_graph(&graph,&spec,None).unwrap();
        let b=evaluate_world_graph(&graph,&spec,None).unwrap();
        assert_eq!(a.grid.heights,b.grid.heights); assert_eq!(a.water_m,b.water_m);
        assert_eq!(a.temperature_c,b.temperature_c); assert_eq!(a.biomass,b.biomass);
        assert_eq!(a.scatter.iter().map(|x|x.pos).collect::<Vec<_>>(),b.scatter.iter().map(|x|x.pos).collect::<Vec<_>>());
        assert!(a.biomass.iter().any(|x|*x>0.0)); assert!(!a.scatter.is_empty());
        let noise=evaluate_world_graph(&graph,&spec,Some(1)).unwrap();
        let erosion=evaluate_world_graph(&graph,&spec,Some(2)).unwrap();
        assert_ne!(noise.grid.heights,erosion.grid.heights);
        graph.nodes[0].params=NodeParameters::Noise {seed:77,frequency:3.0,warp:0.12};
        assert_ne!(noise.grid.heights,evaluate_world_graph(&graph,&spec,Some(1)).unwrap().grid.heights);
        graph.nodes[2].params=NodeParameters::Temperature {ambient_c:-10.0,lapse_c_per_m:0.0,diffusion:0.0};
        assert!(evaluate_world_graph(&graph,&spec,Some(3)).unwrap().temperature_c.iter().all(|t|*t == -10.0));
        graph.nodes[5].params=NodeParameters::Scatter {seed:1,density:0.0,spacing_m:4.0};
        assert!(evaluate_world_graph(&graph,&spec,None).unwrap().scatter.is_empty());
    }
    #[test]
    fn pair_fluxes_preserve_water_and_heat_and_inputs_are_bounded() {
        let (graph,spec)=fixture();
        let field=evaluate_world_graph(&graph,&spec,Some(4)).unwrap();
        assert!((field.water_m.iter().sum::<f64>()-289.0*(0.05f32 as f64)).abs()<1e-9);
        assert!(field.water_m.iter().all(|x|*x>=0.0));
        let mut t=vec![0.0;25]; t[12]=100.0;
        for _ in 0..100 {diffuse_temperature(5,&mut t,0.2);}
        assert!((t.iter().sum::<f64>()-100.0).abs()<1e-10);
        let mut invalid=spec.clone(); invalid.resolution=1;
        assert!(evaluate_world_graph(&graph,&invalid,None).is_err());
        invalid=spec.clone(); invalid.size=4096.0;
        let mut large=graph.clone(); large.nodes[5].params=NodeParameters::Scatter {seed:1,density:1.0,spacing_m:0.5};
        assert!(evaluate_world_graph(&large,&invalid,None).is_err());
        let mut missing=graph; missing.edges.retain(|e|e.to!=3);
        assert!(evaluate_world_graph(&missing,&spec,None).is_err());
    }
}

#[cfg(test)]
mod integration_graph_tests {
    use super::*;
    #[test]
    fn templates_roundtrip_and_reject_invalid_topology() {
        for events in [false, true] {
            let mut g = StudioGraph::template(events);
            assert_eq!(g.validate().unwrap().len(), g.nodes.len());
            assert_eq!(ron::from_str::<StudioGraph>(&ron::to_string(&g).unwrap()).unwrap(), g);
            g.edges.push(g.edges[0]); assert!(g.validate().is_err());
        }
        let mut g = StudioGraph::template(false);
        g.edges = vec![StudioGraphEdge { from: 2, to: 3 }, StudioGraphEdge { from: 3, to: 2 }];
        assert!(g.validate().is_err());
        g.edges.clear(); g.nodes[0].value = f32::NAN; assert!(g.validate().is_err());
    }
}
use std::collections::HashSet;
use std::hash::Hash;

use bevy::prelude::{EulerRot, Quat, Reflect, ReflectDefault, ReflectResource, Resource, Vec2, Vec3};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};

pub const SPLAT_LAYERS: usize = 6; // sand, gravel/shore, grass, moss, rock, snow

pub fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

// ───────────────────────────── WorldSpec ─────────────────────────────

#[derive(Reflect, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum QualityPreset {
    Low,
    #[default]
    Medium,
    High,
    Ultra,
}

#[derive(Reflect, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum WorldPreset {
    MistyDuskLake,
    SunnyAlpineNoon,
    MidnightCampfire,
    GoldenAutumnDawn,
}

#[derive(Resource, Reflect, Serialize, Deserialize, Clone, PartialEq, Debug)]
#[reflect(Resource, Default, Clone)]
pub struct WorldSpec {
    /// false = `Empty Grid Scene`: no procedural terrain, lake or scatter; a flat infinite grid at Y = 0 instead.
    #[serde(default = "default_true")]
    pub terrain_enabled: bool,
    // terrain
    pub seed: u32,
    pub size: f32,
    pub resolution: u32,
    pub base_height: f32,
    pub mountain_height: f32,
    pub max_height: f32,
    pub noise_frequency: f32,
    pub warp_amount: f32,
    // erosion
    pub erosion_droplets: u32,
    pub thermal_iterations: u32,
    // lake
    pub lake_center: [f32; 2],
    pub lake_radius: f32,
    pub lake_depth: f32,
    pub water_level: f32,
    pub shore_foam: f32,
    // camp
    pub camp_center: [f32; 2],
    pub clearing_radius: f32,
    // atmosphere
    pub time_of_day: f32,
    pub auto_sun: bool,
    pub sun_speed: f32, // hours per second while auto_sun
    pub sun_intensity: f32,
    pub fog_enabled: bool,
    pub fog_color: [f32; 3],
    pub fog_density: f32,
    pub god_rays: bool,
    pub bloom: bool,
    pub reflections: bool,
    pub shadows: bool,
    pub quality_preset: QualityPreset,
    // biome
    pub forest_density: f32,
    pub boulder_density: f32,
    pub grass_density: f32,
}

fn default_true() -> bool {
    true
}

impl Default for WorldSpec {
    fn default() -> Self {
        Self {
            terrain_enabled: true,
            seed: 1337,
            size: 256.0,
            resolution: 257,
            base_height: 18.0,
            mountain_height: 60.0,
            max_height: 90.0,
            noise_frequency: 3.0,
            warp_amount: 0.12,
            erosion_droplets: 60_000,
            thermal_iterations: 24,
            lake_center: [-20.0, 14.0],
            lake_radius: 32.0,
            lake_depth: 5.0,
            water_level: 6.0,
            shore_foam: 0.8,
            camp_center: [38.0, -22.0],
            clearing_radius: 9.0,
            time_of_day: 17.5,
            auto_sun: false, // true = time_of_day advances automatically
            sun_speed: 0.15,
            sun_intensity: 1.0,
            fog_enabled: true,
            fog_color: [0.62, 0.68, 0.78],
            fog_density: 0.004,
            god_rays: true,
            bloom: true,
            reflections: true,
            shadows: true,
            quality_preset: QualityPreset::High,
            forest_density: 0.6,
            boulder_density: 0.4,
            grass_density: 0.5,
        }
    }
}

impl WorldSpec {
    pub fn from_preset(p: WorldPreset) -> Self {
        let mut s = Self::default();
        match p {
            WorldPreset::MistyDuskLake => {
                s.time_of_day = 20.8;
                s.fog_density = 0.007;
                s.fog_color = [0.52, 0.56, 0.68];
                s.sun_intensity = 0.7;
                s.forest_density = 0.7;
                s.shore_foam = 0.6;
            }
            WorldPreset::SunnyAlpineNoon => {
                s.time_of_day = 13.5;
                s.fog_density = 0.0018;
                s.fog_color = [0.70, 0.80, 0.95];
                s.mountain_height = 75.0;
                s.forest_density = 0.4;
                s.grass_density = 0.7;
            }
            WorldPreset::MidnightCampfire => {
                s.time_of_day = 23.5;
                s.fog_density = 0.006;
                s.fog_color = [0.03, 0.04, 0.09];
                s.sun_intensity = 0.4;
                s.god_rays = false;
                s.forest_density = 0.8;
            }
            WorldPreset::GoldenAutumnDawn => {
                s.time_of_day = 6.8;
                s.fog_density = 0.007;
                s.fog_color = [0.95, 0.72, 0.50];
                s.sun_intensity = 0.9;
                s.boulder_density = 0.5;
            }
        }
        s
    }

    /// Clamp all fields to legal ranges (used after UI / BRP edits).
    pub fn sanitize(&mut self) {
        self.time_of_day = self.time_of_day.rem_euclid(24.0);
        self.resolution = self.resolution.clamp(33, 1025);
        self.size = self.size.clamp(32.0, 4096.0);
        for v in [&mut self.forest_density, &mut self.boulder_density, &mut self.grass_density, &mut self.shore_foam] {
            *v = v.clamp(0.0, 1.0);
        }
    }
}

// ───────────────────────────── Heightfield ─────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BrushMode {
    Raise,
    Lower,
    Smooth,
    Flatten,
}

#[derive(Clone, Copy, Debug)]
pub struct Splat {
    pub weights: [f32; SPLAT_LAYERS],
    pub albedo: [f32; 3],
    pub roughness: f32,
}

/// Disjoint coverage intervals for stochastic rendering of all six normalized splat weights.
pub fn splat_intervals(weights: [f32; SPLAT_LAYERS]) -> [[f32; 2]; SPLAT_LAYERS] {
    let mut end = 0.0;
    std::array::from_fn(|i| {
        let start = end;
        end = if i + 1 == SPLAT_LAYERS { 1.0 } else { (end + weights[i]).clamp(start, 1.0) };
        [start, end]
    })
}

#[derive(Reflect, Serialize, Deserialize, Clone, Debug, PartialEq)]
#[reflect(Clone)]
pub struct HeightfieldGrid {
    pub n: usize,
    pub size: f32,
    pub heights: Vec<f32>,
    pub erosion_mask: Vec<f32>,
}

/// Perlin fBm (value ≈ [-1,1]).
/// Lake radius along direction `rel` (from the lake centre), distorted by multi-octave noise (±35 %).
fn lake_shore_radius(np: &noise::Perlin, spec: &WorldSpec, rel: Vec2) -> f32 {
    let dir = rel.try_normalize().unwrap_or(Vec2::X);
    let n = fbm(np, dir.x as f64 * 1.6 + 40.0, dir.y as f64 * 1.6 + 40.0, 3);
    spec.lake_radius * (1.0 + n * 0.35 * 1.6).clamp(0.6, 1.5)
}

impl WorldSpec {
    /// Camp position: on the shore of the lake, in the direction of `camp_center` seen from `lake_center`.
    pub fn camp_pos(&self) -> Vec2 {
        let lake = Vec2::from(self.lake_center);
        let dir = (Vec2::from(self.camp_center) - lake).try_normalize().unwrap_or(Vec2::X);
        let np = noise::Perlin::new(self.seed);
        lake + dir * (lake_shore_radius(&np, self, dir) * 1.5 + self.clearing_radius * 0.6)
    }
}

fn fbm(noise: &noise::Perlin, x: f64, y: f64, octaves: u32) -> f32 {
    use noise::NoiseFn;
    let (mut sum, mut amp, mut freq, mut norm) = (0.0, 1.0, 1.0, 0.0);
    for _ in 0..octaves {
        sum += amp * noise.get([x * freq, y * freq]);
        norm += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    (sum / norm) as f32
}

const LAYER_ALBEDO: [[f32; 3]; SPLAT_LAYERS] = [
    [0.36, 0.30, 0.20], // wet sand
    [0.52, 0.50, 0.46], // gravel
    [0.16, 0.32, 0.08], // grass
    [0.07, 0.14, 0.06], // moss
    [0.20, 0.20, 0.22], // basalt
    [0.93, 0.95, 1.00], // snow
];
const LAYER_ROUGH: [f32; SPLAT_LAYERS] = [0.35, 0.55, 0.85, 0.80, 0.75, 0.45];

impl HeightfieldGrid {
    pub fn new(n: usize, size: f32) -> Self {
        Self { n, size, heights: vec![0.0; n * n], erosion_mask: vec![0.0; n * n] }
    }

    pub fn cell(&self) -> f32 {
        self.size / (self.n as f32 - 1.0)
    }

    #[inline]
    pub fn idx(&self, i: usize, j: usize) -> usize {
        j * self.n + i
    }

    pub fn world_to_grid(&self, x: f32, z: f32) -> (f32, f32) {
        ((x / self.size + 0.5) * (self.n as f32 - 1.0), (z / self.size + 0.5) * (self.n as f32 - 1.0))
    }

    pub fn grid_to_world(&self, i: usize, j: usize) -> (f32, f32) {
        let k = self.size / (self.n as f32 - 1.0);
        (i as f32 * k - self.size * 0.5, j as f32 * k - self.size * 0.5)
    }

    /// Domain-warped fBm valley with lake bowl, camp clearing and path.
    pub fn generate(spec: &WorldSpec) -> Self {
        let n = spec.resolution as usize;
        let mut g = Self::new(n, spec.size);
        let np = noise::Perlin::new(spec.seed);
        let (o1, o2) = (17.3_f64, 91.7_f64);
        let s = spec.size;
        let lake_c = Vec2::from(spec.lake_center);
        let camp_c = spec.camp_pos();
        let lake_floor = spec.water_level - spec.lake_depth;
        // camp height: raw terrain height at the camp centre, lifted above water
        let raw = |x: f32, z: f32| -> f32 {
            let p = Vec2::new(x, z) / s;
            let f = spec.noise_frequency as f64;
            let w = |ox: f64| fbm(&np, (p.x as f64) * f + ox, (p.y as f64) * f + ox * 0.5, 4);
            let q = p + spec.warp_amount * Vec2::new(w(o1), w(o2));
            let r = p.length() * 2.0;
            let qf = |m: f64| fbm(&np, (q.x as f64) * f * m, (q.y as f64) * f * m, 6);
            let base = 0.5 + 0.5 * qf(1.0);
            let ring = smoothstep(0.55, 0.95, r);
            base * spec.base_height + ring * spec.mountain_height * (0.6 + 0.4 * qf(2.0))
        };
        let h_camp = raw(camp_c.x, camp_c.y).max(spec.water_level + 1.5).min(spec.base_height + 6.0);
        let (px0, pamp, pk, pphi) = (camp_c.x, 14.0, 0.035, 0.7);
        for j in 0..n {
            for i in 0..n {
                let (x, z) = g.grid_to_world(i, j);
                let pos = Vec2::new(x, z);
                let mut h = raw(x, z);
                // organic shoreline: distance rescaled by the angular radius distortion
                let rel = pos - lake_c;
                let d = rel.length() * spec.lake_radius / lake_shore_radius(&np, spec, rel);
                // everywhere outside the lake stays above the waterline (smooth max, no second lake)
                let floor = spec.water_level + 1.0;
                let k = 1.5;
                // Stable softplus: low waterlines must not overflow exp and poison interpolation.
                let q = (h - floor) / k;
                h = floor + k * (q.max(0.0) + (-q.abs()).exp().ln_1p());
                let bowl = 1.0 - smoothstep(0.0, spec.lake_radius, d);
                h -= bowl * spec.lake_depth;
                let shallow = smoothstep(spec.lake_radius, spec.lake_radius * 1.35, d);
                h = lerp(lake_floor + 0.3, h, shallow);
                // path from the camp, sinusoidal centreline in x over z
                let xc = px0 + pamp * (pk * z + pphi).sin();
                let wp = 2.0;
                let wpath = (1.0 - smoothstep(wp, 2.0 * wp, (x - xc).abs())) * (1.0 - smoothstep(0.0, 70.0, (z - camp_c.y).abs()));
                h = lerp(h, h_camp, 0.7 * wpath);
                // camp clearing (applied last so it stays flat)
                let w = 1.0 - smoothstep(spec.clearing_radius, spec.clearing_radius * 1.6, pos.distance(camp_c));
                h = lerp(h, h_camp, w);
                let idx = g.idx(i, j);
                g.heights[idx] = h.clamp(0.0, spec.max_height);
            }
        }
        g
    }

    pub fn clamp_heights(&mut self, max_h: f32) {
        for h in &mut self.heights {
            *h = h.clamp(0.0, max_h);
        }
    }

    // ── sampling ──

    fn h_at(&self, i: i32, j: i32) -> f32 {
        let m = self.n as i32 - 1;
        self.heights[self.idx(i.clamp(0, m) as usize, j.clamp(0, m) as usize)]
    }

    /// Bilinear height in grid coordinates.
    pub fn height_grid(&self, gx: f32, gz: f32) -> f32 {
        let m = self.n as f32 - 1.0;
        let (gx, gz) = (gx.clamp(0.0, m), gz.clamp(0.0, m));
        let (i, j) = (gx.floor() as i32, gz.floor() as i32);
        let (fx, fz) = (gx - i as f32, gz - j as f32);
        let a = lerp(self.h_at(i, j), self.h_at(i + 1, j), fx);
        let b = lerp(self.h_at(i, j + 1), self.h_at(i + 1, j + 1), fx);
        lerp(a, b, fz)
    }

    pub fn sample_height(&self, x: f32, z: f32) -> f32 {
        let (gx, gz) = self.world_to_grid(x, z);
        self.height_grid(gx, gz)
    }

    pub fn sample_normal(&self, x: f32, z: f32) -> Vec3 {
        let e = self.cell();
        let dx = self.sample_height(x + e, z) - self.sample_height(x - e, z);
        let dz = self.sample_height(x, z + e) - self.sample_height(x, z - e);
        Vec3::new(-dx / (2.0 * e), 1.0, -dz / (2.0 * e)).normalize()
    }

    fn grad_cells(&self, gx: f32, gz: f32) -> (f32, f32) {
        let (i, j) = (gx.floor() as i32, gz.floor() as i32);
        let (fx, fz) = (gx - i as f32, gz - j as f32);
        let (h00, h10, h01, h11) = (self.h_at(i, j), self.h_at(i + 1, j), self.h_at(i, j + 1), self.h_at(i + 1, j + 1));
        (lerp(h10 - h00, h11 - h01, fz), lerp(h01 - h00, h11 - h10, fx))
    }

    // ── erosion ──

    pub fn erode_hydraulic(&mut self, droplets: u32, seed: u64) {
        const INERTIA: f32 = 0.05;
        const K_C: f32 = 4.0;
        const MIN_SLOPE: f32 = 0.01;
        const K_E: f32 = 0.3;
        const K_D: f32 = 0.3;
        const K_V: f32 = 0.01;
        const G: f32 = 4.0;
        const LIFE: u32 = 30;
        const R: i32 = 3;
        let mut rng = StdRng::seed_from_u64(seed);
        let lim = self.n as f32 - 3.0;
        // radial erosion brush is identical for every droplet: weigh it once
        let mut wsum = 0.0;
        for b in -R..=R {
            for a in -R..=R {
                wsum += (1.0 - ((a * a + b * b) as f32).sqrt() / R as f32).max(0.0);
            }
        }
        let mut brush = Vec::new();
        for b in -R..=R {
            for a in -R..=R {
                let w = (1.0 - ((a * a + b * b) as f32).sqrt() / R as f32).max(0.0) / wsum;
                if w > 0.0 {
                    brush.push((a, b, w));
                }
            }
        }
        for _ in 0..droplets {
            let (mut px, mut pz) = (rng.gen_range(1.0..lim), rng.gen_range(1.0..lim));
            let (mut dx, mut dz, mut speed, mut water, mut sed) = (0.0_f32, 0.0_f32, 1.0_f32, 1.0_f32, 0.0_f32);
            for _ in 0..LIFE {
                let h_old = self.height_grid(px, pz);
                let (gx, gz) = self.grad_cells(px, pz);
                dx = dx * INERTIA - gx * (1.0 - INERTIA);
                dz = dz * INERTIA - gz * (1.0 - INERTIA);
                let len = (dx * dx + dz * dz).sqrt();
                if len < 1e-6 {
                    let a = rng.gen_range(0.0..std::f32::consts::TAU);
                    dx = a.cos();
                    dz = a.sin();
                } else {
                    dx /= len;
                    dz /= len;
                }
                let (ox, oz) = (px, pz);
                px += dx;
                pz += dz;
                if px < 1.0 || pz < 1.0 || px > lim || pz > lim {
                    break;
                }
                let dh = self.height_grid(px, pz) - h_old;
                let cap = (-dh).max(MIN_SLOPE) * speed * water * K_C;
                if sed > cap || dh > 0.0 {
                    let dep = if dh > 0.0 { dh.min(sed) } else { (sed - cap) * K_D };
                    sed -= dep;
                    let (i, j) = (ox.floor() as usize, oz.floor() as usize);
                    let (fx, fz) = (ox - i as f32, oz - j as f32);
                    let n = self.n;
                    self.heights[j * n + i] += dep * (1.0 - fx) * (1.0 - fz);
                    self.heights[j * n + i + 1] += dep * fx * (1.0 - fz);
                    self.heights[(j + 1) * n + i] += dep * (1.0 - fx) * fz;
                    self.heights[(j + 1) * n + i + 1] += dep * fx * fz;
                } else {
                    let ero = ((cap - sed) * K_E).min(-dh);
                    let (ci, cj) = (ox.round() as i32, oz.round() as i32);
                    let n = self.n as i32;
                    for &(a, b, w) in &brush {
                        let (ii, jj) = (ci + a, cj + b);
                        if ii >= 0 && jj >= 0 && ii < n && jj < n {
                            let k = (jj * n + ii) as usize;
                            self.heights[k] -= ero * w;
                            self.erosion_mask[k] += ero * w;
                        }
                    }
                    sed += ero;
                }
                speed = (speed * speed + dh * G).max(0.0).sqrt();
                water *= 1.0 - K_V;
            }
        }
    }

    pub fn erode_thermal(&mut self, iterations: u32, talus_deg: f32) {
        let n = self.n;
        let c = self.cell();
        let tan_t = talus_deg.to_radians().tan();
        let neighbours: [(i32, i32, f32); 8] = std::array::from_fn(|k| {
            let (di, dj) = [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)][k];
            (di, dj, c * ((di * di + dj * dj) as f32).sqrt())
        });
        let mut delta = vec![0.0_f32; n * n];
        for _ in 0..iterations {
            delta.fill(0.0);
            for j in 0..n {
                for i in 0..n {
                    let hc = self.heights[j * n + i];
                    for &(di, dj, dist) in &neighbours {
                        let (ii, jj) = (i as i32 + di, j as i32 + dj);
                        if ii < 0 || jj < 0 || ii >= n as i32 || jj >= n as i32 {
                            continue;
                        }
                        let k = jj as usize * n + ii as usize;
                        let dh = hc - self.heights[k];
                        if dh / dist > tan_t {
                            let m = 0.5 * (dh - dist * tan_t) * 0.5;
                            delta[j * n + i] -= m * 0.5; // 8 neighbours share the flow
                            delta[k] += m * 0.5;
                        }
                    }
                }
            }
            for (h, d) in self.heights.iter_mut().zip(&delta) {
                *h += d;
            }
        }
    }

    // ── splatmap ──

    pub fn splat_weights(&self, i: usize, j: usize, spec: &WorldSpec) -> [f32; SPLAT_LAYERS] {
        let (x, z) = self.grid_to_world(i, j);
        let h = self.heights[self.idx(i, j)];
        let nrm = self.sample_normal(x, z);
        let slope = 1.0 - nrm.y;
        let d = h - spec.water_level;
        let h_n = h / spec.max_height;
        let moss_mask = (self.erosion_mask[self.idx(i, j)] * 1.5).clamp(0.0, 1.0);
        let w_sand = (1.0 - smoothstep(0.0, 0.6, d)) * if d > -0.3 { 1.0 } else { 0.0 };
        let w_shore = smoothstep(0.0, 0.3, d) * (1.0 - smoothstep(0.6, 1.4, d));
        let w_rock = smoothstep(0.32, 0.42, slope);
        let w_snow = smoothstep(0.78, 0.9, h_n + 0.05) * (1.0 - w_rock * 0.7);
        let w_moss = moss_mask * (1.0 - w_rock);
        let max_o = w_sand.max(w_shore).max(w_rock).max(w_snow).max(w_moss);
        let w_grass = 1.0 - max_o;
        let mut w = [w_sand, w_shore, w_grass, w_moss, w_rock, w_snow];
        let sum: f32 = w.iter().sum::<f32>().max(1e-6);
        for v in &mut w {
            *v /= sum;
        }
        w
    }

    pub fn splat_at(&self, i: usize, j: usize, spec: &WorldSpec) -> Splat {
        let weights = self.splat_weights(i, j, spec);
        let d = self.heights[self.idx(i, j)] - spec.water_level;
        let mut albedo = [0.0; 3];
        let mut roughness = 0.0;
        for (l, w) in weights.iter().enumerate() {
            let mut a = LAYER_ALBEDO[l];
            if l == 1 {
                let foam = (1.0 - smoothstep(0.0, 0.15, d.abs())) * spec.shore_foam;
                for c in 0..3 {
                    a[c] = lerp(a[c], [0.90, 0.92, 0.92][c], foam);
                }
            }
            for c in 0..3 {
                albedo[c] += w * a[c];
            }
            roughness += w * LAYER_ROUGH[l];
        }
        Splat { weights, albedo, roughness }
    }

    pub fn build_splatmap(&self, spec: &WorldSpec) -> Vec<Splat> {
        let mut out = Vec::with_capacity(self.n * self.n);
        for j in 0..self.n {
            for i in 0..self.n {
                out.push(self.splat_at(i, j, spec));
            }
        }
        out
    }

    pub(crate) fn nearest_cell(&self, x: f32, z: f32) -> (usize, usize) {
        let (gx, gz) = self.world_to_grid(x, z);
        let m = self.n as f32 - 1.0;
        (gx.round().clamp(0.0, m) as usize, gz.round().clamp(0.0, m) as usize)
    }

    // ── brushes ──

    /// Apply one dab. Returns dirty rectangle (i0, j0, i1, j1) inclusive.
    pub fn apply_brush(&mut self, mode: BrushMode, x: f32, z: f32, radius: f32, strength: f32, dt: f32, flatten_target: f32) -> (usize, usize, usize, usize) {
        let (cx, cz) = self.world_to_grid(x, z);
        let rg = radius / self.cell();
        let m = self.n as i32 - 1;
        let i0 = ((cx - rg).floor() as i32).clamp(0, m);
        let i1 = ((cx + rg).ceil() as i32).clamp(0, m);
        let j0 = ((cz - rg).floor() as i32).clamp(0, m);
        let j1 = ((cz + rg).ceil() as i32).clamp(0, m);
        let mut updates = Vec::with_capacity(((i1 - i0 + 1) * (j1 - j0 + 1)) as usize);
        for j in j0..=j1 {
            for i in i0..=i1 {
                let (wx, wz) = self.grid_to_world(i as usize, j as usize);
                let d = ((wx - x).powi(2) + (wz - z).powi(2)).sqrt();
                let f = 1.0 - smoothstep(0.0, radius, d);
                if f <= 0.0 {
                    continue;
                }
                let h = self.h_at(i, j);
                let nh = match mode {
                    BrushMode::Raise => h + strength * f * dt,
                    BrushMode::Lower => h - strength * f * dt,
                    BrushMode::Smooth => {
                        let mut s = 0.0;
                        for b in -1..=1 {
                            for a in -1..=1 {
                                s += self.h_at(i + a, j + b);
                            }
                        }
                        h + (s / 9.0 - h) * (strength * f).min(1.0)
                    }
                    BrushMode::Flatten => h + (flatten_target - h) * (strength * f).min(1.0),
                };
                updates.push((self.idx(i as usize, j as usize), nh));
            }
        }
        for (k, v) in updates {
            self.heights[k] = v;
        }
        (i0 as usize, j0 as usize, i1 as usize, j1 as usize)
    }

    // ── raycast ──

    /// March along the ray then refine with 10 bisection steps. Returns world hit point.
    pub fn raycast(&self, origin: Vec3, dir: Vec3) -> Option<Vec3> {
        let dir = dir.normalize_or_zero();
        if dir == Vec3::ZERO {
            return None;
        }
        let half = self.size * 0.5;
        let step = self.cell() * 0.5;
        let t_max = self.size * 3.0;
        let inside = |p: Vec3| p.x.abs() <= half && p.z.abs() <= half;
        let above = |p: Vec3| p.y - self.sample_height(p.x, p.z);
        let mut t_prev = 0.0;
        let mut prev_above: Option<bool> = None;
        let mut t = 0.0;
        while t <= t_max {
            let p = origin + dir * t;
            if inside(p) {
                let a = above(p) > 0.0;
                if let Some(pa) = prev_above {
                    if pa && !a {
                        let (mut lo, mut hi) = (t_prev, t);
                        for _ in 0..10 {
                            let mid = 0.5 * (lo + hi);
                            if above(origin + dir * mid) > 0.0 {
                                lo = mid;
                            } else {
                                hi = mid;
                            }
                        }
                        let q = origin + dir * (0.5 * (lo + hi));
                        return Some(Vec3::new(q.x, self.sample_height(q.x, q.z), q.z));
                    }
                } else if !a {
                    return None; // starts below ground
                }
                prev_above = Some(a);
            } else {
                prev_above = None;
            }
            t_prev = t;
            t += step;
        }
        None
    }
}

// ───────────────────────────── Scatter ─────────────────────────────

#[derive(Reflect, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScatterKind {
    Pine,
    Boulder,
    Grass,
    Reed,
    /// Wooden pier: `pos` = shore end (deck height), local +X runs towards the lake, `scale` = length in metres.
    WoodenPier,
    /// Fire ring, logs, benches and tent around the camp centre.
    CampfireSite,
}

#[derive(Reflect, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct ScatterItem {
    pub kind: ScatterKind,
    pub pos: Vec3,
    pub rot: Quat,
    pub scale: f32,
}

/// Bridson Poisson-disk sampling over a square [-half, half]². `accept` filters candidates.
pub fn poisson_disk(rng: &mut StdRng, half: f32, r: f32, k: u32, max_pts: usize, accept: &mut dyn FnMut(Vec2, &mut StdRng) -> bool) -> Vec<Vec2> {
    let cell = r / std::f32::consts::SQRT_2;
    let gn = ((2.0 * half / cell).ceil() as usize).max(1);
    let mut grid = vec![-1_i32; gn * gn];
    let to_cell = |p: Vec2| (((p.x + half) / cell) as usize).min(gn - 1);
    let to_cell_z = |p: Vec2| (((p.y + half) / cell) as usize).min(gn - 1);
    let mut pts: Vec<Vec2> = Vec::new();
    let mut active: Vec<usize> = Vec::new();
    let mut tries = 0;
    while pts.is_empty() && tries < 200 {
        tries += 1;
        let p = Vec2::new(rng.gen_range(-half..half), rng.gen_range(-half..half));
        if accept(p, rng) {
            grid[to_cell_z(p) * gn + to_cell(p)] = 0;
            pts.push(p);
            active.push(0);
        }
    }
    while !active.is_empty() && pts.len() < max_pts {
        let ai = rng.gen_range(0..active.len());
        let base = pts[active[ai]];
        let mut found = false;
        for _ in 0..k {
            let ang = rng.gen_range(0.0..std::f32::consts::TAU);
            let rad = rng.gen_range(r..2.0 * r);
            let c = base + Vec2::new(ang.cos(), ang.sin()) * rad;
            if c.x.abs() >= half || c.y.abs() >= half {
                continue;
            }
            let (ci, cj) = (to_cell(c) as i32, to_cell_z(c) as i32);
            let mut ok = true;
            'n: for b in (cj - 2).max(0)..=(cj + 2).min(gn as i32 - 1) {
                for a in (ci - 2).max(0)..=(ci + 2).min(gn as i32 - 1) {
                    let g = grid[b as usize * gn + a as usize];
                    if g >= 0 && pts[g as usize].distance(c) < r {
                        ok = false;
                        break 'n;
                    }
                }
            }
            if ok && accept(c, rng) {
                grid[cj as usize * gn + ci as usize] = pts.len() as i32;
                active.push(pts.len());
                pts.push(c);
                found = true;
                break;
            }
        }
        if !found {
            active.swap_remove(ai);
        }
    }
    pts
}

fn ground_rot(normal: Vec3, yaw: f32, tilt: f32) -> Quat {
    Quat::from_rotation_arc(Vec3::Y, Vec3::Y.lerp(normal, tilt).normalize()) * Quat::from_rotation_y(yaw)
}

pub fn generate_scatter_plan(spec: &WorldSpec, grid: &HeightfieldGrid) -> Vec<ScatterItem> {
    if !spec.terrain_enabled {
        return Vec::new();
    }
    let mut rng = StdRng::seed_from_u64(spec.seed as u64 ^ 0x9E37_79B9);
    let half = grid.size * 0.5 - 2.0;
    let camp = spec.camp_pos();
    let lake = Vec2::from(spec.lake_center);
    let mut items: Vec<ScatterItem> = Vec::new();

    let slope_ok = |x: f32, z: f32| 1.0 - grid.sample_normal(x, z).y < 0.35;
    let weights_at = |p: Vec2| {
        let (i, j) = grid.nearest_cell(p.x, p.y);
        grid.splat_weights(i, j, spec)
    };

    // pines: grass/moss density mask, away from camp and water
    let r_pine = lerp(14.0, 4.5, spec.forest_density);
    let mut pines: Vec<Vec2> = Vec::new();
    if spec.forest_density > 0.0 {
        pines = poisson_disk(&mut rng, half, r_pine, 30, 2500, &mut |p, rng| {
            let h = grid.sample_height(p.x, p.y);
            if !slope_ok(p.x, p.y) || h < spec.water_level + 0.2 || p.distance(camp) < spec.clearing_radius * 1.5 {
                return false;
            }
            if p.distance(lake) < spec.lake_radius * 1.2 {
                return false;
            }
            let w = weights_at(p);
            rng.gen_range(0.0..1.0) < (w[2] + w[3]).clamp(0.05, 1.0)
        });
        for p in &pines {
            let n = grid.sample_normal(p.x, p.y);
            let s = rng.gen_range(0.8..1.5);
            items.push(ScatterItem { kind: ScatterKind::Pine, pos: Vec3::new(p.x, grid.sample_height(p.x, p.y), p.y), rot: ground_rot(n, rng.gen_range(0.0..std::f32::consts::TAU), 0.15), scale: s });
        }
    }

    // boulders: prefer rock weight, spacing against pines
    if spec.boulder_density > 0.0 {
        let r_b = lerp(22.0, 8.0, spec.boulder_density);
        let pts = poisson_disk(&mut rng, half, r_b, 30, 400, &mut |p, rng| {
            let h = grid.sample_height(p.x, p.y);
            if h < spec.water_level - 0.3 || p.distance(camp) < spec.clearing_radius * 1.2 {
                return false;
            }
            if pines.iter().any(|q| q.distance(p) < r_b.max(r_pine) * 0.35) {
                return false;
            }
            let w = weights_at(p);
            rng.gen_range(0.0..1.0) < 0.2 + w[4]
        });
        for p in pts {
            let n = grid.sample_normal(p.x, p.y);
            items.push(ScatterItem { kind: ScatterKind::Boulder, pos: Vec3::new(p.x, grid.sample_height(p.x, p.y) - 0.15, p.y), rot: ground_rot(n, rng.gen_range(0.0..std::f32::consts::TAU), 0.6), scale: rng.gen_range(0.6..2.4) });
        }
    }

    // grass tufts
    if spec.grass_density > 0.0 {
        let r_g = lerp(4.0, 1.4, spec.grass_density);
        let pts = poisson_disk(&mut rng, half, r_g, 20, 4000, &mut |p, rng| {
            let h = grid.sample_height(p.x, p.y);
            if h < spec.water_level + 0.6 || !slope_ok(p.x, p.y) {
                return false;
            }
            let w = weights_at(p);
            rng.gen_range(0.0..1.0) < w[2]
        });
        for p in pts {
            let n = grid.sample_normal(p.x, p.y);
            items.push(ScatterItem { kind: ScatterKind::Grass, pos: Vec3::new(p.x, grid.sample_height(p.x, p.y), p.y), rot: ground_rot(n, rng.gen_range(0.0..std::f32::consts::TAU), 0.5), scale: rng.gen_range(0.6..1.3) });
        }
    }

    // reeds along the shoreline
    let pts = poisson_disk(&mut rng, half, 1.6, 20, 600, &mut |p, _| {
        let d = grid.sample_height(p.x, p.y) - spec.water_level;
        (-0.3..0.5).contains(&d)
    });
    for p in pts {
        items.push(ScatterItem { kind: ScatterKind::Reed, pos: Vec3::new(p.x, grid.sample_height(p.x, p.y), p.y), rot: Quat::from_rotation_y(rng.gen_range(0.0..std::f32::consts::TAU)), scale: rng.gen_range(0.8..1.6) });
    }

    // pier: from the camp-side shore towards the lake centre until the water is deep enough
    let to_camp = (camp - lake).normalize_or_zero();
    let dir = if to_camp == Vec2::ZERO { Vec2::X } else { to_camp };
    let mut shore: Option<f32> = None;
    let mut s = spec.lake_radius * 1.6;
    let mut end = None;
    while s > 0.0 {
        let p = lake + dir * s;
        let d = grid.sample_height(p.x, p.y) - spec.water_level;
        if shore.is_none() && d < 0.35 {
            shore = Some(s);
        }
        if let Some(s0) = shore {
            if d < -2.6 || s0 - s > 20.0 {
                end = Some(s);
                break;
            }
        }
        s -= 0.5;
    }
    if let (Some(s0), Some(s1)) = (shore, end) {
        let p = lake + dir * (s0 + 1.2); // start slightly on land
        let yaw = dir.y.atan2(-dir.x);
        items.push(ScatterItem { kind: ScatterKind::WoodenPier, pos: Vec3::new(p.x, spec.water_level + 0.55, p.y), rot: Quat::from_rotation_y(yaw), scale: (s0 - s1 + 1.2).max(10.0) });
    }

    // camp site (one composite prefab)
    let cy = grid.sample_height(camp.x, camp.y);
    items.push(ScatterItem { kind: ScatterKind::CampfireSite, pos: Vec3::new(camp.x, cy, camp.y), rot: Quat::IDENTITY, scale: 1.0 });
    items
}

// ───────────────────────────── Gizmo math (doc 02) ─────────────────────────────

/// Constant screen-space handle size.
pub fn calc_handle_size(eye: Vec3, origin: Vec3, cam_forward: Vec3, fov_y: f32) -> f32 {
    (eye - origin).dot(cam_forward).abs() * (fov_y * 0.5).tan() * 0.16
}

fn ray_plane(o: Vec3, d: Vec3, plane_o: Vec3, n: Vec3) -> Option<f32> {
    let denom = d.dot(n);
    if denom.abs() < 1e-8 {
        return None;
    }
    let s = (plane_o - o).dot(n) / denom;
    (s > 0.0).then_some(s)
}

/// Parameter along `axis` where the ray meets the camera-facing helper plane. `None` in the dead zone.
pub fn axis_hit_param(origin: Vec3, axis: Vec3, view_dir: Vec3, ray_o: Vec3, ray_d: Vec3) -> Option<f32> {
    if axis.dot(view_dir).abs() > 0.96 {
        return None;
    }
    let n = axis.cross(view_dir).cross(axis).normalize_or_zero();
    if n == Vec3::ZERO {
        return None;
    }
    let s = ray_plane(ray_o, ray_d, origin, n)?;
    Some((ray_o + ray_d * s - origin).dot(axis))
}

/// New distance along the axis relative to drag start: `t_hit_curr - grab_offset`.
pub fn axis_drag_delta(origin: Vec3, axis: Vec3, view_dir: Vec3, ray_o: Vec3, ray_d: Vec3, grab_offset: f32, snap: Option<f32>) -> Option<f32> {
    let t = axis_hit_param(origin, axis, view_dir, ray_o, ray_d)? - grab_offset;
    Some(match snap {
        Some(step) if step > 0.0 => (t / step).round() * step,
        _ => t,
    })
}

/// Point where the ray meets the drag plane (normal `n`). Rejects grazing planes.
pub fn plane_drag_point(origin: Vec3, normal: Vec3, view_dir: Vec3, ray_o: Vec3, ray_d: Vec3) -> Option<Vec3> {
    if normal.dot(view_dir).abs() < 0.08 {
        return None;
    }
    let s = ray_plane(ray_o, ray_d, origin, normal)?;
    Some(ray_o + ray_d * s)
}

/// Signed angle swept around `axis` from `v_start` (unit, in ring plane) to the current ray hit.
pub fn disc_rotation_angle(center: Vec3, axis: Vec3, view_dir: Vec3, ray_o: Vec3, ray_d: Vec3, v_start: Vec3, snap_deg: Option<f32>) -> Option<f32> {
    if axis.dot(view_dir).abs() < 0.08 {
        return None; // edge-on: caller uses tangent drag
    }
    let s = ray_plane(ray_o, ray_d, center, axis)?;
    let v = (ray_o + ray_d * s - center).normalize_or_zero();
    if v == Vec3::ZERO {
        return None;
    }
    let mut a = axis.dot(v_start.cross(v)).atan2(v_start.dot(v));
    if let Some(step) = snap_deg {
        let st = step.to_radians();
        a = (a / st).round() * st;
    }
    Some(a)
}

/// Wrap an angle into (-π, π].
pub fn wrap_angle(a: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    let w = (a + PI).rem_euclid(TAU) - PI;
    if w <= -PI { PI } else { w }
}


// ───────────────────────────── Selection & snapping (doc 03) ─────────────────────────────

/// Normalised marquee rectangle (min, max) in pixels.
pub fn normalize_rect(a: Vec2, b: Vec2) -> (Vec2, Vec2) {
    (a.min(b), a.max(b))
}

pub fn rect_contains(r: (Vec2, Vec2), p: Vec2) -> bool {
    p.x >= r.0.x && p.x <= r.1.x && p.y >= r.0.y && p.y <= r.1.y
}

/// Marquee result: none → rect, Shift → union, Ctrl → symmetric difference (Ctrl wins over Shift).
pub fn combine_selection<T: Clone + Eq + Hash>(prev: &HashSet<T>, rect: &HashSet<T>, shift: bool, ctrl: bool) -> HashSet<T> {
    if ctrl {
        prev.symmetric_difference(rect).cloned().collect()
    } else if shift {
        prev.union(rect).cloned().collect()
    } else {
        rect.clone()
    }
}

/// Final rotation `q = from_rotation_arc(Y, normalize(lerp(Y, n, w))) * Rot_Y(yaw)`.
pub fn surface_align_rotation(normal: Vec3, yaw: f32, weight: f32) -> Quat {
    let n = Vec3::Y.lerp(normal, weight.clamp(0.0, 1.0)).normalize_or_zero();
    let n = if n == Vec3::ZERO { Vec3::Y } else { n };
    Quat::from_rotation_arc(Vec3::Y, n) * Quat::from_rotation_y(yaw)
}

pub fn yaw_of(q: Quat) -> f32 {
    q.to_euler(EulerRot::YXZ).0
}

/// Scale-gizmo ratio of the current axis parameter to the grab parameter, kept positive and bounded.
pub fn scale_factor(t_curr: f32, t_grab: f32) -> f32 {
    if t_grab.abs() < 1e-4 {
        return 1.0;
    }
    (t_curr / t_grab).clamp(0.05, 50.0)
}

/// Distance (px) from point `p` to segment `a`–`b`.
pub fn dist_point_segment(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let l2 = ab.length_squared();
    let t = if l2 < 1e-6 { 0.0 } else { ((p - a).dot(ab) / l2).clamp(0.0, 1.0) };
    p.distance(a + ab * t)
}

/// Point inside a convex quad (any winding).
pub fn point_in_quad(p: Vec2, q: [Vec2; 4]) -> bool {
    let mut sign = 0.0_f32;
    for i in 0..4 {
        let (a, b) = (q[i], q[(i + 1) % 4]);
        let c = (b - a).perp_dot(p - a);
        if c.abs() < 1e-6 {
            continue;
        }
        if sign == 0.0 {
            sign = c.signum();
        } else if sign != c.signum() {
            return false;
        }
    }
    true
}

// ───────────────────────────── Undo history (doc 05) ─────────────────────────────

#[derive(Clone, Debug)]
pub struct UndoEntry<S> {
    pub label: String,
    pub before: S,
    pub after: S,
}

#[derive(Clone, Debug)]
struct OpenTxn<S> {
    label: String,
    before: S,
    preview: S,
}

/// Transactional history: Begin → UpdatePreview* → Commit collapses into one entry.
#[derive(Resource, Debug, Clone)]
pub struct UndoHistory<S: Clone + PartialEq + Send + Sync + 'static> {
    undo: Vec<UndoEntry<S>>,
    redo: Vec<UndoEntry<S>>,
    open: Option<OpenTxn<S>>,
    pub limit: usize,
}

impl<S: Clone + PartialEq + Send + Sync + 'static> Default for UndoHistory<S> {
    fn default() -> Self {
        Self { undo: Vec::new(), redo: Vec::new(), open: None, limit: 200 }
    }
}

impl<S: Clone + PartialEq + Send + Sync + 'static> UndoHistory<S> {
    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }
    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }
    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }
    pub fn preview(&self) -> Option<&S> {
        self.open.as_ref().map(|t| &t.preview)
    }

    /// Begin (or restart) a transaction with a snapshot of the target.
    pub fn begin(&mut self, label: impl Into<String>, before: S) {
        self.cancel();
        self.open = Some(OpenTxn { label: label.into(), preview: before.clone(), before });
    }

    /// Live preview value; never touches the stacks. Returns false if no transaction is open.
    pub fn update_preview(&mut self, value: S) -> bool {
        match &mut self.open {
            Some(t) => {
                t.preview = value;
                true
            }
            None => false,
        }
    }

    /// Commit with the final state. Returns true if an entry was pushed (false when nothing changed).
    pub fn commit(&mut self, after: S) -> bool {
        let Some(t) = self.open.take() else { return false };
        if t.before == after {
            return false;
        }
        self.undo.push(UndoEntry { label: t.label, before: t.before, after });
        self.redo.clear();
        if self.undo.len() > self.limit {
            let extra = self.undo.len() - self.limit;
            self.undo.drain(..extra);
        }
        true
    }

    /// Abort the open transaction; returns the snapshot to restore.
    pub fn cancel(&mut self) -> Option<S> {
        self.open.take().map(|t| t.before)
    }

    /// One-shot transaction (begin + commit).
    pub fn record(&mut self, label: impl Into<String>, before: S, after: S) -> bool {
        self.begin(label, before);
        self.commit(after)
    }

    /// Pop the latest entry; returns the state to apply. An open transaction is cancelled first.
    pub fn undo(&mut self) -> Option<S> {
        self.cancel();
        let e = self.undo.pop()?;
        let s = e.before.clone();
        self.redo.push(e);
        Some(s)
    }

    /// Drop the redo stack (a fresh edit in any other history invalidates it).
    pub fn clear_redo(&mut self) {
        self.redo.clear();
    }

    pub fn redo(&mut self) -> Option<S> {
        self.cancel();
        let e = self.redo.pop()?;
        let s = e.after.clone();
        self.undo.push(e);
        Some(s)
    }
}

// ───────────────────────────── Unit scale (feet ↔ world) ─────────────────────────────

/// Rules are expressed in feet; the world is in metres. `world_per_foot` links them
/// (`REAL` = exact conversion, `GAME` = 1 ft is 1 world unit, so 5 ft = 5 m).
#[derive(Resource, Reflect, Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
#[reflect(Resource)]
pub struct UnitScale {
    pub world_per_foot: f32,
}

impl UnitScale {
    pub const REAL: Self = Self { world_per_foot: 0.3048 };
    pub const GAME: Self = Self { world_per_foot: 1.0 };

    pub fn feet_to_world(&self, feet: f32) -> f32 {
        feet * self.world_per_foot
    }

    pub fn world_to_feet(&self, world: f32) -> f32 {
        world / self.world_per_foot
    }
}

impl Default for UnitScale {
    fn default() -> Self {
        Self::GAME
    }
}

// ───────────────────────────── Tests ─────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_scale_modes() {
        assert!((UnitScale::REAL.feet_to_world(5.0) - 1.524).abs() < 1e-5);
        assert_eq!(UnitScale::GAME.feet_to_world(5.0), 5.0);
        for s in [UnitScale::REAL, UnitScale::GAME] {
            assert!((s.world_to_feet(s.feet_to_world(30.0)) - 30.0).abs() < 1e-4);
        }
    }

    fn small_spec() -> WorldSpec {
        WorldSpec { resolution: 129, erosion_droplets: 0, thermal_iterations: 0, ..WorldSpec::default() }
    }

    #[test]
    fn erosion_changes_terrain_and_thermal_relaxes_slopes() {
        let spec = small_spec();
        let base = HeightfieldGrid::generate(&spec);
        let mut g = base.clone();
        g.erode_hydraulic(20_000, 7);
        g.clamp_heights(spec.max_height);
        assert!(g.erosion_mask.iter().sum::<f32>() > 0.0);
        assert!(g.heights.iter().zip(&base.heights).any(|(a, b)| (a - b).abs() > 1e-3));
        assert!(g.heights.iter().all(|h| h.is_finite() && *h >= 0.0));
        let max_slope = |g: &HeightfieldGrid| {
            let n = g.n;
            let mut m = 0.0_f32;
            for j in 0..n {
                for i in 0..n - 1 {
                    m = m.max((g.heights[j * n + i + 1] - g.heights[j * n + i]).abs() / g.cell());
                }
            }
            m
        };
        let mut t = base.clone();
        t.erode_thermal(30, 35.0);
        assert!(max_slope(&t) <= max_slope(&base) + 1e-4);
    }

    #[test]
    fn raycast_hits_terrain_and_splat_normalised() {
        let spec = small_spec();
        let g = HeightfieldGrid::generate(&spec);
        let hit = g.raycast(Vec3::new(10.0, 300.0, -5.0), Vec3::new(0.2, -1.0, 0.1)).expect("hit");
        assert!((hit.y - g.sample_height(hit.x, hit.z)).abs() < 0.05);
        assert!(g.raycast(Vec3::new(0.0, 300.0, 0.0), Vec3::Y).is_none());
        for (i, j) in [(10, 10), (64, 64), (100, 30)] {
            let s = g.splat_at(i, j, &spec);
            assert!((s.weights.iter().sum::<f32>() - 1.0).abs() < 1e-4);
        }
        let plan = generate_scatter_plan(&spec, &g);
        assert!(plan.iter().any(|i| i.kind == ScatterKind::Pine) && plan.iter().any(|i| i.kind == ScatterKind::CampfireSite));
    }

    #[test]
    fn gizmo_trigonometry() {
        let eye = Vec3::new(0.0, 0.0, 10.0);
        let hs = calc_handle_size(eye, Vec3::ZERO, Vec3::NEG_Z, 1.0);
        assert!((hs - 10.0 * 0.5_f32.tan() * 0.16).abs() < 1e-5);
        // axis X drag: ray straight down -Z at x=3 → delta 3 minus grab offset 1
        let d = axis_drag_delta(Vec3::ZERO, Vec3::X, Vec3::NEG_Z, Vec3::new(3.0, 0.0, 10.0), Vec3::NEG_Z, 1.0, None).unwrap();
        assert!((d - 2.0).abs() < 1e-4);
        assert!(axis_drag_delta(Vec3::ZERO, Vec3::Z, Vec3::NEG_Z, eye, Vec3::NEG_Z, 0.0, None).is_none());
        let p = plane_drag_point(Vec3::ZERO, Vec3::Z, Vec3::NEG_Z, Vec3::new(2.0, -1.0, 10.0), Vec3::NEG_Z).unwrap();
        assert!((p - Vec3::new(2.0, -1.0, 0.0)).length() < 1e-4);
        // disc: start on +X, ray hits +Y → +90° about Z
        let a = disc_rotation_angle(Vec3::ZERO, Vec3::Z, Vec3::NEG_Z, Vec3::new(0.0, 4.0, 10.0), Vec3::NEG_Z, Vec3::X, None).unwrap();
        assert!((a - std::f32::consts::FRAC_PI_2).abs() < 1e-4);
        assert!((wrap_angle(3.0 * std::f32::consts::PI) - std::f32::consts::PI).abs() < 1e-4);
    }

    #[test]
    fn selection_and_snap_math() {
        let prev: HashSet<u32> = [1, 2].into();
        let rect: HashSet<u32> = [2, 3].into();
        assert_eq!(combine_selection(&prev, &rect, false, false), rect);
        assert_eq!(combine_selection(&prev, &rect, true, false), [1, 2, 3].into());
        assert_eq!(combine_selection(&prev, &rect, true, true), [1, 3].into());
        let r = normalize_rect(Vec2::new(10.0, 40.0), Vec2::new(2.0, 5.0));
        assert!(rect_contains(r, Vec2::new(5.0, 20.0)) && !rect_contains(r, Vec2::new(11.0, 20.0)));
        let q = surface_align_rotation(Vec3::new(1.0, 1.0, 0.0).normalize(), 0.7, 1.0);
        assert!((q * Vec3::Y - Vec3::new(1.0, 1.0, 0.0).normalize()).length() < 1e-4);
        assert!((yaw_of(Quat::from_rotation_y(0.7)) - 0.7).abs() < 1e-5);
        assert!((dist_point_segment(Vec2::new(5.0, 3.0), Vec2::ZERO, Vec2::new(10.0, 0.0)) - 3.0).abs() < 1e-5);
        assert!(point_in_quad(Vec2::splat(0.5), [Vec2::ZERO, Vec2::X, Vec2::ONE, Vec2::Y]) && !point_in_quad(Vec2::splat(2.0), [Vec2::ZERO, Vec2::X, Vec2::ONE, Vec2::Y]));
        assert!((scale_factor(2.0, 1.0) - 2.0).abs() < 1e-6);
    }

    #[test]
    fn undo_transaction_collapses_drag() {
        let mut h: UndoHistory<f32> = UndoHistory::default();
        h.begin("slider", 1.0);
        for v in [1.1, 1.5, 2.0, 3.0] {
            assert!(h.update_preview(v));
        }
        assert_eq!(h.undo_len(), 0);
        assert!(h.commit(3.0));
        assert_eq!(h.undo_len(), 1);
        assert_eq!(h.undo(), Some(1.0));
        assert_eq!(h.redo(), Some(3.0));
        // no-op transaction is discarded
        h.begin("noop", 3.0);
        assert!(!h.commit(3.0));
        assert_eq!(h.undo_len(), 1);
    }

    #[test]
    fn undo_cancel_limit_and_redo_clear() {
        let mut h: UndoHistory<i32> = UndoHistory { limit: 3, ..Default::default() };
        for k in 0..5 {
            h.record("t", k, k + 1);
        }
        assert_eq!(h.undo_len(), 3);
        h.begin("open", 5);
        h.update_preview(99);
        assert_eq!(h.undo(), Some(4)); // cancels open txn, then undoes top entry
        assert!(!h.is_open());
        assert_eq!(h.redo_len(), 1);
        h.record("new", 4, 10);
        assert_eq!(h.redo_len(), 0);
        let spec = WorldSpec::from_preset(WorldPreset::MidnightCampfire);
        let json = serde_json::to_string(&spec).unwrap();
        assert_eq!(serde_json::from_str::<WorldSpec>(&json).unwrap(), spec);
    }
}
