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
//! next one and delivers it done; each stored hull has a small chance a tick
//! of wearing out; a battle's losses are whole hulls, rolled (each of a
//! side's hulls there with the chance its expected losses give it), the
//! mercs' among them by their share. A side with a hull in a system is in
//! the fight there: a battle goes on to the last hull, mercs' included.
//!
//! **Realms, not factions, are the actors** (#315; see `realm`). A faction
//! is a people; what fights is a realm, a ruler holding a handful of systems
//! from a seat. Every side's sites and hulls in a system are one realm's
//! (`System::realms`); each realm banks its own crystal (crystal is light, so
//! moving it within the realm costs nothing), launches its own offensives,
//! and keeps what it takes. Realms split at their edges and merge, by
//! distance. Nobody watches the whole galaxy.
//!
//! **A tick:**
//! 1. *Production.* Every production site adds hulls and crystals to its realm.
//!    Realms pay their pilots and train more.
//!    Hulls cost nothing to build, except a little crystal for the Resonant.
//!    Stored hulls wear out slowly, so an idle world's garrison levels off.
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
//! 3. *Supply.* A people's systems that hang together (within FTL range of
//!    each other) fewer than a few dozen are a **pocket**: once an enemy
//!    borders it, every enemy around goes for it. Enclaves don't last. Every few ticks realms
//!    split and merge.
//! 4. *Offensives.* A realm banks crystals until it can afford a push, aims
//!    it at the stretch of its border where it's strongest against the
//!    defense, brings hulls up toward it, and fights there with the
//!    initiative (a fire bonus). The push rolls on from what it takes, until
//!    the crystals run low or it stalls. Then quiet, while it banks for the next.
//! 5. *Sending hulls.* Each realm looks at every system it could reach
//!    (within FTL range of its systems with hulls to spare) and sends hulls
//!    where it sees a high chance of capture: to defend anywhere, but to
//!    attack only within its offensive, or a pocket. Several systems can send
//!    to one. FTL costs crystals per hull by distance. An attack
//!    the realm is set on but can't make alone, or a world of its own it
//!    can't hold alone, can hire a **mercenary company** to make up the
//!    difference (see `merc`, #318, #340): paid in crystal, up front and a
//!    bonus if it comes off; the company buys the hulls it lacks from the
//!    cheapest realm nearby.
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

/// The four warring sides (the Company holds no ground).
pub const SIDES: usize = 4;

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
    /// Each hull has this chance a tick of wearing out (so a garrison levels off at
    /// production / wear).
    pub wear: f32,
    /// Crystals per hull for an FTL jump of the full range (shorter ones cost less).
    pub ftl_cost: f32,
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
    /// An attacker needs this many times the defense (its fire) to go in.
    pub odds: f32,
    /// ... and only this much when going for a pocket,
    pub gang_odds: f32,
    /// ... and this much anywhere off its offensive.
    pub opportunity: f32,
    /// A people's systems hanging together (by FTL range) fewer than this
    /// are a pocket, cut off from the rest: once an enemy borders it, the
    /// enemy goes for it.
    pub pocket: u32,
    /// A realm launches an offensive once it has banked this many ticks of
    /// crystal income...
    pub launch: f32,
    /// ... and it runs until the bank is down to this many ticks of income,
    pub spent: f32,
    /// ... or until this many ticks pass without taking a site.
    pub stall: u32,
    /// Fire bonus for a side fighting where its offensive is: it picked the
    /// time and place, and massed for it.
    pub initiative: f32,
    /// Most a side alone in a system walks into of the enemy's unguarded
    /// sites a tick.
    pub blitz: u32,
    /// How far from where it's aimed an offensive attacks, light years.
    pub front: f32,
    /// Share of a quiet system's spare hulls brought up a tick...
    pub bring_up: f32,
    /// ... spending at most this many ticks of crystal income a tick on it.
    pub tempo: f32,
    /// A realm keeps at least this fraction of a quiet system's hulls home.
    pub keep: f32,
    /// An attack takes at least this share of what could reach the target.
    pub commit: f32,
    /// Ticks of crystal income a realm keeps back from attacks, to pay for
    /// defending what it holds.
    pub reserve: f32,
    /// Most crystal a realm can bank, in ticks of its income: the rest is
    /// lost to the war's waste (no realm sits on a war chest for long).
    pub hoard: f32,
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
    /// What a realm asks for a spare hull, crystals, where its garrison is
    /// at the level it settles at (more below it, less above).
    pub hull_price: f32,
    /// Crystal to move a company's ship the full FTL range (its hulls pay as any do).
    pub ship_cost: f32,
    /// A company's margin over its costs at the start, and the least it goes down to,
    pub merc_margin: f32,
    pub merc_floor: f32,
    /// ... falling by this fraction each tick it waits for work,
    pub merc_haggle: f32,
    /// ... and rising by this fraction each contract it wins.
    pub merc_raise: f32,
    /// A contract runs out after this many ticks.
    pub merc_term: u32,
    /// How far a company goes for work, light years.
    pub merc_range: f32,
    /// A company idle this many ticks sails to the nearest war to look for
    /// work (`Galaxy::seek_work`).
    pub merc_idle: u32,
    /// A realm pays for a world at most what it turns out over this many
    /// ticks (crystal, and hulls at `hull_price`).
    pub merc_horizon: f32,
    /// An idle company worth more than this splits in two; one worth less
    /// than this joins another (crystals, its hulls at `hull_price`).
    pub merc_split: f32,
    pub merc_join: f32,
    /// A realm that owes a company a bonus pays back this share of its
    /// income each tick (and no company works for it until it has).
    pub merc_dues: f32,
    /// A realm puts this share of its crystal income into researching its
    /// next gear tier (#438)...
    pub research: f32,
    /// ... and a step costs this many crystals a system it holds, growing
    /// a step at a time as upgrades do (`economy::upgrade_cost`).
    pub research_price: f32,
    /// What a realm pays each pilot a tick when it's founded: it moves
    /// from there, up while it loses pilots and down while it doesn't.
    pub pilot_upkeep: f32,
    /// Share of its income a realm spends on pilots.
    pub pilot_budget: f32,
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
    /// A realm losing pilots raises its pay by this share a tick; one
    /// that isn't lowers it by this.
    pub pay_raise: f32,
    pub pay_ease: f32,
    /// A pilot retires once they've saved this many crystals.
    pub retire: f32,
}

impl Default for Rules {
    fn default() -> Rules {
        Rules {
            wear: 0.03,
            ftl_cost: 0.225,
            kill: 0.35,
            frontage: 85.0,
            seats: 6.0,
            home_ground: 0.3,
            take: 0.3,
            odds: 1.5,
            gang_odds: 1.15,
            opportunity: 3.0,
            pocket: 20,
            launch: 60.0,
            spent: 2.0,
            stall: 30,
            initiative: 1.0,
            blitz: 2,
            front: 500.0,
            bring_up: 0.5,
            tempo: 0.5,
            keep: 0.3,
            commit: 0.6,
            reserve: 0.0,
            hoard: 80.0,
            reach: 500.0,
            fold: 8,
            found: 300.0,
            politics: 10,
            companies: 60,
            merc_capital: 150.0,
            hull_price: 0.03,
            ship_cost: 3.0,
            merc_margin: 0.3,
            merc_floor: 0.05,
            merc_haggle: 0.01,
            merc_raise: 0.1,
            merc_term: 30,
            merc_range: 1200.0,
            merc_idle: 30,
            merc_horizon: 300.0,
            merc_split: 1000.0,
            merc_join: 40.0,
            merc_dues: 0.5,
            research: 0.15,
            research_price: 1.2,
            pilot_upkeep: 0.01,
            pilot_budget: 0.25,
            train: 2,
            crew: 40,
            wage_cr: 50.0,
            kill_share: 0.3,
            pay_raise: 0.05,
            pay_ease: 0.002,
            defect: 0.01,
            retire: 5.0,
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
    /// Stored hulls worn out.
    pub worn: u32,
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
            - (self.worn + self.lost + self.left + self.released) as i64
    }

    /// These without `o`'s (none below zero).
    pub fn less(&self, o: &Moves) -> Moves {
        Moves {
            built: self.built.saturating_sub(o.built),
            worn: self.worn.saturating_sub(o.worn),
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

/// The whole war.
#[cfg_attr(test, derive(Clone))]
pub struct Galaxy {
    /// Tests: a hire (realm, system) forbidden, or forced through.
    #[cfg(test)]
    pub(crate) ban: Option<(u32, u32)>,
    #[cfg(test)]
    pub(crate) force: Option<(u32, u32)>,
    /// Tests: hires refused as lost even with mercs (realm, system, side,
    /// defend, worth).
    #[cfg(test)]
    pub(crate) guesses: Vec<(u32, u32, f32, f32)>,
    #[cfg(test)]
    pub(crate) asks: Vec<(u32, u32, usize, bool)>,
    /// Tests: force the smallest package through, not the largest.
    #[cfg(test)]
    pub(crate) small: bool,
    #[cfg(test)]
    pub(crate) ask_log: bool,
    #[cfg(test)]
    pub(crate) lost_calls: Vec<(u32, u32, usize, bool, f32)>,
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
    /// The best open bid this tick for a pilot at each mark (0 for none).
    pub offers: [f32; merc::MARKS],
    /// What realms pay their pilots a tick, on average, as of the last
    /// tick: what a new pilot starts on, and the rate CR trade at.
    pub pay: f32,
    /// How far realms trust their forecasts of a contract, as contracts
    /// have lately come out: the worth they got of what they expected, and
    /// the ticks they paid for of those they expected.
    pub trust: [f32; 2],
    pub trust_ticks: f32,
    /// The share of merc pilots in work lately, as pilots see it; and
    /// this tick's, so far.
    pub employment: f32,
    pub(crate) employed: u32,
    /// Ended contracts waiting for their realms to look back on, once
    /// their term is up (not saved: a few lessons lost).
    pub(crate) reviews: Vec<merc::Contract>,
    /// Scratch: realms' pay, lowest first, and the pilots on no more than
    /// it, as of `pays_tick`.
    pub(crate) pays: Vec<(f32, f32)>,
    /// What realms have seen of fights alike (`Galaxy::alike`): how many,
    /// and the shares of the worlds held, all told. And those not yet
    /// looked back on.
    pub(crate) memory: Vec<(f32, f32)>,
    pub(crate) lessons: Vec<merc::Lesson>,
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
    /// Each system's hulls in and out, each side's (#445), since the last
    /// played battle began (`step_played`): that battle's losses and the
    /// rest of its tick, then any ticks waited through after. Not saved.
    pub moves: Vec<[Moves; SIDES]>,
    /// Of `moves`, the companies' hulls on contract for the side: hired,
    /// released, lost and worn out. Not saved.
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

/// A realm pushing into one stretch of the front: it attacks only there
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
            #[cfg(test)]
            ban: None,
            #[cfg(test)]
            force: None,
            #[cfg(test)]
            guesses: Vec::new(),
            #[cfg(test)]
            asks: Vec::new(),
            #[cfg(test)]
            small: false,
            #[cfg(test)]
            ask_log: false,
            #[cfg(test)]
            lost_calls: Vec::new(),
            next_pilot: 1,
            you: Pilot::rookie(1),
            owed_you: Vec::new(),
            pool: Vec::new(),
            wage_curve: [rules.pilot_upkeep; merc::MARKS],
            offers: [0.0; merc::MARKS],
            pay: rules.pilot_upkeep,
            trust: [1.0; 2],
            trust_ticks: 1.0,
            employment: 0.5,
            employed: 0,
            reviews: Vec::new(),
            pays: Vec::new(),
            memory: vec![(0.0, 0.0); merc::MEMORY],
            lessons: Vec::new(),
            pays_tick: 0,
            crew: None,
            own: None,
            ended: Vec::new(),
            crew_news: None,
            crew_choice: None,
            destroyed: 0.0,
            rng,
            members: Vec::new(),
            spans: Vec::new(),
            free: Vec::new(),
        };
        // Start every garrison near where it would level off.
        for s in 0..g.systems.len() {
            let owner = g.systems[s].owner();
            g.systems[s].hulls[owner] = (g.production(s)[owner] / rules.wear * 0.8).round() as u32;
        }
        g.form_realms();
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
                .map(|c| c.capital(self.rules.hull_price))
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
            self.cut[s] = small && self.neighbors(s).any(|(t, _)| owner[s] != owner[t]);
        }
    }

    /// Each realm starts or ends its offensive.
    fn plan(&mut self) {
        for r in 0..self.realms.len() {
            let realm = self.realms[r];
            if realm.systems == 0 {
                continue;
            }
            let bank = realm.bank();
            if let Some(o) = realm.offensive {
                if bank < self.rules.spent || self.tick > o.gained + self.rules.stall as u64 {
                    self.realms[r].offensive = None;
                }
            } else if bank >= self.rules.launch {
                self.realms[r].offensive = self.aim(r);
            }
        }
    }

    /// Where realm `r` should push: of a sample of the enemy systems on its
    /// border, the one where its hulls nearby most outweigh the defense.
    fn aim(&mut self, r: usize) -> Option<Offensive> {
        let f = self.realms[r].faction as usize;
        let (a, b) = self.spans[r];
        if a == b {
            return None;
        }
        let mut best: Option<(f32, usize)> = None;
        for _ in 0..40 {
            let s = self.members[a as usize + self.rng.index((b - a) as usize)] as usize;
            let k = self.neighbors(s).count();
            if k == 0 {
                continue;
            }
            let t = self.neighbors[s * NEIGHBORS + self.rng.index(k)].0 as usize;
            if self.systems[t].owner() == f {
                continue;
            }
            let ours: f32 = self
                .neighbors(t)
                .filter(|&(s, _)| self.holder(s) == r as u32)
                .map(|(s, _)| self.systems[s].hulls[f] as f32)
                .sum();
            if ours <= 0.0 {
                continue;
            }
            let defense: f32 = self
                .strength(t)
                .iter()
                .enumerate()
                .filter(|&(g, _)| g != f)
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

    /// Run the whole galaxy forward one tick.
    /// `played`: a battle this tick that was played (see `step_played`).
    fn advance_tick(&mut self, played: Option<&battle::Played>) -> Option<battle::Settled> {
        self.tick += 1;
        self.produce();
        let settled = self.battles(played);
        self.mercs();
        self.update_share();
        self.update_supply();
        self.plan();
        self.send();
        settled
    }

    fn produce(&mut self) {
        let mut crystals = vec![0.0; self.realms.len()];
        let resonant = Faction::Resonant.index();
        for s in 0..self.systems.len() {
            let (a, b) = self.systems[s].sites;
            // Stored hulls wear out, each with a small chance a tick.
            for f in 0..SIDES {
                let worn = self.rng.binomial(self.systems[s].hulls[f], self.rules.wear);
                self.systems[s].hulls[f] -= worn;
                self.moves[s][f].worn += worn;
            }
            let sys = &mut self.systems[s];
            for site in &self.sites[a as usize..b as usize] {
                if let SiteKind::Production { hulls, crystals: c } = site.kind {
                    let f = site.owner as usize;
                    sys.building[f] += hulls;
                    let r = sys.realms[f];
                    if r == NONE {
                        continue;
                    }
                    crystals[r as usize] += c;
                    if f == resonant {
                        crystals[r as usize] -= hulls * RESONANT_HULL_COST;
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
        for (realm, c) in self.realms.iter_mut().zip(crystals) {
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
            let cap = self.rules.hoard * c.max(0.05);
            realm.crystals = (realm.crystals + c).clamp(0.0, cap);
            realm.income = c + study;
            realm.keep_pilots(&self.rules);
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
            if self.pushing(sys.realms[f], sys.pos) {
                out[f] *= 1.0 + self.rules.initiative;
            }
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
                let mut levels: Vec<f32> = c.pilots.iter().map(|p| p.level()).collect();
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

    /// Realm `r`'s offensive (if `r` is a realm, on one) takes in `pos`.
    fn pushing(&self, r: u32, pos: Vec3) -> bool {
        r != NONE
            && self.realms[r as usize]
                .offensive
                .is_some_and(|o| o.at.distance(pos) <= self.rules.front)
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
                    faction: Faction::ALL[f].name().to_string(),
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
        let maker = Faction::ALL[f];
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
        let sys = &mut self.systems[s];
        sys.held[old] -= 1;
        sys.held[w] += 1;
        self.taken += 1;
        self.claim(s, w);
        // An offensive rolls on from what it takes.
        let (pos, r) = (self.systems[s].pos, self.systems[s].realms[w]);
        if self.pushing(r, pos)
            && let Some(o) = &mut self.realms[r as usize].offensive
        {
            o.gained = self.tick;
            o.at = pos;
        }
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

    /// Every realm decides where to send its hulls.
    fn send(&mut self) {
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
        // A different people moves first each tick, starting at a different
        // system, so nobody always gets first pick.
        let mut order = [0, 1, 2, 3];
        for i in (1..SIDES).rev() {
            order.swap(i, self.rng.index(i + 1));
        }
        self.stage();
        let offset = self.rng.index(n);
        for f in order {
            for i in 0..n {
                let t = (i + offset) % n;
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
                for &r in &from[..k] {
                    if here == NONE || here == r {
                        self.consider(t, f, r);
                    }
                }
            }
        }
    }

    /// Each realm on an offensive brings hulls up toward it: every quiet
    /// system of its own sends some of its spare hulls one jump on, to its
    /// own system nearest the offensive's aim (paying the FTL).
    fn stage(&mut self) {
        let mut moves: Vec<(usize, usize, usize, f32)> = Vec::new();
        for r in 0..self.realms.len() {
            let Some(o) = self.realms[r].offensive else {
                continue;
            };
            let f = self.realms[r].faction as usize;
            let first = moves.len();
            let mut cost = 0.0;
            for &s in self.territory(r) {
                let s = s as usize;
                let here = self.systems[s].pos.distance(o.at);
                if self.spare[s][f] < MIN_SEND {
                    continue;
                }
                let next = self
                    .neighbors(s)
                    .filter(|&(t, _)| self.holder(t) == r as u32)
                    .map(|(t, d)| (t, d, self.systems[t].pos.distance(o.at)))
                    .filter(|&(_, _, there)| there < here)
                    .min_by(|a, b| a.2.total_cmp(&b.2));
                if let Some((t, d, _)) = next {
                    let m = self.spare[s][f] as f32 * self.rules.bring_up;
                    cost += m * d / FTL_RANGE * self.rules.ftl_cost;
                    moves.push((f, s, t, m));
                }
            }
            // Bring up only what this tick's budget pays for, from everywhere alike.
            let realm = &mut self.realms[r];
            let budget = (self.rules.tempo * realm.income)
                .min(realm.crystals)
                .max(0.0);
            let afford = if cost > budget { budget / cost } else { 1.0 };
            for mv in &mut moves[first..] {
                mv.3 *= afford;
            }
            realm.crystals -= cost * afford;
        }
        for (f, s, t, m) in moves {
            // Whole hulls go.
            let m = (m as u32).min(self.spare[s][f]);
            self.spare[s][f] -= m;
            self.systems[s].hulls[f] -= m;
            self.systems[t].hulls[f] += m;
            self.moves[s][f].left += m;
            self.moves[t][f].arrived += m;
        }
    }

    /// Should realm `r` (of side `f`) send hulls to system `t`, and how many
    /// from where?
    fn consider(&mut self, t: usize, f: usize, r: u32) {
        let sys = &self.systems[t];
        let fire = self.strength(t);
        // Who's in the way.
        let (mut defense, mut enemy) = (0.0, false);
        for g in (0..SIDES).filter(|&g| g != f) {
            defense += fire[g];
            enemy |= sys.held[g] > 0 || sys.hulls[g] >= AT_WAR;
        }
        if !enemy {
            return;
        }
        let (have, ours) = (fire[f], sys.held[f] > 0);
        // Defending is always on. An attack goes in where the realm's
        // offensive is, or on an enemy pocket (which nobody leaves standing);
        // anywhere else only at long odds.
        let pocket = self.cut[t] && !ours;
        let odds = if pocket {
            self.rules.gang_odds
        } else if ours || self.pushing(r, sys.pos) {
            self.rules.odds
        } else {
            self.rules.opportunity
        };
        // Already winning there by enough: nothing to add.
        if have >= odds * defense && defense > 0.0 {
            return;
        }
        let want = odds * defense * 1.2 + 10.0 - have;
        // What could reach it: the realm's own spare hulls in range.
        let mine = |g: &Galaxy, s: usize| {
            if g.systems[s].realms[f] == r {
                g.spare[s][f]
            } else {
                0
            }
        };
        let pool: u32 = self.neighbors(t).map(|(s, _)| mine(self, s)).sum();
        let pool_f = pool as f32;
        // With a company hired here, it goes in with it: that's what it
        // hired it for.
        let hired = self.companies.iter().any(|c| {
            c.contract
                .is_some_and(|k| k.target as usize == t && k.side as usize == f)
        });
        if (have + pool_f < odds * defense && !hired) || pool < MIN_SEND {
            // Not a good enough chance alone: an attack it's set on, or a
            // world of its own under attack, can hire a company to make up
            // the difference, then go in with it.
            let attacked = (0..SIDES).any(|g| g != f && sys.hulls[g] >= AT_WAR);
            let worth_it = if ours {
                attacked
            } else {
                pocket || self.pushing(r, sys.pos)
            };
            let need = (want - pool_f).max(0.0).ceil() as u32;
            let own = (sys.hulls[f] + pool) as f32;
            if worth_it && self.hire(t, f, r, need, own, ours) {
                self.consider(t, f, r);
            }
            return;
        }
        // Go in hard: a decisive blow, not a top-up the defender can match
        // next tick. What's left of it rolls on to the next world.
        let send = want.max(pool_f * self.rules.commit).min(pool_f);
        if send < MIN_SEND as f32 {
            return;
        }
        // Pay for it: FTL by distance.
        let frac = send / pool_f;
        let cost: f32 = self
            .neighbors(t)
            .map(|(s, d)| mine(self, s) as f32 * frac * d / FTL_RANGE * self.rules.ftl_cost)
            .sum();
        // Defending a world of ours may spend everything; an attack only
        // what's above the reserve.
        let realm = &self.realms[r as usize];
        let purse = if ours {
            realm.crystals
        } else {
            realm.crystals - self.rules.reserve * realm.income.max(0.0)
        };
        let afford = if cost > 0.0 {
            (purse / cost).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let frac = frac * afford;
        if frac * pool_f < MIN_SEND as f32 {
            return;
        }
        self.realms[r as usize].crystals -= cost * afford;
        self.sent += 1;
        let mut moved = 0;
        for k in 0..NEIGHBORS {
            let (s, _) = self.neighbors[t * NEIGHBORS + k];
            if s == u32::MAX {
                break;
            }
            let s = s as usize;
            // Whole hulls go.
            let m = (mine(self, s) as f32 * frac) as u32;
            self.spare[s][f] -= m;
            self.systems[s].hulls[f] -= m;
            self.moves[s][f].left += m;
            moved += m;
        }
        self.systems[t].hulls[f] += moved;
        self.moves[t][f].arrived += moved;
        self.systems[t].realms[f] = r;
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
            let shares: Vec<String> = (0..SIDES)
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
            let pushing = g.living().filter(|(_, r)| r.offensive.is_some()).count();
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
                "tick {:6}  {}  at war {:4.1}%  sent {:4} shields {:4} taken {:4}  hulls {:8.0}  pushing {pushing:3}  splits {} merges {}  pockets {}  vaults {:.0}  companies {} on contract {} worth {:.0}  tiers {tiers}",
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
