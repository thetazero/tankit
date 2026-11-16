//! Missions: a map plus a contract (game mode). Only one for now.

use crate::MapKind;
use crate::combat::{KILL_XP, ROUND_TIME};
use crate::faction::Faction;

pub struct Mission {
    pub name: &'static str,
    /// The contract: what you're paid for.
    pub mode: &'static str,
    pub map: &'static str,
    pub map_kind: MapKind,
    /// Map seed (only matters for procedural maps).
    pub seed: u64,
    /// Tanks per side, the player included.
    pub team_size: usize,
    /// Who hires you, and who you're sent against (the map's sides).
    pub sides: [Faction; 2],
    pub brief: &'static str,
}

pub const MISSIONS: &[Mission] = &[Mission {
    name: "DEAD TOWN",
    mode: "ATTRITION",
    map: "SALTMARKET",
    map_kind: MapKind::Saltmarket,
    seed: 0,
    team_size: 6,
    sides: [Faction::Imperium, Faction::Resonant],
    brief: "PAID PER KILL. MOST KILLS FOR YOUR SIDE TAKES THE TOWN",
}];

impl Mission {
    /// Bots to fill both sides around the player.
    pub fn bots(&self) -> usize {
        2 * self.team_size - 1
    }

    pub fn teams(&self) -> String {
        format!("{0} V {0}", self.team_size)
    }

    pub fn round_time(&self) -> String {
        let s = ROUND_TIME as u32;
        format!("{}:{:02}", s / 60, s % 60)
    }

    pub fn pay(&self) -> String {
        format!("CRYSTALS · {KILL_XP} XP PER KILL")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mission_sides_match_the_map() {
        for m in MISSIONS {
            assert_eq!(m.sides, crate::make_map(m.seed, m.map_kind).factions);
        }
    }
}
