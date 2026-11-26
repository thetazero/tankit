//! A reservoir town's skeleton: a small settlement out in the desert, laid
//! out round where its water came from.
//!
//! A **dam** town lies below its dam: an earth bank across the arena, broken
//! through in the middle (what the fight's about), the reservoir's dry bed
//! behind it, the wash the flood cut running on out of the breach, the town's
//! main street bending across below it with lanes off it. A **well** town's
//! streets run in from all round to the court its well stands in, sunk into
//! the ground.
//!
//! Either way the houses stand along the streets, turned to face them; the
//! rest is the desert's: its sand, its mesas, its boulders, its spawns placed
//! for a fair drive.

use std::f32::consts::TAU;

use glam::Vec2;

use super::skeleton::*;
use crate::map::OBox;
use crate::rng::Rng;

/// The fewest houses a town's worth its name with, and the most.
pub const MIN_HOUSES: usize = 14;
const MAX_HOUSES: i32 = 34;
/// Each side's spawns: kept clear and level this far round.
pub const PAD: f32 = 16.0;
/// The furthest apart two sides' spawns may be.
pub const FAR: f32 = 205.0;
/// Nothing's built nearer the arena's edge than this.
const EDGE: f32 = 16.0;

/// Where a town's water came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Dam,
    Well,
}

/// The source a seed's town lies round: each in turn.
pub fn source_of(seed: u64) -> Source {
    match seed % 2 {
        0 => Source::Dam,
        _ => Source::Well,
    }
}

impl Skeleton {
    /// A town round `source`, all but its spawns and houses (see
    /// `place_bases` and `place_houses`).
    pub fn town(rng: &mut Rng, source: Source) -> Skeleton {
        let size = match source {
            Source::Dam => rng.range(240.0, 260.0),
            Source::Well => rng.range(220.0, 240.0),
        };
        let size = (size / 2.0).round() * 2.0;
        let mid = Vec2::splat(size * 0.5);
        let down = Vec2::from_angle(rng.range(0.0, TAU));
        let at = match source {
            Source::Dam => mid + down * rng.range(12.0, 26.0),
            Source::Well => mid + down * rng.range(0.0, 14.0),
        };
        let (kind, clear, reach) = match source {
            Source::Dam => (LandmarkKind::Dam, 11.0, 26.0),
            Source::Well => (LandmarkKind::Well, 11.0, 15.0),
        };
        let land = Land {
            high: High {
                height: rng.range(7.5, 9.5),
                parks: vec![(at, reach + 30.0)],
                ..High::default()
            },
            dunes: Some(Dunes {
                wind: Vec2::from_angle(rng.range(0.0, TAU)),
                wave: rng.range(38.0, 60.0),
                height: rng.range(1.0, 1.8),
                phase: rng.range(0.0, 1.0),
                calm: vec![(at, 40.0, 60.0)],
            }),
            ..Land::default()
        };
        let mut sk = Skeleton {
            size,
            land,
            landmarks: vec![Landmark {
                center: at,
                radius: clear,
                precinct: clear,
                doors: Vec::new(),
                fallen: None,
                kind,
                clear,
                reach,
                facing: down,
            }],
            heart: at,
            pad: PAD,
            ..Skeleton::default()
        };
        match source {
            Source::Dam => sk.below_the_dam(rng, at, down),
            Source::Well => sk.round_the_well(rng, at),
        }
        // The streets lie on quiet ground.
        for s in &sk.streets {
            let calm = (s.from.midpoint(s.to), s.len() * 0.5 + 14.0);
            let d = sk.land.dunes.as_mut().expect("a desert's dunes");
            d.calm.push((calm.0, calm.1, calm.1 + 18.0));
        }
        sk.mesas(rng);
        sk.place_ramps(rng);
        sk
    }

    /// A street along `pts`.
    fn lay(&mut self, pts: &[Vec2], half: f32) {
        for w in pts.windows(2) {
            self.streets.push(Street {
                from: w[0],
                to: w[1],
                half,
                clear: half,
                rank: Rank::Lane,
                out: false,
            });
        }
    }

    fn below_the_dam(&mut self, rng: &mut Rng, at: Vec2, down: Vec2) {
        let across = down.perp();
        let half = rng.range(34.0, 42.0);
        self.land.dam = Some(Dam {
            a: at - across * half,
            b: at + across * half,
            height: rng.range(4.6, 5.6),
            crest: 4.0,
            run: 17.0,
            breach: 11.0,
        });
        let radius = rng.range(27.0, 33.0);
        self.land.lake = Some(Lake {
            center: at - down * (radius * 1.15 + 20.0) + across * rng.range(-10.0, 10.0),
            radius,
            phase: rng.range(0.0, TAU),
            depth: rng.range(3.4, 4.2),
            shore: 20.0,
        });
        // What broke through ran off this way.
        let from = at + down * 24.0;
        self.land.washes.push(Wash {
            line: meander(rng, &[from, from + down * self.size], 0.3),
            floor: 3.0,
            bank: 7.0,
            depth: 2.2,
            sway: 0.0,
            phase: 0.0,
        });
        // The main street, bending across below it.
        let heart = at + down * rng.range(46.0, 52.0) + across * rng.range(-14.0, 14.0);
        let long = rng.range(38.0, 48.0);
        let bend = rng.range(-9.0, 9.0);
        let main: Vec<Vec2> = (0..5)
            .map(|k| {
                let t = k as f32 / 2.0 - 1.0;
                heart + across * (t * long) + down * (bend * t * t + rng.range(-4.0, 4.0))
            })
            .collect();
        self.lay(&main, 5.0);
        // Up to the breach, a lane up to the dam's foot either side, lanes
        // off below.
        self.lay(&[main[2], at + down * 20.0], 5.0);
        let mut back = Vec::new();
        for k in [1, 3] {
            let up = (main[k] - at).dot(down) - 26.0;
            let lean = across * rng.range(-6.0, 6.0);
            self.lay(&[main[k], main[k] - down * up + lean], 4.0);
            let p = main[k] + down * rng.range(20.0, 27.0) + across * rng.range(-8.0, 8.0);
            self.lay(&[main[k], p], 4.0);
            back.push(p);
        }
        // A back street joining those, if not all the way.
        let mid = back[0].lerp(back[1], 0.5) + down * rng.range(-5.0, 5.0);
        match rng.int(0, 3) {
            0 => self.lay(&[back[0], mid], 4.0),
            1 => self.lay(&[back[1], mid], 4.0),
            _ => self.lay(&[back[0], mid, back[1]], 4.0),
        }
    }

    fn round_the_well(&mut self, rng: &mut Rng, at: Vec2) {
        self.land.bowls.push(Bowl {
            at,
            clear: 9.0,
            depth: rng.range(1.8, 2.4),
            rim: 0.0,
        });
        self.squares.push(Square {
            center: at,
            radius: 16.0,
        });
        let n = rng.int(4, 5);
        let a0 = rng.range(0.0, TAU);
        let mut bends = Vec::new();
        for k in 0..n {
            let a = a0 + (k as f32 + rng.range(-0.14, 0.14)) * TAU / n as f32;
            let dir = Vec2::from_angle(a);
            let bend = at + dir * rng.range(34.0, 42.0);
            let on = Vec2::from_angle(rng.range(-0.5, 0.5)).rotate(dir);
            let end = bend + on * rng.range(18.0, 30.0);
            self.lay(&[at + dir * 14.0, bend, end], 4.5);
            bends.push(bend);
        }
        // Part of a ring joining them.
        let gap = rng.index(bends.len());
        for k in 0..bends.len() {
            if k != gap {
                self.lay(&[bends[k], bends[(k + 1) % bends.len()]], 4.0);
            }
        }
        if rng.chance(0.4) {
            let off = Vec2::from_angle(rng.range(0.0, TAU));
            let via = at + off * rng.range(84.0, 94.0);
            let run = off.perp() * self.size;
            self.land.washes.push(Wash {
                line: meander(rng, &[via - run, via, via + run], 0.3),
                floor: WASH_FLOOR,
                bank: WASH_BANK,
                depth: WASH_DEPTH,
                sway: 0.0,
                phase: 0.0,
            });
        }
    }

    /// A long mesa along one side of it, perhaps, and buttes, all well clear
    /// of the town.
    fn mesas(&mut self, rng: &mut Rng) {
        let mid = Vec2::splat(self.size * 0.5);
        let lm = self.landmarks[0].center;
        if rng.chance(0.6) {
            // Beside a dam town, or anywhere round a well's.
            let into = match self.land.dam {
                Some(d) => d.along() * if rng.chance(0.5) { 1.0 } else { -1.0 },
                None => Vec2::from_angle(rng.range(0.0, TAU)),
            };
            self.land.high.slabs.push(Slab::Beyond {
                at: mid + into * (self.size * 0.5 - rng.range(20.0, 32.0)),
                into,
                amp: rng.range(4.0, 8.0),
                wave: rng.range(25.0, 45.0),
                phase: rng.range(0.0, TAU),
            });
        }
        let want = rng.int(1, 3);
        let mut placed = 0;
        for _ in 0..200 {
            if placed >= want {
                break;
            }
            let radius = rng.range(9.0, 15.0);
            let reach = radius * 1.25;
            let m = reach + 8.0;
            let c = Vec2::new(rng.range(m, self.size - m), rng.range(m, self.size - m));
            let ok = lm.distance(c) > 60.0 + reach
                && self.land.plateau(c) > reach + 30.0
                && self.streets.iter().all(|s| s.distance(c) > reach + 32.0)
                && self.land.lake.is_none_or(|l| l.sdf(c) > reach + 14.0)
                && self.land.dam.is_none_or(|d| d.distance(c) > reach + 32.0)
                && self.land.wash_distance(c) > reach + 22.0;
            if ok {
                self.land.high.slabs.push(Slab::Island {
                    center: c,
                    radius,
                    phase: rng.range(0.0, TAU),
                });
                placed += 1;
            }
        }
    }

    /// The town's houses.
    pub fn houses(&self) -> impl Iterator<Item = &Lot> {
        self.lots
            .iter()
            .filter(|l| matches!(l.kind, LotKind::Small | LotKind::Rubble))
    }

    /// Ground a house can stand on: level, and nobody's way.
    fn buildable(&self, p: Vec2) -> bool {
        let lm = &self.landmarks[0];
        p.min_element() > EDGE
            && p.max_element() < self.size - EDGE
            && lm.center.distance(p) > lm.reach + 3.0
            && self
                .base_spots()
                .iter()
                .all(|b| b.distance(p) > self.pad + 6.0)
            && self.streets.iter().all(|s| s.distance(p) > s.half + 1.0)
            && self.land.lake.is_none_or(|l| l.sdf(p) > 3.0)
            && self
                .land
                .dam
                .is_none_or(|d| d.distance(p) > d.crest + d.run)
            && self
                .land
                .washes
                .iter()
                .all(|w| w.distance(p) > w.floor + w.bank + 1.0)
            && self.land.plateau(p) > FOOT + 9.0
            && self.land.ramp_distance(p) > RAMP_HALF + 5.0
    }

    /// The houses along the streets, each turned to face its own, and a few
    /// rock lots out past them.
    pub fn place_houses(&mut self, rng: &mut Rng) {
        let streets = self.streets.clone();
        // Nearest the middle of things first, so the town's edge is ragged,
        // not its heart.
        let mut sites: Vec<(f32, OBox, (usize, f32))> = Vec::new();
        // (Several goes along each: most of what's tried won't fit.)
        for s in streets.iter().cycle().take(streets.len() * 12) {
            let dir = s.dir();
            for side in [-1.0f32, 1.0] {
                let out = dir.perp() * side;
                let mut t = rng.range(-5.0, 8.0);
                while t < s.len() {
                    let (front, depth) = (rng.range(7.5, 12.0), rng.range(7.5, 11.0));
                    // (Some a row back.)
                    let row = if rng.chance(0.2) {
                        rng.range(10.0, 14.0)
                    } else {
                        0.0
                    };
                    let off = s.half + row + rng.range(2.1, 4.2) + depth * 0.5;
                    let c = s.from + dir * (t + front * 0.5) + out * off;
                    // Not quite square to the street.
                    let turn = rng.range(-0.1, 0.1);
                    let footprint = OBox {
                        center: c,
                        yaw: (-dir.y).atan2(dir.x) + turn,
                        half: Vec2::new(front, depth) * 0.5,
                        y0: 0.0,
                        y1: 1.0,
                        ..OBox::default()
                    };
                    let face = if footprint.axes().1.dot(-out) > 0.0 {
                        1.0
                    } else {
                        -1.0
                    };
                    let far = c.distance(self.heart) * rng.range(0.7, 1.3);
                    sites.push((far, footprint, (1, face)));
                    t += front + rng.range(1.5, 7.0);
                }
            }
        }
        sites.sort_by(|a, b| a.0.total_cmp(&b.0));
        let most = rng.int(MAX_HOUSES - 10, MAX_HOUSES) as usize;
        for (_, f, front) in sites {
            if self.lots.len() >= most {
                break;
            }
            let h = f.half;
            let (ax, az) = f.axes();
            let on = (0..25).all(|k| {
                let u = (k % 5) as f32 / 2.0 - 1.0;
                let v = (k / 5) as f32 / 2.0 - 1.0;
                self.buildable(f.center + ax * h.x * u + az * h.y * v)
            });
            let apart = self.lots.iter().all(|o| !o.footprint.overlaps(&f, 1.6));
            if on && apart {
                let kind = if rng.chance(0.14) {
                    LotKind::Rubble
                } else {
                    LotKind::Small
                };
                self.lots.push(Lot {
                    footprint: f,
                    front,
                    kind,
                });
            }
        }
        for (kind, want, (lo, hi)) in [
            (LotKind::Spires, rng.int(1, 3), (8.0, 11.0)),
            (LotKind::Fins, rng.int(1, 3), (9.0, 12.0)),
            (LotKind::Boulders, rng.int(2, 4), (12.0, 18.0)),
        ] {
            let mut placed = 0;
            for _ in 0..200 {
                if placed >= want {
                    break;
                }
                let r = rng.range(lo, hi);
                let p = Vec2::new(
                    rng.range(r + 6.0, self.size - r - 6.0),
                    rng.range(r + 6.0, self.size - r - 6.0),
                );
                let ok = (0..12)
                    .all(|k| self.buildable(p + Vec2::from_angle(k as f32 * TAU / 12.0) * r))
                    && self.buildable(p)
                    && self
                        .streets
                        .iter()
                        .all(|s| s.distance(p) > s.half + r + 4.0)
                    && self
                        .lots
                        .iter()
                        .all(|o| o.at().distance(p) > o.footprint.half.length() + r + 6.0);
                if ok {
                    self.lots.push(Lot::round(kind, p, r));
                    placed += 1;
                }
            }
        }
    }
}
