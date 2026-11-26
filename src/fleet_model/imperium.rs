//! The Imperium's warship: a basilica under way. A crimson hull drawn out to
//! a gilt ram, a nave down its deck with its windows lit, a gold dome between
//! two half-domes, and the emblem's arch standing over them. Guns down the
//! flanks, three drive bells at the stern.

use std::f32::consts::{PI, TAU};

use glam::{Mat4, Quat, Vec3, Vec4};

use crate::render::material::Material;
use crate::render::mesh::{MeshData, Vertex};
use crate::ship_model::{Rng, Section, block, col, disc, ease, lathe, loft, standing};

const BOW: f32 = 17.0;
const STERN: f32 = -13.0;
/// Where the dome stands, and the level it springs from.
const DOME_X: f32 = -4.0;
const DRUM_Y: f32 = 3.3;

struct Paints {
    red: Vec4,
    deep: Vec4,
    under: Vec4,
    gold: Vec4,
    steel: Vec4,
    window: Vec4,
    drive: Vec4,
}

pub(super) fn mesh() -> MeshData {
    let mut m = MeshData::default();
    let p = Paints {
        red: col([0.27, 0.042, 0.042], Material::Paint),
        deep: col([0.2, 0.034, 0.036], Material::Paint),
        under: col([0.17, 0.12, 0.12], Material::Paint),
        gold: col([0.8, 0.55, 0.3], Material::Metal),
        steel: col([0.22, 0.2, 0.2], Material::Paint),
        window: col([1.0, 0.7, 0.3], Material::Glow),
        drive: col([1.0, 0.72, 0.4], Material::Glow),
    };
    hull(&mut m, &p);
    nave(&mut m, &p);
    domes(&mut m, &p);
    arch(&mut m, &p);
    guns(&mut m, &p);
    plating(&mut m, &p);
    ports(&mut m, &p);
    drives(&mut m, &p);
    m
}

/// The hull's section at `x`: widest at the stern, drawn out to the ram.
fn hull_at(x: f32) -> Section {
    let t = ((x - STERN) / (BOW - STERN)).clamp(0.0, 1.0);
    let aft = ease(-9.0, -11.0, x);
    Section {
        w: (3.5 * (1.0 - t.powf(2.2)).powf(0.8) + 0.35 * aft).max(0.04),
        h: (1.75 * (1.0 - t.powf(2.6)).powf(0.75) + 0.2 * aft).max(0.04),
        y: -0.5 * t * t,
        n: 2.1 + 0.2 * (1.0 - t),
    }
}

/// Where the hull's skin is at `x`, `t` of the way round, and its normal.
fn on_hull(x: f32, t: f32) -> (Vec3, Vec3) {
    let at = |x: f32, t: f32| hull_at(x).at(x, t);
    let p = at(x, t);
    let round = at(x, t + 0.002) - at(x, t - 0.002);
    let along = at(x + 0.05, t) - at(x - 0.05, t);
    let n = round.cross(along).normalize_or_zero();
    let out = p - Vec3::new(x, hull_at(x).y, 0.0);
    (p, if n.dot(out) < 0.0 { -n } else { n })
}

fn hull(m: &mut MeshData, p: &Paints) {
    let stations: Vec<(f32, Section)> = (0..=90)
        .map(|i| {
            let x = BOW - (BOW - STERN) * (i as f32 / 90.0).powf(1.2);
            (x, hull_at(x))
        })
        .collect();
    loft(m, &stations, 48, |c, t, i| {
        let up = (TAU * t).sin();
        if i == usize::MAX {
            return p.steel;
        }
        // The ram, a collar behind it, and one round the drive housing.
        if c.x > 14.2
            || (12.9..13.3).contains(&c.x)
            || (10.6..11.2).contains(&c.x)
            || (-11.4..-10.8).contains(&c.x)
        {
            return p.gold;
        }
        if up < -0.5 {
            return p.under;
        }
        // A gilt stringcourse along each shoulder, armour under it.
        if up < 0.0 && c.x > -10.8 {
            return p.steel;
        }
        if (0.36..0.5).contains(&up) {
            return p.gold;
        }
        // Plating in broad bands.
        if up > 0.0 && (c.x * 0.4).floor() as i32 % 2 == 0 {
            p.red
        } else {
            p.deep
        }
    });
    // A keel blade, gilt along its foot.
    let keel = |x: f32| hull_at(x).y - hull_at(x).h + 0.3;
    let (a, b, c, d) = (6.0, -8.5, -6.5, -1.5);
    let foot = -3.9;
    for (y0, y1, paint, w) in [(0.0, 0.8, p.deep, 0.3), (0.8, 1.0, p.gold, 0.36)] {
        let at = |x0: f32, x1: f32, u: f32, z: f32| {
            let top = keel(x0);
            Vec3::new(x0 + (x1 - x0) * u, top + (foot - top) * u, z)
        };
        let inside = at(a, d, 0.5, 0.0) * 0.5 + at(b, c, 0.5, 0.0) * 0.5;
        let side = |z: f32| {
            [
                at(a, d, y0, z),
                at(b, c, y0, z),
                at(b, c, y1, z),
                at(a, d, y1, z),
            ]
        };
        let (l, r) = (side(-w), side(w));
        for q in [
            l,
            r,
            [l[0], l[3], r[3], r[0]],
            [l[1], l[2], r[2], r[1]],
            [l[2], l[3], r[3], r[2]],
        ] {
            m.facet([q[0], q[1], q[2]], inside, paint);
            m.facet([q[0], q[2], q[3]], inside, paint);
        }
    }
}

/// The nave down the deck: aisles, a taller hall on them, a gilt cornice on
/// each, and a row of tall lit windows between buttresses.
fn nave(m: &mut MeshData, p: &Paints) {
    let no = Quat::IDENTITY;
    for (x0, x1, w, y0, y1) in [(-10.6, 5.6, 2.1, 0.4, 2.1), (-9.6, 3.8, 1.45, 1.6, 3.3)] {
        let tier = |grow: f32, y0: f32, y1: f32, rake: f32| -> Vec<(f32, Section)> {
            (0..=60)
                .map(|i| {
                    let x = x1 + grow - (x1 - x0 + 2.0 * grow) * i as f32 / 60.0;
                    // Raked back at both ends.
                    let k = ease(x1 + grow, x1 - rake, x) * ease(x0 - grow, x0 + rake * 0.6, x);
                    let top = y0 + (y1 - y0) * (0.35 + 0.65 * k);
                    let (w, h) = ((w + grow) * (0.75 + 0.25 * k), (top - y0) * 0.5);
                    (
                        x,
                        Section {
                            w,
                            h,
                            y: y0 + h,
                            n: 9.0,
                        },
                    )
                })
                .collect()
        };
        loft(m, &tier(0.0, y0, y1, 1.6), 48, |c, t, _| {
            let up = (TAU * t).sin();
            if up < 0.0 {
                p.deep
            } else if (c.x * 0.8).floor() as i32 % 2 == 0 {
                p.red
            } else {
                p.deep
            }
        });
        // The cornice along its eaves.
        for s in [-1.0f32, 1.0] {
            block(
                m,
                Vec3::new((x0 + x1) * 0.5 - 0.1, y1 - 0.22, s * (w + 0.02)),
                Vec3::new(x1 - x0 - 2.6, 0.14, 0.16),
                no,
                p.gold,
            );
        }
        // A ridge down its roof.
        block(
            m,
            Vec3::new((x0 + x1) * 0.5, y1 + 0.04, 0.0),
            Vec3::new(x1 - x0 - 2.8, 0.14, 0.5),
            no,
            p.steel,
        );
    }
    for s in [-1.0f32, 1.0] {
        for k in 0..9 {
            let x = -8.2 + 1.25 * k as f32;
            let c = Vec3::new(x, 2.55, s * 1.44);
            block(m, c, Vec3::new(0.3, 0.55, 0.12), no, p.window);
            let c = Vec3::new(x + 0.62, 2.4, s * 1.6);
            block(m, c, Vec3::new(0.22, 1.3, 0.45), no, p.deep);
        }
    }
}

/// A lathe's rings stood upright at `c`: (height, radius) from the top down.
fn upright(m: &mut MeshData, c: Vec3, rings: &[(f32, f32)], color: Vec4) {
    let mut part = MeshData::default();
    lathe(&mut part, Vec3::ZERO, rings, 40, color);
    let up = Quat::from_rotation_z(PI * 0.5);
    m.append(&part, Mat4::from_rotation_translation(up, c), color);
}

/// A dome's rings, radius `r` and `h` high.
fn bowl(r: f32, h: f32) -> Vec<(f32, f32)> {
    (0..=12)
        .map(|i| PI * 0.5 * i as f32 / 12.0)
        .map(|a| (a.cos() * h, a.sin() * r))
        .collect()
}

fn domes(m: &mut MeshData, p: &Paints) {
    // The great dome on its drum, windows all round.
    let c = Vec3::new(DOME_X, DRUM_Y, 0.0);
    upright(m, c, &[(0.8, 1.8), (-2.0, 1.8)], p.red);
    for (y, r, h) in [(0.7, 2.0, 0.14), (-0.05, 2.0, 0.14)] {
        let at = c + Vec3::Y * y;
        upright(m, at, &[(h, 0.0), (h, r), (0.0, r), (0.0, 0.0)], p.gold);
    }
    for k in 0..24 {
        let turn = Quat::from_rotation_y(TAU * k as f32 / 24.0);
        let at = c + Vec3::Y * 0.4 + turn * Vec3::X * 1.8;
        block(m, at, Vec3::new(0.1, 0.36, 0.2), turn, p.window);
    }
    upright(m, c + Vec3::Y * 0.84, &bowl(1.85, 1.6), p.gold);
    // Ribs over it, in crimson.
    for k in 0..12 {
        let turn = Quat::from_rotation_y(TAU * k as f32 / 12.0);
        let rib = |a: f32| Vec3::new(a.cos() * 1.87, 0.84 + a.sin() * 1.62, 0.0);
        for i in 0..8 {
            let (u, v) = (rib(0.18 * i as f32), rib(0.18 * (i + 1) as f32));
            let d = v - u;
            let lean = turn * Quat::from_rotation_z(d.y.atan2(d.x));
            let mid = c + turn * ((u + v) * 0.5);
            block(m, mid, Vec3::new(d.length() * 1.1, 0.07, 0.1), lean, p.deep);
        }
    }
    // Its lantern and finial.
    let top = c + Vec3::Y * 2.38;
    upright(m, top, &[(0.4, 0.28), (0.0, 0.28)], p.window);
    upright(m, top + Vec3::Y * 0.4, &bowl(0.36, 0.35), p.gold);
    upright(m, top + Vec3::Y * 0.7, &[(0.9, 0.0), (0.0, 0.07)], p.gold);
    // The lesser domes fore and aft of it.
    for (x, r) in [(-0.9, 1.05), (-7.1, 1.05), (1.4, 0.7)] {
        let c = Vec3::new(x, 3.25, 0.0);
        upright(m, c, &[(0.3, r), (-0.3, r)], p.deep);
        upright(m, c + Vec3::Y * 0.3, &bowl(r, r * 0.8), p.gold);
        upright(
            m,
            c + Vec3::Y * 0.24,
            &[(0.12, r + 0.06), (0.0, r + 0.06)],
            p.gold,
        );
        let tip = c + Vec3::Y * (0.28 + r * 0.8);
        upright(m, tip, &[(0.5, 0.0), (0.0, 0.1)], p.gold);
    }
}

/// A hoop across the ship round `c`, from angle `from` to `to`.
fn hoop(m: &mut MeshData, c: Vec3, r: f32, thick: f32, (from, to): (f32, f32), color: Vec4) {
    let (n, sides) = (64, 12);
    let base = m.verts.len() as u32;
    for i in 0..=n {
        let a = from + (to - from) * i as f32 / n as f32;
        let dir = Vec3::new(0.0, a.sin(), a.cos());
        for k in 0..sides {
            let b = TAU * k as f32 / sides as f32;
            let out = dir * b.cos() + Vec3::X * b.sin();
            m.verts.push(Vertex {
                pos: (c + dir * r + out * thick).to_array(),
                normal: out.to_array(),
                color: color.to_array(),
            });
        }
    }
    for i in 0..n {
        for k in 0..sides {
            let g = (k + 1) % sides;
            let [a, b] = [base + i * sides + k, base + i * sides + g];
            let [d, e] = [a + sides, b + sides];
            m.indices.extend_from_slice(&[a, b, e, a, e, d]);
        }
    }
}

/// The emblem's arch: two gilt hoops, braced, standing on the shoulders
/// behind the great dome, lit between.
fn arch(m: &mut MeshData, p: &Paints) {
    let c = Vec3::new(-8.9, 1.9, 0.0);
    let span = (-0.18 * PI, 1.18 * PI);
    hoop(m, c, 4.75, 0.24, span, p.gold);
    hoop(m, c, 3.95, 0.13, span, p.gold);
    hoop(m, c, 4.35, 0.07, span, p.window);
    for k in 0..=22 {
        let a = span.0 + (span.1 - span.0) * k as f32 / 22.0;
        let at = c + Vec3::new(0.0, a.sin(), a.cos()) * 4.35;
        let size = Vec3::new(0.3, 0.16, 0.8);
        block(m, at, size, Quat::from_rotation_x(-a), p.deep);
    }
    let finial = c + Vec3::Y * 4.9;
    upright(m, finial, &[(1.1, 0.0), (0.35, 0.22), (0.0, 0.1)], p.gold);
    // Its feet: a bastion on each shoulder.
    for s in [-1.0f32, 1.0] {
        block(
            m,
            Vec3::new(c.x, 0.2, s * 4.1),
            Vec3::new(3.2, 1.3, 1.5),
            Quat::IDENTITY,
            p.deep,
        );
        block(
            m,
            Vec3::new(c.x, 0.86, s * 4.1),
            Vec3::new(3.3, 0.12, 1.6),
            Quat::IDENTITY,
            p.gold,
        );
    }
}

/// Twin-barrelled turrets down each shoulder, and two on the foredeck.
fn guns(m: &mut MeshData, p: &Paints) {
    for x in [7.2, 9.4] {
        gun(m, p, x, 0.25);
    }
    for x in [5.0, 2.4, -0.2, -2.8, -5.4] {
        for t in [0.12, 0.38] {
            gun(m, p, x, t);
        }
    }
}

fn gun(m: &mut MeshData, p: &Paints, x: f32, t: f32) {
    let (at, n) = on_hull(x, t);
    let rot = standing(n);
    let mut g = MeshData::default();
    upright(&mut g, Vec3::ZERO, &[(0.15, 0.55), (-0.3, 0.55)], p.gold);
    m.append(&g, Mat4::from_rotation_translation(rot, at), p.gold);
    let place = |v: Vec3| at + rot * v;
    block(
        m,
        place(Vec3::new(0.0, 0.32, 0.0)),
        Vec3::new(0.95, 0.4, 0.75),
        rot,
        p.deep,
    );
    for z in [-0.17, 0.17] {
        block(
            m,
            place(Vec3::new(0.95, 0.36, z)),
            Vec3::new(1.3, 0.11, 0.11),
            rot,
            p.steel,
        );
    }
}

/// Armour plates and vents over the deck and shoulders.
fn plating(m: &mut MeshData, p: &Paints) {
    let mut rng = Rng(0x0a3b_c0de);
    let picks = [p.red, p.deep, p.deep, p.red];
    for _ in 0..70 {
        let x = rng.range(-12.0, 12.0);
        let t = rng.range(0.125, 0.21);
        let size = Vec3::new(
            rng.range(0.4, 1.7),
            rng.range(0.12, 0.2),
            rng.range(0.2, 0.6),
        );
        let color = picks[(rng.f() * 4.0) as usize % 4];
        // Mass-made: the same on both sides.
        for t in [t, 0.5 - t] {
            let (at, n) = on_hull(x, t);
            block(m, at, size, standing(n), color);
        }
    }
}

/// Rows of lit ports down the flanks.
fn ports(m: &mut MeshData, p: &Paints) {
    for t in [0.004f32, -0.03] {
        for k in 0..44 {
            let x = -10.0 + 0.44 * k as f32;
            // In groups of five, a gap between.
            if k % 6 == 5 {
                continue;
            }
            for t in [t, 0.5 - t] {
                let (at, n) = on_hull(x, t);
                let size = Vec3::new(0.2, 0.09, 0.1);
                block(m, at, size, standing(n), p.window);
            }
        }
    }
}

fn drives(m: &mut MeshData, p: &Paints) {
    for (z, r) in [(0.0, 1.35), (-2.35, 0.95), (2.35, 0.95)] {
        let c = Vec3::new(STERN - 1.6, 0.0, z);
        let rings: Vec<(f32, f32)> = (0..=12)
            .map(|i| i as f32 / 12.0)
            .map(|u| (2.2 * (1.0 - u), r * (0.65 + 0.35 * u.powf(1.8))))
            .collect();
        lathe(m, c, &rings, 28, p.gold);
        lathe(m, c, &[(0.25, r * 1.02), (0.0, r * 1.02)], 28, p.gold);
        lathe(m, c, &[(0.02, r * 0.94), (1.2, r * 0.6)], 28, p.drive);
        disc(m, c + Vec3::X * 1.2, r * 0.62, -1.0, 28, p.drive);
    }
}
