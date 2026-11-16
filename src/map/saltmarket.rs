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

use crate::render::material::Material;
use glam::{Vec2, Vec3};

use super::ruins::{self, Drift};
use super::{DEFAULT_SIDES, Map, OBox, Rise, Terrain, Wedge};
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

/// Signed distance to the reservoir's rounded-rectangle outline (negative inside).
fn bowl_sdf(p: Vec2) -> f32 {
    let q = (p - BOWL_CENTER).abs() - BOWL_HALF + Vec2::splat(BOWL_ROUND);
    q.max(Vec2::ZERO).length() + q.x.max(q.y).min(0.0) - BOWL_ROUND
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How far outside the arena a point is (0 or less: inside).
fn edge_distance(p: Vec2) -> f32 {
    let q = (p - Vec2::splat(SIZE * 0.5)).abs() - Vec2::splat(SIZE * 0.5);
    q.max(Vec2::ZERO).length() + q.x.max(q.y).min(0.0)
}

/// Visual dunes outside the arena: meets the play ground at the edge, rises
/// into a berm, then rolls. Inside, it hides under the play terrain.
fn scenery_ground(x: f32, z: f32) -> f32 {
    let p = Vec2::new(x, z);
    let d = edge_distance(p);
    if d < -0.01 {
        return -8.0;
    }
    let edge = p.clamp(Vec2::ZERO, Vec2::splat(SIZE));
    let base = ground(edge.x, edge.y);
    // The berm rises and falls along the edge, so the arena doesn't read as a
    // rectangular pit.
    let along = edge.x * 0.9 + edge.y * 1.1;
    let vary = 0.55 + 0.3 * (along * 0.045).sin() + 0.25 * (along * 0.11 + 1.3).sin();
    let rise = 10.0 + 8.0 * (along * 0.07).sin().abs();
    let berm = 7.5 * vary * smoothstep(0.0, rise, d) * (1.0 - 0.45 * smoothstep(20.0, 60.0, d));
    let dunes = smoothstep(10.0, 40.0, d)
        * (6.0 * (x * 0.021 + (z * 0.013).sin() * 2.0).sin().abs()
            + 3.0 * ((x * 0.007 - z * 0.011).sin() + 1.0)
            + 1.5 * (x * 0.05).sin() * (z * 0.043).cos());
    base + berm + dunes
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
            side_mat: Material::Wall,
            top_mat: Material::Roof,
            ..OBox::default()
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
        for (i, lot) in lots.iter().enumerate() {
            // Ground in front of a collapsed floor must be free of other buildings.
            let others: Vec<OBox> = lots
                .iter()
                .enumerate()
                .filter(|&(j, _)| j != i)
                .map(|(_, o)| *o)
                .chain(reserved.iter().copied())
                .collect();
            let clear = |p: Vec2| {
                others.iter().all(|o| o.distance(p) > 1.5)
                    && bowl_sdf(p) > 2.0
                    && p.cmpgt(Vec2::splat(6.0)).all()
                    && p.cmplt(Vec2::splat(SIZE - 6.0)).all()
            };
            self.ruin(rng, *lot, &roads, &clear, &mut drifts);
        }
        drifts
    }

    /// A box in a lot's local frame: `min`/`max` are local footprint coords.
    #[allow(clippy::too_many_arguments)]
    fn local_box(
        &mut self,
        lot: &OBox,
        min: Vec2,
        max: Vec2,
        y0: f32,
        y1: f32,
        color: Vec3,
        material: Material,
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
            side_mat: material,
            top_mat: material,
            ..OBox::default()
        };
        self.oboxes.push(b);
        b
    }

    /// One building: standing (sometimes stepped), a gutted shell with a
    /// jagged broken top and a breach toward the street, a low stub with sand
    /// banked up to its roof, or a collapsed rubble heap you can drive over.
    fn ruin(
        &mut self,
        rng: &mut Rng,
        lot: OBox,
        roads: &RoadNet,
        clear: &dyn Fn(Vec2) -> bool,
        drifts: &mut Vec<Drift>,
    ) {
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
        if roll < 0.15 {
            // Low stub, sand banked up its street face to the roof: a way up.
            let height = 2.2;
            self.local_box(
                &lot,
                -h,
                h,
                0.0,
                height,
                RUIN * rng.range(0.9, 1.0),
                Material::Plaster,
            );
            drifts.push(Drift {
                b: lot,
                axis: front_axis,
                sign: front_sign,
                height: height + 0.15,
                run: 8.0,
            });
        } else if roll < 0.25 {
            // Collapsed to a stepped rubble heap (each step low enough to climb),
            // one wall stub still standing.
            for (k, s) in [1.0f32, 0.72, 0.45].iter().enumerate() {
                let y0 = k as f32 * 0.45;
                self.local_box(
                    &lot,
                    -h * *s,
                    h * *s,
                    y0,
                    y0 + 0.45,
                    RUBBLE * (1.0 - 0.04 * k as f32),
                    Material::Rubble,
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
                Material::Plaster,
            );
        } else {
            // A real building, built then damaged (see ruins.rs).
            drifts.extend(ruins::building(
                self,
                rng,
                lot,
                (front_axis, front_sign),
                clear,
            ));
            return;
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
            scenery_terrain: None,
            roads: Vec::new(),
            open_spots: Vec::new(),
            spawn_yaw: None,
            // Late afternoon on a desert world: low warm sun, dusty sky, sand haze.
            atmosphere: crate::render::Atmosphere {
                sun_dir: Vec3::new(0.62, 0.34, 0.42),
                sun_color: Vec3::new(2.7, 2.1, 1.45),
                sky_zenith: Vec3::new(0.26, 0.4, 0.62),
                sky_horizon: Vec3::new(0.72, 0.62, 0.48),
                ground_color: Vec3::new(0.36, 0.28, 0.19),
                fog_density: 0.0032,
                // Wind blows along the ripples' fall line, carrying sand.
                wind: Vec2::new(5.0, 2.3),
                blowing_sand: 1.0,
                dust: Vec3::new(0.74, 0.62, 0.45),
            },
            bases: Default::default(),
            factions: DEFAULT_SIDES,
            // Repair crates come down in exposed places worth fighting over:
            // the market square, the reservoir bed, the ridge, the dam's foot.
            repair_spots: vec![
                Vec2::new(80.0, 101.0),
                Vec2::new(184.0, 104.0),
                Vec2::new(60.0, 206.0),
                Vec2::new(166.0, 9.0),
            ],
            hold_site: None,
            cores: Vec::new(),
            crystals: Vec::new(),
            glow_paths: Vec::new(),
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
        terrain.material = Material::Sand;
        map.terrain = Some(terrain);
        map.market_hall(&mut rng);
        map.traces(&mut rng);
        map.dam(&mut rng);
        map.landmarks();
        map.reservoir_cover(&mut rng);
        map.ridge();
        map.spawns();
        // The edge is dunes, not a wall: colliders stay, the grey wall goes.
        map.perimeter(false);
        map.horizon(&mut rng);
        // Everything masonry is knocked about a bit (visual only).
        for o in &mut map.oboxes {
            let masonry =
                [Material::Plaster, Material::Concrete, Material::Rubble].contains(&o.side_mat);
            if o.solid && o.tilt == 0.0 && o.chip == 0.0 && masonry {
                o.chip = 0.25;
            }
        }
        map.drop_tiny_hitboxes();
        map.separate_coplanar_faces();
        map
    }

    /// Small things on the ground (wall stubs, rubble lumps lower than a road
    /// wheel's travel, thin specks) shouldn't stop or jolt a tank: they keep
    /// their looks but lose their colliders. Taller pieces stay solid and the
    /// suspension takes them.
    fn drop_tiny_hitboxes(&mut self) {
        for o in &mut self.oboxes {
            let g = ground(o.center.x, o.center.y);
            let on_ground = o.y0 <= g + 0.3;
            let low = o.y1 - g < 0.45;
            let speck = o.half.x.max(o.half.y) < 0.35 && o.y1 - g < 1.2;
            if o.solid && !o.ramp && o.tilt == 0.0 && on_ground && (low || speck) {
                o.solid = false;
            }
        }
    }

    /// Pieces built flush against each other share face planes, which flicker
    /// (z-fight). Grow each piece by a different few millimeters (cycling
    /// through 11 sizes, so neighbors built one after another always differ)
    /// so no two faces are ever exactly coplanar. Invisible at play scale.
    fn separate_coplanar_faces(&mut self) {
        // Each piece gets a "color" 0..29 different from every piece it
        // touches (greedy, over a spatial grid), and its offsets come from the
        // color: sideways c, up 13c+3, down 17c+8 (mod 29, 0.45 mm steps).
        // 13 and 17 are invertible mod 29, so touching pieces differ in all
        // three, guaranteed rather than likely.
        const COLORS: usize = 29;
        let step = |c: usize| 0.002 + 0.00045 * (c % COLORS) as f32;
        let side = |c: usize| step(c * 11);
        let up = |c: usize| step(c * 13 + 3);
        let down = |c: usize| step(c * 17 + 8);
        // Footprint circles for every piece (rotated boxes, then plain boxes).
        let mut shapes: Vec<(Vec2, f32, f32, f32)> = self
            .oboxes
            .iter()
            .map(|o| (o.center, o.half.length(), o.y0, o.y1))
            .collect();
        shapes.extend(self.boxes.iter().map(|b| {
            let c = Vec2::new(b.min.x + b.max.x, b.min.z + b.max.z) * 0.5;
            (
                c,
                (Vec2::new(b.max.x - b.min.x, b.max.z - b.min.z) * 0.5).length(),
                b.min.y,
                b.max.y,
            )
        }));
        let cell = 16.0;
        let key = |p: Vec2| ((p.x / cell).floor() as i32, (p.y / cell).floor() as i32);
        let mut grid: std::collections::HashMap<(i32, i32), Vec<usize>> =
            std::collections::HashMap::new();
        for (i, s) in shapes.iter().enumerate() {
            grid.entry(key(s.0)).or_default().push(i);
        }
        let mut color = vec![usize::MAX; shapes.len()];
        for i in 0..shapes.len() {
            let (c, r, y0, y1) = shapes[i];
            let mut used = [false; COLORS];
            let (kx, kz) = key(c);
            let reach = (r / cell).ceil() as i32 + 1;
            for dx in -reach..=reach {
                for dz in -reach..=reach {
                    for &j in grid.get(&(kx + dx, kz + dz)).into_iter().flatten() {
                        let (cj, rj, y0j, y1j) = shapes[j];
                        let touch =
                            c.distance(cj) < r + rj + 0.05 && y0 < y1j + 0.05 && y0j < y1 + 0.05;
                        if j != i && color[j] != usize::MAX && touch {
                            used[color[j]] = true;
                        }
                    }
                }
            }
            color[i] = (0..COLORS).find(|&k| !used[k]).unwrap_or(i % COLORS);
        }
        let n = self.oboxes.len();
        for (i, o) in self.oboxes.iter_mut().enumerate() {
            let k = color[i];
            o.half += Vec2::splat(side(k));
            o.y1 += up(k);
            // Bottoms in the air (slab undersides, upper storeys) too; ones on
            // the ground stay put.
            if o.y0 > ground(o.center.x, o.center.y) + 0.05 {
                o.y0 -= down(k);
            }
        }
        for (i, b) in self.boxes.iter_mut().enumerate() {
            let k = color[n + i];
            let (e, u) = (side(k), up(k));
            b.min -= Vec3::new(e, 0.0, e);
            b.max += Vec3::new(e, u, e);
        }
    }

    fn block_c(&mut self, min: Vec3, max: Vec3, color: Vec3, material: Material) {
        // Masonry and stone are worn rotated boxes (chipped, visual only);
        // rocks get bigger chips so they read as stone, not blocks.
        if [Material::Concrete, Material::Rubble, Material::Plaster].contains(&material) {
            let size = max - min;
            self.oboxes.push(OBox {
                center: Vec2::new(min.x + max.x, min.z + max.z) * 0.5,
                half: Vec2::new(size.x, size.z) * 0.5,
                y0: min.y,
                y1: max.y,
                color,
                side_mat: material,
                top_mat: material,
                // Big blocks get bigger chips (a fixed size reads as perfect at scale).
                chip: if material == Material::Rubble {
                    0.9
                } else {
                    (0.3 + 0.08 * size.x.min(size.z)).min(1.0)
                },
                ..OBox::default()
            });
            return;
        }
        self.boxes.push(super::VisBox {
            min,
            max,
            color,
            side_mat: material,
            top_mat: material,
        });
        self.solids.push(super::Solid { min, max });
    }

    #[allow(clippy::too_many_arguments)]
    fn ramp(
        &mut self,
        min: Vec2,
        max: Vec2,
        base_y: f32,
        height: f32,
        rise: Rise,
        color: Vec3,
        material: Material,
    ) {
        self.wedges.push(Wedge {
            min,
            max,
            base_y,
            height,
            rise,
            color,
            material: Some(material),
        });
    }

    /// The market hall: a big drive-through landmark in the square. Four piers
    /// carry a roof slab high enough to drive under.
    fn market_hall(&mut self, rng: &mut Rng) {
        self.market(rng, (SQUARE_MIN + SQUARE_MAX) * 0.5);
    }

    /// The dam across the reservoir's south end: 5 m high, breached in the
    /// middle, with a ramp up from the town and a ramp down into the bowl.
    fn dam(&mut self, rng: &mut Rng) {
        let (z0, z1, h) = (18.0, 30.0, 5.0);
        let (x0, x1) = (150.0, SIZE - 2.0);
        let (gap0, gap1) = (182.0, 194.0);
        self.concrete_dam(rng, x0, x1, z0, z1, h, (gap0, gap1));
        // Rubble spilling out of the breach.
        self.ramp(
            Vec2::new(gap0 + 1.0, z0 - 6.0),
            Vec2::new(gap1 - 1.0, z0),
            0.0,
            1.0,
            Rise::PosZ,
            RUBBLE,
            Material::Rubble,
        );
        // Up from the town's south edge onto the west half of the dam.
        self.ramp(
            Vec2::new(x0 - 14.0, z0),
            Vec2::new(x0, z1),
            0.0,
            h,
            Rise::PosX,
            DAM * 0.95,
            Material::Concrete,
        );
        // Down from the east half into the bowl.
        self.ramp(
            Vec2::new(205.0, z1),
            Vec2::new(217.0, z1 + 16.0),
            -2.5,
            h + 2.5,
            Rise::NegZ,
            DAM * 0.95,
            Material::Concrete,
        );
    }

    /// Landmarks standing in the streets, placed before the buildings so the
    /// town fills in around them: the toppled water tower lying across the
    /// east lane, and a bus with its tires gone on the main road.
    fn street_landmarks(&mut self) -> Vec<OBox> {
        let mut placed = Vec::new();
        let mut put = |m: &mut Map,
                       center: Vec2,
                       yaw: f32,
                       half: Vec2,
                       y1: f32,
                       color: Vec3,
                       material: Material| {
            let b = OBox {
                center,
                yaw,
                half,
                y0: 0.0,
                y1,
                color,
                side_mat: material,
                top_mat: material,
                ..OBox::default()
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
            0.8,
            DAM,
            Material::Concrete,
        );
        let footing = OBox {
            center: Vec2::new(140.0, 84.0),
            yaw: yaw_along(lane),
            half: Vec2::splat(3.0),
            ..OBox::default()
        };
        let (fx, fz) = footing.axes();
        for (sx, sz, h) in [
            (-1.0f32, -1.0f32, 3.2f32),
            (1.0, -1.0, 2.1),
            (-1.0, 1.0, 1.4),
            (1.0, 1.0, 2.7),
        ] {
            let c = footing.center + fx * sx * 2.2 + fz * sz * 2.2;
            self.oboxes.push(OBox {
                center: c,
                yaw: footing.yaw,
                half: Vec2::splat(0.35),
                y0: 0.8,
                y1: 0.8 + h,
                color: RUST,
                side_mat: Material::Rust,
                top_mat: Material::Rust,
                ..OBox::default()
            });
        }
        put(
            self,
            Vec2::new(129.0, 82.0),
            yaw_along(Vec2::new(lane.y, -lane.x)) + 0.3,
            Vec2::new(5.0, 3.2),
            4.5,
            Vec3::new(0.58, 0.68, 0.64),
            Material::Painted,
        );
        // Riveted bands around the fallen tank.
        let tank = *placed.last().unwrap();
        let (tx, tz) = tank.axes();
        for k in [-3.6f32, -1.2, 1.2, 3.6] {
            self.oboxes.push(OBox {
                center: tank.center + tx * k,
                yaw: tank.yaw,
                half: Vec2::new(0.22, tank.half.y + 0.07),
                y0: -0.1,
                y1: tank.y1 + 0.07,
                color: RUST * 0.8,
                side_mat: Material::Rust,
                top_mat: Material::Rust,
                ..OBox::default()
            });
        }
        let _ = tz;
        // The bus, pulled half onto the verge of the main road.
        let road = (Vec2::new(48.0, 121.0) - Vec2::new(30.0, 112.0)).normalize();
        let nrm = Vec2::new(-road.y, road.x);
        let bus = OBox {
            center: Vec2::new(39.0, 116.5) + nrm * 3.5,
            yaw: yaw_along(road),
            half: Vec2::new(5.5, 1.3),
            y0: 0.0,
            y1: 2.8,
            ..OBox::default()
        };
        placed.push(bus);
        self.bus(bus);
        placed
    }

    /// A bus with its tires long gone: lower body, window pillars over a dark
    /// gutted interior, roof, and bare wheel rims. `b` is its footprint.
    fn bus(&mut self, b: OBox) {
        let (ax, az) = b.axes();
        let paint = Vec3::new(0.56, 0.66, 0.74); // faded blue
        let cream = Vec3::new(0.84, 0.8, 0.68);
        let dark = Vec3::new(0.12, 0.11, 0.1);
        let part =
            |m: &mut Map, lo: Vec3, hi: Vec3, color: Vec3, material: Material, solid: bool| {
                let mid = (lo + hi) * 0.5;
                m.oboxes.push(OBox {
                    center: b.center + ax * mid.x + az * mid.z,
                    yaw: b.yaw,
                    half: Vec2::new((hi.x - lo.x) * 0.5, (hi.z - lo.z) * 0.5),
                    y0: lo.y,
                    y1: hi.y,
                    color,
                    side_mat: material,
                    top_mat: material,
                    solid,
                    ..OBox::default()
                });
            };
        let (l, w) = (b.half.x, b.half.y);
        part(
            self,
            Vec3::new(-l, 0.45, -w),
            Vec3::new(l, 1.5, w),
            paint,
            Material::Painted,
            true,
        );
        part(
            self,
            Vec3::new(-l + 0.3, 0.12, -w + 0.15),
            Vec3::new(l - 0.3, 0.45, w - 0.15),
            dark,
            Material::Rust,
            true,
        );
        part(
            self,
            Vec3::new(-l + 0.15, 1.45, -w + 0.2),
            Vec3::new(l - 0.15, 2.5, w - 0.2),
            dark,
            Material::Rubble,
            false,
        );
        let pillars = 9;
        for i in 0..pillars {
            let x = -l + 0.1 + i as f32 * (2.0 * l - 0.3) / (pillars - 1) as f32;
            part(
                self,
                Vec3::new(x, 1.5, -w),
                Vec3::new(x + 0.18, 2.55, w),
                cream,
                Material::Painted,
                false,
            );
        }
        part(
            self,
            Vec3::new(-l, 2.55, -w),
            Vec3::new(l, 2.8, w),
            cream,
            Material::Painted,
            true,
        );
        for sx in [-3.4f32, 3.4] {
            for sz in [-1.0f32, 1.0] {
                let z = sz * (w + 0.02);
                part(
                    self,
                    Vec3::new(sx - 0.4, 0.0, z.min(z - sz * 0.25)),
                    Vec3::new(sx + 0.4, 0.8, z.max(z - sz * 0.25)),
                    dark * 2.5,
                    Material::Rust,
                    false,
                );
            }
        }
    }

    fn landmarks(&mut self) {
        // The war machine, sunk nose-first into the reservoir bed, gun
        // toward the town.
        let floor = -BOWL_DEPTH;
        let (cx, cz) = (200.0, 150.0);
        let mut wrng = Rng::new(0x3a77);
        let deck = self.war_machine(&mut wrng, Vec3::new(cx, floor + 1.0, cz), -1.75);
        // A sand drift banked against its stern: the way up onto the engine
        // deck.
        self.ramp(
            Vec2::new(deck.x + 0.5, deck.z - 3.0),
            Vec2::new(deck.x + 10.0, deck.z + 3.0),
            floor - 0.3,
            deck.y - floor + 0.2,
            Rise::NegX,
            SAND,
            Material::Sand,
        );
    }

    /// Cover in the reservoir bed, so the open lane has things to fight around:
    /// a pier out from the town shore, a half-buried pipeline, two stranded
    /// boats and rock outcrops.
    fn reservoir_cover(&mut self, rng: &mut Rng) {
        let at = |x: f32, z: f32| ground(x, z);
        // Pier: a timber deck at town level running out over the shore.
        self.pier(rng, Vec2::new(146.0, 139.0), Vec2::X, 31.0, 6.0, 0.4);
        // Pipeline: runs half sunk in the bed, with gaps to drive through.
        for i in 0..6 {
            self.pipeline_run(rng, Vec2::new(172.0, 55.0 + i as f32 * 11.0), 8.0, 1.1);
        }
        // Stranded boats, listing in the mud.
        self.boat(rng, Vec2::new(213.0, 70.0), Vec2::Y, 12.0);
        self.boat(rng, Vec2::new(180.0, 188.0), Vec2::new(-1.0, 0.0), 12.0);
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
                Material::Rubble,
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
            Material::Concrete,
        );
        let g = at(96.0, 222.0);
        self.block_c(
            Vec3::new(95.0, g - 0.5, 221.0),
            Vec3::new(97.0, g + 11.0, 223.0),
            RUST,
            Material::Rust,
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
                Material::Rubble,
            );
        }
    }

    /// Traces of the people who lived here (see props.rs): power poles with
    /// cables along the main roads, burned-out cars, market stalls around the
    /// hall, dead palms.
    fn traces(&mut self, rng: &mut Rng) {
        let hall = SQUARE_C;
        // Free ground: clear of buildings, collapsed-floor approaches, repair pads.
        let free = |m: &Map, p: Vec2, r: f32| {
            m.oboxes.iter().filter(|o| o.solid).all(|o| {
                let reach = if o.is_ramp() { r + 7.0 } else { r };
                o.distance(p) > reach
            }) && m
                .solids
                .iter()
                .all(|s| p.clamp(s.min2(), s.max2()).distance(p) > r)
                && m.repair_spots.iter().all(|q| q.distance(p) > 5.0)
                && bowl_sdf(p) > 4.0
                && edge_distance(p) < -6.0
        };

        // Power poles along one side of the main roads, wired to their neighbors.
        for (pts, w) in ROADS.iter().take(4) {
            let mut last: Option<[Vec3; 2]> = None;
            for seg in pts.windows(2) {
                let (a, b) = (Vec2::from(seg[0]), Vec2::from(seg[1]));
                let dir = (b - a).normalize();
                let nrm = Vec2::new(-dir.y, dir.x);
                let len = a.distance(b);
                let mut t = 6.0;
                while t < len {
                    let p = a + dir * t + nrm * (w * 0.5 + 1.4);
                    t += 22.0;
                    if !free(self, p, 1.2) {
                        last = None;
                        continue;
                    }
                    let tops = self.pole(rng, p, dir);
                    if let (Some(prev), Some(now)) = (last, tops)
                        && prev[0].distance(now[0]) < 32.0
                    {
                        for k in 0..2 {
                            let sag = rng.range(0.4, 1.0);
                            self.cable(prev[k], now[k], sag);
                        }
                    }
                    last = tops;
                }
            }
        }

        // Burned-out cars, some slewed across the road.
        let mut cars = 0;
        for _ in 0..300 {
            if cars >= 11 {
                break;
            }
            let (pts, w) = ROADS[rng.index(ROADS.len())];
            let k = rng.index(pts.len() - 1);
            let (a, b) = (Vec2::from(pts[k]), Vec2::from(pts[k + 1]));
            let dir = (b - a).normalize();
            let nrm = Vec2::new(-dir.y, dir.x);
            let p = a.lerp(b, rng.range(0.15, 0.85)) + nrm * rng.range(-0.3, 0.3) * w;
            if !free(self, p, 3.2) || p.distance(hall) < SQUARE_R + 2.0 {
                continue;
            }
            let yaw = dir.x.atan2(dir.y)
                + if rng.chance(0.3) {
                    rng.range(-1.2, 1.2)
                } else {
                    rng.range(-0.15, 0.15)
                };
            self.car(rng, p, yaw);
            cars += 1;
        }

        // Market stalls along the hall's sides, clear of the drive-through.
        for side in 0..4 {
            let a = side as f32 * std::f32::consts::FRAC_PI_2;
            let out = Vec2::new(a.cos(), a.sin());
            let along = Vec2::new(-out.y, out.x);
            for s in [-1.0f32, 1.0] {
                let p = hall + out * 15.5 + along * s * 7.5;
                if !free(self, p, 2.6) {
                    continue;
                }
                self.stall(rng, p, out.x.atan2(out.y));
            }
        }

        // Dead palms: a ring around the square, a few along the main road.
        let mut palms = 0;
        for _ in 0..200 {
            if palms >= 9 {
                break;
            }
            let p = if palms < 5 {
                let a = rng.range(0.0, std::f32::consts::TAU);
                hall + Vec2::new(a.cos(), a.sin()) * rng.range(18.5, 21.0)
            } else {
                let (pts, w) = ROADS[0];
                let k = rng.index(pts.len() - 1);
                let (a, b) = (Vec2::from(pts[k]), Vec2::from(pts[k + 1]));
                let nrm = (b - a).normalize().perp();
                a.lerp(b, rng.f32()) - nrm * (w * 0.5 + 1.2)
            };
            if free(self, p, 1.5) {
                self.palm(rng, p);
                palms += 1;
            }
        }
    }

    /// Beyond the arena, visual only: sand rises into a berm at the edge,
    /// rolls away in dunes to the haze, with the half-buried ruins of the rest
    /// of the town on them and mesas on the horizon.
    fn horizon(&mut self, rng: &mut Rng) {
        // Dunes on a 4 m grid out to ~560 m past each edge (the camera's far
        // plane is 600 m). Inside the arena it sits well below the play
        // terrain; the 0 and 240 m grid lines match the arena edge, where it
        // meets the play ground and rises.
        let (cell, reach) = (4.0f32, 560.0f32);
        let n = ((SIZE + 2.0 * reach) / cell) as usize + 1;
        let mut t = Terrain::from_fn(Vec2::splat(-reach), cell, n, n, scenery_ground);
        t.color = SAND;
        t.material = Material::Sand;
        self.scenery_terrain = Some(t);

        // Half-buried ruins out on the dunes.
        let first = self.oboxes.len();
        let mut placed: Vec<Vec2> = Vec::new();
        for _ in 0..400 {
            if placed.len() >= 34 {
                break;
            }
            let a = rng.range(0.0, std::f32::consts::TAU);
            let d = rng.range(25.0, 230.0);
            let c = Vec2::splat(SIZE * 0.5)
                + Vec2::new(a.cos(), a.sin()) * (SIZE * 0.5 + d) * Vec2::new(1.0, 1.0);
            if edge_distance(c) < 18.0 || placed.iter().any(|p| p.distance(c) < 26.0) {
                continue;
            }
            let lot = OBox {
                center: c,
                yaw: rng.range(-0.4, 0.4) + if rng.chance(0.5) { 1.5708 } else { 0.0 },
                half: Vec2::new(rng.range(5.0, 9.0), rng.range(5.0, 8.0)),
                ..OBox::default()
            };
            let start = self.oboxes.len();
            ruins::building(self, rng, lot, (0, 1.0), &|_| true);
            // Seat it on the dune and sink it, as if the sand took it.
            let sink = rng.range(1.0, 3.0);
            let g = scenery_ground(c.x, c.y) - sink;
            for o in &mut self.oboxes[start..] {
                o.y0 += g;
                o.y1 += g;
            }
            placed.push(c);
        }
        // Mesas and outcrops on the horizon.
        for k in 0..14 {
            let a = k as f32 / 14.0 * std::f32::consts::TAU + rng.range(-0.15, 0.15);
            let d = rng.range(330.0, 470.0);
            let c = Vec2::splat(SIZE * 0.5) + Vec2::new(a.cos(), a.sin()) * d;
            let h = rng.range(25.0, 70.0);
            let g = scenery_ground(c.x, c.y);
            self.oboxes.push(OBox {
                center: c,
                yaw: rng.range(0.0, 3.0),
                half: Vec2::new(rng.range(30.0, 90.0), rng.range(25.0, 60.0)),
                y0: g - 5.0,
                y1: g + h,
                color: Vec3::new(0.66, 0.5, 0.36) * rng.range(0.85, 1.05),
                side_mat: Material::Rubble,
                top_mat: Material::Rubble,
                chip: 1.0,
                ..OBox::default()
            });
        }
        // Scenery never collides or blocks bots.
        for o in &mut self.oboxes[first..] {
            o.solid = false;
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
        // Team bases: the Imperium dug in along the north ridge (bunker and
        // mast), the Resonant at the south edge of town and the dam's foot.
        let line = |zs: &[f32], m: &Map| -> Vec<Vec2> {
            zs.iter()
                .flat_map(|&z| (0..20).map(move |k| Vec2::new(30.0 + k as f32 * 10.0, z)))
                .filter(|&p| clear(p, m))
                .collect()
        };
        self.bases = [line(&[206.0, 214.0], self), line(&[6.0, 11.0], self)];
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
        // Room for a full team at each end: north ridge and south edge.
        for (side, base) in m.bases.iter().enumerate() {
            assert!(base.len() >= 12, "side {side} has {} spawns", base.len());
        }
        assert!(m.bases[0].iter().all(|p| p.y > 200.0));
        assert!(m.bases[1].iter().all(|p| p.y < 20.0));
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
            side_mat: Material::Wall,
            top_mat: Material::Roof,
            ..OBox::default()
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
    fn ruins_have_ramps_and_stay_affordable() {
        let m = Map::saltmarket();
        let ramps = m.oboxes.iter().filter(|o| o.solid && o.is_ramp()).count();
        let solid = m.oboxes.iter().filter(|o| o.solid).count();
        eprintln!(
            "saltmarket: {} pieces ({solid} solid), {ramps} collapsed-floor ramps",
            m.oboxes.len()
        );
        assert!(ramps >= 5, "only {ramps} collapsed floors to drive up");
        assert!(solid < 12_000, "{solid} solid pieces is too many colliders");
    }

    /// Tanks really can drive up the collapsed floors into the first storey.
    #[test]
    fn collapsed_floors_are_drivable() {
        use crate::sim::{Command, HullKind, TurretKind, World};
        let map = Map::saltmarket();
        let ramps: Vec<OBox> = map
            .oboxes
            .iter()
            .filter(|o| o.solid && o.is_ramp())
            .copied()
            .collect();
        let mut climbed = 0;
        let tried = ramps.len();
        for r in ramps.iter().take(tried) {
            let down = r.axes().1; // local +z runs down the slope
            let foot = r.center + down * (r.half.y * r.tilt.cos() + 5.0);
            let up = -down;
            let mut w = World::empty(map.clone());
            w.spawn_tank(
                Vec3::ONE,
                foot,
                up.x.atan2(up.y),
                HullKind::Lancer,
                TurretKind::Lance,
            );
            let mut top = 0.0f32;
            for _ in 0..(6.0 / crate::game::DT) as usize {
                w.step(&[Command {
                    throttle: 1.0,
                    ..Default::default()
                }]);
                top = top.max(w.tanks[0].pos.y);
            }
            if top <= super::ruins::FIRST_FLOOR - 0.6 {
                eprintln!(
                    "stuck: ramp at {:?} yaw {} tilt {}, foot {foot}, top y={top}, end {:?}",
                    r.center, r.yaw, r.tilt, w.tanks[0].pos
                );
            }
            if top > super::ruins::FIRST_FLOOR - 0.6 {
                climbed += 1;
            }
        }
        eprintln!("climbed {climbed} of {tried} collapsed floors");
        assert!(
            climbed == tried,
            "only {climbed} of {tried} collapsed floors could be driven up"
        );
    }

    /// No two pieces have overlapping coplanar faces pointing the same way:
    /// those z-fight (flicker) on screen.
    #[test]
    fn no_z_fighting() {
        let m = Map::saltmarket();
        let mut boxes: Vec<OBox> = m.oboxes.iter().filter(|o| o.tilt == 0.0).copied().collect();
        for b in &m.boxes {
            boxes.push(OBox {
                center: Vec2::new((b.min.x + b.max.x) * 0.5, (b.min.z + b.max.z) * 0.5),
                half: Vec2::new((b.max.x - b.min.x) * 0.5, (b.max.z - b.min.z) * 0.5),
                y0: b.min.y,
                y1: b.max.y,
                ..OBox::default()
            });
        }
        // Faces in a box's own frame: (axis 0=x,1=y,2=z, sign).
        let face = |b: &OBox, axis: usize, sign: f32| -> (Vec3, f32, [Vec2; 2]) {
            let (ax, az) = b.axes();
            let ax3 = Vec3::new(ax.x, 0.0, ax.y);
            let az3 = Vec3::new(az.x, 0.0, az.y);
            let c = Vec3::new(b.center.x, (b.y0 + b.y1) * 0.5, b.center.y);
            let hy = (b.y1 - b.y0) * 0.5;
            let (n, h) = match axis {
                0 => (ax3, b.half.x),
                1 => (Vec3::Y, hy),
                _ => (az3, b.half.y),
            };
            let n = n * sign;
            let p = c + n * h;
            (n, n.dot(p), [Vec2::ZERO, Vec2::ZERO])
        };
        // Overlap test of two coplanar faces via 2D rects in the shared plane (same yaw only).
        let rect = |b: &OBox, axis: usize, e1: Vec3, e2: Vec3| -> (Vec2, Vec2) {
            let (ax, az) = b.axes();
            let ax3 = Vec3::new(ax.x, 0.0, ax.y);
            let az3 = Vec3::new(az.x, 0.0, az.y);
            let c = Vec3::new(b.center.x, (b.y0 + b.y1) * 0.5, b.center.y);
            let hy = (b.y1 - b.y0) * 0.5;
            let corners: Vec<Vec3> = [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
                .iter()
                .map(|&(u, v)| match axis {
                    0 => c + Vec3::Y * hy * u + az3 * b.half.y * v,
                    1 => c + ax3 * b.half.x * u + az3 * b.half.y * v,
                    _ => c + ax3 * b.half.x * u + Vec3::Y * hy * v,
                })
                .collect();
            let p: Vec<Vec2> = corners
                .iter()
                .map(|q| Vec2::new(q.dot(e1), q.dot(e2)))
                .collect();
            let lo = p.iter().fold(Vec2::splat(f32::INFINITY), |a, &b| a.min(b));
            let hi = p
                .iter()
                .fold(Vec2::splat(f32::NEG_INFINITY), |a, &b| a.max(b));
            (lo, hi)
        };
        let mut fights = Vec::new();
        for i in 0..boxes.len() {
            for j in i + 1..boxes.len() {
                let (a, b) = (&boxes[i], &boxes[j]);
                if a.center.distance(b.center) > a.half.length() + b.half.length() + 0.1 {
                    continue;
                }
                let dy = (a.yaw - b.yaw).rem_euclid(std::f32::consts::FRAC_PI_2);
                if dy > 1e-3 && dy < std::f32::consts::FRAC_PI_2 - 1e-3 {
                    continue;
                }
                for axis in 0..3 {
                    for sign in [-1.0f32, 1.0] {
                        let (na, da, _) = face(a, axis, sign);
                        for axis_b in 0..3 {
                            for sign_b in [-1.0f32, 1.0] {
                                let (nb, db, _) = face(b, axis_b, sign_b);
                                if na.dot(nb) < 0.999 || (da - db).abs() > 3e-4 {
                                    continue;
                                }
                                // Broken tops are drawn jagged, not as a flat face.
                                if na.y > 0.5 && (a.broken || b.broken) {
                                    continue;
                                }
                                // Bottoms resting on (or buried in) the ground are never seen.
                                if na.y < -0.5 && a.y0 <= ground(a.center.x, a.center.y) + 0.05 {
                                    continue;
                                }
                                // In-plane axes aligned with the boxes (they share a yaw).
                                let (ax2, _) = a.axes();
                                let e1 = if na.y.abs() > 0.5 {
                                    Vec3::new(ax2.x, 0.0, ax2.y)
                                } else {
                                    Vec3::Y
                                };
                                let e2 = na.cross(e1).normalize();
                                let (alo, ahi) = rect(a, axis, e1, e2);
                                let (blo, bhi) = rect(b, axis_b, e1, e2);
                                let ov = (ahi.min(bhi) - alo.max(blo)).max(Vec2::ZERO);
                                if ov.x > 0.02 && ov.y > 0.02 {
                                    fights.push((a.center, b.center));
                                }
                            }
                        }
                    }
                }
            }
        }
        eprintln!(
            "{} z-fighting face pairs; first: {:?}",
            fights.len(),
            fights.iter().take(5).collect::<Vec<_>>()
        );
        assert!(
            fights.is_empty(),
            "{} pairs of coplanar overlapping faces",
            fights.len()
        );
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
