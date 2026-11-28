use super::*;
use crate::ai::nav::NavGrid;
use crate::map::procgen::foundry::Part;
use crate::map::procgen::skeleton::LotKind;
use crate::map::procgen::{check, map};

fn spec(seed: u64) -> Spec {
    crate::hoard_spec(seed, Ground::ALL[seed as usize % 3])
}

fn check_dressed(seed: u64) {
    let m = map(&spec(seed));
    if let Err(why) = check::playable(&m) {
        panic!("seed {seed}: {why}");
    }
    let open = m.open_spots.len();
    assert!(open > 100, "seed {seed}: {open} open");
    let repairs = m.repair_spots.len();
    assert!(repairs >= 3, "seed {seed}: {repairs} repair drops");
    assert_eq!((m.vistas.len(), m.bases.len()), (2, 2), "seed {seed}");
    let nav = NavGrid::build(&m);
    let from = m.bases[0][m.bases[0].len() / 2];
    for p in m.bases.iter().flatten() {
        assert!(!nav.path(from, *p).is_empty(), "seed {seed}: {p} cut off");
    }
}

#[test]
fn every_seed_is_playable_dressed() {
    (0..3).for_each(check_dressed);
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow unoptimized: run with cargo test --release"
)]
fn many_seeds_are_playable() {
    (0..6).for_each(check_dressed);
}

/// Stacks enough to call it a hoard, the sides far apart, and the counting
/// floor over the land round it.
#[test]
fn every_hoard_is_one() {
    for seed in 0..12 {
        let sk = skeleton(&spec(seed));
        let count = |part| {
            let is = |l: &&crate::map::procgen::skeleton::Lot| l.kind == LotKind::Works(part);
            sk.lots.iter().filter(is).count()
        };
        let stacks = count(Part::Stack) + count(Part::Monolith);
        // One at each of the counting floor's corners among them.
        let near = |l: &&crate::map::procgen::skeleton::Lot| {
            l.kind == LotKind::Works(Part::Stack) && l.at().distance(sk.heart) < 22.0
        };
        // (A road may take one's place.)
        assert!(sk.lots.iter().filter(near).count() >= 3, "seed {seed}");
        assert!(stacks >= 30, "seed {seed}: {stacks} stacks");
        let [a, b] = [0, 1].map(|k| sk.bases[k].middle());
        let apart = a.distance(b);
        assert!(apart > sk.size * 0.6, "seed {seed}: {apart} m apart");
        let top = sk.ground(sk.heart);
        let foot = (0..8)
            .map(|k| glam::Vec2::from_angle(k as f32 * 0.785) * 46.0 + sk.heart)
            .map(|p| sk.ground(p))
            .fold(f32::MIN, f32::max);
        assert!(top > foot + 2.0, "seed {seed}: {top} over {foot}");
    }
}

/// Whatever its ground, a seed's the same hoard.
#[test]
fn the_ground_changes_no_layout() {
    let on = Ground::ALL.map(|g| skeleton(&crate::hoard_spec(2, g)));
    for sk in &on[1..] {
        assert_eq!((sk.heart, sk.lots.len()), (on[0].heart, on[0].lots.len()));
    }
    assert_ne!(skeleton(&spec(1)).heart, on[0].heart);
}
