#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionError {
    NotEnoughActions,
    NoReaction,
    OutOfRange,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TurnActionEconomy {
    pub actions: u8,
    pub reaction: bool,
}

impl Default for TurnActionEconomy {
    fn default() -> Self {
        Self { actions: 3, reaction: true }
    }
}

impl TurnActionEconomy {
    pub fn spend(&mut self, n: u8) -> Result<(), ActionError> {
        if self.actions < n {
            return Err(ActionError::NotEnoughActions);
        }
        self.actions -= n;
        Ok(())
    }

    pub fn spend_reaction(&mut self) -> Result<(), ActionError> {
        if !self.reaction {
            return Err(ActionError::NoReaction);
        }
        self.reaction = false;
        Ok(())
    }
}

/// Stride: 1 action to move up to `speed_ft`. Nothing is spent on failure.
pub fn stride(econ: &mut TurnActionEconomy, distance_ft: f32, speed_ft: f32) -> Result<(), ActionError> {
    if distance_ft > speed_ft {
        return Err(ActionError::OutOfRange);
    }
    econ.spend(1)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MultipleAttackPenalty {
    pub attacks_made: u8,
}

impl MultipleAttackPenalty {
    /// Penalty for the next attack: 0, -5, -10 (agile: 0, -4, -8).
    pub fn penalty(&self, agile: bool) -> i32 {
        let step = if agile { 4 } else { 5 };
        -(step * self.attacks_made.min(2) as i32)
    }

    pub fn record_attack(&mut self) {
        self.attacks_made = self.attacks_made.saturating_add(1);
    }

    pub fn reset(&mut self) {
        self.attacks_made = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_values() {
        let mut m = MultipleAttackPenalty::default();
        let seq: Vec<_> = (0..4)
            .map(|_| {
                let p = (m.penalty(false), m.penalty(true));
                m.record_attack();
                p
            })
            .collect();
        assert_eq!(seq, vec![(0, 0), (-5, -4), (-10, -8), (-10, -8)]);
        m.reset();
        assert_eq!(m.penalty(false), 0);
    }

    #[test]
    fn economy() {
        let mut e = TurnActionEconomy::default();
        assert_eq!((e.actions, e.reaction), (3, true));
        assert!(e.spend(2).is_ok());
        assert_eq!(e.spend(2), Err(ActionError::NotEnoughActions));
        assert!(e.spend_reaction().is_ok());
        assert_eq!(e.spend_reaction(), Err(ActionError::NoReaction));
    }

    #[test]
    fn stride_limits() {
        let mut e = TurnActionEconomy::default();
        assert_eq!(stride(&mut e, 30.0, 25.0), Err(ActionError::OutOfRange));
        assert_eq!(e.actions, 3);
        assert!(stride(&mut e, 25.0, 25.0).is_ok());
        assert_eq!(e.actions, 2);
    }
}
