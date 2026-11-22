//! The crystal mine biome: a mine dug into a desert escarpment, on levels
//! stacked over each other. What's always here (the grammar), laid out
//! fresh from each seed (`Layout::generate`):
//!
//! - **The Face**, along the north: an escarpment 10 m high, broken
//!   natural rock, headlands and bays wherever the seed puts them. Its top
//!   is the mesa. One side drives up the mesa's long west end; the other
//!   climbs the scree where the Face fell in, in the east.
//! - **The Crag**, rising 40 m sheer out of the middle of the Face, over the
//!   mine and back past the north edge: the crystal-rich cliff they're
//!   cutting into. Nothing climbs it; it splits the mesa in two.
//! - **The mine**, under the mesa and the Crag: the rail adit straight in
//!   from the Crag's face, the new heading beside it, the pillared hall
//!   west of it (high where the Crag is over it, a crosscut through to the
//!   adit), the west adit out to the Face, the east drift to the breach the
//!   other side broke in by, and the box cut, a slot sawn up through the
//!   Crag from the adit's end to the mesa's east top. At the heading's end
//!   the borer's bay, high under the Crag, with the machine cutting the
//!   bore on east.
//! - **The wash**, a dry ravine winding right across the map under the
//!   Face and out at the east edge: the seam runs along its floor. The rail
//!   bridge leads out of the adit's mouth across it; haul ramps lead down
//!   into it, and a crossing near each side's spawns.
//! - **The flats**, south of the wash: the railroad runs out across them to
//!   the south edge under the loading tipple, a branch into the warehouse
//!   on one side or the other; the waste dump, sandstone fins and hoodoos.
//!
//! What varies with the seed: the Face's line, where the Crag stands and
//! how wide, the mine's rooms (their places and sizes, the pillars), the
//! scree and the mesa's west ramp, the wash's whole course, where the
//! bridge crosses it and the line runs on to, which side the warehouse is
//! on, the dump, fins and hoodoos, the spawns and the sun; and which way
//! round it all is (half the mines are built the other way round, east to
//! west, so which side has the long ramp and which the scree varies too).
//!
//! Nothing here is anyone's: whose mine it is (their lamps, their machine
//! in the bay, their freight, their marker by their spawns) is the style's
//! (`MineStyle`; `ithessar` is the Ithessar's). The earthworks and the
//! mining gear any mine has are dug in `dig`.

mod dig;
mod ithessar;
#[cfg(test)]
mod tests;

use std::f32::consts::{PI, TAU};

use glam::{Vec2, Vec3};

use super::{Look, Spec};
use crate::map::Map;
use crate::rng::Rng;

pub(in crate::map) use dig::Dig;

/// The arena's size: 220 m square (+x east, +z north).
pub const SIZE: f32 = 220.0;
/// Clearance a spawn needs from anything solid.
const TANK_CLEAR: f32 = 4.6;

/// The mesa's top over the desert floor.
pub const TOP: f32 = 10.0;
/// How far the Face's foot is from its lip (horizontally).
pub const FACE_RISE: f32 = 4.0;
/// The scree runs this far out from the Face's foot up to the lip.
const SCREE_RUN: f32 = 30.0;
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
/// How far a crossing's ramp runs, from the top down to the floor.
const FORD_RUN: f32 = 16.0;
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
/// A heap: its middle, rough radius, the height of its flat top, and how
/// far its sides run out.
pub type Heap = (Vec2, f32, f32, f32);

/// A crystal mine, laid out: everything the ground and the build need.
#[derive(Clone, Debug)]
pub struct Layout {
    pub size: f32,
    /// The Face's foot, west to east (x, z), eased between.
    face: Vec<(f32, f32)>,
    /// The scree, where the Face fell in (x): a long slope instead of a
    /// cliff.
    pub scree: (f32, f32),
    /// The worn way up the scree's middle (x).
    scree_x: f32,
    /// The mesa's west end slopes down to the floor between these (x).
    pub west_ramp: (f32, f32),
    /// The Crag's west and east sides at its front, how far it flares west
    /// as it runs back (from `flare_z`, over 45 m), and its wobble's phase.
    crag_w: f32,
    pub crag_e: f32,
    flare: f32,
    flare_z: f32,
    phase: f32,
    /// The mine, under the mesa: every edge on the terrain's 2 m grid so
    /// the cut is clean.
    pub adit: Rect,
    pub hall: Rect,
    /// From the hall to the adit, wide and high where the cut crystal is
    /// stacked for the train.
    pub crosscut: Rect,
    /// From the hall out to the Face.
    pub west_adit: Rect,
    /// Along from the adit to the breach.
    pub drift: Rect,
    /// The other side's way in, from the Face to the drift.
    pub breach: Rect,
    /// The newest tunnel into the Crag, still being cut.
    pub heading: Rect,
    /// The borer's bay at the heading's end: a big chamber, high under the
    /// Crag, where the machine stands cutting the bore on east.
    pub bay: Rect,
    /// The round tunnel it's cutting, its face still molten.
    pub bore: Rect,
    /// All of them: the adit, hall, crosscut, west adit, drift, breach,
    /// heading, bay and bore.
    pub caves: [Rect; 9],
    /// The borer's beam: from its emitter to the bore's face, and how
    /// thick.
    pub beam: (Vec3, Vec3, f32),
    /// An open trench climbing east from the adit's end to the mesa's top.
    pub box_cut: Rect,
    /// The hall's rock pillars, left standing to hold the roof up.
    pub pillars: [Vec2; 4],
    /// The wash's line, west to east, edge to edge (its ends run on out
    /// past the map), and the phase of its floor's narrowing and widening.
    pub wash: Vec<Vec2>,
    wash_phase: f32,
    /// How far along the wash the bridge crosses it.
    pub bridge_s: f32,
    /// Haul ramps down into the wash: one into the south bank short of the
    /// bridge, one into the north bank past it, one into the east arm's
    /// bank below the east camp.
    pub ramps: [Ramp; 3],
    /// Crossings: a ramp down one bank and up the other, top to top. One by
    /// the west spawns, one below the east camp.
    pub fords: [Ramp; 2],
    /// The railroad, out of the mine to the south edge: the adit, then the
    /// bridge (from the mouth to the far bank), then out over the flats.
    pub track: Vec<Vec2>,
    /// The warehouse's branch, from the junction to the buffer stop: it
    /// leaves the main line `junction` m past its fifth point, off to the
    /// left of the way out (`branch_side` 1) or the right (-1).
    pub branch: Vec<Vec2>,
    pub junction: f32,
    pub branch_side: f32,
    /// The warehouse's middle, and its way along (from the rail door in).
    pub shed: (Vec2, Vec2),
    /// The seam's line along the wash floor, either side of the bridge.
    pub seam: Vec<Vec2>,
    pub dump: Heap,
    /// A second, smaller heap by it.
    pub heap: Heap,
    /// Sandstone fins: (from, to, height). Steep sided, not to be climbed.
    pub fins: Vec<(Vec2, Vec2, f32)>,
    /// Hoodoos among them: where, and how tall.
    pub hoodoos: Vec<(Vec2, f32)>,
    /// Each side's spawns: their columns (x), the first row (z) and how
    /// many rows, 6 m apart.
    pub spawns: [(Vec<f32>, f32, usize); 2],
    /// The west side's yard by its spawns, and the east side's camp.
    pub yard: Vec2,
    pub camp: Vec2,
    /// The ways up onto the mesa, kept clear: the haul road up its west
    /// end, the worn track up the scree, and the way round the Face's west
    /// end to the haul road's foot.
    pub ways_up: [(Vec2, Vec2); 3],
    /// The Hold site: on the wash floor short of the bridge.
    pub hold: Vec2,
    /// The cutting rigs: on the wash floor, and in the hall (where, which
    /// way their booms reach).
    pub wash_rig: (Vec2, f32),
    pub hall_rig: (Vec2, f32),
    /// The headframe over the hall's shaft, on the mesa.
    pub headframe: Vec2,
    /// Where repair drops are wanted, best first: under the Face either
    /// side, on the flats either side of the line, and more places on the
    /// flats should those be taken.
    pub repairs: Vec<Vec2>,
    /// Where each side's Raid stand is wanted: out in front of its spawns.
    pub stands: [Vec2; 2],
    /// Places bots head for (each moved to the clear ground nearest it once
    /// the map's built).
    pub spots: Vec<Vec2>,
    /// Built the other way round, east to west (see `Map::mirror_x`): the
    /// layout itself is always laid out with the mesa's long ramp in the
    /// west and the scree in the east.
    pub mirror: bool,
}

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

impl Layout {
    /// A mine from `rng`, if this draw of it holds together (the caller
    /// draws again if not).
    pub fn generate(rng: &mut Rng) -> Result<Layout, u32> {
        let size = SIZE;
        // The Crag: its west side at its front, the adit 14-20 m in from
        // it, the borer's bay beside the adit, the bore out of the bay's
        // east end, and the Crag's east side a few meters past the bore.
        let crag_w = even(rng.range(76.0, 84.0));
        let ax = crag_w + even(rng.range(14.0, 20.0));
        let bay_w = even(rng.range(20.0, 24.0));
        let bay_x = (ax + 14.0, ax + 14.0 + bay_w);
        let bore_x = (bay_x.1, bay_x.1 + 4.0);
        let crag_e = bore_x.1 + rng.range(3.5, 6.0);

        // The Face: low under the Crag (its mouths go in at it), wandering
        // elsewhere.
        let zf = rng.range(142.0, 150.0);
        let mut face = Vec::new();
        let mut x = -10.0;
        while x < size + 10.0 {
            let z = if (crag_w - 8.0..crag_e + 8.0).contains(&x) {
                rng.range(zf - 4.0, (zf + 3.0).min(151.0))
            } else {
                // (West of the Crag the hall's under the mesa: not too far
                // back there.)
                let hi = if x < crag_w { 154.0 } else { 162.0 };
                rng.range(zf - 8.0, zf + 10.0).clamp(138.0, hi)
            };
            face.push((x, z));
            x += rng.range(12.0, 17.0);
        }
        face.push((x.max(size + 10.0), rng.range(zf - 6.0, zf + 6.0)));
        let mut l = Layout {
            size,
            face,
            scree: (0.0, 0.0),
            scree_x: 0.0,
            west_ramp: (0.0, 0.0),
            crag_w,
            crag_e,
            flare: 0.0,
            flare_z: 0.0,
            phase: rng.range(0.0, TAU),
            adit: (Vec2::ZERO, Vec2::ZERO),
            hall: (Vec2::ZERO, Vec2::ZERO),
            crosscut: (Vec2::ZERO, Vec2::ZERO),
            west_adit: (Vec2::ZERO, Vec2::ZERO),
            drift: (Vec2::ZERO, Vec2::ZERO),
            breach: (Vec2::ZERO, Vec2::ZERO),
            heading: (Vec2::ZERO, Vec2::ZERO),
            bay: (Vec2::ZERO, Vec2::ZERO),
            bore: (Vec2::ZERO, Vec2::ZERO),
            caves: [(Vec2::ZERO, Vec2::ZERO); 9],
            beam: (Vec3::ZERO, Vec3::ZERO, 0.0),
            box_cut: (Vec2::ZERO, Vec2::ZERO),
            pillars: [Vec2::ZERO; 4],
            wash: Vec::new(),
            wash_phase: rng.range(0.0, TAU),
            bridge_s: 0.0,
            ramps: [(Vec2::ZERO, Vec2::ZERO, 0.0); 3],
            fords: [(Vec2::ZERO, Vec2::ZERO, 0.0); 2],
            track: Vec::new(),
            branch: Vec::new(),
            junction: 0.0,
            branch_side: 1.0,
            shed: (Vec2::ZERO, Vec2::Y),
            seam: Vec::new(),
            dump: (Vec2::ZERO, 0.0, 0.0, 0.0),
            heap: (Vec2::ZERO, 0.0, 0.0, 0.0),
            fins: Vec::new(),
            hoodoos: Vec::new(),
            spawns: [(Vec::new(), 0.0, 0), (Vec::new(), 0.0, 0)],
            yard: Vec2::ZERO,
            camp: Vec2::ZERO,
            ways_up: [(Vec2::ZERO, Vec2::ZERO); 3],
            hold: Vec2::ZERO,
            wash_rig: (Vec2::ZERO, 0.0),
            hall_rig: (Vec2::ZERO, 0.0),
            headframe: Vec2::ZERO,
            repairs: Vec::new(),
            stands: [Vec2::ZERO; 2],
            spots: Vec::new(),
            mirror: false,
        };
        l.dig_mine(rng, ax, bay_x, bore_x)?;
        l.lay_wash(rng)?;
        l.lay_track(rng)?;
        l.place_sides(rng)?;
        l.place_flats(rng)?;
        l.spots = l.find_spots();
        l.mirror = rng.chance(0.5);
        Ok(l)
    }

    /// The Face's highest and lowest foot over [x0, x1].
    fn face_range(&self, x0: f32, x1: f32) -> (f32, f32) {
        let n = ((x1 - x0) / 0.5).ceil().max(1.0) as usize;
        (0..=n)
            .map(|k| self.face(x0 + (x1 - x0) * k as f32 / n as f32))
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), z| {
                (lo.min(z), hi.max(z))
            })
    }

    /// The mine's rooms, the box cut, the mesa's west ramp and the scree.
    fn dig_mine(
        &mut self,
        rng: &mut Rng,
        ax: f32,
        bay_x: (f32, f32),
        bore_x: (f32, f32),
    ) -> Result<(), u32> {
        let s = self.size;
        let v = Vec2::new;
        // The adit, from in front of the Face to past the north edge.
        let (lo, _) = self.face_range(ax, ax + 12.0);
        self.adit = (v(ax, even_down(lo - 8.0)), v(ax + 12.0, s - 8.0));
        // The drift runs east under the box cut, the bay south of it.
        self.drift = (v(ax + 12.0, s - 34.0), v(0.0, s - 24.0));
        let bay_z1 = s - 36.0;
        self.bay = (
            v(bay_x.0, bay_z1 - even(rng.range(22.0, 24.0))),
            v(bay_x.1, bay_z1),
        );
        let mid = even((self.bay.0.y + self.bay.1.y) * 0.5);
        self.bore = (v(bore_x.0, mid - 4.0), v(bore_x.1, mid + 4.0));
        self.beam = (
            Vec3::new(bay_x.1 - 5.4, 2.1, mid),
            Vec3::new(bore_x.1, 2.1, mid),
            0.55,
        );
        // The heading: in at the Crag's face, up to the bay. The Face must
        // stand well short of the bay all along the Crag's front.
        let (lo, _) = self.face_range(ax + 20.0, ax + 32.0);
        self.heading = (
            v(ax + 20.0, even_down(lo - 6.0)),
            v(ax + 32.0, self.bay.0.y),
        );
        let (_, hi) = self.face_range(self.crag_w, self.crag_e);
        if hi + FACE_RISE + 4.0 > self.bay.0.y {
            return Err(line!());
        }
        // The hall, west of the adit, the crosscut between: the hall's west
        // end out from under the Crag (the mesa's top over it), room west of
        // it for the west adit, out of the Crag's rock.
        let x1 = ax - 6.0;
        let width = even(rng.range(28.0, 34.0)).max(x1 - (self.crag_w - 20.0));
        let x0 = x1 - width;
        let (_, hi) = self.face_range(x0 - 4.0, x1);
        let z0 = even((hi + rng.range(17.0, 22.0)).min(170.0));
        if z0 < hi + FACE_RISE + 10.0 {
            return Err(line!());
        }
        let z1 = z0 + even(rng.range(24.0, 30.0));
        if z1 > s - 18.0 {
            return Err(line!());
        }
        self.hall = (v(x0, z0), v(x1, z1));
        self.crosscut = (v(x1, z0 + 6.0), v(ax, z1 - 4.0));
        let wx = x0 + 4.0;
        let (lo, _) = self.face_range(wx, wx + 10.0);
        self.west_adit = (v(wx, even_down(lo - 8.0)), v(wx + 10.0, z0));
        // The Crag flares west as it runs back, never over the hall's west
        // end.
        self.flare_z = z0 - 8.0;
        self.flare = rng.range(10.0, 15.0).min(self.crag_w - x0 - 9.0);
        if self.flare < 4.0 {
            return Err(line!());
        }
        // The breach, east of the Crag, and the drift along to it.
        let bx = even(self.crag_e + rng.range(6.0, 9.0));
        let (lo, _) = self.face_range(bx, bx + 10.0);
        self.breach = (v(bx, even_down(lo - 6.0)), v(bx + 10.0, s - 24.0));
        self.drift.1.x = bx;
        self.box_cut = (v(ax + 12.0, s - 20.0), v(even(self.crag_e + 9.0), s - 10.0));
        self.caves = [
            self.adit,
            self.hall,
            self.crosscut,
            self.west_adit,
            self.drift,
            self.breach,
            self.heading,
            self.bay,
            self.bore,
        ];
        // The pillars, where they stood in the Seam's hall, a little off:
        // 10 m apart up it, room to drive between them.
        let w = x1 - x0;
        let mut j = || Vec2::new(rng.range(-0.5, 0.5), rng.range(-0.5, 0.5));
        self.pillars = [
            v(x0 + w * 0.28, z0 + 10.0) + j(),
            v(x0 + w * 0.7, z0 + 7.5) + j(),
            v(x0 + w * 0.31, z0 + 20.0) + j(),
            v(x0 + w * 0.69, z0 + 20.0) + j(),
        ];
        // The mesa's west end, sloping down short of the west adit: long
        // enough for the heaviest hull to drive up (the Seam's ran 40 m).
        // Its foot in from the west edge: room to drive round onto it
        // between the edge's rocks and the end of the Face.
        let wr0 = rng.range(9.0, 12.0);
        let wr1 = rng.range(48.0, 54.0).min(wx - 8.0);
        if wr1 - wr0 < 36.0 {
            return Err(line!());
        }
        self.west_ramp = (wr0, wr1);
        // The scree, past the breach, short of the east edge.
        let x0 = even(bx + 10.0 + rng.range(4.0, 8.0));
        let x1 = (x0 + even(rng.range(26.0, 32.0))).min(s - 20.0);
        if x1 - x0 < 24.0 {
            return Err(line!());
        }
        self.scree = (x0, x1);
        // The Face runs straight across where it fell in, so the scree's
        // slope doesn't lean sideways (a tank driving up it would slide off
        // its way). Then the breach's mouth is where that Face is.
        let across: Vec<usize> = (0..self.face.len())
            .filter(|&k| (x0 - 12.0..x1 + 12.0).contains(&self.face[k].0))
            .collect();
        if !across.is_empty() {
            let z = across.iter().map(|&k| self.face[k].1).sum::<f32>() / across.len() as f32;
            for k in across {
                self.face[k].1 = z;
            }
        }
        let (lo, _) = self.face_range(bx, bx + 10.0);
        self.breach.0.y = even_down(lo - 6.0);
        self.caves[5] = self.breach;
        let (_, hi) = self.face_range(self.crag_w, self.crag_e);
        if hi + FACE_RISE + 4.0 > self.bay.0.y {
            return Err(line!());
        }
        let (_, z) = self.face_range(0.0, self.west_ramp.1 + 12.0);
        let sx = (x0 + x1) * 0.5;
        let f = self.face(sx);
        self.scree_x = sx;
        let w0 = self.west_ramp.0;
        self.ways_up = [
            (v(w0, z + 12.0), v(self.west_ramp.1 + 12.0, z + 12.0)),
            (v(sx, f - SCREE_RUN - 6.0), v(sx, f + FACE_RISE + 16.0)),
            // Round the Face's west end, from the flats onto the ramp's foot.
            (v(w0 - 2.0, self.face(w0) - 14.0), v(w0, z + 12.0)),
        ];
        self.headframe = v(self.hall.0.x - 2.0, (self.hall.1.y + 9.0).min(s - 10.0));
        self.hall_rig = (v(self.hall.1.x - 4.0, z0 + 4.5), -PI * 0.5 - 0.3);
        Ok(())
    }

    /// The wash: in at the west edge under the Face, rising toward it by
    /// the adit, then swinging away south-east and out at the east edge.
    fn lay_wash(&mut self, rng: &mut Rng) -> Result<(), u32> {
        let s = self.size;
        let ax = self.adit.0.x;
        let z_west = rng.range(100.0, 116.0);
        let z_peak = self.face(ax + 6.0) - rng.range(19.5, 23.0);
        let turn = self.crag_e + rng.range(-6.0, 8.0);
        let mut pts = Vec::new();
        let mut x = -14.0;
        while x < turn {
            let t = smoothstep(-14.0, ax + 6.0, x);
            let z = z_west + (z_peak - z_west) * t + rng.range(-2.0, 2.0);
            let z = z.min(self.face_range(x - 26.0, x + 26.0).0 - 19.5);
            pts.push(Vec2::new(x, z));
            x += rng.range(20.0, 27.0);
        }
        // Round the bend: south-east, down south, then east out of the map.
        let mut at = *pts.last().unwrap();
        for k in 0.. {
            let a =
                [-0.46, -0.91, -1.1, -1.19, -0.61, -0.21, -0.1][k.min(6)] + rng.range(-0.12, 0.12);
            at += Vec2::new(a.cos(), a.sin()) * rng.range(19.0, 24.0);
            pts.push(at);
            if at.x > s + 16.0 || k > 20 {
                break;
            }
        }
        if !(28.0..80.0).contains(&at.y) || at.x < s {
            return Err(line!());
        }
        // The way up the scree starts on the flat short of its foot, out of
        // the wash.
        let x = self.scree_x;
        let f = self.face(x);
        let start = (0..=6)
            .map(|k| f - SCREE_RUN - 6.0 + k as f32)
            .find(|&z| self.wash_near_line(&pts, Vec2::new(x, z)) > 15.0)
            .ok_or(line!())?;
        self.ways_up[1].0.y = start;
        // Clear of the Face along its whole run.
        self.wash = pts;
        let mut x = 0.0;
        while x < s {
            let mut z = 0.0;
            while z < s {
                let p = Vec2::new(x, z);
                if self.wash_near(p).0 < 12.0 && p.y > self.face(p.x) - 7.0 {
                    return Err(line!());
                }
                z += 4.0;
            }
            x += 4.0;
        }
        Ok(())
    }

    /// How far `p` is from the line `pts`.
    fn wash_near_line(&self, pts: &[Vec2], p: Vec2) -> f32 {
        line_distance(pts, p)
    }

    /// Where along the wash it's nearest `x` going east (its first point
    /// there), as metres along it.
    fn wash_at_x(&self, x: f32) -> f32 {
        let mut acc = 0.0;
        for w in self.wash.windows(2) {
            let len = w[0].distance(w[1]);
            if w[1].x >= x {
                let t = ((x - w[0].x) / (w[1].x - w[0].x)).clamp(0.0, 1.0);
                return acc + t * len;
            }
            acc += len;
        }
        acc
    }

    /// The wash's line `s` m along it, its way along, and its normal to the
    /// right of that (south, on its west-east run).
    fn wash_frame(&self, s: f32) -> (Vec2, Vec2, Vec2) {
        let (p, t) = line_at(&self.wash, s);
        (p, t, Vec2::new(t.y, -t.x))
    }

    /// The railroad, the bridge over the wash, the branch and warehouse,
    /// and what's in the wash by the bridge: the seam, the rig, the ramps.
    fn lay_track(&mut self, rng: &mut Rng) -> Result<(), u32> {
        let s = self.size;
        let x = self.adit.0.x + 6.0;
        let mouth = Vec2::new(x, self.face(x) + 1.0);
        let a = rng.range(0.12, 0.38);
        let dir = Vec2::new(a.sin(), -a.cos());
        // Where the line crosses the wash: nearest its middle.
        let (mut best, mut cross) = (f32::INFINITY, mouth);
        for k in 0..400 {
            let p = mouth + dir * (k as f32 * 0.25);
            let (d, _) = self.wash_near(p);
            if d < best {
                best = d;
                cross = p;
            }
        }
        if best > 1.0 || cross.distance(mouth) < 14.0 {
            return Err(line!());
        }
        let (_, along) = self.wash_near(cross);
        self.bridge_s = along;
        let (_, t, _) = self.wash_frame(along);
        if t.dot(dir).abs() > 0.6 {
            return Err(line!());
        }
        let end = cross + dir * 18.0;
        let mut track = vec![Vec2::new(x, s - 4.0), mouth, end];
        let mut at = end + dir * rng.range(11.0, 14.0);
        track.push(at);
        let mut d = dir;
        for (turn, len) in [(0.12, (28.0, 34.0)), (0.15, (36.0, 44.0))] {
            d = Vec2::from_angle(rng.range(-turn, turn)).rotate(d);
            at += d * rng.range(len.0, len.1);
            track.push(at);
        }
        d = Vec2::from_angle(rng.range(-0.15, 0.15)).rotate(d);
        at += d * ((at.y + 6.0) / -d.y).max(10.0);
        track.push(at);
        if track[2..].iter().any(|p| p.x < 50.0 || p.x > s - 40.0) {
            return Err(line!());
        }
        self.track = track;
        // The warehouse's branch, off one side or the other.
        let first = if rng.chance(0.5) { 1.0 } else { -1.0 };
        let mut placed = false;
        'sides: for side in [first, -first] {
            for junction in [14.0, 20.0, 9.0] {
                self.lay_branch(side, junction);
                if self.shed_fits() {
                    placed = true;
                    break 'sides;
                }
            }
        }
        if !placed {
            return Err(line!());
        }
        // In the wash by the bridge: the seam along its floor, the rig
        // cutting it short of the bridge, the Hold site by them, the ramps.
        let b = self.bridge_s;
        self.seam = (0..=24)
            .map(|k| {
                let u = b - 43.0 + 70.0 * k as f32 / 24.0;
                let (p, _, n) = self.wash_frame(u);
                p + n * (1.6 * (u * 0.21).sin() - 1.0)
            })
            .collect();
        let (p, t, n) = self.wash_frame(b - 16.0);
        self.wash_rig = (p - n * 3.5, yaw_along(t));
        self.hold = self.wash_frame(b - 25.0).0;
        // A ramp's top is out on the flat, in from the edge (not up the
        // scree, not under the Face).
        let flat = |l: &Layout, p: Vec2| {
            p.min_element() > 8.0
                && p.max_element() < s - 8.0
                && l.mesa(p) < 0.02
                && p.y < l.face(p.x) - 5.0
        };
        // The first of a few places along the wash (from `u` m along it, on
        // in steps of `step`) where `ramp` lays a ramp out on the flat.
        let first = |l: &Layout, u: f32, step: f32, ramp: &dyn Fn(Vec2, Vec2, Vec2) -> Ramp| {
            (0..6).find_map(|k| {
                let (w, t, n) = l.wash_frame(u + step * k as f32);
                let r = ramp(w, t, n);
                flat(l, r.0).then_some(r)
            })
        };
        let ramp0 = first(self, b - rng.range(24.0, 30.0), -3.0, &|w, t, n| {
            (w + n * 20.0, w, 5.0)
        })
        .ok_or(line!())?;
        // Up the north bank slantwise, toward the east landing.
        let ramp1 = first(self, b + rng.range(24.0, 30.0), 3.0, &|w, t, n| {
            (w - n * 17.0 + t * 9.0, w, 5.0)
        })
        .ok_or(line!())?;
        // On the east arm: a crossing near the east edge, a ramp short of it
        // up into the camp's side.
        // Where it runs most nearly south out there.
        let total = line_len(&self.wash, self.wash.len() - 1);
        let (mut e, mut best) = (0.0, f32::INFINITY);
        let mut u = 0.0;
        while u < total {
            let (p, t, _) = self.wash_frame(u);
            if (s - 52.0..s - 22.0).contains(&p.x) && p.y > 24.0 && t.y < best {
                (e, best) = (u, t.y);
            }
            u += 1.0;
        }
        if best > -0.4 {
            // The arm must run across the map there, not along its edge.
            return Err(line!());
        }
        let (w, _, n) = self.wash_frame(e);
        self.fords[1] = (w - n * 24.0, w + n * 24.0, 6.0);
        let ford = self.fords[1].0;
        // The way up the scree, at `x` across it.
        let scree_way = |l: &Layout, x: f32| {
            let f = l.face(x);
            (
                Vec2::new(x, f - SCREE_RUN - 6.0),
                Vec2::new(x, f + FACE_RISE + 16.0),
            )
        };
        // Where across the scree its way is furthest from `r`, and how far.
        let (x0, x1) = self.scree;
        let best_x = |l: &Layout, r: Ramp| {
            (0..=10)
                .map(|k| x0 + 10.0 + (x1 - x0 - 20.0) * k as f32 / 10.0)
                .map(|x| {
                    let (a, b) = scree_way(l, x);
                    (x, seg(r.0, a, b).0.min(seg(r.1, a, b).0))
                })
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .unwrap()
        };
        // Up the camp's bank, or failing that the other: clear of the
        // crossing, of the other ramp up that bank (where the wash bends
        // sharp they'd meet), and of the way up the scree.
        let start = rng.range(38.0, 42.0);
        let (ramp2, x) = [-1.0f32, 1.0]
            .iter()
            .flat_map(|&side| (0..8).map(move |k| (side, e - start + 3.0 * k as f32)))
            .find_map(|(side, u)| {
                let (w, _, n) = self.wash_frame(u);
                let r = (w + n * 20.0 * side, w, 5.0);
                let clear = flat(self, r.0)
                    && r.0.distance(ford) > 16.0
                    && r.0.distance(self.fords[1].1) > 16.0
                    && seg(r.0, ramp1.0, ramp1.1).0 > 14.0
                    && seg(r.1, ramp1.0, ramp1.1).0 > 14.0;
                let (x, d) = best_x(self, r);
                (clear && d > 12.0).then_some((r, x))
            })
            .ok_or(line!())?;
        self.ramps = [ramp0, ramp1, ramp2];
        // The worn way up the scree, starting on the flat short of its
        // foot, out of the wash.
        let (a, b) = scree_way(self, x);
        let start = (0..=6)
            .map(|k| a.y + k as f32)
            .find(|&z| self.wash_near(Vec2::new(x, z)).0 > 15.0)
            .ok_or(line!())?;
        self.scree_x = x;
        self.ways_up[1] = (Vec2::new(x, start), b);
        // Its north end short of the Face's foot, however near the wash
        // runs to it there.
        let (w, _, n) = self.wash_frame(self.wash_at_x(34.0));
        let north = (0..16)
            .map(|k| 23.5 - k as f32 * 0.5)
            .find(|&k| {
                // Room to drive onto it under the rock (the Face stands
                // out a few meters from its foot).
                let p = w - n * k;
                p.y < self.face_range(p.x - 6.0, p.x + 6.0).0 - 10.0 && self.mesa(p) < 0.02
            })
            .ok_or(line!())?;
        self.fords[0] = (w + n * 23.5, w - n * north, 6.0);
        // In from the edge, on the flat (not up the scree or the mesa).
        let inside =
            |p: Vec2| p.min_element() > 8.0 && p.max_element() < s - 8.0 && self.mesa(p) < 0.02;
        if !self
            .ramps
            .iter()
            .chain(&self.fords)
            .all(|&(a, b, _)| inside(a) && inside(b))
        {
            return Err(line!());
        }
        Ok(())
    }

    /// The branch line `junction` m past the track's fifth point, swinging
    /// off to the left of the way in (`side` 1) or the right (-1).
    fn lay_branch(&mut self, side: f32, junction: f32) {
        self.junction = junction;
        self.branch_side = side;
        let (j, dir) = self.branch_from();
        let back = -dir;
        let centre = j + back.perp() * BRANCH_R * side;
        let turn = Vec2::from_angle(BRANCH_TURN * side);
        let n = 12;
        let mut line: Vec<Vec2> = (0..=n)
            .map(|k| {
                centre
                    + Vec2::from_angle(BRANCH_TURN * side * k as f32 / n as f32).rotate(j - centre)
            })
            .collect();
        let end = centre + turn.rotate(j - centre);
        let along = turn.rotate(back);
        let at = end + along * (SHED_LEAD + SHED_HALF.y);
        line.push(at - along * SHED_HALF.y);
        line.push(at + along * SHED_BUFFER);
        self.branch = line;
        self.shed = (at, along);
    }

    /// The warehouse stands on the flats, its doors' run-outs too: in from
    /// the edge, clear of the wash.
    fn shed_fits(&self) -> bool {
        let s = self.size;
        let (at, along) = self.shed;
        let across = along.perp();
        (0..=8).all(|i| {
            (0..=8).all(|k| {
                let u = (i as f32 / 4.0 - 1.0) * (SHED_HALF.x + 14.0);
                let v = (k as f32 / 4.0 - 1.0) * (SHED_HALF.y + 14.0);
                let p = at + across * u + along * v;
                p.min_element() > 6.0 && p.max_element() < s - 6.0 && self.wash_near(p).0 > 18.0
            })
        })
    }

    /// The spawns, the yard and camp, and where the stands and repair
    /// drops are wanted.
    fn place_sides(&mut self, rng: &mut Rng) -> Result<(), u32> {
        let s = self.size;
        // The west side south of the wash's west arm.
        let z = self.wash_frame(self.wash_at_x(18.0)).0.y - 55.0;
        if z < 16.0 {
            return Err(line!());
        }
        self.spawns[0] = (vec![11.0, 18.0, 25.0], z, 7);
        self.yard = Vec2::new(37.0, z + 14.0);
        // The east side north of its east arm, south of the Face.
        let z = self.wash_frame(self.wash_at_x(s - 14.0)).0.y + 39.0;
        let top = self.face(s - 14.0).min(self.face(s - 11.0)) - 8.0;
        let rows = (((top - z) / 6.0).floor() as usize + 1).min(10);
        if rows < 7 {
            return Err(line!());
        }
        self.spawns[1] = (vec![s - 11.0, s - 18.0], z, rows);
        self.camp = Vec2::new(s - 28.0, z + 18.0);
        let mid = |k: usize| {
            let (xs, z, rows) = &self.spawns[k];
            let x = xs.iter().sum::<f32>() / xs.len() as f32;
            Vec2::new(x, z + (*rows as f32 - 1.0) * 3.0)
        };
        self.stands = [
            mid(0) + Vec2::new(38.0, 14.0),
            mid(1) + Vec2::new(-24.0, -5.0),
        ];
        let f = |x: f32| Vec2::new(x, self.face(x) - 15.0);
        let x = self.track[3].x;
        self.repairs = vec![
            f(self.west_adit.0.x - 20.0),
            f(self.breach.1.x + 6.0),
            Vec2::new(x - rng.range(45.0, 55.0), rng.range(70.0, 85.0)),
            Vec2::new(x + rng.range(26.0, 34.0), rng.range(62.0, 76.0)),
            Vec2::new(x - rng.range(25.0, 35.0), rng.range(30.0, 45.0)),
            Vec2::new(x + rng.range(25.0, 35.0), rng.range(25.0, 40.0)),
            f(self.west_adit.0.x - 36.0),
            f(self.breach.1.x + 20.0),
        ];
        Ok(())
    }

    /// Room on the flats for a thing `r` across at `p`: off the wash, the
    /// line, the warehouse, the spawns and camps, the ramps and crossings,
    /// out from under the Face, in from the edge.
    fn flats_free(&self, p: Vec2, r: f32) -> bool {
        let s = self.size;
        let spawn = |k: usize| {
            let (xs, z, rows) = &self.spawns[k];
            let (x0, x1) = (
                xs.iter().copied().fold(f32::INFINITY, f32::min),
                xs.iter().copied().fold(f32::NEG_INFINITY, f32::max),
            );
            rect_distance(
                (
                    Vec2::new(x0, *z),
                    Vec2::new(x1, z + (*rows as f32 - 1.0) * 6.0),
                ),
                p,
            )
        };
        let (at, along) = self.shed;
        let l = Vec2::new((p - at).dot(along.perp()), (p - at).dot(along)).abs() - SHED_HALF;
        p.min_element() > r + 8.0
            && p.max_element() < s - r - 8.0
            && self.wash_near(p).0 > r + 14.0
            && p.y < self.face(p.x) - r - 24.0
            && line_distance(&self.track, p) > r + 8.0
            && line_distance(&self.branch, p) > r + 8.0
            && l.max(Vec2::ZERO).length() > r + 12.0
            && spawn(0) > r + 10.0
            && spawn(1) > r + 10.0
            && p.distance(self.yard) > r + 10.0
            && p.distance(self.camp) > r + 12.0
            && self
                .ramps
                .iter()
                .chain(&self.fords)
                .all(|&(a, b, h)| seg(p, a, b).0 > r + h + 6.0)
            && self.ways_up.iter().all(|&(a, b)| seg(p, a, b).0 > r + 8.0)
    }

    /// The dump and its heap on one side of the line, fins and hoodoos on
    /// the flats, all on free ground.
    fn place_flats(&mut self, rng: &mut Rng) -> Result<(), u32> {
        let s = self.size;
        let mut found = None;
        for _ in 0..400 {
            let c = Vec2::new(rng.range(30.0, s - 30.0), rng.range(24.0, 90.0));
            let (r, run) = (rng.range(13.0, 16.0), 11.0);
            // (Its slopes run on out past its lobes: well clear of the
            // warehouse's doors.)
            if self.flats_free(c, (r + run) * 0.6) && self.shed_distance(c) > r * 1.3 + run + 6.0 {
                found = Some((c, r, rng.range(4.0, 5.0), run));
                break;
            }
        }
        self.dump = found.ok_or(line!())?;
        let c = self.dump.0;
        let mut found = None;
        for _ in 0..100 {
            let a = rng.range(0.0, TAU);
            let p = c + Vec2::new(a.cos(), a.sin()) * rng.range(30.0, 42.0);
            if self.flats_free(p, 8.0) {
                found = Some((p, 6.0, 3.0, 8.0));
                break;
            }
        }
        self.heap = found.ok_or(line!())?;
        let clear_of_heaps = |l: &Layout, p: Vec2, r: f32| {
            p.distance(l.dump.0) > l.dump.1 + l.dump.3 + r + 6.0
                && p.distance(l.heap.0) > l.heap.1 + l.heap.3 + r + 4.0
        };
        let want = rng.int(3, 5) as usize;
        for _ in 0..300 {
            if self.fins.len() >= want {
                break;
            }
            let a = Vec2::new(rng.range(14.0, s - 14.0), rng.range(10.0, 120.0));
            let dir = Vec2::from_angle(rng.range(0.0, PI));
            let len = rng.range(10.0, 22.0);
            let b = a + dir * len;
            let ok = (0..=4).all(|k| {
                let p = a.lerp(b, k as f32 / 4.0);
                self.flats_free(p, 4.0) && clear_of_heaps(self, p, 4.0)
            }) && self
                .fins
                .iter()
                .all(|&(c, d, _)| seg(a, c, d).0 > 16.0 && seg(b, c, d).0 > 16.0);
            if ok {
                self.fins.push((a, b, rng.range(2.4, 3.8)));
            }
        }
        if self.fins.len() < 3 {
            return Err(line!());
        }
        let want = rng.int(5, 7) as usize;
        for _ in 0..300 {
            if self.hoodoos.len() >= want {
                break;
            }
            let p = Vec2::new(rng.range(14.0, s - 14.0), rng.range(10.0, 120.0));
            let ok = self.flats_free(p, 3.0)
                && clear_of_heaps(self, p, 3.0)
                && self.fins.iter().all(|&(a, b, _)| seg(p, a, b).0 > 8.0)
                && self.hoodoos.iter().all(|&(q, _)| q.distance(p) > 14.0);
            if ok {
                self.hoodoos.push((p, rng.range(5.5, 9.0)));
            }
        }
        Ok(())
    }

    /// Places bots head for: across the flats and the landing under the
    /// Face, along the wash, in the warehouse, the mine and up on the mesa.
    fn find_spots(&self) -> Vec<Vec2> {
        let lay = self;
        let s = lay.size;
        let mut spots = Vec::new();
        let mut z = 14.0;
        while z < s - 8.0 {
            let mut x = 18.0 + (z * 0.37).sin() * 6.0;
            while x < s - 12.0 {
                let p = Vec2::new(x, z);
                let below = p.y < lay.face(p.x) - 6.0 && lay.wash_near(p).0 > 12.0;
                let above =
                    p.y > lay.face(p.x) + 14.0 && lay.mesa(p) > 0.95 && lay.crag_depth(p) < -6.0;
                if below || above {
                    spots.push(p);
                }
                x += 32.0;
            }
            z += 30.0;
        }
        let total = line_len(&lay.wash, lay.wash.len() - 1);
        let mut u = 10.0;
        while u < total {
            let (p, _) = line_at(&lay.wash, u);
            if p.min_element() > 22.0 && p.max_element() < s - 22.0 {
                spots.push(p);
            }
            u += 28.0;
        }
        let mid = |r: Rect| (r.0 + r.1) * 0.5;
        spots.push(lay.shed.0);
        spots.push(Vec2::new(mid(lay.adit).x, lay.bay.0.y + 10.0));
        spots.push(mid(lay.hall));
        spots.push(mid(lay.drift));
        spots.push(Vec2::new(mid(lay.breach).x, lay.drift.0.y - 14.0));
        spots
    }

    pub fn center(&self) -> Vec2 {
        Vec2::splat(self.size * 0.5)
    }

    /// The Face's foot at `x` (z).
    pub fn face(&self, x: f32) -> f32 {
        // (Straight where it fell in: the scree's slope mustn't lean.)
        let wobble = 0.8 * (0.5 * x).sin() * (0.13 * x).sin() * (1.0 - self.scree(x));
        for w in self.face.windows(2) {
            let ((x0, z0), (x1, z1)) = (w[0], w[1]);
            if x <= x1 {
                let t = smoothstep(x0, x1, x);
                return z0 + (z1 - z0) * t + wobble;
            }
        }
        self.face[self.face.len() - 1].1
    }

    /// How much of the scree (0..1) there is at `x`, in place of the cliff.
    pub fn scree(&self, x: f32) -> f32 {
        smoothstep(self.scree.0 - 4.0, self.scree.0 + 6.0, x)
            * (1.0 - smoothstep(self.scree.1 - 6.0, self.scree.1 + 4.0, x))
    }

    /// The Crag's west side (x), at `z`.
    pub fn crag_west(&self, z: f32) -> f32 {
        let z = z.min(self.size);
        // Flaring west as it runs back, into the massif beyond the edge.
        self.crag_w - self.flare * smoothstep(self.flare_z, self.flare_z + 45.0, z)
            + 1.6 * (z * 0.13 + self.phase).sin()
            + 1.0 * (z * 0.31).sin()
    }

    /// The Crag's east side (x), at `z`.
    pub fn crag_east(&self, z: f32) -> f32 {
        let z = z.min(self.size);
        self.crag_e + 1.5 * (z * 0.11 + 1.0 + self.phase).sin() + 0.8 * (z * 0.27).sin()
    }

    /// How far into the Crag a point is (m; negative: outside). Its front
    /// rises behind the mesa's own face.
    pub fn crag_depth(&self, p: Vec2) -> f32 {
        (p.x - self.crag_west(p.y))
            .min(self.crag_east(p.y) - p.x)
            .min(p.y - self.face(p.x) - FACE_RISE)
    }

    /// How much of the Crag (0..1) stands over a point.
    pub fn crag(&self, p: Vec2) -> f32 {
        smoothstep(0.0, 3.0, self.crag_depth(p))
    }

    /// Feet of the Crag's face further back than this (z) run up its
    /// sides, not along its front.
    pub fn front_limit(&self) -> f32 {
        self.bay.0.y + 10.0
    }

    /// How high the mesa stands at a point, before the mine is cut out: 0..1
    /// of `TOP`.
    pub fn mesa(&self, p: Vec2) -> f32 {
        let f = self.face(p.x);
        let cliff = smoothstep(f, f + FACE_RISE, p.y);
        let slope = ((p.y - (f - SCREE_RUN)) / (SCREE_RUN + FACE_RISE)).clamp(0.0, 1.0);
        let s = self.scree(p.x);
        let across = cliff + (slope - cliff) * s;
        let (w0, w1) = self.west_ramp;
        let west = ((p.x - w0) / (w1 - w0)).clamp(0.0, 1.0);
        across * west
    }

    /// Distance from a point to the nearest cave (0 inside one).
    pub fn cave_distance(&self, p: Vec2) -> f32 {
        self.caves
            .iter()
            .map(|&r| rect_distance(r, p))
            .fold(f32::INFINITY, f32::min)
    }

    pub fn in_cave(&self, p: Vec2) -> bool {
        self.caves.iter().any(|&r| within(r, p, 0.0))
    }

    /// The Crag's mouths in its face: the rail adit and the new heading.
    pub fn mouths(&self) -> [Rect; 2] {
        [self.adit, self.heading]
    }

    /// The box cut's floor at `x`: from the adit's floor up to the mesa's top.
    pub fn box_cut_floor(&self, x: f32) -> f32 {
        let (a, b) = self.box_cut;
        TOP * ((x - a.x - 2.0) / (b.x - a.x - 4.0)).clamp(0.0, 1.0)
    }

    /// Rock: under the mesa, back from its foot (not out in front of the Face).
    pub fn in_rock(&self, p: Vec2) -> bool {
        self.mesa(p) > 0.5 && !self.in_cave(p) && !within(self.box_cut, p, 0.0)
    }

    /// Where the wash is nearest `p`: (distance from its line, metres along it
    /// from its west end).
    pub fn wash_near(&self, p: Vec2) -> (f32, f32) {
        let (mut best, mut at, mut acc) = (f32::INFINITY, 0.0, 0.0);
        for w in self.wash.windows(2) {
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
    /// narrows and widens as it winds.
    pub fn wash_profile(&self, s: f32) -> (f32, f32) {
        (WASH_DEPTH, 6.5 + 1.5 * (0.08 * s + self.wash_phase).sin())
    }

    pub fn wash(&self, p: Vec2) -> f32 {
        let (d, s) = self.wash_near(p);
        let (depth, half) = self.wash_profile(s);
        -depth * (1.0 - smoothstep(half, half + WASH_BANK, d))
    }

    /// The ground with the haul ramps and the crossings cut into the wash.
    fn with_ramps(&self, p: Vec2, g: f32) -> f32 {
        let mut g = g;
        for (s, e, half) in self.ramps {
            let len = s.distance(e);
            let u = (e - s) / len;
            let rel = p - s;
            let (a, c) = (rel.dot(u), rel.perp_dot(u).abs());
            if a < -2.0 || a > len + 6.0 || c > half + 3.0 {
                continue;
            }
            let floor = self.wash(e);
            let h = floor * (a / len).clamp(0.0, 1.0);
            let k = (1.0 - smoothstep(half, half + 2.5, c))
                * (1.0 - smoothstep(len + 2.0, len + 6.0, a));
            g += (h - g) * k;
        }
        for (s, e, half) in self.fords {
            let len = s.distance(e);
            let u = (e - s) / len;
            let rel = p - s;
            let (a, c) = (rel.dot(u), rel.perp_dot(u).abs());
            if a < -3.0 || a > len + 3.0 || c > half + 6.0 {
                continue;
            }
            // Down from either top to the floor, flat across the floor; its
            // sides laid back, not cut sheer.
            let floor = self.wash(s.lerp(e, 0.5));
            let h = floor * (a.min(len - a) / FORD_RUN).clamp(0.0, 1.0);
            let k = (1.0 - smoothstep(half, half + 6.0, c))
                * smoothstep(-3.0, 0.0, a)
                * smoothstep(-3.0, 0.0, len - a);
            g += (h - g) * k;
        }
        g
    }

    pub fn fins(&self, p: Vec2) -> f32 {
        self.fins
            .iter()
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
    pub fn track_distance(&self, p: Vec2) -> f32 {
        self.main_line_distance(p)
            .min(line_distance(&self.branch, p))
    }

    /// How near the main line.
    pub fn main_line_distance(&self, p: Vec2) -> f32 {
        line_distance(&self.track, p)
    }

    /// Position and heading `s` m along the track.
    pub fn track_at(&self, s: f32) -> (Vec2, Vec2) {
        line_at(&self.track, s)
    }

    /// Length of the track up to its point `k`.
    pub fn track_len(&self, k: usize) -> f32 {
        line_len(&self.track, k)
    }

    /// Where the branch leaves the main line, and the main line's way there.
    pub fn branch_from(&self) -> (Vec2, Vec2) {
        self.track_at(self.track_len(4) + self.junction)
    }

    /// The bridge is the track's second leg.
    pub fn bridge(&self) -> (Vec2, Vec2) {
        (self.track[1], self.track[2])
    }

    /// Over the bridge's deck (not its end plates).
    pub fn on_bridge(&self, p: Vec2) -> bool {
        let (a, b) = self.bridge();
        let (d, t) = seg(p, a, b);
        d < DECK_HALF && t > 0.0 && t < 1.0
    }

    /// A point in the warehouse's own frame: (across, along) from its middle,
    /// +across to the left of the way in.
    pub fn shed_local(&self, p: Vec2) -> Vec2 {
        let (at, along) = self.shed;
        let d = p - at;
        Vec2::new(d.dot(Vec2::new(along.y, -along.x)), d.dot(along))
    }

    /// How far outside the warehouse's walls (0 inside).
    pub fn shed_distance(&self, p: Vec2) -> f32 {
        let q = self.shed_local(p).abs() - SHED_HALF;
        q.max(Vec2::ZERO).length()
    }

    /// The ground straight out in front of the warehouse's doors, at the sides
    /// and the ends: room to drive out before there's anything in the way.
    pub fn shed_doorways(&self, p: Vec2) -> bool {
        let l = self.shed_local(p).abs();
        (l.y < 7.0 && l.x < SHED_HALF.x + 16.0) || (l.x < 7.0 && l.y < SHED_HALF.y + 16.0)
    }

    /// Each side's spawns, as laid out (before any that aren't clear are
    /// dropped).
    pub fn spawn_points(&self, side: usize) -> Vec<Vec2> {
        let (xs, z0, rows) = &self.spawns[side];
        xs.iter()
            .flat_map(|&x| (0..*rows).map(move |k| Vec2::new(x, z0 + k as f32 * 6.0)))
            .collect()
    }

    /// Ground kept clear round each side's spawns.
    pub fn by_spawns(&self, p: Vec2) -> bool {
        let rows = |side: usize| {
            let (_, z0, rows) = &self.spawns[side];
            *z0..z0 + (*rows as f32 - 1.0) * 6.0
        };
        let (w, e) = (rows(0), rows(1));
        (p.x < 31.0 && (w.start - 6.0..w.end + 8.0).contains(&p.y))
            || p.distance(self.yard) < 9.0
            || (p.x > self.size - 32.0 && (e.start - 6.0..e.end + 8.0).contains(&p.y))
            || p.distance(self.camp) < 12.0
    }

    /// Play ground: dunes on the flats, the mesa along the north with the mine
    /// cut out under it, the wash across under it, the dump and the fins.
    pub fn ground(&self, x: f32, z: f32) -> f32 {
        self.surface(x, z, true)
    }

    /// The rock the mine was cut from: the ground as if there were no caves
    /// under the mesa. Their roofs are drawn to it (`roof_lid`), and the ground
    /// round them is shaded by it.
    pub fn uncut(&self, p: Vec2) -> f32 {
        self.surface(p.x, p.y, false)
    }

    fn surface(&self, x: f32, z: f32, caves: bool) -> f32 {
        let p = Vec2::new(x, z);
        let m = self.mesa(p);
        let dunes = 0.45 * (0.061 * x + 0.3).sin() * (0.047 * z).cos()
            + 0.25 * (0.13 * x - 0.09 * z).sin()
            + 0.15 * (0.21 * z + 0.4 * (0.05 * x).sin()).sin();
        // Flat along the railroad, under the warehouse, on the mesa, and
        // graded round the haul ramps and crossings.
        let shed = smoothstep(1.0, 8.0, self.shed_distance(p));
        let graded = self
            .ramps
            .iter()
            .chain(&self.fords)
            .map(|&(a, b, h)| smoothstep(h + 2.0, h + 8.0, seg(p, a, b).0))
            .fold(1.0, f32::min);
        let (a, b) = self.ways_up[0];
        let road = smoothstep(4.0, 10.0, seg(p, a, b).0);
        let flat = smoothstep(3.0, 9.0, self.track_distance(p)) * shed * graded * road * (1.0 - m);
        // The mesa's top rolls a little, away from the mine's roofs and back
        // from the lip.
        let f = self.face(x);
        let cave = self.cave_distance(p);
        let by_cut = rect_distance(self.box_cut, p);
        // (Graded flat along the haul road up the west end.)
        let (a, b) = self.ways_up[0];
        let road = smoothstep(5.0, 11.0, seg(p, a, b).0);
        let relief = (1.8 * (0.09 * x + 0.7).sin() * (0.07 * z).cos()
            + 0.6 * (0.21 * x - 0.17 * z).sin())
        .max(-0.6)
            * road
            * smoothstep(5.0, 14.0, cave)
            * smoothstep(f + FACE_RISE + 1.0, f + FACE_RISE + 9.0, z)
            * smoothstep(4.0, 10.0, by_cut)
            * m.powi(4);
        // The Crag over it, its top rolling too (flat over the mine's roofs).
        let c = self.crag(p);
        let crag_relief = (1.0 * (0.05 * x + 1.3).sin() * (0.06 * z + 0.4).cos()
            + 0.5 * (0.17 * x + 0.11 * z).sin())
            * smoothstep(5.0, 14.0, cave)
            * smoothstep(4.0, 10.0, by_cut)
            * smoothstep(3.0, 8.0, self.crag_depth(p));
        let relief = relief * (1.0 - c) + crag_relief;
        let cut = if caves {
            smoothstep(0.0, 2.0, cave)
        } else {
            1.0
        };
        let mut rock = (TOP * m + (CRAG_TOP - TOP) * c + relief) * cut;
        // The box cut, down through the rock.
        let (a, b) = self.box_cut;
        rock =
            rock.min(self.box_cut_floor(x.clamp(a.x, b.x)) + (rock * smoothstep(0.0, 2.0, by_cut)));
        let g = dunes * flat
            + rock
            + self.wash(p)
            + (heap(self.dump, p) + heap(self.heap, p)) * shed
            + self.fins(p);
        self.with_ramps(p, g)
    }

    /// Desert out past the edge: the mesa runs on north, the floor rises into
    /// broken hills.
    pub fn scenery_ground(&self, x: f32, z: f32) -> f32 {
        let size = self.size;
        let p = Vec2::new(x, z);
        let q = (p - self.center()).abs() - Vec2::splat(size * 0.5);
        let d = q.max(Vec2::ZERO).length() + q.x.max(q.y).min(0.0);
        if d < -0.01 {
            return -10.0;
        }
        // The edge's ground carried on out, the mesa solid (its adit runs on
        // under the rock).
        let edge = p.clamp(Vec2::ZERO, Vec2::splat(size));
        let q = Vec2::new(edge.x, z.max(edge.y));
        let base = self
            .ground(edge.x, edge.y)
            .max(TOP * self.mesa(q) + (CRAG_TOP - TOP) * self.crag(q));
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

    /// Under the Crag's rock (a roof there is the Crag's, up to its top).
    pub fn under_crag(&self, p: Vec2) -> bool {
        self.crag(p) > 0.5
    }

    /// Under a cave roof (or on top of one): within the walls' reach of a
    /// cave, back from the Face's lip, not in the box cut.
    pub fn roofed(&self, p: Vec2) -> bool {
        self.cave_distance(p) < CAVE_WALL + 0.5
            && self.mesa(Vec2::new(p.x, p.y - 1.0)) > 0.9
            && !within(self.box_cut, p, 0.0)
            && p.y < self.size + 6.0
    }

    /// A high room's roof over a point in it (not out in its walls), if it's
    /// in one: the bay, and the hall and crosscut where the Crag is deep over
    /// them (not where the mesa's top is just over the roof).
    pub fn high_roof(&self, p: Vec2) -> Option<f32> {
        if within(self.bay, p, 0.0) {
            Some(BAY_ROOF)
        } else if (within(self.hall, p, 0.0) || within(self.crosscut, p, 0.0))
            && self.crag_depth(p) > 4.0
        {
            Some(HALL_ROOF)
        } else {
            None
        }
    }

    /// A roof slab's underside over 2 m cell `c`: a high room's, as high in the
    /// rock round one, the usual elsewhere.
    pub fn slab_under(&self, c: Vec2) -> f32 {
        if let Some(h) = self.high_roof(c) {
            return h;
        }
        if !self.in_cave(c) {
            let mut best: Option<f32> = None;
            for dz in -1..=1 {
                for dx in -1..=1 {
                    let q = c + Vec2::new(dx as f32, dz as f32) * 2.0;
                    if let Some(h) = self.high_roof(q) {
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
    pub fn drawn_roof(&self, p: Vec2) -> f32 {
        let under = ROOF.0 - 0.08;
        let target = if rect_distance(self.bay, p) < 3.0 {
            BAY_ROOF
        } else if rect_distance(self.hall, p) < 3.0 || rect_distance(self.crosscut, p) < 3.0 {
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
                if self.in_cave(c) && self.high_roof(c).is_none() {
                    d = d.min(rect_distance((a, a + Vec2::splat(cell)), p));
                }
            }
        }
        under + (target - ROOF.0) * smoothstep(0.0, 3.5, d)
    }

    /// The span of ground the mine's roofs and lining are worked over:
    /// round every cave, from in front of the Face to past the north edge.
    pub fn mine_bounds(&self) -> Rect {
        let lo = self
            .caves
            .iter()
            .fold(Vec2::splat(f32::INFINITY), |m, r| m.min(r.0));
        let hi = self
            .caves
            .iter()
            .fold(Vec2::splat(f32::NEG_INFINITY), |m, r| m.max(r.1));
        (
            Vec2::new(even_down(lo.x - 6.0), even_down(lo.y - 4.0)),
            Vec2::new(even(hi.x + 6.0), self.size + 6.0),
        )
    }
}

/// A heap (the dump): flat-topped, its outline lobed.
pub fn heap((c, r, h, run): Heap, p: Vec2) -> f32 {
    if r <= 0.0 {
        return 0.0;
    }
    let d = p - c;
    let a = d.y.atan2(d.x);
    let edge = r + 0.25 * r * (3.0 * a + c.x).sin() + 0.12 * r * (5.0 * a + 1.0).sin();
    h * (1.0 - smoothstep(edge, edge + run, d.length()))
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
    /// The machine cutting the bore in the borer's bay, along `beam`.
    fn borer(&self, d: &mut Dig, rng: &mut Rng);
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
        Look::Imperium => &ithessar::Ithessar,
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
        match super::check::playable(&dig::greybox(&l, &spec.sides)) {
            Ok(()) => return l,
            Err(why) => log::debug!("mine seed {} attempt {attempt}: {why}", spec.seed),
        }
        last = Some(l);
    }
    last.expect("no crystal mine laid out in 64 draws")
}

/// Builds `spec`'s mine.
pub fn build(spec: &Spec) -> Map {
    let l = layout(spec);
    let mut m = dig::build(&l, spec, style(spec.look));
    if l.mirror {
        m.mirror_x();
    }
    m
}

/// `spec`'s mine undressed: its ground, the caves' walls and roofs, the
/// bridge's deck.
pub fn greybox(spec: &Spec) -> Map {
    let l = layout(spec);
    let mut m = dig::greybox(&l, &spec.sides);
    m.atmosphere = atmosphere(&mut Rng::new(spec.seed ^ 0x5EED_D2E5));
    if l.mirror {
        m.mirror_x();
    }
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
