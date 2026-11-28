//! How a stretch of hoard is laid out: a tract's railways (`network`), a
//! window cut out of it somewhere worth fighting over, and every bit of
//! ground that isn't line packed with cubes, as high as it bears.
//!
//! In order: the window (`Stretch`); which lattice squares are open (lines,
//! yards, pads, cuts and galleries through the blocks), the rest stacked;
//! the tracks and what stands on them.

use std::f32::consts::{PI, TAU};

use glam::Vec2;

use super::network::{Network, TRACT};
use crate::map::OBox;
use crate::map::noise::noise2;
use crate::map::procgen::city;
use crate::map::procgen::foundry::plan::{PAD, bed, lot, repairs};
use crate::map::procgen::foundry::{CUBE, Part};
use crate::map::procgen::land::{Land, Swell};
use crate::map::procgen::mine::seg;
use crate::map::procgen::skeleton::*;
use crate::rng::Rng;

/// What part of the network a window's on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stretch {
    /// The trunk, side lines off it.
    Trunk,
    /// Where a big line splits.
    Fork,
    /// The growing edge: blocks half built, then bare ground.
    Front,
}

impl Stretch {
    pub const ALL: [Stretch; 3] = [Stretch::Front, Stretch::Fork, Stretch::Trunk];
}

const SIZE: f32 = 300.0;
/// The open ground round the heart, and round a pad.
const YARD: f32 = 20.0;
const PADDED: f32 = PAD + 4.0;
/// Half a cut's, or gallery's, width.
const CUT: f32 = 7.5;
/// How far past the edge it's seen to go on.
pub const BEYOND: f32 = 130.0;
/// Between one track and the next.
const TRACK: f32 = 5.6;

/// The window, and what's open in it.
struct Window {
    turn: Vec2,
    /// Where in the tract the heart is, and on the map.
    from: Vec2,
    heart: Vec2,
    /// The network's reaches near it, on the map: ends, half widths.
    reaches: Vec<(Vec2, Vec2, f32, f32)>,
    /// Ways through the blocks; roofed or not.
    cuts: Vec<(Vec2, Vec2, bool)>,
    pads: Vec<Vec2>,
    /// The way the hoard grows, if it's still growing here.
    front: Option<Vec2>,
    salt: u32,
}

impl Window {
    fn to_map(&self, p: Vec2) -> Vec2 {
        self.heart + self.turn.rotate(p - self.from)
    }

    /// How built up it is at `p`: none of it to all.
    fn built(&self, p: Vec2) -> f32 {
        let Some(d) = self.front else { return 1.0 };
        let n = noise2(p.x / 38.0, p.y / 38.0, self.salt) - 0.5;
        (0.42 - (p - self.heart).dot(d) / 130.0 + 0.5 * n).clamp(0.0, 1.0)
    }

    /// Courses of cubes standing at `p`, as many as the ground bears.
    fn courses(&self, p: Vec2) -> u8 {
        // Rock here, soil there: whole districts of it.
        let n = |s: f32, k: u32| noise2(p.x / s, p.y / s, self.salt + k);
        let bears = 1.4 + 5.2 * smoothstep(0.2, 0.8, n(78.0, 1));
        (bears * self.built(p)).round() as u8
    }

    /// Whether a square at `p` is open, and whether it's roofed.
    fn open(&self, p: Vec2) -> Option<bool> {
        let h = CUBE * 0.5;
        let line = |&(a, b, ha, hb): &(Vec2, Vec2, f32, f32)| {
            let (d, t) = seg(p, a, b);
            d < ha + (hb - ha) * t + h
        };
        if self.reaches.iter().any(line)
            || p.distance(self.heart) < YARD + h
            || self.pads.iter().any(|q| q.distance(p) < PADDED + h)
        {
            return Some(false);
        }
        let on = |c: &&(Vec2, Vec2, bool)| seg(p, c.0, c.1).0 < CUT + h * 0.6;
        let cut: Vec<bool> = self.cuts.iter().filter(on).map(|c| c.2).collect();
        (!cut.is_empty()).then(|| cut.iter().all(|&roofed| roofed))
    }
}

pub fn skeleton(rng: &mut Rng, stretch: Stretch) -> Skeleton {
    let net = Network::grow(rng);
    let heart =
        Vec2::splat(SIZE * 0.5) + Vec2::new(rng.range(-0.05, 0.05), rng.range(-0.05, 0.05)) * SIZE;
    let spot = pick(&net, rng, stretch);
    let turn = Vec2::from_angle(rng.range(0.0, TAU));
    let from = net.nodes[spot].at;
    let mut w = Window {
        turn,
        from,
        heart,
        reaches: Vec::new(),
        cuts: Vec::new(),
        pads: Vec::new(),
        front: None,
        salt: (rng.next_u64() >> 40) as u32,
    };
    let at: Vec<Vec2> = net.nodes.iter().map(|n| w.to_map(n.at)).collect();
    let out = |p: Vec2| (-p).max(p - Vec2::splat(SIZE)).max_element();
    for n in &net.nodes {
        let Some(f) = n.from else { continue };
        let (a, b) = (w.to_map(net.nodes[f].at), w.to_map(n.at));
        if out(a).min(out(b)) < BEYOND + 30.0 {
            w.reaches
                .push((a, b, net.nodes[f].half.min(n.half * 1.5), n.half));
        }
    }
    let inside: Vec<usize> = (0..at.len())
        .filter(|&k| out(at[k]) < -(PAD + 7.0))
        .collect();
    // The sides: on lines, as far apart as any two places in the window, or
    // nearly, and about as far from the heart.
    let mut best = (f32::MIN, heart, heart);
    for (i, &a) in inside.iter().enumerate() {
        for &b in &inside[i + 1..] {
            let (p, q) = (at[a], at[b]);
            let uneven = (p.distance(heart) - q.distance(heart)).abs();
            let score = p.distance(q) - 0.7 * uneven + rng.range(0.0, 18.0);
            if score > best.0 {
                best = (score, p, q);
            }
        }
    }
    w.pads = vec![best.1, best.2];
    if stretch == Stretch::Front {
        // Across the way between the sides: each has some of both.
        let d = (best.2 - best.1).normalize_or(Vec2::X).perp();
        let outward = turn.rotate(from).dot(d) < 0.0;
        w.front = Some(if outward { -d } else { d });
    }
    cuts(&mut w, &net, &at, &inside, rng);

    let mut land = Land::rolling(rng, SIZE, &[]);
    land.swells.retain(|s| !matches!(s, Swell::Cut(_)));
    let mut sk = Skeleton {
        size: SIZE,
        land,
        heart,
        bases: w.pads.iter().map(|&p| Base::pad(p, PAD)).collect(),
        ..Skeleton::default()
    };
    lines(&mut sk, &w, &net, &at, rng);
    bed(&mut sk);
    blocks(&mut sk, &w);
    if stretch == Stretch::Front {
        loaders(&mut sk, &w, rng);
    }
    repairs(&mut sk, rng);
    sk.squares.push(Square {
        center: heart,
        radius: 14.0,
    });
    city::level(&mut sk);
    sk
}

/// The node a window's cut round.
fn pick(net: &Network, rng: &mut Rng, stretch: Stretch) -> usize {
    // (Far enough in that there's network all round it.)
    let mut pool: Vec<usize> = (0..net.nodes.len())
        .filter(|&k| net.nodes[k].at.abs().max_element() < TRACT - 190.0)
        .collect();
    // How much the second line off a fork carries.
    let second = |k: usize| {
        let mut e: Vec<u32> = net.nodes[k]
            .next
            .iter()
            .map(|&n| net.nodes[n].ends)
            .collect();
        e.sort();
        e.len().checked_sub(2).map_or(0, |i| e[i])
    };
    match stretch {
        Stretch::Trunk => pool.sort_by_key(|&k| std::cmp::Reverse((net.nodes[k].ends, second(k)))),
        _ => pool.sort_by_key(|&k| std::cmp::Reverse(second(k))),
    }
    let first = match stretch {
        Stretch::Trunk => 12,
        _ => 5,
    };
    pool[rng.index(first.min(pool.len()))]
}

/// Ways through the blocks from one line to another that's far off by rail:
/// half of them roofed, the galleries the cubes are counted from.
fn cuts(w: &mut Window, net: &Network, at: &[Vec2], inside: &[usize], rng: &mut Rng) {
    for _ in 0..6000 {
        if w.cuts.len() >= 10 || inside.len() < 2 {
            break;
        }
        let (a, b) = (
            inside[rng.index(inside.len())],
            inside[rng.index(inside.len())],
        );
        let (p, q) = (at[a], at[b]);
        let d = p.distance(q);
        if !(28.0..125.0).contains(&d) || (net.along(a, b) as f32) * 10.0 < d * 1.7 {
            continue;
        }
        let mid = p.lerp(q, 0.5);
        let apart = w.cuts.iter().all(|c| {
            c.0.lerp(c.1, 0.5).distance(mid) > 46.0 && c.0.distance(p).min(c.1.distance(q)) > 20.0
        });
        // Through one block, not down a line.
        let shut = (1..8)
            .filter(|&k| w.open(p.lerp(q, k as f32 / 8.0)).is_none())
            .count();
        if apart && shut >= 2 {
            let roofed = w.cuts.len() % 2 == 1 && w.courses(mid) >= 3;
            w.cuts.push((p, q, roofed));
        }
    }
}

/// The tracks, side by side as many as a line has room for; a street down
/// every line and cut; cars standing where it's wide enough to pass them.
fn lines(sk: &mut Skeleton, w: &Window, net: &Network, at: &[Vec2], rng: &mut Rng) {
    let out = |p: Vec2| (-p).max(p - Vec2::splat(SIZE)).max_element();
    let mut mouths: city::Mouths = Default::default();
    for line in net.lines() {
        // The stretches of it that are near.
        let mut runs: Vec<Vec<usize>> = vec![Vec::new()];
        for &k in &line {
            if out(at[k]) < BEYOND {
                runs.last_mut().unwrap().push(k);
            } else if !runs.last().unwrap().is_empty() {
                runs.push(Vec::new());
            }
        }
        for run in runs.iter().filter(|r| r.len() >= 3) {
            let pts: Vec<Vec2> = run.iter().map(|&k| at[k]).collect();
            // (The narrowest it gets, bar its very end.)
            let halves: Vec<f32> = run[1..].iter().map(|&k| net.nodes[k].half).collect();
            streets(sk, &pts, &halves, &mut mouths);
            let start = halves[0];
            let n = (((start - 4.0) * 2.0 / TRACK) as i32).clamp(1, 5);
            for k in 0..n {
                let off = (k as f32 - (n - 1) as f32 * 0.5) * TRACK;
                // A track ends where its line's too narrow for it.
                let fits = halves.iter().take_while(|&&h| off.abs() < h - 3.0).count();
                if fits < 3 {
                    continue;
                }
                let track = offset(&pts[..fits + 1], off);
                cars(sk, w, &track, off, &halves, rng);
                sk.tracks.push(track);
            }
        }
    }
    for &(a, b, _) in &w.cuts {
        sk.streets.push(Street {
            from: a,
            to: b,
            half: CUT - 1.5,
            clear: 3.0,
            rank: Rank::Lane,
            out: false,
        });
    }
    city::cordon(sk, rng, &mouths);
}

/// `pts` moved `off` to its left.
fn offset(pts: &[Vec2], off: f32) -> Vec<Vec2> {
    let last = pts.len() - 1;
    (0..pts.len())
        .map(|k| {
            let d = pts[(k + 1).min(last)] - pts[k.saturating_sub(1)];
            pts[k] + d.normalize_or(Vec2::X).perp() * off
        })
        .collect()
}

fn streets(sk: &mut Skeleton, pts: &[Vec2], halves: &[f32], mouths: &mut city::Mouths) {
    let size = sk.size;
    let inside = |p: Vec2| p.min_element() >= 0.0 && p.max_element() <= size;
    for (k, s) in pts.windows(2).enumerate() {
        let (a, b) = (s[0], s[1]);
        let half = halves[k];
        if inside(a) && inside(b) {
            sk.streets.push(Street {
                from: a,
                to: b,
                half: half - 1.5,
                clear: 3.0,
                rank: if half > 12.0 {
                    Rank::Avenue
                } else {
                    Rank::Lane
                },
                out: false,
            });
        } else if inside(a) || inside(b) {
            let p = a.lerp(b, 0.5);
            for (side, &(start, along, inward)) in city::sides(size).iter().enumerate() {
                if (p - start).dot(inward).abs() < 8.0 {
                    let t = (p - start).dot(along);
                    mouths[side].push((t - half - 3.0, t + half + 3.0));
                }
            }
        }
    }
}

/// Trains standing on a track, gaps to drive through between.
fn cars(sk: &mut Skeleton, w: &Window, track: &[Vec2], off: f32, halves: &[f32], rng: &mut Rng) {
    let mut gap = rng.int(0, 3);
    for (k, s) in track.windows(2).enumerate() {
        let half = halves[k.min(halves.len() - 1)];
        // (Room to pass it on one side at least.)
        let wide = half + off.abs() > 12.5;
        let mid = s[0].lerp(s[1], 0.5);
        let d = s[1] - s[0];
        if gap > 0 {
            gap -= 1;
            continue;
        }
        let inside = mid.min_element() > 8.0 && mid.max_element() < sk.size - 8.0;
        let clear = mid.distance(w.heart) > 13.0
            && w.pads.iter().all(|p| p.distance(mid) > PADDED + 6.0)
            && w.cuts
                .iter()
                .all(|c| c.0.distance(mid).min(c.1.distance(mid)) > half + 8.0)
            && sk.lots.iter().all(|l| l.at().distance(mid) > 4.4);
        // (Cars straddling a bend would stick out of it.)
        let straight = track.get(k + 2).is_none_or(|n| {
            (*n - s[1])
                .normalize_or(Vec2::X)
                .dot(d.normalize_or(Vec2::X))
                > 0.96
        });
        if wide && inside && clear && straight {
            let b = OBox {
                center: mid,
                yaw: d.x.atan2(d.y),
                half: Vec2::new(1.55, d.length() * 0.5 - 0.2),
                ..OBox::default()
            };
            sk.lots.push(lot(b, Part::Flat));
            if rng.chance(0.35) {
                gap = rng.int(1, 4);
            }
        } else {
            gap = 1;
        }
    }
}

/// Every square of the lattice that isn't open, stacked; squares alike
/// gathered into oblongs.
fn blocks(sk: &mut Skeleton, w: &Window) {
    let n = ((SIZE * 0.5 + BEYOND) * 1.5 / CUBE) as i32;
    // The lattice runs with the trunk.
    let (ax, yaw) = (w.turn, -w.turn.y.atan2(w.turn.x));
    let az = ax.perp();
    let at = |i: f32, j: f32| w.heart + (ax * i + az * j) * CUBE;
    let side = (2 * n) as usize;
    let mut left: Vec<Option<(u8, bool, bool)>> = vec![None; side * side];
    for i in 0..side {
        for j in 0..side {
            let p = at(i as f32 - n as f32 + 0.5, j as f32 - n as f32 + 0.5);
            let out = (-p).max(p - Vec2::splat(SIZE)).max_element();
            if out > BEYOND {
                continue;
            }
            let high = w.courses(p);
            let far = out > 12.0;
            left[i * side + j] = match w.open(p) {
                Some(false) => None,
                Some(true) if high >= 3 => Some((high, true, far)),
                Some(true) => None,
                None if high > 0 => Some((high, false, far)),
                None => None,
            };
        }
    }
    const LONG: usize = 8;
    for i in 0..side {
        for j in 0..side {
            let Some(kind) = left[i * side + j] else {
                continue;
            };
            let is = |i: usize, j: usize| i < side && j < side && left[i * side + j] == Some(kind);
            let dj = (0..LONG).take_while(|&d| is(i, j + d)).count();
            let di = (0..LONG)
                .take_while(|&d| (0..dj).all(|e| is(i + d, j + e)))
                .count();
            for a in i..i + di {
                for b in j..j + dj {
                    left[a * side + b] = None;
                }
            }
            let mid = |k: usize, d: usize| k as f32 - n as f32 + d as f32 * 0.5;
            let (high, roofed, far) = kind;
            let b = OBox {
                center: at(mid(i, di), mid(j, dj)),
                yaw,
                half: Vec2::new(di as f32, dj as f32) * CUBE * 0.5,
                ..OBox::default()
            };
            sk.lots.push(lot(b, Part::Block { high, roofed, far }));
        }
    }
}

/// The loaders at work where the hoard's growing: the nearest to the
/// heart over it.
fn loaders(sk: &mut Skeleton, w: &Window, rng: &mut Rng) {
    let mut put: Vec<Vec2> = Vec::new();
    for t in 0..400 {
        let p = match t {
            0 => w.heart,
            _ => Vec2::new(rng.range(30.0, SIZE - 30.0), rng.range(30.0, SIZE - 30.0)),
        };
        let edge = t == 0 || (0.12..0.4).contains(&w.built(p));
        let half = Part::Loader.half(rng);
        let b = OBox {
            center: p,
            yaw: -w.turn.y.atan2(w.turn.x) + if rng.chance(0.5) { PI * 0.5 } else { 0.0 },
            half,
            ..OBox::default()
        };
        let legs = b.corners().into_iter().all(|c| {
            sk.lots.iter().all(|l| l.footprint.distance(c) > 2.0)
                && sk
                    .tracks
                    .iter()
                    .all(|l| l.windows(2).all(|s| seg(c, s[0], s[1]).0 > 3.0))
        });
        let apart = put.iter().all(|q| q.distance(p) > 62.0)
            && w.pads.iter().all(|q| q.distance(p) > PADDED + 20.0);
        if edge && legs && apart && put.len() < 4 {
            put.push(p);
            sk.lots.push(lot(b, Part::Loader));
        }
    }
}
