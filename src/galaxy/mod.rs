//! The forever war as a simulation of its own (#303, from the design on #302).
//!
//! Some ten thousand colonized star systems, each a few numbers, ticking
//! forward: one tick of the whole galaxy per battle the player plays. Each
//! battle the player plays is drawn from it and fed back into it (`battle`,
//! #304); postings come later.
//!
//! **State.** A system is a point in a 2D region (light years) with a few
//! planets. Each planet has **sites**: production sites, each owned by a
//! faction and turning out hulls (into that faction's storage in the system)
//! and crystals (into the faction's one global supply: crystal is light, so
//! moving it costs nothing), and one or two **shield generators** (at least
//! one). Hull storage is per faction per system.
//!
//! **A tick:**
//! 1. *Production.* Every production site adds hulls and crystals to its owner.
//!    Hulls cost nothing to build, except a little crystal for the Resonant.
//!    Stored hulls wear out slowly, so an idle world's garrison levels off.
//! 2. *Battles.* Wherever two or more sides have hulls, they fight: a cheap
//!    Lanchester-style exchange, with a bonus for the side holding the ground.
//!    The tick's strongest side then makes progress on one planet: knocks down
//!    an enemy shield generator, or, once that planet's are all down, takes a
//!    site. A side alone in a system repairs its downed generators, one step a
//!    tick, several ticks to bring one back up.
//! 3. *Sending hulls.* Each faction looks at every system it could reach
//!    (within FTL range of systems where it has hulls to spare) and sends
//!    hulls where it sees a high chance of capture: several systems can send
//!    to one, so no defender is out of reach for long. FTL costs crystals per
//!    hull per light year. Landing on a planet under an enemy's shield is
//!    limited in rate and costs crystals per hull; once all of a planet's
//!    generators are down, landing is free.
//!
//! **What keeps the war forever:** defenders build hulls where they stand,
//! attackers pay to move theirs (a defensive asymmetry); and when one faction
//! holds too much of the galaxy, the other three stop fighting each other and
//! go for it together.

pub mod battle;
pub mod chart;
pub mod save;

use glam::Vec2;

use crate::faction::Faction;
use crate::rng::Rng;

/// The four warring sides (the Company holds no ground).
pub const SIDES: usize = 4;

/// Colonized systems in the war.
pub const SYSTEMS: usize = 10_000;
/// The region the war fills, in light years.
pub const REGION: Vec2 = Vec2::new(400.0, 250.0);
/// How far a faction sends hulls in one jump, light years.
pub const FTL_RANGE: f32 = 9.0;
/// Most systems a system can send to (the nearest ones within range).
const NEIGHBORS: usize = 12;

/// Crystals a hull costs to build, for the Resonant (whose hulls are vessels for a crystal).
const RESONANT_HULL_COST: f32 = 0.005;
/// Ticks to bring a downed shield generator back up.
const REPAIR_TICKS: u8 = 4;
/// Smallest sending worth the FTL (hulls).
const MIN_SEND: f32 = 2.0;
/// A system counts as at war when two sides each have this many hulls in it.
pub const AT_WAR: f32 = 0.5;

/// The numbers the war is tuned by.
#[derive(Clone, Copy, Debug)]
pub struct Rules {
    /// Hulls wear out at this fraction a tick (so a garrison levels off at
    /// production / wear).
    pub wear: f32,
    /// Crystals per hull per light year of FTL.
    pub ftl_cost: f32,
    /// Crystals per hull to land through an enemy's shield.
    pub breach_cost: f32,
    /// Hulls a tick that can land through one planet's shield (split between its
    /// generators: two generators halve it).
    pub breach_rate: f32,
    /// Fraction of each side's hulls the others destroy a tick, per unit of their fire.
    pub kill: f32,
    /// Fire bonus for a side fighting on ground it holds (all its sites in the system).
    pub home_ground: f32,
    /// Chance a tick that a side losing a battle still wins a raid on a shield
    /// generator, scaled by its share of the battle's fire.
    pub raid: f32,
    /// An attacker needs this many times the defense (its fire) to go in.
    pub odds: f32,
    /// ... and only this much when going for a dominant faction.
    pub gang_odds: f32,
    /// A faction holding more than this share of the galaxy's sites is dominant:
    /// the other three go for it together, and leave each other alone.
    pub dominant: f32,
    /// A side keeps at least this fraction of a quiet system's hulls home.
    pub keep: f32,
    /// An attack takes at least this share of what could reach the target.
    pub commit: f32,
    /// Ticks of crystal income a side keeps back from attacks, to pay for
    /// defending what it holds.
    pub reserve: f32,
    /// Most crystal a side can bank, in ticks of its income: the rest is
    /// lost to the war's waste (no side sits on a war chest for long).
    pub hoard: f32,
}

impl Default for Rules {
    fn default() -> Rules {
        Rules {
            wear: 0.03,
            ftl_cost: 0.25,
            breach_cost: 1.0,
            breach_rate: 12.0,
            kill: 0.35,
            home_ground: 0.3,
            raid: 0.5,
            odds: 1.5,
            gang_odds: 1.15,
            dominant: 0.3,
            keep: 0.3,
            commit: 0.6,
            reserve: 0.0,
            hoard: 30.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SiteKind {
    /// Turns out hulls (into its owner's storage in the system) and crystals
    /// (into its owner's global supply) each tick.
    Production { hulls: f32, crystals: f32 },
    /// A shield generator. `down`: 0 while up, else repair ticks still to go.
    Shield { down: u8 },
}

#[derive(Clone, Copy, Debug)]
pub struct Site {
    /// Which planet of its system it's on.
    pub planet: u8,
    /// Index into `Faction::ALL`.
    pub owner: u8,
    pub kind: SiteKind,
}

#[derive(Clone, Debug)]
pub struct System {
    /// Light years, in `REGION`.
    pub pos: Vec2,
    pub planets: u8,
    /// This system's sites, a range of `Galaxy::sites`.
    pub sites: (u32, u32),
    /// Each side's hulls here.
    pub hulls: [f32; SIDES],
    /// Each side's sites here (kept in step with `sites`).
    pub held: [u8; SIDES],
}

impl System {
    /// The side holding most of the system's sites.
    pub fn owner(&self) -> usize {
        (0..SIDES).max_by_key(|&f| self.held[f]).unwrap_or(0)
    }

    /// Two or more sides have hulls here.
    pub fn at_war(&self) -> bool {
        self.hulls.iter().filter(|&&h| h >= AT_WAR).count() >= 2
    }
}

/// The whole war.
pub struct Galaxy {
    /// What it was generated from (the layout of systems and sites).
    pub seed: u64,
    pub systems: Vec<System>,
    pub sites: Vec<Site>,
    /// Each system's reachable neighbors (nearest first), `NEIGHBORS` slots
    /// a system, `u32::MAX` where there are fewer.
    neighbors: Vec<(u32, f32)>,
    /// Each side's crystal supply.
    pub crystals: [f32; SIDES],
    /// Each side's crystal income last tick.
    pub income: [f32; SIDES],
    /// Each side's share of all sites (updated each tick).
    pub share: [f32; SIDES],
    pub rules: Rules,
    /// The faction the other three are ganging up on, if any.
    dominant: Option<usize>,
    pub tick: u64,
    /// This tick: hull sendings, shield generators destroyed, sites taken.
    pub sent: u32,
    pub shields_down: u32,
    pub taken: u32,
    rng: Rng,
    /// Scratch for sending: hulls each side can still send from each system this tick.
    spare: Vec<[f32; SIDES]>,
}

/// One tick's numbers, for charts and tests.
#[derive(Clone, Copy, Debug, Default)]
pub struct Census {
    pub tick: u64,
    /// Each side's share of all sites.
    pub share: [f32; SIDES],
    /// Fraction of systems at war.
    pub at_war: f32,
    pub hulls: [f32; SIDES],
    pub crystals: [f32; SIDES],
}

impl Galaxy {
    /// A fresh war from a seed: systems scattered over the region, the humans
    /// (Imperium, Resonant) holding the west and the aliens and machines
    /// (Ithessar, Uthenn) the east, overlapping in the middle.
    pub fn new(seed: u64) -> Galaxy {
        Galaxy::with_size(seed, SYSTEMS, REGION, Rules::default())
    }

    /// A war of `count` systems in a `region` of that size (tests use small
    /// ones), played by `rules`.
    pub fn with_size(seed: u64, count: usize, region: Vec2, rules: Rules) -> Galaxy {
        let mut rng = Rng::new(seed);
        // Where each faction's space centers: six heartlands each, on a
        // jittered 6x4 grid, the humans' checkered over the west half and the
        // aliens' and machines' over the east.
        let (imperium, resonant) = (Faction::Imperium.index(), Faction::Resonant.index());
        let (uthenn, ithessar) = (Faction::Uthenn.index(), Faction::Ithessar.index());
        let mut hearts: Vec<(Vec2, u8)> = Vec::new();
        for col in 0..6 {
            for row in 0..4 {
                let odd = (col + row) % 2 == 1;
                let f = match (col < 3, odd) {
                    (true, false) => imperium,
                    (true, true) => resonant,
                    (false, false) => ithessar,
                    (false, true) => uthenn,
                };
                let cell = Vec2::new(
                    (col as f32 + rng.range(0.2, 0.8)) / 6.0,
                    (row as f32 + rng.range(0.2, 0.8)) / 4.0,
                );
                hearts.push((cell * region, f as u8));
            }
        }
        let mut systems = Vec::with_capacity(count);
        let mut sites = Vec::new();
        for _ in 0..count {
            let pos = Vec2::new(rng.f32(), rng.f32()) * region;
            // Nearest heartland.
            let owner = hearts
                .iter()
                .min_by(|a, b| a.0.distance(pos).total_cmp(&b.0.distance(pos)))
                .map(|h| h.1)
                .unwrap_or(0);
            let planets = match rng.f32() {
                x if x < 0.5 => 1,
                x if x < 0.85 => 2,
                _ => 3,
            };
            let start = sites.len() as u32;
            for planet in 0..planets {
                // Most planets have one or two generators.
                let shields = match rng.f32() {
                    x if x < 0.55 => 1,
                    x if x < 0.95 => 2,
                    _ => 3,
                };
                for _ in 0..shields {
                    sites.push(Site {
                        planet,
                        owner,
                        kind: SiteKind::Shield { down: 0 },
                    });
                }
                for _ in 0..rng.int(1, 3) {
                    sites.push(Site {
                        planet,
                        owner,
                        kind: SiteKind::Production {
                            hulls: rng.range(0.1, 0.5),
                            crystals: rng.range(0.02, 0.1),
                        },
                    });
                }
            }
            let end = sites.len() as u32;
            let mut held = [0; SIDES];
            held[owner as usize] = (end - start) as u8;
            systems.push(System {
                pos,
                planets,
                sites: (start, end),
                hulls: [0.0; SIDES],
                held,
            });
        }
        let neighbors = find_neighbors(&systems, region);
        let mut g = Galaxy {
            seed,
            spare: vec![[0.0; SIDES]; systems.len()],
            systems,
            sites,
            neighbors,
            crystals: [50.0; SIDES],
            income: [0.0; SIDES],
            share: [0.0; SIDES],
            dominant: None,
            rules,
            tick: 0,
            sent: 0,
            shields_down: 0,
            taken: 0,
            rng,
        };
        // Start every garrison near where it would level off.
        for s in 0..g.systems.len() {
            let owner = g.systems[s].owner();
            g.systems[s].hulls[owner] = g.production(s)[owner] / rules.wear * 0.8;
        }
        g.update_share();
        g
    }

    /// Hulls each side builds in system `s` a tick.
    fn production(&self, s: usize) -> [f32; SIDES] {
        let mut out = [0.0; SIDES];
        let (a, b) = self.systems[s].sites;
        for site in &self.sites[a as usize..b as usize] {
            if let SiteKind::Production { hulls, .. } = site.kind {
                out[site.owner as usize] += hulls;
            }
        }
        out
    }

    pub fn neighbors(&self, s: usize) -> impl Iterator<Item = (usize, f32)> + '_ {
        self.neighbors[s * NEIGHBORS..(s + 1) * NEIGHBORS]
            .iter()
            .take_while(|n| n.0 != u32::MAX)
            .map(|&(n, d)| (n as usize, d))
    }

    /// The galaxy's state now, for charts and tests.
    pub fn census(&self) -> Census {
        let mut hulls = [0.0; SIDES];
        let mut war = 0;
        for s in &self.systems {
            for f in 0..SIDES {
                hulls[f] += s.hulls[f];
            }
            war += s.at_war() as usize;
        }
        Census {
            tick: self.tick,
            share: self.share,
            at_war: war as f32 / self.systems.len() as f32,
            hulls,
            crystals: self.crystals,
        }
    }

    pub(crate) fn update_share(&mut self) {
        let mut held = [0u32; SIDES];
        for s in &self.systems {
            for f in 0..SIDES {
                held[f] += s.held[f] as u32;
            }
        }
        let total = held.iter().sum::<u32>().max(1) as f32;
        self.share = held.map(|h| h as f32 / total);
        self.dominant = (0..SIDES).find(|&f| self.share[f] > self.rules.dominant);
    }

    /// Run the whole galaxy forward one tick.
    pub fn step(&mut self) {
        self.tick += 1;
        (self.sent, self.shields_down, self.taken) = (0, 0, 0);
        self.produce();
        self.battles();
        self.update_share();
        self.send();
    }

    fn produce(&mut self) {
        let mut crystals = [0.0; SIDES];
        let resonant = Faction::Resonant.index();
        for s in 0..self.systems.len() {
            let (a, b) = self.systems[s].sites;
            let sys = &mut self.systems[s];
            for h in &mut sys.hulls {
                *h *= 1.0 - self.rules.wear;
            }
            for site in &self.sites[a as usize..b as usize] {
                if let SiteKind::Production { hulls, crystals: c } = site.kind {
                    let f = site.owner as usize;
                    sys.hulls[f] += hulls;
                    crystals[f] += c;
                    if f == resonant {
                        crystals[f] -= hulls * RESONANT_HULL_COST;
                    }
                }
            }
        }
        for f in 0..SIDES {
            let cap = self.rules.hoard * crystals[f].max(1.0);
            self.crystals[f] = (self.crystals[f] + crystals[f]).clamp(0.0, cap);
        }
        self.income = crystals;
    }

    /// Each side's fire in system `s`: its hulls, with the home-ground bonus
    /// for the share of the system's sites it holds.
    fn fire(&self, s: usize) -> [f32; SIDES] {
        let sys = &self.systems[s];
        let total = (sys.sites.1 - sys.sites.0).max(1) as f32;
        let mut fire = [0.0; SIDES];
        for f in 0..SIDES {
            fire[f] = sys.hulls[f] * (1.0 + self.rules.home_ground * sys.held[f] as f32 / total);
        }
        fire
    }

    /// Are `f` and `g` fighting each other? Everyone is, except while one
    /// faction is dominant: then the other three are allies against it.
    fn hostile(&self, f: usize, g: usize) -> bool {
        hostile(self.dominant, f, g)
    }

    fn battles(&mut self) {
        let dom = self.dominant;
        for s in 0..self.systems.len() {
            let hulls = self.systems[s].hulls;
            let present = hulls.map(|h| h >= AT_WAR);
            let fighting = |f: usize| (0..SIDES).any(|g| present[g] && hostile(dom, f, g));
            if !(0..SIDES).any(|f| present[f] && fighting(f)) {
                // Nobody here to fight: each side holds what it has, repairs
                // its generators, and walks into any enemy site left unguarded.
                for f in 0..SIDES {
                    if present[f] {
                        self.hold(s, f);
                    }
                }
                continue;
            }
            let mut fire = self.fire(s);
            for x in &mut fire {
                *x *= self.rng.range(0.75, 1.25);
            }
            let mut losses = [0.0; SIDES];
            // Each side's fire spreads over its enemies here by how many hulls each has.
            for g in (0..SIDES).filter(|&g| present[g]) {
                let enemies = |f: usize| present[f] && hostile(dom, f, g);
                let targets: f32 = (0..SIDES).filter(|&f| enemies(f)).map(|f| hulls[f]).sum();
                for f in (0..SIDES).filter(|&f| enemies(f)) {
                    losses[f] += self.rules.kill * fire[g] * hulls[f] / targets;
                }
            }
            let sys = &mut self.systems[s];
            for f in 0..SIDES {
                sys.hulls[f] = (sys.hulls[f] - losses[f]).max(0.0);
            }
            // The tick goes to the side with the most fire.
            let winner = (0..SIDES)
                .filter(|&f| present[f])
                .max_by(|&a, &b| fire[a].total_cmp(&fire[b]))
                .unwrap_or(0);
            let against = |f: usize| -> f32 {
                (0..SIDES)
                    .filter(|&g| present[g] && hostile(dom, f, g))
                    .map(|g| fire[g])
                    .sum()
            };
            let margin = fire[winner] / against(winner).max(1e-3);
            self.advance(s, winner, margin);
            // The losers can still get at a shield generator: a raid on one
            // is won by the smaller side often enough that shields alone
            // never hold a world forever.
            for f in 0..SIDES {
                if f != winner && present[f] && fighting(f) {
                    let odds = (fire[f] / against(f).max(1e-3)).min(1.0);
                    if self.rng.chance(self.rules.raid * odds) {
                        self.raid(s, f);
                    }
                }
            }
        }
    }

    /// Up enemy shield generators on each planet of system `s`, as `f` sees them.
    fn shields_up(&self, s: usize, f: usize) -> [u8; 4] {
        let (a, b) = self.systems[s].sites;
        let mut up = [0u8; 4];
        for site in &self.sites[a as usize..b as usize] {
            if self.hostile(f, site.owner as usize) && site.kind == (SiteKind::Shield { down: 0 }) {
                up[site.planet as usize] += 1;
            }
        }
        up
    }

    /// Side `f` destroys an enemy shield generator in system `s`, on the
    /// planet with fewest of them up, if there is one.
    fn raid(&mut self, s: usize, f: usize) {
        let up = self.shields_up(s, f);
        let Some(p) = (0..self.systems[s].planets as usize)
            .filter(|&p| up[p] > 0)
            .min_by_key(|&p| up[p])
        else {
            return;
        };
        let (a, b) = self.systems[s].sites;
        let i = (a as usize..b as usize).find(|&i| {
            let site = &self.sites[i];
            site.planet as usize == p
                && self.hostile(f, site.owner as usize)
                && site.kind == (SiteKind::Shield { down: 0 })
        });
        if let Some(i) = i {
            self.sites[i].kind = SiteKind::Shield { down: REPAIR_TICKS };
            self.shields_down += 1;
        }
    }

    /// Side `f` has no enemy in system `s`: repairs its generators there,
    /// and walks into any enemy site nobody's left to defend.
    fn hold(&mut self, s: usize, f: usize) {
        let (a, b) = self.systems[s].sites;
        let mut enemy_sites = false;
        for i in a as usize..b as usize {
            let owner = self.sites[i].owner as usize;
            if owner == f {
                if let SiteKind::Shield { down } = &mut self.sites[i].kind {
                    *down = down.saturating_sub(1);
                }
            } else if self.hostile(f, owner) {
                enemy_sites = true;
            }
        }
        if enemy_sites {
            self.advance(s, f, 10.0);
        }
    }

    /// Side `w` won a tick in system `s` by `margin` (its fire over its
    /// enemies'): progress on the planet where it has least in its way.
    fn advance(&mut self, s: usize, w: usize, margin: f32) {
        let (a, b) = self.systems[s].sites;
        let (a, b) = (a as usize, b as usize);
        let planets = self.systems[s].planets as usize;
        let mut up = self.shields_up(s, w);
        let mut enemy = [0u8; 4];
        for site in &self.sites[a..b] {
            if self.hostile(w, site.owner as usize) {
                enemy[site.planet as usize] += 1;
            }
        }
        let Some(p) = (0..planets)
            .filter(|&p| enemy[p] > 0)
            .min_by_key(|&p| (up[p], enemy[p]))
        else {
            // Nothing of the enemy's here: a defender's won tick counts toward repairs.
            for site in &mut self.sites[a..b] {
                if site.owner as usize == w
                    && let SiteKind::Shield { down } = &mut site.kind
                {
                    *down = down.saturating_sub(1);
                }
            }
            return;
        };
        // A clear win takes more than one thing in a tick.
        let steps = if margin > 3.0 { 2 } else { 1 };
        for _ in 0..steps {
            // A shield up first; else the planet's production, its generators last.
            let candidate = |site: &Site, production: bool| {
                site.planet as usize == p
                    && self.hostile(w, site.owner as usize)
                    && if up[p] > 0 {
                        site.kind == (SiteKind::Shield { down: 0 })
                    } else {
                        matches!(site.kind, SiteKind::Production { .. }) == production
                    }
            };
            let i = (a..b)
                .find(|&i| candidate(&self.sites[i], true))
                .or_else(|| (a..b).find(|&i| candidate(&self.sites[i], false)));
            let Some(i) = i else { break };
            let site = &mut self.sites[i];
            if up[p] > 0 {
                // Destroy the generator (it stays its owner's, down).
                site.kind = SiteKind::Shield { down: REPAIR_TICKS };
                up[p] -= 1;
                self.shields_down += 1;
            } else {
                // Take the site. A taken generator has to be repaired.
                let old = site.owner as usize;
                site.owner = w as u8;
                if let SiteKind::Shield { down } = &mut site.kind {
                    *down = REPAIR_TICKS;
                }
                let sys = &mut self.systems[s];
                sys.held[old] -= 1;
                sys.held[w] += 1;
                self.taken += 1;
            }
        }
    }

    /// How side `f` can land in system `s`: (hulls a tick, crystals a hull).
    /// Free on any planet with no enemy generator up; else through the
    /// least-shielded planet, slowly and for crystals.
    fn landing(&self, s: usize, f: usize) -> (f32, f32) {
        let up = self.shields_up(s, f);
        let least = up[..self.systems[s].planets as usize]
            .iter()
            .copied()
            .min()
            .unwrap_or(0);
        if least == 0 {
            (f32::INFINITY, 0.0)
        } else {
            (
                self.rules.breach_rate / least as f32,
                self.rules.breach_cost,
            )
        }
    }

    /// Every side decides where to send its hulls.
    fn send(&mut self) {
        let n = self.systems.len();
        // What each side can spare where: hulls beyond what it keeps home.
        for s in 0..n {
            let sys = &self.systems[s];
            let contested = sys.at_war();
            for f in 0..SIDES {
                self.spare[s][f] = if contested || sys.held[f] == 0 {
                    // Fighting here (or here to fight): nothing to spare.
                    0.0
                } else {
                    sys.hulls[f] * (1.0 - self.rules.keep)
                };
            }
        }
        // A different faction moves first each tick, starting at a different
        // system, so nobody always gets first pick.
        let mut order = [0, 1, 2, 3];
        for i in (1..SIDES).rev() {
            order.swap(i, self.rng.index(i + 1));
        }
        let offset = self.rng.index(n);
        for f in order {
            for i in 0..n {
                self.consider((i + offset) % n, f);
            }
        }
    }

    /// Should side `f` send hulls to system `t`, and how many from where?
    fn consider(&mut self, t: usize, f: usize) {
        let sys = &self.systems[t];
        let fire = self.fire(t);
        // Who's in the way, and who's with us.
        let (mut defense, mut have, mut enemy, mut ours) = (0.0, 0.0, false, false);
        for g in 0..SIDES {
            if g == f || !self.hostile(f, g) {
                have += fire[g];
                ours |= sys.held[g] > 0;
            } else {
                defense += fire[g];
                enemy |= sys.held[g] > 0 || sys.hulls[g] >= AT_WAR;
            }
        }
        if !enemy {
            return;
        }
        let odds = if self.dominant.is_some_and(|d| d != f) {
            self.rules.gang_odds
        } else {
            self.rules.odds
        };
        // Already winning there by enough: nothing to add.
        if have >= odds * defense && defense > 0.0 {
            return;
        }
        let want = odds * defense * 1.2 + 1.0 - have;
        // What could reach it.
        let pool: f32 = self.neighbors(t).map(|(s, _)| self.spare[s][f]).sum();
        if have + pool < odds * defense || pool < MIN_SEND {
            return; // Not a good enough chance.
        }
        let (rate, breach) = self.landing(t, f);
        // Go in hard: a decisive blow, not a top-up the defender can match
        // next tick. What's left of it rolls on to the next world.
        let send = want.max(pool * self.rules.commit).min(pool).min(rate);
        if send < MIN_SEND {
            return;
        }
        // Pay for it: FTL by distance, plus the breach.
        let frac = send / pool;
        let cost: f32 = self
            .neighbors(t)
            .map(|(s, d)| self.spare[s][f] * frac * (d * self.rules.ftl_cost + breach))
            .sum();
        // Defending a world of ours (or an ally's) may spend everything;
        // an attack only what's above the reserve.
        let purse = if ours {
            self.crystals[f]
        } else {
            self.crystals[f] - self.rules.reserve * self.income[f].max(0.0)
        };
        let afford = if cost > 0.0 {
            (purse / cost).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let frac = frac * afford;
        if frac * pool < MIN_SEND {
            return;
        }
        self.crystals[f] -= cost * afford;
        self.sent += 1;
        let mut moved = 0.0;
        for k in 0..NEIGHBORS {
            let (s, _) = self.neighbors[t * NEIGHBORS + k];
            if s == u32::MAX {
                break;
            }
            let s = s as usize;
            let m = self.spare[s][f] * frac;
            self.spare[s][f] -= m;
            self.systems[s].hulls[f] -= m;
            moved += m;
        }
        self.systems[t].hulls[f] += moved;
    }
}

/// Are `f` and `g` fighting each other, while `dominant` is? Everyone is,
/// except while one faction is dominant: then the other three are allies.
fn hostile(dominant: Option<usize>, f: usize, g: usize) -> bool {
    f != g && dominant.is_none_or(|d| f == d || g == d)
}

/// Each system's nearest neighbors within FTL range, via a grid.
fn find_neighbors(systems: &[System], region: Vec2) -> Vec<(u32, f32)> {
    let cell = FTL_RANGE;
    let cols = (region.x / cell).ceil() as usize + 1;
    let rows = (region.y / cell).ceil() as usize + 1;
    let key = |p: Vec2| {
        let c = ((p.x / cell) as usize).min(cols - 1);
        let r = ((p.y / cell) as usize).min(rows - 1);
        (c, r)
    };
    let mut grid: Vec<Vec<u32>> = vec![Vec::new(); cols * rows];
    for (i, s) in systems.iter().enumerate() {
        let (c, r) = key(s.pos);
        grid[r * cols + c].push(i as u32);
    }
    let mut out = vec![(u32::MAX, 0.0); systems.len() * NEIGHBORS];
    let mut near: Vec<(u32, f32)> = Vec::new();
    for (i, s) in systems.iter().enumerate() {
        near.clear();
        let (c, r) = key(s.pos);
        for rr in r.saturating_sub(1)..=(r + 1).min(rows - 1) {
            for cc in c.saturating_sub(1)..=(c + 1).min(cols - 1) {
                for &j in &grid[rr * cols + cc] {
                    let d = systems[j as usize].pos.distance(s.pos);
                    if j as usize != i && d <= FTL_RANGE {
                        near.push((j, d));
                    }
                }
            }
        }
        near.sort_by(|a, b| a.1.total_cmp(&b.1));
        for (k, &n) in near.iter().take(NEIGHBORS).enumerate() {
            out[i * NEIGHBORS + k] = n;
        }
    }
    out
}

#[cfg(test)]
mod tests;

/// `--galaxy <ticks>`: run a war from `seed` for that many ticks, print how
/// it goes and how long a tick takes, and (with `out`) save the chart.
pub fn run(seed: u64, ticks: u64, out: Option<&str>) -> anyhow::Result<()> {
    let t0 = std::time::Instant::now();
    let mut g = Galaxy::new(seed);
    println!(
        "generated {} systems, {} sites in {:.1?}",
        g.systems.len(),
        g.sites.len(),
        t0.elapsed()
    );
    let mut history = vec![g.census()];
    let mut snaps = vec![chart::Snapshot::take(&g)];
    let every = (ticks / 25).max(1);
    let (mut total, mut worst) = (std::time::Duration::ZERO, std::time::Duration::ZERO);
    for _ in 0..ticks {
        let t = std::time::Instant::now();
        g.step();
        let dt = t.elapsed();
        total += dt;
        worst = worst.max(dt);
        let c = g.census();
        history.push(c);
        if g.tick % every == 0 {
            let names = Faction::ALL.map(|f| f.name());
            let shares: Vec<String> = (0..SIDES)
                .map(|f| format!("{} {:4.1}%", &names[f][..3], c.share[f] * 100.0))
                .collect();
            let hulls: f32 = c.hulls.iter().sum();
            println!(
                "tick {:6}  {}  at war {:4.1}%  sent {:4} shields {:4} taken {:4}  hulls {:8.0}  crystals {:?}",
                g.tick,
                shares.join("  "),
                c.at_war * 100.0,
                g.sent,
                g.shields_down,
                g.taken,
                hulls,
                c.crystals.map(|x| x as i64),
            );
        }
        if [ticks / 3, 2 * ticks / 3, ticks].contains(&g.tick) {
            snaps.push(chart::Snapshot::take(&g));
        }
    }
    println!(
        "tick: {:.2?} on average, {:.2?} at worst",
        total / ticks.max(1) as u32,
        worst
    );
    if let Some(out) = out {
        chart::render(&snaps, &history).save(out)?;
        println!("saved {out}");
    }
    Ok(())
}
