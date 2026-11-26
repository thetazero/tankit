//! The Imperium's other great buildings, in the rotunda's place:
//!
//! - the **amphitheatre**: an open bowl, its seats a slope tanks drive up
//!   to the rim, ways in cut through it; whoever's on the rim looks down
//!   on the sand.
//! - the **forum**: an open square between double colonnades, a podium in
//!   the middle, a temple front across one side; long sight lines.

use super::*;
use crate::map::procgen::shield::{between_doors, dir, shelled};

const C: Material = Material::Concrete;

/// Where the seats start, and how high their rim is.
const SAND: f32 = 8.5;
const RIM: f32 = 4.0;

/// How far round from the nearest door `a` is, m, at `r` from the middle.
fn off_door(lm: &Landmark, a: f32, r: f32) -> f32 {
    lm.doors
        .iter()
        .map(|&d| apart(a, angle(d)) * r)
        .fold(f32::MAX, f32::min)
}

pub(super) fn amphitheatre(map: &mut Map, rng: &mut Rng, lm: &Landmark, base: f32) -> Vec<Drift> {
    let c = lm.center;
    let n = 44;
    let way = 5.2;
    let outer = lm.radius - 1.6;
    let run = outer - SAND;
    let slope = RIM.atan2(run);
    for k in 0..n {
        let a = (k as f32 + 0.5) / n as f32 * TAU;
        let d = dir(a);
        let half = lm.radius * PI / n as f32 * 1.03;
        if off_door(lm, a, lm.radius) < way + half {
            continue;
        }
        let gone = shelled(lm, a);
        let yaw = -a + FRAC_PI_2;
        // The outer wall: an arcade of piers under a band.
        let h = if gone {
            RIM + rng.range(-1.6, 1.2)
        } else {
            RIM + 4.6
        };
        let p = c + d * (lm.radius - 0.8);
        map.block(
            p,
            Vec2::new(half, 0.8),
            yaw,
            (base - 0.2, base + RIM),
            CONCRETE * 0.95,
            C,
            true,
        )
        .chip = 0.01;
        if gone {
            map.block(
                p,
                Vec2::new(half * 0.7, 0.75),
                yaw,
                (base + RIM - 0.05, base + h.max(RIM + 0.3)),
                CONCRETE * 0.9,
                C,
                true,
            )
            .broken = true;
        } else {
            map.block(
                p,
                Vec2::new(half * 0.42, 0.72),
                yaw,
                (base + RIM - 0.05, base + h - 0.9),
                CONCRETE,
                C,
                true,
            );
            map.block(
                p,
                Vec2::new(half * 1.01, 0.95),
                yaw,
                (base + h - 0.9, base + h),
                STONE * 0.9,
                C,
                true,
            )
            .chip = 0.01;
        }
        // The seats: a slope down to the sand, in bands.
        let mid = c + d * (SAND + run * 0.5);
        map.oboxes.push(OBox {
            center: mid,
            // Local +z in to the sand, down it.
            yaw: (-d.x).atan2(-d.y),
            half: Vec2::new(outer * PI / n as f32 * 1.02, run * 0.5 / slope.cos() + 0.05),
            y0: base + RIM * 0.5 - 0.5 - 0.004 * (k % 2) as f32,
            y1: base + RIM * 0.5 - 0.004 * (k % 2) as f32,
            color: STONE * if k % 2 == 0 { 0.8 } else { 0.76 },
            side_mat: C,
            top_mat: C,
            tilt: slope,
            ramp: true,
            chip: 0.01,
            ..OBox::default()
        });
        for i in 1..4 {
            let u = i as f32 / 4.0;
            let r = SAND + run * u;
            map.block(
                c + d * r,
                Vec2::new(r * PI / n as f32 * 0.98, 0.16),
                yaw,
                (base + RIM * u - 0.3, base + RIM * u + 0.06),
                STONE * 0.62,
                C,
                false,
            );
        }
    }
    // The cut sides of the ways in.
    for &d in &lm.doors {
        let side = Vec2::new(-d.y, d.x);
        for e in [-1.0f32, 1.0] {
            let m = 4;
            for k in 0..m {
                let (u0, u1) = (k as f32 / m as f32, (k + 1) as f32 / m as f32);
                let r = SAND + 1.0 + (lm.radius - SAND - 1.2) * (u0 + u1) * 0.5;
                let top = RIM * u1;
                map.block(
                    c + d * r + side * e * (way + 0.4),
                    Vec2::new((lm.radius - SAND - 1.2) * 0.5 / m as f32, 0.5),
                    -angle(d),
                    (base - 0.2, base + top),
                    CONCRETE * 0.85,
                    C,
                    true,
                )
                .chip = 0.01;
            }
            let p = c + d * (lm.radius - 0.4) + side * e * (way + 0.9);
            map.block(
                p,
                Vec2::splat(1.3),
                -angle(d),
                (base - 0.2, base + RIM + 5.2),
                STONE * 0.92,
                C,
                true,
            );
        }
        let p = c + d * (lm.radius - 0.4);
        map.banner(
            Vec3::new(p.x, base + RIM + 5.0, p.y)
                + Vec3::new(side.x, 0.0, side.y) * (way + 0.9)
                + Vec3::new(d.x, 0.0, d.y) * 1.35,
            d,
            2.0,
            6.0,
        );
    }
    // The sand's marker.
    let at = |y: f32| Vec3::new(c.x, base + y, c.y);
    map.drum(at(0.0), at(0.9), 2.2, STONE * 0.9);
    map.block(
        c,
        Vec2::splat(0.7),
        0.0,
        (base + 0.9, base + 4.6),
        STONE,
        C,
        true,
    );
    map.disc(at(5.6), Vec3::X * 0.12, 1.0, GOLD, Material::Metal);
    map.sound(Ambient::Dome, at(3.0));
    map.sound(Ambient::Banner, at(6.0));
    map.generator_rubble(rng, lm, base, CONCRETE * 0.9)
}

pub(super) fn forum(map: &mut Map, rng: &mut Rng, lm: &Landmark, base: f32) -> Vec<Drift> {
    let c = lm.center;
    let at = |y: f32| Vec3::new(c.x, base + y, c.y);
    // Two rings of columns under a beam, broken at the ways in.
    for (r, n, h) in [(lm.radius - 1.0, 30, 9.0), (lm.radius - 6.0, 24, 9.0)] {
        let mut up = vec![false; n];
        for (k, up) in up.iter_mut().enumerate() {
            let a = (k as f32 + 0.5) / n as f32 * TAU;
            if off_door(lm, a, r) < 5.6 {
                continue;
            }
            let p = c + dir(a) * r;
            let foot = Vec3::new(p.x, base, p.y);
            if shelled(lm, a) || rng.chance(0.1) {
                if rng.chance(0.6) {
                    map.column(foot, 0.8, rng.range(1.8, 4.5), true);
                }
                continue;
            }
            map.column(foot, 0.8, h, false);
            *up = true;
        }
        for k in 0..n {
            if !(up[k] && up[(k + 1) % n]) {
                continue;
            }
            let a = (k as f32 + 1.0) / n as f32 * TAU;
            map.block(
                c + dir(a) * r,
                Vec2::new(r * PI / n as f32 + 0.9, 1.0),
                -a + FRAC_PI_2,
                (base + h, base + h + 1.3),
                STONE * 0.94,
                C,
                true,
            )
            .chip = 0.01;
        }
    }
    // The podium, and a standard on it.
    map.drum(at(0.0), at(1.0), 4.5, STONE * 0.94);
    map.block(
        c,
        Vec2::splat(1.4),
        0.0,
        (base + 1.0, base + 4.0),
        CONCRETE,
        C,
        true,
    );
    map.pipe(at(4.0), at(16.0), 0.3, GOLD * 0.8, Material::Metal, false);
    for out in AXES {
        let p = c + out * 1.56;
        map.standing_emblem(Vec3::new(p.x, base + 2.7, p.y), out, 0.6);
    }
    map.disc(at(16.8), Vec3::X * 0.14, 1.3, GOLD, Material::Metal);
    // A temple front, across the widest way between two doors.
    let widest = between_doors(lm)
        .into_iter()
        .filter(|&a| !shelled(lm, a))
        .max_by(|&a, &b| off_door(lm, a, 1.0).total_cmp(&off_door(lm, b, 1.0)));
    if let Some(a) = widest {
        let (d, side) = (dir(a), Vec2::new(-dir(a).y, dir(a).x));
        let m = c + d * (lm.radius + 5.0);
        map.block(
            m,
            Vec2::new(8.0, 2.6),
            -a + FRAC_PI_2,
            (base - 0.2, base + 12.0),
            CONCRETE,
            C,
            true,
        )
        .chip = 0.01;
        for k in 0..3 {
            let u = 1.0 - k as f32 * 0.3;
            let y = base + 12.0 + k as f32 * 1.4;
            map.block(
                m - d * 0.2,
                Vec2::new(8.6 * u, 3.2),
                -a + FRAC_PI_2,
                (y, y + 1.4),
                STONE * 0.9,
                C,
                false,
            );
        }
        for k in -1..=1 {
            let p = m - d * 2.65 + side * k as f32 * 4.6;
            map.banner(Vec3::new(p.x, base + 11.0, p.y), -d, 2.6, 8.0);
        }
    }
    map.sound(Ambient::Dome, at(6.0));
    map.generator_rubble(rng, lm, base, STONE * 0.85)
}
