//! Pathfinder 2e for Bevy ArcStudio, as a plugin on top of the universal editor.
//!
//! - [`rules`]: pure, deterministic, Bevy-free rules math (checks, degrees of success, conditions, actions).
//! - [`actors`]: reflect components, `.ron` actor templates, factions and trigger volumes (registered through the editor's
//!   `SpawnKinds`, `EditorRegistry` inspector overrides and scene-component registry).
//! - [`encounter`] / [`ui_combat`]: initiative, 3-action turns, Reactive Strike `ActionGate` and the combat UI.
//!
//! ```ignore
//! app.add_plugins(BevyArcStudioPlugin::default()).add_plugins(bevy_arc_pf2e::Pf2ePlugin);
//! ```

pub mod actors;
pub mod encounter;
pub mod rules;
pub mod ui_combat;

use bevy::prelude::*;
use bevy_arc_studio::arc_core::UnitScale;
use bevy_arc_studio::modules::{AppModuleExt, ModuleDef};

pub const MODULE_ID: &str = "pf2e";

/// Registers the `pf2e` module (actors, trigger volumes, tactical encounter and its UI). Requires `BevyArcStudioPlugin`
/// to be added first. The module starts *available*: connect it from the editor's Modules tab (or `SetModule`).
pub struct Pf2ePlugin;

impl Plugin for Pf2ePlugin {
    fn build(&self, app: &mut App) {
        let def = ModuleDef::new(MODULE_ID, "Pathfinder 2e")
            .version("0.1.0")
            .description("RON actor archetypes, factions, trigger volumes and pilot tactical combat (initiative, 3 actions, Reactive Strike). Distances in feet.")
            .on_activate(|w| *w.resource_mut::<UnitScale>() = UnitScale::REAL)
            .on_deactivate(|w| {
                encounter::reset_encounter(w);
                *w.resource_mut::<UnitScale>() = UnitScale::default();
            });
        app.register_module(def, |app| {
            app.add_plugins((actors::ActorsPlugin, encounter::EncounterPlugin, ui_combat::CombatUiPlugin));
        });
    }
}
