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
    let mut m = MeshData::sphere(320, 160);
    // Each seed shifts the noise to a different patch of an endless field.
    let mut rng = crate::rng::Rng::new(seed);
    let mut patch = || {
        Vec3::new(
            rng.range(0.0, 500.0),
            rng.range(0.0, 500.0),
            rng.range(0.0, 500.0),
        )
    };
    let (land_at, warp_at, cloud_at) = (patch(), patch(), patch());
    let thresh = 0.72 - 0.32 * look.cloudiness.clamp(0.0, 1.0);
    let edge = |v: f32, at: f32, w: f32| {
        let t = ((v - at) / w + 0.5).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    let paint = |n: Vec3| {
        // The coasts' shapes are bent by a slower noise, so they wander.
        let warp = Vec3::new(
            noise(n * 2.2 + warp_at),
            noise(n * 2.2 + warp_at + Vec3::splat(31.0)),
            noise(n * 2.2 + warp_at + Vec3::splat(67.0)),
        ) - Vec3::splat(0.5);
        let at = n * 2.4 + warp * 1.5 + land_at;
        let land = fbm(at);
        let fine = fbm(at * 5.0);
        let mut c = look
            .land
            .lerp(look.basin, edge(land + 0.12 * fine, 0.52, 0.16));
        // Relief: ridges catch the light, gullies lose it.
        c *= 0.72 + 0.56 * fine;
        // Seas, with a hard shore.
        c = c.lerp(look.sea, edge(land + 0.05 * fine, 0.6, 0.02));
        // Ice at the poles.
        let ice = n.y.abs() + 0.1 * (fine - 0.5);
        c = c.lerp(Vec3::splat(0.4), edge(ice, 0.93, 0.03));
        // Cloud, in streaks along the latitudes, curled.
        let q = n + warp * 0.35;
        let streak = fbm(Vec3::new(q.x * 3.5, q.y * 9.0, q.z * 3.5) + cloud_at);
        c.lerp(look.cloud, 0.8 * edge(streak, thresh + 0.06, 0.14))
    };
    // A few threads: one would hitch the screen it's shown on.
    let chunk = m.verts.len().div_ceil(3);
    std::thread::scope(|scope| {
        for part in m.verts.chunks_mut(chunk) {
            scope.spawn(|| {
                for v in part {
                    let c = paint(Vec3::from(v.normal));
                    v.color = [c.x, c.y, c.z, Material::Plain.id()];
                }
            });
        }
    });
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
