//! The land under a generated map: landforms, and one height for every
//! point made from them, in the arena and on out to the horizon.
//!
//! Every biome's grammar builds its ground out of these. They're kept as
//! what they are (a plateau and the canyons through it, a wash, a plaza),
//! not only as heights, so whatever reads the skeleton (a dresser, the
//! bots) can ask about the plateau's edge or the wash's line.
//!
//! In order: the swells (tilts, ridges, hills), leveled where something
//! needs it (`base`); then plazas, the high ground and the ramps up it,
//! dunes, washes and bowls (`height`).

use std::collections::HashMap;
use std::f32::consts::TAU;

use glam::Vec2;

use super::mine::{line_distance, seg};
use super::skeleton::{Platform, smoothstep};
use crate::map::OBox;
use crate::map::valley::{noise, noise2};
use crate::rng::Rng;

/// The high ground falls to the floor over this far out from its edge
/// (under the cliff face, which stands over it).
pub const EDGE_FALL: f32 = 2.2;
/// The cliff's foot line: this far out from the high ground's edge.
pub const FOOT: f32 = 1.6;
/// A ramp's half-width, its sides falling away past it.
pub const RAMP_HALF: f32 = 7.0;

/// A broad shape the whole place lies on.
#[derive(Clone, Copy, Debug)]
pub enum Swell {
    /// Rising `grade` m per m along `dir`, from `from`, leveling off far out.
    Tilt {
        from: Vec2,
        dir: Vec2,
        grade: f32,
    },
    Cut(Cut),
    Hill {
        at: Vec2,
        height: f32,
        reach: f32,
    },
}

/// A valley or a ridge, right across the city: level along the middle,
/// `floor` m to either side of a line, then a bank.
#[derive(Clone, Copy, Debug)]
pub struct Cut {
    pub at: Vec2,
    pub dir: Vec2,
    /// How high (a valley's is negative).
    pub height: f32,
    pub floor: f32,
    bank: f32,
    sway: f32,
    phase: f32,
}

impl Cut {
    /// How far its line is off its straight course, `t` along it.
    fn wander(&self, t: f32) -> f32 {
        self.sway * (t / 85.0 + self.phase).sin()
    }

    /// Along it, and off its line.
    pub fn local(&self, p: Vec2) -> (f32, f32) {
        let q = p - self.at;
        let t = q.dot(self.dir);
        (t, self.dir.perp_dot(q) - self.wander(t))
    }

    /// Its line, `t` along.
    pub fn point(&self, t: f32) -> Vec2 {
        let side = Vec2::new(-self.dir.y, self.dir.x);
        self.at + self.dir * t + side * self.wander(t)
    }

    /// How wide the bank is there: steep here, easy there.
    pub fn bank(&self, t: f32) -> f32 {
        let u = 0.5 + 0.5 * (t / 47.0 + self.phase * 2.3).sin();
        self.bank * (1.0 + 1.7 * u * u)
    }

    /// 1 along the middle, 0 past the bank.
    pub fn depth(&self, p: Vec2) -> f32 {
        let (t, off) = self.local(p);
        1.0 - smoothstep(self.floor, self.floor + self.bank(t), off.abs())
    }
}

impl Swell {
    fn height(&self, p: Vec2) -> f32 {
        let bell = |u: f32| (1.0 - (u * u).min(1.0)).powi(2);
        match *self {
            Swell::Tilt { from, dir, grade } => {
                let t = (p - from).dot(dir);
                let far = 340.0;
                grade * far * (t / far).tanh()
            }
            Swell::Cut(c) => c.height * c.depth(p),
            Swell::Hill { at, height, reach } => height * bell(p.distance(at) / reach),
        }
    }
}

/// Level ground: `b` grown by `round`, blending into the land over `blend`.
#[derive(Clone, Copy, Debug)]
pub(super) struct Level {
    b: OBox,
    round: f32,
    blend: f32,
    height: f32,
    /// How far from its middle it changes anything, squared.
    reach2: f32,
}

impl Level {
    fn new(b: OBox, round: f32, blend: f32, height: f32) -> Level {
        let reach = b.half.length() + round + blend;
        Level {
            b,
            round,
            blend,
            height,
            reach2: reach * reach,
        }
    }
}

/// A piece of the high ground's shape.
#[derive(Clone, Debug)]
pub enum Slab {
    /// Everywhere (a plateau, cut by its canyons).
    Everywhere,
    /// Past a wavering line: `at` on it, `into` the high side.
    Beyond {
        at: Vec2,
        into: Vec2,
        amp: f32,
        wave: f32,
        phase: f32,
    },
    /// Outside a wavering ring round `center`.
    Outside {
        center: Vec2,
        radius: f32,
        phase: f32,
    },
    /// An island: a mesa or a butte.
    Island {
        center: Vec2,
        radius: f32,
        phase: f32,
    },
}

fn lobed(radius: f32, phase: f32, dir: Vec2) -> f32 {
    let a = dir.y.atan2(dir.x);
    radius * (1.0 + 0.14 * (3.0 * a + phase).sin() + 0.07 * (5.0 * a + 2.0 * phase).sin())
}

impl Slab {
    /// Signed distance (about) to its edge, negative inside.
    fn sdf(&self, p: Vec2) -> f32 {
        match *self {
            Slab::Everywhere => -1.0e4,
            Slab::Beyond {
                at,
                into,
                amp,
                wave,
                phase,
            } => {
                let d = (p - at).dot(into);
                let along = (p - at).dot(into.perp());
                let wobble = amp
                    * ((along / wave + phase).sin()
                        + 0.4 * (along / (wave * 0.41) + 2.0 * phase).sin())
                    + 4.0 * (noise(along / 23.0 + 500.0, 71) * 2.0 - 1.0);
                wobble - d
            }
            Slab::Outside {
                center,
                radius,
                phase,
            } => {
                let d = p - center;
                let r = d.length().max(1e-3);
                lobed(radius, phase, d / r) - r
            }
            Slab::Island {
                center,
                radius,
                phase,
            } => {
                let d = p - center;
                let r = d.length().max(1e-3);
                r - lobed(radius, phase, d / r)
            }
        }
    }
}

/// Low ground cut through the high: a canyon, a pass.
#[derive(Clone, Debug)]
pub struct Channel {
    pub line: Vec<Vec2>,
    pub half: f32,
}

/// A way up a cliff: from its foot on the floor to its top on the high
/// ground.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ramp {
    pub foot: Vec2,
    pub top: Vec2,
}

impl Ramp {
    /// The ramp's ground at `p`, on high ground `high` m up (0 off it).
    fn height(&self, p: Vec2, high: f32) -> f32 {
        let d = self.top - self.foot;
        let len = d.length();
        let dir = d / len;
        let t = (p - self.foot).dot(dir) / len;
        if !(0.0..=1.0).contains(&t) {
            return 0.0;
        }
        let across = (p - self.foot).perp_dot(dir).abs();
        high * t * (1.0 - smoothstep(RAMP_HALF, RAMP_HALF + 2.5, across))
    }

    pub fn distance(&self, p: Vec2) -> f32 {
        seg(p, self.foot, self.top).0
    }
}

/// A second level, `height` m over the floor, cliffs round it: a union of
/// slabs, channels and parks cut out of it, ramps up it.
#[derive(Clone, Debug, Default)]
pub struct High {
    pub height: f32,
    pub slabs: Vec<Slab>,
    pub channels: Vec<Channel>,
    /// Open ground cut out of it.
    pub parks: Vec<(Vec2, f32)>,
    pub ramps: Vec<Ramp>,
}

/// A dry wash cut into the floor along `line`.
#[derive(Clone, Debug)]
pub struct Wash {
    pub line: Vec<Vec2>,
    pub floor: f32,
    pub bank: f32,
    pub depth: f32,
}

impl Wash {
    pub fn distance(&self, p: Vec2) -> f32 {
        line_distance(&self.line, p)
    }

    /// How deep it's cut at `p`.
    pub fn cut(&self, p: Vec2) -> f32 {
        let d = self.distance(p);
        self.depth * (1.0 - smoothstep(self.floor, self.floor + self.bank, d))
    }
}

/// A round hollow with a rim (a crater), open `clear` m about.
#[derive(Clone, Copy, Debug)]
pub struct Bowl {
    pub at: Vec2,
    pub clear: f32,
    pub depth: f32,
    pub rim: f32,
}

impl Bowl {
    fn height(&self, p: Vec2) -> f32 {
        let d = self.at.distance(p);
        let bowl = -self.depth * (1.0 - smoothstep(0.0, self.clear + 5.0, d));
        let rim = self.rim * (1.0 - smoothstep(0.0, 8.0, (d - (self.clear + 9.0)).abs()));
        bowl + rim
    }
}

/// Dunes lying across the wind, none within `calm` (middle, none inside,
/// full outside).
#[derive(Clone, Debug)]
pub struct Dunes {
    pub wind: Vec2,
    pub wave: f32,
    pub height: f32,
    pub phase: f32,
    pub calm: Vec<(Vec2, f32, f32)>,
}

impl Dunes {
    /// The dunes at `p`, before anything flattens them.
    fn at(&self, p: Vec2) -> f32 {
        let warp = 0.6 * noise2(p.x / 70.0, p.y / 70.0, 81);
        let u = p.dot(self.wind) / self.wave + self.phase + warp;
        let f = u - u.floor();
        // A long gentle windward side, a shorter lee.
        let dune = smoothstep(0.0, 0.68, f) * (1.0 - smoothstep(0.68, 1.0, f));
        let along = 0.5 + 0.5 * noise2(p.x / 55.0, p.y / 55.0, 82);
        let ripple = 0.06 * (p.dot(self.wind) * 1.3 + 2.0 * noise(p.x / 9.0 + 500.0, 83)).sin();
        self.height * dune * along + ripple
    }
}

/// Smooth max and min, blending over `k`.
fn smax(a: f32, b: f32, k: f32) -> f32 {
    let h = (0.5 + 0.5 * (a - b) / k).clamp(0.0, 1.0);
    b + (a - b) * h + k * h * (1.0 - h)
}

fn smin(a: f32, b: f32, k: f32) -> f32 {
    -smax(-a, -b, k)
}

#[derive(Clone, Debug, Default)]
pub struct Land {
    pub swells: Vec<Swell>,
    pub(super) levels: Vec<Level>,
    /// Raised ground, sloping down to the rest at its edge.
    pub platforms: Vec<Platform>,
    pub high: High,
    pub washes: Vec<Wash>,
    pub bowls: Vec<Bowl>,
    pub dunes: Option<Dunes>,
}

impl Land {
    /// The lie of a city `size` across: up to two valleys or ridges right
    /// across it, kept off `avoid` (places, and how far round them), and
    /// hills and a tilt, low where those are.
    pub fn rolling(rng: &mut Rng, size: f32, avoid: &[(Vec2, f32)]) -> Land {
        let mid = Vec2::splat(size * 0.5);
        let any = |rng: &mut Rng| Vec2::from_angle(rng.range(0.0, TAU));
        let want = match rng.f32() {
            x if x < 0.22 => 0,
            x if x < 0.72 => 1,
            _ => 2,
        };
        let mut cuts: Vec<Cut> = Vec::new();
        let mut tries = 0;
        while cuts.len() < want && tries < 300 {
            tries += 1;
            let up = rng.chance(0.45);
            let h = rng.range(7.5, 13.0);
            let c = Cut {
                at: mid + any(rng) * size * rng.range(0.0, 0.36),
                dir: any(rng),
                height: if up { h } else { -h },
                floor: if up {
                    rng.range(7.0, 13.0)
                } else {
                    rng.range(12.0, 20.0)
                },
                bank: h * rng.range(1.1, 1.7),
                sway: rng.range(4.0, 16.0),
                phase: rng.range(0.0, TAU),
            };
            let reach = c.floor + c.bank * 1.4;
            let ease = tries as f32 / 300.0 * 14.0;
            let clear = avoid
                .iter()
                .all(|&(p, r)| c.local(p).1.abs() > reach + r - ease)
                // Two run much the same way, well apart, or cross squarely.
                && cuts.iter().all(|o| {
                    let turn = o.dir.perp_dot(c.dir).abs();
                    turn > 0.75 || (turn < 0.35 && o.local(c.at).1.abs() > 110.0)
                });
            if clear {
                cuts.push(c);
            }
        }
        let mut swells: Vec<Swell> = Vec::new();
        if rng.chance(0.4) {
            let grade = rng.range(0.015, 0.045);
            swells.push(Swell::Tilt {
                from: mid,
                dir: any(rng),
                grade,
            });
        }
        let hills = if cuts.is_empty() {
            rng.int(1, 4)
        } else {
            rng.int(0, 3)
        };
        for _ in 0..hills {
            let at = Vec2::new(rng.range(-0.15, 1.15), rng.range(-0.15, 1.15)) * size;
            let reach = rng.range(40.0, 100.0);
            // Not where it would fill a valley in or bury a ridge.
            if cuts
                .iter()
                .any(|c| c.local(at).1.abs() < reach * 0.6 + c.floor + c.bank)
            {
                continue;
            }
            swells.push(Swell::Hill {
                at,
                height: reach * rng.range(0.07, 0.18),
                reach,
            });
        }
        swells.extend(cuts.into_iter().map(Swell::Cut));
        Land {
            swells,
            ..Land::default()
        }
    }

    pub fn cuts(&self) -> Vec<Cut> {
        self.swells
            .iter()
            .filter_map(|s| match s {
                Swell::Cut(c) => Some(*c),
                _ => None,
            })
            .collect()
    }

    /// How steep the land is, before any of it's leveled.
    pub fn steep(&self, p: Vec2) -> f32 {
        let e = 2.0;
        let d = |v: Vec2| (self.swell(p + v) - self.swell(p - v)) / (2.0 * e);
        Vec2::new(d(Vec2::X * e), d(Vec2::Y * e)).length()
    }

    /// Levels `b`'s ground and `margin` m round it, at `height`.
    pub fn level_at(&mut self, b: OBox, margin: f32, blend: f32, height: f32) {
        self.levels.push(Level::new(b, margin, blend, height));
    }

    /// What swells it's made of, for the probes.
    #[cfg(test)]
    pub fn describe(&self) -> String {
        let of = |s: &Swell| match s {
            Swell::Tilt { grade, .. } => format!("tilt {grade:.3}"),
            Swell::Cut(c) => format!("cut {:.1} m, floor {:.0} at {:.0}", c.height, c.floor, c.at),
            Swell::Hill { at, height, reach } => {
                format!("hill {height:.1} m over {reach:.0} m at {at:.0}")
            }
        };
        self.swells.iter().map(of).collect::<Vec<_>>().join("; ")
    }

    /// The swells alone, before any of it's leveled.
    pub fn swell(&self, p: Vec2) -> f32 {
        self.swells.iter().map(|s| s.height(p)).sum()
    }

    /// The swells, leveled where something needs it: what everything else
    /// lies on.
    pub fn base(&self, p: Vec2) -> f32 {
        let mut h = self.swell(p);
        for l in &self.levels {
            if l.b.center.distance_squared(p) > l.reach2 {
                continue;
            }
            let d = l.b.distance(p) - l.round;
            h += (l.height - h) * (1.0 - smoothstep(0.0, l.blend, d));
        }
        h
    }

    /// The ground's height at `p`.
    pub fn height(&self, p: Vec2) -> f32 {
        let plaza = self
            .platforms
            .iter()
            .map(|pl| pl.height_at(p))
            .fold(0.0, f32::max);
        let mut h = self.base(p) + plaza;
        let s = self.plateau(p);
        let up = 1.0 - smoothstep(0.0, EDGE_FALL, s);
        let high = &self.high;
        h += high
            .ramps
            .iter()
            .map(|r| r.height(p, high.height))
            .fold(high.height * up, f32::max);
        let low = 1.0 - up;
        if let Some(d) = &self.dunes {
            h += d.at(p) * self.drift(d, p, s) * (0.3 + 0.7 * low);
        }
        let bowls: f32 = self.bowls.iter().map(|b| b.height(p)).sum();
        h + (bowls - self.wash_cut(p)) * low
    }

    /// How much of the dunes is left at `p` (`s`: the plateau's distance):
    /// none where it's calm, on the ramps, by the cliffs or in a wash.
    fn drift(&self, d: &Dunes, p: Vec2, s: f32) -> f32 {
        let calm = d
            .calm
            .iter()
            .map(|&(at, r0, r1)| smoothstep(r0, r1, at.distance(p)))
            .fold(1.0, f32::min);
        let ramps = smoothstep(RAMP_HALF + 2.0, RAMP_HALF + 12.0, self.ramp_distance(p));
        let cliff = smoothstep(2.0, 10.0, s.abs());
        let wash = self
            .washes
            .iter()
            .map(|w| {
                let edge = w.floor + w.bank;
                smoothstep(edge, edge + 10.0, w.distance(p))
            })
            .fold(1.0, f32::min);
        calm.min(ramps).min(cliff).min(wash)
    }

    /// The high ground's signed distance at `p`: negative on it, very far
    /// where there's none.
    pub fn plateau(&self, p: Vec2) -> f32 {
        // Joined and cut with the corners rounded off, so where a canyon
        // meets a cliff there's no sliver of rock left standing.
        let high = &self.high;
        let mut s = high
            .slabs
            .iter()
            .map(|sl| sl.sdf(p))
            .fold(1.0e4, |a, b| smin(a, b, 8.0));
        for c in &high.channels {
            s = smax(s, c.half - line_distance(&c.line, p), 8.0);
        }
        for &(c, r) in &high.parks {
            s = smax(s, r - c.distance(p), 8.0);
        }
        s
    }

    /// On the high ground (well in from its edge).
    pub fn on_high(&self, p: Vec2) -> bool {
        self.plateau(p) < -2.0
    }

    /// How deep the washes are cut at `p`.
    pub fn wash_cut(&self, p: Vec2) -> f32 {
        self.washes.iter().map(|w| w.cut(p)).fold(0.0, f32::max)
    }

    pub fn wash_distance(&self, p: Vec2) -> f32 {
        self.washes
            .iter()
            .map(|w| w.distance(p))
            .fold(f32::INFINITY, f32::min)
    }

    pub fn ramp_distance(&self, p: Vec2) -> f32 {
        self.high
            .ramps
            .iter()
            .map(|r| r.distance(p))
            .fold(f32::INFINITY, f32::min)
    }

    /// The cliffs' foot lines, the floor on the left going along (see
    /// `Map::rock_face`): traced round the high
    /// ground `FOOT` m out from its edge, a little past the arena's.
    pub fn cliff_lines(&self, size: f32) -> Vec<Vec<Vec2>> {
        let lo = -14.0;
        let hi = size + 14.0;
        let cell = 1.0;
        let n = ((hi - lo) / cell) as usize + 1;
        let at = |i: usize, j: usize| Vec2::new(lo + i as f32 * cell, lo + j as f32 * cell);
        let mut v = vec![0.0f32; n * n];
        for j in 0..n {
            for i in 0..n {
                v[j * n + i] = self.plateau(at(i, j)) - FOOT;
            }
        }
        // Marching squares: a segment across each cell the level crosses.
        let mut segs: Vec<(Vec2, Vec2)> = Vec::new();
        let cross = |a: Vec2, b: Vec2, fa: f32, fb: f32| a.lerp(b, fa / (fa - fb));
        for j in 0..n - 1 {
            for i in 0..n - 1 {
                let c = [at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1)];
                let f = [
                    v[j * n + i],
                    v[j * n + i + 1],
                    v[(j + 1) * n + i + 1],
                    v[(j + 1) * n + i],
                ];
                let mut pts = Vec::new();
                for k in 0..4 {
                    let (a, b) = (k, (k + 1) % 4);
                    if (f[a] < 0.0) != (f[b] < 0.0) {
                        pts.push(cross(c[a], c[b], f[a], f[b]));
                    }
                }
                if pts.len() == 2 {
                    segs.push((pts[0], pts[1]));
                } else if pts.len() == 4 {
                    segs.push((pts[0], pts[1]));
                    segs.push((pts[2], pts[3]));
                }
            }
        }
        // Chained into lines.
        let key = |p: Vec2| ((p.x * 64.0).round() as i64, (p.y * 64.0).round() as i64);
        let mut ends: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
        for (k, &(a, b)) in segs.iter().enumerate() {
            ends.entry(key(a)).or_default().push(k);
            ends.entry(key(b)).or_default().push(k);
        }
        let mut used = vec![false; segs.len()];
        let mut lines = Vec::new();
        for start in 0..segs.len() {
            if used[start] {
                continue;
            }
            used[start] = true;
            let mut line = vec![segs[start].0, segs[start].1];
            // Grow at the back end, then the front.
            for _ in 0..2 {
                loop {
                    let tip = *line.last().unwrap();
                    let next = ends
                        .get(&key(tip))
                        .and_then(|ks| ks.iter().copied().find(|&k| !used[k]));
                    let Some(k) = next else { break };
                    used[k] = true;
                    let (a, b) = segs[k];
                    line.push(if key(a) == key(tip) { b } else { a });
                }
                line.reverse();
            }
            if line.len() < 4 {
                continue;
            }
            // Turned so the floor's on its left.
            let m = line.len() / 2;
            let dir = (line[m + 1] - line[m - 1]).normalize_or(Vec2::X);
            let left = Vec2::new(-dir.y, dir.x);
            if self.plateau(line[m] + left) < self.plateau(line[m] - left) {
                line.reverse();
            }
            lines.push(line);
        }
        lines
    }

    /// The cliffs as they're built: each foot line cut where a ramp comes up
    /// it, kept to near the arena.
    pub fn cliffs(&self, size: f32) -> Vec<Vec<Vec2>> {
        let mut out = Vec::new();
        for line in self.cliff_lines(size) {
            let mut piece = Vec::new();
            for p in line {
                let gap = self
                    .high
                    .ramps
                    .iter()
                    .any(|r| r.distance(p) < RAMP_HALF + 2.0);
                let far = p.min_element() < -12.0 || p.max_element() > size + 12.0;
                if gap || far {
                    if piece.len() >= 4 {
                        out.push(std::mem::take(&mut piece));
                    }
                    piece.clear();
                } else {
                    piece.push(p);
                }
            }
            if piece.len() >= 4 {
                out.push(piece);
            }
        }
        out
    }

    /// Levels round ground, `r` across, at the height it has.
    pub fn level_round(&mut self, at: Vec2, r: f32, blend: f32) {
        let b = OBox {
            center: at,
            half: Vec2::splat(0.01),
            ..OBox::default()
        };
        self.push(b, r, blend);
    }

    /// Levels `b`'s ground and `margin` m round it, at the height it has.
    pub fn level_box(&mut self, b: &OBox, margin: f32, blend: f32) {
        self.push(*b, margin, blend);
    }

    fn push(&mut self, b: OBox, round: f32, blend: f32) {
        if self.swells.is_empty() {
            return;
        }
        let height = self.base(b.center);
        self.levels.push(Level::new(b, round, blend, height));
    }
}
