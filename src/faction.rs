//! The sides of the war (see docs/setting.md). Every faction makes turrets;
//! the two human factions also field teams (the Uthenn and the Ithessar come
//! later as sides).

use glam::Vec3;

use crate::sim::{HullKind, TurretKind};
use crate::ui::Rgba;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Faction {
    /// Human; automated war machines commanded from orbit.
    Imperium,
    /// Human; a cult built around the crystals.
    Resonant,
    /// A machine faction.
    Uthenn,
    /// The aliens.
    Ithessar,
}

impl Faction {
    pub const ALL: [Faction; 4] = [
        Faction::Imperium,
        Faction::Resonant,
        Faction::Uthenn,
        Faction::Ithessar,
    ];

    /// Position in `ALL`, for per-faction tables (trust).
    pub fn index(self) -> usize {
        self as usize
    }

    pub fn by_name(name: &str) -> Option<Faction> {
        Faction::ALL
            .into_iter()
            .find(|f| f.name().eq_ignore_ascii_case(name))
    }

    pub fn name(self) -> &'static str {
        match self {
            Faction::Imperium => "IMPERIUM",
            Faction::Resonant => "RESONANT",
            Faction::Uthenn => "UTHENN",
            Faction::Ithessar => "ITHESSAR",
        }
    }

    /// The side's color out in the world (the Hold site's rings). Tanks
    /// don't wear it: they keep their own paint.
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
        }
    }

    /// HUD color: scores, kill feed, health bars over tanks.
    pub fn ui(self) -> Rgba {
        match self {
            Faction::Imperium => [0.95, 0.28, 0.26, 1.0],
            Faction::Resonant => [0.3, 0.55, 1.0, 1.0],
            Faction::Uthenn => [0.35, 1.0, 0.5, 1.0],
            Faction::Ithessar => [1.0, 0.45, 0.68, 1.0],
        }
    }

    /// The hulls this side's bots drive.
    pub fn hulls(self) -> &'static [HullKind] {
        match self {
            Faction::Ithessar => &[HullKind::Sethuun],
            Faction::Resonant => &[HullKind::Vigil],
            Faction::Uthenn => &[HullKind::Gravis],
            _ => &[HullKind::Kataphrakt, HullKind::Kastron],
        }
    }

    /// The turrets this faction makes (and its bots fight with).
    pub fn turrets(self) -> Vec<TurretKind> {
        TurretKind::ALL
            .into_iter()
            .filter(|k| k.maker() == self)
            .collect()
    }
}
