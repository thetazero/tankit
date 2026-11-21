//! The forever war as a simulation of its own (#303, from the design on #302).
//!
//! Some ten thousand colonized star systems, each a few numbers, ticking
//! forward: one tick of the whole galaxy per battle the player plays. Nothing
//! in the game reads it yet; hooking it up to the player's battles and to
//! postings comes later.
//!
//! **State.** A system is a point in a slab of a spiral galaxy's disk (light
//! years; see `structure`: two arms, the gap between, a spur across it) with
//! a few planets. Each planet has **sites**: production sites, each owned by a
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
//! 3. *Supply.* A side's systems that hang together (within FTL range of
//!    each other) fewer than a few dozen are a **pocket**: once an enemy
//!    borders it, it builds slower, can't repair its generators, and every
//!    enemy around goes for it. Enclaves don't last.
//! 4. *Offensives.* A side banks crystals until it can afford a push, aims it
//!    at the stretch of its border where it's strongest against the defense,
//!    brings hulls up toward it, and fights there with the initiative (a
//!    fire bonus). The push rolls on from what it takes, until the crystals
//!    run low or it stalls. Then quiet, while it banks for the next.
//! 5. *Sending hulls.* Each faction looks at every system it could reach
//!    (within FTL range of systems where it has hulls to spare) and sends
//!    hulls where it sees a high chance of capture: to defend anywhere, but
//!    to attack only within its offensive, or a pocket. Several systems can
//!    send to one. FTL costs crystals per hull by distance. Landing on a planet under an enemy's shield is
//!    limited in rate and costs crystals per hull; once all of a planet's
//!    generators are down, landing is free.
//!
//! A step runs ticks until somewhere is at war: the player is posted to a
//! battle, so quiet ticks pass unseen.
//!
//! **What keeps the war forever:** defenders build hulls where they stand,
//! attackers pay to move theirs (a defensive asymmetry); and when one faction
//! holds too much of the galaxy, the sides bordering it stop fighting each
//! other and go for it together, until it's well back under the line.

pub mod chart;
pub mod structure;

use glam::{Vec2, Vec3};

use crate::faction::Faction;
use crate::rng::Rng;

/// The four warring sides (the Company holds no ground).
pub const SIDES: usize = 4;

/// Colonized systems in the war.
pub const SYSTEMS: usize = 10_000;
/// The region the war fills, in light years (in the galaxy's plane; see
/// `structure` for its depth and shape).
pub const REGION: Vec2 = Vec2::new(8800.0, 5500.0);
/// How far a faction sends hulls in one jump, light years.
pub const FTL_RANGE: f32 = 250.0;
/// Most systems a system can send to (the nearest ones within range).
const NEIGHBORS: usize = 12;

/// Crystals a hull costs to build, for the Resonant (whose hulls are vessels for a crystal).
const RESONANT_HULL_COST: f32 = 0.005;
/// Ticks to bring a downed shield generator back up.
const REPAIR_TICKS: u8 = 4;
/// Smallest sending worth the FTL (hulls).
const MIN_SEND: f32 = 2.0;
/// Most quiet ticks a step skips looking for the next war.
const QUIET_SKIP: usize = 1000;
/// A system counts as at war when two sides each have this many hulls in it.
const AT_WAR: f32 = 0.5;

/// The numbers the war is tuned by.
#[derive(Clone, Copy, Debug)]
pub struct Rules {
    /// Hulls wear out at this fraction a tick (so a garrison levels off at
    /// production / wear).
    pub wear: f32,
    /// Crystals per hull for an FTL jump of the full range (shorter ones cost less).
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
    /// ... and only this much when going for a dominant faction (or a pocket),
    pub gang_odds: f32,
    /// ... and this much anywhere off its offensive.
    pub opportunity: f32,
    /// A faction holding more than this share of the galaxy's sites is dominant:
    /// the other three go for it together, and leave each other alone...
    pub dominant: f32,
    /// ... until it's back under this share. A side joins the alliance only
    /// if this many of its systems border the dominant faction.
    pub release: f32,
    pub ally_border: u32,
    /// A side's systems hanging together (by FTL range) fewer than this are a
    /// pocket, cut off from the rest: once an enemy borders it, it builds hulls
    /// slower and can't repair its shield generators, and the enemy goes for it.
    pub pocket: u32,
    /// How much of its hull production a pocket keeps.
    pub cut_off: f32,
    /// A side launches an offensive once it has banked this many ticks of
    /// crystal income...
    pub launch: f32,
    /// ... and it runs until the bank is down to this many ticks of income,
    pub spent: f32,
    /// ... or until this many ticks pass without taking a site.
    pub stall: u32,
    /// Fire bonus for a side fighting where its offensive is: it picked the
    /// time and place, and massed for it.
    pub initiative: f32,
    /// Most a side can take of a system in one won tick (sites or generators).
    pub blitz: u32,
    /// How far from where it's aimed an offensive attacks, light years...
    pub front: f32,
    /// ... and how far behind it hulls are brought up toward it.
    pub staging: f32,
    /// Share of a quiet system's spare hulls brought up a tick...
    pub bring_up: f32,
    /// ... spending at most this many ticks of crystal income a tick on it.
    pub tempo: f32,
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
            ftl_cost: 2.25,
            breach_cost: 1.0,
            breach_rate: 12.0,
            kill: 0.35,
            home_ground: 0.3,
            raid: 0.5,
            odds: 1.5,
            gang_odds: 1.15,
            opportunity: 3.0,
            dominant: 0.3,
            release: 0.27,
            ally_border: 20,
            pocket: 20,
            cut_off: 0.4,
            launch: 60.0,
            spent: 2.0,
            stall: 30,
            initiative: 1.0,
            blitz: 2,
            front: 1000.0,
            staging: 3000.0,
            bring_up: 0.5,
            tempo: 0.5,
            keep: 0.3,
            commit: 0.6,
            reserve: 0.0,
            hoard: 80.0,
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
    /// Light years: in `REGION` in the plane, `z` off the midplane.
    pub pos: Vec3,
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
    /// The faction the others are ganging up on, if any.
    pub dominant: Option<usize>,
    /// The sides allied against it: those that border it (a side with no
    /// border with it has nothing to gain from the alliance and stays out).
    pub allied: [bool; SIDES],
    /// Who's at peace with whom (the allies, with each other).
    peace: [[bool; SIDES]; SIDES],
    /// Each side's offensive, if it's on one.
    pub offensives: [Option<Offensive>; SIDES],
    /// Systems whose holder is cut off from the rest of its space (a pocket).
    pub cut: Vec<bool>,
    pub tick: u64,
    /// This tick: hull sendings, shield generators destroyed, sites taken.
    pub sent: u32,
    pub shields_down: u32,
    pub taken: u32,
    rng: Rng,
    /// Scratch for sending: hulls each side can still send from each system this tick.
    spare: Vec<[f32; SIDES]>,
}

/// A side pushing into one stretch of the front: it attacks only there
/// (and into enemy pockets), until its crystals run low or the push stalls.
#[derive(Clone, Copy, Debug)]
pub struct Offensive {
    /// Where it's aimed.
    pub at: Vec3,
    /// The last tick it took a site.
    pub gained: u64,
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
        let disk = structure::Disk::new(region, &mut rng);
        let mut systems = Vec::with_capacity(count);
        let mut sites = Vec::new();
        let spots = disk.scatter(count, &mut rng);
        let owners = settle(&spots, &hearts);
        for (pos, owner) in spots.into_iter().zip(owners) {
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
            spare: vec![[0.0; SIDES]; systems.len()],
            systems,
            sites,
            neighbors,
            crystals: [50.0; SIDES],
            income: [0.0; SIDES],
            share: [0.0; SIDES],
            dominant: None,
            allied: [false; SIDES],
            peace: [[false; SIDES]; SIDES],
            offensives: [None; SIDES],
            cut: vec![false; count],
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
        g.update_supply();
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

    fn neighbors(&self, s: usize) -> impl Iterator<Item = (usize, f32)> + '_ {
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

    fn update_share(&mut self) {
        let mut held = [0u32; SIDES];
        for s in &self.systems {
            for f in 0..SIDES {
                held[f] += s.held[f] as u32;
            }
        }
        let total = held.iter().sum::<u32>().max(1) as f32;
        self.share = held.map(|h| h as f32 / total);
        // The three stay allies until the dominant faction is well back
        // under the line, so the alliance doesn't flicker on and off.
        if self
            .dominant
            .is_none_or(|d| self.share[d] < self.rules.release)
        {
            self.dominant = (0..SIDES).find(|&f| self.share[f] > self.rules.dominant);
        }
    }

    /// Finds the pockets: systems whose holder's space there (systems it
    /// holds, linked by FTL range) is small and bordered by an enemy.
    fn update_supply(&mut self) {
        let n = self.systems.len();
        let owner: Vec<usize> = self.systems.iter().map(|s| s.owner()).collect();
        let mut root: Vec<u32> = (0..n as u32).collect();
        fn find(root: &mut [u32], mut i: usize) -> usize {
            while root[i] as usize != i {
                root[i] = root[root[i] as usize];
                i = root[i] as usize;
            }
            i
        }
        for s in 0..n {
            for k in 0..NEIGHBORS {
                let t = self.neighbors[s * NEIGHBORS + k].0;
                if t == u32::MAX {
                    break;
                }
                if owner[t as usize] == owner[s] {
                    let (a, b) = (find(&mut root, s), find(&mut root, t as usize));
                    root[a] = b as u32;
                }
            }
        }
        // Who borders whom, and so who joins the alliance.
        let mut border = [[0u32; SIDES]; SIDES];
        for s in 0..n {
            let mut seen = [false; SIDES];
            for (t, _) in self.neighbors(s) {
                seen[owner[t]] = true;
            }
            for g in 0..SIDES {
                border[owner[s]][g] += (seen[g] && g != owner[s]) as u32;
            }
        }
        self.allied = [false; SIDES];
        self.peace = [[false; SIDES]; SIDES];
        if let Some(d) = self.dominant {
            for f in 0..SIDES {
                self.allied[f] = f != d && border[f][d] >= self.rules.ally_border;
            }
            for f in 0..SIDES {
                for g in 0..SIDES {
                    self.peace[f][g] = self.allied[f] && self.allied[g];
                }
            }
        }
        let mut size = vec![0u32; n];
        for s in 0..n {
            let r = find(&mut root, s);
            size[r] += 1;
        }
        for s in 0..n {
            let small = size[find(&mut root, s)] < self.rules.pocket;
            self.cut[s] = small
                && self
                    .neighbors(s)
                    .any(|(t, _)| self.hostile(owner[s], owner[t]));
        }
    }

    /// Each side starts or ends its offensive.
    fn plan(&mut self) {
        for f in 0..SIDES {
            let bank = self.crystals[f] / self.income[f].max(1.0);
            if let Some(o) = self.offensives[f] {
                if bank < self.rules.spent || self.tick > o.gained + self.rules.stall as u64 {
                    self.offensives[f] = None;
                }
            } else if bank >= self.rules.launch {
                self.offensives[f] = self.aim(f);
            }
        }
    }

    /// Where side `f` should push: of a sample of the enemy systems on its
    /// border (the dominant faction's, while there is one), the one where its
    /// hulls nearby most outweigh the defense.
    fn aim(&mut self, f: usize) -> Option<Offensive> {
        let n = self.systems.len();
        let (dom, peace, allied) = (self.dominant, self.peace, self.allied[f]);
        let target = move |g: usize| match dom {
            Some(d) if allied => g == d,
            _ => hostile(peace, f, g),
        };
        let mut best: Option<(f32, usize)> = None;
        for _ in 0..400 {
            let t = self.rng.index(n);
            if !target(self.systems[t].owner()) {
                continue;
            }
            let ours: f32 = self
                .neighbors(t)
                .filter(|&(s, _)| self.systems[s].owner() == f)
                .map(|(s, _)| self.systems[s].hulls[f])
                .sum();
            if ours <= 0.0 {
                continue;
            }
            let defense: f32 = self
                .fire(t)
                .iter()
                .enumerate()
                .filter(|&(g, _)| target(g))
                .map(|(_, x)| x)
                .sum();
            let score = ours / (defense + 1.0);
            if best.is_none_or(|b| score > b.0) {
                best = Some((score, t));
            }
        }
        best.map(|(_, t)| Offensive {
            at: self.systems[t].pos,
            gained: self.tick,
        })
    }

    /// Run the whole galaxy forward to its next battle: one tick, or, if
    /// that leaves nowhere at war, on through the quiet until somewhere is
    /// (the player is always posted to a war, so quiet ticks pass unseen).
    pub fn step(&mut self) {
        (self.sent, self.shields_down, self.taken) = (0, 0, 0);
        for _ in 0..QUIET_SKIP {
            self.advance_tick();
            if self.systems.iter().any(|s| s.at_war()) {
                break;
            }
        }
    }

    /// Run the whole galaxy forward one tick.
    fn advance_tick(&mut self) {
        self.tick += 1;
        self.produce();
        self.battles();
        self.update_share();
        self.update_supply();
        self.plan();
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
            // A pocket's yards run short of what the rest of its space sends them.
            let holder = sys.owner();
            let cut = if self.cut[s] { self.rules.cut_off } else { 1.0 };
            for site in &self.sites[a as usize..b as usize] {
                if let SiteKind::Production { hulls, crystals: c } = site.kind {
                    let f = site.owner as usize;
                    sys.hulls[f] += if f == holder { hulls * cut } else { hulls };
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
            if self.offensives[f].is_some_and(|o| o.at.distance(sys.pos) <= self.rules.front) {
                fire[f] *= 1.0 + self.rules.initiative;
            }
        }
        fire
    }

    /// Are `f` and `g` fighting each other? Everyone is, except the allies
    /// against a dominant faction.
    fn hostile(&self, f: usize, g: usize) -> bool {
        hostile(self.peace, f, g)
    }

    fn battles(&mut self) {
        let peace = self.peace;
        for s in 0..self.systems.len() {
            let hulls = self.systems[s].hulls;
            let present = hulls.map(|h| h >= AT_WAR);
            let fighting = |f: usize| (0..SIDES).any(|g| present[g] && hostile(peace, f, g));
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
                let enemies = |f: usize| present[f] && hostile(peace, f, g);
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
                    .filter(|&g| present[g] && hostile(peace, f, g))
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
        let cut = self.cut[s];
        for i in a as usize..b as usize {
            let owner = self.sites[i].owner as usize;
            if owner == f && !cut {
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
            // Nothing of the enemy's here: a defender's won tick counts
            // toward repairs (except in a pocket, with no parts coming).
            if self.cut[s] {
                return;
            }
            for site in &mut self.sites[a..b] {
                if site.owner as usize == w
                    && let SiteKind::Shield { down } = &mut site.kind
                {
                    *down = down.saturating_sub(1);
                }
            }
            return;
        };
        // A clear win takes more than one thing in a tick: a step for each
        // time over the enemy's fire, up to the blitz.
        let steps = (margin as u32).clamp(1, self.rules.blitz);
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
                // An offensive rolls on from what it takes.
                let pos = self.systems[s].pos;
                if let Some(o) = &mut self.offensives[w]
                    && o.at.distance(pos) <= self.rules.front
                {
                    o.gained = self.tick;
                    o.at = pos;
                }
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
        self.stage();
        let offset = self.rng.index(n);
        for f in order {
            for i in 0..n {
                self.consider((i + offset) % n, f);
            }
        }
    }

    /// Each side on an offensive brings hulls up toward it: every quiet system
    /// of its own behind the front sends some of its spare hulls one jump on,
    /// to its own system nearest the offensive's aim (paying the FTL).
    fn stage(&mut self) {
        let n = self.systems.len();
        let mut moves: Vec<(usize, usize, usize, f32)> = Vec::new();
        for f in 0..SIDES {
            let Some(o) = self.offensives[f] else {
                continue;
            };
            let first = moves.len();
            let mut cost = 0.0;
            for s in 0..n {
                let here = self.systems[s].pos.distance(o.at);
                if self.spare[s][f] < MIN_SEND || here > self.rules.staging {
                    continue;
                }
                let next = self
                    .neighbors(s)
                    .filter(|&(t, _)| self.systems[t].held[f] > 0)
                    .map(|(t, d)| (t, d, self.systems[t].pos.distance(o.at)))
                    .filter(|&(_, _, there)| there < here)
                    .min_by(|a, b| a.2.total_cmp(&b.2));
                if let Some((t, d, _)) = next {
                    let m = self.spare[s][f] * self.rules.bring_up;
                    cost += m * d / FTL_RANGE * self.rules.ftl_cost;
                    moves.push((f, s, t, m));
                }
            }
            // Bring up only what this tick's budget pays for, from everywhere alike.
            let budget = (self.rules.tempo * self.income[f])
                .min(self.crystals[f])
                .max(0.0);
            let afford = if cost > budget { budget / cost } else { 1.0 };
            for mv in &mut moves[first..] {
                mv.3 *= afford;
            }
            self.crystals[f] -= cost * afford;
        }
        for (f, s, t, m) in moves {
            self.spare[s][f] -= m;
            self.systems[s].hulls[f] -= m;
            self.systems[t].hulls[f] += m;
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
        // Defending is always on. An attack goes in where the side's
        // offensive is, or on an enemy pocket (which nobody leaves standing);
        // anywhere else only at long odds.
        let pocket = self.cut[t] && !ours;
        let pushing =
            self.offensives[f].is_some_and(|o| o.at.distance(sys.pos) <= self.rules.front);
        let odds = if pocket || (self.allied[f] && self.dominant.is_some_and(|d| sys.held[d] > 0)) {
            self.rules.gang_odds
        } else if ours || pushing {
            self.rules.odds
        } else {
            self.rules.opportunity
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
            .map(|(s, d)| self.spare[s][f] * frac * (d / FTL_RANGE * self.rules.ftl_cost + breach))
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

/// Are `f` and `g` fighting each other, given who's at `peace`?
fn hostile(peace: [[bool; SIDES]; SIDES], f: usize, g: usize) -> bool {
    f != g && !peace[f][g]
}

/// Who holds each system at the start: the side of its nearest heartland,
/// with each side's reach stretched or shrunk until all four hold about as
/// many systems (the stars are thicker in some heartlands than others).
fn settle(spots: &[Vec3], hearts: &[(Vec2, u8)]) -> Vec<u8> {
    let mut reach = [0.0f32; SIDES];
    let mut owners = vec![0u8; spots.len()];
    for _ in 0..40 {
        let mut count = [0usize; SIDES];
        for (p, o) in spots.iter().zip(&mut owners) {
            let p = p.truncate();
            *o = hearts
                .iter()
                .min_by(|a, b| {
                    let da = a.0.distance(p) - reach[a.1 as usize];
                    let db = b.0.distance(p) - reach[b.1 as usize];
                    da.total_cmp(&db)
                })
                .map(|h| h.1)
                .unwrap_or(0);
            count[*o as usize] += 1;
        }
        let fair = spots.len() as f32 / SIDES as f32;
        for f in 0..SIDES {
            // A side with too much pulls in its borders, and vice versa.
            reach[f] += 60.0 * (1.0 - count[f] as f32 / fair).clamp(-1.0, 1.0);
        }
    }
    owners
}

/// Each system's nearest neighbors within FTL range, via a grid.
fn find_neighbors(systems: &[System], region: Vec2) -> Vec<(u32, f32)> {
    let cell = FTL_RANGE;
    let cols = (region.x / cell).ceil() as usize + 1;
    let rows = (region.y / cell).ceil() as usize + 1;
    let key = |p: Vec3| {
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
    // Steps skip quiet ticks, so go by the tick count, not the steps.
    let (mut printed, mut snapped) = (0, 1);
    while g.tick < ticks {
        let t = std::time::Instant::now();
        g.step();
        let dt = t.elapsed();
        total += dt;
        worst = worst.max(dt);
        let c = g.census();
        history.push(c);
        if g.tick / every > printed {
            printed = g.tick / every;
            let names = Faction::ALL.map(|f| f.name());
            let shares: Vec<String> = (0..SIDES)
                .map(|f| {
                    let push = if g.offensives[f].is_some() { '>' } else { ' ' };
                    format!("{}{push}{:4.1}%", &names[f][..3], c.share[f] * 100.0)
                })
                .collect();
            let hulls: f32 = c.hulls.iter().sum();
            println!(
                "tick {:6}  {}  at war {:4.1}%  sent {:4} shields {:4} taken {:4}  hulls {:8.0}  bank {:?} pockets {}",
                g.tick,
                shares.join("  "),
                c.at_war * 100.0,
                g.sent,
                g.shields_down,
                g.taken,
                hulls,
                (0..SIDES)
                    .map(|f| (g.crystals[f] / g.income[f].max(1.0)) as i64)
                    .collect::<Vec<_>>(),
                g.cut.iter().filter(|&&c| c).count(),
            );
        }
        if snapped < 4 && (g.tick >= snapped * ticks / 3 || g.tick >= ticks) {
            snapped += 1;
            snaps.push(chart::Snapshot::take(&g));
        }
    }
    println!(
        "tick: {:.2?} on average, a step {:.2?} at worst (quiet ticks included)",
        total / g.tick.max(1) as u32,
        worst
    );
    if let Some(out) = out {
        chart::render(&snaps, &history).save(out)?;
        println!("saved {out}");
    }
    Ok(())
}
