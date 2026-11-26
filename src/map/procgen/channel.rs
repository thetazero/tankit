//! A plasma river's channel through a city: a trench with sheer walls of
//! dark glass, the surface well below the quays along both its banks, in
//! level reaches with a drop between one and the next (it's plumbing: it
//! doesn't follow the ground). Bridges are set from the quays, so their
//! decks are always clear of it.

use std::f32::consts::TAU;

use glam::{Vec2, Vec3};

use super::land::Land;
use super::skeleton::smoothstep;
use crate::map::river::River;
use crate::map::{Map, OBox};
use crate::render::material::Material;
use crate::rng::Rng;

/// How far its surface is below the quays, at least, and its bed below that.
pub const FALL: f32 = 5.0;
const BED: f32 = 3.5;
/// How thick its walls are: the ground's slope down is inside them.
pub const WALL: f32 = 6.0;
/// How far past its walls the quays run, level, and then how far it takes
/// them to meet the land.
pub const QUAY: f32 = 13.0;
const BLEND: f32 = 16.0;
/// A deck's thickness, and how far its top is over the quay.
const DECK: f32 = 0.8;
pub const RISE: f32 = 0.9;
const STEP: f32 = 4.0;

#[derive(Clone, Copy, Debug)]
pub struct Bridge {
    /// Where along it.
    pub t: f32,
    /// Its ends, on the quays, past its ramps.
    pub ends: [Vec2; 2],
    pub half: f32,
}

#[derive(Clone, Debug)]
pub struct Channel {
    at: Vec2,
    dir: Vec2,
    sway: f32,
    phase: f32,
    pub half: f32,
    /// How far along the samples start.
    from: f32,
    quay: Vec<f32>,
    surface: Vec<f32>,
    pub bridges: Vec<Bridge>,
}

impl Channel {
    /// One winding across a place `size` across, as far off `avoid` as it
    /// can be.
    pub fn pick(rng: &mut Rng, size: f32, avoid: &[(Vec2, f32)]) -> Channel {
        let mid = Vec2::splat(size * 0.5);
        let mut best: Option<(f32, Channel)> = None;
        for _ in 0..600 {
            let c = Channel {
                at: mid + Vec2::from_angle(rng.range(0.0, TAU)) * size * rng.range(0.05, 0.36),
                dir: Vec2::from_angle(rng.range(0.0, TAU)),
                sway: rng.range(16.0, 40.0),
                phase: rng.range(0.0, TAU),
                half: rng.range(14.0, 18.0),
                from: 0.0,
                quay: Vec::new(),
                surface: Vec::new(),
                bridges: Vec::new(),
            };
            let room = avoid
                .iter()
                .map(|&(p, r)| c.local(p).1.abs() - r - c.reach())
                .fold(f32::MAX, f32::min);
            // Through the place, not along its edge.
            let room = room.min(size * 0.3 - c.local(mid).1.abs());
            if best.as_ref().is_none_or(|b| room > b.0) {
                best = Some((room, c));
            }
            if room > 3.0 && rng.chance(0.2) {
                break;
            }
        }
        best.unwrap().1
    }

    /// How far off its middle it shapes the land.
    pub fn reach(&self) -> f32 {
        self.half + WALL + QUAY + BLEND
    }

    fn wander(&self, t: f32) -> f32 {
        self.sway * ((t / 62.0 + self.phase).sin() + 0.45 * (t / 27.0 - self.phase).sin())
    }

    /// Along it, and off its middle.
    pub fn local(&self, p: Vec2) -> (f32, f32) {
        let q = p - self.at;
        let t = q.dot(self.dir);
        // Square to its course, where it runs aslant.
        let slope = (self.wander(t + 1.0) - self.wander(t - 1.0)) * 0.5;
        let off = self.dir.perp_dot(q) - self.wander(t);
        (t, off / (1.0 + slope * slope).sqrt())
    }

    /// The point `off` to the side of its middle, `t` along.
    pub fn point(&self, t: f32, off: f32) -> Vec2 {
        let side = Vec2::new(-self.dir.y, self.dir.x);
        let mid = self.at + self.dir * t + side * self.wander(t);
        let h = self.heading(t);
        mid + Vec2::new(-h.y, h.x) * off
    }

    pub fn heading(&self, t: f32) -> Vec2 {
        let slope = (self.wander(t + 1.0) - self.wander(t - 1.0)) * 0.5;
        let side = Vec2::new(-self.dir.y, self.dir.x);
        (self.dir + side * slope).normalize()
    }

    fn sample(&self, v: &[f32], t: f32) -> f32 {
        if v.is_empty() {
            return 0.0;
        }
        let u = ((t - self.from) / STEP).clamp(0.0, (v.len() - 1) as f32);
        let k = (u as usize).min(v.len().saturating_sub(2));
        v[k] + (v[(k + 1).min(v.len() - 1)] - v[k]) * (u - k as f32)
    }

    pub fn quay_at(&self, t: f32) -> f32 {
        self.sample(&self.quay, t)
    }

    pub fn surface_at(&self, t: f32) -> f32 {
        self.sample(&self.surface, t)
    }

    /// Its heights, from the `land` it's cut in (which it isn't in yet).
    pub fn settle(&mut self, rng: &mut Rng, land: &Land, size: f32) {
        let n = ((size * 1.5 + 700.0) / STEP) as i32;
        self.from = -(n / 2) as f32 * STEP;
        let raw: Vec<f32> = (0..n)
            .map(|k| land.height(self.point(self.from + k as f32 * STEP, 0.0)))
            .collect();
        // The quays: the land, evened out a long way.
        let w = 11i32;
        self.quay = (0..n)
            .map(|k| {
                let (a, b) = ((k - w).max(0) as usize, (k + w).min(n - 1) as usize);
                raw[a..=b].iter().sum::<f32>() / (b - a + 1) as f32
            })
            .collect();
        // Its reaches, each level, running down from its higher end.
        let n = n as usize;
        let back = self.quay[0] < self.quay[n - 1];
        let at = |k: usize| if back { n - 1 - k } else { k };
        self.surface = vec![0.0; n];
        let (mut k, mut last) = (0, f32::MAX);
        while k < n {
            let end = (k + rng.int(16, 26) as usize).min(n);
            let low = (k..end).map(|i| self.quay[at(i)]).fold(f32::MAX, f32::min);
            last = last.min(low - FALL);
            for i in k..end {
                self.surface[at(i)] = last;
            }
            k = end;
        }
    }

    /// The ground at `p`, `h` without it.
    pub fn shape(&self, p: Vec2, h: f32) -> f32 {
        let (t, off) = self.local(p);
        let a = off.abs();
        if a > self.reach() || self.quay.is_empty() {
            return h;
        }
        if a < self.half + 2.0 {
            return self.surface_at(t) - BED;
        }
        let level = self.half + WALL + QUAY;
        let q = self.quay_at(t);
        q + (h - q) * smoothstep(level, level + BLEND, a)
    }

    /// Is `p` in it or its walls, or within `margin` of them?
    pub fn wet(&self, p: Vec2, margin: f32) -> bool {
        self.local(p).1.abs() < self.half + WALL + margin
    }

    /// A bridge over it, `t` along and `half` its width, unless one's there
    /// already: which one it is.
    pub fn bridge(&mut self, t: f32, half: f32) -> usize {
        if let Some(k) = self.bridges.iter().position(|b| (b.t - t).abs() < 56.0) {
            return k;
        }
        let out = self.half + WALL + RISE / 0.2 + 1.0;
        self.bridges.push(Bridge {
            t,
            ends: [self.point(t, -out), self.point(t, out)],
            half,
        });
        self.bridges.len() - 1
    }

    /// A bridge's deck's top.
    pub fn deck(&self, b: &Bridge) -> f32 {
        self.quay_at(b.t) + RISE
    }

    /// The river in it.
    pub fn river(&self) -> River {
        River {
            points: (0..self.surface.len())
                .map(|k| {
                    let t = self.from + k as f32 * STEP;
                    let p = self.point(t, 0.0);
                    Vec3::new(p.x, self.surface[k], p.y)
                })
                .collect(),
            half: self.half,
        }
    }

    /// Its walls, their lip, its lamps and its bridges.
    pub fn build(&self, map: &mut Map, size: f32) {
        let glass = Vec3::new(0.05, 0.065, 0.11);
        let plasma = Vec3::new(1.0, 0.16, 0.52);
        let mut put = |at: Vec2, yaw: f32, half: Vec2, y: (f32, f32), c: Vec3, m: Material| {
            map.oboxes.push(OBox {
                center: at,
                yaw,
                half,
                y0: y.0,
                y1: y.1,
                color: c,
                side_mat: m,
                top_mat: m,
                ..OBox::default()
            });
            map.oboxes.len() - 1
        };
        let mut loose = Vec::new();
        let mut ramps: Vec<(usize, f32)> = Vec::new();
        let mut lamps = Vec::new();
        let inside = |p: Vec2| p.min_element() > -10.0 && p.max_element() < size + 10.0;
        for k in 0..self.surface.len() - 1 {
            let t = self.from + (k as f32 + 0.5) * STEP;
            let mid = self.point(t, 0.0);
            let near = mid.min_element() > -330.0 && mid.max_element() < size + 330.0;
            if !near {
                continue;
            }
            let h = self.heading(t);
            let yaw = h.x.atan2(h.y);
            let (q, s) = (self.quay_at(t), self.surface_at(t));
            let spanned = self.bridges.iter().any(|b| (b.t - t).abs() < b.half + 2.0);
            for e in [-1.0f32, 1.0] {
                // A little longer on a bend's outside.
                let (a, b) = (
                    self.point(t - STEP * 0.5, e * (self.half + WALL)),
                    self.point(t + STEP * 0.5, e * (self.half + WALL)),
                );
                let long = a.distance(b).max(STEP) * 0.5 + 0.12;
                let wall = self.point(t, e * (self.half + WALL * 0.5));
                let i = put(
                    wall,
                    yaw,
                    Vec2::new(WALL * 0.5, long),
                    (s - BED - 1.0, q + 0.02 + 0.006 * (k % 3) as f32),
                    glass,
                    Material::Vhaal,
                );
                map_solid(&mut loose, i, true);
                if spanned {
                    continue;
                }
                // The lip: too low to stop a shove.
                let lip = self.point(t, e * (self.half + 0.5));
                let i = put(
                    lip,
                    yaw,
                    Vec2::new(0.45, long),
                    (q - 0.3, q + 0.4),
                    glass * 1.8,
                    Material::Vhaal,
                );
                map_solid(&mut loose, i, false);
                // Its light, along the wall's foot.
                let foot = self.point(t, e * (self.half - 0.02));
                let i = put(
                    foot,
                    yaw,
                    Vec2::new(0.06, long),
                    (s + 1.5, s + 1.75),
                    plasma * 0.9,
                    Material::Glow,
                );
                map_solid(&mut loose, i, false);
                if k % 5 == 0 {
                    let post = self.point(t, e * (self.half + 0.8));
                    put(
                        post,
                        yaw,
                        Vec2::splat(0.5),
                        (q - 0.3, q + 3.2),
                        glass,
                        Material::Vhaal,
                    );
                    let i = put(
                        post,
                        yaw,
                        Vec2::splat(0.2),
                        (q + 3.15, q + 3.8),
                        plasma,
                        Material::Glow,
                    );
                    map_solid(&mut loose, i, false);
                }
            }
            if inside(mid) && k % 5 == 2 {
                // Down in it: it lights its walls and what's over it from
                // below.
                lamps.push(Vec3::new(mid.x, s + 1.2, mid.y));
            }
        }
        let stone = Vec3::new(0.09, 0.1, 0.14);
        for b in &self.bridges {
            let h = self.heading(b.t);
            let across = Vec2::new(-h.y, h.x);
            let yaw = across.x.atan2(across.y);
            let (top, q) = (self.deck(b), self.quay_at(b.t));
            let span = self.half + WALL + 0.3;
            let mid = self.point(b.t, 0.0);
            let i = put(
                mid,
                yaw,
                Vec2::new(b.half, span),
                (top - DECK, top),
                stone,
                Material::Concrete,
            );
            ramps.push((i, 0.0));
            // Its ribs, deep at the walls and thin in the middle: an arch.
            let n = 30;
            for k in 0..n {
                let u = (k as f32 + 0.5) / n as f32 * 2.0 - 1.0;
                let deep = 0.25 + 2.9 * u * u;
                for x in [-b.half + 0.5, 0.0, b.half - 0.5] {
                    let i = put(
                        mid + across * u * self.half + h * x,
                        yaw,
                        Vec2::new(0.35, self.half / n as f32 + 0.02),
                        (top - DECK - deep, top - DECK + 0.05),
                        glass * 1.5,
                        Material::Vhaal,
                    );
                    map_solid(&mut loose, i, false);
                }
            }
            for e in [-1.0f32, 1.0] {
                // A parapet, a strip of light down it.
                let side = mid + h * e * (b.half - 0.3);
                put(
                    side,
                    yaw,
                    Vec2::new(0.28, span - 0.6),
                    (top - 0.02, top + 0.8),
                    glass * 1.5,
                    Material::Vhaal,
                );
                let i = put(
                    side + h * e * 0.3,
                    yaw,
                    Vec2::new(0.05, span - 1.2),
                    (top + 0.2, top + 0.34),
                    plasma * 0.8,
                    Material::Glow,
                );
                map_solid(&mut loose, i, false);
                for f in [-1.0f32, 1.0] {
                    put(
                        side + across * f * span,
                        yaw,
                        Vec2::splat(0.75),
                        (q - 0.4, top + 2.6),
                        glass,
                        Material::Vhaal,
                    );
                }
                // The ramp up onto it, from the quay.
                let slope = 0.2f32;
                let run = RISE / slope.tan();
                let down = across * e;
                let i = put(
                    mid + down * (span + run * 0.5),
                    down.x.atan2(down.y),
                    Vec2::new(b.half, run * 0.5 / slope.cos() + 0.05),
                    (q + RISE * 0.5 - 0.5, q + RISE * 0.5),
                    stone * 1.1,
                    Material::Concrete,
                );
                ramps.push((i, slope));
            }
        }
        for (i, solid) in loose {
            map.oboxes[i].solid = solid;
        }
        for (i, tilt) in ramps {
            map.oboxes[i].ramp = true;
            map.oboxes[i].tilt = tilt;
        }
        for at in lamps {
            map.sound(crate::map::Ambient::River, at);
        }
    }
}

fn map_solid(list: &mut Vec<(usize, bool)>, i: usize, solid: bool) {
    list.push((i, solid));
}
