//! The Seam: an Ithessar crystal mine dug into a desert escarpment, the
//! Attrition map where the Ithessar hold their mine against the Resonant.
//! The Resonant call the seam holy, and have broken into the rock from the
//! east to take it back.
//!
//! 220 m square (+x east, +z north). The Ithessar come in from the west
//! edge, the Resonant from the east. Nothing is mirrored: each side has its
//! own way up, its own way in and its own cover.
//!
//! - **The Face**, along the north: an escarpment 10 m high, broken
//!   natural rock. Its top is the mesa, open ground with the headframe on
//!   it. The Ithessar drive up its long west end; the Resonant climb the
//!   scree where the face fell in, in the east.
//! - **The Crag**, rising 40 m sheer out of the middle of the Face, over the
//!   mine and back past the north edge: the Ithessar's crystal-rich cliff,
//!   seams of crystal and plasma running through its red beds, quarried flat
//!   in benches along its foot where they're cutting into it. Nothing
//!   climbs it; it splits the mesa in two, and you drive round its foot.
//! - **The mine**, under the mesa and the Crag: the rail adit straight in
//!   from the Crag's face, the new heading beside it, the pillared hall
//!   where the seam is worked west of it (high where the Crag is over it,
//!   a wide crosscut through to the adit), the west adit out to the Face,
//!   the east drift to the Resonant's breach, and the box cut, a slot sawn
//!   up through the Crag from the adit's end to the mesa's east top. Tanks
//!   on the mesa drive over tanks in the hall's west end. Inside it's
//!   smooth rock (`vault`), the walls arching over into the roof, veins of
//!   crystal showing in them; steel sets hold up the tunnels and round
//!   pillars of rock the hall.
//! - **The borer's bay**, at the new heading's end: a high chamber where
//!   an Ithessar plasma borer stands braced on the floor, its beam held on
//!   for good into the round bore it's cutting east, the face molten where
//!   it strikes. Driving into the beam destroys you (bots keep clear).
//! - **The wash**, a dry ravine 7 m deep winding right across the map
//!   under the face, in at the west edge by the Ithessar's yard and out at
//!   the east edge below the Resonant's camp: the seam crops out on its
//!   floor, where a cutting rig works it. The rail bridge leads out of the
//!   adit's mouth across the wash, on crystal piers, so the fight on the
//!   wash floor goes on under the deck. Haul ramps lead down into it, and a
//!   crossing near each side's spawns runs down one bank and up the other.
//! - **The flats**, south of the wash: the railroad runs out across them to
//!   the south edge, the ore train stopped on it under the loading tipple.
//!   A branch leaves it south of the train and swings back west into the
//!   warehouse, a plain steel shed where the ore waits in open-top bins,
//!   doors at both ends and both sides to drive through. The Ithessar's
//!   waste dump stands in the west, sandstone fins and hoodoos in the east.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use glam::{Quat, Vec2, Vec3, Vec4};

use super::crag;
use super::landmarks::Rig;
use super::valley::{hash, noise, noise2};
use super::vault;
use super::{Ambient, Core, Crystal, Map, OBox, Terrain};
use crate::faction::Faction;
use crate::render::material::Material;
use crate::render::mesh::MeshData;
use crate::rng::Rng;
use crate::tank::ithessar::{PLASMA, PLASMA_HOT};
use crate::tank::model::{self, Skin};

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
/// The scree, where the Face fell in (x): a long slope instead of a cliff,
/// from `SCREE_RUN` m out from the foot up to the lip.
const SCREE: (f32, f32) = (166.0, 196.0);
const SCREE_RUN: f32 = 30.0;
/// The mesa's west end slopes down to the floor between these (x).
const WEST_RAMP: (f32, f32) = (8.0, 48.0);
/// The Crag: sheer rock rising from the middle of the Face to this height,
/// over the mine, back past the north edge. Nothing climbs it.
const CRAG_TOP: f32 = 40.0;
/// Its foot is quarried flat this high (over the floor) along its face.
const CRAG_CUT: f32 = 12.0;

/// The mine, under the mesa: (min, max) on the ground plane, every edge on
/// the terrain's 2 m grid so the cut is clean.
const ADIT: (Vec2, Vec2) = (Vec2::new(100.0, 140.0), Vec2::new(112.0, 212.0));
const HALL: (Vec2, Vec2) = (Vec2::new(62.0, 170.0), Vec2::new(94.0, 198.0));
const CAVES: [(Vec2, Vec2); 9] = [
    ADIT,
    HALL,
    CROSSCUT,
    // The west adit, from the hall out to the face.
    (Vec2::new(66.0, 140.0), Vec2::new(76.0, 170.0)),
    // The east drift, along to the breach.
    (Vec2::new(112.0, 186.0), Vec2::new(148.0, 196.0)),
    // The Resonant's breach, in from the face.
    (Vec2::new(148.0, 150.0), Vec2::new(158.0, 196.0)),
    // The new heading: the Ithessar's newest tunnel into the Crag, still
    // being cut.
    HEADING,
    BAY,
    BORE,
];
/// The crosscut from the hall to the adit, wide and high where the cut
/// crystal is stacked for the train.
const CROSSCUT: (Vec2, Vec2) = (Vec2::new(94.0, 176.0), Vec2::new(100.0, 194.0));
const HEADING: (Vec2, Vec2) = (Vec2::new(120.0, 146.0), Vec2::new(132.0, 160.0));
/// The borer's bay at the heading's end: a big chamber, high under the
/// Crag, where the plasma borer stands cutting the bore on east.
const BAY: (Vec2, Vec2) = (Vec2::new(114.0, 160.0), Vec2::new(136.0, 184.0));
/// The bore: the round tunnel the borer is cutting, its face still molten.
const BORE: (Vec2, Vec2) = (Vec2::new(136.0, 168.0), Vec2::new(140.0, 176.0));
/// The borer's beam: from its emitter to the bore's face, and how thick.
const BEAM: (Vec3, Vec3, f32) = (
    Vec3::new(130.6, 2.1, 172.0),
    Vec3::new(140.0, 2.1, 172.0),
    0.55,
);
/// Roofs this high (their underside) in the high rooms: the bay, and the
/// hall and crosscut where they're under the Crag.
const BAY_ROOF: f32 = 12.0;
const HALL_ROOF: f32 = 11.0;
/// The Crag's mouths in its face: the rail adit and the new heading.
const MOUTHS: [(Vec2, Vec2); 2] = [ADIT, HEADING];
/// The box cut: an open trench climbing east from the adit's end to the
/// mesa's top.
const BOX_CUT: (Vec2, Vec2) = (Vec2::new(112.0, 200.0), Vec2::new(150.0, 210.0));
/// The hall's rock pillars, left standing to hold the roof up.
const PILLARS: [Vec2; 4] = [
    Vec2::new(71.0, 180.0),
    Vec2::new(84.5, 177.5),
    Vec2::new(72.0, 190.0),
    Vec2::new(84.0, 190.0),
];
/// Cave roofs: underside and top (flush with the mesa).
const ROOF: (f32, f32) = (7.4, TOP + 0.05);
/// A roof's top under the Crag: just under its top (the lid over the
/// tunnels is drawn at the top itself, see `crag_lid`).
const CRAG_ROOF: f32 = CRAG_TOP - 0.3;
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

/// The warehouse's branch line: it leaves the main line this far past the
/// track's fifth point (south of the stopped train), back the way the
/// trains came, swings west on a curve of this radius through this angle,
/// and runs straight in at the warehouse's rail door this far on.
const JUNCTION: f32 = 14.0;
const BRANCH_R: f32 = 14.0;
const BRANCH_TURN: f32 = 85.0 * PI / 180.0;
const SHED_LEAD: f32 = 4.0;
/// The warehouse: half its width (across the line) and length (along it),
/// its eaves and ridge.
const SHED_HALF: Vec2 = Vec2::new(13.0, 18.0);
const SHED_EAVES: f32 = 8.0;
const SHED_RIDGE: f32 = 11.3;
/// Where the branch ends, inside, at its buffer stop (along from the middle).
const SHED_BUFFER: f32 = 13.0;

/// The branch's line, from the junction to the buffer stop.
static BRANCH: std::sync::LazyLock<Vec<Vec2>> = std::sync::LazyLock::new(|| {
    let (j, dir) = track_at(track_len(4) + JUNCTION);
    let back = -dir;
    let centre = j + back.perp() * BRANCH_R;
    let n = 12;
    let mut line: Vec<Vec2> = (0..=n)
        .map(|k| centre + Vec2::from_angle(BRANCH_TURN * k as f32 / n as f32).rotate(j - centre))
        .collect();
    let (at, along) = shed();
    line.push(at - along * SHED_HALF.y);
    line.push(at + along * SHED_BUFFER);
    line
});

/// The warehouse's middle, and its way along (from the rail door in).
fn shed() -> (Vec2, Vec2) {
    let (j, dir) = track_at(track_len(4) + JUNCTION);
    let back = -dir;
    let centre = j + back.perp() * BRANCH_R;
    let turn = Vec2::from_angle(BRANCH_TURN);
    let end = centre + turn.rotate(j - centre);
    let along = turn.rotate(back);
    (end + along * (SHED_LEAD + SHED_HALF.y), along)
}

/// A point in the warehouse's own frame: (across, along) from its middle,
/// +across to the left of the way in.
fn shed_local(p: Vec2) -> Vec2 {
    let (at, along) = shed();
    let d = p - at;
    Vec2::new(d.dot(Vec2::new(along.y, -along.x)), d.dot(along))
}

/// How far outside the warehouse's walls (0 inside).
fn shed_distance(p: Vec2) -> f32 {
    let q = shed_local(p).abs() - SHED_HALF;
    q.max(Vec2::ZERO).length()
}

/// Places bots head for: across the flats, the wash, the landing under the
/// Face, the mine and the mesa.
const SPOTS: [(f32, f32); 29] = [
    (40.0, 70.0),
    (70.0, 30.0),
    (56.0, 52.0),
    (100.0, 44.0),
    (120.0, 70.0),
    // In the warehouse.
    (102.6, 69.0),
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
    (52.0, 214.0),
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
    (Vec2::new(181.0, 118.0), Vec2::new(181.0, 174.0)),
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
/// Marks the stand-ins for the blocks that used to stand along the Face.
const FACE_BLOCK: Vec3 = Vec3::new(0.123, 0.456, 0.789);
const STONE_PALE: Vec3 = Vec3::new(0.7, 0.5, 0.34);
const SAND: Vec3 = Vec3::new(0.66, 0.46, 0.3);
/// Mine waste: pale crushed rock.
const TAILINGS: Vec3 = Vec3::new(0.47, 0.41, 0.35);
/// The mesa's top: weathered rock, and the cave roofs laid flush with it.
const MESA_TOP: Vec3 = Vec3::new(0.46, 0.26, 0.17);
/// Ithessar worked crystal: their piers, cars, rigs and lamps.
const GLASS: Vec3 = Vec3::new(0.03, 0.07, 0.16);
/// Raw crystal, in the rock and in the ore: the crystal everyone's here
/// for, nobody's yet. Cloudy blue-grey, dull until it's cut.
const RAW: Vec3 = Vec3::new(0.1, 0.125, 0.17);
const STEEL: Vec3 = Vec3::new(0.2, 0.21, 0.24);
const RUST: Vec3 = Vec3::new(0.36, 0.22, 0.14);
const TIMBER: Vec3 = Vec3::new(0.42, 0.32, 0.22);
const CONCRETE: Vec3 = Vec3::new(0.6, 0.57, 0.52);
/// How bright a plasma lamp's light is, and how far it reaches (m).
const LAMP_BRIGHT: f32 = 4.5;
const LAMP_REACH: f32 = 10.0;
/// Fresh-cut stone, and the scorch along a plasma cut.
const CUT: Vec3 = Vec3::new(0.66, 0.47, 0.34);
const SCORCH: Vec3 = Vec3::new(0.2, 0.12, 0.11);
/// Crystal's light: the Resonant's blue, and how cut crystal glows.
const HOLY: Vec3 = super::CRYSTAL_LIGHT;
/// How bright cut crystal stacked for the train glows, against the
/// Resonant's set crystal.
const CUT_GLOW: f32 = 0.32;

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

/// The Crag's west side (x), at `z`.
fn crag_west(z: f32) -> f32 {
    let z = z.min(SIZE);
    // Flaring west as it runs back, into the massif beyond the edge.
    82.0 - 14.0 * smoothstep(160.0, 205.0, z) + 1.6 * (z * 0.13).sin() + 1.0 * (z * 0.31).sin()
}

/// The Crag's east side (x), at `z`.
fn crag_east(z: f32) -> f32 {
    let z = z.min(SIZE);
    141.0 + 1.5 * (z * 0.11 + 1.0).sin() + 0.8 * (z * 0.27).sin()
}

/// How far into the Crag a point is (m; negative: outside). Its front
/// rises behind the mesa's own face.
fn crag_depth(p: Vec2) -> f32 {
    (p.x - crag_west(p.y))
        .min(crag_east(p.y) - p.x)
        .min(p.y - face(p.x) - FACE_RISE)
}

/// How much of the Crag (0..1) stands over a point.
fn crag(p: Vec2) -> f32 {
    smoothstep(0.0, 3.0, crag_depth(p))
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

/// How near the track, main line or branch (distance from its line).
fn track_distance(p: Vec2) -> f32 {
    main_line_distance(p).min(line_distance(&BRANCH, p))
}

/// How near the main line.
fn main_line_distance(p: Vec2) -> f32 {
    line_distance(&TRACK, p)
}

fn line_distance(line: &[Vec2], p: Vec2) -> f32 {
    line.windows(2)
        .map(|w| seg(p, w[0], w[1]).0)
        .fold(f32::INFINITY, f32::min)
}

/// Play ground: dunes on the flats, the mesa along the north with the mine
/// cut out under it, the wash across under it, the dump and the fins.
pub fn ground(x: f32, z: f32) -> f32 {
    surface(x, z, true)
}

/// The rock the mine was cut from: the ground as if there were no caves
/// under the mesa. Their roofs are drawn to it (`roof_lid`), and the ground
/// round them is shaded by it.
fn uncut(p: Vec2) -> f32 {
    surface(p.x, p.y, false)
}

fn surface(x: f32, z: f32, caves: bool) -> f32 {
    let p = Vec2::new(x, z);
    let m = mesa(p);
    let dunes = 0.45 * (0.061 * x + 0.3).sin() * (0.047 * z).cos()
        + 0.25 * (0.13 * x - 0.09 * z).sin()
        + 0.15 * (0.21 * z + 0.4 * (0.05 * x).sin()).sin();
    // Flat along the railroad, under the warehouse and on the mesa.
    let shed = smoothstep(1.0, 8.0, shed_distance(p));
    let flat = smoothstep(3.0, 9.0, track_distance(p)) * shed * (1.0 - m);
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
    // The Crag over it, its top rolling too (flat over the mine's roofs).
    let c = crag(p);
    let crag_relief = (1.0 * (0.05 * x + 1.3).sin() * (0.06 * z + 0.4).cos()
        + 0.5 * (0.17 * x + 0.11 * z).sin())
        * smoothstep(5.0, 14.0, cave_distance(p))
        * smoothstep(4.0, 10.0, rect_distance(BOX_CUT, p))
        * smoothstep(3.0, 8.0, crag_depth(p));
    let relief = relief * (1.0 - c) + crag_relief;
    let cut = if caves {
        smoothstep(0.0, 2.0, cave_distance(p))
    } else {
        1.0
    };
    let mut rock = (TOP * m + (CRAG_TOP - TOP) * c + relief) * cut;
    // The box cut, down through the rock.
    let cut = rect_distance(BOX_CUT, p);
    rock = rock
        .min(box_cut_floor(x.clamp(BOX_CUT.0.x, BOX_CUT.1.x)) + (rock * smoothstep(0.0, 2.0, cut)));
    let g = dunes * flat + rock + wash(p) + (heap(DUMP, p) + heap(HEAP, p)) * shed + fins(p);
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
    let q = Vec2::new(edge.x, z.max(edge.y));
    let base = ground(edge.x, edge.y).max(TOP * mesa(q) + (CRAG_TOP - TOP) * crag(q));
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
    // Ballast along the track (not on the rock over the adit).
    if h < ROOF.0 {
        c = c.lerp(
            Vec3::new(0.36, 0.32, 0.3),
            1.0 - smoothstep(2.4, 3.6, track_distance(p)),
        );
    }
    // The mine's floors (not the roofs over them).
    if mesa(p) > 0.5 && cave_distance(p) < 0.5 && h < ROOF.0 {
        c = STONE_DARK * 0.8 * mottle;
    }
    c
}

/// Position and heading `s` m along the track.
fn track_at(s: f32) -> (Vec2, Vec2) {
    line_at(&TRACK, s)
}

/// Position and heading `s` m along a line.
fn line_at(line: &[Vec2], s: f32) -> (Vec2, Vec2) {
    let mut left = s;
    for w in line.windows(2) {
        let len = w[0].distance(w[1]);
        let dir = (w[1] - w[0]) / len;
        if left <= len {
            return (w[0] + dir * left, dir);
        }
        left -= len;
    }
    let n = line.len();
    (line[n - 1], (line[n - 1] - line[n - 2]).normalize())
}

/// Length of a line up to its point `k`.
fn line_len(line: &[Vec2], k: usize) -> f32 {
    line[..=k].windows(2).map(|w| w[0].distance(w[1])).sum()
}

/// Yaw that turns a box's local +z along `dir`.
fn yaw_along(dir: Vec2) -> f32 {
    dir.x.atan2(dir.y)
}

impl Map {
    pub fn mine() -> Map {
        static BUILT: std::sync::OnceLock<Map> = std::sync::OnceLock::new();
        Map::built_once(&BUILT, Map::build_mine)
    }

    fn build_mine() -> Map {
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
            beams: Vec::new(),
            lights: Vec::new(),
            fall_y: None,
            hold_name: "SEAM",
            routes: Vec::new(),
        };
        let mut terrain = Terrain::from_fn(Vec2::ZERO, 2.0, 111, 111, ground);
        terrain.color = Vec3::ONE;
        terrain.material = Material::Sand;
        // Round the caves the ground dips away under the roofs (drawn over
        // it, see `roof_lid` and `crag_lid`): shade it as the rock it was
        // cut from, not as a slope down into the cave, which ringed every
        // roof with a dark edge.
        terrain.normals = (0..terrain.nz)
            .flat_map(|iz| (0..terrain.nx).map(move |ix| (ix, iz)))
            .map(|(ix, iz)| {
                let p = Vec2::new(ix as f32, iz as f32) * terrain.cell;
                if in_cave(p) || cave_distance(p) > CAVE_WALL + 3.0 {
                    return terrain.normal(ix, iz);
                }
                let e = terrain.cell;
                let (dx, dz) = (Vec2::new(e, 0.0), Vec2::new(0.0, e));
                Vec3::new(
                    uncut(p - dx) - uncut(p + dx),
                    2.0 * e,
                    uncut(p - dz) - uncut(p + dz),
                )
                .normalize()
            })
            .collect();
        terrain.paint(paint);
        map.terrain = Some(terrain);
        // The ways round first (they're on the ground and the deck), so
        // what's strewn about keeps off them.
        map.mine_routes();
        let mut rng = Rng::new(0x5ea3_0213);

        map.seam(&mut rng);
        map.crag_face(&mut rng);
        map.crag_lid();
        map.rough_face(&mut rng);
        map.scree_rocks(&mut rng);
        // The mine's inside has its own dice too.
        let mut inside = Rng::new(0x1a5e_0254);
        map.mine_caves(&mut inside);
        map.pillared_hall(&mut inside);
        map.breach(&mut rng);
        map.box_cut(&mut rng);
        map.railroad();
        map.rail_bridge();
        map.wash_rocks(&mut rng);
        map.cutting_rig(Vec2::new(97.0, 131.5), PI * 0.5 + 0.35, -WASH_DEPTH);
        map.cutting_rig(Vec2::new(90.0, 174.5), -PI * 0.5 - 0.3, 0.0);
        // (Its own dice, so what it strews about doesn't move the rest.)
        map.borer_bay(&mut Rng::new(0xb0e3_0254));
        map.mine_supports();
        map.mine_gear(&mut Rng::new(0x9ea3_0254));
        map.tipple();
        map.ore_train(&mut rng);
        map.headframe(Vec2::new(60.0, 207.0));
        map.mesa_cover(&mut rng);
        map.dump_cover(&mut rng);
        map.warehouse(&mut rng);
        map.fins_and_hoodoos(&mut rng);
        map.flats_cover(&mut rng);
        map.fill_cover(&mut rng);
        map.mine_ithessar_base(&mut rng);
        map.mine_resonant_base(&mut rng);
        map.edge_rocks(&mut rng);
        map.clear_face_blocks();
        map.mesa_face();
        map.mine_spawns();
        map.mine_horizon(&mut rng);
        map.rock_lumps();
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
        // The borer: its machinery, and the beam's roar on the rock.
        found.push((Ambient::Machine, Vec3::new(BEAM.0.x - 5.0, 3.0, BEAM.0.z)));
        found.push((Ambient::Core, BEAM.1));
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

    /// A rock bigger than a tank, often with a second block broken off
    /// against it, and a piece of rubble: cover. It grows bigger only where there's room for it (off the ways
    /// round, the fords, the doors): it stays as asked for otherwise.
    fn rock_pile(&mut self, rng: &mut Rng, at: Vec2, size: f32) {
        let size = if self.room(at, size * 1.4 + 1.0) {
            size * 1.25
        } else {
            size
        };
        self.boulder(rng, at, size, true);
        if rng.chance(0.5) {
            let a = rng.range(0.0, TAU);
            let q = at + Vec2::new(a.cos(), a.sin()) * size * rng.range(1.0, 1.3);
            let s = size * rng.range(0.55, 0.75);
            if self.room(q, s * 1.2 + 1.0) {
                self.boulder(rng, q, s, true);
            }
        }
        if rng.chance(0.6) {
            let a = rng.range(0.0, TAU);
            let q = at + Vec2::new(a.cos(), a.sin()) * rng.range(size + 1.0, size + 3.0);
            let s = rng.range(0.4, 0.7);
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
            color: RAW * (0.85 + 0.3 * ((seed >> 40) % 1000) as f32 / 1000.0),
            lit,
            solid,
            raw: true,
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

    /// The seam: where the crystal crops out along the wash floor (it runs
    /// on up the Crag's face, and through the hall). Crystal breaks out
    /// along it.
    fn seam(&mut self, rng: &mut Rng) {
        let line = seam_line();
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

    /// The Crag's face, sculpted along its foot: down its west side, along
    /// the Face (quarried in benches, the natural rock overhanging the cut),
    /// up its east side and in and out of the box cut's slot. Colliders
    /// along it, lintels over its mouths, crystal breaking out of its veins.
    fn crag_face(&mut self, rng: &mut Rng) {
        let feet = crag_feet(self);
        let mut mesh = std::mem::take(&mut self.sculpted);
        let fronts = crag::sculpt(&mut mesh, &feet, &|q| self.ground_height(q.x, q.y));
        self.sculpted = mesh;
        self.face_colliders(&feet, &fronts);
        // Lintels over the mouths in the Face: rock from the face back to
        // where the tunnel's roof starts.
        for (a, b) in MOUTHS {
            let xc = (a.x + b.x) * 0.5;
            let z0 = face(xc) - 0.1;
            let mut z1 = face(xc);
            while !roofed(Vec2::new(xc, z1)) {
                z1 += 0.5;
            }
            let z1 = (z1 / 2.0).ceil() * 2.0 + 0.2;
            self.stone(
                Vec2::new(xc, (z0 + z1) * 0.5),
                0.0,
                Vec2::new((b.x - a.x) * 0.5 + 0.6, (z1 - z0) * 0.5),
                ROOF.0,
                ROOF.0 + 14.0,
                STONE_DARK,
                0.0,
            );
        }
        // The seams: crystal growing out of them all the way up.
        let n = feet.len();
        let mut placed = 0;
        for i in (0..n).step_by(2) {
            let f = feet[i];
            for (_, y, width, present) in crag::veins_at(&f) {
                if f.mouth && y < ROOF.0 + 0.5 {
                    continue;
                }
                // Not low on the Face itself: the way along its foot is
                // narrow between the wash and the rock.
                let low_front = f.nrm.y < -0.7 && f.p.y < 170.0 && y < f.floor + 4.0;
                if present > 0.5 && !low_front && rng.chance(0.18 + 0.25 * width) {
                    // Now and then a great mass of it, standing out.
                    let big = rng.chance(0.15) && y > f.floor + 5.0;
                    self.wall_crystals(rng, &f, y, big);
                    placed += 1;
                }
            }
        }
        debug_assert!(placed > 40, "{placed} crystal clusters on the Crag");
        // The benches once had plasma cuts glowing along them, gone now (the
        // rock is nobody's); their draws stay, so the rest of the map is
        // laid out as it was.
        let (x0, x1) = (crag_west(150.0) + 3.0, crag_east(150.0) - 3.0);
        let mut cuts = 0;
        for _ in 0..40 {
            if cuts >= 7 {
                break;
            }
            let start = rng.range(x0, x1 - 6.0);
            let end = start + rng.range(4.0, 9.0);
            if MOUTHS
                .iter()
                .any(|(a, b)| start < b.x + 2.0 && end > a.x - 2.0)
            {
                continue;
            }
            rng.index(3);
            cuts += 1;
        }
    }

    /// Colliders for a sculpted face: one thin slab per stretch of it,
    /// from its most forward point at tank height back into the rock.
    fn face_colliders(&mut self, feet: &[crag::Foot], fronts: &[Option<Vec2>]) {
        for k in (0..feet.len() - 1).step_by(2) {
            let j = (k + 2).min(feet.len() - 1);
            let (Some(a), Some(b)) = (fronts[k], fronts[j]) else {
                continue;
            };
            let along = b - a;
            let len = along.length();
            if len < 1e-3 || len > 4.0 {
                continue;
            }
            let nrm = (feet[k].nrm + feet[j].nrm).normalize_or(feet[k].nrm);
            let front = (a - feet[k].p).dot(nrm).max((b - feet[j].p).dot(nrm));
            let mid = (feet[k].p + feet[j].p) * 0.5 + nrm * front;
            let depth = 3.0;
            let dir = along / len;
            let floor = feet[k].floor.min(feet[j].floor);
            self.oboxes.push(OBox {
                center: mid - nrm * depth * 0.5,
                yaw: (-dir.y).atan2(dir.x),
                half: Vec2::new(len * 0.5 + 0.35, depth * 0.5),
                y0: floor - 3.0,
                y1: floor + 10.0,
                hidden: true,
                ..OBox::default()
            });
        }
    }

    /// The Face out from under the Crag, sculpted as one rock face like the
    /// Crag's: the mesa's cliff, its lip at the mesa's top, hanging down to
    /// a lintel over the west adit and the breach, petering out where the
    /// mesa slopes down at its west end and into the scree.
    fn mesa_face(&mut self) {
        let mouths = [CAVES[3], CAVES[5]];
        // The stretches: west of the Crag, between it and the scree, and
        // east of the scree.
        let spans = [
            (-6.0, crag_west(face(80.0)) + 1.0),
            (crag_east(face(141.0)) - 1.0, SCREE.0 + 4.0),
            (SCREE.1 - 4.0, SIZE + 6.0),
        ];
        let mut mesh = std::mem::take(&mut self.sculpted);
        let mut all_feet = Vec::new();
        let mut all_fronts = Vec::new();
        for (x0, x1) in spans {
            let mut corners = Vec::new();
            let mut x = x0;
            while x < x1 {
                corners.push(Vec2::new(x, face(x) + 1.2));
                x += 2.0;
            }
            corners.push(Vec2::new(x1, face(x1) + 1.2));
            let pts = crag::resample(&corners, 0.7, 1.2);
            let mut feet = Vec::with_capacity(pts.len());
            let mut along = x0 * 1.7 + 300.0;
            for (i, &(p, dir)) in pts.iter().enumerate() {
                if i > 0 {
                    along += p.distance(pts[i - 1].0);
                }
                let nrm = Vec2::new(dir.y, -dir.x);
                let full = mesa(Vec2::new(p.x, face(p.x) + FACE_RISE + 1.0)) * (1.0 - scree(p.x));
                let base = self.level(p + nrm * 2.5);
                let lip = TOP * full + 0.8 * (noise(along / 6.0, 31) - 0.5);
                let mouth = mouths.iter().any(|(a, b)| p.x > a.x && p.x < b.x);
                let by_mouth = mouths
                    .iter()
                    .map(|(a, b)| (a.x - p.x).max(p.x - b.x).max(0.0))
                    .fold(f32::INFINITY, f32::min);
                // Low where the mesa slopes away: no face at all.
                if lip - base < 0.3 && !mouth {
                    if feet.len() > 1 {
                        all_fronts.extend(crag::sculpt(&mut mesh, &feet, &|q| {
                            self.ground_height(q.x, q.y)
                        }));
                        all_feet.append(&mut feet);
                    }
                    feet.clear();
                    continue;
                }
                feet.push(crag::Foot {
                    p,
                    nrm,
                    along,
                    floor: if mouth { ROOF.0 } else { base },
                    lip,
                    cut: -100.0,
                    mouth,
                    relief: 0.8 * smoothstep(0.5, 5.0, by_mouth),
                });
            }
            if feet.len() > 1 {
                all_fronts.extend(crag::sculpt(&mut mesh, &feet, &|q| {
                    self.ground_height(q.x, q.y)
                }));
                all_feet.append(&mut feet);
            }
        }
        self.sculpted = mesh;
        self.face_colliders(&all_feet, &all_fronts);
    }

    /// The Crag's top over its tunnels: drawn as the ground there would be,
    /// over the roofs, so the top reads as one stretch of rock (the roofs
    /// stay for colliding, just under it).
    fn crag_lid(&mut self) {
        let cell = 2.0;
        let lid = |p: Vec2| crag_depth(p) > 3.5 && !within(BOX_CUT, p, 2.5);
        let m = &mut self.sculpted;
        let up = Vec3::Y.to_array();
        let mut iz = 0;
        while (iz as f32) * cell < SIZE + 6.0 {
            for ix in 0..(SIZE / cell) as usize {
                let a = Vec2::new(ix as f32 * cell, iz as f32 * cell);
                let corners = [
                    a,
                    a + Vec2::new(cell, 0.0),
                    a + Vec2::new(cell, cell),
                    a + Vec2::new(0.0, cell),
                ];
                if !corners.iter().all(|&c| lid(c))
                    || !corners.iter().any(|&c| cave_distance(c) < 4.5)
                {
                    continue;
                }
                let base = m.verts.len() as u32;
                for c in corners {
                    let color = paint(c.x, c.y, CRAG_TOP, 0.0);
                    m.verts.push(crate::render::mesh::Vertex {
                        pos: [c.x, CRAG_TOP + 0.03, c.y],
                        normal: up,
                        color: color.extend(Material::Sand.id()).to_array(),
                    });
                }
                m.indices
                    .extend([base, base + 2, base + 1, base, base + 3, base + 2]);
            }
            iz += 1;
        }
    }

    /// A cluster of crystal growing out of the Crag's face at `f`, height
    /// `y`: bigger low down, where a tank can hide behind it.
    fn wall_crystals(&mut self, rng: &mut Rng, f: &crag::Foot, y: f32, big: bool) {
        let low = y < f.floor + 3.0;
        let count = if big {
            4 + rng.index(3)
        } else {
            1 + rng.index(3)
        };
        for k in 0..count {
            let dy = if k == 0 { 0.0 } else { rng.range(-0.8, 0.8) };
            let side = f.nrm.perp() * rng.range(-1.0, 1.0) * k as f32;
            let yy = (y + dy).max(f.floor + 0.3);
            let o = crag::out(f, yy);
            let at = f.p + f.nrm * o + side;
            // Out of the rock and up, turned a little sideways.
            let rise = rng.range(0.4, 1.4);
            let dir = Vec3::new(f.nrm.x, rise, f.nrm.y) + Vec3::new(side.x, 0.0, side.y) * 0.3;
            let dir = dir.normalize();
            let (h, r) = if big {
                let s = if k == 0 { 1.0 } else { rng.range(0.45, 0.8) };
                (rng.range(4.5, 7.0) * s, rng.range(0.9, 1.4) * s)
            } else if low && k == 0 {
                (rng.range(2.4, 3.6), rng.range(0.6, 0.9))
            } else {
                (
                    rng.range(1.4, 3.6) * (1.0 - 0.3 * k as f32 / 3.0),
                    rng.range(0.28, 0.6),
                )
            };
            self.crystals.push(Crystal {
                base: Vec3::new(at.x, yy, at.y) - dir * 0.7,
                axis: dir * (h + 0.7),
                radius: r,
                sides: 5 + rng.index(3) as u32,
                seed: rng.next_u64() as u32,
                color: RAW * rng.range(0.85, 1.15),
                lit: rng.chance(0.65),
                solid: low && k == 0,
                raw: true,
            });
        }
    }

    /// The borer's bay at the new heading's end: the plasma borer, braced
    /// on the floor, its beam held on into the bore it's cutting east; the
    /// power it draws held in a core behind it; spoil, raw crystal and the
    /// gear round it.
    fn borer_bay(&mut self, rng: &mut Rng) {
        let (from, to, radius) = BEAM;
        self.beams.push(crate::map::Beam { from, to, radius });
        let dir = (to - from).normalize();
        let rig = Rig {
            pos: Vec3::new(from.x - 5.8 * dir.x, 0.0, from.z - 5.8 * dir.z),
            rot: Quat::from_rotation_y(yaw_along(Vec2::new(dir.x, dir.z))),
        };
        let axis_y = from.y;
        // Hover pads under it, glowing on the floor.
        for (x, z) in [
            (-2.1, -3.6),
            (2.1, -3.6),
            (-2.3, 0.0),
            (2.3, 0.0),
            (-2.1, 3.4),
            (2.1, 3.4),
        ] {
            rig.part(
                self,
                Vec3::new(x, 0.42, z),
                Vec3::new(0.75, 0.3, 1.0),
                Quat::IDENTITY,
                STEEL,
                Material::Metal,
                false,
                0.0,
            );
            rig.part(
                self,
                Vec3::new(x, 0.1, z),
                Vec3::new(0.6, 0.04, 0.85),
                Quat::IDENTITY,
                PLASMA,
                Material::Glow,
                false,
                0.0,
            );
        }
        // What a tank hits: one block from the floor up, nothing to climb.
        let c = rig.point(Vec3::new(0.0, 0.0, -0.6));
        self.oboxes.push(OBox {
            center: Vec2::new(c.x, c.z),
            yaw: rig.rot.to_euler(glam::EulerRot::YXZ).0,
            half: Vec2::new(2.9, 4.6),
            y0: -0.2,
            y1: 3.6,
            hidden: true,
            ..OBox::default()
        });
        // The body: worked crystal, a plasma seam round it.
        rig.part(
            self,
            Vec3::new(0.0, 1.45, -0.6),
            Vec3::new(2.5, 0.75, 4.4),
            Quat::IDENTITY,
            GLASS,
            Material::Vhaal,
            true,
            0.0,
        );
        rig.part(
            self,
            Vec3::new(0.0, 2.24, -0.6),
            Vec3::new(2.55, 0.05, 4.45),
            Quat::IDENTITY,
            PLASMA,
            Material::Glow,
            false,
            0.0,
        );
        rig.part(
            self,
            Vec3::new(0.0, 2.9, -1.4),
            Vec3::new(2.0, 0.65, 3.4),
            Quat::IDENTITY,
            GLASS * 1.25,
            Material::Vhaal,
            true,
            0.0,
        );
        // Heat fins along its back, plasma glowing between them.
        for k in 0..6 {
            let z = -4.0 + k as f32 * 1.05;
            rig.part(
                self,
                Vec3::new(0.0, 3.95, z),
                Vec3::new(1.5, 0.42, 0.12),
                Quat::IDENTITY,
                STEEL * 0.8,
                Material::Metal,
                false,
                0.0,
            );
        }
        rig.part(
            self,
            Vec3::new(0.0, 3.6, -1.4),
            Vec3::new(1.4, 0.06, 2.9),
            Quat::IDENTITY,
            PLASMA,
            Material::Glow,
            false,
            0.0,
        );
        // The operator's cab on its left, a slit of light for a window.
        rig.part(
            self,
            Vec3::new(-2.0, 3.6, 1.6),
            Vec3::new(0.8, 0.7, 1.0),
            Quat::IDENTITY,
            GLASS * 1.5,
            Material::Vhaal,
            true,
            0.0,
        );
        rig.part(
            self,
            Vec3::new(-2.0, 3.8, 2.62),
            Vec3::new(0.6, 0.12, 0.03),
            Quat::IDENTITY,
            PLASMA,
            Material::Glow,
            false,
            0.0,
        );
        // The barrel, out of the body's front to the emitter, ringed with
        // focusing crystal.
        let barrel0 = rig.point(Vec3::new(0.0, axis_y, 1.5));
        let barrel1 = rig.point(Vec3::new(0.0, axis_y, 5.0));
        self.pipe(barrel0, barrel1, 1.05, GLASS * 1.1, Material::Vhaal, true);
        for (k, z) in [2.3f32, 3.4, 4.5].into_iter().enumerate() {
            let c = rig.point(Vec3::new(0.0, axis_y, z));
            let ax = dir * 0.16;
            self.disc(c, ax, 1.55 - 0.1 * k as f32, GLASS * 1.6, Material::Vhaal);
            self.disc(c, ax * 0.5, 1.65 - 0.1 * k as f32, PLASMA, Material::Glow);
        }
        // The emitter: four crystal claws reaching in round the beam.
        for k in 0..4 {
            let a = k as f32 * FRAC_PI_2 + PI * 0.25;
            let (sa, ca) = a.sin_cos();
            let root = rig.point(Vec3::new(ca * 1.0, axis_y + sa * 1.0, 4.9));
            let knee = rig.point(Vec3::new(ca * 1.35, axis_y + sa * 1.35, 5.6));
            let tip = rig.point(Vec3::new(ca * 0.62, axis_y + sa * 0.62, 6.4));
            self.segment(root, knee, 0.2, GLASS * 1.4, Material::Vhaal, false);
            self.segment(knee, tip, 0.14, GLASS * 1.6, Material::Vhaal, false);
        }
        let mouth = rig.point(Vec3::new(0.0, axis_y, 5.25));
        self.disc(mouth, dir * 0.25, 0.75, PLASMA, Material::Glow);
        // Jacks braced down to the floor against its kick.
        for (x, z) in [(-2.4, 2.8), (2.4, 2.8), (-2.4, -4.6), (2.4, -4.6)] {
            let top = rig.point(Vec3::new(x, 1.6, z));
            let foot = rig.point(Vec3::new(x * 1.45, 0.0, z + 0.9 * z.signum()));
            self.segment(top, foot, 0.17, STEEL, Material::Metal, false);
            let f = Vec2::new(foot.x, foot.z);
            self.oboxes.push(OBox {
                center: f,
                yaw: rng.range(0.0, 0.2),
                half: Vec2::splat(0.45),
                y0: -0.1,
                y1: 0.12,
                color: STEEL,
                side_mat: Material::Metal,
                top_mat: Material::Metal,
                solid: false,
                ..OBox::default()
            });
        }
        // What it draws on: a core held between two monoliths behind it.
        let mut prongs = Vec::new();
        for x in [-1.7f32, 1.7] {
            let foot = rig.point(Vec3::new(x, 0.0, -6.2));
            self.oboxes.push(OBox {
                center: Vec2::new(foot.x, foot.z),
                yaw: rig.rot.to_euler(glam::EulerRot::YXZ).0,
                half: Vec2::new(0.45, 0.55),
                y0: -0.2,
                y1: 5.4,
                color: GLASS * 1.3,
                side_mat: Material::Vhaal,
                top_mat: Material::Vhaal,
                chip: 0.12,
                ..OBox::default()
            });
            prongs.push(rig.point(Vec3::new(x * 0.8, 5.5, -6.2)));
        }
        // The beam lights the bay: the blaze on the face, and along it.
        for (at, k) in [(to - dir * 1.2, 2.2), ((from + to) * 0.5, 1.2)] {
            self.lights.push(crate::render::PointLight {
                pos: at,
                color: PLASMA.lerp(PLASMA_HOT, 0.5) * LAMP_BRIGHT * k,
                radius: LAMP_REACH * k,
            });
        }
        self.cores.push(Core {
            at: rig.point(Vec3::new(0.0, 4.4, -6.2)),
            radius: 0.85,
            prongs,
        });
        // Conduits of their dark glass along the floor, from the core to
        // the body.
        for x in [-0.9f32, 0.9] {
            let a = rig.point(Vec3::new(x, 0.12, -5.6));
            let b = rig.point(Vec3::new(x, 0.12, -4.4));
            self.segment(a, b, 0.12, GLASS * 1.2, Material::Vhaal, false);
        }
        // Spoil from the bore, heaped either side of its mouth, clear of
        // the beam.
        let (a, b) = BAY;
        for &(x, z, size) in &[
            (b.x - 2.2, BORE.0.y - 3.0, 1.3),
            (b.x - 4.5, BORE.0.y - 4.6, 0.8),
            (b.x - 2.0, BORE.1.y + 3.2, 1.2),
            (b.x - 4.0, BORE.1.y + 5.0, 0.9),
        ] {
            self.rock_pile(rng, Vec2::new(x, z), size);
        }
        // Raw crystal standing out of the floor and walls in the bay's far
        // corners: the seam they're after, thick here.
        for (x, z, h) in [
            (a.x + 3.0, b.y - 3.0, 3.6),
            (b.x - 3.0, b.y - 2.6, 2.8),
            (a.x + 2.6, a.y + 3.2, 2.4),
        ] {
            self.outcrop(rng, Vec2::new(x, z), h);
        }
        for _ in 0..7 {
            let side = rng.index(3);
            let (p, out) = match side {
                0 => (Vec2::new(rng.range(a.x + 2.0, b.x - 2.0), b.y), Vec2::Y),
                1 => (Vec2::new(a.x, rng.range(a.y + 2.0, b.y - 2.0)), -Vec2::X),
                _ => (Vec2::new(rng.range(a.x + 2.0, b.x - 6.0), a.y), -Vec2::Y),
            };
            // Not over the way in from the heading.
            if side == 2 && p.x > HEADING.0.x - 1.0 && p.x < HEADING.1.x + 1.0 {
                continue;
            }
            // A cluster, growing out of the wall where the seam shows.
            let y = rng.range(1.2, 6.5);
            for _ in 0..4 {
                let q = p + out.perp() * rng.range(-0.8, 0.8) + out * 0.3;
                self.shard(
                    rng.next_u64(),
                    Vec3::new(q.x, y + rng.range(-0.5, 0.5) + 1.0, q.y),
                    rng.range(0.5, 1.4),
                    0.26,
                    -out + out.perp() * rng.range(-0.5, 0.5),
                    rng.range(0.6, 1.0),
                    rng.chance(0.6),
                    false,
                );
            }
        }
        // Columns of steel and crystal holding up the bay's high roof.
        for x in [a.x + 4.0, b.x - 7.0] {
            self.prop_column(Vec2::new(x, b.y - 4.5), drawn_roof(Vec2::new(x, b.y - 4.5)));
        }
        // Floodlights on stands, turned on the borer.
        for &(x, z) in &[(a.x + 2.0, a.y + 2.2), (b.x - 2.0, a.y + 2.0)] {
            self.floodlight(Vec2::new(x, z), Vec2::new(rig.pos.x, rig.pos.z));
        }
        // An ore cart waiting by the way in, and cut crystal stacked.
        self.ore_cart(rng, Vec2::new(HEADING.1.x + 1.6, a.y + 2.6), FRAC_PI_2);
        self.block_stack(rng, Vec2::new(a.x + 2.6, (a.y + b.y) * 0.5 + 6.0), 0.2);
    }

    /// Steel sets holding up the tunnels' roofs: a post up each wall and a
    /// cap across under the arch, braced at its corners, a lamp under it;
    /// every 8 m, where both walls are rock (never across a way in).
    fn mine_supports(&mut self) {
        let runs = [
            (ADIT, true, 4.0),
            (CAVES[3], true, 4.0),
            (CAVES[4], false, 6.0),
            (HEADING, true, 4.0),
        ];
        for ((a, b), along_z, first) in runs {
            let (lo, hi) = if along_z { (a.y, b.y) } else { (a.x, b.x) };
            let mut t = lo + first;
            while t < hi - 2.0 {
                let (p0, p1, out) = if along_z {
                    (Vec2::new(a.x, t), Vec2::new(b.x, t), Vec2::X)
                } else {
                    (Vec2::new(t, a.y), Vec2::new(t, b.y), Vec2::Y)
                };
                let walls = in_rock(p0 - out * 1.2)
                    && in_rock(p1 + out * 1.2)
                    && in_rock(p0 - out * 1.2 + out.perp() * 1.0)
                    && in_rock(p0 - out * 1.2 - out.perp() * 1.0)
                    && in_rock(p1 + out * 1.2 + out.perp() * 1.0)
                    && in_rock(p1 + out * 1.2 - out.perp() * 1.0);
                if walls && roofed(p0) && roofed(p1) {
                    self.steel_set(p0 + out * 0.3, p1 - out * 0.3);
                }
                t += 8.0;
            }
        }
    }

    /// One steel set, its posts at `a` and `b` (against the walls).
    fn steel_set(&mut self, a: Vec2, b: Vec2) {
        let mid = (a + b) * 0.5;
        let h = drawn_roof(mid) - arch(a).max(arch(b)) - 0.1;
        let across = (b - a).normalize();
        for p in [a, b] {
            self.pipe(
                Vec3::new(p.x, -0.2, p.y),
                Vec3::new(p.x, h, p.y),
                0.26,
                STEEL,
                Material::Metal,
                true,
            );
        }
        // The cap, into the rock at each end (over every tank: it's never
        // in the way).
        let w = a.distance(b) * 0.5 + 0.8;
        self.oboxes.push(OBox {
            center: mid,
            yaw: yaw_along(across.perp()),
            half: Vec2::new(w, 0.22),
            y0: h - 0.05,
            y1: h + 0.45,
            color: STEEL * 0.9,
            side_mat: Material::Metal,
            top_mat: Material::Metal,
            solid: false,
            ..OBox::default()
        });
        // Knee braces.
        for (p, s) in [(a, 1.0), (b, -1.0)] {
            self.segment(
                Vec3::new(p.x, h - 1.3, p.y),
                Vec3::new(p.x + across.x * s * 1.3, h, p.y + across.y * s * 1.3),
                0.1,
                STEEL,
                Material::Metal,
                false,
            );
        }
    }

    /// The mine's gear along its tunnels: the vent duct down the adit, cable
    /// trays along the drift, cut crystal stacked in the crosscut.
    fn mine_gear(&mut self, rng: &mut Rng) {
        // The duct, high up the adit's west wall, in sections.
        let x = ADIT.0.x + 0.95;
        let mut z = 158.0;
        while z < 208.0 {
            let (a, b) = (Vec3::new(x, 5.0, z), Vec3::new(x, 5.0, z + 3.9));
            self.pipe(a, b, 0.42, RUST, Material::Rust, false);
            self.pipe(
                b - Vec3::Z * 0.12,
                b + Vec3::Z * 0.1,
                0.5,
                STEEL,
                Material::Metal,
                false,
            );
            z += 4.0;
        }
        // Cable trays along the drift's north wall, plasma conduit in them.
        let (a, b) = CAVES[4];
        let zt = b.y - 0.45;
        self.oboxes.push(OBox {
            center: Vec2::new((a.x + b.x) * 0.5, zt),
            half: Vec2::new((b.x - a.x) * 0.5 - 1.0, 0.25),
            y0: 3.9,
            y1: 4.0,
            color: STEEL,
            side_mat: Material::Metal,
            top_mat: Material::Metal,
            solid: false,
            ..OBox::default()
        });
        self.oboxes.push(OBox {
            center: Vec2::new((a.x + b.x) * 0.5, zt),
            half: Vec2::new((b.x - a.x) * 0.5 - 1.0, 0.07),
            y0: 4.0,
            y1: 4.12,
            color: STEEL * 0.4,
            side_mat: Material::Rubble,
            top_mat: Material::Rubble,
            solid: false,
            ..OBox::default()
        });
        // Cut crystal in the crosscut, waiting for the train.
        let (a, b) = CROSSCUT;
        self.block_stack(rng, Vec2::new(b.x - 1.6, b.y - 1.8), 0.1);
        self.block_stack(rng, Vec2::new(a.x + 1.2, a.y + 1.6), -0.15);
    }

    /// A column of steel from the floor to the roof at `top`, its foot and
    /// head spread on plates, a crystal collar lit half way up.
    fn prop_column(&mut self, at: Vec2, top: f32) {
        let foot = Vec3::new(at.x, 0.0, at.y);
        let head = Vec3::new(at.x, top + 0.3, at.y);
        self.pipe(
            foot - Vec3::Y * 0.3,
            head,
            0.42,
            STEEL,
            Material::Metal,
            true,
        );
        for (y0, y1) in [(-0.1, 0.25), (top - 0.35, top + 0.3)] {
            self.oboxes.push(OBox {
                center: at,
                half: Vec2::splat(0.85),
                y0,
                y1,
                color: STEEL * 0.8,
                side_mat: Material::Metal,
                top_mat: Material::Metal,
                ..OBox::default()
            });
        }
        self.pipe(
            foot + Vec3::Y * 3.0,
            foot + Vec3::Y * 3.4,
            0.52,
            STEEL * 0.8,
            Material::Metal,
            false,
        );
    }

    /// A floodlight on a steel stand at `at`, its lamp turned toward `look`.
    fn floodlight(&mut self, at: Vec2, look: Vec2) {
        let base = Vec3::new(at.x, 0.0, at.y);
        let top = base + Vec3::Y * 4.6;
        for k in 0..3 {
            let a = k as f32 * TAU / 3.0;
            let leg = base + Vec3::new(a.cos(), 0.0, a.sin()) * 0.9;
            self.segment(
                leg,
                top - Vec3::Y * 1.2,
                0.07,
                STEEL,
                Material::Metal,
                false,
            );
        }
        self.pipe(
            base + Vec3::Y * 1.0,
            top,
            0.09,
            STEEL,
            Material::Metal,
            false,
        );
        let d = (look - at).normalize_or(Vec2::X);
        let rig = Rig {
            pos: top,
            rot: Quat::from_rotation_y(yaw_along(d)) * Quat::from_rotation_x(0.35),
        };
        rig.part(
            self,
            Vec3::ZERO,
            Vec3::new(0.7, 0.4, 0.3),
            Quat::IDENTITY,
            STEEL * 0.7,
            Material::Metal,
            false,
            0.0,
        );
        rig.part(
            self,
            Vec3::new(0.0, 0.0, 0.31),
            Vec3::new(0.6, 0.3, 0.02),
            Quat::IDENTITY,
            Vec3::new(1.0, 0.85, 0.95),
            Material::Glow,
            false,
            0.0,
        );
    }

    /// Broken natural rock along the rest of the Face, so it reads as stone,
    /// not sand: tall blocks against the cliff, their tops ragged.
    fn rough_face(&mut self, rng: &mut Rng) {
        let mut x = -4.0;
        while x < SIZE + 4.0 {
            let w = rng.range(2.0, 3.4);
            let c = x + w;
            x += w * 2.0 - 0.5;
            if (crag_west(face(c)) + 1.0..crag_east(face(c)) - 1.0).contains(&c) || scree(c) > 0.3 {
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
            // Only a stand-in, so what's placed round it after stays where
            // it was: the Face is sculpted (`mesa_face`), and these blocks
            // that used to stand along it go once the map is laid out
            // (`clear_face_blocks`).
            if let Some(o) = self.oboxes.last_mut() {
                o.hidden = true;
                o.color = FACE_BLOCK;
            }
            // Blocks fallen out of it.
            if rng.chance(0.45) {
                let q = p - Vec2::new(rng.range(-2.0, 2.0), rng.range(3.5, 6.0));
                if wash(q) > -0.3 {
                    let s = rng.range(1.0, 2.0);
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
            let big = placed % 3 != 2;
            let s = if big {
                rng.range(1.8, 2.8)
            } else {
                rng.range(0.7, 1.2)
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
        let cell = 2.0;
        // Roofs: the 2 m cells within the walls' reach of a cave, back from
        // the Face's lip, gathered into slabs that don't overlap.
        // Each run is (from, to, (under the Crag, underside)).
        let mut rows: Vec<(f32, Vec<RoofRun>)> = Vec::new();
        let mut z = 136.0;
        while z < SIZE + 6.0 {
            let mut runs = Vec::new();
            let mut x = 56.0;
            let mut start: Option<(f32, (bool, f32))> = None;
            while x <= 164.0 {
                let c = Vec2::new(x + cell * 0.5, z + cell * 0.5);
                let on = (x < 164.0 && roofed(c)).then(|| (under_crag(c), slab_under(c)));
                match (on, start) {
                    (Some(k), None) => start = Some((x, k)),
                    (on, Some((s, k))) if on != Some(k) => {
                        runs.push((s, x, k));
                        start = on.map(|k| (x, k));
                    }
                    _ => {}
                }
                x += cell;
            }
            rows.push((z, runs));
            z += cell;
        }
        // Merge rows with the same runs into slabs.
        let mut open: Vec<(RoofRun, f32)> = Vec::new();
        let mut slabs: Vec<(Vec2, Vec2, (bool, f32))> = Vec::new();
        for (z, runs) in rows
            .iter()
            .chain(std::iter::once(&(SIZE + 6.0, Vec::new())))
        {
            let mut next = Vec::new();
            for &(run, z0) in &open {
                if runs.contains(&run) {
                    next.push((run, z0));
                } else {
                    slabs.push((Vec2::new(run.0, z0), Vec2::new(run.1, *z), run.2));
                }
            }
            for &run in runs {
                if !next.iter().any(|&(r, _)| r == run) {
                    next.push((run, *z));
                }
            }
            open = next;
        }
        for (a, b, (tall, under)) in slabs {
            self.oboxes.push(OBox {
                center: (a + b) * 0.5,
                half: (b - a) * 0.5,
                y0: under,
                y1: if tall { CRAG_ROOF } else { ROOF.1 },
                color: MESA_TOP,
                side_mat: Material::Rubble,
                top_mat: Material::Sand,
                ramp: true,
                // Out on the mesa the roof is drawn as ground (`roof_lid`):
                // a slab there stood out as a flat rectangle with straight
                // sides, stepping out over the Face's lip. Under the Crag
                // its face and lid hide the slab.
                hidden: !tall,
                ..OBox::default()
            });
        }
        self.roof_lid();
        self.cave_lining();
        // Lamps: plasma in a crystal cage on the walls every 12 m (the
        // Resonant's breach is lit their own way), and veins of raw crystal
        // breaking out of the walls.
        for &(a, b) in CAVES[..5].iter().chain(&CAVES[6..]) {
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
                        self.lamp(p - out * 0.15, -out);
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
        // Timber portals at the Ithessar's mouths in the Face.
        for (a, b) in [CAVES[0], CAVES[3], HEADING] {
            let xc = (a.x + b.x) * 0.5;
            let mut z = face(xc);
            while !roofed(Vec2::new(xc, z)) {
                z += 0.5;
            }
            let z = z + 0.6;
            // Standing a little proud of the cave walls, not flush.
            for x in [a.x - 0.25, b.x + 0.25] {
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

    /// The cave roofs out on the mesa, drawn as ground over their hidden
    /// slabs: level with the rock round them and painted like it, their
    /// edges tucked under the terrain where they meet it, drooping with the
    /// Face's lip instead of stepping out over it. Rock below, down to the
    /// ceiling the caves see.
    fn roof_lid(&mut self) {
        let cell = 2.0;
        let origin = Vec2::new(56.0, 136.0);
        let (nx, nz) = (54i32, 45i32);
        let at = |ix: i32, iz: i32| origin + Vec2::new(ix as f32, iz as f32) * cell;
        // Over the mouths it runs on down the Face to the roof's underside,
        // so the mouth's top is the cliff, not a slab's 2 m steps.
        let lidded = |ix: i32, iz: i32| {
            (0..nx).contains(&ix) && (0..nz).contains(&iz) && {
                let a = at(ix, iz);
                let c = a + Vec2::splat(cell * 0.5);
                let over_mouth = cave_distance(c) < CAVE_WALL + 0.5
                    && !within(BOX_CUT, c, 0.0)
                    && [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)]
                        .iter()
                        .any(|&(u, v)| uncut(a + Vec2::new(u, v) * cell) > ROOF.0 + 0.2);
                (roofed(c) || over_mouth) && !under_crag(c)
            }
        };
        // The rock's top at a corner, as if no cave were under it.
        let floor = ROOF.0;
        let height = |ix: i32, iz: i32| {
            let p = at(ix, iz);
            let rock = uncut(p).clamp(floor, ROOF.1);
            let inner = lidded(ix - 1, iz - 1)
                && lidded(ix, iz - 1)
                && lidded(ix - 1, iz)
                && lidded(ix, iz);
            let g = ground(p.x, p.y);
            if !inner && g > rock - 0.3 {
                // Meeting the terrain: on its vertex (the grids line up),
                // so there's no seam.
                g
            } else {
                rock
            }
        };
        let point = |ix: i32, iz: i32| {
            let p = at(ix, iz);
            Vec3::new(p.x, height(ix, iz), p.y)
        };
        // Shaded like the ground round it (see `build_mine`).
        let normal = |ix: i32, iz: i32| {
            let p = at(ix, iz);
            let (dx, dz) = (Vec2::new(cell, 0.0), Vec2::new(0.0, cell));
            let h = |q: Vec2| uncut(q).clamp(floor, ROOF.1);
            Vec3::new(h(p - dx) - h(p + dx), 2.0 * cell, h(p - dz) - h(p + dz)).normalize()
        };
        let m = &mut self.sculpted;
        // A convex polygon, its corners in order round it, wound to face
        // along `n`.
        let mut poly = |q: &[Vec3], n: &[Vec3], c: &[Vec3], mat: Material| {
            if q.len() < 3 {
                return;
            }
            let base = m.verts.len() as u32;
            for k in 0..q.len() {
                m.verts.push(crate::render::mesh::Vertex {
                    pos: q[k].to_array(),
                    normal: n[k].to_array(),
                    color: c[k].extend(mat.id()).to_array(),
                });
            }
            let flip = (q[1] - q[0]).cross(q[2] - q[0]).dot(n[0]) < 0.0;
            for k in 1..q.len() as u32 - 1 {
                let (b, c) = if flip { (k + 1, k) } else { (k, k + 1) };
                m.indices.extend([base, base + b, base + c]);
            }
        };
        let rock = |p: Vec3, k: f32| {
            STONE.lerp(STONE_DARK, k) * (1.0 + 0.06 * (0.37 * p.x + 0.8 * (0.11 * p.z).sin()).sin())
        };
        // How far a corner's rock stands over the roof's underside: the lid
        // is cut off where that's 0, on the Face over a mouth.
        let over = |ix: i32, iz: i32| uncut(at(ix, iz)) - ROOF.0;
        let cross = |a: Vec3, b: Vec3, fa: f32, fb: f32| {
            let t = fa / (fa - fb);
            let p = a.lerp(b, t);
            Vec3::new(p.x, ROOF.0, p.z)
        };
        for iz in 0..nz {
            for ix in 0..nx {
                if !lidded(ix, iz) {
                    continue;
                }
                let ids = [(ix, iz), (ix + 1, iz), (ix + 1, iz + 1), (ix, iz + 1)];
                let q = ids.map(|(i, j)| point(i, j));
                let n = ids.map(|(i, j)| normal(i, j));
                let f = ids.map(|(i, j)| over(i, j));
                // The cell, cut back to where the rock stands over the
                // underside.
                let (mut top, mut norms) = (Vec::new(), Vec::new());
                for k in 0..4 {
                    let l = (k + 1) % 4;
                    if f[k] >= 0.0 {
                        top.push(q[k]);
                        norms.push(n[k]);
                    }
                    if (f[k] >= 0.0) != (f[l] >= 0.0) {
                        top.push(cross(q[k], q[l], f[k], f[l]));
                        norms.push(n[k].lerp(n[l], 0.5).normalize());
                    }
                }
                let colors: Vec<Vec3> = top
                    .iter()
                    .zip(&norms)
                    .map(|(p, n)| paint(p.x, p.z, p.y, Vec2::new(n.x, n.z).length() / n.y))
                    .collect();
                poly(&top, &norms, &colors, Material::Sand);
                // The cave's ceiling.
                let under: Vec<Vec3> = top.iter().map(|p| Vec3::new(p.x, ROOF.0, p.z)).collect();
                let shade: Vec<Vec3> = under.iter().map(|&p| rock(p, 0.6)).collect();
                poly(
                    &under,
                    &vec![Vec3::NEG_Y; under.len()],
                    &shade,
                    Material::Rubble,
                );
                // Rock faces where the lid ends (in the terrain, or over a
                // mouth back from the Face).
                for (k, (di, dj)) in [(0, -1), (1, 0), (0, 1), (-1, 0)].into_iter().enumerate() {
                    let l = (k + 1) % 4;
                    if lidded(ix + di, iz + dj) || (f[k] < 0.0 && f[l] < 0.0) {
                        continue;
                    }
                    let a = if f[k] >= 0.0 {
                        q[k]
                    } else {
                        cross(q[k], q[l], f[k], f[l])
                    };
                    let b = if f[l] >= 0.0 {
                        q[l]
                    } else {
                        cross(q[k], q[l], f[k], f[l])
                    };
                    if a.y.max(b.y) < ROOF.0 + 0.01 {
                        continue;
                    }
                    let side = [
                        a,
                        b,
                        Vec3::new(b.x, ROOF.0, b.z),
                        Vec3::new(a.x, ROOF.0, a.z),
                    ];
                    let out = Vec3::new(di as f32, 0.0, dj as f32);
                    let shade = side.map(|p| rock(p, 0.2));
                    poly(&side, &[out; 4], &shade, Material::Rubble);
                }
            }
        }
    }

    /// The mine's inside drawn as smooth rock over its boxes: walls that
    /// wander in and out, arching over into the roof (see `vault`).
    fn cave_lining(&mut self) {
        let wall_here = |c: Vec2, n: Vec2| {
            let back = c + n;
            cave_distance(c) < 1.0 && in_rock(back) && back.y < SIZE && !within(BOX_CUT, c, 1.0)
        };
        let cave = vault::Cave {
            sdf: &open_sdf,
            roof: &drawn_roof,
            arch: &arch,
            wall_here: &wall_here,
            roof_here: &roofed,
            shade: &rock_shade,
            veins: &rock_veins,
            vein_color: RAW,
        };
        vault::line(
            &mut self.sculpted,
            &cave,
            Vec2::new(56.0, 136.0),
            Vec2::new(166.0, SIZE + 2.0),
        );
    }

    /// A pillar of rock left standing to hold a roof up, at `at`, `r`
    /// across, up to the roof at `top`: round, flaring into the floor and
    /// the roof, drawn smooth over an eight-sided collider.
    fn rock_pillar(&mut self, at: Vec2, r: f32, top: f32) {
        // Four bars, 45 degrees apart, make the eight sides.
        for k in 0..4 {
            self.oboxes.push(OBox {
                center: at,
                yaw: k as f32 * PI * 0.25,
                half: Vec2::new(r, r * (PI / 8.0).tan()),
                y0: -0.5,
                y1: top + 0.2,
                hidden: true,
                ..OBox::default()
            });
        }
        let seed = (at.x * 7.0 + at.y * 13.0) as u32;
        let radius = |a: f32, y: f32| {
            let flare = smoothstep(1.2, -0.4, y) * 0.12 + smoothstep(top - 2.4, top + 0.2, y) * 0.8;
            r * (1.09 + flare)
                + 0.18 * noise2(a * 1.3, y / 1.7, seed)
                + 0.06 * noise2(a * 4.0, y / 0.6, seed + 1)
        };
        let point = |a: f32, y: f32| {
            let rr = radius(a, y);
            Vec3::new(at.x + a.cos() * rr, y, at.y + a.sin() * rr)
        };
        let (sides, rows) = (28usize, 16usize);
        let y_at = |k: f32| -0.5 + (top + 0.4) * k / rows as f32;
        let m = &mut self.sculpted;
        let base = m.verts.len() as u32;
        for j in 0..=rows {
            for i in 0..=sides {
                let a = i as f32 / sides as f32 * TAU;
                let y = y_at(j as f32);
                let p = point(a, y);
                let e = 0.02;
                let da = point(a + e, y) - point(a - e, y);
                let dy = point(a, y_at(j as f32 + 0.1)) - point(a, y_at(j as f32 - 0.1));
                let mut n = da.cross(dy).normalize_or_zero();
                if n.dot(Vec3::new(a.cos(), 0.0, a.sin())) < 0.0 {
                    n = -n;
                }
                let (c, mat) = rock_shade(p, n);
                m.verts.push(crate::render::mesh::Vertex {
                    pos: p.to_array(),
                    normal: n.to_array(),
                    color: c.extend(mat.id()).to_array(),
                });
            }
        }
        let row = sides as u32 + 1;
        for j in 0..rows as u32 {
            for i in 0..sides as u32 {
                let q = [
                    base + j * row + i,
                    base + j * row + i + 1,
                    base + (j + 1) * row + i + 1,
                    base + (j + 1) * row + i,
                ];
                // Wound to face out.
                m.indices.extend([q[0], q[2], q[1], q[0], q[3], q[2]]);
            }
        }
    }

    /// A plasma lamp on a cave wall at `at`, on a bracket standing out
    /// along `out`.
    fn lamp(&mut self, at: Vec2, out: Vec2) {
        self.wall_lamp(Vec3::new(at.x, 5.9, at.y), out, 1.5);
    }

    /// A cave wall from `a` to `b`, standing outward (`out`) of the cave.
    fn cave_wall(&mut self, rng: &mut Rng, a: Vec2, b: Vec2, out: Vec2) {
        let len = a.distance(b);
        let along = (b - a) / len;
        // Is there rock past each end, round the corner?
        let ext = (
            in_rock(a - along * 1.0 + out * 1.0),
            in_rock(b + along * 1.0 + out * 1.0),
        );
        // In pieces 4-6 m long, their faces flush (a step between two
        // would catch a tank scraping along the wall and throw it).
        // As high as the room's roof: a high room's, where the Crag's rock
        // is over the wall (not out under the mesa's top).
        let top = |p: Vec2| match high_roof(p - out * 1.0) {
            Some(h) if crag_depth(p + out * CAVE_WALL) > 0.5 => h + 0.3,
            _ => ROOF.0 + 0.3,
        };
        let mut t = 0.0;
        while t < len - 0.01 {
            let piece = rng.range(4.0, 6.0).min(len - t);
            let piece = if len - t - piece < 1.5 {
                len - t
            } else {
                piece
            };
            // Where the height changes, in 2 m pieces (on the roof's grid).
            let at = |d: f32| top(a + along * (t + d));
            let piece = if at(0.5) != at(piece - 0.5) {
                2.0f32.min(len - t)
            } else {
                piece
            };
            let y1 = at(0.5).max(at(piece - 0.5));
            // Each piece runs on past its ends into the rock to close the
            // corners, but not past the wall's own ends where it stops at a
            // way through (it would stand there unseen in the way).
            let back = if t < 0.01 && !ext.0 {
                0.0
            } else {
                CAVE_WALL * 0.5
            };
            let on = if t + piece > len - 0.01 && !ext.1 {
                0.0
            } else {
                CAVE_WALL * 0.5
            };
            let (lo, hi) = (t - back, t + piece + on);
            let c = a + along * ((lo + hi) * 0.5) + out * (CAVE_WALL * 0.5 - 0.02);
            let half = if along.x.abs() > 0.5 {
                Vec2::new((hi - lo) * 0.5, CAVE_WALL * 0.5)
            } else {
                Vec2::new(CAVE_WALL * 0.5, (hi - lo) * 0.5)
            };
            let shade = [STONE, STONE_DARK, STONE_PALE][rng.index(3)] * rng.range(0.85, 1.0);
            self.stone(c, 0.0, half, -0.5, y1, shade, 0.3);
            // The lining is drawn in its place (see `cave_lining`).
            if let Some(o) = self.oboxes.last_mut() {
                o.hidden = true;
            }
            t += piece;
        }
    }

    /// The hall, where the seam is worked under the rock: pillars left to
    /// hold the roof, the vein glowing across the floor between them,
    /// crystal standing where it hasn't been cut yet, a stack of cut blocks.
    fn pillared_hall(&mut self, rng: &mut Rng) {
        for &p in &PILLARS {
            let s = rng.range(2.1, 2.5);
            let top = drawn_roof(p) + 0.3;
            self.rock_pillar(p, s, top);
            // Steel straps round it, to keep it from spalling.
            let band = 2.6 + rng.range(0.0, 0.6);
            for y in [band, band + 1.1] {
                self.pipe(
                    Vec3::new(p.x, y, p.y),
                    Vec3::new(p.x, y + 0.3, p.y),
                    s * 1.12 + 0.15,
                    STEEL * 0.8,
                    Material::Metal,
                    false,
                );
            }
            // A lamp on each pillar's face toward the middle.
            let mid = (HALL.0 + HALL.1) * 0.5;
            let out = if p.x < mid.x { Vec2::X } else { -Vec2::X };
            self.lamp(p + out * (s * 1.1 + 0.2), out);
        }
        // Crystal not yet cut, in the hall's west end (clear of the way in
        // from the west adit, round the pillar to the hall's middle).
        for (x, z, h) in [(66.0, 184.0, 3.2), (65.5, 175.5, 2.4), (78.0, 195.0, 2.6)] {
            self.outcrop(rng, Vec2::new(x, z), h);
        }
        // Cut blocks stacked for the train, by the crosscut.
        self.block_stack(rng, Vec2::new(92.0, 195.0), 0.0);
    }

    /// Cut crystal blocks, stacked: a big one with smaller round it. Cut,
    /// the crystal glows, as it does at the Resonant's sanctum.
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
                color: HOLY * CUT_GLOW * rng.range(0.85, 1.1),
                side_mat: Material::Glow,
                top_mat: Material::Glow,
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
                x += w;
                // Inside the Crag the cut's sides are its sculpted face.
                if c + w * 0.5 < crag_east(z) - 0.5 {
                    continue;
                }
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
            }
        }
    }

    /// The track, from the end of the adit out across the bridge and the
    /// flats to the south edge, and the branch off it into the warehouse:
    /// two rails on sleepers. Visual only.
    fn railroad(&mut self) {
        self.rails(&TRACK, false);
        let branch = BRANCH.clone();
        self.rails(&branch, true);
        // The points lever by the junction, on the outside of the curve.
        let (j, dir) = track_at(track_len(4) + JUNCTION - 2.0);
        let at = j + dir.perp() * 2.8;
        let g = self.ground_height(at.x, at.y);
        self.oboxes.push(OBox {
            center: at,
            yaw: yaw_along(dir),
            half: Vec2::new(0.25, 0.35),
            y0: g - 0.1,
            y1: g + 0.5,
            color: STEEL,
            side_mat: Material::Metal,
            top_mat: Material::Metal,
            solid: false,
            ..OBox::default()
        });
        self.segment(
            Vec3::new(at.x, g + 0.45, at.y),
            Vec3::new(at.x + dir.x * 0.3, g + 1.3, at.y + dir.y * 0.3),
            0.04,
            RUST,
            Material::Rust,
            false,
        );
    }

    /// Rails on sleepers along `line`, following the ground (or the
    /// bridge's deck). A branch leaves out the sleepers where the main
    /// line's already are.
    fn rails(&mut self, line: &[Vec2], branch: bool) {
        let total = line_len(line, line.len() - 1);
        let y_at = |m: &Map, s: f32| {
            let (p, dir) = line_at(line, s);
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
            let (p, dir) = line_at(line, s);
            if branch && main_line_distance(p) < 2.4 {
                continue;
            }
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
        // line's bends.
        let mut s = if branch { 1.0 } else { 0.0 };
        let bends: Vec<f32> = (1..line.len()).map(|k| line_len(line, k)).collect();
        while s < total - 0.01 {
            let next_bend = bends
                .iter()
                .copied()
                .find(|&b| b > s + 0.01)
                .unwrap_or(total);
            let e = (s + 6.0).min(next_bend);
            let (pa, dir) = line_at(line, s + 0.001);
            let (pb, _) = line_at(line, e - 0.001);
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
                || self.on_way(q)
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

    /// A small ore cart at `at` heading `yaw`: a steel tub on a steel
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
            RUST * 1.1,
            Material::Painted,
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
    /// onto its side (a derailed car): an ore car, or the locomotive (`loco`),
    /// its prow toward local +z. Ithessar freight runs on rails, but it's
    /// built like the rest of their machines: knapped crystal over dark
    /// alloy running gear, each body held a hand's width off its bogies by
    /// the field, plasma showing through its seams. The hulls are drawn
    /// sculpted; a plain box round each is what tanks hit and hide behind.
    fn rail_car(&mut self, rng: &mut Rng, at: Vec2, yaw: f32, roll: f32, loco: bool) {
        // Its detail comes from its own stream; the map's advances by what
        // the old train drew from it, so everything built after keeps its
        // layout.
        skip(
            rng,
            if loco {
                1
            } else if roll.abs() < 0.3 {
                40
            } else {
                0
            },
        );
        let rng = &mut own_rng(at);
        let g = self.ground_height(at.x, at.y);
        let half_w = 1.75;
        let pivot = Vec3::new(-half_w * roll.signum(), 0.0, 0.0);
        let rot = Quat::from_rotation_y(yaw) * Quat::from_rotation_z(roll);
        let base = Vec3::new(at.x, g + 0.2, at.y);
        let rig = Rig {
            pos: base + Quat::from_rotation_y(yaw) * pivot - rot * pivot,
            rot,
        };
        let mut m = MeshData::default();
        let bogies: [f32; 2] = if loco { [-4.0, 3.0] } else { [-2.5, 2.5] };
        for z in bogies {
            bogie(&mut m, z);
        }
        // The collider: the body, from the bogies' frames to the rim.
        let (cz, hz, top) = if loco {
            (-0.3, 6.2, 3.95)
        } else {
            (0.0, 4.3, 3.8)
        };
        self.hidden_part(
            &rig,
            Vec3::new(0.0, (0.45 + top) * 0.5, cz),
            Vec3::new(half_w, (top - 0.45) * 0.5, hz),
        );
        if loco {
            locomotive(&mut m);
            self.hidden_part(&rig, Vec3::new(0.0, 4.4, -2.4), Vec3::new(1.0, 0.5, 2.2));
            // Its drive: a knot of plasma held between two crystal horns
            // behind the canopy.
            let core = rig.point(Vec3::new(0.0, 5.25, -5.4));
            let prongs = [-1.0f32, 1.0]
                .map(|s| rig.point(Vec3::new(s * 0.75, 5.35, -5.4)))
                .to_vec();
            self.cores.push(Core {
                at: core,
                radius: 0.32,
                prongs,
            });
        } else {
            ore_tub(&mut m);
        }
        self.sculpt(&rig, &m);
        if !loco {
            self.ore_load(rng, &rig, roll.abs() < 0.3);
        }
    }

    /// A collider at `local` in `rig` (half extents `half`), not drawn: the
    /// box round something drawn sculpted.
    fn hidden_part(&mut self, rig: &Rig, local: Vec3, half: Vec3) {
        rig.part(
            self,
            local,
            half,
            Quat::IDENTITY,
            GLASS,
            Material::Vhaal,
            true,
            0.0,
        );
        if let Some(o) = self.oboxes.last_mut() {
            o.hidden = true;
        }
    }

    /// Draws `m`, built in `rig`'s frame, into the sculpted surfaces.
    fn sculpt(&mut self, rig: &Rig, m: &MeshData) {
        let base = self.sculpted.verts.len() as u32;
        for v in &m.verts {
            let mut v = *v;
            v.pos = rig.point(Vec3::from(v.pos)).to_array();
            v.normal = (rig.rot * Vec3::from(v.normal)).to_array();
            self.sculpted.verts.push(v);
        }
        self.sculpted
            .indices
            .extend(m.indices.iter().map(|i| i + base));
    }

    /// An ore car's load: crystal-rich rock from the Crag heaped up over
    /// the rim (`full`), raw crystal breaking out of it. Spilled, only a
    /// few lumps are left in the tub.
    fn ore_load(&mut self, rng: &mut Rng, rig: &Rig, full: bool) {
        let lump = |m: &mut Map, rng: &mut Rng, at: Vec3, s: f32| {
            let shade = [ORE, ORE * 0.75, ORE * 1.2, MESA_TOP * 0.8][rng.index(4)];
            let turn = Quat::from_euler(
                glam::EulerRot::YXZ,
                rng.range(0.0, TAU),
                rng.range(-0.6, 0.6),
                rng.range(-0.6, 0.6),
            );
            rig.part(
                m,
                at,
                Vec3::new(
                    s * rng.range(0.8, 1.2),
                    s * rng.range(0.55, 0.85),
                    s * rng.range(0.8, 1.2),
                ),
                turn,
                shade * rng.range(0.9, 1.1),
                Material::Rubble,
                false,
                s * 0.4,
            );
        };
        if !full {
            for _ in 0..6 {
                let at = Vec3::new(
                    rng.range(-0.9, 0.9),
                    rng.range(1.7, 2.2),
                    rng.range(-2.6, 2.6),
                );
                let size = rng.range(0.3, 0.55);
                lump(self, rng, at, size);
            }
            return;
        }
        // The heap, highest down the middle.
        let heap =
            |x: f32, z: f32| 3.25 + 1.05 * (1.0 - (x / 1.8).powi(2)) * (1.0 - (z / 4.3).powi(2));
        for _ in 0..26 {
            let x = rng.range(-1.4, 1.4);
            let reach = 3.1 + 1.0 * (1.0 - x.abs() / 1.4);
            let z = rng.range(-reach, reach);
            let s = rng.range(0.32, 0.7);
            lump(self, rng, Vec3::new(x, heap(x, z) - s * 0.35, z), s);
        }
        // Lumps riding up on the rim, hanging over the side.
        for _ in 0..4 {
            let side = if rng.chance(0.5) { 1.0 } else { -1.0 };
            let at = Vec3::new(side * rng.range(1.4, 1.6), 3.55, rng.range(-2.6, 2.6));
            let size = rng.range(0.25, 0.42);
            lump(self, rng, at, size);
        }
        // Raw crystal breaking out of the rock, and lumps of it in the heap.
        for _ in 0..5 {
            let x = rng.range(-1.1, 1.1);
            let z = rng.range(-2.8, 2.8);
            let s = rng.range(0.25, 0.4);
            rig.part(
                self,
                Vec3::new(x, heap(x, z), z),
                Vec3::new(s, s * 0.7, s * 1.3),
                Quat::from_euler(glam::EulerRot::YXZ, rng.range(0.0, TAU), 0.4, 0.3),
                RAW * 1.15,
                Material::Ore,
                false,
                0.0,
            );
        }
        for k in 0..8 {
            let x = rng.range(-1.1, 1.1);
            let z = rng.range(-3.0, 3.0);
            let p = rig.point(Vec3::new(x, heap(x, z), z));
            let out = rig.rot * Vec3::new(x, 0.0, z * 0.3);
            let dir =
                Vec2::new(out.x, out.z) + Vec2::new(rng.range(-0.4, 0.4), rng.range(-0.4, 0.4));
            let big = k < 3;
            self.shard(
                rng.next_u64(),
                p,
                if big {
                    rng.range(1.5, 2.2)
                } else {
                    rng.range(0.7, 1.1)
                },
                if big { 0.4 } else { 0.22 },
                dir,
                rng.range(0.2, 0.6),
                big || rng.chance(0.35),
                false,
            );
        }
    }

    /// The ore train, stopped on the flats headed out: the locomotive, two
    /// ore cars behind it under the tipple. Far down the line, two cars off
    /// the rails, one on its side, its load spilled.
    fn ore_train(&mut self, rng: &mut Rng) {
        let s0 = track_len(3) + 6.0;
        // Ore cars under and past the tipple, then the loco ahead (south).
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
        // The spill, from its own stream (see `rail_car`).
        skip(rng, 72);
        let rng = &mut own_rng(p);
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
        // The rock it was carrying, strewn out of its mouth.
        for _ in 0..16 {
            let c = p - side * rng.range(8.5, 14.0) + dir * rng.range(-5.0, 5.0);
            let g = self.ground_height(c.x, c.y);
            let r = rng.range(0.3, 0.65);
            let shade = [ORE, ORE * 0.75, ORE * 1.2][rng.index(3)];
            self.oboxes.push(OBox {
                center: c,
                yaw: rng.range(0.0, TAU),
                tilt: rng.range(-0.4, 0.4),
                half: Vec2::new(r * rng.range(0.8, 1.2), r * rng.range(0.8, 1.2)),
                y0: g - r * 0.4,
                y1: g + r * rng.range(0.6, 1.0),
                color: shade,
                side_mat: Material::Rubble,
                top_mat: Material::Rubble,
                chip: r * 0.4,
                solid: false,
                ..OBox::default()
            });
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
            color: RUST,
            side_mat: Material::Painted,
            top_mat: Material::Rust,
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
            if crag_depth(p) > -6.0 {
                continue;
            }
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
            let q = p + Vec2::new(rng.range(-6.0, 6.0), rng.range(-4.0, 4.0));
            if in_rock(q) {
                let s = rng.range(1.2, 1.8);
                self.boulder(rng, q, s, true);
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
        // Timber waiting on the landing below the Face.
        self.crib(rng, Vec2::new(60.0, 143.0), 0.15);
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
            let s = rng.range(1.0, 1.9);
            self.boulder(rng, p, s, s > 1.0);
        }
        let _ = h;
    }

    /// The warehouse at the end of the branch: a plain steel shed on a
    /// concrete floor, pitched roof on portal frames, a rail door at the
    /// junction's end, a door at the far end and one in each side, all big
    /// enough to drive through. Open-top bins of crystal-rich ore stand
    /// along both walls, a hopper at the buffer stop. Only the plasma lamps,
    /// and the emblem small on the front, say whose it is.
    fn warehouse(&mut self, rng: &mut Rng) {
        let (at, along) = shed();
        let g = self.ground_height(at.x, at.y);
        let yaw = yaw_along(along);
        let rig = Rig {
            pos: Vec3::new(at.x, g, at.y),
            rot: Quat::from_rotation_y(yaw),
        };
        let id = Quat::IDENTITY;
        let (hw, hl) = (SHED_HALF.x, SHED_HALF.y);
        let (eaves, ridge) = (SHED_EAVES, SHED_RIDGE);
        // Wall half-thickness; the doors' half-widths and height.
        let t = 0.2;
        let (end_door, side_door, door_h) = (5.5, 3.5, 6.0);
        let clad = Vec3::new(0.58, 0.56, 0.5);
        let dado = Vec3::new(0.42, 0.36, 0.3);
        let frame = Vec3::new(0.3, 0.31, 0.33);
        // The floor.
        rig.part(
            self,
            Vec3::new(0.0, -0.14, 0.0),
            Vec3::new(hw + 0.9, 0.18, hl + 0.9),
            id,
            CONCRETE,
            Material::Concrete,
            false,
            0.0,
        );
        // A wall panel: from `a` to `b` (local x, z) along the wall, `y0` to
        // `y1` up, `half_t` thick each side, with a grimy band low down.
        let panel = |m: &mut Map, a: Vec2, b: Vec2, y0: f32, y1: f32| {
            let mid = (a + b) * 0.5;
            let half = ((b - a).abs() * 0.5).max(Vec2::splat(t));
            rig.part(
                m,
                Vec3::new(mid.x, (y0 + y1) * 0.5, mid.y),
                Vec3::new(half.x, (y1 - y0) * 0.5, half.y),
                id,
                clad * (1.0 + 0.03 * (mid.x * 1.7 + mid.y * 0.9).sin()),
                Material::Painted,
                true,
                0.0,
            );
            if y0 < 0.5 {
                let grow = Vec2::splat(0.04);
                let h = ((b - a).abs() * 0.5).max(Vec2::splat(t)) + grow;
                rig.part(
                    m,
                    Vec3::new(mid.x, 0.55, mid.y),
                    Vec3::new(h.x, 0.65, h.y),
                    id,
                    dado,
                    Material::Concrete,
                    false,
                    0.0,
                );
            }
        };
        // The sides, a door in the middle of each.
        for sx in [-1.0f32, 1.0] {
            let x = sx * hw;
            panel(
                self,
                Vec2::new(x, -hl),
                Vec2::new(x, -side_door),
                0.0,
                eaves,
            );
            panel(self, Vec2::new(x, side_door), Vec2::new(x, hl), 0.0, eaves);
            panel(
                self,
                Vec2::new(x, -side_door),
                Vec2::new(x, side_door),
                door_h,
                eaves,
            );
        }
        // The ends, a door in the middle of each.
        for sz in [-1.0f32, 1.0] {
            let z = sz * hl;
            let w = hw - t;
            panel(self, Vec2::new(-w, z), Vec2::new(-end_door, z), 0.0, eaves);
            panel(self, Vec2::new(end_door, z), Vec2::new(w, z), 0.0, eaves);
            panel(
                self,
                Vec2::new(-end_door, z),
                Vec2::new(end_door, z),
                door_h,
                eaves,
            );
            // The gable over it: a plate under each slope of the roof, its
            // lower edge hidden in the wall.
            let rise = ridge - eaves;
            let a = (rise / hw).atan();
            let len = (hw * hw + rise * rise).sqrt();
            let d = rise / a.cos() + 0.2;
            for (sx, th) in [(1.0f32, 0.15), (-1.0, 0.13)] {
                let n = Vec2::new(-sx * a.sin(), -a.cos());
                let mid = Vec2::new(sx * hw * 0.5, (eaves + ridge) * 0.5) + n * (d * 0.5);
                rig.part(
                    self,
                    Vec3::new(mid.x, mid.y, z),
                    Vec3::new(len * 0.5, d * 0.5, th),
                    Quat::from_rotation_z(-sx * a),
                    clad * 0.97,
                    Material::Painted,
                    false,
                    0.0,
                );
            }
        }
        // The roof: two slopes overhanging the walls, a cap along the ridge.
        let rise = ridge - eaves;
        let a = (rise / hw).atan();
        let over = 0.8;
        let len = ((hw + over) * (hw + over) + (rise + over * a.tan()).powi(2)).sqrt();
        for sx in [-1.0f32, 1.0] {
            let up = Vec2::new(sx * a.sin(), a.cos()) * 0.13;
            let mid = Vec2::new(
                sx * (hw + over) * 0.5,
                ridge - (rise + over * a.tan()) * 0.5,
            ) + up;
            rig.part(
                self,
                Vec3::new(mid.x, mid.y, 0.0),
                Vec3::new(len * 0.5, 0.12, hl + 0.7),
                Quat::from_rotation_z(-sx * a),
                Vec3::new(0.44, 0.44, 0.43),
                Material::Painted,
                true,
                0.0,
            );
        }
        rig.part(
            self,
            Vec3::new(0.0, ridge + 0.28, 0.0),
            Vec3::new(0.35, 0.1, hl + 0.75),
            id,
            frame,
            Material::Metal,
            false,
            0.0,
        );
        // Portal frames inside: columns along the walls, a tie across at
        // the eaves, rafters up under the roof. Plasma lamps hang from the
        // ties over the aisles.
        let mut z = -hl + 6.0;
        while z < hl - 1.0 {
            for sx in [-1.0f32, 1.0] {
                if z.abs() > side_door + 0.4 {
                    rig.part(
                        self,
                        Vec3::new(sx * (hw - t - 0.25), eaves * 0.5, z),
                        Vec3::new(0.25, eaves * 0.5, 0.2),
                        id,
                        frame,
                        Material::Metal,
                        true,
                        0.0,
                    );
                }
                let e = rig.point(Vec3::new(sx * (hw - t), eaves - 0.4, z));
                let r = rig.point(Vec3::new(0.0, ridge - 0.4, z));
                self.segment(e, r, 0.18, frame, Material::Metal, false);
            }
            let (a, b) = (
                rig.point(Vec3::new(-(hw - t), eaves - 0.5, z)),
                rig.point(Vec3::new(hw - t, eaves - 0.5, z)),
            );
            self.segment(a, b, 0.12, frame, Material::Metal, false);
            for x in [-4.5f32, 4.5] {
                self.plasma_lamp(rig.point(Vec3::new(x, eaves - 0.6, z)), 1.0, 1.0);
            }
            z += 6.0;
        }
        // Outside: a rolled-up door drum over each door, a plasma lamp
        // either side of it.
        for (c, out, w) in [
            (Vec2::new(0.0, -hl), Vec2::new(0.0, -1.0), end_door),
            (Vec2::new(0.0, hl), Vec2::new(0.0, 1.0), end_door),
            (Vec2::new(-hw, 0.0), Vec2::new(-1.0, 0.0), side_door),
            (Vec2::new(hw, 0.0), Vec2::new(1.0, 0.0), side_door),
        ] {
            let face = c + out * (t + 0.4);
            let across = out.perp();
            let (a, b) = (face - across * (w + 0.3), face + across * (w + 0.3));
            self.pipe(
                rig.point(Vec3::new(a.x, door_h + 0.45, a.y)),
                rig.point(Vec3::new(b.x, door_h + 0.45, b.y)),
                0.42,
                frame * 1.2,
                Material::Painted,
                false,
            );
            for s in [-1.0f32, 1.0] {
                let p = c + out * (t + 0.02) + across * s * (w + 1.2);
                let dir = rig.rot * Vec3::new(out.x, 0.0, out.y);
                self.wall_lamp(
                    rig.point(Vec3::new(p.x, door_h - 0.2, p.y)),
                    Vec2::new(dir.x, dir.z),
                    1.0,
                );
            }
        }
        // The emblem, small, high on the front over the rail door: plasma
        // held in crystal, an orb in an almond-shaped lens.
        self.lens_emblem(&rig, Vec3::new(0.0, eaves + 1.4, -hl - 0.2), 0.7);
        // The bins of ore along the walls (a gap at each side door), one
        // left out in the aisle by the rail door.
        let colours = [
            Vec3::new(0.52, 0.3, 0.16),
            Vec3::new(0.55, 0.47, 0.25),
            Vec3::new(0.38, 0.4, 0.41),
            Vec3::new(0.3, 0.37, 0.31),
        ];
        let mut spots = Vec::new();
        for sx in [-1.0f32, 1.0] {
            for z in [-14.2f32, -8.4, 8.4, 14.2] {
                spots.push((Vec2::new(sx * 9.0, z), rng.range(-0.04, 0.04)));
            }
        }
        spots.push((Vec2::new(-4.6, -9.0), 0.45));
        for (p, turn) in spots {
            let w = rig.point(Vec3::new(p.x, 0.0, p.y));
            let c = colours[rng.index(colours.len())];
            self.ore_bin(rng, Vec2::new(w.x, w.z), yaw + turn, c);
        }
        // A hopper at the buffer stop, and the stop.
        let car = rig.point(Vec3::new(0.0, 0.0, SHED_BUFFER - 5.4));
        self.rail_car(rng, Vec2::new(car.x, car.z), yaw, 0.0, false);
        rig.part(
            self,
            Vec3::new(0.0, 0.7, SHED_BUFFER + 0.3),
            Vec3::new(1.6, 0.7, 0.3),
            id,
            RUST,
            Material::Rust,
            true,
            0.0,
        );
        for sx in [-1.0f32, 1.0] {
            rig.part(
                self,
                Vec3::new(sx * GAUGE * 0.5, 0.85, SHED_BUFFER - 0.15),
                Vec3::new(0.22, 0.22, 0.2),
                id,
                STEEL,
                Material::Metal,
                false,
                0.0,
            );
        }
    }

    /// An open-top steel bin, tank high, heaped with crystal-rich ore:
    /// broken red rock with raw crystal through it.
    fn ore_bin(&mut self, rng: &mut Rng, at: Vec2, yaw: f32, colour: Vec3) {
        let g = self.ground_height(at.x, at.y);
        let rig = Rig {
            pos: Vec3::new(at.x, g, at.y),
            rot: Quat::from_rotation_y(yaw),
        };
        let id = Quat::IDENTITY;
        let (hx, hz, h, t) = (1.5f32, 2.4f32, 1.8f32, 0.08f32);
        let paint = colour * rng.range(0.9, 1.05);
        // Floor, sides and ends, a rolled rim on top.
        rig.part(
            self,
            Vec3::new(0.0, 0.12, 0.0),
            Vec3::new(hx, 0.12, hz),
            id,
            paint * 0.8,
            Material::Painted,
            true,
            0.0,
        );
        for s in [-1.0f32, 1.0] {
            rig.part(
                self,
                Vec3::new(s * (hx - t), h * 0.5 + 0.12, 0.0),
                Vec3::new(t, h * 0.5, hz),
                id,
                paint,
                Material::Painted,
                true,
                0.0,
            );
            rig.part(
                self,
                Vec3::new(0.0, h * 0.5 + 0.12, s * (hz - t)),
                Vec3::new(hx - 2.0 * t, h * 0.5, t),
                id,
                paint,
                Material::Painted,
                true,
                0.0,
            );
            rig.part(
                self,
                Vec3::new(s * (hx - t), h + 0.16, 0.0),
                Vec3::new(t + 0.05, 0.06, hz + 0.05),
                id,
                paint * 0.85,
                Material::Painted,
                false,
                0.0,
            );
            // Ribs down the sides, a lifting lug at each end.
            for rz in [-1.2f32, 0.0, 1.2] {
                rig.part(
                    self,
                    Vec3::new(s * (hx + 0.05), h * 0.5 + 0.12, rz),
                    Vec3::new(0.05, h * 0.5, 0.09),
                    id,
                    paint * 0.9,
                    Material::Painted,
                    false,
                    0.0,
                );
            }
            rig.part(
                self,
                Vec3::new(0.0, h + 0.05, s * (hz + 0.1)),
                Vec3::new(0.3, 0.12, 0.08),
                id,
                STEEL,
                Material::Metal,
                false,
                0.0,
            );
        }
        // The ore, heaped just under the rim.
        let top = h + 0.02;
        for _ in 0..7 {
            let p = Vec3::new(
                rng.range(-hx + 0.5, hx - 0.5),
                top - rng.range(0.25, 0.5),
                rng.range(-hz + 0.5, hz - 0.5),
            );
            let s = rng.range(0.25, 0.5);
            let tip = Quat::from_euler(
                glam::EulerRot::YXZ,
                rng.range(0.0, TAU),
                rng.range(-0.5, 0.5),
                rng.range(-0.5, 0.5),
            );
            let shade = rng.range(0.75, 1.05);
            rig.part(
                self,
                p,
                Vec3::new(s, s * 0.7, s * 1.2),
                tip,
                STONE * shade,
                Material::Rubble,
                false,
                0.0,
            );
        }
        rig.part(
            self,
            Vec3::new(0.0, top - 0.55, 0.0),
            Vec3::new(hx - 2.0 * t - 0.02, 0.3, hz - 2.0 * t - 0.02),
            id,
            STONE_DARK,
            Material::Rubble,
            false,
            0.0,
        );
        for _ in 0..4 {
            let p = Vec3::new(
                rng.range(-hx + 0.4, hx - 0.4),
                top - 0.1,
                rng.range(-hz + 0.4, hz - 0.4),
            );
            let tip = Quat::from_euler(
                glam::EulerRot::YXZ,
                rng.range(0.0, TAU),
                rng.range(-0.6, 0.6),
                rng.range(-0.6, 0.6),
            );
            let s = rng.range(0.1, 0.18);
            rig.part(
                self,
                p,
                Vec3::new(s, s * 2.6, s),
                tip,
                RAW * (0.85 + (s - 0.1) * 3.75),
                Material::Ore,
                false,
                0.0,
            );
        }
    }

    /// A plasma lamp hanging `drop` m on a rod from `at`: the Ithessar's
    /// light everywhere they work, their emblem made real. An orb of plasma
    /// held in an almond-shaped cage of crystal prongs, pinched to a point
    /// below and gathered into a crystal cap above.
    fn plasma_lamp(&mut self, at: Vec3, drop: f32, size: f32) {
        let top = at - Vec3::Y * drop;
        if drop > 0.05 {
            self.segment(at, top, 0.025, STEEL, Material::Metal, false);
        }
        // The cap the cage hangs from: a short hexagonal collar.
        self.pipe(
            top,
            top - Vec3::Y * 0.12 * size,
            0.11 * size,
            GLASS * 1.3,
            Material::Vhaal,
            false,
        );
        let (h, r) = (0.82 * size, 0.24 * size);
        let crown = top - Vec3::Y * 0.12 * size;
        let tip = crown - Vec3::Y * h;
        let mid = (crown + tip) * 0.5;
        // The orb: a glowing core, haloed.
        let orb = mid + Vec3::Y * 0.03 * size;
        let core = PLASMA * 1.6;
        self.lights.push(crate::render::PointLight {
            pos: orb,
            color: PLASMA.lerp(PLASMA_HOT, 0.45) * LAMP_BRIGHT * size,
            radius: LAMP_REACH * size,
        });
        for k in 0..3 {
            let a = PI / 3.0 * k as f32;
            let axis = Vec3::new(a.cos(), 0.0, a.sin());
            self.disc(orb, axis * 0.1 * size, 0.18 * size, core, Material::Glow);
        }
        self.disc(orb, Vec3::Y * 0.1 * size, 0.18 * size, core, Material::Glow);
        // The lens: five prongs bowed out round it, swept a little like
        // grown crystal, meeting top and bottom.
        let n = 4;
        for k in 0..n {
            let a = TAU * k as f32 / n as f32;
            let swirl = 0.35;
            let bow = |t: f32, twist: f32| {
                let ang = a + twist;
                let w = r * (PI * t).sin();
                let y = crown.y + (tip.y - crown.y) * t;
                Vec3::new(mid.x + ang.cos() * w, y, mid.z + ang.sin() * w)
            };
            let pts = [
                bow(0.0, 0.0),
                bow(0.22, swirl * 0.3),
                bow(0.5, swirl * 0.6),
                bow(0.78, swirl * 0.85),
                bow(1.0, swirl),
            ];
            for w in pts.windows(2) {
                self.segment(
                    w[0],
                    w[1],
                    0.014 * size,
                    GLASS * 3.0,
                    Material::Vhaal,
                    false,
                );
            }
        }
        // The point it's pinched to, below, a bead of plasma on it.
        self.segment(
            tip + Vec3::Y * 0.04 * size,
            tip - Vec3::Y * 0.14 * size,
            0.03 * size,
            GLASS * 1.3,
            Material::Vhaal,
            false,
        );
        self.disc(
            tip - Vec3::Y * 0.16,
            Vec3::Y * 0.025,
            0.04,
            PLASMA,
            Material::Glow,
        );
    }

    /// A plasma lamp on a wall, `size` times the usual: a steel bracket out from `at` (on the
    /// wall's face) along `out`, the lamp hanging off its end.
    fn wall_lamp(&mut self, at: Vec3, out: Vec2, size: f32) {
        let o = Vec3::new(out.x, 0.0, out.y).normalize() * 0.45 * size;
        let end = at + o;
        // The arm, a strut under it back to the wall, a plate on the wall.
        self.segment(at, end, 0.04, STEEL, Material::Metal, false);
        self.segment(
            at - Vec3::Y * 0.4,
            at + o * 0.65,
            0.025,
            STEEL,
            Material::Metal,
            false,
        );
        self.oboxes.push(OBox {
            center: Vec2::new(at.x, at.z),
            yaw: out.x.atan2(out.y),
            half: Vec2::new(0.12, 0.03),
            y0: at.y - 0.5,
            y1: at.y + 0.1,
            color: STEEL,
            side_mat: Material::Metal,
            top_mat: Material::Metal,
            solid: false,
            ..OBox::default()
        });
        self.plasma_lamp(end - Vec3::Y * 0.04, 0.0, size);
    }

    /// The Ithessar emblem on a wall, in `rig`'s frame at `at`, facing its
    /// -z: an almond-shaped lens of crystal `r` m across each way from
    /// the middle, a plasma orb in it.
    fn lens_emblem(&mut self, rig: &Rig, at: Vec3, r: f32) {
        // As the HUD draws it: two arcs meeting in points either side.
        let h = 0.58 * r;
        let off = (r * r - h * h) / (2.0 * h);
        let rad = off + h;
        let a = (off / r).atan();
        let n = 10;
        for (cy, from, to) in [(off, PI + a, TAU - a), (-off, a, PI - a)] {
            let p = |k: usize| {
                let t = from + (to - from) * k as f32 / n as f32;
                rig.point(at + Vec3::new(t.cos() * rad, cy + t.sin() * rad, 0.0))
            };
            for k in 0..n {
                self.segment(p(k), p(k + 1), 0.05, GLASS * 1.3, Material::Vhaal, false);
            }
        }
        let c = rig.point(at);
        self.disc(
            c,
            rig.rot * Vec3::Z * -0.03,
            0.3 * r,
            PLASMA,
            Material::Glow,
        );
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
                let s = rng.range(1.0, 1.8);
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
            (116.0, 46.0, 1.7),
            (100.0, 16.0, 1.5),
            (62.0, 14.0, 1.8),
            (30.0, 34.0, 1.6),
            (146.0, 100.0, 1.6),
            (176.0, 121.0, 1.5),
            (195.0, 88.0, 1.7),
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
            (Vec2::new(105.0, 116.0), 0.4),
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
            if under_crag(p) { CRAG_ROOF } else { ROOF.1 }
        } else {
            self.ground_height(p.x, p.y)
        }
    }

    /// Near a point on one of the ways round the bots take, or the Hold
    /// site (kept open).
    /// Nothing to keep clear for driving within `r` of `at`: no way round,
    /// ramp or ford, track, the run out of the warehouse's doors, or the
    /// foot of the Face.
    fn room(&self, at: Vec2, r: f32) -> bool {
        (0..9).all(|k| {
            let q = if k == 0 {
                at
            } else {
                let a = k as f32 / 8.0 * TAU;
                at + Vec2::new(a.cos(), a.sin()) * r
            };
            let f = face(q.x);
            !self.on_way(q)
                && track_distance(q) > 6.0
                && shed_distance(q) > 14.0
                && !(q.y > f - 8.0 && q.y < f + FACE_RISE + 5.0)
                && !RAMPS
                    .iter()
                    .chain(&FORDS)
                    .any(|&(s, e, h)| seg(q, s, e).0 < h + 4.0)
                && !WAYS_UP.iter().any(|&(a, b)| seg(q, a, b).0 < 6.0)
        })
    }

    fn on_way(&self, q: Vec2) -> bool {
        self.routes
            .iter()
            .flatten()
            .any(|w| Vec2::new(w.x, w.z).distance(q) < 4.5)
            || self.hold_site.is_some_and(|h| h.distance(q) < 7.0)
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
            if keep_clear(q) || self.on_way(q) {
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
            } else if roll < 0.6 {
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
                raw: false,
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

    /// Take out the stand-ins for the blocks along the Face (see
    /// `rough_face`).
    fn clear_face_blocks(&mut self) {
        self.oboxes.retain(|o| !(o.hidden && o.color == FACE_BLOCK));
    }

    /// Every block of stone on the map drawn as rock, not a box: rounded,
    /// lumpy, over its box (still its collider). The map's edge,
    /// the box cut's sides, the hoodoos, boulders and rubble.
    fn rock_lumps(&mut self) {
        let mut mesh = std::mem::take(&mut self.sculpted);
        for (k, o) in self.oboxes.iter_mut().enumerate() {
            let stone = o.side_mat == Material::Rubble && o.top_mat == Material::Rubble;
            if !stone || o.hidden || o.sculpted {
                continue;
            }
            o.sculpted = true;
            let hy = (o.y1 - o.y0) * 0.5;
            let half = Vec3::new(o.half.x, hy, o.half.y);
            let centre = o.center3();
            let rot = o.rotation();
            let seed = k as u32 * 7 + 3;
            let small = half.min_element();
            // Big blocks get more facets.
            let big = half.max_element() > 2.0;
            let (rows, cols) = if half.max_element() > 6.0 {
                (12usize, 18usize)
            } else if big {
                (8, 12)
            } else {
                (6, 8)
            };
            let jag = |i: usize, j: usize, c: u32| hash(i as i32, j as i32, seed + c) - 0.5;
            // A rounded box over the block, a little proud of it so its
            // corners don't show, broken into facets.
            let spow = |c: f32, e: f32| c.signum() * c.abs().powf(e);
            let point = |i: usize, j: usize| {
                let j = j % cols;
                let wobble = if i == 0 || i == rows - 1 {
                    0.0
                } else {
                    0.35 * jag(i, j, 0)
                };
                let phi = -FRAC_PI_2 + PI * (i as f32 + wobble * 0.5) / (rows - 1) as f32;
                let th = TAU * (j as f32 + wobble) / cols as f32 + seed as f32;
                // Flat-ish top and steep sides, round across.
                let (ev, eh) = (0.45, 0.8);
                let d = Vec3::new(
                    spow(phi.cos(), ev) * spow(th.cos(), eh),
                    spow(phi.sin(), ev),
                    spow(phi.cos(), ev) * spow(th.sin(), eh),
                );
                let q = d * half * 1.1;
                let w = rot * q + centre;
                // Lumps across it, and every corner pushed in or out.
                let lump = noise2(w.x * 0.35 + w.z * 0.15, w.y * 0.4, seed) - 0.5;
                let push = (0.45 * lump + 0.35 * jag(i, j, 1))
                    * (0.4 * small + 0.15).min(1.2 + 0.08 * half.max_element());
                w + (rot * d.normalize_or_zero()) * push
            };
            let at = |i: usize, j: usize| point(i, j);
            let mut tris: Vec<[Vec3; 3]> = Vec::new();
            for i in 0..rows - 1 {
                for j in 0..cols {
                    let q = [at(i, j), at(i, j + 1), at(i + 1, j + 1), at(i + 1, j)];
                    tris.push([q[0], q[2], q[1]]);
                    tris.push([q[0], q[3], q[2]]);
                }
            }
            // Flat shaded: each facet its own plane.
            for t in tris {
                let mut n = (t[1] - t[0]).cross(t[2] - t[0]).normalize_or_zero();
                let mid = (t[0] + t[1] + t[2]) / 3.0;
                if n.dot(mid - centre) < 0.0 {
                    n = -n;
                }
                let shade = 0.88
                    + 0.24
                        * hash(
                            (mid.x * 3.0) as i32,
                            (mid.y * 3.0 + mid.z * 5.0) as i32,
                            seed,
                        );
                let base = mesh.verts.len() as u32;
                for p in t {
                    mesh.verts.push(crate::render::mesh::Vertex {
                        pos: p.to_array(),
                        normal: n.to_array(),
                        color: (o.color * shade).extend(Material::Cliff.id()).to_array(),
                    });
                }
                mesh.indices.extend([base, base + 1, base + 2]);
            }
        }
        self.sculpted = mesh;
    }

    /// The edge: a wall of boulders all the way round, on whatever ground
    /// it meets (the flats, the mesa, the adit's floor).
    fn edge_rocks(&mut self, rng: &mut Rng) {
        let shades = [STONE, STONE_DARK, STONE_PALE];
        for (p, inward) in self.edge_points(3.2, 2.0) {
            let at = p + inward * rng.range(0.4, 1.0);
            let q = at.clamp(Vec2::splat(0.5), Vec2::splat(SIZE - 0.5));
            if crag_depth(q) > 1.5 {
                continue;
            }
            let g = self.ground_height(q.x, q.y);
            // Lower inside the adit, under its roof.
            let tall = if cave_distance(q) < 1.0 {
                5.5
            } else {
                rng.range(5.0, 8.0)
            };
            let size = rng.range(3.2, 4.2);
            // Down to the lowest ground under it (on a slope, the wash's
            // bank, its low side mustn't leave a gap under it).
            let low = (0..8)
                .map(|k| {
                    let a = k as f32 * TAU / 8.0;
                    let p = at + Vec2::new(a.cos(), a.sin()) * size * 1.4;
                    self.ground_height(p.x, p.y)
                })
                .fold(g, f32::min);
            self.oboxes.push(OBox {
                center: at,
                yaw: rng.range(0.0, TAU),
                half: Vec2::new(size, size * rng.range(0.85, 1.0)),
                y0: low.min(g - 1.5),
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
    /// adit, through the hall, along the drift, out at the breach), along
    /// the Crag's foot into the new heading, and down the box cut into the
    /// mine.
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
            // From the east landing along the Crag's foot and into the new
            // heading.
            vec![
                g(m, 160.0, 140.0),
                g(m, 136.0, 148.5),
                g(m, 126.0, 148.0),
                mine(123.0, 153.0),
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
            // Not up on the Crag, where nothing gets.
            .filter(|&q| crag_depth(q) < -4.0)
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

/// The Crag's foot, end to end: down its west side from past the north
/// edge, along the Face, up its east side to the box cut, in round the
/// slot's walls, on up to past the edge.
fn crag_feet(m: &Map) -> Vec<crag::Foot> {
    let mut corners = Vec::new();
    let zw = face(crag_west(150.0));
    let mut z = SIZE + 6.0;
    while z > zw + 3.0 {
        corners.push(Vec2::new(crag_west(z), z));
        z -= 3.0;
    }
    let (xw, xe) = (crag_west(zw), crag_east(face(crag_east(152.0))));
    let mut x = xw;
    while x < xe {
        corners.push(Vec2::new(x, face(x)));
        x += 2.0;
    }
    let ze = face(xe);
    let (s0, s1) = (BOX_CUT.0.y, BOX_CUT.1.y);
    let mut z = ze + 3.0;
    while z < s0 {
        corners.push(Vec2::new(crag_east(z), z));
        z += 3.0;
    }
    corners.push(Vec2::new(crag_east(s0), s0));
    corners.push(Vec2::new(BOX_CUT.0.x, s0));
    corners.push(Vec2::new(BOX_CUT.0.x, s1));
    corners.push(Vec2::new(crag_east(s1), s1));
    let mut z = s1 + 3.0;
    while z < SIZE + 6.0 {
        corners.push(Vec2::new(crag_east(z), z));
        z += 3.0;
    }
    let pts = crag::resample(&corners, 0.7, 1.2);
    let n = pts.len();
    let mut along = 0.0;
    let mut feet = Vec::with_capacity(n);
    for i in 0..n {
        let (p, dir) = pts[i];
        if i > 0 {
            along += p.distance(pts[i - 1].0);
        }
        let nrm = Vec2::new(dir.y, -dir.x);
        let in_slot = p.y > s0 - 0.6 && p.y < s1 + 0.6 && p.x < crag_east(p.y) - 0.5;
        let on_front = nrm.y < -0.7 && p.y < 170.0;
        let mouth = (on_front && MOUTHS.iter().any(|(a, b)| p.x > a.x && p.x < b.x))
            || (in_slot && nrm.x > 0.7 && p.y > s0 + 0.2 && p.y < s1 - 0.2);
        let q = p + nrm * 1.2;
        let base = m.level(q);
        // How sharply the foot turns here, over a few meters.
        let (d0, d1) = (pts[i.saturating_sub(6)].1, pts[(i + 6).min(n - 1)].1);
        let bend = 1.0 - smoothstep(0.25, 1.0, d0.angle_to(d1).abs());
        let by_mouth = MOUTHS
            .iter()
            .map(|(a, b)| (a.x - p.x).max(p.x - b.x).max(0.0))
            .fold(f32::INFINITY, f32::min);
        let plain = if on_front {
            smoothstep(0.5, 5.0, by_mouth)
        } else {
            1.0
        };
        let lip = if in_slot {
            CRAG_TOP
        } else {
            CRAG_TOP
                + 1.6 * ((noise(along / 7.0, 14) * 4.0).floor() / 4.0 - 0.4)
                + 0.9 * (noise(along / 4.0, 16) - 0.5)
                - 3.5 * smoothstep(0.78, 0.92, noise(along / 16.0, 17))
        };
        let cut = if in_slot {
            CRAG_TOP + 1.0
        } else if on_front {
            base + CRAG_CUT + 2.5 * (noise(along / 13.0, 21) - 0.5)
        } else {
            -100.0
        };
        feet.push(crag::Foot {
            p,
            nrm,
            along,
            floor: if mouth { ROOF.0 } else { base },
            lip,
            cut,
            mouth,
            relief: bend * plain * if in_slot { 0.35 } else { 1.0 },
        });
    }
    feet
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
    let (depth, half) = wash_profile(along);
    let bank = d > half - 1.5 && d < half + WASH_BANK + 2.5;
    // The way along the wash's floor, down its middle.
    let floor_way = depth > 1.0 && d < 3.0;
    let f = face(p.x);
    spawns
        || bank
        || floor_way
        || track_distance(p) < 6.0
        || shed_distance(p) < 6.0
        || seg(p, BRIDGE.0, BRIDGE.1).0 < DECK_HALF + 5.0
        || RAMPS
            .iter()
            .chain(&FORDS)
            .any(|&(s, e, h)| seg(p, s, e).0 < h + 4.0)
        || cave_distance(p) < 4.0 && mesa(p) > 0.3 && !roofed(p)
        || within((BOX_CUT.0, BOX_CUT.1 + Vec2::new(14.0, 0.0)), p, 4.0)
        || crag_depth(p) > -4.0
        || (p.y > f - 8.0 && p.y < f + FACE_RISE + 5.0)
        || WAYS_UP.iter().any(|&(a, b)| seg(p, a, b).0 < 6.0)
        || SPOTS
            .iter()
            .any(|&(x, z)| p.distance(Vec2::new(x, z)) < 6.5)
        || REPAIRS.iter().any(|&q| p.distance(q) < 6.0)
        || p.min_element() < 7.0
        || p.max_element() > SIZE - 7.0
}

/// Under the Crag's rock (a roof there is the Crag's, up to its top).
fn under_crag(p: Vec2) -> bool {
    crag(p) > 0.5
}

/// Under a cave roof (or on top of one): within the walls' reach of a
/// cave, back from the Face's lip, not in the box cut.
fn roofed(p: Vec2) -> bool {
    cave_distance(p) < CAVE_WALL + 0.5
        && mesa(Vec2::new(p.x, p.y - 1.0)) > 0.9
        && !within(BOX_CUT, p, 0.0)
        && p.y < SIZE + 6.0
}
/// Draws `n` from `rng`, unused.
fn skip(rng: &mut Rng, n: usize) {
    for _ in 0..n {
        rng.next_u64();
    }
}

/// A stream of its own for the thing at `at`.
fn own_rng(at: Vec2) -> Rng {
    Rng::new(((at.x.to_bits() as u64) << 32) | at.y.to_bits() as u64)
}

/// Ithessar freight: worked crystal over dark alloy.
const HULL: Skin = Skin::new(
    Vec4::new(0.02, 0.05, 0.13, Material::Vhaal.id()),
    Vec4::new(0.05, 0.12, 0.28, Material::Vhaal.id()),
);
/// The locomotive's canopy: a lens of clearer crystal.
const LENS: Skin = Skin::new(
    Vec4::new(0.06, 0.14, 0.3, Material::Vhaal.id()),
    Vec4::new(0.12, 0.24, 0.45, Material::Vhaal.id()),
);
const GEAR: Skin = Skin::new(
    Vec4::new(0.12, 0.11, 0.13, Material::Metal.id()),
    Vec4::new(0.22, 0.2, 0.23, Material::Metal.id()),
);
/// Plasma showing through the seams.
const SEAM: Skin = Skin::new(
    Vec4::new(PLASMA.x, PLASMA.y, PLASMA.z, Material::Glow.id()),
    Vec4::new(PLASMA.x, PLASMA.y, PLASMA.z, Material::Glow.id()),
);
/// Loose crystal held in the field, lit rose by the plasma in it.
const LOOSE: Skin = Skin::new(
    Vec4::new(0.42, 0.13, 0.3, Material::Glow.id()),
    Vec4::new(0.6, 0.22, 0.42, Material::Glow.id()),
);
/// Ore from the Crag's red beds.
const ORE: Vec3 = Vec3::new(0.36, 0.15, 0.09);

/// A thin glowing bar through the points of `line` (each a center in the
/// car's frame and its half size across there).
fn seam_bar(m: &mut MeshData, line: &[(Vec3, Vec2)]) {
    let rings: Vec<Vec<Vec3>> = line
        .iter()
        .map(|(c, h)| model::at_z(&model::rect(Vec2::new(c.x, c.y), *h, 0.005), c.z))
        .collect();
    model::solid(m, &rings, 0.0, SEAM);
}

/// A bogie centered `z` along the car: a pointed alloy frame on four
/// wheels, plasma in their hubs, and the field pad on top that holds the
/// body up.
fn bogie(m: &mut MeshData, z: f32) {
    let full = model::trap(0.95, 0.7, 0.4, 0.95, 0.08);
    let end = model::trap(0.45, 0.3, 0.55, 0.9, 0.05);
    model::solid(
        m,
        &[
            model::at_z(&end, z - 1.8),
            model::at_z(&full, z - 1.3),
            model::at_z(&full, z + 1.3),
            model::at_z(&end, z + 1.8),
        ],
        0.04,
        GEAR,
    );
    for x in [-1.2f32, 1.2] {
        for dz in [-0.9, 0.9] {
            let c = Vec2::new(z + dz, 0.6);
            let wheel = model::ngon(10, 0.5, c);
            model::round(
                m,
                &[model::at_x(&wheel, x - 0.09), model::at_x(&wheel, x + 0.09)],
                0.03,
                GEAR,
            );
            let hub = model::ngon(6, 0.17, c);
            let o = x + x.signum() * 0.1;
            model::round(
                m,
                &[
                    model::at_x(&hub, o),
                    model::at_x(&hub, o + x.signum() * 0.03),
                ],
                0.0,
                SEAM,
            );
        }
    }
    seam_bar(
        m,
        &[
            (Vec3::new(0.0, 1.02, z - 1.0), Vec2::new(0.55, 0.03)),
            (Vec3::new(0.0, 1.02, z + 1.0), Vec2::new(0.55, 0.03)),
        ],
    );
}

/// An ore car's tub: open-topped, flared, pointed at both ends like a
/// lens, two bands of crystal with plasma glowing between them, on a keel.
/// A blade stands up off each point.
fn ore_tub(m: &mut MeshData) {
    // Plan outline (x, z), going round: the flanks, then a point.
    let hex = |hw: f32, hz: f32, tip: f32| {
        [
            Vec2::new(hw, -hz),
            Vec2::new(hw, hz),
            Vec2::new(0.0, tip),
            Vec2::new(-hw, hz),
            Vec2::new(-hw, -hz),
            Vec2::new(0.0, -tip),
        ]
    };
    let (bot, rim) = (hex(1.2, 2.7, 3.7), hex(1.75, 3.25, 4.45));
    let (y0, y1) = (1.4, 3.45);
    let at = |i: usize, t: f32, inset: f32| {
        let p = bot[i % 6].lerp(rim[i % 6], t) * inset;
        Vec3::new(p.x, y0 + (y1 - y0) * t, p.y)
    };
    let inside = Vec3::new(0.0, 2.4, 0.0);
    for i in 0..6 {
        for (t0, t1) in [(0.0, 0.46), (0.54, 1.0)] {
            model::plate(
                m,
                [
                    at(i, t0, 1.0),
                    at(i + 1, t0, 1.0),
                    at(i + 1, t1, 1.0),
                    at(i, t1, 1.0),
                ],
                0.12,
                inside,
                HULL,
            );
        }
        model::plate(
            m,
            [
                at(i, 0.44, 0.97),
                at(i + 1, 0.44, 0.97),
                at(i + 1, 0.56, 0.97),
                at(i, 0.56, 0.97),
            ],
            0.04,
            inside,
            SEAM,
        );
    }
    let keel: Vec<Vec2> = bot.iter().map(|p| *p * Vec2::new(0.35, 0.8)).collect();
    model::solid(
        m,
        &[model::at_y(&keel, 1.12), model::at_y(&bot, y0 + 0.02)],
        0.03,
        HULL,
    );
    // Inside, the floor of the load (the lumps heap up on it).
    let bed: Vec<Vec2> = rim.iter().map(|p| *p * 0.9).collect();
    let floor: Vec<Vec2> = bot.iter().map(|p| *p * 0.92).collect();
    model::solid(
        m,
        &[model::at_y(&floor, y0 + 0.1), model::at_y(&bed, 3.2)],
        0.02,
        Skin::new(
            ORE.extend(Material::Rubble.id()),
            (ORE * 1.15).extend(Material::Rubble.id()),
        ),
    );
    for s in [-1.0f32, 1.0] {
        model::shard(
            m,
            Vec3::new(0.0, 2.7, s * 4.15),
            Vec3::new(0.0, 1.0, s * 0.6),
            1.7,
            0.17,
            HULL,
        );
    }
}

/// The locomotive's body, prow toward +z: two long blades of crystal split
/// down the spine, the plasma that drives it glowing up through the split,
/// raking down to a knife-edged prow; a lens of clearer crystal over the
/// spine aft, and loose crystals riding the field along the prow.
fn locomotive(m: &mut MeshData) {
    // One blade's section (x, y), the right one; the left mirrors it.
    let blade = [
        Vec2::new(0.12, 1.25),
        Vec2::new(1.2, 1.3),
        Vec2::new(1.72, 2.3),
        Vec2::new(1.45, 3.3),
        Vec2::new(0.32, 4.0),
        Vec2::new(0.12, 3.75),
    ];
    // Stations along it: (z, width, height) as fractions of the full section.
    let stations = [
        (-6.5, 0.85, 0.82),
        (-6.0, 1.0, 1.0),
        (1.6, 1.0, 1.0),
        (4.4, 0.72, 0.62),
        (6.3, 0.26, 0.24),
        (6.9, 0.06, 0.1),
    ];
    let foot = 1.55;
    for side in [-1.0f32, 1.0] {
        let rings: Vec<Vec<Vec3>> = stations
            .iter()
            .map(|&(z, sx, sy)| {
                blade
                    .iter()
                    .map(|p| Vec3::new(side * p.x * sx, foot + (p.y - foot) * sy, z))
                    .collect()
            })
            .collect();
        model::solid(m, &rings, 0.03, HULL);
        // Plasma along the chine, where the flank turns in.
        seam_bar(
            m,
            &[
                (Vec3::new(side * 1.74, 2.3, -6.0), Vec2::new(0.03, 0.05)),
                (Vec3::new(side * 1.74, 2.3, 1.6), Vec2::new(0.03, 0.05)),
                (
                    Vec3::new(side * 1.26, foot + 0.75 * 0.62, 4.4),
                    Vec2::new(0.03, 0.04),
                ),
            ],
        );
    }
    // The plasma in the split.
    seam_bar(
        m,
        &[
            (Vec3::new(0.0, 2.65, -6.0), Vec2::new(0.1, 1.1)),
            (Vec3::new(0.0, 2.65, 1.6), Vec2::new(0.1, 1.1)),
            (Vec3::new(0.0, 2.2, 4.4), Vec2::new(0.07, 0.75)),
            (Vec3::new(0.0, 1.8, 6.2), Vec2::new(0.025, 0.25)),
        ],
    );
    // The canopy lens, over the spine aft, raked forward.
    let lens = [
        Vec2::new(-0.9, 3.6),
        Vec2::new(0.9, 3.6),
        Vec2::new(1.05, 4.25),
        Vec2::new(0.5, 4.85),
        Vec2::new(-0.5, 4.85),
        Vec2::new(-1.05, 4.25),
    ];
    let lens_at = |z: f32, s: f32, drop: f32| -> Vec<Vec3> {
        lens.iter()
            .map(|p| Vec3::new(p.x * s, 3.6 + (p.y - 3.6) * s - drop, z))
            .collect()
    };
    model::solid(
        m,
        &[
            lens_at(-4.9, 0.55, 0.0),
            lens_at(-4.4, 1.0, 0.0),
            lens_at(-1.8, 1.0, 0.0),
            lens_at(0.4, 0.35, 0.1),
        ],
        0.03,
        LENS,
    );
    for s in [-1.0f32, 1.0] {
        seam_bar(
            m,
            &[
                (Vec3::new(s * 1.07, 4.25, -4.3), Vec2::new(0.02, 0.04)),
                (Vec3::new(s * 1.07, 4.25, -1.9), Vec2::new(0.02, 0.04)),
            ],
        );
        // The horns that hold the drive's plasma, aft of the canopy.
        model::shard(
            m,
            Vec3::new(s * 0.55, 3.7, -5.4),
            Vec3::new(s * 0.35, 1.0, 0.0),
            1.75,
            0.16,
            HULL,
        );
        // Loose crystals riding the field along the prow, points forward.
        for k in 0..2 {
            let z = 2.0 + k as f32 * 1.6;
            let x = 2.15 - (z - 1.6) * 0.2;
            model::shard(
                m,
                Vec3::new(s * x, 2.7 - k as f32 * 0.3, z),
                Vec3::new(s * 0.1, -0.06, 1.0),
                0.8 - k as f32 * 0.15,
                0.07,
                LOOSE,
            );
        }
    }
}

/// A high room's roof over a point in it (not out in its walls), if it's
/// in one: the bay, and the hall and crosscut where the Crag is deep over
/// them (not where the mesa's top is just over the roof).
fn high_roof(p: Vec2) -> Option<f32> {
    if within(BAY, p, 0.0) {
        Some(BAY_ROOF)
    } else if (within(HALL, p, 0.0) || within(CROSSCUT, p, 0.0)) && crag_depth(p) > 4.0 {
        Some(HALL_ROOF)
    } else {
        None
    }
}

/// A roof slab's underside over 2 m cell `c`: a high room's, as high in the
/// rock round one, the usual elsewhere.
fn slab_under(c: Vec2) -> f32 {
    if let Some(h) = high_roof(c) {
        return h;
    }
    if !in_cave(c) {
        let mut best: Option<f32> = None;
        for dz in -1..=1 {
            for dx in -1..=1 {
                let q = c + Vec2::new(dx as f32, dz as f32) * 2.0;
                if let Some(h) = high_roof(q) {
                    best = Some(best.map_or(h, |b: f32| b.max(h)));
                }
            }
        }
        if let Some(h) = best {
            return h;
        }
    }
    ROOF.0
}

/// The roof's underside as drawn over a point: just under the slabs, and
/// in a high room rising from where it meets a low one.
fn drawn_roof(p: Vec2) -> f32 {
    let under = ROOF.0 - 0.08;
    let target = if rect_distance(BAY, p) < 3.0 {
        BAY_ROOF
    } else if rect_distance(HALL, p) < 3.0 || rect_distance(CROSSCUT, p) < 3.0 {
        HALL_ROOF
    } else {
        return under;
    };
    // How far it is to the nearest low room's cell.
    let cell = 2.0;
    let i0 = (p / cell).floor();
    let mut d = 4.0f32;
    for dz in -3..=3 {
        for dx in -3..=3 {
            let a = (i0 + Vec2::new(dx as f32, dz as f32)) * cell;
            let c = a + Vec2::splat(cell * 0.5);
            if in_cave(c) && high_roof(c).is_none() {
                d = d.min(rect_distance((a, a + Vec2::splat(cell)), p));
            }
        }
    }
    under + (target - ROOF.0) * smoothstep(0.0, 3.5, d)
}

/// The open space underground, as a distance (negative inside): the
/// caves and the box cut, their corners rounded, and the air out past the
/// Face.
fn open_sdf(p: Vec2) -> f32 {
    let mut d = p.y - (face(p.x) + FACE_RISE * 0.5);
    d = d.min(vault::rect_sdf(BOX_CUT, 0.3, p));
    for &r in &CAVES {
        // The bore is round: its end too.
        let round = if r == BORE { 3.0 } else { 0.8 };
        d = vault::smin(d, vault::rect_sdf(r, round, p), 1.0);
    }
    d
}

/// How far the walls arch over into the roof: wide in the high rooms,
/// right over in the bore (it's round).
fn arch(c: Vec2) -> f32 {
    let near = |r: (Vec2, Vec2), w: f32| 1.0 - smoothstep(0.0, w, rect_distance(r, c));
    let mut a: f32 = 2.2;
    a = a.max(2.2 + 1.7 * near(BORE, 1.0));
    a = a.max(2.2 + 1.6 * near(BAY, 2.0));
    a = a.max(2.2 + 1.0 * near(HALL, 2.0) * smoothstep(3.0, 6.0, crag_depth(c)));
    a
}

/// The veins of raw crystal showing in the mine's walls at edge point
/// `c`: two, low and high, pinching out and swelling, thick where the borer
/// is cutting (they bore where the seam is richest); none in the
/// Resonant's breach.
fn rock_veins(c: Vec2) -> [vault::Vein; 2] {
    let mut out = [vault::Vein::default(); 2];
    if rect_distance(CAVES[5], c) < 1.0 {
        return out;
    }
    let u = c.x + c.y;
    let rich = 1.0 - smoothstep(2.0, 8.0, rect_distance(BAY, c));
    for (k, v) in out.iter_mut().enumerate() {
        let k = k as u32;
        let y = [1.5, 4.4][k as usize] + 1.6 * (noise(u / 13.0, 30 + k) * 2.0 - 1.0);
        let present = smoothstep(
            0.45,
            0.65,
            noise(u / 15.0 + 3.7 * k as f32, 34 + k) + 0.35 * rich,
        );
        let half = (0.08 + 0.3 * noise(u / 6.0, 32 + k) + 0.25 * rich) * present;
        v.y = y;
        v.half = half;
    }
    out
}

/// The mine's rock at a point on its lining facing `n`: red beds, stained
/// dark round the veins; the bore freshly cut and scorched, molten at its
/// face.
fn rock_shade(p: Vec3, n: Vec3) -> (Vec3, Material) {
    let q = Vec2::new(p.x, p.z);
    let u = p.x + p.z;
    let bed = noise(p.y / 1.3 + 0.4 * noise(u / 9.0, 20), 21);
    let mut c = STONE_DARK.lerp(STONE, bed) * (0.82 + 0.12 * noise(u / 2.3, 22));
    if n.y < -0.6 {
        // The roof: darker, smoke-stained.
        c *= 0.72;
    }
    // Darker round the veins, where the crystal's dust stains it.
    if n.y > -0.6 {
        let near = rock_veins(q)
            .iter()
            .map(|v| {
                (1.0 - smoothstep(v.half, v.half + 0.9, (p.y - v.y).abs()))
                    * (v.half > 0.0) as u8 as f32
            })
            .fold(0.0, f32::max);
        c = c.lerp(STONE_DARK * 0.55, 0.6 * near);
    }
    // The bore: cut clean and smooth, scorched toward its face, where the
    // beam strikes (its glow is drawn with the beam).
    let strike = p.distance(BEAM.1);
    let bore = 1.0 - smoothstep(0.0, 1.5, rect_distance(BORE, q));
    if bore > 0.0 || strike < 7.0 {
        let fresh = c.lerp(CUT, bore.max(1.0 - smoothstep(4.0, 7.0, strike)));
        let scorch =
            (1.0 - smoothstep(2.0, 6.0, strike)).max(0.3 * bore * noise(u * 0.9 + p.y, 38));
        c = fresh.lerp(SCORCH, scorch);
    }
    (c, Material::Cliff)
}

/// A run of roof cells along a row: from, to, and what's over it (under
/// the Crag, the underside's height).
type RoofRun = (f32, f32, (bool, f32));

/// Length of the track up to its point `k`.
fn track_len(k: usize) -> f32 {
    line_len(&TRACK, k)
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

    /// No block of stone on the map is drawn as a box (#249): each is
    /// drawn as a lump of rock, or stands in for a sculpted face.
    #[test]
    fn no_stone_drawn_as_a_box() {
        let map = Map::mine();
        for o in &map.oboxes {
            let stone = o.side_mat == Material::Rubble && o.top_mat == Material::Rubble;
            assert!(
                !stone || o.hidden || o.sculpted,
                "a stone box drawn at {} ({}..{})",
                o.center,
                o.y0,
                o.y1
            );
            assert!(o.color != FACE_BLOCK, "a stand-in left at {}", o.center);
        }
    }

    /// Out on the mesa the caves' roofs read as ground (#249): no slab
    /// is drawn over them, and the roof drawn in its place is level with
    /// the rock round it.
    #[test]
    fn roofs_are_ground_not_slabs() {
        let map = Map::mine();
        for o in &map.oboxes {
            let roof = o.y0 == ROOF.0 && o.y1 == ROOF.1 && o.color == MESA_TOP;
            assert!(!roof || o.hidden, "a roof slab drawn at {}", o.center);
        }
        // Over the hall, the west adit and the breach: the roof's top is
        // drawn just where the ground runs on round it.
        for p in [
            Vec2::new(78.0, 184.0),
            Vec2::new(71.0, 162.0),
            Vec2::new(153.0, 175.0),
        ] {
            let top = map
                .sculpted
                .verts
                .iter()
                .filter(|v| Vec2::new(v.pos[0], v.pos[2]).distance(p) < 1.5)
                .map(|v| v.pos[1])
                .fold(f32::NEG_INFINITY, f32::max);
            assert!(
                (top - uncut(p)).abs() < 0.1,
                "roof at {p} drawn at {top}, rock at {}",
                uncut(p)
            );
        }
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
        // Over the adit, the hall and the drift: the mesa's top (or the
        // Crag's), and the mine's floor.
        for p in [
            Vec2::new(106.0, 175.0),
            Vec2::new(64.0, 184.0),
            Vec2::new(86.0, 184.0),
            Vec2::new(130.0, 191.0),
        ] {
            let roof = if under_crag(p) { CRAG_ROOF } else { ROOF.1 };
            assert!(
                (land_on(&w, p, 60.0) - roof).abs() < 0.1,
                "roof over {p} at {}",
                land_on(&w, p, 60.0)
            );
            assert!(
                land_on(&w, p, ROOF.0 - 0.5).abs() < 0.6,
                "cave floor under {p} at {}",
                land_on(&w, p, ROOF.0 - 0.5)
            );
        }
        // The mesa's top elsewhere.
        let top = land_on(&w, Vec2::new(176.0, 190.0), 30.0);
        assert!(top > TOP - 1.0 && top < TOP + 2.0, "mesa top at {top}");
        // The Crag over the mine.
        let top = land_on(&w, Vec2::new(116.0, 176.0), 60.0);
        assert!(top > CRAG_TOP - 1.0, "the Crag's top at {top}");
    }

    /// From each side's spawns, every part of the map is a nav path away,
    /// and neither side is much further than the other from what's worth
    /// taking: the bridge, the wash, the mine's mouths, the mesa's top.
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
            ("the mine", Vec2::new(64.0, 184.0)),
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
        // Not held close: the Crag splits the mesa, so its west end (and the
        // hall's roof under it) are the Ithessar's, and fairness isn't the
        // point here. Neither side is shut out of anything, though.
        let (a, b) = (total(0), total(1));
        assert!(
            (a - b).abs() / a.max(b) < 0.3,
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

    /// Every hull drives in at the new heading's mouth, up it into the bay
    /// (and on into the borer's side: no climbing it either).
    #[test]
    fn tanks_drive_into_the_heading() {
        let x = HEADING.0.x + 3.0;
        for hull in HULLS {
            let mut w = crate::sim::World::empty(Map::mine());
            w.spawn_tank(
                Vec3::ONE,
                Vec2::new(x, face(x) - 4.0),
                0.0,
                hull,
                crate::sim::TurretKind::Kontos,
            );
            let cmd = crate::sim::Command {
                throttle: 1.0,
                ..Default::default()
            };
            let mut high = f32::NEG_INFINITY;
            for _ in 0..(6.0 / crate::game::DT) as usize {
                w.step(&[cmd]);
                high = high.max(w.tanks[0].pos.y);
            }
            let end = w.tanks[0].pos;
            assert!(end.z > BAY.0.y + 2.0, "{hull:?} only got to {end}");
            assert!(high < 3.0, "{hull:?} went up to {high}");
        }
    }

    /// The Crag stands tall over the mine, and nothing drives up it: a
    /// tank driven at its face from the landing stops at its foot.
    #[test]
    fn the_crag_stops_tanks() {
        let m = Map::mine();
        assert!(m.ground_height(88.0, 164.0) > CRAG_TOP - 1.0);
        for x in [86.0, 96.0, 116.0, 138.0] {
            for hull in HULLS {
                let (end, _, high) = drive(hull, Vec2::new(x, face(x) - 8.0), 0.0, 6.0);
                // It may slide along the foot into a mouth, but not into
                // the rock.
                let at = Vec2::new(end.x, end.z);
                assert!(
                    end.z < face(end.x) + 0.5 || in_cave(at),
                    "{hull:?} got in to {end} at {x}"
                );
                // (Rearing against the rock where the foot runs slantwise
                // lifts it a little, no more.)
                assert!(high < 5.0, "{hull:?} climbed to {high} at {x}");
            }
        }
        // Nor up its sides from the mesa.
        for (p, yaw) in [
            (Vec2::new(64.0, 196.0), FRAC_PI_2),
            (Vec2::new(164.0, 190.0), -FRAC_PI_2),
        ] {
            let (end, _, high) = drive(HULLS[0], p, yaw, 8.0);
            assert!(high < TOP + 2.5, "climbed to {high} from {p} ({end})");
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
                    // Until it's across (then it's done: what's strewn on
                    // past the far top isn't the crossing).
                    let mut w = crate::sim::World::empty(Map::mine());
                    w.spawn_tank(
                        Vec3::ONE,
                        a - dir * 4.0,
                        yaw_along(dir),
                        hull,
                        crate::sim::TurretKind::Kontos,
                    );
                    let cmd = crate::sim::Command {
                        throttle: 1.0,
                        ..Default::default()
                    };
                    let mut low = f32::INFINITY;
                    for _ in 0..(12.0 / crate::game::DT) as usize {
                        w.step(&[cmd]);
                        let p = w.tanks[0].pos;
                        low = low.min(p.y);
                        if (Vec2::new(p.x, p.z) - b).dot(dir) > 2.0 {
                            break;
                        }
                    }
                    let end = w.tanks[0].pos;
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
            // Over the hall's west end, on the roof.
            (
                Vec3::new(64.0, TOP, 176.0),
                Vec3::new(64.0, TOP, 196.0),
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
            Vec2::new(64.0, 190.0),
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
        // Nor up the Crag (its face has colliders of its own).
        Map::mine().assert_edge_walled(0.5, |p| {
            let d = cave_distance(p);
            (d > 0.0 && d < CAVE_WALL) || crag_depth(p) > -1.0
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
    /// Where a tank `local` in the warehouse's frame (across, along) is in
    /// the world, and the yaw that points it `dir` in that frame.
    fn in_shed(local: Vec2, dir: Vec2) -> (Vec2, f32) {
        let (at, along) = shed();
        let across = Vec2::new(along.y, -along.x);
        let w = |v: Vec2| across * v.x + along * v.y;
        (at + w(local), yaw_along(w(dir)))
    }

    /// Every hull drives in at one side door, across between the bins and
    /// out of the other.
    #[test]
    fn tanks_drive_through_the_warehouse() {
        for hull in HULLS {
            let (at, yaw) = in_shed(Vec2::new(-SHED_HALF.x - 6.0, -1.0), Vec2::X);
            let mut w = crate::sim::World::empty(Map::mine());
            w.spawn_tank(Vec3::ONE, at, yaw, hull, crate::sim::TurretKind::Kontos);
            let cmd = crate::sim::Command {
                throttle: 1.0,
                ..Default::default()
            };
            // Until it's out of the far door (not whatever it climbs after,
            // driving on blind).
            let mut high = f32::NEG_INFINITY;
            let mut p = shed_local(at);
            for _ in 0..(8.0 / crate::game::DT) as usize {
                w.step(&[cmd]);
                let t = w.tanks[0].pos;
                p = shed_local(Vec2::new(t.x, t.z));
                high = high.max(t.y);
                if p.x > SHED_HALF.x + 1.0 {
                    break;
                }
            }
            assert!(p.x > SHED_HALF.x + 1.0, "{hull:?} only got to {p} (local)");
            assert!(high < 2.0, "{hull:?} went up to {high}");
        }
    }

    /// Every hull drives in at the rail door, along the line past the middle.
    #[test]
    fn tanks_drive_in_at_the_rail_door() {
        for hull in HULLS {
            let (at, yaw) = in_shed(Vec2::new(0.0, -SHED_HALF.y - 8.0), Vec2::Y);
            let (end, _, _) = drive(hull, at, yaw, 6.0);
            let p = shed_local(Vec2::new(end.x, end.z));
            assert!(p.y > -2.0, "{hull:?} only got to {p} (local)");
        }
    }

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

    /// The rocks strewn about run big: more over a tank's width than
    /// pebbles (#267).
    #[test]
    fn rocks_run_big() {
        let m = Map::mine();
        let (mut small, mut big) = (0, 0);
        for o in &m.oboxes {
            let on_map = o.center.min_element() > 0.0 && o.center.max_element() < SIZE;
            // Rock on the ground (not ore heaped in a car), short of the
            // edge's wall.
            if o.side_mat != Material::Rubble
                || o.broken
                || !on_map
                || o.y0 >= m.ground_height(o.center.x, o.center.y)
            {
                continue;
            }
            let s = o.half.max_element();
            if s < 0.7 {
                small += 1;
            } else if (1.8..3.0).contains(&s) {
                big += 1;
            }
        }
        assert!(small < 60, "{small} small rocks");
        assert!(big > 70, "only {big} big rocks");
    }
}
