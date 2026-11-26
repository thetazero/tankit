//! Mercenary companies: capital, hired by realms (#318).
//!
//! A **company** is a single ship over some system, with crystal and hulls
//! aboard. It isn't a side and holds no ground: it fights inside a realm's
//! attack, as that realm's people, for pay.
//!
//! **Hiring.** A realm fighting for a world, or thinking of it, gets quotes
//! from the nearest few companies within `Rules::merc_range`, and signs
//! whichever the battle maths says does it the most good for its price, if
//! any does. A company brings hulls of its own.
//!
//! **A quote** is what the company pays out (hulls it buys, flying there,
//! training pilots it's short of) up front, and its wages and losses a tick,
//! both marked up by its margin. What the realm can't pay at once it owes,
//! with no interest, as far as its income would clear it in the ticks the
//! company is willing to wait.
//!
//! **Pilots** sign with whoever bids most, no less than makes up for the time
//! mercs are out of work. Realm pilots defect for that, unless matching it
//! costs their realm less than training others. Pilots out of work wait in
//! the pool, then quit; they all retire in time.
//!
//! **Companies live and die.** Each has its own margin, way of pricing pilots
//! and patience, passed on, a little changed, when it grows rich and splits
//! (`Rules::merc_split`). One with too little left joins another. Retired
//! pilots' savings found new ones while companies are making money. A ship
//! costs something to keep, working or not.
//!
//! **Raids.** An idle company raids a rich, weakly held world if what's in
//! its vaults beats what it'd cost (`Galaxy::raid`). It holds what it takes
//! only till it's done, and then it's its old owner's again.
//!
//! **Looking for work.** An idle company sails to where realms have looked
//! for one and found none, if a contract would likely pay the fare.

use super::theater::{ENDED_KEPT, Ended, Outcome};
use super::{AT_WAR, FTL_RANGE, Galaxy, MIN_SEND, SIDES, SiteKind};
use crate::economy::{PARTS, Part};
use crate::sim::{HullKind, TurretKind};

/// A pilot: an id, and what they've reverse-engineered of each part.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pilot {
    /// Never reused.
    pub id: u32,
    pub known: crate::economy::Known,
    /// Their gear's level as of when it last changed (`Pilot::learned`): so
    /// the market needn't work it out each time.
    pub rank: f32,
    /// Crystals earned so far.
    pub wallet: f32,
    /// Bonuses their ship couldn't pay yet.
    pub owed: f32,
    /// What their ship pays them a tick.
    pub wage: f32,
    /// Ticks they've been out of work: aboard, or in the pool.
    pub idle: u32,
    /// What they've earned a tick lately (fading), aboard their ship.
    pub earning: f32,
    /// The realm they left (an index into `Galaxy::realms`), if any.
    pub home: Option<u32>,
}

/// Buckets of five ticks `Books::won_after` counts wins in.
pub const WON_AFTER: usize = 7;

/// How fast a company's guesses at its losses and kills follow what
/// happens: this share of the way each tick it works.
const RATE_FADE: f32 = 0.05;

/// Share of the word of unmet work at a system forgotten each tick.
const UNMET_FADE: f32 = 0.1;

/// How far realms' trust in forecasts moves toward each contract's outcome.
const LEARN: f32 = 0.02;

/// Share of a pilot's recent earnings forgotten each tick.
const EARN_FADE: f32 = 0.02;

/// Share of a company's recent earnings and work forgotten each tick.
const FADE: f32 = 0.005;

/// Gear marks the wage curve is kept by (T0 to T3).
pub const MARKS: usize = crate::economy::MAX_MARK as usize + 1;

/// How well gear at `level` fights: its damage and armor together.
pub(super) fn gear_value(level: f32) -> f32 {
    let l = level.round() as u8;
    crate::economy::damage_factor(l) * crate::economy::armor_factor(l)
}

/// How a company prices pilots: a rule, and how far over (or under) it
/// bids. Each company has its own, and passes it on when it splits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pricing {
    pub rule: Rule,
    pub k: f32,
}

/// The ways a company can price a pilot, a tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rule {
    /// What it's lately earned per pilot-tick worked.
    Earnings,
    /// The going rate for pilots of that gear.
    Markup,
    /// Realm pay, scaled by how much better the pilot's gear fights.
    Gear,
    /// What it can spend, spread over the seats it needs for a term.
    Budget,
}

pub const RULES: [Rule; 4] = [Rule::Earnings, Rule::Markup, Rule::Gear, Rule::Budget];

/// Mark of a gear level, for the wage curve.
fn mark(level: f32) -> usize {
    ((level / crate::economy::STEPS_PER_MARK as f32) as usize).min(MARKS - 1)
}

impl Pilot {
    /// A pilot who knows nothing yet.
    pub fn rookie(id: u32) -> Pilot {
        Pilot {
            id,
            known: crate::economy::Known::default(),
            rank: 0.0,
            wallet: 0.0,
            owed: 0.0,
            wage: 0.0,
            idle: 0,
            earning: 0.0,
            home: None,
        }
    }

    /// What they know has changed.
    pub fn learned(&mut self) {
        self.rank = self.level();
    }

    /// The gear they fly: every part built to what they know of it.
    pub fn levels(&self) -> crate::economy::Levels {
        self.known.levels()
    }

    /// The kit they drive (#481): their own, whoever hired them. The hull
    /// and the turret they can build best: the highest whole step known,
    /// then the furthest into the next, then the parts' order.
    /// Knowing nothing yet, that's the Company's starter kit, the one you
    /// start in: Kataphrakt · Kontos, at T0.
    pub fn kit(&self) -> (HullKind, TurretKind) {
        let best = |parts: &mut dyn Iterator<Item = Part>| {
            parts
                .enumerate()
                .max_by(|&(i, a), &(j, b)| {
                    let k = &self.known;
                    k.cap(a)
                        .cmp(&k.cap(b))
                        .then(k.of(a).total_cmp(&k.of(b)))
                        .then(j.cmp(&i))
                })
                .map(|(_, p)| p)
        };
        let hull = match best(&mut PARTS.into_iter().filter(|p| matches!(p, Part::Hull(_)))) {
            Some(Part::Hull(h)) => h,
            _ => HullKind::Kataphrakt,
        };
        let turret = match best(&mut PARTS.into_iter().filter(|p| matches!(p, Part::Turret(_)))) {
            Some(Part::Turret(t)) => t,
            _ => TurretKind::Kontos,
        };
        (hull, turret)
    }

    /// Their gear's level: the kit they drive's (`kit`), hull and turret
    /// averaged. What they know of parts they don't drive doesn't fight.
    pub fn level(&self) -> f32 {
        // (The kit is the best of each, so its levels are the highest.)
        let (mut hull, mut turret) = (0, 0);
        for p in PARTS {
            let cap = self.known.cap(p);
            match p {
                Part::Hull(_) => hull = hull.max(cap),
                Part::Turret(_) => turret = turret.max(cap),
            }
        }
        (hull as f32 + turret as f32) / 2.0
    }
}

/// One free company: a ship.
#[derive(Clone, Debug)]
pub struct Company {
    /// The system its ship is over.
    pub at: u32,
    pub crystals: f32,
    /// Hulls aboard (not those out on a contract: see `Contract::field`).
    pub hulls: u32,
    /// What it asks over its costs, as a fraction of them.
    pub margin: f32,
    pub contract: Option<Contract>,
    /// The tick it was founded (split off, or the war's start).
    pub founded: u64,
    /// The tick it last went idle: founded, back from a contract, or come
    /// to a new system looking for work (`Galaxy::seek_work`).
    pub idle_since: u64,
    /// Its own name, for charts: never reused.
    pub id: u32,
    /// Its crew. Each pilot flies one hull at a time, so the crew caps how
    /// many of its hulls fire.
    pub pilots: Vec<Pilot>,
    /// How it prices pilots.
    pub pricing: Pricing,
    /// Ticks it'll wait to be paid back: it lends a realm what its income
    /// would clear in as many.
    pub patience: f32,
    /// Its contract income lately, and pilot-ticks worked lately (both
    /// fading a little each tick).
    pub earned: f32,
    pub worked: f32,
    /// Of its hulls on a contract, the share lost a tick lately, and kills
    /// a hull a tick: what it guesses a contract will cost it.
    pub loss_rate: f32,
    pub kill_rate: f32,
    /// Scratch: its pilots' kills this tick, and its contract's hulls lost
    /// as of last tick.
    pub kills: u32,
    pub lost_before: u32,
}

/// A company's work for a realm: help take one system.
#[derive(Clone, Copy, Debug, Default)]
pub struct Contract {
    /// Who hired it (an index into `Galaxy::realms`), and as which people it fights.
    pub realm: u32,
    pub side: u8,
    /// The system to take.
    pub target: u32,
    pub signed: u64,
    /// Paid at signing (the hulls it buys, its flight there), and billed
    /// each tick it works, at a rate fixed at signing; billed so far.
    pub upfront: f32,
    pub rate: f32,
    pub billed: f32,
    /// What it quoted to pay a pilot a tick, at T0.
    pub wage: f32,
    /// The hulls it committed to the contract, all told.
    pub committed: u32,
    /// Its hulls in the fight now, and lost so far.
    pub field: u32,
    pub lost: u32,
    /// On the way there: the tick it lands, and the hulls it brings.
    pub arrives: u64,
    pub inbound: u32,
    /// Hired to hold one of the employer's own worlds, not to take one.
    pub defend: bool,
    /// What the realm expected of it when it signed: the worth of the
    /// world it would hold at the end, and the ticks it would pay for.
    pub expected: f32,
    /// And to gain by it, over going without, after paying.
    pub net: f32,
    pub expected_ticks: f32,
    /// The sites of the world it held when it signed, and what the world
    /// was worth to it.
    pub held: u8,
    pub worth: f32,
}

/// A bill a realm didn't pay: it owes `crystals` to company `company` (a `Company::id`).
#[derive(Clone, Copy, Debug)]
pub struct Debt {
    pub realm: u32,
    pub company: u32,
    pub crystals: f32,
    /// What it pays back a tick.
    pub due: f32,
}

/// The mercenary trade, all told (for charts and tuning).
#[derive(Clone, Copy, Debug, Default)]
pub struct Books {
    /// Raids companies set out on for themselves, those that got all there
    /// was, and what they took.
    /// Looks for a raid, by what came of them: nowhere rich in range, no
    /// hulls or crew, can't pay, not worth it; and raids that ended with
    /// hulls spent or out of time. And what raids expected to take, the
    /// share of it they thought they'd get, hulls sent and lost, and ticks.
    pub raid_looks: [u64; 5],
    /// Crew quoted and had; defenders' hulls when it set out, and as it left,
    /// and their pilots forecast and firing (first tick).
    pub raid_crew: [f64; 2],
    pub raid_foes: [f64; 2],
    pub raid_fire: [f64; 2],
    pub raid_spent: u64,
    pub raid_late: u64,
    pub raid_there: f64,
    pub raid_expected: f64,
    pub raid_hulls: f64,
    pub raid_lost: f64,
    pub raid_ticks: f64,
    /// Rookies companies trained.
    pub trained: u64,
    /// Jobs called off on the way.
    pub called_off: u64,
    pub raids: u64,
    pub raids_won: u64,
    pub raided: f64,
    pub signed: u64,
    /// Contracts that ended with the world taken, with the cap on losses
    /// reached, and otherwise (ran out of time, employer gone).
    pub won: u64,
    pub broken: u64,
    pub expired: u64,
    /// Of those signed, to hold a world (not take one); of those, the ones won.
    pub defenses: u64,
    pub defended: u64,
    /// Crystal realms paid companies up front and billed by the tick, and
    /// bills they couldn't pay when due; of those, paid back later, and
    /// written off (the realm gone, or the company).
    pub upfront: f64,
    pub billed: f64,
    pub defaulted: f64,
    pub repaid: f64,
    pub written_off: f64,
    /// Quotes a company would have made but didn't, to a realm in its debt.
    pub refused: u64,
    /// Times a realm looked for a company, and found: none in range or
    /// free; none with the hulls; none it could pay up front; none worth
    /// the price (the rest, it hired).
    pub asked: u64,
    pub none_near: u64,
    pub none_able: u64,
    pub none_affordable: u64,
    pub none_worth: u64,
    /// Of the times none could field enough: the hulls wanted, the most
    /// any company could field, and the hulls for sale, all told.
    pub able_need: f64,
    pub able_best: f64,
    pub able_for_sale: f64,
    /// Every contract's outcome for its realm as expected at signing and as
    /// it came out (the world's worth if won, else nothing), and its ticks.
    pub expected: f64,
    pub realized: f64,
    /// What realms paid for every contract, up front and billed; and
    /// owed for what companies fronted them.
    pub paid_all: f64,
    pub financed: f64,
    /// Quotes turned down: that wouldn't change the outcome, and that
    /// would but cost more than they gain (the gain, the price, of it the
    /// billing and the upfront).
    pub hopeless: u64,
    /// Of those, where the realm wins without help, and loses with it.
    pub won_alone: u64,
    pub lost_anyway: u64,
    pub dear: u64,
    /// What realms expected to gain by hiring, after the price.
    pub surplus: f64,
    pub dear_gain: f64,
    pub dear_price: f64,
    pub dear_wages: f64,
    pub dear_upfront: f64,
    pub dear_wage_part: f64,
    pub dear_loss_part: f64,
    pub dear_hulls: f64,
    pub expected_ticks: f64,
    pub realized_ticks: f64,
    /// The same, attacks and defenses apart: expected, got, expected
    /// ticks, took, how many, and how many spent all their hulls.
    pub by_kind: [[f64; 6]; 2],
    /// Merc pilot-ticks, aboard a ship or in the pool, working or not: what
    /// merc pilots' pay is spread over; and what realms would have paid
    /// them over those.
    pub crew_ticks: f64,
    pub crew_realm_pay: f64,
    /// Crystal realms got for hulls they sold companies.
    pub sold: f64,
    /// Hulls companies lost in fights.
    pub lost: f64,
    /// Of the contracts won: the worlds' worth to their employers (what they
    /// turn out over the horizon) and what they paid for them.
    pub worth_won: f64,
    pub paid_won: f64,
    /// Companies split off and joined into others.
    pub splits: u64,
    pub joins: u64,
    pub founded: u64,
    /// What companies have made, all told.
    pub profit: f64,
    /// Voyages idle companies made to look for work.
    pub sailed: u64,
    /// Crystal companies paid their pilots: wages, and bonuses for kills;
    /// and bonuses they still owe.
    pub wages: f64,
    pub kill_bonuses: f64,
    pub owed_pilots: f64,
    /// Pilots who left a company that couldn't pay them; realm pilots who
    /// defected to the pool; pilots hired from it; and pilots who quit the
    /// pool for good, or retired.
    pub left: u64,
    pub defected: u64,
    pub hired: u64,
    pub quit: u64,
    pub retired: u64,
    /// Crystal companies spent: on hulls for contracts, flying to them,
    /// sailing to look for work, and fares to join another.
    pub bought: f64,
    pub flown: f64,
    pub sailing: f64,
    pub fares: f64,
    /// Contracts won, by how long they took: 0-4 ticks, 5-9, and so on.
    pub won_after: [u64; WON_AFTER],
    /// Wages signed for, by the pilot's gear mark: their sum, and how many.
    pub signed_wages: [f64; MARKS],
    pub signings: [u64; MARKS],
}

/// A realm's offer of hulls near a target: from system `s`, `hulls` at `price` each, delivered.
#[derive(Clone, Copy)]
struct Offer {
    s: u32,
    side: u8,
    seller: u32,
    hulls: u32,
    /// What the seller gets a hull.
    ask: f32,
    /// What a hull costs the buyer, there: the ask plus its FTL.
    price: f32,
}

/// A company's bid for a contract.
#[derive(Clone, Copy)]
struct Quote {
    company: usize,
    /// Hulls it commits, of them bought.
    hulls: u32,
    bought: u32,
    /// What it pays out (hulls bought, FTL).
    cost: f32,
    upfront: f32,
    rate: f32,
    /// Of the rate (before its margin): its wages.
    wages: f32,
    /// Pilots it reckons it'll have, and what it'd pay one at T0.
    crew: u32,
    wage: f32,
    /// Ticks it'd fly.
    fly: u64,
    /// Of the crew, rookies it trains for it.
    trained: u32,
}

/// One way of fighting for a system (`Galaxy::forecast`): a side's own hulls
/// there, those on the way and the ticks till they land, a company's hulls
/// and crew and theirs, and its own pilots.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Plan {
    pub own: f32,
    pub late: f32,
    pub late_at: f32,
    pub mercs: f32,
    pub crew: f32,
    pub merc_at: f32,
    /// The gear the crew flies.
    pub rank: f32,
    pub flown: f32,
    pub defend: bool,
}

/// A hire to look back on (`Galaxy::learn`).
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct Lesson {
    pub system: u32,
    pub side: u8,
    pub defend: bool,
    pub held: u8,
    pub expected: f32,
    pub due: u64,
}

/// How things stand in a system for a side that might fight there
/// (`Galaxy::scene`): sites its and others', its fire and armor, its enemies'
/// hulls, fire, armor and pilots, what they could bring and how soon, and
/// their generators and what those shield.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Scene {
    mine0: f32,
    theirs0: f32,
    raid: bool,
    tier: u8,
    pf: f32,
    af: f32,
    he: f32,
    pe: f32,
    ae: f32,
    spare0: f32,
    rescue: f32,
    crew_e: f32,
    gens0: f32,
    shielded: f32,
}

/// How a fight's expected to go (`Galaxy::forecast`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Forecast {
    pub share: f32,
    pub ticks: f32,
    pub lost: f32,
    /// Ticks till there's nobody left to fight.
    pub length: f32,
}

/// A contract's terms, but for the company.
struct Terms {
    t: usize,
    f: usize,
    r: u32,
    defend: bool,
    worth: f32,
    expected: f32,
    expected_ticks: f32,
}

impl Company {
    /// What it's worth: its crystal, and its hulls at the going price.
    pub fn capital(&self, hull_price: f32) -> f32 {
        let field = self.contract.map_or(0, |c| c.inbound + c.field);
        self.crystals + (self.hulls + field) as f32 * hull_price
    }
}

impl Galaxy {
    /// The first companies: a few dozen ships over systems picked at random.
    pub(crate) fn found_companies(&mut self) {
        let n = (self.rules.companies as usize * self.systems.len()).div_ceil(super::SYSTEMS);
        for _ in 0..n {
            let at = self.rng.index(self.systems.len()) as u32;
            let crystals = self.rules.merc_capital * self.rng.range(0.5, 1.5);
            let c = self.new_company(at, crystals, 0);
            for _ in 0..self.rules.crew {
                let p = self.new_pilot();
                self.companies[c].pilots.push(p);
            }
        }
    }

    fn new_company(&mut self, at: u32, crystals: f32, hulls: u32) -> usize {
        self.next_company += 1;
        self.companies.push(Company {
            at,
            crystals,
            hulls,
            margin: 0.02 * 50f32.powf(self.rng.f32()),
            contract: None,
            founded: self.tick,
            idle_since: self.tick,
            id: self.next_company,
            pilots: Vec::new(),
            pricing: Pricing {
                rule: RULES[self.rng.index(RULES.len())],
                k: self.rng.range(0.5, 2.0),
            },
            patience: 10.0 * 100f32.powf(self.rng.f32()),
            earned: 0.0,
            worked: 0.0,
            loss_rate: 0.0,
            kill_rate: 0.0,
            kills: 0,
            lost_before: 0,
        });
        self.companies.len() - 1
    }

    /// Set what your pilot knows.
    pub fn know(&mut self, known: crate::economy::Known) {
        self.you.known = known;
    }

    /// A new rookie with a fresh id, on what realms pay.
    pub(crate) fn new_pilot(&mut self) -> Pilot {
        self.next_pilot += 1;
        Pilot {
            wage: self.pay,
            ..Pilot::rookie(self.next_pilot)
        }
    }

    /// The labour market, a tick of it. Pilots retire once they've saved
    /// enough, and quit after too long out of work. Ships short of pilots
    /// bid for the free ones, each by its own pricing, and pay what they
    /// bid; seats still open draw realm pilots from their realms.
    pub(crate) fn pilot_market(&mut self) {
        // Retiring: what they saved goes to whoever founds the next company.
        let career = self.rules.career;
        let (mut retired, mut saved) = (0, 0.0);
        let mut keep = |p: &Pilot, rng: &mut crate::rng::Rng| {
            if rng.chance(career) {
                retired += 1;
                saved += p.wallet;
                return false;
            }
            true
        };
        for co in &mut self.companies {
            co.pilots.retain(|p| keep(p, &mut self.rng));
        }
        self.pool.retain(|p| keep(p, &mut self.rng));
        self.books.retired += retired as u64;
        self.savings += saved;
        // A ship with no work has none for its pilots: they go to the pool,
        // where any ship with work can hire them. Too long out of work, they
        // quit for good.
        for co in &mut self.companies {
            if co.contract.is_some() {
                continue;
            }
            self.books.left += co.pilots.len() as u64;
            self.pool.append(&mut co.pilots);
        }
        for p in &mut self.pool {
            p.idle += 1;
            p.earning *= 1.0 - EARN_FADE;
        }
        // (those let go by realms and never yet aboard a ship aren't mercs)
        let served = self.pool.iter().filter(|p| p.wallet > 0.0).count();
        self.books.crew_ticks += served as f64;
        // How much of the time mercs are in work, as pilots see it.
        let mercs = self.pool.len() + self.companies.iter().map(|c| c.pilots.len()).sum::<usize>();
        if mercs > 0 {
            let now = self.employed as f32 / mercs as f32;
            self.employment += LEARN * (now - self.employment);
        }
        self.employed = 0;
        self.books.crew_realm_pay += (served as f32 * self.pay) as f64;
        self.fill_seats(true);
        // Too long out of work, they quit for good.
        let patience = self.rules.patience;
        let mut saved = 0.0;
        let before = self.pool.len();
        self.pool.retain(|p| {
            if p.idle > patience {
                saved += p.wallet;
            }
            p.idle <= patience
        });
        self.books.quit += (before - self.pool.len()) as u64;
        self.savings += saved;
    }

    /// `n` pilots leave realm `r` for the pool, with half what they knew of
    /// its gear.
    pub(crate) fn free_pilots(&mut self, r: usize, n: u32) {
        let realm = &self.realms[r];
        let half = realm.tier as f32 / 2.0;
        let maker = crate::faction::Faction::side(realm.faction as usize);
        for _ in 0..n {
            let mut p = self.new_pilot();
            p.home = Some(r as u32);
            for part in crate::economy::PARTS
                .into_iter()
                .filter(|p| p.maker() == maker)
            {
                p.known.set(part, half);
            }
            p.learned();
            self.pool.push(p);
        }
    }

    /// What pilots on realm pay `pay` want of a ship: as much, for the share
    /// of the time mercs are in work.
    pub(crate) fn reserve(&self, pay: f32) -> f32 {
        pay / self.employment.clamp(0.1, 1.0)
    }

    /// Ships with hulls in a fight and no pilots for them take on pilots:
    /// seats still open first draw realm pilots to the pool, then the pool
    /// is hired out at one price. Every tick, and as soon as a ship signs.
    pub(crate) fn fill_seats(&mut self, ease: bool) {
        // Each ship wants a pilot for each hull that can fire, as far as it
        // can pay them a tick: its crystal spread over the contract.
        let front = self.rules.frontage as u32;
        let mut want: Vec<(u32, f32)> = self
            .companies
            .iter()
            .map(|co| {
                let Some(k) = co.contract else {
                    return (0, 0.0);
                };
                let n = (k.field + k.inbound).min(front);
                let wages: f32 = co.pilots.iter().take(n as usize).map(|p| p.wage).sum();
                let ticks = (k.expected_ticks * self.trust_ticks).max(1.0);
                (
                    n.saturating_sub(co.pilots.len() as u32),
                    co.crystals.max(0.0) / ticks - wages,
                )
            })
            .collect();
        let pay = self.realm_pay();
        self.pay = pay;
        // What pilots will take: a realm's pay, in work half the time (as a
        // pilot reckons it, as when they defected).
        let reserve = self.reserve(self.rules.pilot_upkeep);
        self.defect(&want, ease);
        // The free pilots, best first, each to the ship that bids most for
        // them, at its bid: a ship pays what it offers, so one that bids
        // too high pays for it.
        let mut pool = std::mem::take(&mut self.pool);
        pool.sort_by_cached_key(|p| -((p.rank * 2.0) as i32));
        let mut left = Vec::new();
        for mut p in pool {
            let Some((c, wage)) = self.clearing(&want, p.rank, reserve) else {
                left.push(p);
                continue;
            };
            // New to it, they reckon on work half the time.
            if p.earning == 0.0 {
                p.earning = 0.5 * wage;
            }
            p.wage = wage;
            p.idle = 0;
            want[c].0 -= 1;
            want[c].1 -= wage;
            self.hire_rank += 0.01 * (p.rank - self.hire_rank);
            let m = mark(p.rank);
            self.companies[c].pilots.push(p);
            self.books.hired += 1;
            self.books.signed_wages[m] += wage as f64;
            self.books.signings[m] += 1;
            self.wage_curve[m] += 0.05 * (wage - self.wage_curve[m]);
        }
        self.pool = left;
        for m in 0..MARKS {
            let level = (m * crate::economy::STEPS_PER_MARK as usize) as f32;
            self.offers[m] = self.clearing(&want, level, reserve).map_or(0.0, |b| b.1);
        }
    }

    /// Who hires a pilot at gear `level`, among ships with seats open
    /// (`want`: how many, and what each can spend over a term), and at
    /// what wage, if the pilot won't take less than `reserve`: the ship
    /// that bids most, at its bid.
    fn clearing(&self, want: &[(u32, f32)], level: f32, reserve: f32) -> Option<(usize, f32)> {
        (0..self.companies.len())
            .filter(|&c| want[c].0 > 0)
            .map(|c| (c, self.offer(c, level)))
            .filter(|&(c, bid)| bid >= reserve && want[c].1 >= bid)
            .max_by(|a, b| a.1.total_cmp(&b.1))
    }

    /// What ship `c` offers a pilot at gear `level` a tick on its contract:
    /// what it quoted, by how well they fight.
    fn offer(&self, c: usize, level: f32) -> f32 {
        let wage = self.companies[c].contract.map_or(0.0, |k| k.wage);
        wage * gear_value(level) / gear_value(0.0)
    }

    /// Pilots who'd sign at `wage` now: the free ones, and the realm pilots
    /// who'd defect for it this tick.
    fn supply(&self, wage: f32) -> f32 {
        let i = self.pays.partition_point(|x| self.reserve(x.0) <= wage);
        let realms = if i > 0 { self.pays[i - 1].1 } else { 0.0 };
        self.pool.len() as f32 + realms * self.rules.defect
    }

    /// Realms' pay, lowest first, and the pilots on no more than it.
    fn tally_pays(&mut self) {
        let mut pays: Vec<(f32, f32)> = self
            .living()
            .map(|(_, r)| (r.pay(&self.rules), r.pilots as f32))
            .collect();
        pays.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut all = 0.0;
        for x in &mut pays {
            all += x.1;
            x.1 = all;
        }
        self.pays = pays;
    }

    /// What ship `c` bids a tick for a pilot at gear `level`, by its
    /// pricing (`want`: its seats open, and what it can spend over a term;
    /// `pay`: what realms pay their pilots, on average).
    fn bid(&self, c: usize, level: f32, want: (u32, f32), pay: f32) -> f32 {
        let co = &self.companies[c];
        let going = self.wage_curve[mark(level)];
        let base = match co.pricing.rule {
            Rule::Earnings if co.worked > 1.0 => co.earned / co.worked,
            Rule::Earnings | Rule::Markup => going,
            Rule::Gear => pay * gear_value(level) / gear_value(0.0),
            Rule::Budget => want.1.max(0.0) / (want.0.max(1) as f32 * self.rules.merc_term as f32),
        };
        co.pricing.k * base
    }

    /// What realms pay their pilots a tick, on average over their pilots.
    pub(crate) fn realm_pay(&self) -> f32 {
        let (mut paid, mut n) = (0.0, 0.0);
        for (_, r) in self.living() {
            paid += r.pay(&self.rules) * r.pilots as f32;
            n += r.pilots as f32;
        }
        if n > 0.0 {
            paid / n
        } else {
            self.rules.pilot_upkeep
        }
    }

    /// Realm pilots defect to the pool, for seats still open, when half
    /// the best bid for them beats their realm's pay: they expect to be
    /// in work half the time. They take half what they knew of their
    /// realm's gear. A realm that loses pilots pays more; one that keeps
    /// them pays less, down to the least a realm pays.
    fn defect(&mut self, want: &[(u32, f32)], ease: bool) {
        let start = self.rng.index(self.realms.len().max(1));
        let open: u32 = want.iter().map(|w| w.0).sum();
        let mut room = open.saturating_sub(self.pool.len() as u32);
        for i in 0..self.realms.len() {
            let r = (start + i) % self.realms.len();
            let realm = &self.realms[r];
            if realm.systems == 0 {
                continue;
            }
            let half = realm.tier as f32 / 2.0;
            let wage = realm.pay(&self.rules);
            let offer = if room > 0 && realm.pilots > 0 {
                self.clearing(want, half, self.reserve(wage))
                    .map_or(0.0, |b| b.1)
            } else {
                0.0
            };
            // Better to match an offer, or to lose some of its pilots and
            // train others? Whichever costs it less.
            let (rules, employed) = (&self.rules, self.employment.clamp(0.1, 1.0));
            let need = offer * employed;
            let lose = rules.defect * rules.train_cost;
            let matched = need > wage && need - wage < lose && need < realm.pilot_worth;
            let n = if room > 0 && realm.pilots > 0 && offer > 0.0 && !matched {
                self.rng.binomial(realm.pilots, self.rules.defect).min(room)
            } else {
                0
            };
            let (step, floor) = (self.rules.pay_step, self.rules.pilot_upkeep);
            let realm = &mut self.realms[r];
            realm.wage = if matched {
                need
            } else if n > 0 || !ease {
                wage
            } else {
                (wage * (1.0 - step)).max(floor)
            };
            if n == 0 {
                continue;
            }
            room -= n;
            realm.pilots -= n;
            self.free_pilots(r, n);
            self.books.defected += n as u64;
        }
    }

    /// Crystal to move a ship carrying `hulls` hulls `d` light years.
    pub(super) fn voyage(&self, hulls: u32, d: f32) -> f32 {
        d / FTL_RANGE * (self.rules.ship_cost + hulls as f32 * self.rules.ftl_cost)
    }

    /// System `t`'s worth to side `f`: what its production sites turn out a
    /// tick (those `f` holds, or with not `own` those it doesn't) over the
    /// ticks it can expect to hold them, in crystal. (Its yards build no
    /// cheaper than any other's, so they add nothing.)
    pub(crate) fn worth(&self, t: usize, f: usize, own: bool) -> f32 {
        let (a, b) = self.systems[t].sites;
        let income: f32 = self.sites[a as usize..b as usize]
            .iter()
            .filter(|s| (s.owner as usize == f) == own)
            .map(|s| match s.kind {
                SiteKind::Production { crystals, .. } => crystals,
                SiteKind::Shield { .. } | SiteKind::Outpost => 0.0,
            })
            .sum();
        // And what's in the vaults, won or lost with the planets.
        let vaults: f32 = self
            .vaults_of(t)
            .iter()
            .filter(|v| (v.side as usize == f) == own)
            .map(|v| v.crystals)
            .sum();
        income * self.hold_for(!own) + vaults
    }

    /// Hulls for sale to a company at system `from` working for realm `r`
    /// of side `f` in system `t`: within range of the target or of the
    /// ship, delivered to the target, cheapest first.
    fn offers(&self, t: usize, f: usize, r: u32, from: usize) -> Vec<Offer> {
        let defenders = self.systems[t].realms;
        let mut out: Vec<Offer> = Vec::new();
        // Around the target, and around the ship: it can carry what it
        // buys there.
        let mut near: Vec<usize> = self.neighbors(t).map(|(s, _)| s).collect();
        near.push(from);
        near.extend(self.neighbors(from).map(|(s, _)| s));
        near.sort();
        near.dedup();
        let to = self.systems[t].pos;
        for s in near {
            let d = self.systems[s].pos.distance(to);
            let sys = &self.systems[s];
            let g = sys.owner();
            let seller = sys.realms[g];
            if sys.at_war()
                || seller == super::realm::NONE
                || seller == r
                || (0..SIDES).any(|h| h != f && defenders[h] == seller)
            {
                continue;
            }
            let hulls = self.spare[s][g];
            if hulls < MIN_SEND {
                continue;
            }
            // A realm sells at what one's worth to it there.
            let ask = self.hull_worth[s].max(0.0);
            out.push(Offer {
                s: s as u32,
                side: g as u8,
                seller,
                hulls,
                ask,
                price: ask + d / FTL_RANGE * self.rules.ftl_cost,
            });
        }
        out.sort_by(|a, b| a.price.total_cmp(&b.price));
        out
    }

    /// Company `c`'s quote for `size` hulls in the fight at `pos`: those
    /// aboard first, the rest bought from `offers`, cheapest first. Its
    /// upfront covers what it pays out; its rate, its guess at a tick of
    /// the work (a pilot for every hull, those aboard at their wage and
    /// the rest at what pilots lately sign for, `pay` the least; the hulls
    /// it loses; the kills it pays for), both with its margin.
    fn quote(
        &self,
        c: usize,
        size: u32,
        offers: &[Offer],
        pos: glam::Vec3,
        pay: f32,
        train: bool,
    ) -> Quote {
        let co = &self.companies[c];
        let mut short = size.saturating_sub(co.hulls);
        let (mut bought, mut spend) = (0, 0.0);
        for o in offers {
            if short == 0 {
                break;
            }
            let m = o.hulls.min(short);
            bought += m;
            spend += m as f32 * o.price;
            short -= m;
        }
        let hulls = co.hulls.min(size) + bought;
        let d = self.systems[co.at as usize].pos.distance(pos);
        let mut cost = spend + self.voyage(co.hulls, d);
        let fly = self.flight(d);
        let unit = if bought > 0 {
            spend / bought as f32
        } else {
            self.rules.hull_cost
        };
        // A pilot for each hull that can fire: those aboard, and as many as
        // will sign for its bid (no less than pilots take).
        let seats = hulls.min(self.rules.frontage as u32) as usize;
        let aboard = co.pilots.len().min(seats);
        let mut wages: f32 = co.pilots.iter().take(aboard).map(|p| p.wage).sum();
        let missing = (seats - aboard) as u32;
        let wage = self
            .bid(c, 0.0, (missing, co.crystals), pay)
            .max(self.reserve(pay));
        let hired = (missing as f32).min(self.supply(wage));
        // (they're paid by how well their gear fights: as those hired lately)
        let geared = wage * gear_value(self.hire_rank) / gear_value(0.0);
        wages += hired * geared;
        let mut crew = aboard as u32 + hired as u32;
        // And the rest it can train itself, as a realm would.
        let trained = if train { seats as u32 - crew } else { 0 };
        cost += trained as f32 * self.rules.train_cost;
        wages += trained as f32 * wage;
        crew += trained;
        let losses = co.loss_rate * hulls as f32 * unit;
        let kills = co.kill_rate * wages * self.rules.kill_share;
        let up = 1.0 + co.margin;
        Quote {
            company: c,
            hulls,
            bought,
            cost,
            upfront: (cost + wages * fly as f32) * up,
            fly,
            rate: (wages + losses + kills) * up,
            wages,
            crew,
            wage,
            trained,
        }
    }

    /// The spare hulls realm `r` (of side `f`) could bring to system `t` for
    /// `purse` crystals.
    fn can_move(&self, t: usize, f: usize, r: u32, purse: f32) -> f32 {
        let (mut hulls, mut cost) = (0.0, 0.0);
        for (s, d) in self.neighbors(t) {
            if self.systems[s].realms[f] == r {
                let n = self.spare[s][f] as f32;
                hulls += n;
                cost += n * d / FTL_RANGE * self.rules.ftl_cost;
            }
        }
        if cost > 0.0 {
            hulls * (purse / cost).clamp(0.0, 1.0)
        } else {
            hulls
        }
    }

    /// The fight for system `t` played forward by the galaxy's own battle
    /// math, without the dice: side `f` with `own` hulls and `mercs` hired
    /// ones, against everyone else there and what they can spare nearby,
    /// for a contract's
    /// term. Each tick there's a battle for each site, and in each, each
    /// side's fire (no more hulls than the frontage) destroys the other's
    /// hulls, less its armor; and wins it each site
    /// fought over with the chance its share of the fire gives; a side
    /// left alone takes what's left unguarded. Returns
    /// the share of the world's sites `f` holds at the end (of those it
    /// set out to take, or to keep), the ticks the mercs fight, and its own
    /// hulls lost.
    pub(crate) fn forecast(&self, scene: &Scene, plan: &Plan) -> Forecast {
        let &Plan {
            own,
            late,
            late_at,
            mercs,
            crew,
            merc_at,
            rank,
            flown,
            defend,
        } = plan;
        let &Scene {
            mine0,
            theirs0,
            raid,
            tier,
            pf,
            af,
            he,
            pe,
            ae,
            spare0,
            rescue,
            crew_e,
            gens0,
            shielded,
        } = scene;
        // (for the crew, theirs)
        let kit =
            crate::economy::damage_factor(rank.round() as u8) / crate::economy::damage_factor(tier);
        let reinforce = |he: &mut f32, spare: &mut f32, ours: f32| {
            // Attacked, they rush in all they can, unless they've plenty.
            let against = ours * pf * 3.0;
            if *spare < MIN_SEND as f32 || *he * pe >= against {
                return;
            }
            let want = *spare;
            let more = want.clamp(0.0, *spare);
            *he += more;
            *spare -= more;
        };
        let cut = self.shield_cut();
        let front = self.rules.frontage;
        let (k, take, blitz) = (self.rules.kill, self.rules.take, self.rules.blitz as f32);
        let (mut paid, mut length) = (0.0f32, 0.0f32);
        {
            let (mut ho, mut hm, mut he, mut spare) = (own, 0.0, he, spare0);
            let (mut late, mut seen) = (late, 0.0f32);
            let (mut mine, mut theirs, mut gens) = (mine0, theirs0, gens0);
            for i in 0..self.rules.merc_term {
                if i as f32 >= late_at {
                    ho += late;
                    late = 0.0;
                }
                let here = i as f32 >= merc_at;
                if here && i as f32 - merc_at < 1.0 {
                    hm = mercs;
                }
                let hf = ho + hm;
                // Once they've seen it, help's some ticks off.
                if hf >= 1.0 {
                    seen += 1.0;
                }
                if seen > rescue {
                    reinforce(&mut he, &mut spare, hf);
                }
                if he >= 1.0 && hf >= 1.0 {
                    length += 1.0;
                }
                if hm >= 1.0 {
                    paid += 1.0;
                }
                // A side left alone walks into what's unguarded.
                // (or nobody's come yet)
                if he < 1.0 || hf < 1.0 {
                    let n = if hf >= 1.0 {
                        blitz.min(theirs)
                    } else if he >= 1.0 {
                        -blitz.min(mine)
                    } else {
                        0.0
                    };
                    mine += n;
                    theirs -= n;
                } else {
                    // Its crews fire first: there's a place for so many.
                    let crews = hm.min(crew).min(front);
                    let own_fire = ho.min(flown).min(front - crews);
                    let ff = (crews * kit + own_fire) * pf;
                    let fe = he.min(crew_e).min(front) * pe;
                    let share = ff / (ff + fe).max(1e-3);
                    // Theirs open to it: while generators are up, those and
                    // what they don't shield, a seat short.
                    let (open, at) = if gens >= 1.0 {
                        (
                            (theirs - shielded).max(1.0),
                            ff * cut / (ff * cut + fe).max(1e-3),
                        )
                    } else {
                        (theirs, share)
                    };
                    let won = (take * at * open).min(theirs);
                    let lost = (take * (1.0 - share) * mine).min(mine);
                    gens = (gens - won).max(0.0);
                    mine += won - lost;
                    theirs += lost - won;
                    // Fire's split between the sites' battles, so it's all
                    // told what one would do; the mercs lose their share.
                    let left = 1.0 - (k * fe / af / hf).min(1.0);
                    ho *= left;
                    hm *= left;
                    he = (he - k * ff / ae).max(0.0);
                }
                // A raid goes on till it has everything (or all but).
                if raid && theirs < 0.3 {
                    return Forecast {
                        share: 1.0,
                        ticks: f32::max(paid, 1.0),
                        lost: 0.0,
                        length: f32::max(length, 1.0),
                    };
                }
                // Done, the mercs go, and it holds on alone.
                if here
                    && hm >= 1.0
                    && if defend {
                        he < AT_WAR as f32
                    } else if raid {
                        false
                    } else {
                        mine > theirs
                    }
                {
                    hm = 0.0;
                }
            }
            let share = if defend {
                (mine / mine0.max(1.0)).min(1.0)
            } else {
                ((mine - mine0) / theirs0.max(1.0)).max(0.0)
            };
            Forecast {
                share,
                ticks: f32::max(paid, 1.0),
                lost: (own + plan.late - late - ho).max(0.0),
                length: f32::max(length, 1.0),
            }
        }
    }

    /// How things stand for side `f` (realm `r`'s) fighting for system `t`.
    pub(crate) fn scene(&self, t: usize, f: usize, r: u32) -> Scene {
        let sys = &self.systems[t];
        let total = (sys.sites.1 - sys.sites.0).max(1) as f32;
        let mine0 = sys.held[f] as f32;
        let theirs0 = total - mine0;
        let hulls = sys.hulls;
        let strength = self.strength(t);
        let per = |g: usize| {
            if hulls[g] > 0 {
                strength[g] / hulls[g] as f32
            } else {
                1.0
            }
        };
        let gear = |g: usize| self.gear(t, g);
        // Its own side: the realm's gear (the mercs' is their own, but the
        // realm reckons by what it knows), the ground it holds here, here.
        let raid = r == super::realm::NONE;
        let tier = if raid {
            0
        } else {
            self.realms[r as usize].tier
        };
        let mut pf = 1.0 + self.rules.home_ground * mine0 / total;
        pf *= crate::economy::damage_factor(tier);
        let af = crate::economy::armor_factor(tier);
        // Everyone else, as one: their hulls, and their fire and armor
        // averaged over them.
        let (mut he, mut fire_e, mut armor_e) = (0.0, 0.0, 0.0);
        for g in (0..SIDES).filter(|&g| g != f && hulls[g] > 0) {
            let h = hulls[g] as f32;
            he += h;
            fire_e += h * per(g) * crate::economy::damage_factor(gear(g));
            armor_e += h * crate::economy::armor_factor(gear(g));
        }
        let (pe, ae) = if he > 0.0 {
            (fire_e / he, armor_e / he)
        } else {
            (1.0, 1.0)
        };
        // And what they'll bring up (`Galaxy::consider`): a realm's spare
        // hulls in range, as many as it can pay to move, enough to reach
        // its odds against what's there, or none if they can't.
        let mut spare0 = 0.0f32;
        let mut rescue = f32::INFINITY;
        // The pilots they'd fly with: their realms', and their mercs'.
        let mut crew_e = 0.0f32;
        for g in (0..SIDES).filter(|&g| g != f) {
            let e = sys.realms[g];
            if e == super::realm::NONE {
                crew_e += self.crewed(t, g).1 as f32;
                continue;
            }
            let (m, crewed) = self.crewed(t, g);
            let theirs = (hulls[g] - m.min(hulls[g])) as f32;
            let more = self.can_move(t, g, e, self.realms[e as usize].crystals);
            crew_e += crewed as f32 + self.pilots_for(e, theirs + more);
            spare0 += more;
            // (they see the attack, and then they fly)
            let wait = self.rescue(t, g, e) as f32;
            rescue = rescue.min(wait);
        }
        // Their sites: generators up, what those shield, and the rest. An
        // attacker can only go for the generators and what's in the open
        // while any are up, and fights a seat short against them.
        let (a, b) = sys.sites;
        let planets = self.shields_up(t, f);
        let (mut gens0, mut shielded) = (0.0f32, 0.0f32);
        for site in &self.sites[a as usize..b as usize] {
            if site.owner as usize == f {
                continue;
            }
            match site.kind {
                SiteKind::Shield { damaged: false } => gens0 += 1.0,
                _ if planets[site.planet as usize] > 0 => shielded += 1.0,
                _ => {}
            }
        }
        Scene {
            mine0,
            theirs0,
            raid,
            tier,
            pf,
            af,
            he,
            pe,
            ae,
            spare0,
            rescue,
            crew_e,
            gens0,
            shielded,
        }
    }

    /// The gear company `c` would fly on quote `q`: its crew's, those it'd
    /// hire as those hired lately, and rookies.
    fn crew_rank(&self, c: usize, q: &Quote) -> f32 {
        let n = q.crew.max(1) as f32;
        let aboard = &self.companies[c].pilots;
        let known: f32 = aboard.iter().take(q.crew as usize).map(|p| p.rank).sum();
        let hired = q.crew as f32 - aboard.len().min(q.crew as usize) as f32 - q.trained as f32;
        (known + hired.max(0.0) * self.hire_rank) / n
    }

    /// Hires whose term's up, looked back on: the share of the world held,
    /// of what was expected.
    fn learn(&mut self) {
        let tick = self.tick;
        let (due, wait): (Vec<Lesson>, Vec<Lesson>) = std::mem::take(&mut self.lessons)
            .into_iter()
            .partition(|l| tick >= l.due);
        self.lessons = wait;
        for l in due {
            let sys = &self.systems[l.system as usize];
            let total = (sys.sites.1 - sys.sites.0).max(1) as f32;
            let (now, then) = (sys.held[l.side as usize] as f32, l.held as f32);
            let got = if l.defend {
                (now / then.max(1.0)).min(1.0)
            } else {
                ((now - then) / (total - then).max(1.0)).max(0.0)
            };
            if l.expected > 0.05 {
                let trust = &mut self.hire_trust[l.defend as usize];
                *trust += LEARN * ((got / l.expected).min(1.5) - *trust);
            }
        }
    }

    /// Ticks for realm `e` (of side `g`) to bring help to system `t`.
    fn rescue(&self, t: usize, g: usize, e: u32) -> u64 {
        self.neighbors(t)
            .filter(|&(s, _)| self.systems[s].realms[g] == e && self.spare[s][g] >= MIN_SEND)
            .map(|(_, d)| self.flight(d))
            .min()
            .unwrap_or(self.rules.merc_term as u64)
    }

    /// Realm `r` owes company `c` (an index) `crystals` more: no interest, to
    /// be paid back over the ticks the company waits.
    fn lend(&mut self, r: u32, c: usize, crystals: f32) {
        let (id, patience) = (self.companies[c].id, self.companies[c].patience);
        let i = match self
            .debts
            .iter()
            .position(|d| d.realm == r && d.company == id)
        {
            Some(i) => i,
            None => {
                self.debts.push(Debt {
                    realm: r,
                    company: id,
                    crystals: 0.0,
                    due: 0.0,
                });
                self.debts.len() - 1
            }
        };
        let d = &mut self.debts[i];
        d.crystals += crystals;
        d.due = d.crystals / patience.max(1.0);
        self.books.financed += crystals as f64;
    }

    /// Realm `r` pays company `c` (an index) up to `x` of what it owes it.
    fn settle_up(&mut self, r: u32, c: usize, x: f32) {
        let id = self.companies[c].id;
        let Some(d) = self
            .debts
            .iter_mut()
            .find(|d| d.realm == r && d.company == id)
        else {
            return;
        };
        let realm = &mut self.realms[r as usize];
        let pay = x.min(d.crystals).min(realm.crystals.max(0.0));
        d.crystals -= pay;
        realm.crystals -= pay;
        self.companies[c].crystals += pay;
        self.companies[c].earned += pay;
        self.books.repaid += pay as f64;
    }

    /// What realms owe company `id`: part of what it's worth.
    pub fn lent(&self, id: u32) -> f32 {
        self.debts
            .iter()
            .filter(|d| d.company == id)
            .map(|d| d.crystals)
            .sum()
    }

    /// What realm `r` owes, to anyone.
    fn owes(&self, r: u32) -> f32 {
        self.debts
            .iter()
            .filter(|d| d.realm == r)
            .map(|d| d.crystals)
            .sum()
    }

    /// Realm `r` (of side `f`), with `own` hulls to fight for system `t` with
    /// (to take it, or hold it: `defend`), hires whichever company does it
    /// the most good for its price, over `base`: what it expects without.
    /// `plan` is how it'd fight alone, `start` ticks from now. Returns whether
    /// it did.
    pub(crate) fn hire(
        &mut self,
        t: usize,
        f: usize,
        r: u32,
        plan: Plan,
        start: u64,
        base: f32,
    ) -> bool {
        if self.ban == Some((r, t as u32)) {
            return false;
        }
        let defend = plan.defend;
        let worth = self.worth(t, f, defend);
        let realm = self.realms[r as usize];
        // Nothing left to gain by it.
        if worth <= 0.0 || base >= worth * 0.99 {
            return false;
        }
        let pos = self.systems[t].pos;
        let near: Vec<usize> = (0..self.companies.len())
            .filter(|&c| {
                let co = &self.companies[c];
                co.contract.is_none()
                    && co.idle_since <= self.tick
                    && co.crystals > 0.0
                    && self.systems[co.at as usize].pos.distance(pos) <= self.rules.merc_range
            })
            .collect();
        // The nearest few quote.
        let mut near = near;
        near.sort_by(|&a, &b| {
            let d = |c: usize| {
                self.systems[self.companies[c].at as usize]
                    .pos
                    .distance(pos)
            };
            d(a).total_cmp(&d(b))
        });
        near.truncate(6);
        self.books.asked += 1;
        if near.is_empty() {
            self.books.none_near += 1;
            self.unmet[t] += 1.0;
            return false;
        }
        if self.pays_tick != self.tick {
            self.pays_tick = self.tick;
            self.tally_pays();
            self.pay = self.realm_pay();
        }
        let pay = self.pay;
        let (owed, income) = (self.owes(r), realm.income.max(0.0));
        let mut best: Option<(Quote, f32, f32, f32, Vec<Offer>)> = None;
        let mut affordable = false;
        let scene = self.scene(t, f, r);
        for c in near {
            let offers = self.offers(t, f, r, self.companies[c].at as usize);
            for (size, train) in [
                (30, false),
                (80, false),
                (200, false),
                (200, true),
                (500, false),
            ] {
                let q = self.quote(c, size, &offers, pos, pay, train);
                if q.hulls < MIN_SEND || q.crew == 0 || q.upfront + q.rate >= worth - base {
                    continue;
                }
                let mercs = q.hulls as f32;
                let merc_at = q.fly.saturating_sub(start) as f32;
                let with = Plan {
                    mercs,
                    crew: q.crew as f32,
                    merc_at,
                    rank: self.crew_rank(c, &q),
                    ..plan
                };
                let fc = self.forecast(&scene, &with);
                let ticks = fc.ticks * self.trust_ticks;
                let price = q.upfront + q.rate * ticks;
                let co = &self.companies[c];
                // It pays its way out of its own crystal, and lends what the
                // realm can't pay now, if its income would clear it in time.
                let now = q.upfront.min(realm.crystals.max(0.0));
                if q.cost + q.wages * (ticks + q.fly as f32) > co.crystals + now
                    || q.cost > co.crystals + now
                {
                    continue;
                }
                affordable = true;
                if owed + price > realm.crystals.max(0.0) + income * co.patience {
                    self.books.refused += 1;
                    continue;
                }
                // As far as hires have lately come to what was expected of them.
                let gain = fc.share * worth - fc.lost * realm.hull_worth - base;
                let net = gain * self.hire_trust[defend as usize] - price;
                if net <= 0.0 {
                    self.books.dear += 1;
                    self.books.dear_gain += gain as f64;
                    self.books.dear_price += price as f64;
                    continue;
                }
                if best.as_ref().is_none_or(|b| net > b.1) {
                    best = Some((q, net, fc.share * worth, fc.ticks, offers.clone()));
                }
            }
        }
        let Some((q, net, expected, expected_ticks, offers)) = best else {
            if affordable {
                self.books.none_worth += 1;
            } else {
                self.books.none_affordable += 1;
            }
            // Word gets round: help was wanted here, and none was any use.
            self.unmet[t] += 1.0;
            return false;
        };
        self.books.surplus += net as f64;
        let terms = Terms {
            t,
            f,
            r,
            defend,
            worth,
            expected,
            expected_ticks,
        };
        let c = q.company;
        let (t, side) = (t as u32, f as u8);
        self.lessons.push(Lesson {
            system: t,
            side,
            defend,
            held: self.systems[t as usize].held[f],
            expected: expected / worth.max(1e-3),
            due: self.tick + q.fly + self.rules.merc_term as u64,
        });
        self.sign(q, &offers, &terms);
        if let Some(k) = &mut self.companies[c].contract {
            k.net = net;
        }
        true
    }

    /// Companies that get to their jobs this tick land.
    pub(crate) fn land_mercs(&mut self) {
        for c in 0..self.companies.len() {
            let Some(mut k) = self.companies[c].contract else {
                continue;
            };
            if k.inbound == 0 || self.tick < k.arrives {
                continue;
            }
            let (t, f) = (k.target as usize, k.side as usize);
            let sys = &mut self.systems[t];
            // Someone else got to raid it first.
            if f == super::RAID && (sys.hulls[f] > 0 || sys.held[f] > 0) {
                self.call_off(c);
                continue;
            }
            if f != super::RAID && sys.realms[f] == super::realm::NONE {
                if self.realms[k.realm as usize].systems == 0 {
                    self.call_off(c);
                    continue;
                }
                sys.realms[f] = k.realm;
            }
            sys.hulls[f] += k.inbound;
            self.moves[t][f].hired += k.inbound;
            self.merc_moves[t][f].hired += k.inbound;
            k.field += k.inbound;
            k.inbound = 0;
            k.held = self.systems[t].held[f];
            self.companies[c].contract = Some(k);
        }
    }

    /// Company `c`, on its way to a job, has none any more.
    fn call_off(&mut self, c: usize) {
        let tick = self.tick;
        let co = &mut self.companies[c];
        if let Some(k) = co.contract.take() {
            co.hulls += k.inbound;
        }
        co.lost_before = 0;
        co.idle_since = tick;
        self.books.called_off += 1;
    }

    /// Company `q.company` signs on `terms`.
    fn sign(&mut self, q: Quote, offers: &[Offer], terms: &Terms) {
        let &Terms {
            t,
            f,
            r,
            defend,
            worth,
            expected,
            expected_ticks,
        } = terms;
        // Sign: the company buys and goes, and the realm owes it.
        if r != super::realm::NONE {
            self.lend(r, q.company, q.upfront);
            let owed = q.upfront;
            self.settle_up(r, q.company, owed);
        }
        let mut short = q.bought;
        for o in offers {
            if short == 0 {
                break;
            }
            let m = o.hulls.min(short);
            short -= m;
            let (s, g) = (o.s as usize, o.side as usize);
            self.spare[s][g] -= m;
            self.systems[s].hulls[g] -= m;
            self.moves[s][g].left += m;
            let paid = m as f32 * o.ask;
            self.realms[o.seller as usize].crystals += paid;
            self.books.sold += paid as f64;
            self.hull_rate += 0.02 * (o.ask - self.hull_rate);
        }
        let land = q.hulls;
        let spent: f32 = offers_spend(offers, q.bought);
        self.books.bought += spent as f64;
        let schooling = q.trained as f32 * self.rules.train_cost;
        self.books.trained += q.trained as u64;
        self.books.flown += (q.cost - spent - schooling) as f64;
        for _ in 0..q.trained {
            let p = self.new_pilot();
            self.companies[q.company]
                .pilots
                .push(Pilot { wage: q.wage, ..p });
        }
        let co = &mut self.companies[q.company];
        co.crystals -= q.cost;
        // Its own hulls go in first; the rest stay aboard.
        let own = co.hulls.min(q.hulls - q.bought);
        co.hulls -= own;
        co.at = t as u32;
        co.contract = Some(Contract {
            realm: r,
            side: f as u8,
            target: t as u32,
            signed: self.tick,
            upfront: q.upfront,
            rate: q.rate,
            billed: 0.0,
            wage: q.wage,
            committed: land,
            field: 0,
            inbound: land,
            arrives: self.tick + q.fly,
            lost: 0,
            defend,
            expected,
            net: 0.0,
            expected_ticks,
            held: self.systems[t].held[f],
            worth,
        });
        self.note_incoming(t as u32, f as u8, land, self.tick + q.fly);
        if r == super::realm::NONE {
            self.books.raids += 1;
        } else {
            self.books.signed += 1;
        }
        self.books.defenses += defend as u64;
        // It takes on pilots for its hulls before the fight.
        self.fill_seats(false);
        self.books.upfront += q.upfront as f64;
    }

    /// Each company pays its crew what it owes them, and on a contract,
    /// the wages of those flying its hulls. Idle pilots aren't paid.
    fn pay_crews(&mut self) {
        let ship_upkeep = self.rules.ship_upkeep;
        for co in &mut self.companies {
            for p in &mut co.pilots {
                let x = p.owed.min(co.crystals.max(0.0));
                p.owed -= x;
                p.wallet += x;
                co.crystals -= x;
            }
            // The ship costs something to keep, working or not.
            co.crystals = (co.crystals - ship_upkeep).max(0.0);
            co.earned *= 1.0 - FADE;
            co.worked *= 1.0 - FADE;
            self.books.crew_ticks += co.pilots.len() as f64;
            self.books.crew_realm_pay += (co.pilots.len() as f32 * self.pay) as f64;
            for p in &mut co.pilots {
                p.idle += 1;
                p.earning *= 1.0 - EARN_FADE;
            }
            // Only those flying its hulls on a contract are working, and
            // paid: its best. Those it can't pay leave for the pool.
            let Some(k) = co.contract else { continue };
            co.pilots.sort_by_cached_key(|p| -((p.rank * 2.0) as i32));
            let seats = k.field + k.inbound;
            let flying = (seats.min(self.rules.frontage as u32) as usize).min(co.pilots.len());
            let mut paid = 0;
            while paid < flying && co.pilots[paid].wage <= co.crystals.max(0.0) {
                let p = &mut co.pilots[paid];
                co.crystals -= p.wage;
                p.wallet += p.wage;
                p.earning += EARN_FADE * p.wage;
                p.idle = 0;
                self.books.wages += p.wage as f64;
                paid += 1;
            }
            co.worked += paid as f32;
            self.employed += paid as u32;
            if paid < flying {
                self.books.left += (flying - paid) as u64;
                let unpaid: Vec<Pilot> = co.pilots.drain(paid..flying).collect();
                self.pool
                    .extend(unpaid.into_iter().map(|p| Pilot { idle: 0, ..p }));
            }
        }
        // What companies owe you.
        let mut owed = std::mem::take(&mut self.owed_you);
        for (id, x) in &mut owed {
            if let Some(c) = self.company(*id) {
                let pay = x.min(self.companies[c].crystals.max(0.0));
                self.companies[c].crystals -= pay;
                *x -= pay;
                self.you.wallet += pay;
            } else {
                *x = 0.0;
            }
        }
        owed.retain(|o| o.1 > 1e-4);
        self.owed_you = owed;
    }

    /// Company `c` pays pilot `p` (an index into its crew) a bonus: as much
    /// as it has, owing the rest.
    pub(crate) fn bonus(&mut self, c: usize, p: usize, crystals: f32) {
        let co = &mut self.companies[c];
        let paid = crystals.min(co.crystals.max(0.0));
        co.crystals -= paid;
        co.pilots[p].wallet += paid;
        co.pilots[p].owed += crystals - paid;
        self.books.owed_pilots += (crystals - paid) as f64;
    }

    /// Your company pays you `cr` CR for a battle: as much as it has, owing
    /// the rest. Returns the CR paid now.
    pub fn pay_you(&mut self, cr: u32) -> u32 {
        let want = cr as f32 / self.cr_per_crystal();
        // With no company (a round outside the war), nobody pays: it's
        // the Company's.
        let Some(c) = self.crew_company() else {
            return cr;
        };
        let co = &mut self.companies[c];
        let paid = want.min(co.crystals.max(0.0));
        co.crystals -= paid;
        if want - paid > 1e-4 {
            self.owed_you.push((co.id, want - paid));
        }
        (paid * self.cr_per_crystal()).round() as u32
    }

    /// CR to the crystal: a pilot's wage a tick is `Rules::wage_cr`.
    pub fn cr_per_crystal(&self) -> f32 {
        self.rules.wage_cr / self.pay.max(1e-6)
    }

    /// Back pay that's reached you since last asked, in CR.
    pub fn take_wallet(&mut self) -> u32 {
        let rate = self.cr_per_crystal();
        let cr = (self.you.wallet * rate).floor();
        self.you.wallet -= cr / rate;
        cr as u32
    }

    /// Company `c` breaks off its raid `k`: what it held goes back to whoever
    /// held the planet before it came, and what's left of its hulls aboard.
    fn leave(&mut self, c: usize, k: Contract) {
        let t = k.target as usize;
        let back = k.field.min(self.systems[t].hulls[super::RAID]);
        self.systems[t].hulls[super::RAID] = 0;
        self.moves[t][super::RAID].released += back;
        self.merc_moves[t][super::RAID].released += back;
        let (a, b) = self.systems[t].sites;
        for i in a as usize..b as usize {
            if self.sites[i].owner as usize != super::RAID {
                continue;
            }
            let v = self.vault_at[t] as usize + self.sites[i].planet as usize;
            let f = self.vaults[v].side;
            self.sites[i].owner = f;
            let sys = &mut self.systems[t];
            sys.held[super::RAID] -= 1;
            sys.held[f as usize] += 1;
            let r = self.vaults[v].realm;
            if sys.realms[f as usize] == super::realm::NONE && r != super::realm::NONE {
                sys.realms[f as usize] = r;
            }
            self.claim(t, f as usize);
        }
        let co = &mut self.companies[c];
        co.hulls += back;
        co.contract = None;
        co.lost_before = 0;
        co.idle_since = self.tick;
    }

    /// Idle companies look for a rich world too weakly held, and raid it if
    /// the battle maths says what's in its vaults beats what it'd cost: the
    /// flight, the hulls, the crew. Yours doesn't (yet).
    fn raid(&mut self) {
        self.count_spare();
        self.count_incoming();
        if self.pays_tick != self.tick {
            self.pays_tick = self.tick;
            self.tally_pays();
            self.pay = self.realm_pay();
        }
        let pay = self.pay;
        // Worlds by what's in their vaults for their defenders, as far as
        // one can see.
        let rich: Vec<(f32, usize)> = (0..self.systems.len())
            .filter_map(|s| {
                let x: f32 = self.vaults_of(s).iter().map(|v| v.crystals).sum();
                let h: u32 = self.systems[s].hulls.iter().sum();
                (x > 5.0).then_some((x * 200.0 / (200.0 + h as f32), s))
            })
            .collect();
        for c in 0..self.companies.len() {
            let co = &self.companies[c];
            if co.contract.is_some()
                || co.idle_since > self.tick
                || Some(co.id) == self.crew
                || co.crystals <= 0.0
                || !(self.tick + co.id as u64).is_multiple_of(super::THINK)
            {
                continue;
            }
            let here = self.systems[co.at as usize].pos;
            // The best few in range, for what's there less the flight.
            let load = self.voyage(200, FTL_RANGE) / FTL_RANGE;
            let mut seen: Vec<(f32, usize)> = rich
                .iter()
                .filter(|&&(_, s)| {
                    let sys = &self.systems[s];
                    sys.hulls[super::RAID] == 0
                        && sys.held[super::RAID] == 0
                        && self.on_the_way(s, super::RAID).0 == 0.0
                })
                .filter_map(|&(x, s)| {
                    let d = self.systems[s].pos.distance_squared(here);
                    if d > self.rules.merc_range.powi(2) {
                        return None;
                    }
                    let net = x - load * d.sqrt();
                    (net > 0.0).then_some((net, s))
                })
                .collect();
            let n = seen.len().min(6);
            if n < seen.len() {
                seen.select_nth_unstable_by(n, |a, b| b.0.total_cmp(&a.0));
                seen.truncate(n);
            }
            let mut best: Option<(f32, Quote, usize, f32, f32, Vec<Offer>)> = None;
            let mut far = 0;
            for &(_, t) in &seen {
                let loot: f32 = self.vaults_of(t).iter().map(|v| v.crystals).sum();
                let pos = self.systems[t].pos;
                let none = super::realm::NONE;
                let offers = self.offers(t, super::RAID, none, self.companies[c].at as usize);
                let scene = self.scene(t, super::RAID, none);
                for (size, train) in [
                    (30, false),
                    (80, false),
                    (80, true),
                    (200, false),
                    (200, true),
                    (500, true),
                ] {
                    let q = self.quote(c, size, &offers, pos, pay, train);
                    if q.hulls < MIN_SEND || q.crew == 0 || q.cost + q.wages >= loot {
                        far = far.max(if q.hulls < MIN_SEND || q.crew == 0 {
                            1
                        } else {
                            3
                        });
                        continue;
                    }
                    let (hulls, crew) = (q.hulls as f32, q.crew as f32);
                    let with = Plan {
                        mercs: hulls,
                        crew,
                        rank: self.crew_rank(c, &q),
                        ..Default::default()
                    };
                    let fc = self.forecast(&scene, &with);
                    let ticks = fc.ticks * self.trust_ticks;
                    // What it brings back is still worth something.
                    let cost = q.cost + q.wages * (ticks + q.fly as f32);
                    if cost > self.companies[c].crystals {
                        far = far.max(2);
                        continue;
                    }
                    let net = loot * fc.share.powi(2) - cost;
                    far = far.max(3);
                    if net > 0.0 && best.as_ref().is_none_or(|b| net > b.0) {
                        best = Some((net, q, t, loot * fc.share, fc.ticks, offers.clone()));
                    }
                }
            }
            self.books.raid_looks[if best.is_some() { 4 } else { far }] += 1;
            if let Some((_, q, t, expected, expected_ticks, offers)) = best {
                self.books.raid_there +=
                    self.vaults_of(t).iter().map(|v| v.crystals).sum::<f32>() as f64;
                self.books.raid_expected += expected as f64;
                self.books.raid_hulls += q.hulls as f64;
                let terms = Terms {
                    t,
                    f: super::RAID,
                    r: super::realm::NONE,
                    defend: false,
                    worth: expected,
                    expected,
                    expected_ticks,
                };
                let foes: u32 = (0..super::PEOPLES).map(|g| self.systems[t].hulls[g]).sum();
                let fire: f32 = (0..super::PEOPLES)
                    .filter(|&g| self.systems[t].hulls[g] > 0)
                    .map(|g| self.firing(t, g))
                    .sum();
                self.sign(q, &offers, &terms);
                let b = &mut self.books;
                b.raid_crew[0] += q.crew as f64;
                b.raid_crew[1] += self.companies[q.company].pilots.len() as f64;
                b.raid_foes[0] += foes as f64;
                b.raid_fire[0] += fire as f64;
            }
        }
    }

    /// Each realm in a company's debt pays some of it back, out of its income.
    fn collect_debts(&mut self) {
        let mut debts = std::mem::take(&mut self.debts);
        for d in &mut debts {
            let creditor = self.companies.iter().position(|c| c.id == d.company);
            let realm = &mut self.realms[d.realm as usize];
            let Some(c) = creditor.filter(|_| realm.systems > 0) else {
                self.books.written_off += d.crystals as f64;
                d.crystals = 0.0;
                continue;
            };
            let pay = d.crystals.min(d.due).min(realm.crystals.max(0.0));
            realm.crystals -= pay;
            d.crystals -= pay;
            self.companies[c].crystals += pay;
            self.companies[c].earned += pay;
            self.books.repaid += pay as f64;
        }
        debts.retain(|d| d.crystals > 1e-3);
        self.debts = debts;
    }

    /// After the tick's battles: contracts that
    /// are done end, idle companies' margins thin, debts are paid down; and every few
    /// ticks companies split and join.
    pub(crate) fn mercs(&mut self) {
        self.learn();
        self.collect_debts();
        self.pay_crews();
        self.pilot_market();
        for c in 0..self.companies.len() {
            let co = &mut self.companies[c];
            let Some(mut k) = co.contract else {
                continue;
            };
            let (t, f) = (k.target as usize, k.side as usize);
            if self.tick < k.arrives {
                // On its way; nothing to do there if its employer's gone.
                if f != super::RAID && self.realms[k.realm as usize].systems == 0 {
                    self.call_off(c);
                }
                continue;
            }
            // Never more than its side has there. What it committed is what
            // it's lost plus what it has left (#429).
            let before = k.field;
            k.field = k.field.min(self.systems[t].hulls[f]);
            k.lost += before - k.field;
            self.merc_moves[t][f].lost += before - k.field;
            // What a tick of it cost the company, for its next guess: the
            // share of its hulls lost (in the battles too), its kills.
            let lost_now = k.lost.saturating_sub(self.companies[c].lost_before);
            self.companies[c].lost_before = k.lost;
            let fielded = (k.field + lost_now).max(1) as f32;
            let co = &mut self.companies[c];
            co.loss_rate += RATE_FADE * (lost_now as f32 / fielded - co.loss_rate);
            co.kill_rate += RATE_FADE * (co.kills as f32 / fielded - co.kill_rate);
            co.kills = 0;
            if f == super::RAID {
                // Raiding for itself: off once it's got what's there, or it's
                // lost most of what it brought, or it's taking too long.
                let got = self.vaults_of(t).iter().all(|v| v.crystals < 1.0);
                let spent = k.field * 4 < k.committed;
                if got || spent || self.tick >= k.arrives + self.rules.merc_term as u64 {
                    self.books.raids_won += got as u64;
                    self.books.raid_spent += (!got && spent) as u64;
                    self.books.raid_late += (!got && !spent) as u64;
                    for g in 0..super::PEOPLES {
                        if self.systems[t].hulls[g] > 0 {
                            self.books.raid_foes[1] += self.systems[t].hulls[g] as f64;
                            self.books.raid_fire[1] += self.firing(t, g) as f64;
                        }
                    }
                    self.books.raid_lost += k.lost as f64;
                    self.books.raid_ticks += (self.tick - k.arrives) as f64;
                    self.leave(c, k);
                } else {
                    self.companies[c].contract = Some(k);
                }
                continue;
            }
            // The tick's bill, on the realm's account.
            k.billed += k.rate;
            self.books.billed += k.rate as f64;
            self.lend(k.realm, c, k.rate);
            self.settle_up(k.realm, c, k.rate);
            let sys = &self.systems[t];
            // A realm that folds into another, or loses the attack's ground
            // to one breaking away, hands the contract on to it.
            if sys.realms[f] != super::realm::NONE {
                k.realm = sys.realms[f];
            }
            let gone = self.realms[k.realm as usize].systems == 0 || sys.realms[f] != k.realm;
            // An attack is won when the world falls; a defense when the
            // attackers are gone and the world's still held. A defense ends
            // too if the world falls anyway.
            let won = !gone
                && sys.held[f] > 0
                && if k.defend {
                    (0..SIDES).all(|g| g == f || sys.hulls[g] < AT_WAR)
                } else {
                    sys.owner() == f
                };
            let fell = k.defend && sys.held[f] == 0;
            // Spent: every hull it committed is gone (#402, #429). Not
            // most of them: a company that commits x hulls fights with all
            // x, down to the last.
            let broken = k.field == 0;
            let expired = self.tick >= k.arrives + self.rules.merc_term as u64;
            if !(won || broken || expired || gone || fell) {
                self.companies[c].contract = Some(k);
                continue;
            }
            if won {
                let after = ((self.tick - k.arrives) / 5).min(WON_AFTER as u64 - 1) as usize;
                self.books.won_after[after] += 1;
                self.books.won += 1;
                self.books.defended += k.defend as u64;
                self.books.worth_won += self.worth(t, f, true) as f64;
                self.books.paid_won += (k.upfront + k.billed) as f64;
            }
            self.books.expected_ticks += k.expected_ticks as f64;
            self.books.realized_ticks += (self.tick - k.arrives) as f64;
            // What it came to, the realm reckons once the term it forecast
            // is up (`review`); how long it paid for, now.
            {
                let i = k.defend as usize;
                self.books.by_kind[i][2] += k.expected_ticks as f64;
                self.books.by_kind[i][3] += (self.tick - k.arrives) as f64;
                self.books.by_kind[i][4] += 1.0;
                self.books.by_kind[i][5] += broken as u8 as f64;
                if k.expected_ticks > 0.0 {
                    let took = (self.tick - k.arrives) as f32;
                    self.trust_ticks +=
                        LEARN * ((took / k.expected_ticks).min(5.0) - self.trust_ticks);
                }
                self.books.paid_all += (k.upfront + k.billed) as f64;
                // What a job makes a company, over its costs.
                let m = self.companies[c].margin;
                let made = (k.upfront + k.billed) * m / (1.0 + m);
                self.job_worth += LEARN * (made - self.job_worth);
            }
            if won {
                // (counted above)
            } else if broken {
                self.books.broken += 1;
            } else {
                self.books.expired += 1;
            }
            // If this is the player's company, record how the contract ended for the summary card (#414).
            let id = self.companies[c].id;
            if self.crew == Some(id) {
                let outcome = if won {
                    Outcome::Won
                } else if fell {
                    Outcome::Fell
                } else if broken {
                    Outcome::Spent
                } else if gone {
                    Outcome::Gone
                } else {
                    Outcome::Expired
                };
                let own = self
                    .own
                    .filter(|o| o.ship == id && o.signed == k.signed)
                    .unwrap_or_default();
                self.ended.push(Ended {
                    ship: id,
                    signed: k.signed,
                    tick: self.tick,
                    outcome,
                    defend: k.defend,
                    committed: k.committed,
                    lost: k.lost,
                    own,
                });
                let extra = self.ended.len().saturating_sub(ENDED_KEPT);
                self.ended.drain(..extra);
            }
            // Back aboard, what's left.
            let back = k.field.min(self.systems[t].hulls[f]);
            self.systems[t].hulls[f] -= back;
            self.moves[t][f].released += back;
            self.merc_moves[t][f].released += back;
            let co = &mut self.companies[c];
            co.hulls += back;
            co.contract = None;
            co.lost_before = 0;
            co.idle_since = self.tick;
        }
        self.raid();
        self.seek_work();
        self.reckon_profit();
        if self.tick.is_multiple_of(self.rules.politics.max(1) as u64) {
            self.split_and_join();
        }
    }

    /// What companies made this tick, all told: their capital and what
    /// they're owed, against last tick's.
    fn reckon_profit(&mut self) {
        let price = self.rules.hull_cost;
        let worth = self.companies.iter().map(|c| c.capital(price)).sum::<f32>()
            + self.debts.iter().map(|d| d.crystals).sum::<f32>();
        if self.worth_before > 0.0 {
            let made = worth - self.worth_before;
            self.books.profit += made as f64;
            let each = made / self.companies.len().max(1) as f32;
            self.profit += 0.01 * (each - self.profit);
        }
        self.worth_before = worth;
    }

    /// Idle companies too rich split in two; those too small join the
    /// nearest other (or, worth nothing and with nobody to join, are gone).
    pub(crate) fn split_and_join(&mut self) {
        let price = self.rules.hull_cost;
        let n = self.companies.len();
        for c in 0..n {
            let co = self.companies[c].clone();
            if co.contract.is_none() && co.capital(price) > self.rules.merc_split {
                let (crystals, hulls) = (co.crystals / 2.0, co.hulls / 2);
                self.companies[c].crystals -= crystals;
                self.companies[c].hulls -= hulls;
                let new = self.new_company(co.at, crystals, hulls);
                // Its pilots split between the two, every other one.
                let (mut keep, mut go) = (Vec::new(), Vec::new());
                for (i, &p) in co.pilots.iter().enumerate() {
                    if i % 2 == 0 { keep.push(p) } else { go.push(p) }
                }
                self.companies[c].pilots = keep;
                self.companies[new].pilots = go;
                self.companies[new].margin = (co.margin * self.rng.range(0.8, 1.25)).max(0.0);
                // Its pricing too, give or take a little.
                self.companies[new].pricing = Pricing {
                    k: co.pricing.k * self.rng.range(0.9, 1.1),
                    ..co.pricing
                };
                self.companies[new].patience = co.patience * self.rng.range(0.9, 1.1);
                self.companies[new].earned = co.earned / 2.0;
                self.companies[new].worked = co.worked / 2.0;
                self.companies[c].earned -= co.earned / 2.0;
                self.companies[c].worked -= co.worked / 2.0;
                self.companies[new].idle_since = co.idle_since;
                self.books.splits += 1;
            }
        }
        // Pooled savings found new ships, where there's war, with ways of
        // their own.
        // Nobody founds one while those there are lose money, unless there's
        // none in range of where it's wanted.
        // Where help's most wanted, and whether there's any about: a company
        // not at work, with something to work with.
        let price = self.rules.hull_cost;
        let wanted = (0..self.systems.len())
            .filter(|&s| self.unmet[s] >= 1.0)
            .max_by(|&a, &b| self.unmet[a].total_cmp(&self.unmet[b]));
        if self.savings > self.rules.merc_capital
            && self.rules.companies > 0
            && let Some(s) = wanted
        {
            let pos = self.systems[s].pos;
            let about = self.companies.iter().any(|c| {
                c.capital(price) >= self.rules.merc_join
                    && self.systems[c.at as usize].pos.distance(pos) <= self.rules.merc_range
            });
            if self.profit > 0.0 || !about {
                self.savings -= self.rules.merc_capital;
                self.new_company(s as u32, self.rules.merc_capital, 0);
                self.books.founded += 1;
                self.worth_before += self.rules.merc_capital;
            }
        }
        let mut gone = vec![false; self.companies.len()];
        for c in 0..self.companies.len() {
            let co = self.companies[c].clone();
            if gone[c]
                || co.contract.is_some()
                || (co.capital(price) + self.lent(co.id)) >= self.rules.merc_join
            {
                continue;
            }
            let here = self.systems[co.at as usize].pos;
            let into = (0..self.companies.len())
                .filter(|&o| o != c && !gone[o])
                .min_by(|&a, &b| {
                    let da = self.systems[self.companies[a].at as usize]
                        .pos
                        .distance(here);
                    let db = self.systems[self.companies[b].at as usize]
                        .pos
                        .distance(here);
                    da.total_cmp(&db)
                });
            let Some(o) = into else { continue };
            let d = self.systems[self.companies[o].at as usize]
                .pos
                .distance(here);
            // Its hulls go over if it can pay their way; its crystal always does.
            let fare = self.voyage(co.hulls, d);
            let (crystals, hulls) = if fare <= co.crystals {
                self.books.fares += fare as f64;
                (co.crystals - fare, co.hulls)
            } else {
                (co.crystals, 0)
            };
            self.companies[o].crystals += crystals;
            self.companies[o].hulls += hulls;
            // Joined, the ship has both crews.
            self.companies[o].pilots.extend(co.pilots.iter().copied());
            // What realms owed it, they owe the company it joined.
            let (from, to) = (co.id, self.companies[o].id);
            for d in self.debts.iter_mut().filter(|d| d.company == from) {
                d.company = to;
            }
            // And you go with it.
            if self.crew == Some(from) {
                self.crew = Some(to);
            }
            gone[c] = true;
            self.books.joins += 1;
        }
        let mut i = 0;
        self.companies.retain(|_| {
            i += 1;
            !gone[i - 1]
        });
    }

    /// Each company idle `Rules::merc_idle` ticks sails to the nearest
    /// system where realms have lately looked for a company and found none
    /// in range, if the fare's less than a contract earns a company, on
    /// average. Returns how many went.
    pub(crate) fn seek_work(&mut self) -> usize {
        for u in &mut self.unmet {
            *u *= 1.0 - UNMET_FADE;
        }
        let wanted: Vec<usize> = (0..self.systems.len())
            .filter(|&s| self.unmet[s] >= 1.0)
            .collect();
        let mut went = 0;
        for c in 0..self.companies.len() {
            let co = self.companies[c].clone();
            if co.contract.is_some() || self.tick < co.idle_since + self.rules.merc_idle as u64 {
                continue;
            }
            let here = self.systems[co.at as usize].pos;
            let d = |s: usize| self.systems[s].pos.distance(here);
            let nearest = |of: &[usize]| {
                of.iter()
                    .copied()
                    .filter(|&s| s != co.at as usize)
                    .min_by(|&a, &b| d(a).total_cmp(&d(b)))
            };
            // Where it's wanted, if a contract there would pay the fare.
            let wanted =
                nearest(&wanted).filter(|&s| self.voyage(co.hulls, d(s)) <= self.job_worth);
            let Some(to) = wanted else {
                continue;
            };
            let fare = self.voyage(co.hulls, d(to)).min(co.crystals.max(0.0));
            let fly = self.flight(d(to));
            self.books.sailing += fare as f64;
            let co = &mut self.companies[c];
            co.crystals -= fare;
            co.at = to as u32;
            co.idle_since = self.tick + fly;
            self.books.sailed += 1;
            went += 1;
        }
        went
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pilot drives the hull and turret they can build best (#481);
    /// knowing nothing, the Company's starter kit.
    #[test]
    fn pilots_drive_what_they_know_best() {
        use crate::economy::Known;
        let fresh = Pilot::rookie(1);
        assert_eq!(fresh.kit(), (HullKind::Kataphrakt, TurretKind::Kontos));
        assert_eq!(fresh.level(), 0.0);
        // A Vigil known to T0.3 beats the starter hull at T0.
        let mut known = Known::default();
        known.set(Part::Hull(HullKind::Vigil), 3.4);
        known.set(Part::Turret(TurretKind::Hymn), 0.9);
        let p = Pilot {
            known,
            ..Pilot::rookie(1)
        };
        assert_eq!(p.kit(), (HullKind::Vigil, TurretKind::Hymn));
        assert_eq!(p.level(), 1.5);
        // On a tie in whole steps, the one known furthest into the next.
        known.set(Part::Turret(TurretKind::Kontos), 2.0);
        known.set(Part::Turret(TurretKind::Knell), 2.7);
        assert_eq!(
            Pilot {
                known,
                ..Pilot::rookie(1)
            }
            .kit()
            .1,
            TurretKind::Knell
        );
        known.set(Part::Hull(HullKind::Gravis), 3.8);
        assert_eq!(
            Pilot {
                known,
                ..Pilot::rookie(1)
            }
            .kit()
            .0,
            HullKind::Gravis
        );
    }
}

/// What buying `n` hulls from `offers`, cheapest first, costs.
fn offers_spend(offers: &[Offer], mut n: u32) -> f32 {
    let mut spend = 0.0;
    for o in offers {
        if n == 0 {
            break;
        }
        let m = o.hulls.min(n);
        spend += m as f32 * o.price;
        n -= m;
    }
    spend
}
