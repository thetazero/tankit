//! The Seam: an Ithessar crystal mine dug into a desert escarpment, the
//! Attrition map where the Ithessar hold their mine against the Resonant.
//! The Resonant call the seam holy, and have broken into the rock from the
//! east to take it back.
//!
//! 220 m square (+x east, +z north). The Ithessar come in from the west
//! edge, the Resonant from the east. Nothing is mirrored: each side has its
//! own way up, its own way in and its own cover, matched for distance.
//!
//! - **The Face**, along the north: an escarpment 10 m high. Where the
//!   Ithessar have quarried it, in the middle, it's sheer cut stone with
//!   the seam glowing up it; elsewhere it's broken natural rock. Its top is
//!   the mesa, open ground with the headframe on it. The Ithessar drive up
//!   its long west end; the Resonant climb the scree where the face fell in,
//!   in the east.
//! - **The mine**, under the mesa: the rail adit straight in from the face,
//!   the pillared hall where the seam is worked west of it, the west adit
//!   out to the face, the east drift to the Resonant's breach, and the box
//!   cut, an open ramp climbing from the adit's end to the mesa top. Tanks
//!   on the mesa drive over tanks in the mine.
//! - **The wash**, a dry ravine 7 m deep winding right across the map
//!   under the face, in at the west edge by the Ithessar's yard and out at
//!   the east edge below the Resonant's camp: the seam crops out on its
//!   floor, where a cutting rig works it. The rail bridge leads out of the
//!   adit's mouth across the wash, on crystal piers, so the fight on the
//!   wash floor goes on under the deck. Haul ramps lead down into it, and a
//!   crossing near each side's spawns runs down one bank and up the other.
//! - **The flats**, south of the wash: the railroad runs out across them to
//!   the south edge, the ore train stopped on it under the loading tipple.
//!   The Ithessar's waste dump stands in the west, the sorting house in the
//!   middle, sandstone fins and hoodoos in the east.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use glam::{Quat, Vec2, Vec3};

use super::landmarks::Rig;
use super::{Ambient, Core, Crystal, GlowPath, Map, OBox, Terrain};
use crate::faction::Faction;
use crate::render::material::Material;
use crate::rng::Rng;
use crate::tank::ithessar::PLASMA;

const SIZE: f32 = 220.0;
const MID: f32 = SIZE * 0.5;
const CENTER: Vec2 = Vec2::new(MID, MID);
/// Clearance a spawn needs from anything solid.
const TANK_CLEAR: f32 = 4.6;

/// The mesa's top over the desert floor.
const TOP: f32 = 10.0;
/// The Face's foot, west to east (x, z), eased between: headlands pushing
/// out, bays cut back.
const FACE: [(f32, f32); 14] = [
    (-10.0, 158.0),
    (20.0, 155.0),
    (44.0, 156.0),
    (62.0, 151.0),
    (76.0, 145.0),
    (90.0, 147.0),
    (104.0, 152.0),
    (118.0, 152.0),
    (132.0, 151.0),
    (146.0, 157.0),
    (160.0, 160.0),
    (176.0, 155.0),
    (196.0, 150.0),
    (230.0, 152.0),
];
/// How far the Face's foot is from its lip (horizontally).
const FACE_RISE: f32 = 4.0;
/// The quarried stretch of the Face: cut stone, sheer (x).
const QUARRY: (f32, f32) = (78.0, 146.0);
/// The scree, where the Face fell in (x): a long slope instead of a cliff,
/// from `SCREE_RUN` m out from the foot up to the lip.
const SCREE: (f32, f32) = (166.0, 196.0);
const SCREE_RUN: f32 = 30.0;
/// The mesa's west end slopes down to the floor between these (x).
const WEST_RAMP: (f32, f32) = (8.0, 48.0);

/// The mine, under the mesa: (min, max) on the ground plane, every edge on
/// the terrain's 2 m grid so the cut is clean.
const ADIT: (Vec2, Vec2) = (Vec2::new(100.0, 140.0), Vec2::new(112.0, 212.0));
const HALL: (Vec2, Vec2) = (Vec2::new(62.0, 170.0), Vec2::new(94.0, 198.0));
const CAVES: [(Vec2, Vec2); 6] = [
    ADIT,
    HALL,
    // The crosscut from the hall to the adit.
    (Vec2::new(94.0, 180.0), Vec2::new(100.0, 190.0)),
    // The west adit, from the hall out to the face.
    (Vec2::new(66.0, 140.0), Vec2::new(76.0, 170.0)),
    // The east drift, along to the breach.
    (Vec2::new(112.0, 186.0), Vec2::new(148.0, 196.0)),
    // The Resonant's breach, in from the face.
    (Vec2::new(148.0, 150.0), Vec2::new(158.0, 196.0)),
];
/// The box cut: an open trench climbing east from the adit's end to the
/// mesa's top.
const BOX_CUT: (Vec2, Vec2) = (Vec2::new(112.0, 200.0), Vec2::new(150.0, 210.0));
/// The hall's rock pillars, left standing to hold the roof up.
const PILLARS: [Vec2; 4] = [
    Vec2::new(71.0, 180.0),
    Vec2::new(84.0, 179.0),
    Vec2::new(72.0, 190.0),
    Vec2::new(84.0, 190.0),
];
/// Cave roofs: underside and top (flush with the mesa).
const ROOF: (f32, f32) = (7.4, TOP + 0.05);
/// Cave walls stand this thick round each corridor.
const CAVE_WALL: f32 = 2.4;

/// The wash: its line, west to east, edge to edge (its ends run on out
/// past the map), and its depth.
const WASH: [Vec2; 14] = [
    Vec2::new(-14.0, 108.0),
    Vec2::new(8.0, 112.0),
    Vec2::new(30.0, 116.0),
    Vec2::new(56.0, 121.0),
    Vec2::new(84.0, 127.0),
    Vec2::new(110.0, 133.0),
    Vec2::new(132.0, 132.0),
    Vec2::new(152.0, 122.0),
    Vec2::new(166.0, 104.0),
    Vec2::new(176.0, 84.0),
    Vec2::new(184.0, 64.0),
    Vec2::new(198.0, 54.0),
    Vec2::new(216.0, 50.0),
    Vec2::new(236.0, 48.0),
];
const WASH_DEPTH: f32 = 7.0;
/// Its banks, from the floor's edge to the top.
const WASH_BANK: f32 = 5.0;
/// Haul ramps down into the wash: from the top (start) to the floor (end),
/// and their half-width. One cut into the south bank west of the bridge,
/// one into the north bank east of it, one into the east arm's bank below
/// the Resonant's camp.
const RAMPS: [(Vec2, Vec2, f32); 3] = [
    (Vec2::new(90.0, 110.0), Vec2::new(86.0, 127.0), 4.0),
    (Vec2::new(153.0, 146.0), Vec2::new(143.0, 130.0), 4.0),
    (Vec2::new(190.0, 97.0), Vec2::new(172.0, 92.0), 4.0),
];
/// Crossings: a ramp cut down one bank and up the other, top to top, and
/// their half-width. One in the west by the Ithessar's yard, one in the
/// south-east below the Resonant's camp.
const FORDS: [(Vec2, Vec2, f32); 2] = [
    (Vec2::new(38.5, 93.2), Vec2::new(29.5, 140.3), 6.0),
    (Vec2::new(203.0, 80.0), Vec2::new(175.1, 40.9), 6.0),
];
/// How far a crossing's ramp runs, from the top down to the floor.
const FORD_RUN: f32 = 16.0;

/// The railroad, out of the mine to the south edge: the adit, then the
/// bridge (from the mouth to the far bank), then out over the flats.
const TRACK: [Vec2; 7] = [
    Vec2::new(106.0, 216.0),
    Vec2::new(106.0, 153.0),
    Vec2::new(120.0, 116.0),
    Vec2::new(124.0, 104.0),
    Vec2::new(131.0, 74.0),
    Vec2::new(140.0, 34.0),
    Vec2::new(146.0, -6.0),
];
/// The bridge is the track's second leg.
const BRIDGE: (Vec2, Vec2) = (TRACK[1], TRACK[2]);
const DECK_HALF: f32 = 3.6;
const DECK_TOP: f32 = 0.25;
const DECK_T: f32 = 1.0;
/// Track gauge (rail to rail, m): broad, for big ore cars.
const GAUGE: f32 = 2.2;

/// Places bots head for: across the flats, the wash, the landing under the
/// Face, the mine and the mesa.
const SPOTS: [(f32, f32); 28] = [
    (40.0, 70.0),
    (70.0, 30.0),
    (56.0, 52.0),
    (100.0, 44.0),
    (120.0, 70.0),
    (150.0, 60.0),
    (176.0, 36.0),
    (180.0, 74.0),
    (60.0, 92.0),
    (150.0, 92.0),
    (100.0, 110.0),
    (60.0, 124.0),
    (88.0, 131.0),
    (130.0, 128.0),
    (164.0, 108.0),
    (44.0, 144.0),
    (90.0, 141.0),
    (160.0, 140.0),
    (180.0, 140.0),
    (106.0, 170.0),
    (78.0, 184.0),
    (130.0, 191.0),
    (153.0, 172.0),
    (40.0, 176.0),
    (70.0, 210.0),
    (190.0, 160.0),
    (170.0, 200.0),
    (200.0, 180.0),
];
/// Repair drops: under the Face either side, and on the flats.
const REPAIRS: [Vec2; 4] = [
    Vec2::new(47.0, 137.0),
    Vec2::new(163.0, 137.0),
    Vec2::new(67.0, 83.0),
    Vec2::new(158.0, 72.0),
];
/// The Ithessar's yard, by their spawns, under its plasma core.
const ITHESSAR_YARD: Vec2 = Vec2::new(37.0, 72.0);
/// The ways up onto the mesa, kept clear: the haul road up its west end,
/// and the worn track up the scree.
const WAYS_UP: [(Vec2, Vec2); 2] = [
    (Vec2::new(8.0, 167.0), Vec2::new(60.0, 167.0)),
    (Vec2::new(181.0, 118.0), Vec2::new(181.0, 166.0)),
];

/// The waste dump in the west of the flats: its middle, rough radius, the
/// height of its flat top, and how far its sides run out.
const DUMP: (Vec2, f32, f32, f32) = (Vec2::new(56.0, 52.0), 15.0, 4.5, 11.0);
/// A second, smaller heap by it.
const HEAP: (Vec2, f32, f32, f32) = (Vec2::new(86.0, 26.0), 6.0, 3.0, 8.0);
/// Sandstone fins in the east of the flats: (from, to, height). Steep
/// sided, not to be climbed.
const FINS: [(Vec2, Vec2, f32); 4] = [
    (Vec2::new(152.0, 38.0), Vec2::new(170.0, 52.0), 3.4),
    (Vec2::new(180.0, 24.0), Vec2::new(198.0, 36.0), 3.8),
    (Vec2::new(146.0, 78.0), Vec2::new(151.0, 92.0), 2.8),
    (Vec2::new(84.0, 92.0), Vec2::new(98.0, 97.0), 2.4),
];

/// Sandstone, in a few shades; the desert floor.
const STONE: Vec3 = Vec3::new(0.58, 0.34, 0.22);
const STONE_DARK: Vec3 = Vec3::new(0.4, 0.23, 0.15);
const STONE_PALE: Vec3 = Vec3::new(0.7, 0.5, 0.34);
const SAND: Vec3 = Vec3::new(0.66, 0.46, 0.3);
/// Mine waste: pale crushed rock.
const TAILINGS: Vec3 = Vec3::new(0.47, 0.41, 0.35);
/// The mesa's top: weathered rock, and the cave roofs laid flush with it.
const MESA_TOP: Vec3 = Vec3::new(0.46, 0.26, 0.17);
/// Ithessar worked crystal (their piers, cars and rigs), and raw seam crystal.
const GLASS: Vec3 = Vec3::new(0.03, 0.07, 0.16);
const RAW: Vec3 = Vec3::new(0.05, 0.1, 0.2);
const STEEL: Vec3 = Vec3::new(0.2, 0.21, 0.24);
const RUST: Vec3 = Vec3::new(0.36, 0.22, 0.14);
const TIMBER: Vec3 = Vec3::new(0.42, 0.32, 0.22);
/// The Resonant's crystal light.
const HOLY: Vec3 = Vec3::new(0.25, 0.55, 1.0);

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Nearest point on segment a-b to p: (distance, 0..1 along).
fn seg(p: Vec2, a: Vec2, b: Vec2) -> (f32, f32) {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
    (p.distance(a + ab * t), t)
}

/// Inside a rect, grown by `grow`.
fn within((a, b): (Vec2, Vec2), p: Vec2, grow: f32) -> bool {
    p.x > a.x - grow && p.x < b.x + grow && p.y > a.y - grow && p.y < b.y + grow
}

fn rect_distance((a, b): (Vec2, Vec2), p: Vec2) -> f32 {
    let q = (p - (a + b) * 0.5).abs() - (b - a) * 0.5;
    q.max(Vec2::ZERO).length()
}

/// The Face's foot at `x` (z).
fn face(x: f32) -> f32 {
    let wobble = 0.8 * (0.5 * x).sin() * (0.13 * x).sin();
    for w in FACE.windows(2) {
        let ((x0, z0), (x1, z1)) = (w[0], w[1]);
        if x <= x1 {
            let t = smoothstep(x0, x1, x);
            return z0 + (z1 - z0) * t + wobble;
        }
    }
    FACE[FACE.len() - 1].1
}

/// How much of the scree (0..1) there is at `x`, in place of the cliff.
fn scree(x: f32) -> f32 {
    smoothstep(SCREE.0 - 4.0, SCREE.0 + 6.0, x)
        * (1.0 - smoothstep(SCREE.1 - 6.0, SCREE.1 + 4.0, x))
}

/// How high the mesa stands at a point, before the mine is cut out: 0..1
/// of `TOP`.
fn mesa(p: Vec2) -> f32 {
    let f = face(p.x);
    let cliff = smoothstep(f, f + FACE_RISE, p.y);
    let slope = ((p.y - (f - SCREE_RUN)) / (SCREE_RUN + FACE_RISE)).clamp(0.0, 1.0);
    let s = scree(p.x);
    let across = cliff + (slope - cliff) * s;
    let west = ((p.x - WEST_RAMP.0) / (WEST_RAMP.1 - WEST_RAMP.0)).clamp(0.0, 1.0);
    across * west
}

/// Distance from a point to the nearest cave (0 inside one).
fn cave_distance(p: Vec2) -> f32 {
    CAVES
        .iter()
        .map(|&r| rect_distance(r, p))
        .fold(f32::INFINITY, f32::min)
}

fn in_cave(p: Vec2) -> bool {
    CAVES.iter().any(|&r| within(r, p, 0.0))
}

/// The box cut's floor at `x`: from the adit's floor up to the mesa's top.
fn box_cut_floor(x: f32) -> f32 {
    TOP * ((x - BOX_CUT.0.x - 2.0) / (BOX_CUT.1.x - BOX_CUT.0.x - 4.0)).clamp(0.0, 1.0)
}

/// Rock: under the mesa, back from its foot (not out in front of the Face).
fn in_rock(p: Vec2) -> bool {
    mesa(p) > 0.5 && !in_cave(p) && !within(BOX_CUT, p, 0.0)
}

/// Where the wash is nearest `p`: (distance from its line, metres along it
/// from its west end).
fn wash_near(p: Vec2) -> (f32, f32) {
    let (mut best, mut at, mut acc) = (f32::INFINITY, 0.0, 0.0);
    for w in WASH.windows(2) {
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

/// The wash's depth and floor half-width, `s` m along it: the floor
/// narrows and widens as it winds (in step from where it used to begin,
/// at its third point, so the middle keeps the banks it had).
fn wash_profile(s: f32) -> (f32, f32) {
    let from = WASH[0].distance(WASH[1]) + WASH[1].distance(WASH[2]);
    (WASH_DEPTH, 6.5 + 1.5 * (0.08 * (s - from)).sin())
}

fn wash(p: Vec2) -> f32 {
    let (d, s) = wash_near(p);
    let (depth, half) = wash_profile(s);
    -depth * (1.0 - smoothstep(half, half + WASH_BANK, d))
}

/// The ground with the haul ramps and the crossings cut into the wash.
fn with_ramps(p: Vec2, g: f32) -> f32 {
    let mut g = g;
    for (s, e, half) in RAMPS {
        let len = s.distance(e);
        let u = (e - s) / len;
        let rel = p - s;
        let (a, c) = (rel.dot(u), rel.perp_dot(u).abs());
        if a < -2.0 || a > len + 6.0 || c > half + 3.0 {
            continue;
        }
        let floor = wash(e);
        let h = floor * (a / len).clamp(0.0, 1.0);
        let k =
            (1.0 - smoothstep(half, half + 2.5, c)) * (1.0 - smoothstep(len + 2.0, len + 6.0, a));
        g += (h - g) * k;
    }
    for (s, e, half) in FORDS {
        let len = s.distance(e);
        let u = (e - s) / len;
        let rel = p - s;
        let (a, c) = (rel.dot(u), rel.perp_dot(u).abs());
        if a < -3.0 || a > len + 3.0 || c > half + 6.0 {
            continue;
        }
        // Down from either top to the floor, flat across the floor; its
        // sides laid back, not cut sheer.
        let floor = wash(s.lerp(e, 0.5));
        let h = floor * (a.min(len - a) / FORD_RUN).clamp(0.0, 1.0);
        let k = (1.0 - smoothstep(half, half + 6.0, c))
            * smoothstep(-3.0, 0.0, a)
            * smoothstep(-3.0, 0.0, len - a);
        g += (h - g) * k;
    }
    g
}

/// A heap (the dump): flat-topped, its outline lobed.
fn heap((c, r, h, run): (Vec2, f32, f32, f32), p: Vec2) -> f32 {
    let d = p - c;
    let a = d.y.atan2(d.x);
    let edge = r + 0.25 * r * (3.0 * a + c.x).sin() + 0.12 * r * (5.0 * a + 1.0).sin();
    h * (1.0 - smoothstep(edge, edge + run, d.length()))
}

fn fins(p: Vec2) -> f32 {
    FINS.iter()
        .map(|&(a, b, h)| {
            let (d, t) = seg(p, a, b);
            // Ragged along the top, tapering at the ends.
            let h = h
                * (0.75 + 0.25 * (t * 19.0 + a.x).sin())
                * smoothstep(0.0, 0.15, t.min(1.0 - t) + 0.05);
            h * (1.0 - smoothstep(1.2, 3.6, d))
        })
        .fold(0.0, f32::max)
}

/// How near the track (distance from its line).
fn track_distance(p: Vec2) -> f32 {
    TRACK
        .windows(2)
        .map(|w| seg(p, w[0], w[1]).0)
        .fold(f32::INFINITY, f32::min)
}

/// Play ground: dunes on the flats, the mesa along the north with the mine
/// cut out under it, the wash across under it, the dump and the fins.
pub fn ground(x: f32, z: f32) -> f32 {
    let p = Vec2::new(x, z);
    let m = mesa(p);
    let dunes = 0.45 * (0.061 * x + 0.3).sin() * (0.047 * z).cos()
        + 0.25 * (0.13 * x - 0.09 * z).sin()
        + 0.15 * (0.21 * z + 0.4 * (0.05 * x).sin()).sin();
    // Flat along the railroad and on the mesa.
    let flat = smoothstep(3.0, 9.0, track_distance(p)) * (1.0 - m);
    // The mesa's top rolls a little, away from the mine's roofs and back
    // from the lip.
    let f = face(x);
    let relief = (1.8 * (0.09 * x + 0.7).sin() * (0.07 * z).cos()
        + 0.6 * (0.21 * x - 0.17 * z).sin())
    .max(-0.6)
        * smoothstep(5.0, 14.0, cave_distance(p))
        * smoothstep(f + FACE_RISE + 1.0, f + FACE_RISE + 9.0, z)
        * smoothstep(4.0, 10.0, rect_distance(BOX_CUT, p))
        * m.powi(4);
    let mut rock = (TOP * m + relief) * smoothstep(0.0, 2.0, cave_distance(p));
    // The box cut, down through the rock.
    let cut = rect_distance(BOX_CUT, p);
    rock = rock
        .min(box_cut_floor(x.clamp(BOX_CUT.0.x, BOX_CUT.1.x)) + (rock * smoothstep(0.0, 2.0, cut)));
    let g = dunes * flat + rock + wash(p) + heap(DUMP, p) + heap(HEAP, p) + fins(p);
    with_ramps(p, g)
}

/// Desert out past the edge: the mesa runs on north, the floor rises into
/// broken hills.
fn scenery_ground(x: f32, z: f32) -> f32 {
    let p = Vec2::new(x, z);
    let q = (p - CENTER).abs() - Vec2::splat(MID);
    let d = q.max(Vec2::ZERO).length() + q.x.max(q.y).min(0.0);
    if d < -0.01 {
        return -10.0;
    }
    // The edge's ground carried on out, the mesa solid (its adit runs on
    // under the rock).
    let edge = p.clamp(Vec2::ZERO, Vec2::splat(SIZE));
    let base = ground(edge.x, edge.y).max(TOP * mesa(Vec2::new(edge.x, z.max(edge.y))));
    let hills = smoothstep(6.0, 60.0, d)
        * (6.0 * ((x * 0.017).sin() * (z * 0.021).cos()).abs()
            + 4.0 * ((x * 0.007 - z * 0.005).sin() + 1.0)
            + 9.0
                * smoothstep(
                    0.55,
                    0.9,
                    ((x * 0.004 + 1.0).sin() * (z * 0.006).cos()).abs(),
                ));
    base + hills
}

/// The seam's line along the wash floor, west to east of the bridge.
fn seam_line() -> Vec<Vec2> {
    (0..=24)
        .map(|k| {
            let x = 70.0 + 70.0 * k as f32 / 24.0;
            // Follow the wash, a little off its middle.
            let z = WASH
                .windows(2)
                .find(|w| x <= w[1].x)
                .map(|w| w[0].lerp(w[1], (x - w[0].x) / (w[1].x - w[0].x)).y)
                .unwrap_or(120.0);
            Vec2::new(x, z + 1.6 * (x * 0.21).sin() - 1.0)
        })
        .collect()
}

/// The ground's colour: sand on the flats, red rock on the Face and the
/// mesa, the wash floor dark with crystal dust and darker yet along the
/// seam, the dump pale, ballast along the track, the mine's floors dim.
fn paint(x: f32, z: f32, h: f32, slope: f32) -> Vec3 {
    let p = Vec2::new(x, z);
    let mottle =
        1.0 + 0.06 * (0.37 * x + 0.8 * (0.11 * z).sin()).sin() + 0.04 * (0.9 * z - 0.3 * x).sin();
    let mut c = SAND * mottle;
    // Rock where the ground is high or steep.
    let rocky = smoothstep(0.55, 1.2, slope).max(smoothstep(TOP * 0.35, TOP * 0.8, h) * 0.75);
    let rock = STONE.lerp(
        STONE_PALE,
        0.5 + 0.5 * (0.08 * x + 0.3 * (0.2 * z).sin()).sin(),
    ) * mottle;
    c = c.lerp(rock, rocky);
    // The mesa's top.
    c = c.lerp(
        MESA_TOP * mottle,
        smoothstep(TOP * 0.45, TOP * 0.8, h) * (1.0 - smoothstep(0.4, 0.9, slope)),
    );
    // The wash's floor: grey-blue with crystal dust.
    let dust = Vec3::new(0.25, 0.25, 0.3) * mottle;
    c = c.lerp(dust, smoothstep(-4.5, -6.6, h));
    // Darker along the seam.
    let seam = seam_line()
        .windows(2)
        .map(|w| seg(p, w[0], w[1]).0)
        .fold(f32::INFINITY, f32::min);
    c = c.lerp(
        Vec3::new(0.1, 0.12, 0.18),
        (1.0 - smoothstep(1.0, 6.0, seam)) * smoothstep(-3.0, -6.0, h),
    );
    // The dump and the heap: pale crushed rock.
    let waste = (heap(DUMP, p) / DUMP.2).max(heap(HEAP, p) / HEAP.2);
    c = c.lerp(TAILINGS * mottle, smoothstep(0.05, 0.4, waste));
    // The ways up, worn into the rock and sand.
    let way = WAYS_UP
        .iter()
        .map(|&(a, b)| seg(p, a, b).0)
        .fold(f32::INFINITY, f32::min);
    c = c.lerp(
        Vec3::new(0.5, 0.38, 0.28) * mottle,
        0.6 * (1.0 - smoothstep(2.5, 4.0, way)),
    );
    // Ballast along the track.
    c = c.lerp(
        Vec3::new(0.36, 0.32, 0.3),
        1.0 - smoothstep(2.4, 3.6, track_distance(p)),
    );
    // The mine's floors.
    if mesa(p) > 0.5 && cave_distance(p) < 0.5 {
        c = STONE_DARK * 0.8 * mottle;
    }
    c
}

/// Position and heading `s` m along the track.
fn track_at(s: f32) -> (Vec2, Vec2) {
    let mut left = s;
    for w in TRACK.windows(2) {
        let len = w[0].distance(w[1]);
        let dir = (w[1] - w[0]) / len;
        if left <= len {
            return (w[0] + dir * left, dir);
        }
        left -= len;
    }
    let n = TRACK.len();
    (TRACK[n - 1], (TRACK[n - 1] - TRACK[n - 2]).normalize())
}

/// Yaw that turns a box's local +z along `dir`.
fn yaw_along(dir: Vec2) -> f32 {
    dir.x.atan2(dir.y)
}

impl Map {
    pub fn mine() -> Map {
        let sides = [Faction::Ithessar, Faction::Resonant];
        let mut map = Map {
            size: SIZE,
            solids: Vec::new(),
            boxes: Vec::new(),
            wedges: Vec::new(),
            oboxes: Vec::new(),
            terrain: None,
            scenery_terrain: None,
            sculpted: crate::render::mesh::MeshData::default(),
            sounds: Vec::new(),
            open_spots: Vec::new(),
            spawn_yaw: None,
            // Hot late afternoon: a low gold sun, a hazy sky going white at
            // the horizon, sand blowing off the mesa.
            atmosphere: crate::render::Atmosphere {
                sun_dir: Vec3::new(0.55, 0.5, -0.32),
                sun_color: Vec3::new(2.2, 1.85, 1.45),
                sky_zenith: Vec3::new(0.2, 0.36, 0.62),
                sky_horizon: Vec3::new(0.7, 0.6, 0.5),
                ground_color: Vec3::new(0.42, 0.3, 0.2),
                fog_density: 0.0013,
                wind: Vec2::new(3.0, 1.2),
                blowing_sand: 0.3,
                dust: Vec3::new(0.72, 0.55, 0.38),
                settle: crate::render::SAND_SETTLE,
                ambient: 1.0,
            },
            bases: Map::no_bases(&sides),
            factions: sides.to_vec(),
            repair_spots: Vec::new(),
            hold_site: Some(Vec2::new(88.0, 131.0)),
            cores: Vec::new(),
            crystals: Vec::new(),
            glow_paths: Vec::new(),
            fall_y: None,
            hold_name: "SEAM",
            routes: Vec::new(),
        };
        let mut terrain = Terrain::from_fn(Vec2::ZERO, 2.0, 111, 111, ground);
        terrain.color = Vec3::ONE;
        terrain.material = Material::Sand;
        terrain.paint(paint);
        map.terrain = Some(terrain);
        let mut rng = Rng::new(0x5ea3_0213);

        map.seam(&mut rng);
        map.quarry_face(&mut rng);
        map.rough_face(&mut rng);
        map.scree_rocks(&mut rng);
        map.mine_caves(&mut rng);
        map.pillared_hall(&mut rng);
        map.breach(&mut rng);
        map.box_cut(&mut rng);
        map.railroad();
        map.rail_bridge();
        map.wash_rocks(&mut rng);
        map.cutting_rig(Vec2::new(97.0, 131.5), PI * 0.5 + 0.35, -WASH_DEPTH);
        map.cutting_rig(Vec2::new(90.0, 174.5), -PI * 0.5 - 0.3, 0.0);
        map.tipple();
        map.ore_train(&mut rng);
        map.headframe(Vec2::new(82.0, 206.0));
        map.mesa_cover(&mut rng);
        map.dump_cover(&mut rng);
        map.sorting_house(&mut rng);
        map.fins_and_hoodoos(&mut rng);
        map.flats_cover(&mut rng);
        map.fill_cover(&mut rng);
        map.mine_ithessar_base(&mut rng);
        map.mine_resonant_base(&mut rng);
        map.edge_rocks(&mut rng);
        map.mine_spawns();
        map.mine_routes();
        map.perimeter(false);
        map.mine_horizon(&mut rng);
        map.mine_sounds();
        map
    }

    /// What sounds here: the seam's lit crystals singing, the cutting rigs
    /// still running, the stopped train ticking in the heat, the Resonant's
    /// crystals at their breach and camp.
    fn mine_sounds(&mut self) {
        let mut found: Vec<(Ambient, Vec3)> = self
            .crystals
            .iter()
            .filter(|c| c.lit && c.radius > 0.6)
            .map(|c| (Ambient::Crystal, c.base + c.axis * 0.35))
            .collect();
        found.push((Ambient::Machine, Vec3::new(97.0, -4.0, 131.5)));
        found.push((Ambient::Machine, Vec3::new(90.0, 3.0, 174.5)));
        let (at, _) = track_at(track_len(3) + 18.0);
        let g = self.ground_height(at.x, at.y);
        found.push((Ambient::Hull, Vec3::new(at.x, g + 2.5, at.y)));
        found.push((Ambient::River, Vec3::new(110.0, -6.5, 133.0)));
        found.push((Ambient::Shrine, Vec3::new(153.0, 2.0, 154.0)));
        found.push((Ambient::Shrine, Vec3::new(192.0, 2.0, 108.0)));
        found.push((
            Ambient::Core,
            Vec3::new(ITHESSAR_YARD.x, 10.0, ITHESSAR_YARD.y),
        ));
        for (kind, at) in found {
            self.sound(kind, at);
        }
    }

    /// A box of rock, solid, with chipped edges.
    #[allow(clippy::too_many_arguments)]
    fn stone(
        &mut self,
        center: Vec2,
        yaw: f32,
        half: Vec2,
        y0: f32,
        y1: f32,
        color: Vec3,
        chip: f32,
    ) {
        self.oboxes.push(OBox {
            center,
            yaw,
            half,
            y0,
            y1,
            color,
            side_mat: Material::Rubble,
            top_mat: Material::Rubble,
            chip,
            ..OBox::default()
        });
    }

    /// A boulder: a lump of rock (drawn and collided as one).
    fn boulder(&mut self, rng: &mut Rng, at: Vec2, size: f32, solid: bool) {
        let g = self.level(at);
        let shades = [STONE, STONE_DARK, STONE_PALE];
        self.oboxes.push(OBox {
            center: at,
            yaw: rng.range(0.0, TAU),
            half: Vec2::new(size * rng.range(0.8, 1.1), size * rng.range(0.6, 0.9)),
            y0: g - size * 0.4,
            y1: g + size * rng.range(0.9, 1.4),
            color: shades[rng.index(shades.len())] * rng.range(0.9, 1.08),
            side_mat: Material::Rubble,
            top_mat: Material::Rubble,
            chip: size.max(0.6),
            solid,
            ..OBox::default()
        });
    }

    /// A tank-sized rock with rubble round it: cover.
    fn rock_pile(&mut self, rng: &mut Rng, at: Vec2, size: f32) {
        self.boulder(rng, at, size, true);
        for _ in 0..3 {
            let a = rng.range(0.0, TAU);
            let q = at + Vec2::new(a.cos(), a.sin()) * rng.range(size + 0.8, size + 3.0);
            let s = rng.range(0.3, 0.6);
            self.boulder(rng, q, s, false);
        }
    }

    /// One crystal standing in the ground at `at`: `h` tall, `r` thick,
    /// leaning `lean` toward `dir`.
    #[allow(clippy::too_many_arguments)]
    fn shard(
        &mut self,
        seed: u64,
        at: Vec3,
        h: f32,
        r: f32,
        dir: Vec2,
        lean: f32,
        lit: bool,
        solid: bool,
    ) {
        let d = dir.normalize_or(Vec2::X);
        let axis = Vec3::new(d.x * lean.sin(), lean.cos(), d.y * lean.sin()) * (h + 1.0);
        self.crystals.push(Crystal {
            base: at - Vec3::Y * 1.0,
            axis,
            radius: r,
            sides: 5 + (seed % 3) as u32,
            seed: (seed >> 8) as u32,
            color: RAW * (0.85 + 0.4 * ((seed >> 40) % 1000) as f32 / 1000.0),
            lit,
            solid,
        });
    }

    /// An outcrop of seam crystal at ground level: one big shard for cover
    /// and small ones round it.
    fn outcrop(&mut self, rng: &mut Rng, p: Vec2, h: f32) {
        let g = self.ground_height(p.x, p.y);
        let lean_dir = Vec2::new(rng.range(-1.0, 1.0), rng.range(-1.0, 1.0));
        self.shard(
            rng.next_u64(),
            Vec3::new(p.x, g, p.y),
            h,
            1.1,
            lean_dir,
            rng.range(0.05, 0.3),
            true,
            true,
        );
        for _ in 0..4 {
            let a = rng.range(0.0, TAU);
            let q = p + Vec2::new(a.cos(), a.sin()) * rng.range(1.6, 2.8);
            let gq = self.ground_height(q.x, q.y);
            self.shard(
                rng.next_u64(),
                Vec3::new(q.x, gq, q.y),
                rng.range(0.8, 1.8),
                0.35,
                q - p,
                rng.range(0.3, 0.7),
                rng.chance(0.4),
                false,
            );
        }
    }

    /// The seam: a river of plasma along the wash floor where the rock is
    /// cut back to it, a vein glowing up the quarried Face by the adit's
    /// mouth, and on through the hall. Crystal breaks out along it.
    fn seam(&mut self, rng: &mut Rng) {
        let line = seam_line();
        let mut pts = Vec::new();
        let mut widths = Vec::new();
        for p in &line {
            pts.push(Vec3::new(p.x, self.ground_height(p.x, p.y) + 0.05, p.y));
            widths.push(0.8 + 0.45 * (p.x * 0.3).sin().abs());
        }
        self.glow_paths.push(GlowPath {
            points: pts,
            widths,
            color: PLASMA * 0.9,
        });
        // The vein up the Face, west of the adit's mouth: from the foot up
        // the cut stone to the lip, wandering and thinning.
        let n = 9;
        let at = |k: usize| {
            let t = k as f32 / n as f32;
            let x = 95.5 + 2.2 * (t * 5.0).sin() + 1.5 * t;
            // In front of the cut stone (the lower tier's face, then the
            // upper's, set back).
            let z = face(x) - 0.12 + if t > 0.5 { 1.2 } else { 0.0 };
            Vec3::new(x, -0.3 + t * (TOP + 0.2), z)
        };
        for k in 0..n {
            let (a, b) = (at(k), at(k + 1));
            let b = if k == n / 2 {
                Vec3::new(b.x, b.y, a.z)
            } else {
                b
            };
            self.segment(
                a,
                b,
                0.22 - 0.012 * k as f32,
                PLASMA * 0.85,
                Material::Glow,
                false,
            );
        }
        // Crystal along the seam on the wash floor, cover under the bridge
        // and either side of it.
        for (x, h) in [
            (76.0, 3.0),
            (88.0, 3.6),
            (104.0, 2.6),
            (117.0, 4.2),
            (131.0, 3.2),
        ] {
            let p = line
                .windows(2)
                .find(|w| x <= w[1].x)
                .map(|w| w[0].lerp(w[1], (x - w[0].x) / (w[1].x - w[0].x)))
                .unwrap_or(line[0]);
            let p = p + Vec2::new(0.0, rng.range(-2.5, 2.5));
            if seg(p, BRIDGE.0, BRIDGE.1).0 < DECK_HALF + 2.0
                || RAMPS
                    .iter()
                    .any(|&(s, e, hw)| seg(p, s, e + (e - s).normalize() * 4.0).0 < hw + 4.0)
            {
                continue;
            }
            self.outcrop(rng, p, h);
        }
        // Cut faces: low steps of dark rock either side of the vein where
        // the rig has cut it back.
        for (p, yaw) in [
            (Vec2::new(76.0, 124.5), 0.25),
            (Vec2::new(136.0, 134.0), -0.4),
        ] {
            let g = self.ground_height(p.x, p.y);
            self.stone(
                p,
                yaw,
                Vec2::new(3.2, 1.4),
                g - 0.5,
                g + 1.5,
                STONE_DARK,
                0.25,
            );
        }
    }

    /// The quarried Face: sheer cut stone in big blocks, two tiers, the top
    /// one set back, scorched along the plasma cuts. Clear of the mouths.
    fn quarry_face(&mut self, rng: &mut Rng) {
        let mut x = QUARRY.0;
        while x < QUARRY.1 {
            let w = rng.range(5.0, 8.0).min(QUARRY.1 - x);
            let c = x + w * 0.5;
            x += w;
            if CAVES.iter().any(|(a, b)| {
                c + w * 0.5 > a.x - 0.5 && c - w * 0.5 < b.x + 0.5 && a.y < face(c) + FACE_RISE
            }) {
                continue;
            }
            // Along the face here.
            let (f0, f1) = (face(c - w * 0.5), face(c + w * 0.5));
            let along = Vec2::new(w, f1 - f0).normalize();
            let yaw = (-along.y).atan2(along.x);
            let inward = Vec2::new(-along.y, along.x);
            let foot = Vec2::new(c, (f0 + f1) * 0.5);
            let shade = [STONE, STONE_PALE, STONE * 1.1][rng.index(3)] * rng.range(0.92, 1.05);
            // The lower tier, its face on the foot; the upper set back.
            let low = rng.range(4.5, 6.0);
            let depth = 3.0;
            self.oboxes.push(OBox {
                center: foot + inward * (depth * 0.5),
                yaw,
                half: Vec2::new(w * 0.5 + 0.02, depth * 0.5),
                y0: -1.0,
                y1: low,
                color: shade,
                side_mat: Material::Rubble,
                top_mat: Material::Rubble,
                chip: 0.15,
                ..OBox::default()
            });
            let back = rng.range(1.2, 2.0);
            self.oboxes.push(OBox {
                center: foot + inward * (back + depth * 0.5),
                yaw,
                half: Vec2::new(w * 0.5 + 0.03, depth * 0.5),
                y0: low - 0.2,
                y1: TOP + rng.range(0.1, 0.5),
                color: shade * rng.range(0.9, 1.0),
                side_mat: Material::Rubble,
                top_mat: Material::Rubble,
                chip: 0.2,
                ..OBox::default()
            });
            // A plasma scorch along the cut, now and then.
            if rng.chance(0.5) {
                let y = rng.range(1.5, low - 1.0);
                self.oboxes.push(OBox {
                    center: foot - inward * 0.03,
                    yaw,
                    half: Vec2::new(w * 0.5 * rng.range(0.5, 0.9), 0.03),
                    y0: y,
                    y1: y + 0.12,
                    color: PLASMA * 0.6,
                    side_mat: Material::Glow,
                    top_mat: Material::Glow,
                    solid: false,
                    ..OBox::default()
                });
            }
            // Spoil at the foot.
            if rng.chance(0.4) {
                let q = foot - inward * rng.range(2.5, 4.0) + along * rng.range(-2.0, 2.0);
                if wash(q) > -0.5 && seg(q, BRIDGE.0, BRIDGE.1).0 > DECK_HALF + 2.0 {
                    let s = rng.range(0.5, 1.1);
                    self.boulder(rng, q, s, false);
                }
            }
        }
    }

    /// Broken natural rock along the rest of the Face, so it reads as stone,
    /// not sand: tall blocks against the cliff, their tops ragged.
    fn rough_face(&mut self, rng: &mut Rng) {
        let mut x = -4.0;
        while x < SIZE + 4.0 {
            let w = rng.range(2.0, 3.4);
            let c = x + w;
            x += w * 2.0 - 0.5;
            if (QUARRY.0 - 1.0..QUARRY.1 + 1.0).contains(&c) || scree(c) > 0.3 {
                continue;
            }
            if CAVES
                .iter()
                .any(|(a, b)| c + w > a.x - 0.8 && c - w < b.x + 0.8 && a.y < face(c) + FACE_RISE)
            {
                continue;
            }
            let f = face(c);
            let full = mesa(Vec2::new(c, f + FACE_RISE + 1.0));
            if full < 0.15 {
                continue;
            }
            let p = Vec2::new(c, f + 1.4 + rng.range(-0.3, 0.6));
            let top = TOP * full + rng.range(-1.8, 0.6);
            let slant = (face(c + 1.0) - face(c - 1.0)) * 0.5;
            self.stone(
                p,
                -slant.atan() + rng.range(-0.1, 0.1),
                Vec2::new(w, rng.range(1.6, 2.4)),
                -0.6,
                top,
                [STONE, STONE_DARK, STONE_PALE][rng.index(3)] * rng.range(0.85, 1.05),
                0.5,
            );
            if let Some(o) = self.oboxes.last_mut() {
                o.broken = true;
            }
            // Blocks fallen out of it.
            if rng.chance(0.45) {
                let q = p - Vec2::new(rng.range(-2.0, 2.0), rng.range(3.5, 6.0));
                if wash(q) > -0.3 {
                    let s = rng.range(0.6, 1.6);
                    self.boulder(rng, q, s, s > 1.4);
                }
            }
        }
    }

    /// The scree where the Face fell in: rock strewn down the slope, big
    /// blocks to hide behind on the climb.
    fn scree_rocks(&mut self, rng: &mut Rng) {
        let mut placed = 0;
        for _ in 0..400 {
            if placed >= 34 {
                break;
            }
            let x = rng.range(SCREE.0 + 2.0, SCREE.1 - 2.0);
            let f = face(x);
            let z = rng.range(f - SCREE_RUN + 2.0, f + FACE_RISE);
            let p = Vec2::new(x, z);
            // A worn way up the middle stays clear.
            if WAYS_UP.iter().any(|&(a, b)| seg(p, a, b).0 < 5.0)
                || in_cave(p)
                || wash(p) < -0.3
                || self
                    .oboxes
                    .iter()
                    .any(|o| o.solid && o.center.distance(p) < 6.0)
            {
                continue;
            }
            let big = placed % 3 == 0;
            let s = if big {
                rng.range(1.6, 2.4)
            } else {
                rng.range(0.5, 1.1)
            };
            self.boulder(rng, p, s, big);
            placed += 1;
        }
        // The broken lip at the top: blocks tipped over the edge.
        for k in 0..5 {
            let x = SCREE.0 + 5.0 + k as f32 * 5.5 + rng.range(-1.0, 1.0);
            if (x - WAYS_UP[1].0.x).abs() < 8.0 {
                continue;
            }
            let p = Vec2::new(x, face(x) + FACE_RISE + rng.range(0.0, 2.0));
            let g = self.ground_height(p.x, p.y);
            self.oboxes.push(OBox {
                center: p,
                yaw: rng.range(-0.4, 0.4),
                half: Vec2::new(rng.range(1.2, 2.0), rng.range(0.9, 1.4)),
                y0: g - 1.0,
                y1: g + rng.range(1.0, 2.0),
                tilt: rng.range(-0.3, 0.3),
                color: STONE * rng.range(0.85, 1.05),
                side_mat: Material::Rubble,
                top_mat: Material::Rubble,
                chip: 0.6,
                ..OBox::default()
            });
        }
    }

    /// The mine: rock walls round every corridor, roofs flush with the
    /// mesa, plasma lamps, crystal veins in the walls, timber portals at the
    /// Ithessar's mouths, ore carts.
    fn mine_caves(&mut self, rng: &mut Rng) {
        // Walls: along every edge of every cave, where the other side is
        // rock (not another cave, the box cut, or open air past the Face).
        let step = 2.0;
        for &(a, b) in &CAVES {
            let edges = [
                (Vec2::new(a.x, a.y), Vec2::new(b.x, a.y), -Vec2::Y),
                (Vec2::new(a.x, b.y), Vec2::new(b.x, b.y), Vec2::Y),
                (Vec2::new(a.x, a.y), Vec2::new(a.x, b.y), -Vec2::X),
                (Vec2::new(b.x, a.y), Vec2::new(b.x, b.y), Vec2::X),
            ];
            for (e0, e1, out) in edges {
                let len = e0.distance(e1);
                let along = (e1 - e0) / len;
                let n = (len / step).round() as usize;
                let mut start: Option<f32> = None;
                for k in 0..=n {
                    let t = k as f32 * step;
                    let closed = k < n && {
                        let p = e0 + along * (t + step * 0.5) + out * 1.0;
                        in_rock(p) && (0.0..SIZE).contains(&p.y)
                    };
                    match (closed, start) {
                        (true, None) => start = Some(t),
                        (false, Some(s)) => {
                            self.cave_wall(rng, e0 + along * s, e0 + along * t, out);
                            start = None;
                        }
                        _ => {}
                    }
                }
            }
        }
        // Roofs: the 2 m cells within the walls' reach of a cave, back from
        // the Face's lip, gathered into slabs that don't overlap.
        let cell = 2.0;
        let mut rows: Vec<(f32, Vec<(f32, f32)>)> = Vec::new();
        let mut z = 136.0;
        while z < SIZE + 6.0 {
            let mut runs = Vec::new();
            let mut x = 56.0;
            let mut start: Option<f32> = None;
            while x <= 164.0 {
                let on = x < 164.0 && roofed(Vec2::new(x + cell * 0.5, z + cell * 0.5));
                match (on, start) {
                    (true, None) => start = Some(x),
                    (false, Some(s)) => {
                        runs.push((s, x));
                        start = None;
                    }
                    _ => {}
                }
                x += cell;
            }
            rows.push((z, runs));
            z += cell;
        }
        // Merge rows with the same runs into slabs.
        let mut open: Vec<((f32, f32), f32)> = Vec::new();
        let mut slabs: Vec<(Vec2, Vec2)> = Vec::new();
        for (z, runs) in rows
            .iter()
            .chain(std::iter::once(&(SIZE + 6.0, Vec::new())))
        {
            let mut next = Vec::new();
            for &(run, z0) in &open {
                if runs.contains(&run) {
                    next.push((run, z0));
                } else {
                    slabs.push((Vec2::new(run.0, z0), Vec2::new(run.1, *z)));
                }
            }
            for &run in runs {
                if !next.iter().any(|&(r, _)| r == run) {
                    next.push((run, *z));
                }
            }
            open = next;
        }
        for (a, b) in slabs {
            self.oboxes.push(OBox {
                center: (a + b) * 0.5,
                half: (b - a) * 0.5,
                y0: ROOF.0,
                y1: ROOF.1,
                color: MESA_TOP,
                side_mat: Material::Rubble,
                top_mat: Material::Sand,
                ramp: true,
                ..OBox::default()
            });
        }
        // Lamps: plasma in a crystal cage on the walls every 12 m (the
        // Resonant's breach is lit their own way), and veins of raw crystal
        // breaking out of the walls.
        for &(a, b) in &CAVES[..5] {
            let long_z = b.y - a.y > b.x - a.x;
            let len = if long_z { b.y - a.y } else { b.x - a.x };
            let n = (len / 12.0) as usize;
            for k in 0..n {
                let t = (k as f32 + 0.5) / n as f32;
                for side in [0.0f32, 1.0] {
                    let (p, out) = if long_z {
                        let x = a.x + (b.x - a.x) * side;
                        (
                            Vec2::new(x, a.y + (b.y - a.y) * t),
                            Vec2::new(side * 2.0 - 1.0, 0.0),
                        )
                    } else {
                        let z = a.y + (b.y - a.y) * side;
                        (
                            Vec2::new(a.x + (b.x - a.x) * t, z),
                            Vec2::new(0.0, side * 2.0 - 1.0),
                        )
                    };
                    if !in_rock(p + out * 1.0) || !roofed(p) {
                        continue;
                    }
                    if (k + side as usize).is_multiple_of(2) {
                        self.lamp(p - out * 0.15);
                    } else {
                        // A vein: shards growing out of the wall, low.
                        for _ in 0..3 {
                            let q = p + Vec2::new(rng.range(-1.5, 1.5), rng.range(-1.5, 1.5))
                                * out.perp().abs()
                                - out * 0.2;
                            let h = rng.range(0.6, 1.6);
                            self.shard(
                                rng.next_u64(),
                                Vec3::new(q.x, rng.range(0.5, 3.0), q.y),
                                h,
                                0.25,
                                -out,
                                rng.range(0.9, 1.3),
                                true,
                                false,
                            );
                        }
                    }
                }
            }
        }
        // Timber portals at the Ithessar's two mouths in the Face.
        for (a, b) in [CAVES[0], CAVES[3]] {
            let xc = (a.x + b.x) * 0.5;
            let mut z = face(xc);
            while !roofed(Vec2::new(xc, z)) {
                z += 0.5;
            }
            let z = z + 0.6;
            for x in [a.x - 0.35, b.x + 0.35] {
                self.oboxes.push(OBox {
                    center: Vec2::new(x, z),
                    half: Vec2::splat(0.35),
                    y0: -0.3,
                    y1: ROOF.0,
                    color: TIMBER,
                    side_mat: Material::Wood,
                    top_mat: Material::Wood,
                    ..OBox::default()
                });
            }
            self.oboxes.push(OBox {
                center: Vec2::new(xc, z),
                half: Vec2::new((b.x - a.x) * 0.5 + 0.4, 0.4),
                y0: ROOF.0 - 0.7,
                y1: ROOF.0 + 0.02,
                color: TIMBER * 0.9,
                side_mat: Material::Wood,
                top_mat: Material::Wood,
                ..OBox::default()
            });
        }
        // Ore carts: on the drift, and in the west adit.
        for &(x, z, yaw) in &[
            (124.0f32, 187.6f32, FRAC_PI_2),
            (140.0, 194.4, FRAC_PI_2 + 0.1),
            (67.6, 158.0, 0.0),
        ] {
            self.ore_cart(rng, Vec2::new(x, z), yaw);
        }
    }

    /// A plasma lamp in a crystal cage, on a cave wall.
    fn lamp(&mut self, at: Vec2) {
        self.oboxes.push(OBox {
            center: at,
            half: Vec2::splat(0.22),
            y0: 4.4,
            y1: 5.1,
            color: PLASMA,
            side_mat: Material::Glow,
            top_mat: Material::Glow,
            solid: false,
            ..OBox::default()
        });
        self.oboxes.push(OBox {
            center: at,
            half: Vec2::splat(0.34),
            y0: 5.1,
            y1: 5.25,
            color: GLASS,
            side_mat: Material::Vhaal,
            top_mat: Material::Vhaal,
            solid: false,
            ..OBox::default()
        });
    }

    /// A cave wall from `a` to `b`, standing outward (`out`) of the cave.
    fn cave_wall(&mut self, rng: &mut Rng, a: Vec2, b: Vec2, out: Vec2) {
        let len = a.distance(b);
        let along = (b - a) / len;
        // In pieces 4-6 m long, their faces flush (a step between two
        // would catch a tank scraping along the wall and throw it).
        let mut t = 0.0;
        while t < len - 0.01 {
            let piece = rng.range(4.0, 6.0).min(len - t);
            let piece = if len - t - piece < 1.5 {
                len - t
            } else {
                piece
            };
            let c = a + along * (t + piece * 0.5) + out * (CAVE_WALL * 0.5 - 0.02);
            let half = if along.x.abs() > 0.5 {
                Vec2::new(piece * 0.5 + CAVE_WALL * 0.5, CAVE_WALL * 0.5)
            } else {
                Vec2::new(CAVE_WALL * 0.5, piece * 0.5 + CAVE_WALL * 0.5)
            };
            let shade = [STONE, STONE_DARK, STONE_PALE][rng.index(3)] * rng.range(0.85, 1.0);
            self.stone(c, 0.0, half, -0.5, ROOF.0 + 0.3, shade, 0.3);
            t += piece;
        }
    }

    /// The hall, where the seam is worked under the rock: pillars left to
    /// hold the roof, the vein glowing across the floor between them,
    /// crystal standing where it hasn't been cut yet, a stack of cut blocks.
    fn pillared_hall(&mut self, rng: &mut Rng) {
        for (k, &p) in PILLARS.iter().enumerate() {
            let s = rng.range(2.3, 2.8);
            self.stone(
                p,
                rng.range(-0.08, 0.08),
                Vec2::new(s, s * rng.range(0.85, 1.0)),
                -0.5,
                ROOF.0 + 0.1,
                [STONE, STONE_DARK][k % 2] * rng.range(0.9, 1.0),
                0.5,
            );
            // A lamp on each pillar's face toward the middle.
            let mid = (HALL.0 + HALL.1) * 0.5;
            let out = if p.x < mid.x { Vec2::X } else { -Vec2::X };
            self.lamp(p + out * (s + 0.1));
        }
        // The vein across the hall floor, west to east toward the adit.
        let mut pts = Vec::new();
        let mut widths = Vec::new();
        for k in 0..=14 {
            let t = k as f32 / 14.0;
            let x = HALL.0.x + 2.0 + t * (ADIT.1.x - 2.0 - HALL.0.x - 2.0);
            let z = 184.5 + 3.0 * (t * 7.0).sin() - 2.0 * t;
            pts.push(Vec3::new(x, 0.04, z));
            widths.push(0.6 + 0.3 * (t * 11.0).sin().abs());
        }
        self.glow_paths.push(GlowPath {
            points: pts,
            widths,
            color: PLASMA * 0.85,
        });
        // Crystal not yet cut, in the hall's west end (clear of the way in
        // from the west adit, round the pillar to the hall's middle).
        for (x, z, h) in [(66.0, 184.0, 3.2), (65.5, 175.5, 2.4), (78.0, 195.0, 2.6)] {
            self.outcrop(rng, Vec2::new(x, z), h);
        }
        // Cut blocks stacked for the train, by the crosscut.
        self.block_stack(rng, Vec2::new(92.0, 195.0), 0.0);
    }

    /// Cut crystal blocks, stacked: a big one with smaller round it.
    fn block_stack(&mut self, rng: &mut Rng, at: Vec2, yaw: f32) {
        let g = self.ground_height(at.x, at.y);
        let rot = Vec2::from_angle(-yaw);
        for (k, (dx, dz, h)) in [
            (-1.9, 0.0, 1.6),
            (0.0, 0.0, 3.0),
            (1.9, 0.0, 1.6),
            (0.0, 1.9, 1.2),
        ]
        .into_iter()
        .enumerate()
        {
            let off = rot.rotate(Vec2::new(dx, dz));
            self.oboxes.push(OBox {
                center: at + off,
                yaw,
                half: Vec2::splat(0.9 - 0.02 * k as f32),
                y0: g - 0.2,
                y1: g + h,
                color: GLASS * rng.range(1.0, 1.4),
                side_mat: Material::Vhaal,
                top_mat: Material::Vhaal,
                ..OBox::default()
            });
        }
    }

    /// The Resonant's breach: their own tunnel in from the Face to the
    /// drift, rough-cut, lit with their blue crystal, rubble at its mouth.
    fn breach(&mut self, rng: &mut Rng) {
        let (a, b) = CAVES[5];
        // Blue crystal set in the walls up the tunnel.
        let mut z = face((a.x + b.x) * 0.5) + 7.0;
        let mut side = 0.0f32;
        while z < b.y - 4.0 {
            let x = a.x + (b.x - a.x) * side;
            let out = Vec2::new(side * 2.0 - 1.0, 0.0);
            self.oboxes.push(OBox {
                center: Vec2::new(x, z) - out * 0.2,
                yaw: rng.range(-0.3, 0.3),
                half: Vec2::splat(0.3),
                y0: 1.0,
                y1: rng.range(3.5, 5.0),
                tilt: rng.range(-0.25, 0.25),
                color: HOLY * rng.range(0.8, 1.05),
                side_mat: Material::Glow,
                top_mat: Material::Glow,
                solid: false,
                ..OBox::default()
            });
            z += 9.0;
            side = 1.0 - side;
        }
        // Their shards raised either side of the mouth, and rubble they
        // dug out heaped by it.
        let f = face(153.0);
        for (x, dz, h, lean) in [
            (145.0, -2.0, 5.5, 0.25),
            (161.0, -2.5, 4.5, -0.3),
            (164.0, -4.5, 3.2, -0.15),
        ] {
            let p = Vec2::new(x, face(x) + dz);
            let g = self.ground_height(p.x, p.y);
            self.oboxes.push(OBox {
                center: p,
                yaw: rng.range(0.0, TAU),
                half: Vec2::splat(0.5),
                y0: g - 0.5,
                y1: g + h,
                tilt: lean,
                color: HOLY * rng.range(0.85, 1.1),
                side_mat: Material::Glow,
                top_mat: Material::Glow,
                ..OBox::default()
            });
        }
        for p in [Vec2::new(142.0, f - 6.0), Vec2::new(164.0, f - 8.0)] {
            self.rock_pile(rng, p, 1.6);
        }
    }

    /// The box cut: an open trench up from the adit's end to the mesa, its
    /// sides faced in cut stone.
    fn box_cut(&mut self, rng: &mut Rng) {
        let (a, b) = BOX_CUT;
        for (z, out) in [(a.y, -1.0f32), (b.y, 1.0)] {
            let mut x = a.x + 2.0;
            while x < b.x {
                let w = rng.range(4.0, 6.0).min(b.x - x);
                let c = x + w * 0.5;
                let top = TOP + 0.3;
                self.stone(
                    Vec2::new(c, z + out * (CAVE_WALL * 0.5 - 0.02)),
                    0.0,
                    Vec2::new(w * 0.5 + 0.01, CAVE_WALL * 0.5),
                    -0.5,
                    top,
                    [STONE, STONE_PALE][rng.index(2)] * rng.range(0.9, 1.0),
                    0.2,
                );
                x += w;
            }
        }
    }

    /// The track, from the end of the adit out across the bridge and the
    /// flats to the south edge: two rails on sleepers. Visual only.
    fn railroad(&mut self) {
        let total = track_len(TRACK.len() - 1);
        let y_at = |m: &Map, s: f32| {
            let (p, dir) = track_at(s);
            if on_bridge(p) {
                DECK_TOP
            } else {
                let side = dir.perp() * 1.5;
                m.ground_height(p.x, p.y)
                    .max(m.ground_height(p.x + side.x, p.y + side.y))
                    .max(m.ground_height(p.x - side.x, p.y - side.y))
            }
        };
        let step = 1.6;
        let n = (total / step) as usize;
        for k in 0..n {
            let s = step * (k as f32 + 0.5);
            let (p, dir) = track_at(s);
            let y = y_at(self, s);
            self.oboxes.push(OBox {
                center: p,
                yaw: yaw_along(dir),
                half: Vec2::new(GAUGE * 0.5 + 0.7, 0.18),
                y0: y - 0.1,
                y1: y + 0.08,
                color: TIMBER * 0.8,
                side_mat: Material::Wood,
                top_mat: Material::Wood,
                solid: false,
                ..OBox::default()
            });
        }
        // Rails, in runs of up to 6 m following the ground, broken at the
        // track's bends.
        let mut s = 0.0;
        let bends: Vec<f32> = (1..TRACK.len()).map(track_len).collect();
        while s < total - 0.01 {
            let next_bend = bends
                .iter()
                .copied()
                .find(|&b| b > s + 0.01)
                .unwrap_or(total);
            let e = (s + 6.0).min(next_bend);
            let (pa, dir) = track_at(s + 0.001);
            let (pb, _) = track_at(e - 0.001);
            let (ya, yb) = (y_at(self, s) + 0.16, y_at(self, e - 0.001) + 0.16);
            for side in [-1.0f32, 1.0] {
                let o = dir.perp() * side * GAUGE * 0.5;
                self.segment(
                    Vec3::new(pa.x + o.x, ya, pa.y + o.y),
                    Vec3::new(pb.x + o.x, yb, pb.y + o.y),
                    0.08,
                    STEEL * 1.4,
                    Material::Metal,
                    false,
                );
            }
            s = e;
        }
    }

    /// The rail bridge out of the adit's mouth over the wash: a deck on
    /// piers of worked crystal, low steel girders along its edges, a truss
    /// over them, tilted plates up onto it at each end.
    fn rail_bridge(&mut self) {
        let (a, b) = BRIDGE;
        let len = a.distance(b);
        let dir = (b - a) / len;
        let yaw = yaw_along(dir);
        let mid = (a + b) * 0.5;
        self.oboxes.push(OBox {
            center: mid,
            yaw,
            half: Vec2::new(DECK_HALF, len * 0.5),
            y0: DECK_TOP - DECK_T,
            y1: DECK_TOP,
            color: STEEL * 1.2,
            side_mat: Material::Metal,
            top_mat: Material::Metal,
            ramp: true,
            ..OBox::default()
        });
        // Plates up onto it from the ground at each end.
        for (end, out) in [(a, -dir), (b, dir)] {
            let run = 2.6;
            let rise = DECK_TOP + 0.15;
            let slope = (rise / run).atan();
            let c = end + out * (run * 0.5 - 0.05);
            // Local +z points along `out`; tip its far (+z) end down.
            self.oboxes.push(OBox {
                center: c,
                yaw: yaw_along(out),
                half: Vec2::new(DECK_HALF - 0.4, run * 0.5 + 0.1),
                y0: DECK_TOP * 0.5 - 0.2 - 0.15,
                y1: DECK_TOP * 0.5 + 0.2 - 0.15,
                tilt: slope,
                color: STEEL,
                side_mat: Material::Metal,
                top_mat: Material::Metal,
                ramp: true,
                ..OBox::default()
            });
        }
        // Girders along each edge: low enough to shoot over, high enough to
        // keep a tank on the deck. A truss of posts and diagonals over them.
        let side_dir = dir.perp();
        for side in [-1.0f32, 1.0] {
            let off = side_dir * side * (DECK_HALF - 0.25);
            self.oboxes.push(OBox {
                center: mid + off,
                yaw,
                half: Vec2::new(0.25, len * 0.5),
                y0: DECK_TOP - 0.01,
                y1: DECK_TOP + 0.9,
                color: RUST,
                side_mat: Material::Rust,
                top_mat: Material::Rust,
                ..OBox::default()
            });
            let n = (len / 6.0) as usize;
            let at = |k: usize, y: f32| {
                let p = a + dir * (len * k as f32 / n as f32) + off;
                Vec3::new(p.x, y, p.y)
            };
            for k in 0..=n {
                let foot = at(k, DECK_TOP + 0.9);
                self.segment(
                    foot,
                    foot + Vec3::Y * 2.6,
                    0.12,
                    RUST,
                    Material::Rust,
                    false,
                );
                if k < n {
                    let (p, q) = if k % 2 == 0 {
                        (foot, at(k + 1, DECK_TOP + 3.5))
                    } else {
                        (foot + Vec3::Y * 2.6, at(k + 1, DECK_TOP + 0.9))
                    };
                    self.segment(p, q, 0.08, RUST, Material::Rust, false);
                }
            }
            self.segment(
                at(0, DECK_TOP + 3.5),
                at(n, DECK_TOP + 3.5),
                0.14,
                RUST,
                Material::Rust,
                false,
            );
        }
        // Piers: a crystal column under each rail where the wash falls away,
        // a cap beam across their tops, a seam of plasma up the faces.
        // Ten meters apart: room to drive between them.
        let mut t = 9.0;
        while t < len - 4.0 {
            let c = a + dir * t;
            let floor = self.ground_height(c.x, c.y);
            t += 10.0;
            if floor > -2.5 {
                continue;
            }
            for side in [-1.0f32, 1.0] {
                let at = c + side_dir * side * 2.4;
                let g = self.ground_height(at.x, at.y);
                self.oboxes.push(OBox {
                    center: at,
                    yaw: yaw + PI / 4.0,
                    half: Vec2::splat(0.8),
                    y0: g - 0.5,
                    y1: DECK_TOP - DECK_T,
                    color: GLASS,
                    side_mat: Material::Vhaal,
                    top_mat: Material::Vhaal,
                    ..OBox::default()
                });
                let glow = at + side_dir * side * 1.1;
                self.oboxes.push(OBox {
                    center: glow,
                    yaw,
                    half: Vec2::new(0.06, 0.12),
                    y0: g + 0.3,
                    y1: DECK_TOP - DECK_T - 0.4,
                    color: PLASMA,
                    side_mat: Material::Glow,
                    top_mat: Material::Glow,
                    solid: false,
                    ..OBox::default()
                });
            }
            self.oboxes.push(OBox {
                center: c,
                yaw,
                half: Vec2::new(DECK_HALF + 0.3, 0.9),
                y0: DECK_TOP - DECK_T - 0.9,
                y1: DECK_TOP - DECK_T + 0.01,
                color: GLASS * 1.2,
                side_mat: Material::Vhaal,
                top_mat: Material::Vhaal,
                ..OBox::default()
            });
            let (l, r) = (c - side_dir * 2.4, c + side_dir * 2.4);
            self.segment(
                Vec3::new(l.x, floor + 3.0, l.y),
                Vec3::new(r.x, DECK_TOP - DECK_T - 0.9, r.y),
                0.1,
                STEEL,
                Material::Metal,
                false,
            );
        }
    }

    /// Rock in the wash: blocks fallen from the banks, cover along the
    /// floor.
    fn wash_rocks(&mut self, rng: &mut Rng) {
        let mut placed = 0;
        for _ in 0..600 {
            if placed >= 13 {
                break;
            }
            let k = rng.index(WASH.len() - 1);
            let p = WASH[k].lerp(WASH[k + 1], rng.f32());
            let (_, along) = wash_near(p);
            let (_, half) = wash_profile(along);
            let dir = (WASH[k + 1] - WASH[k]).normalize().perp();
            // Toward one side, so the middle of the floor stays a way through.
            let side = if rng.chance(0.5) { 1.0 } else { -1.0 };
            let q = p + dir * side * rng.range(half * 0.45, half + 0.5);
            if q.min_element() < 12.0
                || q.max_element() > SIZE - 12.0
                || seg(q, BRIDGE.0, BRIDGE.1).0 < DECK_HALF + 3.0
                || RAMPS
                    .iter()
                    .chain(&FORDS)
                    .any(|&(s, e, h)| seg(q, s, e).0 < h + 3.0)
                || self
                    .oboxes
                    .iter()
                    .any(|o| o.solid && o.center.distance(q) < 11.0)
                || self
                    .crystals
                    .iter()
                    .any(|c| c.solid && Vec2::new(c.base.x, c.base.z).distance(q) < 7.0)
            {
                continue;
            }
            let s = rng.range(1.3, 2.2);
            self.rock_pile(rng, q, s);
            placed += 1;
        }
    }

    /// A cutting rig at `at` on ground at `floor`, its boom reaching out
    /// along `yaw`: a crystal-hulled body on hover pads, a plasma head
    /// glowing at the end of the boom.
    fn cutting_rig(&mut self, at: Vec2, yaw: f32, floor: f32) {
        let rig = Rig {
            pos: Vec3::new(at.x, floor, at.y),
            rot: Quat::from_rotation_y(yaw),
        };
        for (x, z) in [(-1.8, -2.2), (1.8, -2.2), (-1.8, 2.2), (1.8, 2.2)] {
            rig.part(
                self,
                Vec3::new(x, 0.45, z),
                Vec3::new(0.8, 0.3, 1.0),
                Quat::IDENTITY,
                STEEL,
                Material::Metal,
                true,
                0.0,
            );
            rig.part(
                self,
                Vec3::new(x, 0.12, z),
                Vec3::new(0.6, 0.05, 0.8),
                Quat::IDENTITY,
                PLASMA,
                Material::Glow,
                false,
                0.0,
            );
        }
        rig.part(
            self,
            Vec3::new(0.0, 1.7, 0.0),
            Vec3::new(2.2, 0.95, 3.2),
            Quat::IDENTITY,
            GLASS,
            Material::Vhaal,
            true,
            0.0,
        );
        rig.part(
            self,
            Vec3::new(0.0, 3.1, -1.6),
            Vec3::new(1.4, 0.5, 1.2),
            Quat::IDENTITY,
            GLASS * 1.3,
            Material::Vhaal,
            true,
            0.0,
        );
        rig.part(
            self,
            Vec3::new(0.0, 2.68, 0.0),
            Vec3::new(2.25, 0.04, 3.25),
            Quat::IDENTITY,
            PLASMA,
            Material::Glow,
            false,
            0.0,
        );
        let root = Vec3::new(0.0, 2.9, 2.6);
        let elbow = Vec3::new(0.0, 5.0, 6.0);
        let head = Vec3::new(0.0, 1.2, 8.5);
        for x in [-0.6, 0.6] {
            let o = Vec3::new(x, 0.0, 0.0);
            self.segment(
                rig.point(root + o),
                rig.point(elbow + o),
                0.28,
                STEEL,
                Material::Metal,
                false,
            );
            self.segment(
                rig.point(elbow + o),
                rig.point(head + o),
                0.24,
                STEEL,
                Material::Metal,
                false,
            );
        }
        let hc = rig.point(head);
        let axle = rig.rot * Vec3::X * 0.35;
        self.disc(hc, axle, 1.3, GLASS * 1.5, Material::Vhaal);
        self.disc(hc, axle * 1.25, 0.6, PLASMA, Material::Glow);
    }

    /// A small ore cart at `at` heading `yaw`: a crystal tub on a steel
    /// frame, full of raw crystal.
    fn ore_cart(&mut self, rng: &mut Rng, at: Vec2, yaw: f32) {
        let g = self.ground_height(at.x, at.y);
        let rig = Rig {
            pos: Vec3::new(at.x, g, at.y),
            rot: Quat::from_rotation_y(yaw),
        };
        rig.part(
            self,
            Vec3::new(0.0, 0.5, 0.0),
            Vec3::new(0.9, 0.25, 1.5),
            Quat::IDENTITY,
            STEEL,
            Material::Metal,
            true,
            0.0,
        );
        rig.part(
            self,
            Vec3::new(0.0, 1.35, 0.0),
            Vec3::new(1.05, 0.6, 1.6),
            Quat::IDENTITY,
            GLASS * 1.2,
            Material::Vhaal,
            true,
            0.0,
        );
        for (x, z) in [(-0.95, -1.0), (0.95, -1.0), (-0.95, 1.0), (0.95, 1.0)] {
            let c = rig.point(Vec3::new(x, 0.35, z));
            self.disc(
                c,
                rig.rot * Vec3::X * 0.08,
                0.32,
                STEEL * 0.8,
                Material::Metal,
            );
        }
        for _ in 0..3 {
            let p = rig.point(Vec3::new(rng.range(-0.6, 0.6), 1.7, rng.range(-1.0, 1.0)));
            let dir = Vec2::new(rng.range(-1.0, 1.0), rng.range(-1.0, 1.0));
            self.shard(
                rng.next_u64(),
                p,
                rng.range(0.4, 0.9),
                0.25,
                dir,
                rng.range(0.2, 0.6),
                rng.chance(0.5),
                false,
            );
        }
    }

    /// One rail car centered at `at`, its length along `yaw`, rolled `roll`
    /// onto its side (a derailed car). A hopper of raw crystal, or the
    /// locomotive (`loco`), its crystal prow toward local +z.
    fn rail_car(&mut self, rng: &mut Rng, at: Vec2, yaw: f32, roll: f32, loco: bool) {
        let g = self.ground_height(at.x, at.y);
        let len: f32 = if loco { 13.0 } else { 9.0 };
        let half_w = 1.7;
        let pivot = Vec3::new(-half_w * roll.signum(), 0.0, 0.0);
        let rot = Quat::from_rotation_y(yaw) * Quat::from_rotation_z(roll);
        let base = Vec3::new(at.x, g + 0.2, at.y);
        let rig = Rig {
            pos: base + Quat::from_rotation_y(yaw) * pivot - rot * pivot,
            rot,
        };
        let id = Quat::IDENTITY;
        for z in [-(len * 0.5 - 2.0), len * 0.5 - 2.0] {
            rig.part(
                self,
                Vec3::new(0.0, 0.75, z),
                Vec3::new(1.2, 0.3, 1.5),
                id,
                STEEL * 0.8,
                Material::Metal,
                true,
                0.0,
            );
            for (x, dz) in [(-1.2, -0.9), (1.2, -0.9), (-1.2, 0.9), (1.2, 0.9)] {
                let c = rig.point(Vec3::new(x, 0.6, z + dz));
                self.disc(c, rig.rot * Vec3::X * 0.09, 0.55, STEEL, Material::Metal);
            }
        }
        rig.part(
            self,
            Vec3::new(0.0, 1.3, 0.0),
            Vec3::new(half_w, 0.25, len * 0.5),
            id,
            STEEL,
            Material::Metal,
            true,
            0.0,
        );
        if loco {
            rig.part(
                self,
                Vec3::new(0.0, 2.75, -0.5),
                Vec3::new(1.4, 1.2, len * 0.5 - 1.0),
                id,
                GLASS,
                Material::Vhaal,
                true,
                0.0,
            );
            rig.part(
                self,
                Vec3::new(0.0, 4.4, -len * 0.5 + 2.4),
                Vec3::new(1.55, 0.6, 1.8),
                id,
                GLASS * 1.3,
                Material::Vhaal,
                true,
                0.0,
            );
            for x in [-1.42, 1.42] {
                rig.part(
                    self,
                    Vec3::new(x, 2.6, 0.0),
                    Vec3::new(0.03, 0.12, len * 0.5 - 2.0),
                    id,
                    PLASMA,
                    Material::Glow,
                    false,
                    0.0,
                );
            }
            for k in 0..3 {
                rig.part(
                    self,
                    Vec3::new(0.0, 4.05, 1.0 + k as f32 * 1.6),
                    Vec3::new(0.5, 0.12, 0.4),
                    id,
                    PLASMA,
                    Material::Glow,
                    false,
                    0.0,
                );
            }
            let nose = rig.point(Vec3::new(0.0, 2.6, len * 0.5 - 1.6));
            let ahead = rig.rot * Vec3::Z;
            self.crystals.push(Crystal {
                base: nose,
                axis: ahead * 3.2 + Vec3::Y * 0.3,
                radius: 1.1,
                sides: 6,
                seed: rng.next_u64() as u32,
                color: GLASS * 1.4,
                lit: true,
                solid: false,
            });
        } else {
            rig.part(
                self,
                Vec3::new(0.0, 2.6, 0.0),
                Vec3::new(half_w, 1.05, len * 0.5 - 0.3),
                id,
                GLASS * 1.1,
                Material::Vhaal,
                true,
                0.0,
            );
            for x in [-half_w + 0.06, half_w - 0.06] {
                rig.part(
                    self,
                    Vec3::new(x, 2.5, 0.0),
                    Vec3::new(0.02, 0.06, len * 0.5 - 0.6),
                    id,
                    PLASMA,
                    Material::Glow,
                    false,
                    0.0,
                );
            }
            if roll.abs() < 0.3 {
                for _ in 0..5 {
                    let p = rig.point(Vec3::new(
                        rng.range(-1.1, 1.1),
                        3.5,
                        rng.range(-len * 0.4, len * 0.4),
                    ));
                    let dir = Vec2::new(rng.range(-1.0, 1.0), rng.range(-1.0, 1.0));
                    self.shard(
                        rng.next_u64(),
                        p,
                        rng.range(0.6, 1.4),
                        0.35,
                        dir,
                        rng.range(0.1, 0.6),
                        rng.chance(0.4),
                        false,
                    );
                }
            }
        }
    }

    /// The ore train, stopped on the flats headed out: the locomotive, two
    /// hoppers behind it under the tipple. Far down the line, two cars off
    /// the rails, one on its side, their load spilled.
    fn ore_train(&mut self, rng: &mut Rng) {
        let s0 = track_len(3) + 6.0;
        // Hoppers under and past the tipple, then the loco ahead (south).
        for (s, loco) in [(s0, false), (s0 + 10.0, false), (s0 + 22.0, true)] {
            let (p, dir) = track_at(s);
            self.rail_car(rng, p, yaw_along(dir), 0.0, loco);
        }
        // The wreck, far down the line.
        let (p, dir) = track_at(track_len(5) - 10.0);
        let side = dir.perp();
        self.rail_car(rng, p - side * 6.5, yaw_along(dir) + 0.35, 1.5, false);
        let (q, dir2) = track_at(track_len(5) + 2.0);
        self.rail_car(
            rng,
            q - dir2.perp() * 1.2,
            yaw_along(dir2) - 0.12,
            0.0,
            false,
        );
        for _ in 0..9 {
            let c = p - side * rng.range(10.0, 15.0) + dir * rng.range(-6.0, 6.0);
            let g = self.ground_height(c.x, c.y);
            let d = Vec2::new(rng.range(-1.0, 1.0), rng.range(-1.0, 1.0));
            self.shard(
                rng.next_u64(),
                Vec3::new(c.x, g, c.y),
                rng.range(0.4, 1.2),
                0.3,
                d,
                rng.range(0.6, 1.3),
                rng.chance(0.3),
                false,
            );
        }
    }

    /// The loading tipple over the track south of the bridge: a crystal bin
    /// on four legs high enough to run a train (or a tank) under, its chute
    /// over the rails, and the conveyor climbing to it out of the wash from
    /// the cutting rig.
    fn tipple(&mut self) {
        let (at, dir) = track_at(track_len(3) + 6.0);
        let g = self.ground_height(at.x, at.y);
        let rig = Rig {
            pos: Vec3::new(at.x, g, at.y),
            rot: Quat::from_rotation_y(yaw_along(dir)),
        };
        let (bin0, bin1) = (7.5, 11.0);
        for (sx, sz) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
            let foot = rig.point(Vec3::new(sx * 3.6, 0.0, sz * 5.4));
            let gf = self.ground_height(foot.x, foot.z);
            self.oboxes.push(OBox {
                center: Vec2::new(foot.x, foot.z),
                yaw: yaw_along(dir),
                half: Vec2::splat(0.4),
                y0: gf - 0.3,
                y1: g + bin0,
                color: RUST,
                side_mat: Material::Rust,
                top_mat: Material::Rust,
                ..OBox::default()
            });
        }
        rig.part(
            self,
            Vec3::new(0.0, (bin0 + bin1) * 0.5, 0.0),
            Vec3::new(4.0, (bin1 - bin0) * 0.5, 5.8),
            Quat::IDENTITY,
            GLASS * 1.1,
            Material::Vhaal,
            true,
            0.0,
        );
        rig.part(
            self,
            Vec3::new(0.0, bin0 - 0.8, 0.0),
            Vec3::new(1.4, 0.8, 1.4),
            Quat::IDENTITY,
            STEEL,
            Material::Metal,
            false,
            0.0,
        );
        rig.part(
            self,
            Vec3::new(0.0, bin1 - 0.7, 0.0),
            Vec3::new(4.05, 0.1, 5.85),
            Quat::IDENTITY,
            PLASMA,
            Material::Glow,
            false,
            0.0,
        );
        // The conveyor from beside the rig in the wash, up over the bank,
        // to the bin's side.
        let a = Vec3::new(101.0, -WASH_DEPTH + 1.4, 128.0);
        let side = rig.point(Vec3::new(-4.0, bin1 - 0.3, 0.0));
        let b = Vec3::new(side.x, side.y, side.z);
        self.segment(a, b, 0.5, STEEL, Material::Metal, false);
        if let Some(o) = self.oboxes.last_mut() {
            o.half.x = 0.9;
        }
        self.segment(
            a + Vec3::Y * 0.52,
            b + Vec3::Y * 0.52,
            0.03,
            PLASMA * 0.7,
            Material::Glow,
            false,
        );
        if let Some(o) = self.oboxes.last_mut() {
            o.half.x = 0.5;
        }
        for f in [0.2f32, 0.42, 0.64, 0.84] {
            let p = a.lerp(b, f);
            let gp = self.ground_height(p.x, p.z);
            let side = (b - a).cross(Vec3::Y).normalize();
            for s in [-1.0f32, 1.0] {
                let top = p + side * s - Vec3::Y * 0.4;
                self.segment(
                    Vec3::new(top.x, gp - 0.3, top.z),
                    top,
                    0.18,
                    RUST,
                    Material::Rust,
                    true,
                );
            }
        }
    }

    /// The headframe over the hall's shaft, on the mesa: four legs and two
    /// back braces of steel, a sheave wheel on top, the hoist house behind,
    /// the shaft collar capped with crystal.
    fn headframe(&mut self, at: Vec2) {
        let g = self.ground_height(at.x, at.y);
        let top = g + 16.0;
        let hx = 2.6;
        for (sx, sz) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
            let foot = Vec3::new(at.x + sx * hx, g - 0.2, at.y + sz * hx);
            let head = Vec3::new(at.x + sx * 0.9, top, at.y + sz * 0.9);
            self.segment(foot, head, 0.22, RUST, Material::Rust, true);
        }
        for f in [0.3f32, 0.6, 0.85] {
            let y = g + (top - g) * f;
            let r = hx + (0.9 - hx) * f;
            let c = |sx: f32, sz: f32| Vec3::new(at.x + sx * r, y, at.y + sz * r);
            for (a, b) in [
                (c(-1.0, -1.0), c(1.0, -1.0)),
                (c(1.0, -1.0), c(1.0, 1.0)),
                (c(1.0, 1.0), c(-1.0, 1.0)),
                (c(-1.0, 1.0), c(-1.0, -1.0)),
            ] {
                self.segment(a, b, 0.1, RUST, Material::Rust, false);
            }
        }
        // Back braces west, toward the hoist house.
        for sz in [-1.0f32, 1.0] {
            let foot = Vec3::new(at.x - 9.0, g - 0.2, at.y + sz * 2.0);
            self.segment(
                foot,
                Vec3::new(at.x, top - 1.0, at.y + sz * 0.9),
                0.18,
                RUST,
                Material::Rust,
                true,
            );
        }
        let sheave = Vec3::new(at.x, top + 1.4, at.y);
        self.disc(
            sheave,
            Vec3::new(0.0, 0.0, 0.15),
            1.5,
            STEEL,
            Material::Metal,
        );
        self.oboxes.push(OBox {
            center: at,
            half: Vec2::splat(1.2),
            y0: top - 0.2,
            y1: top + 0.25,
            color: STEEL,
            side_mat: Material::Metal,
            top_mat: Material::Metal,
            solid: false,
            ..OBox::default()
        });
        let house = Vec2::new(at.x - 15.0, at.y);
        self.segment(
            sheave - Vec3::Y * 1.5,
            Vec3::new(at.x, g + 0.5, at.y),
            0.04,
            STEEL * 0.6,
            Material::Rust,
            false,
        );
        self.segment(
            sheave,
            Vec3::new(house.x + 3.2, g + 3.6, house.y),
            0.04,
            STEEL * 0.6,
            Material::Rust,
            false,
        );
        self.oboxes.push(OBox {
            center: at,
            half: Vec2::splat(1.6),
            y0: g - 0.3,
            y1: g + 0.22,
            color: GLASS,
            side_mat: Material::Vhaal,
            top_mat: Material::Vhaal,
            ramp: true,
            ..OBox::default()
        });
        self.oboxes.push(OBox {
            center: house,
            half: Vec2::new(3.2, 2.6),
            y0: g - 0.3,
            y1: g + 4.2,
            color: GLASS * 1.15,
            side_mat: Material::Vhaal,
            top_mat: Material::Vhaal,
            ..OBox::default()
        });
        self.oboxes.push(OBox {
            center: house + Vec2::new(3.21, 0.0),
            half: Vec2::new(0.02, 1.8),
            y0: g + 3.2,
            y1: g + 3.45,
            color: PLASMA,
            side_mat: Material::Glow,
            top_mat: Material::Glow,
            solid: false,
            ..OBox::default()
        });
        self.sound(Ambient::Cable, sheave);
    }

    /// Cover on the mesa: outcrops of rock, a stack of cut blocks, ore bins,
    /// Resonant shards at the top of the scree.
    fn mesa_cover(&mut self, rng: &mut Rng) {
        // Rock ribs: low natural outcrops, a few long ones across the top.
        for (p, yaw, len) in [
            (Vec2::new(52.0, 176.0), 0.4, 5.0),
            (Vec2::new(50.0, 200.0), -0.2, 4.0),
            (Vec2::new(124.0, 170.0), 0.1, 6.0),
            (Vec2::new(138.0, 178.0), -0.5, 4.5),
            (Vec2::new(176.0, 186.0), 0.3, 5.5),
            (Vec2::new(198.0, 170.0), -0.1, 4.0),
            (Vec2::new(186.0, 204.0), 0.6, 5.0),
            (Vec2::new(163.0, 195.0), -0.3, 4.0),
            (Vec2::new(32.0, 196.0), 0.9, 4.5),
            (Vec2::new(66.0, 212.0), 0.2, 3.5),
            (Vec2::new(166.0, 178.0), -0.7, 3.5),
        ] {
            let g = self.ground_height(p.x, p.y);
            self.stone(
                p,
                yaw,
                Vec2::new(len, rng.range(1.3, 1.8)),
                g - 1.0,
                g + rng.range(1.8, 2.6),
                [STONE, STONE_DARK, STONE_PALE][rng.index(3)] * rng.range(0.9, 1.0),
                0.7,
            );
            if let Some(o) = self.oboxes.last_mut() {
                o.broken = true;
            }
            for _ in 0..2 {
                let q = p + Vec2::new(rng.range(-6.0, 6.0), rng.range(-4.0, 4.0));
                if in_rock(q) {
                    let s = rng.range(0.4, 0.8);
                    self.boulder(rng, q, s, false);
                }
            }
        }
        for p in [
            Vec2::new(40.0, 186.0),
            Vec2::new(52.0, 210.0),
            Vec2::new(204.0, 190.0),
        ] {
            let s = rng.range(1.5, 2.1);
            self.rock_pile(rng, p, s);
        }
        self.block_stack(rng, Vec2::new(98.0, 166.0), 0.3);
        // Blocks cut from the Face, waiting on the landing below it.
        self.block_stack(rng, Vec2::new(60.0, 143.0), 0.15);
        self.block_stack(rng, Vec2::new(164.0, 176.0), -0.2);
        // The Resonant's shards at the top of the scree, facing the mine.
        for (x, z, h) in [
            (172.0, 165.0, 4.0),
            (191.0, 168.5, 5.5),
            (195.0, 164.0, 3.5),
        ] {
            let g = self.ground_height(x, z);
            self.oboxes.push(OBox {
                center: Vec2::new(x, z),
                yaw: rng.range(0.0, TAU),
                half: Vec2::splat(0.45),
                y0: g - 0.5,
                y1: g + h,
                tilt: rng.range(0.1, 0.3),
                color: HOLY * rng.range(0.85, 1.1),
                side_mat: Material::Glow,
                top_mat: Material::Glow,
                ..OBox::default()
            });
        }
    }

    /// On and round the dump: ore carts tipped over its edge, boulders on
    /// top, spoil.
    fn dump_cover(&mut self, rng: &mut Rng) {
        let (c, _, h, _) = DUMP;
        for (dx, dz, yaw) in [(-6.0, 4.0, 0.4), (5.0, -3.0, 1.9), (0.0, 9.0, -0.8)] {
            let p = c + Vec2::new(dx, dz);
            self.ore_cart(rng, p, yaw);
        }
        for (dx, dz, s) in [(8.0, 7.0, 1.6), (-9.0, -6.0, 1.8), (2.0, -10.0, 1.4)] {
            self.rock_pile(rng, c + Vec2::new(dx, dz), s);
        }
        // Spoil heaped round its foot.
        for k in 0..10 {
            let a = k as f32 / 10.0 * TAU + rng.range(-0.2, 0.2);
            let p = c + Vec2::new(a.cos(), a.sin()) * rng.range(24.0, 30.0);
            if (p.x - 0.0).min(SIZE - p.x) < 24.0 {
                continue;
            }
            let s = rng.range(0.6, 1.3);
            self.boulder(rng, p, s, s > 1.0);
        }
        let _ = h;
    }

    /// The sorting house: an Ithessar shed on the flats by the line, a
    /// crystal roof on columns, high enough to drive under, sorting bins
    /// inside to hide behind.
    fn sorting_house(&mut self, rng: &mut Rng) {
        let at = Vec2::new(94.0, 70.0);
        let yaw = 0.2;
        let rig = Rig {
            pos: Vec3::new(at.x, self.ground_height(at.x, at.y), at.y),
            rot: Quat::from_rotation_y(yaw),
        };
        let (hx, hz) = (9.0f32, 6.0f32);
        let roof = 5.6;
        for (sx, sz) in [
            (-1.0f32, -1.0f32),
            (0.0, -1.0),
            (1.0, -1.0),
            (-1.0, 1.0),
            (0.0, 1.0),
            (1.0, 1.0),
        ] {
            let foot = rig.point(Vec3::new(sx * hx, 0.0, sz * hz));
            let g = self.ground_height(foot.x, foot.z);
            self.oboxes.push(OBox {
                center: Vec2::new(foot.x, foot.z),
                yaw: yaw + PI / 4.0,
                half: Vec2::splat(0.6),
                y0: g - 0.5,
                y1: rig.pos.y + roof,
                color: GLASS,
                side_mat: Material::Vhaal,
                top_mat: Material::Vhaal,
                ..OBox::default()
            });
        }
        rig.part(
            self,
            Vec3::new(0.0, roof + 0.3, 0.0),
            Vec3::new(hx + 1.2, 0.3, hz + 1.2),
            Quat::IDENTITY,
            GLASS * 1.2,
            Material::Vhaal,
            true,
            0.0,
        );
        rig.part(
            self,
            Vec3::new(0.0, roof - 0.05, 0.0),
            Vec3::new(hx + 0.6, 0.05, 0.12),
            Quat::IDENTITY,
            PLASMA * 0.8,
            Material::Glow,
            false,
            0.0,
        );
        // Bins inside: crystal tubs to sort the ore into, tank height.
        for (x, z) in [(-5.0, -1.5), (0.5, 2.0), (5.5, -2.0)] {
            rig.part(
                self,
                Vec3::new(x, 1.0, z),
                Vec3::new(1.6, 1.1, 1.0),
                Quat::IDENTITY,
                GLASS * rng.range(1.1, 1.4),
                Material::Vhaal,
                true,
                0.0,
            );
        }
        self.block_stack(rng, at + Vec2::new(-13.0, 6.0), 0.5);
    }

    /// Hoodoos among the fins: sandstone pillars, capped with harder rock.
    fn fins_and_hoodoos(&mut self, rng: &mut Rng) {
        for (x, z, h) in [
            (186.0, 38.0, 7.0),
            (208.0, 30.0, 9.0),
            (162.0, 22.0, 6.0),
            (198.0, 14.0, 8.0),
            (166.0, 66.0, 5.5),
            (116.0, 46.0, 6.5),
        ] {
            let p = Vec2::new(x, z);
            let g = self.ground_height(x, z);
            let w = rng.range(1.8, 2.4);
            let lean = Vec2::new(rng.range(-0.3, 0.3), rng.range(-0.3, 0.3));
            // Base, waist, cap: narrower in the middle.
            self.stone(
                p,
                rng.range(0.0, TAU),
                Vec2::splat(w),
                g - 1.0,
                g + h * 0.45,
                STONE * rng.range(0.9, 1.05),
                0.6,
            );
            self.stone(
                p + lean,
                rng.range(0.0, TAU),
                Vec2::splat(w * 0.7),
                g + h * 0.4,
                g + h * 0.85,
                STONE_PALE * rng.range(0.9, 1.0),
                0.5,
            );
            self.stone(
                p + lean * 1.6,
                rng.range(0.0, TAU),
                Vec2::new(w * 1.15, w * 0.95),
                g + h * 0.82,
                g + h,
                STONE_DARK * rng.range(0.9, 1.05),
                0.6,
            );
            if let Some(o) = self.oboxes.last_mut() {
                o.broken = true;
            }
        }
        // The fins' rock, standing out of the sand drifted against them.
        for &(a, b, _) in &FINS {
            let len = a.distance(b);
            let dir = (b - a) / len;
            let mut t = 0.8;
            while t < len - 0.8 {
                let piece = rng.range(2.2, 3.6).min(len - 0.8 - t);
                let p = a + dir * (t + piece * 0.5) + dir.perp() * rng.range(-0.3, 0.3);
                let g = self.ground_height(p.x, p.y);
                let rise = fins(p);
                self.stone(
                    p,
                    yaw_along(dir) + rng.range(-0.1, 0.1),
                    Vec2::new(rng.range(0.9, 1.3), piece * 0.5 + 0.2),
                    g - rise - 0.3,
                    g + rng.range(0.6, 1.8),
                    [STONE, STONE_DARK, STONE_PALE][rng.index(3)] * rng.range(0.88, 1.02),
                    0.5,
                );
                if let Some(o) = self.oboxes.last_mut() {
                    o.broken = true;
                }
                t += piece;
            }
        }
        // Blocks fallen off the fins.
        for &(a, b, _) in &FINS {
            for _ in 0..2 {
                let t = rng.range(0.2, 0.8);
                let side = (b - a).normalize().perp() * if rng.chance(0.5) { 1.0 } else { -1.0 };
                let p = a.lerp(b, t) + side * rng.range(4.5, 6.5);
                let s = rng.range(0.7, 1.3);
                self.boulder(rng, p, s, s > 1.0);
            }
        }
    }

    /// Cover across the rest of the flats and the landing under the Face:
    /// rock piles, crystal outcrops, timber cribs.
    fn flats_cover(&mut self, rng: &mut Rng) {
        for (x, z, s) in [
            (48.0, 96.0, 1.7),
            (62.0, 104.0, 1.5),
            (44.0, 142.0, 1.8),
            (110.0, 92.0, 1.6),
            (112.0, 54.0, 1.7),
            (100.0, 16.0, 1.5),
            (62.0, 14.0, 1.8),
            (30.0, 34.0, 1.6),
            (146.0, 100.0, 1.6),
            (176.0, 121.0, 1.5),
            (196.0, 86.0, 1.7),
            (212.0, 70.0, 1.4),
            (166.0, 140.0, 1.4),
        ] {
            self.rock_pile(rng, Vec2::new(x, z), s);
        }
        for (x, z, h) in [
            (50.0, 118.0, 3.0),
            (140.0, 112.0, 3.4),
            (76.0, 84.0, 2.6),
            (156.0, 86.0, 2.8),
        ] {
            self.outcrop(rng, Vec2::new(x, z), h);
        }
        // Timber cribs: stacked pit props waiting by the Ithessar's mouth,
        // and by the line.
        for (p, yaw) in [
            (Vec2::new(116.0, 90.0), -0.2),
            (Vec2::new(62.0, 131.0), 1.2),
        ] {
            self.crib(rng, p, yaw);
        }
    }

    /// A timber crib: pit props stacked crosswise, four courses high.
    fn crib(&mut self, rng: &mut Rng, p: Vec2, yaw: f32) {
        let g = self.ground_height(p.x, p.y);
        for k in 0..4 {
            let y = g + k as f32 * 0.5;
            let along = if k % 2 == 0 { yaw } else { yaw + FRAC_PI_2 };
            for s in [-1.0f32, 1.0] {
                let off = Vec2::from_angle(-along).rotate(Vec2::new(s * 1.3, 0.0));
                self.oboxes.push(OBox {
                    center: p + off,
                    yaw: along,
                    half: Vec2::new(0.3, 1.8),
                    y0: y - 0.2 - 0.01 * k as f32,
                    y1: y + 0.5,
                    color: TIMBER * rng.range(0.85, 1.05),
                    side_mat: Material::Wood,
                    top_mat: Material::Wood,
                    ..OBox::default()
                });
            }
        }
    }

    /// The top of what a tank stands on at `p`, from above: a cave roof,
    /// or the ground.
    fn level(&self, p: Vec2) -> f32 {
        if roofed(p) {
            ROOF.1
        } else {
            self.ground_height(p.x, p.y)
        }
    }

    /// Something solid to hide behind near `p`, at the ground's level.
    fn cover_near(&self, p: Vec2, r: f32) -> bool {
        let g = self.level(p);
        self.oboxes.iter().any(|o| {
            let (lo, hi) = o.y_range();
            o.solid && !o.ramp && hi > g + 1.4 && lo < g + 2.0 && o.distance(p) < r
        }) || self.crystals.iter().any(|c| c.solid && c.distance(p) < r)
            || {
                // A bank or a rise of the ground itself.
                (0..8).any(|k| {
                    let a = k as f32 * TAU / 8.0;
                    let q = p + Vec2::new(a.cos(), a.sin()) * (r * 0.5);
                    self.ground_height(q.x, q.y) > g + 2.0
                })
            }
    }

    /// Fill the gaps: wherever open ground is more than `REACH` m from
    /// anything to hide behind, put something there, in keeping with where
    /// it is. Natural rock on the mesa and in the east, the mine's gear in
    /// the west and by the line, crystal in the wash.
    fn fill_cover(&mut self, rng: &mut Rng) {
        const REACH: f32 = 10.0;
        let mut pts = Vec::new();
        let mut z = 6.0;
        while z < SIZE - 6.0 {
            let mut x = 6.0;
            while x < SIZE - 6.0 {
                pts.push(Vec2::new(x, z));
                x += 4.0;
            }
            z += 4.0;
        }
        // Visit in a random order so the fill isn't laid out in rows.
        for i in (1..pts.len()).rev() {
            let j = rng.index(i + 1);
            pts.swap(i, j);
        }
        for p in pts {
            if keep_clear(p) || self.cover_near(p, REACH) {
                continue;
            }
            let q = p + Vec2::new(rng.range(-2.0, 2.0), rng.range(-2.0, 2.0));
            if keep_clear(q) {
                continue;
            }
            let g = self.level(q);
            let roll = rng.f32();
            if roofed(q) {
                let s = rng.range(1.4, 2.1);
                self.rock_pile(rng, q, s);
            } else if g < -3.0 {
                if roll < 0.5 {
                    {
                        let v = rng.range(2.4, 3.6);
                        self.outcrop(rng, q, v);
                    }
                } else {
                    let s = rng.range(1.4, 2.0);
                    self.rock_pile(rng, q, s);
                }
            } else if g > TOP * 0.7 || q.x > 140.0 {
                let s = rng.range(1.4, 2.1);
                self.rock_pile(rng, q, s);
            } else if roll < 0.3 {
                {
                    let v = rng.range(0.0, TAU);
                    self.ore_cart(rng, q, v);
                }
                let s = rng.range(0.8, 1.2);
                self.boulder(rng, q + Vec2::new(2.6, 1.0), s, true);
            } else if roll < 0.5 {
                {
                    let v = rng.range(0.0, PI);
                    self.block_stack(rng, q, v);
                }
            } else if roll < 0.65 {
                {
                    let v = rng.range(0.0, PI);
                    self.crib(rng, q, v);
                }
            } else if roll < 0.8 {
                {
                    let v = rng.range(2.2, 3.2);
                    self.outcrop(rng, q, v);
                }
            } else {
                let s = rng.range(1.4, 2.0);
                self.rock_pile(rng, q, s);
            }
        }
    }

    /// The Ithessar's yard at the west edge: a plasma core held over it, as
    /// at every Ithessar place, and worked crystal standing round.
    fn mine_ithessar_base(&mut self, rng: &mut Rng) {
        let at = ITHESSAR_YARD;
        let g = self.ground_height(at.x, at.y);
        let mut prongs = Vec::new();
        for k in 0..3 {
            let a = k as f32 / 3.0 * TAU + 0.3;
            let p = at + Vec2::new(a.cos(), a.sin()) * 5.0;
            let lean = -Vec2::new(a.cos(), a.sin());
            let h = 8.0;
            let axis = Vec3::new(lean.x * 0.35, 1.0, lean.y * 0.35).normalize() * (h + 1.0);
            let base = Vec3::new(p.x, g - 1.0, p.y);
            self.crystals.push(Crystal {
                base,
                axis,
                radius: 0.9,
                sides: 6,
                seed: rng.next_u64() as u32,
                color: GLASS * 1.2,
                lit: true,
                solid: true,
            });
            prongs.push(base + axis);
        }
        self.cores.push(Core {
            at: Vec3::new(at.x, g + 10.0, at.y),
            radius: 1.0,
            prongs,
        });
    }

    /// The Resonant's camp at the east edge: shards of their blue crystal
    /// raised round a cleared ring, facing the mine they call holy.
    fn mine_resonant_base(&mut self, rng: &mut Rng) {
        let at = Vec2::new(192.0, 108.0);
        let g = self.ground_height(at.x, at.y);
        for k in 0..5 {
            let a = k as f32 / 5.0 * TAU + 0.6;
            let p = at + Vec2::new(a.cos(), a.sin()) * 5.5;
            self.oboxes.push(OBox {
                center: p,
                yaw: FRAC_PI_2 - a,
                half: Vec2::splat(0.45),
                y0: g - 0.5,
                y1: g + rng.range(3.0, 5.0),
                color: HOLY * rng.range(0.85, 1.1),
                side_mat: Material::Glow,
                top_mat: Material::Glow,
                tilt: rng.range(0.1, 0.3),
                ..OBox::default()
            });
        }
    }

    /// The edge: a wall of boulders all the way round, on whatever ground
    /// it meets (the flats, the mesa, the adit's floor).
    fn edge_rocks(&mut self, rng: &mut Rng) {
        let shades = [STONE, STONE_DARK, STONE_PALE];
        for (p, inward) in self.edge_points(3.2, 2.0) {
            let at = p + inward * rng.range(0.4, 1.0);
            let q = at.clamp(Vec2::splat(0.5), Vec2::splat(SIZE - 0.5));
            let g = self.ground_height(q.x, q.y);
            // Lower inside the adit, under its roof.
            let tall = if cave_distance(q) < 1.0 {
                5.5
            } else {
                rng.range(5.0, 8.0)
            };
            let size = rng.range(3.2, 4.2);
            self.oboxes.push(OBox {
                center: at,
                yaw: rng.range(0.0, TAU),
                half: Vec2::new(size, size * rng.range(0.85, 1.0)),
                y0: g - 1.5,
                y1: g + tall,
                color: shades[rng.index(shades.len())] * rng.range(0.85, 1.05),
                side_mat: Material::Rubble,
                top_mat: Material::Rubble,
                chip: 1.0,
                ..OBox::default()
            });
        }
    }

    /// The ways round for bots, on every level: over the rail bridge into
    /// the mine and along the wash's floor under it, through the mine (in at the west
    /// adit, through the hall, along the drift, out at the breach), over
    /// the mesa's top above it, and down the box cut into it.
    fn mine_routes(&mut self) {
        // On whatever's underfoot seen from above (the flats, the wash's
        // floor, the mesa's top), on the bridge's deck, in the mine (under
        // its roof).
        let g = |m: &Map, x: f32, z: f32| {
            let p = m.open_near(Vec2::new(x, z), 3.5).unwrap_or(Vec2::new(x, z));
            Vec3::new(p.x, m.ground_height(p.x, p.y), p.y)
        };
        let (a, b) = BRIDGE;
        let deck = |t: f32| {
            let p = a.lerp(b, t);
            Vec3::new(p.x, DECK_TOP, p.y)
        };
        let off_deck = |m: &Map, t: f32| {
            let p = a.lerp(b, t);
            g(m, p.x, p.y)
        };
        let mine = |x: f32, z: f32| Vec3::new(x, 0.0, z);
        let top = |x: f32, z: f32| Vec3::new(x, TOP, z);
        let m = &*self;
        let ways = vec![
            // From the west flats over the deck and in at the adit's mouth
            // (and the other way, out of the mine onto the flats).
            vec![
                g(m, 66.0, 96.0),
                off_deck(m, 1.15),
                deck(0.8),
                deck(0.2),
                mine(106.0, 160.0),
                mine(106.0, 174.0),
            ],
            // Along the wash's floor under the bridge: down the west ramp,
            // between the piers, up the east one.
            vec![
                g(m, 92.0, 106.0),
                g(m, 86.0, 127.0),
                g(m, 98.0, 125.0),
                g(m, 107.0, 129.0),
                g(m, 113.0, 131.0),
                g(m, 121.0, 132.0),
                g(m, 135.0, 126.0),
                g(m, 143.0, 130.0),
                g(m, 155.0, 149.0),
            ],
            // The wash end to end, on its floor: down the west crossing by
            // the Ithessar's yard, along, up the south-east one by the
            // Resonant's camp.
            vec![
                g(m, 37.0, 101.0),
                g(m, 34.0, 117.0),
                g(m, 44.0, 118.0),
                g(m, 70.0, 127.0),
                g(m, 98.0, 125.0),
                g(m, 113.0, 131.0),
                g(m, 121.0, 132.0),
                g(m, 140.0, 129.0),
                g(m, 160.0, 112.0),
                g(m, 174.0, 88.0),
                g(m, 183.0, 68.0),
                g(m, 190.0, 62.0),
                g(m, 199.0, 74.0),
            ],
            // Through the mine: in at the west adit, through the hall and
            // the crosscut, along the drift, out at the breach.
            vec![
                g(m, 71.0, 140.0),
                mine(71.0, 165.0),
                mine(78.0, 185.0),
                mine(97.0, 185.0),
                mine(106.0, 191.0),
                mine(130.0, 191.0),
                mine(153.0, 180.0),
                mine(153.0, 156.0),
                g(m, 160.0, 140.0),
            ],
            // Over the mesa's top, over the mine: up the west end, along,
            // down the scree.
            vec![
                g(m, 14.0, 167.0),
                top(60.0, 174.0),
                top(78.0, 184.0),
                top(106.0, 176.0),
                top(132.0, 172.0),
                top(166.0, 170.0),
                g(m, 181.0, 160.0),
                g(m, 181.0, 124.0),
            ],
            // Off the mesa's east down the box cut into the mine, and out
            // at the west adit.
            vec![
                top(176.0, 196.0),
                top(153.0, 205.0),
                g(m, 131.0, 205.0),
                mine(114.0, 205.0),
                mine(106.0, 192.0),
                mine(97.0, 185.0),
                mine(78.0, 185.0),
                mine(71.0, 162.0),
                g(m, 71.0, 140.0),
            ],
        ];
        self.routes = ways;
    }

    fn mine_spawns(&mut self) {
        // The Ithessar south of the wash's west arm, the Resonant north of
        // its east arm.
        for (side, xs, z0, rows) in [
            (0, &[11.0, 18.0, 25.0][..], 58.0, 7),
            (1, &[SIZE - 11.0, SIZE - 18.0][..], 90.0, 10),
        ] {
            for &x in xs {
                for k in 0..rows {
                    self.bases[side].push(Vec2::new(x, z0 + k as f32 * 6.0));
                }
            }
        }
        // Places bots head for: across the flats, the wash, the landing
        // under the Face, the mine and the mesa.
        let spots = SPOTS;
        // Each at the clear ground nearest where it's meant to be.
        self.open_spots = spots
            .into_iter()
            .filter_map(|(x, z)| self.open_near(Vec2::new(x, z), TANK_CLEAR))
            .collect();
        self.repair_spots = REPAIRS.to_vec();
        for side in 0..2 {
            let base = std::mem::take(&mut self.bases[side]);
            self.bases[side] = base
                .into_iter()
                .filter(|&p| self.mine_clear(p, TANK_CLEAR))
                .collect();
        }
    }

    /// The ground nearest `p` (within 10 m) with nothing solid within `r`,
    /// if there's any.
    fn open_near(&self, p: Vec2, r: f32) -> Option<Vec2> {
        (0..40).find_map(|k| {
            let a = k as f32 * 2.4;
            let q = p + Vec2::new(a.cos(), a.sin()) * (k as f32 * 0.25);
            self.mine_clear(q, r).then_some(q)
        })
    }

    /// Nothing solid within `r` of a ground point, at the ground's height.
    fn mine_clear(&self, p: Vec2, r: f32) -> bool {
        let g = self.ground_height(p.x, p.y);
        !self.oboxes.iter().any(|o| {
            let (lo, hi) = o.y_range();
            o.solid && lo < g + 3.0 && hi > g + 0.3 && o.distance(p) < r
        }) && !self.crystals.iter().any(|c| c.solid && c.distance(p) < r)
    }

    /// Beyond the edge: the desert rising into broken hills, and buttes.
    fn mine_horizon(&mut self, rng: &mut Rng) {
        let (cell, reach) = (4.0f32, 520.0f32);
        let n = ((SIZE + 2.0 * reach) / cell) as usize + 1;
        let mut t = Terrain::from_fn(Vec2::splat(-reach), cell, n, n, scenery_ground);
        t.color = SAND * 0.95;
        t.material = Material::Sand;
        self.scenery_terrain = Some(t);
        for _ in 0..40 {
            let a = rng.range(0.0, TAU);
            let r = rng.range(190.0, 460.0);
            let at = CENTER + Vec2::new(a.cos(), a.sin()) * r;
            let g = scenery_ground(at.x, at.y) - 2.0;
            let w = rng.range(10.0, 30.0);
            let h = rng.range(12.0, 40.0);
            self.oboxes.push(OBox {
                center: at,
                yaw: rng.range(0.0, TAU),
                half: Vec2::new(w, w * rng.range(0.5, 0.9)),
                y0: g,
                y1: g + h,
                color: [STONE, STONE_DARK, STONE_PALE][rng.index(3)] * rng.range(0.85, 1.0),
                side_mat: Material::Rubble,
                top_mat: Material::Rubble,
                chip: 1.0,
                solid: false,
                ..OBox::default()
            });
        }
    }
}

/// Ground the cover fill leaves open: the spawns and the camps, the track
/// and the bridge, the ramps and crossings, the wash's banks, the mine's
/// corridors, the box cut and the run out of its top, the scree's way up, the top of the Face.
fn keep_clear(p: Vec2) -> bool {
    let spawns = (p.x < 31.0 && (52.0..102.0).contains(&p.y))
        || p.distance(ITHESSAR_YARD) < 9.0
        || (p.x > 188.0 && (84.0..152.0).contains(&p.y))
        || p.distance(Vec2::new(192.0, 108.0)) < 12.0;
    let (d, along) = wash_near(p);
    let (_, half) = wash_profile(along);
    let bank = d > half - 1.5 && d < half + WASH_BANK + 2.5;
    let f = face(p.x);
    spawns
        || bank
        || track_distance(p) < 6.0
        || seg(p, BRIDGE.0, BRIDGE.1).0 < DECK_HALF + 5.0
        || RAMPS
            .iter()
            .chain(&FORDS)
            .any(|&(s, e, h)| seg(p, s, e).0 < h + 4.0)
        || cave_distance(p) < 4.0 && mesa(p) > 0.3 && !roofed(p)
        || within((BOX_CUT.0, BOX_CUT.1 + Vec2::new(14.0, 0.0)), p, 4.0)
        || (p.y > f - 8.0 && p.y < f + FACE_RISE + 5.0)
        || WAYS_UP.iter().any(|&(a, b)| seg(p, a, b).0 < 6.0)
        || SPOTS
            .iter()
            .any(|&(x, z)| p.distance(Vec2::new(x, z)) < 6.5)
        || REPAIRS.iter().any(|&q| p.distance(q) < 6.0)
        || p.min_element() < 7.0
        || p.max_element() > SIZE - 7.0
}

/// Under a cave roof (or on top of one): within the walls' reach of a
/// cave, back from the Face's lip, not in the box cut.
fn roofed(p: Vec2) -> bool {
    cave_distance(p) < CAVE_WALL + 0.5
        && mesa(Vec2::new(p.x, p.y - 1.0)) > 0.9
        && !within(BOX_CUT, p, 0.0)
        && p.y < SIZE + 6.0
}

/// Length of the track up to its point `k`.
fn track_len(k: usize) -> f32 {
    TRACK[..=k].windows(2).map(|w| w[0].distance(w[1])).sum()
}

/// Over the bridge's deck (not its end plates).
fn on_bridge(p: Vec2) -> bool {
    let (d, t) = seg(p, BRIDGE.0, BRIDGE.1);
    d < DECK_HALF && t > 0.0 && t < 1.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::NavGrid;

    /// A drop straight down from high above `p` lands on this height.
    fn land_on(w: &crate::sim::World, p: Vec2, from: f32) -> f32 {
        from - w.raycast(Vec3::new(p.x, from, p.y), -Vec3::Y, from + 20.0)
    }

    #[test]
    fn levels_stack() {
        let w = crate::sim::World::empty(Map::mine());
        // Over the bridge: the deck, and the wash floor under it.
        let p = BRIDGE.0.lerp(BRIDGE.1, 0.45);
        assert!(
            (land_on(&w, p, 30.0) - DECK_TOP).abs() < 0.1,
            "deck at {}",
            land_on(&w, p, 30.0)
        );
        assert!(
            land_on(&w, p, -1.0) < -WASH_DEPTH + 1.0,
            "wash floor at {}",
            land_on(&w, p, -1.0)
        );
        // Over the adit, the hall and the drift: the mesa's top, and the
        // mine's floor.
        for p in [
            Vec2::new(106.0, 175.0),
            Vec2::new(78.0, 184.0),
            Vec2::new(130.0, 191.0),
        ] {
            assert!(
                (land_on(&w, p, 30.0) - ROOF.1).abs() < 0.1,
                "roof over {p} at {}",
                land_on(&w, p, 30.0)
            );
            assert!(
                land_on(&w, p, ROOF.0 - 0.5).abs() < 0.6,
                "cave floor under {p} at {}",
                land_on(&w, p, ROOF.0 - 0.5)
            );
        }
        // The mesa's top elsewhere.
        let top = land_on(&w, Vec2::new(129.0, 167.0), 30.0);
        assert!(top > TOP - 1.0 && top < TOP + 2.0, "mesa top at {top}");
    }

    /// From each side's spawns, every part of the map is a nav path away,
    /// and each side is about as far as the other from what's worth taking:
    /// the bridge, the wash, the mine's mouths, the mesa's top.
    #[test]
    fn every_spawn_reaches_everything_and_the_sides_are_fair() {
        let m = Map::mine();
        assert!(
            m.open_spots.len() >= 20,
            "{} open spots",
            m.open_spots.len()
        );
        for (side, base) in m.bases.iter().enumerate() {
            assert!(base.len() >= 14, "side {side} has {} spawns", base.len());
        }
        let nav = NavGrid::build(&m);
        let goals = [
            ("the bridge's far end", BRIDGE.1 + Vec2::new(2.0, -6.0)),
            ("the wash under the bridge", Vec2::new(104.0, 131.0)),
            ("the flats", Vec2::new(120.0, 30.0)),
            ("the landing", Vec2::new(90.0, 141.0)),
            ("the east landing", Vec2::new(160.0, 140.0)),
            ("the mesa's west", Vec2::new(44.0, 190.0)),
            ("the mesa's east", Vec2::new(176.0, 196.0)),
            ("the mine", Vec2::new(80.0, 184.0)),
        ];
        let length = |path: &[Vec2], from: Vec2| {
            let mut at = from;
            let mut len = 0.0;
            for &p in path {
                len += at.distance(p);
                at = p;
            }
            len
        };
        for &p in m.open_spots.iter().chain(m.bases.iter().flatten()) {
            for (name, g) in goals {
                assert!(!nav.path(p, g).is_empty(), "no path from {p} to {name}");
            }
        }
        // Fair: from the middle of each side's spawns.
        let mid =
            |side: usize| m.bases[side].iter().copied().sum::<Vec2>() / m.bases[side].len() as f32;
        for (name, g) in goals {
            let (a, b) = (
                length(&nav.path(mid(0), g), mid(0)),
                length(&nav.path(mid(1), g), mid(1)),
            );
            eprintln!("{name}: {a:.0} m from the Ithessar, {b:.0} m from the Resonant");
        }
        let total = |side: usize| {
            goals
                .iter()
                .map(|&(_, g)| length(&nav.path(mid(side), g), mid(side)))
                .sum::<f32>()
        };
        let (a, b) = (total(0), total(1));
        assert!(
            (a - b).abs() / a.max(b) < 0.12,
            "sides are {a:.0} m and {b:.0} m from it all"
        );
    }

    /// Drive `hull` from `at` heading `yaw` at full throttle for `secs`:
    /// where it ends up, and the lowest and highest it went on the way.
    fn drive(hull: crate::sim::HullKind, at: Vec2, yaw: f32, secs: f32) -> (Vec3, f32, f32) {
        let mut w = crate::sim::World::empty(Map::mine());
        w.spawn_tank(Vec3::ONE, at, yaw, hull, crate::sim::TurretKind::Kontos);
        let cmd = crate::sim::Command {
            throttle: 1.0,
            ..Default::default()
        };
        let (mut low, mut high) = (f32::INFINITY, f32::NEG_INFINITY);
        for _ in 0..(secs / crate::game::DT) as usize {
            w.step(&[cmd]);
            low = low.min(w.tanks[0].pos.y);
            high = high.max(w.tanks[0].pos.y);
        }
        (w.tanks[0].pos, low, high)
    }

    const HULLS: [crate::sim::HullKind; 4] = [
        crate::sim::HullKind::Kataphrakt,
        crate::sim::HullKind::Sethuun,
        crate::sim::HullKind::Vigil,
        crate::sim::HullKind::Gravis,
    ];

    /// Every hull drives out of the adit, onto the bridge and across the
    /// wash on its deck.
    #[test]
    fn tanks_cross_the_bridge() {
        let (a, b) = BRIDGE;
        let dir = (b - a).normalize();
        for hull in HULLS {
            // From just inside the adit's mouth, lined up with the bridge,
            // until it's across.
            let mut w = crate::sim::World::empty(Map::mine());
            w.spawn_tank(
                Vec3::ONE,
                a - dir * 5.0,
                yaw_along(dir),
                hull,
                crate::sim::TurretKind::Kontos,
            );
            let cmd = crate::sim::Command {
                throttle: 1.0,
                ..Default::default()
            };
            let mut low = f32::INFINITY;
            for _ in 0..(16.0 / crate::game::DT) as usize {
                w.step(&[cmd]);
                let p = w.tanks[0].pos;
                low = low.min(p.y);
                if seg(Vec2::new(p.x, p.z), a, b).1 > 0.99 {
                    break;
                }
            }
            let end = w.tanks[0].pos;
            assert!(low > -1.5, "{hull:?} fell into the wash ({low})");
            assert!(
                seg(Vec2::new(end.x, end.z), a, b).1 > 0.99,
                "{hull:?} only got to {end}"
            );
        }
    }

    /// Every hull drives in at the west adit's mouth, up it into the hall.
    #[test]
    fn tanks_drive_into_the_mine() {
        for hull in HULLS {
            let (end, _, _) = drive(hull, Vec2::new(72.0, 142.0), 0.0, 6.0);
            assert!(end.z > 165.0, "{hull:?} only got to {end}");
            assert!(end.y < 2.0, "{hull:?} is up at {end}");
        }
    }

    /// Every hull climbs the box cut out of the mine onto the mesa.
    #[test]
    fn tanks_climb_the_box_cut() {
        for hull in HULLS {
            let (end, _, high) = drive(hull, Vec2::new(108.0, 205.0), FRAC_PI_2, 10.0);
            assert!(end.x > BOX_CUT.1.x - 4.0, "{hull:?} only got to {end}");
            assert!(high > TOP - 0.5, "{hull:?} only got {high} m up");
        }
    }

    /// Every hull drives up the mesa's west end, and up the scree.
    #[test]
    fn tanks_climb_onto_the_mesa() {
        for hull in HULLS {
            let (end, _, high) = drive(hull, WAYS_UP[0].0 + Vec2::new(6.0, 0.0), FRAC_PI_2, 10.0);
            assert!(
                end.x > WEST_RAMP.1,
                "{hull:?} only got to {end} up the west end"
            );
            assert!(
                high > TOP - 0.5,
                "{hull:?} only got {high} m up the west end"
            );
            let x = WAYS_UP[1].0.x;
            let (end, _, high) = drive(hull, WAYS_UP[1].0, 0.0, 10.0);
            assert!(
                end.z > face(x) + FACE_RISE,
                "{hull:?} only got to {end} up the scree"
            );
            assert!(high > TOP - 0.8, "{hull:?} only got {high} m up the scree");
        }
    }

    /// Every hull drives down each haul ramp into the wash, and back up.
    #[test]
    fn tanks_use_the_wash_ramps() {
        for hull in HULLS {
            for (s, e, _) in RAMPS {
                let dir = (e - s).normalize();
                let (end, low, _) = drive(hull, s - dir * 4.0, yaw_along(dir), 6.0);
                assert!(
                    low < -WASH_DEPTH + 1.5,
                    "{hull:?} never got down ramp {s} ({end})"
                );
                let (end, _, high) = drive(hull, e + dir * 1.0, yaw_along(-dir), 6.0);
                assert!(high > -0.5, "{hull:?} never got up ramp {s} ({end})");
            }
        }
    }

    /// Every hull drives across the wash at each crossing, down one bank
    /// and up the other, both ways.
    #[test]
    fn tanks_cross_the_wash_at_the_fords() {
        for hull in HULLS {
            for (s, e, _) in FORDS {
                for (a, b) in [(s, e), (e, s)] {
                    let dir = (b - a).normalize();
                    let (end, low, _) = drive(hull, a - dir * 4.0, yaw_along(dir), 12.0);
                    assert!(
                        low < -WASH_DEPTH + 1.5,
                        "{hull:?} never got down into the wash from {a} ({end})"
                    );
                    let past = (Vec2::new(end.x, end.z) - b).dot(dir);
                    assert!(
                        past > -2.0 && end.y > -0.8,
                        "{hull:?} crossing from {a} only got to {end}"
                    );
                }
            }
        }
    }

    /// Routes keep to the level they're on: along the deck over the wash,
    /// along the wash's floor under it, through the mine, over the mesa's
    /// top above it.
    #[test]
    fn routes_keep_to_their_level() {
        let m = Map::mine();
        let nav = NavGrid::build(&m);
        let deck = |t: f32| {
            let p = BRIDGE.0.lerp(BRIDGE.1, t);
            Vec3::new(p.x, DECK_TOP, p.y)
        };
        let floor = |x: f32, z: f32| Vec3::new(x, m.ground_height(x, z), z);
        let cases = [
            (deck(0.15), deck(0.85), DECK_TOP, 1.0),
            // Under the deck, from one side of it to the other.
            (floor(100.0, 131.0), floor(126.0, 133.0), -WASH_DEPTH, 1.0),
            // From the west adit, through the hall and along the drift.
            (
                Vec3::new(71.0, 0.0, 160.0),
                Vec3::new(140.0, 0.0, 191.0),
                0.0,
                1.0,
            ),
            // Over the hall and the adit, on the roof.
            (
                Vec3::new(70.0, TOP, 186.0),
                Vec3::new(128.0, TOP, 176.0),
                TOP,
                1.0,
            ),
        ];
        for (a, b, y, slack) in cases {
            let r = nav.route(a, b);
            assert!(!r.is_empty(), "no route from {a} to {b}");
            let end = *r.last().unwrap();
            assert!(end.distance(b) < 3.0, "route from {a} to {b} ends at {end}");
            assert!(
                r.iter().all(|p| (p.y - y).abs() < slack),
                "route from {a} to {b} leaves its level: {r:?}"
            );
        }
    }

    /// Every way round the map the bots take is open ground on its level.
    #[test]
    fn the_ways_round_are_open() {
        let m = Map::mine();
        let nav = NavGrid::build(&m);
        assert!(m.routes.len() >= 6);
        for p in m.routes.iter().flatten() {
            let f = nav.floor_at(*p);
            assert!(f.is_some_and(|f| (f - p.y).abs() < 1.0), "{p} is on {f:?}");
        }
    }

    /// Bot `Hymn` (it drives right up to what it's after) from `at` after
    /// a parked tank at `goal`, for up to `secs`: how near it got on the
    /// target's level.
    fn chase(at: Vec2, yaw: f32, goal: Vec2, secs: f32) -> f32 {
        use crate::ai::{Bot, Contact};
        let m = Map::mine();
        let nav = NavGrid::build(&m);
        let mut w = crate::sim::World::empty(m);
        let hull = crate::sim::HullKind::Kataphrakt;
        w.spawn_tank(Vec3::ONE, at, yaw, hull, crate::sim::TurretKind::Hymn);
        w.spawn_tank(Vec3::ONE, goal, 0.0, hull, crate::sim::TurretKind::Kontos);
        let mut bot = Bot::new(5);
        let contacts = [0, 1].map(|team| Contact {
            targetable: true,
            team,
            health: 1.0,
        });
        let mut nearest = f32::INFINITY;
        for _ in 0..(secs / crate::game::DT) as usize {
            let (cmd, _) = bot.think(0, &w, &contacts, None, &[], &nav, crate::game::DT);
            w.step(&[cmd, crate::sim::Command::default()]);
            let (p, q) = (w.tanks[0].pos, w.tanks[1].pos);
            if (p.y - q.y).abs() < 2.0 {
                nearest = nearest.min(p.distance(q));
            }
        }
        nearest
    }

    /// After a tank on the wash's floor under the bridge, a bot up on the
    /// flats by the bridge's end doesn't drive out over it on the deck: it
    /// goes down a haul ramp and in under.
    #[test]
    fn a_bot_gets_under_the_bridge() {
        let d = chase(Vec2::new(118.0, 98.0), 0.0, Vec2::new(112.0, 133.0), 40.0);
        assert!(d < 12.0, "got no nearer than {d:.1} m");
    }

    /// After a tank in the pillared hall, a bot on the mesa's top over it
    /// finds the way down and in.
    #[test]
    fn a_bot_gets_into_the_mine_under_it() {
        let d = chase(
            Vec2::new(78.0, 186.0),
            FRAC_PI_2,
            Vec2::new(78.0, 176.0),
            60.0,
        );
        assert!(d < 12.0, "got no nearer than {d:.1} m");
    }

    #[test]
    fn the_edge_is_rock() {
        // Inside a cave's wall the rock is solid up to the roof: no tank
        // gets in there to reach the edge.
        Map::mine().assert_edge_walled(0.5, |p| {
            let d = cave_distance(p);
            d > 0.0 && d < CAVE_WALL
        });
    }

    #[test]
    fn repair_spots_are_open_to_the_sky() {
        let m = Map::mine();
        let w = crate::sim::World::empty(m.clone());
        let nav = NavGrid::build(&m);
        for &p in m.repair_spots.iter().chain(m.hold_site.iter()) {
            assert!(!nav.is_blocked(p), "{p} is blocked");
            let g = m.ground_height(p.x, p.y);
            assert!((land_on(&w, p, 60.0) - g).abs() < 0.3, "something over {p}");
        }
    }

    /// Cover at tank scale everywhere: from any open ground (bar what's
    /// kept clear on purpose: spawns, track, ramps, ways up) there's
    /// something to hide behind within a short dash.
    #[test]
    fn cover_everywhere() {
        let m = Map::mine();
        let nav = NavGrid::build(&m);
        let mut bare = Vec::new();
        for iz in 0..44 {
            for ix in 0..44 {
                let p = Vec2::new(ix as f32 * 5.0 + 2.5, iz as f32 * 5.0 + 2.5);
                if keep_clear(p) || nav.is_blocked(p) {
                    continue;
                }
                if !m.cover_near(p, 13.0) {
                    bare.push(p);
                }
            }
        }
        assert!(bare.is_empty(), "no cover near {bare:?}");
    }

    /// Not mirrored: the halves are built differently. Far fewer solid
    /// pieces have a twin across the middle than not.
    #[test]
    fn the_halves_differ() {
        let m = Map::mine();
        let solid: Vec<&OBox> = m
            .oboxes
            .iter()
            .filter(|o| {
                o.solid && o.center.min_element() > 6.0 && o.center.max_element() < SIZE - 6.0
            })
            .collect();
        let twins = solid
            .iter()
            .filter(|o| {
                let p = Vec2::new(SIZE - o.center.x, o.center.y);
                solid.iter().any(|q| q.center.distance(p) < 1.5)
            })
            .count();
        assert!(
            twins * 5 < solid.len(),
            "{twins} of {} pieces are mirrored",
            solid.len()
        );
    }
}

#[cfg(test)]
mod play {
    use super::*;
    use crate::combat::Match;
    use crate::sim::Command;

    /// Two minutes of a 6v6 round in the mine: both sides score. Slow (run
    /// with `--ignored --release`).
    #[test]
    #[ignore]
    fn bots_fight_through_the_mine() {
        let mut m = Match::new(Map::mine(), 0, 11, 7);
        let steps = (120.0 / crate::game::DT) as usize;
        for _ in 0..steps {
            m.step(Command::default(), false);
        }
        let (a, b) = (m.score(0), m.score(1));
        eprintln!("score {a} : {b}");
        assert!(a > 0 && b > 0, "score {a} : {b}");
    }

    /// The levels a tank can be on in the mine.
    const LEVELS: [&str; 6] = ["deck", "under bridge", "wash", "mine", "mesa", "flats"];

    /// Which of `LEVELS` a tank at `p` is on.
    fn level(p: Vec3) -> usize {
        let q = Vec2::new(p.x, p.z);
        let (d, t) = seg(q, BRIDGE.0, BRIDGE.1);
        if on_bridge(q) && p.y > -2.0 {
            0
        } else if d < DECK_HALF + 4.0 && t > 0.0 && t < 1.0 && p.y < -3.0 {
            1
        } else if wash(q) < -3.0 && p.y < -2.0 {
            2
        } else if cave_distance(q) == 0.0 && mesa(q) > 0.5 && p.y < 4.0 {
            3
        } else if p.y > 6.0 {
            4
        } else {
            5
        }
    }

    /// What a bots-only round did, level by level.
    struct Survey {
        /// Share of bot time on each level, and of the time bots had an
        /// enemy in sight and in reach.
        time: [f32; 6],
        fighting: [f32; 6],
        deaths: [u32; 6],
        /// Steep drops of more than 3 m nobody threw them off.
        falls: u32,
        /// Share of bot time stopped more than 5 s on end.
        stuck: f32,
        /// The most bot time any one 20 m square of the map had.
        crowd: f32,
    }

    /// `secs` of a 6v6 round of bots on the mine, watched.
    fn survey(seed: u64, secs: f32) -> Survey {
        let mut m = Match::new(Map::mine(), 0, 11, seed);
        m.bots_only(seed);
        let dt = crate::game::DT;
        let n = m.world.tanks.len();
        let (mut time, mut fighting, mut deaths) = ([0.0f32; 6], [0.0f32; 6], [0u32; 6]);
        let (mut falls, mut stuck) = (0, 0.0);
        let mut heat = vec![0.0f32; 11 * 11];
        let mut last = vec![Vec3::ZERO; n];
        let mut alive = vec![true; n];
        let mut still = vec![0.0f32; n];
        let mut launched = vec![f32::NEG_INFINITY; n];
        // Each tank's last second of positions.
        let mut trail: Vec<Vec<(f32, Vec3)>> = vec![Vec::new(); n];
        for k in 0..(secs / dt) as usize {
            m.step(Command::default(), false);
            let now = k as f32 * dt;
            for i in 0..n {
                let (t, f) = (&m.world.tanks[i], &m.fighters[i]);
                let p = t.pos;
                if alive[i] && !f.alive {
                    deaths[level(last[i])] += 1;
                }
                alive[i] = f.alive;
                if !f.alive {
                    still[i] = 0.0;
                    trail[i].clear();
                    continue;
                }
                last[i] = p;
                time[level(p)] += dt;
                let c = (p / 20.0).clamp(Vec3::ZERO, Vec3::splat(10.0));
                heat[c.z as usize * 11 + c.x as usize] += dt;
                // In a fight: an enemy in sight and in reach (looked for
                // every quarter second).
                if k % 30 == 0 {
                    let eye = p + Vec3::Y * t.gun_height();
                    let seen = (0..n).any(|j| {
                        m.fighters[j].alive
                            && m.fighters[j].team != f.team
                            && m.world.tanks[j].pos.distance(p) < 110.0
                            && crate::ai::can_see(&m.world, eye, &m.world.tanks[j])
                    });
                    if seen {
                        fighting[level(p)] += dt * 30.0;
                    }
                }
                still[i] = if t.speed().abs() < 0.5 {
                    still[i] + dt
                } else {
                    0.0
                };
                if still[i] > 5.0 {
                    stuck += dt;
                }
                // Off something: a drop of more than 3 m inside a second,
                // steeper than any slope, driving on (not a Vigil setting
                // down where it stands), that no hit or blast threw it into
                // (and not a spawn's drop in, nor a husk going down).
                if t.vel.y > 3.0 || f.since_hit < 0.5 {
                    launched[i] = now;
                }
                let h = &mut trail[i];
                h.push((now, p));
                h.retain(|&(s, _)| s > now - 1.0);
                if let Some(&(_, q)) = h.iter().max_by(|a, b| a.1.y.total_cmp(&b.1.y)) {
                    let (drop, across) = (q.y - p.y, Vec2::new(q.x - p.x, q.z - p.z).length());
                    if drop > 3.0 && drop > across * 0.8 && across > 1.0 {
                        if f.arrived > 3.0 && !f.is_husk() && launched[i] < now - 2.0 {
                            falls += 1;
                            eprintln!(
                                "t={now:.0} tank {i} fell {drop:.1} m: {q} to {p} ({:?})",
                                t.hull_kind
                            );
                        }
                        h.clear();
                    }
                }
            }
        }
        let total: f32 = time.iter().sum();
        let fights: f32 = fighting.iter().sum::<f32>().max(1e-3);
        Survey {
            time: time.map(|t| t / total),
            fighting: fighting.map(|t| t / fights),
            deaths,
            falls,
            stuck: stuck / total,
            crowd: heat.iter().fold(0.0f32, |a, &b| a.max(b)) / total,
        }
    }

    /// Bots get round the whole map and fight on every level of it: over
    /// the bridge and under it, along the wash, in the mine, on the mesa
    /// and out on the flats. They don't fall off things, sit stuck, or all crowd
    /// into one spot. Slow (`--ignored --release --nocapture`).
    #[test]
    #[ignore]
    fn bots_play_every_level() {
        let seeds = [7, 8, 9];
        let (mut time, mut fighting) = ([0.0f32; 6], [0.0f32; 6]);
        for seed in seeds {
            let s = survey(seed, 180.0);
            eprintln!("seed {seed}:");
            for (l, name) in LEVELS.iter().enumerate() {
                eprintln!(
                    "  {name:>12}: {:5.1}% of bot time, {:5.1}% of the fighting, {} deaths",
                    s.time[l] * 100.0,
                    s.fighting[l] * 100.0,
                    s.deaths[l]
                );
                time[l] += s.time[l] / seeds.len() as f32;
                fighting[l] += s.fighting[l] / seeds.len() as f32;
            }
            eprintln!(
                "  {} falls, {:.1}% stuck, {:.1}% in the busiest 20 m square",
                s.falls,
                s.stuck * 100.0,
                s.crowd * 100.0
            );
            for (l, name) in LEVELS.iter().enumerate() {
                assert!(s.time[l] > 0.0005, "seed {seed}: nobody went {name}");
            }
            assert!(s.falls <= 6, "seed {seed}: {} falls", s.falls);
            assert!(s.stuck < 0.02, "seed {seed}: stuck {:.1}%", s.stuck * 100.0);
            assert!(
                s.crowd < 0.08,
                "seed {seed}: {:.1}% in one square",
                s.crowd * 100.0
            );
        }
        // Over the three rounds, some of the fighting is on every level,
        // and a fair share of it off the flats.
        for (l, name) in LEVELS.iter().enumerate() {
            assert!(fighting[l] > 0.002, "nobody fought {name}");
        }
        assert!(
            fighting[5] < 0.9,
            "{:.0}% of the fighting on the flats",
            fighting[5] * 100.0
        );
        assert!(time[3] > 0.02 && time[4] > 0.03, "mine and mesa {time:?}");
    }
}
