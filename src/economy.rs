//! Crystals: what contracts pay and what parts cost.
//!
//! A finished contract pays crystals (completion, your side's victory,
//! contribution, kill bounty, and your time on the site under Hold); reputation
//! with your employer is paid for actions in the match. Crystals buy parts
//! (which standing with their maker puts up for sale) and upgrade them
//! a step at a time through M0..M3.

use crate::combat::{self, Gun, Match};
use crate::faction::Faction;
use crate::sim::{HullKind, TurretKind};

/// A hull or a turret.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Hull(HullKind),
    Turret(TurretKind),
}

pub const PARTS: [Part; 11] = [
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
/// Per step: hull armor +0.8%; turret damage +0.6% and reload −0.4% (so +24%, +18%
/// and −12% at M3).
const ARMOR_PER_LEVEL: f32 = 0.008;
const DAMAGE_PER_LEVEL: f32 = 0.006;
const RELOAD_PER_LEVEL: f32 = 0.004;

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

    pub fn max_health(&self, hull: HullKind) -> f32 {
        let l = self.of(Part::Hull(hull)) as f32;
        combat::max_health(hull) * (1.0 + ARMOR_PER_LEVEL * l)
    }

    pub fn gun(&self, turret: TurretKind) -> Gun {
        let l = self.of(Part::Turret(turret)) as f32;
        let g = combat::gun(turret);
        Gun {
            damage: g.damage * (1.0 + DAMAGE_PER_LEVEL * l),
            reload: g.reload * (1.0 - RELOAD_PER_LEVEL * l),
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
        Payout {
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
        }
    }

    pub fn total(&self) -> u32 {
        self.completion + self.victory + self.contribution + self.bounty + self.hold
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
        // M3 is worth what it was when it was three big upgrades.
        let mut l = Levels::default();
        l.set(Part::Hull(HullKind::Kataphrakt), MAX_LEVEL);
        let hp = l.max_health(HullKind::Kataphrakt) / combat::max_health(HullKind::Kataphrakt);
        assert!((hp - 1.24).abs() < 1e-4);
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
