//! Missions: one map per contract (game mode). The Company takes a mission
//! for whichever of its sides you pick, at any rank (#437).
//! (Schematics come from reverse engineering wrecks now: #438.)

use crate::MapKind;
use crate::faction::Faction;
use crate::map::procgen::{Look, foundry::Ground};

/// What a mission pays you for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Contract {
    /// A battle of attrition: the job is to cost the enemy hulls (a win
    /// still takes the site, as under any contract: #419, #422). Paid per kill; the side that destroys most wins (or
    /// the side still in, if the other runs out of hulls).
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
    },
    Mission {
        name: "HOLY GROUND",
        contract: Contract::Hold,
        map: "THE SANCTUM",
        map_kind: MapKind::Sanctum,
        seed: 0,
        team_size: 6,
        sides: &[Faction::Imperium, Faction::Resonant],
    },
    Mission {
        name: "KEPT FIRE",
        contract: Contract::Attrition,
        map: "THE KILN",
        map_kind: MapKind::Kiln,
        seed: 0,
        team_size: 6,
        sides: &[Faction::Ithessar, Faction::Imperium],
    },
    Mission {
        name: "BROKEN STREETS",
        contract: Contract::Attrition,
        map: "CITY",
        map_kind: MapKind::City(
            crate::map::procgen::Look::Imperium,
            crate::map::procgen::setting::Setting::Plain,
        ),
        seed: 0,
        team_size: 6,
        sides: &[Faction::Imperium, Faction::Resonant],
    },
    Mission {
        name: "COLD IRON",
        contract: Contract::Attrition,
        map: "HULL FOUNDRY",
        // A site's is in its founder's style, on its world's ground (see
        // `SiteView::map_kind`).
        map_kind: MapKind::Foundry(Look::Imperium, Ground::Desert),
        seed: 0,
        team_size: 6,
        sides: &[Faction::Imperium, Faction::Resonant],
    },
    Mission {
        name: "HOT METAL",
        contract: Contract::Hold,
        map: "HULL FOUNDRY",
        map_kind: MapKind::Foundry(Look::Imperium, Ground::Desert),
        seed: 0,
        team_size: 6,
        sides: &[Faction::Imperium, Faction::Resonant],
    },
    Mission {
        name: "DEEP VEIN",
        contract: Contract::Attrition,
        map: "CRYSTAL MINE",
        map_kind: MapKind::Mine,
        seed: 0,
        team_size: 6,
        sides: &[Faction::Ithessar, Faction::Resonant],
    },
    Mission {
        name: "STOLEN FLAG",
        contract: Contract::Raid,
        map: "CRYSTAL MINE",
        map_kind: MapKind::Mine,
        seed: 0,
        team_size: 6,
        sides: &[Faction::Ithessar, Faction::Resonant],
    },
    Mission {
        name: "DRY WIND",
        contract: Contract::Hold,
        map: "OPEN DESERT",
        map_kind: MapKind::Desert,
        seed: 0,
        team_size: 6,
        sides: &[Faction::Imperium, Faction::Resonant],
    },
    Mission {
        name: "LONG SHADOWS",
        contract: Contract::Attrition,
        map: "OPEN DESERT",
        map_kind: MapKind::Desert,
        seed: 0,
        team_size: 6,
        sides: &[Faction::Imperium, Faction::Resonant],
    },
    // A theater's shield generators are fought over here (#341): the
    // generator is the Hold site, its holder's model in the shape the
    // site's in (see `SiteView::map_kind`); the sides are the theater's.
    Mission {
        name: "SHIELD WALL",
        contract: Contract::Hold,
        map: "SHIELD CITY",
        map_kind: MapKind::ShieldSite(
            crate::map::procgen::Look::Imperium,
            crate::map::procgen::setting::Setting::Plain,
            crate::Generator::up(Faction::Imperium),
        ),
        seed: 0,
        team_size: 6,
        sides: &[Faction::Imperium, Faction::Resonant],
    },
    // The same out in the open, on desert worlds only.
    Mission {
        name: "BARE SKY",
        contract: Contract::Hold,
        map: "SHIELD DESERT",
        map_kind: MapKind::ShieldDesert(crate::Generator::up(Faction::Imperium)),
        seed: 0,
        team_size: 6,
        sides: &[Faction::Imperium, Faction::Resonant],
    },
    Mission {
        name: "HIGH SEAT",
        contract: Contract::Hold,
        map: "CITY",
        map_kind: MapKind::City(
            crate::map::procgen::Look::Imperium,
            crate::map::procgen::setting::Setting::Plain,
        ),
        seed: 0,
        team_size: 6,
        sides: &[Faction::Imperium, Faction::Resonant],
    },
    // A site's is in its founder's style (see `SiteView::map_kind`).
    Mission {
        name: "DEAD LINE",
        contract: Contract::Attrition,
        map: "RAIL DEPOT",
        map_kind: MapKind::Depot(Look::Imperium),
        seed: 0,
        team_size: 6,
        sides: &[Faction::Imperium, Faction::Resonant],
    },
    Mission {
        name: "NIGHT FREIGHT",
        contract: Contract::Raid,
        map: "RAIL DEPOT",
        map_kind: MapKind::Depot(Look::Imperium),
        seed: 0,
        team_size: 6,
        sides: &[Faction::Imperium, Faction::Resonant],
    },
];

/// The missions a site on a `class` world can be fought with (#311): every
/// two-sided one on one of the class's maps, its map and contract standing
/// in for the site's. Your rank has no say (#437), so a site keeps its
/// mission as you rank up.
pub fn for_sites(class: crate::galaxy::theater::Class) -> Vec<usize> {
    (0..MISSIONS.len())
        .filter(|&i| {
            let m = &MISSIONS[i];
            m.sides.len() == 2 && class.fights_on(m.map_kind)
        })
        .collect()
}

/// The missions a hull foundry on a `class` world can be fought with: those
/// in a foundry (#525), if its ground has one, else any of theirs.
pub fn for_foundries(class: crate::galaxy::theater::Class) -> Vec<usize> {
    let mut pool = for_sites(class);
    if class.ground().is_some() {
        pool.retain(|&i| matches!(MISSIONS[i].map_kind, MapKind::Foundry(..)));
    }
    pool
}

/// The missions a crystal mine on a `class` world can be fought with: on a
/// desert world, those in a mine (#523). Other worlds have no mine maps, so
/// any of theirs but the foundries.
pub fn for_mines(class: crate::galaxy::theater::Class) -> Vec<usize> {
    let mut pool = for_sites(class);
    pool.retain(|&i| !matches!(MISSIONS[i].map_kind, MapKind::Foundry(..)));
    if class == crate::galaxy::theater::Class::Desert {
        pool.retain(|&i| MISSIONS[i].map_kind == MapKind::Mine);
    }
    pool
}

/// The missions an outpost on a `class` world can be fought with (#547):
/// those on none of the maps production sites are fought on.
pub fn for_outposts(class: crate::galaxy::theater::Class) -> Vec<usize> {
    let mut pool = for_sites(class);
    pool.retain(|&i| !matches!(MISSIONS[i].map_kind, MapKind::Foundry(..) | MapKind::Mine));
    pool
}

/// The missions a shield generator on a `class` world can be fought with: a
/// Hold on the shield city, or on a desert world perhaps the shield desert.
pub fn for_shields(class: crate::galaxy::theater::Class) -> Vec<usize> {
    (0..MISSIONS.len())
        .filter(|&i| class.shields_on(MISSIONS[i].map_kind))
        .collect()
}

impl Mission {
    /// Bots to fill every side around the player.
    pub fn bots(&self) -> usize {
        self.sides.len() * self.team_size - 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mission_sides_match_the_map() {
        for m in MISSIONS {
            assert_eq!(m.sides, crate::make_map(m.map_kind, m.seed).factions);
        }
    }

    /// A new pilot's theater has maps to fight on.
    #[test]
    fn sites_have_maps_from_the_start() {
        use crate::galaxy::theater::Class;
        for class in Class::ALL {
            assert!(!for_foundries(class).is_empty(), "{class:?}");
            assert!(!for_sites(class).is_empty(), "{class:?}");
            assert!(!for_mines(class).is_empty(), "{class:?}");
            assert!(!for_outposts(class).is_empty(), "{class:?}");
            assert!(!for_shields(class).is_empty(), "{class:?}");
        }
        // Cities among them (two-sided, as every site's battle is).
        assert!(
            for_sites(Class::Desert)
                .iter()
                .any(|&i| matches!(MISSIONS[i].map_kind, MapKind::City(..)))
        );
        // Every desert mission is there from the start: the open desert's
        // Hold and Attrition both.
        for contract in [Contract::Hold, Contract::Attrition] {
            assert!(
                for_sites(Class::Desert).iter().any(|&i| {
                    let m = &MISSIONS[i];
                    m.map_kind == MapKind::Desert && m.contract == contract
                }),
                "{contract:?}"
            );
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
    fn raids_are_in_the_crystal_mine_and_the_rail_depot() {
        let raids: Vec<MapKind> = MISSIONS
            .iter()
            .filter(|m| m.contract == Contract::Raid)
            .map(|m| m.map_kind)
            .collect();
        assert_eq!(raids, [MapKind::Mine, MapKind::Depot(Look::Imperium)]);
    }
}
