//! The Ithessar's medium structures:
//!
//! - a courtyard's: a **kiln court**, a ring of curved wall in two shells,
//!   its gates out of line, a blade in the middle.
//! - a market's: a **lens hall**, a great slab of glass carried on crystal
//!   legs.
//! - a terrace's: the raised floor, with blades leaning over it.

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
        CivicKind::Courtyard => kiln_court(map, rng, lot),
        CivicKind::Market => lens_hall(map, lot),
        CivicKind::Terrace => {
            civic::build(map, rng, lot, kind, s);
            let f = &lot.footprint;
            let (_, az) = f.axes();
            let w = f.half.x;
            let (ax, _) = f.axes();
            for e in [-1.0f32, 1.0] {
                let p = f.center + ax * e * (w - 1.6) + az * e * 2.0;
                let lean = Vec3::new(-ax.x * e * 0.4, 1.0, -ax.y * e * 0.4).normalize();
                crystal(
                    map,
                    rng,
                    Vec3::new(p.x, civic::FLOOR_H - 0.3, p.y),
                    lean * 9.0,
                    0.8,
                    true,
                    true,
                );
            }
        }
    }
}

fn kiln_court(map: &mut Map, rng: &mut Rng, lot: &Lot) {
    let o = &lot.footprint;
    let big = o.half.min_element() - 0.8;
    let turn = rng.range(0.0, TAU);
    for (r, gates, h, shift) in [(big, 4, 4.6, 0.0), (big * 0.55, 3, 3.2, 0.7)] {
        let n = (r * 1.5) as i32 + 8;
        for k in 0..n {
            let a = turn + shift + (k as f32 + 0.5) / n as f32 * TAU;
            let open = (0..gates).any(|g| {
                let at = turn + shift + g as f32 / gates as f32 * TAU;
                apart(a, at) * r < 4.6
            });
            if open {
                continue;
            }
            let broken = rng.chance(0.12);
            let hh = if broken { rng.range(0.8, 2.0) } else { h };
            let half = Vec2::new(r * PI / n as f32 * 1.05, 0.6);
            put(
                map,
                o,
                dir(a) * r,
                half,
                -a + FRAC_PI_2,
                (-0.4, hh),
                WALLS[(k % 2) as usize] * 1.05,
                C,
                true,
            );
            map.oboxes.last_mut().unwrap().broken = broken;
            if !broken {
                put(
                    map,
                    o,
                    dir(a) * r,
                    half + Vec2::new(0.04, 0.16),
                    -a + FRAC_PI_2,
                    (hh - 0.02, hh + 0.3 + 0.004 * (k % 3) as f32),
                    GLASS * 1.6,
                    V,
                    false,
                );
            }
        }
    }
    let c = o.center;
    for k in 0..3 {
        let d = dir(turn + k as f32 * TAU / 3.0);
        let lean = Vec3::new(-d.x * 0.35, 1.0, -d.y * 0.35).normalize();
        let h = rng.range(6.0, 9.0);
        crystal(
            map,
            rng,
            Vec3::new(c.x + d.x * 1.8, -0.5, c.y + d.y * 1.8),
            lean * h,
            0.7,
            true,
            true,
        );
    }
    map.sound(Ambient::Crystal, Vec3::new(c.x, 3.0, c.y));
}

fn lens_hall(map: &mut Map, lot: &Lot) {
    let mut site = Site::new(map, lot);
    let (w, l) = site.half();
    let h = 7.2;
    let n = ((l * 2.0) / 9.0).round().max(2.0) as i32;
    let mut legs = Vec::new();
    for j in 0..=n {
        for e in [-1.0f32, 1.0] {
            // In at the ends, as a lens's edge is.
            let u = j as f32 / n as f32 * 2.0 - 1.0;
            let x = e * (w - 1.2) * (1.0 - 0.4 * u * u);
            legs.push(Vec2::new(x, u * (l - 1.2)));
        }
    }
    for at in legs {
        site.put(at, Vec2::splat(0.6), (-0.3, h + 0.1), GLASS * 1.4, V, true);
        site.map.oboxes.last_mut().unwrap().chip = 0.01;
    }
    // Its roof in slabs, wide in the middle.
    let m = 7;
    for k in 0..m {
        let u = (k as f32 + 0.5) / m as f32 * 2.0 - 1.0;
        let half = Vec2::new(w * (1.0 - 0.42 * u * u), l / m as f32 * 1.01);
        let y = h + 0.02 * (k % 2) as f32;
        site.put(
            Vec2::new(0.0, u * l),
            half,
            (y, y + 0.9),
            STONE * 1.1,
            C,
            true,
        );
        site.put(
            Vec2::new(0.0, u * l),
            Vec2::new(half.x * 0.55, half.y * 0.9),
            (y + 0.85, y + 1.5),
            GLASS * 1.7,
            V,
            false,
        );
        site.put(
            Vec2::new(0.0, u * l),
            Vec2::new(half.x * 0.12, half.y * 0.8),
            (y - 0.08, y + 0.02),
            PLASMA * 0.9,
            G,
            false,
        );
    }
}
