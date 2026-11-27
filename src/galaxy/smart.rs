//! How smart realms and companies run their war.
//!
//! They play by the same rules as the others, knowing no more than they do:
//! only what they decide differs.
//!
//! **Pilots.** Fights go to whoever fills more seats, and crystal in a vault
//! only waits for someone to take it. So a smart realm budgets: it keeps as
//! many pilots as its income and a share of its savings pay for, whatever
//! its last fights showed, and lets them go only when the budget's short.
//!
//! **Campaigns.** It doesn't go for whatever world it looks at first: it
//! weighs every one in reach, with forces of every size, and launches the
//! best attack it has idle pilots and crystal for, looking further ahead
//! than others do and sticking with the neighbor it's begun on.
//!
//! **Hiring.** Where its own pilots are no match it looks for mercs, and
//! takes only a good deal it can pay for. With a smart company it pays in
//! instalments, keeping its pilots' pay by it.
//!
//! **Companies.** A smart company raids for the most crystal for its time,
//! judging a world by the pilots there to fly its hulls and not by the
//! hulls. It lends what a realm's income clears, and moves its margin with
//! demand.

use super::merc::Plan;
use super::realm::{Arm, NONE, Realm};
use super::{FTL_RANGE, Fleet, Galaxy, MIN_SEND, Rules, SIDES};

/// Share of its income a smart realm's payroll may take for good...
const PAYROLL: f32 = 0.6;
/// ... and the ticks it spreads its savings over, to pay for more.
const RUNWAY: f32 = 80.0;

impl Realm {
    /// The pilots a smart realm can keep paying.
    pub(crate) fn crew_budget(&self, rules: &Rules) -> u32 {
        let net = self.income.max(0.0) * (1.0 - rules.research);
        ((PAYROLL * net + self.crystals.max(0.0) / RUNWAY) / self.pay(rules)) as u32
    }

    /// A smart realm takes pilots on toward its budget, those out of work
    /// first, or lets some go. Returns those let go.
    pub(crate) fn crew_up(&mut self, rules: &Rules, free: u32) -> u32 {
        let budget = self.crew_budget(rules);
        let open = budget.saturating_sub(self.pilots);
        if open > 0 {
            let n = rules.train.min(open);
            // (one tick's intake is all of one or all of the other, as anyone's)
            if free > 0 {
                self.hired = n.min(free);
                self.pilots += self.hired;
            } else {
                let fee = rules.train_cost.max(1e-6);
                let n = n.min((self.crystals / fee) as u32);
                self.pilots += n;
                self.crystals -= n as f32 * rules.train_cost;
            }
            0
        } else {
            let n = rules
                .train
                .min((self.pilots - budget).saturating_sub(budget / 10));
            self.pilots -= n;
            n
        }
    }
}

/// Shares of what it could bring a smart realm thinks of sending.
pub(crate) const SENDS: [f32; 6] = [0.01, 0.03, 0.1, 0.25, 0.5, 1.0];
/// Ticks of a world's output a smart realm counts on when it attacks: it
/// means to keep it. And what any world's worth to it, crystal or none, for
/// its ground.
const HORIZON: f32 = 100.0;
const LAND: f32 = 5.0;
/// Share of a world it must expect to take, to attack.
const SURE: f32 = 0.5;
/// How much more it's after a world of the neighbor it's out to conquer.
const GRUDGE: f32 = 1.5;
/// It hires if it'd gain this much over the price, and the price, with what
/// it owes, is at most this share of its crystal.
pub(crate) const BARGAIN: f32 = 1.0;
pub(crate) const AT_MOST: f32 = 0.5;
/// Raids a smart company weighs at a time, how often it looks, the ticks it
/// reckons it loses between two, and the pilots that make a world half as
/// good a target.
pub(crate) const LOOKS: usize = 20;
pub(crate) const RAID_LOOK: u64 = 5;
pub(crate) const PAUSE: f32 = 4.0;
const GUARD: f32 = 10.0;

impl Galaxy {
    /// System `t`'s worth to realm `r` (`worth`, as long as it looks ahead).
    pub(crate) fn prize(&self, r: u32, t: usize, f: usize, own: bool) -> f32 {
        let hold = self.hold_for();
        if self.realms[r as usize].arm != Arm::Smart {
            return self.worth_over(t, f, own, hold);
        }
        let h = if own { 0.0 } else { HORIZON };
        self.worth_over(t, f, own, hold.max(h))
    }
}

/// Force sizes a smart realm thinks of attacking with.
const FORCES: [u32; 6] = [300, 1000, 3000, 8000, 20_000, 60_000];

/// An attack a smart realm might launch.
#[derive(Clone)]
struct Attack {
    t: usize,
    /// Hulls from each system.
    from: Vec<(usize, u32, f32)>,
    cost: f32,
    score: f32,
}

impl Galaxy {
    /// Smart realms of side `f` launch their attacks: each, now and then,
    /// the best it can find, with pilots to spare for it.
    pub(crate) fn campaigns(&mut self, f: usize) {
        for r in 0..self.realms.len() {
            let realm = &self.realms[r];
            if realm.arm != Arm::Smart
                || realm.faction as usize != f
                || realm.systems == 0
                || !(self.tick + r as u64).is_multiple_of(3)
            {
                continue;
            }
            let (best, rich) = self.best_attack(f, r as u32);
            if let Some(a) = best {
                self.launch(f, r as u32, &a);
            } else {
                // Its own pilots are no match: would mercs' be?
                for t in rich {
                    if self.hire(t, f, r as u32, Plan::default(), 0, 0.0) {
                        break;
                    }
                }
            }
        }
    }

    fn best_attack(&self, f: usize, r: u32) -> (Option<Attack>, Vec<usize>) {
        let realm = &self.realms[r as usize];
        let pilots = realm.pilots as f32;
        let rival = realm.rival;
        // What's spoken for: its fights now, and those it's flying to.
        let (mut flying, mut out) = (0u32, 0.0f32);
        let mut seen: Vec<u32> = Vec::new();
        for x in self.fleets.iter().filter(|x| x.realm == r) {
            if self.systems[x.to as usize].held[f] == 0 {
                flying += x.hulls;
                if !seen.contains(&x.to) {
                    seen.push(x.to);
                    out += self.seats_at(x.to as usize, f) as f32;
                }
            }
        }
        let idle = pilots - realm.seats_now - out;
        let fund = realm.crystals - KEEP_BY * pilots * realm.pay(&self.rules);
        if idle < self.rules.seats || fund <= 0.0 {
            return (None, Vec::new());
        }
        let war = self.war_hulls.get(r as usize).copied().unwrap_or(0) as f32 + flying as f32;
        let mut near: Vec<usize> = Vec::new();
        for &s in self.territory(r as usize) {
            if self.border[s as usize] && self.spare[s as usize][f] > 0 {
                near.extend(self.neighbors(s as usize).map(|(t, _)| t));
            }
        }
        near.sort_unstable();
        near.dedup();
        let seat = self.systems[realm.seat as usize].pos;
        let mut best: Option<Attack> = None;
        let mut rich: Vec<(f32, usize)> = Vec::new();
        for t in near {
            let sys = &self.systems[t];
            if sys.hulls[f] > 0
                || sys.held[f] > 0
                || !(sys.realms[f] == NONE || sys.realms[f] == r)
                || self.on_the_way(t, f).0 > 0.0
                || (0..SIDES).all(|g| sys.held[g] == 0)
            {
                continue;
            }
            let mut prize = self.worth_over(t, f, false, self.hold_for().max(HORIZON));
            prize += LAND;
            if sys.pos.distance(seat) > self.rules.reach {
                prize *= 0.5;
            }
            rich.push((
                prize * 1e4 / (1e4 + sys.hulls.iter().sum::<u32>() as f32),
                t,
            ));
            let pool: Vec<(usize, u32, f32)> = self
                .neighbors(t)
                .filter(|&(s, _)| self.systems[s].realms[f] == r && self.spare[s][f] > 0)
                .map(|(s, d)| (s, self.spare[s][f], d))
                .collect();
            let all: u32 = pool.iter().map(|p| p.1).sum();
            let scene = self.scene(t, f, r);
            for size in FORCES {
                if size > all {
                    break;
                }
                // The nearest go.
                let (mut left, mut cost, mut from) = (size, 0.0, Vec::new());
                for &(s, n, d) in &pool {
                    let m = n.min(left);
                    if m == 0 {
                        break;
                    }
                    left -= m;
                    cost += m as f32 * d / FTL_RANGE * self.rules.ftl_cost;
                    from.push((s, m, d));
                }
                if size < MIN_SEND || cost > fund {
                    break;
                }
                let own = size as f32;
                let flown = (pilots * own / (war + own)).min(own);
                let fc = self.forecast(
                    &scene,
                    &Plan {
                        own,
                        flown,
                        ..Default::default()
                    },
                );
                if fc.share < SURE {
                    continue;
                }
                let score = prize * fc.share - cost - fc.lost * realm.hull_worth;
                // It sticks to a neighbor once it's begun on it.
                let score = if rival != NONE && self.holder(t) == rival {
                    score * GRUDGE
                } else {
                    score
                };
                if score > best.as_ref().map_or(0.0, |b| b.score) {
                    best = Some(Attack {
                        t,
                        from,
                        cost,
                        score,
                    });
                }
            }
        }
        rich.sort_by(|a, b| b.0.total_cmp(&a.0));
        rich.truncate(2);
        (best, rich.into_iter().map(|x| x.1).collect())
    }

    fn launch(&mut self, f: usize, r: u32, a: &Attack) {
        self.realms[r as usize].crystals -= a.cost;
        // Till it's gone, or a better prize comes up, it's the one to beat.
        self.realms[r as usize].rival = self.holder(a.t);
        self.sent += 1;
        // It lands all together.
        let wait = a.from.iter().map(|x| self.flight(x.2)).max().unwrap_or(1);
        let arrives = self.tick + wait;
        for &(s, m, _) in &a.from {
            self.spare[s][f] -= m;
            self.systems[s].hulls[f] -= m;
            self.moves[s][f].left += m;
            self.fleets.push(Fleet {
                to: a.t as u32,
                side: f as u8,
                realm: r,
                hulls: m,
                arrives,
            });
            self.note_incoming(a.t as u32, f as u8, m, arrives);
        }
    }
}

impl Galaxy {
    /// Whether company `co`, idle, looks for a raid this tick: now and then;
    /// a smart one more often, and at once as it's done with its last work,
    /// its crew still aboard.
    pub(crate) fn looks_for_raid(&self, co: &super::merc::Company) -> bool {
        let every = if co.smart { RAID_LOOK } else { super::THINK };
        (co.smart && co.idle_since == self.tick) || (self.tick + co.id as u64).is_multiple_of(every)
    }
}

/// Ways a smart company thinks of raiding: hulls, and rookies it'd train at
/// most. An unguarded world takes only a few pilots.
pub(crate) const RAIDS: [(u32, u32); 10] = [
    (200, 0),
    (200, 3),
    (300, 0),
    (300, 6),
    (300, 12),
    (1000, 0),
    (1000, 24),
    (1000, u32::MAX),
    (3000, 0),
    (3000, u32::MAX),
];

impl Galaxy {
    /// How open system `s` is to a raid: less the more pilots its realms
    /// have to fly its hulls. A garrison nobody flies is no defense.
    pub(crate) fn soft(&self, s: usize) -> f32 {
        let sys = &self.systems[s];
        let pilots: f32 = (0..super::PEOPLES)
            .filter(|&g| sys.hulls[g] > 0 && sys.realms[g] != NONE)
            .map(|g| self.pilots_for(sys.realms[g], sys.hulls[g] as f32))
            .sum();
        GUARD / (GUARD + pilots)
    }
}

/// Share of a realm's income a smart company takes in instalments, the
/// longest it waits for the last, and the least it spreads a bill over.
const INSTALMENT: f32 = 0.3;
const CREDIT: f32 = 120.0;
/// A smart realm keeps this many ticks' payroll by it when it can pay a bill
/// in instalments.
const KEEP_BY: f32 = 10.0;
/// How a smart company's margin moves with each contract, and each time it
/// goes a while without one; and how far.
pub(crate) const MARGIN_UP: f32 = 1.1;
const MARGIN_DOWN: f32 = 0.98;
pub(crate) const MARGINS: (f32, f32) = (0.05, 1.0);

impl Galaxy {
    /// Ticks a smart company gives realm `r` to pay off `crystals`.
    pub(crate) fn term(&self, r: u32, crystals: f32) -> f32 {
        let income = self.realms[r as usize].income.max(0.01);
        (crystals / (INSTALMENT * income)).clamp(1.0, CREDIT)
    }

    /// Ticks of a realm's income company `c` lends against.
    pub(crate) fn patience_with(&self, c: usize) -> f32 {
        let co = &self.companies[c];
        if co.smart {
            INSTALMENT * CREDIT
        } else {
            co.patience
        }
    }

    /// What realm `r` pays company `c` at once of a bill of `bill`: all it
    /// can; but a smart company takes instalments from a smart realm, which
    /// keeps its pilots' pay by it.
    pub(crate) fn down_payment(&self, r: u32, c: usize, bill: f32) -> f32 {
        let realm = &self.realms[r as usize];
        let plan = realm.arm == Arm::Smart && self.companies[c].smart;
        let keep = if plan {
            KEEP_BY * realm.pilots as f32 * realm.pay(&self.rules)
        } else {
            0.0
        };
        bill.min((realm.crystals - keep).max(0.0))
    }

    /// Smart companies long without a contract ask less.
    pub(crate) fn thin_margins(&mut self) {
        let tick = self.tick;
        for co in self
            .companies
            .iter_mut()
            .filter(|c| c.smart && c.contract.is_none())
        {
            if tick > co.idle_since && (tick - co.idle_since).is_multiple_of(super::THINK) {
                co.margin = (co.margin * MARGIN_DOWN).max(MARGINS.0.min(co.margin));
            }
        }
    }
}
