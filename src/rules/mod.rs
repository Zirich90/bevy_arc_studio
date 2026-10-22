//! Pathfinder 2e rules: pure, deterministic, Bevy-free. Enabled by the `pf2e` feature.

pub mod actions;
pub mod check;
pub mod conditions;
pub mod stats;
pub mod templates;

pub use actions::{ActionError, MultipleAttackPenalty, TurnActionEconomy, stride};
pub use check::{CheckResult, DegreeOfSuccess, Dice, degree_of_success};
pub use conditions::Conditions;
pub use stats::{Abilities, Ability, Hp, Proficiency, Save};
pub use templates::{Combatant, Weapon};
