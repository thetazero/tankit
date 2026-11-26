//! The sides of the war (see docs/setting.md). Every warring faction makes
//! turrets and fields teams; most maps are two of them against each other, a
//! free-for-all map all four. The Company, the mercenaries, is a faction too,
//! but fights for whoever hires it, or raids for itself, and holds no ground.

use glam::Vec3;

use crate::sim::{HullKind, TurretKind};
use crate::ui::Rgba;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Faction {
    /// Human; automated war machines commanded from orbit.
    Imperium,
    /// Human; a cult built around the crystals.
    Resonant,
    /// A machine faction.
    Uthenn,
    /// The aliens.
    Ithessar,
    /// The mercenaries: you. Not one of the war's sides (not in `ALL`): it fights
    /// inside whichever side hires it, in its own colors, on stolen gear.
    Company,
}

impl Faction {
    pub const ALL: [Faction; 4] = [
        Faction::Imperium,
        Faction::Resonant,
        Faction::Uthenn,
        Faction::Ithessar,
    ];

    /// The side at `i`: one of `ALL`, or past them the Company (a company
    /// fighting for itself).
    pub fn side(i: usize) -> Faction {
        Faction::ALL.get(i).copied().unwrap_or(Faction::Company)
    }

    /// Position in `ALL` (the Company, not in it, comes after).
    pub fn index(self) -> usize {
        self as usize
    }

    pub fn by_name(name: &str) -> Option<Faction> {
        Faction::ALL
            .into_iter()
            .find(|f| f.name().eq_ignore_ascii_case(name))
    }

    /// `by_name`, or the Company.
    pub fn by_name_or_company(name: &str) -> Option<Faction> {
        let company = Faction::Company.name().eq_ignore_ascii_case(name);
        Faction::by_name(name).or(company.then_some(Faction::Company))
    }

    pub fn name(self) -> &'static str {
        match self {
            Faction::Imperium => "IMPERIUM",
            Faction::Resonant => "RESONANT",
            Faction::Uthenn => "UTHENN",
            Faction::Ithessar => "ITHESSAR",
            Faction::Company => "COMPANY",
        }
    }

    /// The side's color out in the world (the Hold site's rings). Tanks
    /// don't wear it: they keep their own paint, except the Company's, which
    /// wear the Company's.
    pub fn paint(self) -> Vec3 {
        match self {
            // Deep crimson.
            Faction::Imperium => Vec3::new(0.52, 0.05, 0.07),
            // Crystal blue.
            Faction::Resonant => Vec3::new(0.14, 0.38, 0.95),
            // Laser green.
            Faction::Uthenn => Vec3::new(0.16, 0.7, 0.3),
            // Rose, the color of their plasma cooled.
            Faction::Ithessar => Vec3::new(0.8, 0.3, 0.46),
            // Hazard amber, a working color nobody's war is fought in.
            Faction::Company => Vec3::new(0.85, 0.52, 0.08),
        }
    }

    /// HUD color: scores, kill feed, health bars over tanks.
    pub fn ui(self) -> Rgba {
        match self {
            Faction::Imperium => [0.95, 0.28, 0.26, 1.0],
            Faction::Resonant => [0.3, 0.55, 1.0, 1.0],
            Faction::Uthenn => [0.35, 1.0, 0.5, 1.0],
            Faction::Ithessar => [1.0, 0.45, 0.68, 1.0],
            Faction::Company => [1.0, 0.74, 0.22, 1.0],
        }
    }

    /// The hulls this side's bots drive (the Company's starter kit is the Imperium's).
    pub fn hulls(self) -> &'static [HullKind] {
        match self {
            Faction::Ithessar => &[HullKind::Sethuun],
            Faction::Resonant => &[HullKind::Vigil],
            Faction::Uthenn => &[HullKind::Gravis],
            _ => &[HullKind::Kataphrakt, HullKind::Kastron],
        }
    }

    /// The turrets this faction makes (and its bots fight with). The Company
    /// makes none: its starter kit's, the Imperium's.
    pub fn turrets(self) -> Vec<TurretKind> {
        if self == Faction::Company {
            return Faction::Imperium.turrets();
        }
        TurretKind::ALL
            .into_iter()
            .filter(|k| k.maker() == self)
            .collect()
    }
}
