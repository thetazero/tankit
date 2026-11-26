//! The Resonant's other great buildings, in the cathedral's place:
//!
//! - the **abbey**: a walled close, cells round its wall, an inner wall
//!   with its gates set between the outer's; corners everywhere.
//! - the **carillon**: a bell tower lifted on great arches, open under it
//!   from any side, bells hung over the crystal.

use super::*;

const P: Material = Material::Plaster;
const M: Material = Material::Metal;

fn heart(map: &mut Map, rng: &mut Rng, lm: &Landmark, base: f32, y: f32) {
    let at = |y: f32| Vec3::new(lm.center.x, base + y, lm.center.y);
    map.drum(at(0.0), at(1.0), 3.4, IVORY * 0.95);
    map.block(
        lm.center,
        Vec2::splat(1.2),
        0.0,
        (base + 1.0, base + 2.8),
        IVORY,
        P,
        true,
    );
    map.block(
        lm.center,
        Vec2::splat(1.4),
        0.0,
        (base + 2.8, base + 3.1),
        GILT,
        M,
        false,
    );
    for (len, up) in [(5.0f32, 1.0f32), (3.2, -1.0)] {
        map.crystals.push(Crystal {
            base: at(y),
            axis: Vec3::Y * len * up,
            radius: 1.2,
            sides: 6,
            seed: rng.next_u64() as u32,
            color: CRYSTAL_LIGHT,
            lit: false,
            solid: false,
            raw: false,
        });
    }
    map.lights.push(PointLight {
        pos: at(y),
        color: CRYSTAL_LIGHT * 13.0,
        radius: 24.0,
    });
    map.sound(Ambient::Altar, at(y));
}

pub(super) fn abbey(map: &mut Map, rng: &mut Rng, lm: &Landmark, base: f32) {
    let c = lm.center;
    // The outer wall, gabled cells on its inside.
    for (k, (a, half)) in ring(lm, lm.radius, 36).into_iter().enumerate() {
        let d = dir(a);
        let yaw = -a + FRAC_PI_2;
        let gone = shelled(lm, a);
        // Wider at the doors than a wall's one opening.
        if lm
            .doors
            .iter()
            .any(|&q| apart(a, angle(q)) * lm.radius < 5.4 + half)
        {
            continue;
        }
        let h = if gone { rng.range(0.9, 3.0) } else { 5.6 };
        let p = c + d * (lm.radius - 0.7);
        map.block(
            p,
            Vec2::new(half, 0.7),
            yaw,
            (base - 0.2, base + h),
            IVORY,
            P,
            true,
        )
        .chip = 0.01;
        if gone {
            map.oboxes.last_mut().unwrap().broken = true;
            continue;
        }
        map.block(
            p,
            Vec2::new(half * 1.02, 0.95),
            yaw,
            (base + h, base + h + 0.4),
            SLATE,
            Material::Roof,
            false,
        );
        if k % 3 == 0 {
            let q = c + d * (lm.radius - 3.2);
            let (w, deep, tall) = (2.0, 1.9, 4.2);
            map.block(
                q,
                Vec2::new(w, deep),
                yaw,
                (base - 0.2, base + tall),
                IVORY * 0.94,
                P,
                true,
            );
            for i in 0..4 {
                let u = 1.0 - i as f32 * 0.25;
                let y = base + tall + i as f32 * 0.6;
                map.block(
                    q,
                    Vec2::new(w * u + 0.2, deep + 0.2),
                    yaw,
                    (y, y + 0.6),
                    SLATE * 1.05,
                    Material::Roof,
                    false,
                );
            }
            let m = q - d * (deep + 0.04);
            map.block(
                m,
                Vec2::new(0.45, 0.06),
                yaw,
                (base + 1.4, base + 3.2),
                CRYSTAL_LIGHT * 0.6,
                Material::Glow,
                false,
            );
        }
    }
    // The inner wall: open where the outer's shut.
    let r = 12.5;
    let gates = between_doors(lm);
    let n = 24;
    for k in 0..n {
        let a = (k as f32 + 0.5) / n as f32 * TAU;
        let half = r * std::f32::consts::PI / n as f32 * 1.03;
        let open = gates.iter().any(|&g| apart(a, g) * r < 4.6 + half);
        if open || shelled(lm, a) {
            continue;
        }
        let h = if rng.chance(0.2) {
            rng.range(1.0, 2.0)
        } else {
            3.8
        };
        map.block(
            c + dir(a) * r,
            Vec2::new(half, 0.6),
            -a + FRAC_PI_2,
            (base - 0.2, base + h),
            IVORY * 0.96,
            P,
            true,
        )
        .chip = 0.01;
        if h > 3.0 {
            map.block(
                c + dir(a) * r,
                Vec2::new(half * 1.02, 0.8),
                -a + FRAC_PI_2,
                (base + h, base + h + 0.3),
                GILT,
                M,
                false,
            );
        }
    }
    heart(map, rng, lm, base, 7.5);
    // A slim spire over it on four posts.
    let at = |x: f32, y: f32, z: f32| Vec3::new(c.x + x, base + y, c.y + z);
    for (x, z) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        map.pipe(
            at(x * 4.6, 0.0, z * 4.6),
            at(x * 3.0, 14.0, z * 3.0),
            0.4,
            GILT,
            M,
            true,
        );
    }
    cone(
        map,
        at(0.0, 0.0, 0.0),
        (4.8, 14.0),
        (0.15, 36.0),
        6,
        |_, _| false,
    );
}

pub(super) fn carillon(map: &mut Map, rng: &mut Rng, lm: &Landmark, base: f32) {
    let c = lm.center;
    let at = |a: f32, r: f32, y: f32| Vec3::new(c.x + a.cos() * r, base + y, c.y + a.sin() * r);
    // Eight piers leaning in, clear of the ways.
    let turn = lm.doors.first().map_or(0.0, |&d| angle(d)) + TAU / 16.0;
    let (foot, spring, shoulder) = (13.5, 12.0, 7.0);
    for k in 0..8 {
        let a = turn + k as f32 / 8.0 * TAU;
        if lm.doors.iter().any(|&d| apart(a, angle(d)) * foot < 4.4) {
            continue;
        }
        let d = dir(a);
        map.block(
            c + d * foot,
            Vec2::new(1.5, 1.2),
            -a,
            (base - 0.2, base + 5.0),
            IVORY,
            P,
            true,
        );
        map.block(
            c + d * foot,
            Vec2::new(1.75, 1.45),
            -a,
            (base + 5.0, base + 5.4),
            GILT,
            M,
            false,
        );
        map.pipe(
            at(a, foot, 5.2),
            at(a, shoulder, spring),
            0.85,
            IVORY * 0.97,
            P,
            false,
        );
        // A pinnacle standing on the pier.
        map.block(
            c + d * (foot + 0.4),
            Vec2::splat(0.6),
            -a,
            (base + 5.4, base + 10.5),
            IVORY * 0.92,
            P,
            false,
        );
        map.block(
            c + d * (foot + 0.4),
            Vec2::splat(0.4),
            -a + FRAC_PI_4,
            (base + 10.5, base + 12.2),
            GILT,
            M,
            false,
        );
    }
    // The tower over them: eight sides, belfry openings, a spire.
    for k in 0..8 {
        let a = turn + (k as f32 + 0.5) / 8.0 * TAU;
        let half = shoulder * (std::f32::consts::PI / 8.0).tan() + 0.1;
        let p = c + dir(a) * shoulder;
        let yaw = -a + FRAC_PI_2;
        for (y0, y1, w) in [
            (spring - 0.4, spring + 6.0, half),
            (spring + 14.0, spring + 17.0, half),
        ] {
            map.block(
                p,
                Vec2::new(w, 0.6),
                yaw,
                (base + y0, base + y1),
                IVORY,
                P,
                false,
            )
            .chip = 0.01;
        }
        map.block(
            p,
            Vec2::new(half * 1.03, 0.85),
            yaw,
            (base + spring + 6.0, base + spring + 6.5),
            GILT,
            M,
            false,
        );
        map.pipe(
            at(turn + k as f32 / 8.0 * TAU, shoulder, spring + 6.0),
            at(turn + k as f32 / 8.0 * TAU, shoulder, spring + 14.4),
            0.55,
            IVORY * 0.96,
            P,
            false,
        );
    }
    cone(
        map,
        at(0.0, 0.0, 0.0),
        (shoulder + 1.0, spring + 17.0),
        (0.2, spring + 46.0),
        8,
        |_, _| false,
    );
    map.pipe(
        at(0.0, 0.0, spring + 45.0),
        at(0.0, 0.0, spring + 49.0),
        0.14,
        GILT,
        M,
        false,
    );
    // The bells.
    for (k, r) in [(0, 1.7f32), (1, 1.3), (2, 1.05)] {
        let a = turn + k as f32 * TAU / 3.0;
        let top = at(a, 3.4, spring + 13.0);
        map.segment(top, top + Vec3::Y * 1.6, 0.1, GILT, M, false);
        for (y0, y1, u) in [(0.0, 0.7, 0.5), (0.7, 2.2, 0.78), (2.2, 3.0, 1.0)] {
            map.pipe(
                top - Vec3::Y * y1 * r * 0.8,
                top - Vec3::Y * y0 * r * 0.8,
                r * u,
                BRONZE * (1.0 - u * 0.08),
                M,
                false,
            );
        }
        map.sound(Ambient::Bell, top);
    }
    heart(map, rng, lm, base, 7.0);
}
