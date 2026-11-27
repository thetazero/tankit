//! Tungsten cubes: what the Uthenn make, and nobody else wants.
//!
//! A planet with every production site the Uthenn's turns out cubes, as many
//! as there's tungsten for, and they're stashed there. They stay there
//! whoever takes the planet. Moving them between the stars costs crystal, as
//! hulls do, and takes as long.

use serde::{Deserialize, Serialize};

use super::{FTL_RANGE, Galaxy};
use crate::faction::Faction;

/// A planet's cubes: whole ones, and how far along the next is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Stash {
    pub cubes: u32,
    pub making: f32,
}

/// Cubes in flight, to a planet of system `to`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Cargo {
    pub to: u32,
    pub planet: u8,
    pub cubes: u32,
    /// The tick they land.
    pub arrives: u64,
}

impl Galaxy {
    /// System `s`'s planets' stashes.
    pub fn stashes_of(&self, s: usize) -> &[Stash] {
        let a = self.vault_at[s] as usize;
        &self.stashes[a..a + self.systems[s].planets as usize]
    }

    /// System `s`'s cubes: with `own`, those the Uthenn have there and those
    /// they'd make in `ticks`; else those they'd have, if they took the rest
    /// of it. (A planet's cubes go with its vault, with the whole planet.)
    pub(crate) fn cubes_in(&self, s: usize, own: bool, ticks: f32) -> f32 {
        let uthenn = Faction::Uthenn.index();
        let makes = self.makes_cubes(s);
        let (a, n) = (self.vault_at[s] as usize, self.systems[s].planets as usize);
        (0..n)
            .map(|p| {
                let stash = if (self.vaults[a + p].side as usize == uthenn) == own {
                    self.stashes[a + p].cubes as f32
                } else {
                    0.0
                };
                let made = if makes[p] == own {
                    self.rules.cubes * self.riches[a + p].tungsten * ticks
                } else {
                    0.0
                };
                stash + made
            })
            .sum()
    }

    /// Which of system `s`'s planets make cubes: those with no production
    /// site that isn't the Uthenn's.
    fn makes_cubes(&self, s: usize) -> [bool; 4] {
        let uthenn = Faction::Uthenn.index();
        let sys = &self.systems[s];
        let mut kept = [true; 4];
        for site in &self.sites[sys.sites.0 as usize..sys.sites.1 as usize] {
            kept[site.planet as usize] &= !site.kind.produces() || site.owner as usize == uthenn;
        }
        kept
    }

    /// Uthenn planets turn out their cubes.
    pub(crate) fn make_cubes(&mut self) {
        let uthenn = Faction::Uthenn.index();
        for (s, sys) in self.systems.iter().enumerate() {
            if sys.held[uthenn] == 0 {
                continue;
            }
            let kept = self.makes_cubes(s);
            let a = self.vault_at[s] as usize;
            for p in (0..sys.planets as usize).filter(|&p| kept[p]) {
                let stash = &mut self.stashes[a + p];
                stash.making += self.rules.cubes * self.riches[a + p].tungsten;
                let done = stash.making.floor();
                stash.cubes += done as u32;
                stash.making -= done;
            }
        }
    }

    /// Cubes in flight and due land, whoever holds the planet.
    pub(crate) fn land_cubes(&mut self) {
        let tick = self.tick;
        let mut cargo = std::mem::take(&mut self.cargo);
        cargo.retain(|x| {
            if tick < x.arrives {
                return true;
            }
            let v = self.vault_at[x.to as usize] as usize + x.planet as usize;
            self.stashes[v].cubes += x.cubes;
            false
        });
        self.cargo = cargo;
    }

    /// Send `n` cubes from planet `from` to planet `to`, each `(system,
    /// planet)`, of one Uthenn realm's systems in range of each other: as many
    /// as are there and it can pay to move. Returns how many went. (Nobody
    /// has a reason to yet.)
    #[allow(dead_code)]
    pub(crate) fn send_cubes(&mut self, from: (usize, u8), to: (usize, u8), n: u32) -> u32 {
        let uthenn = Faction::Uthenn.index();
        let r = self.systems[from.0].realms[uthenn];
        if r == super::realm::NONE
            || self.holder(from.0) != r
            || self.holder(to.0) != r
            || from.1 >= self.systems[from.0].planets
            || to.1 >= self.systems[to.0].planets
        {
            return 0;
        }
        let Some((_, d)) = self.neighbors(from.0).find(|&(t, _)| t == to.0) else {
            return 0;
        };
        let each = d / FTL_RANGE * self.rules.cube_cost;
        let purse = self.realms[r as usize].crystals.max(0.0);
        let v = self.vault_at[from.0] as usize + from.1 as usize;
        let sent = n
            .min(self.stashes[v].cubes)
            .min((purse / each.max(1e-6)) as u32);
        if sent == 0 {
            return 0;
        }
        self.stashes[v].cubes -= sent;
        let realm = &mut self.realms[r as usize];
        realm.crystals = (realm.crystals - sent as f32 * each).max(0.0);
        self.cargo.push(Cargo {
            to: to.0 as u32,
            planet: to.1,
            cubes: sent,
            arrives: self.tick + self.flight(d),
        });
        sent
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::galaxy::{REGION, Rules, SYSTEMS, SiteKind};

    fn war() -> Galaxy {
        Galaxy::with_size(4, SYSTEMS / 4, REGION * 0.5, Rules::default())
    }

    /// A planet makes cubes by its tungsten while every production site on it
    /// is the Uthenn's, and no other planet does; what's there stays.
    #[test]
    fn uthenn_planets_make_cubes() {
        let mut g = war();
        let uthenn = Faction::Uthenn.index();
        for _ in 0..50 {
            g.make_cubes();
        }
        for s in 0..g.systems.len() {
            let theirs = g.systems[s].owner() == uthenn;
            for (stash, rich) in g.stashes_of(s).iter().zip(g.riches_of(s)) {
                let want = if theirs {
                    50.0 * g.rules.cubes * rich.tungsten
                } else {
                    0.0
                };
                let has = stash.cubes as f32 + stash.making;
                assert!((has - want).abs() < 0.01 * want + 0.01, "{has} of {want}");
            }
        }
        let s = (0..g.systems.len())
            .find(|&s| g.systems[s].owner() == uthenn)
            .unwrap();
        let (a, b) = g.systems[s].sites;
        let of = |g: &Galaxy, kind: fn(SiteKind) -> bool| {
            (a as usize..b as usize)
                .find(|&i| g.sites[i].planet == 0 && kind(g.sites[i].kind))
                .unwrap()
        };
        // A generator lost changes nothing; a production site does.
        let before = g.stashes_of(s)[0];
        let shield = of(&g, |k| matches!(k, SiteKind::Shield { .. }));
        g.sites[shield].owner = ((uthenn + 1) % 4) as u8;
        g.make_cubes();
        let made = g.stashes_of(s)[0];
        assert!(made.cubes as f32 + made.making > before.cubes as f32 + before.making);
        let site = of(&g, SiteKind::produces);
        g.sites[site].owner = ((uthenn + 1) % 4) as u8;
        g.make_cubes();
        assert_eq!(g.stashes_of(s)[0], made);
    }

    /// Cubes fly for crystal, only as many as there are and can be paid for,
    /// between one Uthenn realm's systems, and land later; none are lost.
    #[test]
    fn cubes_fly_for_crystal() {
        let mut g = war();
        let uthenn = Faction::Uthenn.index();
        let (s, t, d) = (0..g.systems.len())
            .filter(|&s| g.systems[s].owner() == uthenn)
            .find_map(|s| {
                let r = g.holder(s);
                let (t, d) = g.neighbors(s).find(|&(t, _)| g.holder(t) == r)?;
                Some((s, t, d))
            })
            .unwrap();
        let r = g.holder(s) as usize;
        let v = g.vault_at[s] as usize;
        g.stashes[v].cubes = 500;
        g.realms[r].crystals = 1000.0;
        assert_eq!(g.send_cubes((s, 0), (t, 0), 800), 500);
        assert_eq!(g.stashes_of(s)[0].cubes, 0);
        let paid = 1000.0 - g.realms[r].crystals;
        let cost = 500.0 * d / FTL_RANGE * g.rules.cube_cost;
        assert!((paid - cost).abs() < 0.01, "{paid} of {cost}");
        g.land_cubes();
        assert_eq!(g.stashes_of(t)[0].cubes, 0, "not there yet");
        g.tick = g.cargo[0].arrives;
        g.land_cubes();
        assert_eq!(g.stashes_of(t)[0].cubes, 500);
        assert!(g.cargo.is_empty());
        // No crystal, no flight; nor to anyone else's.
        g.realms[r].crystals = 0.0;
        assert_eq!(g.send_cubes((t, 0), (s, 0), 500), 0);
        g.realms[r].crystals = 1000.0;
        let other = (0..g.systems.len())
            .find(|&o| g.holder(o) != r as u32)
            .unwrap();
        assert_eq!(g.send_cubes((t, 0), (other, 0), 500), 0);
    }
}
