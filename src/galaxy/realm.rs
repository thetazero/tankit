//! Realms: the war's actors (#315).
//!
//! A faction is a people, not a state: news crosses the stars too slowly for
//! anyone to rule them all. What fights is a **realm**: a ruler holding a
//! handful of systems from a **seat**, with its own crystal in its own vaults.
//! What a realm takes becomes its own. Realms of one people don't fight each
//! other (yet), and don't help each other either: each sends hulls only from
//! its own systems, and only where no other realm of its people already is.
//!
//! **Realms change shape** every few ticks (`Rules::politics`), by distance:
//! - *Split.* No realm can rule far from its seat. Holdings farther than
//!   `Rules::reach` from it, or cut off from the seat's own stretch of space,
//!   break away, each stretch of them a realm of its own with its share of
//!   the vaults.
//! - *Merge.* A small realm (fewer than `Rules::fold` systems) folds into the
//!   neighbor of its people with the nearest seat, if that seat is near
//!   enough to rule all of it (well inside the reach, so it doesn't break
//!   away again at once).
//! - *Seat.* A realm whose seat falls moves it to its system nearest the old one.
//!
//! So realms grow by conquest, split at their edges, and merge, and nobody
//! ever wins.

use glam::Vec3;

use super::{Galaxy, NEIGHBORS, SIDES};

/// No realm.
pub const NONE: u32 = u32::MAX;
/// A small realm folds into a neighbor only if that neighbor's seat is
/// within this share of the reach of all of it.
const FOLD_REACH: f32 = 0.8;

/// One realm.
#[derive(Clone, Copy, Debug)]
pub struct Realm {
    /// Which people: an index into `Faction::ALL`.
    pub faction: u8,
    /// The system it rules from.
    pub seat: u32,
    pub crystals: f32,
    /// Its crystal income last tick.
    pub income: f32,
    /// The tick it was founded (broke away, or the war's start).
    pub founded: u64,
    /// Systems it holds (updated each tick); 0 for a realm that's gone.
    pub systems: u32,
    /// Its gear tier (#438), in upgrade steps (`economy::STEPS_PER_MARK` to
    /// a mark): every pilot of the realm flies gear built to it.
    pub tier: u8,
    /// Crystal put into researching its next tier so far.
    pub research: f32,
    /// Its pilots, all alike, shared between all its fights.
    pub pilots: u32,
    /// What it pays each a tick.
    pub wage: f32,
    /// What one more pilot, and one more hull, have lately been worth to
    /// it: crystals a tick, and crystals.
    pub pilot_worth: f32,
    pub hull_worth: f32,
    /// Its best use this tick for one more of each (scratch).
    pub pilot_use: f32,
    pub hull_use: f32,
    /// The seats its pilots have lately had to fill; and those in its fights
    /// now, and in the biggest it could start (scratch).
    pub seats: f32,
    pub seats_now: f32,
    pub seats_next: f32,
    /// Free pilots it took on this tick (scratch).
    pub hired: u32,
    /// Whether it hires mercs at all: realms are split at random between
    /// those that do and those that don't, to see which fare better. A
    /// realm breaking away keeps its parent's way.
    pub hires: bool,
    /// Systems it's fighting in, as of the last tick's battles.
    pub fronts: u32,
    /// Scratch: systems it's lost since they were last counted.
    pub fell: u32,
}

impl Realm {
    pub fn new(faction: u8, seat: u32, tick: u64) -> Realm {
        Realm {
            faction,
            seat,
            crystals: 0.0,
            income: 0.0,
            founded: tick,
            systems: 0,
            tier: 0,
            research: 0.0,
            pilots: 0,
            wage: 0.0,
            pilot_worth: 0.1,
            hull_worth: 0.01,
            pilot_use: 0.0,
            hull_use: 0.0,
            seats: 0.0,
            seats_now: 0.0,
            seats_next: 0.0,
            hired: 0,
            hires: true,
            fronts: 0,
            fell: 0,
        }
    }

    /// What it pays each pilot a tick: never less than one lives on.
    pub fn pay(&self, rules: &super::Rules) -> f32 {
        self.wage.max(rules.pilot_upkeep)
    }

    /// The garrison a realm starts with and keeps: its share of the income's
    /// worth of pilots, or a front's if that's fewer.
    pub fn pilots_wanted(&self, rules: &super::Rules) -> u32 {
        (rules.garrison * self.income.max(0.0) / self.pay(rules)).min(rules.seats * 6.0) as u32
    }

    /// Pay its pilots (those it can't pay leave), then train some if there's
    /// a seat for them and one's worth more over a career than its training
    /// and pay, or let some go if one's worth less than its pay, or has no
    /// seat to fill; but keep up its garrison either way. Returns those who left or were let go.
    pub fn keep_pilots(&mut self, rules: &super::Rules, retired: u32, free: u32) -> u32 {
        // What its fights showed this tick.
        self.pilot_worth += rules.sense * (self.pilot_use - self.pilot_worth);
        // (and it keeps till it's wanted, so quiet ticks say little)
        let sense = if self.hull_use > 0.0 {
            rules.sense
        } else {
            rules.sense / 10.0
        };
        self.hull_worth += sense * (self.hull_use - self.hull_worth);
        (self.pilot_use, self.hull_use) = (0.0, 0.0);
        let need = self.seats_now + self.seats_next;
        self.seats += (need - self.seats).max(rules.sense * (need - self.seats));
        self.seats_next = 0.0;
        self.pilots -= retired.min(self.pilots);
        let upkeep = self.pay(rules);
        let before = self.pilots;
        self.pilots = self.pilots.min((self.crystals / upkeep) as u32);
        self.crystals = (self.crystals - self.pilots as f32 * upkeep).max(0.0);
        let mut left = before - self.pilots;
        // Over a career, what one earns it over their pay.
        let gain = (self.pilot_worth - upkeep) / rules.career.max(1e-6);
        // Its garrison it keeps up, whatever they're worth.
        let garrison = self.pilots_wanted(rules);
        let seats = (self.seats.ceil() as u32).max(garrison);
        let idle = self.pilots.saturating_sub(seats);
        let open = |worth: bool| {
            let to = if worth { seats } else { garrison };
            to.saturating_sub(self.pilots)
        };
        let (free_open, open) = (open(gain > 0.0), open(gain > rules.train_cost));
        if free > 0 && free_open > 0 {
            // Pilots out of work need no training.
            let n = rules.train.min(free).min(free_open);
            self.pilots += n;
            self.hired = n;
        } else if open > 0 {
            let n = rules
                .train
                .min(open)
                .min((self.crystals / rules.train_cost.max(1e-6)) as u32);
            self.pilots += n;
            self.crystals -= n as f32 * rules.train_cost;
        } else if gain < 0.0 || idle > 0 {
            let all = if gain < 0.0 {
                self.pilots - garrison.min(self.pilots)
            } else {
                idle
            };
            let n = rules.train.min(all);
            self.pilots -= n;
            left += n;
        }
        left
    }
}

/// A union-find's root of `i`.
pub(crate) fn find(root: &mut [u32], mut i: usize) -> usize {
    while root[i] as usize != i {
        root[i] = root[root[i] as usize];
        i = root[i] as usize;
    }
    i
}

impl Galaxy {
    /// The realm holding system `s` (the realm of the side holding most of its sites).
    pub fn holder(&self, s: usize) -> u32 {
        let sys = &self.systems[s];
        sys.realms[sys.owner()]
    }

    /// The realms that are still around, with their indices.
    pub fn living(&self) -> impl Iterator<Item = (usize, &Realm)> + '_ {
        self.realms
            .iter()
            .enumerate()
            .filter(|(_, r)| r.systems > 0)
    }

    /// The systems realm `r` holds (as of the last bookkeeping).
    pub fn territory(&self, r: usize) -> &[u32] {
        let (a, b) = self.spans[r];
        &self.members[a as usize..b as usize]
    }

    /// A new realm of `faction` seated at `seat`: in a slot of a realm that's
    /// gone if there is one.
    pub(crate) fn found(&mut self, faction: u8, seat: u32) -> u32 {
        let realm = Realm {
            hires: self.rng.chance(0.5),
            ..Realm::new(faction, seat, self.tick)
        };
        // A slot is free once nothing is tagged with it (`systems` and `alive`
        // both say so); taking one marks it, so it isn't handed out twice.
        if let Some(r) = self.free.pop() {
            self.realms[r as usize] = realm;
            self.realms[r as usize].systems = 1;
            r
        } else {
            self.realms.push(Realm {
                systems: 1,
                ..realm
            });
            self.spans.push((0, 0));
            (self.realms.len() - 1) as u32
        }
    }

    /// Side `f` now holds a site in system `s`: make sure a realm of its has
    /// it. The realm whose hulls are there does; if none are, the realm of `f`
    /// holding the nearest system within range; else a new one, seated here.
    pub(crate) fn claim(&mut self, s: usize, f: usize) {
        if f == super::RAID || self.systems[s].realms[f] != NONE {
            return;
        }
        let near = self
            .neighbors(s)
            .map(|(t, _)| self.systems[t].realms[f])
            .find(|&r| r != NONE);
        let r = near.unwrap_or_else(|| self.found(f as u8, s as u32));
        self.systems[s].realms[f] = r;
    }

    /// The first realms: each people's space carved into realms around seats
    /// at least `Rules::found` apart, every system ruled from its nearest seat
    /// of its people; then whatever's cut off from its seat breaks away.
    pub(crate) fn form_realms(&mut self) {
        let n = self.systems.len();
        let mut order: Vec<usize> = (0..n).collect();
        for i in (1..n).rev() {
            order.swap(i, self.rng.index(i + 1));
        }
        let mut seats: [Vec<u32>; SIDES] = Default::default();
        for s in order {
            let f = self.systems[s].owner();
            let pos = self.systems[s].pos;
            let crowded = seats[f]
                .iter()
                .any(|&t| self.systems[t as usize].pos.distance(pos) < self.rules.found);
            if !crowded {
                seats[f].push(s as u32);
            }
        }
        let mut of_seat = vec![NONE; n];
        for (f, seats) in seats.iter().enumerate() {
            for &seat in seats {
                of_seat[seat as usize] = self.realms.len() as u32;
                let hires = self.rng.chance(0.5);
                self.realms.push(Realm {
                    hires,
                    ..Realm::new(f as u8, seat, 0)
                });
                self.spans.push((0, 0));
            }
        }
        for s in 0..n {
            let f = self.systems[s].owner();
            let pos = self.systems[s].pos;
            let seat = seats[f]
                .iter()
                .min_by(|&&a, &&b| {
                    let da = self.systems[a as usize].pos.distance_squared(pos);
                    let db = self.systems[b as usize].pos.distance_squared(pos);
                    da.total_cmp(&db)
                })
                .copied()
                .unwrap_or(s as u32);
            self.systems[s].realms[f] = of_seat[seat as usize];
        }
        self.keep_books();
        self.reshape();
        self.keep_books();
        // Vaults part-filled, so the realms don't all launch at once.
        let income = self.realm_income();
        for (r, realm) in self.realms.iter_mut().enumerate() {
            realm.income = income[r];
            realm.crystals = income[r] * self.rng.range(0.0, 60.0);
            // Start with as many pilots as it can afford.
            realm.pilots = realm.pilots_wanted(&self.rules);
        }
    }

    /// Each realm's crystal income a tick, as its sites stand.
    pub(crate) fn realm_income(&self) -> Vec<f32> {
        let mut out = vec![0.0; self.realms.len()];
        for sys in &self.systems {
            for site in &self.sites[sys.sites.0 as usize..sys.sites.1 as usize] {
                if let super::SiteKind::Mine { crystals } = site.kind {
                    let r = sys.realms[site.owner as usize];
                    if r != NONE {
                        out[r as usize] += crystals;
                    }
                }
            }
        }
        out
    }

    /// Bookkeeping: let go of systems a realm has nothing left in, count who holds what, list each realm's
    /// territory, free the slots of realms that are gone, and move seats that fell.
    pub(crate) fn keep_books(&mut self) {
        let n = self.systems.len();
        let mut tagged = vec![false; self.realms.len()];
        for sys in &mut self.systems {
            for f in 0..SIDES {
                // Nothing held and no hull left: the realm's gone from here.
                if sys.held[f] == 0 && sys.hulls[f] == 0 {
                    sys.realms[f] = NONE;
                }
                if sys.realms[f] != NONE {
                    tagged[sys.realms[f] as usize] = true;
                }
            }
        }
        // Territory, as a list of systems sorted by holder.
        let holders: Vec<u32> = (0..n).map(|s| self.holder(s)).collect();
        let mut count = vec![0u32; self.realms.len()];
        for &h in &holders {
            if h != NONE {
                count[h as usize] += 1;
            }
        }
        let mut at = 0u32;
        for (r, &c) in count.iter().enumerate() {
            self.spans[r] = (at, at);
            at += c;
        }
        self.members.clear();
        self.members.resize(at as usize, 0);
        for (s, &h) in holders.iter().enumerate() {
            if h != NONE {
                let span = &mut self.spans[h as usize];
                self.members[span.1 as usize] = s as u32;
                span.1 += 1;
            }
        }
        self.free.clear();
        for r in 0..self.realms.len() {
            // A realm with hulls out but no system left still counts as one
            // until they're gone (it might take something yet).
            self.realms[r].systems = count[r].max(tagged[r] as u32);
            self.realms[r].fell = 0;
            if !tagged[r] {
                // (its fleets have nobody to fly for)
                for x in self.fleets.iter_mut().filter(|x| x.realm == r as u32) {
                    x.realm = NONE;
                }
                self.free.push(r as u32);
                continue;
            }
            let seat = self.realms[r].seat as usize;
            if count[r] > 0 && holders[seat] != r as u32 {
                // The seat fell: rule from what's left, nearest the old one.
                let old = self.systems[seat].pos;
                let (a, b) = self.spans[r];
                let next = self.members[a as usize..b as usize]
                    .iter()
                    .min_by(|&&x, &&y| {
                        let dx = self.systems[x as usize].pos.distance_squared(old);
                        let dy = self.systems[y as usize].pos.distance_squared(old);
                        dx.total_cmp(&dy)
                    })
                    .copied();
                if let Some(next) = next {
                    self.realms[r].seat = next;
                }
            }
        }
        self.free.reverse();
    }

    /// Realms split and merge, by distance (see the module's notes). Call
    /// `keep_books` first; it's stale after.
    pub(crate) fn reshape(&mut self) {
        let n = self.systems.len();
        let reach = self.rules.reach;
        let holders: Vec<u32> = (0..n).map(|s| self.holder(s)).collect();
        let far: Vec<bool> = (0..n)
            .map(|s| {
                let h = holders[s];
                h != NONE
                    && self.systems[self.realms[h as usize].seat as usize]
                        .pos
                        .distance(self.systems[s].pos)
                        > reach
            })
            .collect();
        // Stretches of one realm's space, near and far apart.
        let mut root: Vec<u32> = (0..n as u32).collect();
        for s in 0..n {
            for k in 0..NEIGHBORS {
                let t = self.neighbors[s * NEIGHBORS + k].0;
                if t == u32::MAX {
                    break;
                }
                let t = t as usize;
                if holders[t] == holders[s] && far[t] == far[s] {
                    let (a, b) = (find(&mut root, s), find(&mut root, t));
                    root[a] = b as u32;
                }
            }
        }
        // Every stretch but the seat's breaks away.
        let mut kept = vec![u32::MAX; self.realms.len()];
        for (r, realm) in self.realms.iter().enumerate() {
            if realm.systems > 0 && holders[realm.seat as usize] == r as u32 {
                kept[r] = find(&mut root, realm.seat as usize) as u32;
            }
        }
        // Each breakaway stretch: its systems, and its center.
        let mut stretch: std::collections::HashMap<u32, (Vec<u32>, Vec3)> = Default::default();
        for s in 0..n {
            let h = holders[s];
            if h == NONE {
                continue;
            }
            let c = find(&mut root, s) as u32;
            if kept[h as usize] == c {
                continue;
            }
            let e = stretch.entry(c).or_default();
            e.0.push(s as u32);
            e.1 += self.systems[s].pos;
        }
        let mut breakaways: Vec<(Vec<u32>, Vec3)> = stretch.into_values().collect();
        // In system order, so the dice (and the realms' slots) don't depend
        // on the hash map's.
        breakaways.sort_by_key(|b| b.0[0]);
        let vaults: Vec<(f32, u32)> = self
            .realms
            .iter()
            .map(|r| (r.crystals, r.systems))
            .collect();
        for (members, sum) in breakaways {
            let old = holders[members[0] as usize];
            let center = sum / members.len() as f32;
            let seat = *members
                .iter()
                .min_by(|&&a, &&b| {
                    let da = self.systems[a as usize].pos.distance_squared(center);
                    let db = self.systems[b as usize].pos.distance_squared(center);
                    da.total_cmp(&db)
                })
                .unwrap_or(&members[0]);
            let f = self.realms[old as usize].faction;
            let new = self.found(f, seat);
            // It breaks away with the gear it has, and its way with mercs.
            self.realms[new as usize].tier = self.realms[old as usize].tier;
            self.realms[new as usize].wage = self.realms[old as usize].wage;
            self.realms[new as usize].pilot_worth = self.realms[old as usize].pilot_worth;
            self.realms[new as usize].hull_worth = self.realms[old as usize].hull_worth;
            self.realms[new as usize].hires = self.realms[old as usize].hires;
            // Its share of the vaults, by systems.
            let (vault, size) = vaults[old as usize];
            let take = vault * (members.len() as f32 / size.max(1) as f32).min(1.0);
            let parent = &mut self.realms[old as usize];
            parent.crystals = (parent.crystals - take).max(0.0);
            // And its share of the pilots.
            let pilots = (parent.pilots as f32
                * (members.len() as f32 / size.max(1) as f32).min(1.0))
                as u32;
            parent.pilots -= pilots;
            self.realms[new as usize].crystals = take;
            self.realms[new as usize].pilots = pilots;
            for &s in &members {
                self.systems[s as usize].realms[f as usize] = new;
            }
            self.splits += 1;
        }
        self.keep_books();
        self.fold();
    }

    /// Small realms fold into a neighbor of their people (see the module's notes).
    fn fold(&mut self) {
        let n = self.systems.len();
        let holders: Vec<u32> = (0..n).map(|s| self.holder(s)).collect();
        let mut small: Vec<usize> = self
            .living()
            .filter(|(r, realm)| realm.systems < self.rules.fold && !self.territory(*r).is_empty())
            .map(|(r, _)| r)
            .collect();
        small.sort_by_key(|&r| (self.realms[r].systems, r));
        let mut into: Vec<u32> = (0..self.realms.len() as u32).collect();
        // A realm that's taken another in this round can't be folded away.
        let mut grown = vec![false; self.realms.len()];
        let mut merged = false;
        for a in small {
            if into[a] != a as u32 || grown[a] {
                continue;
            }
            let f = self.realms[a].faction;
            let seat = self.systems[self.realms[a].seat as usize].pos;
            let mut best: Option<(f32, usize)> = None;
            for &s in self.territory(a) {
                for (t, _) in self.neighbors(s as usize) {
                    let b = holders[t];
                    if b == NONE || b as usize == a || into[b as usize] != b {
                        continue;
                    }
                    let realm = &self.realms[b as usize];
                    // (and only with its own kind)
                    if realm.faction != f || realm.hires != self.realms[a].hires {
                        continue;
                    }
                    let d = self.systems[realm.seat as usize].pos.distance(seat);
                    if best.is_none_or(|x| d < x.0) {
                        best = Some((d, b as usize));
                    }
                }
            }
            let Some((_, b)) = best else { continue };
            let there = self.systems[self.realms[b].seat as usize].pos;
            let ruled = self.territory(a).iter().all(|&s| {
                self.systems[s as usize].pos.distance(there) <= self.rules.reach * FOLD_REACH
            });
            if !ruled {
                continue;
            }
            into[a] = b as u32;
            grown[b] = true;
            let take = self.realms[a].crystals;
            // Its debts too.
            for d in self.debts.iter_mut().filter(|d| d.realm as usize == a) {
                d.realm = b as u32;
            }
            self.realms[b].crystals += take;
            self.realms[a].crystals = 0.0;
            self.realms[b].pilots += std::mem::take(&mut self.realms[a].pilots);
            // Folded together, it keeps the better gear.
            self.realms[b].tier = self.realms[b].tier.max(self.realms[a].tier);
            self.merges += 1;
            merged = true;
        }
        if !merged {
            return;
        }
        for x in &mut self.fleets {
            if x.realm != NONE {
                x.realm = into[x.realm as usize];
            }
        }
        for sys in &mut self.systems {
            for r in &mut sys.realms {
                if *r != NONE {
                    *r = into[*r as usize];
                }
            }
        }
        self.keep_books();
    }
}
