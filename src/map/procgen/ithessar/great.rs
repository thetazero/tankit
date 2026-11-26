//! The Ithessar's great buildings:
//!
//! - the **Sun**: what feeds the river. Great blades lean in over a dais and
//!   hold a ball of plasma high between their tips; a low broken ring round
//!   them, open all about.
//! - the **Lens**: their emblem, built. Two tall curved walls meeting in
//!   points, an almond to drive through, an orb held in the middle.
//! - the **Weir**: where the plasma's parted. A ring of sluices on stone
//!   piers, basins between, plasma falling from each.

use super::*;
use crate::map::Core;
use crate::map::procgen::shield::{between_doors, ring, shelled};
use crate::render::PointLight;

fn at(lm: &Landmark, base: f32, y: f32) -> Vec3 {
    Vec3::new(lm.center.x, base + y, lm.center.y)
}

fn off_door(lm: &Landmark, a: f32, r: f32) -> f32 {
    lm.doors
        .iter()
        .map(|&d| apart(a, angle(d)) * r)
        .fold(f32::MAX, f32::min)
}

fn orb(map: &mut Map, lm: &Landmark, base: f32, y: f32, radius: f32, prongs: Vec<Vec3>) {
    map.cores.push(Core {
        at: at(lm, base, y),
        radius,
        prongs,
    });
    map.lights.push(PointLight {
        pos: at(lm, base, y),
        color: PLASMA * 26.0,
        radius: 42.0,
    });
    map.sound(Ambient::Core, at(lm, base, y));
}

pub(super) fn sun(map: &mut Map, rng: &mut Rng, lm: &Landmark, base: f32) {
    let c = lm.center;
    // The dais, stepped, plasma round its foot.
    for (r, y0, y1, u) in [(6.5, 0.0, 0.7, 1.0), (4.6, 0.65, 1.5, 1.15)] {
        map.pipe(at(lm, base, y0), at(lm, base, y1), r, STONE * u, C, true);
        map.disc(
            at(lm, base, y1 - 0.06),
            Vec3::Y * 0.03,
            r - 0.1,
            STONE * u * 0.95,
            C,
        );
    }
    let n = 32;
    map.glow_paths.push(GlowPath {
        points: (0..=n)
            .map(|k| {
                let q = c + dir(k as f32 / n as f32 * TAU) * 7.3;
                Vec3::new(q.x, base + 0.05, q.y)
            })
            .collect(),
        widths: vec![0.45; n + 1],
        color: PLASMA,
    });
    // The blades, clear of the ways in.
    let heart = 22.0;
    let mut prongs = Vec::new();
    let turn = lm.doors.first().map_or(0.0, |&d| angle(d)) + TAU / 14.0;
    for k in 0..7 {
        let a = turn + k as f32 / 7.0 * TAU;
        if off_door(lm, a, 14.5) < 5.0 {
            continue;
        }
        let d = dir(a);
        let foot = c + d * 14.5;
        let tip = at(lm, base, heart + 1.0) + Vec3::new(d.x, 0.0, d.y) * 4.6;
        let from = Vec3::new(foot.x, base - 0.8, foot.y);
        let axis = (tip - from) * if shelled(lm, a) { 0.4 } else { 1.0 };
        crystal(map, rng, from, axis, 1.9, true, true);
        if !shelled(lm, a) {
            prongs.push(tip);
        }
    }
    orb(map, lm, base, heart, 3.4, prongs);
    // The ring: low, in stretches.
    for (k, (a, half)) in ring(lm, lm.radius, 36).into_iter().enumerate() {
        if off_door(lm, a, lm.radius) < 6.0 + half || k % 5 == 4 {
            continue;
        }
        let h = if shelled(lm, a) {
            rng.range(0.6, 1.6)
        } else {
            3.2
        };
        map.block(
            c + dir(a) * (lm.radius - 0.8),
            Vec2::new(half, 0.8),
            -a + FRAC_PI_2,
            (base - 0.3, base + h),
            STONE * 1.1,
            C,
            true,
        )
        .chip = 0.01;
        if h > 3.0 {
            map.block(
                c + dir(a) * (lm.radius - 0.8),
                Vec2::new(half * 1.02, 1.0),
                -a + FRAC_PI_2,
                (base + h, base + h + 0.4 + 0.004 * (k % 3) as f32),
                GLASS * 1.6,
                V,
                false,
            );
        }
    }
}

pub(super) fn lens(map: &mut Map, rng: &mut Rng, lm: &Landmark, base: f32) {
    let c = lm.center;
    // Its points toward the first door and away from it.
    let long = lm.doors.first().copied().unwrap_or(Vec2::X);
    let wide = Vec2::new(-long.y, long.x);
    let (a, b) = (lm.radius - 0.8, lm.radius * 0.56);
    let n = 15;
    for e in [-1.0f32, 1.0] {
        for k in 0..n {
            let u = (k as f32 + 0.5) / n as f32 * 2.0 - 1.0;
            let x = u * (a - 3.0);
            let curve = |x: f32| (1.0 - (x / a).powi(2)).max(0.0) * b;
            let p = c + long * x + wide * e * curve(x);
            // Open at the points, and where a door comes in on its side.
            let to = p - c;
            if off_door(lm, angle(to), to.length()) < 5.6 {
                continue;
            }
            let (q0, q1) = (curve(x - 0.5), curve(x + 0.5));
            let run = (long + wide * e * (q1 - q0)).normalize();
            let gone = shelled(lm, angle(to));
            let h = if gone {
                rng.range(1.0, 3.0)
            } else {
                5.0 + 8.0 * (1.0 - u * u)
            };
            let yaw = -angle(run);
            let half = Vec2::new((a - 3.0) / n as f32 * 1.12, 0.8);
            map.block(p, half, yaw, (base - 0.3, base + h), STONE * 1.05, C, true)
                .chip = 0.01;
            if gone {
                continue;
            }
            map.block(
                p,
                half + Vec2::new(0.04, 0.2),
                yaw,
                (base + h, base + h + 0.45 + 0.004 * (k % 4) as f32),
                GLASS * 1.6,
                V,
                false,
            );
            for f in [-1.0f32, 1.0] {
                if k % 2 == 0 {
                    let n = Vec2::new(-run.y, run.x) * f;
                    map.block(
                        p + n * 0.81,
                        Vec2::new(0.16, 0.05),
                        yaw,
                        (base + 1.2, base + h - 1.0),
                        PLASMA * 0.85,
                        G,
                        false,
                    );
                }
            }
        }
    }
    // The orb, held between four blades.
    let mut prongs = Vec::new();
    for k in 0..4 {
        let an = angle(long) + FRAC_PI_4 + k as f32 * FRAC_PI_2;
        let d = dir(an);
        let foot = c + d * 5.0;
        let tip = at(lm, base, 8.4) + Vec3::new(d.x, 0.0, d.y) * 2.3;
        let from = Vec3::new(foot.x, base - 0.6, foot.y);
        crystal(map, rng, from, tip - from, 0.8, true, true);
        prongs.push(tip);
    }
    orb(map, lm, base, 8.0, 1.7, prongs);
}

pub(super) fn weir(map: &mut Map, rng: &mut Rng, lm: &Landmark, base: f32) {
    let c = lm.center;
    // Sluices between the doors, plasma falling from each to a basin.
    let mut spots = between_doors(lm);
    let extra: Vec<f32> = (0..8)
        .map(|k| k as f32 / 8.0 * TAU)
        .filter(|&a| off_door(lm, a, 15.0) > 9.0)
        .collect();
    for a in extra {
        if spots.iter().all(|&s| apart(a, s) > 0.62) {
            spots.push(a);
        }
    }
    for a in spots {
        let d = dir(a);
        let side = Vec2::new(-d.y, d.x);
        let gone = shelled(lm, a);
        let p = c + d * (lm.radius - 4.5);
        let yaw = -a + FRAC_PI_2;
        for e in [-1.0f32, 1.0] {
            let h = if gone { rng.range(2.0, 5.0) } else { 12.0 };
            let q = p + side * e * 3.4;
            map.block(
                q,
                Vec2::new(1.1, 1.6),
                yaw,
                (base - 0.3, base + h),
                STONE * 1.1,
                C,
                true,
            )
            .chip = 0.01;
            if !gone {
                let lean = Vec3::new(-d.x * 0.3, 1.0, -d.y * 0.3).normalize();
                crystal(
                    map,
                    rng,
                    Vec3::new(q.x, base + h - 0.3, q.y),
                    lean * 6.5,
                    0.9,
                    true,
                    false,
                );
            }
        }
        // Its basin.
        let m = p - d * 1.2;
        map.block(
            m,
            Vec2::new(2.2, 2.6),
            yaw,
            (base - 0.3, base + 0.9),
            STONE * 0.95,
            C,
            true,
        )
        .chip = 0.01;
        if gone {
            continue;
        }
        map.block(
            p,
            Vec2::new(2.35, 0.9),
            yaw,
            (base + 10.4, base + 11.8),
            GLASS * 1.6,
            V,
            false,
        );
        map.block(
            p - d * 0.6,
            Vec2::new(1.9, 0.12),
            yaw,
            (base + 0.95, base + 10.5),
            PLASMA * 0.9,
            G,
            false,
        );
        map.block(
            m,
            Vec2::new(1.95, 2.3),
            yaw,
            (base + 0.88, base + 0.98),
            PLASMA * 0.75,
            G,
            false,
        );
        map.lights.push(PointLight {
            pos: Vec3::new(m.x, base + 4.0, m.y),
            color: PLASMA * 9.0,
            radius: 16.0,
        });
        map.sound(Ambient::River, Vec3::new(m.x, base + 2.0, m.y));
        // And its run to the middle.
        let n = 5;
        map.glow_paths.push(GlowPath {
            points: (0..=n)
                .map(|k| {
                    let r = lm.radius - 9.0 - (lm.radius - 13.5) * k as f32 / n as f32;
                    let q = c + d * r;
                    Vec3::new(q.x, base + 0.05, q.y)
                })
                .collect(),
            widths: vec![0.5; n + 1],
            color: PLASMA,
        });
    }
    // The middle: a round basin, a blade standing in it.
    let n = 12;
    for k in 0..n {
        let a = (k as f32 + 0.5) / n as f32 * TAU;
        map.block(
            c + dir(a) * 3.8,
            Vec2::new(3.8 * (PI / n as f32).tan() + 0.12, 0.45),
            -a + FRAC_PI_2,
            (base - 0.2, base + 1.1),
            STONE * 1.1,
            C,
            true,
        )
        .chip = 0.01;
    }
    map.disc(at(lm, base, 0.4), Vec3::Y * 0.05, 3.3, PLASMA * 0.8, G);
    crystal(
        map,
        rng,
        at(lm, base, -0.6),
        Vec3::Y * 11.0,
        1.1,
        true,
        true,
    );
    map.lights.push(PointLight {
        pos: at(lm, base, 4.0),
        color: PLASMA * 14.0,
        radius: 24.0,
    });
}
