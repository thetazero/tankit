//! Missions: a map plus a contract (game mode). Only one for now.

use crate::combat::{KILL_XP, ROUND_TIME, WIN_XP};

pub struct Mission {
    pub name: &'static str,
    /// The contract: what you're paid for.
    pub mode: &'static str,
    pub map: &'static str,
    /// Fixed map seed, so the mission is always the same place.
    pub seed: u64,
    pub bots: usize,
    pub brief: &'static str,
}

pub const MISSIONS: &[Mission] = &[Mission {
    name: "DEAD TOWN",
    mode: "ATTRITION",
    map: "DESERT TOWN",
    seed: 21,
    bots: 5,
    brief: "PAID PER KILL UNTIL THE CLOCK RUNS OUT",
}];

impl Mission {
    pub fn round_time(&self) -> String {
        let s = ROUND_TIME as u32;
        format!("{}:{:02}", s / 60, s % 60)
    }

    pub fn pay(&self) -> String {
        format!("{KILL_XP} XP PER KILL  +{WIN_XP} WIN")
    }
}
