//! The sides of the war (see docs/setting.md). Only the two human factions
//! exist so far; the machines and the aliens come later.

use glam::Vec3;

use crate::ui::Rgba;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Faction {
    /// Human; automated war machines commanded from orbit.
    Imperium,
    /// Human; a cult built around the crystals.
    Resonant,
}

impl Faction {
    pub fn name(self) -> &'static str {
        match self {
            Faction::Imperium => "IMPERIUM",
            Faction::Resonant => "RESONANT",
        }
    }

    /// Tint for the tank's light armor plates.
    pub fn paint(self) -> Vec3 {
        match self {
            // Pale, cold grey-white.
            Faction::Imperium => Vec3::new(0.80, 0.86, 0.98),
            // Crystal teal.
            Faction::Resonant => Vec3::new(0.12, 0.78, 0.70),
        }
    }

    /// HUD color: scores, kill feed, health bars over tanks.
    pub fn ui(self) -> Rgba {
        match self {
            Faction::Imperium => [0.62, 0.78, 1.0, 1.0],
            Faction::Resonant => [0.25, 0.95, 0.82, 1.0],
        }
    }
}
