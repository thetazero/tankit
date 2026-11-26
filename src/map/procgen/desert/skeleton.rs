//! A desert's skeleton: the map in placeholders, before anything's built.
//!
//! First its **form**, the shape of the land: an erg (a sea of dunes with
//! mesas standing out of it), canyonlands (a plateau cut by a network of
//! canyons), a basin (a bowl ringed by high ground, passes cut through the
//! ring), an escarpment (a long cliff between high ground and low, cut
//! back by a canyon) or a salt flat (a dead level pan: its cover is ridges
//! of heaved salt, a channel sunk in it, an island or two of rock). Every form is two levels, the desert floor and the
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

use std::f32::consts::TAU;

use glam::Vec2;

use crate::ai::nav::NavGrid;
pub use crate::map::procgen::land::*;
use crate::map::procgen::mine::{line_distance, smoothstep};
pub use crate::map::procgen::skeleton::*;
use crate::rng::Rng;

/// Each side's spawns, in the open: kept clear and level this far round.
pub const PAD: f32 = 24.0;
/// How far a ramp runs out from the rim, per m of height (15 degrees).
pub const RAMP_RUN: f32 = 3.7;
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
    /// A level pan, open a long way: ridges of heaved salt lying across it,
    /// a brine channel sunk in it, an island or two of rock.
    SaltFlat,
}

impl Form {
    pub const ALL: [Form; 5] = [
        Form::Erg,
        Form::Canyonlands,
        Form::Basin,
        Form::Escarpment,
        Form::SaltFlat,
    ];
}

/// A generator's ring wall: this far round from its middle (a city's
/// landmark's is 21 to 26 m; any less and the Imperium's plant inside
/// crowds its doors shut).
pub const GENERATOR_RING: f32 = 23.0;
/// A generator's precinct (where its pylons or obelisks stand): this far.
pub const GENERATOR_PRECINCT: f32 = GENERATOR_RING + 16.0;

/// A line wandering through `via` in turn, wiggling up to `wiggle` rad
/// off its way each 10 m, smoothed.
pub(super) fn meander(rng: &mut Rng, via: &[Vec2], wiggle: f32) -> Vec<Vec2> {
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
    /// The dunes (every desert has some).
    pub fn dunes(&self) -> &Dunes {
        self.land.dunes.as_ref().expect("a desert's dunes")
    }

    fn dunes_mut(&mut self) -> &mut Dunes {
        self.land.dunes.as_mut().expect("a desert's dunes")
    }

    /// A desert of form `form` for `sides` sides, laid out from `rng` (all
    /// but its spawns and lots: see `place_bases` and `place_lots`), with a
    /// shield generator's slot for its landmark if `generator`.
    pub fn desert(rng: &mut Rng, form: Form, generator: bool) -> Skeleton {
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
        let at = mid + Vec2::from_angle(rng.range(0.0, TAU)) * rng.range(0.0, size * 0.1);
        let ring = if generator { GENERATOR_RING } else { clear };
        let landmark = Landmark {
            center: at,
            radius: ring,
            precinct: if generator { GENERATOR_PRECINCT } else { clear },
            doors: Vec::new(),
            fallen: None,
            kind,
            clear,
            reach,
            facing: Vec2::from_angle(rng.range(0.0, TAU)),
        };
        let mut land = Land {
            high: High {
                height: rng.range(8.0, 10.0),
                parks: vec![(at, reach + rng.range(10.0, 18.0))],
                ..High::default()
            },
            dunes: Some(Dunes {
                wind: Vec2::from_angle(rng.range(0.0, TAU)),
                wave: rng.range(38.0, 60.0),
                height: rng.range(1.0, 1.8),
                phase: rng.range(0.0, 1.0),
                calm: vec![(at, clear + 4.0, reach + 16.0)],
            }),
            ..Land::default()
        };
        if kind == LandmarkKind::Crater {
            land.bowls.push(Bowl {
                at,
                clear,
                depth: 2.2,
                rim: 1.5,
            });
        }
        let mut sk = Skeleton {
            size,
            land,
            streets: Vec::new(),
            squares: Vec::new(),
            landmarks: vec![landmark],
            heart: at,
            pad: PAD,
            bases: Vec::new(),
            lots: Vec::new(),
            spans: Vec::new(),
            edges: Vec::new(),
            decos: Vec::new(),
            repair_spots: Vec::new(),
            crossings: Vec::new(),
            ..Skeleton::default()
        };
        let out = |rng: &mut Rng, dir: Vec2| {
            mid + dir * (size * 0.5 + 40.0) + dir.perp() * rng.range(-30.0, 30.0)
        };
        let lm = at;
        match form {
            Form::Erg => {
                // Bigger dunes, further apart.
                sk.dunes_mut().height = rng.range(2.2, 3.0);
                sk.dunes_mut().wave = rng.range(50.0, 70.0);
                let want = rng.int(3, 5) as usize;
                sk.islands(rng, want, (12.0, 26.0));
                if rng.chance(0.5) {
                    sk.cut_desert_wash(rng);
                }
            }
            Form::Canyonlands => {
                sk.land.high.height = rng.range(6.5, 8.0);
                sk.land.high.slabs.push(Slab::Everywhere);
                // The main canyon through the middle, edge to edge.
                let a = Vec2::from_angle(rng.range(0.0, TAU));
                let b = Vec2::from_angle(rng.range(-0.5, 0.5)).rotate(-a);
                let via = [out(rng, a), lm, out(rng, b)];
                sk.land.high.channels.push(Channel {
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
                    let line = sk.land.high.channels[rng.index(sk.land.high.channels.len())]
                        .line
                        .clone();
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
                        sk.land
                            .high
                            .channels
                            .iter()
                            .all(|c| line_distance(&c.line, q) > c.half + 30.0)
                    }) && line_distance(&branch, lm) > 50.0 + extra;
                    if !apart {
                        continue;
                    }
                    sk.land.high.channels.push(Channel {
                        line: branch,
                        half: rng.range(8.0, 12.0),
                    });
                    made += 1;
                }
            }
            Form::Basin => {
                sk.dunes_mut().height = rng.range(1.2, 2.0);
                let radius = size * rng.range(0.33, 0.4);
                sk.land.high.slabs.push(Slab::Outside {
                    center: lm,
                    radius,
                    phase: rng.range(0.0, TAU),
                });
                sk.land.high.parks.clear();
                // Passes out through the ring.
                let n = rng.int(2, 4);
                let a0 = rng.range(0.0, TAU);
                for k in 0..n {
                    let dir =
                        Vec2::from_angle(a0 + k as f32 * TAU / n as f32 + rng.range(-0.4, 0.4));
                    let via = [lm + dir * radius * 0.6, out(rng, dir)];
                    sk.land.high.channels.push(Channel {
                        line: meander(rng, &via, 0.35),
                        half: rng.range(8.0, 11.0),
                    });
                }
                let want = rng.int(0, 2) as usize;
                sk.islands(rng, want, (10.0, 16.0));
                if rng.chance(0.4) {
                    sk.cut_desert_wash(rng);
                }
            }
            Form::Escarpment => {
                sk.dunes_mut().height = rng.range(1.2, 2.2);
                let into = Vec2::from_angle(rng.range(0.0, TAU));
                let at = lm + into * (rng.range(55.0, 80.0) + extra);
                sk.land.high.slabs.push(Slab::Beyond {
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
                    sk.land.high.channels.push(Channel {
                        line: meander(rng, &[from, end], 0.55),
                        half: rng.range(9.0, 12.0),
                    });
                }
                let want = rng.int(1, 3) as usize;
                sk.islands(rng, want, (10.0, 18.0));
                if rng.chance(0.4) {
                    sk.cut_desert_wash(rng);
                }
            }
            Form::SaltFlat => {
                sk.land.salt = true;
                sk.dunes_mut().height = 0.2;
                sk.cut_desert_wash(rng);
                let want = rng.int(1, 2) as usize;
                sk.islands(rng, want, (10.0, 16.0));
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
            if self.landmarks[0].center.distance(c) < self.landmarks[0].reach + reach + 30.0
                || self.land.plateau(c) < reach + 30.0
            {
                continue;
            }
            self.land.high.slabs.push(Slab::Island {
                center: c,
                radius,
                phase: rng.range(0.0, TAU),
            });
            placed += 1;
        }
    }

    /// A dry wash winding across the floor, clear of the landmark.
    fn cut_desert_wash(&mut self, rng: &mut Rng) {
        let s = self.size;
        let lm = self.landmarks[0].clone();
        for _ in 0..12 {
            let off = Vec2::from_angle(rng.range(0.0, TAU));
            let at = lm.center + off * rng.range(lm.reach + 22.0, lm.reach + 50.0);
            let dir = Vec2::from_angle(rng.range(-0.45, 0.45)).rotate(off.perp());
            let mid = Vec2::splat(s * 0.5);
            let a = mid + (at - mid) - dir * s;
            let b = mid + (at - mid) + dir * s;
            let line = meander(rng, &[a, at, b], 0.4);
            if line_distance(&line, lm.center) > lm.reach + WASH_FLOOR + WASH_BANK + 4.0 {
                self.land.washes.push(Wash {
                    line,
                    floor: WASH_FLOOR,
                    bank: WASH_BANK,
                    depth: WASH_DEPTH,
                    sway: 0.0,
                    phase: 0.0,
                    ragged: false,
                });
                return;
            }
        }
    }

    /// The ground out past the edge: the arena's carried on from its edge,
    /// rising into hills and big dunes.
    pub fn scenery_ground(&self, x: f32, z: f32) -> f32 {
        let s = self.size;
        let dunes = self.dunes();
        let p = Vec2::new(x, z);
        let q = (p - Vec2::splat(s * 0.5)).abs() - Vec2::splat(s * 0.5);
        let d = q.max(Vec2::ZERO).length() + q.x.max(q.y).min(0.0);
        if d < -0.01 {
            return -10.0;
        }
        let e = p.clamp(Vec2::ZERO, Vec2::splat(s));
        let base = self.ground(e);
        let big = 1.0 + (p.dot(dunes.wind) / (dunes.wave * 2.2) + dunes.phase).sin();
        // (A salt flat runs on a long way first.)
        let (near, far) = match self.land.salt {
            true => (90.0, 330.0),
            false => (6.0, 70.0),
        };
        let hills = smoothstep(near, far, d)
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

    /// Ramps up the cliffs, by rule: every cliff line of any length in the
    /// arena gets one, and one more for each 140 m of it, spread out,
    /// where there's a run of open floor for it.
    pub(super) fn place_ramps(&mut self, rng: &mut Rng) {
        let run = self.land.high.height * RAMP_RUN;
        let s = self.size;
        let inside = |p: Vec2, m: f32| p.min_element() > m && p.max_element() < s - m;
        for line in self.land.cliff_lines(self.size) {
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
                if self.land.plateau(top) > -3.0 || self.land.plateau(top - out * 8.0) > -3.0 {
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
                        let centre = t < 0.35 || self.land.plateau(q) > FOOT + 1.5;
                        let outer =
                            t < 0.2 || self.land.plateau(q + side * (RAMP_HALF + 2.0)) > FOOT + 2.0;
                        let inner = slant != 0.0
                            || t < 0.2
                            || self.land.plateau(q - side * (RAMP_HALF + 2.0)) > FOOT + 2.0;
                        // Room to drive on past it.
                        let lane =
                            t < 0.5 || self.land.plateau(q + side * (RAMP_HALF + 10.0)) > 0.5;
                        centre && outer && inner && lane
                    });
                    let ok = open
                        && inside(foot, 12.0)
                        && self.landmarks[0].center.distance(foot) > self.landmarks[0].reach + 10.0
                        && self.land.wash_distance(foot) > WASH_FLOOR + WASH_BANK + 6.0;
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
                let far: Vec<Ramp> =
                    sites
                        .iter()
                        .copied()
                        .filter(|r| {
                            self.land.high.ramps.iter().all(|o| {
                                o.foot.distance(r.foot) > 50.0 && o.top.distance(r.top) > 50.0
                            })
                        })
                        .collect();
                if far.is_empty() {
                    break;
                }
                self.land.high.ramps.push(far[rng.index(far.len())]);
            }
        }
    }

    /// Kept clear of anything put down: the spawns, the landmark's open
    /// ground, the ramps.
    pub fn keep_clear(&self, p: Vec2) -> bool {
        self.base_spots().iter().any(|b| b.distance(p) < self.pad)
            || self.landmarks[0].center.distance(p) < self.landmarks[0].clear
            || self
                .land
                .high
                .ramps
                .iter()
                .any(|r| r.distance(p) < RAMP_HALF + 3.0)
            || self.streets.iter().any(|s| s.distance(p) < s.half + 1.0)
            || self.tracks.iter().any(|t| line_distance(t, p) < 4.5)
            || self.spur.as_ref().is_some_and(|s| {
                line_distance(&s.line, p) < 4.5
                    || self.shed_distance(p) < 6.0
                    || self.shed_doorways(p)
            })
            || self
                .crossings
                .iter()
                .any(|x| x.distance(p) < x.half + 6.0)
            || self
                .land
                .fords
                .iter()
                .any(|&(a, b, h)| crate::map::procgen::mine::seg(p, a, b).0 < h + 3.0)
            // Out in front of a dig's ways in.
            || self.land.digs.iter().any(|d| {
                let q = d.to_dig(p);
                let way = |r: &Hollow| q.x > r.min.x - 2.0 && q.x < r.max.x + 2.0;
                q.y > -20.0 && q.y < 4.0 && d.rooms.iter().any(|r| r.min.y < 0.0 && way(r))
            })
    }

    /// Where side spawns could go: level ground all on one level, off the
    /// ramps and washes, well out from the landmark.
    pub(super) fn base_sites(&self) -> Vec<Vec2> {
        let s = self.size;
        let mut out = Vec::new();
        let m = self.pad + 10.0;
        let mut z = m;
        while z <= s - m {
            let mut x = m;
            while x <= s - m {
                let p = Vec2::new(x, z);
                let g = self.ground(p);
                let ok = self.land.plateau(p).abs() > self.pad + 3.0
                    // (A town's are on the floor: its mesa's a strip along the edge.)
                    && (self.streets.is_empty() || self.land.plateau(p) > 0.0)
                    && self.land.tower_distance(p) > self.pad + 8.0
                    && self
                        .land
                        .high
                        .ramps
                        .iter()
                        .all(|r| r.distance(p) > self.pad + 6.0)
                    && self.land.wash_distance(p) > self.pad + WASH_FLOOR + WASH_BANK
                    && self.streets.iter().all(|s| s.distance(p) > self.pad + 5.0)
                    && self.land.lake.is_none_or(|l| l.sdf(p) > self.pad - 6.0)
                    && self
                        .land
                        .dam
                        .is_none_or(|d| d.distance(p) > self.pad + 14.0)
                    && self.landmarks[0].center.distance(p) > s * 0.3
                    && self.tracks.iter().all(|t| line_distance(t, p) > self.pad + 4.0)
                    && self.shed_distance(p) > self.pad + 10.0
                    && self
                        .land
                        .mounds
                        .iter()
                        .all(|m| m.at.distance(p) > self.pad + 34.0)
                    && (0..12).all(|k| {
                        let q = p + Vec2::from_angle(k as f32 * TAU / 12.0) * self.pad;
                        (self.ground(q) - g).abs() < 2.5
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
        let lm = self.landmarks[0].center;
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
        // A town's sides aren't out of sight of the fight.
        let far = match self.streets.is_empty() {
            true => f32::INFINITY,
            false => super::town::FAR,
        };
        let mut best: Option<(f32, Vec<Vec2>)> = None;
        for &(first, len) in &found {
            let mut pick = vec![first];
            for &(p, l) in &found {
                if pick.len() >= sides {
                    break;
                }
                if (l - len).abs() / len.max(l) < 0.12
                    && pick
                        .iter()
                        .all(|q| q.distance(p) > apart && q.distance(p) < far)
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
                for &at in &b {
                    let pad = self.pad;
                    self.dunes_mut().calm.push((at, pad - 4.0, pad + 14.0));
                }
                self.bases = b.into_iter().map(|at| Base::pad(at, self.pad)).collect();
                true
            }
            None => false,
        }
    }

    /// A salt flat's ridges: most of them across the way between the sides,
    /// apart, off the spawns, the landmark, the channel and the rock.
    fn place_ridges(&mut self, rng: &mut Rng) {
        const RUN: f32 = 7.0;
        let spots = self.base_spots();
        let way = (spots[spots.len() - 1] - spots[0]).normalize_or(Vec2::X);
        let want = rng.int(7, 10) as usize;
        for _ in 0..600 {
            if self.land.ridges.len() >= want {
                break;
            }
            let turn = match rng.chance(0.7) {
                true => rng.range(-0.5, 0.5),
                false => rng.range(0.0, TAU),
            };
            let dir = Vec2::from_angle(turn).rotate(way.perp());
            let mid = Vec2::new(
                rng.range(20.0, self.size - 20.0),
                rng.range(20.0, self.size - 20.0),
            );
            let half = rng.range(22.0, 48.0);
            let line = meander(rng, &[mid - dir * half, mid, mid + dir * half], 0.45);
            let lm = &self.landmarks[0];
            let ok = line.iter().all(|&p| {
                p.min_element() > 14.0
                    && p.max_element() < self.size - 14.0
                    && lm.center.distance(p) > lm.reach + RUN + 3.0
                    && spots.iter().all(|b| b.distance(p) > self.pad + RUN + 2.0)
                    && self.land.plateau(p) > RUN + 8.0
                    && self.land.ramp_distance(p) > RAMP_HALF + RUN + 6.0
                    && self.land.wash_distance(p) > WASH_FLOOR + WASH_BANK + RUN
                    && self.land.ridge_distance(p) > 30.0
            });
            if ok {
                self.land.ridges.push(Ridge {
                    line,
                    height: rng.range(2.1, 2.5),
                    run: RUN,
                });
            }
        }
    }

    /// The lots: clusters of spires, fins and boulder fields, each wholly
    /// on one level, off what's kept clear and apart.
    pub fn place_lots(&mut self, rng: &mut Rng, form: Form) {
        if form == Form::SaltFlat {
            self.place_ridges(rng);
        }
        let (spires, fins, boulders) = match form {
            Form::Erg => (rng.int(3, 5), rng.int(3, 5), rng.int(3, 5)),
            Form::Canyonlands => (rng.int(3, 5), rng.int(1, 3), rng.int(2, 4)),
            Form::Basin => (rng.int(3, 4), rng.int(2, 4), rng.int(3, 4)),
            Form::Escarpment => (rng.int(2, 4), rng.int(2, 4), rng.int(3, 5)),
            Form::SaltFlat => (rng.int(1, 3), rng.int(2, 4), rng.int(1, 2)),
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
                let lm = &self.landmarks[0];
                let ok = self.land.plateau(at).abs() > reach + 4.0
                    && self.land.tower_distance(at).abs() > reach + 5.0
                    && lm.center.distance(at) > lm.reach + reach + 6.0
                    && self
                        .base_spots()
                        .iter()
                        .all(|&b| b.distance(at) > self.pad + reach + 4.0)
                    && self
                        .land
                        .high
                        .ramps
                        .iter()
                        .all(|r| r.distance(at) > reach + RAMP_HALF + 4.0)
                    && self.land.wash_distance(at) > reach + WASH_FLOOR
                    && self.land.ridge_distance(at) > reach + 9.0
                    && self
                        .tracks
                        .iter()
                        .all(|t| line_distance(t, at) > reach + 5.0)
                    && self.shed_distance(at) > reach + 10.0
                    && self
                        .land
                        .mounds
                        .iter()
                        .all(|m| m.at.distance(at) > m.radius * 1.4 + m.run + reach)
                    && !self.keep_clear(at)
                    && self.land.digs.iter().all(|d| d.distance(at) > reach + 6.0)
                    && self
                        .lots
                        .iter()
                        .all(|o| o.at().distance(at) > o.reach() + reach + 8.0);
                if ok {
                    self.lots.push(Lot::round(kind, at, reach));
                    placed += 1;
                }
            }
        }
    }
}
