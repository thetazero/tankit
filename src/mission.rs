//! Missions: one map per contract (game mode). The Company takes a mission
//! for whichever of its sides you pick, once its rank is high enough, and
//! most offer a schematic stolen from the side you fight as the prize.

use crate::MapKind;
use crate::economy::Part;
use crate::faction::Faction;
use crate::sim::{HullKind, TurretKind};

/// What a mission pays you for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Contract {
    /// Team deathmatch: paid per kill, most kills for your side wins.
    #[default]
    Attrition,
    /// King of the hill: secure a crystal deposit; the side that holds it
    /// longest wins.
    Hold,
    /// One-sided capture the flag: one side defends a flag at its base,
    /// the other carries it home; sides swap each round (see `raid`).
    Raid,
}

impl Contract {
    pub fn name(self) -> &'static str {
        match self {
            Contract::Attrition => "ATTRITION",
            Contract::Hold => "HOLD",
            Contract::Raid => "RAID",
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
    /// The Company rank it takes before the Company takes this contract.
    pub rank: u32,
    /// What winning steals, for each side that can hire you (in `sides` order): a
    /// schematic from an enemy's vaults, or none (crystals only).
    pub prizes: &'static [Option<Part>],
}

/// The Company rank each mission tier opens at: T0 from the start.
const T0_5_RANK: u32 = 4;
const T1_RANK: u32 = 8;

const PELEKYS: Option<Part> = Some(Part::Turret(TurretKind::Pelekys));
const KASTRON: Option<Part> = Some(Part::Hull(HullKind::Kastron));
const AMMETH: Option<Part> = Some(Part::Turret(TurretKind::Ammeth));
const OSSRIN: Option<Part> = Some(Part::Turret(TurretKind::Ossrin));
const SETHUUN: Option<Part> = Some(Part::Hull(HullKind::Sethuun));
const HYMN: Option<Part> = Some(Part::Turret(TurretKind::Hymn));
const KNELL: Option<Part> = Some(Part::Turret(TurretKind::Knell));
const VIGIL: Option<Part> = Some(Part::Hull(HullKind::Vigil));
const LUMEN: Option<Part> = Some(Part::Turret(TurretKind::Lumen));
const GRAVIS: Option<Part> = Some(Part::Hull(HullKind::Gravis));

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
        rank: 1,
        prizes: &[HYMN, PELEKYS],
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
        rank: 1,
        prizes: &[VIGIL, KASTRON],
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
        rank: 1,
        prizes: &[None, OSSRIN],
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
        rank: T0_5_RANK,
        prizes: &[LUMEN, SETHUUN, None, None],
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
        rank: 1,
        prizes: &[None, GRAVIS],
    },
    Mission {
        name: "DEEP VEIN",
        contract: Contract::Attrition,
        map: "THE SEAM",
        map_kind: MapKind::Mine,
        seed: 0,
        team_size: 6,
        sides: &[Faction::Ithessar, Faction::Resonant],
        briefs: &[
            "Keep the Resonant out of the mine. Most kills holds it.",
            "The seam is holy. Drive the miners off it.",
        ],
        rating: 10,
        rank: T1_RANK,
        prizes: &[KNELL, AMMETH],
    },
    Mission {
        name: "STOLEN FLAG",
        contract: Contract::Raid,
        map: "THE SEAM",
        map_kind: MapKind::Mine,
        seed: 0,
        team_size: 6,
        sides: &[Faction::Ithessar, Faction::Resonant],
        briefs: &[
            "Take the Resonant flag from their camp and bring it home. Then keep yours.",
            "Take the miners' flag from their yard and bring it home. Then keep yours.",
        ],
        rating: 10,
        rank: T1_RANK,
        // Crystals only: every schematic is already some mission's prize.
        prizes: &[None, None],
    },
];

/// The missions `faction` hires for, locked or not: indices into `MISSIONS`.
pub fn hiring(faction: Faction) -> Vec<usize> {
    (0..MISSIONS.len())
        .filter(|&i| MISSIONS[i].sides.contains(&faction))
        .collect()
}

/// The mission (index into `MISSIONS`) and side whose win steals `part`'s schematic.
pub fn offering(part: Part) -> Option<(usize, usize)> {
    MISSIONS.iter().enumerate().find_map(|(i, m)| {
        (0..m.sides.len())
            .find(|&side| m.prize(side) == Some(part))
            .map(|side| (i, side))
    })
}

impl Mission {
    /// Where `faction` stands among the sides (it must be one of them).
    pub fn side_of(&self, faction: Faction) -> usize {
        self.sides
            .iter()
            .position(|&f| f == faction)
            .expect("a side of this mission")
    }

    /// The rating as a tier: "T0", "T0.5".
    pub fn rating_label(&self) -> String {
        crate::economy::level_label(self.rating)
    }

    /// Whether the Company takes this contract at Company rank `rank`.
    pub fn open_at(&self, rank: u32) -> bool {
        rank >= self.rank
    }

    /// The schematic winning steals for side `side`, if any.
    pub fn prize(&self, side: usize) -> Option<Part> {
        self.prizes.get(side).copied().flatten()
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
    fn a_prize_slot_for_every_side() {
        for m in MISSIONS {
            assert_eq!(m.prizes.len(), m.sides.len(), "{}", m.name);
        }
    }

    #[test]
    fn four_winds_is_m0_5_and_earned() {
        let m = MISSIONS.iter().find(|m| m.name == "FOUR WINDS").unwrap();
        assert_eq!(m.rating_label(), "T0.5");
        assert!(!m.open_at(1), "open to a new pilot");
        assert!(m.open_at(T0_5_RANK));
        // Pays half as much again as an M0 contract.
        assert_eq!(m.win_pay(), MISSIONS[0].win_pay() * 3 / 2);
        // The rest but the Seam's two are T0, open from the start.
        for m in MISSIONS
            .iter()
            .filter(|m| !["FOUR WINDS", "DEEP VEIN", "STOLEN FLAG"].contains(&m.name))
        {
            assert_eq!((m.rating, m.open_at(1)), (0, true), "{}", m.name);
        }
    }

    #[test]
    fn deep_vein_is_t1_and_earned() {
        let m = MISSIONS.iter().find(|m| m.name == "DEEP VEIN").unwrap();
        assert_eq!(m.rating_label(), "T1");
        assert!(!m.open_at(T1_RANK - 1));
        assert!(m.open_at(T1_RANK));
        // Pays twice what a T0 contract does.
        assert_eq!(m.win_pay(), MISSIONS[0].win_pay() * 2);
    }

    /// Harder tiers take a higher rank.
    #[test]
    fn rank_opens_tiers_in_order() {
        for a in MISSIONS {
            for b in MISSIONS {
                if a.rating < b.rating {
                    assert!(a.rank < b.rank, "{} vs {}", a.name, b.name);
                }
            }
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

    /// Every part but the starters is some mission's prize, stolen from a side you
    /// fight there: never from whoever hired you, never twice.
    #[test]
    fn every_schematic_is_stolen_from_the_enemy_once() {
        let mut offered = Vec::new();
        for m in MISSIONS {
            for (side, &employer) in m.sides.iter().enumerate() {
                if let Some(part) = m.prize(side) {
                    assert!(!part.starter(), "{} offers a starter", m.name);
                    assert_ne!(part.maker(), employer, "{} for {}", m.name, employer.name());
                    assert!(
                        m.sides.contains(&part.maker()),
                        "{}: {}",
                        m.name,
                        part.name()
                    );
                    assert!(!offered.contains(&part), "{} offered twice", part.name());
                    offered.push(part);
                }
            }
        }
        for part in crate::economy::PARTS.into_iter().filter(|p| !p.starter()) {
            assert!(offered.contains(&part), "nobody offers {}", part.name());
        }
    }

    /// A map can carry more than one contract, but never the same one twice.
    #[test]
    fn one_mission_per_map_and_contract() {
        for (i, a) in MISSIONS.iter().enumerate() {
            for b in &MISSIONS[i + 1..] {
                assert!(
                    a.map_kind != b.map_kind || a.contract != b.contract,
                    "{} and {} are the same contract on the same map",
                    a.name,
                    b.name
                );
            }
        }
    }

    #[test]
    fn raid_is_on_the_seam() {
        let m = MISSIONS
            .iter()
            .find(|m| m.contract == Contract::Raid)
            .unwrap();
        assert_eq!(m.map_kind, MapKind::Mine);
        // The Seam's tier, like Deep Vein.
        assert_eq!(m.rating_label(), "T1");
        assert!(m.open_at(T1_RANK));
    }
}
