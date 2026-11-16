//! The pilot profile: XP, rank and the loadout you deploy in. One profile,
//! saved as JSON in the OS data directory (`~/.local/share/tankit/profile.json`
//! on Linux) whenever it changes, so there's no save button.
//!
//! Rank is derived from XP and decides which parts are for sale. Crystals,
//! paid for finished contracts, buy parts and upgrades.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::economy::{Levels, PARTS, Part, upgrade_cost};
use crate::garage::Loadout;
use crate::sim::{HullKind, TurretKind};

/// XP from one rank to the next: 100, then 50 more for each rank after.
fn rank_span(rank: u32) -> u32 {
    100 + 50 * (rank - 1)
}

/// Where a given XP total sits on the rank ladder.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rank {
    /// Starts at 1.
    pub rank: u32,
    /// XP earned since reaching this rank.
    pub into: u32,
    /// XP this rank takes to get through.
    pub span: u32,
}

impl Rank {
    pub fn of(xp: u32) -> Rank {
        let (mut rank, mut left) = (1, xp);
        while left >= rank_span(rank) {
            left -= rank_span(rank);
            rank += 1;
        }
        Rank {
            rank,
            into: left,
            span: rank_span(rank),
        }
    }

    pub fn progress(&self) -> f32 {
        self.into as f32 / self.span as f32
    }
}

/// What's on disk. Parts are stored by name so the file stays readable;
/// fields added later default, so older profiles still load.
#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct Saved {
    xp: u32,
    crystals: u32,
    hull: String,
    turret: String,
    /// Bought parts (starters are always owned).
    owned: Vec<String>,
    /// Upgrade level by part name; missing means M0.
    levels: BTreeMap<String, u8>,
}

/// Why a purchase didn't go through.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Refusal {
    Owned,
    NotOwned,
    Rank(u32),
    Crystals(u32),
    MaxLevel,
}

pub struct Profile {
    pub xp: u32,
    pub crystals: u32,
    /// The kit you deploy in (and respawn in, until the garage changes it).
    pub loadout: Loadout,
    /// Bought parts, starters not included.
    bought: Vec<Part>,
    pub levels: Levels,
    /// Where to save; `None` keeps the profile in memory only (headless runs, tests).
    path: Option<PathBuf>,
}

impl Profile {
    /// A fresh pilot that is never written to disk.
    pub fn in_memory() -> Profile {
        Profile {
            xp: 0,
            crystals: 0,
            loadout: (HullKind::Lancer, TurretKind::Lance),
            bought: Vec::new(),
            levels: Levels::default(),
            path: None,
        }
    }

    pub fn default_path() -> Option<PathBuf> {
        dirs::data_dir().map(|d| d.join("tankit").join("profile.json"))
    }

    /// Load from `path`, or start a fresh pilot there if it's missing or unreadable.
    pub fn load(path: PathBuf) -> Profile {
        let mut p = Profile {
            path: Some(path.clone()),
            ..Profile::in_memory()
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<Saved>(&text) {
                Ok(s) => p.restore(s),
                Err(e) => log::warn!("profile {} unreadable, starting fresh: {e}", path.display()),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                log::info!("new pilot profile at {}", path.display())
            }
            Err(e) => log::warn!("profile {} unreadable, starting fresh: {e}", path.display()),
        }
        p
    }

    fn restore(&mut self, s: Saved) {
        self.xp = s.xp;
        self.crystals = s.crystals;
        self.bought = s.owned.iter().filter_map(|n| Part::by_name(n)).collect();
        for (name, level) in &s.levels {
            if let Some(part) = Part::by_name(name) {
                self.levels.set(part, *level);
            }
        }
        // Only deploy in parts you own (older profiles predate buying).
        if let Some(Part::Hull(h)) = Part::by_name(&s.hull).filter(|&p| self.owns(p)) {
            self.loadout.0 = h;
        }
        if let Some(Part::Turret(t)) = Part::by_name(&s.turret).filter(|&p| self.owns(p)) {
            self.loadout.1 = t;
        }
    }

    pub fn rank(&self) -> Rank {
        Rank::of(self.xp)
    }

    pub fn owns(&self, part: Part) -> bool {
        part.price().is_none() || self.bought.contains(&part)
    }

    pub fn owns_kit(&self, kit: Loadout) -> bool {
        self.owns(Part::Hull(kit.0)) && self.owns(Part::Turret(kit.1))
    }

    /// Can `part` be bought right now? Its price if so.
    pub fn can_buy(&self, part: Part) -> Result<u32, Refusal> {
        let Some(price) = part.price().filter(|_| !self.owns(part)) else {
            return Err(Refusal::Owned);
        };
        if self.rank().rank < price.rank {
            return Err(Refusal::Rank(price.rank));
        }
        if self.crystals < price.crystals {
            return Err(Refusal::Crystals(price.crystals));
        }
        Ok(price.crystals)
    }

    pub fn buy(&mut self, part: Part) -> Result<(), Refusal> {
        let cost = self.can_buy(part)?;
        self.crystals -= cost;
        self.bought.push(part);
        self.save();
        Ok(())
    }

    /// Can `part` be upgraded right now? The cost if so.
    pub fn can_upgrade(&self, part: Part) -> Result<u32, Refusal> {
        if !self.owns(part) {
            return Err(Refusal::NotOwned);
        }
        let cost = upgrade_cost(self.levels.of(part)).ok_or(Refusal::MaxLevel)?;
        if self.crystals < cost {
            return Err(Refusal::Crystals(cost));
        }
        Ok(cost)
    }

    pub fn upgrade(&mut self, part: Part) -> Result<(), Refusal> {
        let cost = self.can_upgrade(part)?;
        self.crystals -= cost;
        self.levels.set(part, self.levels.of(part) + 1);
        self.save();
        Ok(())
    }

    /// Bank XP and save.
    pub fn add_xp(&mut self, xp: u32) {
        if xp > 0 {
            self.xp += xp;
            self.save();
        }
    }

    /// Bank contract pay and save.
    pub fn add_crystals(&mut self, crystals: u32) {
        if crystals > 0 {
            self.crystals += crystals;
            self.save();
        }
    }

    /// Change the deploy kit and save.
    pub fn set_loadout(&mut self, kit: Loadout) {
        if kit != self.loadout {
            self.loadout = kit;
            self.save();
        }
    }

    fn save(&self) {
        let Some(path) = &self.path else { return };
        if let Err(e) = self.write(path) {
            log::warn!("couldn't save profile to {}: {e:#}", path.display());
        }
    }

    fn write(&self, path: &Path) -> anyhow::Result<()> {
        let name = |p: Part| p.name().to_ascii_lowercase();
        let saved = Saved {
            xp: self.xp,
            crystals: self.crystals,
            hull: name(Part::Hull(self.loadout.0)),
            turret: name(Part::Turret(self.loadout.1)),
            owned: self.bought.iter().map(|&p| name(p)).collect(),
            levels: PARTS
                .iter()
                .filter(|&&p| self.levels.of(p) > 0)
                .map(|&p| (name(p), self.levels.of(p)))
                .collect(),
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        // Write then rename, so a crash mid-save can't leave a half-written profile.
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(&saved)?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rank_ladder() {
        assert_eq!(
            Rank::of(0),
            Rank {
                rank: 1,
                into: 0,
                span: 100
            }
        );
        assert_eq!(Rank::of(99).rank, 1);
        assert_eq!(
            Rank::of(100),
            Rank {
                rank: 2,
                into: 0,
                span: 150
            }
        );
        assert_eq!(
            Rank::of(260),
            Rank {
                rank: 3,
                into: 10,
                span: 200
            }
        );
    }

    #[test]
    fn saves_and_loads() {
        let dir = std::env::temp_dir().join(format!("tankit-profile-test-{}", std::process::id()));
        let path = dir.join("profile.json");
        let _ = std::fs::remove_dir_all(&dir);

        let mut p = Profile::load(path.clone());
        assert_eq!(p.xp, 0);
        p.add_xp(300);
        p.add_crystals(2000);
        p.buy(MAUL).unwrap();
        p.upgrade(MAUL).unwrap();
        p.set_loadout((HullKind::Lancer, TurretKind::Maul));

        let back = Profile::load(path.clone());
        assert_eq!(back.xp, 300);
        assert_eq!(back.crystals, p.crystals);
        assert!(back.owns(MAUL));
        assert_eq!(back.levels.of(MAUL), 1);
        assert_eq!(back.loadout, (HullKind::Lancer, TurretKind::Maul));

        // An older profile (no crystals, deploying in a part it never bought) still loads.
        std::fs::write(&path, r#"{"xp": 40, "hull": "bastion", "turret": "lance"}"#).unwrap();
        let old = Profile::load(path.clone());
        assert_eq!((old.xp, old.crystals), (40, 0));
        assert_eq!(old.loadout, (HullKind::Lancer, TurretKind::Lance));

        // A corrupt file starts a fresh pilot rather than failing.
        std::fs::write(&path, "not json").unwrap();
        assert_eq!(Profile::load(path).xp, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    const MAUL: Part = Part::Turret(TurretKind::Maul);

    #[test]
    fn buying_needs_rank_and_crystals() {
        let mut p = Profile::in_memory();
        assert!(p.owns(Part::Hull(HullKind::Lancer)));
        assert!(!p.owns(MAUL));
        let price = MAUL.price().unwrap();
        assert_eq!(p.buy(MAUL), Err(Refusal::Rank(price.rank)));
        p.xp = 10_000;
        assert_eq!(p.buy(MAUL), Err(Refusal::Crystals(price.crystals)));
        assert_eq!(p.upgrade(MAUL), Err(Refusal::NotOwned));
        p.crystals = price.crystals;
        p.buy(MAUL).unwrap();
        assert_eq!(p.crystals, 0);
        assert_eq!(p.buy(MAUL), Err(Refusal::Owned));
        p.crystals = 100_000;
        for _ in 0..crate::economy::MAX_LEVEL {
            p.upgrade(MAUL).unwrap();
        }
        assert_eq!(p.upgrade(MAUL), Err(Refusal::MaxLevel));
    }
}
