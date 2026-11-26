//! How a works is laid out, on the shared skeleton and land. In order: the
//! land; the plant (the big buildings, where the land takes them and turned
//! with it); the rails and roads between them, bridged and tunnelled where
//! they cross a valley or a ridge; the yard (medium structures along the
//! roads and round the plant); then cover on what's left.
//!
//! Whose works it is bends the plan a little (`Habit`): where the next
//! building goes and how it's turned, nothing else. Nothing says the sides
//! start as far from the fight as each other.

use std::f32::consts::{FRAC_PI_2, TAU};

use glam::Vec2;

use super::parts::Part;
use crate::map::OBox;
use crate::map::procgen::Look;
use crate::map::procgen::city;
use crate::map::procgen::land::Land;
use crate::map::procgen::skeleton::*;
use crate::rng::Rng;

/// The open ground kept round a side's spawns.
const PAD: f32 = 21.0;

/// How a people lays its plant out.
#[derive(Clone, Copy)]
enum Habit {
    /// Along an axis, near enough.
    Axis(f32),
    /// Round a yard, facing it.
    Court,
    /// Wherever, but square to each other.
    Square(f32),
    /// In a loose ring.
    Ring,
}

pub fn skeleton(rng: &mut Rng, look: Look) -> Skeleton {
    let size = 20.0 * rng.int(13, 16) as f32;
    let mid = Vec2::splat(size * 0.5);
    let heart = mid + Vec2::new(rng.range(-0.15, 0.15), rng.range(-0.15, 0.15)) * size;
    let mut bases: Vec<Base> = Vec::new();
    for t in 0..4000 {
        let ease = 1.0 - t as f32 / 5000.0;
        let edge = PAD + 6.0;
        let p = Vec2::new(rng.range(edge, size - edge), rng.range(edge, size - edge));
        if bases.len() < 2
            && p.distance(heart) > 78.0 * ease
            && bases
                .iter()
                .all(|b| b.middle().distance(p) > size * 0.5 * ease)
        {
            bases.push(Base::pad(p, PAD));
        }
    }
    let mut avoid: Vec<(Vec2, f32)> = bases.iter().map(|b| (b.middle(), PAD + 4.0)).collect();
    avoid.push((heart, 34.0));
    let mut sk = Skeleton {
        size,
        land: Land::rolling(rng, size, &avoid),
        streets: Vec::new(),
        squares: Vec::new(),
        landmarks: Vec::new(),
        heart,
        bases,
        lots: Vec::new(),
        spans: Vec::new(),
        edges: Vec::new(),
        decos: Vec::new(),
        repair_spots: Vec::new(),
        crossings: Vec::new(),
        rails: Vec::new(),
    };
    let a = rng.range(0.0, TAU);
    let habit = match look {
        Look::Imperium => Habit::Axis(a),
        Look::Resonant => Habit::Court,
        Look::Uthenn => Habit::Square(a),
        Look::Ithessar => Habit::Ring,
    };
    plant(&mut sk, rng, habit);
    railway(&mut sk, rng);
    roads(&mut sk, rng);
    bed(&mut sk);
    yard(&mut sk, rng);
    cover(&mut sk, rng);
    repairs(&mut sk, rng);
    sk.squares.push(Square {
        center: heart,
        radius: 15.0,
    });
    city::level(&mut sk);
    sk
}

/// How a box lies along the land: across the slope's fall.
fn lie(sk: &Skeleton, p: Vec2) -> Option<f32> {
    let e = 12.0;
    let d = |v: Vec2| sk.land.swell(p + v * e) - sk.land.swell(p - v * e);
    let fall = Vec2::new(d(Vec2::X), d(Vec2::Y)) / (2.0 * e);
    (fall.length() > 0.03).then(|| (-fall.y).atan2(fall.x))
}

/// Room for `b`, `gap` m clear of what's down, on ground it can be put on.
fn room(sk: &Skeleton, b: &OBox, gap: f32) -> bool {
    let pts: Vec<Vec2> = b.corners().into_iter().chain([b.center]).collect();
    let heights = pts.iter().map(|&p| sk.land.swell(p));
    let (lo, hi) = heights.fold((f32::MAX, f32::MIN), |(l, h), x| (l.min(x), h.max(x)));
    let cuts = sk.land.cuts();
    hi - lo < 1.0 + b.half.length() * 0.13
        && pts.iter().all(|&p| {
            p.min_element() > 9.0
                && p.max_element() < sk.size - 9.0
                && cuts.iter().all(|c| !(0.02..0.98).contains(&c.depth(p)))
        })
        && sk.bases.iter().all(|k| b.distance(k.middle()) > PAD + gap)
        && b.distance(sk.heart) > 17.0
        && sk.lots.iter().all(|l| !l.footprint.overlaps(b, gap))
}

/// The big buildings: each off one that's down already, turned with the
/// land where it falls.
fn plant(sk: &mut Skeleton, rng: &mut Rng, habit: Habit) {
    let want = 4 + (sk.size / 70.0) as usize + rng.index(2);
    let mut kinds = vec![Part::Smelter, Part::Power, Part::Mill, Part::Casting];
    let more = [
        Part::Casting,
        Part::Assembly,
        Part::Ore,
        Part::Smelter,
        Part::Mill,
        Part::Cooling,
    ];
    while kinds.len() < want {
        kinds.push(more[rng.index(more.len())]);
    }
    for i in (2..kinds.len()).rev() {
        kinds.swap(i, 1 + rng.index(i));
    }
    for (n, kind) in kinds.into_iter().enumerate() {
        let half = kind.half(rng);
        for t in 0..400 {
            // Anywhere, if nowhere near takes it.
            let spread = 1.0 + t as f32 / 160.0;
            let from = match sk.lots.len() {
                0 => sk.heart,
                k => sk.lots[rng.index(k)].at(),
            };
            let any = rng.range(0.0, TAU);
            let (at, turn) = match habit {
                Habit::Axis(a) => {
                    let d = Vec2::from_angle(a);
                    let s = rng.range(-1.0, 1.0) * sk.size * 0.42;
                    let off = rng.range(-1.0, 1.0) * 46.0 * spread;
                    (sk.heart + d * s + d.perp() * off, Some(-a + FRAC_PI_2))
                }
                Habit::Court => {
                    let d = Vec2::from_angle(any);
                    let r = rng.range(30.0, 62.0) * spread;
                    (sk.heart + d * r, Some(-any + FRAC_PI_2))
                }
                Habit::Ring => {
                    let a = n as f32 * 2.4 + rng.range(-0.5, 0.5) * spread;
                    let r = sk.size * rng.range(0.2, 0.36);
                    (sk.heart + Vec2::from_angle(a) * r, Some(-a))
                }
                Habit::Square(a) => {
                    let r = half.length() + rng.range(24.0, 52.0) * spread;
                    (from + Vec2::from_angle(any) * r, Some(-a))
                }
            };
            let lean = rng.range(-0.22, 0.22);
            let yaw = match (habit, lie(sk, at), turn) {
                (Habit::Square(_), _, Some(y)) => y + FRAC_PI_2 * rng.index(2) as f32,
                (_, Some(y), _) => y + lean,
                (_, None, Some(y)) => y + lean * 2.0,
                (_, None, None) => any,
            };
            let b = OBox {
                center: at,
                yaw,
                half,
                ..OBox::default()
            };
            if room(sk, &b, 15.0) {
                sk.lots.push(lot(b, kind));
                break;
            }
        }
    }
}

fn lot(footprint: OBox, part: Part) -> Lot {
    Lot {
        footprint,
        front: (1, 1.0),
        kind: LotKind::Works(part),
    }
}

/// The nearest point of any road to `p`.
fn nearest(sk: &Skeleton, p: Vec2) -> Option<Vec2> {
    sk.streets
        .iter()
        .filter(|s| matches!(s.rank, Rank::Lane | Rank::Avenue))
        .map(|s| {
            let d = s.to - s.from;
            let t = ((p - s.from).dot(d) / d.length_squared()).clamp(0.0, 1.0);
            s.from + d * t
        })
        .min_by(|a, b| a.distance(p).total_cmp(&b.distance(p)))
}

/// A road's cell.
const CELL: f32 = 5.0;
/// How far past the edge roads are found.
const OUT: f32 = 10.0;

/// The way a road `half` wide would be built from `a` to `b`: round what
/// stands, along the land, down a road that's there already if it's on the
/// way, over a valley or through a ridge where that's a good deal shorter.
fn way(sk: &Skeleton, rng: &mut Rng, (a, b): (Vec2, Vec2), half: f32) -> Option<Vec<Vec2>> {
    let n = ((sk.size + 2.0 * OUT) / CELL) as usize + 1;
    let at = |i: usize| Vec2::new((i % n) as f32, (i / n) as f32) * CELL - Vec2::splat(OUT);
    let cell = |p: Vec2| {
        let q = ((p + Vec2::splat(OUT)) / CELL)
            .round()
            .clamp(Vec2::ZERO, Vec2::splat((n - 1) as f32));
        q.y as usize * n + q.x as usize
    };
    let cuts = sk.land.cuts();
    let salt = rng.int(0, 900) as u32;
    let cost: Vec<f32> = (0..n * n)
        .map(|i| {
            let p = at(i);
            let near = p.distance(a).min(p.distance(b)) < CELL * 1.5;
            let inside = sk.bases.iter().any(|k| k.middle().distance(p) < PAD + half);
            let stood = sk.lots.iter().any(|l| l.footprint.distance(p) < half + 1.5);
            if (stood || inside) && !near {
                return f32::INFINITY;
            }
            let on = sk
                .streets
                .iter()
                .any(|s| s.rank != Rank::Rail && s.distance(p) < s.half);
            let cut = cuts.iter().any(|c| c.depth(p) > 0.03);
            // A bridge costs what a fair way round does; up a bank, more.
            let slope = if cut { 1.6 } else { sk.land.steep(p) * 9.0 };
            let bends = crate::map::valley::noise2(p.x / 31.0, p.y / 31.0, salt) * 1.6;
            (1.0 + slope + bends) * if on { 0.35 } else { 1.0 }
        })
        .collect();
    let (from, to) = (cell(a), cell(b));
    let mut best = vec![f32::INFINITY; n * n];
    let mut came = vec![usize::MAX; n * n];
    let mut open = std::collections::BTreeSet::new();
    let key = |c: f32| (c * 64.0) as u64;
    best[from] = 0.0;
    open.insert((0u64, from));
    while let Some((_, i)) = open.pop_first() {
        if i == to {
            break;
        }
        let (x, z) = ((i % n) as i32, (i / n) as i32);
        for dz in -1..=1 {
            for dx in -1..=1 {
                let (u, v) = (x + dx, z + dz);
                if (dx, dz) == (0, 0) || u < 0 || v < 0 || u >= n as i32 || v >= n as i32 {
                    continue;
                }
                let k = v as usize * n + u as usize;
                let c = best[i] + cost[k] * ((dx * dx + dz * dz) as f32).sqrt();
                if c < best[k] {
                    best[k] = c;
                    came[k] = i;
                    open.insert((key(c), k));
                }
            }
        }
    }
    if !best[to].is_finite() {
        return None;
    }
    let mut cells = vec![b];
    let mut i = to;
    while i != from {
        i = came[i];
        cells.push(at(i));
    }
    *cells.last_mut().unwrap() = a;
    cells.reverse();
    // Pulled straight between its bends, where nothing's in the way and it
    // strays little; then the corners cut off.
    let clear = |p: Vec2, q: Vec2| {
        let m = (p.distance(q) / 2.5).ceil().max(1.0) as i32;
        (0..=m).all(|k| cost[cell(p.lerp(q, k as f32 / m as f32))].is_finite())
    };
    let mut pts = vec![cells[0]];
    let mut k = 0;
    while k + 1 < cells.len() {
        let mut j = k + 1;
        while j + 1 < cells.len() {
            let (p, q) = (cells[k], cells[j + 1]);
            let d = (q - p).normalize_or(Vec2::X);
            let strays = cells[k..=j]
                .iter()
                .any(|c| d.perp_dot(*c - p).abs() > CELL * 0.9);
            if strays || !clear(p, q) {
                break;
            }
            j += 1;
        }
        pts.push(cells[j]);
        k = j;
    }
    let mut out = vec![pts[0]];
    for w in pts.windows(3) {
        out.push(w[1].lerp(w[0], 0.28));
        out.push(w[1].lerp(w[2], 0.28));
    }
    out.push(*pts.last().unwrap());
    out.dedup_by(|p, q| p.distance(*q) < 1.0);
    Some(out)
}

/// Builds a way from `a` to `b`, the stretches that aren't road already.
fn lay(
    sk: &mut Skeleton,
    rng: &mut Rng,
    ends: (Vec2, Vec2),
    half: f32,
    rank: Rank,
    out: bool,
) -> bool {
    let Some(pts) = way(sk, rng, ends, half) else {
        return false;
    };
    let n = pts.len() - 1;
    let there: Vec<bool> = pts
        .windows(2)
        .map(|w| {
            rank != Rank::Rail
                && [0.0, 0.5, 1.0].iter().all(|&t| {
                    let p = w[0].lerp(w[1], t);
                    sk.streets
                        .iter()
                        .any(|s| s.rank != Rank::Rail && s.distance(p) < s.half * 0.8)
                })
        })
        .collect();
    for (k, w) in pts.windows(2).enumerate() {
        if !there[k] {
            sk.streets.push(Street {
                from: w[0],
                to: w[1],
                half,
                clear: 3.0,
                rank,
                out: out && k + 1 == n,
            });
        }
    }
    true
}

/// Corners cut off, twice: as easy a curve as a train wants.
fn eased(pts: &[Vec2]) -> Vec<Vec2> {
    let mut line = pts.to_vec();
    for _ in 0..2 {
        let mut out = vec![line[0]];
        for w in line.windows(3) {
            out.push(w[1].lerp(w[0], 0.25));
            out.push(w[1].lerp(w[2], 0.25));
        }
        out.push(*line.last().unwrap());
        out.dedup_by(|p, q| p.distance(*q) < 0.5);
        line = out;
    }
    line
}

/// How far past the edge a line's seen running on.
const RUN_ON: f32 = 150.0;

/// The railway: in over one edge, by the buildings it serves, out over
/// another; then spurs off it to others.
fn railway(sk: &mut Skeleton, rng: &mut Rng) {
    let size = sk.size;
    let half = 2.6;
    // Beside a building, along its long wall.
    let halt = |l: &Lot, s: f32| {
        let (ax, az) = l.footprint.axes();
        let h = l.footprint.half;
        let off = ax * s * (h.x + half + 3.0);
        (l.at() + off - az * h.y, l.at() + off + az * h.y)
    };
    let clear = |sk: &Skeleton, p: Vec2| {
        p.min_element() > 8.0
            && p.max_element() < size - 8.0
            && sk.lots.iter().all(|l| l.footprint.distance(p) > half + 1.6)
            && sk.bases.iter().all(|b| b.middle().distance(p) > PAD + half)
    };
    let big: Vec<Lot> = sk.lots.clone();
    let add = |sk: &mut Skeleton, pts: &[Vec2]| {
        for w in pts.windows(2) {
            sk.streets.push(Street {
                from: w[0],
                to: w[1],
                half,
                clear: 2.0,
                rank: Rank::Rail,
                out: false,
            });
        }
    };
    for _ in 0..60 {
        let (k, turn) = (rng.index(4), 1 + rng.index(3));
        let ends = [k, k + turn].map(|e| {
            let (start, along, inward) = city::sides(size)[e % 4];
            (start + along * size * rng.range(0.15, 0.85), -inward)
        });
        let serve = big[rng.index(big.len())];
        let (p, q) = halt(&serve, if rng.chance(0.5) { 1.0 } else { -1.0 });
        if ends[0].0.distance(ends[1].0) < size * 0.6
            || ![p, q, p.lerp(q, 0.5)].iter().all(|&x| clear(sk, x))
        {
            continue;
        }
        let (p, q) = if p.distance(ends[0].0) < q.distance(ends[0].0) {
            (p, q)
        } else {
            (q, p)
        };
        let (Some(a), Some(b)) = (
            way(sk, rng, (ends[0].0, p), half),
            way(sk, rng, (q, ends[1].0), half),
        ) else {
            continue;
        };
        let pts: Vec<Vec2> = a.into_iter().chain(b).collect();
        // Not back on itself.
        let sharp = pts.windows(3).any(|w| {
            let (d, e) = (w[1] - w[0], w[2] - w[1]);
            d.normalize_or(Vec2::X).dot(e.normalize_or(Vec2::X)) < 0.1
        });
        if sharp {
            continue;
        }
        let line = eased(&pts);
        add(sk, &line);
        let mut whole = vec![ends[0].0 + ends[0].1 * RUN_ON];
        whole.extend(&line);
        whole.push(ends[1].0 + ends[1].1 * RUN_ON);
        sk.rails.push(whole);
        break;
    }
    let Some(main) = sk.rails.first().cloned() else {
        return;
    };
    // Spurs: off the line, up to another building's wall.
    let mut want = rng.int(1, 2);
    for _ in 0..40 {
        if want == 0 {
            break;
        }
        let l = big[rng.index(big.len())];
        let (p, q) = halt(&l, if rng.chance(0.5) { 1.0 } else { -1.0 });
        let k = 2 + rng.index(main.len().saturating_sub(4).max(1));
        let Some(&from) = main.get(k) else { continue };
        let far = main
            .iter()
            .all(|m| m.distance(p) > 22.0 && m.distance(q) > 22.0);
        if !far || !clear(sk, p) || !clear(sk, q) || !clear(sk, from) {
            continue;
        }
        let (p, q) = if p.distance(from) < q.distance(from) {
            (p, q)
        } else {
            (q, p)
        };
        if let Some(mut pts) = way(sk, rng, (from, p), half) {
            pts.push(q);
            // It leaves the line the way the line runs, near enough.
            let lead = (main[k] - main[k - 1]).normalize_or(Vec2::X);
            let turn = (pts[1] - pts[0]).normalize_or(Vec2::X).dot(lead).abs();
            if turn < 0.5 {
                continue;
            }
            let line = eased(&pts);
            add(sk, &line);
            sk.rails.push(line);
            want -= 1;
        }
    }
}

/// Grades the ground under the rails, off the bridges and tunnels.
fn bed(sk: &mut Skeleton) {
    for line in sk.rails.clone() {
        let mut fine = Vec::new();
        for w in line.windows(2) {
            let n = (w[0].distance(w[1]) / 4.0).ceil().max(1.0) as i32;
            fine.extend((0..n).map(|k| w[0].lerp(w[1], k as f32 / n as f32)));
        }
        let h: Vec<f32> = fine.iter().map(|&p| sk.land.base(p)).collect();
        for k in 1..fine.len().saturating_sub(1) {
            let p = fine[k];
            let outside = p.min_element() < -8.0 || p.max_element() > sk.size + 8.0;
            if outside || sk.crossings.iter().any(|c| c.distance(p) < 16.0) {
                continue;
            }
            let (a, b) = (k.saturating_sub(5), (k + 6).min(h.len()));
            let mean = h[a..b].iter().sum::<f32>() / (b - a) as f32;
            let d = fine[k + 1] - fine[k - 1];
            let strip = OBox {
                center: p,
                yaw: d.x.atan2(d.y),
                half: Vec2::new(1.6, 2.4),
                ..OBox::default()
            };
            sk.land.level_at(strip, 0.8, 5.0, mean);
        }
    }
}

/// The haul road past the heart, a road to every door
/// and base, and a few more to make loops of it.
fn roads(sk: &mut Skeleton, rng: &mut Rng) {
    let size = sk.size;
    let edge = |rng: &mut Rng, k: usize| {
        let (start, along, _) = city::sides(size)[k % 4];
        start + along * size * rng.range(0.2, 0.8)
    };
    // The haul road: in at one side, by the heart, out at another.
    let k = rng.index(4);
    let on = k + 1 + rng.index(3);
    let heart = sk.heart;
    let mut mouths: city::Mouths = Default::default();
    for side in [k, on] {
        for _ in 0..12 {
            let e = edge(rng, side);
            if lay(sk, rng, (heart, e), 6.5, Rank::Avenue, true) {
                let (start, along, _) = city::sides(size)[side % 4];
                let t = (e - start).dot(along);
                mouths[side % 4].push((t - 9.0, t + 9.0));
                break;
            }
        }
    }
    // Doors and bases onto the nearest road so far, nearest first.
    let mut doors: Vec<Vec2> = sk
        .lots
        .iter()
        .flat_map(|l| {
            let (ax, az) = l.footprint.axes();
            let h = l.footprint.half;
            [
                l.at() + az * (h.y + 8.0),
                l.at() - az * (h.y + 8.0),
                l.at() + ax * (h.x + 8.0),
            ]
        })
        .collect();
    doors.sort_by(|a, b| a.distance(heart).total_cmp(&b.distance(heart)));
    let bases: Vec<(Vec2, Vec2)> = sk
        .bases
        .iter()
        .map(|b| {
            let d = (heart - b.middle()).normalize();
            (b.middle(), d)
        })
        .collect();
    for &(c, d) in &bases {
        let mouth = c + Vec2::from_angle(rng.range(-0.5, 0.5)).rotate(d) * (PAD * 1.6 + 7.0);
        doors.push(mouth);
    }
    for p in doors {
        let half = rng.range(4.2, 5.4);
        if let Some(q) = nearest(sk, p)
            && q.distance(p) > half + 5.0
        {
            lay(sk, rng, (p, q), half, Rank::Lane, false);
        }
    }
    // Loops: between things a fair way apart.
    for _ in 0..rng.int(5, 8) {
        let any = |rng: &mut Rng, sk: &Skeleton| {
            let s = sk.streets[rng.index(sk.streets.len())];
            s.from.lerp(s.to, 0.5)
        };
        if sk.streets.is_empty() {
            break;
        }
        for _ in 0..20 {
            let (a, b) = (any(rng, sk), any(rng, sk));
            let d = a.distance(b);
            if (70.0..180.0).contains(&d)
                && a.min(b).min_element() > 20.0
                && a.max(b).max_element() < size - 20.0
                && lay(sk, rng, (a, b), 4.6, Rank::Lane, false)
            {
                break;
            }
        }
    }
    city::crossings(sk);
    city::cordon(sk, rng, &mouths);
}

/// Off every road by `gap`.
fn off_roads(sk: &Skeleton, b: &OBox, gap: f32) -> bool {
    let (ax, az) = b.axes();
    let pts = (-2..=2).flat_map(|i| {
        (-2..=2).map(move |j| {
            b.center + ax * b.half.x * i as f32 * 0.5 + az * b.half.y * j as f32 * 0.5
        })
    });
    let streets: Vec<&Street> = sk
        .streets
        .iter()
        .filter(|s| s.distance(b.center) < b.half.length() + s.half + gap)
        .collect();
    pts.into_iter()
        .all(|p| streets.iter().all(|s| s.distance(p) > s.half + gap))
}

/// The yard: what stands along the roads, the rails and the buildings'
/// walls, each turned the way what it serves runs.
fn yard(sk: &mut Skeleton, rng: &mut Rng) {
    // Over a road: pipe bridges, cranes on portals.
    let over: Vec<Street> = sk
        .streets
        .iter()
        .copied()
        .filter(|s| matches!(s.rank, Rank::Lane | Rank::Avenue) && s.len() > 12.0)
        .collect();
    for s in over {
        if !rng.chance(0.22) {
            continue;
        }
        let d = s.dir();
        let part = if rng.chance(0.5) {
            Part::PipeBridge
        } else {
            Part::Portal
        };
        let deep = if part == Part::Portal { 5.0 } else { 1.2 };
        let b = OBox {
            center: s.from.lerp(s.to, 0.5),
            yaw: d.x.atan2(d.y),
            half: Vec2::new(s.half + 2.6, deep),
            ..OBox::default()
        };
        if room(sk, &b, 4.0) {
            sk.lots.push(lot(b, part));
        }
    }
    // Along the rails: wagons standing on it, docks and bunkers by it.
    let rails: Vec<Street> = sk
        .streets
        .iter()
        .copied()
        .filter(|s| s.rank == Rank::Rail)
        .collect();
    for s in &rails {
        let d = s.dir();
        let yaw = d.x.atan2(d.y);
        if s.len() > 14.0 && rng.chance(0.55) {
            let b = OBox {
                center: s.from.lerp(s.to, rng.range(0.4, 0.6)),
                yaw,
                half: Vec2::new(1.5, (s.len() * 0.5 - 2.0).min(rng.range(4.5, 10.0))),
                ..OBox::default()
            };
            if room(sk, &b, 3.0) && clear_of(sk, &b, Rank::Rail) {
                sk.lots.push(lot(b, Part::Wagons));
            }
        }
    }
    // Beside everything.
    let sides: Vec<Street> = sk
        .streets
        .iter()
        .copied()
        .filter(|s| !matches!(s.rank, Rank::Beyond | Rank::Crossing))
        .collect();
    for s in sides {
        let d = s.dir();
        let mut t = rng.range(0.0, 14.0);
        while t < s.len() {
            for e in [-1.0f32, 1.0] {
                if rng.chance(0.2) {
                    continue;
                }
                let part = if s.rank == Rank::Rail {
                    [Part::Dock, Part::Bunker, Part::Crane][rng.index(3)]
                } else {
                    Part::YARD[rng.index(Part::YARD.len())]
                };
                let half = part.half(rng);
                let b = OBox {
                    center: s.from
                        + d * t
                        + Vec2::new(-d.y, d.x) * e * (s.half + half.x + rng.range(1.4, 5.0)),
                    yaw: d.x.atan2(d.y) + rng.range(-0.12, 0.12),
                    half,
                    ..OBox::default()
                };
                if room(sk, &b, 5.4) && off_roads(sk, &b, 0.8) {
                    sk.lots.push(lot(b, part));
                }
            }
            t += rng.range(8.0, 19.0);
        }
    }
    // Yards off the roads: a few things together, turned together.
    for _ in 0..260 {
        let p = Vec2::new(
            rng.range(14.0, sk.size - 14.0),
            rng.range(14.0, sk.size - 14.0),
        );
        let open = sk
            .lots
            .iter()
            .map(|l| l.footprint.distance(p))
            .fold(f32::MAX, f32::min);
        if open < 9.0 {
            continue;
        }
        let yaw = lie(sk, p).unwrap_or_else(|| rng.range(0.0, TAU));
        let (ax, az) = (Vec2::from_angle(-yaw), Vec2::from_angle(-yaw).perp());
        let part = Part::YARD[rng.index(Part::YARD.len())];
        for _ in 0..rng.int(2, 5) {
            let part = if rng.chance(0.55) {
                part
            } else {
                Part::YARD[rng.index(Part::YARD.len())]
            };
            let q = p + ax * rng.range(-18.0, 18.0) + az * rng.range(-18.0, 18.0);
            let b = OBox {
                center: q,
                yaw: yaw + FRAC_PI_2 * rng.index(2) as f32 + rng.range(-0.05, 0.05),
                half: part.half(rng),
                ..OBox::default()
            };
            if room(sk, &b, 5.4) && off_roads(sk, &b, 0.8) {
                sk.lots.push(lot(b, part));
            }
        }
    }
    // Conveyors, high from one big building to another near it.
    let big: Vec<OBox> = sk
        .lots
        .iter()
        .filter(|l| matches!(l.kind, LotKind::Works(p) if p.big()))
        .map(|l| l.footprint)
        .collect();
    let mut belts: Vec<OBox> = Vec::new();
    for (i, a) in big.iter().enumerate() {
        for b in &big[i + 1..] {
            let d = b.center - a.center;
            let (ra, rb) = (reach(a, d), reach(b, -d));
            let gap = d.length() - ra - rb;
            if !(14.0..60.0).contains(&gap) || !rng.chance(0.6) {
                continue;
            }
            let dir = d.normalize();
            let belt = OBox {
                center: a.center + dir * (ra + gap * 0.5),
                yaw: dir.x.atan2(dir.y),
                half: Vec2::new(1.3, gap * 0.5 + 1.0),
                ..OBox::default()
            };
            let foul = big
                .iter()
                .filter(|o| o.center != a.center && o.center != b.center)
                .any(|o| o.overlaps(&belt, 1.0))
                || belts.iter().any(|o| o.overlaps(&belt, 1.0));
            let cuts = sk.land.cuts();
            let level = (0..=8).all(|k| {
                let p = a.center.lerp(b.center, k as f32 / 8.0);
                cuts.iter().all(|c| c.depth(p) < 0.02)
            });
            if !foul && level {
                belts.push(belt);
            }
        }
    }
    sk.lots
        .extend(belts.into_iter().map(|b| lot(b, Part::Conveyor)));
}

/// No road but one of `rank` under `b`.
fn clear_of(sk: &Skeleton, b: &OBox, rank: Rank) -> bool {
    let az = b.axes().1;
    (-4..=4).all(|k| {
        let p = b.center + az * b.half.y * k as f32 / 4.0;
        sk.streets
            .iter()
            .all(|s| s.rank == rank || s.distance(p) > s.half + 2.0)
    })
}

/// How far from `b`'s middle its edge is along `d`.
fn reach(b: &OBox, d: Vec2) -> f32 {
    let d = d.normalize();
    let (ax, az) = b.axes();
    (b.half.x / d.dot(ax).abs().max(1e-3)).min(b.half.y / d.dot(az).abs().max(1e-3))
}

/// What's left: something to hide behind wherever there's nothing near.
fn cover(sk: &mut Skeleton, rng: &mut Rng) {
    for _ in 0..1600 {
        let p = Vec2::new(
            rng.range(10.0, sk.size - 10.0),
            rng.range(10.0, sk.size - 10.0),
        );
        let part = Part::SMALL[rng.index(Part::SMALL.len())];
        let b = OBox {
            center: p,
            yaw: rng.range(0.0, TAU),
            half: part.half(rng),
            ..OBox::default()
        };
        let bare = sk.lots.iter().all(|l| l.footprint.distance(p) > 15.0);
        if bare && room(sk, &b, 5.0) && off_roads(sk, &b, 0.4) {
            sk.lots.push(lot(b, part));
        }
    }
}

/// Repair drops: out by the roads, under open sky, well apart.
fn repairs(sk: &mut Skeleton, rng: &mut Rng) {
    for t in 0..2000 {
        if sk.repair_spots.len() >= 5 {
            break;
        }
        let ease = 1.0 - t as f32 / 2500.0;
        let s = sk.streets[rng.index(sk.streets.len())];
        let p = s.from.lerp(s.to, rng.f32());
        if matches!(s.rank, Rank::Lane | Rank::Avenue)
            && p.min_element() > 16.0
            && p.max_element() < sk.size - 16.0
            && sk.lots.iter().all(|l| l.footprint.distance(p) > 6.5)
            && sk.crossings.iter().all(|c| c.distance(p) > 14.0)
            && sk.land.steep(p) < 0.12
            && sk.repair_spots.iter().all(|q| q.distance(p) > 70.0 * ease)
        {
            sk.repair_spots.push(p);
        }
    }
}
