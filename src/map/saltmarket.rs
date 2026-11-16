//! Saltmarket: a ruined trading town on the shore of a dry reservoir, on a
//! desert world (see docs/setting.md). Attrition only.
//!
//! Blockout layout, 240 m square (+x east, +z north):
//! - Town (west two thirds): a hand-drawn road network (bends, T-junctions,
//!   loops, old-town alleys around a market square). Buildings line every
//!   road turned to face it, with varied frontages, depths and setbacks, then
//!   infill behind them: standing (some stepped), gutted shells with jagged
//!   broken tops and a breach toward the street, low stubs with sand banked up
//!   to the roof (terrain drifts), or stepped rubble heaps. The market hall
//!   (drive-through: four piers and a roof) stands in the square.
//! - Reservoir bed (east third): a bowl ~6 m deep with gentle shores, so you
//!   can drive in and out anywhere. Open, long sight lines. A half-buried war
//!   machine sits in it as cover and landmark.
//! - High ground: a dune ridge along the north edge overlooking town and bowl,
//!   and the dam across the reservoir's south end (ramp up from town, ramp
//!   down into the bowl, a breach through the middle).
//!
//! The three lanes loop into each other: no dead ends.

use glam::{Vec2, Vec3};

use super::{Map, OBox, Rise, Terrain, Wedge, mat};
use crate::rng::Rng;

const SIZE: f32 = 240.0;
/// Reservoir bed floor relative to the town.
const BOWL_DEPTH: f32 = 6.0;
const BOWL_CENTER: Vec2 = Vec2::new(189.0, 120.0);
const BOWL_HALF: Vec2 = Vec2::new(39.0, 85.0);
const BOWL_ROUND: f32 = 25.0;
/// Width of the reservoir's sloped shore.
const SHORE: f32 = 28.0;
/// Clearance a spawn needs from anything solid (about a tank's half length).
const TANK_CLEAR: f32 = 3.0;

const SAND: Vec3 = Vec3::new(0.72, 0.62, 0.45);
const CONCRETE: Vec3 = Vec3::new(0.78, 0.74, 0.66);
const RUIN: Vec3 = Vec3::new(0.66, 0.62, 0.55);
const RUBBLE: Vec3 = Vec3::new(0.6, 0.56, 0.5);
const DAM: Vec3 = Vec3::new(0.6, 0.58, 0.54);
const RUST: Vec3 = Vec3::new(0.5, 0.36, 0.24);
const WRECK: Vec3 = Vec3::new(0.36, 0.34, 0.31);

/// Signed distance to the reservoir's rounded-rectangle outline (negative inside).
fn bowl_sdf(p: Vec2) -> f32 {
    let q = (p - BOWL_CENTER).abs() - BOWL_HALF + Vec2::splat(BOWL_ROUND);
    q.max(Vec2::ZERO).length() + q.x.max(q.y).min(0.0) - BOWL_ROUND
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Ground height: flat town at 0, the reservoir bowl, the north dune ridge,
/// small dunes along the west edge.
pub fn ground(x: f32, z: f32) -> f32 {
    let inside = smoothstep(0.0, SHORE, -bowl_sdf(Vec2::new(x, z)));
    let cracks = (x * 0.15).sin() * (z * 0.11).cos() * 0.4;
    let bowl = -BOWL_DEPTH * inside + inside * cracks;
    let ridge = 5.0 * (-((z - 218.0) / 14.0).powi(2)).exp() * (0.8 + 0.2 * (x * 0.07).sin());
    let west = 2.0 * (-((x - 6.0) / 7.0).powi(2)).exp() * (0.6 + 0.4 * (z * 0.1).sin());
    bowl + ridge + west
}

/// Market square (open, holds the hall).
const SQUARE_MIN: Vec2 = Vec2::new(62.0, 100.0);
const SQUARE_MAX: Vec2 = Vec2::new(98.0, 140.0);

/// Roads: hand-drawn polylines (x, z) and full width. They bend, meet at
/// T-junctions and loop back on each other: no grid.
const ROADS: &[(&[(f32, f32)], f32)] = &[
    // Main road: west gate, kinking into the market square.
    (
        &[(4.0, 108.0), (30.0, 112.0), (48.0, 121.0), (64.0, 121.0)],
        12.0,
    ),
    // Square down to the reservoir shore.
    (
        &[
            (96.0, 119.0),
            (116.0, 113.0),
            (134.0, 122.0),
            (152.0, 130.0),
        ],
        11.0,
    ),
    // Square north, winding up toward the ridge.
    (
        &[
            (81.0, 139.0),
            (76.0, 160.0),
            (86.0, 180.0),
            (102.0, 196.0),
            (112.0, 208.0),
        ],
        10.0,
    ),
    // Square south to the dam ramp.
    (
        &[
            (78.0, 101.0),
            (70.0, 80.0),
            (80.0, 58.0),
            (100.0, 42.0),
            (124.0, 30.0),
            (140.0, 24.0),
        ],
        11.0,
    ),
    // West lane.
    (
        &[(8.0, 56.0), (34.0, 68.0), (52.0, 84.0), (66.0, 104.0)],
        8.0,
    ),
    // North-east lane.
    (
        &[
            (96.0, 134.0),
            (112.0, 150.0),
            (124.0, 168.0),
            (132.0, 190.0),
        ],
        7.0,
    ),
    // South-west lane.
    (&[(20.0, 22.0), (44.0, 40.0), (68.0, 64.0)], 8.0),
    // East lane past the water tower, back down to the south road.
    (
        &[
            (116.0, 113.0),
            (126.0, 94.0),
            (130.0, 72.0),
            (124.0, 52.0),
            (110.0, 40.0),
        ],
        8.0,
    ),
    // North-west loop.
    (
        &[
            (30.0, 112.0),
            (24.0, 138.0),
            (34.0, 162.0),
            (54.0, 176.0),
            (78.0, 166.0),
        ],
        8.0,
    ),
    // Old-town alleys hugging the square.
    (
        &[(64.0, 121.0), (60.0, 140.0), (70.0, 152.0), (81.0, 139.0)],
        6.0,
    ),
    (
        &[(96.0, 119.0), (104.0, 102.0), (92.0, 90.0), (78.0, 101.0)],
        6.0,
    ),
];
/// Market square: open ground around the hall.
const SQUARE_C: Vec2 = Vec2::new(80.0, 120.0);
const SQUARE_R: f32 = 21.0;
/// Town extents (buildings stay inside).
const TOWN_MIN: Vec2 = Vec2::new(10.0, 12.0);
const TOWN_MAX: Vec2 = Vec2::new(150.0, 200.0);

/// A sand drift banked against one face of a building, raising the terrain.
#[derive(Clone, Copy)]
struct Drift {
    b: OBox,
    /// Which face: local +x, -x, +z, -z as (axis 0/1, sign).
    axis: usize,
    sign: f32,
    height: f32,
    run: f32,
}

impl Drift {
    fn height_at(&self, p: Vec2) -> f32 {
        let q = self.b.local_point(p);
        let (along, across, half_along, half_across) = if self.axis == 0 {
            (q.x, q.y, self.b.half.x, self.b.half.y)
        } else {
            (q.y, q.x, self.b.half.y, self.b.half.x)
        };
        let d = along * self.sign - half_along;
        if d < -0.5 || d > self.run {
            return 0.0;
        }
        let lateral = 1.0 - smoothstep(half_across * 0.6, half_across + 2.0, across.abs());
        self.height * (1.0 - d.max(0.0) / self.run).powf(0.8) * lateral
    }
}

struct RoadNet {
    /// Dense samples along every road: (point, half width, direction).
    samples: Vec<(Vec2, f32, Vec2)>,
}

impl RoadNet {
    fn new() -> RoadNet {
        let mut samples = Vec::new();
        for (pts, w) in ROADS {
            for seg in pts.windows(2) {
                let (a, b) = (Vec2::from(seg[0]), Vec2::from(seg[1]));
                let len = a.distance(b);
                let dir = (b - a) / len;
                let n = (len / 1.5).ceil() as usize;
                for i in 0..=n {
                    samples.push((a.lerp(b, i as f32 / n as f32), w * 0.5, dir));
                }
            }
        }
        RoadNet { samples }
    }

    /// True if the footprint stays `margin` clear of every road.
    fn clear(&self, b: &OBox, margin: f32) -> bool {
        self.samples
            .iter()
            .all(|&(p, half, _)| b.distance(p) > half + margin)
    }

    /// Direction of the nearest road.
    fn nearest_dir(&self, p: Vec2) -> Vec2 {
        self.samples
            .iter()
            .min_by(|a, b| a.0.distance_squared(p).total_cmp(&b.0.distance_squared(p)))
            .unwrap()
            .2
    }
}

/// Yaw that turns a box's local +x along `dir` (xz).
fn yaw_along(dir: Vec2) -> f32 {
    (-dir.y).atan2(dir.x)
}

impl Map {
    /// The town: buildings lining every road, turned to face it, with varied
    /// frontages, depths and setbacks, then infill behind them. Returns the
    /// sand drifts to bank against them.
    fn town(&mut self, rng: &mut Rng, reserved: &[OBox]) -> Vec<Drift> {
        let roads = RoadNet::new();
        let mut lots: Vec<OBox> = Vec::new();
        let fits = |b: &OBox, lots: &[OBox]| {
            let inside = b
                .corners()
                .iter()
                .all(|c| c.cmpge(TOWN_MIN).all() && c.cmple(TOWN_MAX).all());
            inside
                && b.corners()
                    .iter()
                    .all(|&c| bowl_sdf(c) > 6.0 && c.distance(SQUARE_C) > SQUARE_R)
                && b.center.distance(SQUARE_C) > SQUARE_R + 4.0
                && roads.clear(b, 0.8)
                && lots.iter().chain(reserved).all(|o| !o.overlaps(b, 1.0))
        };
        let footprint = |center: Vec2, yaw: f32, front: f32, depth: f32| OBox {
            center,
            yaw,
            half: Vec2::new(front * 0.5, depth * 0.5),
            y0: 0.0,
            y1: 1.0,
            color: CONCRETE,
            side_mat: mat::WALL,
            top_mat: mat::ROOF,
        };

        // Frontage: walk each road on both sides.
        for (pts, w) in ROADS {
            for seg in pts.windows(2) {
                let (a, b) = (Vec2::from(seg[0]), Vec2::from(seg[1]));
                let len = a.distance(b);
                let dir = (b - a) / len;
                let nrm = Vec2::new(-dir.y, dir.x);
                for side in [-1.0f32, 1.0] {
                    let mut t = rng.range(0.0, 4.0);
                    while t < len {
                        let front = rng.range(7.0, 14.0);
                        let depth = rng.range(8.0, 15.0);
                        let setback = rng.range(0.8, 3.0);
                        let p = a + dir * (t + front * 0.5);
                        let c = p + nrm * side * (w * 0.5 + setback + depth * 0.5);
                        // Old towns aren't square: a little twist off the road line.
                        let yaw = yaw_along(dir) + rng.range(-0.06, 0.06);
                        let b = footprint(c, yaw, front, depth);
                        if fits(&b, &lots) {
                            lots.push(b);
                        }
                        t += front + rng.range(1.0, 3.5);
                    }
                }
            }
        }
        // Infill behind the frontage, aligned to the nearest road.
        for i in 0..4000 {
            let c = Vec2::new(
                rng.range(TOWN_MIN.x, TOWN_MAX.x),
                rng.range(TOWN_MIN.y, TOWN_MAX.y),
            );
            let yaw = yaw_along(roads.nearest_dir(c)) + rng.range(-0.12, 0.12);
            // Big buildings first, then smaller ones squeezed into the gaps.
            let s = if i < 1500 {
                rng.range(8.0, 14.0)
            } else {
                rng.range(5.0, 9.0)
            };
            let b = footprint(c, yaw, s, s * rng.range(0.7, 1.4));
            if fits(&b, &lots) {
                lots.push(b);
            }
        }

        let mut drifts = Vec::new();
        for lot in lots {
            self.ruin(rng, lot, &roads, &mut drifts);
        }
        drifts
    }

    /// A box in a lot's local frame: `min`/`max` are local footprint coords.
    fn local_box(
        &mut self,
        lot: &OBox,
        min: Vec2,
        max: Vec2,
        y0: f32,
        y1: f32,
        color: Vec3,
    ) -> OBox {
        let (ax, az) = lot.axes();
        let mid = (min + max) * 0.5;
        let b = OBox {
            center: lot.center + ax * mid.x + az * mid.y,
            yaw: lot.yaw,
            half: (max - min) * 0.5,
            y0,
            y1,
            color,
            side_mat: mat::WALL,
            top_mat: mat::ROOF,
        };
        self.oboxes.push(b);
        b
    }

    /// One building: standing (sometimes stepped), a gutted shell with a
    /// jagged broken top and a breach toward the street, a low stub with sand
    /// banked up to its roof, or a collapsed rubble heap you can drive over.
    fn ruin(&mut self, rng: &mut Rng, lot: OBox, roads: &RoadNet, drifts: &mut Vec<Drift>) {
        let h = lot.half;
        // Which local face looks at the nearest road (the street front).
        let to_road = {
            let p = roads
                .samples
                .iter()
                .min_by(|a, b| {
                    a.0.distance_squared(lot.center)
                        .total_cmp(&b.0.distance_squared(lot.center))
                })
                .unwrap()
                .0;
            lot.local_point(p)
        };
        let (front_axis, front_sign) = if (to_road.x / h.x).abs() > (to_road.y / h.y).abs() {
            (0, to_road.x.signum())
        } else {
            (1, to_road.y.signum())
        };
        let roll = rng.f32();
        if roll < 0.2 {
            let height = rng.range(7.0, 13.0);
            let c = CONCRETE * rng.range(0.92, 1.05);
            self.local_box(&lot, -h, h, 0.0, height, c);
            if rng.chance(0.5) {
                // Upper storey set back from the street.
                let inset = Vec2::splat(rng.range(1.5, 3.0));
                self.local_box(
                    &lot,
                    -h + inset,
                    h - inset,
                    height,
                    height + rng.range(2.5, 4.0),
                    c * 0.97,
                );
            }
            drifts.push(Drift {
                b: lot,
                axis: front_axis,
                sign: front_sign,
                height: 1.0,
                run: 4.0,
            });
        } else if roll < 0.55 {
            // Gutted shell. Each wall is split into pieces of different heights
            // (a jagged broken top); the street wall has a breach to drive in.
            let (t, base_h) = (1.0, rng.range(5.0, 9.0));
            let c = RUIN * rng.range(0.9, 1.05);
            for (axis, sign) in [(0usize, 1.0f32), (0, -1.0), (1, 1.0), (1, -1.0)] {
                let len = if axis == 0 { h.y } else { h.x } * 2.0;
                let breach = axis == front_axis && sign == front_sign;
                let pieces = if len > 10.0 { 3 } else { 2 };
                for k in 0..pieces {
                    let (s0, s1) = (
                        -len * 0.5 + len * k as f32 / pieces as f32,
                        -len * 0.5 + len * (k + 1) as f32 / pieces as f32,
                    );
                    if breach && k == pieces / 2 {
                        continue; // the gap you drive through
                    }
                    let top = base_h * rng.range(0.45, 1.0);
                    let (lo, hi) = if axis == 0 {
                        let x = sign * (h.x - t * 0.5);
                        (Vec2::new(x - t * 0.5, s0), Vec2::new(x + t * 0.5, s1))
                    } else {
                        let z = sign * (h.y - t * 0.5);
                        (Vec2::new(s0, z - t * 0.5), Vec2::new(s1, z + t * 0.5))
                    };
                    self.local_box(&lot, lo, hi, 0.0, top, c);
                }
            }
            // A fallen floor slab leaning in one corner.
            if rng.chance(0.5) {
                let q = h * 0.45;
                self.local_box(
                    &lot,
                    Vec2::new(-q.x, -q.y),
                    Vec2::new(0.0, 0.0),
                    0.0,
                    0.5,
                    RUBBLE,
                );
            }
        } else if roll < 0.8 {
            // Low stub, sand banked up its street face to the roof.
            let height = 2.2;
            self.local_box(&lot, -h, h, 0.0, height, RUIN * rng.range(0.9, 1.0));
            drifts.push(Drift {
                b: lot,
                axis: front_axis,
                sign: front_sign,
                height: height + 0.15,
                run: 8.0,
            });
        } else {
            // Collapsed: a stepped rubble heap (each step low enough to climb)
            // and one wall stub still standing.
            for (k, s) in [1.0f32, 0.72, 0.45].iter().enumerate() {
                let y0 = k as f32 * 0.45;
                self.local_box(
                    &lot,
                    -h * *s,
                    h * *s,
                    y0,
                    y0 + 0.45,
                    RUBBLE * (1.0 - 0.04 * k as f32),
                );
            }
            let wall_len = h.x * rng.range(0.6, 1.4);
            self.local_box(
                &lot,
                Vec2::new(-h.x, h.y - 1.0),
                Vec2::new(-h.x + wall_len, h.y),
                0.0,
                rng.range(3.0, 6.0),
                RUIN,
            );
        }
        // Wind-blown sand against a random back wall.
        if rng.chance(0.4) {
            drifts.push(Drift {
                b: lot,
                axis: rng.index(2),
                sign: if rng.chance(0.5) { 1.0 } else { -1.0 },
                height: 0.9,
                run: 3.5,
            });
        }
    }
}

impl Map {
    pub fn saltmarket() -> Map {
        let mut map = Map {
            size: SIZE,
            solids: Vec::new(),
            boxes: Vec::new(),
            trees: Vec::new(),
            wedges: Vec::new(),
            oboxes: Vec::new(),
            terrain: None,
            roads: Vec::new(),
            open_spots: Vec::new(),
            spawn_yaw: None,
            // Repair crates come down in exposed places worth fighting over:
            // the market square, the reservoir bed, the ridge, the dam's foot.
            repair_spots: vec![
                Vec2::new(80.0, 101.0),
                Vec2::new(184.0, 104.0),
                Vec2::new(60.0, 206.0),
                Vec2::new(166.0, 9.0),
            ],
        };
        let mut rng = Rng::new(0x5a17_3a2c);
        let reserved = map.street_landmarks();
        let mut drifts = map.town(&mut rng, &reserved);
        for b in &reserved {
            // The fallen water tank gets sand banked up one side: a way on top.
            if b.y1 > 4.0 && b.y1 < 5.0 {
                drifts.push(Drift {
                    b: *b,
                    axis: 0,
                    sign: -1.0,
                    height: b.y1,
                    run: 9.0,
                });
            }
        }
        let mut terrain = Terrain::from_fn(Vec2::ZERO, 2.0, 121, 121, |x, z| {
            let p = Vec2::new(x, z);
            ground(x, z) + drifts.iter().map(|d| d.height_at(p)).fold(0.0, f32::max)
        });
        terrain.color = SAND;
        terrain.material = mat::PLAIN;
        map.terrain = Some(terrain);
        map.market_hall();
        map.dam();
        map.landmarks();
        map.reservoir_cover(&mut rng);
        map.ridge();
        map.spawns();
        map.perimeter_walls();
        map
    }

    fn block_c(&mut self, min: Vec3, max: Vec3, color: Vec3) {
        self.add_block(min, max, color);
    }

    fn ramp(&mut self, min: Vec2, max: Vec2, base_y: f32, height: f32, rise: Rise, color: Vec3) {
        self.wedges.push(Wedge {
            min,
            max,
            base_y,
            height,
            rise,
            color,
        });
    }

    /// The market hall: a big drive-through landmark in the square. Four piers
    /// carry a roof slab high enough to drive under.
    fn market_hall(&mut self) {
        let c = (SQUARE_MIN + SQUARE_MAX) * 0.5;
        let (half, pier, roof_lo, roof_hi) = (12.0, 5.0, 7.5, 9.5);
        for (sx, sz) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
            let p = c + Vec2::new(sx, sz) * (half - pier * 0.5);
            self.block_c(
                Vec3::new(p.x - pier * 0.5, 0.0, p.y - pier * 0.5),
                Vec3::new(p.x + pier * 0.5, roof_lo, p.y + pier * 0.5),
                CONCRETE,
            );
        }
        self.block_c(
            Vec3::new(c.x - half, roof_lo, c.y - half),
            Vec3::new(c.x + half, roof_hi, c.y + half),
            CONCRETE * 1.04,
        );
        // A smaller drum on the roof stands in for the dome.
        self.block_c(
            Vec3::new(c.x - 6.0, roof_hi, c.y - 6.0),
            Vec3::new(c.x + 6.0, roof_hi + 3.5, c.y + 6.0),
            CONCRETE * 0.97,
        );
    }

    /// The dam across the reservoir's south end: 5 m high, breached in the
    /// middle, with a ramp up from the town and a ramp down into the bowl.
    fn dam(&mut self) {
        let (z0, z1, h) = (18.0, 30.0, 5.0);
        let (x0, x1) = (150.0, SIZE - 2.0);
        let (gap0, gap1) = (182.0, 194.0);
        self.block_c(Vec3::new(x0, 0.0, z0), Vec3::new(gap0, h, z1), DAM);
        self.block_c(Vec3::new(gap1, 0.0, z0), Vec3::new(x1, h, z1), DAM);
        // Rubble spilling out of the breach.
        self.ramp(
            Vec2::new(gap0 + 1.0, z0 - 6.0),
            Vec2::new(gap1 - 1.0, z0),
            0.0,
            1.0,
            Rise::PosZ,
            RUBBLE,
        );
        // Up from the town's south edge onto the west half of the dam.
        self.ramp(
            Vec2::new(x0 - 14.0, z0),
            Vec2::new(x0, z1),
            0.0,
            h,
            Rise::PosX,
            DAM * 0.95,
        );
        // Down from the east half into the bowl.
        self.ramp(
            Vec2::new(205.0, z1),
            Vec2::new(217.0, z1 + 16.0),
            -2.5,
            h + 2.5,
            Rise::NegZ,
            DAM * 0.95,
        );
    }

    /// Landmarks standing in the streets, placed before the buildings so the
    /// town fills in around them: the toppled water tower lying across the
    /// east lane, and a bus with its tires gone on the main road.
    fn street_landmarks(&mut self) -> Vec<OBox> {
        let mut placed = Vec::new();
        let mut put = |m: &mut Map, center: Vec2, yaw: f32, half: Vec2, y1: f32, color: Vec3| {
            let b = OBox {
                center,
                yaw,
                half,
                y0: 0.0,
                y1,
                color,
                side_mat: mat::PLAIN,
                top_mat: mat::PLAIN,
            };
            m.oboxes.push(b);
            placed.push(b);
        };
        // Water tower: the stump of its legs beside the lane, and the tank
        // lying across the road where it fell.
        let lane = (Vec2::new(130.0, 72.0) - Vec2::new(126.0, 94.0)).normalize();
        put(
            self,
            Vec2::new(140.0, 84.0),
            yaw_along(lane),
            Vec2::new(3.0, 3.0),
            3.0,
            RUST,
        );
        put(
            self,
            Vec2::new(129.0, 82.0),
            yaw_along(Vec2::new(lane.y, -lane.x)) + 0.3,
            Vec2::new(5.0, 3.2),
            4.5,
            RUST * 1.1,
        );
        // The bus, pulled half onto the verge of the main road.
        let road = (Vec2::new(48.0, 121.0) - Vec2::new(30.0, 112.0)).normalize();
        let nrm = Vec2::new(-road.y, road.x);
        put(
            self,
            Vec2::new(39.0, 116.5) + nrm * 3.5,
            yaw_along(road),
            Vec2::new(5.5, 1.3),
            3.2,
            Vec3::new(0.62, 0.52, 0.34),
        );
        placed
    }

    fn landmarks(&mut self) {
        // The half-buried war machine in the reservoir bed: hull sunk into the
        // floor, turret on top, the gun barrel reaching west toward the town.
        let floor = -BOWL_DEPTH;
        let (cx, cz) = (200.0, 150.0);
        self.block_c(
            Vec3::new(cx - 8.0, floor - 2.0, cz - 5.0),
            Vec3::new(cx + 8.0, floor + 4.5, cz + 5.0),
            WRECK,
        );
        self.block_c(
            Vec3::new(cx - 3.5, floor + 4.5, cz - 3.5),
            Vec3::new(cx + 3.5, floor + 7.5, cz + 3.5),
            WRECK * 1.05,
        );
        self.block_c(
            Vec3::new(cx - 18.0, floor + 5.2, cz - 0.7),
            Vec3::new(cx - 3.5, floor + 6.6, cz + 0.7),
            WRECK * 0.9,
        );
        // A sand drift banked against its west flank: the way up.
        self.ramp(
            Vec2::new(cx - 15.0, cz - 4.0),
            Vec2::new(cx - 8.0, cz + 4.0),
            floor,
            4.5,
            Rise::PosX,
            SAND,
        );
    }

    /// Cover in the reservoir bed, so the open lane has things to fight around:
    /// a pier out from the town shore, a half-buried pipeline, two stranded
    /// boats and rock outcrops.
    fn reservoir_cover(&mut self, rng: &mut Rng) {
        let at = |x: f32, z: f32| ground(x, z);
        // Pier: a deck at town level running out over the shore, on piles.
        let (pz0, pz1) = (136.0, 142.0);
        self.block_c(
            Vec3::new(146.0, -1.0, pz0),
            Vec3::new(176.0, 0.4, pz1),
            RUST * 0.8,
        );
        for x in [154.0f32, 162.0, 170.0] {
            let g = at(x, pz0);
            self.block_c(
                Vec3::new(x - 0.6, g - 0.5, pz0 + 0.5),
                Vec3::new(x + 0.6, -1.0, pz0 + 1.7),
                RUST * 0.7,
            );
            self.block_c(
                Vec3::new(x - 0.6, g - 0.5, pz1 - 1.7),
                Vec3::new(x + 0.6, -1.0, pz1 - 0.5),
                RUST * 0.7,
            );
        }
        // Pipeline: segments half sunk in the bed, with gaps to drive through.
        for i in 0..6 {
            let z0 = 55.0 + i as f32 * 11.0;
            let g = at(172.0, z0 + 4.0);
            self.block_c(
                Vec3::new(170.5, g - 0.8, z0),
                Vec3::new(173.5, g + 1.4, z0 + 8.0),
                RUST,
            );
        }
        // Stranded boats: hull block with a bow ramp, lying in the mud.
        for (x, z, along_x) in [(213.0f32, 70.0f32, false), (180.0, 188.0, true)] {
            let g = at(x, z);
            let (hx, hz) = if along_x { (6.0, 2.2) } else { (2.2, 6.0) };
            self.block_c(
                Vec3::new(x - hx, g - 0.5, z - hz),
                Vec3::new(x + hx, g + 2.4, z + hz),
                Vec3::new(0.44, 0.4, 0.36),
            );
            if along_x {
                self.ramp(
                    Vec2::new(x + hx, z - hz),
                    Vec2::new(x + hx + 4.0, z + hz),
                    g - 0.5,
                    2.9,
                    Rise::NegX,
                    SAND,
                );
            } else {
                self.ramp(
                    Vec2::new(x - hz, z - hx - 4.0),
                    Vec2::new(x + hz, z - hx),
                    g - 0.5,
                    2.9,
                    Rise::PosZ,
                    SAND,
                );
            }
        }
        // Rock outcrops.
        for (x, z) in [
            (196.0f32, 92.0f32),
            (184.0, 120.0),
            (208.0, 126.0),
            (190.0, 60.0),
            (205.0, 180.0),
            (176.0, 160.0),
        ] {
            let g = at(x, z);
            let (w, d, h) = (
                rng.range(3.0, 6.0),
                rng.range(3.0, 6.0),
                rng.range(2.5, 4.5),
            );
            self.block_c(
                Vec3::new(x - w, g - 1.0, z - d),
                Vec3::new(x + w, g + h, z + d),
                Vec3::new(0.62, 0.52, 0.4),
            );
        }
    }

    /// The dune ridge along the north edge: a bunker, a radio mast stump and rocks.
    fn ridge(&mut self) {
        let at = |x: f32, z: f32| ground(x, z);
        let g = at(150.0, 216.0);
        self.block_c(
            Vec3::new(142.0, g - 1.0, 212.0),
            Vec3::new(156.0, g + 2.8, 220.0),
            DAM,
        );
        let g = at(96.0, 222.0);
        self.block_c(
            Vec3::new(95.0, g - 0.5, 221.0),
            Vec3::new(97.0, g + 11.0, 223.0),
            RUST,
        );
        for (x, z, w, h) in [
            (30.0f32, 220.0f32, 4.0f32, 3.0f32),
            (70.0, 214.0, 3.0, 2.5),
            (126.0, 224.0, 5.0, 3.5),
            (190.0, 220.0, 4.0, 3.0),
            (222.0, 214.0, 3.0, 2.5),
        ] {
            let g = at(x, z);
            self.block_c(
                Vec3::new(x - w, g - 1.0, z - w * 0.8),
                Vec3::new(x + w, g + h, z + w * 0.8),
                Vec3::new(0.62, 0.52, 0.4),
            );
        }
    }

    /// Drop zones around the edges, plus street intersections for bots to roam to.
    fn spawns(&mut self) {
        let drops = [
            Vec2::new(18.0, 40.0),
            Vec2::new(18.0, 120.0),
            Vec2::new(18.0, 160.0),
            Vec2::new(40.0, 212.0),
            Vec2::new(110.0, 214.0),
            Vec2::new(205.0, 220.0),
            Vec2::new(215.0, 100.0),
            Vec2::new(165.0, 8.0),
            Vec2::new(90.0, 8.0),
        ];
        let mut spots: Vec<Vec2> = drops.to_vec();
        // Roam points along every road.
        for (pts, _) in ROADS {
            for seg in pts.windows(2) {
                spots.push((Vec2::from(seg[0]) + Vec2::from(seg[1])) * 0.5);
            }
        }
        spots.push((SQUARE_MIN + SQUARE_MAX) * 0.5 + Vec2::new(0.0, -16.0));
        spots.push(Vec2::new(175.0, 80.0));
        spots.push(Vec2::new(185.0, 175.0));
        // Keep only spots clear of anything solid (with room for a tank).
        let clear = |p: Vec2, m: &Map| {
            m.solids.iter().all(|s| {
                let c = p.clamp(s.min2(), s.max2());
                c.distance(p) >= TANK_CLEAR + 0.5
            }) && m.oboxes.iter().all(|o| o.distance(p) >= TANK_CLEAR + 0.5)
                && m.wedges.iter().all(|w| {
                    let c = p.clamp(w.min, w.max);
                    c.distance(p) >= TANK_CLEAR
                })
        };
        let kept: Vec<Vec2> = spots.into_iter().filter(|&p| clear(p, self)).collect();
        self.open_spots = kept;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_spawns_everywhere_and_they_are_clear() {
        let m = Map::saltmarket();
        assert!(
            m.open_spots.len() >= 12,
            "only {} spawn spots",
            m.open_spots.len()
        );
        // At least one drop zone in each region: town west, ridge, reservoir/dam side.
        assert!(m.open_spots.iter().any(|p| p.x < 30.0));
        assert!(m.open_spots.iter().any(|p| p.y > 200.0));
        assert!(m.open_spots.iter().any(|p| p.x > 150.0));
        // Repair drop zones sit on open ground (a crate has to land and be reachable).
        for &p in &m.repair_spots {
            assert!(
                m.oboxes.iter().all(|o| o.distance(p) > 4.0),
                "repair spot {p} touches a building"
            );
            assert!(
                m.solids
                    .iter()
                    .all(|s| p.clamp(s.min2(), s.max2()).distance(p) > 4.0),
                "repair spot {p} touches a solid"
            );
            assert!(
                m.wedges
                    .iter()
                    .all(|w| p.clamp(w.min, w.max).distance(p) > 2.0),
                "repair spot {p} on a ramp"
            );
        }
    }

    /// A rotated box blocks exactly where its footprint says (physics, nav and
    /// rendering all share OBox's rotation convention).
    #[test]
    fn rotated_boxes_collide_where_they_are() {
        let mut map = Map::empty(100.0);
        let b = OBox {
            center: Vec2::new(50.0, 50.0),
            yaw: 0.7,
            half: Vec2::new(6.0, 2.0),
            y0: 0.0,
            y1: 4.0,
            color: CONCRETE,
            side_mat: mat::WALL,
            top_mat: mat::ROOF,
        };
        map.oboxes.push(b);
        let w = crate::sim::World::empty(map);
        for k in 0..16 {
            let a = k as f32 / 16.0 * std::f32::consts::TAU;
            let dir = Vec2::new(a.cos(), a.sin());
            let origin = b.center - dir * 20.0;
            let hit = w.raycast(
                Vec3::new(origin.x, 1.0, origin.y),
                Vec3::new(dir.x, 0.0, dir.y),
                40.0,
            );
            // Expected: first point along the ray inside the footprint.
            let expect = (0..4000)
                .map(|i| i as f32 * 0.01)
                .find(|&t| b.distance(origin + dir * t) == 0.0)
                .unwrap();
            assert!(
                (hit - expect).abs() < 0.05,
                "angle {a}: hit at {hit}, footprint at {expect}"
            );
        }
    }

    #[test]
    fn town_is_not_a_grid() {
        let m = Map::saltmarket();
        assert!(m.oboxes.len() > 150, "only {} town pieces", m.oboxes.len());
        // Plenty of distinct building orientations.
        let mut yaws: Vec<i32> = m
            .oboxes
            .iter()
            .map(|o| ((o.yaw.rem_euclid(std::f32::consts::FRAC_PI_2)) * 20.0) as i32)
            .collect();
        yaws.sort();
        yaws.dedup();
        assert!(yaws.len() > 10, "only {} distinct orientations", yaws.len());
    }

    #[test]
    fn bowl_is_sunken_and_shores_are_drivable() {
        assert!(ground(BOWL_CENTER.x, BOWL_CENTER.y) < -BOWL_DEPTH + 0.6);
        assert!(ground(60.0, 60.0).abs() < 0.3, "town should be flat");
        // Steepest slope anywhere stays under ~22 degrees.
        let mut worst = 0.0f32;
        for i in 0..120 {
            for j in 0..120 {
                let (x, z) = (i as f32 * 2.0, j as f32 * 2.0);
                let s = ((ground(x + 1.0, z) - ground(x, z)).powi(2)
                    + (ground(x, z + 1.0) - ground(x, z)).powi(2))
                .sqrt();
                worst = worst.max(s.atan().to_degrees());
            }
        }
        assert!(worst < 22.0, "slope of {worst} degrees");
    }
}
