use super::actions::{MultipleAttackPenalty, TurnActionEconomy};
use super::check::{CheckResult, DegreeOfSuccess, Dice};
use super::conditions::Conditions;
use super::stats::{Abilities, Ability, Hp, Proficiency, Save};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Weapon {
    pub name: &'static str,
    pub dice: u32,
    pub sides: u32,
    pub agile: bool,
}

impl Weapon {
    pub const LONGSWORD: Weapon = Weapon { name: "Longsword", dice: 1, sides: 8, agile: false };
    pub const DAGGER: Weapon = Weapon { name: "Dagger", dice: 1, sides: 4, agile: true };
}

#[derive(Clone, Debug)]
pub struct Combatant {
    pub name: &'static str,
    pub level: i32,
    pub abilities: Abilities,
    pub hp: Hp,
    pub ac: i32,
    /// Fortitude, Reflex, Will totals.
    pub saves: [i32; 3],
    pub speed_ft: f32,
    /// Attack bonus before MAP and conditions.
    pub attack_bonus: i32,
    pub weapon: Weapon,
    pub conditions: Conditions,
    pub economy: TurnActionEconomy,
    pub map: MultipleAttackPenalty,
}

impl Combatant {
    /// Level 1 human Fighter: Str 18, Dex 14, Con 14, chain mail, longsword.
    pub fn fighter() -> Self {
        let abilities = Abilities::new(18, 14, 14, 10, 12, 10);
        let level = 1;
        let save = |p: Proficiency, a: Ability| p.bonus(level) + abilities.modifier(a);
        Self {
            name: "Fighter",
            level,
            // ancestry 8 + class 10 + Con
            hp: Hp::new(8 + 10 + abilities.modifier(Ability::Con)),
            // 10 + Dex (capped at +1 by chain mail) + trained armor + 4 armor item
            ac: 10 + abilities.modifier(Ability::Dex).min(1) + Proficiency::Trained.bonus(level) + 4,
            saves: [
                save(Proficiency::Expert, Ability::Con),
                save(Proficiency::Trained, Ability::Dex),
                save(Proficiency::Trained, Ability::Wis),
            ],
            speed_ft: 25.0,
            attack_bonus: Proficiency::Expert.bonus(level) + abilities.modifier(Ability::Str),
            abilities,
            weapon: Weapon::LONGSWORD,
            conditions: Conditions::default(),
            economy: TurnActionEconomy::default(),
            map: MultipleAttackPenalty::default(),
        }
    }

    /// Goblin warrior (level -1), dagger.
    pub fn goblin() -> Self {
        Self {
            name: "Goblin Warrior",
            level: -1,
            abilities: Abilities::new(10, 16, 12, 10, 10, 8),
            hp: Hp::new(6),
            ac: 16,
            saves: [5, 5, 3],
            speed_ft: 25.0,
            attack_bonus: 7,
            weapon: Weapon::DAGGER,
            conditions: Conditions::default(),
            economy: TurnActionEconomy::default(),
            map: MultipleAttackPenalty::default(),
        }
    }

    pub fn save_bonus(&self, s: Save) -> i32 {
        self.saves[s as usize] + self.conditions.check_penalty()
    }

    pub fn effective_ac(&self) -> i32 {
        self.ac + self.conditions.ac_penalty() + self.conditions.check_penalty()
    }

    pub fn start_turn(&mut self) {
        self.economy = TurnActionEconomy::default();
        self.map.reset();
    }

    pub fn end_turn(&mut self) {
        self.conditions.end_turn();
    }

    /// Strike (1 action) applying MAP and conditions; applies damage on a hit.
    pub fn strike(&mut self, target: &mut Combatant, dice: &mut Dice) -> Option<(CheckResult, i32)> {
        self.economy.spend(1).ok()?;
        let modifier = self.attack_bonus
            + self.map.penalty(self.weapon.agile)
            + self.conditions.attack_penalty()
            + self.conditions.check_penalty();
        self.map.record_attack();
        let res = dice.check(modifier, target.effective_ac());
        let mut dmg = 0;
        if res.degree >= DegreeOfSuccess::Success {
            let crit = res.degree == DegreeOfSuccess::CriticalSuccess;
            dmg = dice.roll(self.weapon.dice, self.weapon.sides)
                + self.abilities.modifier(Ability::Str).max(0);
            if crit {
                dmg *= 2;
            }
            target.hp.take_damage(dmg, crit);
        }
        Some((res, dmg))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fighter_stats() {
        let f = Combatant::fighter();
        assert_eq!(f.hp.max, 20);
        assert_eq!(f.ac, 18);
        assert_eq!(f.attack_bonus, 9);
        assert_eq!(f.weapon, Weapon::LONGSWORD);
        assert_eq!(f.saves, [7, 5, 4]);
    }

    #[test]
    fn goblin_stats() {
        let g = Combatant::goblin();
        assert_eq!((g.hp.max, g.ac, g.speed_ft), (6, 16, 25.0));
        assert_eq!(g.weapon, Weapon::DAGGER);
        assert!(g.weapon.agile);
    }

    #[test]
    fn strike_uses_map_and_economy() {
        let mut f = Combatant::fighter();
        let mut g = Combatant::goblin();
        let mut d = Dice::new(1);
        for _ in 0..3 {
            assert!(f.strike(&mut g, &mut d).is_some());
        }
        assert!(f.strike(&mut g, &mut d).is_none());
        assert_eq!(f.map.attacks_made, 3);
        f.start_turn();
        assert_eq!(f.economy.actions, 3);
    }

    #[test]
    fn prone_and_frightened() {
        let mut g = Combatant::goblin();
        let base = g.effective_ac();
        g.conditions.prone = true;
        assert_eq!(g.effective_ac(), base - 2);
        g.conditions.frightened = 1;
        assert_eq!(g.save_bonus(Save::Will), 2);
    }
}
