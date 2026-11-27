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
//! Uthenn planets also turn out tungsten cubes, which only the Uthenn care
//! for (`cubes`).
//!
//! Outposts make nothing: they're sites a world isn't taken without.
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
//!    in that many, the same seat a played battle closes. Most sides go for
//!    the generators first, all of them together, to get out from under
//!    them; a smart realm picks what's worth fighting for. Only so many of a side's hulls fire a tick however big it
//!    is (`Rules::frontage`), so a world holds out for many ticks.
//!    Each firing hull needs a pilot (`Galaxy::firing`), and a pilot fights
//!    one battle a tick: a side's are split between all it's in. Pilots cap fire,
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
//!    less what it'd cost. FTL costs crystal per hull
//!    by distance, and takes time (`Fleet`): what's in flight, its enemies
//!    don't see till it lands.
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
pub mod cubes;
pub mod judge;
pub mod ledger;
pub mod merc;
pub mod realm;
pub mod save;
mod smart;
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

/// How far either way luck takes a side's fire in a battle.
const LUCK: f32 = 0.25;
/// What a planet of ordinary riches makes a tick, all its sites together:
/// crystals, and hulls.
const CRYSTALS: f32 = 0.127;
const HULLS: f32 = 31.7;
/// How far planets' riches, and stars, spread: the deviation of their
/// logarithm. Most are a little poor; a few, very rich.
const RICH_SPREAD: f32 = 0.8;
const STAR_SPREAD: f32 = 0.9;
/// The richest of either, against the ordinary.
const RICHEST: f32 = 8.0;
/// Smallest sending worth the FTL (hulls).
const MIN_SEND: u32 = 200;
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
    /// Tungsten cubes a tick a planet of ordinary tungsten makes, and what a
    /// cube's FTL costs, as a hull's.
    pub cubes: f32,
    pub cube_cost: f32,
    /// Light years a fleet, or a company's ship, flies in a tick.
    pub ftl_speed: f32,
    /// Hulls a pilot destroys in a battle, gear and numbers even: what bots
    /// do in a played one (`--arena`: about 80 kills a round among 12).
    pub kill: f32,
    /// Seats a side has in a battle, as in a played one: an attacker under
    /// an enemy's shield fights with `SHIELD_SEATS` fewer.
    pub seats: f32,
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
            ftl_cost: 0.01,
            cubes: 5.0,
            cube_cost: 0.01,
            ftl_speed: 40.0,
            kill: 6.7,
            seats: 6.0,
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
            train_cost: 3.0,
            career: 0.002,
            patience: 60,
            pay_step: 0.005,
            sense: 0.03,
            train: 2,
            crew: 40,
            wage_cr: 100.0,
            kill_share: 0.1,
            defect: 0.01,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SiteKind {
    /// Can turn out so many hulls a tick, into its holder's storage in the
    /// system.
    Foundry { hulls: f32 },
    /// Turns out so much crystal a tick, into its planet's vault.
    Mine { crystals: f32 },
    /// A shield generator: working, or damaged (#422). It's damaged when
    /// it's taken, and working again once its holder wins a battle for it,
    /// or a tick goes by without one.
    Shield { damaged: bool },
    /// Makes nothing and shields nothing, but a world isn't taken without it.
    Outpost,
}

impl SiteKind {
    /// It makes something.
    pub fn produces(self) -> bool {
        matches!(self, SiteKind::Foundry { .. } | SiteKind::Mine { .. })
    }
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

/// What's in a planet's ground, 1 the galaxy's average: crystal and what
/// hulls are made of, as its mines and foundries make of them; and
/// tungsten. One says nothing of the others.
#[derive(Clone, Copy, Debug)]
pub struct Riches {
    pub crystal: f32,
    pub hulls: f32,
    pub tungsten: f32,
}

impl Riches {
    fn roll(rng: &mut Rng) -> Riches {
        Riches {
            crystal: spread(rng, RICH_SPREAD),
            hulls: spread(rng, RICH_SPREAD),
            tungsten: spread(rng, RICH_SPREAD),
        }
    }

    /// Its mines and its foundries: more where it's richer, none where
    /// there's hardly any, but always one or the other.
    fn sites(&self) -> (u32, u32) {
        let of = |x: f32| match x {
            x if x < 0.45 => 0,
            x if x < 1.4 => 1,
            x if x < 3.0 => 2,
            _ => 3,
        };
        match (of(self.crystal), of(self.hulls)) {
            (0, 0) if self.crystal > self.hulls => (1, 0),
            (0, 0) => (0, 1),
            n => n,
        }
    }
}

/// How much of something there is, 1 on average: a little, mostly, now and
/// then a lot.
fn spread(rng: &mut Rng, deviation: f32) -> f32 {
    (deviation * rng.normal() - deviation * deviation / 2.0)
        .exp()
        .min(RICHEST)
}

#[derive(Clone, Debug)]
pub struct System {
    /// Light years: in `REGION` in the plane, `z` off the midplane.
    pub pos: Vec3,
    /// How bright its star is, 1 the galaxy's average.
    pub starlight: f32,
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
#[derive(Clone)]
pub struct Galaxy {
    /// A hire (realm, system) that isn't to happen (`judge`).
    pub(crate) ban: Option<(u32, u32)>,
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
    /// The tick each system was last fought over.
    pub(crate) fought_at: Vec<u64>,
    pub(crate) lose_new: f32,
    pub(crate) lose_old: f32,
    /// Savings of retired pilots and dead ships, for new companies.
    pub(crate) savings: f32,
    /// Hulls in flight.
    pub fleets: Vec<Fleet>,
    /// Scratch: the system whose battles are being fought, and how many each
    /// side was in as the tick's began.
    engaged: Option<(usize, [u32; SIDES])>,
    /// What one more hull in each system has lately been worth to its holder,
    /// and its best use this tick (scratch).
    pub(crate) hull_worth: Vec<f32>,
    hull_use: Vec<f32>,
    /// Scratch: what each side has on the way to a system: hulls, and their
    /// ticks to go all told.
    incoming: std::collections::HashMap<(u32, u8), (f32, f32)>,
    /// Every planet's vault, and each system's first.
    pub vaults: Vec<Vault>,
    /// Every planet's riches, in the vaults' order.
    pub riches: Vec<Riches>,
    /// Every planet's tungsten cubes, likewise; and those in flight.
    pub stashes: Vec<cubes::Stash>,
    pub cargo: Vec<cubes::Cargo>,
    pub(crate) vault_at: Vec<u32>,
    /// What a company has lately made a tick, on average; and what they
    /// were all worth last tick.
    pub(crate) profit: f32,
    /// What a contract has lately made a company, over its costs.
    pub(crate) job_worth: f32,
    /// What hires have lately come to, of what was expected of them (attacks,
    /// defenses), and those yet to be looked back on.
    pub hire_trust: [f32; 2],
    pub(crate) lessons: Vec<merc::Lesson>,
    /// The gear of pilots ships have lately hired.
    pub(crate) hire_rank: f32,
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
    /// The contract you're signed on for (#557).
    pub job: Option<theater::Job>,
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
    pub(crate) rng: Rng,
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
        // (Outposts' own dice: the rest of the galaxy's as it was without them.)
        let mut posts = Rng::new(seed ^ 0x0007_9057);
        // (And its riches' and stars'.)
        let mut ground = Rng::new(seed ^ 0x0060_0D5E);
        let mut riches = Vec::new();
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
                // Mines and foundries, sharing out what the planet makes.
                let rich = Riches::roll(&mut ground);
                riches.push(rich);
                let (mines, foundries) = rich.sites();
                for k in 0..mines + foundries {
                    let uneven = rng.range(0.7, 1.3);
                    sites.push(Site {
                        planet,
                        owner,
                        kind: if k < mines {
                            SiteKind::Mine {
                                crystals: CRYSTALS * rich.crystal / mines as f32 * uneven,
                            }
                        } else {
                            SiteKind::Foundry {
                                hulls: HULLS * rich.hulls / foundries as f32 * uneven,
                            }
                        },
                    });
                }
                // Most planets have an outpost or two.
                let outposts = match posts.f32() {
                    x if x < 0.1 => 0,
                    x if x < 0.5 => 1,
                    x if x < 0.9 => 2,
                    _ => 3,
                };
                for _ in 0..outposts {
                    sites.push(Site {
                        planet,
                        owner,
                        kind: SiteKind::Outpost,
                    });
                }
            }
            let end = sites.len() as u32;
            let mut held = [0; SIDES];
            held[owner as usize] = (end - start) as u8;
            systems.push(System {
                pos,
                starlight: spread(&mut ground, STAR_SPREAD),
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
            fought_at: vec![0; count],
            lose_new: 0.01,
            lose_old: 0.003,
            savings: 0.0,
            fleets: Vec::new(),
            engaged: None,
            hull_worth: vec![0.01; count],
            hull_use: vec![0.0; count],
            incoming: Default::default(),
            vaults: Vec::new(),
            stashes: vec![Default::default(); riches.len()],
            cargo: Vec::new(),
            riches,
            vault_at: Vec::new(),
            profit: 0.0,
            ban: None,
            job_worth: 5.0,
            hire_trust: [1.0; 2],
            lessons: Vec::new(),
            hire_rank: 0.0,
            worth_before: 0.0,
            hull_rate: 0.01,
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
            job: None,
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
            if let SiteKind::Foundry { hulls } = site.kind {
                out[site.owner as usize] += hulls;
            }
        }
        out
    }

    #[cfg(test)]
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
                .map(|c| c.capital(self.hull_rate))
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
    /// the borders.
    pub(crate) fn update_supply(&mut self) {
        self.keep_books();
        if self.tick > 0 && self.tick.is_multiple_of(self.rules.politics.max(1) as u64) {
            self.reshape();
        }
        self.find_borders();
    }

    /// The books and the borders brought up to date as things stand (for a
    /// galaxy just loaded: its realms already reshaped this tick).
    pub(crate) fn refresh(&mut self) {
        self.keep_books();
        self.find_borders();
        self.count_war();
    }

    /// Which systems have an enemy's within reach.
    fn find_borders(&mut self) {
        let owner: Vec<usize> = self.systems.iter().map(|s| s.owner()).collect();
        for s in 0..self.systems.len() {
            let border = self.neighbors(s).any(|(t, _)| owner[s] != owner[t]);
            self.border[s] = border;
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
        self.make_cubes();
        let settled = self.battles(played);
        self.mercs();
        self.reckon_holds();
        self.update_share();
        self.update_supply();
        self.send();
        // As things stand now: what realms pick to fight over, as shown, goes by it.
        self.count_war();
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
        self.land_cubes();
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
        let offset = self.rng.index(self.systems.len());
        // What a hull's worth where: it keeps, so quiet ticks say little.
        for (worth, seen) in self.hull_worth.iter_mut().zip(&mut self.hull_use) {
            let sense = if *seen > 0.0 {
                self.rules.sense
            } else {
                self.rules.sense / 100.0
            };
            *worth += sense * (*seen - *worth);
            *seen = 0.0;
        }
        for i in 0..self.systems.len() {
            // Whoever's first gets the crystal, so a different one each tick.
            let s = (i + offset) % self.systems.len();
            let (a, b) = self.systems[s].sites;
            let sys = &mut self.systems[s];
            for site in &self.sites[a as usize..b as usize] {
                let f = site.owner as usize;
                let r = sys.realms[f];
                if r == NONE {
                    continue;
                }
                match site.kind {
                    // A mine pays only its planet's vault's owner.
                    SiteKind::Mine { crystals: c } => {
                        let v = self.vault_at[s] as usize + site.planet as usize;
                        if self.vaults[v].side as usize == f {
                            crystals[r as usize] += c;
                            mined[v] += c * (1.0 - self.rules.research);
                        }
                    }
                    // A foundry always works, and hulls cost nothing.
                    SiteKind::Foundry { hulls } => sys.building[f] += hulls,
                    SiteKind::Shield { .. } | SiteKind::Outpost => {}
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
            // What they saved, as a merc's: their pay over a career.
            let pay = self.realms[r].pay(&self.rules);
            self.savings += retired as f32 * pay / self.rules.career.max(1e-6);
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

    /// System `s`'s planets' riches.
    pub fn riches_of(&self, s: usize) -> &[Riches] {
        let a = self.vault_at[s] as usize;
        &self.riches[a..a + self.systems[s].planets as usize]
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
                self.books.trade[1] += loot as f64;
                if let Some(k) = &mut c.contract {
                    k.loot += loot;
                }
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
        let mut out = [0.0; SIDES];
        for f in 0..SIDES {
            out[f] = sys.hulls[f] as f32;
        }
        out
    }

    /// How many battles each side's in, in system `s` this tick: one for each
    /// site it attacks, and each of its own that's attacked.
    pub(crate) fn battles_in(&self, s: usize) -> [u32; SIDES] {
        let mut n = [0; SIDES];
        let mut attacked: Vec<usize> = Vec::new();
        for f in (0..SIDES).filter(|&f| self.fighting(s, f)) {
            for t in self.targets(s, f) {
                n[f] += 1;
                if !attacked.contains(&t) {
                    attacked.push(t);
                }
            }
        }
        for t in attacked {
            n[self.sites[t].owner as usize] += 1;
        }
        n
    }

    /// Each side's fire in the battle for site `t` in system `s`: its
    /// fire with no shield in its way, a seat short for a side attacking
    /// under an enemy's shield up on the site's planet (#426).
    fn fire(&self, s: usize, t: usize) -> [f32; SIDES] {
        // A pilot fights one battle a tick: each side's are split between
        // all it's in.
        let in_battles = match self.engaged {
            Some((at, n)) if at == s => n,
            _ => self.battles_in(s),
        };
        let planet = self.sites[t].planet as usize;
        let owner = self.sites[t].owner as usize;
        std::array::from_fn(|f| {
            // Under an enemy's shield, a seat's closed: pilots with no seat
            // don't fire.
            let shielded = f != owner && self.shields_up(s, f)[planet] > 0;
            let seats = self.seats_in(shielded);
            let pilots = self.able(s, f) / in_battles[f].max(1) as f32;
            pilots.min(seats) * crate::economy::damage_factor(self.gear(s, f))
        })
    }

    /// Each side's fire in system `s` with no shield in its way: its
    /// strength, scaled to the hulls that fire (`firing`).
    #[cfg(test)]
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
    /// pilots for, and no more than its battles have seats.
    pub fn firing(&self, s: usize, f: usize) -> f32 {
        let battles = match self.engaged {
            Some((at, n)) if at == s => n[f],
            _ => self.battles_in(s)[f],
        };
        self.able(s, f)
            .min(self.rules.seats * battles.max(1) as f32)
    }

    /// Side `f`'s pilots with hulls in system `s`, its realm's and its mercs':
    /// they share its seats, all alike.
    pub fn able(&self, s: usize, f: usize) -> f32 {
        let (mercs, crewed) = self.crewed(s, f);
        let own = self.systems[s].hulls[f] - mercs;
        let realm = match self.systems[s].realms[f] {
            NONE => own as f32,
            r => self.pilots_for(r, own as f32).min(own as f32),
        };
        realm + crewed as f32
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
        self.count_war();
        for realm in &mut self.realms {
            realm.fronts = 0;
            realm.seats_now = 0.0;
        }
        for s in 0..self.systems.len() {
            for f in 0..SIDES {
                let r = self.systems[s].realms[f];
                if r != NONE && self.fighting(s, f) {
                    let (mercs, crewed) = self.crewed(s, f);
                    let own = self.systems[s].hulls[f] - mercs;
                    // Its own seats there, past its mercs'.
                    let seats = (self.rules.seats as u32 * self.battles_in(s)[f].max(1))
                        .saturating_sub(crewed)
                        .min(own);
                    let realm = &mut self.realms[r as usize];
                    realm.fronts += 1;
                    realm.seats_now += seats as f32;
                }
            }
        }
    }

    /// Count each realm's hulls at war (what its pilots are split by).
    fn count_war(&mut self) {
        let mut war = vec![0; self.realms.len()];
        for s in 0..self.systems.len() {
            for f in 0..SIDES {
                let r = self.systems[s].realms[f];
                if r != NONE && self.fighting(s, f) {
                    war[r as usize] += self.systems[s].hulls[f] - self.crewed(s, f).0;
                }
            }
        }
        self.war_hulls = war;
    }

    /// A side's seats in a battle: one fewer under an enemy's shield.
    pub(crate) fn seats_in(&self, shielded: bool) -> f32 {
        let closed = if shielded { SHIELD_SEATS as f32 } else { 0.0 };
        (self.rules.seats - closed).max(0.0)
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
        let own = (self.able(s, f) - crewed).max(0.0);
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
            self.fought_at[s] = self.tick;
            self.engaged = Some((s, self.battles_in(s)));
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
        self.engaged = None;
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
    /// attackers, then its holder), each with every hull it has left here,
    /// as it's played: each side's pilots (`fire`, a little luck either way)
    /// wreck what they can, spread over its enemies by their hulls (better
    /// armor takes more stopping, #438), till a side's out of hulls (`clash`);
    /// each loss is some enemy pilot's kill (`credit`), and some of them its
    /// mercs' (`mercs_share`). The site is the side's with the most kills, of
    /// those with hulls left; its holder's, if that's even.
    fn sim(&mut self, s: usize, t: usize, sides: &[usize]) -> battle::Outcome {
        let hulls = self.systems[s].hulls;
        let base = self.fire(s, t);
        let mut fire = [0.0; SIDES];
        for &f in sides {
            fire[f] = base[f] * self.rng.range(1.0 - LUCK, 1.0 + LUCK);
        }
        let by = self.clash(s, &fire, sides);
        let mut o = battle::Outcome {
            site: Some(t),
            lost: Default::default(),
            winner: None,
            scrap: Vec::new(),
        };
        let mut kills = [0u32; SIDES];
        let mut left = hulls;
        for &f in sides {
            let x: f32 = by[f].iter().sum();
            if hulls[f] == 0 || x <= 0.0 {
                continue;
            }
            let lost = self.rng.binomial(hulls[f], (x / hulls[f] as f32).min(1.0));
            left[f] -= lost;
            // Whose kills they are.
            let mut theirs = [0u32; SIDES];
            let (mut n, mut rest) = (lost, x);
            for g in 0..SIDES {
                if by[f][g] > 0.0 {
                    let k = self.rng.binomial(n, (by[f][g] / rest.max(1e-6)).min(1.0));
                    theirs[g] = k;
                    kills[g] += k;
                    n -= k;
                    rest -= by[f][g];
                }
            }
            let scrap = self.credit(s, f, &theirs);
            o.scrap.extend(scrap);
            let mercs = self.mercs_share(s, f, lost);
            let theirs: u32 = mercs.iter().map(|m| m.1).sum();
            o.lost[f] = (lost - theirs, mercs);
        }
        let (&holder, attackers) = sides.split_last().expect("a battle has a holder");
        let mut best = (left[holder] > 0).then_some((holder, kills[holder]));
        for &f in attackers.iter().filter(|&&f| left[f] > 0) {
            if best.is_none_or(|(_, k)| kills[f] > k) {
                best = Some((f, kills[f]));
            }
        }
        o.winner = Some(best.map_or(holder, |b| b.0));
        o
    }

    /// The hulls each of `sides` loses in a battle in system `s`, at each
    /// side's `fire`, to each of the others (`[lost][killer]`): each side's
    /// fire spreads over its enemies by their hulls, less their armor, and a
    /// side out of hulls fires no more.
    fn clash(&self, s: usize, fire: &[f32; SIDES], sides: &[usize]) -> [[f32; SIDES]; SIDES] {
        let hulls = self.systems[s].hulls;
        let armor: [f32; SIDES] = std::array::from_fn(|f| {
            if sides.contains(&f) {
                crate::economy::armor_factor(self.gear(s, f))
            } else {
                1.0
            }
        });
        let mut by = [[0.0f32; SIDES]; SIDES];
        for &g in sides {
            let enemies = || sides.iter().copied().filter(move |&f| f != g);
            let targets: f32 = enemies().map(|f| hulls[f] as f32).sum();
            if targets <= 0.0 {
                continue;
            }
            for f in enemies() {
                by[f][g] = self.rules.kill * fire[g] * hulls[f] as f32 / targets / armor[f];
            }
        }
        // How much of the battle each side's hulls last it.
        let last: [f32; SIDES] = std::array::from_fn(|f| {
            let x: f32 = by[f].iter().sum();
            if x > hulls[f] as f32 {
                hulls[f] as f32 / x
            } else {
                1.0
            }
        });
        for f in 0..SIDES {
            for g in 0..SIDES {
                by[f][g] *= last[g];
            }
            let x: f32 = by[f].iter().sum();
            if x > hulls[f] as f32 {
                by[f].iter_mut().for_each(|v| *v *= hulls[f] as f32 / x);
            }
        }
        by
    }

    /// What the galaxy's own battle math (`sim`) expects of the battle for
    /// site `t` in system `s` between `sides` (its attackers, then its
    /// holder), with no luck either way (#494): each side's hulls, gear,
    /// fire, hulls lost on average, and chance to win. For setting played
    /// battles beside, to see how far off it is.
    pub fn predict(&self, s: usize, t: usize, sides: &[usize]) -> Vec<ledger::Predicted> {
        let hulls = self.systems[s].hulls;
        let fire = self.fire(s, t);
        let by = self.clash(s, &fire, sides);
        let lost = |f: usize| by[f].iter().sum::<f32>();
        let kills = |g: usize| (0..SIDES).map(|f| by[f][g]).sum::<f32>();
        // Each side's kills, against each other's in turn.
        let mut win = [0.0f32; SIDES];
        let mut sum = 0.0;
        for &f in sides {
            let mut p = if hulls[f] > 0 { 1.0 } else { 0.0 };
            for &g in sides.iter().filter(|&&g| g != f && hulls[g] > 0) {
                p *= odds(
                    (kills(f), hulls[f] as f32 - lost(f)),
                    (kills(g), hulls[g] as f32 - lost(g)),
                );
            }
            win[f] = p;
            sum += p;
        }
        if sum > 0.0 {
            win.iter_mut().for_each(|p| *p /= sum);
        } else if let Some(&h) = sides.last() {
            win[h] = 1.0;
        }
        sides
            .iter()
            .map(|&f| ledger::Predicted {
                faction: Faction::side(f).name().to_string(),
                hulls: hulls[f],
                gear: self.gear(s, f),
                fire: fire[f],
                lost: lost(f).min(hulls[f] as f32),
                win: win[f],
            })
            .collect()
    }

    /// Side `f` lost hulls in system `s` (#438), to each side as many as
    /// `theirs`: each is a kill, by one of that side's hulls: a company's (one of its pilots
    /// takes apart the wreck) with the chance its hulls make up of the
    /// side's here, else the realm's (whose pilots fly its researched tier,
    /// and learn nothing from wrecks). Returns the wrecks to take apart.
    fn credit(&mut self, s: usize, f: usize, theirs: &[u32; SIDES]) -> Vec<battle::Scrap> {
        let mut out = Vec::new();
        if theirs.iter().all(|&n| n == 0) {
            return out;
        }
        let level = self.gear(s, f);
        let maker = Faction::side(f);
        let made = |hull: bool| -> Vec<crate::economy::Part> {
            crate::economy::PARTS
                .into_iter()
                .filter(|p| p.maker() == maker)
                .filter(|p| matches!(p, crate::economy::Part::Hull(_)) == hull)
                .collect()
        };
        let (bodies, guns) = (made(true), made(false));
        if bodies.is_empty() || guns.is_empty() {
            return out;
        }
        // The companies here, each side's: (index, side, its pilots flying),
        // and each side's, all told.
        let mut here: Vec<(usize, usize, f32)> = Vec::new();
        let mut flying = [0.0f32; SIDES];
        for (c, co) in self.companies.iter().enumerate() {
            if let Some(k) = co.contract
                && k.target as usize == s
                && k.field > 0
                && !co.pilots.is_empty()
            {
                let n = (k.field as usize).min(co.pilots.len()) as f32;
                here.push((c, k.side as usize, n));
            }
        }
        if here.is_empty() {
            return out;
        }
        for g in 0..SIDES {
            // (all those in reach of a seat, sharing the seats)
            flying[g] = self.able(s, g);
        }
        let killers = (0..SIDES).flat_map(|g| std::iter::repeat_n(g, theirs[g] as usize));
        for g in killers {
            if !here.iter().any(|h| h.1 == g) {
                continue;
            }
            let mut y = self.rng.f32() * flying[g].max(1.0);
            let Some(&(c, ..)) = here.iter().filter(|h| h.1 == g).find(|h| {
                y -= h.2;
                y < 0.0
            }) else {
                continue;
            };
            let p = self.rng.index(self.companies[c].pilots.len());
            let body = bodies[self.rng.index(bodies.len())];
            let gun = guns[self.rng.index(guns.len())];
            let co = &self.companies[c];
            out.push((co.id, co.pilots[p].id, body, level));
            out.push((co.id, co.pilots[p].id, gun, level));
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
        self.fought_at[s] = self.tick;
    }

    /// How long worlds last: the share of those lately taken, and of the
    /// others lately fought over, that changed hands this tick. (What
    /// nobody's fought over says nothing of what's being fought for.)
    fn reckon_holds(&mut self) {
        let lately = |at: u64| at > 0 && self.tick < at + 300;
        let (mut new, mut old) = (0u32, 0u32);
        for s in 0..self.systems.len() {
            if lately(self.taken_at[s]) {
                new += 1;
            } else if lately(self.fought_at[s]) {
                old += 1;
            }
        }
        // With few to go on, they say little.
        if new > 20 {
            self.lose_new += 0.02 * (self.fell_new as f32 / new as f32 - self.lose_new);
        }
        if old > 20 {
            self.lose_old += 0.02 * (self.fell_old as f32 / old as f32 - self.lose_old);
        }
        (self.fell_new, self.fell_old) = (0, 0);
    }

    /// Ticks having a world that's fought over makes a difference: till it'd
    /// be lost, or if it were lost, be had again. (Won, or kept: those lately
    /// taken fall at one rate, those kept at the other, and a world not won
    /// now may be later.)
    pub(crate) fn hold_for(&self) -> f32 {
        1.0 / (self.lose_new + self.lose_old).max(0.001)
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
            self.campaigns(f);
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
        // A smart realm picks its own attacks (`campaigns`).
        if here == 0 && !ours && self.realms[r as usize].arm == realm::Arm::Smart {
            return;
        }
        let worth = self.prize(r, t, f, ours);
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
        let scene = self.scene(t, f, r);
        // (what it'd hold, what it'd cost it) with `more` of its own.
        let play = |g: &Galaxy, more: f32, pilots: f32| {
            let fc = g.forecast(&scene, &plan(more, pilots));
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
            for n in [6.0, 18.0, 36.0] {
                pilot = pilot.max((play(self, more, p + n).0 - fly - now) / n / length);
                hull = hull.max((play(self, all + 100.0 * n, reserve).0 - most) / (100.0 * n));
            }
            // And the seats it'd fill.
            let seats = (self.seats_at(t, f) as f32).min(have + late + more);
            let x = &mut self.realms[r as usize];
            if here == 0 {
                x.seats_next = x.seats_next.max(seats);
            }
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
            let shares: &[f32] = if realm.arm == realm::Arm::Smart {
                &smart::SENDS
            } else {
                &[0.1, 0.25, 0.5, 1.0]
            };
            for &share in shares {
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
        // Or with a company, going in with all it can pay to move (nothing,
        // if that's too little to send).
        let sends = if best.1 > 0.0 {
            best.1
        } else if all >= MIN_SEND as f32 {
            afford
        } else {
            0.0
        };
        let with = pool as f32 * sends;
        // What it's to beat: what it'd do alone, its flight not counted as
        // that's paid either way; or staying, its flight then a cost.
        let base = if best.1 > 0.0 {
            stay + best.0 + cost_all * best.1
        } else {
            stay + cost_all * sends
        };
        let help = plan(with, self.pilots_for(r, have + late + with));
        let start = if fresh && with > 0.0 { slowest } else { 0 };
        if (attacked || pool >= MIN_SEND) && self.hire(t, f, r, help, start, base) && best.1 == 0.0
        {
            best.1 = sends;
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

/// The chance a side that expects so many kills, and so many hulls left
/// (`(kills, left)`), wins a battle against another: that its kills come out
/// ahead, each side's scattered by the dice and by luck; or that it's the
/// one not out of hulls.
pub(crate) fn odds(ours: (f32, f32), theirs: (f32, f32)) -> f32 {
    let wiped = |x: f32| x < 0.5;
    match (wiped(ours.1), wiped(theirs.1)) {
        (true, true) => return 0.0,
        (true, false) => return 0.02,
        (false, true) => return 0.98,
        _ => {}
    }
    let (a, b) = (ours.0, theirs.0);
    let sd = (a + b + LUCK * LUCK / 3.0 * (a * a + b * b))
        .sqrt()
        .max(0.5);
    // (near enough the normal curve)
    1.0 / (1.0 + (-1.7 * (a - b) / sd).exp())
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
    let at = |mut v: Vec<f32>| {
        v.sort_by(f32::total_cmp);
        [0.5, 0.9, 0.99, 1.0].map(|q| v[((v.len() - 1) as f32 * q) as usize])
    };
    let ground = || (0..g.systems.len()).flat_map(|s| g.riches_of(s));
    println!(
        "at 50/90/99/100%: starlight {:.1?}; a planet's crystal {:.1?}, hulls {:.1?}, tungsten {:.1?}",
        at(g.systems.iter().map(|s| s.starlight).collect()),
        at(ground().map(|x| x.crystal).collect()),
        at(ground().map(|x| x.hulls).collect()),
        at(ground().map(|x| x.tungsten).collect()),
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
                "tick {:6}  {}  at war {:4.1}%  sent {:4} shields {:4} taken {:4}  hulls {:8.0}  splits {} merges {}  vaults {:.0}  companies {} on contract {} worth {:.0}  tiers {tiers}",
                g.tick,
                shares.join("  "),
                c.at_war * 100.0,
                g.sent,
                g.shields_down,
                g.taken,
                hulls,
                g.splits,
                g.merges,
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
    println!(
        "tungsten cubes: {}, {} in the biggest stash",
        g.stashes.iter().map(|x| x.cubes as u64).sum::<u64>(),
        g.stashes.iter().map(|x| x.cubes).max().unwrap_or(0),
    );
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
