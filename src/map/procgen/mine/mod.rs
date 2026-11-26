//! The crystal mine biome: mines dug into a desert escarpment, on levels
//! stacked over each other. Laid out in layers from each seed (`layout`):
//! landforms (the escarpment with one or two mines in it, each under its
//! Crag with a scree beside it; none to two washes, a second one perhaps
//! running into the first; none to two buttes), then a railroad out of each
//! mine bridging what it crosses, then whatever's cut off joined up by rule
//! (haul ramps, crossings, ways up), then each side's spawns where it's
//! fairest, then the details. The arena is sized to what's there, and the
//! finished map is mirrored and turned so the escarpment can lie along any
//! edge.
//!
//! Each mine is the Seam's (the hand-built mine this replaced): the rail
//! adit in from its Crag's face, the new heading and the borer's bay at its
//! end, the pillared hall, the west adit, the drift to the breach, the box
//! cut up through the Crag; its rooms placed and sized by the seed.
//!
//! Nothing here is anyone's: whose mine it is (their lamps, their machine
//! in the bay, their freight, their marker by their spawns) is the style's
//! (`MineStyle`; `ithessar` is the Ithessar's). The earthworks and the
//! mining gear any mine has are dug in `dig`.

mod dig;
mod ithessar;
mod layout;
#[cfg(test)]
mod tests;

use std::f32::consts::PI;

use glam::{Vec2, Vec3};

use super::{Look, Spec};
use crate::map::Map;
use crate::rng::Rng;

pub use super::land::Butte;
pub use super::skeleton::Mine;
pub(in crate::map) use dig::Dig;
pub use layout::Layout;

/// Clearance a spawn needs from anything solid.
const TANK_CLEAR: f32 = 4.6;

/// The mesa's top over the desert floor.
pub const TOP: f32 = 10.0;
/// How far the Face's foot is from its lip (horizontally).
pub const FACE_RISE: f32 = 4.0;
/// The scree runs this far out from the Face's foot up to the lip.
const SCREE_RUN: f32 = 38.0;
/// The Crag rises to this height. Nothing climbs it.
pub const CRAG_TOP: f32 = 40.0;
/// Its foot is quarried flat this high (over the floor) along its face.
const CRAG_CUT: f32 = 12.0;
/// Roofs this high (their underside) in the high rooms: the bay, and the
/// hall and crosscut where they're under the Crag.
const BAY_ROOF: f32 = 12.0;
const HALL_ROOF: f32 = 11.0;
/// Cave roofs: underside and top (flush with the mesa).
pub const ROOF: (f32, f32) = (7.4, TOP + 0.05);
/// A roof's top under the Crag: just under its top (the lid over the
/// tunnels is drawn at the top itself, see `crag_lid`).
pub const CRAG_ROOF: f32 = CRAG_TOP - 0.3;
/// Cave walls stand this thick round each corridor.
pub const CAVE_WALL: f32 = 2.4;
pub const WASH_DEPTH: f32 = 7.0;
/// The wash's banks, from the floor's edge to the top.
const WASH_BANK: f32 = 5.0;
pub const DECK_HALF: f32 = 3.6;
pub const DECK_TOP: f32 = 0.25;
const DECK_T: f32 = 1.0;
/// Track gauge (rail to rail, m): broad, for big ore cars.
const GAUGE: f32 = 2.2;
/// The warehouse's branch line: it leaves the main line `Layout::junction`
/// m past the track's fifth point, back the way the trains came, swings
/// off on a curve of this radius through this angle, and runs straight in
/// at the warehouse's rail door this far on.
const BRANCH_R: f32 = 14.0;
const BRANCH_TURN: f32 = 85.0 * PI / 180.0;
const SHED_LEAD: f32 = 4.0;
/// The warehouse: half its width (across the line) and length (along it),
/// its eaves and ridge.
pub const SHED_HALF: Vec2 = Vec2::new(13.0, 18.0);
const SHED_EAVES: f32 = 8.0;
const SHED_RIDGE: f32 = 11.3;
/// Where the branch ends, inside, at its buffer stop (along from the middle).
const SHED_BUFFER: f32 = 13.0;

/// A rect on the ground: (min, max).
pub type Rect = (Vec2, Vec2);
/// A ramp or crossing: from its top to its floor (or the far top), and
/// its half-width.
pub type Ramp = (Vec2, Vec2, f32);

pub(crate) fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Nearest point on segment a-b to p: (distance, 0..1 along).
pub(crate) fn seg(p: Vec2, a: Vec2, b: Vec2) -> (f32, f32) {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
    (p.distance(a + ab * t), t)
}

/// Inside a rect, grown by `grow`.
pub(crate) fn within((a, b): Rect, p: Vec2, grow: f32) -> bool {
    p.x > a.x - grow && p.x < b.x + grow && p.y > a.y - grow && p.y < b.y + grow
}

pub(crate) fn rect_distance((a, b): Rect, p: Vec2) -> f32 {
    let q = (p - (a + b) * 0.5).abs() - (b - a) * 0.5;
    q.max(Vec2::ZERO).length()
}

/// Position and heading `s` m along a line.
pub(crate) fn line_at(line: &[Vec2], s: f32) -> (Vec2, Vec2) {
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
pub(crate) fn line_len(line: &[Vec2], k: usize) -> f32 {
    line[..=k].windows(2).map(|w| w[0].distance(w[1])).sum()
}

pub(crate) fn line_distance(line: &[Vec2], p: Vec2) -> f32 {
    line.windows(2)
        .map(|w| seg(p, w[0], w[1]).0)
        .fold(f32::INFINITY, f32::min)
}

/// Yaw that turns a box's local +z along `dir`.
pub(crate) fn yaw_along(dir: Vec2) -> f32 {
    dir.x.atan2(dir.y)
}

/// On the 2 m grid: nearest, down, up.
fn even(x: f32) -> f32 {
    (x / 2.0).round() * 2.0
}
fn even_down(x: f32) -> f32 {
    (x / 2.0).floor() * 2.0
}

/// What a mine's owner puts in it: their lamps, the machine in the borer's
/// bay, their freight on the rails, their mark on the warehouse, and what
/// stands by their spawns. Any mine laid out by this biome can be dressed
/// by any style: an Ithessar crystal mine and an Imperium one are the same
/// ground, someone else's.
pub(in crate::map) trait MineStyle {
    /// The Hold site's name on the HUD.
    fn hold_name(&self) -> &'static str;
    /// A lamp on a wall at `at`, its bracket standing out along `out`,
    /// `size` its scale (1.5 in the tunnels).
    fn wall_lamp(&self, d: &mut Dig, at: Vec3, out: Vec2, size: f32);
    /// A lamp hanging `drop` m from `at`.
    fn hanging_lamp(&self, d: &mut Dig, at: Vec3, drop: f32, size: f32);
    /// The machine cutting the bore in a borer's bay, along `beam` (from
    /// its emitter to the bore's face, and how thick).
    fn borer(&self, d: &mut Dig, beam: (Vec3, Vec3, f32), rng: &mut Rng);
    /// A rail car centered at `at`, its length along `yaw`, rolled `roll`
    /// onto its side: a freight car, or the locomotive (`loco`).
    fn rail_car(&self, d: &mut Dig, at: Vec2, yaw: f32, roll: f32, loco: bool);
    /// Their mark, small, on a building at `at` in `rig`'s frame.
    fn emblem(&self, d: &mut Dig, rig: &crate::map::landmarks::Rig, at: Vec3, r: f32);
    /// What stands by side `side`'s spawns at `at`, if that's theirs.
    fn base(&self, d: &mut Dig, side: usize, at: Vec2, rng: &mut Rng);
}

fn style(look: Look) -> &'static dyn MineStyle {
    match look {
        Look::Ithessar => &ithessar::Ithessar,
        // Nobody else's mine yet: any other look is dressed as theirs.
        Look::Imperium | Look::Resonant => &ithessar::Ithessar,
    }
}

/// The layout `spec` makes: the first, from its seed on, that holds
/// together and whose greybox is playable.
pub fn layout(spec: &Spec) -> Layout {
    let mut last = None;
    for attempt in 0..64u64 {
        let mut rng = Rng::new(spec.seed.wrapping_mul(0x9E37_79B9).wrapping_add(attempt));
        let l = match Layout::generate(&mut rng) {
            Ok(l) => l,
            Err(line) => {
                log::trace!(
                    "mine seed {} attempt {attempt}: no layout ({line})",
                    spec.seed
                );
                continue;
            }
        };
        let grey = dig::greybox(&l, &spec.sides);
        let nav = crate::ai::nav::NavGrid::build(&grey);
        match super::check::playable_on(&grey, &nav).and_then(|()| fair(&l, &grey, &nav)) {
            Ok(()) => return l,
            Err(why) => log::debug!("mine seed {} attempt {attempt}: {why}", spec.seed),
        }
        last = Some(l);
    }
    last.expect("no crystal mine laid out in 64 draws")
}

/// Whether neither side is much further than the other from what's worth
/// taking, driving there on `map` (the layout's greybox): the spawns were
/// placed by straight-line distance, and a wash or a mesa between can
/// make one side's drive much longer.
fn fair(l: &Layout, map: &Map, nav: &crate::ai::nav::NavGrid) -> Result<(), String> {
    let total = |side: usize| {
        let base = &map.bases[side];
        let from = base.iter().copied().sum::<Vec2>() / base.len().max(1) as f32;
        l.goals()
            .iter()
            .map(|&g| {
                let mut at = from;
                let mut len = 0.0;
                for p in nav.path(from, g) {
                    len += at.distance(p);
                    at = p;
                }
                len
            })
            .sum::<f32>()
    };
    let (a, b) = (total(0), total(1));
    if (a - b).abs() / a.max(b).max(1.0) > 0.22 {
        return Err(format!("unfair: {a:.0} m against {b:.0} m"));
    }
    Ok(())
}

/// Builds `spec`'s mine.
pub fn build(spec: &Spec) -> Map {
    let l = layout(spec);
    let mut m = dig::build(&l, spec, style(spec.look));
    l.orient(&mut m);
    m
}

/// `spec`'s mine undressed: its ground, the caves' walls and roofs, the
/// bridge's deck.
pub fn greybox(spec: &Spec) -> Map {
    let l = layout(spec);
    let mut m = dig::greybox(&l, &spec.sides);
    m.atmosphere = atmosphere(&mut Rng::new(spec.seed ^ 0x5EED_D2E5));
    l.orient(&mut m);
    m
}

/// The sky over the mine: hot late afternoon, a low gold sun from a
/// different quarter each seed (but never straight down the Face), a hazy
/// sky going white at the horizon, sand blowing off the mesa.
pub fn atmosphere(rng: &mut Rng) -> crate::render::Atmosphere {
    // The Seam's sun came from the south-east, low.
    let az = -0.53 + rng.range(-1.1, 1.1);
    let up = rng.range(0.5, 0.62);
    let flat = 0.81 * (1.0 - up * up).sqrt();
    crate::render::Atmosphere {
        sun_dir: Vec3::new(az.cos() * flat, up * 0.81, az.sin() * flat),
        sun_color: Vec3::new(2.2, 1.85, 1.45),
        sky_zenith: Vec3::new(0.2, 0.36, 0.62),
        sky_horizon: Vec3::new(0.7, 0.6, 0.5),
        ground_color: Vec3::new(0.42, 0.3, 0.2),
        fog_density: rng.range(0.0011, 0.0015),
        wind: Vec2::new(rng.range(1.5, 3.5), rng.range(-1.5, 1.5)),
        blowing_sand: 0.3,
        dust: Vec3::new(0.72, 0.55, 0.38),
        settle: crate::render::SAND_SETTLE,
        ambient: 1.0,
        stars: 0.0,
    }
}

/// A mine world from orbit: red desert, dark basins, little sea, thin
/// cloud.
pub const PLANET: crate::planet::Look = crate::planet::Look {
    land: Vec3::new(0.34, 0.2, 0.12),
    basin: Vec3::new(0.17, 0.1, 0.07),
    sea: Vec3::new(0.03, 0.06, 0.1),
    cloud: Vec3::new(0.45, 0.42, 0.4),
    cloudiness: 0.35,
};

impl Layout {
    /// Turns the map built from this layout the way round it's meant to
    /// be: mirrored, then turned.
    pub fn orient(&self, m: &mut Map) {
        if self.mirror {
            m.mirror_x();
        }
        for _ in 0..self.turns {
            m.turn();
        }
    }
}
