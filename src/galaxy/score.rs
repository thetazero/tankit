//! `--score <ticks> <seed,seed..> [quick]`: how everyone in the war does by
//! what they're after. Everyone's after profit: realms, companies and pilots,
//! in crystal. And each people wants something of its own (`docs/setting.md`,
//! "What they want"). Realms by their people and arm: each galaxy is run with
//! the arms handed round, so luck evens out (`quick`: run once).

use super::realm::{Arm, NONE};
use super::{Galaxy, PEOPLES, REGION, Rules, SYSTEMS};
use crate::faction::Faction;

/// Turns that hand round two arms, or three, as often to everyone.
const TURNS: usize = 6;
const ARMS: usize = Arm::ALL.len();

/// What side `f`'s people want.
pub fn want(f: usize) -> &'static str {
    match Faction::side(f) {
        Faction::Imperium => "systems held",
        Faction::Resonant => "crystal held",
        Faction::Uthenn => "cubes controlled",
        Faction::Ithessar => "starlight held",
        _ => "",
    }
}

/// One arm of a people's realms, all together.
#[derive(Clone, Copy, Default)]
struct Realms {
    realms: f64,
    systems: f64,
    /// Crystal a tick from mines, and crystal less what's owed.
    income: f64,
    crystal: f64,
    /// Of what they want.
    goal: f64,
}

/// Every people's arms.
type Tally = [[Realms; ARMS]; PEOPLES];

fn tally(g: &Galaxy) -> Tally {
    let mut out = Tally::default();
    for (_, r) in g.living() {
        let x = &mut out[r.faction as usize][r.arm as usize];
        x.realms += 1.0;
        x.income += r.income as f64;
        x.crystal += r.crystals as f64;
    }
    for d in &g.debts {
        let r = &g.realms[d.realm as usize];
        out[r.faction as usize][r.arm as usize].crystal -= d.crystals as f64;
    }
    // What they owe isn't theirs to hold.
    for x in &mut out[Faction::Resonant.index()] {
        x.goal = x.crystal;
    }
    for (s, sys) in g.systems.iter().enumerate() {
        let h = g.holder(s);
        if h != NONE {
            let r = &g.realms[h as usize];
            let x = &mut out[r.faction as usize][r.arm as usize];
            x.systems += 1.0;
            match Faction::side(r.faction as usize) {
                Faction::Imperium => x.goal += 1.0,
                Faction::Ithessar => x.goal += sys.starlight as f64,
                _ => {}
            }
        }
        for (v, stash) in g.vaults_of(s).iter().zip(g.stashes_of(s)) {
            if v.realm != NONE && Faction::side(v.side as usize) == Faction::Uthenn {
                let arm = g.realms[v.realm as usize].arm as usize;
                out[v.side as usize][arm].goal += stash.cubes as f64;
            }
        }
    }
    out
}

/// Companies and pilots over a run.
#[derive(Clone, Copy, Default)]
struct Mercs {
    companies: f64,
    /// What each is worth, makes a tick, and how, less its costs.
    worth: f64,
    profit: f64,
    loot: f64,
    contracts: f64,
    /// Merc pilots, the share of them in work, what they make a tick, and
    /// what a realm would have paid them; realms' pilots, and their pay.
    pilots: f64,
    employed: f64,
    pay: f64,
    realm_pay: f64,
    realm_pilots: f64,
    realm_wage: f64,
}

impl Mercs {
    fn of(g: &Galaxy) -> Mercs {
        let b = &g.books;
        let n = g.companies.len() as f64;
        let ticks = b.trade[0].max(1.0);
        let crew = b.crew_ticks.max(1.0);
        let owed: f64 = g.debts.iter().map(|d| d.crystals as f64).sum();
        let capital: f64 = g
            .companies
            .iter()
            .map(|c| c.capital(g.hull_rate) as f64)
            .sum();
        let realm_pilots: f64 = g.living().map(|(_, r)| r.pilots as f64).sum();
        Mercs {
            companies: n,
            worth: (capital + owed) / n.max(1.0),
            profit: b.profit / ticks,
            loot: b.trade[1] / ticks,
            contracts: b.trade[2] / ticks,
            pilots: (g.pool.len() + g.companies.iter().map(|c| c.pilots.len()).sum::<usize>())
                as f64,
            employed: g.employment as f64,
            pay: (b.wages + b.kill_bonuses) / crew,
            realm_pay: b.crew_realm_pay / crew,
            realm_pilots,
            realm_wage: g.realm_pay() as f64,
        }
    }

    fn numbers(&self) -> [f64; 11] {
        [
            self.companies,
            self.worth,
            self.profit,
            self.loot,
            self.contracts,
            self.pilots,
            self.employed,
            self.pay,
            self.realm_pay,
            self.realm_pilots,
            self.realm_wage,
        ]
    }
}

pub fn run(ticks: u64, seeds: &[u64], quick: bool) {
    let turns = if quick { 1 } else { TURNS };
    let (mut began, mut ended) = (Tally::default(), Tally::default());
    let mut mercs = [0.0; 11];
    let runs = (seeds.len() * turns).max(1) as f64;
    for &seed in seeds {
        for turn in 0..turns {
            let mut g = Galaxy::with_size(seed, SYSTEMS, REGION, Rules::default());
            for r in &mut g.realms {
                let arms = Arm::of(r.faction as usize);
                r.arm = arms[(r.arm as usize + turn) % arms.len()];
            }
            add(&mut began, &tally(&g), runs);
            while g.tick < ticks {
                g.tick_once();
            }
            add(&mut ended, &tally(&g), runs);
            for (x, y) in mercs.iter_mut().zip(Mercs::of(&g).numbers()) {
                *x += y / runs;
            }
            eprintln!("seed {seed} turn {} of {turns}", turn + 1);
        }
    }
    println!("after {ticks} ticks, {} galaxies, on average", runs as u32);
    println!(
        "people    arm    realms  systems   income   crystal  profit/t        want of it    (share)"
    );
    for f in 0..PEOPLES {
        let total = sum(&ended[f]);
        let before = sum(&began[f]);
        for (arm, name) in Arm::of(f).iter().map(|a| (*a as usize, a.name())) {
            row(f, name, &ended[f][arm], &began[f][arm], total.goal, ticks);
        }
        row(f, "all", &total, &before, total.goal, ticks);
    }
    let [
        n,
        worth,
        profit,
        loot,
        contracts,
        pilots,
        employed,
        pay,
        realm_pay,
        rp,
        rw,
    ] = mercs;
    println!(
        "companies: {n:.0}, worth {worth:.0} each, making {profit:+.3} a tick each (loot {loot:.2}, contracts {contracts:.2}, less costs)"
    );
    println!(
        "pilots: {pilots:.0} mercs, {:.0}% in work, on {pay:.3} a tick (a realm would pay them {realm_pay:.3}); {rp:.0} with realms, on {rw:.3}",
        100.0 * employed
    );
}

fn add(to: &mut Tally, x: &Tally, runs: f64) {
    for (to, x) in to.iter_mut().flatten().zip(x.iter().flatten()) {
        to.realms += x.realms / runs;
        to.systems += x.systems / runs;
        to.income += x.income / runs;
        to.crystal += x.crystal / runs;
        to.goal += x.goal / runs;
    }
}

fn sum(arms: &[Realms; ARMS]) -> Realms {
    let mut all = Realms::default();
    for x in arms {
        all.realms += x.realms;
        all.systems += x.systems;
        all.income += x.income;
        all.crystal += x.crystal;
        all.goal += x.goal;
    }
    all
}

/// A row of the table. Its profit is the crystal gained a tick since the
/// start, all its spending taken out.
fn row(f: usize, arm: &str, x: &Realms, was: &Realms, all: f64, ticks: u64) {
    println!(
        "{:9} {arm:6} {:6.0}  {:7.0}  {:7.1}  {:8.0}  {:+8.2}  {:12.0} {} ({:.1}%)",
        Faction::side(f).name(),
        x.realms,
        x.systems,
        x.income,
        x.crystal,
        (x.crystal - was.crystal) / ticks.max(1) as f64,
        x.goal,
        want(f),
        100.0 * x.goal / all.max(1e-9),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each people's scored by what it wants, every arm's share of it adding
    /// up to all there is, and only its own.
    #[test]
    fn peoples_are_scored_by_what_they_want() {
        let mut g = Galaxy::with_size(4, SYSTEMS / 4, REGION * 0.5, Rules::default());
        for _ in 0..40 {
            g.step();
        }
        let owes = (0..g.realms.len())
            .find(|&r| g.realms[r].faction as usize == Faction::Resonant.index())
            .unwrap();
        g.debts.push(crate::galaxy::merc::Debt {
            realm: owes as u32,
            company: g.companies[0].id,
            crystals: 50.0,
            due: 1.0,
        });
        let t = tally(&g);
        let of = |f: Faction| t[f.index()].iter().map(|x| x.goal).sum::<f64>();
        let held = |f: Faction| {
            let f = f.index();
            move |g: &Galaxy, s: usize| {
                let h = g.holder(s);
                h != NONE && g.realms[h as usize].faction as usize == f
            }
        };
        let systems = 0..g.systems.len();
        let imperium = systems
            .clone()
            .filter(|&s| held(Faction::Imperium)(&g, s))
            .count();
        assert_eq!(of(Faction::Imperium), imperium as f64);
        let light: f64 = systems
            .clone()
            .filter(|&s| held(Faction::Ithessar)(&g, s))
            .map(|s| g.systems[s].starlight as f64)
            .sum();
        assert!((of(Faction::Ithessar) - light).abs() < 1e-3);
        let crystal: f64 = g
            .living()
            .filter(|(_, r)| r.faction as usize == Faction::Resonant.index())
            .map(|(_, r)| r.crystals as f64)
            .sum();
        let owed: f64 = g
            .debts
            .iter()
            .filter(|d| g.realms[d.realm as usize].faction as usize == Faction::Resonant.index())
            .map(|d| d.crystals as f64)
            .sum();
        assert!(owed >= 50.0);
        assert!((of(Faction::Resonant) - (crystal - owed)).abs() < 1e-3);
        let cubes: u64 = g.stashes.iter().map(|x| x.cubes as u64).sum();
        let theirs = of(Faction::Uthenn);
        assert!(theirs > 0.0 && theirs <= cubes as f64);
        let all: f64 = t.iter().flatten().map(|x| x.systems).sum();
        assert_eq!(all as usize, g.systems.len());
    }
}
