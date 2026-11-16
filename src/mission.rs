//! Missions: a map plus a contract (game mode).

use crate::MapKind;
use crate::combat::{KILL_XP, ROUND_TIME};
use crate::hold;

/// What a mission pays you for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Contract {
    /// Deathmatch: paid per kill until the clock runs out.
    #[default]
    Attrition,
    /// King of the hill: paid while you're alone on the site.
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
    pub bots: usize,
    pub brief: &'static str,
}

pub const MISSIONS: &[Mission] = &[
    Mission {
        name: "DEAD TOWN",
        contract: Contract::Attrition,
        map: "SALTMARKET",
        map_kind: MapKind::Saltmarket,
        seed: 0,
        bots: 5,
        brief: "PAID PER KILL UNTIL THE CLOCK RUNS OUT",
    },
    Mission {
        name: "MARKET HALL",
        contract: Contract::Hold,
        map: "SALTMARKET",
        map_kind: MapKind::Saltmarket,
        seed: 0,
        bots: 5,
        brief: "PAID WHILE YOU HOLD THE HALL ALONE",
    },
];

impl Mission {
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
            Contract::Attrition => format!("CRYSTALS · {KILL_XP} XP PER KILL"),
            Contract::Hold => format!("CRYSTALS · {} XP / {:.0} S", hold::HOLD_XP, hold::XP_EVERY),
        }
    }
}
