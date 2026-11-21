use super::*;

/// The user's target over a long run: "most of the galaxy is not at war, but
/// there is always war". No faction takes the galaxy and the fronts keep moving.
#[test]
fn the_war_goes_on_forever() {
    // A quarter-size galaxy at the same density, to keep the test quick.
    let mut g = Galaxy::with_size(7, SYSTEMS / 4, REGION * 0.5, Rules::default());
    let mut taken = 0;
    for t in 1..=2000 {
        g.step();
        taken += g.taken;
        let c = g.census();
        if t < 100 {
            continue;
        }
        assert!(c.at_war > 0.0, "tick {t}: no war anywhere");
        assert!(
            c.at_war < 0.1,
            "tick {t}: {:.1}% of the galaxy at war",
            c.at_war * 100.0
        );
        for f in 0..SIDES {
            assert!(
                (0.05..0.5).contains(&c.share[f]),
                "tick {t}: {} holds {:.1}%",
                Faction::ALL[f].name(),
                c.share[f] * 100.0
            );
        }
        if t % 200 == 0 {
            assert!(taken > 0, "tick {t}: no site changed hands in 200 ticks");
            taken = 0;
        }
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
    let t = std::time::Instant::now();
    for _ in 0..100 {
        g.step();
    }
    let per = t.elapsed() / 100;
    assert!(per.as_millis() < 10, "a tick took {per:?}");
}
