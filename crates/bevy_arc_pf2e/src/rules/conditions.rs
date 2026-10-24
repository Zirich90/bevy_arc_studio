#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Conditions {
    pub frightened: u8,
    pub off_guard: bool,
    pub prone: bool,
}

impl Conditions {
    /// Status penalty applied to all checks and DCs.
    pub fn check_penalty(&self) -> i32 {
        -(self.frightened as i32)
    }

    /// Circumstance penalty to attack rolls.
    pub fn attack_penalty(&self) -> i32 {
        if self.prone { -2 } else { 0 }
    }

    /// Off-Guard (implied by Prone) gives -2 AC.
    pub fn ac_penalty(&self) -> i32 {
        if self.off_guard || self.prone { -2 } else { 0 }
    }

    /// Frightened drops by 1 at the end of the creature's turn.
    pub fn end_turn(&mut self) {
        self.frightened = self.frightened.saturating_sub(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frightened_decays() {
        let mut c = Conditions { frightened: 2, ..Default::default() };
        assert_eq!(c.check_penalty(), -2);
        c.end_turn();
        assert_eq!(c.check_penalty(), -1);
        c.end_turn();
        c.end_turn();
        assert_eq!(c.check_penalty(), 0);
    }

    #[test]
    fn off_guard_and_prone() {
        let og = Conditions { off_guard: true, ..Default::default() };
        assert_eq!((og.ac_penalty(), og.attack_penalty()), (-2, 0));
        let pr = Conditions { prone: true, ..Default::default() };
        assert_eq!((pr.ac_penalty(), pr.attack_penalty()), (-2, -2));
        assert_eq!(Conditions::default().ac_penalty(), 0);
    }
}
