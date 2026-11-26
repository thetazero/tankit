use super::skeleton::*;
use super::town::{MIN_HOUSES, Source, source_of};
use super::*;
use crate::map::OBox;

/// Two towns of each source.
const SEEDS: [u64; 4] = [0, 1, 2, 3];

fn spec(seed: u64) -> Spec {
    crate::reservoir_spec(seed)
}

fn playable(seed: u64) {
    let m = Map::reservoir(seed);
    let what = format!("seed {seed} ({:?})", source_of(seed));
    assert_eq!(m.bases.len(), 2, "{what}");
    if let Err(why) = crate::map::procgen::check::playable(&m) {
        panic!("{what}: {why}");
    }
    let sk = skeleton(&spec(seed));
    assert!(sk.houses().count() >= MIN_HOUSES, "{what}");
    let nav = NavGrid::build(&m);
    // (The houses are ruined since the skeleton was measured: a doorway may
    // be a short cut, or a heap of rubble in the way.)
    let lens: Vec<f32> = sk
        .base_spots()
        .iter()
        .map(|&from| {
            let mut path = vec![from];
            path.extend(nav.path(from, sk.heart));
            path.windows(2).map(|w| w[0].distance(w[1])).sum()
        })
        .collect();
    let (lo, hi) = (lens[0].min(lens[1]), lens[0].max(lens[1]));
    assert!((hi - lo) / hi < 0.3, "{what}: unfair, {lens:?}");
    assert!(m.open_spots.len() > 80, "{what}: {}", m.open_spots.len());
    assert!(m.repair_spots.len() >= 3, "{what}: {:?}", m.repair_spots);
    assert!(!m.routes.is_empty(), "{what}: no ways round");
    assert_eq!(m.vistas.len(), 2, "{what}");
    assert_eq!(m.hold_site, Some(sk.heart), "{what}");
}

#[test]
fn every_town_is_playable_dressed() {
    SEEDS.into_iter().for_each(playable);
}

/// The same over many more seeds.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow unoptimized: run with cargo test --release"
)]
fn many_towns_are_playable_dressed() {
    // (50 shut a side in, up on its mesa.)
    (4..12).chain([50]).for_each(playable);
}

/// Each source's town has what it's named for, and no two are alike.
#[test]
fn towns_lie_round_their_water() {
    let sks = SEEDS.map(|s| skeleton(&spec(s)));
    for (seed, sk) in SEEDS.into_iter().zip(&sks) {
        let (dam, kind) = match source_of(seed) {
            Source::Dam => (true, LandmarkKind::Dam),
            Source::Well => (false, LandmarkKind::Well),
        };
        assert_eq!(sk.landmarks[0].kind, kind, "seed {seed}");
        assert_eq!(sk.land.dam.is_some(), dam, "seed {seed}");
        assert_eq!(sk.land.lake.is_some(), dam, "seed {seed}");
        if let (Some(d), Some(l)) = (sk.land.dam, sk.land.lake) {
            // The bed's well down, the crest well up, the breach open.
            assert!(sk.ground(l.center) < -3.0, "seed {seed}");
            let crest = d.middle().lerp(d.a, 0.8);
            assert!(sk.ground(crest) > 3.5, "seed {seed}");
            assert!(sk.ground(d.middle()).abs() < 0.5, "seed {seed}");
        }
    }
    for (a, b) in [(0, 2), (1, 3)] {
        assert_ne!(sks[a].base_spots(), sks[b].base_spots());
        let at = |k: usize| -> Vec<_> { sks[k].houses().map(|h| h.at()).collect() };
        assert_ne!(at(a), at(b));
    }
    let again = skeleton(&spec(2));
    assert_eq!(again.base_spots(), sks[2].base_spots());
    assert_eq!(again.lots.len(), sks[2].lots.len());
}

/// Every house keeps off the streets, and the ground's drivable off the
/// cliffs: up over the dam, down into the bed, into the breach.
#[test]
fn streets_are_open_and_the_ground_drivable() {
    for seed in 0..12 {
        let sk = skeleton(&spec(seed));
        for s in &sk.streets {
            let mut t = 0.0;
            while t < s.len() {
                let p = s.from + s.dir() * t;
                for h in sk.houses() {
                    let d = h.footprint.distance(p);
                    assert!(d > s.half + 0.5, "seed {seed}: a house {d} m off {p}");
                }
                t += 1.0;
            }
        }
        let mut z = 2.0;
        while z < sk.size - 2.0 {
            let mut x = 2.0;
            while x < sk.size - 2.0 {
                let p = Vec2::new(x, z);
                let s = sk.land.plateau(p);
                let by_ramp = sk.land.ramp_distance(p) < RAMP_HALF + 4.0;
                if !(-3.0..EDGE_FALL + 3.0).contains(&s) && !by_ramp {
                    let dx = sk.ground(Vec2::new(x + 1.0, z)) - sk.ground(Vec2::new(x - 1.0, z));
                    let dz = sk.ground(Vec2::new(x, z + 1.0)) - sk.ground(Vec2::new(x, z - 1.0));
                    let slope = Vec2::new(dx, dz).length() / 2.0;
                    assert!(slope < 0.66, "seed {seed}: {slope} steep at {p}");
                }
                x += 2.0;
            }
            z += 2.0;
        }
    }
}

/// A tank drives up onto the dam from either end and down off it.
#[test]
fn tanks_drive_over_the_dam() {
    use crate::sim::{Command, HullKind, TurretKind, World};
    let m = Map::reservoir(0);
    let sk = skeleton(&spec(0));
    let d = sk.land.dam.unwrap();
    for end in [d.a, d.b] {
        let out = (end - d.middle()).normalize();
        let from = end + out * (d.run + 4.0);
        if !(20.0..sk.size - 20.0).contains(&from.x) || !(20.0..sk.size - 20.0).contains(&from.y) {
            continue;
        }
        let mut w = World::empty(m.clone());
        let hull = HullKind::Kataphrakt;
        w.spawn_tank(
            Vec3::ONE,
            from,
            (-out.x).atan2(-out.y),
            hull,
            TurretKind::Kontos,
        );
        let mut top = f32::MIN;
        for _ in 0..(7.0 / crate::game::DT) as usize {
            w.step(&[Command {
                throttle: 1.0,
                ..Default::default()
            }]);
            top = top.max(w.tanks[0].pos.y);
        }
        assert!(top > d.height - 0.6, "from {from}: only up to {top}");
    }
}

/// Nothing of the houses hangs in the air.
#[test]
fn no_house_floats() {
    for seed in SEEDS {
        let m = Map::reservoir(seed);
        let sk = skeleton(&spec(seed));
        let floating: Vec<OBox> = m
            .floating()
            .into_iter()
            .map(|i| m.oboxes[i])
            .filter(|o| o.side_mat == crate::render::material::Material::Plaster)
            .filter(|o| sk.houses().any(|h| h.footprint.distance(o.center) < 1.0))
            .collect();
        assert!(
            floating.is_empty(),
            "seed {seed}: {} float: {:?}",
            floating.len(),
            floating.first()
        );
    }
}

/// Tanks really can drive up the collapsed floors into the first storey.
#[test]
fn collapsed_floors_are_drivable() {
    use crate::sim::{Command, HullKind, TurretKind, World};
    let (mut tried, mut stuck) = (0, Vec::new());
    for seed in 0..6 {
        let m = Map::reservoir(seed);
        let ramps = m
            .oboxes
            .iter()
            .filter(|o| o.solid && o.ramp && o.is_turned());
        for r in ramps {
            // Local +z runs down the slope.
            let down = r.axes().1;
            let foot = r.center + down * (r.half.y * r.tilt.cos() + 5.0);
            let mut w = World::empty(m.clone());
            let hull = HullKind::Kataphrakt;
            w.spawn_tank(
                Vec3::ONE,
                foot,
                (-down.x).atan2(-down.y),
                hull,
                TurretKind::Kontos,
            );
            let mut top = f32::MIN;
            for _ in 0..(6.0 / crate::game::DT) as usize {
                w.step(&[Command {
                    throttle: 1.0,
                    ..Default::default()
                }]);
                top = top.max(w.tanks[0].pos.y);
            }
            tried += 1;
            if top < r.y_range().1 - 0.7 {
                stuck.push((seed, r.center, foot, top, r.y_range()));
            }
        }
    }
    assert!(tried >= 4, "only {tried} collapsed floors");
    assert!(stuck.is_empty(), "{} of {tried}: {stuck:?}", stuck.len());
}
