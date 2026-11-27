use super::*;
use crate::ai::nav::NavGrid;
use crate::map::procgen::skeleton::LotKind;
use crate::map::procgen::{check, map};
use glam::Vec2;

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
    let nav = NavGrid::build(&m);
    if let Err(why) = check::playable_on(&m, &nav) {
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
    let from = m.bases[0][m.bases[0].len() / 2];
    for p in m.bases.iter().flatten() {
        assert!(!nav.path(from, *p).is_empty(), "{what}: {p} cut off");
    }
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
fn many_seeds_are_playable_across_the_styles() {
    // (The first few in every style, the rest in one each, by turns.)
    for seed in 0..40 {
        for (k, look) in LOOKS.into_iter().enumerate() {
            if seed < 8 || k == seed as usize % 4 {
                check_dressed(seed, look);
            }
        }
    }
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
    let (mut railed, mut spurs) = (0, 0);
    for seed in 0..24 {
        let sp = spec(seed, LOOKS[seed as usize % 4]);
        let sk = skeleton(&sp);
        if let Err(why) = check::playable(&sk.greybox(&sp.sides)) {
            panic!("seed {seed}: {why}");
        }
        let has = |k: Kind| sk.crossings.iter().any(|c| c.kind == k) as i32;
        railed += !sk.tracks.is_empty() as i32;
        spurs += sk.tracks.len().saturating_sub(1);
        bridges += has(Kind::Bridge);
        tunnels += has(Kind::Tunnel);
        big += sk
            .lots
            .iter()
            .filter(|l| matches!(l.kind, LotKind::Works(p) if p.big()))
            .count();
        lots += sk.lots.len();
    }
    eprintln!(
        "of 24: {bridges} with bridges, {tunnels} with tunnels; {big} big, {lots} parts; {railed} railed, {spurs} spurs"
    );
    assert!(railed >= 22, "{railed} of 24 have a railway");
    assert!(bridges > 0 && tunnels > 0, "{bridges} {tunnels}");
}

/// Whoever built it, what's solid is the plan's: the greybox's boxes and the
/// dressed map's are the same over the works.
#[test]
fn styles_keep_to_the_parts() {
    for (seed, look) in LOOKS.into_iter().enumerate() {
        let spec = spec(seed as u64, look);
        let sk = skeleton(&spec);
        for lot in &sk.lots {
            let LotKind::Works(part) = lot.kind else {
                continue;
            };
            let solids = |st: Option<&dyn style::Works>| -> Vec<(Vec2, f32)> {
                let mut m = Map::empty(sk.size);
                build_part(&mut m, &lot.footprint, part, 0.0, st);
                m.oboxes
                    .iter()
                    .filter(|o| o.solid && o.y0 < 3.0)
                    .map(|o| (o.center, o.y0))
                    .collect()
            };
            let (grey, dressed) = (solids(None), solids(Some(works(look))));
            assert_eq!(grey, dressed, "{look:?} {part:?}");
        }
    }
}

#[test]
fn different_seeds_make_different_foundries() {
    let sk = SEEDS.map(|s| skeleton(&spec(s, Look::Imperium)));
    for (i, a) in sk.iter().enumerate() {
        for b in &sk[i + 1..] {
            assert!(a.heart != b.heart && a.lots.len() > 30);
        }
    }
    let again = lay_out(&spec(SEEDS[1], Look::Imperium));
    assert_eq!(again.heart, sk[1].heart);
    assert_eq!(again.lots.len(), sk[1].lots.len());
}
