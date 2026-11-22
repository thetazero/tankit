//! Generated maps, in three steps:
//!
//! 1. **Biome** (`Biome`): what kind of ground this is (a city, ...). It
//!    sets the sky and lays out the skeleton with its own grammar: a city
//!    wants a different structure from a canyon whatever goes on it.
//! 2. **Skeleton** (`skeleton`): the map in placeholders: streets, lots
//!    ("a big building", "a building on pillars", "rubble"), the landmark
//!    in the middle, bases, gates over streets, smaller decos, the edge.
//!    Nobody's architecture yet, and checked for play (`check`) on its
//!    greybox: a skeleton that fails is laid out again.
//! 3. **Style** (`Style`): whose place it is. It fills each placeholder in
//!    (the Imperium's rotunda for the landmark, their blocks for the lots,
//!    a triumphal arch for a gate) keeping to its envelope, and dresses
//!    what's beyond the edge. The same skeleton in another style is the
//!    same map, someone else's.
//!
//! A style's fillers are its library: any biome whose skeleton has a
//! monument gets the style's monument, so pieces are shared across every
//! map built in that style.

pub mod check;
pub mod city;
mod imperium;
pub mod skeleton;

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use glam::Vec2;

use self::skeleton::*;
use super::Map;
use super::ruins::Drift;
use crate::faction::Faction;
use crate::rng::Rng;

/// What kind of ground a map is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Biome {
    City,
}

/// Whose place it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Look {
    Imperium,
}

/// What stands in the middle, the thing the fight's about. Either keeps to
/// the skeleton's landmark: the same map round it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Centerpiece {
    /// The style's great building (the Imperium's rotunda).
    Monument,
    /// A shield generator: the standard model of whoever holds the place.
    ShieldGenerator,
}

/// A map to generate.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Spec {
    pub seed: u64,
    pub biome: Biome,
    pub look: Look,
    pub sides: Vec<Faction>,
    pub centerpiece: Centerpiece,
}

/// A style: fills in each kind of placeholder. Each filler builds onto the
/// map and keeps its solid parts to the placeholder's envelope (see
/// `skeleton`); heaps it banks up come back as drifts for the terrain.
pub(in crate::map) trait Style {
    /// The Hold site's name on the HUD, with `piece` in the middle.
    fn hold_name(&self, piece: Centerpiece) -> &'static str;
    fn landmark(&self, map: &mut Map, sk: &Skeleton, rng: &mut Rng) -> Vec<Drift>;
    /// The style's shield generator, in the landmark's place.
    fn shield_generator(&self, map: &mut Map, sk: &Skeleton, rng: &mut Rng) -> Vec<Drift>;
    /// Paving and the like on a platform's top.
    fn platform(&self, map: &mut Map, sk: &Skeleton, p: &Platform, rng: &mut Rng);
    fn street(&self, map: &mut Map, sk: &Skeleton, s: &Street, rng: &mut Rng);
    fn span(&self, map: &mut Map, sk: &Skeleton, s: &Span, rng: &mut Rng);
    fn lot(&self, map: &mut Map, sk: &Skeleton, lot: &Lot, rng: &mut Rng) -> Vec<Drift>;
    fn base(&self, map: &mut Map, sk: &Skeleton, side: usize, b: &Base, rng: &mut Rng);
    fn edge(&self, map: &mut Map, sk: &Skeleton, e: &Edge, rng: &mut Rng);
    fn deco(&self, map: &mut Map, sk: &Skeleton, d: &Deco, rng: &mut Rng);
    /// What's seen beyond the edge.
    fn beyond(&self, map: &mut Map, sk: &Skeleton, rng: &mut Rng);
    /// Last touches over the whole map (wear on the masonry).
    fn finish(&self, map: &mut Map);
}

fn style(look: Look) -> &'static dyn Style {
    match look {
        Look::Imperium => &imperium::Imperium,
    }
}

/// What a filler built: the placeholder, and the range of `oboxes` it added.
#[derive(Clone, Debug)]
#[cfg_attr(not(test), allow(dead_code))]
pub enum Claim {
    Lot(usize),
    Deco(usize),
    Span(usize),
}

/// The skeleton `spec` lays out: the first, from its seed on, whose
/// greybox passes `check`.
pub fn skeleton(spec: &Spec) -> Skeleton {
    let mut last = None;
    for attempt in 0..8u64 {
        let mut rng = Rng::new(spec.seed.wrapping_mul(0x9E37_79B9).wrapping_add(attempt));
        let sk = match spec.biome {
            Biome::City => city::skeleton(&mut rng, spec.sides.len()),
        };
        match check::playable(&sk.greybox(&spec.sides)) {
            Ok(()) => return sk,
            Err(why) => log::debug!("map seed {} attempt {attempt}: {why}", spec.seed),
        }
        last = Some(sk);
    }
    last.unwrap()
}

/// Builds `spec`'s map, and what each filler claimed.
pub fn build(spec: &Spec) -> (Map, Vec<(Claim, std::ops::Range<usize>)>) {
    let sk = skeleton(spec);
    let st = style(spec.look);
    let mut rng = Rng::new(spec.seed ^ 0x5EED_D2E5);
    let mut map = sk.blank(&spec.sides);
    map.atmosphere = match spec.biome {
        Biome::City => city::atmosphere(&mut rng),
    };
    map.hold_name = st.hold_name(spec.centerpiece);
    map.planet = Some((spec.seed, planet(spec.biome)));
    let mut claims = Vec::new();
    let mut drifts = match spec.centerpiece {
        Centerpiece::Monument => st.landmark(&mut map, &sk, &mut rng),
        Centerpiece::ShieldGenerator => st.shield_generator(&mut map, &sk, &mut rng),
    };
    for (k, lot) in sk.lots.iter().enumerate() {
        let start = map.oboxes.len();
        drifts.extend(st.lot(&mut map, &sk, lot, &mut rng));
        claims.push((Claim::Lot(k), start..map.oboxes.len()));
    }
    let mut terrain = sk.terrain(|p| drifts.iter().map(|d| d.height_at(p)).fold(0.0, f32::max));
    terrain.color = imperium::GROUND;
    terrain.material = crate::render::material::Material::Concrete;
    map.terrain = Some(terrain);
    for p in &sk.platforms {
        st.platform(&mut map, &sk, p, &mut rng);
    }
    for s in &sk.streets {
        st.street(&mut map, &sk, s, &mut rng);
    }
    for (k, s) in sk.spans.iter().enumerate() {
        let start = map.oboxes.len();
        st.span(&mut map, &sk, s, &mut rng);
        claims.push((Claim::Span(k), start..map.oboxes.len()));
    }
    for (k, d) in sk.decos.iter().enumerate() {
        let start = map.oboxes.len();
        st.deco(&mut map, &sk, d, &mut rng);
        claims.push((Claim::Deco(k), start..map.oboxes.len()));
    }
    for (k, b) in sk.bases.iter().enumerate() {
        st.base(&mut map, &sk, k, b, &mut rng);
    }
    for e in &sk.edges {
        st.edge(&mut map, &sk, e, &mut rng);
    }
    st.beyond(&mut map, &sk, &mut rng);
    st.finish(&mut map);
    map.drop_tiny_hitboxes();
    map.separate_coplanar_faces();
    sk.finish(&mut map);
    map.vistas = vistas(&map, &sk);
    (map, claims)
}

/// The map `spec` makes, built only the first time it's asked for (the
/// last few are kept: the tests and the menus ask for the same ones over
/// and over).
pub fn map(spec: &Spec) -> Map {
    static CACHE: LazyLock<Mutex<HashMap<Spec, Map>>> = LazyLock::new(Default::default);
    if let Some(m) = CACHE.lock().unwrap().get(spec) {
        return m.clone();
    }
    let (m, _) = build(spec);
    let mut cache = CACHE.lock().unwrap();
    if cache.len() >= 6 {
        cache.clear();
    }
    cache.insert(spec.clone(), m.clone());
    m
}

/// How a world of `biome` looks from orbit.
fn planet(biome: Biome) -> crate::planet::Look {
    match biome {
        Biome::City => city::PLANET,
    }
}

/// The skeleton `spec` lays out, as its greybox.
pub fn greybox(spec: &Spec) -> Map {
    let sk = skeleton(spec);
    let mut map = sk.greybox(&spec.sides);
    let mut rng = Rng::new(spec.seed ^ 0x5EED_D2E5);
    map.atmosphere = match spec.biome {
        Biome::City => city::atmosphere(&mut rng),
    };
    map.vistas = vistas(&map, &sk);
    map
}

/// Where the menus show each side's tank: in or near its court, in the
/// sun, the camera looking past it into the city.
fn vistas(map: &Map, sk: &Skeleton) -> Vec<(Vec2, f32)> {
    let sun = map.atmosphere.sun_dir.normalize();
    let view = |p: Vec2| {
        // The camera behind the tank, looking past it to the middle.
        let d = sk.center() - p;
        (-d.x).atan2(-d.y)
    };
    sk.bases
        .iter()
        .map(|b| {
            // In the court if the sun's on it, else as near it as it's lit
            // (with the sun low behind the edge, the court's in shadow):
            // nearest the court's middle first.
            let mid = b.at(b.size * 0.55, b.size * 0.55);
            let reach = b.size + 60.0;
            let mut spots = Vec::new();
            let mut u = 12.0;
            while u < reach {
                let mut v = 12.0;
                while v < reach {
                    spots.push(b.at(u, v));
                    v += 3.0;
                }
                u += 3.0;
            }
            spots.sort_by(|a, c| a.distance(mid).total_cmp(&c.distance(mid)));
            let good = |p: Vec2| {
                let g = map.ground_height(p.x, p.y);
                let at = glam::Vec3::new(p.x, g + 1.9, p.y);
                let blocked =
                    |dir: glam::Vec3, len: f32| map.oboxes.iter().any(|o| o.ray_hit(at, dir, len));
                // Nothing under the tank or round it but the ground, and
                // level ground out to the camera.
                let under = map
                    .oboxes
                    .iter()
                    .any(|o| o.distance(p) < 3.4 && o.y1 > g + 0.05 && o.y0 < g + 3.0);
                let v = view(p);
                let back = Vec2::new(v.sin(), v.cos());
                let level = (0..=14).all(|k| {
                    let q = p + back * k as f32;
                    (map.ground_height(q.x, q.y) - g).abs() < 0.3
                });
                level && !under && !blocked(sun, 300.0) && !blocked(glam::Vec3::Y, 60.0)
            };
            let p = spots.into_iter().find(|&p| good(p)).unwrap_or(b.middle());
            (p, view(p))
        })
        .collect()
}

impl Map {
    /// The imperial city `seed` names (see `crate::city_spec`).
    pub fn city(seed: u64) -> Map {
        map(&crate::city_spec(seed))
    }

    /// The imperial city `seed` names, with a shield generator in the
    /// middle in place of the rotunda (see `crate::shield_city_spec`).
    pub fn shield_city(seed: u64) -> Map {
        map(&crate::shield_city_spec(seed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::OBox;

    /// The seeds the tests build: enough to catch a grammar that only
    /// sometimes goes wrong, few enough to build in a debug test run.
    const SEEDS: [u64; 4] = [0, 1, 2, 3];

    fn spec(seed: u64) -> Spec {
        crate::city_spec(seed)
    }

    #[test]
    fn every_seed_is_playable_dressed() {
        for seed in SEEDS {
            let m = Map::city(seed);
            assert_eq!(m.factions, Faction::ALL.to_vec());
            assert_eq!(m.bases.len(), 4);
            if let Err(why) = check::playable(&m) {
                panic!("seed {seed}: {why}");
            }
            assert!(
                m.open_spots.len() > 100,
                "seed {seed}: {} open",
                m.open_spots.len()
            );
            assert!(
                m.repair_spots.len() >= 4,
                "seed {seed}: {:?}",
                m.repair_spots
            );
            assert_eq!(m.vistas.len(), 4, "seed {seed}");
        }
    }

    #[test]
    fn different_seeds_make_different_cities() {
        let sk: Vec<Skeleton> = SEEDS.iter().map(|&s| skeleton(&spec(s))).collect();
        for (i, a) in sk.iter().enumerate() {
            for b in &sk[i + 1..] {
                let same = a.size == b.size
                    && a.landmark.center == b.landmark.center
                    && a.lots.len() == b.lots.len();
                assert!(!same, "two seeds laid out the same city");
            }
        }
        // And the same seed, the same city.
        let again = skeleton(&spec(SEEDS[1]));
        assert_eq!(again.lots.len(), sk[1].lots.len());
        assert_eq!(again.landmark.center, sk[1].landmark.center);
    }

    /// Along the edge, never a long way without a building or something
    /// put across it; and each avenue's end is heaped with rubble.
    #[test]
    fn the_edge_is_cordoned() {
        for seed in SEEDS {
            let sk = skeleton(&spec(seed));
            for (start, along, _) in city::sides(sk.size) {
                let mut open = 0.0;
                let mut t = 0.0;
                while t <= sk.size {
                    let p = start + along * t;
                    let in_building = sk.lots.iter().any(|l| {
                        matches!(l.kind, LotKind::Big | LotKind::Small) && {
                            let q = l.footprint.local_point(p).abs() - l.footprint.half;
                            q.max_element() < 0.0
                        }
                    });
                    let across = sk
                        .edges
                        .iter()
                        .any(|e| e.along == along && e.from <= t && t <= e.to);
                    open = if in_building || across {
                        0.0
                    } else {
                        open + 0.5
                    };
                    assert!(open < 40.0, "seed {seed}: the edge is open for 40 m to {p}");
                    t += 0.5;
                }
            }
            for a in sk.avenues() {
                assert!(
                    sk.edges.iter().any(|e| e.kind == EdgeKind::Blockade
                        && (a.to - e.start).perp_dot(e.along).abs() < 0.01
                        && (e.from..=e.to).contains(&(a.to - e.start).dot(e.along))),
                    "seed {seed}: nothing across the avenue to {}",
                    a.to
                );
            }
        }
    }

    /// The city doesn't stop at the edge: its blocks run on out past it on
    /// every side.
    #[test]
    fn the_city_runs_on_past_the_edge() {
        for seed in SEEDS {
            let sk = skeleton(&spec(seed));
            for (start, _, inward) in city::sides(sk.size) {
                let out = |p: Vec2| -(p - start).dot(inward);
                let near = sk
                    .lots
                    .iter()
                    .filter(|l| (4.0..city::NEAR).contains(&out(l.footprint.center)))
                    .count();
                let far = sk
                    .lots
                    .iter()
                    .filter(|l| out(l.footprint.center) > city::NEAR + 20.0)
                    .count();
                assert!(
                    near >= 4 && far >= 8,
                    "seed {seed}: {near} near, {far} far past {start}"
                );
            }
        }
    }

    /// The courts are cleared out of the city, not put down in the open:
    /// buildings stand right round each one.
    #[test]
    fn buildings_stand_round_every_court() {
        for seed in SEEDS {
            let sk = skeleton(&spec(seed));
            for (k, b) in sk.bases.iter().enumerate() {
                let (a, z) = (b.at(0.0, 0.0), b.at(b.size, b.size));
                let court = OBox {
                    center: (a + z) * 0.5,
                    half: (z - a).abs() * 0.5,
                    ..OBox::default()
                };
                let round = sk
                    .lots
                    .iter()
                    .filter(|l| l.kind != LotKind::Distant && l.footprint.overlaps(&court, 20.0))
                    .count();
                assert!(round >= 4, "seed {seed}: {round} lots round court {k}");
            }
        }
    }

    /// A placeholder's filler keeps its solid parts at tank height to the
    /// placeholder's ground: a lot's to its footprint (and a little
    /// spill), a deco's to its reach, a gate's off the middle of the
    /// street. What drove on the greybox drives on the dressed map.
    #[test]
    fn fillers_keep_to_their_envelopes() {
        for seed in SEEDS {
            let sk = skeleton(&spec(seed));
            let (m, claims) = build(&spec(seed));
            for (claim, range) in claims {
                for o in &m.oboxes[range] {
                    let (y0, _) = o.y_range();
                    let g = sk.ground(o.center);
                    // A ramp (a floor fallen to the street) is there to drive up.
                    if !o.solid || o.ramp || y0 > g + 3.0 {
                        continue;
                    }
                    let shadow = o.base_shadow(3.0);
                    let worst = |inside: &dyn Fn(Vec2) -> f32| {
                        shadow.iter().map(|&p| inside(p)).fold(0.0, f32::max)
                    };
                    match claim {
                        Claim::Lot(k) => {
                            let lot = &sk.lots[k].footprint;
                            let out = worst(&|p| {
                                let q = lot.local_point(p).abs() - lot.half;
                                q.max_element()
                            });
                            assert!(
                                out < LOT_SPILL,
                                "seed {seed} lot {k} {:?}: {out} m out: {o:?}",
                                sk.lots[k]
                            );
                        }
                        Claim::Deco(k) => {
                            let d = &sk.decos[k];
                            let out = worst(&|p| p.distance(d.at) - d.kind.reach());
                            assert!(out < 0.01, "seed {seed} {:?} {k}: {out} m out", d.kind);
                        }
                        Claim::Span(k) => {
                            let s = &sk.spans[k];
                            let side = Vec2::new(-s.along.y, s.along.x);
                            let near = shadow
                                .iter()
                                .map(|&p| (p - s.at).dot(side).abs())
                                .fold(f32::MAX, f32::min);
                            // Its opening stays clear (the ruined one's
                            // attic fell beside the piers).
                            assert!(
                                near > s.opening - 0.6,
                                "seed {seed} gate {k} blocks the street: {near} m from the middle"
                            );
                        }
                    }
                }
            }
        }
    }

    /// Down each avenue's clear strip, from the landmark's precinct to the
    /// edge, nothing solid stands at tank height.
    #[test]
    fn the_avenues_are_open_to_the_middle() {
        for seed in SEEDS {
            let sk = skeleton(&spec(seed));
            let m = Map::city(seed);
            for a in sk.avenues() {
                let (dir, side) = (a.dir(), Vec2::new(-a.dir().y, a.dir().x));
                let mut r = sk.landmark.precinct + 4.0;
                while r < a.len() - 8.0 {
                    for across in [-1.0, 0.0, 1.0] {
                        let p = a.from + dir * r + side * across * (a.clear - 1.0);
                        assert!(
                            !m.oboxes.iter().any(|o| {
                                let (y0, _) = o.y_range();
                                o.solid && y0 < sk.ground(p) + 3.0 && o.distance(p) < 1.0
                            }),
                            "seed {seed}: avenue {dir:?} blocked at {r}, {across}: {:?}",
                            m.oboxes.iter().find(|o| {
                                let (y0, _) = o.y_range();
                                o.solid && y0 < sk.ground(p) + 3.0 && o.distance(p) < 1.0
                            })
                        );
                    }
                    r += 2.0;
                }
            }
        }
    }

    /// The shield generator variant: the same city for the same seed, the
    /// generator in the rotunda's place, and as playable.
    #[test]
    fn the_shield_city_is_the_city_with_a_generator() {
        for seed in SEEDS {
            let plain = skeleton(&spec(seed));
            let shield = crate::shield_city_spec(seed);
            let sk = skeleton(&shield);
            assert_eq!(sk.landmark.center, plain.landmark.center);
            assert_eq!(sk.lots.len(), plain.lots.len());
            let m = Map::shield_city(seed);
            assert_eq!(m.hold_name, "GENERATOR");
            assert_eq!(m.hold_site, Some(sk.landmark.center));
            if let Err(why) = check::playable(&m) {
                panic!("seed {seed}: {why}");
            }
            assert!(m.open_spots.len() > 100, "seed {seed}");
            assert_eq!(m.vistas.len(), 4, "seed {seed}");
            // The mast stands in the middle, its beam up over the city; the
            // blast wall stays inside the landmark's ring.
            let c = sk.landmark.center;
            assert!(
                m.oboxes
                    .iter()
                    .any(|o| o.solid && o.distance(c) < 0.5 && o.y_range().1 > 25.0),
                "seed {seed}: no mast"
            );
            assert!(
                m.glow_paths.len() >= 2 * sk.landmark.doors.len() - 2,
                "seed {seed}"
            );
        }
    }

    /// The plaza's slope is gentle enough to drive up anywhere.
    #[test]
    fn the_plaza_slope_is_drivable() {
        for seed in SEEDS {
            let sk = skeleton(&spec(seed));
            let pl = sk.plaza();
            for k in 0..16 {
                let a = k as f32 / 16.0 * std::f32::consts::TAU;
                let dir = Vec2::new(a.cos(), a.sin());
                let mut prev = sk.ground(pl.center);
                for r in 1..(pl.half as i32 * 2) {
                    let h = sk.ground(pl.center + dir * r as f32);
                    assert!((prev - h).abs() < 0.25, "seed {seed}: plaza edge too steep");
                    prev = h;
                }
            }
        }
    }
}
