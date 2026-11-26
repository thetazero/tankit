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
}

impl Pilot {
    /// A pilot who knows nothing yet.
    pub fn rookie(id: u32) -> Pilot {
        Pilot {
            id,
            known: crate::economy::Known::default(),
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
    pub upfront: f32,
    pub bonus: f32,
    /// The hulls it committed to the contract, all told.
    pub committed: u32,
    /// Its hulls in the fight now, and lost so far.
    pub field: u32,
    pub lost: u32,
    /// Hired to hold one of the employer's own worlds, not to take one.
    pub defend: bool,
}

/// A bonus a realm didn't pay: it owes `crystals` to company `company` (a `Company::id`).
#[derive(Clone, Copy, Debug)]
pub struct Debt {
    pub realm: u32,
    pub company: u32,
    pub crystals: f32,
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
    /// Crystal realms paid companies up front and in bonuses, and bonuses
    /// they couldn't pay when due; of those, paid back later, and written off
    /// (the realm gone, or the company).
    pub upfront: f64,
    pub bonus: f64,
    pub defaulted: f64,
    pub repaid: f64,
    pub written_off: f64,
    /// Quotes a company would have made but didn't, to a realm in its debt.
    pub refused: u64,
    /// Crystal realms got for hulls they sold companies.
    pub sold: f64,
    /// Hulls companies lost in fights.
    pub lost: f64,
    /// Of the contracts won: the worlds' worth to their employers (what they
    /// over the horizon) and what they paid for them.
    pub worth_won: f64,
    pub paid_won: f64,
    /// Companies split off and joined into others.
    pub splits: u64,
    pub joins: u64,
    /// Voyages idle companies made to look for work.
    pub sailed: u64,
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
    bonus: f32,
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

    /// A new rookie with a fresh id.
    pub(crate) fn new_pilot(&mut self) -> Pilot {
        self.next_pilot += 1;
        Pilot::rookie(self.next_pilot)
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
                    crystals + hulls * self.rules.hull_price
                }
                SiteKind::Shield { .. } => 0.0,
            })
            .sum();
        income * self.rules.merc_horizon
    }

    /// Hulls for sale within range of system `t`, to a company working for
    /// realm `r` of side `f`, cheapest first.
    fn offers(&self, t: usize, f: usize, r: u32) -> Vec<Offer> {
        let defenders = self.systems[t].realms;
        let mut out: Vec<Offer> = Vec::new();
        for (s, d) in self.neighbors(t) {
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
            // Short of the garrison it levels off at, it asks more.
            let level = self.production(s)[g] / self.rules.wear;
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

    /// Whether realm `r` owes any company a bonus.
    pub fn in_debt(&self, r: u32) -> bool {
        self.debts.iter().any(|d| d.realm == r)
    }

    /// Realm `r` (of side `f`) needs `need` more hulls to go into system `t`
    /// (to take it, or to hold it: `defend`): it hires a company if one will
    /// do it for a price worth paying. Returns whether it did.
    pub(crate) fn hire(&mut self, t: usize, f: usize, r: u32, need: u32, defend: bool) -> bool {
        if self.companies.is_empty()
            || self.companies.iter().any(|c| {
                c.contract
                    .is_some_and(|k| k.target as usize == t && k.side as usize == f)
            })
        {
            return false;
        }
        // Nobody works for a realm that hasn't paid what it owes.
        if self.in_debt(r) {
            self.books.refused += 1;
            return false;
        }
        // What the world is worth: what taking it gains, or what losing it would cost.
        let worth = self.worth(t, f, defend);
        let realm = &self.realms[r as usize];
        // Holding a world of its own may spend everything; an attack only
        // what's above the reserve.
        let purse = if defend {
            realm.crystals
        } else {
            realm.crystals - self.rules.reserve * realm.income.max(0.0)
        };
        if worth <= 0.0 || purse <= 0.0 {
            return false;
        }
        let pos = self.systems[t].pos;
        let near: Vec<usize> = (0..self.companies.len())
            .filter(|&c| {
                let co = &self.companies[c];
                co.contract.is_none()
                    && self.systems[co.at as usize].pos.distance(pos) <= self.rules.merc_range
            })
            .collect();
        if near.is_empty() {
            return false;
        }
        let offers = self.offers(t, f, r);
        let mut best: Option<Quote> = None;
        for c in near {
            let co = &self.companies[c];
            // Buy what it lacks, cheapest first.
            let mut short = need.saturating_sub(co.hulls);
            let (mut bought, mut spend) = (0, 0.0);
            for o in &offers {
                if short == 0 {
                    break;
                }
                let m = o.hulls.min(short);
                bought += m;
                spend += m as f32 * o.price;
                short -= m;
            }
            let hulls = co.hulls.min(need) + bought;
            if (hulls as f32) < need as f32 * 0.75 || hulls < MIN_SEND {
                continue;
            }
            let d = self.systems[co.at as usize].pos.distance(pos);
            let cost = spend + self.voyage(co.hulls, d);
            // What a hull it might lose would cost it to replace.
            let unit = if bought > 0 {
                spend / bought as f32
            } else {
                self.rules.hull_price
            };
            let up = 1.0 + co.margin;
            let quote = Quote {
                company: c,
                hulls,
                bought,
                cost,
                upfront: cost * up,
                bonus: self.rules.merc_bonus * hulls as f32 * unit * up,
            };
            // It has to be able to pay its way there (the upfront helps).
            if cost > co.crystals + quote.upfront
                || quote.upfront > purse
                || quote.upfront + quote.bonus > worth
            {
                continue;
            }
            if best.is_none_or(|b| quote.upfront + quote.bonus < b.upfront + b.bonus) {
                best = Some(quote);
            }
        }
        let Some(q) = best else { return false };
        // Sign: the realm pays up front, the company buys and goes.
        self.realms[r as usize].crystals -= q.upfront;
        let mut short = q.bought;
        for o in &offers {
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
        let co = &mut self.companies[q.company];
        co.crystals += q.upfront - q.cost;
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
            bonus: q.bonus,
            committed: q.hulls,
            field: land,
            lost: 0,
            defend,
        });
        self.systems[t].hulls[f] += land;
        self.moves[t][f].hired += land;
        self.merc_moves[t][f].hired += land;
        if self.systems[t].realms[f] == super::realm::NONE {
            self.systems[t].realms[f] = r;
        }
        self.books.signed += 1;
        self.books.defenses += defend as u64;
        self.books.upfront += q.upfront as f64;
        true
    }

    /// Each realm in a company's debt pays some of it back, out of its income.
    fn collect_debts(&mut self) {
        let dues = self.rules.merc_dues;
        let mut debts = std::mem::take(&mut self.debts);
        for d in &mut debts {
            let creditor = self.companies.iter().position(|c| c.id == d.company);
            let realm = &mut self.realms[d.realm as usize];
            let Some(c) = creditor.filter(|_| realm.systems > 0) else {
                self.books.written_off += d.crystals as f64;
                d.crystals = 0.0;
                continue;
            };
            let pay = d
                .crystals
                .min(dues * realm.income.max(0.0))
                .min(realm.crystals.max(0.0));
            realm.crystals -= pay;
            d.crystals -= pay;
            self.companies[c].crystals += pay;
            self.books.repaid += pay as f64;
        }
        debts.retain(|d| d.crystals > 1e-3);
        self.debts = debts;
    }

    /// After the tick's battles: each company's hulls wear, contracts that
    /// are done end, idle companies' margins thin, debts are paid down; and every few
    /// ticks companies split and join.
    pub(crate) fn mercs(&mut self) {
        let wear = self.rules.wear;
        self.collect_debts();
        for c in 0..self.companies.len() {
            let worn = self.rng.binomial(self.companies[c].hulls, wear);
            let co = &mut self.companies[c];
            co.hulls -= worn;
            let Some(mut k) = co.contract else {
                co.margin = (co.margin * (1.0 - self.rules.merc_haggle)).max(self.rules.merc_floor);
                continue;
            };
            let (t, f) = (k.target as usize, k.side as usize);
            // Its hulls there wear like the rest of its side's (they took
            // their share of the battle's losses in it: `Galaxy::lose`).
            let before = k.field;
            let worn = self.rng.binomial(k.field, wear);
            k.field = (k.field - worn).min(self.systems[t].hulls[f]);
            // Worn out or shot down, a hull gone is a hull lost: what it
            // committed is what it's lost plus what it has left (#429).
            k.lost += before - k.field;
            self.merc_moves[t][f].worn += before - k.field;
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
                let realm = &mut self.realms[k.realm as usize];
                let paid = k.bonus.min(realm.crystals.max(0.0));
                realm.crystals -= paid;
                self.companies[c].crystals += paid;
                // What it can't pay, it owes.
                let owed = k.bonus - paid;
                if owed > 1e-3 {
                    self.debts.push(Debt {
                        realm: k.realm,
                        company: self.companies[c].id,
                        crystals: owed,
                    });
                }
                self.books.won += 1;
                self.books.defended += k.defend as u64;
                self.books.bonus += paid as f64;
                self.books.defaulted += owed.max(0.0) as f64;
                self.books.worth_won += self.worth(t, f, true) as f64;
                self.books.paid_won += (k.upfront + paid) as f64;
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
                self.companies[new].margin = co.margin;
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
            if gone[c] || co.contract.is_some() || co.capital(price) >= self.rules.merc_join {
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

    /// Each company idle `Rules::merc_idle` ticks sails to the nearest other
    /// system at war to look for work: one where some side fighting has no
    /// company on its side yet, if there's one, else any. It pays the FTL
    /// as far as its crystal goes. Returns how many went.
    pub(crate) fn seek_work(&mut self) -> usize {
        let wars: Vec<usize> = (0..self.systems.len())
            .filter(|&s| self.systems[s].at_war())
            .collect();
        if wars.is_empty() {
            return 0;
        }
        // Wars with a side fighting that nobody's hired for.
        let open: Vec<bool> = wars
            .iter()
            .map(|&s| {
                (0..SIDES).any(|f| {
                    self.fighting(s, f)
                        && !self.companies.iter().any(|c| {
                            c.contract
                                .is_some_and(|k| k.target as usize == s && k.side as usize == f)
                        })
                })
            })
            .collect();
        let mut went = 0;
        for c in 0..self.companies.len() {
            let co = self.companies[c].clone();
            if co.contract.is_some() || self.tick < co.idle_since + self.rules.merc_idle as u64 {
                continue;
            }
            let here = self.systems[co.at as usize].pos;
            let d = |s: usize| self.systems[s].pos.distance(here);
            let nearest = |want_open: bool| {
                (0..wars.len())
                    .filter(|&w| wars[w] != co.at as usize && (!want_open || open[w]))
                    .min_by(|&a, &b| d(wars[a]).total_cmp(&d(wars[b])))
                    .map(|w| wars[w])
            };
            let Some(to) = nearest(true).or_else(|| nearest(false)) else {
                continue;
            };
            let fare = self.voyage(co.hulls, d(to)).min(co.crystals.max(0.0));
            let from = co.at;
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
        let p = Pilot { id: 1, known };
        assert_eq!(p.kit(), (HullKind::Vigil, TurretKind::Hymn));
        assert_eq!(p.level(), 1.5);
        // On a tie in whole steps, the one known furthest into the next.
        known.set(Part::Turret(TurretKind::Kontos), 2.0);
        known.set(Part::Turret(TurretKind::Knell), 2.7);
        assert_eq!(Pilot { id: 1, known }.kit().1, TurretKind::Knell);
        known.set(Part::Hull(HullKind::Gravis), 3.8);
        assert_eq!(Pilot { id: 1, known }.kit().0, HullKind::Gravis);
    }
}
