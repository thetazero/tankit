//! The pilot profile: XP, rank and the loadout you deploy in. One profile,
//! saved as JSON in the OS data directory (`~/.local/share/tankit/profile.json`
//! on Linux) whenever it changes, so there's no save button.
//!
//! Rank is derived from XP. Later ranks will unlock parts; for now it's just
//! tracked and shown.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::garage::{self, Loadout};
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

/// What's on disk. Parts are stored by name so the file stays readable.
#[derive(Serialize, Deserialize)]
struct Saved {
    xp: u32,
    hull: String,
    turret: String,
}

pub struct Profile {
    pub xp: u32,
    /// The kit you deploy in (and respawn in, until the garage changes it).
    pub loadout: Loadout,
    /// Where to save; `None` keeps the profile in memory only (headless runs, tests).
    path: Option<PathBuf>,
}

impl Profile {
    /// A fresh pilot that is never written to disk.
    pub fn in_memory() -> Profile {
        Profile {
            xp: 0,
            loadout: (HullKind::Lancer, TurretKind::Lance),
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
                Ok(s) => {
                    p.xp = s.xp;
                    let hull = garage::HULLS
                        .iter()
                        .find(|&&k| garage::hull_name(k).eq_ignore_ascii_case(&s.hull));
                    let turret = garage::TURRETS
                        .iter()
                        .find(|&&k| garage::turret_name(k).eq_ignore_ascii_case(&s.turret));
                    p.loadout = (
                        hull.copied().unwrap_or(p.loadout.0),
                        turret.copied().unwrap_or(p.loadout.1),
                    );
                }
                Err(e) => log::warn!("profile {} unreadable, starting fresh: {e}", path.display()),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                log::info!("new pilot profile at {}", path.display())
            }
            Err(e) => log::warn!("profile {} unreadable, starting fresh: {e}", path.display()),
        }
        p
    }

    pub fn rank(&self) -> Rank {
        Rank::of(self.xp)
    }

    /// Bank XP and save.
    pub fn add_xp(&mut self, xp: u32) {
        if xp > 0 {
            self.xp += xp;
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
        let saved = Saved {
            xp: self.xp,
            hull: garage::hull_name(self.loadout.0).to_ascii_lowercase(),
            turret: garage::turret_name(self.loadout.1).to_ascii_lowercase(),
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
        p.add_xp(30);
        p.set_loadout((HullKind::Bastion, TurretKind::Maul));

        let back = Profile::load(path.clone());
        assert_eq!(back.xp, 30);
        assert_eq!(back.loadout, (HullKind::Bastion, TurretKind::Maul));

        // A corrupt file starts a fresh pilot rather than failing.
        std::fs::write(&path, "not json").unwrap();
        assert_eq!(Profile::load(path).xp, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
