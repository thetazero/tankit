//! Crystals: what contracts pay and what parts cost.
//!
//! A finished contract pays crystals (completion, placement, contribution,
//! kill bounty, and time held under Hold); XP stays for actions in the match. Crystals buy parts (which
//! pilot rank unlocks for sale) and upgrade them through M0..M3.

use crate::combat::{self, Gun, Match};
use crate::sim::{HullKind, TurretKind};

/// A hull or a turret.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Hull(HullKind),
    Turret(TurretKind),
}

pub const PARTS: [Part; 4] = [
    Part::Hull(HullKind::Lancer),
    Part::Hull(HullKind::Bastion),
    Part::Turret(TurretKind::Lance),
    Part::Turret(TurretKind::Maul),
];

impl Part {
    pub fn name(self) -> &'static str {
        match self {
            Part::Hull(k) => crate::garage::hull_name(k),
            Part::Turret(k) => crate::garage::turret_name(k),
        }
    }

    pub fn by_name(name: &str) -> Option<Part> {
        PARTS
            .into_iter()
            .find(|p| p.name().eq_ignore_ascii_case(name))
    }

    fn index(self) -> usize {
        PARTS.iter().position(|&p| p == self).unwrap_or(0)
    }

    /// What it costs to buy, and the pilot rank it's for sale at. `None`: a
    /// starter part, owned from the start.
    pub fn price(self) -> Option<Price> {
        match self {
            Part::Hull(HullKind::Lancer) | Part::Turret(TurretKind::Lance) => None,
            Part::Turret(TurretKind::Maul) => Some(Price {
                rank: 2,
                crystals: 500,
            }),
            Part::Hull(HullKind::Bastion) => Some(Price {
                rank: 3,
                crystals: 900,
            }),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Price {
    pub rank: u32,
    pub crystals: u32,
}

/// Highest upgrade level (M3).
pub const MAX_LEVEL: u8 = 3;
/// Crystals to go from level `n` to `n + 1`.
const UPGRADE_COST: [u32; MAX_LEVEL as usize] = [250, 500, 1000];
/// Per level: hull armor +8%; turret damage +6% and reload −4%.
const ARMOR_PER_LEVEL: f32 = 0.08;
const DAMAGE_PER_LEVEL: f32 = 0.06;
const RELOAD_PER_LEVEL: f32 = 0.04;

/// Cost of the next upgrade from `level`, or `None` at max.
pub fn upgrade_cost(level: u8) -> Option<u32> {
    UPGRADE_COST.get(level as usize).copied()
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
        }
    }
}

/// Contract pay: flat for finishing, by placement, for damage dealt.
const COMPLETION: u32 = 50;
const PLACEMENT_MAX: u32 = 100;
const DAMAGE_PER_CRYSTAL: f32 = 10.0;
const BOUNTY: u32 = 15;
/// Hold contract: crystals per second you held the site.
const HOLD_PER_SECOND: f32 = 1.5;

/// The player's pay for a finished round, line by line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Payout {
    pub completion: u32,
    /// 1-based place, of `field`.
    pub place: usize,
    pub field: usize,
    pub placement: u32,
    pub damage: f32,
    pub contribution: u32,
    pub kills: u32,
    pub bounty: u32,
    /// Seconds held, under the Hold contract.
    pub held: f32,
    pub hold: u32,
}

impl Payout {
    pub fn of(m: &Match) -> Payout {
        let field = m.fighters.len();
        let place = m.standings().iter().position(|&i| i == 0).unwrap_or(0) + 1;
        // First takes it all, last nothing; alone, you're first.
        let placement = if field <= 1 {
            PLACEMENT_MAX
        } else {
            (PLACEMENT_MAX as f32 * (field - place) as f32 / (field - 1) as f32).round() as u32
        };
        let me = m.player();
        Payout {
            completion: COMPLETION,
            place,
            field,
            placement,
            damage: me.damage_dealt,
            contribution: (me.damage_dealt / DAMAGE_PER_CRYSTAL) as u32,
            kills: me.kills,
            bounty: me.kills * BOUNTY,
            held: m.held(0),
            hold: (m.held(0) * HOLD_PER_SECOND) as u32,
        }
    }

    pub fn total(&self) -> u32 {
        self.completion + self.placement + self.contribution + self.bounty + self.hold
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upgrades_scale_stats() {
        let mut l = Levels::default();
        let base = l.max_health(HullKind::Lancer);
        let g0 = l.gun(TurretKind::Lance);
        l.set(Part::Hull(HullKind::Lancer), 2);
        l.set(Part::Turret(TurretKind::Lance), 9);
        assert_eq!(l.of(Part::Turret(TurretKind::Lance)), MAX_LEVEL);
        assert!(l.max_health(HullKind::Lancer) > base);
        assert_eq!(
            l.max_health(HullKind::Bastion),
            combat::max_health(HullKind::Bastion)
        );
        let g = l.gun(TurretKind::Lance);
        assert!(g.damage > g0.damage && g.reload < g0.reload);
        assert_eq!(upgrade_cost(MAX_LEVEL), None);
    }

    #[test]
    fn payout_scales_with_place_and_contribution() {
        let mut m = Match::new(crate::map::Map::generate(21), 0, 5, 21);
        m.fighters[0].kills = 3;
        m.fighters[0].damage_dealt = 205.0;
        let p = Payout::of(&m);
        assert_eq!((p.place, p.field), (1, 6));
        assert_eq!(p.placement, PLACEMENT_MAX);
        assert_eq!(p.contribution, 20);
        assert_eq!(p.bounty, 3 * BOUNTY);
        assert_eq!(p.total(), COMPLETION + PLACEMENT_MAX + 20 + 3 * BOUNTY);
        // Dead last: still paid for showing up.
        for f in &mut m.fighters[1..] {
            f.kills = 5;
        }
        let p = Payout::of(&m);
        assert_eq!((p.place, p.placement), (6, 0));
        assert_eq!(p.hold, 0);
    }

    #[test]
    fn hold_pays_for_time_held() {
        let mut m = Match::new(crate::map::Map::generate(21), 0, 2, 21);
        m.set_contract(crate::mission::Contract::Hold);
        m.hold.as_mut().unwrap().held[0] = 40.0;
        let p = Payout::of(&m);
        assert_eq!(p.place, 1, "time held beats kills");
        assert_eq!(p.hold, (40.0 * HOLD_PER_SECOND) as u32);
        assert_eq!(p.total(), COMPLETION + PLACEMENT_MAX + p.hold);
    }
}
