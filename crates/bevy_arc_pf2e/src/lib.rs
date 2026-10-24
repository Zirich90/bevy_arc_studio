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

/// Adds actors, trigger volumes, the tactical encounter and its UI. Requires `BevyArcStudioPlugin` to be added first.
pub struct Pf2ePlugin;

impl Plugin for Pf2ePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((actors::ActorsPlugin, encounter::EncounterPlugin, ui_combat::CombatUiPlugin));
    }
}
