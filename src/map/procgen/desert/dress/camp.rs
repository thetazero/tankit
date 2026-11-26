//! A shield generator out in the desert (#337): the holder's standard
//! model (see `shield`), whoever's it is, on a graded pad in the
//! landmark's slot, and round it what keeps one going out here:
//!
//! - **The pad**: the ground graded flat out past the precinct and packed
//!   hard, paler than the sand round it.
//! - **Tracks** beaten out from its doors toward each side's spawns.
//! - **Sand fences** across the windward approach, slatted panels between
//!   posts, sand banked up against them, some panels gone.
//! - **Supply dumps** by the doors: crates stacked on pallets in the
//!   holder's colours, fuel drums, a tarp over some of it.
//!
//! The generator's ring is laid out here (`site`) as a city's landmark is:
//! its doors toward each side's spawns (and more, so none is a long way
//! round), a side that may have been shelled.

use std::f32::consts::{PI, TAU};

use glam::{Vec2, Vec3};

use super::super::skeleton::*;
use super::Sand;
use crate::faction::Faction;
use crate::map::procgen::mine::{line_distance, seg, smoothstep};
use crate::map::procgen::shield::{self, Condition};
use crate::map::procgen::skeleton as city;
use crate::map::ruins::Drift;
use crate::map::valley::noise2;
use crate::map::{OBox, rocks};
use crate::render::material::Material;
use crate::rng::Rng;

/// The generator's ring on `sk`'s slot, shaped as a city's landmark (what
/// the models are built to): doors toward each side's spawns and more in
/// any wide gap between them, perhaps a side shelled.
pub fn site(sk: &Skeleton, seed: u64) -> city::Landmark {
    let lm = &sk.landmarks[0];
    let mut rng = Rng::new(seed ^ 0xD00B_5EED);
    let mut doors: Vec<f32> = Vec::new();
    for &b in &sk.base_spots() {
        let a = shield::angle(b - lm.center);
        if doors.iter().all(|&d| shield::apart(a, d) > 0.6) {
            doors.push(a);
        }
    }
    if doors.is_empty() {
        doors.push(rng.range(0.0, TAU));
    }
    // A door in the middle of every gap over a third of the way round,
    // and at least four in all.
    loop {
        doors.sort_by(|a, b| a.rem_euclid(TAU).total_cmp(&b.rem_euclid(TAU)));
        let gaps: Vec<(f32, f32)> = (0..doors.len())
            .map(|k| {
                let a0 = doors[k].rem_euclid(TAU);
                let mut a1 = doors[(k + 1) % doors.len()].rem_euclid(TAU);
                if a1 <= a0 {
                    a1 += TAU;
                }
                (a1 - a0, a0)
            })
            .collect();
        let &(gap, from) = gaps.iter().max_by(|a, b| a.0.total_cmp(&b.0)).unwrap();
        if doors.len() >= 4 && gap < TAU / 3.0 + 0.01 {
            break;
        }
        doors.push(from + gap * 0.5);
    }
    let doors: Vec<Vec2> = doors.into_iter().map(shield::dir).collect();
    let fallen = rng.chance(0.5).then(|| {
        let k = rng.index(doors.len());
        (doors[k] + doors[(k + 1) % doors.len()]).normalize_or(doors[k])
    });
    city::Landmark {
        center: lm.center,
        radius: GENERATOR_RING,
        precinct: GENERATOR_PRECINCT,
        doors,
        fallen,
        ..lm.clone()
    }
}

/// What the camp does to the ground: heaps banked up (rubble where it was
/// shelled, sand against the fences), the pad and the tracks painted.
pub struct Camp {
    at: Vec2,
    tracks: Vec<Vec<Vec2>>,
    fences: Vec<(Vec2, Vec2)>,
    /// Upwind: the side of a fence the sand banks up on.
    upwind: Vec2,
    drifts: Vec<Drift>,
}

/// How high sand banks up against a fence.
const BANK: f32 = 0.7;
/// A track's half-width.
const TRACK: f32 = 2.2;

impl Camp {
    /// How much is heaped on the ground at `p`.
    pub fn heap(&self, p: Vec2) -> f32 {
        let rubble = self
            .drifts
            .iter()
            .map(|d| d.height_at(p))
            .fold(0.0, f32::max);
        let sand = self
            .fences
            .iter()
            .map(|&(a, b)| {
                let (d, t) = seg(p, a, b);
                // Banked up on the windward side, a little over it.
                let side = (p - a).dot(self.upwind);
                let run = if side > 0.0 { 4.5 } else { 1.5 };
                let ends = smoothstep(0.0, 0.12, t) * smoothstep(0.0, 0.12, 1.0 - t);
                BANK * (1.0 - smoothstep(0.0, run, d)) * (0.4 + 0.6 * ends)
            })
            .fold(0.0, f32::max);
        rubble.max(sand)
    }

    /// The ground's colour `c` at `p` with the camp on it: the pad packed
    /// pale, the tracks dark.
    pub fn paint(&self, p: Vec2, c: Vec3, pal: &super::super::Palette) -> Vec3 {
        let d = self.at.distance(p);
        // Graded down to the hard ground under the sand: gravel, darker
        // and greyer than the loose sand round it.
        let packed = pal.low.lerp(pal.stone, 0.5) * 0.82;
        let packed = packed * (0.95 + 0.06 * noise2(p.x / 3.0, p.y / 3.0, 140));
        let pad = 1.0 - smoothstep(GENERATOR_PRECINCT - 2.0, GENERATOR_PRECINCT + 7.0, d);
        let mut c = c.lerp(packed, pad * 0.85);
        let worn = pal.stone_dark * 0.85;
        for t in &self.tracks {
            let off = line_distance(t, p);
            let k = (1.0 - smoothstep(TRACK * 0.5, TRACK + 1.2, off))
                * (0.75 + 0.25 * noise2(p.x / 5.0, p.y / 5.0, 141));
            c = c.lerp(worn, k * 0.75);
        }
        c
    }
}

/// The holder's colours on what it ships out here: (crates, their bands,
/// drums).
fn colours(owner: Faction) -> (Vec3, Vec3, Vec3) {
    match owner {
        // Olive drab with crimson bands; grey drums.
        Faction::Imperium | Faction::Company => (
            Vec3::new(0.3, 0.32, 0.22),
            Vec3::new(0.5, 0.08, 0.07),
            Vec3::new(0.32, 0.33, 0.34),
        ),
        // Ivory with blue bands; brass drums.
        Faction::Resonant => (
            Vec3::new(0.72, 0.68, 0.58),
            Vec3::new(0.16, 0.3, 0.6),
            Vec3::new(0.55, 0.42, 0.2),
        ),
        // Tungsten grey with green bands; dark drums.
        Faction::Uthenn => (
            Vec3::new(0.36, 0.37, 0.38),
            Vec3::new(0.15, 0.55, 0.2),
            Vec3::new(0.2, 0.21, 0.22),
        ),
        // Near-black glass with ember bands; dark bronze drums.
        Faction::Ithessar => (
            Vec3::new(0.12, 0.11, 0.13),
            Vec3::new(0.75, 0.32, 0.08),
            Vec3::new(0.28, 0.2, 0.14),
        ),
    }
}

impl Sand<'_> {
    /// The generator `owner` holds, in the shape it's in, and its camp.
    /// Rebuilds the ground under it with the camp's heaps and paint.
    pub(super) fn generator(
        &mut self,
        rng: &mut Rng,
        seed: u64,
        owner: Faction,
        condition: Condition,
    ) {
        let sk = self.sk;
        let lm = &sk.landmarks[0];
        let site = site(sk, seed);
        let base = sk.ground(lm.center);
        let mark = shield::Mark::of(self.map);
        let drifts = shield::build_at(self.map, &site, base, rng, owner);
        shield::ruin(self.map, rng, &site, base, &mark, owner, condition);

        let camp = self.lay_out_camp(rng, &site, drifts);
        self.map.terrain = Some(super::terrain_of(sk, self.pal, Some(&camp)));
        let mut ground = sk.land.surface(sk.size, 1.0, |p| camp.heap(p));
        ground.rock = self.pal.stone;
        self.map.solid = Some(ground.into());
        let fences = camp.fences.clone();
        let upwind = camp.upwind;
        for (a, b) in fences {
            self.sand_fence(rng, a, b, upwind);
        }
        self.supply_dumps(rng, &site, owner);
    }

    /// Where the tracks run and the fences stand.
    fn lay_out_camp(&mut self, rng: &mut Rng, site: &city::Landmark, drifts: Vec<Drift>) -> Camp {
        let sk = self.sk;
        let lm = &sk.landmarks[0];
        // A track out of each door that faces a side's spawns, out to them
        // (as far as the floor goes: none is beaten up a cliff).
        let mut tracks = Vec::new();
        for &b in &sk.base_spots() {
            let d = (b - lm.center).normalize_or(Vec2::X);
            let door = site
                .doors
                .iter()
                .copied()
                .max_by(|x, y| x.dot(d).total_cmp(&y.dot(d)))
                .unwrap_or(d);
            let from = lm.center + door * (GENERATOR_RING - 3.0);
            let len = from.distance(b);
            let bend = door.perp() * rng.range(-12.0, 12.0);
            let level = sk.land.on_high(from);
            let mut line = Vec::new();
            let n = (len / 6.0).ceil().max(2.0) as usize;
            for k in 0..=n {
                let t = k as f32 / n as f32;
                // Out of the door straight, then curving round to the spawns.
                let straight = from + door * (len * t);
                let p =
                    straight.lerp(from.lerp(b, t), smoothstep(0.1, 0.6, t)) + bend * (t * PI).sin();
                if sk.land.on_high(p) != level
                    || p.min_element() < 4.0
                    || p.max_element() > sk.size - 4.0
                {
                    break;
                }
                line.push(p);
            }
            if line.len() >= 2 {
                tracks.push(line);
            }
        }

        // Fences across the windward approach, between the doors, out past
        // the precinct.
        let upwind = -sk.dunes().wind.normalize_or(Vec2::X);
        let mut fences = Vec::new();
        let gaps = shield::between_doors(site);
        for a in gaps {
            let out = shield::dir(a);
            if out.dot(upwind) < 0.2 {
                continue;
            }
            for row in 0..rng.int(1, 2) {
                let r = lm.clear + 3.0 + row as f32 * 6.0 + rng.range(0.0, 2.0);
                let mid = lm.center + out * r;
                // Square across the wind, as long as fits between the
                // doors' ways in.
                let across = upwind.perp();
                let half = rng.range(7.0, 11.0);
                let (a0, a1) = (mid - across * half, mid + across * half);
                let clear_of_doors = (0..=8).all(|k| {
                    let q = a0.lerp(a1, k as f32 / 8.0);
                    let qa = shield::angle(q - lm.center);
                    site.doors
                        .iter()
                        .all(|&d| shield::apart(qa, shield::angle(d)) > 0.35)
                });
                let on_floor = (0..=4).all(|k| {
                    let q = a0.lerp(a1, k as f32 / 4.0);
                    !sk.land.on_high(q)
                        && sk.land.plateau(q) > FOOT + 8.0
                        && q.min_element() > 6.0
                        && q.max_element() < sk.size - 6.0
                        && tracks.iter().all(|t| line_distance(t, q) > TRACK + 3.0)
                });
                if clear_of_doors && on_floor {
                    fences.push((a0, a1));
                }
            }
        }
        Camp {
            at: lm.center,
            tracks,
            fences,
            upwind,
            drifts,
        }
    }

    /// A sand fence from `a` to `b`: posts, slatted panels between them
    /// (some gone, some leaning), half buried in the sand banked against
    /// it. Nothing a tank can't push through.
    fn sand_fence(&mut self, rng: &mut Rng, a: Vec2, b: Vec2, upwind: Vec2) {
        let len = a.distance(b);
        let along = (b - a) / len;
        let yaw = -shield::angle(along);
        let n = (len / 2.6).round().max(1.0) as usize;
        let wood = Vec3::new(0.42, 0.33, 0.23);
        for k in 0..=n {
            let p = a.lerp(b, k as f32 / n as f32);
            let g = self.g(p);
            self.map.block(
                p,
                Vec2::splat(0.07),
                yaw,
                (g - 0.6, g + rng.range(1.2, 1.5)),
                wood * rng.range(0.8, 1.0),
                Material::Wood,
                false,
            );
            if k == n || rng.chance(0.15) {
                continue;
            }
            // The panel: slats wired together, leaning a little downwind,
            // the sand banked against it come up its foot; a slat or two
            // gone.
            let lean = rng.range(-0.12, 0.05);
            let span = len / n as f32;
            let slats = (span / 0.3).floor() as usize;
            let top = rng.range(0.95, 1.15);
            for j in 0..slats {
                if rng.chance(0.08) {
                    continue;
                }
                let t = (k as f32 + (j as f32 + 0.5) / slats as f32) / n as f32;
                let q = a.lerp(b, t) - upwind * 0.05;
                let gq = self.g(q);
                let o = self.map.block(
                    q,
                    Vec2::new(0.055, 0.015),
                    yaw,
                    (gq - 0.3, gq + top * rng.range(0.96, 1.03)),
                    wood * rng.range(0.8, 1.05),
                    Material::Wood,
                    false,
                );
                o.tilt = lean;
            }
            for wire in [0.25f32, 0.8] {
                let q = a.lerp(b, (k as f32 + 0.5) / n as f32) - upwind * 0.07;
                let gq = self.g(q);
                let y = gq + top * wire;
                self.map.block(
                    q,
                    Vec2::new(span * 0.5, 0.01),
                    yaw,
                    (y, y + 0.025),
                    Vec3::splat(0.25),
                    Material::Metal,
                    false,
                );
            }
        }
    }

    /// A dump of supplies by a door or two: crates on pallets in the
    /// holder's colours, a drum stand, a tarp over a stack.
    fn supply_dumps(&mut self, rng: &mut Rng, site: &city::Landmark, owner: Faction) {
        let lm = &self.sk.landmarks[0];
        let (crate_c, band, drum) = colours(owner);
        let mut made = 0;
        let mut doors = site.doors.clone();
        for i in (1..doors.len()).rev() {
            doors.swap(i, rng.index(i + 1));
        }
        for d in doors {
            if made >= 3 {
                break;
            }
            let side = if rng.chance(0.5) { 1.0 } else { -1.0 };
            let a = shield::angle(d) + side * rng.range(0.32, 0.45);
            let at = lm.center + shield::dir(a) * rng.range(lm.clear + 3.5, lm.clear + 5.0);
            if !self.dump_room(at, 5.5) {
                continue;
            }
            self.dump(rng, at, shield::dir(a), crate_c, band, drum);
            made += 1;
        }
    }

    /// Room for a dump `r` round at `p`: on the floor, flat, nothing there.
    fn dump_room(&self, p: Vec2, r: f32) -> bool {
        let sk = self.sk;
        if p.min_element() < r + 4.0 || p.max_element() > sk.size - r - 4.0 {
            return false;
        }
        let g = self.g(p);
        let flat = (0..8).all(|k| {
            let q = p + Vec2::from_angle(k as f32 * TAU / 8.0) * r;
            !sk.land.on_high(q) && sk.land.plateau(q) > FOOT + 8.0 && (self.g(q) - g).abs() < 0.8
        });
        flat && !self.map.oboxes.iter().any(|o| {
            let (lo, hi) = o.y_range();
            hi > g + 0.1 && lo < g + 4.0 && o.distance(p) < r
        })
    }

    /// One dump at `p`, facing `out` from the generator.
    fn dump(&mut self, rng: &mut Rng, p: Vec2, out: Vec2, crate_c: Vec3, band: Vec3, drum: Vec3) {
        let across = out.perp();
        let yaw = -shield::angle(out);
        // Two or three pallets of crates in a row across the way out.
        let stacks = rng.int(2, 3);
        for k in 0..stacks {
            let c = p
                + across * ((k as f32 - (stacks - 1) as f32 * 0.5) * 2.9)
                + out * rng.range(-0.3, 0.3);
            let g = self.g(c);
            let skew = rng.range(-0.08, 0.08);
            self.map.block(
                c,
                Vec2::splat(1.2),
                yaw + skew,
                (g - 0.2, g + 0.16),
                Vec3::new(0.4, 0.31, 0.21),
                Material::Wood,
                false,
            );
            // Crates two by two on it, a layer or two, one collider
            // round the lot.
            let mut y = g + 0.16;
            let layers = rng.int(1, 2);
            for layer in 0..layers {
                let h = rng.range(0.8, 0.95);
                for (u, v) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                    if layer > 0 && rng.chance(0.3) {
                        continue;
                    }
                    let q = c + (across * u + out * v).rotate(Vec2::from_angle(skew)) * 0.56;
                    let half = Vec2::splat(rng.range(0.48, 0.52));
                    let turn = yaw + skew + rng.range(-0.06, 0.06);
                    let hh = h * rng.range(0.94, 1.0);
                    self.map.block(
                        q,
                        half,
                        turn,
                        (y, y + hh),
                        crate_c * rng.range(0.88, 1.05),
                        Material::Painted,
                        false,
                    );
                    // A band round it, standing just proud.
                    self.map.block(
                        q,
                        half + Vec2::splat(0.025),
                        turn,
                        (y + hh * 0.62, y + hh * 0.74),
                        band,
                        Material::Paint,
                        false,
                    );
                }
                y += h;
            }
            self.map.oboxes.push(OBox {
                center: c,
                yaw: yaw + skew,
                half: Vec2::splat(1.08),
                y0: g,
                y1: g + 0.16 + 0.9 * layers as f32,
                solid: true,
                hidden: true,
                ..OBox::default()
            });
            // A tarp over the end stack, sagging over its corners.
            if k == 0 && rng.chance(0.6) {
                let o = self.map.block(
                    c,
                    Vec2::splat(1.24),
                    yaw + skew,
                    (y - 0.55, y + 0.06),
                    Vec3::new(0.44, 0.4, 0.3) * rng.range(0.85, 1.0),
                    Material::Cloth,
                    false,
                );
                o.tilt = rng.range(-0.06, 0.06);
            }
        }
        // Fuel drums, standing in a cluster to one side, one on its side.
        let side = if rng.chance(0.5) { 1.0 } else { -1.0 };
        let c = p + across * side * (stacks as f32 * 1.45 + 2.2) - out * 0.6;
        for k in 0..rng.int(4, 6) {
            let q = c
                + Vec2::new(((k % 3) as f32 - 1.0) * 0.72, (k / 3) as f32 * 0.72).rotate(out)
                + Vec2::new(rng.range(-0.06, 0.06), rng.range(-0.06, 0.06));
            let g = self.g(q);
            self.drum(q, g, 0.3, 0.92, drum * rng.range(0.85, 1.05));
        }
        let q = c - out * 1.8;
        let g = self.g(q);
        let o = self.map.block(
            q,
            Vec2::new(0.46, 0.3),
            yaw + rng.range(-0.6, 0.6),
            (g - 0.02, g + 0.58),
            drum * 0.8,
            Material::Metal,
            false,
        );
        o.roll = PI / 8.0;
        // Spilled sand round the lot, a few stones.
        for _ in 0..4 {
            let q = p + Vec2::from_angle(rng.range(0.0, TAU)) * rng.range(4.5, 6.5);
            let gq = self.g(q);
            let s = rng.range(0.25, 0.45);
            let color = self.pal.stone * rng.range(0.9, 1.05);
            self.map.oboxes.push(OBox {
                solid: false,
                ..rocks::boulder(
                    q,
                    rng.range(0.0, TAU),
                    Vec2::new(s, s * 0.8),
                    gq - s * 0.4,
                    gq + s,
                    color,
                )
            });
        }
    }

    /// A drum standing at `p`: three boxes turned to round it off, a rim.
    fn drum(&mut self, p: Vec2, g: f32, r: f32, h: f32, color: Vec3) {
        for k in 0..3 {
            let yaw = k as f32 * PI / 6.0;
            let half = Vec2::new(r, r * 0.42) * (1.0 - 0.004 * k as f32);
            self.map.block(
                p,
                half,
                yaw,
                (g, g + h - 0.003 * k as f32),
                color,
                Material::Metal,
                k == 0,
            );
        }
        self.map.block(
            p,
            Vec2::new(r * 1.03, r * 0.43),
            PI / 4.0,
            (g + h * 0.48, g + h * 0.54),
            color * 0.8,
            Material::Metal,
            false,
        );
    }
}
