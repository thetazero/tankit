//! Open land that isn't desert: the desert's landforms under snow (rock
//! blown bare, drifts) or grass (limestone, downs, copses past the edge).
//! Where things stand is the desert's own.

use super::Palette;
use super::skeleton::{GENERATOR_PRECINCT, LandmarkKind, Skeleton};
use crate::map::Map;
use crate::map::procgen::foundry::Ground;
use crate::map::procgen::mine::land as base;
use crate::render::Atmosphere;
use crate::rng::Rng;
use glam::Vec3;

/// The colors of `seed`'s land on `ground`: a few kinds, as deserts come in.
pub(super) fn palette(ground: Ground, seed: u64) -> Palette {
    let pal = base::palette(ground);
    let kind = Rng::new(seed ^ 0x9A1E_77E0).index(3);
    match (ground, kind) {
        // Brown rock, the snow thin over frozen heath.
        (Ground::Tundra, 1) => Palette {
            stone: Vec3::new(0.3, 0.25, 0.21),
            stone_dark: Vec3::new(0.19, 0.16, 0.14),
            stone_pale: Vec3::new(0.4, 0.35, 0.3),
            weathered: Vec3::new(0.28, 0.24, 0.2),
            bare: Some(Vec3::new(0.33, 0.29, 0.23)),
            ..pal
        },
        // Pale granite under deep snow.
        (Ground::Tundra, 2) => Palette {
            stone: Vec3::new(0.42, 0.44, 0.47),
            stone_dark: Vec3::new(0.29, 0.31, 0.34),
            stone_pale: Vec3::new(0.54, 0.56, 0.59),
            weathered: Vec3::new(0.38, 0.4, 0.42),
            bare: Some(Vec3::new(0.46, 0.47, 0.48)),
            ..pal
        },
        // Late summer's yellower grass, brown sandstone.
        (Ground::Grassland, 1) => Palette {
            sand: Vec3::new(0.25, 0.29, 0.1),
            low: Vec3::new(0.18, 0.23, 0.08),
            stone: Vec3::new(0.43, 0.36, 0.28),
            stone_dark: Vec3::new(0.3, 0.25, 0.19),
            stone_pale: Vec3::new(0.54, 0.46, 0.36),
            weathered: Vec3::new(0.4, 0.34, 0.25),
            ..pal
        },
        // Heath: dull green, dark slate.
        (Ground::Grassland, 2) => Palette {
            sand: Vec3::new(0.21, 0.23, 0.12),
            low: Vec3::new(0.16, 0.2, 0.1),
            stone: Vec3::new(0.31, 0.32, 0.33),
            stone_dark: Vec3::new(0.21, 0.22, 0.23),
            stone_pale: Vec3::new(0.41, 0.42, 0.42),
            weathered: Vec3::new(0.29, 0.3, 0.28),
            ..pal
        },
        _ => pal,
    }
}

/// The sky over it: its ground's, without a works' smoke.
pub(super) fn atmosphere(ground: Ground, rng: &mut Rng) -> Atmosphere {
    let sky = ground.atmosphere(rng);
    Atmosphere {
        fog_density: sky.fog_density * 0.75,
        ..sky
    }
}

/// What's grown or drifted on `map`, off a generator's pad.
pub(super) fn growth(map: &mut Map, sk: &Skeleton, ground: Ground, pal: &Palette, seed: u64) {
    if ground == Ground::Grassland {
        // The horizon's buttes are tors here, out of the downs.
        let buttes = map.oboxes.iter_mut();
        for o in buttes.filter(|o| !o.solid && o.half.min_element() > 4.0) {
            o.y1 = o.y0 + (o.y1 - o.y0) * 0.4;
        }
    }
    let lm = &sk.landmarks[0];
    let pad = match lm.kind {
        LandmarkKind::Generator | LandmarkKind::Great(_) => GENERATOR_PRECINCT + 4.0,
        _ => 0.0,
    };
    // (Its own dice: where things stand doesn't turn on it.)
    let mut rng = Rng::new(seed ^ 0x0006_2055);
    base::growth(map, sk, ground, pal.sand, &mut rng, |p| {
        p.distance(lm.center) > pad
    });
}
