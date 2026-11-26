//! Mercenary companies: capital, hired by realms (#318).
//!
//! A **company** is a single ship over some system, with crystal and hulls
//! aboard. It isn't a side and holds no ground: it fights inside a realm's
//! attack, as that realm's people, for pay.
//!
//! **Hiring.** A realm that wants a world (on its offensive, or an enemy
//! pocket) but can't take it alone, or that can't hold one of its own
//! against an attack (#340), asks the companies within
//! `Rules::merc_range` for a quote to make up the difference, and signs the
//! cheapest it can afford and that's worth it: what the world turns out over
//! `Rules::merc_horizon` ticks (crystal, and hulls at the going price) must
//! cover the price. Each side of a fight can have one company there.
//!
//! **A quote** is what the company puts in, marked up by its margin:
//! - the hulls it needs beyond what it has aboard, bought from whichever
//!   realms near the target sell cheapest (a realm sells only spare hulls from
//!   a quiet system, never to an attack on itself; it asks more the further
//!   below its garrison's level the system is), plus their FTL to the target;
//! - its own FTL there: the ship, and every hull aboard, by distance.
//!
//! A contract pays that **up front**, plus a **bonus** if the world falls,
//! for the hulls the company commits. It fights with all of them: once
//! they're spent, or after `Rules::merc_term` ticks, or with its employer
//! gone, it's done, and takes back aboard what's left of its hulls. The bonus comes out of the
//! employer's vaults when the world falls (or, for a defense, when the attack
//! is beaten off). An employer who can't pay it all pays what it has and
//! **owes the rest** (#340): the company takes `Rules::merc_dues` of the
//! realm's income each tick until it's paid, and no company works for a
//! realm while it owes one. A debt dies with the realm; when the company
//! joins another, the debt goes with it.
//!
//! **The market.** Each company's margin rises a little each time it wins a
//! contract and falls while it waits for one, so where companies crowd,
//! margins thin.
//!
//! **Splitting and joining** (every `Rules::politics` ticks, idle companies
//! only): one ship works one contract at a time, so a company worth more than
//! `Rules::merc_split` splits in two (off for now); one worth less than `Rules::merc_join`
//! can't change a fight, so it joins the nearest company (paying the FTL).
//! A split halves the crew; a join adds both crews together.
//! The pilot's company (`Galaxy::crew`) keeps its id when it splits, and
//! the pilot goes with it when it joins another; either way, the pilot then
//! chooses which ship to go with (`Galaxy::crew_choice`, #364): either half
//! of a split, or, on a join, the company it joined or the next nearest.
//!
//! **Looking for work** (#355). A company idle `Rules::merc_idle` ticks
//! sails to the nearest other system at war (one with a side no company
//! works for yet, if there's any), paying the FTL as far as its crystal
//! goes, and waits there. Nobody stays idle forever: every war is somebody's
//! chance. The pilot's company is no different.

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

/// Fights alike, as realms remember them: attack or defense, by the odds in
/// hulls and by how hard its own fire.
pub const ODDS: usize = 9;
pub const FIRES: usize = 7;
pub const MEMORY: usize = 2 * ODDS * FIRES;

/// Fights a realm takes the forecast's word as being worth, before it's
/// seen any like it.
const PRIOR: f32 = 4.0;

/// A fight a realm looked for help with, to look back on.
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct Lesson {
    pub cell: usize,
    pub system: u32,
    pub side: u8,
    pub realm: u32,
    pub defend: bool,
    pub held: u8,
    pub due: u64,
}

/// Times a realm plays a fight through to guess how it'll go.
const ROLLS: usize = 8;

/// How far realms' trust in forecasts moves toward each contract's outcome.
const LEARN: f32 = 0.02;

/// Share of a pilot's recent earnings forgotten each tick.
const EARN_FADE: f32 = 0.02;

/// Share of a company's recent earnings and work forgotten each tick.
const FADE: f32 = 0.005;

/// Gear marks the wage curve is kept by (T0 to T3).
pub const MARKS: usize = crate::economy::MAX_MARK as usize + 1;

/// How well gear at `level` fights: its damage and armor together.
fn gear_value(level: f32) -> f32 {
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
            wallet: 0.0,
            owed: 0.0,
            wage: 0.0,
            idle: 0,
            earning: 0.0,
            home: None,
        }
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
        let (hull, turret) = self.kit();
        let l = self.levels();
        (l.of(Part::Hull(hull)) as f32 + l.of(Part::Turret(turret)) as f32) / 2.0
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
#[derive(Clone, Copy, Debug)]
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
    /// Hired to hold one of the employer's own worlds, not to take one.
    pub defend: bool,
    /// What the realm expected of it when it signed: the worth of the
    /// world it would hold at the end, and the ticks it would pay for.
    pub expected: f32,
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
    /// Of the rate (before its margin): wages, and hulls lost.
    wages: f32,
    losses: f32,
    /// Pilots it reckons it'll have, and what it'd pay one at T0.
    crew: u32,
    wage: f32,
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
        let field = self.contract.map_or(0, |c| c.field);
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
            self.new_company(at, crystals, 0);
        }
    }

    fn new_company(&mut self, at: u32, crystals: f32, hulls: u32) -> usize {
        self.next_company += 1;
        self.companies.push(Company {
            at,
            crystals,
            hulls,
            margin: self.rules.merc_margin,
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
        let pilots = (0..self.rules.crew).map(|_| self.new_pilot()).collect();
        let c = self.companies.len() - 1;
        self.companies[c].pilots = pilots;
        c
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
        // Retiring, and quitting the pool.
        let retire = self.rules.retire;
        let mut retired = 0;
        for co in &mut self.companies {
            let before = co.pilots.len();
            co.pilots.retain(|p| p.wallet < retire);
            retired += before - co.pilots.len();
        }
        let before = self.pool.len();
        self.pool.retain(|p| p.wallet < retire);
        retired += before - self.pool.len();
        self.books.retired += retired as u64;
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
        self.books.crew_ticks += self.pool.len() as f64;
        // How much of the time mercs are in work, as pilots see it.
        let mercs = self.pool.len() + self.companies.iter().map(|c| c.pilots.len()).sum::<usize>();
        if mercs > 0 {
            let now = self.employed as f32 / mercs as f32;
            self.employment += LEARN * (now - self.employment);
        }
        self.employed = 0;
        self.books.crew_realm_pay += (self.pool.len() as f32 * self.pay) as f64;
        self.fill_seats(true);
        // Out of work, a pilot goes home: waiting costs them, and ships
        // that need pilots draw them straight from the realms.
        let pay = self.pay;
        let pool = std::mem::take(&mut self.pool);
        for p in pool {
            if p.earning >= pay {
                self.pool.push(p);
                continue;
            }
            self.books.quit += 1;
            if let Some(r) = p.home
                && self.realms[r as usize].systems > 0
            {
                self.realms[r as usize].pilots += 1;
            }
        }
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
                let n = k.field.min(front);
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
        let reserve = 2.0 * pay;
        self.defect(&want, ease);
        // The free pilots, best first, each to the ship that bids most for
        // them, at its bid: a ship pays what it offers, so one that bids
        // too high pays for it.
        let mut pool = std::mem::take(&mut self.pool);
        pool.sort_by_cached_key(|p| -((p.level() * 2.0) as i32));
        let mut left = Vec::new();
        for mut p in pool {
            let Some((c, wage)) = self.clearing(&want, p.level(), reserve) else {
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
            let m = mark(p.level());
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
        let i = self.pays.partition_point(|x| 2.0 * x.0 <= wage);
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
                self.clearing(want, half, 2.0 * wage).map_or(0.0, |b| b.1)
            } else {
                0.0
            };
            let n = if room > 0 && realm.pilots > 0 && offer * 0.5 >= wage && offer > 0.0 {
                self.rng.binomial(realm.pilots, self.rules.defect).min(room)
            } else {
                0
            };
            let realm = &mut self.realms[r];
            realm.wage = if n > 0 {
                wage * (1.0 + self.rules.pay_raise)
            } else if !ease {
                wage
            } else {
                (wage * (1.0 - self.rules.pay_ease)).max(1e-6)
            };
            if n == 0 {
                continue;
            }
            room -= n;
            realm.pilots -= n;
            let maker = crate::faction::Faction::ALL[realm.faction as usize];
            for _ in 0..n {
                let mut p = self.new_pilot();
                p.home = Some(r as u32);
                for part in crate::economy::PARTS
                    .into_iter()
                    .filter(|p| p.maker() == maker)
                {
                    p.known.set(part, half);
                }
                self.pool.push(p);
            }
            self.books.defected += n as u64;
        }
    }

    /// Crystal to move a ship carrying `hulls` hulls `d` light years.
    pub(super) fn voyage(&self, hulls: u32, d: f32) -> f32 {
        d / FTL_RANGE * (self.rules.ship_cost + hulls as f32 * self.rules.ftl_cost)
    }

    /// System `t`'s worth to side `f` over the horizon: what its production
    /// sites turn out a tick, crystal and hulls (at the going price), those
    /// `f` holds (`own`) or those it doesn't (what taking it would gain).
    fn worth(&self, t: usize, f: usize, own: bool) -> f32 {
        let (a, b) = self.systems[t].sites;
        let income: f32 = self.sites[a as usize..b as usize]
            .iter()
            .filter(|s| (s.owner as usize == f) == own)
            .map(|s| match s.kind {
                SiteKind::Production { hulls, crystals } => {
                    crystals + hulls * self.rules.yards * self.rules.hull_price
                }
                SiteKind::Shield { .. } => 0.0,
            })
            .sum();
        income * self.rules.merc_horizon
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
            // Short of its usual garrison, it asks more.
            let level = self.production(s)[g] * self.rules.garrison;
            let ask = self.rules.hull_price * (level / sys.hulls[g].max(1) as f32).clamp(0.5, 4.0);
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
    fn quote(&self, c: usize, size: u32, offers: &[Offer], pos: glam::Vec3, pay: f32) -> Quote {
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
        let cost = spend + self.voyage(co.hulls, d);
        let unit = if bought > 0 {
            spend / bought as f32
        } else {
            self.rules.hull_price
        };
        // A pilot for each hull that can fire: those aboard, and as many as
        // will sign for its bid (no less than pilots take).
        let seats = hulls.min(self.rules.frontage as u32) as usize;
        let aboard = co.pilots.len().min(seats);
        let mut wages: f32 = co.pilots.iter().take(aboard).map(|p| p.wage).sum();
        let missing = (seats - aboard) as u32;
        let wage = self.bid(c, 0.0, (missing, co.crystals), pay).max(2.0 * pay);
        let hired = (missing as f32).min(self.supply(wage));
        wages += hired * wage;
        let crew = aboard as u32 + hired as u32;
        let losses = co.loss_rate * hulls as f32 * unit;
        let kills = co.kill_rate * wages * self.rules.kill_share;
        let up = 1.0 + co.margin;
        Quote {
            company: c,
            hulls,
            bought,
            cost,
            upfront: cost * up,
            rate: (wages + losses + kills) * up,
            wages,
            losses,
            crew,
            wage,
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

    /// The gear of the pilots a ship could take on: the free ones', on
    /// average, else half what realms fly.
    fn free_level(&self) -> f32 {
        if self.pool.is_empty() {
            let n = self.living().count().max(1) as f32;
            return self.living().map(|(_, r)| r.tier as f32 / 2.0).sum::<f32>() / n;
        }
        self.pool.iter().map(|p| p.level()).sum::<f32>() / self.pool.len() as f32
    }

    /// Which fights this one's like (`memory`): side `f` with `ours` hulls
    /// in system `t`, `firing` of them at once (each by its gear).
    fn alike(&self, t: usize, f: usize, ours: f32, firing: f32, defend: bool) -> usize {
        let enemy: f32 = (0..SIDES)
            .filter(|&g| g != f)
            .map(|g| self.systems[t].hulls[g] as f32)
            .sum();
        let odds = ((ours + 1.0) / (enemy + 1.0)).log2() * 1.5 + ODDS as f32 / 2.0;
        let odds = (odds.max(0.0) as usize).min(ODDS - 1);
        let fire = (firing / self.rules.frontage * 4.0).round();
        let fire = (fire.max(0.0) as usize).min(FIRES - 1);
        (defend as usize * ODDS + odds) * FIRES + fire
    }

    /// What's come of fights like `cell`: the share of the world held a term
    /// on, starting from what `forecast` says until there's evidence.
    fn remembered(&self, cell: usize, forecast: f32) -> f32 {
        let (n, sum) = self.memory[cell];
        (sum + PRIOR * forecast) / (n + PRIOR)
    }

    /// Fights whose term is up, looked back on.
    fn learn(&mut self) {
        let tick = self.tick;
        let (due, wait): (Vec<Lesson>, Vec<Lesson>) = std::mem::take(&mut self.lessons)
            .into_iter()
            .partition(|l| tick >= l.due);
        self.lessons = wait;
        for l in due {
            self.learning.remove(&(l.realm, l.system));
            let sys = &self.systems[l.system as usize];
            let total = (sys.sites.1 - sys.sites.0).max(1) as f32;
            let (now, then) = (sys.held[l.side as usize] as f32, l.held as f32);
            let got = if l.defend {
                now / then.max(1.0)
            } else {
                (now - then) / (total - then).max(1.0)
            };
            let m = &mut self.memory[l.cell];
            m.0 += 1.0;
            m.1 += got.clamp(0.0, 1.0);
            // Old fights fade.
            if m.0 > 400.0 {
                m.0 *= 0.5;
                m.1 *= 0.5;
            }
        }
    }

    /// What it'd cost realm `r` to bring those (`can_move`).
    fn move_cost(&self, t: usize, f: usize, r: u32, purse: f32) -> f32 {
        let cost: f32 = self
            .neighbors(t)
            .filter(|&(s, _)| self.systems[s].realms[f] == r)
            .map(|(s, d)| self.spare[s][f] as f32 * d / FTL_RANGE * self.rules.ftl_cost)
            .sum();
        cost.min(purse.max(0.0))
    }

    /// Whether realm `r` expects to gain by attacking system `t`: alone, or
    /// with what a company nearby would bring on credit.
    pub(crate) fn promising(&self, r: usize, t: usize) -> bool {
        let realm = &self.realms[r];
        let (f, id) = (realm.faction as usize, r as u32);
        let worth = self.worth(t, f, false);
        let own = self.systems[t].hulls[f] as f32 + self.can_move(t, f, id, realm.crystals);
        let front = self.rules.frontage;
        let tier = crate::economy::damage_factor(realm.tier);
        let flown = (realm.pilots as f32).min(own).min(front);
        let move_cost = self.move_cost(t, f, id, realm.crystals);
        let expect = |mercs: f32, crew: f32| {
            let cell = self.alike(t, f, own + mercs, (flown * tier).min(front) + crew, false);
            let (share, ticks) = self.forecast(t, f, id, own, mercs, crew, 0.0, false);
            (self.remembered(cell, share), ticks)
        };
        if expect(0.0, 0.0).0 * worth > move_cost {
            return true;
        }
        if !realm.hires {
            return false;
        }
        let (owed, income, pay) = (self.owes(id), realm.income.max(0.0), self.pay);
        let pos = self.systems[t].pos;
        (0..self.companies.len())
            .filter(|&c| {
                let co = &self.companies[c];
                co.contract.is_none()
                    && self.systems[co.at as usize].pos.distance(pos) <= self.rules.merc_range
            })
            .take(4)
            .any(|c| {
                let co = &self.companies[c];
                let offers = self.offers(t, f, id, co.at as usize);
                [100, 300, 900].into_iter().any(|size| {
                    let q = self.quote(c, size, &offers, pos, pay);
                    let (share, ticks) = expect(q.hulls as f32, q.crew as f32);
                    let price = q.upfront + q.rate * ticks * self.trust_ticks;
                    q.cost + q.wages * ticks <= co.crystals
                        && owed + price <= income * co.patience
                        && share * worth > price + move_cost
                })
            })
    }

    /// Several companies that together do realm `r` more good than `single`
    /// does: (company, hulls to ask, worth expected, ticks expected) of
    /// each; none if one's as good.
    #[allow(clippy::too_many_arguments)]
    fn bundle(
        &self,
        t: usize,
        f: usize,
        r: u32,
        need: u32,
        own: f32,
        defend: bool,
        worth: f32,
        flown: f32,
        alone: f32,
        single: f32,
    ) -> Option<Vec<(usize, u32, f32, f32)>> {
        let (pos, pay) = (self.systems[t].pos, self.pay);
        let (owed, income) = (self.owes(r), self.realms[r as usize].income.max(0.0));
        let front = self.rules.frontage;
        let tier = crate::economy::damage_factor(self.realms[r as usize].tier);
        let mut quotes: Vec<(f32, Quote)> = Vec::new();
        for c in 0..self.companies.len() {
            let co = &self.companies[c];
            if co.contract.is_some()
                || self.systems[co.at as usize].pos.distance(pos) > self.rules.merc_range
            {
                continue;
            }
            let offers = self.offers(t, f, r, co.at as usize);
            let q = self.quote(c, need.max(MIN_SEND), &offers, pos, pay);
            if q.hulls >= MIN_SEND && q.cost + q.wages <= co.crystals {
                quotes.push(((q.upfront + q.rate) / q.hulls as f32, q));
            }
        }
        quotes.sort_by(|a, b| a.0.total_cmp(&b.0));
        quotes.truncate(6);
        let (mut hulls, mut crew, mut upfront, mut rate) = (0.0, 0.0f32, 0.0, 0.0);
        let mut best: Option<(f32, usize, f32, f32)> = None;
        for (k, (_, q)) in quotes.iter().enumerate() {
            hulls += q.hulls as f32;
            crew = (crew + q.crew as f32).min(front);
            upfront += q.upfront;
            rate += q.rate;
            if k == 0 {
                continue;
            }
            let cell = self.alike(
                t,
                f,
                own + hulls,
                flown.min((front - crew).max(0.0)) * tier + crew,
                defend,
            );
            let (share, raw) = self.forecast(t, f, r, own, hulls, crew, 0.0, defend);
            let with = self.remembered(cell, share);
            let ticks = raw * self.trust_ticks;
            let price = upfront + rate * ticks;
            // Every lender has to be happy with all of it.
            if quotes[..=k]
                .iter()
                .any(|(_, q)| owed + price > income * self.companies[q.company].patience)
            {
                break;
            }
            let net = (with - alone) * worth - price;
            if net > single && best.is_none_or(|b| net > b.0) {
                best = Some((net, k, with * worth, raw));
            }
        }
        let (_, k, expected, raw) = best?;
        let n = (k + 1) as f32;
        Some(
            quotes[..=k]
                .iter()
                .map(|(_, q)| (q.company, q.hulls, expected / n, raw))
                .collect(),
        )
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
    /// set out to take, or to keep), and the ticks the mercs fight: until
    /// the world falls, they're spent, or the term's up.
    fn forecast(
        &self,
        t: usize,
        f: usize,
        r: u32,
        own: f32,
        mercs: f32,
        crew: f32,
        upfront: f32,
        defend: bool,
    ) -> (f32, f32) {
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
        // realm reckons by what it knows), the ground it holds here, and
        // its offensive's initiative.
        let tier = self.realms[r as usize].tier;
        let mut pf = 1.0 + self.rules.home_ground * mine0 / total;
        if self.pushing(r, sys.pos) {
            pf *= 1.0 + self.rules.initiative;
        }
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
        let (mut spare0, mut odds) = (0.0f32, self.rules.opportunity);
        for g in (0..SIDES).filter(|&g| g != f) {
            let e = sys.realms[g];
            if e == super::realm::NONE {
                continue;
            }
            spare0 += self.can_move(t, g, e, self.realms[e as usize].crystals);
            if sys.held[g] > 0 || self.pushing(e, sys.pos) {
                odds = self.rules.odds;
            }
        }
        let reinforce = |he: &mut f32, spare: &mut f32, ours: f32| {
            let against = odds * ours * pf;
            if *spare < MIN_SEND as f32 || *he * pe >= against || (*he + *spare) * pe < against {
                return;
            }
            let want = (against * 1.2 + 10.0) / pe.max(1e-3) - *he;
            let more = want.clamp(0.0, *spare);
            *he += more;
            *spare -= more;
        };
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
        let cut = self.shield_cut();
        // Of its own, those here and those it can pay to bring, once it's
        // paid the company.
        let here = (hulls[f] as f32).min(own);
        let own = here
            + (own - here).min(self.can_move(t, f, r, self.realms[r as usize].crystals - upfront));
        // Its own hulls fire only as far as its pilots go, shared between
        // its fights (`Galaxy::firing`); the mercs come with theirs.
        let realm = &self.realms[r as usize];
        let fights = self.war_hulls.get(r as usize).copied().unwrap_or(0) as f32 + own;
        let flown = realm.pilots as f32 * own / fights.max(1.0);
        let front = self.rules.frontage;
        let (k, take, blitz) = (self.rules.kill, self.rules.take, self.rules.blitz as f32);
        // The same dice for every way of fighting it, so they're compared
        // on the same luck.
        let seed = (self.tick << 24) ^ ((t as u64) << 8) ^ f as u64;
        let mut rng = crate::rng::Rng::from_state(seed | 1);
        let (mut held, mut paid) = (0.0, 0.0);
        for _ in 0..ROLLS {
            let (mut ho, mut hm, mut he, mut spare) = (own, mercs, he, spare0);
            let (mut mine, mut theirs, mut gens) = (mine0, theirs0, gens0);
            for _ in 0..self.rules.merc_term {
                let luck = (rng.range(0.75, 1.25), rng.range(0.75, 1.25));
                let roll = (rng.f32(), rng.f32());
                let hf = ho + hm;
                reinforce(&mut he, &mut spare, hf);
                if hm >= 1.0 {
                    paid += 1.0;
                }
                // A side left alone walks into what's unguarded.
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
                    let ff = (ho.min(flown) + hm.min(crew)).min(front) * pf * luck.0;
                    let fe = he.min(front) * pe * luck.1;
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
                    // Whole sites change hands: the expected number, and the
                    // fraction left over as a chance.
                    let whole = |x: f32, roll: f32| x.floor() + (roll < x.fract()) as u8 as f32;
                    let won = whole(take * at * open, roll.0).min(theirs);
                    let lost = whole(take * (1.0 - share) * mine, roll.1).min(mine);
                    gens = (gens - won).max(0.0);
                    let battles = open + mine;
                    mine += won - lost;
                    theirs += lost - won;
                    // Hulls are lost in each site's battle, the mercs' by
                    // their share of the side's.
                    let left = 1.0 - (battles * k * fe / af / hf).min(1.0);
                    ho *= left;
                    hm *= left;
                    he = (he - battles * k * ff / ae).max(0.0);
                }
                // Done, the mercs go, and it holds on alone.
                if hm >= 1.0
                    && if defend {
                        he < AT_WAR as f32
                    } else {
                        mine > theirs
                    }
                {
                    hm = 0.0;
                }
            }
            held += if defend {
                (mine / mine0.max(1.0)).min(1.0)
            } else {
                ((mine - mine0) / theirs0.max(1.0)).max(0.0)
            };
        }
        let n = ROLLS as f32;
        (held / n, (paid / n).max(1.0))
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

    /// Realm `r` (of side `f`) needs `need` more hulls to go into system `t`
    /// (to take it, or to hold it: `defend`), with `own` hulls of its own
    /// to fight there with: it hires a company if one will do it for a
    /// price worth paying. Returns whether it did.
    pub(crate) fn hire(
        &mut self,
        t: usize,
        f: usize,
        r: u32,
        need: u32,
        own: f32,
        defend: bool,
    ) -> bool {
        #[cfg(test)]
        if self.ask_log {
            self.asks.push((r, t as u32, f, defend));
        }
        #[cfg(test)]
        if self.ban == Some((r, t as u32)) {
            return false;
        }
        if !self.realms[r as usize].hires || self.companies.is_empty() {
            return false;
        }
        let (owed, income) = (self.owes(r), self.realms[r as usize].income.max(0.0));
        // What the world is worth: what taking it gains, or what losing it would cost.
        let worth = self.worth(t, f, defend);
        if worth <= 0.0 {
            return false;
        }
        let pos = self.systems[t].pos;
        if self.pays_tick != self.tick {
            self.pays_tick = self.tick;
            self.tally_pays();
        }
        self.books.asked += 1;
        let near: Vec<usize> = (0..self.companies.len())
            .filter(|&c| {
                let co = &self.companies[c];
                co.contract.is_none()
                    && self.systems[co.at as usize].pos.distance(pos) <= self.rules.merc_range
            })
            .collect();
        if near.is_empty() {
            self.books.none_near += 1;
            // Word gets round: there's work here for a company.
            self.unmet[t] += 1.0;
            return false;
        }

        let (mut able, mut affordable) = (false, false);
        let (mut most, mut sale) = (0, 0u32);
        let mut best: Option<(Quote, f32, f32, f32, Vec<Offer>, usize)> = None;
        let pay = self.realm_pay();
        // Each company quotes for a few sizes of the job; the realm takes
        // whichever does it the most good for its price, if any does.
        let alone = self.forecast(t, f, r, own, 0.0, 0.0, 0.0, defend).0;
        let front = self.rules.frontage;
        let flown = {
            let here = &self.realms[r as usize];
            let fights = self.war_hulls.get(r as usize).copied().unwrap_or(0) as f32 + own;
            (here.pilots as f32 * own / fights.max(1.0))
                .min(own)
                .min(front)
        };
        let tier = crate::economy::damage_factor(self.realms[r as usize].tier);
        let alone_cell = self.alike(t, f, own, flown * tier, defend);
        let alone = self.remembered(alone_cell, alone);
        let mut lesson = Lesson {
            cell: alone_cell,
            system: t as u32,
            side: f as u8,
            realm: r,
            defend,
            held: self.systems[t].held[f],
            due: self.tick + self.rules.merc_term as u64,
        };

        let rest = crate::economy::damage_factor(self.free_level().round() as u8);
        for c in near {
            let mut best_levels: Vec<f32> = self.companies[c]
                .pilots
                .iter()
                .map(|p| crate::economy::damage_factor(p.level().round() as u8))
                .collect();
            best_levels.sort_by(|a, b| b.total_cmp(a));
            let offers = self.offers(t, f, r, self.companies[c].at as usize);
            sale = sale.max(offers.iter().map(|o| o.hulls).sum());
            for share in [0.05, 0.1, 0.25, 0.5, 1.0] {
                let size = ((need as f32 * share).ceil() as u32).max(MIN_SEND);
                let quote = self.quote(c, size, &offers, pos, pay);
                most = most.max(quote.hulls);
                if quote.hulls < MIN_SEND {
                    continue;
                }
                able = true;
                // Not worth a guess if taking it all wouldn't pay for a tick.
                #[cfg(test)]
                let forced = self.force.is_some();
                #[cfg(not(test))]
                let forced = false;
                let least = quote.upfront + quote.rate * self.trust_ticks;
                if !forced && (1.0 - alone) * worth <= least {
                    affordable = true;
                    continue;
                }
                // The realm's guess: how much more of the world it gets
                // with the company than without, and for how long it pays.
                let (with, raw_ticks) = self.forecast(
                    t,
                    f,
                    r,
                    own,
                    quote.hulls as f32,
                    quote.crew as f32,
                    0.0,
                    defend,
                );
                // The company's pilots fire by their own gear.
                let crew = (quote.crew.min(quote.hulls) as f32).min(front);
                let n = (crew as usize).min(best_levels.len());
                let known: f32 = best_levels[..n].iter().sum();
                // Those it's still to hire, it reckons at what's going.
                let kit = (known + (crew - n as f32) * rest) / crew.max(1.0);
                let cell = self.alike(
                    t,
                    f,
                    own + quote.hulls as f32,
                    flown.min((front - crew).max(0.0)) * tier + crew * kit,
                    defend,
                );
                let with = self.remembered(cell, with);

                // Discounted by how forecasts have lately come out: realms
                // learn from what contracts got them, and how long they ran.
                let ticks = raw_ticks * self.trust_ticks;
                let gain = with - alone;
                // All of it on credit: the company pays its way out of its
                // own crystal, and lends only what the realm's income would
                // clear, with all it owes, in the ticks the company waits.
                let co = &self.companies[c];
                let price = quote.upfront + quote.rate * ticks;
                if quote.cost + quote.wages * ticks > co.crystals {
                    continue;
                }
                affordable = true;
                if owed + price > income * co.patience {
                    self.books.refused += 1;
                    continue;
                }
                let net = gain * worth - price;
                #[cfg(test)]
                let net = if self.force == Some((r, t as u32)) {
                    if self.small {
                        1e6 - quote.hulls as f32
                    } else {
                        1.0 + quote.hulls as f32
                    }
                } else {
                    if with <= 0.01 && share == 1.0 {
                        self.lost_calls.push((r, t as u32, f, defend, worth));
                    }
                    net
                };
                if net <= 0.0 {
                    let b = &mut self.books;
                    if gain <= 0.01 {
                        b.hopeless += 1;
                        if alone >= 0.99 {
                            b.won_alone += 1;
                        } else if with <= 0.01 {
                            b.lost_anyway += 1;
                        }
                    } else {
                        b.dear += 1;
                        b.dear_gain += (gain * worth) as f64;
                        b.dear_price += price as f64;
                        b.dear_wages += quote.rate as f64 * ticks as f64;
                        b.dear_upfront += quote.upfront as f64;
                        b.dear_wage_part += quote.wages as f64;
                        b.dear_loss_part += quote.losses as f64;
                        b.dear_hulls += quote.hulls as f64;
                    }
                    continue;
                }
                if best.as_ref().is_none_or(|b| net > b.1) {
                    best = Some((quote, net, with * worth, raw_ticks, offers.clone(), cell));
                }
            }
        }
        #[cfg(test)]
        let alone_guess = alone;
        if self.learning.insert((r, t as u32)) {
            if let Some(b) = &best {
                lesson.cell = b.5;
            }
            self.lessons.push(lesson);
        }
        // Or several at once, as one alone may not be enough: the cheapest by
        // the hull first, as many as does it the most good.
        let single = best.as_ref().map_or(0.0, |b| b.1);
        if let Some(bundle) = self.bundle(t, f, r, need, own, defend, worth, flown, alone, single) {
            for (c, size, expected, expected_ticks) in bundle {
                let offers = self.offers(t, f, r, self.companies[c].at as usize);
                let q = self.quote(c, size, &offers, pos, pay);
                if q.hulls < MIN_SEND || q.cost > self.companies[c].crystals {
                    continue;
                }
                let terms = Terms {
                    t,
                    f,
                    r,
                    defend,
                    worth,
                    expected,
                    expected_ticks,
                };
                self.sign(q, &offers, &terms);
            }
            return true;
        }
        let Some((q, _, expected, expected_ticks, offers, _)) = best else {
            if !able {
                self.books.none_able += 1;
                self.books.able_need += need as f64;
                self.books.able_best += most as f64;
                self.books.able_for_sale += sale as f64;
            } else if !affordable {
                self.books.none_affordable += 1;
            } else {
                self.books.none_worth += 1;
            }
            return false;
        };
        let terms = Terms {
            t,
            f,
            r,
            defend,
            worth,
            expected,
            expected_ticks,
        };
        #[cfg(test)]
        self.guesses
            .push((r, t as u32, alone_guess, expected / worth.max(1e-6)));
        self.sign(q, &offers, &terms);
        true
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
        self.lend(r, q.company, q.upfront);
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
        }
        let land = q.hulls;
        let spent: f32 = offers_spend(offers, q.bought);
        self.books.bought += spent as f64;
        self.books.flown += (q.cost - spent) as f64;
        let co = &mut self.companies[q.company];
        co.crystals -= q.cost;
        // Its own hulls go in first; the rest stay aboard.
        let own = co.hulls.min(q.hulls - q.bought);
        co.hulls -= own;
        co.at = t as u32;
        co.margin *= 1.0 + self.rules.merc_raise;
        co.contract = Some(Contract {
            realm: r,
            side: f as u8,
            target: t as u32,
            signed: self.tick,
            upfront: q.upfront,
            rate: q.rate,
            billed: 0.0,
            wage: q.wage,
            committed: q.hulls,
            field: land,
            lost: 0,
            defend,
            expected,
            expected_ticks,
            held: self.systems[t].held[f],
            worth,
        });
        self.systems[t].hulls[f] += land;
        self.moves[t][f].hired += land;
        self.merc_moves[t][f].hired += land;
        if self.systems[t].realms[f] == super::realm::NONE {
            self.systems[t].realms[f] = r;
        }
        self.books.signed += 1;
        self.books.defenses += defend as u64;
        // It takes on pilots for its hulls before the fight.
        self.fill_seats(false);
        self.books.upfront += q.upfront as f64;
    }

    /// Contracts whose term is up, looked back on by their realms: what
    /// they came to, the share of the world held now of what they set out
    /// to keep or take (the outcome their forecast was of). Realms learn
    /// from it how far to trust a forecast.
    fn review(&mut self) {
        let term = self.rules.merc_term as u64;
        let tick = self.tick;
        let (due, wait): (Vec<Contract>, Vec<Contract>) = std::mem::take(&mut self.reviews)
            .into_iter()
            .partition(|k| tick >= k.signed + term);
        self.reviews = wait;
        for k in due {
            let sys = &self.systems[k.target as usize];
            let total = (sys.sites.1 - sys.sites.0).max(1) as f32;
            let (now, then) = (sys.held[k.side as usize] as f32, k.held as f32);
            let share = if k.defend {
                now / then.max(1.0)
            } else {
                (now - then).max(0.0) / (total - then).max(1.0)
            };
            let got = share.clamp(0.0, 1.0) * k.worth;
            self.books.realized += got as f64;
            self.books.expected += k.expected as f64;
            let i = k.defend as usize;
            self.books.by_kind[i][0] += k.expected as f64;
            self.books.by_kind[i][1] += got as f64;
            if k.expected > 0.0 {
                let trust = &mut self.trust[i];
                *trust += LEARN * ((got / k.expected).min(2.0) - *trust);
            }
        }
    }

    /// Each company pays its crew what it owes them, and on a contract,
    /// the wages of those flying its hulls. Idle pilots aren't paid.
    fn pay_crews(&mut self) {
        for co in &mut self.companies {
            for p in &mut co.pilots {
                let x = p.owed.min(co.crystals.max(0.0));
                p.owed -= x;
                p.wallet += x;
                co.crystals -= x;
            }
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
            co.pilots
                .sort_by_cached_key(|p| -((p.level() * 2.0) as i32));
            let flying = (k.field.min(self.rules.frontage as u32) as usize).min(co.pilots.len());
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
        self.review();
        self.learn();
        self.collect_debts();
        self.pay_crews();
        self.pilot_market();
        for c in 0..self.companies.len() {
            let co = &mut self.companies[c];
            let Some(mut k) = co.contract else {
                co.margin = (co.margin * (1.0 - self.rules.merc_haggle)).max(self.rules.merc_floor);
                continue;
            };
            let (t, f) = (k.target as usize, k.side as usize);
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
            // The tick's bill, on the realm's account.
            k.billed += k.rate;
            self.books.billed += k.rate as f64;
            self.lend(k.realm, c, k.rate);
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
            let expired = self.tick >= k.signed + self.rules.merc_term as u64;
            if !(won || broken || expired || gone || fell) {
                self.companies[c].contract = Some(k);
                continue;
            }
            if won {
                let after = ((self.tick - k.signed) / 5).min(WON_AFTER as u64 - 1) as usize;
                self.books.won_after[after] += 1;
                self.books.won += 1;
                self.books.defended += k.defend as u64;
                self.books.worth_won += self.worth(t, f, true) as f64;
                self.books.paid_won += (k.upfront + k.billed) as f64;
            }
            self.books.expected_ticks += k.expected_ticks as f64;
            self.books.realized_ticks += (self.tick - k.signed) as f64;
            // What it came to, the realm reckons once the term it forecast
            // is up (`review`); how long it paid for, now.
            {
                self.reviews.push(k);
                let i = k.defend as usize;
                self.books.by_kind[i][2] += k.expected_ticks as f64;
                self.books.by_kind[i][3] += (self.tick - k.signed) as f64;
                self.books.by_kind[i][4] += 1.0;
                self.books.by_kind[i][5] += broken as u8 as f64;
                if k.expected_ticks > 0.0 {
                    let took = (self.tick - k.signed) as f32;
                    self.trust_ticks +=
                        LEARN * ((took / k.expected_ticks).min(5.0) - self.trust_ticks);
                }
                self.books.paid_all += (k.upfront + k.billed) as f64;
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
        self.seek_work();
        if self.tick.is_multiple_of(self.rules.politics.max(1) as u64) {
            self.split_and_join();
        }
    }

    /// Idle companies too rich split in two; those too small join the
    /// nearest other (or, worth nothing and with nobody to join, are gone).
    pub(crate) fn split_and_join(&mut self) {
        let price = self.rules.hull_price;
        let n = self.companies.len();
        for c in 0..n {
            let co = self.companies[c].clone();
            if co.contract.is_none()
                && (co.capital(price) + self.lent(co.id)) > self.rules.merc_split
            {
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
                self.companies[new].margin = co.margin;
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
                if self.crew == Some(co.id) {
                    self.crew_choice = Some(super::theater::CrewChoice {
                        from: co.id,
                        joined: false,
                        ships: [co.id, self.companies[new].id],
                    });
                }
                self.books.splits += 1;
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
            // And you go with it, or (your choice) with the next nearest.
            if self.crew == Some(from) {
                self.crew = Some(to);
                self.crew_news = Some(super::theater::CrewNews::Joined { from });
                let next = (0..self.companies.len())
                    .filter(|&n| n != c && n != o && !gone[n])
                    .min_by(|&a, &b| {
                        let d = |n: usize| {
                            self.systems[self.companies[n].at as usize]
                                .pos
                                .distance(here)
                        };
                        d(a).total_cmp(&d(b))
                    });
                self.crew_choice = next.map(|n| super::theater::CrewChoice {
                    from,
                    joined: true,
                    ships: [to, self.companies[n].id],
                });
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
        // What companies have lately been making.
        let n = self.companies.len().max(1) as f32;
        let earns = self.companies.iter().map(|c| c.earned).sum::<f32>() / n;
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
                nearest(&wanted).filter(|&s| self.voyage(co.hulls, d(s)) <= earns.max(0.0));
            let Some(to) = wanted else {
                continue;
            };
            let fare = self.voyage(co.hulls, d(to)).min(co.crystals.max(0.0));
            let from = co.at;
            self.books.sailing += fare as f64;
            let co = &mut self.companies[c];
            co.crystals -= fare;
            co.at = to as u32;
            co.idle_since = self.tick;
            if self.crew == Some(co.id) {
                self.crew_news = Some(super::theater::CrewNews::Sailed { from });
            }
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
