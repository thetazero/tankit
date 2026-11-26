//! The forever war as a simulation of its own (#303, from the design on #302).
//!
//! Some ten thousand colonized star systems, each a few numbers, ticking
//! forward: one tick of the whole galaxy per battle the player plays. Each
//! battle the player plays is drawn from it and fed back into it (`battle`,
//! #304); postings come later.
//!
//! **State.** A system is a point in a slab of a spiral galaxy's disk (light
//! years; see `structure`: two arms, the gap between, a spur across it) with
//! a few planets. Each planet has **sites**: production sites, each held by a
//! side and turning out hulls (into that side's storage in the system) and
//! crystals, and one or two **shield generators** (at least one). Hull
//! storage is per side per system. Hulls are whole: a yard builds toward its
//! next one and delivers it done; a battle's losses are whole hulls, rolled (each of a
//! side's hulls there with the chance its expected losses give it), the
//! mercs' among them by their share. A side with a hull in a system is in
//! the fight there: a battle goes on to the last hull, mercs' included.
//!
//! **Realms, not factions, are the actors** (#315; see `realm`). A faction
//! is a people; what fights is a realm, a ruler holding a handful of systems
//! from a seat. Every side's sites and hulls in a system are one realm's
//! (`System::realms`); each realm has its own crystal, in its planets' vaults, and
//! keeps what it takes. Realms split at their edges and merge, by
//! distance. Nobody watches the whole galaxy.
//!
//! **A tick:**
//! 1. *Production.* Every mine adds crystal to its planet's vault (`Vault`),
//!    which changes hands only with the whole planet. A yard builds while a
//!    hull's worth more to its realm than it costs; hulls last until they're
//!    shot down. Realms pay their pilots, and train more or let some go by
//!    what their fights show one's worth.
//! 2. *Battles.* Wherever two or more sides have hulls, they fight: a cheap
//!    Lanchester-style exchange, with a bonus for the side holding the ground.
//!    Each side in a fight attacks every site there it can at once
//!    (`targets`, #460): every enemy shield generator up, and every enemy
//!    site on a planet whose generators are down. It wins each, a chance by
//!    its share of the fire there; else its holder does. A battle is a
//!    battle, played or not (#422): its winner takes the site from its holder
//!    (a generator taken is damaged), or, the holder winning, a damaged
//!    generator is working again (`Galaxy::win`); a draw is the holder's. A
//!    damaged generator nobody fights over is working again the next tick.
//!    An attacker fighting for a site under an enemy's working shield fights
//!    a seat short there (`Rules::seats`, #426): its fire is cut by one seat
//!    in that many, the same seat a played battle closes. A planet's other
//!    sites can't be attacked while its generators are up, so it goes for
//!    them first, all of them together, to get out from under them. Only so many of a side's hulls fire a tick however big it
//!    is (`Rules::frontage`), so a world holds out for many ticks.
//!    Each firing hull needs a pilot (`Galaxy::firing`). Pilots cap fire,
//!    not losses: a pilot who loses a hull takes the next.
//!    Mercenary companies' hulls in a fight take their share of their side's
//!    losses there (`Galaxy::lose`); contracts done (world taken, hulls
//!    spent, time up) end.
//! 3. *Politics.* Every few ticks realms split and merge.
//! 4. *Sending hulls.* Each realm looks at every fight it's in, and now and
//!    then at every world it could reach, and plays each way of fighting for
//!    it through by the battle maths (`Galaxy::forecast`): alone, with more
//!    or less of what it could bring, with a company's help (see `merc`). It
//!    does whatever gains it most, if anything does: what the world turns out
//!    for as long as worlds have lately been held, and what's in its vaults,
//!    less what it'd cost. FTL costs crystal per hull by distance.
//! 5. *Companies* that aren't working for anyone raid rich, weakly held
//!    worlds for themselves, as a side of their own (`RAID`, one company to a
//!    system). They take the vaults and go, and give the sites back.
//!
//! A step runs ticks until somewhere is at war: the player is posted to a
//! battle, so quiet ticks pass unseen.
//!
//! **What keeps the war forever:** defenders build hulls where they stand,
//! attackers pay to move theirs (a defensive asymmetry); a realm fights with
//! its own vaults only, so even a people winning everywhere wins slowly; and
//! what a realm takes far from its seat breaks away, poor, as a realm of its own.

pub mod battle;
pub mod chart;
pub mod judge;
pub mod ledger;
pub mod merc;
pub mod realm;
pub mod save;
pub mod structure;
pub mod theater;

use glam::{Vec2, Vec3};

use crate::faction::Faction;
use crate::rng::Rng;
use merc::{Books, Company, Debt, Pilot};
use realm::{NONE, Realm};

/// The four warring peoples.
pub const PEOPLES: usize = 4;
/// Every side there can be in a system: the peoples, and a company fighting
/// for itself (one to a system).
pub const SIDES: usize = 5;
/// That company's side.
pub const RAID: usize = 4;

/// A planet's vault: the crystal its mines have turned out and its realm
/// hasn't spent. It changes hands only with the whole planet.
#[derive(Clone, Copy, Debug)]
pub struct Vault {
    /// Whose it is: the side, and its realm (`NONE`: nobody's left to own it).
    pub side: u8,
    pub realm: u32,
    pub crystals: f32,
}

/// Colonized systems in the war.
pub const SYSTEMS: usize = 10_000;
/// The region the war fills, in light years (in the galaxy's plane; see
/// `structure` for its depth and shape).
pub const REGION: Vec2 = Vec2::new(8800.0, 5500.0);
/// How far a realm sends hulls in one jump, light years.
pub const FTL_RANGE: f32 = 250.0;
/// Most systems a system can send to (the nearest ones within range).
const NEIGHBORS: usize = 12;

/// Crystals a hull costs to build, for the Resonant (whose hulls are vessels for a crystal).
const RESONANT_HULL_COST: f32 = 0.0005;
/// Smallest sending worth the FTL (hulls).
const MIN_SEND: u32 = 20;
/// Most quiet ticks a step skips looking for the next war.
const QUIET_SKIP: usize = 1000;
/// A system counts as at war when two sides each have a hull in it: a
/// battle goes on to a side's last hull, mercs' included.
pub const AT_WAR: u32 = 1;
/// Seats an attacker fights short while the defender has a shield generator
/// up on the planet it's fighting for (#426): in a played battle, that many
/// of its seats stay empty; in the galaxy's, its fire is cut to match.
pub const SHIELD_SEATS: usize = 1;

/// The numbers the war is tuned by.
#[derive(Clone, Copy, Debug)]
pub struct Rules {
    /// Crystals per hull for an FTL jump of the full range (shorter ones cost less).
    pub ftl_cost: f32,
    /// Light years a fleet, or a company's ship, flies in a tick.
    pub ftl_speed: f32,
    /// Fraction of each side's hulls the others destroy a tick, per unit of their fire.
    pub kill: f32,
    /// Most of a side's hulls in a system that can fire in a tick. Machines
    /// can only be in contact so fast: past this, more hulls don't shoot
    /// harder, they only last longer, so a big army grinds a world down
    /// over many ticks instead of erasing it in one.
    pub frontage: f32,
    /// Seats a side fights the galaxy's battles with, as a played one's do:
    /// an attacker under an enemy's shield fights with `SHIELD_SEATS` fewer,
    /// its fire cut by that share.
    pub seats: f32,
    /// Fire bonus for a side fighting on ground it holds (all its sites in the system).
    pub home_ground: f32,
    /// Chance a tick that a side in a fight wins its objective there (the
    /// next site it goes for), times its share of the fight's fire.
    pub take: f32,
    /// A people's systems hanging together (by FTL range) fewer than this
    /// are a pocket, cut off from the rest: once an enemy borders it, the
    /// enemy goes for it.
    pub pocket: u32,
    /// Most a side alone in a system walks into of the enemy's unguarded
    /// sites a tick.
    pub blitz: u32,
    /// A realm keeps at least this fraction of a quiet system's hulls home.
    pub keep: f32,
    /// How far from its seat a realm can rule, light years: holdings past
    /// this break away.
    pub reach: f32,
    /// A realm of fewer systems than this folds into a neighbor of its
    /// people, if that neighbor's seat can rule all of it.
    pub fold: u32,
    /// The first realms' seats are at least this far apart, light years.
    pub found: f32,
    /// Realms split and merge every this many ticks (and companies split and join).
    pub politics: u32,
    /// Mercenary companies at the start, for the full galaxy (fewer in a
    /// smaller one; 0 for none). See `merc`.
    pub companies: u32,
    /// Crystal each starts with (give or take half).
    pub merc_capital: f32,
    /// Crystal to move a company's ship the full FTL range (its hulls pay as any do).
    pub ship_cost: f32,
    /// A contract runs out after this many ticks.
    pub merc_term: u32,
    /// How far a company goes for work, light years.
    pub merc_range: f32,
    /// A company idle this many ticks sails to the nearest war to look for
    /// work (`Galaxy::seek_work`).
    pub merc_idle: u32,
    /// An idle company worth more than this splits in two; one worth less
    /// than this joins another (crystals, its hulls at `hull_price`).
    pub merc_split: f32,
    pub merc_join: f32,
    /// A realm puts this share of its crystal income into researching its
    /// next gear tier (#438)...
    pub research: f32,
    /// ... and a step costs this many crystals a system it holds, growing
    /// a step at a time as upgrades do (`economy::upgrade_cost`).
    pub research_price: f32,
    /// What a realm pays each pilot a tick when it's founded: it moves
    /// from there, up while it loses pilots and down while it doesn't.
    pub pilot_upkeep: f32,
    /// Crystals a ship costs its company a tick.
    pub ship_upkeep: f32,
    /// Crystals a hull costs to build.
    pub hull_cost: f32,
    /// Crystals it costs to train a pilot.
    pub train_cost: f32,
    /// Chance a tick a pilot retires, realm or merc.
    pub career: f32,
    /// Ticks out of work before a pilot quits for good.
    pub patience: u32,
    /// How fast a realm's pay moves: up by this share a tick it loses
    /// pilots, down by this otherwise, never below what a pilot lives on.
    pub pay_step: f32,
    /// How fast a realm's sense of what a pilot or hull is worth follows
    /// what its fights show, a tick.
    pub sense: f32,
    /// Most pilots a realm trains or dismisses a tick.
    pub train: u32,
    /// Pilots a new company starts with.
    pub crew: u32,
    /// What a pilot's wage a tick is in CR: your pay for a battle is in
    /// CR, so CR and crystal trade at the going wage.
    pub wage_cr: f32,
    /// A ship pays a pilot this share of their wage for each kill.
    pub kill_share: f32,
    /// Share of a realm's pilots a tick who defect, when ships offer them
    /// more than their realm pays.
    pub defect: f32,
}

impl Default for Rules {
    fn default() -> Rules {
        Rules {
            ftl_cost: 0.1,
            ftl_speed: 40.0,
            kill: 0.35,
            frontage: 85.0,
            seats: 6.0,
            home_ground: 0.3,
            take: 0.3,
            pocket: 20,
            blitz: 2,
            keep: 0.3,
            reach: 800.0,
            fold: 12,
            found: 300.0,
            politics: 10,
            companies: 60,
            merc_capital: 150.0,
            ship_cost: 3.0,
            merc_term: 30,
            merc_range: 1200.0,
            merc_idle: 30,
            merc_split: 300.0,
            merc_join: 40.0,
            research: 0.15,
            research_price: 1.2,
            pilot_upkeep: 0.02,
            ship_upkeep: 0.2,
            hull_cost: 0.1,
            train_cost: 3.0,
            career: 0.002,
            patience: 60,
            pay_step: 0.005,
            sense: 0.03,
            train: 2,
            crew: 40,
            wage_cr: 50.0,
            kill_share: 0.3,
            defect: 0.01,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SiteKind {
    /// Turns out hulls (into its holder's storage in the system) and crystals
    /// (into its holder's realm's vaults) each tick.
    Production { hulls: f32, crystals: f32 },
    /// A shield generator: working, or damaged (#422). It's damaged when
    /// it's taken, and working again once its holder wins a battle for it,
    /// or a tick goes by without one.
    Shield { damaged: bool },
}

#[derive(Clone, Copy, Debug)]
pub struct Site {
    /// Which planet of its system it's on.
    pub planet: u8,
    /// Index into `Faction::ALL` (which realm of that people: `System::realms`).
    pub owner: u8,
    pub kind: SiteKind,
}

/// How one side's hulls in a system came and went (#445): every change
/// to `System::hulls`, by what made it, so the missions screen can show
/// where a count went from 5 to 55.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Moves {
    /// Delivered by its yards here.
    pub built: u32,
    /// Shot down in battles here, played or the galaxy's (mercs' too).
    pub lost: u32,
    /// Sent in by its realm from other systems (FTL).
    pub arrived: u32,
    /// Sent off to other systems, or sold to a company.
    pub left: u32,
    /// A company's, landed on a new contract for the side.
    pub hired: u32,
    /// A company's, back aboard as its contract ended.
    pub released: u32,
}

impl Moves {
    /// Hulls in less hulls out.
    pub fn net(&self) -> i64 {
        (self.built + self.arrived + self.hired) as i64
            - (self.lost + self.left + self.released) as i64
    }

    /// These without `o`'s (none below zero).
    pub fn less(&self, o: &Moves) -> Moves {
        Moves {
            built: self.built.saturating_sub(o.built),
            lost: self.lost.saturating_sub(o.lost),
            arrived: self.arrived.saturating_sub(o.arrived),
            left: self.left.saturating_sub(o.left),
            hired: self.hired.saturating_sub(o.hired),
            released: self.released.saturating_sub(o.released),
        }
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        *self == Moves::default()
    }
}

#[derive(Clone, Debug)]
pub struct System {
    /// Light years: in `REGION` in the plane, `z` off the midplane.
    pub pos: Vec3,
    pub planets: u8,
    /// This system's sites, a range of `Galaxy::sites`.
    pub sites: (u32, u32),
    /// Each side's hulls here: whole hulls.
    pub hulls: [u32; SIDES],
    /// Each side's yards here: how far along its next hulls are (under one).
    pub building: [f32; SIDES],
    /// Each side's sites here (kept in step with `sites`).
    pub held: [u8; SIDES],
    /// Whose each side's sites and hulls here are: an index into
    /// `Galaxy::realms`, or `realm::NONE` where the side has neither. One
    /// realm of a people to a system.
    pub realms: [u32; SIDES],
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

/// Hulls in flight: nobody else sees them till they land.
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct Fleet {
    pub to: u32,
    pub side: u8,
    pub realm: u32,
    pub hulls: u32,
    /// The tick it lands.
    pub arrives: u64,
}

/// The whole war.
#[cfg_attr(test, derive(Clone))]
pub struct Galaxy {
    /// What it was generated from (the layout of systems and sites).
    pub seed: u64,
    pub systems: Vec<System>,
    pub sites: Vec<Site>,
    /// Each system's reachable neighbors (nearest first), `NEIGHBORS` slots
    /// a system, `u32::MAX` where there are fewer.
    neighbors: Vec<(u32, f32)>,
    /// Every realm there has been; a slot whose realm is gone (`systems` 0)
    /// is handed to the next one founded.
    pub realms: Vec<Realm>,
    /// Each side's share of all sites (updated each tick).
    pub share: [f32; SIDES],
    pub rules: Rules,
    /// Systems whose holder is cut off from the rest of its people's space (a pocket).
    pub cut: Vec<bool>,
    pub tick: u64,
    /// This tick: hull sendings, shield generators destroyed, sites taken.
    pub sent: u32,
    pub shields_down: u32,
    pub taken: u32,
    /// Realms broken away and folded together, all told.
    pub splits: u64,
    pub merges: u64,
    /// The mercenary companies, and their trade all told.
    pub companies: Vec<Company>,
    pub books: Books,
    /// Bonuses realms owe companies.
    pub debts: Vec<Debt>,
    /// Companies founded, all told (each one's `id`).
    pub(crate) next_company: u32,
    /// The last pilot id handed out.
    pub(crate) next_pilot: u32,
    /// You, as a pilot. Your knowledge is copied from the profile.
    pub you: Pilot,
    /// Pay companies owe you: (`Company::id`, crystals).
    pub owed_you: Vec<(u32, f32)>,
    /// Pilots on no ship, looking for work.
    pub pool: Vec<Pilot>,
    /// What pilots have lately been signing for a tick, by gear mark.
    pub wage_curve: [f32; merc::MARKS],
    /// When each system last changed hands, and the chance a tick that one
    /// lately taken, or one on a border, does (fading averages).
    pub(crate) taken_at: Vec<u64>,
    pub(crate) lose_new: f32,
    pub(crate) lose_old: f32,
    /// Savings of retired pilots and dead ships, for new companies.
    pub(crate) savings: f32,
    /// Hulls in flight.
    pub fleets: Vec<Fleet>,
    /// What one more hull in each system has lately been worth to its holder,
    /// and its best use this tick (scratch).
    pub(crate) hull_worth: Vec<f32>,
    hull_use: Vec<f32>,
    /// Scratch: what each side has on the way to a system: hulls, and their
    /// ticks to go all told.
    incoming: std::collections::HashMap<(u32, u8), (f32, f32)>,
    /// Every planet's vault, and each system's first.
    pub vaults: Vec<Vault>,
    pub(crate) vault_at: Vec<u32>,
    /// What a company has lately made a tick, on average; and what they
    /// were all worth last tick.
    pub(crate) profit: f32,
    /// What a contract has lately made a company, over its costs.
    pub(crate) job_worth: f32,
    pub(crate) worth_before: f32,
    /// What hulls have lately sold for.
    pub hull_rate: f32,
    pub(crate) fell_new: u32,
    pub(crate) fell_old: u32,
    /// Systems with an enemy's within reach.
    pub(crate) border: Vec<bool>,
    /// The best open bid this tick for a pilot at each mark (0 for none).
    pub offers: [f32; merc::MARKS],
    /// What realms pay their pilots a tick, on average, as of the last
    /// tick: what a new pilot starts on, and the rate CR trade at.
    pub pay: f32,
    /// The ticks contracts have lately run, of those forecast.
    pub trust_ticks: f32,
    /// The share of merc pilots in work lately, as pilots see it; and
    /// this tick's, so far.
    pub employment: f32,
    pub(crate) employed: u32,
    /// Scratch: realms' pay, lowest first, and the pilots on no more than
    /// it, as of `pays_tick`.
    pub(crate) pays: Vec<(f32, f32)>,
    pub(crate) pays_tick: u64,
    /// Your company (a `Company::id`), once you're with one (#350): you stay
    /// with it, and go with it if it joins another.
    pub crew: Option<u32>,
    /// Your own hulls on your company's contract (#385; see `claim_own`).
    pub own: Option<theater::Own>,
    /// Your company's last few ended contracts, newest last (#414).
    pub ended: Vec<theater::Ended>,
    /// What just happened to your company, for the missions screen (not saved).
    pub crew_news: Option<theater::CrewNews>,
    /// Your company just split or joined another: which ship you go with,
    /// for you to choose on the missions screen (#364). You're with the
    /// first meanwhile, and the war waits on your choice (not saved: a
    /// choice left open when you quit stays the first).
    pub crew_choice: Option<theater::CrewChoice>,
    /// Hulls destroyed in the galaxy's battles, all told.
    pub destroyed: f64,
    /// Crystal captured with fallen systems, all told (not saved).
    pub captured: f64,
    /// Each system's hulls in and out, each side's (#445), since the last
    /// played battle began (`step_played`): that battle's losses and the
    /// rest of its tick, then any ticks waited through after. Not saved.
    pub moves: Vec<[Moves; SIDES]>,
    /// Of `moves`, the companies' hulls on contract for the side: hired,
    /// released and lost. Not saved.
    pub merc_moves: Vec<[Moves; SIDES]>,
    rng: Rng,
    /// Scratch: the sites fought over this tick (see `Galaxy::win`).
    assaulted: Vec<bool>,
    /// Scratch: each realm's hulls in this tick's fights.
    war_hulls: Vec<u32>,
    /// Realms lately looking for a company at each system and finding none
    /// in range (fading): where idle companies go for work.
    pub(crate) unmet: Vec<f32>,
    /// Scratch for sending: hulls each side can still send from each system this tick.
    spare: Vec<[u32; SIDES]>,
    /// Each realm's territory: `members[spans[r]]` (see `Galaxy::territory`).
    members: Vec<u32>,
    spans: Vec<(u32, u32)>,
    /// Slots of realms that are gone.
    free: Vec<u32>,
}

/// Ticks between a realm's looks for somewhere to push.
const THINK: u64 = 10;

/// One tick's numbers, for charts and tests.
#[derive(Clone, Copy, Debug, Default)]
pub struct Census {
    pub tick: u64,
    /// Each side's share of all sites.
    pub share: [f32; SIDES],
    /// Fraction of systems at war.
    pub at_war: f32,
    pub hulls: [f32; SIDES],
    /// Each people's realms.
    pub realms: [u32; SIDES],
    /// Realms broken away and folded together, all told.
    pub splits: u64,
    pub merges: u64,
    /// Companies, what they're worth all told, and how many are on a contract.
    pub companies: u32,
    pub merc_capital: f32,
    pub contracts: u32,
    /// Crystal in all the realms' vaults.
    pub vaults: f32,
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
                        kind: SiteKind::Shield { damaged: false },
                    });
                }
                for _ in 0..rng.int(1, 3) {
                    sites.push(Site {
                        planet,
                        owner,
                        kind: SiteKind::Production {
                            hulls: rng.range(1.0, 5.0),
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
                hulls: [0; SIDES],
                building: [0.0; SIDES],
                held,
                realms: [NONE; SIDES],
            });
        }
        let neighbors = find_neighbors(&systems, region);
        let mut g = Galaxy {
            seed,
            spare: vec![[0; SIDES]; systems.len()],
            moves: vec![[Moves::default(); SIDES]; systems.len()],
            merc_moves: vec![[Moves::default(); SIDES]; systems.len()],
            assaulted: Vec::new(),
            war_hulls: Vec::new(),
            unmet: vec![0.0; systems.len()],
            systems,
            sites,
            neighbors,
            realms: Vec::new(),
            share: [0.0; SIDES],
            cut: vec![false; count],
            rules,
            tick: 0,
            sent: 0,
            shields_down: 0,
            taken: 0,
            splits: 0,
            merges: 0,
            companies: Vec::new(),
            books: Books::default(),
            debts: Vec::new(),
            next_company: 0,
            next_pilot: 1,
            you: Pilot::rookie(1),
            owed_you: Vec::new(),
            pool: Vec::new(),
            taken_at: vec![0; count],
            lose_new: 0.01,
            lose_old: 0.003,
            savings: 0.0,
            fleets: Vec::new(),
            hull_worth: vec![rules.hull_cost; count],
            hull_use: vec![0.0; count],
            incoming: Default::default(),
            vaults: Vec::new(),
            vault_at: Vec::new(),
            profit: 0.0,
            job_worth: 5.0,
            worth_before: 0.0,
            hull_rate: rules.hull_cost,
            fell_new: 0,
            fell_old: 0,
            border: vec![false; count],
            wage_curve: [rules.pilot_upkeep; merc::MARKS],
            offers: [0.0; merc::MARKS],
            pay: rules.pilot_upkeep,
            trust_ticks: 1.0,
            employment: 0.5,
            employed: 0,
            pays: Vec::new(),
            pays_tick: 0,
            crew: None,
            own: None,
            ended: Vec::new(),
            crew_news: None,
            crew_choice: None,
            destroyed: 0.0,
            captured: 0.0,
            rng,
            members: Vec::new(),
            spans: Vec::new(),
            free: Vec::new(),
        };
        // Start every garrison near its usual size.
        for s in 0..g.systems.len() {
            let owner = g.systems[s].owner();
            g.systems[s].hulls[owner] = (g.production(s)[owner] * 27.0).round() as u32;
        }
        for s in 0..g.systems.len() {
            g.vault_at.push(g.vaults.len() as u32);
            for _ in 0..g.systems[s].planets {
                g.vaults.push(Vault {
                    side: g.systems[s].owner() as u8,
                    realm: NONE,
                    crystals: 0.0,
                });
            }
        }
        g.form_realms();
        g.fill_vaults();
        (g.splits, g.merges) = (0, 0);
        g.found_companies();
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

    /// Hulls each side's yards in system `s` turn out a tick, as `produce`
    /// adds them. A pocket builds at the full rate (#446).
    pub fn built(&self, s: usize) -> [f32; SIDES] {
        self.production(s)
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
                hulls[f] += s.hulls[f] as f32;
            }
            war += s.at_war() as usize;
        }
        let mut realms = [0; SIDES];
        for (_, r) in self.living() {
            realms[r.faction as usize] += 1;
        }
        Census {
            tick: self.tick,
            share: self.share,
            at_war: war as f32 / self.systems.len() as f32,
            hulls,
            realms,
            splits: self.splits,
            merges: self.merges,
            companies: self.companies.len() as u32,
            merc_capital: self
                .companies
                .iter()
                .map(|c| c.capital(self.rules.hull_cost))
                .sum(),
            contracts: self
                .companies
                .iter()
                .filter(|c| c.contract.is_some())
                .count() as u32,
            vaults: self.living().map(|(_, r)| r.crystals).sum(),
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
    }

    /// Keeps the realms' books (reshaping them every few ticks), and finds
    /// the pockets: systems whose people's space there (systems its realms
    /// hold, linked by FTL range) is small and bordered by an enemy.
    pub(crate) fn update_supply(&mut self) {
        self.keep_books();
        if self.tick > 0 && self.tick.is_multiple_of(self.rules.politics.max(1) as u64) {
            self.reshape();
        }
        self.find_pockets();
    }

    /// The books and the pockets brought up to date as things stand (for a
    /// galaxy just loaded: its realms already reshaped this tick).
    pub(crate) fn refresh(&mut self) {
        self.keep_books();
        self.find_pockets();
    }

    fn find_pockets(&mut self) {
        let n = self.systems.len();
        let owner: Vec<usize> = self.systems.iter().map(|s| s.owner()).collect();
        let mut root: Vec<u32> = (0..n as u32).collect();
        for s in 0..n {
            for k in 0..NEIGHBORS {
                let t = self.neighbors[s * NEIGHBORS + k].0;
                if t == u32::MAX {
                    break;
                }
                if owner[t as usize] == owner[s] {
                    let (a, b) = (
                        realm::find(&mut root, s),
                        realm::find(&mut root, t as usize),
                    );
                    root[a] = b as u32;
                }
            }
        }
        let mut size = vec![0u32; n];
        for s in 0..n {
            let r = realm::find(&mut root, s);
            size[r] += 1;
        }
        for s in 0..n {
            let small = size[realm::find(&mut root, s)] < self.rules.pocket;
            let border = self.neighbors(s).any(|(t, _)| owner[s] != owner[t]);
            self.border[s] = border;
            self.cut[s] = small && self.border[s];
        }
    }

    /// Run the whole galaxy forward to its next battle: one tick, or, if
    /// that leaves nowhere at war, on through the quiet until somewhere is
    /// (the player is always posted to a war, so quiet ticks pass unseen).
    pub fn step(&mut self) {
        self.step_from(None);
    }

    /// The tick a played battle is part of: it's its system's battle
    /// between its sides this tick, settled with the rest of the tick's
    /// battles (#422; see `Galaxy::settle`), while every other battle
    /// fights on. One combined tick, not your battle and then another round
    /// of fighting over the same ground. Returns what the battle did.
    pub fn step_played(&mut self, played: &battle::Played) -> battle::Settled {
        self.clear_moves();
        self.step_from(Some(played))
            .expect("a played battle is settled in its tick")
    }

    /// Start `moves` over: nothing has come or gone yet.
    pub fn clear_moves(&mut self) {
        self.moves.fill([Moves::default(); SIDES]);
        self.merc_moves.fill([Moves::default(); SIDES]);
    }

    fn step_from(&mut self, mut played: Option<&battle::Played>) -> Option<battle::Settled> {
        (self.sent, self.shields_down, self.taken) = (0, 0, 0);
        let mut settled = None;
        for _ in 0..QUIET_SKIP {
            if let Some(s) = self.advance_tick(played.take()) {
                settled = Some(s);
            }
            if self.systems.iter().any(|s| s.at_war()) {
                break;
            }
        }
        settled
    }

    /// One tick, at war or not (`judge`).
    pub(crate) fn tick_once(&mut self) {
        self.taken = 0;
        self.advance_tick(None);
    }

    /// Run the whole galaxy forward one tick.
    /// `played`: a battle this tick that was played (see `step_played`).
    fn advance_tick(&mut self, played: Option<&battle::Played>) -> Option<battle::Settled> {
        self.tick += 1;
        self.land();
        self.produce();
        let settled = self.battles(played);
        self.mercs();
        self.reckon_holds();
        self.update_share();
        self.update_supply();
        self.send();
        settled
    }

    /// What a hull's worth in each system, and its best use this tick, to save.
    pub(crate) fn hull_marks(&self) -> Vec<(f32, f32)> {
        self.hull_worth
            .iter()
            .copied()
            .zip(self.hull_use.iter().copied())
            .collect()
    }

    pub(crate) fn set_hull_marks(&mut self, marks: &[(f32, f32)]) {
        if marks.len() == self.hull_worth.len() {
            self.hull_worth = marks.iter().map(|x| x.0).collect();
            self.hull_use = marks.iter().map(|x| x.1).collect();
        }
    }

    /// Ticks to fly `d` light years.
    pub(crate) fn flight(&self, d: f32) -> u64 {
        (d / self.rules.ftl_speed.max(1.0)).ceil().max(1.0) as u64
    }

    /// What's in flight and due lands.
    fn land(&mut self) {
        let tick = self.tick;
        let (due, fly): (Vec<Fleet>, Vec<Fleet>) = std::mem::take(&mut self.fleets)
            .into_iter()
            .partition(|x| tick >= x.arrives);
        self.fleets = fly;
        for x in due {
            let (t, f) = (x.to as usize, x.side as usize);
            let sys = &mut self.systems[t];
            // Its realm gone, it's whoever of its people's is there.
            if sys.realms[f] == NONE {
                if x.realm == NONE {
                    continue;
                }
                sys.realms[f] = x.realm;
            }
            sys.hulls[f] += x.hulls;
            self.moves[t][f].arrived += x.hulls;
        }
        self.land_mercs();
    }

    /// What's on the way, for each side to a system it's its own.
    fn count_incoming(&mut self) {
        self.incoming.clear();
        for i in 0..self.fleets.len() {
            let x = self.fleets[i];
            self.note_incoming(x.to, x.side, x.hulls, x.arrives);
        }
        for c in 0..self.companies.len() {
            if let Some(k) = self.companies[c].contract
                && k.inbound > 0
            {
                self.note_incoming(k.target, k.side, k.inbound, k.arrives);
            }
        }
    }

    pub(crate) fn note_incoming(&mut self, t: u32, f: u8, hulls: u32, arrives: u64) {
        let e = self.incoming.entry((t, f)).or_default();
        e.0 += hulls as f32;
        e.1 += hulls as f32 * arrives.saturating_sub(self.tick) as f32;
    }

    /// Side `f`'s hulls on the way to `t`, and the ticks till they're there,
    /// on average.
    pub(crate) fn on_the_way(&self, t: usize, f: usize) -> (f32, f32) {
        match self.incoming.get(&(t as u32, f as u8)) {
            Some(&(h, wait)) if h > 0.0 => (h, wait / h),
            _ => (0.0, 0.0),
        }
    }

    fn produce(&mut self) {
        self.fill_vaults();
        let mut mined = vec![0.0f32; self.vaults.len()];
        let mut crystals = vec![0.0; self.realms.len()];
        let resonant = Faction::Resonant.index();
        let extra = |f: usize| {
            if f == resonant {
                RESONANT_HULL_COST
            } else {
                0.0
            }
        };
        let offset = self.rng.index(self.systems.len());
        // What a hull's worth where: no more than building another and
        // bringing it; and it keeps, so quiet ticks say little.
        let most = self.rules.hull_cost + self.rules.ftl_cost;
        for (worth, seen) in self.hull_worth.iter_mut().zip(&mut self.hull_use) {
            let sense = if *seen > 0.0 {
                self.rules.sense
            } else {
                self.rules.sense / 100.0
            };
            *worth += sense * (seen.min(most) - *worth);
            *seen = 0.0;
        }
        for i in 0..self.systems.len() {
            // Whoever's first gets the crystal, so a different one each tick.
            let s = (i + offset) % self.systems.len();
            let (a, b) = self.systems[s].sites;
            let sys = &mut self.systems[s];
            for site in &self.sites[a as usize..b as usize] {
                if let SiteKind::Production { hulls, crystals: c } = site.kind {
                    let f = site.owner as usize;
                    let r = sys.realms[f];
                    if r == NONE {
                        continue;
                    }
                    // A mine pays only its planet's vault's owner.
                    let v = self.vault_at[s] as usize + site.planet as usize;
                    if self.vaults[v].side as usize == f {
                        crystals[r as usize] += c;
                        mined[v] += c * (1.0 - self.rules.research);
                    }
                    // A yard works while a hull's worth more to the realm
                    // than it costs, and it can pay for it.
                    let cost = hulls * (self.rules.hull_cost + extra(f));
                    let realm = &mut self.realms[r as usize];
                    if self.hull_worth[s] > self.rules.hull_cost && realm.crystals >= cost {
                        realm.crystals -= cost;
                        sys.building[f] += hulls;
                    }
                }
            }
        }
        // The yards deliver whole hulls: those done, the rest still building.
        for (sys, moves) in self.systems.iter_mut().zip(&mut self.moves) {
            for f in 0..SIDES {
                let done = sys.building[f].floor();
                sys.hulls[f] += done as u32;
                sys.building[f] -= done;
                moves[f].built += done as u32;
            }
        }
        for (r, c) in crystals.into_iter().enumerate() {
            let retired = self.rng.binomial(self.realms[r].pilots, self.rules.career);
            let realm = &mut self.realms[r];
            // Some of it goes to research (#438): a step at a time, at a
            // price by how much there is to refit.
            let study = self.rules.research * c.max(0.0);
            realm.research += study;
            let c = c - study;
            if let Some(step) = crate::economy::upgrade_cost(realm.tier) {
                let price = self.rules.research_price * realm.systems.max(1) as f32 * step as f32
                    / crate::economy::upgrade_cost(0).unwrap_or(1) as f32;
                if realm.research >= price {
                    realm.research -= price;
                    realm.tier += 1;
                }
            }
            realm.crystals = (realm.crystals + c).max(0.0);
            realm.income = c + study;
            let left = realm.keep_pilots(&self.rules, retired, self.pool.len() as u32);
            let hired = std::mem::take(&mut self.realms[r].hired);
            let n = self.pool.len().saturating_sub(hired as usize);
            for p in self.pool.drain(n..) {
                self.savings += p.wallet;
            }
            self.free_pilots(r, left);
        }
        for (v, x) in self.vaults.iter_mut().zip(mined) {
            v.crystals += x;
        }
        self.fill_vaults();
    }

    /// Each realm's crystal is in its planets' vaults: what it's spent since
    /// comes out of them all alike, and what it's been paid goes in.
    pub(crate) fn fill_vaults(&mut self) {
        let mut sum = vec![0.0f64; self.realms.len()];
        let mut count = vec![0u32; self.realms.len()];
        for s in 0..self.systems.len() {
            let a = self.vault_at[s] as usize;
            for v in a..a + self.systems[s].planets as usize {
                let vault = &mut self.vaults[v];
                let here = self.systems[s].realms[vault.side as usize];
                if here != NONE {
                    vault.realm = here;
                } else if vault.realm != NONE {
                    let r = &self.realms[vault.realm as usize];
                    if r.systems == 0 || r.faction != vault.side {
                        vault.realm = NONE;
                    }
                }
                if vault.realm != NONE {
                    sum[vault.realm as usize] += vault.crystals as f64;
                    count[vault.realm as usize] += 1;
                }
            }
        }
        for v in &mut self.vaults {
            if v.realm == NONE {
                continue;
            }
            let r = v.realm as usize;
            let bank = self.realms[r].crystals.max(0.0) as f64;
            v.crystals = if sum[r] > 1e-3 {
                (v.crystals as f64 * bank / sum[r]) as f32
            } else {
                (bank / count[r] as f64) as f32
            };
        }
    }

    /// System `s`'s vaults.
    pub fn vaults_of(&self, s: usize) -> &[Vault] {
        let a = self.vault_at[s] as usize;
        &self.vaults[a..a + self.systems[s].planets as usize]
    }

    /// If side `w` holds every site on `planet` of system `s` now, its vault
    /// is theirs: the realm's, with the planet, or the raiding company's, who
    /// takes the crystal and leaves the planet its owner's.
    fn loot(&mut self, s: usize, planet: u8, w: usize) {
        let (a, b) = self.systems[s].sites;
        if self.sites[a as usize..b as usize]
            .iter()
            .any(|x| x.planet == planet && x.owner as usize != w)
        {
            return;
        }
        let v = self.vault_at[s] as usize + planet as usize;
        let vault = self.vaults[v];
        if vault.side as usize == w {
            return;
        }
        let loot = vault.crystals.max(0.0);
        if vault.realm != NONE {
            let r = &mut self.realms[vault.realm as usize];
            r.crystals = (r.crystals - loot).max(0.0);
        }
        self.captured += loot as f64;
        if w == RAID {
            self.vaults[v].crystals = 0.0;
            self.books.raided += loot as f64;
            if let Some(c) = self.companies.iter_mut().find(|c| {
                c.contract
                    .is_some_and(|k| k.target as usize == s && k.side as usize == RAID)
            }) {
                c.crystals += loot;
            }
            return;
        }
        let r = self.systems[s].realms[w];
        self.vaults[v].side = w as u8;
        self.vaults[v].realm = r;
        if r != NONE {
            self.realms[r as usize].crystals += loot;
        } else {
            self.vaults[v].crystals = 0.0;
        }
    }

    /// Each side's strength in system `s`: its hulls, with the home-ground
    /// bonus for the share of the system's sites it holds, and the initiative
    /// where its realm's offensive is. What a fight there is worth sizing up
    /// by: more hulls don't fire harder past the frontage, but they last.
    fn strength(&self, s: usize) -> [f32; SIDES] {
        let sys = &self.systems[s];
        let total = (sys.sites.1 - sys.sites.0).max(1) as f32;
        let mut out = [0.0; SIDES];
        for f in 0..SIDES {
            out[f] =
                sys.hulls[f] as f32 * (1.0 + self.rules.home_ground * sys.held[f] as f32 / total);
        }
        out
    }

    /// Each side's fire in the battle for site `t` in system `s`: its
    /// fire with no shield in its way, a seat short for a side attacking
    /// under an enemy's shield up on the site's planet (#426).
    fn fire(&self, s: usize, t: usize) -> [f32; SIDES] {
        let mut fire = self.open_fire(s);
        let planet = self.sites[t].planet as usize;
        for f in 0..SIDES {
            if f != self.sites[t].owner as usize && self.shields_up(s, f)[planet] > 0 {
                fire[f] *= self.shield_cut();
            }
        }
        fire
    }

    /// Each side's fire in system `s` with no shield in its way: its
    /// strength, scaled to the hulls that fire (`firing`).
    fn open_fire(&self, s: usize) -> [f32; SIDES] {
        let hulls = self.systems[s].hulls;
        let mut fire = self.strength(s);
        for f in 0..SIDES {
            if hulls[f] > 0 {
                fire[f] *= self.firing(s, f) / hulls[f] as f32;
            }
            // Better guns hit harder (#438).
            fire[f] *= crate::economy::damage_factor(self.gear(s, f));
        }
        fire
    }

    /// How many of side `f`'s hulls in system `s` fire: no more than it has
    /// pilots for, and no more than the frontage.
    pub fn firing(&self, s: usize, f: usize) -> f32 {
        let hulls = self.systems[s].hulls[f];
        let (mercs, crewed) = self.crewed(s, f);
        let own = hulls - mercs;
        let realm = match self.systems[s].realms[f] {
            NONE => own as f32,
            r => {
                let pilots = self.realms[r as usize].pilots as f32;
                let fights = self
                    .war_hulls
                    .get(r as usize)
                    .copied()
                    .unwrap_or(0)
                    .max(own);
                (own as f32).min(pilots * own as f32 / fights.max(1) as f32)
            }
        };
        (realm + crewed as f32).min(self.rules.frontage)
    }

    /// Side `f`'s merc hulls in system `s`, and how many of them their
    /// crews can fly.
    pub fn crewed(&self, s: usize, f: usize) -> (u32, u32) {
        let hulls = self.systems[s].hulls[f];
        let (mut mercs, mut crewed) = (0, 0);
        for c in &self.companies {
            if let Some(k) = c.contract
                && k.target as usize == s
                && k.side as usize == f
            {
                let field = k.field.min(hulls - mercs);
                mercs += field;
                crewed += field.min(c.pilots.len() as u32);
            }
        }
        (mercs, crewed)
    }

    /// Count each realm's hulls at war, so `firing` can split its pilots
    /// between its fights.
    fn share_pilots(&mut self) {
        self.lend_hulls();
        let mut war = vec![0; self.realms.len()];
        for realm in &mut self.realms {
            realm.fronts = 0;
        }
        for s in 0..self.systems.len() {
            for f in 0..SIDES {
                let r = self.systems[s].realms[f];
                if r != NONE && self.fighting(s, f) {
                    war[r as usize] += self.systems[s].hulls[f] - self.crewed(s, f).0;
                    self.realms[r as usize].fronts += 1;
                }
            }
        }
        self.war_hulls = war;
    }

    /// What's left of a side's fire under an enemy's shield: a seat short.
    fn shield_cut(&self) -> f32 {
        let seats = self.rules.seats.max(1.0);
        (seats - SHIELD_SEATS as f32).max(0.0) / seats
    }

    /// Side `f`'s gear level in system `s` (#438): its realm's tier for the
    /// realm's hulls, each company's pilots' for its own, by their hulls.
    pub fn gear(&self, s: usize, f: usize) -> u8 {
        let sys = &self.systems[s];
        let realm = match sys.realms[f] {
            NONE => 0,
            r => self.realms[r as usize].tier,
        };
        let hulls = sys.hulls[f];
        if hulls == 0 {
            return realm;
        }
        // By the hulls that fire: a company's flown by its best pilots.
        let (mut crewed, mut merc) = (0.0, 0.0);
        for c in &self.companies {
            if let Some(k) = c.contract
                && k.target as usize == s
                && k.side as usize == f
            {
                let n = (k.field.min(hulls) as usize).min(c.pilots.len());
                let mut levels: Vec<f32> = c.pilots.iter().map(|p| p.rank).collect();
                levels.sort_by(|a, b| b.total_cmp(a));
                crewed += n as f32;
                merc += levels.iter().take(n).sum::<f32>();
            }
        }
        let own = (self.firing(s, f) - crewed).max(0.0);
        if own + crewed <= 0.0 {
            return realm;
        }
        let all = (merc + own * realm as f32) / (own + crewed);
        all.round() as u8
    }

    /// Are `f` and `g` fighting each other? Every people is fighting every
    /// other (realms of one people don't fight each other, yet).
    fn hostile(&self, f: usize, g: usize) -> bool {
        f != g
    }

    /// The tick's battles (#422). A battle is a battle (#493): every one,
    /// played or simmed, ends the same way (`battle::Outcome`: each side's
    /// hulls lost, each kill some pilot's, a winner) and goes into the
    /// galaxy the one way (`fight`). In every system at war, each side
    /// fighting attacks every site it goes for (`targets`, #460), picked
    /// before anything changes hands. The battles are fought one at a time,
    /// each with the hulls its sides have left: the played one first, if
    /// any, its outcome from the match (`settle`); then the battle for each
    /// other site attacked, in site order, its outcome simmed (`sim`). A
    /// damaged generator nobody fought over is working again.
    fn battles(&mut self, played: Option<&battle::Played>) -> Option<battle::Settled> {
        self.assaulted.clear();
        self.assaulted.resize(self.sites.len(), false);
        self.share_pilots();
        let mut settled = None;
        for s in 0..self.systems.len() {
            let here = played.filter(|p| p.battle.system == s);
            let present = self.present(s);
            let war = (0..SIDES).any(|f| self.fighting(s, f));
            if !war && here.is_none() {
                // Nobody here to fight: each side holds what it has, and
                // walks into any enemy site left unguarded.
                for f in 0..SIDES {
                    if present[f] {
                        self.hold(s, f);
                    }
                }
                continue;
            }
            let targets: [Vec<usize>; SIDES] = std::array::from_fn(|f| match war {
                true if self.fighting(s, f) => self.targets(s, f),
                _ => Vec::new(),
            });
            let played_site = here.and_then(|p| p.battle.site);
            if let Some(p) = here {
                settled = Some(self.settle(p));
            }
            let mut sites: Vec<usize> = targets.iter().flatten().copied().collect();
            sites.sort();
            sites.dedup();
            for t in sites.into_iter().filter(|&t| Some(t) != played_site) {
                let holder = self.sites[t].owner as usize;
                let hulls = self.systems[s].hulls;
                let mut sides: Vec<usize> = (0..SIDES)
                    .filter(|&f| f != holder && hulls[f] > 0 && targets[f].contains(&t))
                    .collect();
                if sides.is_empty() {
                    continue;
                }
                sides.push(holder);
                let o = self.sim(s, t, &sides);
                self.fight(s, &o);
            }
        }
        // Nobody fought over them: repaired.
        for (site, &fought) in self.sites.iter_mut().zip(&self.assaulted) {
            if let SiteKind::Shield { damaged } = &mut site.kind
                && !fought
            {
                *damaged = false;
            }
        }
        settled
    }

    /// Sim the battle for site `t` in system `s` between `sides` (its
    /// attackers, then its holder), each with every hull it has left here.
    /// Each side's fire (`fire`, a little luck either way) spreads over its
    /// enemies in it by their hulls; each of a side's hulls is lost with the
    /// chance the fire on it gives (better armor takes more stopping, #438),
    /// each loss some enemy pilot's kill (`credit`), and some of them its
    /// mercs' (`mercs_share`). The holder keeps the site unless an attacker
    /// takes it, a chance by its share of the fire.
    fn sim(&mut self, s: usize, t: usize, sides: &[usize]) -> battle::Outcome {
        let hulls = self.systems[s].hulls;
        let base = self.fire(s, t);
        let mut fire = [0.0; SIDES];
        for &f in sides {
            fire[f] = base[f] * self.rng.range(0.75, 1.25);
        }
        let (losses, by) = self.losses(&hulls, &fire, sides);
        let mut o = battle::Outcome {
            site: Some(t),
            lost: Default::default(),
            winner: None,
            scrap: Vec::new(),
        };
        for &f in sides {
            if hulls[f] == 0 || losses[f] <= 0.0 {
                continue;
            }
            let x = losses[f] / crate::economy::armor_factor(self.gear(s, f));
            let lost = self.rng.binomial(hulls[f], x / hulls[f] as f32);
            let scrap = self.credit(s, f, lost, &by[f], &hulls);
            o.scrap.extend(scrap);
            let mercs = self.mercs_share(s, f, lost);
            let theirs: u32 = mercs.iter().map(|m| m.1).sum();
            o.lost[f] = (lost - theirs, mercs);
        }
        let (holder, attackers) = sides.split_last().expect("a battle has a holder");
        let mut winner = *holder;
        for &f in attackers {
            if self.rng.chance(self.take_chance(&fire, sides, f)) {
                winner = f;
                break;
            }
        }
        o.winner = Some(winner);
        o
    }

    /// The fire each of `sides` (with `hulls`) takes in a battle, before
    /// armor, at each side's `fire`: each side's fire spreads over its
    /// enemies by their hulls. Returns it, and who it's from: each side's
    /// share of each other's.
    fn losses(
        &self,
        hulls: &[u32; SIDES],
        fire: &[f32; SIDES],
        sides: &[usize],
    ) -> ([f32; SIDES], [[f32; SIDES]; SIDES]) {
        let mut losses = [0.0f32; SIDES];
        let mut by = [[0.0f32; SIDES]; SIDES];
        for &g in sides {
            let enemies = || sides.iter().copied().filter(move |&f| f != g);
            let targets: f32 = enemies().map(|f| hulls[f] as f32).sum();
            if targets <= 0.0 {
                continue;
            }
            for f in enemies() {
                let x = self.rules.kill * fire[g] * hulls[f] as f32 / targets;
                losses[f] += x;
                by[f][g] += x;
            }
        }
        (losses, by)
    }

    /// The chance attacker `f` takes the site, if no attacker before it
    /// in `sides` has: `take` times its share of the battle's fire.
    fn take_chance(&self, fire: &[f32; SIDES], sides: &[usize], f: usize) -> f32 {
        let against: f32 = sides.iter().filter(|&&g| g != f).map(|&g| fire[g]).sum();
        self.rules.take * fire[f] / (fire[f] + against).max(1e-3)
    }

    /// What the galaxy's own battle math (`sim`) expects of the battle for
    /// site `t` in system `s` between `sides` (its attackers, then its
    /// holder), with no luck either way (#494): each side's hulls, gear,
    /// fire, hulls lost on average, and chance to win. For setting played
    /// battles beside, to see how far off it is.
    pub fn predict(&self, s: usize, t: usize, sides: &[usize]) -> Vec<ledger::Predicted> {
        let hulls = self.systems[s].hulls;
        let base = self.fire(s, t);
        let mut fire = [0.0; SIDES];
        for &f in sides {
            fire[f] = base[f];
        }
        let (losses, _) = self.losses(&hulls, &fire, sides);
        let mut win = [0.0f32; SIDES];
        let mut none_yet = 1.0;
        if let Some((holder, attackers)) = sides.split_last() {
            for &f in attackers {
                let p = self.take_chance(&fire, sides, f);
                win[f] = none_yet * p;
                none_yet *= 1.0 - p;
            }
            win[*holder] = none_yet;
        }
        sides
            .iter()
            .map(|&f| {
                let gear = self.gear(s, f);
                let lost = losses[f] / crate::economy::armor_factor(gear);
                ledger::Predicted {
                    faction: Faction::side(f).name().to_string(),
                    hulls: hulls[f],
                    gear,
                    fire: fire[f],
                    lost: lost.min(hulls[f] as f32),
                    win: win[f],
                }
            })
            .collect()
    }

    /// Side `f` lost `lost` hulls in system `s` (#438): each is a kill, by a
    /// side with the chance its share of the fire on `f` gives it (`by`),
    /// and on that side, by one of its hulls: a company's (one of its pilots
    /// takes apart the wreck) with the chance its hulls make up of the
    /// side's here, else the realm's (whose pilots fly its researched tier,
    /// and learn nothing from wrecks). Returns the wrecks to take apart.
    fn credit(
        &mut self,
        s: usize,
        f: usize,
        lost: u32,
        by: &[f32; SIDES],
        hulls: &[u32; SIDES],
    ) -> Vec<battle::Scrap> {
        let mut out = Vec::new();
        let total: f32 = by.iter().sum();
        if lost == 0 || total <= 0.0 {
            return out;
        }
        let level = self.gear(s, f);
        let maker = Faction::side(f);
        let parts: Vec<crate::economy::Part> = crate::economy::PARTS
            .into_iter()
            .filter(|p| p.maker() == maker)
            .collect();
        if parts.is_empty() {
            return out;
        }
        // The companies here, each side's: (index, side, its hulls here).
        let here: Vec<(usize, usize, u32)> = (0..self.companies.len())
            .filter_map(|c| {
                let k = self.companies[c].contract?;
                (k.target as usize == s && k.field > 0 && !self.companies[c].pilots.is_empty())
                    .then_some((c, k.side as usize, k.field))
            })
            .collect();
        for _ in 0..lost {
            // Whose side, then whose hull.
            let mut x = self.rng.f32() * total;
            let Some(g) = (0..SIDES).find(|&g| {
                x -= by[g];
                x < 0.0
            }) else {
                continue;
            };
            let mut y = self.rng.f32() * hulls[g].max(1) as f32;
            let Some(&(c, ..)) = here.iter().filter(|h| h.1 == g).find(|h| {
                y -= h.2 as f32;
                y < 0.0
            }) else {
                continue;
            };
            let p = self.rng.index(self.companies[c].pilots.len());
            let part = parts[self.rng.index(parts.len())];
            let co = &self.companies[c];
            out.push((co.id, co.pilots[p].id, part, level));
        }
        out
    }

    /// Of side `f`'s `lost` hulls in system `s`, each company's there
    /// (`Company::id`, hulls): every hull lost is a company's with the
    /// chance its hulls make up of the side's here.
    fn mercs_share(&mut self, s: usize, f: usize, lost: u32) -> Vec<(u32, u32)> {
        let mut pool = self.systems[s].hulls[f];
        let mut left = lost.min(pool);
        let mut out = Vec::new();
        for c in &self.companies {
            let Some(k) = c.contract else { continue };
            if k.target as usize != s || k.side as usize != f || k.field == 0 || pool == 0 {
                continue;
            }
            let field = k.field.min(pool);
            let theirs = self
                .rng
                .binomial(left, field as f32 / pool as f32)
                .min(field);
            pool -= field;
            left -= theirs;
            if theirs > 0 {
                out.push((c.id, theirs));
            }
        }
        out
    }

    /// Side `f` loses hulls in system `s`: `realm` of the realm's, and each
    /// company in `mercs` (`Company::id`, hulls) some of its contract's
    /// (never more than it has there). The one effect of a lost hull
    /// (#422): off the side's hulls here, off the company's contract, and
    /// counted. Returns (lost all told, of those the mercs').
    pub(crate) fn lose(
        &mut self,
        s: usize,
        f: usize,
        realm: u32,
        mercs: &[(u32, u32)],
    ) -> (u32, u32) {
        let mut theirs = 0;
        for &(id, n) in mercs {
            let Some(k) = self
                .companies
                .iter_mut()
                .find(|c| c.id == id)
                .and_then(|c| c.contract.as_mut())
            else {
                continue;
            };
            let off = n.min(k.field);
            k.field -= off;
            k.lost += off;
            self.books.lost += off as f64;
            theirs += off;
        }
        let sys = &mut self.systems[s];
        let total = (realm + theirs).min(sys.hulls[f]);
        sys.hulls[f] -= total;
        self.moves[s][f].lost += total;
        self.merc_moves[s][f].lost += theirs.min(total);
        self.destroyed += total as f64;
        (total, theirs.min(total))
    }

    /// Side `w` won the battle for site `i` in system `s`: the one effect of
    /// a won battle, played or the galaxy's (#422). From another side, it
    /// takes the site (a generator taken is damaged); its holder's, a
    /// damaged generator is working again.
    pub(crate) fn win(&mut self, s: usize, i: usize, w: usize) {
        if let Some(x) = self.assaulted.get_mut(i) {
            *x = true;
        }
        let site = &mut self.sites[i];
        let old = site.owner as usize;
        if old == w {
            if let SiteKind::Shield { damaged } = &mut site.kind {
                *damaged = false;
            }
            return;
        }
        site.owner = w as u8;
        if let SiteKind::Shield { damaged } = &mut site.kind {
            if !*damaged {
                self.shields_down += 1;
            }
            *damaged = true;
        }
        let loser = self.holder(s);
        let sys = &mut self.systems[s];
        sys.held[old] -= 1;
        sys.held[w] += 1;
        self.taken += 1;
        self.claim(s, w);
        if loser != NONE && w != RAID && self.holder(s) != loser {
            self.fallen(s);
        }
        let planet = self.sites[i].planet;
        self.loot(s, planet, w);
    }

    /// System `s` changed hands: the sooner worlds do, the less one's worth.
    fn fallen(&mut self, s: usize) {
        let new = self.tick < self.taken_at[s] + 300 && self.taken_at[s] > 0;
        if new {
            self.fell_new += 1;
        } else {
            self.fell_old += 1;
        }
        self.taken_at[s] = self.tick;
    }

    /// How long worlds last: the share of those lately taken, and of the
    /// others within reach of an enemy, that changed hands this tick.
    fn reckon_holds(&mut self) {
        let (mut new, mut old) = (0u32, 0u32);
        for s in 0..self.systems.len() {
            if self.taken_at[s] > 0 && self.tick < self.taken_at[s] + 300 {
                new += 1;
            } else if self.border[s] {
                old += 1;
            }
        }
        // With few lately taken, there's little to go on but the rest.
        let seen = if new > 20 {
            self.fell_new as f32 / new as f32
        } else {
            self.lose_old
        };
        self.lose_new += 0.02 * (seen - self.lose_new);
        self.lose_old += 0.02 * (self.fell_old as f32 / old.max(1) as f32 - self.lose_old);
        (self.fell_new, self.fell_old) = (0, 0);
    }

    /// Ticks a side can expect to hold a world it takes (`new`), or has.
    pub(crate) fn hold_for(&self, new: bool) -> f32 {
        1.0 / if new { self.lose_new } else { self.lose_old }.max(0.001)
    }

    /// Up enemy shield generators on each planet of system `s`, as `f` sees them.
    fn shields_up(&self, s: usize, f: usize) -> [u8; 4] {
        let (a, b) = self.systems[s].sites;
        let mut up = [0u8; 4];
        for site in &self.sites[a as usize..b as usize] {
            if self.hostile(f, site.owner as usize)
                && site.kind == (SiteKind::Shield { damaged: false })
            {
                up[site.planet as usize] += 1;
            }
        }
        up
    }

    /// Side `f` has no enemy in system `s`: it walks into enemy sites
    /// nobody's left to defend, a few a tick. (Its damaged generators are
    /// repaired with everyone's nobody fought over.)
    fn hold(&mut self, s: usize, f: usize) {
        for _ in 0..self.rules.blitz {
            let Some(&i) = self.targets(s, f).first() else {
                return;
            };
            self.win(s, i, f);
        }
    }

    /// What each side can spare where: hulls beyond what it keeps home.
    pub(crate) fn count_spare(&mut self) {
        let n = self.systems.len();
        // What each side can spare where: hulls beyond what it keeps home.
        for s in 0..n {
            let sys = &self.systems[s];
            let contested = sys.at_war();
            for f in 0..SIDES {
                self.spare[s][f] = if contested || sys.held[f] == 0 {
                    // Fighting here (or here to fight): nothing to spare.
                    0
                } else {
                    (sys.hulls[f] as f32 * (1.0 - self.rules.keep)) as u32
                };
            }
        }
        // Companies' hulls are theirs, not the realm's to send.
        for c in &self.companies {
            if let Some(k) = c.contract {
                let x = &mut self.spare[k.target as usize][k.side as usize];
                *x = x.saturating_sub(k.field);
            }
        }
    }

    /// Every realm decides where to send its hulls.
    fn send(&mut self) {
        let n = self.systems.len();
        self.count_spare();
        self.count_incoming();
        // A different people moves first each tick, starting at a different
        // system, so nobody always gets first pick.
        let mut order = [0, 1, 2, 3];
        for i in (1..PEOPLES).rev() {
            order.swap(i, self.rng.index(i + 1));
        }
        let offset = self.rng.index(n);
        for f in order {
            for i in 0..n {
                let t = (i + offset) % n;
                // Nothing to do deep in anyone's space.
                let sys = &self.systems[t];
                if !self.border[t] && sys.held[f] == 0 && sys.hulls[f] == 0 {
                    continue;
                }
                if sys.held[f] > 0
                    && (0..SIDES).all(|g| g == f || (sys.held[g] == 0 && sys.hulls[g] == 0))
                {
                    continue;
                }
                // The realms of this people that could send here: each
                // with hulls to spare in range, and none of them blocked by
                // another realm of the people already in the system.
                let mut from = [NONE; NEIGHBORS];
                let mut k = 0;
                for (s, _) in self.neighbors(t) {
                    let r = self.systems[s].realms[f];
                    if r != NONE && self.spare[s][f] > 0 && !from[..k].contains(&r) {
                        from[k] = r;
                        k += 1;
                    }
                }
                let here = self.systems[t].realms[f];
                // A realm's own world under attack it always looks at, if only
                // to see what it's missing.
                if here != NONE && self.systems[t].held[f] > 0 && !from[..k].contains(&here) {
                    self.consider(t, f, here);
                }
                for &r in &from[..k] {
                    if here == NONE || here == r {
                        self.consider(t, f, r);
                    }
                }
            }
        }
        // Crews hired to fly their employer's hulls have them.
        self.lend_hulls();
    }

    /// Should realm `r` (of side `f`) send hulls to system `t`, hire for it,
    /// or both? Whatever the battle maths says gains it most over doing
    /// nothing, if anything does.
    fn consider(&mut self, t: usize, f: usize, r: u32) {
        let sys = &self.systems[t];
        let ours = sys.held[f] > 0;
        let here = sys.hulls[f];
        let attacked = (0..SIDES).any(|g| g != f && sys.hulls[g] >= AT_WAR);
        if ours && !attacked && (0..SIDES).all(|g| g == f || sys.held[g] == 0) {
            return;
        }
        // A fight it's in, every tick; somewhere new, now and then.
        let raided = ours && attacked;
        if here == 0 && !raided && !(self.tick + t as u64).is_multiple_of(THINK) {
            return;
        }
        let worth = self.worth(t, f, ours);
        if worth <= 0.0 {
            return;
        }
        let mine = |g: &Galaxy, s: usize| {
            if g.systems[s].realms[f] == r {
                g.spare[s][f]
            } else {
                0
            }
        };
        let pool: u32 = self.neighbors(t).map(|(s, _)| mine(self, s)).sum();
        let cost_all: f32 = self
            .neighbors(t)
            .map(|(s, d)| mine(self, s) as f32 * d / FTL_RANGE * self.rules.ftl_cost)
            .sum();
        let realm = self.realms[r as usize];
        let afford = if cost_all > 0.0 {
            (realm.crystals / cost_all).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let have = here as f32;
        let (late, late_at) = self.on_the_way(t, f);
        // Its attack's on the way already.
        if here == 0 && late > 0.0 && !ours {
            return;
        }
        let pilots = self.pilots_for(r, have + late);
        // How long what it sends would take: an attack lands all together.
        let (mut flight, mut slowest) = (0.0f32, 1u64);
        for (s, d) in self.neighbors(t) {
            let n = mine(self, s);
            if n > 0 {
                flight += n as f32 * self.flight(d) as f32;
                slowest = slowest.max(self.flight(d));
            }
        }
        let fresh = here == 0 && !ours;
        let more_at = if fresh {
            0.0
        } else {
            flight / (pool as f32).max(1.0)
        };
        let plan = |more: f32, pilots: f32| {
            let all = late + more;
            merc::Plan {
                own: if fresh { have + more } else { have },
                late: if fresh { 0.0 } else { all },
                late_at: (late * late_at + more * more_at) / all.max(1e-3),
                flown: pilots,
                defend: ours,
                ..Default::default()
            }
        };
        // (what it'd hold, what it'd cost it) with `more` of its own.
        let play = |g: &Galaxy, more: f32, pilots: f32| {
            let fc = g.forecast(t, f, r, &plan(more, pilots));
            (fc.share * worth - fc.lost * realm.hull_worth, fc.length)
        };
        let (stay, length) = play(self, 0.0, pilots);
        // What more pilots, or hulls, would be worth to it: in a fight it's
        // in, or an attack it could make.
        let all = pool as f32 * afford;
        if (here > 0 && attacked) || raided || (here == 0 && pool >= MIN_SEND) {
            let more = if here > 0 { 0.0 } else { all };
            let p = self.pilots_for(r, have + late + more);
            let fly = if here > 0 { 0.0 } else { cost_all * afford };
            let (now, l) = play(self, more, p);
            let now = (now - fly).max(stay);
            let length = if here > 0 { length } else { l };
            // (a hull more than those it could already bring)
            let reserve = self.pilots_for(r, have + late + all);
            let most = play(self, all, reserve).0;
            let (mut pilot, mut hull) = (0.0f32, 0.0f32);
            for n in [10.0, 40.0, 85.0] {
                pilot = pilot.max((play(self, more, p + n).0 - fly - now) / n / length);
                hull = hull.max((play(self, all + 4.0 * n, reserve).0 - most) / (4.0 * n));
            }
            let x = &mut self.realms[r as usize];
            x.pilot_use = x.pilot_use.max(pilot);
            x.hull_use = x.hull_use.max(hull);
            // There, and wherever it could send one from.
            for s in std::iter::once(t)
                .chain(self.neighbors(t).map(|(s, _)| s))
                .collect::<Vec<_>>()
            {
                if self.systems[s].realms[f] == r {
                    self.hull_use[s] = self.hull_use[s].max(hull);
                }
            }
        }
        let mut best = (0.0, 0.0f32);
        if pool >= MIN_SEND {
            for share in [0.1, 0.25, 0.5, 1.0] {
                let n = pool as f32 * share * afford;
                if n < MIN_SEND as f32 {
                    continue;
                }
                let gain = play(self, n, self.pilots_for(r, have + late + n)).0
                    - stay
                    - cost_all * share * afford;
                if gain > best.0 {
                    best = (gain, share * afford);
                }
            }
        }
        // Or with a company, going in with all it can pay to move.
        let with = if best.1 > 0.0 {
            pool as f32 * best.1
        } else {
            all
        };
        let base = if best.1 > 0.0 {
            stay + best.0
        } else {
            stay + cost_all * afford
        };
        let help = plan(with, self.pilots_for(r, have + late + with));
        let start = if fresh { slowest } else { 0 };
        if realm.hires
            && (attacked || pool >= MIN_SEND)
            && self.hire(t, f, r, help, start, base)
            && best.1 == 0.0
        {
            best.1 = afford;
        }
        if best.1 * (pool as f32) < MIN_SEND as f32 {
            return;
        }
        let cost = (cost_all * best.1).min(self.realms[r as usize].crystals.max(0.0));
        self.realms[r as usize].crystals -= cost;
        self.sent += 1;
        for k in 0..NEIGHBORS {
            let (s, d) = self.neighbors[t * NEIGHBORS + k];
            if s == u32::MAX {
                break;
            }
            let s = s as usize;
            // Whole hulls go.
            let m = (mine(self, s) as f32 * best.1) as u32;
            if m == 0 {
                continue;
            }
            self.spare[s][f] -= m;
            self.systems[s].hulls[f] -= m;
            self.moves[s][f].left += m;
            let arrives = self.tick + if fresh { slowest } else { self.flight(d) };
            self.fleets.push(Fleet {
                to: t as u32,
                side: f as u8,
                realm: r,
                hulls: m,
                arrives,
            });
            self.note_incoming(t as u32, f as u8, m, arrives);
        }
    }

    /// The pilots realm `r` would have for `own` hulls in one fight, sharing
    /// them with its others (`Galaxy::firing`).
    pub(crate) fn pilots_for(&self, r: u32, own: f32) -> f32 {
        let pilots = self.realms[r as usize].pilots as f32;
        let fights = self.war_hulls.get(r as usize).copied().unwrap_or(0) as f32;
        pilots * own / fights.max(own).max(1.0)
    }
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
        let fair = spots.len() as f32 / PEOPLES as f32;
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
        "generated {} systems, {} sites, {} realms in {:.1?}",
        g.systems.len(),
        g.sites.len(),
        g.living().count(),
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
            let shares: Vec<String> = (0..PEOPLES)
                .map(|f| {
                    format!(
                        "{} {:4.1}% {:3}",
                        &names[f][..3],
                        c.share[f] * 100.0,
                        c.realms[f]
                    )
                })
                .collect();
            let hulls: f32 = c.hulls.iter().sum();
            // Realms' gear tiers (#438): lowest, median, highest.
            let mut tiers: Vec<u8> = g.living().map(|(_, r)| r.tier).collect();
            tiers.sort();
            let tier = |q: f32| {
                let t = tiers
                    .get(((tiers.len().max(1) - 1) as f32 * q) as usize)
                    .copied()
                    .unwrap_or(0);
                crate::economy::level_label(t)
            };
            let tiers = format!("{}/{}/{}", tier(0.0), tier(0.5), tier(1.0));
            // Merc pilots' gear, averaged over their parts: median and best.
            let mut pilots: Vec<f32> = g
                .companies
                .iter()
                .flat_map(|c| c.pilots.iter().map(|p| p.level()))
                .collect();
            pilots.sort_by(|a, b| a.total_cmp(b));
            let pilot = |q: f32| {
                let l = pilots
                    .get(((pilots.len().max(1) - 1) as f32 * q) as usize)
                    .copied()
                    .unwrap_or(0.0);
                crate::economy::level_label(l.round() as u8)
            };
            let tiers = format!("{tiers} pilots {}/{}", pilot(0.5), pilot(1.0));
            println!(
                "tick {:6}  {}  at war {:4.1}%  sent {:4} shields {:4} taken {:4}  hulls {:8.0}  splits {} merges {}  pockets {}  vaults {:.0}  companies {} on contract {} worth {:.0}  tiers {tiers}",
                g.tick,
                shares.join("  "),
                c.at_war * 100.0,
                g.sent,
                g.shields_down,
                g.taken,
                hulls,
                g.splits,
                g.merges,
                g.cut.iter().filter(|&&c| c).count(),
                c.vaults,
                c.companies,
                c.contracts,
                c.merc_capital,
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
    println!("captured {:.0} crystal with fallen systems", g.captured);
    let b = g.books;
    println!(
        "mercs: {} contracts signed, {} won, {} spent their hulls, {} ran out; realms paid {:.0} up front and {:.0} billed ({:.0} owed when due); worlds won worth {:.0} to their employers cost them {:.0}; companies split {} times and joined {}",
        b.signed,
        b.won,
        b.broken,
        b.expired,
        b.upfront,
        b.billed,
        b.defaulted,
        b.worth_won,
        b.paid_won,
        b.splits,
        b.joins,
    );
    if let Some(out) = out {
        chart::render(&snaps, &history).save(out)?;
        println!("saved {out}");
    }
    Ok(())
}
