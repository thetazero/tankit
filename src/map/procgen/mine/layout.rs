//! A crystal mine's layout, built up in layers from a seed:
//!
//! 1. **Landforms.** The escarpment along the north (the Face, the mesa
//!    behind it), one or two **mines** dug into it (each with its Crag over
//!    it and a scree beside it where the Face fell in), none to two
//!    **washes** (dry ravines) across the desert, none to two **buttes**
//!    (flat-topped rocks standing on the flats). The arena is sized to fit
//!    what's there: about 230 m for one mine, 330 m for two.
//! 2. **Lines.** A railroad out of each mine's adit to the south edge, a
//!    bridge wherever it crosses a wash; the warehouse on a branch.
//! 3. **Stitching.** Whatever the landforms cut off is joined up by rule: a
//!    haul ramp down into a wash either side of each bridge, two crossings
//!    over each wash, a way up each scree and each butte, the haul road up
//!    the mesa's west end. The greybox is checked for play (`check`).
//! 4. **Sides.** Each side's spawns on its edge, wherever the two come out
//!    fairest from what's worth taking (the mines, the bridges, the buttes).
//! 5. **Details.** The dump, fins and hoodoos on the free ground; the seam
//!    and its rig in the wash; where bots head, repair drops, Raid stands.
//!
//! Then which way round: the whole map is mirrored, and turned a quarter,
//! half or three quarters, so the escarpment can lie along any edge.

use std::f32::consts::{PI, TAU};

use glam::{Vec2, Vec3};

use super::*;

/// One mine dug into the escarpment: its Crag and rooms, every edge on the
/// terrain's 2 m grid so the cut is clean.
#[derive(Clone, Debug)]
pub struct Mine {
    /// The Crag's west and east sides at its front, how far it flares west
    /// as it runs back (from `flare_z`, over 45 m), and its wobble's phase.
    pub crag_w: f32,
    pub crag_e: f32,
    flare: f32,
    flare_z: f32,
    phase: f32,
    pub adit: Rect,
    pub hall: Rect,
    /// From the hall to the adit, where the cut crystal waits for the train.
    pub crosscut: Rect,
    /// From the hall out to the Face.
    pub west_adit: Rect,
    /// Along from the adit to the breach.
    pub drift: Rect,
    /// The other side's way in, from the Face to the drift.
    pub breach: Rect,
    /// The newest tunnel into the Crag, still being cut.
    pub heading: Rect,
    /// The borer's bay at the heading's end, high under the Crag.
    pub bay: Rect,
    /// The round tunnel it's cutting.
    pub bore: Rect,
    /// The borer's beam: from its emitter to the bore's face, and how thick.
    pub beam: (Vec3, Vec3, f32),
    /// An open trench climbing east from the adit's end to the mesa's top.
    pub box_cut: Rect,
    /// The hall's rock pillars.
    pub pillars: [Vec2; 4],
    /// The cutting rig working the hall, and which way its boom reaches.
    pub hall_rig: (Vec2, f32),
    /// The headframe over the hall's shaft, on the mesa.
    pub headframe: Vec2,
    /// The scree east of it, where the Face fell in (x), and the worn way
    /// up its middle (x).
    pub scree: (f32, f32),
    pub scree_x: f32,
}

impl Mine {
    pub fn caves(&self) -> [Rect; 9] {
        [
            self.adit,
            self.hall,
            self.crosscut,
            self.west_adit,
            self.drift,
            self.breach,
            self.heading,
            self.bay,
            self.bore,
        ]
    }

    /// Its mouths in the Crag's face: the rail adit and the new heading.
    pub fn mouths(&self) -> [Rect; 2] {
        [self.adit, self.heading]
    }

    /// The Crag's west side (x), at `z` (`top`: the north edge).
    pub fn crag_west(&self, z: f32, top: f32) -> f32 {
        let z = z.min(top);
        // Flaring west as it runs back, into the massif beyond the edge.
        self.crag_w - self.flare * smoothstep(self.flare_z, self.flare_z + 45.0, z)
            + 1.6 * (z * 0.13 + self.phase).sin()
            + 1.0 * (z * 0.31).sin()
    }

    /// The Crag's east side (x), at `z`.
    pub fn crag_east(&self, z: f32, top: f32) -> f32 {
        let z = z.min(top);
        self.crag_e + 1.5 * (z * 0.11 + 1.0 + self.phase).sin() + 0.8 * (z * 0.27).sin()
    }

    /// Feet of the Crag's face further back than this (z) run up its sides,
    /// not along its front.
    pub fn front_limit(&self) -> f32 {
        self.bay.0.y + 10.0
    }

    /// The box cut's floor at `x`: from the adit's floor up to the mesa's top.
    pub fn box_cut_floor(&self, x: f32) -> f32 {
        let (a, b) = self.box_cut;
        TOP * ((x - a.x - 2.0) / (b.x - a.x - 4.0)).clamp(0.0, 1.0)
    }

    /// A high room's roof over a point in it: the bay, and the hall and
    /// crosscut where the Crag is deep over them.
    fn high_roof(&self, p: Vec2, crag_depth: f32) -> Option<f32> {
        if within(self.bay, p, 0.0) {
            Some(BAY_ROOF)
        } else if (within(self.hall, p, 0.0) || within(self.crosscut, p, 0.0)) && crag_depth > 4.0 {
            Some(HALL_ROOF)
        } else {
            None
        }
    }
}

/// A dry ravine winding across the desert.
#[derive(Clone, Debug)]
pub struct Wash {
    /// Its line, edge to edge (its ends run on out past the map).
    pub line: Vec<Vec2>,
}

impl Wash {
    /// Distance from its line, and how far along it the nearest point is.
    pub fn near(&self, p: Vec2) -> (f32, f32) {
        let (mut best, mut at, mut acc) = (f32::INFINITY, 0.0, 0.0);
        for w in self.line.windows(2) {
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

    /// Its line `s` m along it, its way along, and the normal to the right
    /// of that.
    pub fn frame(&self, s: f32) -> (Vec2, Vec2, Vec2) {
        let (p, t) = line_at(&self.line, s);
        (p, t, Vec2::new(t.y, -t.x))
    }

    pub fn len(&self) -> f32 {
        line_len(&self.line, self.line.len() - 1)
    }

    /// The stretch of it inside an arena `size` across (from, to, along it).
    pub fn inside(&self, size: f32) -> (f32, f32) {
        let total = self.len();
        let mut lo = None;
        let mut hi = 0.0;
        let mut u = 0.0;
        while u < total {
            let (p, _) = line_at(&self.line, u);
            if p.min_element() > 4.0 && p.max_element() < size - 4.0 {
                lo.get_or_insert(u);
                hi = u;
            }
            u += 1.0;
        }
        (lo.unwrap_or(0.0), hi)
    }
}

/// A bridge carrying a line over a wash.
#[derive(Clone, Copy, Debug)]
pub struct Bridge {
    pub a: Vec2,
    pub b: Vec2,
    /// Which line, which wash, and how far along the wash it crosses.
    pub line: usize,
    pub wash: usize,
    pub s: f32,
}

/// A flat-topped rock standing on the flats, steep all round but for the
/// ramp up one side.
#[derive(Clone, Copy, Debug)]
pub struct Butte {
    pub center: Vec2,
    pub radius: f32,
    pub height: f32,
    /// The way up: out from the middle this way.
    pub ramp: Vec2,
    phase: f32,
}

/// How far a butte's ramp runs out from its rim (m per m of height): 15
/// degrees.
const BUTTE_RUN: f32 = 3.7;
/// Its ramp's half-width.
const BUTTE_RAMP: f32 = 4.5;

impl Butte {
    /// Its rim's distance from the middle, this way out.
    pub fn rim(&self, dir: Vec2) -> f32 {
        let a = dir.y.atan2(dir.x);
        self.radius * (1.0 + 0.16 * (3.0 * a + self.phase).sin() + 0.07 * (5.0 * a).sin())
    }

    /// Its ground's height at `p`.
    pub fn height_at(&self, p: Vec2) -> f32 {
        let d = p - self.center;
        let r = d.length();
        if r < 1e-3 {
            return self.height;
        }
        let rim = self.rim(d / r);
        let body = self.height * (1.0 - smoothstep(rim, rim + 2.2, r));
        // The ramp: down from the rim along `ramp`, its sides falling away.
        let along = d.dot(self.ramp);
        let across = d.perp_dot(self.ramp).abs();
        let rr = self.rim(self.ramp);
        let run = self.height * BUTTE_RUN;
        let ramp = if along > 0.0 {
            self.height
                * (1.0 - ((along - rr) / run).clamp(0.0, 1.0))
                * (1.0 - smoothstep(BUTTE_RAMP, BUTTE_RAMP + 2.5, across))
        } else {
            0.0
        };
        body.max(ramp)
    }

    /// Its way up: from the ramp's foot to the top.
    pub fn way(&self) -> (Vec2, Vec2) {
        let rr = self.rim(self.ramp);
        (
            self.center + self.ramp * (rr + self.height * BUTTE_RUN + 3.0),
            self.center + self.ramp * (rr * 0.3),
        )
    }

    /// Its outline's distance from `p` (negative inside), ramp and all.
    pub fn distance(&self, p: Vec2) -> f32 {
        let d = p - self.center;
        let r = d.length().max(1e-3);
        let body = r - self.rim(d / r) - 2.2;
        let (a, b) = self.way();
        body.min(seg(p, a, b).0 - BUTTE_RAMP - 2.5)
    }
}

/// A crystal mine, laid out: everything the ground and the build need.
#[derive(Clone, Debug)]
pub struct Layout {
    pub size: f32,
    /// The Face's foot, west to east (x, z), eased between.
    face: Vec<(f32, f32)>,
    /// The mesa's west end slopes down to the floor between these (x).
    pub west_ramp: (f32, f32),
    pub mines: Vec<Mine>,
    /// Every mine's rooms.
    pub caves: Vec<Rect>,
    pub washes: Vec<Wash>,
    /// The phase of the washes' floors narrowing and widening.
    wash_phase: f32,
    /// A railroad out of each mine: in the adit, out at its mouth, on south
    /// across the flats to the edge.
    pub lines: Vec<Vec<Vec2>>,
    pub bridges: Vec<Bridge>,
    /// The warehouse's branch off the first line, from the junction to the
    /// buffer stop: it leaves the line `junction` m past its fifth point, to
    /// the left of the way out (`branch_side` 1) or the right (-1).
    pub branch: Vec<Vec2>,
    pub junction: f32,
    pub branch_side: f32,
    /// The warehouse's middle, and its way along (from the rail door in).
    pub shed: (Vec2, Vec2),
    /// Haul ramps down into the washes: from the top to the floor.
    pub ramps: Vec<Ramp>,
    /// Crossings: down one bank and up the other, top to top.
    pub fords: Vec<Ramp>,
    pub buttes: Vec<Butte>,
    /// The seam's line along the first bridge's wash floor (if there's a
    /// wash under the Face).
    pub seam: Vec<Vec2>,
    /// The cutting rig working it (where, which way its boom reaches).
    pub wash_rig: Option<(Vec2, f32)>,
    pub dump: Heap,
    pub heap: Heap,
    pub fins: Vec<(Vec2, Vec2, f32)>,
    pub hoodoos: Vec<(Vec2, f32)>,
    /// Each side's spawns: their columns (x), the first row (z) and how
    /// many rows, 6 m apart.
    pub spawns: [(Vec<f32>, f32, usize); 2],
    /// The west side's yard by its spawns, and the east side's camp.
    pub yard: Vec2,
    pub camp: Vec2,
    /// The ways up, kept clear: the haul road up the mesa's west end, the
    /// way round the Face's west end to its foot, a way up each scree and
    /// each butte.
    pub ways_up: Vec<(Vec2, Vec2)>,
    /// The Hold site.
    pub hold: Vec2,
    /// Where repair drops are wanted, best first.
    pub repairs: Vec<Vec2>,
    /// Where each side's Raid stand is wanted.
    pub stands: [Vec2; 2],
    /// Places bots head for.
    pub spots: Vec<Vec2>,
    /// Built the other way round, east to west (`Map::mirror_x`), then
    /// turned this many quarter turns (`Map::turn`): the layout itself is
    /// always laid out with the escarpment along the north.
    pub mirror: bool,
    pub turns: u8,
}

impl Layout {
    /// A mine from `rng`, if this draw holds together (the caller draws
    /// again if not; the error is the line it failed at).
    pub fn generate(rng: &mut Rng) -> Result<Layout, u32> {
        let n_mines = if rng.chance(0.4) { 2 } else { 1 };
        let n_washes = [0, 1, 1, 1, 2, 2][rng.index(6)];
        let n_buttes = [0, 1, 1, 2][rng.index(4)];
        let mut l = Layout {
            size: 0.0,
            face: Vec::new(),
            west_ramp: (0.0, 0.0),
            mines: Vec::new(),
            caves: Vec::new(),
            washes: Vec::new(),
            wash_phase: rng.range(0.0, TAU),
            lines: Vec::new(),
            bridges: Vec::new(),
            branch: Vec::new(),
            junction: 0.0,
            branch_side: 1.0,
            shed: (Vec2::splat(-1000.0), Vec2::Y),
            ramps: Vec::new(),
            fords: Vec::new(),
            buttes: Vec::new(),
            seam: Vec::new(),
            wash_rig: None,
            dump: (Vec2::ZERO, 0.0, 0.0, 0.0),
            heap: (Vec2::ZERO, 0.0, 0.0, 0.0),
            fins: Vec::new(),
            hoodoos: Vec::new(),
            spawns: [(Vec::new(), -1000.0, 0), (Vec::new(), -1000.0, 0)],
            yard: Vec2::splat(-1000.0),
            camp: Vec2::splat(-1000.0),
            ways_up: Vec::new(),
            hold: Vec2::ZERO,
            repairs: Vec::new(),
            stands: [Vec2::ZERO; 2],
            spots: Vec::new(),
            mirror: false,
            turns: 0,
        };
        l.raise_escarpment(
            rng,
            n_mines,
            if n_washes == 2 && n_mines == 1 {
                50.0
            } else {
                0.0
            },
        )?;
        for _ in 0..n_washes {
            l.cut_wash(rng)?;
        }
        l.lay_lines(rng)?;
        l.stitch(rng)?;
        for _ in 0..n_buttes {
            l.raise_butte(rng);
        }
        l.place_sides()?;
        l.place_flats(rng)?;
        l.place_gameplay(rng);
        l.spots = l.find_spots();
        l.mirror = rng.chance(0.5);
        l.turns = rng.index(4) as u8;
        Ok(l)
    }

    /// The Face's lowest and highest foot over [x0, x1].
    pub(super) fn face_range(&self, x0: f32, x1: f32) -> (f32, f32) {
        let n = ((x1 - x0) / 0.5).ceil().max(1.0) as usize;
        (0..=n)
            .map(|k| self.face(x0 + (x1 - x0) * k as f32 / n as f32))
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), z| {
                (lo.min(z), hi.max(z))
            })
    }

    /// The escarpment: the mines' plans west to east, the arena sized to
    /// them, the Face's line, then each mine dug, each scree let fall, and
    /// the ways up the mesa.
    fn raise_escarpment(&mut self, rng: &mut Rng, n: usize, room: f32) -> Result<(), u32> {
        // Plans along x: each mine's Crag, adit, bay and bore, breach and
        // scree, west to east, the next mine's hall clear of the last scree.
        let wr0 = rng.range(9.0, 12.0);
        let mut cursor = wr0 + 66.0;
        let mut plans = Vec::new();
        for _ in 0..n {
            let cw = even(cursor + rng.range(0.0, 6.0));
            let ax = cw + even(rng.range(14.0, 20.0));
            let bay_w = even(rng.range(20.0, 24.0));
            let bay_x = (ax + 14.0, ax + 14.0 + bay_w);
            let bore_x = (bay_x.1, bay_x.1 + 4.0);
            let ce = bore_x.1 + rng.range(3.5, 6.0);
            let bx = even(ce + rng.range(6.0, 9.0));
            let s0 = even(bx + 10.0 + rng.range(4.0, 8.0));
            let s1 = s0 + even(rng.range(26.0, 32.0));
            plans.push((cw, ax, bay_x, bore_x, ce, bx, (s0, s1)));
            cursor = s1 + 28.0;
        }
        let last = plans.last().unwrap().6.1;
        self.size = even((last + rng.range(20.0, 26.0)).max(220.0) + room);
        let s = self.size;
        // The Face: low under each Crag (its mouths go in at it), not far
        // back where a hall's under the mesa, wandering elsewhere.
        let zf = s - rng.range(70.0, 78.0);
        let mut face = Vec::new();
        let mut x = -10.0;
        while x < s + 10.0 {
            let near = |lo: f32, hi: f32| plans.iter().any(|p| (p.0 + lo..p.4 + hi).contains(&x));
            let z = if near(-8.0, 8.0) {
                rng.range(zf - 4.0, (zf + 3.0).min(s - 69.0))
            } else if near(-32.0, 8.0) {
                rng.range(zf - 8.0, zf + 10.0).clamp(s - 82.0, s - 66.0)
            } else {
                rng.range(zf - 8.0, zf + 10.0).clamp(s - 82.0, s - 58.0)
            };
            face.push((x, z));
            x += rng.range(12.0, 17.0);
        }
        face.push((x.max(s + 10.0), rng.range(zf - 6.0, zf + 6.0)));
        self.face = face;
        for (cw, ax, bay_x, bore_x, ce, bx, scree) in plans {
            let m = self.dig_mine(rng, cw, ax, bay_x, bore_x, ce, bx, scree)?;
            self.mines.push(m);
        }
        // The scree: the Face runs straight across where it fell in, so its
        // slope doesn't lean sideways; then each breach's mouth is where
        // that Face is.
        for k in 0..self.mines.len() {
            let (x0, x1) = self.mines[k].scree;
            let across: Vec<usize> = (0..self.face.len())
                .filter(|&i| (x0 - 12.0..x1 + 12.0).contains(&self.face[i].0))
                .collect();
            if !across.is_empty() {
                let z = across.iter().map(|&i| self.face[i].1).sum::<f32>() / across.len() as f32;
                for i in across {
                    self.face[i].1 = z;
                }
            }
        }
        for k in 0..self.mines.len() {
            let (bx0, bx1) = (self.mines[k].breach.0.x, self.mines[k].breach.1.x);
            let (lo, _) = self.face_range(bx0, bx1);
            self.mines[k].breach.0.y = even_down(lo - 6.0);
            let m = &self.mines[k];
            let (_, hi) = self.face_range(m.crag_w, m.crag_e);
            if hi + FACE_RISE + 4.0 > m.bay.0.y {
                return Err(line!());
            }
        }
        self.caves = self.mines.iter().flat_map(|m| m.caves()).collect();
        // The mesa's west end: long enough for the heaviest hull to drive up
        // (the Seam's ran 40 m), its foot in from the west edge (room to
        // drive round onto it), short of the first west adit.
        let wx = self.mines[0].west_adit.0.x;
        let wr1 = rng.range(48.0, 54.0).min(wx - 8.0);
        if wr1 - wr0 < 36.0 {
            return Err(line!());
        }
        self.west_ramp = (wr0, wr1);
        let (_, z) = self.face_range(0.0, wr1 + 12.0);
        self.ways_up = vec![
            (Vec2::new(wr0, z + 12.0), Vec2::new(wr1 + 12.0, z + 12.0)),
            // Round the Face's west end, from the flats onto the road's foot.
            (
                Vec2::new(wr0 - 2.0, self.face(wr0) - 14.0),
                Vec2::new(wr0, z + 12.0),
            ),
        ];
        Ok(())
    }

    /// One mine's rooms, from its plan.
    #[allow(clippy::too_many_arguments)]
    fn dig_mine(
        &self,
        rng: &mut Rng,
        cw: f32,
        ax: f32,
        bay_x: (f32, f32),
        bore_x: (f32, f32),
        ce: f32,
        bx: f32,
        scree: (f32, f32),
    ) -> Result<Mine, u32> {
        let s = self.size;
        let v = Vec2::new;
        // The adit, from in front of the Face to past the north edge.
        let (lo, _) = self.face_range(ax, ax + 12.0);
        let adit = (v(ax, even_down(lo - 8.0)), v(ax + 12.0, s - 8.0));
        // The drift runs east under the box cut, the bay south of it.
        let drift = (v(ax + 12.0, s - 34.0), v(bx, s - 24.0));
        let bay_z1 = s - 36.0;
        let bay = (
            v(bay_x.0, bay_z1 - even(rng.range(22.0, 24.0))),
            v(bay_x.1, bay_z1),
        );
        let mid = even((bay.0.y + bay.1.y) * 0.5);
        let bore = (v(bore_x.0, mid - 4.0), v(bore_x.1, mid + 4.0));
        let beam = (
            Vec3::new(bay_x.1 - 5.4, 2.1, mid),
            Vec3::new(bore_x.1, 2.1, mid),
            0.55,
        );
        // The heading: in at the Crag's face, up to the bay.
        let (lo, _) = self.face_range(ax + 20.0, ax + 32.0);
        let heading = (v(ax + 20.0, even_down(lo - 6.0)), v(ax + 32.0, bay.0.y));
        // The hall west of the adit, the crosscut between: its west end out
        // from under the Crag, room west of it for the west adit.
        let x1 = ax - 6.0;
        let width = even(rng.range(28.0, 34.0)).max(x1 - (cw - 20.0));
        let x0 = x1 - width;
        let (_, hi) = self.face_range(x0 - 4.0, x1);
        let z0 = even((hi + rng.range(17.0, 22.0)).min(s - 50.0));
        if z0 < hi + FACE_RISE + 10.0 {
            return Err(line!());
        }
        let z1 = z0 + even(rng.range(24.0, 30.0));
        if z1 > s - 18.0 {
            return Err(line!());
        }
        let hall = (v(x0, z0), v(x1, z1));
        let crosscut = (v(x1, z0 + 6.0), v(ax, z1 - 4.0));
        let wx = x0 + 4.0;
        let (lo, _) = self.face_range(wx, wx + 10.0);
        let west_adit = (v(wx, even_down(lo - 8.0)), v(wx + 10.0, z0));
        // The Crag flares west as it runs back, never over the hall's west
        // end.
        let flare = rng.range(10.0, 15.0).min(cw - x0 - 9.0);
        if flare < 4.0 {
            return Err(line!());
        }
        let (lo, _) = self.face_range(bx, bx + 10.0);
        let breach = (v(bx, even_down(lo - 6.0)), v(bx + 10.0, s - 24.0));
        let box_cut = (v(ax + 12.0, s - 20.0), v(even(ce + 9.0), s - 10.0));
        // The pillars, where they stood in the Seam's hall, a little off:
        // 10 m apart up it, room to drive between them.
        let w = x1 - x0;
        let mut j = || Vec2::new(rng.range(-0.5, 0.5), rng.range(-0.5, 0.5));
        let pillars = [
            v(x0 + w * 0.28, z0 + 10.0) + j(),
            v(x0 + w * 0.7, z0 + 7.5) + j(),
            v(x0 + w * 0.31, z0 + 20.0) + j(),
            v(x0 + w * 0.69, z0 + 20.0) + j(),
        ];
        if scree.1 - scree.0 < 24.0 {
            return Err(line!());
        }
        Ok(Mine {
            crag_w: cw,
            crag_e: ce,
            flare,
            flare_z: z0 - 8.0,
            phase: rng.range(0.0, TAU),
            adit,
            hall,
            crosscut,
            west_adit,
            drift,
            breach,
            heading,
            bay,
            bore,
            beam,
            box_cut,
            pillars,
            hall_rig: (v(x1 - 4.0, z0 + 4.5), -PI * 0.5 - 0.3),
            headframe: v(x0 - 2.0, (z1 + 9.0).min(s - 10.0)),
            scree,
            scree_x: (scree.0 + scree.1) * 0.5,
        })
    }

    /// Clear of everything there's already: a wash's line must keep this
    /// far off the Face (a landing under it), the other washes, the buttes.
    fn wash_fits(&self, pts: &[Vec2]) -> bool {
        let s = self.size;
        let w = Wash { line: pts.to_vec() };
        let mut x = 0.0;
        while x < s {
            let mut z = 0.0;
            while z < s {
                let p = Vec2::new(x, z);
                let (d, _) = w.near(p);
                if d < 12.0 && p.y > self.face(p.x) - 10.0 {
                    return false;
                }
                // (Its last stretch may run into another: a junction.)
                let end = *pts.last().unwrap();
                if d < 4.0
                    && p.distance(end) > 50.0
                    && self.washes.iter().any(|o| o.near(p).0 < 40.0)
                {
                    return false;
                }
                z += 4.0;
            }
            x += 4.0;
        }
        true
    }

    /// A wash. The first runs under the Face, rising toward each adit (the
    /// line out of it bridges it), then away south-east out of the map; a
    /// second winds across the flats from the west edge or the south.
    fn cut_wash(&mut self, rng: &mut Rng) -> Result<(), u32> {
        // A few tries for one that fits.
        for _ in 0..12 {
            let pts = if self.washes.is_empty() {
                self.under_face_wash(rng)
            } else {
                self.flats_wash(rng)
            };
            if let Some(pts) = pts {
                self.washes.push(Wash { line: pts });
                return Ok(());
            }
        }
        Err(line!())
    }

    /// The wash under the Face, if this try fits: clear of the Face, and
    /// of each scree's foot (the way up it starts there).
    fn under_face_wash(&self, rng: &mut Rng) -> Option<Vec<Vec2>> {
        let s = self.size;
        let mut pts = Vec::new();
        let zf = self.face_range(0.0, s).0;
        let z_west = zf - rng.range(34.0, 50.0);
        // What it heads for along its run: up under each adit, a little
        // away between them.
        let mut marks = vec![(-14.0, z_west)];
        for (k, m) in self.mines.iter().enumerate() {
            let x = m.adit.0.x + 6.0;
            if k > 0 {
                // Well out from under the scree between them (its way up
                // starts at its foot).
                let sx = self.mines[k - 1].scree_x;
                marks.push((sx, self.face(sx) - SCREE_RUN - rng.range(24.0, 30.0)));
            }
            marks.push((x, self.face(x) - rng.range(23.0, 26.0)));
        }
        let at = |x: f32| {
            for w in marks.windows(2) {
                if x <= w[1].0 {
                    let t = smoothstep(w[0].0, w[1].0, x);
                    return w[0].1 + (w[1].1 - w[0].1) * t;
                }
            }
            marks.last().unwrap().1
        };
        let turn = self.mines.last().unwrap().crag_e + rng.range(-6.0, 8.0);
        let mut x = -14.0;
        while x < turn {
            let z =
                (at(x) + rng.range(-2.0, 2.0)).min(self.face_range(x - 26.0, x + 26.0).0 - 23.0);
            pts.push(Vec2::new(x, z));
            x += rng.range(20.0, 27.0);
        }
        // Round the bend: south-east, down south, then east out.
        let mut at = *pts.last().unwrap();
        for k in 0.. {
            let a =
                [-0.46, -0.91, -1.1, -1.19, -0.61, -0.21, -0.1][k.min(6)] + rng.range(-0.12, 0.12);
            at += Vec2::new(a.cos(), a.sin()) * rng.range(19.0, 24.0);
            pts.push(at);
            if at.x > s + 16.0 || at.y < -16.0 || k > 30 {
                break;
            }
        }
        // Out at the east edge, or the south.
        if at.x < s + 16.0 && at.y > -16.0 {
            return None;
        }
        let feet_clear = self.mines.iter().all(|m| {
            let x = m.scree_x;
            Wash { line: pts.clone() }
                .near(Vec2::new(x, self.face(x) - SCREE_RUN - 2.0))
                .0
                > 17.0
        });
        (feet_clear && self.wash_fits(&pts)).then_some(pts)
    }

    /// A wash winding across the flats, if this try fits.
    fn flats_wash(&self, rng: &mut Rng) -> Option<Vec<Vec2>> {
        let s = self.size;
        let mut pts = Vec::new();
        {
            let (mut at, mut a) = if rng.chance(0.5) {
                (
                    Vec2::new(-14.0, rng.range(0.1, 0.3) * s),
                    rng.range(-0.4, 0.3),
                )
            } else {
                (
                    Vec2::new(rng.range(0.25, 0.55) * s, -14.0),
                    rng.range(0.3, 1.2),
                )
            };
            pts.push(at);
            for _ in 0..40 {
                a = (a + rng.range(-0.35, 0.35)).clamp(-1.0, 1.3);
                at += Vec2::new(a.cos(), a.sin()) * rng.range(19.0, 24.0);
                // Not up under the Face.
                if at.y > self.face(at.x) - 70.0 {
                    a -= 0.6;
                    at.y = at.y.min(self.face(at.x) - 70.0);
                }
                pts.push(at);
                // Into the first wash, where it meets it.
                let (d, along) = self.washes[0].near(at);
                if d < 20.0 {
                    let (p, _) = line_at(&self.washes[0].line, along);
                    *pts.last_mut().unwrap() = p;
                    break;
                }
                if at.x > s + 16.0 || at.y < -16.0 {
                    break;
                }
            }
            let end = *pts.last().unwrap();
            let joined = self.washes[0].near(end).0 < 0.5;
            if end.x < s + 16.0 && end.y > -16.0 && !joined {
                return None;
            }
        }
        self.wash_fits(&pts).then_some(pts)
    }

    /// The railroad out of mine `k`, and its bridges, if this try fits.
    fn lay_line(&self, rng: &mut Rng, k: usize) -> Result<(Vec<Vec2>, Vec<Bridge>), u32> {
        let s = self.size;
        let n = self.mines.len();
        let mut bridges = Vec::new();
        let x = self.mines[k].adit.0.x + 6.0;
        let mouth = Vec2::new(x, self.face(x) + 1.0);
        // The first line leans east (or west, with another east of it).
        let a = match (n, k) {
            (1, _) => rng.range(0.12, 0.38),
            (_, 0) => rng.range(-0.28, 0.05),
            _ => rng.range(0.05, 0.35),
        };
        let dir = Vec2::new(a.sin(), -a.cos());
        let mut track = vec![Vec2::new(x, s - 4.0), mouth];
        // Out over the wash under the Face, if there is one.
        let end = if let Some(w) = self.washes.first() {
            let (mut best, mut cross) = (f32::INFINITY, mouth);
            for i in 0..400 {
                let p = mouth + dir * (i as f32 * 0.25);
                let (d, _) = w.near(p);
                if d < best {
                    best = d;
                    cross = p;
                }
            }
            if best > 1.0 || cross.distance(mouth) < 14.0 {
                return Err(line!());
            }
            let (_, along) = w.near(cross);
            let (_, t, _) = w.frame(along);
            if t.dot(dir).abs() > 0.6 {
                return Err(line!());
            }
            let end = cross + dir * 18.0;
            bridges.push(Bridge {
                a: mouth,
                b: end,
                line: k,
                wash: 0,
                s: along,
            });
            end
        } else {
            mouth + dir * rng.range(26.0, 32.0)
        };
        track.push(end);
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
        if track[2..].iter().any(|p| p.x < 40.0 || p.x > s - 40.0) {
            return Err(line!());
        }
        // Other washes it crosses out on the flats: a bridge over each,
        // on a straight stretch, clear of the train by the tipple.
        for wi in 1..self.washes.len() {
            let total = line_len(&track, track.len() - 1);
            let start = line_len(&track, 2);
            let mut u = start;
            let mut prev = f32::INFINITY;
            let mut falling = true;
            while u < total {
                let (p, _) = line_at(&track, u);
                let (dd, along) = self.washes[wi].near(p);
                if dd > prev && falling && prev < 1.5 {
                    let c = u - 0.5;
                    // On one straight leg, either side.
                    let leg = |v: f32| (1..track.len()).find(|&i| line_len(&track, i) >= v);
                    if leg(c - 20.0) != leg(c + 20.0) {
                        return Err(line!());
                    }
                    if k == 0 {
                        let t3 = line_len(&track, 3);
                        if c + 20.0 > t3 - 4.0 && c - 20.0 < t3 + 40.0 {
                            return Err(line!());
                        }
                    }
                    let (_, t, _) = self.washes[wi].frame(along);
                    let (_, dirl) = line_at(&track, c);
                    if t.dot(dirl).abs() > 0.6 {
                        return Err(line!());
                    }
                    bridges.push(Bridge {
                        a: line_at(&track, c - 18.0).0,
                        b: line_at(&track, c + 18.0).0,
                        line: k,
                        wash: wi,
                        s: along,
                    });
                }
                falling = dd <= prev;
                prev = dd;
                u += 0.5;
            }
        }
        if self
            .lines
            .iter()
            .any(|o| track[2..].iter().any(|&p| line_distance(o, p) < 30.0))
        {
            return Err(line!());
        }
        Ok((track, bridges))
    }

    /// A railroad out of each mine, south to the edge, bridging each wash
    /// it crosses; the warehouse's branch off the first.
    fn lay_lines(&mut self, rng: &mut Rng) -> Result<(), u32> {
        let n = self.mines.len();
        for k in 0..n {
            // A few tries for one that fits.
            let mut last = Err(line!());
            for _ in 0..10 {
                last = self.lay_line(rng, k);
                if last.is_ok() {
                    break;
                }
            }
            let (track, bridges) = last?;
            self.bridges.extend(bridges);
            self.lines.push(track);
        }
        // The warehouse's branch off the first line, off one side or the
        // other.
        let first = if rng.chance(0.5) { 1.0 } else { -1.0 };
        for side in [first, -first] {
            for junction in [14.0, 20.0, 9.0] {
                self.lay_branch(side, junction);
                if self.shed_fits() {
                    return Ok(());
                }
            }
        }
        // No room for it: no warehouse.
        self.branch.clear();
        self.shed = (Vec2::splat(-1000.0), Vec2::Y);
        Ok(())
    }

    /// The branch line `junction` m past the first line's fifth point,
    /// swinging off to the left of the way out (`side` 1) or the right (-1).
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
    /// the edge, clear of the washes and the other lines.
    fn shed_fits(&self) -> bool {
        let s = self.size;
        let (at, along) = self.shed;
        let across = along.perp();
        (0..=8).all(|i| {
            (0..=8).all(|k| {
                let u = (i as f32 / 4.0 - 1.0) * (SHED_HALF.x + 14.0);
                let v = (k as f32 / 4.0 - 1.0) * (SHED_HALF.y + 14.0);
                let p = at + across * u + along * v;
                p.min_element() > 6.0
                    && p.max_element() < s - 6.0
                    && self.washes.iter().all(|w| w.near(p).0 > 18.0)
                    && self.lines[1..].iter().all(|l| line_distance(l, p) > 8.0)
            })
        })
    }

    /// Joining it all up: a haul ramp down into the wash either side of
    /// each bridge, two crossings over each wash, the seam and its rig by
    /// the first bridge, a way up each scree.
    fn stitch(&mut self, rng: &mut Rng) -> Result<(), u32> {
        let s = self.size;
        // A ramp's top is out on the flat, in from the edge (not up the
        // scree, not under the Face).
        let flat = |l: &Layout, p: Vec2| {
            p.min_element() > 8.0
                && p.max_element() < s - 8.0
                && l.mesa(p) < 0.02
                && p.y < l.face(p.x) - 5.0
                && l.washes.iter().all(|w| w.near(p).0 > 14.0)
        };
        for bi in 0..self.bridges.len() {
            let b = self.bridges[bi];
            let w = &self.washes[b.wash];
            // Which side of the wash the line comes from: the near bank.
            let (_, _, n) = w.frame(b.s);
            let near = if (b.a - line_at(&w.line, b.s).0).dot(n) > 0.0 {
                1.0
            } else {
                -1.0
            };
            let ramp = |u0: f32, step: f32, side: f32, slant: f32| {
                (0..14).find_map(|k| {
                    let (p, t, n) = w.frame(u0 + step * k as f32);
                    let r = (p + n * side * 20.0 + t * slant, p, 5.0);
                    flat(self, r.0).then_some(r)
                })
            };
            // Down off the far bank short of the bridge, off the near bank
            // past it (slantwise, toward the bridge's mouth).
            let r0 = ramp(b.s - rng.range(24.0, 30.0), -3.0, -near, 0.0).ok_or(line!())?;
            let r1 = ramp(b.s + rng.range(24.0, 30.0), 3.0, near, 9.0).ok_or(line!())?;
            self.ramps.push(r0);
            self.ramps.push(r1);
        }
        // Crossings: two over each wash, a quarter and three quarters along
        // the stretch in the arena, nudged off the bridges and ramps, each
        // end short of the Face's foot.
        for wi in 0..self.washes.len() {
            let w = self.washes[wi].clone();
            let (lo, hi) = w.inside(s);
            for frac in [0.2, 0.8] {
                let u0 = lo + (hi - lo) * frac;
                let mut found = None;
                'nudge: for k in 0..21 {
                    let u = u0 + 6.0 * ((k + 1) / 2) as f32 * if k % 2 == 0 { 1.0 } else { -1.0 };
                    let (p, _, n) = w.frame(u);
                    let clear = self
                        .bridges
                        .iter()
                        .filter(|b| b.wash == wi)
                        .all(|b| (b.s - u).abs() > 30.0)
                        && self
                            .ramps
                            .iter()
                            .chain(&self.fords)
                            .all(|&(a, b, _)| seg(p, a, b).0 > 24.0);
                    if !clear {
                        continue;
                    }
                    // Each end: as far out as 23.5 m, shorter where the Face
                    // comes near (room to drive onto it under the rock).
                    let end = |side: f32| {
                        (0..16)
                            .map(|i| 23.5 - i as f32 * 0.5)
                            .map(|r| p + n * side * r)
                            .find(|&q| {
                                q.y < self.face_range(q.x - 6.0, q.x + 6.0).0 - 10.0
                                    && flat(self, q)
                            })
                    };
                    if let (Some(a), Some(b)) = (end(1.0), end(-1.0)) {
                        // Not across another wash too (where they meet).
                        let alone = (0..=10).all(|i| {
                            let q = a.lerp(b, i as f32 / 10.0);
                            self.washes
                                .iter()
                                .enumerate()
                                .all(|(k, o)| k == wi || o.near(q).0 > 18.0)
                        });
                        if alone {
                            found = Some((a, b, 6.0));
                            break 'nudge;
                        }
                    }
                }
                self.fords.push(found.ok_or(line!())?);
            }
        }
        // The seam in the first bridge's wash (if it's under the Face),
        // the rig cutting it short of the bridge, the Hold site by them.
        if let Some(b) = self.bridges.first().copied() {
            let w = &self.washes[b.wash];
            self.seam = (0..=24)
                .map(|k| {
                    let u = b.s - 43.0 + 70.0 * k as f32 / 24.0;
                    let (p, _, n) = w.frame(u);
                    p + n * (1.6 * (u * 0.21).sin() - 1.0)
                })
                .collect();
            let (p, t, n) = w.frame(b.s - 16.0);
            self.wash_rig = Some((p - n * 3.5, yaw_along(t)));
            self.hold = w.frame(b.s - 25.0).0;
        } else {
            // No wash: the Hold site out on the flats in front of the
            // first mine's mouth, by its line.
            let (p, dir) = line_at(&self.lines[0], line_len(&self.lines[0], 2));
            self.hold = p - dir.perp() * 12.0 * self.branch_side;
        }
        // The worn way up each scree, wherever across it is furthest from
        // the ramps and crossings (they're on the landing too), starting on
        // the flat short of its foot, out of the wash.
        for k in 0..self.mines.len() {
            let (x0, x1) = self.mines[k].scree;
            let way = |l: &Layout, x: f32| {
                let f = l.face(x);
                (
                    Vec2::new(x, f - SCREE_RUN - 6.0),
                    Vec2::new(x, f + FACE_RISE + 16.0),
                )
            };
            let (x, d) = (0..=10)
                .map(|i| x0 + 10.0 + (x1 - x0 - 20.0) * i as f32 / 10.0)
                .map(|x| {
                    let (a, b) = way(self, x);
                    let d = self
                        .ramps
                        .iter()
                        .chain(&self.fords)
                        .map(|&(p, q, _)| seg(p, a, b).0.min(seg(q, a, b).0))
                        .fold(f32::INFINITY, f32::min);
                    (x, d)
                })
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .unwrap();
            if d < 12.0 {
                return Err(line!());
            }
            let (a, b) = way(self, x);
            let start = (0..=6)
                .map(|i| a.y + i as f32)
                .find(|&z| self.washes.iter().all(|w| w.near(Vec2::new(x, z)).0 > 15.0))
                .ok_or(line!())?;
            self.mines[k].scree_x = x;
            self.ways_up.push((Vec2::new(x, start), b));
        }
        Ok(())
    }

    /// Room on the flats for a thing `r` across at `p`: off the washes, the
    /// lines, the warehouse, the spawns and camps, the ramps and crossings,
    /// the buttes and the ways up, out from under the Face, in from the edge.
    fn flats_free(&self, p: Vec2, r: f32) -> bool {
        let s = self.size;
        let spawn = |k: usize| {
            let (xs, z, rows) = &self.spawns[k];
            if xs.is_empty() {
                return f32::INFINITY;
            }
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
            && self.washes.iter().all(|w| w.near(p).0 > r + 14.0)
            && p.y < self.face(p.x) - r - 24.0
            && self.lines.iter().all(|t| line_distance(t, p) > r + 8.0)
            && (self.branch.is_empty() || line_distance(&self.branch, p) > r + 8.0)
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
            && self.buttes.iter().all(|b| b.distance(p) > r + 8.0)
    }

    /// A butte out on the flats, its ramp up the side facing the middle,
    /// if there's room for one.
    fn raise_butte(&mut self, rng: &mut Rng) {
        let s = self.size;
        for _ in 0..200 {
            let c = Vec2::new(rng.range(30.0, s - 30.0), rng.range(30.0, s * 0.6));
            let radius = rng.range(12.0, 18.0);
            let height = rng.range(6.0, 8.0);
            let to_mid = (self.center() - c).normalize_or(Vec2::Y);
            let ramp = Vec2::from_angle(rng.range(-0.6, 0.6)).rotate(to_mid);
            let b = Butte {
                center: c,
                radius,
                height,
                ramp,
                phase: rng.range(0.0, TAU),
            };
            let (foot, _) = b.way();
            let reach = radius * 1.3 + 4.0;
            let ok = self.flats_free(c, reach)
                && self.flats_free(foot, 6.0)
                && self
                    .buttes
                    .iter()
                    .all(|o| o.center.distance(c) > o.radius + reach + 30.0);
            if ok {
                self.ways_up.push(b.way());
                self.buttes.push(b);
                return;
            }
        }
    }

    /// Each side's spawns on its edge (the west, the east), wherever they
    /// come out fairest: the two sides about as far from what's worth
    /// taking (the mines' mouths, the bridges, the buttes' ramps).
    fn place_sides(&mut self) -> Result<(), u32> {
        let s = self.size;
        let goals = self.goals();
        // Where columns of `rows` can stand on the flats.
        let fits = |xs: &[f32], rows: usize, z0: f32| {
            xs.iter().all(|&x| {
                (0..rows).all(|k| {
                    let p = Vec2::new(x, z0 + k as f32 * 6.0);
                    // (Well out from under the Face: the chase camera behind
                    // a tank turning about stays out of the rock.)
                    p.y > 10.0
                        && p.y < self.face(p.x) - 16.0
                        && self.mesa(p) < 0.01
                        && self.washes.iter().all(|w| w.near(p).0 > 14.0)
                        && self.lines.iter().all(|t| line_distance(t, p) > 12.0)
                        && self
                            .ramps
                            .iter()
                            .chain(&self.fords)
                            .all(|&(a, b, h)| seg(p, a, b).0 > h + 8.0)
                        && self.buttes.iter().all(|b| b.distance(p) > 10.0)
                })
            })
        };
        // Three columns of seven rows, or failing that two of nine.
        let shapes = |edge: f32, toward: f32| {
            [
                (
                    vec![edge, edge + toward * 7.0, edge + toward * 14.0],
                    7usize,
                ),
                (vec![edge, edge + toward * 7.0], 9),
                (vec![edge, edge + toward * 7.0], 7),
            ]
        };
        let cands = |edge: f32, toward: f32| {
            let mut out = Vec::new();
            for (xs, rows) in shapes(edge, toward) {
                let mut z = 10.0;
                while z < s {
                    if fits(&xs, rows, z) {
                        out.push((xs.clone(), z, rows));
                    }
                    z += 2.0;
                }
                if !out.is_empty() {
                    break;
                }
            }
            out
        };
        let (ws, es) = (cands(11.0, 1.0), cands(s - 11.0, -1.0));
        let reach = |x: f32, z: f32, rows: usize| {
            let p = Vec2::new(x, z + (rows as f32 - 1.0) * 3.0);
            goals.iter().map(|g| g.distance(p)).sum::<f32>()
        };
        let (w, e) = ws
            .iter()
            .flat_map(|a| es.iter().map(move |b| (a, b)))
            .min_by(|(a, b), (c, d)| {
                let f = |a: &(Vec<f32>, f32, usize), b: &(Vec<f32>, f32, usize)| {
                    (reach(18.0, a.1, a.2) - reach(s - 18.0, b.1, b.2)).abs()
                };
                f(a, b).total_cmp(&f(c, d))
            })
            .ok_or(line!())?;
        let (w, e) = (w.clone(), e.clone());
        let (zw, ze) = (w.1, e.1);
        self.spawns = [w, e];
        self.yard = Vec2::new(37.0, zw + 14.0);
        self.camp = Vec2::new(s - 38.0, ze + 18.0);
        Ok(())
    }

    /// What's worth taking, for weighing the sides against each other: each
    /// mine's mouth, each bridge, each butte's ramp.
    pub fn goals(&self) -> Vec<Vec2> {
        let mut goals: Vec<Vec2> = self
            .mines
            .iter()
            .map(|m| {
                let x = m.adit.0.x + 6.0;
                Vec2::new(x, self.face(x) - 6.0)
            })
            .collect();
        goals.extend(self.bridges.iter().map(|b| b.b));
        goals.extend(self.buttes.iter().map(|b| b.way().0));
        goals
    }

    /// The dump and its heap, fins and hoodoos, all on free ground.
    fn place_flats(&mut self, rng: &mut Rng) -> Result<(), u32> {
        let s = self.size;
        let mut found = None;
        for _ in 0..400 {
            let c = Vec2::new(rng.range(30.0, s - 30.0), rng.range(24.0, s * 0.45));
            let (r, run) = (rng.range(13.0, 16.0), 11.0);
            // (Its slopes run on out past its lobes: well clear of the
            // warehouse's doors.)
            if self.flats_free(c, (r + run) * 0.6) && self.shed_distance(c) > r * 1.3 + run + 6.0 {
                found = Some((c, r, rng.range(4.0, 5.0), run));
                break;
            }
        }
        // (No room for it: no dump.)
        self.dump = found.unwrap_or((Vec2::splat(-1000.0), 0.0, 0.0, 0.0));
        let c = self.dump.0;
        for _ in 0..(if self.dump.1 > 0.0 { 100 } else { 0 }) {
            let a = rng.range(0.0, TAU);
            let p = c + Vec2::new(a.cos(), a.sin()) * rng.range(30.0, 42.0);
            if self.flats_free(p, 8.0) {
                self.heap = (p, 6.0, 3.0, 8.0);
                break;
            }
        }
        let clear_of_heaps = |l: &Layout, p: Vec2, r: f32| {
            (l.dump.1 == 0.0 || p.distance(l.dump.0) > l.dump.1 + l.dump.3 + r + 6.0)
                && (l.heap.1 == 0.0 || p.distance(l.heap.0) > l.heap.1 + l.heap.3 + r + 4.0)
        };
        let want = rng.int(3, 5) as usize;
        for _ in 0..300 {
            if self.fins.len() >= want {
                break;
            }
            let a = Vec2::new(rng.range(14.0, s - 14.0), rng.range(10.0, s * 0.55));
            let dir = Vec2::from_angle(rng.range(0.0, PI));
            let b = a + dir * rng.range(10.0, 22.0);
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
        let want = rng.int(5, 7) as usize;
        for _ in 0..300 {
            if self.hoodoos.len() >= want {
                break;
            }
            let p = Vec2::new(rng.range(14.0, s - 14.0), rng.range(10.0, s * 0.55));
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

    /// Repair drops, Raid stands.
    fn place_gameplay(&mut self, rng: &mut Rng) {
        let s = self.size;
        let mid = |k: usize| {
            let (xs, z, rows) = &self.spawns[k];
            let x = xs.iter().sum::<f32>() / xs.len() as f32;
            Vec2::new(x, z + (*rows as f32 - 1.0) * 3.0)
        };
        self.stands = [
            mid(0) + Vec2::new(26.0, 6.0),
            mid(1) + Vec2::new(-26.0, 6.0),
        ];
        let f = |x: f32| Vec2::new(x, self.face(x) - 15.0);
        let mut repairs = Vec::new();
        for m in &self.mines {
            repairs.push(f(m.west_adit.0.x - 20.0));
            repairs.push(f(m.breach.1.x + 6.0));
        }
        let x = self.lines[0][3].x;
        repairs.extend([
            Vec2::new(x - rng.range(45.0, 55.0), rng.range(0.3, 0.4) * s),
            Vec2::new(x + rng.range(26.0, 34.0), rng.range(0.28, 0.35) * s),
            Vec2::new(x - rng.range(25.0, 35.0), rng.range(0.14, 0.2) * s),
            Vec2::new(x + rng.range(25.0, 35.0), rng.range(0.12, 0.18) * s),
        ]);
        repairs.extend(self.buttes.iter().map(|b| b.center));
        self.repairs = repairs;
    }

    /// Places bots head for: across the flats and the landing under the
    /// Face, along the washes, in the warehouse, the mines, up on the mesa
    /// and the buttes.
    fn find_spots(&self) -> Vec<Vec2> {
        let s = self.size;
        let mut spots = Vec::new();
        let mut z = 14.0;
        while z < s - 8.0 {
            let mut x = 18.0 + (z * 0.37).sin() * 6.0;
            while x < s - 12.0 {
                let p = Vec2::new(x, z);
                let below = p.y < self.face(p.x) - 6.0
                    && self.washes.iter().all(|w| w.near(p).0 > 12.0)
                    && self.buttes.iter().all(|b| b.distance(p) > 4.0);
                let above =
                    p.y > self.face(p.x) + 14.0 && self.mesa(p) > 0.95 && self.crag_depth(p) < -6.0;
                if below || above {
                    spots.push(p);
                }
                x += 32.0;
            }
            z += 30.0;
        }
        for w in &self.washes {
            let mut u = 10.0;
            while u < w.len() {
                let (p, _) = line_at(&w.line, u);
                if p.min_element() > 22.0 && p.max_element() < s - 22.0 {
                    spots.push(p);
                }
                u += 28.0;
            }
        }
        let mid = |r: Rect| (r.0 + r.1) * 0.5;
        spots.push(self.shed.0);
        for m in &self.mines {
            spots.push(Vec2::new(mid(m.adit).x, m.bay.0.y + 10.0));
            spots.push(mid(m.hall));
            spots.push(mid(m.drift));
            spots.push(Vec2::new(mid(m.breach).x, m.drift.0.y - 14.0));
        }
        spots.extend(self.buttes.iter().map(|b| b.center));
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

    /// How much scree (0..1) there is at `x`, in place of the cliff.
    pub fn scree(&self, x: f32) -> f32 {
        self.mines
            .iter()
            .map(|m| {
                let (a, b) = m.scree;
                smoothstep(a - 4.0, a + 6.0, x) * (1.0 - smoothstep(b - 6.0, b + 4.0, x))
            })
            .fold(0.0, f32::max)
    }

    /// The mine whose Crag is nearest `x`.
    pub fn mine_at(&self, x: f32) -> &Mine {
        self.mines
            .iter()
            .min_by(|a, b| {
                let d = |m: &Mine| (x - (m.crag_w + m.crag_e) * 0.5).abs();
                d(a).total_cmp(&d(b))
            })
            .unwrap()
    }

    /// How far into a Crag a point is (m; negative: outside). Its front
    /// rises behind the mesa's own face.
    pub fn crag_depth(&self, p: Vec2) -> f32 {
        let front = p.y - self.face(p.x) - FACE_RISE;
        self.mines
            .iter()
            .map(|m| {
                (p.x - m.crag_west(p.y, self.size))
                    .min(m.crag_east(p.y, self.size) - p.x)
                    .min(front)
            })
            .fold(f32::NEG_INFINITY, f32::max)
    }

    /// How much of a Crag (0..1) stands over a point.
    pub fn crag(&self, p: Vec2) -> f32 {
        smoothstep(0.0, 3.0, self.crag_depth(p))
    }

    /// How high the mesa stands at a point, before the mines are cut out:
    /// 0..1 of `TOP`.
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

    /// Every mouth in a Crag's face.
    pub fn mouths(&self) -> Vec<Rect> {
        self.mines.iter().flat_map(|m| m.mouths()).collect()
    }

    /// In a box cut, grown by `grow`.
    pub fn in_box_cut(&self, p: Vec2, grow: f32) -> bool {
        self.mines.iter().any(|m| within(m.box_cut, p, grow))
    }

    /// Distance to the nearest box cut.
    pub fn box_cut_distance(&self, p: Vec2) -> f32 {
        self.mines
            .iter()
            .map(|m| rect_distance(m.box_cut, p))
            .fold(f32::INFINITY, f32::min)
    }

    /// Rock: under the mesa, back from its foot (not out in front of the
    /// Face).
    pub fn in_rock(&self, p: Vec2) -> bool {
        self.mesa(p) > 0.5 && !self.in_cave(p) && !self.in_box_cut(p, 0.0)
    }

    /// Where the nearest wash is nearest `p`: (distance from its line,
    /// metres along it).
    pub fn wash_near(&self, p: Vec2) -> (f32, f32) {
        self.washes
            .iter()
            .map(|w| w.near(p))
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .unwrap_or((f32::INFINITY, 0.0))
    }

    /// A wash's depth and floor half-width, `s` m along it: the floor
    /// narrows and widens as it winds.
    pub fn wash_profile(&self, s: f32) -> (f32, f32) {
        (WASH_DEPTH, 6.5 + 1.5 * (0.08 * s + self.wash_phase).sin())
    }

    /// How far down into a wash the ground is at `p` (negative).
    pub fn wash(&self, p: Vec2) -> f32 {
        self.washes
            .iter()
            .map(|w| {
                let (d, s) = w.near(p);
                let (depth, half) = self.wash_profile(s);
                -depth * (1.0 - smoothstep(half, half + WASH_BANK, d))
            })
            .fold(0.0, f32::min)
    }

    /// The ground with the haul ramps and the crossings cut into the washes.
    fn with_ramps(&self, p: Vec2, g: f32) -> f32 {
        let mut g = g;
        for &(s, e, half) in &self.ramps {
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
        for &(s, e, half) in &self.fords {
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

    /// How high the buttes stand at `p`.
    pub fn butte(&self, p: Vec2) -> f32 {
        self.buttes
            .iter()
            .map(|b| b.height_at(p))
            .fold(0.0, f32::max)
    }

    /// How near a track, any line or the branch.
    pub fn track_distance(&self, p: Vec2) -> f32 {
        let b = if self.branch.is_empty() {
            f32::INFINITY
        } else {
            line_distance(&self.branch, p)
        };
        self.main_line_distance(p).min(b)
    }

    /// How near any main line.
    pub fn main_line_distance(&self, p: Vec2) -> f32 {
        self.lines
            .iter()
            .map(|l| line_distance(l, p))
            .fold(f32::INFINITY, f32::min)
    }

    /// Position and heading `s` m along the first line.
    pub fn track_at(&self, s: f32) -> (Vec2, Vec2) {
        line_at(&self.lines[0], s)
    }

    /// Length of the first line up to its point `k`.
    pub fn track_len(&self, k: usize) -> f32 {
        line_len(&self.lines[0], k)
    }

    /// Where the branch leaves the first line, and the line's way there.
    pub fn branch_from(&self) -> (Vec2, Vec2) {
        self.track_at(self.track_len(4) + self.junction)
    }

    /// Over a bridge's deck (not its end plates).
    pub fn on_bridge(&self, p: Vec2) -> bool {
        self.bridges.iter().any(|b| {
            let (d, t) = seg(p, b.a, b.b);
            d < DECK_HALF && t > 0.0 && t < 1.0
        })
    }

    /// A point in the warehouse's own frame: (across, along) from its
    /// middle, +across to the left of the way in.
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

    /// The ground straight out in front of the warehouse's doors.
    pub fn shed_doorways(&self, p: Vec2) -> bool {
        let l = self.shed_local(p).abs();
        (l.y < 7.0 && l.x < SHED_HALF.x + 16.0) || (l.x < 7.0 && l.y < SHED_HALF.y + 16.0)
    }

    /// Each side's spawns, as laid out.
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
            || (p.x > self.size - 31.0 && (e.start - 6.0..e.end + 8.0).contains(&p.y))
            || p.distance(self.camp) < 12.0
    }

    /// Play ground.
    pub fn ground(&self, x: f32, z: f32) -> f32 {
        self.surface(x, z, true)
    }

    /// The rock the mines were cut from: the ground as if there were no
    /// caves under the mesa.
    pub fn uncut(&self, p: Vec2) -> f32 {
        self.surface(p.x, p.y, false)
    }

    fn surface(&self, x: f32, z: f32, caves: bool) -> f32 {
        let p = Vec2::new(x, z);
        let m = self.mesa(p);
        let dunes = 0.45 * (0.061 * x + 0.3).sin() * (0.047 * z).cos()
            + 0.25 * (0.13 * x - 0.09 * z).sin()
            + 0.15 * (0.21 * z + 0.4 * (0.05 * x).sin()).sin();
        // Flat along the railroads, under the warehouse, on the mesa, and
        // graded round the ramps, crossings and ways up.
        let shed = smoothstep(1.0, 8.0, self.shed_distance(p));
        let graded = self
            .ramps
            .iter()
            .chain(&self.fords)
            .map(|&(a, b, h)| smoothstep(h + 2.0, h + 8.0, seg(p, a, b).0))
            .fold(1.0, f32::min);
        let road = self
            .ways_up
            .iter()
            .map(|&(a, b)| smoothstep(4.0, 10.0, seg(p, a, b).0))
            .fold(1.0, f32::min);
        let flat = smoothstep(3.0, 9.0, self.track_distance(p)) * shed * graded * road * (1.0 - m);
        // The mesa's top rolls a little, away from the mines' roofs and back
        // from the lip.
        let f = self.face(x);
        let cave = self.cave_distance(p);
        let by_cut = self.box_cut_distance(p);
        let (a, b) = self.ways_up[0];
        let haul = smoothstep(5.0, 11.0, seg(p, a, b).0);
        let relief = (1.8 * (0.09 * x + 0.7).sin() * (0.07 * z).cos()
            + 0.6 * (0.21 * x - 0.17 * z).sin())
        .max(-0.6)
            * haul
            * smoothstep(5.0, 14.0, cave)
            * smoothstep(f + FACE_RISE + 1.0, f + FACE_RISE + 9.0, z)
            * smoothstep(4.0, 10.0, by_cut)
            * m.powi(4);
        // The Crags over it, their tops rolling too (flat over the roofs).
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
        // The box cuts, down through the rock.
        for mine in &self.mines {
            let (a, b) = mine.box_cut;
            let d = rect_distance(mine.box_cut, p);
            rock = rock.min(mine.box_cut_floor(x.clamp(a.x, b.x)) + rock * smoothstep(0.0, 2.0, d));
        }
        let g = dunes * flat
            + rock
            + self.wash(p)
            + (heap(self.dump, p) + heap(self.heap, p)) * shed
            + self.fins(p)
            + self.butte(p);
        self.with_ramps(p, g)
    }

    /// Desert out past the edge: the mesa runs on north, the floor rises
    /// into broken hills.
    pub fn scenery_ground(&self, x: f32, z: f32) -> f32 {
        self.scenery_ground_with(x, z, &|e: Vec2| self.ground(e.x, e.y))
    }

    /// `scenery_ground`, with the play ground at the arena's edge given
    /// (`edge`, at a point on it): looked up, it's much quicker than the
    /// ground worked out afresh for every point out there.
    pub fn scenery_ground_with(&self, x: f32, z: f32, edge_ground: &dyn Fn(Vec2) -> f32) -> f32 {
        let size = self.size;
        let p = Vec2::new(x, z);
        let q = (p - self.center()).abs() - Vec2::splat(size * 0.5);
        let d = q.max(Vec2::ZERO).length() + q.x.max(q.y).min(0.0);
        if d < -0.01 {
            return -10.0;
        }
        let edge = p.clamp(Vec2::ZERO, Vec2::splat(size));
        let q = Vec2::new(edge.x, z.max(edge.y));
        let base = edge_ground(edge).max(TOP * self.mesa(q) + (CRAG_TOP - TOP) * self.crag(q));
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

    /// Under a Crag's rock.
    pub fn under_crag(&self, p: Vec2) -> bool {
        self.crag(p) > 0.5
    }

    /// Under a cave roof (or on top of one).
    pub fn roofed(&self, p: Vec2) -> bool {
        self.cave_distance(p) < CAVE_WALL + 0.5
            && self.mesa(Vec2::new(p.x, p.y - 1.0)) > 0.9
            && !self.in_box_cut(p, 0.0)
            && p.y < self.size + 6.0
    }

    /// A high room's roof over a point in it, if it's in one.
    pub fn high_roof(&self, p: Vec2) -> Option<f32> {
        let depth = self.crag_depth(p);
        self.mines.iter().find_map(|m| m.high_roof(p, depth))
    }

    /// A roof slab's underside over 2 m cell `c`.
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

    /// The roof's underside as drawn over a point.
    pub fn drawn_roof(&self, p: Vec2) -> f32 {
        let under = ROOF.0 - 0.08;
        let near = |r: Rect| rect_distance(r, p) < 3.0;
        let target = if self.mines.iter().any(|m| near(m.bay)) {
            BAY_ROOF
        } else if self.mines.iter().any(|m| near(m.hall) || near(m.crosscut)) {
            HALL_ROOF
        } else {
            return under;
        };
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

    /// The span of ground the mines' roofs and lining are worked over.
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
