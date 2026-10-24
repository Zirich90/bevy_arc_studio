//! Pure encounter rules: Reactive Strike gate along a Stride, strike / burst resolution. Positions are world XZ metres;
//! the caller converts feet with `UnitScale`.

use bevy::math::Vec2;

use super::check::{CheckResult, DegreeOfSuccess, Dice};

/// An enemy able to answer movement / Manipulate actions with a Reactive Strike.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Threat {
    pub id: usize,
    pub pos: Vec2,
    /// Reach in world metres.
    pub reach: f32,
}

/// Reaction trigger point `dist` metres along the route.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GateStop {
    pub dist: f32,
    pub threat: usize,
}

pub fn path_length(p: &[Vec2]) -> f32 {
    p.windows(2).map(|w| w[0].distance(w[1])).sum()
}

pub fn point_at(p: &[Vec2], d: f32) -> Vec2 {
    let mut left = d.max(0.0);
    for w in p.windows(2) {
        let l = w[0].distance(w[1]);
        if left <= l {
            return if l > 1e-6 { w[0].lerp(w[1], left / l) } else { w[0] };
        }
        left -= l;
    }
    p.last().copied().unwrap_or(Vec2::ZERO)
}

/// Sub-polyline between two distances along `p` (both ends included).
pub fn slice_path(p: &[Vec2], from: f32, to: f32) -> Vec<Vec2> {
    let mut out = vec![point_at(p, from)];
    let mut acc = 0.0;
    for w in p.windows(2) {
        acc += w[0].distance(w[1]);
        if acc > from + 1e-4 && acc < to - 1e-4 {
            out.push(w[1]);
        }
    }
    let end = point_at(p, to);
    if out.last().is_none_or(|l| l.distance(end) > 1e-4) {
        out.push(end);
    }
    out
}

const SAMPLE: f32 = 0.05;

/// Points where the route leaves a threat's reach, sorted along the route. A 5-ft Step never provokes (`step = true`).
pub fn stride_triggers(path: &[Vec2], threats: &[Threat], step: bool) -> Vec<GateStop> {
    if step || path.len() < 2 {
        return Vec::new();
    }
    let total = path_length(path);
    let mut stops = Vec::new();
    for t in threats {
        let inside = |d: f32| point_at(path, d).distance(t.pos) <= t.reach;
        let mut was_inside = inside(0.0);
        let mut last_in = 0.0;
        let mut d = SAMPLE;
        loop {
            let dd = d.min(total);
            let now = inside(dd);
            if now {
                last_in = dd;
            } else if was_inside {
                stops.push(GateStop { dist: last_in, threat: t.id });
                was_inside = false;
            }
            was_inside |= now;
            if dd >= total {
                break;
            }
            d += SAMPLE;
        }
    }
    stops.sort_by(|a, b| a.dist.total_cmp(&b.dist));
    stops
}

/// Threats whose reach covers `pos` (a Manipulate action there provokes them).
pub fn manipulate_triggers(pos: Vec2, threats: &[Threat]) -> Vec<usize> {
    threats.iter().filter(|t| t.pos.distance(pos) <= t.reach).map(|t| t.id).collect()
}

/// A single attack roll with damage.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AttackOutcome {
    pub check: CheckResult,
    pub damage: i32,
}

impl AttackOutcome {
    pub fn hit(&self) -> bool {
        self.check.degree >= DegreeOfSuccess::Success
    }

    pub fn crit(&self) -> bool {
        self.check.degree == DegreeOfSuccess::CriticalSuccess
    }
}

/// d20 + `modifier` vs `ac`; hits roll `count`d`sides` + `bonus` (doubled on a critical hit).
pub fn attack(dice: &mut Dice, modifier: i32, ac: i32, count: u32, sides: u32, bonus: i32) -> AttackOutcome {
    let check = dice.check(modifier, ac);
    let mut damage = 0;
    if check.degree >= DegreeOfSuccess::Success {
        damage = (dice.roll(count, sides) + bonus).max(1);
        if check.degree == DegreeOfSuccess::CriticalSuccess {
            damage *= 2;
        }
    }
    AttackOutcome { check, damage }
}

/// Basic-save damage: crit success none, success half, failure full, crit failure double.
pub fn basic_save_damage(degree: DegreeOfSuccess, rolled: i32) -> i32 {
    match degree {
        DegreeOfSuccess::CriticalSuccess => 0,
        DegreeOfSuccess::Success => rolled / 2,
        DegreeOfSuccess::Failure => rolled,
        DegreeOfSuccess::CriticalFailure => rolled * 2,
    }
}

/// Outcome of a gate check on the mover.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Continue,
    /// Critical Reactive Strike: the action is stopped where it stands.
    Interrupt,
    /// Mover dropped to 0 HP.
    Down,
}

pub fn verdict(out: &AttackOutcome, mover_hp_after: i32) -> Verdict {
    if mover_hp_after <= 0 {
        Verdict::Down
    } else if out.crit() {
        Verdict::Interrupt
    } else {
        Verdict::Continue
    }
}

/// Incremental executor of a gated Stride: walk to the next stop, resolve, walk on.
#[derive(Clone, Debug)]
pub struct GatedMove {
    pub total: f32,
    pub stops: Vec<GateStop>,
    pub cursor: usize,
    pub reached: f32,
    pub finished: bool,
    pub interrupted: bool,
}

impl GatedMove {
    pub fn new(total: f32, stops: Vec<GateStop>) -> Self {
        Self { total, stops, cursor: 0, reached: 0.0, finished: total <= 0.0, interrupted: false }
    }

    /// Distance to walk to next: the next gate or the end of the route.
    pub fn next_target(&self) -> f32 {
        self.stops.get(self.cursor).map_or(self.total, |s| s.dist)
    }

    /// Walker arrived at `next_target`. At a gate, `gate` yields the verdict; at the end the move completes.
    pub fn arrive(&mut self, gate: impl FnOnce(GateStop) -> Verdict) {
        self.reached = self.next_target();
        match self.stops.get(self.cursor).copied() {
            Some(stop) => {
                self.cursor += 1;
                match gate(stop) {
                    Verdict::Continue => {}
                    Verdict::Interrupt => {
                        self.interrupted = true;
                        self.finished = true;
                    }
                    Verdict::Down => self.finished = true,
                }
            }
            None => self.finished = true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use DegreeOfSuccess::*;

    fn line(len: f32) -> Vec<Vec2> {
        vec![Vec2::ZERO, Vec2::new(len, 0.0)]
    }

    fn threat(x: f32, y: f32, reach: f32) -> Threat {
        Threat { id: 0, pos: Vec2::new(x, y), reach }
    }

    #[test]
    fn four_degrees_with_nat_shift() {
        let mut d = Dice::new(3);
        for _ in 0..500 {
            let c = d.check(5, 15);
            let base = if c.total >= 25 {
                CriticalSuccess
            } else if c.total >= 15 {
                Success
            } else if c.total <= 5 {
                CriticalFailure
            } else {
                Failure
            };
            let want = match c.natural {
                20 => base.raised(),
                1 => base.lowered(),
                _ => base,
            };
            assert_eq!(c.degree, want);
        }
        assert_eq!(super::super::degree_of_success(20, 3, 15), Failure);
        assert_eq!(super::super::degree_of_success(1, 40, 15), Success);
        assert_eq!(super::super::degree_of_success(10, 25, 15), CriticalSuccess);
        assert_eq!(super::super::degree_of_success(10, 5, 15), CriticalFailure);
    }

    #[test]
    fn leaving_reach_triggers_once_at_exit_point() {
        // enemy 1 m left of the start with 1.5 m reach: the walker starts inside and exits at x=0.5
        let stops = stride_triggers(&line(8.0), &[threat(-1.0, 0.0, 1.5)], false);
        assert_eq!(stops.len(), 1);
        assert!((stops[0].dist - 0.5).abs() < 0.1, "{stops:?}");
    }

    #[test]
    fn step_and_far_threat_do_not_trigger() {
        assert!(stride_triggers(&line(8.0), &[threat(-1.0, 0.0, 1.5)], true).is_empty());
        assert!(stride_triggers(&line(8.0), &[threat(0.0, 9.0, 1.5)], false).is_empty());
        // a path ending inside reach never leaves it
        assert!(stride_triggers(&line(1.0), &[threat(0.0, 0.5, 1.5)], false).is_empty());
    }

    #[test]
    fn passing_through_a_zone_triggers() {
        let stops = stride_triggers(&line(10.0), &[threat(5.0, 0.0, 1.5)], false);
        assert_eq!(stops.len(), 1);
        assert!((stops[0].dist - 6.5).abs() < 0.1);
    }

    #[test]
    fn manipulate_provokes_only_in_reach() {
        let th = [Threat { id: 4, pos: Vec2::new(1.0, 0.0), reach: 1.5 }, Threat { id: 5, pos: Vec2::new(9.0, 0.0), reach: 1.5 }];
        assert_eq!(manipulate_triggers(Vec2::ZERO, &th), vec![4]);
    }

    #[test]
    fn crit_reactive_strike_interrupts_movement() {
        let mut gm = GatedMove::new(8.0, vec![GateStop { dist: 2.0, threat: 0 }]);
        assert_eq!(gm.next_target(), 2.0);
        gm.arrive(|_| Verdict::Interrupt);
        assert!(gm.finished && gm.interrupted);
        assert_eq!(gm.reached, 2.0);
    }

    #[test]
    fn non_crit_reaction_lets_mover_finish() {
        let mut gm = GatedMove::new(8.0, vec![GateStop { dist: 2.0, threat: 0 }, GateStop { dist: 5.0, threat: 1 }]);
        gm.arrive(|_| Verdict::Continue);
        assert!(!gm.finished);
        assert_eq!(gm.next_target(), 5.0);
        gm.arrive(|_| Verdict::Continue);
        assert_eq!(gm.next_target(), 8.0);
        gm.arrive(|_| unreachable!());
        assert!(gm.finished && !gm.interrupted);
        assert_eq!(gm.reached, 8.0);
    }

    #[test]
    fn mover_downed_by_reaction_stops() {
        let mut gm = GatedMove::new(8.0, vec![GateStop { dist: 1.0, threat: 0 }]);
        gm.arrive(|_| Verdict::Down);
        assert!(gm.finished && !gm.interrupted);
    }

    #[test]
    fn verdict_from_attack() {
        let mut d = Dice::new(11);
        let crit = (0..2000).map(|_| attack(&mut d, 30, 10, 1, 8, 4)).find(AttackOutcome::crit).unwrap();
        assert!(crit.damage >= 10, "double damage");
        assert_eq!(verdict(&crit, 5), Verdict::Interrupt);
        assert_eq!(verdict(&crit, 0), Verdict::Down);
        let miss = attack(&mut d, -50, 10, 1, 8, 4);
        assert_eq!((miss.damage, verdict(&miss, 5)), (0, Verdict::Continue));
    }

    #[test]
    fn slice_and_basic_save() {
        let p = vec![Vec2::ZERO, Vec2::new(4.0, 0.0), Vec2::new(4.0, 4.0)];
        let s = slice_path(&p, 3.0, 6.0);
        assert_eq!(s, vec![Vec2::new(3.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 2.0)]);
        assert_eq!(path_length(&p), 8.0);
        assert_eq!([CriticalSuccess, Success, Failure, CriticalFailure].map(|d| basic_save_damage(d, 10)), [0, 5, 10, 20]);
    }
}
