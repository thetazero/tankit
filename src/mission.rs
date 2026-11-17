//! Missions: one map per contract (game mode).

use crate::MapKind;
use crate::combat::{KILL_REPUTATION, ROUND_TIME};
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
    /// Match seed: bot brains, spawns and kits.
    pub seed: u64,
    /// Tanks per side, the player included.
    pub team_size: usize,
    /// The map's sides: whichever of them you pick hires you, against the rest.
    pub sides: &'static [Faction],
    /// The brief from each side's point of view, in `sides` order.
    pub briefs: &'static [&'static str],
}

pub const MISSIONS: &[Mission] = &[
    Mission {
        name: "DEAD TOWN",
        contract: Contract::Attrition,
        map: "SALTMARKET",
        map_kind: MapKind::Saltmarket,
        seed: 0,
        team_size: 6,
        sides: &[Faction::Imperium, Faction::Resonant],
        briefs: &[
            "THE IMPERIUM WANT THE TOWN BACK. PAID PER KILL. MOST KILLS TAKES IT",
            "THE RESONANT WANT THE TOWN KEPT. PAID PER KILL. MOST KILLS TAKES IT",
        ],
    },
    Mission {
        name: "HOLY GROUND",
        contract: Contract::Hold,
        map: "THE SANCTUM",
        map_kind: MapKind::Sanctum,
        seed: 0,
        team_size: 6,
        sides: &[Faction::Imperium, Faction::Resonant],
        briefs: &[
            "TAKE THE ALTAR FROM THE CHURCH. PAID FOR YOUR TIME ON IT. MIND THE EDGE",
            "HOLD THE ALTAR AT THE HEART OF THE CHURCH. PAID FOR YOUR TIME ON IT. MIND THE EDGE",
        ],
    },
    Mission {
        name: "KEPT FIRE",
        contract: Contract::Attrition,
        map: "THE KILN",
        map_kind: MapKind::Kiln,
        seed: 0,
        team_size: 6,
        sides: &[Faction::Ithessar, Faction::Imperium],
        briefs: &[
            "THE ITHESSAR HIRE YOU TO HOLD THE IMPERIUM OFF THEIR KILN. PAID PER KILL",
            "THE IMPERIUM HIRE YOU TO PUT OUT THE ITHESSAR KILN. PAID PER KILL",
        ],
    },
    Mission {
        name: "FOUR WINDS",
        contract: Contract::Attrition,
        map: "THE CAPITAL",
        map_kind: MapKind::Capital,
        seed: 0,
        team_size: 4,
        sides: &Faction::ALL,
        briefs: &["EVERY SIDE AGAINST EVERY OTHER IN THE IMPERIAL CITY. PAID PER KILL"; 4],
    },
    Mission {
        name: "SCRAP METAL",
        contract: Contract::Attrition,
        map: "THE FOUNDRY",
        map_kind: MapKind::Foundry,
        seed: 0,
        team_size: 6,
        sides: &[Faction::Uthenn, Faction::Imperium],
        briefs: &[
            "THE UTHENN HIRE YOU TO DRIVE THE IMPERIUM OUT OF THEIR WRECKED FOUNDRY. PAID PER KILL",
            "THE IMPERIUM HIRE YOU TO KEEP THE UTHENN OUT OF THE FOUNDRY THEY WRECKED. PAID PER KILL",
        ],
    },
];

/// The missions `faction` can be hired for: indices into `MISSIONS`.
pub fn hiring(faction: Faction) -> Vec<usize> {
    (0..MISSIONS.len())
        .filter(|&i| MISSIONS[i].sides.contains(&faction))
        .collect()
}

impl Mission {
    /// Where `faction` stands among the sides (it must be one of them).
    pub fn side_of(&self, faction: Faction) -> usize {
        self.sides
            .iter()
            .position(|&f| f == faction)
            .expect("a side of this mission")
    }

    /// Bots to fill every side around the player.
    pub fn bots(&self) -> usize {
        self.sides.len() * self.team_size - 1
    }

    /// "6 V 6", or "4 V 4 V 4 V 4" with four sides.
    pub fn teams(&self) -> String {
        vec![self.team_size.to_string(); self.sides.len()].join(" V ")
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
            Contract::Attrition => format!("CRYSTALS · {KILL_REPUTATION} REPUTATION PER KILL"),
            Contract::Hold => format!(
                "CRYSTALS · {} REPUTATION / {:.0} S",
                hold::HOLD_REPUTATION,
                hold::REPUTATION_EVERY
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
            assert_eq!(m.sides, crate::make_map(m.map_kind).factions);
        }
    }

    #[test]
    fn a_brief_for_every_side() {
        for m in MISSIONS {
            assert_eq!(m.briefs.len(), m.sides.len(), "{}", m.name);
        }
    }

    #[test]
    fn every_faction_has_work() {
        for f in Faction::ALL {
            assert!(!hiring(f).is_empty(), "nobody hires {}", f.name());
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
