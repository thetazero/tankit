//! Missions: one map per contract (game mode).

use crate::MapKind;
use crate::combat::{KILL_TRUST, ROUND_TIME};
use crate::faction::Faction;
use crate::hold;

/// What a mission pays you for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Contract {
    /// Team deathmatch: paid per kill, most kills for your side wins.
    #[default]
    Attrition,
    /// King of the hill: secure a crystal deposit; the side that holds it
    /// longest wins.
    Hold,
}

impl Contract {
    pub fn name(self) -> &'static str {
        match self {
            Contract::Attrition => "ATTRITION",
            Contract::Hold => "HOLD",
        }
    }
}

pub struct Mission {
    pub name: &'static str,
    pub contract: Contract,
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

pub const MISSIONS: &[Mission] = &[
    Mission {
        name: "DEAD TOWN",
        contract: Contract::Attrition,
        map: "SALTMARKET",
        map_kind: MapKind::Saltmarket,
        seed: 0,
        team_size: 6,
        sides: [Faction::Imperium, Faction::Resonant],
        brief: "PAID PER KILL. MOST KILLS FOR YOUR SIDE TAKES THE TOWN",
    },
    Mission {
        name: "CRYSTAL VEIN",
        contract: Contract::Hold,
        map: "SHARD FLATS",
        map_kind: MapKind::ShardFlats,
        seed: 0,
        team_size: 6,
        sides: [Faction::Imperium, Faction::Resonant],
        brief: "SECURE THE DEPOSIT. PAID FOR YOUR TIME ON IT",
    },
    Mission {
        name: "KEPT FIRE",
        contract: Contract::Attrition,
        map: "THE KILN",
        map_kind: MapKind::Kiln,
        seed: 0,
        team_size: 6,
        sides: [Faction::Ithessar, Faction::Imperium],
        brief: "THE ITHESSAR HIRE YOU TO HOLD THE IMPERIUM OFF THEIR KILN. PAID PER KILL",
    },
];

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
        let clock = format!("{}:{:02}", s / 60, s % 60);
        match self.contract {
            Contract::Attrition => clock,
            Contract::Hold => format!("{clock} OR {:.0} S HELD", hold::TARGET),
        }
    }

    pub fn pay(&self) -> String {
        match self.contract {
            Contract::Attrition => format!("CRYSTALS · {KILL_TRUST} TRUST PER KILL"),
            Contract::Hold => format!(
                "CRYSTALS · {} TRUST / {:.0} S",
                hold::HOLD_TRUST,
                hold::TRUST_EVERY
            ),
        }
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

    #[test]
    fn one_map_per_contract() {
        for (i, a) in MISSIONS.iter().enumerate() {
            for b in &MISSIONS[i + 1..] {
                assert_ne!(
                    a.map_kind, b.map_kind,
                    "{} and {} share a map",
                    a.name, b.name
                );
            }
        }
    }
}
