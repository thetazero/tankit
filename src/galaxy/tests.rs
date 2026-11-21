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

/// Landing under an enemy shield is slow and costs crystals; once all of a
/// planet's generators are down it's free.
#[test]
fn shields_limit_landing() {
    let mut g = Galaxy::with_size(1, 50, Vec2::splat(20.0), Rules::default());
    let s = 0;
    let owner = g.systems[s].owner();
    let other = (owner + 1) % SIDES;
    let (rate, cost) = g.landing(s, other);
    assert!(rate.is_finite() && cost > 0.0);
    // The owner lands freely on its own world.
    assert_eq!(g.landing(s, owner), (f32::INFINITY, 0.0));
    let (a, b) = g.systems[s].sites;
    for site in &mut g.sites[a as usize..b as usize] {
        if site.planet == 0 && matches!(site.kind, SiteKind::Shield { .. }) {
            site.kind = SiteKind::Shield { down: REPAIR_TICKS };
        }
    }
    assert_eq!(g.landing(s, other), (f32::INFINITY, 0.0));
}

/// A downed generator takes several quiet ticks to come back up.
#[test]
fn repairs_take_several_ticks() {
    let mut g = Galaxy::with_size(1, 50, Vec2::splat(20.0), Rules::default());
    let owner = g.systems[0].owner();
    let (a, _) = g.systems[0].sites;
    g.sites[a as usize].kind = SiteKind::Shield { down: REPAIR_TICKS };
    for _ in 1..REPAIR_TICKS {
        g.hold(0, owner);
        assert_ne!(g.sites[a as usize].kind, SiteKind::Shield { down: 0 });
    }
    g.hold(0, owner);
    assert_eq!(g.sites[a as usize].kind, SiteKind::Shield { down: 0 });
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
            offensive_ticks += g.offensives.iter().filter(|o| o.is_some()).count() as u64;
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
            "seed {seed}: share {:4.1}..{:4.1}%  war mean {:4.1}% p10 {:4.1} p90 {:4.1} max {:4.1}  swing/100 {:4.1}%  flips/100 {:5.0}  pockets {:4.1} live {:4.1}  offensives {:3.1}  final {:?}",
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
            "no initiative",
            Rules {
                initiative: 0.0,
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
