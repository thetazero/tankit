//! The Resonant's medium structures, each in one of the shapes the layout
//! keeps ground for (see `CivicKind`):
//!
//! - a courtyard's: a **garth** (a roofed walk on pillars round an open
//!   court, a crystal grown up in the middle) or an **ossuary** (rows of
//!   tombs between shrines, low cover everywhere).
//! - a market's: a **choir**, the ribs of a hall that's lost its roof,
//!   organ pipes down one side.
//! - a terrace's: an **altar**, the raised floor with crystal and bells.

use super::*;
use crate::map::procgen::civic::{self, Site};

pub(in crate::map::procgen) fn build(
    map: &mut Map,
    rng: &mut Rng,
    lot: &Lot,
    kind: CivicKind,
    s: civic::Stone,
) {
    match kind {
        CivicKind::Courtyard if rng.chance(0.5) => garth(map, rng, lot),
        CivicKind::Courtyard => ossuary(map, rng, lot),
        CivicKind::Market if rng.chance(0.65) => choir(map, rng, lot),
        CivicKind::Market => civic::build(map, rng, lot, kind, s),
        CivicKind::Terrace => {
            civic::build(map, rng, lot, kind, s);
            altar(map, rng, lot);
        }
    }
}

const P: Material = Material::Plaster;
const M: Material = Material::Metal;

fn crystal(map: &mut Map, rng: &mut Rng, base: Vec3, lean: Vec3, h: f32, r: f32) {
    map.crystals.push(Crystal {
        base: base - Vec3::Y * 0.6,
        axis: (Vec3::Y + lean).normalize() * (h + 0.6),
        radius: r,
        sides: 6,
        seed: rng.next_u64() as u32,
        color: CRYSTAL_LIGHT,
        lit: false,
        solid: true,
        raw: false,
    });
}

fn garth(map: &mut Map, rng: &mut Rng, lot: &Lot) {
    let mut site = Site::new(map, lot);
    let (w, l) = site.half();
    let (walk, h) = (6.2, 5.4);
    // Pillars along both edges of the walk, a slate roof over it.
    for e in [-1.0f32, 1.0] {
        for (len, off, along_x) in [(w, l, true), (l, w, false)] {
            let n = (len * 2.0 / 7.0).round() as i32;
            for k in 0..=n {
                let u = -len + 0.8 + k as f32 * (len * 2.0 - 1.6) / n as f32;
                for r in [off - 0.8, off - walk] {
                    // The inner row stops at its own corners.
                    if r < off - 1.0 && u.abs() > len - walk + 0.5 {
                        continue;
                    }
                    if u.abs() > len - 1.0 && !along_x {
                        continue;
                    }
                    let at = if along_x {
                        Vec2::new(u, e * r)
                    } else {
                        Vec2::new(e * r, u)
                    };
                    site.put(at, Vec2::splat(0.55), (-0.1, h), IVORY, P, true);
                    site.put(at, Vec2::splat(0.75), (h - 0.9, h - 0.6), GILT, M, false);
                }
            }
            let mid = off - walk * 0.5 - 0.2;
            let reach = if along_x { len } else { len - walk - 0.5 };
            for (inset, y) in [(0.0, h), (0.9, h + 0.7), (1.8, h + 1.3)] {
                let half = walk * 0.5 + 0.4 - inset;
                let (at, hf) = if along_x {
                    (Vec2::new(0.0, e * mid), Vec2::new(reach, half))
                } else {
                    (Vec2::new(e * mid, 0.0), Vec2::new(half, reach))
                };
                let color = SLATE * if along_x { 1.0 } else { 0.94 };
                site.put(at, hf, (y, y + 0.7), color, Material::Roof, inset == 0.0);
            }
        }
    }
    let c = site.point(Vec2::ZERO, 0.0);
    for k in 0..5 {
        let d = dir(k as f32 * 1.256 + rng.range(-0.2, 0.2));
        let (out, lean, h, r) = if k == 0 {
            (0.0, 0.0, 7.0, 1.2)
        } else {
            (1.9, 0.3, rng.range(2.0, 4.0), 0.6)
        };
        let v = Vec3::new(d.x, 0.0, d.y);
        crystal(map, rng, c + v * out, v * lean, h, r);
    }
    map.sound(Ambient::Shrine, c + Vec3::Y * 3.0);
    map.lights.push(PointLight {
        pos: c + Vec3::Y * 4.0,
        color: CRYSTAL_LIGHT * 8.0,
        radius: 18.0,
    });
}

fn ossuary(map: &mut Map, rng: &mut Rng, lot: &Lot) {
    let mut site = Site::new(map, lot);
    let (w, l) = site.half();
    // Rows of tombs, aisles between them wide enough to drive.
    let (nx, nz) = ((w * 2.0 / 9.0) as i32, (l * 2.0 / 8.0) as i32);
    for i in 0..nx {
        for j in 0..nz {
            if rng.chance(0.22) {
                continue;
            }
            let at = Vec2::new(
                -w + 4.0 + i as f32 * (w * 2.0 - 8.0) / (nx - 1).max(1) as f32,
                -l + 4.0 + j as f32 * (l * 2.0 - 8.0) / (nz - 1).max(1) as f32,
            );
            match rng.f32() {
                x if x < 0.55 => {
                    let half = Vec2::new(1.5, 0.8);
                    site.put(at, half, (-0.1, 1.2), IVORY * 0.88, P, true);
                    site.put(at, half + 0.15, (1.2, 1.45), IVORY * 0.97, P, false);
                }
                x if x < 0.85 => {
                    // A mausoleum: taller than a tank.
                    let half = Vec2::new(1.9, 1.6);
                    let h = rng.range(3.0, 4.0);
                    site.put(at, half, (-0.1, h), IVORY * 0.94, P, true);
                    for k in 0..3 {
                        let u = 1.0 - k as f32 * 0.3;
                        let y = h + k as f32 * 0.5;
                        let c = SLATE * 1.1;
                        site.put(at, half * u + 0.2, (y, y + 0.5), c, Material::Roof, false);
                    }
                }
                _ => {
                    site.put(at, Vec2::splat(0.5), (-0.1, 3.6), IVORY, P, true);
                    site.put(at, Vec2::splat(0.32), (3.6, 4.8), GILT, M, false);
                }
            }
        }
    }
}

fn choir(map: &mut Map, rng: &mut Rng, lot: &Lot) {
    let mut site = Site::new(map, lot);
    let (w, l) = site.half();
    let (h, apex) = (6.5, 13.0);
    let n = (l * 2.0 / 7.0).round() as i32;
    let mut ribs = Vec::new();
    for k in 0..=n {
        let z = -l + 0.9 + k as f32 * (l * 2.0 - 1.8) / n as f32;
        let broken = rng.chance(0.25);
        for e in [-1.0f32, 1.0] {
            let top = if broken && e > 0.0 {
                rng.range(2.0, 4.5)
            } else {
                h
            };
            let at = Vec2::new(e * (w - 0.8), z);
            site.put(at, Vec2::splat(0.7), (-0.2, top), IVORY, P, true);
            if top == h {
                let foot = site.point(at, h - 0.3);
                let tip = site.point(Vec2::new(0.0, z), apex);
                let end = if broken {
                    foot.lerp(tip, rng.range(0.4, 0.7))
                } else {
                    tip
                };
                ribs.push((foot, end));
            }
        }
    }
    // A wall of organ pipes down one side, in two lengths with a way
    // through between.
    let x = -(w - 0.8);
    for e in [-1.0f32, 1.0] {
        let at = Vec2::new(x, e * l * 0.5);
        let half = Vec2::new(0.6, l * 0.5 - 4.0);
        if half.y < 1.0 {
            continue;
        }
        site.put(at, half, (-0.2, 2.0), IVORY * 0.9, P, true);
        let m = (half.y * 2.0 / 1.3) as i32;
        for k in 0..m {
            let z = at.y - half.y + 0.65 + k as f32 * 1.3;
            let tall = 6.0 + 3.0 * (k as f32 * 0.7).sin().abs();
            let foot = site.point(Vec2::new(x, z), 1.95);
            ribs.push((foot, foot + Vec3::Y * tall));
        }
    }
    for (a, b) in ribs {
        let upright = (a - b).x.abs() + (a - b).z.abs() < 0.01;
        let (r, c, m) = if upright {
            (0.42, GILT, M)
        } else {
            (0.38, IVORY * 0.96, P)
        };
        map.pipe(a, b, r, c, m, false);
    }
}

fn altar(map: &mut Map, rng: &mut Rng, lot: &Lot) {
    let mut site = Site::new(map, lot);
    let floor = civic::FLOOR_H;
    let half = Vec2::new(1.8, 1.2);
    site.put(
        Vec2::ZERO,
        half,
        (floor - 0.05, floor + 1.3),
        IVORY,
        P,
        true,
    );
    site.put(
        Vec2::ZERO,
        half + 0.2,
        (floor + 1.3, floor + 1.55),
        GILT,
        M,
        false,
    );
    let c = site.point(Vec2::ZERO, floor + 2.1);
    crystal(map, rng, c, Vec3::ZERO, 4.5, 0.8);
    map.lights.push(PointLight {
        pos: c + Vec3::Y * 2.0,
        color: CRYSTAL_LIGHT * 7.0,
        radius: 16.0,
    });
    map.sound(Ambient::Bell, c + Vec3::Y * 3.0);
}
