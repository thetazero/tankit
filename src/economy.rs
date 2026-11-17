//! Crystals: what contracts pay and what parts cost.
//!
//! A finished contract pays crystals (completion, your side's victory,
//! contribution, kill bounty, and your time on the site under Hold); reputation
//! with your employer is paid for actions in the match. Crystals buy parts
//! (which standing with their maker puts up for sale) and upgrade them
//! a step at a time through M0..M3.

use crate::combat::{self, Action, Gun, Match, ShellSpec};
use crate::faction::Faction;
use crate::rng::Rng;
use crate::sim::{HullKind, HullStats, TurretKind};

/// A hull or a turret.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Hull(HullKind),
    Turret(TurretKind),
}

pub const PARTS: [Part; 12] = [
    Part::Hull(HullKind::Kataphrakt),
    Part::Hull(HullKind::Kastron),
    Part::Hull(HullKind::Sethuun),
    Part::Hull(HullKind::Vigil),
    Part::Hull(HullKind::Gravis),
    Part::Turret(TurretKind::Kontos),
    Part::Turret(TurretKind::Pelekys),
    Part::Turret(TurretKind::Ammeth),
    Part::Turret(TurretKind::Ossrin),
    Part::Turret(TurretKind::Lumen),
    Part::Turret(TurretKind::Hymn),
    Part::Turret(TurretKind::Knell),
];

impl Part {
    pub fn name(self) -> &'static str {
        match self {
            Part::Hull(k) => crate::garage::hull_name(k),
            Part::Turret(k) => crate::garage::turret_name(k),
        }
    }

    pub fn by_name(name: &str) -> Option<Part> {
        // Saves from before the Ithessar and Imperium parts were renamed.
        let name = match name.to_ascii_uppercase().as_str() {
            "WISP" => "AMMETH",
            "TRIAD" => "OSSRIN",
            "LANCER" => "KATAPHRAKT",
            "BASTION" => "KASTRON",
            "LANCE" => "KONTOS",
            "MAUL" => "PELEKYS",
            _ => name,
        };
        PARTS
            .into_iter()
            .find(|p| p.name().eq_ignore_ascii_case(name))
    }

    fn index(self) -> usize {
        PARTS.iter().position(|&p| p == self).unwrap_or(0)
    }

    /// Who makes it, and so whose reputation puts it up for sale.
    pub fn maker(self) -> Faction {
        match self {
            Part::Hull(k) => k.maker(),
            Part::Turret(k) => k.maker(),
        }
    }

    /// What it costs to buy, and the standing with its maker it's for sale at. `None`: a
    /// starter part, owned from the start.
    pub fn price(self) -> Option<Price> {
        match self {
            Part::Hull(HullKind::Kataphrakt) | Part::Turret(TurretKind::Kontos) => None,
            Part::Turret(TurretKind::Pelekys) => Some(Price {
                standing: 2,
                crystals: 500,
            }),
            Part::Hull(HullKind::Kastron) => Some(Price {
                standing: 3,
                crystals: 900,
            }),
            Part::Hull(HullKind::Sethuun) => Some(Price {
                standing: 3,
                crystals: 850,
            }),
            Part::Hull(HullKind::Vigil) => Some(Price {
                standing: 3,
                crystals: 850,
            }),
            Part::Hull(HullKind::Gravis) => Some(Price {
                standing: 4,
                crystals: 1000,
            }),
            Part::Turret(TurretKind::Hymn) => Some(Price {
                standing: 2,
                crystals: 600,
            }),
            Part::Turret(TurretKind::Ossrin) => Some(Price {
                standing: 3,
                crystals: 750,
            }),
            Part::Turret(TurretKind::Ammeth) => Some(Price {
                standing: 4,
                crystals: 900,
            }),
            Part::Turret(TurretKind::Lumen) => Some(Price {
                standing: 4,
                crystals: 1000,
            }),
            Part::Turret(TurretKind::Knell) => Some(Price {
                standing: 4,
                crystals: 1100,
            }),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Price {
    pub standing: u32,
    pub crystals: u32,
}

/// Upgrades come in small paid steps: ten from one mark (M0, M1, M2, M3) to the next,
/// shown as M1.4 for four steps past M1.
pub const STEPS_PER_MARK: u8 = 10;
/// The last mark.
pub const MAX_MARK: u8 = 3;
/// Highest upgrade level, in steps (M3).
pub const MAX_LEVEL: u8 = STEPS_PER_MARK * MAX_MARK;
/// Crystals for the first step; each one after costs [`STEP_GROWTH`] times the last,
/// so maxing a part is a grind of ~200 contracts (~41k crystals): about 23 to M1, 54
/// more to M2 and 128 more to M3, at ~200 crystals a contract.
const FIRST_STEP: f32 = 300.0;
const STEP_GROWTH: f32 = 1.09;
/// What each mark (M0..M3) multiplies a part's stats by; the steps between a mark and
/// the next are spread evenly between them. Reload is time per shot, so it falls.
#[derive(Clone, Copy)]
struct Curve([f32; MAX_MARK as usize + 1]);

const ARMOR: Curve = Curve([1.0, 1.25, 1.5, 1.75]);
/// Hull top speed (forward and reverse) and turn rate.
const DRIVE: Curve = Curve([1.0, 1.05, 1.10, 1.15]);
const DAMAGE: Curve = Curve([1.0, 1.2, 1.4, 1.6]);
const RELOAD: Curve = Curve([1.0, 0.93, 0.86, 0.80]);
/// Shot speed, and with it range (or a beam's reach).
const REACH: Curve = Curve([1.0, 1.05, 1.10, 1.15]);

impl Curve {
    fn at(self, level: u8) -> f32 {
        let level = level.min(MAX_LEVEL);
        let mark = (level / STEPS_PER_MARK).min(MAX_MARK - 1) as usize;
        let t = (level as f32 - mark as f32 * STEPS_PER_MARK as f32) / STEPS_PER_MARK as f32;
        self.0[mark] + (self.0[mark + 1] - self.0[mark]) * t
    }
}

/// Cost of the next upgrade step from `level`, or `None` at max. Rounded to 10.
pub fn upgrade_cost(level: u8) -> Option<u32> {
    (level < MAX_LEVEL)
        .then(|| (FIRST_STEP * STEP_GROWTH.powi(level as i32) / 10.0).round() as u32 * 10)
}

/// Crystals from M0 to `level`.
#[cfg(test)]
pub fn cost_to(level: u8) -> u32 {
    (0..level.min(MAX_LEVEL)).filter_map(upgrade_cost).sum()
}

/// `level` as the player sees it: M1 on a mark, M1.4 between.
pub fn level_label(level: u8) -> String {
    let level = level.min(MAX_LEVEL);
    let (mark, step) = (level / STEPS_PER_MARK, level % STEPS_PER_MARK);
    if step == 0 {
        format!("M{mark}")
    } else {
        format!("M{mark}.{step}")
    }
}

/// Deviation of a rated mission's bot gear about its rating, in steps.
const GEAR_SPREAD: f32 = 4.0;

/// Upgrade level of every part.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Levels([u8; PARTS.len()]);

impl Levels {
    pub fn of(&self, part: Part) -> u8 {
        self.0[part.index()]
    }

    pub fn set(&mut self, part: Part, level: u8) {
        self.0[part.index()] = level.min(MAX_LEVEL);
    }

    /// Gear for a bot on a mission rated `rating` (in steps): each part's level drawn
    /// from a normal centred on the rating, kept within half a mark below it and a
    /// mark above (M0.5: M0 to M1.5). Rated M0: all M0.
    pub fn rated(rating: u8, rng: &mut Rng) -> Levels {
        let mut levels = Levels::default();
        if rating == 0 {
            return levels;
        }
        let low = rating.saturating_sub(STEPS_PER_MARK / 2) as f32;
        let high = (rating + STEPS_PER_MARK).min(MAX_LEVEL) as f32;
        for part in PARTS {
            // Redraw out of range rather than clamp, so the ends don't pile up.
            let level = loop {
                let l = (rating as f32 + GEAR_SPREAD * rng.normal()).round();
                if (low..=high).contains(&l) {
                    break l;
                }
            };
            levels.set(part, level as u8);
        }
        levels
    }

    pub fn max_health(&self, hull: HullKind) -> f32 {
        combat::max_health(hull) * ARMOR.at(self.of(Part::Hull(hull)))
    }

    /// The hull's drive at its level: faster, and quicker to turn.
    pub fn hull(&self, hull: HullKind) -> HullStats {
        let k = DRIVE.at(self.of(Part::Hull(hull)));
        let h = HullStats::of(hull);
        HullStats {
            max_speed: h.max_speed * k,
            reverse_speed: h.reverse_speed * k,
            turn_rate: h.turn_rate * k,
            // Enough steering force to reach the faster turn as quickly.
            steer_accel: h.steer_accel * k,
            ..h
        }
    }

    pub fn gun(&self, turret: TurretKind) -> Gun {
        let l = self.of(Part::Turret(turret));
        let g = combat::gun(turret);
        let reach = REACH.at(l);
        Gun {
            damage: g.damage * DAMAGE.at(l),
            reload: g.reload * RELOAD.at(l),
            action: match g.action {
                // Faster shots fly as long, so they carry further.
                Action::Shell(s) => Action::Shell(ShellSpec {
                    speed: s.speed * reach,
                    ..s
                }),
                Action::Beam {
                    range,
                    thermal_mass,
                } => Action::Beam {
                    range: range * reach,
                    thermal_mass,
                },
            },
            ..g
        }
    }
}

/// Contract pay: flat for finishing, for your side winning (half on a draw),
/// for damage dealt, per kill.
const COMPLETION: u32 = 50;
const VICTORY: u32 = 100;
const DAMAGE_PER_CRYSTAL: f32 = 10.0;
const BOUNTY: u32 = 15;
/// Hold contract: crystals per second of your own on the site.
const HOLD_PER_SECOND: f32 = 1.0;
/// A rated mission's pay on top, per mark of rating: M0.5 pays half as much again.
const PAY_PER_MARK: f32 = 1.0;

/// What a mission rated `rating` (in steps) multiplies crystal pay by.
pub fn pay_factor(rating: u8) -> f32 {
    1.0 + PAY_PER_MARK * rating as f32 / STEPS_PER_MARK as f32
}

/// The player's pay for a finished round, line by line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Payout {
    pub completion: u32,
    /// How the player's side did.
    pub outcome: Outcome,
    pub victory: u32,
    pub damage: f32,
    pub contribution: u32,
    pub kills: u32,
    pub bounty: u32,
    /// Seconds of your own on the site, under the Hold contract.
    pub held: f32,
    pub hold: u32,
    /// The mission's rating (in steps), and what it adds on top of the rest.
    pub rating: u8,
    pub rated: u32,
}

impl Payout {
    pub fn of(m: &Match) -> Payout {
        let me = m.player();
        // A draw only if your side shares the lead; trailing a tie is a loss.
        let best = (0..m.factions.len()).map(|t| m.score(t)).max().unwrap_or(0);
        let outcome = match m.leader() {
            Some(t) if t == me.team => Outcome::Won,
            None if m.score(me.team) == best => Outcome::Draw,
            _ => Outcome::Lost,
        };
        let mut pay = Payout {
            completion: COMPLETION,
            outcome,
            victory: match outcome {
                Outcome::Won => VICTORY,
                Outcome::Draw => VICTORY / 2,
                Outcome::Lost => 0,
            },
            damage: me.damage_dealt,
            contribution: (me.damage_dealt / DAMAGE_PER_CRYSTAL) as u32,
            kills: me.kills,
            bounty: me.kills * BOUNTY,
            held: m.held(0),
            hold: (m.held(0) * HOLD_PER_SECOND) as u32,
            rating: m.rating,
            rated: 0,
        };
        let base = pay.total();
        pay.rated = (base as f32 * (pay_factor(m.rating) - 1.0)).round() as u32;
        pay
    }

    pub fn total(&self) -> u32 {
        self.completion + self.victory + self.contribution + self.bounty + self.hold + self.rated
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Won,
    Draw,
    Lost,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upgrades_scale_stats() {
        let mut l = Levels::default();
        let base = l.max_health(HullKind::Kataphrakt);
        let g0 = l.gun(TurretKind::Kontos);
        l.set(Part::Hull(HullKind::Kataphrakt), 2);
        l.set(Part::Turret(TurretKind::Kontos), 99);
        assert_eq!(l.of(Part::Turret(TurretKind::Kontos)), MAX_LEVEL);
        assert!(l.max_health(HullKind::Kataphrakt) > base);
        assert_eq!(
            l.max_health(HullKind::Kastron),
            combat::max_health(HullKind::Kastron)
        );
        let g = l.gun(TurretKind::Kontos);
        assert!(g.damage > g0.damage && g.reload < g0.reload);
        assert_eq!(upgrade_cost(MAX_LEVEL), None);
    }

    #[test]
    fn maxing_a_part_is_a_long_grind_of_small_steps() {
        let costs: Vec<u32> = (0..MAX_LEVEL).filter_map(upgrade_cost).collect();
        assert_eq!(costs.len(), MAX_LEVEL as usize);
        assert!(
            costs.windows(2).all(|w| w[0] < w[1]),
            "each step costs more"
        );
        // The first step is about one contract's pay; the whole climb about 200.
        assert_eq!(costs[0], 300);
        let total = cost_to(MAX_LEVEL);
        assert!((35_000..50_000).contains(&total), "{total}");
        assert!(cost_to(STEPS_PER_MARK) < total / 5, "M1 comes early");
    }

    #[test]
    fn each_mark_is_a_clear_jump() {
        let close = |a: f32, b: f32| (a - b).abs() < 1e-4;
        let (hull, turret) = (HullKind::Kataphrakt, TurretKind::Ammeth);
        let base_h = HullStats::of(hull);
        let base_g = combat::gun(turret);
        let speed = |g: &Gun| match g.action {
            Action::Shell(s) => s.speed,
            Action::Beam { .. } => unreachable!(),
        };
        // (armor, drive, damage, reload, reach) at M0..M3.
        let marks = [
            (1.0, 1.0, 1.0, 1.0, 1.0),
            (1.25, 1.05, 1.2, 0.93, 1.05),
            (1.5, 1.10, 1.4, 0.86, 1.10),
            (1.75, 1.15, 1.6, 0.80, 1.15),
        ];
        let mut l = Levels::default();
        for (m, &(armor, drive, damage, reload, reach)) in marks.iter().enumerate() {
            let level = m as u8 * STEPS_PER_MARK;
            l.set(Part::Hull(hull), level);
            l.set(Part::Turret(turret), level);
            assert!(close(l.max_health(hull), combat::max_health(hull) * armor));
            let h = l.hull(hull);
            assert!(close(h.max_speed, base_h.max_speed * drive));
            assert!(close(h.turn_rate, base_h.turn_rate * drive));
            let g = l.gun(turret);
            assert!(close(g.damage, base_g.damage * damage));
            assert!(close(g.reload, base_g.reload * reload));
            assert!(close(speed(&g), speed(&base_g) * reach));
            assert!(close(g.range(), base_g.range() * reach));
        }
        // Halfway between marks is halfway between their stats.
        l.set(Part::Hull(hull), 15);
        assert!(close(l.max_health(hull), combat::max_health(hull) * 1.375));
        // A beam reaches further too.
        l.set(Part::Turret(TurretKind::Lumen), MAX_LEVEL);
        let r = l.gun(TurretKind::Lumen).range() / combat::gun(TurretKind::Lumen).range();
        assert!(close(r, 1.15));
    }

    /// The faster drive is real on the ground, not only on the spec card: each hull at
    /// M3 drives and pivots about 15% faster than at M0.
    #[test]
    fn upgraded_hulls_drive_and_turn_faster() {
        use crate::sim::Command;
        use crate::sim::tests_support::{flat_world, run};
        let go = Command {
            throttle: 1.0,
            ..Command::default()
        };
        let pivot = Command {
            steer: 1.0,
            ..Command::default()
        };
        let mut l = Levels::default();
        for hull in [
            HullKind::Kataphrakt,
            HullKind::Kastron,
            HullKind::Sethuun,
            HullKind::Vigil,
            HullKind::Gravis,
        ] {
            let drive = |l: &Levels| {
                let mut w = flat_world();
                w.set_hull(0, hull);
                w.tanks[0].hull = l.hull(hull);
                run(&mut w, Command::default(), 1.0);
                run(&mut w, go, 6.0);
                let speed = w.tanks[0].speed();
                let mut w = flat_world();
                w.set_hull(0, hull);
                w.tanks[0].hull = l.hull(hull);
                run(&mut w, Command::default(), 1.0);
                run(&mut w, pivot, 2.0);
                let t = &w.tanks[0];
                let turn = w.physics.bodies[t.body].angvel().y.abs();
                (speed, turn)
            };
            l.set(Part::Hull(hull), 0);
            let (s0, t0) = drive(&l);
            l.set(Part::Hull(hull), MAX_LEVEL);
            let (s3, t3) = drive(&l);
            assert!((s3 / s0 - 1.15).abs() < 0.04, "{hull:?} speed {s0} -> {s3}");
            assert!((t3 / t0 - 1.15).abs() < 0.06, "{hull:?} turn {t0} -> {t3}");
        }
    }

    #[test]
    fn levels_read_as_marks_and_steps() {
        assert_eq!(level_label(0), "M0");
        assert_eq!(level_label(4), "M0.4");
        assert_eq!(level_label(10), "M1");
        assert_eq!(level_label(27), "M2.7");
        assert_eq!(level_label(MAX_LEVEL), "M3");
    }

    #[test]
    fn rated_gear_is_spread_about_the_rating() {
        let mut rng = crate::rng::Rng::new(7);
        assert_eq!(Levels::rated(0, &mut rng), Levels::default());
        let mut all = Vec::new();
        for _ in 0..2000 {
            let l = Levels::rated(5, &mut rng);
            all.extend(PARTS.iter().map(|&p| l.of(p) as f32));
        }
        // M0.5 enemies: M0 to M1.5, centred on M0.5 (a little above, the range
        // reaching further up than down), mixed rather than all one level.
        let (lo, hi) = all
            .iter()
            .fold((99.0f32, 0.0f32), |(a, b), &l| (a.min(l), b.max(l)));
        assert_eq!((lo, hi), (0.0, 15.0));
        let mean = all.iter().sum::<f32>() / all.len() as f32;
        assert!((5.0..6.5).contains(&mean), "mean {mean}");
        let near = all.iter().filter(|&&l| (2.0..=8.0).contains(&l)).count();
        assert!(near * 10 > all.len() * 5, "most within a few steps of M0.5");
        let at = |n: f32| all.iter().filter(|&&l| l == n).count();
        assert!(
            at(5.0) > at(0.0) && at(5.0) > at(12.0),
            "peaks at the rating"
        );
    }

    #[test]
    fn rated_missions_pay_more_and_field_better_gear() {
        let mut m = Match::new(crate::map::Map::capital(), 0, 15, 3);
        m.fighters[0].kills = 2;
        m.fighters[0].damage_dealt = 300.0;
        let base = Payout::of(&m).total();
        let health: Vec<f32> = m.fighters.iter().map(|f| f.health).collect();
        m.set_rating(5);
        let p = Payout::of(&m);
        assert_eq!(p.total(), base + (base as f32 * 0.5).round() as u32);
        assert_eq!(pay_factor(5), 1.5);
        // The player's own gear is theirs; the bots' is better on average.
        assert_eq!(m.fighters[0].levels, Levels::default());
        assert_eq!(m.fighters[0].health, health[0]);
        let tougher = (1..m.fighters.len())
            .filter(|&i| m.fighters[i].health > health[i])
            .count();
        assert!(tougher > m.fighters.len() / 2, "{tougher} tougher");
        for (i, f) in m.fighters.iter().enumerate().skip(1) {
            assert_eq!(f.health, f.levels.max_health(m.world.tanks[i].hull_kind));
        }
    }

    #[test]
    fn payout_scales_with_victory_and_contribution() {
        let mut m = Match::new(crate::map::Map::saltmarket(), 0, 5, 21);
        m.fighters[0].kills = 3;
        m.fighters[0].damage_dealt = 205.0;
        let p = Payout::of(&m);
        assert_eq!(p.outcome, Outcome::Won);
        assert_eq!(p.victory, VICTORY);
        assert_eq!(p.contribution, 20);
        assert_eq!(p.bounty, 3 * BOUNTY);
        assert_eq!(p.total(), COMPLETION + VICTORY + 20 + 3 * BOUNTY);
        // Your side lost: still paid for showing up and for your own kills.
        for f in m.fighters.iter_mut().filter(|f| f.team == 1) {
            f.kills = 5;
        }
        let p = Payout::of(&m);
        assert_eq!((p.outcome, p.victory), (Outcome::Lost, 0));
        assert_eq!(p.bounty, 3 * BOUNTY);
        // Level: half.
        let enemy_kills: u32 = m
            .fighters
            .iter()
            .filter(|f| f.team == 1)
            .map(|f| f.kills)
            .sum();
        m.fighters[0].kills = enemy_kills;
        let p = Payout::of(&m);
        assert_eq!((p.outcome, p.victory), (Outcome::Draw, VICTORY / 2));
    }

    #[test]
    fn hold_pays_for_your_time_on_the_site_and_your_side_holding_longest() {
        let mut m = Match::new(crate::map::Map::saltmarket(), 0, 3, 21);
        m.set_contract(crate::mission::Contract::Hold);
        for f in m.fighters.iter_mut().filter(|f| f.team == 1) {
            f.kills = 5;
        }
        let h = m.hold.as_mut().unwrap();
        h.held[0] = 50.0;
        h.own[0] = 40.0;
        let p = Payout::of(&m);
        assert_eq!(p.outcome, Outcome::Won, "time held beats kills");
        assert_eq!(p.hold, (40.0 * HOLD_PER_SECOND) as u32);
        assert_eq!(p.total(), COMPLETION + VICTORY + p.hold);
    }
}
