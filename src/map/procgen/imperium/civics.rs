//! The Imperium's medium structures, each in one of the shapes the layout
//! keeps ground for (see `CivicKind`):
//!
//! - a courtyard's: **baths** (a dry pool in a ring of columns, vaulted
//!   blocks at the corners) or an **arsenal yard** (bays of crates and
//!   blast walls, a maze).
//! - a market's: a **basilica**, a long nave open at one end, columns down
//!   it, an apse with the emblem at the other.
//! - a terrace's: a **rostra**, the raised floor with a victory column on
//!   it, banners down its sides.

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
        CivicKind::Courtyard if rng.chance(0.5) => baths(map, rng, lot),
        CivicKind::Courtyard => arsenal(map, rng, lot),
        CivicKind::Market if rng.chance(0.65) => basilica(map, rng, lot),
        CivicKind::Market => civic::build(map, rng, lot, kind, s),
        CivicKind::Terrace => {
            civic::build(map, rng, lot, kind, s);
            rostra(map, lot);
        }
    }
}

const C: Material = Material::Concrete;

fn baths(map: &mut Map, rng: &mut Rng, lot: &Lot) {
    let mut site = Site::new(map, lot);
    let (w, l) = site.half();
    // Vaulted blocks in the corners, stepped in at the top.
    for (x, z) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        let r = 3.6;
        let at = Vec2::new(x * (w - r), z * (l - r));
        let h = rng.range(5.0, 7.0);
        site.put(at, Vec2::splat(r), (-0.2, h), CONCRETE, C, true);
        site.put(
            at,
            Vec2::splat(r + 0.35),
            (h, h + 0.6),
            STONE * 0.9,
            C,
            false,
        );
        for k in 1..4 {
            let u = r * (1.0 - k as f32 * 0.24);
            let y = h + 0.6 + (k - 1) as f32 * 0.8;
            site.put(at, Vec2::splat(u), (y, y + 0.8), CONCRETE * 0.92, C, false);
        }
    }
    // The pool: a low lip round it, gaps to drive in by.
    let (pw, pl) = (w * 0.42, l * 0.42);
    for e in [-1.0f32, 1.0] {
        for k in [-1.0f32, 1.0] {
            let (a, b) = ((pw - 3.0) * 0.5, (pl - 3.0) * 0.5);
            let at = Vec2::new(k * (3.0 + a), e * pl);
            site.put(at, Vec2::new(a, 0.45), (-0.2, 1.1), STONE * 0.92, C, true);
            let at = Vec2::new(e * pw, k * (3.0 + b));
            site.put(at, Vec2::new(0.45, b), (-0.2, 1.1), STONE * 0.92, C, true);
        }
    }
    // Columns round it, some down.
    let n = 5;
    for k in 0..n {
        let u = (k as f32 / (n - 1) as f32) * 2.0 - 1.0;
        for e in [-1.0f32, 1.0] {
            for at in [
                Vec2::new(u * (w - 8.6), e * (l - 5.0)),
                Vec2::new(e * (w - 5.0), u * (l - 8.6)),
            ] {
                let h = if rng.chance(0.3) {
                    rng.range(1.4, 3.6)
                } else {
                    6.4
                };
                site.put(at, Vec2::splat(0.75), (-0.1, 0.7), STONE * 0.9, C, true);
                site.put(at, Vec2::splat(0.5), (0.7, h), STONE, C, true);
                if h > 6.0 {
                    site.put(at, Vec2::splat(0.8), (h, h + 0.5), STONE * 0.9, C, false);
                }
            }
        }
    }
}

fn arsenal(map: &mut Map, rng: &mut Rng, lot: &Lot) {
    let mut site = Site::new(map, lot);
    let (w, l) = site.half();
    // Blast walls in rows, staggered, gaps never in line.
    let rows = (l * 2.0 / 8.5) as i32;
    for k in 0..rows {
        let z = -l + 4.0 + k as f32 * (l * 2.0 - 8.0) / (rows - 1).max(1) as f32;
        let gap = if k % 2 == 0 { -1.0 } else { 1.0 } * w * rng.range(0.25, 0.5);
        for (a, b) in [(-w + 0.6, gap - 3.6), (gap + 3.6, w - 0.6)] {
            if b - a < 2.0 {
                continue;
            }
            let h = if rng.chance(0.3) { 1.4 } else { 3.4 };
            let at = Vec2::new((a + b) * 0.5, z);
            let half = Vec2::new((b - a) * 0.5, 0.55);
            site.put(at, half, (-0.2, h), CONCRETE * 0.85, C, true);
            if h > 3.0 {
                site.put(at, half + 0.15, (h, h + 0.3), CONCRETE_DARK, C, false);
            }
            // Crates along it.
            let n = rng.int(1, 2);
            for i in 0..n {
                let x = a + 1.4 + i as f32 * 2.7;
                if x < b - 1.2 {
                    let color = CRIMSON * rng.range(0.7, 1.0);
                    let at = Vec2::new(x, z + 1.75);
                    let half = Vec2::new(1.15, 1.05);
                    site.put(at, half, (-0.05, 1.9), color, Material::Painted, true);
                }
            }
        }
    }
    // A watchtower at one corner.
    let at = Vec2::new(w - 1.8, l - 1.8);
    site.put(at, Vec2::splat(1.7), (-0.2, 9.0), CONCRETE, C, true);
    site.put(at, Vec2::splat(2.1), (9.0, 10.4), CONCRETE_DARK, C, false);
}

fn basilica(map: &mut Map, rng: &mut Rng, lot: &Lot) {
    let mut site = Site::new(map, lot);
    let (w, l) = site.half();
    let aisle = w * 0.5;
    let (low, high) = (6.0, 11.0);
    // Side walls in piers, open between; the far end closed, an apse in it.
    let n = (l * 2.0 / 6.4).round() as i32;
    for e in [-1.0f32, 1.0] {
        for k in 0..=n {
            let z = -l + 0.9 + k as f32 * (l * 2.0 - 1.8) / n as f32;
            let wall = Vec2::new(e * (w - 0.6), z);
            site.put(wall, Vec2::new(0.55, 0.85), (-0.2, low), CONCRETE, C, true);
            if k > 0 && k < n && !rng.chance(0.12) {
                let at = Vec2::new(e * aisle, z);
                site.put(at, Vec2::splat(0.6), (-0.1, high - 1.0), STONE, C, true);
                site.put(
                    at,
                    Vec2::splat(0.85),
                    (high - 1.0, high - 0.4),
                    STONE * 0.9,
                    C,
                    false,
                );
            }
        }
        // The aisle's roof, and the nave's wall over the columns.
        let mid = (w + aisle) * 0.5;
        let half = Vec2::new((w - aisle) * 0.5 - 0.05, l - 0.1);
        let at = Vec2::new(e * mid, 0.0);
        site.put(at, half, (low, low + 0.6), CONCRETE * 0.9, C, true);
        let at = Vec2::new(e * (aisle + 0.02), 0.0);
        let half = Vec2::new(0.5, l - 0.9);
        site.put(at, half, (high - 0.4, high + 1.6), CONCRETE, C, true);
    }
    // The nave's roof, gone over part of it.
    let gone = rng.range(0.15, 0.5) * l;
    let half = Vec2::new(aisle + 0.9, l - gone * 0.5 - 0.4);
    let at = Vec2::new(0.0, gone * 0.5);
    site.put(
        at,
        half,
        (high + 1.6, high + 2.4),
        CONCRETE_DARK * 1.3,
        C,
        true,
    );
    let at = Vec2::new(0.0, l - 0.7);
    site.put(
        at,
        Vec2::new(aisle - 0.5, 0.6),
        (-0.2, high - 0.5),
        CONCRETE * 0.95,
        C,
        true,
    );
    let top = site.point(Vec2::new(0.0, l - 1.4), high - 1.2);
    let out = -site.f.axes().1;
    map.banner(top, out, 3.4, 7.5);
}

fn rostra(map: &mut Map, lot: &Lot) {
    let mut site = Site::new(map, lot);
    let floor = civic::FLOOR_H;
    site.put(
        Vec2::ZERO,
        Vec2::splat(1.5),
        (floor - 0.05, floor + 2.0),
        STONE * 0.92,
        C,
        true,
    );
    let foot = site.point(Vec2::ZERO, floor + 2.0);
    let (ax, _) = site.f.axes();
    let w = site.half().0;
    map.drum(foot, foot + Vec3::Y * 11.0, 0.95, STONE);
    map.disc(
        foot + Vec3::Y * 12.4,
        Vec3::new(ax.x, 0.0, ax.y) * 0.12,
        1.4,
        GOLD,
        Material::Metal,
    );
    map.pipe(
        foot + Vec3::Y * 11.0,
        foot + Vec3::Y * 12.0,
        0.2,
        GOLD,
        Material::Metal,
        false,
    );
    for e in [-1.0f32, 1.0] {
        let p = lot.footprint.center + ax * e * (w + 0.02);
        map.banner(Vec3::new(p.x, floor - 0.1, p.y), ax * e, 2.2, 2.0);
    }
}
