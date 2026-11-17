//! Missions: one map per contract (game mode).

use crate::MapKind;
use crate::faction::Faction;

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
    /// How hard it is, in upgrade steps (shown as a mark: 5 is M0.5): the enemy's
    /// gear is drawn about it, and the pay rises with it.
    pub rating: u8,
    /// Standing a side needs your reputation with it at before it hires you here.
    pub standing: u32,
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
            "The Imperium want the town back. Most kills takes it.",
            "The Resonant want the town kept. Most kills takes it.",
        ],
        rating: 0,
        standing: 1,
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
            "Take the altar from the church and stay on it. Mind the edge.",
            "Hold the altar at the heart of the church. Mind the edge.",
        ],
        rating: 0,
        standing: 1,
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
            "Hold the Imperium off the Ithessar kiln.",
            "Put out the Ithessar kiln.",
        ],
        rating: 0,
        standing: 1,
    },
    Mission {
        name: "FOUR WINDS",
        contract: Contract::Attrition,
        map: "THE CAPITAL",
        map_kind: MapKind::Capital,
        seed: 0,
        team_size: 4,
        sides: &Faction::ALL,
        briefs: &["Every side against every other in the imperial city. Better-geared crews."; 4],
        rating: 5,
        standing: 2,
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
            "Drive the Imperium out of the foundry they wrecked.",
            "Keep the Uthenn out of the foundry they wrecked.",
        ],
        rating: 0,
        standing: 1,
    },
];

/// The missions `faction` hires for, locked or not: indices into `MISSIONS`.
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

    /// The rating as a mark: "M0", "M0.5".
    pub fn rating_label(&self) -> String {
        crate::economy::level_label(self.rating)
    }

    /// Whether a side will hire you here, at standing `standing` with it.
    pub fn open_at(&self, standing: u32) -> bool {
        standing >= self.standing
    }

    /// Bots to fill every side around the player.
    pub fn bots(&self) -> usize {
        self.sides.len() * self.team_size - 1
    }

    /// Crystals a win pays before kills and damage.
    pub fn win_pay(&self) -> u32 {
        crate::economy::win_pay(self.rating)
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
    fn four_winds_is_m0_5_and_earned() {
        let m = MISSIONS.iter().find(|m| m.name == "FOUR WINDS").unwrap();
        assert_eq!(m.rating_label(), "M0.5");
        assert!(!m.open_at(1), "open to a new pilot");
        assert!(m.open_at(2));
        // Pays half as much again as an M0 contract.
        assert_eq!(m.win_pay(), MISSIONS[0].win_pay() * 3 / 2);
        // The rest stay M0, open from the start.
        for m in MISSIONS.iter().filter(|m| m.name != "FOUR WINDS") {
            assert_eq!((m.rating, m.open_at(1)), (0, true), "{}", m.name);
        }
    }

    #[test]
    fn every_faction_has_open_work() {
        for f in Faction::ALL {
            assert!(
                hiring(f).iter().any(|&i| MISSIONS[i].open_at(1)),
                "nothing open for a new pilot with {}",
                f.name()
            );
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
