//! Each people's warship, as the system view shows their fleets: simple
//! shapes that read as theirs at a glance. Bow along +x, up +y.

mod imperium;

use std::f32::consts::TAU;

use glam::{Quat, Vec3};

use crate::faction::Faction;
use crate::render::material::Material;
use crate::render::mesh::MeshData;
use crate::ship_model::{block, col, lathe};

/// How big the system view draws them.
pub const SCALE: f32 = 0.75;

pub fn mesh(f: Faction) -> MeshData {
    match f {
        Faction::Imperium => imperium::mesh(),
        Faction::Resonant => resonant(),
        Faction::Uthenn => uthenn(),
        Faction::Ithessar => ithessar(),
        Faction::Company => crate::ship_model::mesh(),
    }
}

/// A pale reliquary: a thin keel, a pointed arch, plates standing off it
/// and a crystal in the middle.
fn resonant() -> MeshData {
    let mut m = MeshData::default();
    let ivory = col([0.62, 0.58, 0.48], Material::Paint);
    let gilt = col([0.8, 0.6, 0.25], Material::Metal);
    let blue = col([0.3, 0.6, 1.0], Material::Glow);
    let keel = [
        (13.0, 0.0),
        (6.0, 0.9),
        (-4.0, 1.1),
        (-11.0, 0.5),
        (-13.0, 0.0),
    ];
    lathe(&mut m, Vec3::ZERO, &keel, 8, ivory);
    for s in [-1.0f32, 1.0] {
        // The arch's two leaning posts.
        let lean = Quat::from_rotation_x(s * -0.35);
        let post = Vec3::new(0.8, 9.0, 0.8);
        block(&mut m, Vec3::new(-2.0, 4.6, s * 1.6), post, lean, gilt);
        for x in [-7.0, 0.0, 7.0] {
            let c = Vec3::new(x, -0.4, s * 3.2);
            let tip = Quat::from_rotation_x(s * 0.5);
            block(&mut m, c, Vec3::new(5.4, 2.6, 0.35), tip, ivory);
        }
    }
    let shard = [(2.6, 0.0), (0.0, 1.3), (-2.6, 0.0)];
    lathe(&mut m, Vec3::new(-2.0, 3.0, 0.0), &shard, 6, blue);
    m
}

/// A tungsten cube, seamed in green, smaller cubes standing off it.
fn uthenn() -> MeshData {
    let mut m = MeshData::default();
    let pale = col([0.3, 0.42, 0.36], Material::Paint);
    let green = col([0.3, 1.0, 0.5], Material::Glow);
    let no = Quat::IDENTITY;
    let size = 10.0;
    for x in [-1.0, 1.0] {
        for y in [-1.0, 1.0] {
            for z in [-1.0, 1.0] {
                let c = Vec3::new(x, y, z) * size * 0.26;
                block(&mut m, c, Vec3::splat(size * 0.45), no, pale);
            }
        }
    }
    block(&mut m, Vec3::ZERO, Vec3::splat(size * 0.94), no, green);
    for (x, y, z) in [(9.0, 2.0, 3.0), (-9.0, -2.5, -2.0), (7.5, -4.0, -4.0)] {
        block(&mut m, Vec3::new(x, y, z), Vec3::splat(2.6), no, pale);
    }
    m
}

/// A black crystal almond with an orb of plasma held in its eye.
fn ithessar() -> MeshData {
    let mut m = MeshData::default();
    let glass = col([0.07, 0.05, 0.09], Material::Metal);
    let pink = col([1.0, 0.4, 0.7], Material::Glow);
    // Two blades bowed out round the eye.
    for s in [-1.0f32, 1.0] {
        for k in 0..12 {
            let a = k as f32 / 12.0 * 2.0 - 1.0;
            let b = (k + 1) as f32 / 12.0 * 2.0 - 1.0;
            let at = |u: f32| Vec3::new(u * 13.0, s * 4.6 * (1.0 - u * u), 0.0);
            let (p, q) = (at(a), at(b));
            let d = q - p;
            let turn = Quat::from_rotation_z(d.y.atan2(d.x));
            let deep = 1.0 + 3.4 * (1.0 - a * a);
            let size = Vec3::new(d.length() * 1.15, 1.1, deep);
            block(&mut m, (p + q) * 0.5, size, turn, glass);
        }
    }
    let orb: Vec<(f32, f32)> = (0..=12)
        .map(|i| TAU * 0.5 * i as f32 / 12.0)
        .map(|a| (a.cos() * 2.4, a.sin() * 2.4))
        .collect();
    lathe(&mut m, Vec3::ZERO, &orb, 24, pink);
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_people_has_its_own_ship() {
        let counts: Vec<usize> = (0..4).map(|f| mesh(Faction::side(f)).verts.len()).collect();
        for (i, &n) in counts.iter().enumerate() {
            assert!(n > 50);
            assert!(!counts[..i].contains(&n));
        }
    }
}
