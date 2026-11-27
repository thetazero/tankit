//! The land a foundry stands on: what its ground's made of and coloured, the
//! sky over it, what grows or lies past its wall. Nothing that's driven on
//! or into differs: a seed's the same works on any of them.

use glam::{Vec2, Vec3};

use crate::render::Atmosphere;
use crate::render::material::Material;
use crate::rng::Rng;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ground {
    Desert,
    Tundra,
    Grassland,
}

/// The land's colours.
pub struct Land {
    /// The open ground, and where it's bare or wet.
    pub open: Vec3,
    pub low: Vec3,
    pub material: Material,
    /// Trodden and blackened round the works.
    pub soot: Vec3,
    pub slag: Vec3,
    pub ballast: Vec3,
    /// Far ground past the wall.
    pub far: Vec3,
    pub far_material: Material,
    pub rock: Vec3,
}

impl Ground {
    #[cfg(test)]
    pub const ALL: [Ground; 3] = [Ground::Desert, Ground::Tundra, Ground::Grassland];

    pub fn land(self) -> Land {
        match self {
            Ground::Desert => Land {
                open: Vec3::new(0.6, 0.47, 0.32),
                low: Vec3::new(0.5, 0.38, 0.26),
                material: Material::Sand,
                soot: Vec3::new(0.27, 0.23, 0.2),
                slag: Vec3::new(0.17, 0.155, 0.15),
                ballast: Vec3::new(0.34, 0.31, 0.28),
                far: Vec3::new(0.6, 0.47, 0.32),
                far_material: Material::Sand,
                rock: Vec3::new(0.5, 0.36, 0.25),
            },
            Ground::Tundra => Land {
                open: Vec3::new(0.74, 0.77, 0.82),
                low: Vec3::new(0.36, 0.36, 0.34),
                material: Material::Rubble,
                soot: Vec3::new(0.25, 0.25, 0.26),
                slag: Vec3::new(0.16, 0.16, 0.175),
                ballast: Vec3::new(0.33, 0.33, 0.34),
                far: Vec3::new(0.76, 0.79, 0.84),
                far_material: Material::Rubble,
                rock: Vec3::new(0.4, 0.42, 0.45),
            },
            Ground::Grassland => Land {
                open: Vec3::new(0.2, 0.28, 0.11),
                low: Vec3::new(0.3, 0.25, 0.16),
                material: Material::Rubble,
                soot: Vec3::new(0.2, 0.18, 0.15),
                slag: Vec3::new(0.15, 0.14, 0.135),
                ballast: Vec3::new(0.3, 0.29, 0.27),
                far: Vec3::new(0.2, 0.28, 0.11),
                far_material: Material::Rubble,
                rock: Vec3::new(0.36, 0.36, 0.33),
            },
        }
    }

    /// Its world from orbit.
    pub fn planet(self) -> crate::planet::Look {
        match self {
            Ground::Desert => crate::planet::DRY,
            Ground::Tundra => crate::planet::ICE,
            Ground::Grassland => crate::planet::GREEN,
        }
    }

    /// The sky, through the works' smoke: amber over the desert, low and
    /// white over the tundra, clear and broad over grassland.
    pub fn atmosphere(self, rng: &mut Rng) -> Atmosphere {
        let az = rng.range(0.0, std::f32::consts::TAU);
        let up = match self {
            Ground::Desert => rng.range(0.42, 0.6),
            Ground::Tundra => rng.range(0.4, 0.5),
            Ground::Grassland => rng.range(0.5, 0.7),
        };
        let flat = (1.0 - up * up).sqrt();
        let sky = Atmosphere {
            sun_dir: Vec3::new(az.cos() * flat, up, az.sin() * flat),
            sun_color: Vec3::new(2.05, 1.72, 1.36),
            sky_zenith: Vec3::new(0.22, 0.28, 0.38),
            sky_horizon: Vec3::new(0.52, 0.45, 0.37),
            ground_color: Vec3::new(0.24, 0.2, 0.17),
            fog_density: rng.range(0.0012, 0.0019),
            wind: Vec2::new(rng.range(-2.4, 2.4), rng.range(-2.4, 2.4)),
            blowing_sand: 0.25,
            dust: Vec3::new(0.4, 0.34, 0.28),
            settle: Vec3::new(0.34, 0.29, 0.24),
            ambient: 1.0,
            stars: 0.0,
            companions: Default::default(),
        };
        match self {
            Ground::Desert => sky,
            Ground::Tundra => Atmosphere {
                sun_color: Vec3::new(1.6, 1.62, 1.72),
                sky_zenith: Vec3::new(0.3, 0.38, 0.5),
                sky_horizon: Vec3::new(0.6, 0.64, 0.7),
                ground_color: Vec3::new(0.34, 0.36, 0.4),
                fog_density: rng.range(0.0016, 0.0024),
                wind: sky.wind * 1.4,
                // Snow.
                blowing_sand: 0.5,
                dust: Vec3::new(0.78, 0.81, 0.86),
                settle: Vec3::new(0.7, 0.73, 0.78),
                ..sky
            },
            Ground::Grassland => Atmosphere {
                sun_color: Vec3::new(2.05, 1.94, 1.7),
                sky_zenith: Vec3::new(0.2, 0.36, 0.62),
                sky_horizon: Vec3::new(0.56, 0.62, 0.66),
                ground_color: Vec3::new(0.18, 0.21, 0.13),
                fog_density: rng.range(0.0008, 0.0013),
                blowing_sand: 0.0,
                dust: Vec3::new(0.32, 0.28, 0.2),
                settle: Vec3::new(0.28, 0.24, 0.18),
                ..sky
            },
        }
    }
}
