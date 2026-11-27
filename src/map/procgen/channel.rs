//! A plasma river's channel through a city: a trench with sheer walls of
//! dark glass, the surface well below the quays along both its banks, in
//! level reaches with a drop between one and the next (it's plumbing: it
//! doesn't follow the ground). Bridges are set from the quays, so their
//! decks are always clear of it. Slips run down to it along its walls: how
//! hover hulls, which ride on it, get on and off.

use std::f32::consts::TAU;
use std::sync::Arc;

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
const BLEND: f32 = 9.0;
/// A deck's thickness, and how far its top is over the quay.
const DECK: f32 = 0.8;
pub const RISE: f32 = 0.9;
pub const STEP: f32 = 4.0;

#[derive(Clone, Copy, Debug)]
pub struct Bridge {
    /// Where along it.
    pub t: f32,
    /// Its ends, on the quays, past its ramps.
    pub ends: [Vec2; 2],
    pub half: f32,
}

/// How wide a slip is, out from the wall it's along.
const SLIP: f32 = 9.0;
/// Half of how long its landing is, level with the quay at its top.
const LANDING: f32 = 7.0;
/// How far its landing runs on under the wall at its end.
const LEDGE: f32 = 1.0;
/// How steep it is (rise over run), and how far under the surface it goes.
const SLIP_SLOPE: f32 = 0.4;
const SLIP_FOOT: f32 = 1.0;
/// The furthest one goes down.
const SLIP_FALL: f32 = 9.0;

/// A ramp from a quay down into the river, in the channel along its wall.
#[derive(Clone, Copy, Debug)]
pub struct Slip {
    /// The middle of its landing, along the river.
    pub t: f32,
    /// Which bank, and which way along it's down.
    pub side: f32,
    pub down: f32,
    /// Its top's height, and the river's surface by it.
    pub top: f32,
    pub surface: f32,
}

impl Slip {
    /// From its landing's middle to its foot.
    pub fn len(&self) -> f32 {
        LANDING + (self.top - self.surface + SLIP_FOOT) / SLIP_SLOPE
    }

    /// Its height `u` down along it, if it's that long.
    fn height(&self, u: f32) -> Option<f32> {
        (u > -LANDING - LEDGE && u < self.len())
            .then(|| self.top - (u - LANDING).max(0.0) * SLIP_SLOPE)
    }

    /// On its landing's stretch of the bank?
    fn lands(&self, t: f32, side: f32) -> bool {
        side == self.side && (t - self.t).abs() < LANDING + LEDGE
    }
}

#[derive(Clone, Debug)]
pub struct Channel {
    /// Its middle, every `STEP` m along, from `from` (0's the middle of the
    /// stretch that's fought over).
    pts: Arc<Vec<Vec2>>,
    /// The nearest of `pts` from each cell of a grid over it all.
    grid: Arc<Grid>,
    pub half: f32,
    /// How far along the samples start.
    from: f32,
    /// How far along each way is fought over.
    pub play: f32,
    quay: Vec<f32>,
    surface: Vec<f32>,
    pub bridges: Vec<Bridge>,
    pub slips: Vec<Slip>,
}

#[derive(Debug)]
struct Grid {
    origin: Vec2,
    n: usize,
    near: Vec<u16>,
}

const CELL: f32 = 8.0;

impl Grid {
    fn new(pts: &[Vec2]) -> Grid {
        let lo = pts.iter().copied().reduce(Vec2::min).unwrap() - 200.0;
        let hi = pts.iter().copied().reduce(Vec2::max).unwrap() + 200.0;
        let n = ((hi - lo).max_element() / CELL).ceil() as usize + 1;
        let near = (0..n * n)
            .map(|k| {
                let c = lo + Vec2::new((k % n) as f32 + 0.5, (k / n) as f32 + 0.5) * CELL;
                (0..pts.len())
                    .min_by(|&a, &b| {
                        let d = |i: usize| pts[i].distance_squared(c);
                        d(a).total_cmp(&d(b))
                    })
                    .unwrap() as u16
            })
            .collect();
        Grid {
            origin: lo,
            n,
            near,
        }
    }

    fn near(&self, p: Vec2) -> usize {
        let q = ((p - self.origin) / CELL).clamp(Vec2::ZERO, Vec2::splat(self.n as f32 - 1.0));
        self.near[q.y as usize * self.n + q.x as usize] as usize
    }
}

impl Channel {
    /// A winding one, `play` m of it each way from the middle doubling back
    /// on itself, its ends running off straight; its middle at 0.
    pub fn wind(rng: &mut Rng) -> Channel {
        let half = rng.range(13.0, 16.0);
        let mut tries = 0;
        loop {
            tries += 1;
            let play = rng.range(370.0, 430.0);
            let tail = 480.0;
            let n = ((play + tail) / STEP) as i32;
            // Its heading swings from side to side: each bend its own.
            let turn = rng.range(1.25, 1.85) - tries as f32 * 0.003;
            let (wave, phase) = (rng.range(520.0, 720.0), rng.range(0.0, TAU));
            let (wild, wild_phase) = (rng.range(0.15, 0.4), rng.range(0.0, TAU));
            let heading = |s: f32| {
                let fade = 1.0 - smoothstep(play + 10.0, play + 190.0, s.abs());
                let a = turn * (s / wave * TAU + phase).sin()
                    + wild * (s / wave * 2.3 * TAU + wild_phase).sin();
                Vec2::from_angle(a * fade)
            };
            let mut pts = vec![Vec2::ZERO];
            for k in 0..n {
                let last = *pts.last().unwrap();
                pts.push(last + heading((k as f32 + 0.5) * STEP) * STEP);
            }
            let mut back = Vec::new();
            let mut at = Vec2::ZERO;
            for k in 0..n {
                at -= heading(-(k as f32 + 0.5) * STEP) * STEP;
                back.push(at);
            }
            back.reverse();
            back.extend(pts);
            let pts = back;
            // Never too near itself: a neck of city between two bends.
            let gap = ((half + WALL) * 2.0 + 44.0).powi(2);
            let clear = (0..pts.len()).step_by(3).all(|a| {
                (a + 45..pts.len())
                    .step_by(3)
                    .all(|b| pts[a].distance_squared(pts[b]) > gap)
            });
            if !clear && tries < 400 {
                continue;
            }
            return Channel {
                grid: Arc::new(Grid::new(&pts)),
                from: -(n as f32) * STEP,
                pts: Arc::new(pts),
                half,
                play,
                quay: Vec::new(),
                surface: Vec::new(),
                bridges: Vec::new(),
                slips: Vec::new(),
            };
        }
    }

    /// Moved by `by`.
    pub fn moved(mut self, by: Vec2) -> Channel {
        let pts: Vec<Vec2> = self.pts.iter().map(|p| *p + by).collect();
        self.grid = Arc::new(Grid::new(&pts));
        self.pts = Arc::new(pts);
        self
    }

    /// Where it's fought over.
    pub fn course(&self) -> Vec<Vec2> {
        let n = (self.play / STEP) as i32;
        (-n..=n).map(|k| self.point(k as f32 * STEP, 0.0)).collect()
    }

    /// How far off its middle it shapes the land.
    pub fn reach(&self) -> f32 {
        self.half + WALL + QUAY + BLEND
    }

    /// Along it, and off its middle (to its left's positive).
    pub fn local(&self, p: Vec2) -> (f32, f32) {
        let k = self.grid.near(p);
        let (lo, hi) = (k.saturating_sub(4), (k + 4).min(self.pts.len() - 1));
        let mut best = (f32::MAX, 0.0, 0.0);
        for i in lo..hi {
            let (a, d) = (self.pts[i], self.pts[i + 1] - self.pts[i]);
            let t = ((p - a).dot(d) / d.length_squared()).clamp(0.0, 1.0);
            let to = p - (a + d * t);
            let off = to.length_squared();
            if off < best.0 {
                let side = if d.perp_dot(p - a) < 0.0 { -1.0 } else { 1.0 };
                best = (off, (i as f32 + t) * STEP, side);
            }
        }
        (self.from + best.1, best.0.sqrt() * best.2)
    }

    /// The point `off` to the side of its middle, `t` along.
    pub fn point(&self, t: f32, off: f32) -> Vec2 {
        let u = ((t - self.from) / STEP).clamp(0.0, self.pts.len() as f32 - 1.001);
        let k = u as usize;
        let mid = self.pts[k].lerp(self.pts[k + 1], u - k as f32);
        let h = self.heading(t);
        mid + Vec2::new(-h.y, h.x) * off
    }

    pub fn heading(&self, t: f32) -> Vec2 {
        let at = |t: f32| {
            let u = ((t - self.from) / STEP).clamp(0.0, self.pts.len() as f32 - 1.001);
            let k = u as usize;
            self.pts[k].lerp(self.pts[k + 1], u - k as f32)
        };
        (at(t + 3.0) - at(t - 3.0)).normalize_or(Vec2::X)
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
    pub fn settle(&mut self, rng: &mut Rng, land: &Land) {
        let n = self.pts.len() as i32;
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
        for s in self.slips.iter().filter(|s| s.side * off > 0.0) {
            let Some(y) = s.height((t - s.t) * s.down) else {
                continue;
            };
            if s.lands(t, s.side) && a > self.half - SLIP - 0.7 && a < self.half + WALL {
                // Level, over the wall and onto the quay.
                let out = smoothstep(self.half + WALL * 0.5, self.half + WALL, a);
                return y + (self.quay_at(t) - y) * out;
            }
            if a > self.half - SLIP - 0.7 && a < self.half + 2.0 {
                return y;
            }
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

    /// Slips, one on each bank of each stretch between bridges that has room
    /// for them (so together they're a ford between two bridges), where `ok`
    /// says a landing's quay may be.
    pub fn lay_slips(&mut self, ok: impl Fn(Vec2) -> bool) {
        let mut at: Vec<f32> = self.bridges.iter().map(|b| b.t).collect();
        at.sort_by(f32::total_cmp);
        for w in at.windows(2) {
            for e in [-1.0f32, 1.0] {
                let mut best: Option<(f32, Slip)> = None;
                for down in [-1.0f32, 1.0] {
                    let mut t = w[0];
                    while t < w[1] {
                        t += 2.0;
                        let s = Slip {
                            t,
                            side: e,
                            down,
                            top: self.quay_at(t),
                            surface: self.surface_at(t + down * 20.0),
                        };
                        let foot = s.len() + 6.0;
                        let (lo, hi) = (t - LANDING - 3.0, t + down * foot);
                        let (lo, hi) = if down > 0.0 {
                            (lo, hi)
                        } else {
                            (hi, t + LANDING + 3.0)
                        };
                        let clear = s.top - s.surface < SLIP_FALL
                            // All of it by the one reach.
                            && [lo, t, hi]
                                .iter()
                                .all(|&t| (self.surface_at(t) - s.surface).abs() < 0.01)
                            && (self.quay_at(t - LANDING) - self.quay_at(t + LANDING)).abs() < 0.7
                            && self
                                .bridges
                                .iter()
                                .all(|b| hi < b.t - b.half - 3.0 || lo > b.t + b.half + 3.0)
                            && ok(self.point(t, e * (self.half + WALL + 4.0)))
                            && ok(self.point(hi, 0.0))
                            && ok(self.point(lo, 0.0));
                        // In the middle of the stretch.
                        let off = ((lo + hi) - (w[0] + w[1])).abs();
                        if clear && best.as_ref().is_none_or(|b| off < b.0) {
                            best = Some((off, s));
                        }
                    }
                }
                self.slips.extend(best.map(|(_, s)| s));
            }
        }
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
                // In slices, each as long as its own side of a bend.
                let slice = WALL / 3.0;
                let chord = |off: f32| {
                    let (a, b) = (
                        self.point(t - STEP * 0.5, e * off),
                        self.point(t + STEP * 0.5, e * off),
                    );
                    a.distance(b) * 0.5
                };
                for i in 0..3 {
                    let (near, far) = (
                        self.half + slice * i as f32,
                        self.half + slice * (i + 1) as f32,
                    );
                    let long = chord(near).max(chord(far)) + 0.1;
                    let wall = self.point(t, e * (near + far) * 0.5);
                    put(
                        wall,
                        yaw,
                        Vec2::new(slice * 0.5, long),
                        (
                            s - BED - 1.0,
                            q + 0.02 + 0.006 * (k % 3) as f32 + 0.002 * i as f32,
                        ),
                        glass,
                        Material::Vhaal,
                    );
                }
                let long = chord(self.half) + 0.12;
                let landing = self
                    .slips
                    .iter()
                    .any(|s| [-0.5, 0.0, 0.5].iter().any(|k| s.lands(t + k * STEP, e)));
                if spanned || landing {
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
        // The slips' parapets: down their open side in steps, and across
        // their landings' ends.
        for s in &self.slips {
            let bed = s.surface - BED - 1.0;
            let step = 2.0;
            let n = ((s.len() + LANDING) / step) as i32;
            for k in 0..n {
                let u = -LANDING + (k as f32 + 0.5) * step;
                let t = s.t + s.down * u;
                let h = self.heading(t);
                let top = s.height(u - step * 0.5 + 0.01).unwrap_or(s.top) + 0.8;
                let off = self.half - SLIP - 0.7;
                put(
                    self.point(t, s.side * off),
                    h.x.atan2(h.y),
                    Vec2::new(0.9, step * 0.5 + 0.1),
                    (bed, top),
                    glass * 1.5,
                    Material::Vhaal,
                );
            }
            let t = s.t - s.down * (LANDING + 1.3);
            let h = self.heading(t);
            put(
                self.point(t, s.side * (self.half - SLIP * 0.5 - 0.5)),
                h.x.atan2(h.y),
                Vec2::new(SLIP * 0.5 + 1.1, 1.8),
                (bed, s.top + 0.8),
                glass * 1.5,
                Material::Vhaal,
            );
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
