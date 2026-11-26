use super::*;
use crate::map::procgen::{check, map};

const LOOKS: [Look; 4] = [Look::Imperium, Look::Resonant, Look::Uthenn, Look::Ithessar];
const SEEDS: [u64; 6] = [0, 1, 2, 3, 4, 5];

/// The grounds in turn.
fn ground_of(seed: u64) -> Ground {
    Ground::ALL[seed as usize % 3]
}

fn spec(seed: u64, look: Look) -> Spec {
    crate::foundry_spec(seed, look, ground_of(seed))
}

fn check_dressed(seed: u64, look: Look) {
    let what = format!("{look:?} seed {seed}");
    let m = map(&spec(seed, look));
    if let Err(why) = check::playable(&m) {
        panic!("{what}: {why}");
    }
    let open = m.open_spots.len();
    assert!(open > 100, "{what}: {open} open");
    assert!(m.repair_spots.len() >= 3, "{what}: {:?}", m.repair_spots);
    assert_eq!(m.vistas.len(), 2, "{what}");
    assert_eq!(m.bases.len(), 2, "{what}");
    for &p in &m.repair_spots {
        let g = m.ground_height(p.x, p.y);
        let from = Vec3::new(p.x, g + 0.5, p.y);
        let over = m.oboxes.iter().find(|o| o.ray_hit(from, Vec3::Y, 80.0));
        assert!(over.is_none(), "{what}: repair drop at {p} under {over:?}");
    }
    let nav = NavGrid::build(&m);
    let from = m.bases[0][m.bases[0].len() / 2];
    for p in m.bases.iter().flatten() {
        assert!(!nav.path(from, *p).is_empty(), "{what}: {p} cut off");
    }
    for q in m.routes.iter().flatten() {
        let q = Vec2::new(q.x, q.z);
        assert!(
            !nav.is_blocked(q),
            "{what}: a route runs into something at {q}"
        );
    }
    m.assert_edge_walled(0.3, |_| false);
}

/// Every seed, each in turn in another style.
#[test]
fn every_seed_is_playable_dressed() {
    for seed in SEEDS {
        check_dressed(seed, LOOKS[seed as usize % 4]);
    }
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow unoptimized: run with cargo test --release"
)]
fn many_seeds_are_playable_in_every_style() {
    for seed in 0..40 {
        for look in LOOKS {
            check_dressed(seed, look);
        }
    }
}

/// Whoever built it, a seed's the same works: what's driven on and round is
/// the skeleton's. At tank height nothing solid stands off its boxes.
#[test]
fn styles_keep_to_the_skeleton() {
    for (seed, look) in LOOKS.into_iter().enumerate() {
        let spec = spec(seed as u64, look);
        let sk = skeleton(&spec);
        let m = dressed(&spec, ground_of(spec.seed), &sk);
        let mut grey = Map::empty(sk.size);
        sk.boxes(&mut grey);
        let solid = |o: &&crate::map::OBox| {
            let g = sk.ground(o.center);
            let (lo, hi) = o.y_range();
            o.solid && lo < g + 2.5 && hi > g + 0.6
        };
        for o in m.oboxes.iter().filter(solid) {
            // The bases' stores and masts are behind the spawns, by the wall.
            if o.center.x < 13.0 || o.center.x > sk.size - 13.0 {
                continue;
            }
            for p in o.base_shadow(2.5) {
                let near = grey
                    .oboxes
                    .iter()
                    .map(|b| b.distance(p))
                    .fold(f32::MAX, f32::min);
                assert!(near < 0.6, "{look:?}: {near} m off the skeleton: {o:?}");
            }
        }
        assert!(m.crystals.iter().all(|c| !c.solid), "{look:?}");
        // And nothing solid of the skeleton's is left out.
        for b in grey.oboxes.iter().filter(solid) {
            let stood = m
                .oboxes
                .iter()
                .filter(solid)
                .any(|o| o.distance(b.center) < 0.5);
            assert!(stood, "{look:?}: nothing stands at {b:?}");
        }
    }
}

/// A tank drives in through every door of every hall.
#[test]
fn every_door_is_open() {
    for seed in SEEDS {
        let sk = skeleton(&spec(seed, Look::Imperium));
        let nav = NavGrid::build(&greybox_of(&sk, &crate::map::DEFAULT_SIDES));
        for d in sk
            .halls
            .iter()
            .flat_map(|h| h.doors.iter().map(move |d| h.door(d)))
        {
            assert!(
                !nav.is_blocked(d),
                "seed {seed}: the door at {d} is blocked"
            );
            assert!(!nav.path(sk.bases[0], d).is_empty(), "seed {seed}: {d}");
        }
    }
}

/// Most of the Hold ring's floor is open to stand on.
#[test]
fn the_casting_floor_is_open() {
    for seed in SEEDS {
        let spec = spec(seed, LOOKS[seed as usize % 4]);
        let sk = skeleton(&spec);
        let nav = NavGrid::build(&dressed(&spec, ground_of(spec.seed), &sk));
        let ring: Vec<Vec2> = (0..400)
            .map(|k| Vec2::new((k % 20) as f32 - 9.5, (k / 20) as f32 - 9.5) * 1.3)
            .filter(|p| p.length() < crate::hold::RADIUS)
            .collect();
        let open = ring
            .iter()
            .filter(|&&p| !nav.is_blocked(sk.hold + p))
            .count();
        assert!(
            open * 10 > ring.len() * 7,
            "seed {seed}: {open} of {}",
            ring.len()
        );
    }
}

/// Slag's there to drive over.
#[test]
fn the_slag_is_drivable() {
    for seed in SEEDS {
        let sk = skeleton(&spec(seed, Look::Imperium));
        let t = sk.terrain();
        let mut highest = 0.0f32;
        for iz in 0..t.nz {
            for ix in 0..t.nx {
                let n = t.normal(ix, iz);
                let slope = Vec2::new(n.x, n.z).length() / n.y;
                assert!(slope < 0.65, "seed {seed}: {slope} steep");
                highest = highest.max(t.at(ix, iz));
            }
        }
        assert!(highest > 3.0, "seed {seed}: only {highest} m of slag");
    }
}

#[test]
fn different_seeds_make_different_foundries() {
    let sk = SEEDS.map(|s| skeleton(&spec(s, Look::Imperium)));
    for (i, a) in sk.iter().enumerate() {
        for b in &sk[i + 1..] {
            assert!(a.hold != b.hold && a.stock.len() + a.wagons.len() > 20);
        }
    }
    let again = skeleton(&spec(SEEDS[1], Look::Uthenn));
    assert_eq!(again.hold, sk[1].hold);
    assert_eq!(again.stock.len(), sk[1].stock.len());
}

/// A seed's the same works whatever the ground: what's solid doesn't move.
#[test]
fn grounds_differ_only_in_looks() {
    let solid = |g: Ground| {
        let sp = crate::foundry_spec(2, Look::Uthenn, g);
        let m = build(&sp, g);
        (
            m.oboxes.iter().filter(|o| o.solid).count(),
            m.hold_site,
            m.terrain.unwrap().heights,
        )
    };
    let [a, b, c] = Ground::ALL.map(solid);
    assert_eq!(a, b);
    assert_eq!(a, c);
    let lands = Ground::ALL.map(|g| g.land().open);
    assert!(lands[0] != lands[1] && lands[1] != lands[2]);
}

/// Two minutes of a 6v6 round: both sides score, and every bot gets out of
/// its base.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow unoptimized: run with cargo test --release"
)]
fn bots_fight_through_the_foundry() {
    for seed in 0..2 {
        let m = map(&spec(seed, LOOKS[seed as usize]));
        let mut g = crate::combat::Match::new(m, 0, 11, 7);
        g.start();
        let at = |g: &crate::combat::Match| -> Vec<Vec2> {
            let flat = |t: &crate::sim::Tank| Vec2::new(t.pos.x, t.pos.z);
            g.world.tanks.iter().map(flat).collect()
        };
        let start = at(&g);
        let mut furthest = vec![0.0f32; start.len()];
        for k in 0..(120.0 / crate::game::DT) as usize {
            g.step(crate::sim::Command::default(), false);
            if k % 120 == 0 {
                for (i, p) in at(&g).into_iter().enumerate() {
                    furthest[i] = furthest[i].max(p.distance(start[i]));
                }
            }
        }
        let (a, b) = (g.score(0), g.score(1));
        assert!(a > 0 && b > 0, "seed {seed}: {a} : {b}");
        for (i, d) in furthest.iter().enumerate().skip(1) {
            assert!(*d > 35.0, "seed {seed}: bot {i} only got {d} m out");
        }
    }
}

/// The works on the shared land: every seed's first playable draw drives
/// everywhere it must, whoever's it is, and some have bridges and tunnels.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow unoptimized: run with cargo test --release"
)]
fn plans_play_and_some_cross() {
    use crate::map::procgen::crossing::Kind;
    let (mut bridges, mut tunnels, mut big, mut lots) = (0, 0, 0, 0);
    for seed in 0..24 {
        let sp = spec(seed, LOOKS[seed as usize % 4]);
        let sk = works_plan(&sp);
        if let Err(why) = check::playable(&sk.greybox(&sp.sides)) {
            panic!("seed {seed}: {why}");
        }
        let has = |k: Kind| sk.crossings.iter().any(|c| c.kind == k) as i32;
        bridges += has(Kind::Bridge);
        tunnels += has(Kind::Tunnel);
        use crate::map::procgen::skeleton::LotKind;
        big += sk
            .lots
            .iter()
            .filter(|l| matches!(l.kind, LotKind::Works(p) if p.big()))
            .count();
        lots += sk.lots.len();
    }
    eprintln!("of 24: {bridges} with bridges, {tunnels} with tunnels; {big} big, {lots} parts");
    assert!(bridges > 0 && tunnels > 0, "{bridges} {tunnels}");
}
