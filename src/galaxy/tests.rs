use super::*;

/// A step always ends with a war somewhere: quiet ticks pass unseen.
#[test]
fn a_step_ends_at_war() {
    let mut g = Galaxy::with_size(
        7,
        SYSTEMS / 4,
        REGION * Vec2::new(1.0, 0.25),
        Rules::default(),
    );
    for _ in 0..300 {
        let before = g.tick;
        g.step();
        assert!(g.tick > before);
        assert!(
            g.systems.iter().any(|s| s.at_war()),
            "tick {}: no war anywhere",
            g.tick
        );
    }
}

/// An attacker under an enemy's shield fights a seat short there (#426):
/// in the battle for a site on a planet with the holder's generator up,
/// its fire is cut by one seat in `Rules::seats`. The defender never is,
/// and with the generators down, nor is the attacker.
#[test]
fn shields_cost_the_attacker_a_seat() {
    let mut g = Galaxy::with_size(1, 50, Vec2::splat(20.0), Rules::default());
    let s = 0;
    let owner = g.systems[s].owner();
    let other = (owner + 1) % SIDES;
    g.systems[s].hulls = [0; SIDES];
    g.systems[s].hulls[owner] = 40;
    g.systems[s].hulls[other] = 40;
    let up = g.shields_up(s, other);
    let (a, b) = g.systems[s].sites;
    let t = (a as usize..b as usize)
        .find(|&i| g.sites[i].owner as usize == owner && up[g.sites[i].planet as usize] > 0)
        .expect("a site under the owner's shield");
    let open = g.open_fire(s);
    let fire = g.fire(s, t);
    let short = (g.rules.seats - SHIELD_SEATS as f32) / g.rules.seats;
    assert!((fire[other] - open[other] * short).abs() < 1e-3);
    assert_eq!(fire[owner], open[owner], "the defender is never short");
    // Every generator down: no shield, no seat lost.
    for site in &mut g.sites[a as usize..b as usize] {
        if matches!(site.kind, SiteKind::Shield { .. }) {
            site.kind = SiteKind::Shield { damaged: true };
        }
    }
    assert_eq!(g.fire(s, t)[other], g.open_fire(s)[other]);
}

/// Shields don't stand in the way (#426): an attacker's win takes a site
/// at once, the planet's generators up or not, and goes for up ones first.
#[test]
fn a_win_takes_a_site_under_shields() {
    let mut g = Galaxy::with_size(1, 50, Vec2::splat(20.0), Rules::default());
    let s = 0;
    let owner = g.systems[s].owner();
    let other = (owner + 1) % SIDES;
    let held = g.systems[s].held[other];
    let i = *g
        .targets(s, other)
        .iter()
        .find(|&&i| g.sites[i].kind == (SiteKind::Shield { damaged: false }))
        .expect("a generator to take");
    g.win(s, i, other);
    assert_eq!(
        g.sites[i].owner as usize, other,
        "taken, not just knocked down"
    );
    assert_eq!(g.sites[i].kind, SiteKind::Shield { damaged: true });
    assert_eq!(g.systems[s].held[other], held + 1);
    // Every enemy site is on the front, shielded or not.
    g.systems[s].hulls[owner] = 40;
    g.systems[s].hulls[other] = 40;
    let (a, b) = g.systems[s].sites;
    let enemy = (a as usize..b as usize)
        .filter(|&i| g.sites[i].owner as usize == owner)
        .count();
    assert_eq!(g.front(s, other).len(), enemy);
}

/// In the galaxy's own battles, a site's attacker wins it or its holder
/// does (#422): a generator's either taken, damaged, or its holder's and
/// working; never left damaged with its holder.
#[test]
fn every_battle_has_a_winner() {
    let mut g = Galaxy::with_size(1, 50, Vec2::splat(20.0), Rules::default());
    let s = 0;
    let owner = g.systems[s].owner();
    let other = (owner + 1) % SIDES;
    g.systems[s].hulls[owner] = 40;
    g.systems[s].hulls[other] = 40;
    let i = *g
        .targets(s, other)
        .iter()
        .find(|&&i| matches!(g.sites[i].kind, SiteKind::Shield { .. }))
        .expect("a generator to take");
    g.sites[i].kind = SiteKind::Shield { damaged: true };
    g.battles(None);
    let site = g.sites[i];
    if site.owner as usize == owner {
        assert_eq!(site.kind, SiteKind::Shield { damaged: false });
    } else {
        assert_eq!(
            (site.owner as usize, site.kind),
            (other, SiteKind::Shield { damaged: true })
        );
    }
}

/// A damaged enemy generator is fought over even while another of the
/// enemy's on its world still works, so it stays down instead of being
/// repaired for nobody fighting over it.
#[test]
fn damaged_generators_are_kept_down() {
    let mut g = Galaxy::with_size(1, 50, Vec2::splat(20.0), Rules::default());
    // A world with two generators.
    let (s, x, y) = (0..g.systems.len())
        .find_map(|s| {
            let (a, b) = g.systems[s].sites;
            let gens: Vec<usize> = (a as usize..b as usize)
                .filter(|&i| matches!(g.sites[i].kind, SiteKind::Shield { .. }))
                .collect();
            gens.iter().find_map(|&i| {
                let j = *gens
                    .iter()
                    .find(|&&j| j != i && g.sites[j].planet == g.sites[i].planet)?;
                Some((s, i, j))
            })
        })
        .expect("a world with two generators");
    let owner = g.systems[s].owner();
    let other = (owner + 1) % SIDES;
    // The enemy's: one working, one damaged (just retaken).
    for (i, damaged) in [(x, false), (y, true)] {
        g.sites[i].owner = owner as u8;
        g.sites[i].kind = SiteKind::Shield { damaged };
    }
    assert!(
        g.targets(s, other).contains(&y),
        "the damaged one is a target"
    );
    g.systems[s].hulls = [0; SIDES];
    g.systems[s].hulls[owner] = 40;
    g.systems[s].hulls[other] = 40;
    g.battles(None);
    assert!(g.assaulted[y], "fought over");
}

/// An attacker goes for every enemy shield generator up in the system at
/// once, and every enemy site on a planet with none up (#460); the rest of
/// a shielded planet waits for its generators.
#[test]
fn attackers_go_for_every_shield_then_every_site() {
    let mut g = Galaxy::with_size(1, 50, Vec2::splat(20.0), Rules::default());
    let s = (0..g.systems.len())
        .find(|&s| {
            let (a, b) = g.systems[s].sites;
            let gens = (a..b)
                .filter(|&i| matches!(g.sites[i as usize].kind, SiteKind::Shield { .. }))
                .count();
            gens >= 2 && (b - a) as usize > gens
        })
        .expect("a system with generators and more");
    let owner = g.systems[s].owner();
    let (a, b) = g.systems[s].sites;
    let range = a as usize..b as usize;
    // Everything here the owner's, nothing damaged.
    for i in range.clone() {
        let site = &mut g.sites[i];
        site.owner = owner as u8;
        if let SiteKind::Shield { damaged } = &mut site.kind {
            *damaged = false;
        }
    }
    let other = (owner + 1) % SIDES;
    let shielded = |g: &Galaxy, p: u8| {
        range.clone().any(|i| {
            g.sites[i].planet == p && g.sites[i].kind == (SiteKind::Shield { damaged: false })
        })
    };
    let want = |g: &Galaxy| -> Vec<usize> {
        range
            .clone()
            .filter(|&i| {
                let site = &g.sites[i];
                site.owner as usize == owner
                    && (!shielded(g, site.planet) || matches!(site.kind, SiteKind::Shield { .. }))
            })
            .collect()
    };
    let generators = range
        .clone()
        .filter(|&i| matches!(g.sites[i].kind, SiteKind::Shield { .. }))
        .count();
    let targets = g.targets(s, other);
    assert_eq!(targets, want(&g));
    assert!(
        targets
            .iter()
            .filter(|&&i| matches!(g.sites[i].kind, SiteKind::Shield { .. }))
            .count()
            == generators,
        "every generator at once"
    );
    // One planet's generators down: all its sites at once.
    let i = targets
        .iter()
        .copied()
        .find(|&i| matches!(g.sites[i].kind, SiteKind::Shield { .. }))
        .unwrap();
    let p = g.sites[i].planet;
    for j in range.clone() {
        if g.sites[j].planet == p && matches!(g.sites[j].kind, SiteKind::Shield { .. }) {
            g.sites[j].kind = SiteKind::Shield { damaged: true };
        }
    }
    let targets = g.targets(s, other);
    assert_eq!(targets, want(&g));
    for j in range.clone().filter(|&j| g.sites[j].planet == p) {
        assert!(targets.contains(&j), "site {j} on the open planet");
    }
    // A tick at war fights over every one of them, and the theater has
    // the attacker committed to each.
    g.systems[s].hulls = [0; SIDES];
    g.systems[s].hulls[owner] = 40;
    g.systems[s].hulls[other] = 40;
    crewed(&mut g);
    assert_eq!(g.commits(s, other), targets);
    g.battles(None);
    for &j in &targets {
        assert!(g.assaulted[j], "site {j} fought over");
    }
}

/// A tick of the whole galaxy has to fit after every battle unnoticed.
#[test]
#[cfg_attr(debug_assertions, ignore = "timing means something only in release")]
fn a_tick_is_cheap() {
    let mut g = Galaxy::new(3);
    let (t, start) = (std::time::Instant::now(), g.tick);
    for _ in 0..100 {
        g.step();
    }
    let per = t.elapsed() / (g.tick - start) as u32;
    assert!(per.as_millis() < 10, "a tick took {per:?}");
}

/// How a setting of `Rules` plays, over a few seeds of the full galaxy:
/// run with `cargo test --release judge -- --ignored --nocapture`.
/// Per seed: the range of every faction's share, the fraction of systems at
/// war (mean, 10th and 90th percentile, max), the biggest swing of any
/// faction's share over 100 ticks, and how many systems changed holder per
/// 100 ticks.
pub fn judge(label: &str, rules: Rules, seeds: &[u64], ticks: u64) {
    println!("== {label}");
    for &seed in seeds {
        let mut g = Galaxy::with_size(seed, SYSTEMS, REGION, rules);
        // Pockets: how many systems are in one on average, and how long each
        // pocket system lasts before it changes hands or rejoins its side.
        let (mut pocketed, mut since, mut lives) = (0u64, vec![0u64; SYSTEMS], Vec::new());
        let mut offensive_ticks = 0u64;
        let (mut realms, mut sizes, mut biggest) = (0u64, 0u64, 0u32);
        let mut owners: Vec<usize> = g.systems.iter().map(|s| s.owner()).collect();
        let (mut lo, mut hi) = (1.0f32, 0.0f32);
        let mut wars = Vec::new();
        let mut history = vec![g.share];
        let mut flips = 0u64;
        for t in 1..=ticks {
            g.step();
            let c = g.census();
            if t > 100 {
                for f in 0..SIDES {
                    lo = lo.min(c.share[f]);
                    hi = hi.max(c.share[f]);
                }
                wars.push(c.at_war);
            }
            history.push(c.share);
            for (i, &cut) in g.cut.iter().enumerate() {
                pocketed += cut as u64;
                if cut && since[i] == 0 {
                    since[i] = t;
                } else if !cut && since[i] > 0 {
                    lives.push(t - since[i]);
                    since[i] = 0;
                }
            }
            offensive_ticks += g.living().filter(|(_, r)| r.offensive.is_some()).count() as u64;
            for (_, r) in g.living() {
                realms += 1;
                sizes += r.systems as u64;
                biggest = biggest.max(r.systems);
            }
            if t % 100 == 0 {
                for (i, s) in g.systems.iter().enumerate() {
                    let o = s.owner();
                    flips += (o != owners[i]) as u64;
                    owners[i] = o;
                }
            }
        }
        let swing = history
            .windows(101)
            .map(|w| {
                (0..SIDES)
                    .map(|f| (w[100][f] - w[0][f]).abs())
                    .fold(0.0, f32::max)
            })
            .fold(0.0, f32::max);
        wars.sort_by(f32::total_cmp);
        let mean = wars.iter().sum::<f32>() / wars.len().max(1) as f32;
        let pct = |p: f32| wars[((wars.len() - 1) as f32 * p) as usize] * 100.0;
        println!(
            "seed {seed}: share {:4.1}..{:4.1}%  war mean {:4.1}% p10 {:4.1} p90 {:4.1} max {:4.1}  swing/100 {:4.1}%  flips/100 {:5.0}  pockets {:4.1} live {:4.1}  offensives {:3.1}  realms {:3.0} size {:4.1} max {} splits/100 {:4.1} merges/100 {:4.1}  final {:?}",
            lo * 100.0,
            hi * 100.0,
            mean * 100.0,
            pct(0.1),
            pct(0.9),
            pct(1.0),
            swing * 100.0,
            flips as f32 / (ticks / 100) as f32,
            pocketed as f32 / ticks as f32,
            lives.iter().sum::<u64>() as f32 / lives.len().max(1) as f32,
            offensive_ticks as f32 / ticks as f32,
            realms as f32 / ticks as f32,
            sizes as f32 / realms.max(1) as f32,
            biggest,
            g.splits as f32 / (ticks / 100) as f32,
            g.merges as f32 / (ticks / 100) as f32,
            g.share.map(|s| (s * 1000.0).round() / 10.0),
        );
    }
}

#[test]
#[ignore = "a tuning sweep, slow: run by hand in release"]
fn judge_rules() {
    let ticks = std::env::var("JUDGE_TICKS")
        .ok()
        .and_then(|t| t.parse().ok())
        .unwrap_or(3000);
    let base = Rules::default();
    let variants: Vec<(&str, Rules)> = vec![
        ("default", base),
        (
            "reach 350",
            Rules {
                reach: 350.0,
                ..base
            },
        ),
        (
            "reach 800",
            Rules {
                reach: 800.0,
                ..base
            },
        ),
        (
            "launch 30",
            Rules {
                launch: 30.0,
                ..base
            },
        ),
    ];
    let pick = std::env::var("JUDGE").unwrap_or_default();
    for (label, rules) in variants {
        if pick.is_empty() || label.contains(&pick) {
            judge(label, rules, &[1, 2, 3], ticks);
        }
    }
}

/// The neighbor graph the structure makes: how many systems each can reach,
/// and how much of the galaxy hangs together.
#[test]
#[ignore = "a tuning report: run by hand"]
fn judge_structure() {
    for seed in [1, 2, 3] {
        let g = Galaxy::new(seed);
        let n = g.systems.len();
        let mut counts = vec![0usize; NEIGHBORS + 1];
        let mut parent: Vec<usize> = (0..n).collect();
        fn find(p: &mut [usize], mut i: usize) -> usize {
            while p[i] != i {
                p[i] = p[p[i]];
                i = p[i];
            }
            i
        }
        for s in 0..n {
            let k = g.neighbors(s).count();
            counts[k] += 1;
            for (t, _) in g.neighbors(s).collect::<Vec<_>>() {
                let (a, b) = (find(&mut parent, s), find(&mut parent, t));
                parent[a] = b;
            }
        }
        let mut sizes = std::collections::HashMap::new();
        for s in 0..n {
            *sizes.entry(find(&mut parent, s)).or_insert(0usize) += 1;
        }
        let biggest = sizes.values().max().unwrap();
        let zs: Vec<f32> = g.systems.iter().map(|s| s.pos.z.abs()).collect();
        println!(
            "seed {seed}: neighbors {:?}  components {}  biggest {:.1}%  mean |z| {:.0}",
            counts,
            sizes.len(),
            *biggest as f32 / n as f32 * 100.0,
            zs.iter().sum::<f32>() / n as f32
        );
    }
}

fn small_war() -> Galaxy {
    Galaxy::with_size(4, SYSTEMS / 4, REGION * 0.5, Rules::default())
}

/// Realms are the actors: every side's sites and hulls in a system are one
/// realm's, of that side, and every realm still around holds or fights somewhere.
#[test]
fn every_holding_is_a_realms() {
    let mut g = small_war();
    for _ in 0..60 {
        g.step();
        for sys in &g.systems {
            for f in 0..SIDES {
                if sys.held[f] > 0 || sys.hulls[f] >= AT_WAR {
                    let r = sys.realms[f];
                    assert_ne!(
                        r,
                        realm::NONE,
                        "tick {}: a side's holding with no realm",
                        g.tick
                    );
                    assert_eq!(g.realms[r as usize].faction as usize, f);
                    assert!(g.realms[r as usize].systems > 0);
                }
            }
        }
    }
    assert!(g.living().count() > 20, "a people is many realms");
}

/// Holdings too far from their seat break away as realms of their own,
/// with their share of the vaults.
#[test]
fn far_holdings_break_away() {
    let mut g = small_war();
    let before = g.living().count();
    let crystals: f32 = g.realms.iter().map(|r| r.crystals).sum();
    g.rules.reach = 100.0;
    g.reshape();
    assert!(g.splits > 0);
    assert!(g.living().count() > before);
    let after: f32 = g.realms.iter().map(|r| r.crystals).sum();
    assert!(
        (after - crystals).abs() < crystals * 1e-3,
        "the vaults are shared out, not lost"
    );
}

/// Small neighbors of one people fold together when one seat can rule them.
#[test]
fn small_realms_fold_together() {
    let mut g = small_war();
    let before = g.living().count();
    g.rules.fold = 1000;
    g.rules.reach = 5000.0;
    g.reshape();
    assert!(g.merges > 0);
    assert!(g.living().count() < before);
    // And never across peoples.
    for sys in &g.systems {
        for f in 0..SIDES {
            if sys.realms[f] != realm::NONE {
                assert_eq!(g.realms[sys.realms[f] as usize].faction as usize, f);
            }
        }
    }
}

/// How the mercenary trade plays (#318), over a few seeds of the full
/// galaxy, each run with companies and again without: run with
/// `cargo test --release judge_mercs -- --ignored --nocapture`.
/// Per seed and run: the war (systems at war, sites taken, hulls destroyed,
/// hulls standing and crystal banked, on average), and with companies their
/// count and worth over the run and the contracts they worked.
pub fn judge_mercs(label: &str, rules: Rules, seeds: &[u64], ticks: u64) {
    println!("== {label}");
    for &seed in seeds {
        for with in [true, false] {
            let rules = Rules {
                companies: if with { rules.companies } else { 0 },
                ..rules
            };
            let mut g = Galaxy::with_size(seed, SYSTEMS, REGION, rules);
            let (mut war, mut hulls, mut vaults, mut taken, mut samples) =
                (0.0, 0.0, 0.0, 0u64, 0u64);
            let mut quiet_never = true;
            let (mut lo, mut hi) = (1.0f32, 0.0f32);
            let mut track = Vec::new();
            let mut next = 0;
            while g.tick < ticks {
                g.step();
                taken += g.taken as u64;
                let c = g.census();
                quiet_never &= c.at_war > 0.0;
                if g.tick > 100 {
                    for f in 0..SIDES {
                        lo = lo.min(c.share[f]);
                        hi = hi.max(c.share[f]);
                    }
                    war += c.at_war as f64;
                    hulls += c.hulls.iter().sum::<f32>() as f64;
                    vaults += c.vaults as f64;
                    samples += 1;
                }
                if g.tick >= next {
                    next += ticks / 6;
                    track.push(format!(
                        "t{} {}co {:.0}cr {}on",
                        g.tick, c.companies, c.merc_capital, c.contracts
                    ));
                }
            }
            let k = samples.max(1) as f64;
            let per = |x: f64| x / (g.tick as f64 / 100.0);
            println!(
                "seed {seed} {}: share {:4.1}..{:4.1}%  war {:4.2}% always {}  taken/100 {:5.0}  destroyed/100 {:6.0}  hulls {:7.0}  vaults {:6.0}",
                if with { "mercs   " } else { "no mercs" },
                lo * 100.0,
                hi * 100.0,
                war / k * 100.0,
                quiet_never,
                per(taken as f64),
                per(g.destroyed),
                hulls / k,
                vaults / k,
            );
            if with {
                let b = g.books;
                let margins: f32 = g.companies.iter().map(|c| c.margin).sum::<f32>()
                    / g.companies.len().max(1) as f32;
                println!(
                    "    {}\n    signed {} (defenses {}) won {} (defenses {}) broken {} expired {}  upfront {:.0} bonus {:.0} defaulted {:.0} repaid {:.0} written off {:.0} owed at end {:.0}  refused {}  won worlds worth {:.0} for {:.0}  sold hulls {:.0}cr  merc hulls lost {:.0}  splits {} joins {} sailed {}  margin {:.2}",
                    track.join("  "),
                    b.signed,
                    b.defenses,
                    b.won,
                    b.defended,
                    b.broken,
                    b.expired,
                    b.upfront,
                    b.bonus,
                    b.defaulted,
                    b.repaid,
                    b.written_off,
                    g.debts.iter().map(|d| d.crystals).sum::<f32>(),
                    b.refused,
                    b.worth_won,
                    b.paid_won,
                    b.sold,
                    b.lost,
                    b.splits,
                    b.joins,
                    b.sailed,
                    margins,
                );
            }
        }
    }
}

#[test]
#[ignore = "a tuning sweep, slow: run by hand in release"]
fn judge_mercs_rules() {
    let ticks = std::env::var("JUDGE_TICKS")
        .ok()
        .and_then(|t| t.parse().ok())
        .unwrap_or(3000);
    let seeds: Vec<u64> = std::env::var("JUDGE_SEEDS")
        .ok()
        .map(|s| s.split(',').filter_map(|x| x.parse().ok()).collect())
        .unwrap_or(vec![1, 2, 3]);
    judge_mercs("default", Rules::default(), &seeds, ticks);
}

/// Realms hire companies, the companies fight and get paid, and their
/// hulls in a fight are always some of their side's there.
#[test]
fn realms_hire_companies() {
    // Two galaxies' companies together: one galaxy's can have a bad run.
    let (mut start, mut end) = (0.0, 0.0);
    for seed in [4, 5] {
        let (s, e) = hire_companies(Galaxy::with_size(
            seed,
            SYSTEMS / 4,
            REGION * 0.5,
            Rules::default(),
        ));
        start += s;
        end += e;
    }
    assert!(
        end > start,
        "the companies' capital went from {start} to {end}"
    );
}

/// Runs `g` a while checking its contracts; the companies' capital
/// before and after.
fn hire_companies(mut g: Galaxy) -> (f32, f32) {
    assert!(!g.companies.is_empty());
    let start: f32 = g
        .companies
        .iter()
        .map(|c| c.capital(g.rules.hull_price))
        .sum();
    for _ in 0..150 {
        g.step();
        for c in &g.companies {
            assert!(c.crystals >= 0.0);
            if let Some(k) = c.contract {
                let sys = &g.systems[k.target as usize];
                assert!(k.field <= sys.hulls[k.side as usize]);
                let r = sys.realms[k.side as usize];
                assert_eq!(g.realms[r as usize].faction, k.side);
                assert!(k.lost <= k.committed);
            }
        }
    }
    let b = g.books;
    assert!(b.signed > 10, "only {} contracts", b.signed);
    assert!(b.won > 0 && b.broken + b.expired > 0);
    assert!(b.upfront > 0.0 && b.bonus > 0.0 && b.sold > 0.0);
    let end: f32 = g
        .companies
        .iter()
        .map(|c| c.capital(g.rules.hull_price))
        .sum();
    (start, end)
}

/// Defenders hire companies too (#340).
#[test]
fn defenders_hire() {
    let mut g = small_war();
    for _ in 0..150 {
        g.step();
    }
    let b = g.books;
    assert!(b.defenses > 0 && b.defended > 0, "{b:?}");
}

/// A realm that can't pay a bonus owes it: no company signs with it until
/// it's paid, and it pays it back out of its income (#340).
#[test]
fn unpaid_bonuses_are_debts() {
    let mut g = small_war();
    for _ in 0..150 {
        g.step();
        for c in &g.companies {
            if let Some(k) = c.contract
                && k.signed == g.tick
            {
                assert!(!g.in_debt(k.realm), "tick {}: hired while in debt", g.tick);
            }
        }
    }
    let b = g.books;
    assert!(b.defaulted > 0.0 && b.repaid > 0.0, "{b:?}");
    assert!(b.refused > 0, "{b:?}");
}

/// Without companies, nobody's hired, and the war goes on as before.
#[test]
fn no_companies_no_contracts() {
    let rules = Rules {
        companies: 0,
        ..Rules::default()
    };
    let mut g = Galaxy::with_size(4, SYSTEMS / 4, REGION * 0.5, rules);
    for _ in 0..30 {
        g.step();
    }
    assert!(g.companies.is_empty());
    assert_eq!(g.books.signed, 0);
}

/// An idle company too rich splits in two; one too small joins the nearest.
#[test]
fn companies_split_and_join() {
    let mut g = small_war();
    for c in &mut g.companies {
        c.contract = None;
        c.crystals = 100.0;
        c.hulls = 0;
    }
    let n = g.companies.len();
    g.rules.merc_split = 1000.0;
    g.companies[0].crystals = g.rules.merc_split * 3.0;
    g.companies[1].crystals = g.rules.merc_join / 2.0;
    let total: f32 = g.companies.iter().map(|c| c.crystals).sum();
    g.split_and_join();
    assert_eq!((g.books.splits, g.books.joins), (1, 1));
    assert_eq!(g.companies.len(), n);
    // Nothing's lost but the small one's fare over.
    let after: f32 = g.companies.iter().map(|c| c.crystals).sum();
    assert!(after <= total + 1e-2 && after >= total - g.rules.merc_join / 2.0);
}

/// The longest any company in `g` stays idle over `ticks` more ticks
/// (stretches still going at the end count as far as they've run), and
/// the company's id.
pub fn longest_idle(g: &mut Galaxy, ticks: u64) -> (u64, u32) {
    use std::collections::HashMap;
    let end = g.tick + ticks;
    let mut since: HashMap<u32, u64> = HashMap::new();
    let mut worst = (0, 0);
    while g.tick < end {
        for c in &g.companies {
            if c.contract.is_some() {
                if let Some(t) = since.remove(&c.id) {
                    worst = worst.max((g.tick - t, c.id));
                }
            } else {
                since.entry(c.id).or_insert(g.tick);
            }
        }
        // Those gone (joined another) were idle up to now.
        since.retain(|id, t| {
            let here = g.companies.iter().any(|c| c.id == *id);
            if !here {
                worst = worst.max((g.tick - *t, *id));
            }
            here
        });
        g.step();
    }
    for (id, t) in since {
        worst = worst.max((g.tick - t, id));
    }
    worst
}

/// No company stays idle forever: idle long enough, it sails to a war and
/// gets work there (#355).
#[test]
fn no_company_idles_forever() {
    let mut g = small_war();
    for _ in 0..100 {
        g.step();
    }
    let (idle, id) = longest_idle(&mut g, 800);
    assert!(g.books.sailed > 0, "nobody went looking for work");
    assert!(idle < 300, "company {id} idle {idle} ticks");
}

/// Idle stretches on the game's galaxy, with idle companies roaming and
/// without (#355): run with
/// `cargo test --release judge_idle -- --ignored --nocapture`.
#[test]
#[ignore = "a tuning sweep, slow: run by hand in release"]
fn judge_idle() {
    for idle in [u32::MAX, 15, 30, 60] {
        let rules = Rules {
            merc_idle: idle,
            ..Rules::default()
        };
        let mut g = Galaxy::with_size(super::save::SEED, SYSTEMS, REGION, rules);
        for _ in 0..200 {
            g.step();
        }
        let (worst, id) = longest_idle(&mut g, 2000);
        let b = g.books;
        println!(
            "merc_idle {idle}: longest idle {worst} (company {id})  sailed {}  signed {} won {} broken {} expired {}  companies {}",
            b.sailed,
            b.signed,
            b.won,
            b.broken,
            b.expired,
            g.companies.len()
        );
    }
}

/// Your company's contracts, as they end, are kept for the summary screen
/// (#414): which contract, how it ended and its books; only the last few.
#[test]
fn your_ended_contracts_are_kept() {
    use super::theater::{ENDED_KEPT, Outcome};
    let mut g = Galaxy::with_size(3, SYSTEMS / 4, REGION * 0.5, Rules::default());
    for _ in 0..60 {
        g.step();
    }
    g.await_work_staying(None, 1, 400).0.expect("work");
    // Every contract of yours seen ending; `ended` itself never holds more
    // than ENDED_KEPT, so it's these that say when a few more have ended.
    let mut seen = std::collections::HashSet::new();
    for _ in 0..1500 {
        g.step();
        g.await_work_staying(None, 1, 400);
        seen.extend(g.ended.iter().map(|e| (e.ship, e.signed)));
        if seen.len() > ENDED_KEPT {
            break;
        }
    }
    assert!(
        seen.len() > ENDED_KEPT,
        "only {} contracts of yours ended",
        seen.len()
    );
    assert_eq!(g.ended.len(), ENDED_KEPT);
    for e in &g.ended {
        assert!(e.tick >= e.signed);
        assert!(e.lost <= e.committed);
        // Spent means every hull committed is gone (#429).
        assert!(e.outcome != Outcome::Spent || e.lost == e.committed);
        assert!(e.outcome != Outcome::Fell || e.defend);
    }
    // Ended in order, and not the one your company's still on.
    assert!(g.ended.windows(2).all(|w| w[0].tick <= w[1].tick));
    let now = g.crew_company().and_then(|c| {
        g.companies[c]
            .contract
            .map(|k| (g.companies[c].id, k.signed))
    });
    assert!(g.ended.iter().all(|e| Some((e.ship, e.signed)) != now));
}

/// A contract commits all its hulls (#402, #429): a company down to its
/// last few fights on with them, and is spent only once they're all gone.
#[test]
fn a_contract_is_spent_only_with_every_hull_gone() {
    let spent = |field: u32| {
        let mut g = Galaxy::warmed_by(7, 200);
        let c = g
            .companies
            .iter()
            .position(|c| c.contract.is_some())
            .expect("a company under contract");
        let tick = g.tick;
        let k = g.companies[c].contract.as_mut().unwrap();
        k.field = field;
        k.signed = tick;
        let (t, f) = (k.target as usize, k.side as usize);
        g.systems[t].hulls[f] = g.systems[t].hulls[f].max(field);
        let broken = g.books.broken;
        g.mercs();
        g.books.broken > broken
    };
    for field in [3, 5, 9, 15] {
        assert!(!spent(field), "spent with {field} hulls left");
    }
    assert!(spent(0), "not spent with none left");
}

/// A battle goes on to the last hull (mercs' included): a side with a
/// single hull in a system is still at war there, and a battle can take it.
#[test]
fn a_battle_goes_on_to_the_last_hull() {
    let mut g = Galaxy::with_size(1, 50, Vec2::splat(20.0), Rules::default());
    let s = 0;
    let owner = g.systems[s].owner();
    let other = (owner + 1) % SIDES;
    g.systems[s].hulls = [0; SIDES];
    g.systems[s].hulls[owner] = 40;
    g.systems[s].hulls[other] = 1;
    assert!(g.systems[s].at_war());
    assert!(g.fighting(s, other));
    // Fought for long enough, its last hull goes.
    for _ in 0..200 {
        g.battles(None);
        if g.systems[s].hulls[other] == 0 {
            break;
        }
    }
    assert_eq!(g.systems[s].hulls[other], 0);
    assert!(!g.systems[s].at_war());
}

/// Give every realm plenty of pilots, and every side in system 0 a realm,
/// so battle tests aren't limited by pilots.
fn crewed(g: &mut Galaxy) {
    for f in 0..SIDES {
        if g.systems[0].realms[f] == super::realm::NONE {
            let r = g
                .living()
                .find(|(_, x)| x.faction as usize == f)
                .map(|(r, _)| r as u32);
            g.systems[0].realms[f] = r.unwrap_or_else(|| g.found(f as u8, 0));
        }
    }
    for realm in &mut g.realms {
        realm.pilots = 10_000;
    }
}

/// Battles lose whole hulls, rolled: the same fight comes out differently
/// from one roll to the next.
#[test]
fn losses_are_rolled() {
    let mut g = Galaxy::with_size(1, 50, Vec2::splat(20.0), Rules::default());
    let s = 0;
    let owner = g.systems[s].owner();
    let other = (owner + 1) % SIDES;
    crewed(&mut g);
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..40 {
        g.systems[s].hulls = [0; SIDES];
        g.systems[s].hulls[owner] = 100;
        g.systems[s].hulls[other] = 100;
        g.battles(None);
        seen.insert(100 - g.systems[s].hulls[owner]);
    }
    assert!(seen.len() > 3, "always the same losses: {seen:?}");
}

/// In the galaxy's battles every kill is some pilot's (#438): a company's
/// pilots learn from the wrecks their side leaves, a part at a time.
#[test]
fn galaxy_kills_teach_merc_pilots() {
    let mut g = Galaxy::warmed_by(7, 200);
    let c = g
        .companies
        .iter()
        .position(|c| c.contract.is_some_and(|k| k.field >= 20))
        .expect("a company under contract");
    let k = g.companies[c].contract.unwrap();
    let (s, f) = (k.target as usize, k.side as usize);
    // Their enemy here geared well above them, and plenty of fighting.
    let e = (0..SIDES).find(|&e| e != f).unwrap();
    g.systems[s].hulls[e] = g.systems[s].hulls[e].max(200);
    if let Some(r) = Some(g.systems[s].realms[e]).filter(|&r| r != super::realm::NONE) {
        g.realms[r as usize].tier = 20;
    } else {
        g.claim(s, e);
        let r = g.systems[s].realms[e];
        g.realms[r as usize].tier = 20;
    }
    let before: f32 = g.companies[c]
        .pilots
        .iter()
        .map(|p| {
            crate::economy::PARTS
                .iter()
                .map(|&x| p.known.of(x))
                .sum::<f32>()
        })
        .sum();
    for _ in 0..5 {
        g.battles(None);
    }
    let after: f32 = g.companies[c]
        .pilots
        .iter()
        .map(|p| {
            crate::economy::PARTS
                .iter()
                .map(|&x| p.known.of(x))
                .sum::<f32>()
        })
        .sum();
    assert!(after > before, "nothing learned: {before} -> {after}");
}

/// Every hull that comes into or goes out of a system is in its moves
/// (#445): reinforcements, mercs, yards, wear and losses account for
/// every change in a side's count, so the missions screen never shows one
/// it can't explain. A played battle starts them over.
#[test]
fn moves_account_for_every_hull() {
    let mut g = small_war();
    g.clear_moves();
    let before: Vec<[u32; SIDES]> = g.systems.iter().map(|s| s.hulls).collect();
    for _ in 0..40 {
        g.step();
    }
    let mut seen = Moves::default();
    for (s, sys) in g.systems.iter().enumerate() {
        for f in 0..SIDES {
            let m = g.moves[s][f];
            assert_eq!(
                sys.hulls[f] as i64 - before[s][f] as i64,
                m.net(),
                "system {s} side {f}: {m:?}"
            );
            seen.built += m.built;
            seen.worn += m.worn;
            seen.lost += m.lost;
            seen.arrived += m.arrived;
            seen.hired += m.hired;
        }
    }
    // A war's worth of ticks does all of it somewhere.
    assert!(
        seen.built > 0 && seen.worn > 0 && seen.lost > 0 && seen.arrived > 0,
        "{seen:?}"
    );
    g.clear_moves();
    assert!(g.moves.iter().flatten().all(Moves::is_empty));
}

/// A pocket builds hulls as fast as it would anywhere else (#446).
#[test]
fn pockets_build_at_full_rate() {
    let mut g = Galaxy::warmed_by(7, 200);
    let s = (0..g.systems.len())
        .find(|&s| g.production(s)[g.systems[s].owner()] > 0.0)
        .unwrap();
    let before = g.built(s);
    g.cut[s] = true;
    assert_eq!(g.built(s), before);
    assert_eq!(g.built(s), g.production(s));
}

/// The tick summary (#475) says what a system's sides had when the battle
/// began and have now, every side there either time, and how many ticks.
#[test]
fn tick_summary_counts_from_the_battle() {
    let mut g = small_war();
    g.clear_moves();
    let at = g.tick;
    let owners = g.owners();
    let before: Vec<[u32; SIDES]> = g.systems.iter().map(|s| s.hulls).collect();
    for _ in 0..12 {
        g.step();
    }
    let mut changed = false;
    for s in 0..g.systems.len() {
        let t = g.tick_summary(s, at, &owners);
        let sys = &g.systems[s];
        let flipped = (sys.sites.0..sys.sites.1)
            .filter(|&i| g.sites[i as usize].owner != owners[i as usize])
            .count();
        assert_eq!(
            t.flips.len(),
            flipped,
            "system {s}: every site that changed hands"
        );
        assert_eq!(t.ticks, 12);
        for f in 0..SIDES {
            let rows: Vec<_> = t.sides.iter().filter(|c| c.side == f).collect();
            let (was, now) = (before[s][f], g.systems[s].hulls[f]);
            if rows.is_empty() {
                assert_eq!((was, now), (0, 0), "system {s} side {f} left out");
                continue;
            }
            // The side's own hulls, then its mercs', if any.
            assert!(!rows[0].merc && rows[1..].iter().all(|c| c.merc));
            assert!(rows.len() <= 2);
            let sum = |n: fn(&&theater::SideChange) -> u32| rows.iter().map(n).sum::<u32>();
            assert_eq!(sum(|c| c.now), now, "system {s} side {f}");
            // Wear's split between them by chance; with no mercs it's exact.
            if g.merc_moves[s][f] == Moves::default() {
                assert_eq!(sum(|c| c.before), was, "system {s} side {f}");
            }
            changed |= was != now;
        }
        let own: Vec<u32> = t
            .sides
            .iter()
            .filter(|c| !c.merc)
            .map(|c| {
                c.now
                    + t.sides
                        .iter()
                        .filter(|m| m.merc && m.side == c.side)
                        .map(|m| m.now)
                        .sum::<u32>()
            })
            .collect();
        assert!(own.windows(2).all(|w| w[0] >= w[1]));
    }
    assert!(changed, "nothing changed: the test shows nothing");
}

/// A side fires with no more hulls than it has pilots for, but loses
/// hulls as fast as the enemy can kill them.
#[test]
fn pilots_cap_fire_not_losses() {
    let mut g = Galaxy::with_size(1, 50, Vec2::splat(20.0), Rules::default());
    let s = 0;
    let owner = g.systems[s].owner();
    let other = (owner + 1) % SIDES;
    crewed(&mut g);
    g.systems[s].hulls = [0; SIDES];
    g.systems[s].hulls[owner] = 100;
    g.systems[s].hulls[other] = 100;
    let r = g.systems[s].realms[owner] as usize;
    g.realms[r].pilots = 5;
    g.share_pilots();
    assert_eq!(g.firing(s, owner), 5.0);
    assert_eq!(g.firing(s, other), g.rules.frontage);
    g.battles(None);
    assert!(
        100 - g.systems[s].hulls[owner] > 5,
        "losses capped by pilots"
    );
}

/// A realm pays its pilots as far as it can, and trains toward what its
/// income pays for a few at a time.
#[test]
fn realms_pay_and_train_pilots() {
    let rules = Rules {
        pilot_upkeep: 0.01,
        ..Rules::default()
    };
    let mut r = super::realm::Realm::new(0, 0, 0);
    r.income = 4.0;
    // Affords 100; at peace, uses a front's worth.
    assert_eq!(r.pilots_wanted(&rules), rules.frontage as u32);
    r.fronts = 1;
    assert_eq!(r.pilots_wanted(&rules), 100);
    r.crystals = 100.0;
    r.keep_pilots(&rules);
    assert_eq!(r.pilots, rules.train, "trains a few a tick");
    // Broke, it keeps only those it can pay.
    r.pilots = 50;
    r.crystals = 0.1;
    r.keep_pilots(&rules);
    assert!(r.pilots <= 10 + rules.train, "{}", r.pilots);
    assert!(r.crystals < 0.1);
}

/// How realms measure up: their size, who can fight and afford to, and
/// whether pilots are short. Leaves out realms that can reach nobody.
#[test]
#[ignore = "a tuning report: run by hand in release"]
fn judge_realms() {
    for seed in [121, 122] {
        let mut g = Galaxy::with_size(seed, SYSTEMS, REGION, Rules::default());
        while g.tick < 1500 {
            g.step();
        }
        let q = |mut v: Vec<f32>| {
            v.sort_by(f32::total_cmp);
            [0.1, 0.5, 0.9].map(|p| (v[((v.len() - 1) as f32 * p) as usize] * 10.0).round() / 10.0)
        };
        let live: Vec<usize> = g
            .living()
            .map(|(r, _)| r)
            .filter(|&r| {
                g.territory(r)
                    .iter()
                    .any(|&s| g.neighbors(s as usize).count() > 0)
            })
            .collect();
        let near = |r: usize| {
            g.territory(r).iter().any(|&s| {
                g.neighbors(s as usize).any(|(o, _)| {
                    let h = g.holder(o);
                    h != super::realm::NONE && g.realms[h as usize].faction != g.realms[r].faction
                })
            })
        };
        let border: Vec<usize> = live.iter().copied().filter(|&r| near(r)).collect();
        let can = border
            .iter()
            .filter(|&&r| g.realms[r].offensive.is_some() || g.realms[r].bank() >= g.rules.launch)
            .count();
        let (mut sides, mut short, mut seats, mut fire) = (0, 0, 0.0, 0.0);
        for s in 0..g.systems.len() {
            for f in 0..SIDES {
                if g.fighting(s, f) {
                    let most = (g.systems[s].hulls[f] as f32).min(g.rules.frontage);
                    sides += 1;
                    short += (g.firing(s, f) < most - 0.5) as u32;
                    seats += most;
                    fire += g.firing(s, f);
                }
            }
        }
        let pilots = |of: &[usize]| of.iter().map(|&r| g.realms[r].pilots).sum::<u32>();
        println!(
            "SZ seed {seed}: {} realms, systems 10/50/90 {:?}, enemy in range {:.0}%, of those can push {:.0}% | pilots: border {} interior {}, seats in fights {seats:.0}, filled {:.0}%, sides short {:.0}% of {sides} | companies {} signed {} | share {:?}",
            live.len(),
            q(live.iter().map(|&r| g.territory(r).len() as f32).collect()),
            100.0 * border.len() as f32 / live.len() as f32,
            100.0 * can as f32 / border.len() as f32,
            pilots(&border),
            pilots(&live) - pilots(&border),
            100.0 * fire / seats.max(1.0),
            100.0 * short as f32 / sides.max(1) as f32,
            g.companies.len(),
            g.books.signed,
            g.share.map(|s| (s * 100.0).round()),
        );
    }
}

/// Taking a system captures its share of the loser's bank, the last one all
/// that's left; and however big a bank is, it's kept.
#[test]
fn conquest_captures_the_bank() {
    let mut g = Galaxy::with_size(2, SYSTEMS / 10, REGION * 0.3, Rules::default());
    let (r, territory) = g
        .living()
        .map(|(r, _)| (r, g.territory(r).to_vec()))
        .find(|(_, t)| t.len() >= 3)
        .unwrap();
    let f = g.realms[r].faction as usize;
    let w = (f + 1) % SIDES;
    let n = territory.len();
    g.realms[r].crystals = 100.0;
    g.realms[r].systems = n as u32;
    let total = |g: &Galaxy| g.realms.iter().map(|x| x.crystals as f64).sum::<f64>();
    let before = total(&g);
    for (k, &s) in territory.iter().enumerate() {
        let s = s as usize;
        let sites: Vec<usize> = (g.systems[s].sites.0 as usize..g.systems[s].sites.1 as usize)
            .filter(|&i| g.sites[i].owner as usize == f)
            .collect();
        for i in sites {
            g.win(s, i, w);
        }
        let left = 100.0 * (n - k - 1) as f32 / n as f32;
        assert!(
            (g.realms[r].crystals - left).abs() < 0.01,
            "{} of {left}",
            g.realms[r].crystals
        );
    }
    assert!((total(&g) - before).abs() < 0.1, "none lost");
    // However big, a bank stays.
    let by = g.living().map(|(r, _)| r).next().unwrap();
    g.realms[by].crystals = 1e6;
    g.produce();
    assert!(g.realms[by].crystals > 0.99e6);
}
