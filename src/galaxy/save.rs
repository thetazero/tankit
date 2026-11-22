//! The galaxy on disk: `galaxy.json` beside the profile. Only what changes is
//! saved (who owns each site, which generators are down, every side's hulls
//! in every system and whose realm they are, the realms, the mercenary
//! companies, the clock and the dice); the layout of systems and sites is
//! regenerated from the seed.

use std::path::{Path, PathBuf};

use glam::{Vec2, Vec3};
use serde::{Deserialize, Serialize};

use super::merc::{Company, Contract, Debt};
use super::realm::{NONE, Realm};
use super::{Galaxy, Offensive, REGION, Rules, SIDES, SiteKind};
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
    /// Each system's hulls, side by side: `SIDES` numbers a system. Whole
    /// hulls, kept as numbers that needn't be (older saves had
    /// fractions of hulls: rounded on loading).
    hulls: Vec<f32>,
    /// Each system's yards, likewise: how far along their next hulls are.
    #[serde(default)]
    building: Vec<f32>,
    /// Each system's realm for each side, likewise (`u32::MAX` for none).
    tags: Vec<u32>,
    /// Every realm slot. (A galaxy saved before realms has none of these,
    /// and isn't read: a fresh one starts.)
    realms: Vec<SavedRealm>,
    splits: u64,
    merges: u64,
    /// The mercenary companies (a galaxy saved before them has none, and
    /// keeps the ones a fresh galaxy starts with).
    #[serde(default)]
    companies: Option<Vec<SavedCompany>>,
    #[serde(default)]
    next_company: u32,
    /// Bonuses realms owe companies: (realm, company id, crystals).
    #[serde(default)]
    debts: Vec<(u32, u32, f32)>,
    #[serde(default)]
    destroyed: f64,
    /// Your company's id (#350).
    #[serde(default)]
    crew: Option<u32>,
    /// Your own hulls on its contract (#385).
    #[serde(default)]
    own: Option<super::theater::Own>,
    /// Its last few ended contracts (#414).
    #[serde(default)]
    ended: Vec<super::theater::Ended>,
}

#[derive(Serialize, Deserialize)]
struct SavedCompany {
    at: u32,
    crystals: f32,
    hulls: f32,
    margin: f32,
    founded: u64,
    /// Saves from before #355 have none: idle since the save, then.
    #[serde(default)]
    idle_since: Option<u64>,
    id: u32,
    contract: Option<SavedContract>,
}

#[derive(Serialize, Deserialize)]
struct SavedContract {
    realm: u32,
    side: u8,
    target: u32,
    signed: u64,
    upfront: f32,
    bonus: f32,
    #[serde(alias = "max_loss")]
    committed: f32,
    field: f32,
    lost: f32,
    #[serde(default)]
    defend: bool,
}

#[derive(Serialize, Deserialize)]
struct SavedRealm {
    faction: u8,
    seat: u32,
    crystals: f32,
    income: f32,
    founded: u64,
    /// Where its offensive is aimed, and the last tick it took a site.
    offensive: Option<([f32; 3], u64)>,
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
                    SiteKind::Shield { damaged } => damaged as u8,
                    SiteKind::Production { .. } => 0,
                })
                .collect(),
            hulls: self
                .systems
                .iter()
                .flat_map(|s| s.hulls.map(|h| h as f32))
                .collect(),
            building: self.systems.iter().flat_map(|s| s.building).collect(),
            tags: self.systems.iter().flat_map(|s| s.realms).collect(),
            realms: self
                .realms
                .iter()
                .map(|r| SavedRealm {
                    faction: r.faction,
                    seat: r.seat,
                    crystals: r.crystals,
                    income: r.income,
                    founded: r.founded,
                    offensive: r.offensive.map(|o| (o.at.to_array(), o.gained)),
                })
                .collect(),
            splits: self.splits,
            merges: self.merges,
            companies: Some(
                self.companies
                    .iter()
                    .map(|c| SavedCompany {
                        at: c.at,
                        crystals: c.crystals,
                        hulls: c.hulls as f32,
                        margin: c.margin,
                        founded: c.founded,
                        idle_since: Some(c.idle_since),
                        id: c.id,
                        contract: c.contract.map(|k| SavedContract {
                            realm: k.realm,
                            side: k.side,
                            target: k.target,
                            signed: k.signed,
                            upfront: k.upfront,
                            bonus: k.bonus,
                            committed: k.committed as f32,
                            field: k.field as f32,
                            lost: k.lost as f32,
                            defend: k.defend,
                        }),
                    })
                    .collect(),
            ),
            next_company: self.next_company,
            debts: self
                .debts
                .iter()
                .map(|d| (d.realm, d.company, d.crystals))
                .collect(),
            destroyed: self.destroyed,
            crew: self.crew,
            own: self.own,
            ended: self.ended.clone(),
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
                && s.hulls.len() == g.systems.len() * SIDES
                && s.tags.len() == g.systems.len() * SIDES,
            "saved for a different layout"
        );
        for (i, site) in g.sites.iter_mut().enumerate() {
            anyhow::ensure!((s.owners[i] as usize) < SIDES, "a site with no side");
            site.owner = s.owners[i];
            if let SiteKind::Shield { damaged } = &mut site.kind {
                // (An older save counted repair ticks: any left is damaged.)
                *damaged = s.down[i] > 0;
            }
        }
        anyhow::ensure!(
            s.realms
                .iter()
                .all(|r| (r.faction as usize) < SIDES && (r.seat as usize) < g.systems.len()),
            "a realm with no side or seat"
        );
        anyhow::ensure!(
            s.tags
                .iter()
                .all(|&t| t == NONE || (t as usize) < s.realms.len()),
            "hulls of no realm"
        );
        for (i, sys) in g.systems.iter_mut().enumerate() {
            for f in 0..SIDES {
                sys.hulls[f] = whole(s.hulls[i * SIDES + f]);
                sys.building[f] = s.building.get(i * SIDES + f).copied().unwrap_or(0.0);
            }
            sys.realms
                .copy_from_slice(&s.tags[i * SIDES..(i + 1) * SIDES]);
            sys.held = [0; SIDES];
            for site in &g.sites[sys.sites.0 as usize..sys.sites.1 as usize] {
                sys.held[site.owner as usize] += 1;
            }
        }
        g.realms = s
            .realms
            .iter()
            .map(|r| Realm {
                crystals: r.crystals,
                income: r.income,
                offensive: r.offensive.map(|(at, gained)| Offensive {
                    at: Vec3::from_array(at),
                    gained,
                }),
                ..Realm::new(r.faction, r.seat, r.founded)
            })
            .collect();
        g.spans = vec![(0, 0); g.realms.len()];
        if let Some(companies) = s.companies {
            let (systems, realms) = (g.systems.len(), g.realms.len());
            anyhow::ensure!(
                companies.iter().all(|c| (c.at as usize) < systems
                    && c.contract
                        .as_ref()
                        .is_none_or(|k| (k.realm as usize) < realms
                            && (k.side as usize) < SIDES
                            && (k.target as usize) < systems)),
                "a company nowhere, or working for nobody"
            );
            g.companies = companies
                .into_iter()
                .map(|c| Company {
                    at: c.at,
                    crystals: c.crystals,
                    hulls: whole(c.hulls),
                    margin: c.margin,
                    founded: c.founded,
                    idle_since: c.idle_since.unwrap_or(s.tick),
                    id: c.id,
                    contract: c.contract.map(|k| Contract {
                        realm: k.realm,
                        side: k.side,
                        target: k.target,
                        signed: k.signed,
                        upfront: k.upfront,
                        bonus: k.bonus,
                        committed: whole(k.committed),
                        field: whole(k.field),
                        lost: whole(k.lost),
                        defend: k.defend,
                    }),
                })
                .collect();
            g.next_company = s.next_company;
            anyhow::ensure!(
                s.debts.iter().all(|&(r, _, _)| (r as usize) < realms),
                "a debt owed by nobody"
            );
            g.debts = s
                .debts
                .iter()
                .map(|&(realm, company, crystals)| Debt {
                    realm,
                    company,
                    crystals,
                })
                .collect();
        }
        g.destroyed = s.destroyed;
        g.crew = s.crew;
        g.own = s.own;
        g.ended = s.ended;
        (g.tick, g.splits, g.merges) = (s.tick, s.splits, s.merges);
        g.rng = Rng::from_state(s.rng);
        g.update_share();
        g.refresh();
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

    /// The galaxy, if it's in (never waits).
    pub fn try_get(&mut self) -> Option<&mut Galaxy> {
        if let Some(rx) = &self.job {
            match rx.try_recv() {
                Ok(g) => {
                    self.galaxy = Some(g);
                    self.job = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => return None,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.job = None,
            }
        }
        Some(self.galaxy.get_or_insert_with(Galaxy::fresh))
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
        g.crew = Some(g.companies[0].id);
        g.ended = vec![crate::galaxy::theater::Ended {
            ship: g.companies[0].id,
            signed: 3,
            tick: 20,
            outcome: crate::galaxy::theater::Outcome::Fell,
            defend: true,
            committed: 30,
            lost: 21,
            own: Default::default(),
        }];
        let text = serde_json::to_string(&g.saved(REGION * 0.3)).unwrap();
        let mut back = Galaxy::from_json(&text).unwrap();
        assert_eq!(back.tick, g.tick);
        assert_eq!(back.crew, g.crew);
        assert_eq!(back.ended, g.ended);
        for (a, b) in g.systems.iter().zip(&back.systems) {
            assert_eq!(a.held, b.held);
            assert_eq!(a.hulls, b.hulls);
            assert_eq!(a.realms, b.realms);
        }
        assert_eq!(g.living().count(), back.living().count());
        assert_eq!(g.companies.len(), back.companies.len());
        assert_eq!(
            g.debts
                .iter()
                .map(|d| (d.realm, d.company, d.crystals))
                .collect::<Vec<_>>(),
            back.debts
                .iter()
                .map(|d| (d.realm, d.company, d.crystals))
                .collect::<Vec<_>>()
        );
        for (a, b) in g.companies.iter().zip(&back.companies) {
            assert_eq!((a.at, a.crystals, a.hulls), (b.at, b.crystals, b.hulls));
            assert_eq!(a.contract.map(|k| k.target), b.contract.map(|k| k.target));
            assert_eq!(
                a.contract.map(|k| (k.field, k.defend)),
                b.contract.map(|k| (k.field, k.defend))
            );
        }
        for _ in 0..10 {
            g.step();
            back.step();
        }
        assert_eq!(g.share, back.share);
    }
}

/// A count of hulls as saved: whole (an older save may have
/// fractions, rounded).
fn whole(h: f32) -> u32 {
    h.max(0.0).round() as u32
}

/// Deserializes a count of hulls saved as a number that may have a
/// fraction (older saves), rounded to whole hulls.
pub(crate) fn whole_hulls<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    f32::deserialize(d).map(whole)
}
