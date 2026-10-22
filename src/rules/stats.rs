#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ability {
    Str,
    Dex,
    Con,
    Int,
    Wis,
    Cha,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Abilities {
    pub scores: [i32; 6],
}

impl Abilities {
    pub fn new(str_: i32, dex: i32, con: i32, int: i32, wis: i32, cha: i32) -> Self {
        Self { scores: [str_, dex, con, int, wis, cha] }
    }

    pub fn score(&self, a: Ability) -> i32 {
        self.scores[a as usize]
    }

    /// `(score - 10) / 2`, rounded down.
    pub fn modifier(&self, a: Ability) -> i32 {
        (self.score(a) - 10).div_euclid(2)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Proficiency {
    Untrained,
    Trained,
    Expert,
    Master,
    Legendary,
}

impl Proficiency {
    pub fn bonus(self, level: i32) -> i32 {
        match self {
            Self::Untrained => 0,
            Self::Trained => 2 + level,
            Self::Expert => 4 + level,
            Self::Master => 6 + level,
            Self::Legendary => 8 + level,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Save {
    Fortitude,
    Reflex,
    Will,
}

pub const MAX_DYING: u8 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hp {
    pub max: i32,
    pub current: i32,
    pub dying: u8,
    pub wounded: u8,
}

impl Hp {
    pub fn new(max: i32) -> Self {
        Self { max, current: max, dying: 0, wounded: 0 }
    }

    pub fn is_dead(&self) -> bool {
        self.dying >= MAX_DYING
    }

    pub fn is_unconscious(&self) -> bool {
        self.current == 0
    }

    /// Dropping to 0 gives Dying (1 + Wounded, or 2 + Wounded on a crit).
    /// Damage while already dying raises Dying by 1 (2 on a crit).
    pub fn take_damage(&mut self, amount: i32, critical: bool) {
        let bump = if critical { 2 } else { 1 };
        if self.current == 0 {
            if amount > 0 {
                self.dying = (self.dying + bump).min(MAX_DYING);
            }
            return;
        }
        self.current = (self.current - amount.max(0)).max(0);
        if self.current == 0 {
            self.dying = (bump + self.wounded).min(MAX_DYING);
        }
    }

    /// Healing from 0 HP ends Dying and raises Wounded.
    pub fn heal(&mut self, amount: i32) {
        if self.is_dead() || amount <= 0 {
            return;
        }
        if self.current == 0 {
            self.stabilize();
        }
        self.current = (self.current + amount).min(self.max);
    }

    /// Stabilize at 0 HP: Dying ends, Wounded rises by 1.
    pub fn stabilize(&mut self) {
        if self.dying > 0 && !self.is_dead() {
            self.dying = 0;
            self.wounded = (self.wounded + 1).min(MAX_DYING - 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ability_modifiers() {
        let a = Abilities::new(18, 14, 10, 9, 8, 7);
        assert_eq!(a.modifier(Ability::Str), 4);
        assert_eq!(a.modifier(Ability::Dex), 2);
        assert_eq!(a.modifier(Ability::Con), 0);
        assert_eq!(a.modifier(Ability::Int), -1);
        assert_eq!(a.modifier(Ability::Wis), -1);
        assert_eq!(a.modifier(Ability::Cha), -2);
    }

    #[test]
    fn proficiency_bonus() {
        assert_eq!(Proficiency::Untrained.bonus(5), 0);
        assert_eq!(Proficiency::Trained.bonus(1), 3);
        assert_eq!(Proficiency::Expert.bonus(3), 7);
        assert_eq!(Proficiency::Master.bonus(7), 13);
        assert_eq!(Proficiency::Legendary.bonus(15), 23);
    }

    #[test]
    fn dying_and_wounded() {
        let mut hp = Hp::new(20);
        hp.take_damage(25, false);
        assert_eq!((hp.current, hp.dying), (0, 1));
        hp.take_damage(3, false);
        assert_eq!(hp.dying, 2);
        hp.stabilize();
        assert_eq!((hp.dying, hp.wounded), (0, 1));
        hp.heal(5);
        assert_eq!(hp.current, 5);
        hp.take_damage(10, false);
        assert_eq!(hp.dying, 2);
        hp.take_damage(1, true);
        assert_eq!(hp.dying, 4);
        assert!(hp.is_dead());
    }

    #[test]
    fn crit_drop_to_zero() {
        let mut hp = Hp::new(10);
        hp.take_damage(10, true);
        assert_eq!(hp.dying, 2);
    }
}
