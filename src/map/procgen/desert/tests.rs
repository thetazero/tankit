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
        let mut high = 0;
        let mut all = 0;
        let mut z = 5.0;
        while z < sk.size {
            let mut x = 5.0;
            while x < sk.size {
                all += 1;
                if sk.land.on_high(Vec2::new(x, z)) {
                    high += 1;
                }
                x += 10.0;
            }
            z += 10.0;
        }
        let f = high as f32 / all as f32;
        let (lo, hi) = match form_of(seed) {
            Form::Erg => (0.02, 0.25),
            Form::Canyonlands => (0.4, 0.9),
            Form::Basin => (0.15, 0.7),
            Form::Escarpment => (0.1, 0.6),
        };
        assert!(
            (lo..hi).contains(&f),
            "{:?}: {f} high ground",
            form_of(seed)
        );
        assert!(
            form_of(seed) == Form::Erg || !sk.land.high.ramps.is_empty(),
            "{:?}: no ramps",
            form_of(seed)
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
        assert!(
            a.size != b.size
                || a.base_spots() != b.base_spots()
                || a.land.high.ramps != b.land.high.ramps,
            "two {:?} seeds laid out the same desert",
            form_of(form_seed)
        );
    }
    let a = skeleton(&spec(3));
    let b = skeleton(&spec(3));
    assert_eq!(a.base_spots(), b.base_spots());
    assert_eq!(a.land.high.ramps, b.land.high.ramps);
    assert_eq!(a.lots.len(), b.lots.len());
}

/// Nothing solid at tank height in the landmark's open ground.
#[test]
fn the_landmark_ground_is_open() {
    for seed in SEEDS {
        let m = Map::desert(seed);
        let sk = skeleton(&spec(seed));
        let lm = &sk.landmarks[0];
        assert_eq!(m.hold_site, Some(lm.center));
        let g = m.ground_height(lm.center.x, lm.center.y);
        let hit = m.oboxes.iter().find(|o| {
            let (lo, hi) = o.y_range();
            o.solid && lo < g + 3.5 && hi > g + 0.5 && o.distance(lm.center) < lm.clear - 2.0
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
                let s = sk.land.plateau(p);
                let by_ramp = sk
                    .land
                    .high
                    .ramps
                    .iter()
                    .any(|r| r.distance(p) < RAMP_HALF + 4.0);
                if !(-3.0..EDGE_FALL + 3.0).contains(&s) && !by_ramp {
                    let dx = sk.ground(Vec2::new(x + 1.0, z)) - sk.ground(Vec2::new(x - 1.0, z));
                    let dz = sk.ground(Vec2::new(x, z + 1.0)) - sk.ground(Vec2::new(x, z - 1.0));
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
                Claim::Landmark => (sk.landmarks[0].center, sk.landmarks[0].reach),
                Claim::Lot(k) => (sk.lots[k].at(), sk.lots[k].reach()),
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
        let up = m.open_spots.iter().filter(|&&p| sk.land.on_high(p)).count();
        assert!(
            up >= 5,
            "seed {seed} ({:?}): {up} open spots up high",
            form_of(seed)
        );
    }
}

/// The shield desert (#337), one seed of each form, each with a different
/// side's generator: playable, the Hold site the generator, its light high
/// over the middle, its doors open from the plinth out past the precinct,
/// its camp kept to the slot.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow unoptimized: run with cargo test --release"
)]
fn the_shield_desert_comes_with_any_sides_generator() {
    use crate::map::procgen::shield;
    for (k, owner) in Faction::ALL.into_iter().enumerate() {
        let seed = SEEDS[k];
        let damaged = crate::Generator {
            owner,
            condition: shield::Condition::DAMAGED,
        };
        let m = build(&crate::shield_desert_spec(seed, damaged));
        if let Err(why) = crate::map::procgen::check::playable(&m) {
            panic!("damaged {owner:?} seed {seed}: {why}");
        }
        let spec = crate::shield_desert_spec(seed, crate::Generator::up(owner));
        let what = format!("{owner:?} seed {seed} ({:?})", form_of(seed));
        let sk = skeleton(&spec);
        assert_eq!(sk.landmarks[0].kind, LandmarkKind::Generator, "{what}");
        let (m, claims) = build_with_claims(&spec);
        assert_eq!(m.hold_name, "GENERATOR", "{what}");
        assert_eq!(m.hold_site, Some(sk.landmarks[0].center), "{what}");
        if let Err(why) = crate::map::procgen::check::playable(&m) {
            panic!("{what}: {why}");
        }
        assert!(
            m.open_spots.len() > 80,
            "{what}: {} open",
            m.open_spots.len()
        );
        assert_eq!(m.vistas.len(), 2, "{what}");
        let site = dress::site(&sk, seed);
        assert!(site.doors.len() >= 4, "{what}: {} doors", site.doors.len());
        let g = sk.ground(site.center);
        assert!(
            m.lights.iter().any(|l| {
                Vec2::new(l.pos.x, l.pos.z).distance(site.center) < 1.0 && l.pos.y > g + 8.0
            }),
            "{what}: nothing lit high over the middle"
        );
        for &d in &site.doors {
            let side = Vec2::new(-d.y, d.x);
            let mut r = shield::PLINTH_R + 1.5;
            while r < site.precinct + 4.0 {
                for across in [-1.0f32, 0.0, 1.0] {
                    let p = site.center + d * r + side * across;
                    let ground = m.ground_height(p.x, p.y);
                    let hit = m.oboxes.iter().find(|o| {
                        let (y0, _) = o.y_range();
                        o.solid && y0 < ground + 3.0 && o.distance(p) < 0.5
                    });
                    assert!(hit.is_none(), "{what}: door {d} blocked at {r}: {hit:?}");
                    assert!(
                        !m.crystals.iter().any(|c| c.solid
                            && Vec2::new(c.base.x, c.base.z).distance(p) < c.radius + 1.0),
                        "{what}: a crystal in door {d} at {r}"
                    );
                }
                r += 1.5;
            }
        }
        // The camp keeps to the slot, as any filler does.
        let (_, range) = claims
            .iter()
            .find(|(c, _)| matches!(c, Claim::Landmark))
            .unwrap();
        for o in &m.oboxes[range.clone()] {
            let (y0, _) = o.y_range();
            if !o.solid || y0 > m.ground_height(o.center.x, o.center.y) + 3.0 {
                continue;
            }
            let out = o.center.distance(sk.landmarks[0].center) - sk.landmarks[0].reach;
            assert!(out < 1.0, "{what}: {out} m out of the slot: {o:?}");
        }
    }
}

/// Each side's ways in: a track beaten out of the generator's nearest door
/// toward its spawns, and a door facing every side.
#[test]
fn the_generator_faces_every_side() {
    for seed in SEEDS {
        let spec = crate::shield_desert_spec(seed, crate::Generator::up(Faction::Uthenn));
        let sk = skeleton(&spec);
        let site = dress::site(&sk, seed);
        for &b in &sk.base_spots() {
            let to = (b - site.center).normalize();
            let best = site.doors.iter().map(|d| d.dot(to)).fold(-1.0, f32::max);
            assert!(best > 0.8, "seed {seed}: no door toward spawns at {b}");
        }
        // No gap round the ring over a third of the way.
        let mut a: Vec<f32> = site
            .doors
            .iter()
            .map(|d| d.y.atan2(d.x).rem_euclid(std::f32::consts::TAU))
            .collect();
        a.sort_by(f32::total_cmp);
        for k in 0..a.len() {
            let next = if k + 1 < a.len() {
                a[k + 1]
            } else {
                a[0] + std::f32::consts::TAU
            };
            assert!(
                next - a[k] < std::f32::consts::TAU / 3.0 + 0.02,
                "seed {seed}: {a:?}"
            );
        }
    }
}
