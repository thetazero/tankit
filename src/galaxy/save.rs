//! The galaxy on disk: `galaxy.json` beside the profile. Only what changes is
//! saved (who owns each site, which generators are down, every side's hulls
//! in every system, crystals, the clock and the dice); the layout of systems
//! and sites is regenerated from the seed.

use std::path::{Path, PathBuf};

use glam::Vec2;
use serde::{Deserialize, Serialize};

use super::{Galaxy, REGION, Rules, SIDES, SiteKind};
use crate::rng::Rng;

/// The war every new pilot joins: one seed for everyone.
pub const SEED: u64 = 1;
/// Ticks a new galaxy runs before the first battle, so it has fronts.
const WARMUP: u64 = 200;

#[derive(Serialize, Deserialize)]
struct Saved {
    seed: u64,
    systems: usize,
    region: [f32; 2],
    tick: u64,
    rng: u64,
    /// Each site's owner, and repair ticks to go (0 for production sites and
    /// generators that are up).
    owners: Vec<u8>,
    down: Vec<u8>,
    /// Each system's hulls, side by side: `SIDES` numbers a system.
    hulls: Vec<f32>,
    crystals: [f32; SIDES],
    income: [f32; SIDES],
}

impl Galaxy {
    fn saved(&self, region: Vec2) -> Saved {
        Saved {
            seed: self.seed,
            systems: self.systems.len(),
            region: region.to_array(),
            tick: self.tick,
            rng: self.rng.state(),
            owners: self.sites.iter().map(|s| s.owner).collect(),
            down: self
                .sites
                .iter()
                .map(|s| match s.kind {
                    SiteKind::Shield { down } => down,
                    SiteKind::Production { .. } => 0,
                })
                .collect(),
            hulls: self.systems.iter().flat_map(|s| s.hulls).collect(),
            crystals: self.crystals,
            income: self.income,
        }
    }

    /// The full galaxy as JSON.
    pub fn to_json(&self) -> String {
        serde_json::to_string(&self.saved(REGION)).unwrap_or_default()
    }

    fn restore(s: Saved) -> anyhow::Result<Galaxy> {
        let region = Vec2::from_array(s.region);
        let mut g = Galaxy::with_size(s.seed, s.systems, region, Rules::default());
        anyhow::ensure!(
            s.owners.len() == g.sites.len()
                && s.down.len() == g.sites.len()
                && s.hulls.len() == g.systems.len() * SIDES,
            "saved for a different layout"
        );
        for (i, site) in g.sites.iter_mut().enumerate() {
            anyhow::ensure!((s.owners[i] as usize) < SIDES, "a site with no side");
            site.owner = s.owners[i];
            if let SiteKind::Shield { down } = &mut site.kind {
                *down = s.down[i];
            }
        }
        for (i, sys) in g.systems.iter_mut().enumerate() {
            sys.hulls
                .copy_from_slice(&s.hulls[i * SIDES..(i + 1) * SIDES]);
            sys.held = [0; SIDES];
            for site in &g.sites[sys.sites.0 as usize..sys.sites.1 as usize] {
                sys.held[site.owner as usize] += 1;
            }
        }
        (g.tick, g.crystals, g.income) = (s.tick, s.crystals, s.income);
        g.rng = Rng::from_state(s.rng);
        g.update_share();
        Ok(g)
    }

    pub fn from_json(text: &str) -> anyhow::Result<Galaxy> {
        Galaxy::restore(serde_json::from_str(text)?)
    }

    /// A new galaxy for the game: the war already going.
    pub fn fresh() -> Galaxy {
        Galaxy::warmed(SEED)
    }

    /// A galaxy from `seed`, the war already going.
    pub fn warmed(seed: u64) -> Galaxy {
        let mut g = Galaxy::new(seed);
        for _ in 0..WARMUP {
            g.step();
        }
        g
    }

    pub fn default_path() -> Option<PathBuf> {
        crate::profile::Profile::default_path().map(|p| p.with_file_name("galaxy.json"))
    }

    /// The galaxy saved at `path`, or a fresh one if there's none (or it
    /// can't be read).
    pub fn load(path: &Path) -> Galaxy {
        match std::fs::read_to_string(path) {
            Ok(text) => match Galaxy::from_json(&text) {
                Ok(g) => {
                    log::info!("galaxy at tick {} from {}", g.tick, path.display());
                    return g;
                }
                Err(e) => log::warn!("galaxy {} unreadable, starting fresh: {e}", path.display()),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                log::info!("new galaxy at {}", path.display())
            }
            Err(e) => log::warn!("galaxy {} unreadable, starting fresh: {e}", path.display()),
        }
        Galaxy::fresh()
    }

    /// Write the galaxy to `path` on a worker thread (write, then rename).
    pub fn save(&self, path: &Path) {
        let (text, path) = (self.to_json(), path.to_path_buf());
        std::thread::spawn(move || {
            if let Err(e) = crate::profile::write_file(&path, &text) {
                log::warn!("couldn't save the galaxy to {}: {e:#}", path.display());
            }
        });
    }
}

/// The galaxy is generated whole if it's not on disk yet, which isn't free
/// (a few hundred ticks of war): so it's loaded on a worker thread at start,
/// and waited for (if it's not in yet) when a battle needs it.
pub struct War {
    job: Option<std::sync::mpsc::Receiver<Galaxy>>,
    galaxy: Option<Galaxy>,
    path: Option<PathBuf>,
}

impl War {
    /// Load the saved galaxy (or make one) in the background.
    pub fn load() -> War {
        let path = Galaxy::default_path();
        let (tx, rx) = std::sync::mpsc::channel();
        let from = path.clone();
        std::thread::spawn(move || {
            let g = match &from {
                Some(p) => Galaxy::load(p),
                None => Galaxy::fresh(),
            };
            let _ = tx.send(g);
        });
        War {
            job: Some(rx),
            galaxy: None,
            path,
        }
    }

    /// The galaxy, waiting for it if it's still loading.
    pub fn get(&mut self) -> &mut Galaxy {
        if let Some(rx) = self.job.take() {
            self.galaxy = Some(rx.recv().unwrap_or_else(|_| Galaxy::fresh()));
        }
        self.galaxy.get_or_insert_with(Galaxy::fresh)
    }

    pub fn save(&mut self) {
        if let Some(path) = self.path.clone() {
            self.get().save(&path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::galaxy::SYSTEMS;

    /// A saved galaxy comes back the same, and goes on the same.
    #[test]
    fn a_saved_galaxy_picks_up_where_it_left_off() {
        let mut g = Galaxy::with_size(5, SYSTEMS / 10, REGION * 0.3, Rules::default());
        for _ in 0..30 {
            g.step();
        }
        let text = serde_json::to_string(&g.saved(REGION * 0.3)).unwrap();
        let mut back = Galaxy::from_json(&text).unwrap();
        assert_eq!(back.tick, g.tick);
        for (a, b) in g.systems.iter().zip(&back.systems) {
            assert_eq!(a.held, b.held);
            assert_eq!(a.hulls, b.hulls);
        }
        for _ in 0..10 {
            g.step();
            back.step();
        }
        assert_eq!(g.share, back.share);
    }
}
