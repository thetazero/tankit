//! Where a city stands, whoever built it: what its sky, its ground and its
//! horizon are. One pass over a finished city, so a style needn't know.
//!
//! In the desert: the desert's own sky and one of its palettes; sand for
//! ground, banked up the windward sides of buildings and blown over the
//! streets; stone bleached and dusted; dunes and mesas past the houses.

use glam::{Vec2, Vec3};

use super::desert;
use super::skeleton::{LotKind, Skeleton, smoothstep};
use crate::map::noise::noise2;
use crate::map::ruins::Drift;
use crate::map::{Map, rocks};
use crate::render::material::Material;
use crate::rng::Rng;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Setting {
    /// Built-over land, a cold haze.
    #[default]
    Plain,
    Desert,
}

/// Sets `map`, the city laid out as `sk` (its roads tinted `road`), in the
/// desert: the sand it banks up, and its rock's color.
pub(in crate::map) fn desert(
    map: &mut Map,
    sk: &Skeleton,
    seed: u64,
    road: Vec3,
) -> (Vec<Drift>, Vec3) {
    let mut rng = Rng::new(seed ^ 0xDE5E_47ED);
    let pal = &desert::PALETTES[rng.index(desert::PALETTES.len())];
    let pale = rng.range(0.74, 0.86);
    map.atmosphere = desert::atmosphere(&mut rng, pal);
    map.planet = Some((seed, pal.planet));
    let wind = map.atmosphere.wind.normalize_or(Vec2::X);

    // Sand banked against the sides the wind's on.
    let drifts: Vec<Drift> = sk
        .lots
        .iter()
        .filter(|l| !matches!(l.kind, LotKind::Civic(_) | LotKind::Raised))
        .filter_map(|l| {
            let (ax, az) = l.footprint.axes();
            let faces = [
                (0, 1.0f32, ax),
                (0, -1.0, -ax),
                (1, 1.0, az),
                (1, -1.0, -az),
            ];
            let (axis, sign, n) = faces
                .into_iter()
                .max_by(|a, b| (-wind).dot(a.2).total_cmp(&(-wind).dot(b.2)))?;
            ((-wind).dot(n) > 0.6 && rng.chance(0.7)).then(|| Drift {
                b: l.footprint,
                axis,
                sign,
                height: rng.range(0.5, 1.5),
                run: rng.range(4.0, 7.0),
            })
        })
        .collect();
    let banked = |p: Vec2| {
        drifts
            .iter()
            .filter(|d| d.b.center.distance_squared(p) < 400.0)
            .map(|d| d.height_at(p))
            .fold(0.0, f32::max)
    };
    // How much sand lies over the road at `p`.
    let blown = |p: Vec2| {
        let n = noise2(p.x / 17.0, p.y / 17.0, 11);
        smoothstep(0.62, 0.95, n)
    };
    // Past the houses, dunes.
    let dunes = |p: Vec2| {
        let out = (-p).max(p - Vec2::splat(sk.size)).max_element();
        let k = smoothstep(150.0, 260.0, out);
        if k <= 0.0 {
            return 0.0;
        }
        let q = Vec2::new(p.dot(wind), p.perp_dot(wind));
        let along = q.x / 46.0 + noise2(q.y / 60.0, q.x / 90.0, 12) * 1.6;
        k * (9.0 * (0.5 + 0.5 * (along * std::f32::consts::TAU * 0.5).sin())
            + 5.0 * noise2(p.x / 120.0, p.y / 120.0, 13))
    };
    for (t, inside) in [(&mut map.terrain, true), (&mut map.scenery_terrain, false)] {
        let Some(t) = t else { continue };
        t.color = pal.sand * pale;
        t.material = Material::Sand;
        for iz in 0..t.nz {
            for ix in 0..t.nx {
                let k = iz * t.nx + ix;
                let p = t.origin + Vec2::new(ix as f32, iz as f32) * t.cell;
                let bank = if inside { banked(p) } else { 0.0 };
                t.heights[k] += if inside { bank } else { dunes(p) };
                let Some(tint) = t.tint.get_mut(k) else {
                    continue;
                };
                let over = blown(p).max(smoothstep(0.1, 0.5, bank));
                let packed = Vec3::ONE.lerp(road * 1.5, 0.9);
                let r = sk.road(p) * (1.0 - over);
                *tint =
                    Vec3::ONE.lerp(packed, r) * (0.94 + 0.12 * noise2(p.x / 9.0, p.y / 9.0, 14));
            }
        }
    }

    // Stone bleached, and sand-dusted.
    for o in &mut map.oboxes {
        for (m, dust) in [
            (Material::Concrete, 0.3),
            (Material::Plaster, 0.22),
            (Material::Rubble, 0.35),
            (Material::Wall, 0.25),
            (Material::Roof, 0.12),
        ] {
            if o.side_mat == m {
                let light = o.color.element_sum() / 3.0;
                let sand = pal.sand * (0.35 + light * 0.95);
                o.color = o.color.lerp(sand, dust);
            }
        }
    }
    map.rock_dust = Some(pal.sand);

    // Mesas a long way off.
    for _ in 0..rng.int(9, 15) {
        let a = rng.range(0.0, std::f32::consts::TAU);
        let d = sk.size * 0.5 + rng.range(240.0, 420.0);
        let p = Vec2::splat(sk.size * 0.5) + Vec2::from_angle(a) * d;
        let wide = rng.range(22.0, 60.0);
        let half = Vec2::new(wide, wide * rng.range(0.5, 1.0));
        let h = rng.range(26.0, 70.0);
        let g = sk.ground(p) + dunes(p);
        let color = pal.stone.lerp(pal.stone_pale, rng.f32());
        map.oboxes.push(crate::map::OBox {
            solid: false,
            ..rocks::block(p, rng.range(0.0, 6.28), half, g - 8.0, g + h, color)
        });
    }
    (drifts, pal.stone)
}
