//! How a hoard is laid out. In order: the land, flattened for the grid, and
//! the counting floor raised in the middle; each side's pad, at opposite
//! ends; the grid's lanes and the roads out; a stack in every square of it
//! (here and there a monolith, a pair of small stacks, or nothing), gantries
//! over the lanes; sheds and stray cubes out on the land round it.

use std::f32::consts::TAU;

use glam::Vec2;

use crate::map::OBox;
use crate::map::procgen::city;
use crate::map::procgen::foundry::Part;
use crate::map::procgen::foundry::plan::{PAD, lay, lot, off_roads, repairs, room};
use crate::map::procgen::land::{Land, Swell};
use crate::map::procgen::skeleton::*;
use crate::rng::Rng;

/// How far the counting floor's kept clear of the grid's stacks: its top
/// and the slope up to it.
const FLOOR: f32 = 40.0;
/// How high it stands.
const RISE: f32 = 3.2;

/// The grid: its middle, which way it runs, how far apart its lanes are,
/// and how many squares it runs to either way.
struct Grid {
    mid: Vec2,
    along: Vec2,
    pitch: f32,
    n: i32,
}

impl Grid {
    fn at(&self, u: f32, v: f32) -> Vec2 {
        self.mid + self.along * u + self.along.perp() * v
    }

    fn reach(&self) -> f32 {
        self.pitch * self.n as f32
    }

    fn set(&self, u: f32, v: f32, half: Vec2) -> OBox {
        OBox {
            center: self.at(u, v),
            yaw: self.along.x.atan2(self.along.y),
            half,
            ..OBox::default()
        }
    }
}

pub fn skeleton(rng: &mut Rng) -> Skeleton {
    let size = 20.0 * rng.int(13, 15) as f32;
    let mid = Vec2::splat(size * 0.5);
    let heart = mid + Vec2::new(rng.range(-0.04, 0.04), rng.range(-0.04, 0.04)) * size;
    let grid = Grid {
        mid: heart,
        along: Vec2::from_angle(rng.range(0.0, TAU)),
        pitch: rng.range(22.0, 24.0),
        n: 4,
    };
    // No valley through it: hills round it.
    let mut land = Land::rolling(rng, size, &[]);
    land.swells.retain(|s| !matches!(s, Swell::Cut(_)));
    let level = land.swell(heart);
    let flat = grid.set(0.0, 0.0, Vec2::splat(grid.reach()));
    land.level_at(flat, 4.0, 40.0, level);
    let top = grid.set(0.0, 0.0, Vec2::splat(18.0));
    land.level_at(top, 2.0, 16.0, level + RISE);
    let mut sk = Skeleton {
        size,
        land,
        heart,
        ..Skeleton::default()
    };
    bases(&mut sk, rng);
    roads(&mut sk, rng, &grid);
    stacks(&mut sk, rng, &grid);
    gantries(&mut sk, rng, &grid);
    outside(&mut sk, rng, &grid);
    strays(&mut sk, rng, &grid);
    repairs(&mut sk, rng);
    sk.squares.push(Square {
        center: heart,
        radius: 15.0,
    });
    city::level(&mut sk);
    sk
}

/// Each side's pad, about opposite each other, as far out as they go.
fn bases(sk: &mut Skeleton, rng: &mut Rng) {
    let a = rng.range(0.0, TAU);
    let edge = PAD + 6.0;
    let room = sk.size * 0.5 - edge;
    sk.bases = [0.0, TAU * 0.5 + rng.range(-0.35, 0.35)]
        .into_iter()
        .map(|t| {
            let d = Vec2::from_angle(a + t);
            let p = Vec2::splat(sk.size * 0.5) + d * room / d.abs().max_element();
            Base::pad(p, PAD)
        })
        .collect();
}

/// The grid's lanes, inside the arena and off the pads; two roads out; every
/// pad onto them.
fn roads(sk: &mut Skeleton, rng: &mut Rng, grid: &Grid) {
    let size = sk.size;
    let reach = grid.reach();
    let pads = sk.base_spots();
    for flip in [false, true] {
        for k in -grid.n..=grid.n {
            let v = k as f32 * grid.pitch;
            let on = |u: f32| match flip {
                false => grid.at(u, v),
                true => grid.at(v, u),
            };
            let open = |u: f32| {
                let p = on(u);
                p.min_element() > 16.0
                    && p.max_element() < size - 16.0
                    && p.distance(grid.mid) < reach * 1.15
                    && pads.iter().all(|b| b.distance(p) > PAD + 4.0)
            };
            // The stretch through its middle.
            let mut ends = [0.0f32; 2];
            for (e, s) in ends.iter_mut().zip([-1.0f32, 1.0]) {
                while e.abs() < reach && open(*e + s * 2.0) {
                    *e += s * 2.0;
                }
            }
            if ends[1] - ends[0] < grid.pitch {
                continue;
            }
            let (half, rank) = match k {
                0 => (5.0, Rank::Avenue),
                _ => (4.4, Rank::Lane),
            };
            sk.streets.push(Street {
                from: on(ends[0]),
                to: on(ends[1]),
                half,
                clear: 3.0,
                rank,
                out: false,
            });
        }
    }
    let mut mouths: city::Mouths = Default::default();
    let first = rng.index(4);
    for k in [first, (first + 2) % 4] {
        let (start, along, inward) = city::sides(size)[k];
        // From the end of the avenue nearest that edge.
        let Some(from) = sk.avenues().flat_map(|s| [s.from, s.to]).min_by(|a, b| {
            let off = |p: Vec2| (p - start).dot(inward);
            off(*a).total_cmp(&off(*b))
        }) else {
            continue;
        };
        for _ in 0..12 {
            let t = size * rng.range(0.2, 0.8);
            if lay(sk, rng, (from, start + along * t), 6.0, Rank::Avenue, true) {
                mouths[k].push((t - 9.0, t + 9.0));
                break;
            }
        }
    }
    for c in pads {
        let d = (sk.heart - c).normalize_or(Vec2::X);
        let mouth = c + d * (PAD + 3.0);
        let near = sk
            .streets
            .iter()
            .flat_map(|s| [s.from, s.from.lerp(s.to, 0.5), s.to])
            .min_by(|a, b| a.distance(mouth).total_cmp(&b.distance(mouth)));
        if let Some(to) = near {
            lay(sk, rng, (mouth, to), 4.6, Rank::Lane, false);
        }
    }
    city::crossings(sk);
    city::cordon(sk, rng, &mouths);
}

/// Something in every square of the grid that's clear of the counting
/// floor, and a stack at each of the floor's corners.
fn stacks(sk: &mut Skeleton, rng: &mut Rng, grid: &Grid) {
    let put = |sk: &mut Skeleton, b: OBox, part: Part| {
        if room(sk, &b, 2.5) && off_roads(sk, &b, 0.3) {
            sk.lots.push(lot(b, part));
        }
    };
    for (u, v) in [(-1.0, -1.0), (-1.0, 1.0), (1.0, -1.0), (1.0, 1.0)] {
        let b = grid.set(u * 14.7, v * 14.7, Vec2::splat(2.5));
        put(sk, b, Part::Stack);
    }
    for i in -grid.n..grid.n {
        for j in -grid.n..grid.n {
            let (u, v) = ((i as f32 + 0.5) * grid.pitch, (j as f32 + 0.5) * grid.pitch);
            let far = Vec2::new(u, v).length();
            if !(FLOOR..grid.reach() * 1.15).contains(&far) {
                continue;
            }
            match rng.f32() {
                x if x < 0.66 => {
                    let half = Part::Stack.half(rng).x;
                    put(sk, grid.set(u, v, Vec2::splat(half)), Part::Stack);
                }
                x if x < 0.76 => {
                    let half = Part::Monolith.half(rng).x;
                    put(sk, grid.set(u, v, Vec2::splat(half)), Part::Monolith);
                }
                // Two small ones corner to corner, a way through between.
                x if x < 0.92 => {
                    let e = if rng.chance(0.5) { 1.0 } else { -1.0 };
                    for s in [-1.0f32, 1.0] {
                        let b = grid.set(u + s * 3.4, v + s * e * 3.4, Vec2::splat(2.2));
                        put_pair(sk, b);
                    }
                }
                _ => {}
            }
        }
    }
}

/// One of a pair of small stacks: as close to the other as it's set.
fn put_pair(sk: &mut Skeleton, b: OBox) {
    if room(sk, &b, 1.0) && off_roads(sk, &b, 0.3) {
        sk.lots.push(lot(b, Part::Stack));
    }
}

/// Gantries over the lanes, between one crossing and the next.
fn gantries(sk: &mut Skeleton, rng: &mut Rng, grid: &Grid) {
    let mut want = rng.int(4, 6);
    for _ in 0..80 {
        if want == 0 {
            break;
        }
        let cell = |rng: &mut Rng| rng.int(-grid.n, grid.n - 1) as f32 + 0.5;
        let line = rng.int(1, grid.n - 1) as f32 * if rng.chance(0.5) { 1.0 } else { -1.0 };
        let (across, along) = (line * grid.pitch, cell(rng) * grid.pitch);
        let half = Vec2::new(5.4, 3.0);
        let mut b = grid.set(across, along, half);
        // (`set`'s long way is the grid's along.)
        b.yaw += std::f32::consts::FRAC_PI_2;
        if rng.chance(0.5) {
            b = grid.set(along, across, half);
        }
        let (ax, az) = b.axes();
        let legs = [-1.0f32, 1.0].into_iter().flat_map(|x| {
            [-1.0f32, 1.0].map(|z| b.center + ax * x * (half.x - 0.7) + az * z * (half.y - 0.7))
        });
        let clear = legs.into_iter().all(|p| {
            sk.streets.iter().all(|s| s.distance(p) > s.half + 0.1)
                && sk.lots.iter().all(|l| l.footprint.distance(p) > 0.8)
        });
        let apart = sk
            .lots
            .iter()
            .filter(|l| l.kind == LotKind::Works(Part::Gantry))
            .all(|l| l.at().distance(b.center) > 34.0);
        let inside = b.center.min_element() > 16.0
            && b.center.max_element() < sk.size - 16.0
            && b.distance(sk.heart) > FLOOR
            && sk.bases.iter().all(|s| b.distance(s.middle()) > PAD + 3.0);
        if clear && apart && inside {
            sk.lots.push(lot(b, Part::Gantry));
            want -= 1;
        }
    }
}

/// Sheds, offices and walls out on the land round the grid, square to it.
fn outside(sk: &mut Skeleton, rng: &mut Rng, grid: &Grid) {
    for _ in 0..500 {
        let p = Vec2::new(
            rng.range(14.0, sk.size - 14.0),
            rng.range(14.0, sk.size - 14.0),
        );
        let q = p - grid.mid;
        let q = Vec2::new(q.dot(grid.along), q.dot(grid.along.perp()));
        let part = [
            Part::Shed,
            Part::Shed,
            Part::Office,
            Part::Bunker,
            Part::Wall,
            Part::Crane,
            Part::Stack,
        ][rng.index(7)];
        let mut half = part.half(rng);
        if part == Part::Stack {
            half = Vec2::splat(half.x);
        }
        let b = grid.set(q.x, q.y, half);
        let bare = sk.lots.iter().all(|l| l.footprint.distance(p) > 13.0);
        if bare && q.length() > grid.reach() && room(sk, &b, 5.0) && off_roads(sk, &b, 0.8) {
            sk.lots.push(lot(b, part));
        }
    }
}

/// Cubes not stacked yet, wherever there's nothing to hide behind: square to
/// the grid, as everything is.
fn strays(sk: &mut Skeleton, rng: &mut Rng, grid: &Grid) {
    for _ in 0..1600 {
        let p = Vec2::new(
            rng.range(10.0, sk.size - 10.0),
            rng.range(10.0, sk.size - 10.0),
        );
        let q = p - grid.mid;
        let half = Part::Cube.half(rng).x;
        let (u, v) = (q.dot(grid.along), q.dot(grid.along.perp()));
        let b = grid.set(u, v, Vec2::splat(half));
        let bare = sk.lots.iter().all(|l| l.footprint.distance(p) > 11.0);
        if bare && room(sk, &b, 5.0) && off_roads(sk, &b, 0.4) {
            sk.lots.push(lot(b, Part::Cube));
        }
    }
}
