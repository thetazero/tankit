use super::*;
use crate::ai::nav::NavGrid;
use crate::map::procgen::foundry::Part;
use crate::map::procgen::skeleton::LotKind;
use crate::map::procgen::{check, map};

fn spec(seed: u64) -> Spec {
    let k = seed as usize % 3;
    crate::hoard_spec(seed, Ground::ALL[k], Stretch::ALL[k])
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
    (3..6).for_each(check_dressed);
}

/// Blocks, trains and ways between the sides, who start far apart.
#[test]
fn every_stretch_is_a_hoard() {
    for seed in 0..6 {
        let sk = skeleton(&spec(seed));
        let blocks = sk
            .lots
            .iter()
            .filter(|l| matches!(l.kind, LotKind::Works(Part::Block { .. })))
            .count();
        assert!(blocks >= 60, "seed {seed}: {blocks} blocks");
        let [a, b] = [0, 1].map(|k| sk.bases[k].middle());
        let apart = a.distance(b);
        assert!(apart > sk.size * 0.5, "seed {seed}: {apart} m apart");
        assert!(sk.tracks.len() >= 4, "seed {seed}: {}", sk.tracks.len());
    }
}

/// Bots get to each other and fight, and stay on their wheels.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow unoptimized: run with cargo test --release"
)]
fn bots_fight_in_it() {
    use crate::combat::{Match, Phase};
    for seed in 1..2u64 {
        let mut g = Match::new(map(&spec(seed)), 0, 11, seed);
        g.start();
        g.set_contract(crate::mission::Contract::Hold);
        g.bots_only(seed);
        while g.phase == Phase::Playing {
            g.step(crate::sim::Command::default(), false);
            g.sounds.clear();
        }
        let kills: u32 = g.pilots().iter().map(|p| p.kills).sum();
        println!("seed {seed}: {kills} kills");
        assert!(kills > 10, "seed {seed}: {kills} kills");
    }
}
