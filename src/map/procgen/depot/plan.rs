//! How a depot is laid out. In order: the land, flattened for the yard; the
//! railway, a main line across the map and sidings off it, side by side;
//! each side's pad; roads round and across the yard; cars standing on the
//! sidings, gantries over them; sheds, docks, cranes and water towers round
//! the edge; then cover on what's left.

use std::f32::consts::{FRAC_PI_2, TAU};

use glam::Vec2;

use crate::map::OBox;
use crate::map::procgen::city;
use crate::map::procgen::foundry::Part;
use crate::map::procgen::foundry::plan::{
    PAD, RUN_ON, bed, clear_of, cover, lay, lot, off_roads, repairs, room,
};
use crate::map::procgen::land::{Land, Swell};
use crate::map::procgen::skeleton::*;
use crate::rng::Rng;

/// Half a track's width, as a street.
const TRACK: f32 = 2.6;

/// Where the yard is: its middle, which way the sidings run, half how long
/// they are, and each siding's place across it (the main line's first).
struct Yard {
    mid: Vec2,
    along: Vec2,
    half: f32,
    tracks: Vec<f32>,
}

impl Yard {
    fn at(&self, u: f32, v: f32) -> Vec2 {
        self.mid + self.along * u + self.along.perp() * v
    }

    /// Along it and across it.
    fn local(&self, p: Vec2) -> Vec2 {
        let d = p - self.mid;
        Vec2::new(d.dot(self.along), d.dot(self.along.perp()))
    }

    /// How far across it the outermost tracks are (the least, the most).
    fn edges(&self) -> (f32, f32) {
        let v = || self.tracks.iter().copied();
        (v().fold(f32::MAX, f32::min), v().fold(f32::MIN, f32::max))
    }

    fn yaw(&self) -> f32 {
        self.along.x.atan2(self.along.y)
    }

    /// Its tracks from one edge to the other.
    fn sorted(&self) -> Vec<f32> {
        let mut v = self.tracks.clone();
        v.sort_by(f32::total_cmp);
        v
    }
}

pub fn skeleton(rng: &mut Rng) -> Skeleton {
    let size = 20.0 * rng.int(14, 15) as f32;
    let mid = Vec2::splat(size * 0.5);
    let heart = mid + Vec2::new(rng.range(-0.04, 0.04), rng.range(-0.04, 0.04)) * size;
    let mut yard = Yard {
        mid: heart,
        along: Vec2::from_angle(rng.range(0.0, TAU)),
        half: size * rng.range(0.31, 0.35),
        tracks: Vec::new(),
    };
    // Lanes between the tracks; here and there two tracks side by side.
    let mut v = 0.0;
    let mut across = vec![0.0];
    for _ in 0..rng.int(6, 7) {
        v += if rng.chance(0.2) {
            5.2
        } else {
            rng.range(11.2, 13.5)
        };
        across.push(v);
    }
    let off = v * 0.5;
    across.iter_mut().for_each(|x| *x -= off);
    let main = across.remove(rng.index(across.len()));
    across.insert(0, main);
    yard.tracks = across;

    // No valley through a yard: hills round it.
    let mut land = Land::rolling(rng, size, &[]);
    land.swells.retain(|s| !matches!(s, Swell::Cut(_)));
    let (lo, hi) = yard.edges();
    let flat = OBox {
        center: yard.at(0.0, (lo + hi) * 0.5),
        yaw: yard.yaw(),
        half: Vec2::new((hi - lo) * 0.5 + ROAD + 6.0, yard.half + 14.0),
        ..OBox::default()
    };
    let level = land.swell(heart);
    land.level_at(flat, 4.0, 40.0, level);
    let mut sk = Skeleton {
        size,
        land,
        heart,
        ..Skeleton::default()
    };
    railway(&mut sk, rng, &yard);
    bases(&mut sk, rng, &yard);
    roads(&mut sk, rng, &yard);
    bed(&mut sk);
    cars(&mut sk, rng, &yard);
    gantries(&mut sk, rng, &yard);
    sides(&mut sk, rng, &yard);
    cover(&mut sk, rng);
    repairs(&mut sk, rng);
    sk.squares.push(Square {
        center: heart,
        radius: 12.0,
    });
    city::level(&mut sk);
    sk.stands = sk
        .bases
        .iter()
        .map(|b| {
            let c = b.middle();
            c + (heart - c).normalize_or(Vec2::X) * (PAD - 7.0)
        })
        .collect();
    sk
}

/// The main line, off the map at both ends, and the sidings: each leaves it
/// at one end of the yard and most join it again at the other.
fn railway(sk: &mut Skeleton, rng: &mut Rng, yard: &Yard) {
    let main = yard.tracks[0];
    let size = sk.size;
    // As far as the arena's edge either way.
    let edge = |e: f32| {
        let mut u = 0.0;
        while u < size * 2.0 {
            let p = yard.at(e * u, main);
            if p.min_element() < 0.0 || p.max_element() > size {
                break;
            }
            u += 1.0;
        }
        u
    };
    let (a, b) = (edge(-1.0), edge(1.0));
    let mut line = vec![yard.at(-a - RUN_ON, main), yard.at(-a, main)];
    line.push(yard.at(b, main));
    line.push(yard.at(b + RUN_ON, main));
    street(sk, &line[1..3]);
    sk.tracks.push(line);
    for &v in &yard.tracks[1..] {
        let half = yard.half * rng.range(0.82, 1.0);
        // How far along it takes to reach the main line.
        let lead = (v - main).abs() * 1.1 + 4.0;
        let joined = |e: f32, cap: f32| half + lead < cap - 2.0 && e != 0.0;
        let mut ends = [joined(-1.0, a), joined(1.0, b)];
        if rng.chance(0.35) {
            ends[rng.index(2)] = false;
        }
        let mut pts = Vec::new();
        if ends[0] {
            pts.push(yard.at(-half - lead, main));
        }
        pts.push(yard.at(-half, v));
        pts.push(yard.at(half, v));
        if ends[1] {
            pts.push(yard.at(half + lead, main));
        }
        street(sk, &pts);
        sk.tracks.push(pts);
    }
}

fn street(sk: &mut Skeleton, pts: &[Vec2]) {
    for w in pts.windows(2) {
        sk.streets.push(Street {
            from: w[0],
            to: w[1],
            half: TRACK,
            clear: 2.0,
            rank: Rank::Rail,
            out: false,
        });
    }
}

/// Each side's pad: off the rails, far apart, toward opposite ends.
fn bases(sk: &mut Skeleton, rng: &mut Rng, yard: &Yard) {
    let (lo, hi) = yard.edges();
    let edge = PAD + 6.0;
    let mut spots: Vec<Vec2> = Vec::new();
    for _ in 0..4000 {
        let p = Vec2::new(
            rng.range(edge, sk.size - edge),
            rng.range(edge, sk.size - edge),
        );
        let q = yard.local(p);
        let out = (q.x.abs() - yard.half).max(lo - q.y).max(q.y - hi);
        let rails = sk.streets.iter().all(|s| s.distance(p) > PAD + TRACK + 2.0);
        if out > PAD + 3.0 && rails && spots.len() < 160 {
            spots.push(p);
        }
    }
    // Chosen together: the pair furthest along the yard from each other, of
    // a few dozen.
    let far = |a: Vec2, b: Vec2| {
        let d = yard.local(a) - yard.local(b);
        d.x.abs() + 0.5 * d.y.abs()
    };
    let mut best: Option<(Vec2, Vec2)> = None;
    for _ in 0..60 {
        if spots.len() < 2 {
            break;
        }
        let (a, b) = (spots[rng.index(spots.len())], spots[rng.index(spots.len())]);
        if best.is_none_or(|(p, q)| far(a, b) > far(p, q)) {
            best = Some((a, b));
        }
    }
    let bases: Vec<Vec2> = best.into_iter().flat_map(|(a, b)| [a, b]).collect();
    sk.bases = bases.into_iter().map(|p| Base::pad(p, PAD)).collect();
}

/// The haul road across the yard by its middle; a road down either side of
/// it; every base onto them.
fn roads(sk: &mut Skeleton, rng: &mut Rng, yard: &Yard) {
    let (lo, hi) = yard.edges();
    let size = sk.size;
    let mut mouths: city::Mouths = Default::default();
    let ends = [lo - ROAD, hi + ROAD].map(|v| yard.at(0.0, v));
    straight(sk, ends[0], ends[1], 6.0);
    for e in [-1.0f32, 1.0] {
        // The edge the way across points at.
        let d = yard.along.perp() * e;
        let k = (0..4)
            .max_by(|&i, &j| {
                let out = |k: usize| -city::sides(size)[k].2.dot(d);
                out(i).total_cmp(&out(j))
            })
            .unwrap_or(0);
        let (start, along, _) = city::sides(size)[k];
        for _ in 0..12 {
            let t = size * rng.range(0.2, 0.8);
            if lay(
                sk,
                rng,
                (ends[(e > 0.0) as usize], start + along * t),
                6.0,
                Rank::Avenue,
                true,
            ) {
                mouths[k].push((t - 9.0, t + 9.0));
                break;
            }
        }
    }
    let run = yard.half * 0.9;
    for v in [lo - ROAD, hi + ROAD] {
        straight(sk, yard.at(-run, v), yard.at(run, v), 4.8);
    }
    for c in sk.base_spots() {
        let q = yard.local(c);
        let d = (sk.heart - c).normalize_or(Vec2::X);
        let mouth = c + d * (PAD * 1.6 + 7.0);
        // Round the yard, not through it: to the end of the nearer road.
        let v = if q.y < (lo + hi) * 0.5 {
            lo - ROAD
        } else {
            hi + ROAD
        };
        let to = yard.at(q.x.clamp(-run, run), v);
        let inside = (yard.local(mouth).y - (lo + hi) * 0.5).abs() < (hi - lo) * 0.5 + 4.0;
        let from = if inside { c } else { mouth };
        lay(sk, rng, (from, to), 4.6, Rank::Lane, false);
    }
    city::crossings(sk);
    city::cordon(sk, rng, &mouths);
}

/// How far out from the outermost tracks the roads run.
const ROAD: f32 = 21.0;

fn straight(sk: &mut Skeleton, from: Vec2, to: Vec2, half: f32) {
    sk.streets.push(Street {
        from,
        to,
        half,
        clear: 3.0,
        rank: Rank::Lane,
        out: false,
    });
}

/// Cars standing on the sidings, a few together, a way through between one
/// lot and the next. Few on the main line.
fn cars(sk: &mut Skeleton, rng: &mut Rng, yard: &Yard) {
    for (k, &v) in yard.tracks.iter().enumerate() {
        let run = yard.half * 0.82;
        let mut u = -run + rng.range(0.0, 10.0);
        while u < run {
            let n = if k == 0 { rng.int(1, 2) } else { rng.int(1, 3) };
            let half = n as f32 * CAR * 0.5;
            let b = OBox {
                center: yard.at(u + half, v),
                yaw: yard.yaw(),
                half: Vec2::new(1.55, half),
                ..OBox::default()
            };
            let stands = u + half * 2.0 < run
                && rng.chance(if k == 0 { 0.4 } else { 0.93 })
                && b.distance(sk.heart) > 12.0
                && clear_of(sk, &b, Rank::Rail)
                && sk.bases.iter().all(|s| b.distance(s.middle()) > PAD + 3.0);
            if stands {
                sk.lots.push(lot(b, Part::Cars));
            }
            u += half * 2.0 + rng.range(7.5, 11.0);
        }
    }
}

/// How long a car is, buffer to buffer.
pub const CAR: f32 = 10.0;

/// Gantries over two tracks and the lane between.
fn gantries(sk: &mut Skeleton, rng: &mut Rng, yard: &Yard) {
    let v = yard.sorted();
    let mut want = rng.int(3, 4);
    for _ in 0..60 {
        if want == 0 {
            break;
        }
        let i = rng.index(v.len() - 1);
        let gap = v[i + 1] - v[i];
        // (Legs need a lane to stand in, or none beyond.)
        let wide = |g: f32| g > 11.0;
        let outside =
            (i == 0 || wide(v[i] - v[i - 1])) && (i + 2 >= v.len() || wide(v[i + 2] - v[i + 1]));
        if !wide(gap) || !outside {
            continue;
        }
        let b = OBox {
            center: yard.at(rng.range(-0.75, 0.75) * yard.half, (v[i] + v[i + 1]) * 0.5),
            yaw: yard.yaw(),
            half: Vec2::new(gap * 0.5 + 3.6, 3.6),
            ..OBox::default()
        };
        let apart = sk
            .lots
            .iter()
            .filter(|l| l.kind == LotKind::Works(Part::Gantry))
            .all(|l| l.footprint.center.distance(b.center) > 34.0);
        // Its legs off the cars and the roads.
        let (ax, az) = b.axes();
        let clear = [-1.0f32, 1.0].iter().all(|&x| {
            [-1.0f32, 1.0].iter().all(|&z| {
                let p = b.center + ax * x * (b.half.x - 0.7) + az * z * (b.half.y - 0.7);
                sk.streets
                    .iter()
                    .filter(|s| s.rank != Rank::Rail)
                    .all(|s| s.distance(p) > s.half + 1.0)
                    && sk.lots.iter().all(|l| l.footprint.distance(p) > 1.0)
            })
        });
        if apart && clear && b.distance(sk.heart) > 14.0 {
            sk.lots.push(lot(b, Part::Gantry));
            want -= 1;
        }
    }
}

/// Round the yard: docks, sheds, cranes and stores down its long sides,
/// water towers by the sidings' ends.
fn sides(sk: &mut Skeleton, rng: &mut Rng, yard: &Yard) {
    let (lo, hi) = yard.edges();
    let put = |sk: &mut Skeleton, rng: &mut Rng, u: f32, v: f32, part: Part, turn: f32| {
        let b = OBox {
            center: yard.at(u, v),
            yaw: yard.yaw() + turn,
            half: part.half(rng),
            ..OBox::default()
        };
        if room(sk, &b, 5.4) && off_roads(sk, &b, 0.8) {
            sk.lots.push(lot(b, part));
        }
    };
    // A tower at each end at least, where there's room.
    for e in [-1.0f32, 1.0] {
        for _ in 0..rng.int(8, 14) {
            let v = rng.range(lo - 8.0, hi + 8.0);
            let u = e * yard.half * rng.range(0.96, 1.2);
            put(sk, rng, u, v, Part::WaterTower, 0.0);
        }
    }
    // Docks against the outer tracks.
    for (v, e) in [(lo, -1.0f32), (hi, 1.0)] {
        let mut u = -yard.half * 0.8;
        while u < yard.half * 0.8 {
            let part = [Part::Dock, Part::Crane, Part::Dock, Part::Bunker][rng.index(4)];
            // (A crane stands end on.)
            let (w, turn) = match part {
                Part::Crane => (8.6, FRAC_PI_2),
                _ => (7.8, 0.0),
            };
            put(sk, rng, u, v + e * w, part, turn);
            u += rng.range(15.0, 22.0);
        }
        // Past the road.
        let mut u = -yard.half * 1.05;
        while u < yard.half * 1.05 {
            let part = [
                Part::Shed,
                Part::Shed,
                Part::Office,
                Part::Tanks,
                Part::Rack,
                Part::Gatehouse,
                Part::Wall,
            ][rng.index(7)];
            let v = v + e * rng.range(ROAD + 12.0, ROAD + 26.0);
            let turn = rng.range(-0.08, 0.08);
            put(sk, rng, u, v, part, turn);
            u += rng.range(10.0, 16.0);
        }
    }
    // Out on the land round it.
    for _ in 0..500 {
        let p = Vec2::new(
            rng.range(14.0, sk.size - 14.0),
            rng.range(14.0, sk.size - 14.0),
        );
        let part = [
            Part::Shed,
            Part::Office,
            Part::WaterTower,
            Part::Wall,
            Part::Bunker,
            Part::Wall,
        ][rng.index(6)];
        let bare = sk.lots.iter().all(|l| l.footprint.distance(p) > 13.0);
        let q = yard.local(p);
        if bare && (q.x.abs() > yard.half + 14.0 || q.y < lo - ROAD - 8.0 || q.y > hi + ROAD + 8.0)
        {
            let turn = rng.range(0.0, TAU);
            put(sk, rng, q.x, q.y, part, turn);
        }
    }
}
