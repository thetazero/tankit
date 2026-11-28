//! The railways of a tract of hoard far bigger than any map of it: a trunk,
//! and branches off it that fork till there's a line within reach of
//! everywhere. Grown a step at a time toward whatever's furthest from any
//! line; then each line's as wide as what it carries wants.

use std::f32::consts::TAU;

use glam::Vec2;

use crate::rng::Rng;

/// Half the tract's width. Its middle's the origin; the trunk comes in
/// along x from the west.
pub const TRACT: f32 = 400.0;
/// One step of a line.
const STEP: f32 = 10.0;
/// How far apart the places lines grow toward are, and how near one has to
/// get.
const APART: f32 = 50.0;
const SERVED: f32 = 36.0;
/// Steps between the places branches leave the trunk.
const SPROUTS: usize = 8;
/// Half the width of a last twig's cutting.
const TWIG: f32 = 7.0;

pub struct Network {
    pub nodes: Vec<Node>,
}

pub struct Node {
    pub at: Vec2,
    pub from: Option<usize>,
    pub next: Vec<usize>,
    /// How many line ends it serves.
    pub ends: u32,
    /// Half its cutting's width.
    pub half: f32,
}

impl Network {
    pub fn grow(rng: &mut Rng) -> Network {
        let mut nodes: Vec<Node> = Vec::new();
        let add = |nodes: &mut Vec<Node>, at: Vec2, from: Option<usize>| {
            let k = nodes.len();
            if let Some(f) = from {
                nodes[f].next.push(k);
            }
            nodes.push(Node {
                at,
                from,
                next: Vec::new(),
                ends: 0,
                half: 0.0,
            });
        };
        // The trunk, all but straight.
        let (bend, wave) = (rng.range(8.0, 22.0), rng.range(0.0, TAU));
        let mut x = -TRACT;
        while x < TRACT * 0.55 {
            let at = Vec2::new(x, bend * (x / 170.0 + wave).sin());
            let last = nodes.len().checked_sub(1);
            add(&mut nodes, at, last);
            x += STEP;
        }
        let trunk = nodes.len();
        // Branches leave it on alternate sides, at a slant with the flow.
        let mut k = 2 + rng.index(SPROUTS);
        let mut side = if rng.chance(0.5) { 1.0 } else { -1.0 };
        while k + 4 < trunk {
            let mut from = k;
            let slant = rng.range(0.6, 0.95);
            for step in 0..rng.int(5, 9) {
                let d = Vec2::from_angle(side * (slant + 0.02 * step as f32));
                let at = nodes[from].at + d * STEP;
                add(&mut nodes, at, Some(from));
                from = nodes.len() - 1;
            }
            side = -side;
            k += rng.int(SPROUTS as i32 - 2, SPROUTS as i32 + 3) as usize;
        }
        // Where lines are wanted.
        let mut wanted: Vec<Vec2> = Vec::new();
        for _ in 0..3000 {
            let p = Vec2::new(rng.range(-1.0, 1.0), rng.range(-1.0, 1.0)) * TRACT;
            if wanted.iter().all(|q| q.distance(p) > APART) {
                wanted.push(p);
            }
        }
        let near = |p: Vec2, n: &Node| p.distance_squared(n.at);
        for _ in 0..140 {
            wanted.retain(|&p| nodes.iter().all(|n| near(p, n) > SERVED * SERVED));
            let mut pull = vec![Vec2::ZERO; nodes.len()];
            for &p in &wanted {
                // (The trunk itself only goes on at its end.)
                let k = (0..nodes.len())
                    .filter(|&k| k + 1 >= trunk)
                    .min_by(|&a, &b| near(p, &nodes[a]).total_cmp(&near(p, &nodes[b])))
                    .unwrap_or(0);
                pull[k] += (p - nodes[k].at).normalize_or_zero();
            }
            let mut grew = false;
            for (k, d) in pull.into_iter().enumerate() {
                // (Pulled both ways at once goes nowhere.)
                let Some(d) = d.try_normalize() else { continue };
                // A line turns only so sharply.
                let on = match nodes[k].from {
                    Some(f) => (nodes[k].at - nodes[f].at).normalize_or_zero(),
                    None => Vec2::ZERO,
                };
                let keep = 0.9;
                let d = (d + on * keep).normalize_or(d);
                let at = nodes[k].at + d * STEP;
                let crowded = nodes.iter().any(|n| n.at.distance(at) < STEP * 0.75);
                if !crowded && nodes[k].next.len() < 3 {
                    add(&mut nodes, at, Some(k));
                    grew = true;
                }
            }
            if !grew {
                break;
            }
        }
        for k in (0..nodes.len()).rev() {
            let ends = nodes[k].next.iter().map(|&n| nodes[n].ends).sum();
            nodes[k].ends = u32::max(ends, 1);
            nodes[k].half = TWIG * (nodes[k].ends as f32).powf(0.22);
        }
        Network { nodes }
    }

    /// How far it is from node `a` to `b` along the lines, in steps.
    pub fn along(&self, a: usize, b: usize) -> usize {
        let up =
            |k: usize| std::iter::successors(Some(k), |&k| self.nodes[k].from).collect::<Vec<_>>();
        let (a, b) = (up(a), up(b));
        let shared = a.iter().rev().zip(b.iter().rev());
        let shared = shared.take_while(|(x, y)| x == y).count();
        a.len() + b.len() - 2 * shared
    }

    /// The lines, each from where it leaves its parent to its end: the
    /// busiest way on at every fork carries straight on.
    pub fn lines(&self) -> Vec<Vec<usize>> {
        let mut out = Vec::new();
        let mut starts = vec![(None, 0)];
        while let Some((leaves, first)) = starts.pop() {
            let mut line: Vec<usize> = Option::into_iter(leaves).collect();
            let mut k = first;
            loop {
                line.push(k);
                let busiest = |&&n: &&usize| self.nodes[n].ends;
                let Some(&on) = self.nodes[k].next.iter().max_by_key(busiest) else {
                    break;
                };
                for &n in self.nodes[k].next.iter().filter(|&&n| n != on) {
                    starts.push((Some(k), n));
                }
                k = on;
            }
            out.push(line);
        }
        out
    }
}
