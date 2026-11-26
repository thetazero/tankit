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

use glam::{Vec2, Vec3};

use super::mine::{line_at, line_distance, line_len, seg};
use super::skeleton::{Platform, smoothstep};
use crate::map::OBox;
use crate::map::solid::{Rock, Surface};
use crate::map::valley::{noise, noise2};
use crate::rng::Rng;

/// The high ground falls to the floor over this far out from its edge
/// (under the cliff face, which stands over it).
pub const EDGE_FALL: f32 = 2.2;
/// How far a crossing's ramp runs, from the top down to the floor.
const FORD_RUN: f32 = 21.0;
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
    /// Behind a cliff along any line, the high side on its left going
    /// along; straight on past its ends. (The mine's to come.)
    #[cfg_attr(not(test), allow(dead_code))]
    Behind { line: Vec<Vec2> },
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

/// Signed distance to `line` (run straight on past its ends), negative on
/// its left.
fn behind(line: &[Vec2], p: Vec2) -> f32 {
    let n = line.len();
    let mut best = (f32::INFINITY, 0.0);
    for k in 0..n - 1 {
        let (a, b) = (line[k], line[k + 1]);
        let d = b - a;
        let mut t = (p - a).dot(d) / d.length_squared();
        // (The first and last stretches have no end on the outside.)
        if k > 0 {
            t = t.max(0.0);
        }
        if k + 2 < n {
            t = t.min(1.0);
        }
        let q = a + d * t;
        let dist = p.distance(q);
        // At a corner both stretches are as near: the one it's squarer to
        // says which side it's on.
        let side = d.normalize().perp_dot(p - q);
        if dist < best.0 - 1e-4 || (dist < best.0 + 1e-4 && side.abs() > f32::abs(best.1)) {
            best = (dist, side);
        }
    }
    -best.0 * best.1.signum()
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
            Slab::Behind { ref line } => behind(line, p),
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

/// A dry wash cut into the floor along `line`, its floor `floor` m either
/// side of it, narrowing and widening by `sway` as it winds.
#[derive(Clone, Debug)]
pub struct Wash {
    pub line: Vec<Vec2>,
    pub floor: f32,
    pub bank: f32,
    pub depth: f32,
    pub sway: f32,
    pub phase: f32,
}

impl Wash {
    pub fn distance(&self, p: Vec2) -> f32 {
        line_distance(&self.line, p)
    }

    /// Half the floor's width, `s` m along it.
    pub fn half(&self, s: f32) -> f32 {
        self.floor + self.sway * (0.08 * s + self.phase).sin()
    }

    /// How deep it's cut at `p`.
    pub fn cut(&self, p: Vec2) -> f32 {
        let (d, s) = self.near(p);
        let half = self.half(s);
        self.depth * (1.0 - smoothstep(half, half + self.bank, d))
    }

    /// Distance from its line, and how far along it the nearest point is.
    pub fn near(&self, p: Vec2) -> (f32, f32) {
        let (mut best, mut at, mut acc) = (f32::INFINITY, 0.0, 0.0);
        for w in self.line.windows(2) {
            let len = w[0].distance(w[1]);
            let (d, t) = seg(p, w[0], w[1]);
            if d < best {
                best = d;
                at = acc + t * len;
            }
            acc += len;
        }
        (best, at)
    }

    /// Its line `s` m along it, its way along, and the normal to the right
    /// of that.
    pub fn frame(&self, s: f32) -> (Vec2, Vec2, Vec2) {
        let (p, t) = line_at(&self.line, s);
        (p, t, Vec2::new(t.y, -t.x))
    }

    pub fn len(&self) -> f32 {
        line_len(&self.line, self.line.len() - 1)
    }

    /// The stretch of it inside an arena `size` across (from, to, along it).
    pub fn inside(&self, size: f32) -> (f32, f32) {
        let total = self.len();
        let mut lo = None;
        let mut hi = 0.0;
        let mut u = 0.0;
        while u < total {
            let (p, _) = line_at(&self.line, u);
            if p.min_element() > 4.0 && p.max_element() < size - 4.0 {
                lo.get_or_insert(u);
                hi = u;
            }
            u += 1.0;
        }
        (lo.unwrap_or(0.0), hi)
    }
}

/// A room of a dig, in the dig's own frame: x along the cliff to the right
/// looking in, y into the rock from the mouth, its floor and roof over the
/// dig's floor.
#[derive(Clone, Copy, Debug)]
pub struct Hollow {
    pub min: Vec2,
    pub max: Vec2,
    pub floor: f32,
    pub roof: f32,
}

/// Rooms dug into the rock, placed anywhere: the mouth at `at` (its floor
/// `floor` m up), going in along `facing`.
#[derive(Clone, Debug)]
pub struct Dig {
    pub at: Vec2,
    pub facing: Vec2,
    pub floor: f32,
    pub rooms: Vec<Hollow>,
    /// Rock left standing in them: where, in its frame, and how thick.
    pub pillars: Vec<(Vec2, f32)>,
}

impl Dig {
    /// A point of its frame on the map.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn to_map(&self, p: Vec2) -> Vec2 {
        self.at - self.facing.perp() * p.x + self.facing * p.y
    }

    /// A point of the map in its frame.
    pub fn to_dig(&self, p: Vec2) -> Vec2 {
        let d = p - self.at;
        Vec2::new(-d.dot(self.facing.perp()), d.dot(self.facing))
    }

    /// Distance to the rooms' walls at `p`, `y` up: negative inside.
    fn sdf(&self, p: Vec2, y: f32) -> f32 {
        let q = self.to_dig(p);
        let open = self
            .rooms
            .iter()
            .map(|r| {
                let d = ((q - (r.min + r.max) * 0.5).abs() - (r.max - r.min) * 0.5).max_element();
                let mid = self.floor + (r.floor + r.roof) * 0.5;
                d.max((y - mid).abs() - (r.roof - r.floor) * 0.5)
            })
            .fold(f32::INFINITY, f32::min);
        // (Wider at the foot and the head.)
        self.pillars
            .iter()
            .map(|&(c, r)| r + 0.12 * (y - self.floor - 3.0).abs() - q.distance(c))
            .fold(open, f32::max)
    }

    /// Its rooms' corners, on the map.
    pub fn corners(&self) -> impl Iterator<Item = Vec2> + '_ {
        self.rooms.iter().flat_map(|r| {
            [
                r.min,
                r.max,
                Vec2::new(r.min.x, r.max.y),
                Vec2::new(r.max.x, r.min.y),
            ]
            .map(|p| self.to_map(p))
        })
    }

    /// Its rooms' corners, on the map.
    pub fn corners(&self) -> impl Iterator<Item = Vec2> + '_ {
        self.rooms.iter().flat_map(|r| {
            [
                r.min,
                r.max,
                Vec2::new(r.min.x, r.max.y),
                Vec2::new(r.max.x, r.min.y),
            ]
            .map(|p| self.to_map(p))
        })
    }

    /// How far `p` is from its rooms, seen from above (negative: over one).
    pub fn distance(&self, p: Vec2) -> f32 {
        let q = self.to_dig(p);
        self.rooms
            .iter()
            .map(|r| ((q - (r.min + r.max) * 0.5).abs() - (r.max - r.min) * 0.5).max_element())
            .fold(f32::INFINITY, f32::min)
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

/// A flat-topped rock standing on the flats, steep all round but for the
/// ramp up one side.
#[derive(Clone, Copy, Debug)]
pub struct Butte {
    pub center: Vec2,
    pub radius: f32,
    pub height: f32,
    /// The way up: out from the middle this way.
    pub ramp: Vec2,
    pub phase: f32,
}

/// How far a butte's ramp runs out from its rim (m per m of height): 15
/// degrees.
const BUTTE_RUN: f32 = 3.7;
/// Its ramp's half-width.
const BUTTE_RAMP: f32 = 4.5;

impl Butte {
    /// Its rim's distance from the middle, this way out.
    pub fn rim(&self, dir: Vec2) -> f32 {
        let a = dir.y.atan2(dir.x);
        self.radius * (1.0 + 0.16 * (3.0 * a + self.phase).sin() + 0.07 * (5.0 * a).sin())
    }

    /// Its ground's height at `p`.
    pub fn height_at(&self, p: Vec2) -> f32 {
        let d = p - self.center;
        let r = d.length();
        if r < 1e-3 {
            return self.height;
        }
        let rim = self.rim(d / r);
        let body = self.height * (1.0 - smoothstep(rim, rim + 2.2, r));
        // The ramp: down from the rim along `ramp`, its sides falling away.
        let along = d.dot(self.ramp);
        let across = d.perp_dot(self.ramp).abs();
        let rr = self.rim(self.ramp);
        let run = self.height * BUTTE_RUN;
        let ramp = if along > 0.0 {
            self.height
                * (1.0 - ((along - rr) / run).clamp(0.0, 1.0))
                * (1.0 - smoothstep(BUTTE_RAMP, BUTTE_RAMP + 2.5, across))
        } else {
            0.0
        };
        body.max(ramp)
    }

    /// Its way up: from the ramp's foot to the top.
    pub fn way(&self) -> (Vec2, Vec2) {
        let rr = self.rim(self.ramp);
        (
            self.center + self.ramp * (rr + self.height * BUTTE_RUN + 3.0),
            self.center + self.ramp * (rr * 0.3),
        )
    }

    /// Its outline's distance from `p` (negative inside), ramp and all.
    pub fn distance(&self, p: Vec2) -> f32 {
        let d = p - self.center;
        let r = d.length().max(1e-3);
        let body = r - self.rim(d / r) - 2.2;
        let (a, b) = self.way();
        body.min(seg(p, a, b).0 - BUTTE_RAMP - 2.5)
    }
}

/// A fin of rock standing out of the ground, `height` m, from `a` to `b`.
#[derive(Clone, Copy, Debug)]
pub struct Fin {
    pub a: Vec2,
    pub b: Vec2,
    pub height: f32,
}

impl Fin {
    fn height_at(&self, p: Vec2) -> f32 {
        let (d, t) = seg(p, self.a, self.b);
        // Ragged along the top, tapering at the ends.
        let h = self.height
            * (0.75 + 0.25 * (t * 19.0 + self.a.x).sin())
            * smoothstep(0.0, 0.15, t.min(1.0 - t) + 0.05);
        h * (1.0 - smoothstep(1.2, 3.6, d))
    }
}

/// A great rock standing over an escarpment's top, its front rising behind
/// the Face: its sides at its front, how far it flares west as it runs back
/// (from `flare_z`, over 45 m), its wobble's phase.
#[derive(Clone, Copy, Debug)]
pub struct Crag {
    pub w: f32,
    pub e: f32,
    pub flare: f32,
    pub flare_z: f32,
    pub phase: f32,
}

impl Crag {
    /// Its west side (x), at `z` (`top`: the arena's north edge).
    pub fn west(&self, z: f32, top: f32) -> f32 {
        let z = z.min(top);
        self.w - self.flare * smoothstep(self.flare_z, self.flare_z + 45.0, z)
            + 1.6 * (z * 0.13 + self.phase).sin()
            + 1.0 * (z * 0.31).sin()
    }

    /// Its east side (x), at `z`.
    pub fn east(&self, z: f32, top: f32) -> f32 {
        let z = z.min(top);
        self.e + 1.5 * (z * 0.11 + 1.0 + self.phase).sin() + 0.8 * (z * 0.27).sin()
    }
}

/// A long cliff (the Face) along the north of an arena `size` across, the
/// mesa running back behind it, its west end a slope; screes where the Face
/// has fallen in, Crags standing over it.
#[derive(Clone, Debug, Default)]
pub struct Scarp {
    pub size: f32,
    /// How far back the Face rises over, and a scree's slope runs out.
    pub rise: f32,
    pub scree_run: f32,
    /// The Face's foot, west to east (x, z), eased between.
    pub foot: Vec<(f32, f32)>,
    /// The mesa's west end slopes down to the floor between these (x).
    pub west: (f32, f32),
    /// Each from, to (x).
    pub screes: Vec<(f32, f32)>,
    pub crags: Vec<Crag>,
}

impl Scarp {
    /// The Face's foot at `x` (z).
    pub fn face(&self, x: f32) -> f32 {
        // (Straight where it fell in: the scree's slope mustn't lean.)
        let wobble = 0.8 * (0.5 * x).sin() * (0.13 * x).sin() * (1.0 - self.scree(x));
        for w in self.foot.windows(2) {
            let ((x0, z0), (x1, z1)) = (w[0], w[1]);
            if x <= x1 {
                let t = smoothstep(x0, x1, x);
                return z0 + (z1 - z0) * t + wobble;
            }
        }
        self.foot[self.foot.len() - 1].1
    }

    /// How much scree (0..1) there is at `x`, in place of the cliff.
    pub fn scree(&self, x: f32) -> f32 {
        self.screes
            .iter()
            .map(|&(a, b)| {
                smoothstep(a - 4.0, a + 6.0, x) * (1.0 - smoothstep(b - 6.0, b + 4.0, x))
            })
            .fold(0.0, f32::max)
    }

    /// How far into a Crag a point is (m; negative: outside). Its front
    /// rises behind the mesa's own face.
    pub fn crag_depth(&self, p: Vec2) -> f32 {
        let front = p.y - self.face(p.x) - self.rise;
        self.crags
            .iter()
            .map(|m| {
                (p.x - m.west(p.y, self.size))
                    .min(m.east(p.y, self.size) - p.x)
                    .min(front)
            })
            .fold(f32::NEG_INFINITY, f32::max)
    }

    /// How much of a Crag (0..1) stands over a point.
    pub fn crag(&self, p: Vec2) -> f32 {
        smoothstep(0.0, 3.0, self.crag_depth(p))
    }

    /// How high the mesa stands at a point, before anything is cut out
    /// of it, 0..1.
    pub fn mesa(&self, p: Vec2) -> f32 {
        let f = self.face(p.x);
        let cliff = smoothstep(f, f + self.rise, p.y);
        let slope = ((p.y - (f - self.scree_run)) / (self.scree_run + self.rise)).clamp(0.0, 1.0);
        let s = self.scree(p.x);
        let across = cliff + (slope - cliff) * s;
        let (w0, w1) = self.west;
        let west = ((p.x - w0) / (w1 - w0)).clamp(0.0, 1.0);
        across * west
    }
}

/// A heap: flat-topped, its outline lobed, its sides running out `run` m.
#[derive(Clone, Copy, Debug)]
pub struct Mound {
    pub at: Vec2,
    pub radius: f32,
    pub height: f32,
    pub run: f32,
}

impl Mound {
    pub fn height_at(&self, p: Vec2) -> f32 {
        let d = p - self.at;
        let a = d.y.atan2(d.x);
        let r = self.radius;
        let edge = r + 0.25 * r * (3.0 * a + self.at.x).sin() + 0.12 * r * (5.0 * a + 1.0).sin();
        self.height * (1.0 - smoothstep(edge, edge + self.run, d.length()))
    }
}

/// A `Land` as a solid, with `heaps` on it.
struct LandRock<'a, F> {
    land: &'a Land,
    heaps: F,
}

impl<F: Fn(Vec2) -> f32> Rock for LandRock<'_, F> {
    fn column(&self, p: Vec2, ys: &[f32], out: &mut [f32]) {
        let land = self.land;
        let s = land.plateau(p);
        let heap = (self.heaps)(p);
        // (Far from a cliff, there's only the one level.)
        let near = s.abs() < 8.0;
        let only = land.level(p, s, if s < 0.0 { 1.0 } else { 0.0 }) + heap;
        let (top, foot) = if near {
            (land.level(p, s, 1.0) + heap, land.level(p, s, 0.0) + heap)
        } else {
            (only, only)
        };
        let digs: Vec<&Dig> = land.digs.iter().filter(|d| d.distance(p) < 4.0).collect();
        for (&y, d) in ys.iter().zip(out) {
            *d = y - only;
            if near {
                // Beds of rock, some worn back further than others.
                let bed = 0.7 * noise2(y / 1.3, p.dot(Vec2::ONE) / 40.0, 61)
                    + 0.5 * noise2(p.x / 4.0 + y / 9.0, p.y / 4.0, 62);
                *d = (s + bed - 0.6).max(y - top).min(y - foot);
            }
            for dig in &digs {
                *d = d.max(-dig.sdf(p, y));
            }
        }
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
    /// Haul ramps down into the washes: from the top to the floor, and half
    /// their width.
    pub hauls: Vec<(Vec2, Vec2, f32)>,
    /// Crossings of the washes: down one bank and up the other.
    pub fords: Vec<(Vec2, Vec2, f32)>,
    pub buttes: Vec<Butte>,
    pub scarp: Option<Scarp>,
    pub mounds: Vec<Mound>,
    pub fins: Vec<Fin>,
    pub bowls: Vec<Bowl>,
    pub dunes: Option<Dunes>,
    pub digs: Vec<Dig>,
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
        let s = self.plateau(p);
        self.level(p, s, 1.0 - smoothstep(0.0, EDGE_FALL, s))
    }

    /// The ground's height at `p` (`s`: the plateau's distance) with `up` of
    /// the high ground under it (1: its top; 0: the floor below).
    fn level(&self, p: Vec2, s: f32, up: f32) -> f32 {
        let plaza = self
            .platforms
            .iter()
            .map(|pl| pl.height_at(p))
            .fold(0.0, f32::max);
        let mut h = self.base(p) + plaza;
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
        h += (bowls - self.wash_cut(p)) * low + self.butte(p) + self.fin(p) + self.mound(p);
        self.graded(p, h)
    }

    /// The ground as a solid, `size` across, in cubes `cell` across, with
    /// `heaps` on it: the high ground's cliffs stand sheer, their beds worn
    /// back by turns.
    pub fn surface(&self, size: f32, cell: f32, heaps: impl Fn(Vec2) -> f32) -> Surface {
        let rock = LandRock { land: self, heaps };
        let edge = 2.0 * cell;
        Surface::of(
            &rock,
            Vec3::new(-edge, -8.0, -edge),
            Vec3::new(size + edge, self.high.height + 10.0, size + edge),
            cell,
        )
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

    /// The ground with the haul ramps and the crossings cut into the washes.
    pub fn graded(&self, p: Vec2, g: f32) -> f32 {
        let mut g = g;
        for &(s, e, half) in &self.hauls {
            let len = s.distance(e);
            let u = (e - s) / len;
            let rel = p - s;
            let (a, c) = (rel.dot(u), rel.perp_dot(u).abs());
            if a < -2.0 || a > len + 6.0 || c > half + 3.0 {
                continue;
            }
            let floor = -self.wash_cut(e);
            let h = floor * (a / len).clamp(0.0, 1.0);
            let k = (1.0 - smoothstep(half, half + 2.5, c))
                * (1.0 - smoothstep(len + 2.0, len + 6.0, a));
            g += (h - g) * k;
        }
        for &(s, e, half) in &self.fords {
            let len = s.distance(e);
            let u = (e - s) / len;
            let rel = p - s;
            let (a, c) = (rel.dot(u), rel.perp_dot(u).abs());
            if a < -3.0 || a > len + 3.0 || c > half + 6.0 {
                continue;
            }
            // Down from either top to the floor, flat across the floor; its
            // sides laid back, not cut sheer.
            let floor = -self.wash_cut(s.lerp(e, 0.5));
            let h = floor * (a.min(len - a) / FORD_RUN).clamp(0.0, 1.0);
            let k = (1.0 - smoothstep(half, half + 6.0, c))
                * smoothstep(-3.0, 0.0, a)
                * smoothstep(-3.0, 0.0, len - a);
            g += (h - g) * k;
        }
        g
    }

    /// How high the fins stand at `p`.
    pub fn fin(&self, p: Vec2) -> f32 {
        self.fins.iter().map(|f| f.height_at(p)).fold(0.0, f32::max)
    }

    /// How high the heaps stand at `p`.
    pub fn mound(&self, p: Vec2) -> f32 {
        self.mounds.iter().map(|m| m.height_at(p)).sum()
    }

    /// How high the buttes stand at `p`.
    pub fn butte(&self, p: Vec2) -> f32 {
        self.buttes
            .iter()
            .map(|b| b.height_at(p))
            .fold(0.0, f32::max)
    }

    /// The nearest wash to `p`: how far its line is, and half its floor's
    /// width there.
    pub fn wash_near(&self, p: Vec2) -> (f32, f32) {
        self.washes
            .iter()
            .map(|w| {
                let (d, s) = w.near(p);
                (d, w.half(s))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .unwrap_or((f32::INFINITY, 0.0))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn tunnel(length: f32) -> Vec<Hollow> {
        let room = |min: Vec2, max: Vec2| Hollow {
            min,
            max,
            floor: 0.0,
            roof: 6.0,
        };
        // A way in, and a hall off to its right at the end.
        vec![
            room(Vec2::new(-5.0, -6.0), Vec2::new(5.0, length)),
            room(Vec2::new(5.0, length - 16.0), Vec2::new(25.0, length)),
        ]
    }

    /// Two digs on a cliff that turns a corner, each square to its own
    /// stretch, and one in a mesa standing apart: all three have a floor
    /// to drive in along under a roof, and the rock's top over it.
    #[test]
    fn digs_go_anywhere_facing_any_way() {
        let line = vec![
            Vec2::new(-40.0, 120.0),
            Vec2::new(90.0, 100.0),
            Vec2::new(150.0, -40.0),
        ];
        // The high side's on the left, so in is to the left.
        let into = |k: usize| (line[k + 1] - line[k]).normalize().perp();
        let dig = |at: Vec2, facing: Vec2| Dig {
            at,
            facing,
            floor: 0.0,
            rooms: tunnel(34.0),
            pillars: Vec::new(),
        };
        let mesa = Vec2::new(52.0, 50.0);
        let face = Vec2::from_angle(2.1);
        let mut land = Land::default();
        land.high.height = 12.0;
        land.high.slabs = vec![
            Slab::Behind { line: line.clone() },
            Slab::Island {
                center: mesa,
                radius: 34.0,
                phase: 0.0,
            },
        ];
        land.digs = vec![
            dig(line[0].lerp(line[1], 0.5), into(0)),
            dig(line[1].lerp(line[2], 0.5), into(1)),
            dig(mesa - face * 38.0, face),
        ];
        let s = land.surface(180.0, 1.0, |_| 0.0);
        for (k, d) in land.digs.iter().enumerate() {
            let at = |x: f32, y: f32| s.floors(d.to_map(Vec2::new(x, y)));
            // The hall, off to the right of the way in.
            for (x, y) in [(0.0, 14.0), (0.0, 26.0), (16.0, 26.0)] {
                let f = at(x, y);
                assert_eq!(f.len(), 2, "dig {k} at {x}, {y}: {f:?}");
                assert!((f[0] - 12.0).abs() < 0.5, "dig {k}: top {f:?}");
                assert!(f[1].abs() < 0.3, "dig {k}: floor {f:?}");
            }
            // Nothing to the left of the way in, and a level way to the mouth.
            assert_eq!(at(-12.0, 22.0).len(), 1, "dig {k}");
            let mut y = -5.0;
            while y < 30.0 {
                let f = at(0.0, y);
                let p = d.to_map(Vec2::new(0.0, y));
                assert!(
                    f.last().is_some_and(|f| f.abs() < 0.3),
                    "dig {k} at {y} ({p}): {f:?}"
                );
                y += 1.0;
            }
        }
    }

    #[test]
    fn a_cliff_has_its_high_side_on_the_left() {
        let line = [Vec2::ZERO, Vec2::new(10.0, 0.0), Vec2::new(10.0, -10.0)];
        for (p, want) in [
            (Vec2::new(5.0, 3.0), -3.0),
            (Vec2::new(5.0, -3.0), 3.0),
            (Vec2::new(14.0, -5.0), -4.0),
            (Vec2::new(-20.0, 2.0), -2.0),
            (Vec2::new(12.0, 2.0), -(8.0f32.sqrt())),
        ] {
            let d = behind(&line, p);
            assert!((d - want).abs() < 1e-3, "{p}: {d}, want {want}");
        }
    }
}
