//! Each people's warship, as the system view shows their fleets: simple
//! shapes that read as theirs at a glance. Bow along +x, up +y.

use std::f32::consts::{FRAC_PI_4, TAU};

use glam::{Mat4, Quat, Vec3};

use crate::faction::Faction;
use crate::render::material::Material;
use crate::render::mesh::MeshData;
use crate::ship_model::{block, col, lathe};

/// How big the system view draws them.
pub const SCALE: f32 = 0.75;

pub fn mesh(f: Faction) -> MeshData {
    match f {
        Faction::Imperium => imperium(),
        Faction::Resonant => resonant(),
        Faction::Uthenn => uthenn(),
        Faction::Ithessar => ithessar(),
        Faction::Company => crate::ship_model::mesh(),
    }
}

/// A crimson basilica: a long nave, a gold dome, a halo over the stern.
fn imperium() -> MeshData {
    let mut m = MeshData::default();
    let red = col([0.42, 0.06, 0.07], Material::Paint);
    let dark = col([0.2, 0.04, 0.05], Material::Paint);
    let gold = col([0.85, 0.6, 0.2], Material::Metal);
    let glow = col([1.0, 0.75, 0.35], Material::Glow);
    let no = Quat::IDENTITY;
    block(&mut m, Vec3::ZERO, Vec3::new(20.0, 4.0, 5.0), no, red);
    block(
        &mut m,
        Vec3::new(-1.0, -2.4, 0.0),
        Vec3::new(16.0, 1.4, 3.4),
        no,
        dark,
    );
    for z in [-1.0, 1.0] {
        block(
            &mut m,
            Vec3::new(-3.5, -0.4, z * 3.6),
            Vec3::new(10.0, 2.4, 2.4),
            no,
            red,
        );
        block(
            &mut m,
            Vec3::new(1.0, 1.2, z * 2.55),
            Vec3::new(15.0, 0.5, 0.2),
            no,
            glow,
        );
    }
    let prow = [(6.0, 0.0), (3.0, 1.4), (0.0, 2.6)];
    lathe(&mut m, Vec3::new(10.0, 0.0, 0.0), &prow, 4, gold);
    // The dome, stood upright.
    let mut dome = MeshData::default();
    let rings: Vec<(f32, f32)> = (0..=8)
        .map(|i| FRAC_PI_4 * 2.0 * i as f32 / 8.0)
        .map(|a| (a.cos() * 3.2, a.sin() * 3.0))
        .collect();
    lathe(&mut dome, Vec3::ZERO, &rings, 24, gold);
    let up = Quat::from_rotation_z(FRAC_PI_4 * 2.0);
    m.append(
        &dome,
        Mat4::from_rotation_translation(up, Vec3::new(-2.0, 2.0, 0.0)),
        gold,
    );
    ring(&mut m, Vec3::new(-9.0, 2.0, 0.0), 6.0, 0.6, gold);
    m
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

/// An upright hoop across the ship at `c`.
fn ring(m: &mut MeshData, c: Vec3, r: f32, thick: f32, color: glam::Vec4) {
    let n = 32;
    for k in 0..n {
        let a = TAU * k as f32 / n as f32;
        let at = c + Vec3::new(0.0, a.sin(), a.cos()) * r;
        let size = Vec3::new(thick, thick, TAU * r / n as f32 * 1.1);
        block(
            m,
            at,
            size,
            Quat::from_rotation_x(-a + FRAC_PI_4 * 2.0),
            color,
        );
    }
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
