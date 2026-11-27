//! Where a city stands, whoever built it: what its sky, its ground and its
//! horizon are. One pass over a finished city, so a style needn't know.
//!
//! In the desert: the desert's own sky and one of its palettes; sand for
//! ground, banked up the windward sides of buildings and blown over the
//! streets; stone bleached and dusted; dunes and mesas past the houses.
//!
//! On tundra: snow for ground, over the streets in places, banked against
//! buildings and lying on their roofs; a white sky, snowfields rising to
//! crags. On grassland: grass up to the kerbs, worn to earth in places, tufts
//! in the open, a clear sky, downs and copses past the houses.

use glam::{Vec2, Vec3};

use super::desert;
use super::foundry::{self, Ground};
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
    Tundra,
    Grassland,
}

impl From<Ground> for Setting {
    fn from(ground: Ground) -> Setting {
        match ground {
            Ground::Desert => Setting::Desert,
            Ground::Tundra => Setting::Tundra,
            Ground::Grassland => Setting::Grassland,
        }
    }
}

/// Sand or snow banked against the sides of `sk`'s buildings the wind's on:
/// how high.
fn banked(sk: &Skeleton, wind: Vec2, rng: &mut Rng, high: (f32, f32)) -> Vec<Drift> {
    sk.lots
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
                height: rng.range(high.0, high.1),
                run: rng.range(4.0, 7.0),
            })
        })
        .collect()
}

/// How high `drifts` are at `p`.
fn bank(drifts: &[Drift], p: Vec2) -> f32 {
    drifts
        .iter()
        .filter(|d| d.b.center.distance_squared(p) < 400.0)
        .map(|d| d.height_at(p))
        .fold(0.0, f32::max)
}

/// Sets `map`, the city laid out as `sk`, on `ground` (tundra or grassland),
/// its paving `paved`, its roads tinted `road`: the snow it banks up, and
/// its rock's color.
pub(in crate::map) fn green_or_white(
    map: &mut Map,
    sk: &Skeleton,
    seed: u64,
    ground: Ground,
    paved: Vec3,
    road: Vec3,
) -> (Vec<Drift>, Vec3) {
    let mut rng = Rng::new(seed ^ 0x9A5E_0F1E);
    let land = ground.land();
    let snowy = ground == Ground::Tundra;
    map.atmosphere = ground.atmosphere(&mut rng);
    // (No furnaces' smoke, and more to see.)
    map.atmosphere.fog_density *= 0.75;
    map.planet = Some((seed, ground.planet()));
    let wind = map.atmosphere.wind.normalize_or(Vec2::X);
    let drifts = match snowy {
        true => banked(sk, wind, &mut rng, (0.4, 1.2)),
        false => Vec::new(),
    };
    let n = |p: Vec2, s: f32, k: u32| noise2(p.x / s, p.y / s, k);
    let tall = if snowy { 26.0 } else { 13.0 };
    // Past the edge it starts a little under the arena's own ground, so the
    // two don't fight where they meet.
    let lip = |p: Vec2| {
        let off = (-p).max(p - Vec2::splat(sk.size)).max_element();
        0.15 * (1.0 - smoothstep(0.0, 8.0, off))
    };
    let hills = |p: Vec2| {
        let k = smoothstep(150.0, 330.0, sk.out(p));
        // (A river runs on out level.)
        let dry = !sk.land.channel.as_ref().is_some_and(|c| c.wet(p, 30.0));
        match k > 0.0 && dry {
            true => k * tall * (0.75 * n(p, 110.0, 31) + 0.25 * n(p, 47.0, 32)),
            false => 0.0,
        }
    };
    for (t, inside) in [(&mut map.terrain, true), (&mut map.scenery_terrain, false)] {
        let Some(t) = t else { continue };
        t.color = Vec3::ONE;
        t.material = land.material;
        for iz in 0..t.nz {
            for ix in 0..t.nx {
                let k = iz * t.nx + ix;
                let p = t.origin + Vec2::new(ix as f32, iz as f32) * t.cell;
                let bank = if inside { bank(&drifts, p) } else { 0.0 };
                t.heights[k] += if inside { bank } else { hills(p) - lip(p) };
                let Some(tint) = t.tint.get_mut(k) else {
                    continue;
                };
                let patch = n(p, 13.0, 33);
                // Snow lies thick or not at all; grass wears through to mud.
                let bare = match snowy {
                    true => smoothstep(0.52, 0.75, patch),
                    false => smoothstep(0.55, 0.85, patch) * 0.8,
                };
                let open = land.open.lerp(land.low, bare);
                // How much of what's round it lies over the road.
                let over = match snowy {
                    true => smoothstep(0.5, 0.95, n(p, 17.0, 34)) * 0.4,
                    false => smoothstep(0.7, 1.0, n(p, 9.0, 34)) * 0.6,
                };
                let over = over.max(smoothstep(0.1, 0.5, bank));
                let r = sk.road(p);
                let kerb = 1.0 + 0.7 * r * (1.0 - r);
                let c = open.lerp(paved * road * kerb, r * (1.0 - over));
                *tint = c * (0.92 + 0.16 * n(p, 7.0, 35));
            }
        }
    }

    // Stone frosted, or mossed over.
    let (stain, share) = match snowy {
        true => (Vec3::new(0.66, 0.7, 0.76), 0.16),
        false => (Vec3::new(0.2, 0.25, 0.12), 0.1),
    };
    for o in &mut map.oboxes {
        let share = match o.side_mat {
            Material::Rubble => share * 2.0,
            Material::Concrete | Material::Plaster | Material::Wall => share,
            _ => continue,
        };
        let light = o.color.element_sum() / 3.0;
        let c = stain * (0.45 + light * 0.9);
        o.color = o.color.lerp(c, share);
    }

    if snowy {
        foundry::snow(map, sk, land.open, |_| true);
        map.rock_dust = Some(land.open);
    }
    // Nothing grows through paving, wrecks or the river.
    let clear = |map: &Map, p: Vec2| {
        sk.land.platforms.iter().all(|l| l.sdf(p) > l.ramp)
            && sk.squares.iter().all(|q| q.center.distance(p) > q.radius)
            && sk.crossings.iter().all(|c| c.distance(p) > c.half)
            && !sk.land.channel.as_ref().is_some_and(|c| c.wet(p, 2.0))
            && !map.oboxes.iter().any(|o| o.within(p, 0.8))
    };
    for _ in 0..3 {
        foundry::growth(map, sk, &land, ground, &mut rng, &clear);
    }

    let mid = Vec2::splat(sk.size * 0.5);
    let out = |rng: &mut Rng, lo: f32, hi: f32| {
        let dir = Vec2::from_angle(rng.range(0.0, std::f32::consts::TAU));
        let edge = sk.size * 0.5 / dir.abs().max_element();
        mid + dir * (edge + rng.range(lo, hi))
    };
    if !snowy {
        // Copses past the houses, and trees among those nearer.
        for _ in 0..150 {
            let c = out(&mut rng, 10.0, 330.0);
            for _ in 0..rng.int(2, 6) {
                let p = c + Vec2::new(rng.range(-9.0, 9.0), rng.range(-9.0, 9.0));
                let taken = sk.out(p) < 8.0
                    || sk.road(p) > 0.0
                    || sk.land.channel.as_ref().is_some_and(|c| c.wet(p, 6.0))
                    || sk.lots.iter().any(|l| l.footprint.distance(p) < 3.5)
                    // (Other trees don't count.)
                    || map.oboxes.iter().any(|o| o.rock.is_none() && o.within(p, 2.5));
                if !taken {
                    foundry::tree(map, &mut rng, p, sk.ground(p) + hills(p));
                }
            }
        }
        return (drifts, land.rock);
    }
    for _ in 0..30 {
        let w = rng.range(16.0, 44.0);
        let p = out(&mut rng, 280.0 + w, 440.0);
        if sk.land.channel.as_ref().is_some_and(|c| c.wet(p, w + 20.0)) {
            continue;
        }
        let g = sk.ground(p) + hills(p);
        let half = Vec2::new(w, w * rng.range(0.5, 0.9));
        let (yaw, high) = (rng.range(0.0, 6.28), rng.range(24.0, 62.0));
        let color = land.rock * rng.range(0.85, 1.05);
        map.oboxes.push(crate::map::OBox {
            solid: false,
            broken: true,
            ..rocks::boulder(p, yaw, half, g - 10.0 - w * 0.5, g + high, color)
        });
    }
    (drifts, land.rock)
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
    let drifts = banked(sk, wind, &mut rng, (0.5, 1.5));
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
                let bank = if inside { bank(&drifts, p) } else { 0.0 };
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
