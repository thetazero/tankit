use super::dress::Claim;
use super::skeleton::*;
use super::*;

/// One seed of each form (see `form_of`).
const SEEDS: [u64; 4] = [0, 1, 2, 3];

fn spec(seed: u64) -> Spec {
    crate::desert_spec(seed)
}

#[test]
fn every_form_is_playable_dressed() {
    for seed in SEEDS {
        let m = Map::desert(seed);
        let what = format!("seed {seed} ({:?})", form_of(seed));
        assert_eq!(m.bases.len(), 2, "{what}");
        if let Err(why) = crate::map::procgen::check::playable(&m) {
            panic!("{what}: {why}");
        }
        assert!(
            m.open_spots.len() > 80,
            "{what}: {} open",
            m.open_spots.len()
        );
        assert!(m.repair_spots.len() >= 3, "{what}: {:?}", m.repair_spots);
        assert_eq!(m.vistas.len(), 2, "{what}");
    }
}

/// The forms are different land: how much of the arena is high ground.
#[test]
fn the_forms_are_different_land() {
    for seed in SEEDS {
        let sk = skeleton(&spec(seed));
        assert_eq!(sk.form, form_of(seed));
        let mut high = 0;
        let mut all = 0;
        let mut z = 5.0;
        while z < sk.size {
            let mut x = 5.0;
            while x < sk.size {
                all += 1;
                if sk.on_high(Vec2::new(x, z)) {
                    high += 1;
                }
                x += 10.0;
            }
            z += 10.0;
        }
        let f = high as f32 / all as f32;
        let (lo, hi) = match sk.form {
            Form::Erg => (0.02, 0.25),
            Form::Canyonlands => (0.4, 0.9),
            Form::Basin => (0.15, 0.7),
            Form::Escarpment => (0.1, 0.6),
        };
        assert!((lo..hi).contains(&f), "{:?}: {f} high ground", sk.form);
        assert!(
            sk.form == Form::Erg || !sk.ramps.is_empty(),
            "{:?}: no ramps",
            sk.form
        );
    }
}

/// Seeds of the same form lay out different maps, and the same seed the
/// same map.
#[test]
fn seeds_of_a_form_differ() {
    for form_seed in [1u64, 2] {
        let a = skeleton(&spec(form_seed));
        let b = skeleton(&spec(form_seed + 4));
        assert_eq!(a.form, b.form);
        assert!(
            a.size != b.size || a.bases != b.bases || a.ramps != b.ramps,
            "two {:?} seeds laid out the same desert",
            a.form
        );
    }
    let a = skeleton(&spec(3));
    let b = skeleton(&spec(3));
    assert_eq!(a.bases, b.bases);
    assert_eq!(a.ramps, b.ramps);
    assert_eq!(a.lots.len(), b.lots.len());
}

/// Nothing solid at tank height in the landmark's open ground.
#[test]
fn the_landmark_ground_is_open() {
    for seed in SEEDS {
        let m = Map::desert(seed);
        let sk = skeleton(&spec(seed));
        let lm = sk.landmark;
        assert_eq!(m.hold_site, Some(lm.at));
        let g = m.ground_height(lm.at.x, lm.at.y);
        let hit = m.oboxes.iter().find(|o| {
            let (lo, hi) = o.y_range();
            o.solid && lo < g + 3.5 && hi > g + 0.5 && o.distance(lm.at) < lm.clear - 2.0
        });
        assert!(
            hit.is_none(),
            "seed {seed} {:?}: {hit:?} in the middle",
            lm.kind
        );
    }
}

/// Off the cliffs and a ramp's sides, the ground's never steeper than a
/// bot will drive.
#[test]
fn the_floor_is_drivable() {
    for seed in SEEDS {
        let sk = skeleton(&spec(seed));
        let mut z = 2.0;
        while z < sk.size - 2.0 {
            let mut x = 2.0;
            while x < sk.size - 2.0 {
                let p = Vec2::new(x, z);
                let s = sk.plateau(p);
                let by_ramp = sk.ramps.iter().any(|r| r.distance(p) < RAMP_HALF + 4.0);
                if !(-3.0..EDGE_FALL + 3.0).contains(&s) && !by_ramp {
                    let dx = sk.ground(x + 1.0, z) - sk.ground(x - 1.0, z);
                    let dz = sk.ground(x, z + 1.0) - sk.ground(x, z - 1.0);
                    let slope = Vec2::new(dx, dz).length() / 2.0;
                    assert!(slope < 0.6, "seed {seed}: {slope} steep at {p}");
                }
                x += 2.0;
            }
            z += 2.0;
        }
    }
}

/// Each placeholder's filler keeps its solid parts at tank height to its
/// reach.
#[test]
fn fillers_keep_to_their_envelopes() {
    for seed in SEEDS {
        let sk = skeleton(&spec(seed));
        let (m, claims) = build_with_claims(&spec(seed));
        for (claim, range) in claims {
            let (at, reach) = match claim {
                Claim::Landmark => (sk.landmark.at, sk.landmark.reach),
                Claim::Lot(k) => (sk.lots[k].at, sk.lots[k].reach),
            };
            for o in &m.oboxes[range] {
                let (y0, _) = o.y_range();
                if !o.solid || y0 > m.ground_height(o.center.x, o.center.y) + 3.0 {
                    continue;
                }
                let out = o
                    .base_shadow(3.0)
                    .iter()
                    .map(|p| p.distance(at) - reach)
                    .fold(f32::MIN, f32::max);
                assert!(out < 1.0, "seed {seed} {claim:?}: {out} m out: {o:?}");
            }
        }
    }
}

/// Where there's high ground, some of it's somewhere to fight: open spots
/// up there a drive from the spawns.
#[test]
fn the_high_ground_is_reachable() {
    for seed in [1u64, 2, 3] {
        let m = Map::desert(seed);
        let sk = skeleton(&spec(seed));
        let up = m.open_spots.iter().filter(|&&p| sk.on_high(p)).count();
        assert!(
            up >= 5,
            "seed {seed} ({:?}): {up} open spots up high",
            sk.form
        );
    }
}
