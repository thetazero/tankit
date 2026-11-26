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
                seats += (g.systems[s].hulls[f] as f32).min(g.rules.frontage);
                let x = g.firing(s, f);
                filled += x;
                merc += (g.crewed(s, f).1 as f32).min(x);
            }
        }
    }
    let per = |x: f64, y: f64| (x - y) / dt;
    let hulls: u64 = g.systems.iter().flat_map(|s| s.hulls).map(u64::from).sum();
    println!(
        "  t{:5} war sides {sides:4} taken/t {:.1} hulls {:.2}M lost/t {:.0} hull worth p50/90 {:.2}/{:.2} rate {:.2} | hold new/old {:.0}/{:.0} | realms {} GDP {:.0} vaults {:.0} (p50 {:.1}) share {:?}",
        g.tick,
        g.taken,
        hulls as f64 / 1e6,
        per(g.destroyed, was),
        mid(live.iter().map(|r| r.hull_worth).collect(), 0.5),
        mid(live.iter().map(|r| r.hull_worth).collect(), 0.9),
        g.hull_rate,
        g.hold_for(true),
        g.hold_for(false),
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
        .map(|c| c.capital(g.rules.hull_cost))
        .sum::<f32>()
        + owed;
    println!(
        "         companies {} on {} capital {cap:.0} margin p10/50/90 {:.2}/{:.2}/{:.2} | per t: asked {:.0} signed {:.2} (crews {:.2}) refused {:.0} dear {:.0}; per contract: price {:.1} surplus {:.1} wages {:.1} hulls {:.1} flight {:.1}; won {:.0}% | sailing/t {:.1} split {} join {} founded {} profit/t {:.1} raids/t {:.2} won {:.0}% loot/t {:.1} | merc/realm pay {:.3}/{:.3}",
        g.companies.len(),
        g.companies.iter().filter(|c| c.contract.is_some()).count(),
        mid(g.companies.iter().map(|c| c.margin).collect(), 0.1),
        mid(g.companies.iter().map(|c| c.margin).collect(), 0.5),
        mid(g.companies.iter().map(|c| c.margin).collect(), 0.9),
        per(b.asked as f64, last.asked as f64),
        per(b.signed as f64, last.signed as f64),
        per(b.lent as f64, last.lent as f64),
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
        (b.wages + b.kill_bonuses - last.wages - last.kill_bonuses) / ct,
        (b.crew_realm_pay - last.crew_realm_pay) / ct,
    );
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

pub fn run(ticks: u64, seeds: &[u64], arms: bool) {
    let mut diffs = Vec::new();
    for &seed in seeds {
        let mut moved = [0.0; 2];
        for (i, m) in moved.iter_mut().enumerate() {
            if i == 1 && !arms {
                break;
            }
            let rules = Rules::default();
            let rules = Rules {
                companies: if i == 0 && std::env::var("NONE").is_err() {
                    rules.companies
                } else {
                    0
                },
                ..rules
            };
            println!("seed {seed} {}", ["companies", "none"][i]);
            let mut g = Galaxy::with_size(seed, SYSTEMS, REGION, rules);
            // A control for the control: the tags the other way round.
            if std::env::var("FLIP").is_ok() {
                for r in &mut g.realms {
                    r.hires = !r.hires;
                }
            }
            let t0 = std::time::Instant::now();
            let (mut last, mut was, mut at) = (g.books, 0.0, 0);
            g.step();
            let start = hiring(&g);
            while g.tick < ticks {
                g.tick_once();
                if g.tick >= at + ticks / 8 {
                    report(&g, &last, was, (g.tick - at) as f64);
                    raids(&g, &last);
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
            diffs.push(moved[0] - moved[1]);
        }
    }
    if arms {
        let n = diffs.len().max(1) as f64;
        let mean = diffs.iter().sum::<f64>() / n;
        let var = diffs.iter().map(|d| (d - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0);
        println!(
            "ARMS hiring realms, with companies less without: {mean:+.2} points (se {:.2}) {diffs:.2?}",
            (var / n).sqrt()
        );
    }
}
