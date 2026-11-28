//! Dressing a reservoir town: its houses, its dam or well, and what its
//! people left (stalls, dead palms, power poles down the street, boats on
//! the dry bed).

use std::f32::consts::TAU;

use glam::{Vec2, Vec3};

use super::*;
use crate::map::ruins;

/// Mud brick and limewash.
const HOUSES: ruins::Style = ruins::Style {
    colors: &[
        Vec3::new(0.72, 0.58, 0.42),
        Vec3::new(0.66, 0.5, 0.36),
        Vec3::new(0.8, 0.72, 0.58),
        Vec3::new(0.76, 0.64, 0.5),
        Vec3::new(0.6, 0.48, 0.38),
        Vec3::new(0.84, 0.78, 0.68),
    ],
    wall: Material::Plaster,
    storeys: (1, 2),
    light: 0.7,
};
const CONCRETE: Vec3 = Vec3::new(0.64, 0.6, 0.54);
const TIMBER: Vec3 = Vec3::new(0.42, 0.32, 0.22);

/// The ground's color `c` at `p`, with the town on it: streets packed pale,
/// the dry bed's cracked mud and the rings the water left going down, the
/// dam's stone facing.
pub(super) fn paint(sk: &Skeleton, p: Vec2, c: Vec3, pal: &Palette) -> Vec3 {
    let mut c = c;
    if let Some(l) = &sk.land.lake {
        let k = l.cut(p);
        let mud = pal.low.lerp(Vec3::splat(pal.low.element_sum() / 3.0), 0.5);
        let plates = 0.86 + 0.28 * noise2(p.x / 1.5, p.y / 1.5, 61);
        c = c.lerp(mud * 1.05 * plates, smoothstep(0.5, 0.95, k));
        let ring = (k * 24.0).sin().max(0.0) * smoothstep(0.04, 0.2, k) * (1.0 - k);
        c = c.lerp(pal.sand.lerp(Vec3::splat(0.86), 0.5), ring * 0.55);
    }
    if let Some(d) = &sk.land.dam {
        let k = smoothstep(0.3, 1.2, d.height_at(p));
        let stone = pal.stone.lerp(CONCRETE, 0.5) * (0.85 + 0.3 * noise2(p.x / 2.3, p.y / 2.3, 62));
        c = c.lerp(stone, k * 0.75);
    }
    let road = sk.road(p) * (0.5 + 0.5 * smoothstep(0.15, 0.5, noise2(p.x / 11.0, p.y / 11.0, 63)));
    c.lerp(pal.low * 0.62, road * 0.85)
}

impl Sand<'_> {
    /// The houses, built then ruined. Returns what's heaped on the ground
    /// by them.
    pub(super) fn houses(&mut self, rng: &mut Rng) -> Vec<Drift> {
        let sk = self.sk;
        let lots: Vec<Lot> = sk.houses().copied().collect();
        let mut drifts = Vec::new();
        for (k, lot) in lots.iter().enumerate() {
            let mark = Mark::of(self.map);
            if lot.kind == LotKind::Rubble {
                self.heap(rng, lot);
            } else {
                // Room for a fallen floor to come down in.
                let clear = |p: Vec2| {
                    p.min_element() > 8.0
                        && p.max_element() < sk.size - 8.0
                        && lots
                            .iter()
                            .enumerate()
                            .all(|(j, o)| j == k || o.footprint.distance(p) > 1.5)
                };
                drifts.extend(ruins::building_in(
                    self.map,
                    rng,
                    lot.footprint,
                    lot.front,
                    &clear,
                    &HOUSES,
                ));
            }
            mark.raise(self.map, sk.ground(lot.at()));
        }
        drifts
    }

    /// A house come down altogether: low heaps, a corner still standing.
    fn heap(&mut self, rng: &mut Rng, lot: &Lot) {
        let f = lot.footprint;
        let (ax, az) = f.axes();
        let color = HOUSES.colors[rng.index(HOUSES.colors.len())];
        let s = Vec2::new(
            if rng.chance(0.5) { 1.0 } else { -1.0 },
            if rng.chance(0.5) { 1.0 } else { -1.0 },
        );
        let corner = f.center + ax * (f.half.x - 0.3) * s.x + az * (f.half.y - 0.3) * s.y;
        for (along, across) in [(ax * -s.x, true), (az * -s.y, false)] {
            let len = rng.range(2.2, 3.6);
            let shift = if across { 0.0 } else { 0.5 };
            self.map.oboxes.push(OBox {
                center: corner + along * (len * 0.5 + shift),
                yaw: f.yaw,
                half: if across {
                    Vec2::new(len * 0.5, 0.28)
                } else {
                    Vec2::new(0.25, len * 0.5)
                },
                y0: -0.3,
                y1: rng.range(1.8, 3.4),
                color,
                side_mat: Material::Plaster,
                top_mat: Material::Plaster,
                broken: true,
                chip: 0.2,
                ..OBox::default()
            });
        }
        for _ in 0..rng.int(5, 9) {
            let p = f.center
                + ax * f.half.x * rng.range(-0.8, 0.8)
                + az * f.half.y * rng.range(-0.8, 0.8);
            let r = rng.range(0.5, 1.1);
            self.map.oboxes.push(OBox {
                solid: false,
                ..rocks::rubble(
                    p,
                    rng.range(0.0, TAU),
                    Vec2::new(r, r * rng.range(0.6, 1.0)),
                    -0.3,
                    r * rng.range(0.4, 0.7),
                    color * rng.range(0.85, 1.0),
                )
            });
        }
    }

    /// The dam: its crest's parapets, the concrete core standing out of it
    /// by the breach, blocks of it thrown down, the intake tower out on the
    /// bed behind.
    pub(super) fn dam(&mut self, rng: &mut Rng) {
        let sk = self.sk;
        let (Some(d), lm) = (sk.land.dam, &sk.landmarks[0]) else {
            return;
        };
        let (mid, along) = (d.middle(), d.along());
        let out = along.perp();
        let yaw = (-along.y).atan2(along.x);
        let half = d.a.distance(d.b) * 0.5;
        // A stretch of wall from `t0` to `t1` along, `off` across.
        let wall =
            |s: &mut Self, t0: f32, t1: f32, off: f32, thick: f32, top: (f32, f32), c: Vec3| {
                let p = mid + along * (t0 + t1) * 0.5 + out * off;
                let g = s.g(p);
                let low = [t0, t1]
                    .iter()
                    .map(|&t| s.g(mid + along * t + out * off))
                    .fold(g, f32::min);
                s.map.oboxes.push(OBox {
                    center: p,
                    yaw,
                    half: Vec2::new((t1 - t0).abs() * 0.5, thick),
                    y0: low - 0.6,
                    y1: g + top.0,
                    color: c,
                    side_mat: Material::Concrete,
                    top_mat: Material::Concrete,
                    chip: 0.25,
                    broken: top.1 > 0.0,
                    ..OBox::default()
                });
            };
        for side in [-1.0f32, 1.0] {
            // The core, broken off at the breach.
            let mut t = d.breach + 8.0;
            let mut h = 1.2;
            while t < d.breach + 20.0 {
                let len = rng.range(2.4, 3.6);
                let c = CONCRETE * rng.range(0.88, 1.04);
                wall(
                    self,
                    side * t,
                    side * (t + len - 0.06),
                    0.0,
                    0.9,
                    (h, 1.0),
                    c,
                );
                t += len;
                h += rng.range(0.2, 0.7);
            }
            // Parapets either side of the crest, stretches of them gone.
            for off in [-1.0f32, 1.0] {
                let mut t = d.breach + 17.0;
                while t < half - 2.0 {
                    let len = rng.range(3.0, 5.0);
                    if rng.chance(0.62) {
                        let c = CONCRETE * rng.range(0.85, 1.0);
                        let broken = if rng.chance(0.4) { 1.0 } else { 0.0 };
                        wall(
                            self,
                            side * t,
                            side * (t + len - 0.3),
                            off * (d.crest - 0.2),
                            0.28,
                            (rng.range(0.9, 1.25), broken),
                            c,
                        );
                    }
                    t += len;
                }
            }
            // Thrown down into the breach's sides.
            for _ in 0..4 {
                let p = mid
                    + along * side * rng.range(lm.clear + 3.2, d.breach + 9.0)
                    + out * rng.range(-10.0, 10.0);
                let s = rng.range(0.9, 1.7);
                let g = self.g(p);
                self.map.oboxes.push(OBox {
                    center: p,
                    yaw: rng.range(0.0, TAU),
                    half: Vec2::new(s, s * rng.range(0.5, 0.9)),
                    y0: g - 0.7,
                    y1: g + s * rng.range(0.7, 1.2),
                    tilt: rng.range(-0.35, 0.35),
                    roll: rng.range(-0.2, 0.2),
                    color: CONCRETE * rng.range(0.85, 1.0),
                    side_mat: Material::Concrete,
                    top_mat: Material::Concrete,
                    chip: 0.4,
                    ..OBox::default()
                });
            }
        }
        self.strew(rng);
        let Some(l) = sk.land.lake else { return };
        // The tower: a shaft on the bed, a cap over it, its water marks.
        let p = l.center.lerp(mid, 0.45);
        let g = self.g(p);
        let top = rng.range(9.0, 12.0);
        for (r, y0, y1, c) in [
            (2.6, -1.0, 1.1, CONCRETE * 0.8),
            (2.0, 0.6, l.depth + 0.4, CONCRETE * 0.78),
            (1.96, l.depth, top, CONCRETE),
            (2.7, top - 0.3, top + 0.5, CONCRETE * 0.92),
            (1.5, top + 0.2, top + 2.8, CONCRETE * 1.05),
            (1.9, top + 2.6, top + 3.0, CONCRETE * 0.8),
        ] {
            self.map.oboxes.push(OBox {
                center: p,
                yaw: yaw + 0.3,
                half: Vec2::splat(r),
                y0: g + y0,
                y1: g + y1,
                color: c,
                side_mat: Material::Concrete,
                top_mat: Material::Concrete,
                chip: 0.2,
                ..OBox::default()
            });
        }
    }

    /// The well: a round stone kerb in the middle of its sunken court, the
    /// frame over it, steps of kerbstone round the court.
    pub(super) fn well(&mut self, rng: &mut Rng) {
        let lm = &self.sk.landmarks[0];
        let c = lm.center;
        let g = self.g(c);
        let stone = self.pal.stone_pale;
        let n = 12;
        let r = 2.2;
        for k in 0..n {
            let a = k as f32 * TAU / n as f32;
            let dir = Vec2::from_angle(a);
            self.map.oboxes.push(OBox {
                center: c + dir * r,
                yaw: -a + std::f32::consts::FRAC_PI_2,
                half: Vec2::new(r * (TAU * 0.5 / n as f32).tan() + 0.15, 0.4),
                y0: g - 1.0,
                y1: g + 1.05 + 0.06 * (k % 3) as f32,
                color: stone * rng.range(0.85, 1.05),
                side_mat: Material::Concrete,
                top_mat: Material::Concrete,
                chip: 0.15,
                ..OBox::default()
            });
        }
        self.map.oboxes.push(OBox {
            center: c,
            half: Vec2::splat(1.7),
            y0: g - 1.0,
            // (Up to the kerb: nothing drives in it.)
            y1: g + 0.98,
            color: Vec3::splat(0.04),
            side_mat: Material::Rubble,
            top_mat: Material::Rubble,
            ..OBox::default()
        });
        // Two posts, a beam, the winding drum and its rope.
        let f = lm.facing;
        let at = |p: Vec2, y: f32| Vec3::new(p.x, g + y, p.y);
        let (a, b) = (c - f * 2.25, c + f * 2.25);
        let wood = Material::Wood;
        for p in [a, b] {
            self.map
                .segment(at(p, 0.5), at(p, 4.4), 0.17, TIMBER, wood, false);
        }
        self.map.segment(
            at(a - f * 0.5, 4.5),
            at(b + f * 0.5, 4.5),
            0.15,
            TIMBER,
            wood,
            false,
        );
        self.map
            .pipe(at(a, 2.5), at(b, 2.5), 0.26, TIMBER * 1.1, wood, false);
        self.map
            .segment(at(c, 2.4), at(c, 0.3), 0.03, TIMBER * 1.4, wood, false);
        // Kerbstones round the court, open to the streets.
        let ring = lm.reach - 1.0;
        let n = 26;
        for k in 0..n {
            let a = (k as f32 + 0.5) * TAU / n as f32;
            let p = c + Vec2::from_angle(a) * ring;
            let way = self.sk.streets.iter().any(|s| s.distance(p) < s.half + 2.0);
            if way || rng.chance(0.3) {
                continue;
            }
            let g = self.g(p);
            self.map.oboxes.push(OBox {
                center: p,
                yaw: -a + std::f32::consts::FRAC_PI_2,
                half: Vec2::new(rng.range(1.1, 1.5), 0.3),
                y0: g - 0.8,
                y1: g + rng.range(0.7, 1.1),
                color: stone * rng.range(0.82, 1.0),
                side_mat: Material::Concrete,
                top_mat: Material::Concrete,
                chip: 0.2,
                ..OBox::default()
            });
        }
    }

    /// What its people left about.
    pub(super) fn town(&mut self, rng: &mut Rng) {
        let sk = self.sk;
        if sk.streets.is_empty() {
            return;
        }
        // Power poles down one kerb of each stretch of street, wired up.
        let mut last: Option<(Vec2, [Vec3; 2])> = None;
        for s in &sk.streets {
            let dir = s.dir();
            let mut t = 6.0;
            while t < s.len() - 3.0 {
                let p = s.from + dir * t + dir.perp() * (s.half + 0.7);
                t += rng.range(20.0, 26.0);
                if !self.free(p, 0.5) {
                    continue;
                }
                let tips = self.map.pole(rng, p, dir);
                if let (Some((q, from)), Some(to)) = (last, tips)
                    && q.distance(p) < 34.0
                    && self.free_line(from[0], to[0])
                {
                    for k in 0..2 {
                        self.map.cable(from[k], to[k], 0.7);
                    }
                }
                last = tips.map(|t| (p, t));
            }
        }
        // Stalls round the court, or along the main street.
        let mut stalls = 0;
        for _ in 0..60 {
            if stalls >= 6 {
                break;
            }
            let s = sk.streets[rng.index(sk.streets.len().min(6))];
            let side = if rng.chance(0.5) { 1.0 } else { -1.0 };
            let out = s.dir().perp() * side;
            let p = s.from + s.dir() * rng.range(0.0, s.len()) + out * (s.half + 2.4);
            if self.free(p, 2.6) && !sk.streets.iter().any(|q| q.distance(p) < q.half + 1.8) {
                self.map.stall(rng, p, (-out.x).atan2(-out.y));
                stalls += 1;
            }
        }
        // A car or two left where it stopped.
        let mut cars = 0;
        for _ in 0..40 {
            if cars >= 4 {
                break;
            }
            let s = sk.streets[rng.index(sk.streets.len())];
            let p = s.from
                + s.dir() * rng.range(4.0, s.len().max(5.0))
                + s.dir().perp() * (s.half + 1.6);
            if self.free(p, 2.6) && self.flat(p, 2.5) {
                let d = s.dir();
                let yaw = d.x.atan2(d.y) + rng.range(-0.4, 0.4);
                self.map.car(rng, p, yaw);
                cars += 1;
            }
        }
        // Palms that died when the water did.
        let shore = sk.land.lake;
        let mut palms = 0;
        for _ in 0..300 {
            if palms >= 22 {
                break;
            }
            let p = match shore {
                Some(l) if rng.chance(0.5) => {
                    let dir = Vec2::from_angle(rng.range(0.0, TAU));
                    l.center + dir * (l.radius * rng.range(0.9, 1.25))
                }
                _ => {
                    let s = sk.streets[rng.index(sk.streets.len())];
                    s.from.lerp(s.to, rng.f32())
                        + Vec2::from_angle(rng.range(0.0, TAU)) * rng.range(6.0, 20.0)
                }
            };
            let dry = shore.is_none_or(|l| l.sdf(p).abs() < 5.0 || l.sdf(p) > 0.0);
            if dry && self.free(p, 1.2) && !sk.keep_clear(p) {
                self.map.palm(rng, p);
                palms += 1;
            }
        }
        // Boats where they settled.
        let Some(l) = shore else { return };
        let mut boats = 0;
        for _ in 0..40 {
            if boats >= 3 {
                break;
            }
            let dir = Vec2::from_angle(rng.range(0.0, TAU));
            let p = l.center + dir * l.radius * rng.range(0.15, 0.6);
            let len = rng.range(7.0, 10.0);
            if self.free(p, len * 0.6 + 3.0) && self.flat(p, 3.0) {
                let bow = Vec2::from_angle(rng.range(0.0, TAU));
                self.map.boat(rng, p, bow, len);
                boats += 1;
            }
        }
    }

    /// Nothing standing within `r` of `p`, and it's inside, clear of the
    /// spawns, the landmark's ground and the cliffs.
    fn free(&self, p: Vec2, r: f32) -> bool {
        let g = self.g(p);
        let sk = self.sk;
        let lm = &sk.landmarks[0];
        p.min_element() > 8.0
            && p.max_element() < sk.size - 8.0
            && sk.base_spots().iter().all(|b| b.distance(p) > sk.pad + r)
            && lm.center.distance(p) > lm.clear + r
            && sk.land.plateau(p).abs() > FOOT + 5.0 + r
            && sk.houses().all(|h| h.footprint.distance(p) > r)
            && !self.map.oboxes.iter().any(|o| {
                if !o.within(p, r) {
                    return false;
                }
                let (lo, hi) = o.y_range();
                hi > g + 0.2 && lo < g + 5.0
            })
    }

    fn flat(&self, p: Vec2, r: f32) -> bool {
        let g = self.g(p);
        (0..8).all(|k| (self.g(p + Vec2::from_angle(k as f32 * TAU / 8.0) * r) - g).abs() < 0.35)
    }

    /// Nothing in the way of a wire from `a` to `b`.
    fn free_line(&self, a: Vec3, b: Vec3) -> bool {
        let (dir, len) = ((b - a).normalize_or(Vec3::X), a.distance(b));
        !self
            .map
            .oboxes
            .iter()
            .any(|o| o.ray_hit(a + dir * 0.6, dir, len - 1.2))
    }
}
