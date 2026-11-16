//! The sides of the war (see docs/setting.md). Every faction makes turrets;
//! the two human factions also field teams (the machines and the Ithessar come
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
    Machines,
    /// The aliens.
    Ithessar,
}

impl Faction {
    pub const ALL: [Faction; 4] = [
        Faction::Imperium,
        Faction::Resonant,
        Faction::Machines,
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
            Faction::Machines => "MACHINES",
            Faction::Ithessar => "ITHESSAR",
        }
    }

    /// Tint for the tank's light armor plates.
    pub fn paint(self) -> Vec3 {
        match self {
            // Pale, cold grey-white.
            Faction::Imperium => Vec3::new(0.80, 0.86, 0.98),
            // Crystal teal.
            Faction::Resonant => Vec3::new(0.12, 0.78, 0.70),
            // Rust orange.
            Faction::Machines => Vec3::new(0.86, 0.46, 0.2),
            // Rose, the color of their plasma cooled.
            Faction::Ithessar => Vec3::new(0.8, 0.3, 0.46),
        }
    }

    /// HUD color: scores, kill feed, health bars over tanks.
    pub fn ui(self) -> Rgba {
        match self {
            Faction::Imperium => [0.62, 0.78, 1.0, 1.0],
            Faction::Resonant => [0.25, 0.95, 0.82, 1.0],
            Faction::Machines => [1.0, 0.6, 0.3, 1.0],
            Faction::Ithessar => [1.0, 0.45, 0.68, 1.0],
        }
    }

    /// The hulls this side's bots drive. Only the Ithessar build their own;
    /// everyone else fields the common tracked kit.
    pub fn hulls(self) -> &'static [HullKind] {
        match self {
            Faction::Ithessar => &[HullKind::Sethuun],
            _ => &[HullKind::Lancer, HullKind::Bastion],
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
