use glam::{Vec2, Vec3};

use super::*;
use crate::ai::nav::NavGrid;
use crate::map::procgen::foundry::{Part, build_part};
use crate::map::procgen::skeleton::{LotKind, Rank};
use crate::map::procgen::{Look, check, map};

const LOOKS: [Look; 4] = [Look::Imperium, Look::Resonant, Look::Uthenn, Look::Ithessar];

fn spec(seed: u64, look: Look) -> Spec {
    crate::depot_spec(seed, look)
}

fn count(sk: &Skeleton, part: Part) -> usize {
    sk.lots
        .iter()
        .filter(|l| l.kind == LotKind::Works(part))
        .count()
}

fn check_dressed(seed: u64, look: Look) {
    let what = format!("{look:?} seed {seed}");
    let m = map(&spec(seed, look));
    if let Err(why) = check::playable(&m) {
        panic!("{what}: {why}");
    }
    assert!(
        m.open_spots.len() > 100,
        "{what}: {} open",
        m.open_spots.len()
    );
    assert!(m.repair_spots.len() >= 3, "{what}: {:?}", m.repair_spots);
    assert_eq!((m.vistas.len(), m.bases.len()), (2, 2), "{what}");
    assert_eq!(m.raid_stands.len(), 2, "{what}");
    let nav = NavGrid::build(&m);
    let from = m.bases[0][m.bases[0].len() / 2];
    for p in m.bases.iter().flatten().chain(&m.raid_stands) {
        assert!(!nav.path(from, *p).is_empty(), "{what}: {p} cut off");
    }
    for &p in &m.raid_stands {
        let g = m.ground_height(p.x, p.y);
        let up = Vec3::new(p.x, g + 0.5, p.y);
        let over = m
            .oboxes
            .iter()
            .any(|o| o.solid && o.ray_hit(up, Vec3::Y, 80.0));
        assert!(!over, "{what}: a stand under something");
    }
}

#[test]
fn every_seed_is_playable_dressed() {
    for seed in 0..4 {
        check_dressed(seed, LOOKS[seed as usize % 4]);
    }
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow unoptimized: run with cargo test --release"
)]
fn many_seeds_are_playable() {
    for seed in 0..24 {
        check_dressed(seed, LOOKS[seed as usize % 4]);
    }
}

/// It's a yard: sidings side by side, rows of cars on them, gantries over.
#[test]
fn every_depot_is_a_yard() {
    let mut gantries = 0;
    for seed in 0..12 {
        let sk = skeleton(&spec(seed, Look::Imperium));
        assert!(sk.tracks.len() >= 6, "seed {seed}: {}", sk.tracks.len());
        let [a, b] = [0, 1].map(|k| sk.bases[k].middle());
        assert!(
            a.distance(b) > sk.size * 0.6,
            "seed {seed}: bases {:.0} m apart",
            a.distance(b)
        );
        let cars = count(&sk, Part::Cars);
        assert!(cars >= 14, "seed {seed}: {cars} rows of cars");
        assert!(count(&sk, Part::WaterTower) >= 1, "seed {seed}: no tower");
        gantries += count(&sk, Part::Gantry);
        // Cars stand on the rails.
        for l in &sk.lots {
            if l.kind == LotKind::Works(Part::Cars) {
                let on = sk
                    .streets
                    .iter()
                    .any(|s| s.rank == Rank::Rail && s.distance(l.at()) < 0.1);
                assert!(on, "seed {seed}: cars off the rails at {}", l.at());
            }
        }
    }
    assert!(gantries >= 12, "{gantries} gantries on 12");
}

/// Whoever built it, what's solid is the plan's.
#[test]
fn styles_keep_to_the_parts() {
    let sk = skeleton(&spec(1, Look::Imperium));
    for lot in &sk.lots {
        let LotKind::Works(part) = lot.kind else {
            continue;
        };
        let solids = |look: Option<Look>| -> Vec<(Vec2, f32)> {
            let mut m = Map::empty(sk.size);
            let st = look.map(foundry::works);
            build_part(&mut m, &lot.footprint, part, 0.0, st);
            m.oboxes
                .iter()
                .filter(|o| o.solid && o.y0 < 3.0)
                .map(|o| (o.center, o.y0))
                .collect()
        };
        for look in LOOKS {
            assert_eq!(solids(None), solids(Some(look)), "{look:?} {part:?}");
        }
    }
}

/// Bots fight a whole Raid either way round, and flags get carried off.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow unoptimized: run with cargo test --release"
)]
fn bots_finish_a_raid() {
    use crate::combat::{Match, Phase};
    let mut taken = 0;
    for seed in 1..3u64 {
        let mut m = map(&spec(seed, LOOKS[seed as usize]));
        if seed % 2 == 1 {
            m.swap_sides(1);
        }
        let mut g = Match::new(m, 0, 11, seed);
        g.start();
        g.set_contract(crate::mission::Contract::Raid);
        g.bots_only(seed);
        let mut carried = false;
        while g.phase == Phase::Playing {
            g.step(crate::sim::Command::default(), false);
            g.sounds.clear();
            let r = g.raid.as_ref().unwrap();
            carried |= matches!(r.flag, crate::raid::Flag::Carried(_));
        }
        assert!(g.raid.as_ref().unwrap().outcome.is_some(), "seed {seed}");
        taken += carried as u32;
    }
    assert!(taken > 0, "no flag was ever picked up");
}

#[test]
fn different_seeds_make_different_depots() {
    let sk = [0, 1, 2].map(|s| skeleton(&spec(s, Look::Imperium)));
    assert!(sk[0].heart != sk[1].heart && sk[1].heart != sk[2].heart);
    let again = skeleton(&spec(1, Look::Imperium));
    assert_eq!(
        (again.heart, again.lots.len()),
        (sk[1].heart, sk[1].lots.len())
    );
}
