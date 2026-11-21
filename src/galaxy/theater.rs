//! The theater you're posted to (#311): one contested star system, as the
//! missions screen shows it.
//!
//! **Posting.** The Company posts you, for your employer, to a system where
//! that side is at war: the biggest such fight, kept until it's settled
//! (then you're posted to the next).
//!
//! **Contested sites.** Every side fighting in the system has a front: on each
//! planet with an enemy site, the enemy's shield generators still up, or, once
//! they're all down, the enemy's sites there. Those are the contested sites,
//! and any of them can be looked at.
//!
//! **Committed sites.** Only the battles your side's automated combat commits
//! to this tick can be entered: the site it attacks (the one the simulation's
//! tick would go for, see `Galaxy::target`) and any of its own sites an enemy
//! attacks. A side at war always has one: an attacker always has a target,
//! and a defender is always being attacked.
//!
//! **Maps.** Until there are generated maps for sites (#308), each site is
//! fought on one of the existing missions' maps, picked at random (stable for
//! the site).

use super::{AT_WAR, Galaxy, SIDES, Site, SiteKind};
use crate::economy::{PARTS, Part};
use crate::faction::Faction;
use crate::rng::Rng;

/// The theater as the missions screen shows it: a snapshot of one system.
#[derive(Clone, Debug)]
pub struct Theater {
    /// Index into `Galaxy::systems`.
    pub system: usize,
    pub name: String,
    /// Who you're posted for: index into `Faction::ALL`.
    pub employer: usize,
    pub planets: u8,
    /// Each side's hulls here, and sites held.
    pub hulls: [f32; SIDES],
    pub held: [u8; SIDES],
    /// Every site in the system, in the galaxy's order.
    pub sites: Vec<SiteView>,
    /// The contested sites, as indices into `sites`: the rows of the list.
    pub contested: Vec<usize>,
    /// The galaxy's tick this was taken at.
    pub tick: u64,
}

/// One site of the theater.
#[derive(Clone, Copy, Debug)]
pub struct SiteView {
    /// Index into `Galaxy::sites`.
    pub index: usize,
    pub planet: u8,
    /// Index into `Faction::ALL`.
    pub owner: usize,
    pub kind: SiteKind,
    pub contested: bool,
    /// Your side commits to a battle here this tick: you can enter it.
    pub committed: bool,
    /// Who you'd fight here: its owner if it's an enemy's, else whoever's
    /// attacking it (or the strongest enemy in the system).
    pub enemy: usize,
    /// The mission (index into `MISSIONS`) whose map and contract it's fought with.
    pub mission: usize,
}

/// A battle at a site, to deploy into.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sortie {
    pub system: usize,
    /// Index into `Galaxy::sites`.
    pub site: usize,
    /// Index into `MISSIONS`: the map and contract.
    pub mission: usize,
    /// The match's teams: you (your employer) first, then the enemy.
    pub factions: [Faction; 2],
}

/// A system's name: two or three syllables, the same for every pilot (it
/// comes from the system's catalogue number).
pub fn system_name(s: usize) -> String {
    const START: [&str; 24] = [
        "KES", "VAR", "THU", "OR", "SEL", "MAR", "IST", "DOR", "HAL", "NE", "CAR", "VEL", "ASH",
        "TOR", "YS", "BRE", "KAL", "ULM", "SER", "PHA", "ZEN", "GOR", "ETH", "LIR",
    ];
    const MID: [&str; 12] = [
        "A", "E", "I", "O", "U", "AN", "EL", "OR", "IS", "UN", "AR", "EN",
    ];
    const END: [&str; 16] = [
        "RA", "TH", "NE", "MOS", "DIS", "KAR", "LON", "VIA", "RUN", "SET", "TIS", "GAL", "NOR",
        "PHE", "XA", "DUN",
    ];
    let mut r = Rng::new(0x5EED_0000 ^ s as u64);
    let mut name = START[r.index(START.len())].to_string();
    if r.chance(0.6) {
        name += MID[r.index(MID.len())];
    }
    name += END[r.index(END.len())];
    name
}

/// Planet `p` of a system called `name`: "KESSARA II".
pub fn planet_name(name: &str, p: u8) -> String {
    const NUMERALS: [&str; 4] = ["I", "II", "III", "IV"];
    format!("{name} {}", NUMERALS[(p as usize).min(3)])
}

/// The schematic a win at site `site` (index into `Galaxy::sites`) steals
/// from `enemy`: one of theirs you don't have yet, or none (crystals only).
pub fn prize(enemy: Faction, site: usize, owns: impl Fn(Part) -> bool) -> Option<Part> {
    let left: Vec<Part> = PARTS
        .into_iter()
        .filter(|&p| p.maker() == enemy && !p.starter() && !owns(p))
        .collect();
    (!left.is_empty()).then(|| left[site % left.len()])
}

impl SiteView {
    /// What it is, in caps: a shield generator, or what a production site makes most of.
    pub fn kind_name(&self) -> &'static str {
        match self.kind {
            SiteKind::Shield { .. } => "SHIELD GENERATOR",
            SiteKind::Production { hulls, crystals } if crystals * 5.0 > hulls => "CRYSTAL MINE",
            SiteKind::Production { .. } => "HULL FOUNDRY",
        }
    }

    /// A stand-in site, for keys and legends: a shield generator, or a crystal mine.
    pub fn example(shield: bool) -> SiteView {
        SiteView {
            index: 0,
            planet: 0,
            owner: 0,
            kind: if shield {
                SiteKind::Shield { down: 0 }
            } else {
                SiteKind::Production {
                    hulls: 0.1,
                    crystals: 0.1,
                }
            },
            contested: false,
            committed: false,
            enemy: 1,
            mission: 0,
        }
    }

    pub fn is_shield(&self) -> bool {
        matches!(self.kind, SiteKind::Shield { .. })
    }

    /// A generator that's down: repair ticks still to go.
    pub fn down(&self) -> Option<u8> {
        match self.kind {
            SiteKind::Shield { down } if down > 0 => Some(down),
            _ => None,
        }
    }

    /// The schematic a win here steals (see `prize`).
    pub fn prize(&self, owns: impl Fn(Part) -> bool) -> Option<Part> {
        prize(Faction::ALL[self.enemy], self.index, owns)
    }
}

impl Theater {
    /// The site `k`th in the contested list.
    pub fn contested_site(&self, k: usize) -> Option<&SiteView> {
        self.contested.get(k).map(|&i| &self.sites[i])
    }

    /// The battle at contested site `k`, if your side commits to it.
    pub fn sortie(&self, k: usize) -> Option<Sortie> {
        let s = self.contested_site(k)?;
        s.committed.then(|| Sortie {
            system: self.system,
            site: s.index,
            mission: s.mission,
            factions: [Faction::ALL[self.employer], Faction::ALL[s.enemy]],
        })
    }

    /// The sides in the war here (hulls or sites), most hulls first.
    pub fn sides(&self) -> Vec<usize> {
        let mut out: Vec<usize> = (0..SIDES)
            .filter(|&f| self.hulls[f] >= AT_WAR || self.held[f] > 0)
            .collect();
        out.sort_by(|&a, &b| self.hulls[b].total_cmp(&self.hulls[a]));
        out
    }

    /// Generators up and down, all sides'.
    pub fn shields(&self) -> (usize, usize) {
        let gens = self.sites.iter().filter(|s| s.is_shield());
        let down = gens.clone().filter(|s| s.down().is_some()).count();
        (gens.count() - down, down)
    }
}

impl Galaxy {
    fn present(&self, s: usize) -> [bool; SIDES] {
        self.systems[s].hulls.map(|h| h >= AT_WAR)
    }

    /// Side `f` has hulls in system `s` and an enemy there with hulls too.
    pub fn fighting(&self, s: usize, f: usize) -> bool {
        let present = self.present(s);
        present[f] && (0..SIDES).any(|g| present[g] && self.hostile(f, g))
    }

    fn site_range(&self, s: usize) -> std::ops::Range<usize> {
        let (a, b) = self.systems[s].sites;
        a as usize..b as usize
    }

    /// Each planet of system `s`: (enemy generators up, enemy sites), as `f` sees them.
    fn in_the_way(&self, s: usize, f: usize) -> ([u8; 4], [u8; 4]) {
        let up = self.shields_up(s, f);
        let mut enemy = [0u8; 4];
        for site in &self.sites[self.site_range(s)] {
            if self.hostile(f, site.owner as usize) {
                enemy[site.planet as usize] += 1;
            }
        }
        (up, enemy)
    }

    /// The site side `f` would go for in system `s` this tick, if it won the
    /// tick: the planet with least in its way, and on it an enemy generator
    /// still up; once those are down, its production; its generators last.
    /// The same pick as the tick's `advance` makes first.
    pub fn target(&self, s: usize, f: usize) -> Option<usize> {
        let (up, enemy) = self.in_the_way(s, f);
        let p = (0..self.systems[s].planets as usize)
            .filter(|&p| enemy[p] > 0)
            .min_by_key(|&p| (up[p], enemy[p]))?;
        let candidate = |site: &Site, production: bool| {
            site.planet as usize == p
                && self.hostile(f, site.owner as usize)
                && if up[p] > 0 {
                    site.kind == (SiteKind::Shield { down: 0 })
                } else {
                    matches!(site.kind, SiteKind::Production { .. }) == production
                }
        };
        let r = self.site_range(s);
        r.clone()
            .find(|&i| candidate(&self.sites[i], true))
            .or_else(|| r.clone().find(|&i| candidate(&self.sites[i], false)))
    }

    /// The sites side `f` is fighting for in system `s`: on each planet with an
    /// enemy site, the enemy generators up, or (all down) the enemy's sites.
    pub fn front(&self, s: usize, f: usize) -> Vec<usize> {
        if !self.fighting(s, f) {
            return Vec::new();
        }
        let (up, _) = self.in_the_way(s, f);
        self.site_range(s)
            .filter(|&i| {
                let site = &self.sites[i];
                self.hostile(f, site.owner as usize)
                    && (up[site.planet as usize] == 0
                        || site.kind == (SiteKind::Shield { down: 0 }))
            })
            .collect()
    }

    /// The battles side `f` commits to in system `s` this tick: its own
    /// attack, and the defense of each of its sites an enemy attacks.
    pub fn commits(&self, s: usize, f: usize) -> Vec<usize> {
        if !self.fighting(s, f) {
            return Vec::new();
        }
        let present = self.present(s);
        let mut out: Vec<usize> = self.target(s, f).into_iter().collect();
        for g in (0..SIDES).filter(|&g| present[g] && self.hostile(f, g) && self.fighting(s, g)) {
            if let Some(t) = self.target(s, g)
                && self.sites[t].owner as usize == f
                && !out.contains(&t)
            {
                out.push(t);
            }
        }
        out.sort();
        out
    }

    /// Where the Company posts a pilot working for side `f`: `keep` if that's
    /// still a fight `f` is in, else the biggest fight it's in: the most
    /// sites at stake, then the most hulls.
    pub fn posting(&self, f: usize, keep: Option<usize>) -> Option<usize> {
        let ok = |s: usize| !self.commits(s, f).is_empty();
        if let Some(s) = keep.filter(|&s| s < self.systems.len() && ok(s)) {
            return Some(s);
        }
        (0..self.systems.len())
            .filter(|&s| self.fighting(s, f))
            .filter(|&s| ok(s))
            .max_by(|&a, &b| {
                let sites = |s: usize| self.site_range(s).len();
                let total = |s: usize| self.systems[s].hulls.iter().sum::<f32>();
                sites(a)
                    .cmp(&sites(b))
                    .then(total(a).total_cmp(&total(b)))
                    .then(b.cmp(&a))
            })
    }

    /// Side `f`'s theater: its posting (see `posting`), as the missions
    /// screen shows it, with maps picked from the missions open at `rank`.
    pub fn theater(&self, f: usize, keep: Option<usize>, rank: u32) -> Option<Theater> {
        let s = self.posting(f, keep)?;
        let sys = &self.systems[s];
        let present = self.present(s);
        let mut contested: Vec<usize> = Vec::new();
        for g in (0..SIDES).filter(|&g| present[g]) {
            contested.extend(self.front(s, g));
        }
        contested.sort();
        contested.dedup();
        let commits = self.commits(s, f);
        // Who attacks each site this tick.
        let mut attacker: Vec<Option<usize>> = vec![None; self.site_range(s).len()];
        for g in (0..SIDES).filter(|&g| self.fighting(s, g)) {
            if let Some(t) = self.target(s, g) {
                attacker[t - sys.sites.0 as usize].get_or_insert(g);
            }
        }
        let strongest = (0..SIDES)
            .filter(|&g| present[g] && self.hostile(f, g))
            .max_by(|&a, &b| sys.hulls[a].total_cmp(&sys.hulls[b]))
            .unwrap_or((f + 1) % SIDES);
        let pool = crate::mission::for_sites(rank);
        let sites: Vec<SiteView> = self
            .site_range(s)
            .map(|i| {
                let site = self.sites[i];
                let owner = site.owner as usize;
                let enemy = if self.hostile(f, owner) {
                    owner
                } else {
                    attacker[i - sys.sites.0 as usize]
                        .filter(|&g| self.hostile(f, g))
                        .unwrap_or(strongest)
                };
                let mut r = Rng::new(self.seed ^ (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
                SiteView {
                    index: i,
                    planet: site.planet,
                    owner,
                    kind: site.kind,
                    contested: contested.contains(&i),
                    committed: commits.contains(&i),
                    enemy,
                    mission: pool[r.index(pool.len())],
                }
            })
            .collect();
        let contested = (0..sites.len()).filter(|&k| sites[k].contested).collect();
        Some(Theater {
            system: s,
            name: system_name(s),
            employer: f,
            planets: sys.planets,
            hulls: sys.hulls,
            held: sys.held,
            sites,
            contested,
            tick: self.tick,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::galaxy::{REGION, Rules, SYSTEMS};
    use crate::mission::MISSIONS;

    fn galaxy() -> Galaxy {
        let mut g = Galaxy::with_size(3, SYSTEMS / 4, REGION * 0.5, Rules::default());
        for _ in 0..60 {
            g.step();
        }
        g
    }

    /// Every side is posted somewhere it's at war, and there's always a battle
    /// to join there.
    #[test]
    fn every_posting_has_a_battle_to_join() {
        let g = galaxy();
        for f in 0..SIDES {
            let t = g.theater(f, None, 1).expect("a posting");
            assert!(g.fighting(t.system, f));
            let committed: Vec<&SiteView> = t.sites.iter().filter(|s| s.committed).collect();
            assert!(!committed.is_empty(), "{}", Faction::ALL[f].name());
            for s in &committed {
                assert!(s.contested, "a committed site that isn't contested");
                assert_ne!(s.enemy, f);
                assert!(g.hostile(f, s.enemy));
            }
            assert!((0..t.contested.len()).any(|k| t.sortie(k).is_some()));
        }
    }

    /// Any war system: every fighting side's target is on its front, and
    /// contested sites are never the side's own.
    #[test]
    fn targets_are_on_the_front() {
        let g = galaxy();
        let mut seen = 0;
        for s in (0..g.systems.len()).filter(|&s| g.systems[s].at_war()) {
            for f in (0..SIDES).filter(|&f| g.fighting(s, f)) {
                let front = g.front(s, f);
                if let Some(t) = g.target(s, f) {
                    assert!(front.contains(&t));
                    seen += 1;
                }
                for &i in &front {
                    assert!(g.hostile(f, g.sites[i].owner as usize));
                }
            }
        }
        assert!(seen > 0, "no war anywhere");
    }

    /// The tick does to a side's target what `target` says: the site a
    /// winning side downs or takes is the one it was going for.
    #[test]
    fn the_tick_goes_for_the_target() {
        let mut g = galaxy();
        let mut checked = 0;
        let wars: Vec<usize> = (0..g.systems.len())
            .filter(|&s| g.systems[s].at_war())
            .collect();
        for s in wars {
            let sides: Vec<usize> = (0..SIDES).filter(|&f| g.fighting(s, f)).collect();
            for f in sides {
                let Some(t) = g.target(s, f) else { continue };
                let before = g.sites[t];
                let (sites, held) = (g.sites.clone(), g.systems[s].held);
                g.advance(s, f, 1.0);
                let after = g.sites[t];
                assert!(
                    after.owner != before.owner || after.kind != before.kind,
                    "the tick left the target alone"
                );
                g.sites = sites;
                g.systems[s].held = held;
                checked += 1;
            }
        }
        assert!(checked > 0);
    }

    /// A posting sticks while the fight goes on.
    #[test]
    fn a_posting_is_kept() {
        let g = galaxy();
        for f in 0..SIDES {
            let fights: Vec<usize> = (0..g.systems.len())
                .filter(|&s| !g.commits(s, f).is_empty())
                .collect();
            if let Some(&other) = fights.iter().rev().nth(1) {
                assert_eq!(g.posting(f, Some(other)), Some(other));
            }
        }
    }

    #[test]
    fn system_names_are_stable_and_varied() {
        assert_eq!(system_name(42), system_name(42));
        let names: std::collections::HashSet<String> = (0..200).map(system_name).collect();
        assert!(names.len() > 150, "{} names", names.len());
    }

    /// A site's map is one of the missions open at your rank, on two sides.
    #[test]
    fn sites_get_open_two_sided_maps() {
        let g = galaxy();
        let t = g.theater(0, None, 1).unwrap();
        for s in &t.sites {
            let m = &MISSIONS[s.mission];
            assert!(m.open_at(1) && m.sides.len() == 2, "{}", m.name);
        }
    }
}
