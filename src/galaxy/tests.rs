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
    let other = (owner + 1) % super::PEOPLES;
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
    // (split between the battles each is in)
    let split = |g: &Galaxy, f: usize| g.open_fire(s)[f] / g.battles_in(s)[f] as f32;
    let near = |a: f32, b: f32| (a - b).abs() < 1e-3;
    assert!(open[other] > 0.0 && near(fire[other], split(&g, other) * short));
    assert!(
        near(fire[owner], split(&g, owner)),
        "the defender is never short"
    );
    // Every generator down: no shield, no seat lost.
    for site in &mut g.sites[a as usize..b as usize] {
        if matches!(site.kind, SiteKind::Shield { .. }) {
            site.kind = SiteKind::Shield { damaged: true };
        }
    }
    assert!(near(g.fire(s, t)[other], split(&g, other)));
}

/// Shields don't stand in the way (#426): an attacker's win takes a site
/// at once, the planet's generators up or not, and goes for up ones first.
#[test]
fn a_win_takes_a_site_under_shields() {
    let mut g = Galaxy::with_size(1, 50, Vec2::splat(20.0), Rules::default());
    let s = 0;
    let owner = g.systems[s].owner();
    let other = (owner + 1) % super::PEOPLES;
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
    let other = (owner + 1) % super::PEOPLES;
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
    let other = (owner + 1) % super::PEOPLES;
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
    g.systems[s].hulls[owner] = 4000;
    g.systems[s].hulls[other] = 4000;
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
    let other = (owner + 1) % super::PEOPLES;
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
    g.systems[s].hulls[owner] = 400;
    g.systems[s].hulls[other] = 400;
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
    assert!(per.as_millis() < 40, "a tick took {per:?}");
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

/// Realms are the actors: every people's sites and hulls in a system are one
/// realm's, of that people (raiders have none), and every realm still around holds or fights somewhere.
#[test]
fn every_holding_is_a_realms() {
    let mut g = small_war();
    for _ in 0..60 {
        g.step();
        for sys in &g.systems {
            for f in (0..SIDES).filter(|&f| f != super::RAID) {
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
    // The first few hundred ticks shake out the bad ones; there's a trade left.
    assert!(
        end > start * 0.1,
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
        .map(|c| c.capital(g.rules.hull_cost))
        .sum();
    // Realms hire only when the battle math says it's worth it: it takes a
    // while for enough to be.
    for _ in 0..400 {
        g.step();
        for c in &g.companies {
            assert!(c.crystals >= 0.0);
            if let Some(k) = c.contract {
                if k.side as usize == super::RAID || k.field == 0 {
                    continue;
                }
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
    assert!(b.upfront > 0.0 && b.billed > 0.0 && b.sold > 0.0);
    let end: f32 = g
        .companies
        .iter()
        .map(|c| c.capital(g.rules.hull_cost))
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

/// A realm that can't pay a bill owes it: no company signs with it until
/// it's paid, and it pays it back out of its income (#340). (What it owes
/// A contract's on credit, no interest: the realm owes the company, pays it
/// back over the ticks the company waits, and nobody lends a realm more
/// than its income would clear in that.
#[test]
fn realms_hire_on_credit() {
    let mut g = small_war();
    for _ in 0..150 {
        g.step();
        for c in &g.companies {
            assert!(c.crystals >= 0.0, "a company paid out what it didn't have");
        }
    }
    let b = g.books;
    assert!(b.signed > 0 && b.financed > 0.0 && b.repaid > 0.0, "{b:?}");
    assert!(b.repaid <= b.financed + 1.0, "no interest: {b:?}");
    // A realm with no income gets no credit.
    let signed = g.books.signed;
    for r in &mut g.realms {
        (r.income, r.crystals) = (0.0, 0.0);
    }
    for r in 0..g.realms.len() as u32 {
        for t in 0..g.systems.len().min(40) {
            let f = g.realms[r as usize].faction as usize;
            let plan = super::merc::Plan {
                own: 50.0,
                ..Default::default()
            };
            assert!(!g.hire(t, f, r, plan, 0, 0.0));
        }
    }
    assert_eq!(g.books.signed, signed);
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

/// An idle company sails to where realms have been looking for one, if a
/// contract's likely to pay its fare there; not where it isn't.
#[test]
fn idle_companies_go_where_theyre_wanted() {
    let mut g = small_war();
    for _ in 0..100 {
        g.step();
    }
    for c in &mut g.companies {
        c.contract = None;
        c.idle_since = 0;
    }
    g.unmet.fill(0.0);
    let c = 0;
    let at = g.companies[c].at as usize;
    let here = g.systems[at].pos;
    let near = (0..g.systems.len())
        .filter(|&s| s != at)
        .min_by(|&a, &b| {
            let d = |s: usize| g.systems[s].pos.distance(here);
            d(a).total_cmp(&d(b))
        })
        .unwrap();
    // Nobody wanted anywhere: nobody goes.
    assert_eq!(g.seek_work(), 0);
    // Wanted nearby, and contracts pay: it goes.
    g.books.signed = 1;
    g.books.upfront = 1000.0;
    g.unmet[near] = 5.0;
    assert!(g.seek_work() >= 1);
    assert_eq!(g.companies[c].at as usize, near);
    // Wanted, but a contract wouldn't pay the fare: it stays.
    let mut h = small_war();
    h.unmet[near] = 5.0;
    h.books.signed = 1;
    h.books.upfront = 0.0;
    for c in &mut h.companies {
        c.contract = None;
        c.idle_since = 0;
    }
    assert_eq!(h.seek_work(), 0);
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
        // Spent means every hull committed is gone (#429); plundering, more
        // than three quarters (#546).
        let spent = match e.loot {
            Some(_) => e.lost * 4 > e.committed * 3,
            None => e.lost == e.committed,
        };
        assert!(e.outcome != Outcome::Spent || spent);
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
        let mut g = Galaxy::warmed_hired(7);
        let c = g
            .companies
            .iter()
            .position(|c| c.contract.is_some_and(|k| k.side as usize != super::RAID))
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
    let other = (owner + 1) % super::PEOPLES;
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
    let other = (owner + 1) % super::PEOPLES;
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
    let mut g = Galaxy::warmed_hired(7);
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
/// (#445): reinforcements, mercs, yards and losses account for
/// every change in a side's count, so the missions screen never shows one
/// it can't explain. A played battle starts them over.
#[test]
fn moves_account_for_every_hull() {
    let mut g = small_war();
    // (hulls are worth building)
    g.hull_worth.fill(1.0);
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
            seen.lost += m.lost;
            seen.arrived += m.arrived;
            seen.hired += m.hired;
        }
    }
    // A war's worth of ticks does all of it somewhere.
    assert!(
        seen.built > 0 && seen.lost > 0 && seen.arrived > 0,
        "{seen:?}"
    );
    g.clear_moves();
    assert!(g.moves.iter().flatten().all(Moves::is_empty));
}

/// The tick summary (#475) says what a system's sides had when the battle
/// began and have now, every side there either time.
#[test]
fn tick_summary_counts_from_the_battle() {
    let mut g = small_war();
    g.clear_moves();
    let owners = g.owners();
    let before: Vec<[u32; SIDES]> = g.systems.iter().map(|s| s.hulls).collect();
    for _ in 0..12 {
        g.step();
    }
    let mut changed = false;
    for s in 0..g.systems.len() {
        let t = g.tick_summary(s, 0, &owners);
        let sys = &g.systems[s];
        let flipped = (sys.sites.0..sys.sites.1)
            .filter(|&i| g.sites[i as usize].owner != owners[i as usize])
            .count();
        assert_eq!(
            t.flips.len(),
            flipped,
            "system {s}: every site that changed hands"
        );
        // The page runs from what each had to what it has.
        let (first, last) = (t.at(0.0), t.at(60.0));
        assert_eq!(first.hulls, t.before());
        assert_eq!(last.hulls, sys.hulls);
        for f in &t.flips {
            let owner =
                |t: &theater::Theater| t.sites.iter().find(|s| s.index == f.site).unwrap().owner;
            assert_eq!((owner(&first), owner(&last)), (f.from, f.to));
        }
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
            // With no mercs, it's exact.
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
    let other = (owner + 1) % super::PEOPLES;
    crewed(&mut g);
    g.systems[s].hulls = [0; SIDES];
    g.systems[s].hulls[owner] = 100;
    g.systems[s].hulls[other] = 100;
    let r = g.systems[s].realms[owner] as usize;
    g.realms[r].pilots = 5;
    g.share_pilots();
    assert_eq!(g.firing(s, owner), 5.0);
    let battles = g.battles_in(s)[other].max(1) as f32;
    assert_eq!(g.firing(s, other), g.rules.seats * battles);
    g.battles(None);
    assert!(
        100 - g.systems[s].hulls[owner] > 5,
        "losses capped by pilots"
    );
}

/// A realm pays its pilots as far as it can, trains some for the seats it has
/// to fill while one's worth
/// its training and pay over a career, takes on free ones first, and lets
/// them go when one isn't worth its pay.
#[test]
fn realms_pay_and_train_pilots() {
    let rules = Rules::default();
    let mut r = super::realm::Realm::new(0, 0, 0);
    r.crystals = 100.0;
    // Worth a lot: trained, for crystal.
    r.pilot_worth = 1.0;
    // With no seat to fill, it trains nobody.
    assert_eq!(r.keep_pilots(&rules, 0, 0), 0);
    assert_eq!(r.pilots, 0);
    r.seats_now = 100.0;
    assert_eq!(r.keep_pilots(&rules, 0, 0), 0);
    assert_eq!(r.pilots, rules.train);
    let spent = 100.0 - r.crystals;
    assert!((spent - rules.train as f32 * rules.train_cost).abs() < 1e-3);
    // Free ones cost no training.
    let before = r.crystals;
    r.keep_pilots(&rules, 0, 10);
    assert_eq!(r.hired, rules.train);
    assert!(before - r.crystals < 1.0);
    // Worth nothing: let go.
    r.pilot_worth = 0.0;
    let had = r.pilots;
    assert_eq!(r.keep_pilots(&rules, 0, 0), rules.train);
    assert_eq!(r.pilots, had - rules.train);
    // Broke, it keeps only those it can pay.
    (r.pilots, r.crystals, r.pilot_worth) = (50, 0.1, 1.0);
    r.keep_pilots(&rules, 0, 0);
    assert!(
        r.pilots as f32 * r.pay(&rules) <= 0.1 + 1e-3,
        "{}",
        r.pilots
    );
}

/// A vault changes hands only with the whole planet: its crystal goes with
/// it to a realm; a raiding company takes it and leaves the planet.
#[test]
fn vaults_fall_with_the_planet() {
    for raid in [false, true] {
        let mut g = Galaxy::with_size(2, SYSTEMS / 10, REGION * 0.3, Rules::default());
        let s = (0..g.systems.len())
            .find(|&s| g.systems[s].planets == 1 && g.holder(s) != super::realm::NONE)
            .unwrap();
        let f = g.systems[s].owner();
        let r = g.holder(s) as usize;
        let w = if raid {
            super::RAID
        } else {
            (f + 1) % super::PEOPLES
        };
        g.realms[r].crystals = 100.0;
        g.fill_vaults();
        let there = g.vaults_of(s)[0].crystals;
        assert!(there > 0.0);
        let purse = g.companies[0].crystals;
        if raid {
            g.companies[0].contract = Some(super::merc::Contract {
                side: super::RAID as u8,
                target: s as u32,
                ..Default::default()
            });
        }
        let (a, b) = g.systems[s].sites;
        for i in a as usize..b as usize {
            assert_eq!(g.vaults_of(s)[0].side as usize, f, "not till it's all gone");
            assert!(g.realms[r].crystals > 99.9);
            g.win(s, i, w);
        }
        assert!((g.realms[r].crystals - (100.0 - there)).abs() < 0.01);
        if raid {
            assert_eq!(g.vaults_of(s)[0].side as usize, f);
            assert!((g.companies[0].crystals - purse - there).abs() < 0.01);
        } else {
            assert_eq!(g.vaults_of(s)[0].side as usize, w);
        }
    }
}

/// Companies raid for themselves, and give back every site they held.
#[test]
fn companies_raid_and_leave() {
    let mut g = small_war();
    for r in &mut g.realms {
        r.crystals = 5000.0;
    }
    let mut raided = false;
    for _ in 0..150 {
        g.step();
        for (s, sys) in g.systems.iter().enumerate() {
            let here = g.companies.iter().any(|c| {
                c.contract
                    .is_some_and(|k| k.target as usize == s && k.side as usize == super::RAID)
            });
            raided |= here;
            assert!(
                here || (sys.held[super::RAID] == 0 && sys.hulls[super::RAID] == 0),
                "a raider's left something behind"
            );
        }
    }
    assert!(raided && g.books.raids > 0);
}

/// Hulls sent take time to fly, are nowhere meanwhile, and land when due.
#[test]
fn fleets_take_time_and_land() {
    let mut g = small_war();
    g.fleets.clear();
    while g.fleets.iter().all(|x| x.arrives < g.tick + 2) {
        g.step();
    }
    let x = *g
        .fleets
        .iter()
        .max_by_key(|x| x.arrives)
        .expect("a fleet in flight");
    let (t, f) = (x.to as usize, x.side as usize);
    let due = x.arrives;
    while g.tick + 1 < due {
        g.tick_once();
        assert!(g.fleets.iter().any(|y| y.to == x.to && y.arrives == due));
    }
    g.clear_moves();
    g.tick_once();
    assert!(g.moves[t][f].arrived >= x.hulls, "not landed");
    assert!(g.fleets.iter().all(|y| y.arrives > g.tick));
    assert!(g.flight(1.0) == 1 && g.flight(g.rules.ftl_speed * 2.5) == 3);
}

/// A pilot fights one battle a tick: a side's fire's split between the
/// battles it's in, so all of them together are no more than its one.
#[test]
fn a_pilot_fights_one_battle_a_tick() {
    let mut g = small_war();
    for _ in 0..40 {
        g.step();
    }
    g.share_pilots();
    let mut many = 0;
    for s in 0..g.systems.len() {
        let n = g.battles_in(s);
        let open = g.open_fire(s);
        for f in 0..SIDES {
            many += (n[f] > 1) as u32;
            let targets = g.targets(s, f);
            let whole: f32 = (g.systems[s].sites.0 as usize..g.systems[s].sites.1 as usize)
                .filter(|&t| {
                    let attacked =
                        (0..SIDES).any(|e| g.fighting(s, e) && g.targets(s, e).contains(&t));
                    attacked
                        && (g.sites[t].owner as usize == f
                            || (g.fighting(s, f) && targets.contains(&t)))
                })
                .map(|t| g.fire(s, t)[f])
                .sum();
            assert!(
                whole <= open[f] * 1.001 + 1e-3,
                "system {s} side {f}: {whole} of {}",
                open[f]
            );
        }
    }
    assert!(many > 0, "nobody in more than one battle");
}

/// Planets have 0 to 3 outposts, one or two mostly, which make nothing (#547).
#[test]
fn planets_have_outposts() {
    let g = Galaxy::new(1);
    let mut counts = [0u32; 4];
    for s in 0..g.systems.len() {
        for p in 0..g.systems[s].planets {
            let (a, b) = g.systems[s].sites;
            let n = (a as usize..b as usize)
                .filter(|&i| g.sites[i].planet == p && g.sites[i].kind == SiteKind::Outpost)
                .count();
            counts[n] += 1;
        }
    }
    let all: u32 = counts.iter().sum();
    let share = counts.map(|n| n as f32 / all as f32);
    for (k, want) in [0.1, 0.4, 0.4, 0.1].into_iter().enumerate() {
        assert!((share[k] - want).abs() < 0.03, "{share:?}");
    }
    // Worth nothing, and no hulls out of them.
    let mut bare = g.clone();
    for site in &mut bare.sites {
        if site.kind.produces() {
            site.kind = SiteKind::Outpost;
        }
    }
    assert!((0..bare.systems.len()).all(|s| bare.built(s) == [0.0; SIDES]));
    assert!(bare.realm_income().iter().all(|&x| x == 0.0));
}
