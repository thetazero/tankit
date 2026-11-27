//! `--judge <ticks> [seed..]`: does the economy live? What the war and its
//! market come to over a run, and whether realms that hire fare better than
//! they do in the same galaxy with no companies to hire.

use super::{Galaxy, REGION, Rules, SIDES, SYSTEMS};

/// The share of systems the hiring realms hold, %.
fn hiring(g: &Galaxy) -> f64 {
    let (mut all, mut hire) = (0, 0);
    for (_, r) in g.living() {
        all += r.systems;
        hire += r.systems * r.hires as u32;
    }
    100.0 * hire as f64 / all.max(1) as f64
}

fn mid(mut v: Vec<f32>, q: f32) -> f32 {
    v.sort_by(f32::total_cmp);
    if v.is_empty() {
        return 0.0;
    }
    v[((v.len() - 1) as f32 * q) as usize]
}

fn report(g: &Galaxy, last: &super::merc::Books, was: f64, dt: f64) {
    let b = &g.books;
    let live: Vec<_> = g.living().map(|(_, r)| *r).collect();
    let (mut sides, mut seats, mut filled, mut merc) = (0, 0.0, 0.0, 0.0);
    for s in 0..g.systems.len() {
        for f in 0..SIDES {
            if g.fighting(s, f) {
                sides += 1;
                seats += (g.systems[s].hulls[f] as f32)
                    .min(g.rules.seats * g.battles_in(s)[f].max(1) as f32);
                let x = g.firing(s, f);
                filled += x;
                merc += (g.crewed(s, f).1 as f32).min(x);
            }
        }
    }
    let per = |x: f64, y: f64| (x - y) / dt;
    let hulls: u64 = g
        .systems
        .iter()
        .flat_map(|s| s.hulls)
        .map(u64::from)
        .chain(g.fleets.iter().map(|x| x.hulls as u64))
        .sum();
    println!(
        "  t{:5} war sides {sides:4} taken/t {:.1} hulls {:.2}M lost/t {:.0} hull worth p50/90 {:.2}/{:.2} rate {:.2} | hold {:.0} (lose new/old {:.4}/{:.4}) | realms {} GDP {:.0} vaults {:.0} (p50 {:.1}) share {:?}",
        g.tick,
        g.taken,
        hulls as f64 / 1e6,
        per(g.destroyed, was),
        mid(g.hull_worth.clone(), 0.5),
        mid(g.hull_worth.clone(), 0.9),
        g.hull_rate,
        g.hold_for(),
        g.lose_new,
        g.lose_old,
        live.len(),
        live.iter().map(|r| r.income).sum::<f32>(),
        live.iter().map(|r| r.crystals).sum::<f32>(),
        mid(live.iter().map(|r| r.crystals).collect(), 0.5),
        g.share.map(|s| (s * 100.0).round() as u32),
    );
    println!(
        "         pilots {} worth p50/90 {:.3}/{:.3} pay p50/90 {:.3}/{:.3} seats {seats:.0} filled {:.0}% mercs {:.1}% | mercs aboard {} pool {} employed {:.0}%",
        live.iter().map(|r| r.pilots).sum::<u32>(),
        mid(live.iter().map(|r| r.pilot_worth).collect(), 0.5),
        mid(live.iter().map(|r| r.pilot_worth).collect(), 0.9),
        mid(live.iter().map(|r| r.pay(&g.rules)).collect(), 0.5),
        mid(live.iter().map(|r| r.pay(&g.rules)).collect(), 0.9),
        100.0 * filled / seats.max(1.0),
        100.0 * merc / seats.max(1.0),
        g.companies.iter().map(|c| c.pilots.len()).sum::<usize>(),
        g.pool.len(),
        100.0 * g.employment,
    );
    let owed: f32 = g.debts.iter().map(|d| d.crystals).sum();
    let n = (b.signed - last.signed).max(1) as f64;
    let ct = (b.crew_ticks - last.crew_ticks).max(1.0);
    let cap: f32 = g
        .companies
        .iter()
        .map(|c| c.capital(g.hull_rate))
        .sum::<f32>()
        + owed;
    println!(
        "         companies {} on {} capital {cap:.0} margin p10/50/90 {:.2}/{:.2}/{:.2} | per t: asked {:.0} signed {:.2} refused {:.0} dear {:.0}; per contract: price {:.1} surplus {:.1} wages {:.1} hulls {:.1} flight {:.1}; won {:.0}% | sailing/t {:.1} split {} join {} founded {} profit/t {:.1} raids/t {:.2} won {:.0}% loot/t {:.1} trained/t {:.1} defected/t {:.1} | merc/realm pay {:.3}/{:.3}",
        g.companies.len(),
        g.companies.iter().filter(|c| c.contract.is_some()).count(),
        mid(g.companies.iter().map(|c| c.margin).collect(), 0.1),
        mid(g.companies.iter().map(|c| c.margin).collect(), 0.5),
        mid(g.companies.iter().map(|c| c.margin).collect(), 0.9),
        per(b.asked as f64, last.asked as f64),
        per(b.signed as f64, last.signed as f64),
        per(b.refused as f64, last.refused as f64),
        per(b.dear as f64, last.dear as f64),
        (b.paid_all - last.paid_all) / n,
        (b.surplus - last.surplus) / n,
        (b.wages + b.kill_bonuses - last.wages - last.kill_bonuses) / n,
        (b.bought - last.bought) / n,
        (b.flown - last.flown) / n,
        100.0 * (b.won - last.won) as f64 / n,
        per(b.sailing, last.sailing),
        b.splits,
        b.joins,
        b.founded,
        per(b.profit, last.profit),
        per(b.raids as f64, last.raids as f64),
        100.0 * (b.raids_won - last.raids_won) as f64 / (b.raids - last.raids).max(1) as f64,
        per(b.raided, last.raided),
        per(b.trained as f64, last.trained as f64),
        per(b.defected as f64, last.defected as f64),
        (b.wages + b.kill_bonuses - last.wages - last.kill_bonuses) / ct,
        (b.crew_realm_pay - last.crew_realm_pay) / ct,
    );
}

/// Hiring realms beside the others: what each has, for each system held.
fn arms(g: &Galaxy) {
    for hires in [true, false] {
        let (mut n, mut systems, mut vault, mut pilots, mut seats, mut income, mut broke) =
            (0, 0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32, 0);
        let (mut tier, mut fronts) = (0.0f32, 0.0f32);
        for (_, r) in g.living().filter(|(_, r)| r.hires == hires) {
            n += 1;
            systems += r.systems as f32;
            vault += r.crystals;
            pilots += r.pilots as f32;
            seats += r.seats_now;
            income += r.income;
            tier += r.tier as f32 * r.systems as f32;
            fronts += r.fronts as f32;
            broke += (r.crystals < 2.0 * r.pilots as f32 * r.pay(&g.rules)) as u32;
        }
        let (mut hulls, mut owed) = (0.0f32, 0.0f32);
        for (s, sys) in g.systems.iter().enumerate() {
            for f in 0..super::PEOPLES {
                let r = sys.realms[f];
                if r != super::realm::NONE && g.realms[r as usize].hires == hires {
                    hulls += (sys.hulls[f] - g.crewed(s, f).0) as f32;
                }
            }
        }
        for d in &g.debts {
            if g.realms[d.realm as usize].hires == hires {
                owed += d.crystals;
            }
        }
        let per = systems.max(1.0);
        println!(
            "         {}: realms {n} systems {systems:.0}; a system: vault {:.1} owed {:.1} income {:.2} hulls {:.0} pilots {:.2} seats {:.2} fronts {:.3} tier {:.1}; can't pay 2 ticks {broke}",
            if hires { "hirers" } else { "others" },
            vault / per,
            owed / per,
            income / per,
            hulls / per,
            pilots / per,
            seats / per,
            fronts / per,
            tier / per,
        );
    }
}

/// `RULES=name=value,..` changes a rule for a run.
fn tweak(rules: &mut Rules) {
    let Ok(all) = std::env::var("RULES") else {
        return;
    };
    for one in all.split(',') {
        let Some((name, x)) = one.split_once('=') else {
            continue;
        };
        let x: f32 = x.parse().unwrap_or(0.0);
        match name {
            "keep" => rules.keep = x,
            "ftl_speed" => rules.ftl_speed = x,
            "research" => rules.research = x,
            "ftl_cost" => rules.ftl_cost = x,
            "ship_cost" => rules.ship_cost = x,
            "train_cost" => rules.train_cost = x,
            "career" => rules.career = x,
            "ship_upkeep" => rules.ship_upkeep = x,
            "pilot_upkeep" => rules.pilot_upkeep = x,
            "sense" => rules.sense = x,
            "kill_share" => rules.kill_share = x,
            "merc_term" => rules.merc_term = x as u32,
            "blitz" => rules.blitz = x as u32,
            "garrison" => rules.garrison = x,
            "kill" => rules.kill = x,
            _ => println!("no rule {name}"),
        }
    }
}

/// How long wars and contracts last.
struct Spans {
    at_war: Vec<u64>,
    on: std::collections::HashMap<(u32, u64), u64>,
    wars: Vec<u64>,
    jobs: Vec<u64>,
}

impl Spans {
    fn of(g: &Galaxy) -> Spans {
        Spans {
            at_war: vec![0; g.systems.len()],
            on: Default::default(),
            wars: Vec::new(),
            jobs: Vec::new(),
        }
    }

    fn watch(&mut self, g: &Galaxy) {
        for (s, n) in self.at_war.iter_mut().enumerate() {
            if g.systems[s].at_war() {
                *n += 1;
            } else if *n > 0 {
                self.wars.push(std::mem::take(n));
            }
        }
        let now: std::collections::HashMap<(u32, u64), u64> = g
            .companies
            .iter()
            .filter_map(|c| {
                let k = c.contract?;
                (k.side as usize != super::RAID && g.tick >= k.arrives)
                    .then_some(((c.id, k.signed), k.arrives))
            })
            .collect();
        for (k, arrived) in &self.on {
            if !now.contains_key(k) {
                self.jobs.push(g.tick - arrived);
            }
        }
        self.on = now;
    }

    fn report(&mut self, g: &Galaxy) {
        let q = |v: &mut Vec<u64>| {
            v.sort();
            [0.25, 0.5, 0.75, 0.9]
                .map(|q| v.get((v.len() as f32 * q) as usize).copied().unwrap_or(0))
        };
        println!(
            "         ticks at 25/50/75/90%: wars {:?}, contracts once landed {:?}; offers {}",
            q(&mut self.wars),
            q(&mut self.jobs),
            g.open_work(1).len(),
        );
        self.wars.clear();
        self.jobs.clear();
    }
}

/// What raids come to.
fn raids(g: &Galaxy, last: &super::merc::Books) {
    let b = &g.books;
    let looks: Vec<u64> = (0..5)
        .map(|i| b.raid_looks[i] - last.raid_looks[i])
        .collect();
    let n = (b.raids - last.raids).max(1) as f64;
    let mut rich: Vec<f32> = (0..g.systems.len())
        .map(|s| g.vaults_of(s).iter().map(|v| v.crystals).sum())
        .collect();
    rich.sort_by(f32::total_cmp);
    let at = |q: f32| rich[((rich.len() - 1) as f32 * q) as usize];
    println!("         hires come to, of expected: {:.2?}", g.hire_trust);
    println!(
        "         raids: looks nowhere/no crew/can't pay/not worth/went {looks:?}; spent {} late {}; each: there {:.0} expected {:.0} took {:.0} hulls {:.0} lost {:.0} ticks {:.1}; crew quoted {:.0} had {:.0}, foes {:.0} then {:.0}, firing {:.0} then {:.0} | system vaults p50/90/99 {:.0}/{:.0}/{:.0}",
        b.raid_spent - last.raid_spent,
        b.raid_late - last.raid_late,
        (b.raid_there - last.raid_there) / n,
        (b.raid_expected - last.raid_expected) / n,
        (b.raided - last.raided) / n,
        (b.raid_hulls - last.raid_hulls) / n,
        (b.raid_lost - last.raid_lost) / n,
        (b.raid_ticks - last.raid_ticks) / n,
        (b.raid_crew[0] - last.raid_crew[0]) / n,
        (b.raid_crew[1] - last.raid_crew[1]) / n,
        (b.raid_foes[0] - last.raid_foes[0]) / n,
        (b.raid_foes[1] - last.raid_foes[1]) / n,
        (b.raid_fire[0] - last.raid_fire[0]) / n,
        (b.raid_fire[1] - last.raid_fire[1]) / n,
        at(0.5),
        at(0.9),
        at(0.99),
    );
}

/// What realm `r` is worth: its crystal, less what it owes, its hulls, and
/// what its mines will turn out for as long as worlds are held.
fn realm_worth(g: &Galaxy, r: u32) -> f64 {
    let mut x = g.realms[r as usize].crystals as f64;
    x -= g
        .debts
        .iter()
        .filter(|d| d.realm == r)
        .map(|d| d.crystals as f64)
        .sum::<f64>();
    let hold = g.hold_for() as f64;
    for (s, sys) in g.systems.iter().enumerate() {
        for f in 0..super::PEOPLES {
            if sys.realms[f] != r {
                continue;
            }
            let own = sys.hulls[f].saturating_sub(g.crewed(s, f).0);
            x += (own as f32 * g.hull_worth[s]) as f64;
            for site in &g.sites[sys.sites.0 as usize..sys.sites.1 as usize] {
                if site.owner as usize == f
                    && let super::SiteKind::Mine { crystals } = site.kind
                {
                    x += crystals as f64 * hold;
                }
            }
        }
    }
    x
}

/// Contracts signed, each replayed without it, under a few dice: what it
/// gained the realm, against what it expected.
pub fn replay(seeds: &[u64]) {
    const DICE: u64 = 3;
    const AFTER: u64 = 60;
    for defend in [false, true] {
        let (mut n, mut gain, mut sq, mut net) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
        let (mut paid, mut share_with, mut share_without, mut guess) = (0.0, 0.0, 0.0, 0.0);
        for &seed in seeds {
            let mut g = Galaxy::with_size(seed, SYSTEMS, REGION, Rules::default());
            while g.tick < 1500 {
                g.tick_once();
            }
            let mut took = 0;
            for _ in 0..80 {
                if took >= 22 {
                    break;
                }
                let pre = g.clone();
                g.tick_once();
                let Some(k) = g.companies.iter().filter_map(|c| c.contract).find(|k| {
                    k.signed == g.tick && k.side as usize != super::RAID && k.defend == defend
                }) else {
                    continue;
                };
                took += 1;
                let (t, f) = (k.target as usize, k.side as usize);
                let total = (g.systems[t].sites.1 - g.systems[t].sites.0) as f64;
                let held = pre.systems[t].held[f] as f64;
                let got = |g: &Galaxy| {
                    let now = g.systems[t].held[f] as f64;
                    if defend {
                        (now / held.max(1.0)).min(1.0)
                    } else {
                        ((now - held) / (total - held).max(1.0)).max(0.0)
                    }
                };
                let (mut x, mut a_paid) = (0.0, 0.0);
                for dice in 0..DICE {
                    let (mut a, mut b) = (pre.clone(), pre.clone());
                    b.ban = Some((k.realm, k.target));
                    a.tick_once();
                    b.tick_once();
                    a.ban = b.ban;
                    for g in [&mut a, &mut b] {
                        g.rng =
                            crate::rng::Rng::from_state((seed << 20) ^ (g.tick << 4) ^ dice | 1);
                    }
                    for i in 0..AFTER {
                        a.tick_once();
                        b.tick_once();
                        if i == (k.arrives - k.signed) + 29 {
                            share_with += got(&a) / DICE as f64;
                            share_without += got(&b) / DICE as f64;
                        }
                    }
                    x += (realm_worth(&a, k.realm) - realm_worth(&b, k.realm)) / DICE as f64;
                    a_paid += k.upfront as f64 / DICE as f64;
                }
                n += 1.0;
                gain += x;
                sq += x * x;
                net += k.net as f64;
                paid += a_paid;
                guess += (k.expected / k.worth.max(1e-3)) as f64;
            }
        }
        let n = n.max(1.0);
        let se = ((sq / n - (gain / n).powi(2)).max(0.0) / n).sqrt();
        println!(
            "REPLAY {}: {n} contracts; expected to gain {:+.1}, gained {:+.1} (se {se:.1}); up front {:.1}; share of the world expected {:.2}, got {:.2}, without {:.2}",
            if defend { "defenses" } else { "attacks" },
            net / n,
            gain / n,
            paid / n,
            guess / n,
            share_with / n,
            share_without / n,
        );
    }
}

pub fn run(ticks: u64, seeds: &[u64], arms: bool) {
    let mut diffs = Vec::new();
    for &seed in seeds {
        let mut moved = [0.0; 2];
        for (i, m) in moved.iter_mut().enumerate() {
            if i == 1 && !arms {
                break;
            }
            let mut rules = Rules::default();
            tweak(&mut rules);
            let rules = Rules {
                companies: if std::env::var("NONE").is_err() {
                    rules.companies
                } else {
                    0
                },
                ..rules
            };
            println!("seed {seed} {}", ["companies", "swapped"][i]);
            let mut g = Galaxy::with_size(seed, SYSTEMS, REGION, rules);
            // The same galaxy, its hirers' and the others' marks swapped: what's
            // luck in who's marked is against them then.
            if i == 1 {
                for r in &mut g.realms {
                    r.hires = !r.hires;
                }
            }
            let t0 = std::time::Instant::now();
            let (mut last, mut was, mut at) = (g.books, 0.0, 0);
            let mut spans = Spans::of(&g);
            g.step();
            let start = hiring(&g);
            while g.tick < ticks {
                g.tick_once();
                spans.watch(&g);
                if g.tick >= at + ticks / 8 {
                    report(&g, &last, was, (g.tick - at) as f64);
                    raids(&g, &last);
                    self::arms(&g);
                    spans.report(&g);
                    (last, was, at) = (g.books, g.destroyed, g.tick);
                }
            }
            *m = hiring(&g) - start;
            println!(
                "  hiring realms {:+.2} points; {:.1} ms a tick",
                *m,
                t0.elapsed().as_secs_f64() * 1e3 / g.tick as f64
            );
        }
        if arms {
            diffs.push((moved[0] + moved[1]) / 2.0);
        }
    }
    if arms {
        let n = diffs.len().max(1) as f64;
        let mean = diffs.iter().sum::<f64>() / n;
        let var = diffs.iter().map(|d| (d - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0);
        println!(
            "ARMS hiring realms, with the marks one way and swapped, on average: {mean:+.2} points (se {:.2}) {diffs:.2?}",
            (var / n).sqrt()
        );
    }
}
