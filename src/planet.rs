//! A whole world seen from orbit, generated from a seed and a look: land,
//! basins, seas and streaks of cloud painted onto a sphere's vertices. The
//! garage's bay shows one out of its viewport.

use glam::Vec3;

use crate::render::material::Material;
use crate::render::mesh::MeshData;

/// The colors a world is painted in (albedo, before the sun).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    /// Open ground, most of the surface.
    pub land: Vec3,
    /// Darker lowland basins.
    pub basin: Vec3,
    /// Seas in the deepest basins.
    pub sea: Vec3,
    pub cloud: Vec3,
    /// How much of the sky is cloud, 0..1.
    pub cloudiness: f32,
}

/// A dry, ochre world with dark basins, a few small seas, thin cloud.
pub const DRY: Look = Look {
    land: Vec3::new(0.3, 0.2, 0.12),
    basin: Vec3::new(0.15, 0.09, 0.06),
    sea: Vec3::new(0.03, 0.06, 0.1),
    cloud: Vec3::new(0.42, 0.42, 0.44),
    cloudiness: 0.5,
};

/// A cold world: pale grey-blue land, darker rifts, ice-locked seas, a
/// thin haze.
pub const ICE: Look = Look {
    land: Vec3::new(0.42, 0.45, 0.5),
    basin: Vec3::new(0.24, 0.27, 0.32),
    sea: Vec3::new(0.55, 0.6, 0.66),
    cloud: Vec3::new(0.6, 0.62, 0.66),
    cloudiness: 0.35,
};

/// A green world: grassland, brown uplands, blue seas, broken cloud.
pub const GREEN: Look = Look {
    land: Vec3::new(0.17, 0.26, 0.1),
    basin: Vec3::new(0.2, 0.19, 0.11),
    sea: Vec3::new(0.03, 0.09, 0.16),
    cloud: Vec3::new(0.58, 0.6, 0.62),
    cloudiness: 0.5,
};

/// The world `seed` names, as a sphere of radius 0.5 (scale it up where
/// it's drawn), flat-shaded material, colored per vertex.
pub fn mesh(seed: u64, look: &Look) -> MeshData {
    let mut m = MeshData::sphere(160, 80);
    // Each seed shifts the noise to a different patch of an endless field.
    let mut rng = crate::rng::Rng::new(seed);
    let land_at = Vec3::new(
        rng.range(0.0, 500.0),
        rng.range(0.0, 500.0),
        rng.range(0.0, 500.0),
    );
    let cloud_at = Vec3::new(
        rng.range(0.0, 500.0),
        rng.range(0.0, 500.0),
        rng.range(0.0, 500.0),
    );
    let thresh = 0.72 - 0.4 * look.cloudiness.clamp(0.0, 1.0);
    for v in &mut m.verts {
        let n = Vec3::from(v.normal);
        let land = fbm(n * 6.0 + land_at);
        let mut c = look
            .land
            .lerp(look.basin, ((land - 0.45) * 6.0).clamp(0.0, 1.0));
        c = c.lerp(look.sea, ((land - 0.6) * 12.0).clamp(0.0, 1.0));
        // Clouds in streaks along the latitudes.
        let streak = fbm(Vec3::new(n.x * 5.0, n.y * 22.0, n.z * 5.0) + cloud_at);
        c = c.lerp(look.cloud, ((streak - thresh) * 5.0).clamp(0.0, 0.85));
        v.color = [c.x, c.y, c.z, Material::Plain.id()];
    }
    m
}

/// Three octaves of smooth value noise, about 0..1.
fn fbm(p: Vec3) -> f32 {
    noise(p) * 0.55 + noise(p * 2.1) * 0.28 + noise(p * 4.3) * 0.17
}

fn noise(p: Vec3) -> f32 {
    let f = p.floor();
    let t = p - f;
    let t = t * t * (Vec3::splat(3.0) - 2.0 * t);
    let i = [f.x as i32, f.y as i32, f.z as i32];
    let mut v = 0.0;
    for c in 0..8 {
        let (dx, dy, dz) = (c & 1, (c >> 1) & 1, (c >> 2) & 1);
        let w = if dx == 1 { t.x } else { 1.0 - t.x }
            * if dy == 1 { t.y } else { 1.0 - t.y }
            * if dz == 1 { t.z } else { 1.0 - t.z };
        v += w * hash([i[0] + dx, i[1] + dy, i[2] + dz]);
    }
    v
}

fn hash(p: [i32; 3]) -> f32 {
    let mut h = (p[0] as u32).wrapping_mul(0x8da6_b343)
        ^ (p[1] as u32).wrapping_mul(0xd816_3841)
        ^ (p[2] as u32).wrapping_mul(0xcb1a_b31f);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h & 0xffff) as f32 / 65535.0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Different seeds paint different worlds; the same seed, the same one.
    #[test]
    fn seeds_make_different_worlds() {
        let colors =
            |seed| -> Vec<[f32; 4]> { mesh(seed, &DRY).verts.iter().map(|v| v.color).collect() };
        assert_eq!(colors(3), colors(3));
        assert_ne!(colors(3), colors(4));
    }
}
