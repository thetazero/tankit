//! The land a mine's dug in, by the kind of world it's on: its rock and what
//! lies on it, the sky over it, what grows there. Nothing that's driven on or
//! into differs: a seed's the same mine on any of them.

use std::f32::consts::TAU;

use glam::{Vec2, Vec3};

use super::{line_distance, seg};
use crate::map::Map;
use crate::map::procgen::desert::{Palette, WEATHERED};
use crate::map::procgen::foundry::{self, Ground};
use crate::map::procgen::skeleton::Skeleton;
use crate::render::Atmosphere;
use crate::render::material::Material;
use crate::rng::Rng;

/// The colors of the land and its cliffs.
pub(super) fn palette(ground: Ground) -> Palette {
    let planet = planet(ground);
    match ground {
        // Red rock and pale sand.
        Ground::Desert => Palette {
            sand: Vec3::new(0.72, 0.55, 0.38),
            low: Vec3::new(0.56, 0.4, 0.27),
            stone: Vec3::new(0.55, 0.32, 0.21),
            stone_dark: Vec3::new(0.36, 0.2, 0.14),
            stone_pale: Vec3::new(0.66, 0.46, 0.33),
            material: Material::Sand,
            weathered: WEATHERED,
            exposed: 1.0,
            bare: None,
            planet,
        },
        // Dark blue-grey rock under snow, blown bare in places.
        Ground::Tundra => Palette {
            sand: Vec3::new(0.7, 0.73, 0.78),
            low: Vec3::new(0.56, 0.59, 0.64),
            stone: Vec3::new(0.24, 0.26, 0.29),
            stone_dark: Vec3::new(0.15, 0.16, 0.19),
            stone_pale: Vec3::new(0.34, 0.36, 0.4),
            material: Material::Rubble,
            weathered: Vec3::new(0.22, 0.23, 0.25),
            exposed: 0.45,
            bare: Some(Vec3::new(0.34, 0.34, 0.33)),
            planet,
        },
        // Grey limestone standing out of grassland, worn to earth in places.
        Ground::Grassland => Palette {
            sand: Vec3::new(0.2, 0.26, 0.105),
            low: Vec3::new(0.15, 0.21, 0.085),
            stone: Vec3::new(0.46, 0.45, 0.4),
            stone_dark: Vec3::new(0.33, 0.33, 0.3),
            stone_pale: Vec3::new(0.56, 0.55, 0.49),
            material: Material::Rubble,
            weathered: Vec3::new(0.4, 0.4, 0.34),
            exposed: 0.35,
            bare: Some(Vec3::new(0.31, 0.26, 0.16)),
            planet,
        },
    }
}

/// Loose rock in and round the digs.
pub(super) fn loose(ground: Ground) -> [Vec3; 3] {
    match ground {
        Ground::Desert => [
            Vec3::new(0.58, 0.34, 0.22),
            Vec3::new(0.4, 0.23, 0.15),
            Vec3::new(0.7, 0.5, 0.34),
        ],
        Ground::Tundra | Ground::Grassland => {
            let p = palette(ground);
            [p.stone, p.stone_dark, p.stone_pale]
        }
    }
}

/// The sky over the mine: a low sun from a different quarter each seed (but
/// never straight down the Face). Over the desert a hot late afternoon, hazy
/// and white at the horizon, sand blowing off the mesa; over the tundra cold
/// and thin, snow blowing; over grassland clear and broad.
pub(super) fn atmosphere(ground: Ground, rng: &mut Rng) -> Atmosphere {
    // The Seam's sun came from the south-east, low.
    let az = -0.53 + rng.range(-1.1, 1.1);
    let up = rng.range(0.5, 0.62);
    let flat = 0.81 * (1.0 - up * up).sqrt();
    let sky = Atmosphere {
        sun_dir: Vec3::new(az.cos() * flat, up * 0.81, az.sin() * flat),
        sun_color: Vec3::new(2.2, 1.85, 1.45),
        sky_zenith: Vec3::new(0.2, 0.36, 0.62),
        sky_horizon: Vec3::new(0.7, 0.6, 0.5),
        ground_color: Vec3::new(0.42, 0.3, 0.2),
        fog_density: rng.range(0.0011, 0.0015),
        wind: Vec2::new(rng.range(1.5, 3.5), rng.range(-1.5, 1.5)),
        blowing_sand: 0.3,
        dust: Vec3::new(0.72, 0.55, 0.38),
        settle: crate::render::SAND_SETTLE,
        ambient: 1.0,
        stars: 0.0,
    };
    match ground {
        Ground::Desert => sky,
        Ground::Tundra => Atmosphere {
            sun_color: Vec3::new(1.6, 1.56, 1.56),
            sky_zenith: Vec3::new(0.28, 0.39, 0.56),
            sky_horizon: Vec3::new(0.62, 0.67, 0.74),
            ground_color: Vec3::new(0.3, 0.32, 0.37),
            fog_density: sky.fog_density * 1.3,
            wind: sky.wind * 1.4,
            // Snow.
            blowing_sand: 0.45,
            dust: Vec3::new(0.78, 0.81, 0.86),
            settle: Vec3::new(0.68, 0.71, 0.76),
            ..sky
        },
        Ground::Grassland => Atmosphere {
            sun_color: Vec3::new(2.1, 1.95, 1.68),
            sky_zenith: Vec3::new(0.2, 0.37, 0.64),
            sky_horizon: Vec3::new(0.57, 0.63, 0.68),
            ground_color: Vec3::new(0.24, 0.23, 0.17),
            fog_density: sky.fog_density * 0.8,
            blowing_sand: 0.0,
            dust: Vec3::new(0.32, 0.28, 0.2),
            // Earth: grass doesn't grow in the digs.
            settle: Vec3::new(0.28, 0.24, 0.18),
            ..sky
        },
    }
}

/// A mine's world from orbit. A desert one: red, dark basins, little sea,
/// thin cloud.
pub fn planet(ground: Ground) -> crate::planet::Look {
    match ground {
        Ground::Desert => crate::planet::Look {
            land: Vec3::new(0.34, 0.2, 0.12),
            basin: Vec3::new(0.17, 0.1, 0.07),
            sea: Vec3::new(0.03, 0.06, 0.1),
            cloud: Vec3::new(0.45, 0.42, 0.4),
            cloudiness: 0.35,
        },
        Ground::Tundra | Ground::Grassland => ground.planet(),
    }
}

/// What the land's put there itself: drifted snow, and snow on the roofs;
/// tufts of grass, and copses past the edge. Nothing that stops a tank.
pub(super) fn growth(map: &mut Map, sk: &Skeleton, ground: Ground, rng: &mut Rng) {
    let color = palette(ground).sand;
    let density = match ground {
        Ground::Desert => return,
        Ground::Tundra => 0.0035,
        Ground::Grassland => 0.016,
    };
    if ground == Ground::Tundra {
        // (Not on the rails' bridges: they're driven over.)
        let off_bridges = |p: Vec2| sk.crossings.iter().all(|x| seg(p, x.from, x.to).0 > 8.0);
        foundry::snow(map, sk, color, off_bridges);
    }
    let boxes = map.oboxes.len();
    for _ in 0..(density * sk.size * sk.size) as usize {
        let p = Vec2::new(rng.range(6.0, sk.size - 6.0), rng.range(6.0, sk.size - 6.0));
        let g = map.ground_height(p.x, p.y);
        let level = (0..6).all(|k| {
            let q = p + Vec2::from_angle(k as f32 * TAU / 6.0) * 3.0;
            (map.ground_height(q.x, q.y) - g).abs() < 0.9
        });
        let taken = !level
            || sk.lots.iter().any(|l| l.footprint.distance(p) < 1.5)
            || sk.bases.iter().any(|b| b.contains(p, 0.0))
            || sk.tracks.iter().any(|t| line_distance(t, p) < 4.0)
            || sk.crossings.iter().any(|x| seg(p, x.from, x.to).0 < 8.0)
            || (map.oboxes[..boxes].iter()).any(|o| {
                let (lo, hi) = o.y_range();
                hi > g && lo < g + 3.0 && o.distance(p) < 1.0
            });
        if taken {
            continue;
        }
        let yaw = rng.range(0.0, TAU);
        match ground {
            Ground::Tundra => foundry::snowdrift(map, rng, p, g, yaw, color),
            _ => foundry::tuft(map, rng, p, g, yaw, color),
        }
    }
    if ground != Ground::Grassland {
        return;
    }
    let hills: Vec<_> = (map.oboxes.iter())
        .filter(|o| !o.solid && o.half.min_element() > 4.0)
        .copied()
        .collect();
    for _ in 0..60 {
        let dir = Vec2::from_angle(rng.range(0.0, TAU));
        let edge = sk.size * 0.5 / dir.x.abs().max(dir.y.abs());
        let c = Vec2::splat(sk.size * 0.5) + dir * (edge + rng.range(18.0, 190.0));
        for _ in 0..rng.int(2, 6) {
            let p = c + Vec2::new(rng.range(-9.0, 9.0), rng.range(-9.0, 9.0));
            let out = -p.min_element().min(sk.size - p.max_element());
            let clear = out > 10.0
                && sk.tracks.iter().all(|t| line_distance(t, p) > 6.0)
                && hills.iter().all(|o| o.distance(p) > 3.0);
            if clear {
                foundry::tree(map, rng, p, sk.scenery_ground(p.x, p.y));
            }
        }
    }
}
