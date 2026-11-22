//! A desert's skeleton: the map in placeholders, before anything's built.
//!
//! First its **form**, the shape of the land: an erg (a sea of dunes with
//! mesas standing out of it), canyonlands (a plateau cut by a network of
//! canyons), a basin (a bowl ringed by high ground, passes cut through the
//! ring) or an escarpment (a long cliff between high ground and low, cut
//! back by a canyon). Every form is two levels, the desert floor and the
//! high ground `high` m over it, the high ground's shape a union of
//! `Slab`s with `Channel`s and parks cut out of it (`plateau`, a signed
//! distance). Wherever the two levels meet is a cliff (sculpted when it's
//! dressed), all round but where a `Ramp` comes up it; ramps are put on
//! the cliff lines by rule, every stretch of high ground getting at least
//! one.
//!
//! Then the **landmark**'s slot (what the fight's about, the Hold site):
//! open ground kept for it out on the floor, its kind picked (spires, a
//! crater, an arch) but nothing of it built. Then each side's **spawns**,
//! picked on the greybox's nav grid so every side's drive to the landmark
//! is about as long (`place_bases`). Then the **lots**: "a cluster of
//! spires here", "fins of rock", "a boulder field", each with the reach
//! its filler keeps to.

use std::collections::HashMap;
use std::f32::consts::TAU;

use glam::Vec2;

use super::Palette;
use crate::ai::nav::NavGrid;
use crate::map::procgen::mine::{line_distance, seg, smoothstep};
use crate::map::valley::{noise, noise2};
use crate::rng::Rng;

/// Each side's spawns: kept clear and level this far round.
pub const PAD: f32 = 24.0;
/// How far a ramp runs out from the rim, per m of height (15 degrees).
pub const RAMP_RUN: f32 = 3.7;
/// A ramp's half-width, its sides falling away past it.
pub const RAMP_HALF: f32 = 7.0;
/// The high ground falls to the floor over this far out from its edge
/// (under the cliff face, which stands over it).
pub const EDGE_FALL: f32 = 2.2;
/// The cliff's foot line: this far out from the high ground's edge.
pub const FOOT: f32 = 1.6;
/// A dry wash: its floor's half-width, its banks, how deep it's cut.
pub const WASH_FLOOR: f32 = 5.0;
pub const WASH_BANK: f32 = 10.0;
pub const WASH_DEPTH: f32 = 3.0;

/// The shape of the land.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Form {
    /// A sea of dunes, mesas and buttes standing out of it.
    Erg,
    /// A plateau cut by a network of canyons, opening out round the middle.
    Canyonlands,
    /// A bowl ringed by high ground, passes cut through the ring.
    Basin,
    /// A long cliff between the high ground and the floor, a canyon cut
    /// back into it, outliers standing off it.
    Escarpment,
}

impl Form {
    pub const ALL: [Form; 4] = [Form::Erg, Form::Canyonlands, Form::Basin, Form::Escarpment];
}

/// A piece of the high ground's shape.
#[derive(Clone, Debug)]
enum Slab {
    /// Everywhere (the canyonlands' plateau, cut by its canyons).
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

/// What stands at the landmark's slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LandmarkKind {
    /// A ring of sandstone spires.
    Needles,
    /// An old impact crater: a bowl, its rim heaped with blocks.
    Crater,
    /// A great natural arch spanning the open ground.
    Arch,
    /// A shield generator (#337): the holder's standard model, its ring
    /// `GENERATOR_RING` round, on a graded pad out to its precinct.
    Generator,
}

/// A generator's ring wall: this far round from its middle (a city's
/// landmark's is 21 to 26 m; any less and the Imperium's plant inside
/// crowds its doors shut).
pub const GENERATOR_RING: f32 = 23.0;
/// A generator's precinct (where its pylons or obelisks stand): this far.
pub const GENERATOR_PRECINCT: f32 = GENERATOR_RING + 16.0;

/// The landmark's slot: where it stands, the open ground kept round it,
/// how far its filler may reach.
#[derive(Clone, Copy, Debug)]
pub struct Landmark {
    pub at: Vec2,
    pub kind: LandmarkKind,
    /// Nothing solid at tank height within this.
    pub clear: f32,
    /// Its filler keeps to this.
    pub reach: f32,
    /// The arch's span runs across this.
    pub facing: Vec2,
}

impl Landmark {
    /// The crater's ground: a bowl, a low rim round it.
    fn ground(&self, p: Vec2) -> f32 {
        if self.kind != LandmarkKind::Crater {
            return 0.0;
        }
        let d = self.at.distance(p);
        let bowl = -2.2 * (1.0 - smoothstep(0.0, self.clear + 5.0, d));
        let rim = 1.5 * (1.0 - smoothstep(0.0, 8.0, (d - (self.clear + 9.0)).abs()));
        bowl + rim
    }
}

/// What a lot holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LotKind {
    /// A cluster of hoodoos.
    Spires,
    /// Fins of rock standing out of the sand.
    Fins,
    /// A field of boulders.
    Boulders,
}

#[derive(Clone, Copy, Debug)]
pub struct Lot {
    pub kind: LotKind,
    pub at: Vec2,
    pub reach: f32,
}

/// A desert laid out in placeholders.
#[derive(Clone, Debug)]
pub struct Skeleton {
    pub size: f32,
    pub form: Form,
    /// The high ground's height over the floor.
    pub high: f32,
    slabs: Vec<Slab>,
    pub channels: Vec<Channel>,
    /// Open ground cut out of the high ground round the landmark.
    parks: Vec<(Vec2, f32)>,
    pub ramps: Vec<Ramp>,
    pub landmark: Landmark,
    /// Each side's spawns' middle (empty until `place_bases`).
    pub bases: Vec<Vec2>,
    pub lots: Vec<Lot>,
    /// Dry washes on the floor, end to end past the edge.
    pub washes: Vec<Vec<Vec2>>,
    pub wind: Vec2,
    wave: f32,
    pub dune_h: f32,
    phase: f32,
    pub palette: Palette,
}

/// Smooth max and min, blending over `k`.
fn smax(a: f32, b: f32, k: f32) -> f32 {
    let h = (0.5 + 0.5 * (a - b) / k).clamp(0.0, 1.0);
    b + (a - b) * h + k * h * (1.0 - h)
}

fn smin(a: f32, b: f32, k: f32) -> f32 {
    -smax(-a, -b, k)
}

/// A line wandering through `via` in turn, wiggling up to `wiggle` rad
/// off its way each 10 m, smoothed.
fn meander(rng: &mut Rng, via: &[Vec2], wiggle: f32) -> Vec<Vec2> {
    let mut out = vec![via[0]];
    let mut at = via[0];
    for &goal in &via[1..] {
        for _ in 0..200 {
            if at.distance(goal) < 14.0 {
                break;
            }
            let to = (goal - at).normalize();
            at += Vec2::from_angle(rng.range(-wiggle, wiggle)).rotate(to) * 10.0;
            out.push(at);
        }
        at = goal;
        out.push(goal);
    }
    // Corners cut twice (Chaikin), ends kept.
    for _ in 0..2 {
        let mut s = vec![out[0]];
        for w in out.windows(2) {
            s.push(w[0].lerp(w[1], 0.25));
            s.push(w[0].lerp(w[1], 0.75));
        }
        s.push(*out.last().unwrap());
        out = s;
    }
    out
}

impl Skeleton {
    /// A desert of form `form` for `sides` sides, laid out from `rng` (all
    /// but its spawns and lots: see `place_bases` and `place_lots`), with a
    /// shield generator's slot for its landmark if `generator`.
    pub fn generate(rng: &mut Rng, form: Form, generator: bool) -> Skeleton {
        let size = (rng.range(260.0, 320.0) / 2.0).round() * 2.0;
        let mid = Vec2::splat(size * 0.5);
        let kind = [
            LandmarkKind::Needles,
            LandmarkKind::Crater,
            LandmarkKind::Arch,
        ][rng.index(3)];
        // A generator wants a good deal more room: its ring, its plant
        // round it, its precinct, the camp round that.
        let (kind, clear, reach) = match generator {
            true => (
                LandmarkKind::Generator,
                GENERATOR_PRECINCT + 4.0,
                GENERATOR_PRECINCT + 16.0,
            ),
            false => (kind, 14.0, 28.0),
        };
        // How much further out than a natural landmark's the land is kept.
        let extra = reach - 28.0;
        let landmark = Landmark {
            at: mid + Vec2::from_angle(rng.range(0.0, TAU)) * rng.range(0.0, size * 0.1),
            kind,
            clear,
            reach,
            facing: Vec2::from_angle(rng.range(0.0, TAU)),
        };
        let mut sk = Skeleton {
            size,
            form,
            high: rng.range(8.0, 10.0),
            slabs: Vec::new(),
            channels: Vec::new(),
            parks: vec![(landmark.at, landmark.reach + rng.range(10.0, 18.0))],
            ramps: Vec::new(),
            landmark,
            bases: Vec::new(),
            lots: Vec::new(),
            washes: Vec::new(),
            wind: Vec2::from_angle(rng.range(0.0, TAU)),
            wave: rng.range(38.0, 60.0),
            dune_h: rng.range(1.0, 1.8),
            phase: rng.range(0.0, 1.0),
            palette: super::PALETTES[rng.index(super::PALETTES.len())],
        };
        let out = |rng: &mut Rng, dir: Vec2| {
            mid + dir * (size * 0.5 + 40.0) + dir.perp() * rng.range(-30.0, 30.0)
        };
        let lm = landmark.at;
        match form {
            Form::Erg => {
                // Bigger dunes, further apart.
                sk.dune_h = rng.range(2.2, 3.0);
                sk.wave = rng.range(50.0, 70.0);
                let want = rng.int(3, 5) as usize;
                sk.islands(rng, want, (12.0, 26.0));
                if rng.chance(0.5) {
                    sk.wash(rng);
                }
            }
            Form::Canyonlands => {
                sk.high = rng.range(6.5, 8.0);
                sk.slabs.push(Slab::Everywhere);
                // The main canyon through the middle, edge to edge.
                let a = Vec2::from_angle(rng.range(0.0, TAU));
                let b = Vec2::from_angle(rng.range(-0.5, 0.5)).rotate(-a);
                let via = [out(rng, a), lm, out(rng, b)];
                sk.channels.push(Channel {
                    line: meander(rng, &via, 0.5),
                    half: rng.range(10.0, 14.0),
                });
                // Side canyons off it (or off each other), across it, out
                // to the edge or ending blind, not running alongside
                // another.
                let want = rng.int(2, 4) as usize;
                let mut made = 0;
                for _ in 0..40 {
                    if made >= want {
                        break;
                    }
                    let line = sk.channels[rng.index(sk.channels.len())].line.clone();
                    let k = rng.range(1.0, line.len() as f32 - 2.0) as usize;
                    let from = line[k];
                    let inside = from.min_element() > 30.0 && from.max_element() < size - 30.0;
                    if from.distance(lm) < 55.0 + extra || !inside {
                        continue;
                    }
                    let along = (line[k + 1] - line[k - 1]).normalize_or(Vec2::X);
                    let side = if rng.chance(0.5) {
                        along.perp()
                    } else {
                        -along.perp()
                    };
                    let dir = Vec2::from_angle(rng.range(-0.6, 0.6)).rotate(side);
                    let end = if rng.chance(0.3) {
                        from + dir * rng.range(60.0, 100.0)
                    } else {
                        out(rng, dir)
                    };
                    let branch = meander(rng, &[from, end], 0.5);
                    // Clear of the other canyons once it's left its own,
                    // and of the middle.
                    let apart = branch.iter().filter(|q| q.distance(from) > 60.0).all(|&q| {
                        sk.channels
                            .iter()
                            .all(|c| line_distance(&c.line, q) > c.half + 30.0)
                    }) && line_distance(&branch, lm) > 50.0 + extra;
                    if !apart {
                        continue;
                    }
                    sk.channels.push(Channel {
                        line: branch,
                        half: rng.range(8.0, 12.0),
                    });
                    made += 1;
                }
            }
            Form::Basin => {
                sk.dune_h = rng.range(1.2, 2.0);
                let radius = size * rng.range(0.33, 0.4);
                sk.slabs.push(Slab::Outside {
                    center: lm,
                    radius,
                    phase: rng.range(0.0, TAU),
                });
                sk.parks.clear();
                // Passes out through the ring.
                let n = rng.int(2, 4);
                let a0 = rng.range(0.0, TAU);
                for k in 0..n {
                    let dir =
                        Vec2::from_angle(a0 + k as f32 * TAU / n as f32 + rng.range(-0.4, 0.4));
                    let via = [lm + dir * radius * 0.6, out(rng, dir)];
                    sk.channels.push(Channel {
                        line: meander(rng, &via, 0.35),
                        half: rng.range(8.0, 11.0),
                    });
                }
                let want = rng.int(0, 2) as usize;
                sk.islands(rng, want, (10.0, 16.0));
                if rng.chance(0.4) {
                    sk.wash(rng);
                }
            }
            Form::Escarpment => {
                sk.dune_h = rng.range(1.2, 2.2);
                let into = Vec2::from_angle(rng.range(0.0, TAU));
                let at = lm + into * (rng.range(55.0, 80.0) + extra);
                sk.slabs.push(Slab::Beyond {
                    at,
                    into,
                    amp: rng.range(6.0, 14.0),
                    wave: rng.range(25.0, 45.0),
                    phase: rng.range(0.0, TAU),
                });
                // A canyon cut back into it, perhaps out the far side.
                if rng.chance(0.7) {
                    let along = into.perp() * rng.range(-60.0, 60.0);
                    let from = at + along - into * 20.0;
                    let end = if rng.chance(0.5) {
                        from + Vec2::from_angle(rng.range(-0.4, 0.4)).rotate(into)
                            * rng.range(70.0, 110.0)
                    } else {
                        out(rng, into)
                    };
                    sk.channels.push(Channel {
                        line: meander(rng, &[from, end], 0.55),
                        half: rng.range(9.0, 12.0),
                    });
                }
                let want = rng.int(1, 3) as usize;
                sk.islands(rng, want, (10.0, 18.0));
                if rng.chance(0.4) {
                    sk.wash(rng);
                }
            }
        }
        sk.place_ramps(rng);
        sk
    }

    /// Up to `want` mesas and buttes on the floor, apart, off the
    /// landmark's ground, in the arena.
    fn islands(&mut self, rng: &mut Rng, want: usize, (lo, hi): (f32, f32)) {
        let mut placed = 0;
        for _ in 0..300 {
            if placed >= want {
                break;
            }
            let radius = rng.range(lo, hi);
            let reach = radius * 1.25;
            let c = Vec2::new(
                rng.range(reach + 8.0, self.size - reach - 8.0),
                rng.range(reach + 8.0, self.size - reach - 8.0),
            );
            if self.landmark.at.distance(c) < self.landmark.reach + reach + 30.0
                || self.plateau(c) < reach + 30.0
            {
                continue;
            }
            self.slabs.push(Slab::Island {
                center: c,
                radius,
                phase: rng.range(0.0, TAU),
            });
            placed += 1;
        }
    }

    /// A dry wash winding across the floor, clear of the landmark.
    fn wash(&mut self, rng: &mut Rng) {
        let s = self.size;
        let lm = self.landmark;
        for _ in 0..12 {
            let off = Vec2::from_angle(rng.range(0.0, TAU));
            let at = lm.at + off * rng.range(lm.reach + 22.0, lm.reach + 50.0);
            let dir = Vec2::from_angle(rng.range(-0.45, 0.45)).rotate(off.perp());
            let mid = Vec2::splat(s * 0.5);
            let a = mid + (at - mid) - dir * s;
            let b = mid + (at - mid) + dir * s;
            let line = meander(rng, &[a, at, b], 0.4);
            if line_distance(&line, lm.at) > lm.reach + WASH_FLOOR + WASH_BANK + 4.0 {
                self.washes.push(line);
                return;
            }
        }
    }

    /// The high ground's signed distance at `p`: negative on it.
    pub fn plateau(&self, p: Vec2) -> f32 {
        // Joined and cut with the corners rounded off, so where a canyon
        // meets a cliff there's no sliver of rock left standing.
        let mut s = self
            .slabs
            .iter()
            .map(|sl| sl.sdf(p))
            .fold(1.0e4, |a, b| smin(a, b, 8.0));
        for c in &self.channels {
            s = smax(s, c.half - line_distance(&c.line, p), 8.0);
        }
        for &(c, r) in &self.parks {
            s = smax(s, r - c.distance(p), 8.0);
        }
        s
    }

    /// How much of the high ground's height there is at `p` (1 on it, 0 on
    /// the floor).
    fn up(&self, s: f32) -> f32 {
        1.0 - smoothstep(0.0, EDGE_FALL, s)
    }

    /// On the high ground (well in from its edge).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn on_high(&self, p: Vec2) -> bool {
        self.plateau(p) < -2.0
    }

    /// How deep the washes are cut at `p`.
    pub fn wash_cut(&self, p: Vec2) -> f32 {
        self.washes
            .iter()
            .map(|w| {
                let d = line_distance(w, p);
                WASH_DEPTH * (1.0 - smoothstep(WASH_FLOOR, WASH_FLOOR + WASH_BANK, d))
            })
            .fold(0.0, f32::max)
    }

    pub fn wash_distance(&self, p: Vec2) -> f32 {
        self.washes
            .iter()
            .map(|w| line_distance(w, p))
            .fold(f32::INFINITY, f32::min)
    }

    /// The dunes at `p`, before anything flattens them.
    fn dunes(&self, p: Vec2) -> f32 {
        let warp = 0.6 * noise2(p.x / 70.0, p.y / 70.0, 81);
        let u = p.dot(self.wind) / self.wave + self.phase + warp;
        let f = u - u.floor();
        // A long gentle windward side, a shorter lee.
        let dune = smoothstep(0.0, 0.68, f) * (1.0 - smoothstep(0.68, 1.0, f));
        let along = 0.5 + 0.5 * noise2(p.x / 55.0, p.y / 55.0, 82);
        let ripple = 0.06 * (p.dot(self.wind) * 1.3 + 2.0 * noise(p.x / 9.0 + 500.0, 83)).sin();
        self.dune_h * dune * along + ripple
    }

    /// How much of the dunes is left at `p` (`s`: the plateau's distance):
    /// none on the spawns, round the landmark, on the ramps, by the cliffs
    /// or in a wash.
    fn drift(&self, p: Vec2, s: f32) -> f32 {
        let bases = self
            .bases
            .iter()
            .map(|&b| smoothstep(PAD - 4.0, PAD + 14.0, b.distance(p)))
            .fold(1.0, f32::min);
        let lm = &self.landmark;
        let hold = smoothstep(lm.clear + 4.0, lm.reach + 16.0, lm.at.distance(p));
        let ramps = self
            .ramps
            .iter()
            .map(|r| smoothstep(RAMP_HALF + 2.0, RAMP_HALF + 12.0, r.distance(p)))
            .fold(1.0, f32::min);
        let cliff = smoothstep(2.0, 10.0, s.abs());
        let wash = smoothstep(
            WASH_FLOOR + WASH_BANK,
            WASH_FLOOR + WASH_BANK + 10.0,
            self.wash_distance(p),
        );
        bases.min(hold).min(ramps).min(cliff).min(wash)
    }

    /// The ground's height at `(x, z)`.
    pub fn ground(&self, x: f32, z: f32) -> f32 {
        let p = Vec2::new(x, z);
        let s = self.plateau(p);
        let up = self.up(s);
        let mut h = self.high * up;
        for r in &self.ramps {
            h = h.max(r.height(p, self.high));
        }
        let low = 1.0 - up;
        let sand = self.dunes(p) * self.drift(p, s) * (0.3 + 0.7 * low);
        h + sand - self.wash_cut(p) * low + self.landmark.ground(p) * low
    }

    /// The ground out past the edge: the arena's carried on from its edge,
    /// rising into hills and big dunes.
    pub fn scenery_ground(&self, x: f32, z: f32) -> f32 {
        let s = self.size;
        let p = Vec2::new(x, z);
        let q = (p - Vec2::splat(s * 0.5)).abs() - Vec2::splat(s * 0.5);
        let d = q.max(Vec2::ZERO).length() + q.x.max(q.y).min(0.0);
        if d < -0.01 {
            return -10.0;
        }
        let e = p.clamp(Vec2::ZERO, Vec2::splat(s));
        let base = self.ground(e.x, e.y);
        let big = 1.0 + (p.dot(self.wind) / (self.wave * 2.2) + self.phase).sin();
        let hills = smoothstep(6.0, 70.0, d)
            * (4.0 * big
                + 5.0 * ((x * 0.011).sin() * (z * 0.013).cos()).abs()
                + 8.0
                    * smoothstep(
                        0.55,
                        0.9,
                        ((x * 0.004 + 1.0).sin() * (z * 0.005).cos()).abs(),
                    ));
        base + hills
    }

    /// The cliffs' foot lines, the floor on the left going along (see
    /// `Map::rock_face`): traced round the high
    /// ground `FOOT` m out from its edge, a little past the arena's.
    pub fn cliff_lines(&self) -> Vec<Vec<Vec2>> {
        let lo = -14.0;
        let hi = self.size + 14.0;
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
    pub fn cliffs(&self) -> Vec<Vec<Vec2>> {
        let mut out = Vec::new();
        for line in self.cliff_lines() {
            let mut piece = Vec::new();
            for p in line {
                let gap = self.ramps.iter().any(|r| r.distance(p) < RAMP_HALF + 2.0);
                let far = p.min_element() < -12.0 || p.max_element() > self.size + 12.0;
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

    /// Ramps up the cliffs, by rule: every cliff line of any length in the
    /// arena gets one, and one more for each 140 m of it, spread out,
    /// where there's a run of open floor for it.
    fn place_ramps(&mut self, rng: &mut Rng) {
        let run = self.high * RAMP_RUN;
        let s = self.size;
        let inside = |p: Vec2, m: f32| p.min_element() > m && p.max_element() < s - m;
        for line in self.cliff_lines() {
            let mut sites = Vec::new();
            let mut len = 0.0;
            let mut walked = 0.0;
            for w in line.windows(3) {
                walked += w[0].distance(w[1]);
                if walked < 6.0 {
                    continue;
                }
                walked = 0.0;
                let p = w[1];
                if !inside(p, 18.0) {
                    continue;
                }
                len += 6.0;
                let dir = (w[2] - w[0]).normalize_or(Vec2::X);
                let out = Vec2::new(-dir.y, dir.x);
                let top = p - out * (FOOT + 5.0);
                if self.plateau(top) > -3.0 || self.plateau(top - out * 8.0) > -3.0 {
                    continue;
                }
                // Straight out from the cliff if there's the floor for it,
                // else slanting down along it (down a canyon's wall).
                let slants = [0.0f32, 0.5, -0.5, 0.9, -0.9, 1.15, -1.15];
                let found = slants.iter().find_map(|&slant| {
                    let d = Vec2::from_angle(slant).rotate(out);
                    let side = if d.perp().dot(out) >= 0.0 {
                        d.perp()
                    } else {
                        -d.perp()
                    };
                    let foot = p + d * run;
                    let open = (1..=10).all(|k| {
                        let t = k as f32 / 10.0;
                        let q = p + d * (run * t);
                        let centre = t < 0.35 || self.plateau(q) > FOOT + 1.5;
                        let outer =
                            t < 0.2 || self.plateau(q + side * (RAMP_HALF + 2.0)) > FOOT + 2.0;
                        let inner = slant != 0.0
                            || t < 0.2
                            || self.plateau(q - side * (RAMP_HALF + 2.0)) > FOOT + 2.0;
                        // Room to drive on past it.
                        let lane = t < 0.5 || self.plateau(q + side * (RAMP_HALF + 10.0)) > 0.5;
                        centre && outer && inner && lane
                    });
                    let ok = open
                        && inside(foot, 12.0)
                        && self.landmark.at.distance(foot) > self.landmark.reach + 10.0
                        && self.wash_distance(foot) > WASH_FLOOR + WASH_BANK + 6.0;
                    ok.then_some(Ramp { foot, top })
                });
                if let Some(r) = found {
                    sites.push(r);
                }
            }
            if len < 30.0 || sites.is_empty() {
                continue;
            }
            let want = 1 + (len / 140.0) as usize;
            for _ in 0..want {
                let far: Vec<Ramp> = sites
                    .iter()
                    .copied()
                    .filter(|r| {
                        self.ramps
                            .iter()
                            .all(|o| o.foot.distance(r.foot) > 50.0 && o.top.distance(r.top) > 50.0)
                    })
                    .collect();
                if far.is_empty() {
                    break;
                }
                self.ramps.push(far[rng.index(far.len())]);
            }
        }
    }

    /// Kept clear of anything put down: the spawns, the landmark's open
    /// ground, the ramps.
    pub fn keep_clear(&self, p: Vec2) -> bool {
        self.bases.iter().any(|&b| b.distance(p) < PAD)
            || self.landmark.at.distance(p) < self.landmark.clear
            || self.ramps.iter().any(|r| r.distance(p) < RAMP_HALF + 3.0)
    }

    /// Where side spawns could go: level ground all on one level, off the
    /// ramps and washes, well out from the landmark.
    fn base_sites(&self) -> Vec<Vec2> {
        let s = self.size;
        let mut out = Vec::new();
        let m = PAD + 10.0;
        let mut z = m;
        while z <= s - m {
            let mut x = m;
            while x <= s - m {
                let p = Vec2::new(x, z);
                let g = self.ground(p.x, p.y);
                let ok = self.plateau(p).abs() > PAD + 3.0
                    && self.ramps.iter().all(|r| r.distance(p) > PAD + 6.0)
                    && self.wash_distance(p) > PAD + WASH_FLOOR + WASH_BANK
                    && self.landmark.at.distance(p) > s * 0.3
                    && (0..12).all(|k| {
                        let q = p + Vec2::from_angle(k as f32 * TAU / 12.0) * PAD;
                        (self.ground(q.x, q.y) - g).abs() < 2.5
                    });
                if ok {
                    out.push(p);
                }
                x += 10.0;
            }
            z += 10.0;
        }
        out
    }

    /// Each side's spawns, picked on `nav` (the greybox's, no spawns yet):
    /// far apart, each side's drive to the landmark about as long as the
    /// others'. False if there's no fair choice.
    pub fn place_bases(&mut self, rng: &mut Rng, sides: usize, nav: &NavGrid) -> bool {
        let mut sites = self.base_sites();
        for i in (1..sites.len()).rev() {
            sites.swap(i, rng.index(i + 1));
        }
        let lm = self.landmark.at;
        let drive = |p: Vec2| {
            let path = nav.path(p, lm);
            if path.is_empty() {
                return None;
            }
            let mut at = p;
            let mut len = 0.0;
            for q in path {
                len += at.distance(q);
                at = q;
            }
            Some(len)
        };
        let found: Vec<(Vec2, f32)> = sites
            .into_iter()
            .take(48)
            .filter_map(|p| drive(p).map(|l| (p, l)))
            .collect();
        let apart = self.size * if sides <= 2 { 0.5 } else { 0.38 };
        let mut best: Option<(f32, Vec<Vec2>)> = None;
        for &(first, len) in &found {
            let mut pick = vec![first];
            for &(p, l) in &found {
                if pick.len() >= sides {
                    break;
                }
                if (l - len).abs() / len.max(l) < 0.12 && pick.iter().all(|q| q.distance(p) > apart)
                {
                    pick.push(p);
                }
            }
            if pick.len() < sides {
                continue;
            }
            let spread = pick
                .iter()
                .enumerate()
                .flat_map(|(i, a)| pick[i + 1..].iter().map(move |b| a.distance(*b)))
                .fold(f32::MAX, f32::min);
            // Not always the farthest apart: any good spread will do.
            let score = spread * rng.range(0.85, 1.0);
            if best.as_ref().is_none_or(|(s, _)| score > *s) {
                best = Some((score, pick));
            }
        }
        match best {
            Some((_, b)) => {
                self.bases = b;
                true
            }
            None => false,
        }
    }

    /// The lots: clusters of spires, fins and boulder fields, each wholly
    /// on one level, off what's kept clear and apart.
    pub fn place_lots(&mut self, rng: &mut Rng) {
        let (spires, fins, boulders) = match self.form {
            Form::Erg => (rng.int(3, 5), rng.int(3, 5), rng.int(3, 5)),
            Form::Canyonlands => (rng.int(3, 5), rng.int(1, 3), rng.int(2, 4)),
            Form::Basin => (rng.int(3, 4), rng.int(2, 4), rng.int(3, 4)),
            Form::Escarpment => (rng.int(2, 4), rng.int(2, 4), rng.int(3, 5)),
        };
        let wants = [
            (LotKind::Spires, spires, (8.0, 13.0)),
            (LotKind::Fins, fins, (9.0, 12.0)),
            (LotKind::Boulders, boulders, (14.0, 22.0)),
        ];
        for (kind, want, (lo, hi)) in wants {
            let mut placed = 0;
            for _ in 0..400 {
                if placed >= want {
                    break;
                }
                let reach = rng.range(lo, hi);
                let at = Vec2::new(
                    rng.range(reach + 6.0, self.size - reach - 6.0),
                    rng.range(reach + 6.0, self.size - reach - 6.0),
                );
                let lm = &self.landmark;
                let ok = self.plateau(at).abs() > reach + 4.0
                    && lm.at.distance(at) > lm.reach + reach + 6.0
                    && self
                        .bases
                        .iter()
                        .all(|&b| b.distance(at) > PAD + reach + 4.0)
                    && self
                        .ramps
                        .iter()
                        .all(|r| r.distance(at) > reach + RAMP_HALF + 4.0)
                    && self.wash_distance(at) > reach + WASH_FLOOR
                    && self
                        .lots
                        .iter()
                        .all(|o| o.at.distance(at) > o.reach + reach + 8.0);
                if ok {
                    self.lots.push(Lot { kind, at, reach });
                    placed += 1;
                }
            }
        }
    }

    /// Where each side's spawns go: a grid over its pad.
    pub fn spawn_points(&self, side: usize) -> Vec<Vec2> {
        let c = self.bases[side];
        let r = PAD - 5.0;
        let step = 4.5;
        let n = (r / step) as i32;
        let mut out = Vec::new();
        for i in -n..=n {
            for j in -n..=n {
                let p = c + Vec2::new(i as f32, j as f32) * step;
                if p.distance(c) <= r {
                    out.push(p);
                }
            }
        }
        out
    }
}
