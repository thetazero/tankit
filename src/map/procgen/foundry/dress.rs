//! Dressing a works: the ground stained, the parts put up in the style, the
//! roads' crossings, the rails; each side's pad; what the land's put there
//! itself; what's seen past the edge. Then the places bots head for, repair
//! drops, the menus' vistas.

use std::collections::HashMap;
use std::f32::consts::TAU;

use glam::{IVec2, Vec2, Vec3};

use super::ground::{Ground, Land};
use super::parts;
use super::style::Works;
use crate::ai::nav::NavGrid;
use crate::map::noise::noise2;
use crate::map::procgen::Look;
use crate::map::procgen::civic::Stone;
use crate::map::procgen::skeleton::*;
use crate::map::{Map, OBox, Terrain, rocks};
use crate::render::material::Material;
use crate::rng::Rng;

const TIMBER: Vec3 = Vec3::new(0.3, 0.23, 0.17);

pub fn dress(map: &mut Map, sk: &Skeleton, look: Look, ground: Ground, rng: &mut Rng) {
    let st = super::works(look);
    let land = ground.land();
    let pal = st.palette();
    map.terrain = Some(terrain(sk, &land, ground));
    let mut solid = sk.land.plain_surface(sk.size, |_| 0.0);
    solid.rock = land.rock;
    map.solid = Some(solid.into());
    let stone = Stone {
        wall: pal.wall,
        mat: pal.wall_mat,
        trim: pal.dark,
        roof: pal.roof,
        land: land.open,
    };
    for c in &sk.crossings {
        c.build(map, stone, Some(look), |p| sk.ground(p));
    }
    sk.lay_rails(map);
    for lot in &sk.lots {
        if let LotKind::Works(part) = lot.kind {
            let f = &lot.footprint;
            parts::build(map, f, part, sk.ground(f.center), Some(st));
        }
    }
    if ground == Ground::Tundra {
        snow(map, sk, land.open);
    }
    edges(map, sk, rng, pal.wall, pal.wall_mat);
    bases(map, sk, pal.floor, pal.dark);
    growth(map, sk, &land, ground, rng, &|_, _| true);
    beyond(map, sk, st, &land, ground, rng);
    map.separate_coplanar_faces();
    play(map, sk);
    map.vistas = crate::map::procgen::sunlit_views(map, &sk.base_spots(), sk.heart);
}

/// The ground: open land, black round the works, worn down the roads,
/// ballast under the rails, rock where it's steep.
fn terrain(sk: &Skeleton, land: &Land, ground: Ground) -> Terrain {
    let mut t = sk.terrain(|_| 0.0);
    t.color = Vec3::ONE;
    t.material = land.material;
    let big: Vec<OBox> = sk
        .lots
        .iter()
        .filter(|l| matches!(l.kind, LotKind::Works(p) if p.big()))
        .map(|l| l.footprint)
        .collect();
    let rails: Vec<&Street> = sk.streets.iter().filter(|s| s.rank == Rank::Rail).collect();
    let roads: Vec<&Street> = sk
        .streets
        .iter()
        .filter(|s| !matches!(s.rank, Rank::Rail | Rank::Crossing))
        .collect();
    t.paint(|x, z, _, slope| {
        let p = Vec2::new(x, z);
        let n = |s: f32, k: u32| noise2(x / s, z / s, k);
        let mottle = 0.9 + 0.16 * n(6.0, 210) + 0.06 * n(1.0, 211);
        let near = big.iter().map(|b| b.distance(p)).fold(f32::MAX, f32::min);
        let stain = 1.0 - smoothstep(6.0, 44.0, near + 16.0 * n(22.0, 212));
        let patch = n(13.0, 215);
        // Snow lies thick or not at all; grass wears through to mud.
        let bare = match ground {
            Ground::Desert => 0.5 * patch,
            Ground::Tundra => smoothstep(0.4, 0.62, patch + 0.25 * stain),
            Ground::Grassland => smoothstep(0.5, 0.75, patch + 0.3 * stain),
        };
        let mut c = land.open.lerp(land.low, bare);
        c = c.lerp(land.soot, 0.85 * stain);
        c = c.lerp(land.rock, smoothstep(0.35, 0.75, slope) * 0.85);
        let on = |list: &[&Street], soft: f32| {
            list.iter()
                .map(|s| 1.0 - smoothstep(s.half - 0.8, s.half + soft, s.distance(p)))
                .fold(0.0, f32::max)
        };
        let worn = land.low.lerp(land.ballast, 0.6);
        c = c.lerp(worn, on(&roads, 2.0) * (0.65 + 0.3 * n(4.0, 217)));
        // Into what's past the edge.
        let edge = p.min_element().min(sk.size - p.max_element());
        let out = 1.0 - smoothstep(0.0, 26.0, edge + 8.0 * n(9.0, 218));
        c = c.lerp(land.far, out);
        c = c.lerp(land.ballast, on(&rails, 1.2) * 0.9);
        c * (1.0 + (mottle - 1.0) * (1.0 - out))
    });
    t
}

/// Snow lying on every roof that's open to the sky, in places blown or
/// melted off. Nothing solid.
pub(in crate::map) fn snow(map: &mut Map, sk: &Skeleton, color: Vec3) {
    const DEEP: f32 = 0.14;
    // What's over each patch of the map (a city's too big to look through
    // it all for every roof).
    const CELL: f32 = 8.0;
    let cell = |p: Vec2| (p / CELL).floor().as_ivec2();
    let mut over: HashMap<IVec2, Vec<usize>> = HashMap::new();
    for (k, o) in map.oboxes.iter().enumerate() {
        let r = o.half.extend((o.y1 - o.y0) * 0.5).length();
        let (a, b) = (cell(o.center - r), cell(o.center + r));
        for x in a.x..=b.x {
            for z in a.y..=b.y {
                over.entry(IVec2::new(x, z)).or_default().push(k);
            }
        }
    }
    let mut lying = Vec::new();
    for o in &map.oboxes {
        let turn = o.rotation();
        let up = turn * Vec3::Y;
        let roof = o.half.min_element() > 0.9
            && o.y1 - sk.ground(o.center) > 2.2
            && up.y > 0.8
            && !o.hidden
            && !o.ramp
            && o.rock.is_none();
        if !roof || noise2(o.center.x / 5.0, o.center.y / 5.0, 230) < 0.22 {
            continue;
        }
        let top = o.center3() + up * (o.y1 - o.y0) * 0.5;
        let open = |x: f32, z: f32| {
            let from = top + up * DEEP * 2.0 + turn * Vec3::new(x, 0.0, z);
            let near = over.get(&cell(Vec2::new(from.x, from.z)));
            !near
                .into_iter()
                .flatten()
                .any(|&k| map.oboxes[k].ray_hit(from, Vec3::Y, 80.0))
        };
        // (A stepped roof's course shows only at its edges.)
        let (x, z) = (o.half.x - 0.15, o.half.y - 0.15);
        let bare =
            open(0.0, 0.0) || (open(x, 0.0) && open(-x, 0.0)) || (open(0.0, z) && open(0.0, -z));
        if !bare {
            continue;
        }
        let c = top + up * (DEEP * 0.5 - 0.02);
        lying.push(OBox {
            center: Vec2::new(c.x, c.z),
            half: o.half - Vec2::splat(0.06),
            y0: c.y - DEEP * 0.5,
            y1: c.y + DEEP * 0.5,
            color,
            side_mat: Material::Rubble,
            top_mat: Material::Rubble,
            solid: false,
            chip: 0.0,
            broken: false,
            ..*o
        });
    }
    map.oboxes.extend(lying);
}

/// What lies across where the roads leave.
fn edges(map: &mut Map, sk: &Skeleton, rng: &mut Rng, wall: Vec3, mat: Material) {
    for e in &sk.edges {
        let mut t = e.from;
        while t < e.to {
            let l = rng.range(1.4, 2.6);
            let off = match e.kind {
                EdgeKind::Barricade => rng.range(-4.0, 0.5),
                EdgeKind::Blockade => rng.range(-5.0, -0.5),
            };
            let p = e.start + e.along * (t + l) + e.inward * off;
            let h = match e.kind {
                EdgeKind::Barricade => rng.range(1.0, 1.8),
                EdgeKind::Blockade => rng.range(2.6, 4.6),
            };
            let g = sk.ground(p);
            let yaw = e.along.x.atan2(e.along.y) + rng.range(-0.3, 0.3);
            map.oboxes.push(OBox {
                center: p,
                yaw,
                half: Vec2::new(rng.range(0.5, 1.1), l),
                y0: g - 1.5,
                y1: g + h,
                color: wall * rng.range(0.75, 0.95),
                side_mat: mat,
                top_mat: mat,
                tilt: rng.range(-0.1, 0.1),
                broken: true,
                ..OBox::default()
            });
            // (No track's blocked: wagons have to get out.)
            let o = map.oboxes.last().unwrap();
            let on_rails = sk
                .streets
                .iter()
                .any(|s| s.rank == Rank::Rail && s.distance(p) < s.half + o.half.length());
            if on_rails {
                map.oboxes.pop();
            }
            t += l * 2.0 + rng.range(0.1, 0.6);
        }
    }
}

/// Each side's ground: its pad, its stores at the back, its colours flying.
fn bases(map: &mut Map, sk: &Skeleton, floor: Vec3, dark: Vec3) {
    for (k, b) in sk.bases.iter().enumerate() {
        let Some(&who) = map.factions.get(k) else {
            continue;
        };
        let c = b.middle();
        let g = sk.ground(c);
        let face = (sk.heart - c).normalize_or(Vec2::X);
        let yaw = face.x.atan2(face.y);
        let (ax, az) = (Vec2::new(face.y, -face.x), face);
        for i in -2..=2 {
            for j in -2..=2 {
                let p = c + ax * i as f32 * 6.0 + az * j as f32 * 6.0;
                let shade = 0.86 + 0.04 * ((i * 3 + j * 5) % 3) as f32;
                map.block(
                    p,
                    Vec2::splat(2.92),
                    yaw,
                    (g - 0.3, g + 0.02),
                    floor * shade,
                    Material::Concrete,
                    false,
                );
            }
        }
        for (x, z) in [(-7.0, -21.5), (0.0, -22.5), (7.0, -21.5)] {
            let p = c + ax * x + az * z;
            if p.min_element() < 3.0 || p.max_element() > sk.size - 3.0 {
                continue;
            }
            let h = sk.ground(p);
            map.block(
                p,
                Vec2::new(2.8, 1.2),
                yaw,
                (h - 1.0, h + 2.4),
                who.paint() * 0.8 + Vec3::splat(0.05),
                Material::Painted,
                true,
            );
        }
        for e in [-1.0f32, 1.0] {
            let p = c + ax * e * 15.0 - az * 13.5;
            let h = sk.ground(p);
            let top = Vec3::new(p.x, h + 11.0, p.y);
            let foot = Vec3::new(p.x, h - 0.5, p.y);
            map.pipe(foot, top, 0.16, dark, Material::Painted, true);
            map.side_banner(
                top + Vec3::new(face.x, -3.0, face.y) * 0.1,
                face,
                2.4,
                6.0,
                who.paint(),
                who == crate::faction::Faction::Imperium,
            );
        }
    }
}

/// What the land's put there itself: stones, drifted snow, tufts. Nothing
/// that stops a tank, and only where it's `open`.
pub(in crate::map) fn growth(
    map: &mut Map,
    sk: &Skeleton,
    land: &Land,
    ground: Ground,
    rng: &mut Rng,
    open: &dyn Fn(&Map, Vec2) -> bool,
) {
    let n = match ground {
        Ground::Desert => 160,
        Ground::Tundra => 260,
        Ground::Grassland => 1100,
    };
    for _ in 0..n {
        let p = Vec2::new(rng.range(3.0, sk.size - 3.0), rng.range(3.0, sk.size - 3.0));
        let taken = sk.lots.iter().any(|l| l.footprint.distance(p) < 1.5)
            || sk.bases.iter().any(|b| b.contains(p, 0.0))
            || sk.streets.iter().any(|s| s.distance(p) < s.half + 0.5)
            || !open(map, p);
        if taken {
            continue;
        }
        let g = sk.ground(p);
        let yaw = rng.range(0.0, TAU);
        let put = |map: &mut Map, o: OBox| map.oboxes.push(OBox { solid: false, ..o });
        match ground {
            Ground::Desert => {
                let r = rng.range(0.2, 0.6);
                let h = Vec2::splat(r);
                let o = rocks::boulder(p, yaw, h, g - r * 0.5, g + r * 0.7, land.rock);
                put(map, o);
            }
            Ground::Tundra => {
                let half = Vec2::new(rng.range(1.0, 3.0), rng.range(0.7, 1.5));
                let top = g + rng.range(0.12, 0.35);
                let o = rocks::boulder(p, yaw, half, g - 0.8, top, land.open);
                put(map, o);
            }
            Ground::Grassland => {
                for k in 0..4 {
                    let lean = Vec2::from_angle(yaw + k as f32 * 1.7) * rng.range(0.1, 0.3);
                    let q = p + lean;
                    map.segment(
                        Vec3::new(p.x, g - 0.05, p.y),
                        Vec3::new(q.x, g + rng.range(0.3, 0.7), q.y),
                        0.035,
                        land.open * rng.range(0.8, 1.35),
                        Material::Grass,
                        false,
                    );
                }
            }
        }
    }
}

/// A tree past the edge: a trunk and a few rounds of leaf.
pub(in crate::map) fn tree(map: &mut Map, rng: &mut Rng, p: Vec2, g: f32) {
    let h = rng.range(4.0, 7.0);
    map.segment(
        Vec3::new(p.x, g - 1.0, p.y),
        Vec3::new(p.x, g + h, p.y),
        0.3,
        TIMBER,
        Material::Wood,
        false,
    );
    for _ in 0..4 {
        let r = rng.range(1.5, 2.7);
        let c = p + Vec2::from_angle(rng.range(0.0, TAU)) * rng.range(0.0, 1.6);
        let y = g + h * rng.range(0.75, 1.1);
        let color = Vec3::new(0.11, 0.18, 0.06) * rng.range(0.8, 1.4);
        let yaw = rng.range(0.0, TAU);
        let half = Vec2::new(r, r * 0.9);
        let o = rocks::boulder(c, yaw, half, y - r * 0.7, y + r * 0.8, color);
        map.oboxes.push(OBox { solid: false, ..o });
    }
}

/// Past the edge: the land going on, rising into hills; more works far off;
/// crags, copses.
fn beyond(
    map: &mut Map,
    sk: &Skeleton,
    st: &dyn Works,
    land: &Land,
    ground: Ground,
    rng: &mut Rng,
) {
    let s = sk.size;
    let cell = 5.0f32;
    let reach = 480.0;
    let n = ((s + 2.0 * reach) / cell).round() as usize + 1;
    let off = |p: Vec2| {
        let q = (p - Vec2::splat(s * 0.5)).abs() - Vec2::splat(s * 0.5);
        q.max(Vec2::ZERO).length() + q.x.max(q.y).min(0.0)
    };
    let tall = match ground {
        Ground::Desert => 1.0,
        Ground::Tundra => 1.4,
        Ground::Grassland => 0.8,
    };
    let height = |x: f32, z: f32| {
        let p = Vec2::new(x, z);
        let d = off(p);
        if d < -0.01 {
            return sk.ground(p) - 12.0;
        }
        let hills = 5.0 * ((x * 0.011).sin() * (z * 0.013).cos()).abs()
            + 4.0 * (1.0 + (x * 0.021 + z * 0.017).sin());
        let track = sk
            .tracks
            .iter()
            .flat_map(|l| l.windows(2))
            .map(|w| crate::map::procgen::mine::seg(p, w[0], w[1]).0)
            .fold(f32::MAX, f32::min);
        let level = smoothstep(8.0, 40.0, track).max(smoothstep(120.0, 200.0, d));
        // (A little under the arena's own ground where they meet.)
        sk.ground(p) - 0.15 * (1.0 - smoothstep(0.0, 8.0, d))
            + tall * hills * smoothstep(20.0, 120.0, d) * level
    };
    let mut t = Terrain::from_fn(Vec2::splat(-reach), cell, n, n, height);
    t.color = land.far;
    t.material = land.far_material;
    map.scenery_terrain = Some(t);
    let out = |rng: &mut Rng, lo: f32, hi: f32| {
        let dir = Vec2::from_angle(rng.range(0.0, TAU));
        let edge = s * 0.5 / dir.x.abs().max(dir.y.abs());
        Vec2::splat(s * 0.5) + dir * (edge + rng.range(lo, hi))
    };
    let on_track = |p: Vec2, r: f32| {
        sk.tracks
            .iter()
            .flat_map(|l| l.windows(2))
            .any(|w| crate::map::procgen::mine::seg(p, w[0], w[1]).0 < r)
    };
    // More of the works.
    let mut put = 0;
    for _ in 0..80 {
        if put >= 9 {
            break;
        }
        let c = out(rng, 46.0, 130.0);
        let kinds = [
            parts::Part::Mill,
            parts::Part::Power,
            parts::Part::Ore,
            parts::Part::Tanks,
            parts::Part::Office,
        ];
        let part = kinds[rng.index(kinds.len())];
        let half = part.half(rng);
        if on_track(c, half.length() + 5.0) || off(c) < half.length() + 14.0 {
            continue;
        }
        put += 1;
        let f = OBox {
            center: c,
            yaw: rng.range(0.0, TAU),
            half,
            ..OBox::default()
        };
        let low = f
            .corners()
            .iter()
            .map(|p| height(p.x, p.y))
            .fold(f32::MAX, f32::min);
        let mark = (map.oboxes.len(), map.lights.len(), map.sounds.len());
        parts::build(map, &f, part, low, Some(st));
        for o in &mut map.oboxes[mark.0..] {
            o.solid = false;
        }
        map.lights.truncate(mark.1);
        map.sounds.truncate(mark.2);
    }
    if ground == Ground::Grassland {
        for _ in 0..44 {
            let c = out(rng, 18.0, 150.0);
            for _ in 0..rng.int(2, 6) {
                let p = c + Vec2::new(rng.range(-9.0, 9.0), rng.range(-9.0, 9.0));
                if off(p) > 9.0 && !on_track(p, 5.0) {
                    tree(map, rng, p, height(p.x, p.y));
                }
            }
        }
        return;
    }
    for _ in 0..26 {
        let w = rng.range(12.0, 30.0);
        let c = out(rng, 130.0 + w, 300.0);
        let g = height(c.x, c.y) - 2.0;
        let yaw = rng.range(0.0, TAU);
        let half = Vec2::new(w, w * rng.range(0.5, 0.9));
        let color = land.rock * rng.range(0.85, 1.05);
        map.oboxes.push(OBox {
            solid: false,
            broken: true,
            ..rocks::boulder(c, yaw, half, g, g + rng.range(12.0, 36.0), color)
        });
    }
}

/// Spawns, the places bots head for, the repair drops that landed clear.
fn play(map: &mut Map, sk: &Skeleton) {
    sk.finish(map);
    let nav = NavGrid::build(map);
    let Some(from) = sk.bases.first().map(Base::middle) else {
        return;
    };
    map.open_spots
        .retain(|&p| !nav.is_blocked(p) && !nav.path(from, p).is_empty());
    let sky = |m: &Map, p: Vec2| {
        let at = Vec3::new(p.x, m.ground_height(p.x, p.y) + 0.5, p.y);
        !m.oboxes.iter().any(|o| o.ray_hit(at, Vec3::Y, 80.0))
    };
    let mut spots = std::mem::take(&mut map.repair_spots);
    spots.retain(|&p| sky(map, p) && !nav.path(from, p).is_empty());
    map.repair_spots = spots;
}
