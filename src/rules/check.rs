use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DegreeOfSuccess {
    CriticalFailure,
    Failure,
    Success,
    CriticalSuccess,
}

impl DegreeOfSuccess {
    pub fn raised(self) -> Self {
        match self {
            Self::CriticalFailure => Self::Failure,
            Self::Failure => Self::Success,
            _ => Self::CriticalSuccess,
        }
    }

    pub fn lowered(self) -> Self {
        match self {
            Self::CriticalSuccess => Self::Success,
            Self::Success => Self::Failure,
            _ => Self::CriticalFailure,
        }
    }
}

/// Degree from the total vs DC, then adjusted by natural 20 / natural 1.
pub fn degree_of_success(natural: u8, total: i32, dc: i32) -> DegreeOfSuccess {
    let base = if total >= dc + 10 {
        DegreeOfSuccess::CriticalSuccess
    } else if total >= dc {
        DegreeOfSuccess::Success
    } else if total <= dc - 10 {
        DegreeOfSuccess::CriticalFailure
    } else {
        DegreeOfSuccess::Failure
    };
    match natural {
        20 => base.raised(),
        1 => base.lowered(),
        _ => base,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CheckResult {
    pub natural: u8,
    pub total: i32,
    pub dc: i32,
    pub degree: DegreeOfSuccess,
}

/// Seedable dice roller.
pub struct Dice(StdRng);

impl Dice {
    pub fn new(seed: u64) -> Self {
        Self(StdRng::seed_from_u64(seed))
    }

    pub fn d20(&mut self) -> u8 {
        self.0.gen_range(1..=20)
    }

    pub fn roll(&mut self, count: u32, sides: u32) -> i32 {
        (0..count).map(|_| self.0.gen_range(1..=sides) as i32).sum()
    }

    pub fn check(&mut self, modifier: i32, dc: i32) -> CheckResult {
        let natural = self.d20();
        let total = natural as i32 + modifier;
        CheckResult { natural, total, dc, degree: degree_of_success(natural, total, dc) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use DegreeOfSuccess::*;

    #[test]
    fn four_degrees() {
        assert_eq!(degree_of_success(10, 30, 20), CriticalSuccess);
        assert_eq!(degree_of_success(10, 20, 20), Success);
        assert_eq!(degree_of_success(10, 19, 20), Failure);
        assert_eq!(degree_of_success(10, 10, 20), CriticalFailure);
        assert_eq!(degree_of_success(10, 11, 20), Failure);
    }

    #[test]
    fn nat_20_and_1_shift() {
        assert_eq!(degree_of_success(20, 15, 20), Success);
        assert_eq!(degree_of_success(20, 5, 20), Failure);
        assert_eq!(degree_of_success(20, 25, 20), CriticalSuccess);
        assert_eq!(degree_of_success(1, 25, 20), Failure);
        assert_eq!(degree_of_success(1, 35, 20), Success);
        assert_eq!(degree_of_success(1, 5, 20), CriticalFailure);
    }

    #[test]
    fn seed_is_deterministic() {
        let (mut a, mut b) = (Dice::new(42), Dice::new(42));
        for _ in 0..50 {
            assert_eq!(a.d20(), b.d20());
        }
        let mut d = Dice::new(7);
        for _ in 0..200 {
            assert!((1..=20).contains(&d.d20()));
            assert!((1..=8).contains(&d.roll(1, 8)));
        }
    }
}
