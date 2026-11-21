//! The Seam: an Ithessar crystal mine in a rocky desert, the Attrition map
//! where the Ithessar hold their mine against the Resonant. The Resonant
//! call the crystal seam the Ithessar are cutting holy, and have come to
//! take it back.
//!
//! 220 m square (+x east, +z north), mirrored east to west: the Ithessar
//! at the west edge, the Resonant at the east. Real ground to drive on at
//! three heights, stacked over each other:
//!
//! - the pit, in the middle: an open cut 7 m deep, the glowing seam across
//!   its floor and a cutting rig either side of it. Haul roads wind down
//!   its west and east walls.
//! - the rail bridge, straight across the pit north to south: drive over
//!   the deck while the fight goes on under it, between its crystal piers.
//! - two plateaus of rock, north and south, 8 m up, climbed from their
//!   east and west ends. Under each one the rail tunnel runs out to the
//!   edge, and two galleries come in from the cliff face to meet it: caves
//!   under the ground the plateau's tanks are driving on. A headframe
//!   stands over a shaft on each end of the top.
//! - the lanes between the plateaus and the pit, along the desert floor:
//!   the railroad crosses them, an ore train stopped on it in the south, a
//!   car off the rails in the north, where the loading tipple straddles the
//!   track and conveyors climb to it out of the pit.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use glam::{Quat, Vec2, Vec3};

use super::landmarks::Rig;
use super::{Ambient, Core, Crystal, GlowPath, Map, OBox, Rise, Terrain, Wedge};
use crate::faction::Faction;
use crate::render::material::Material;
use crate::rng::Rng;
use crate::tank::ithessar::PLASMA;

const SIZE: f32 = 220.0;
const MID: f32 = SIZE * 0.5;
const CENTER: Vec2 = Vec2::new(MID, MID);
/// Clearance a spawn needs from anything solid.
const TANK_CLEAR: f32 = 4.6;

/// The pit: a rounded rectangle round the middle, `PIT_DEPTH` deep, its
/// walls `PIT_WALL` m across from rim to floor.
const PIT_HALF: Vec2 = Vec2::new(34.0, 26.0);
const PIT_CORNER: f32 = 10.0;
const PIT_DEPTH: f32 = 7.0;
const PIT_WALL: f32 = 3.0;
/// The haul road down the west wall (mirrored east): inside edge x, and
/// where it leaves the rim and reaches the floor (z).
const ROAD_X: (f32, f32) = (MID - PIT_HALF.x, MID - PIT_HALF.x + 12.0);
const ROAD_Z: (f32, f32) = (MID + PIT_HALF.y - 4.0, MID - 6.0);

/// The rail bridge over the pit: deck half-width, top, and its ends (z).
const DECK_HALF: f32 = 3.6;
const DECK_TOP: f32 = 0.25;
const DECK_T: f32 = 1.0;
const DECK_Z: (f32, f32) = (MID - PIT_HALF.y - 3.0, MID + PIT_HALF.y + 3.0);
/// Where the bridge's piers stand (z).
const PIERS: [f32; 4] = [92.0, 104.0, 116.0, 128.0];
/// Track gauge (rail to rail, m): broad, for big ore cars.
const GAUGE: f32 = 2.2;

/// The plateaus' top, the half-width of their flat top (east-west, from
/// the middle) and how far their ramps run down from it.
const PLATEAU_H: f32 = 8.0;
const PLATEAU_FLAT: f32 = 48.0;
const PLATEAU_RAMP: f32 = 32.0;
/// The south plateau's cliff face: its lip, and its foot (z). The north
/// one mirrors it north to south.
const CLIFF: (f32, f32) = (50.0, 54.0);
/// Cave roofs: underside and top.
const ROOF: (f32, f32) = (6.2, PLATEAU_H + 0.05);
/// Cave walls stand this thick round each corridor.
const CAVE_WALL: f32 = 2.4;

/// The caves under the south plateau: (min, max) on the ground plane,
/// every edge on the terrain's 2 m grid so the cut is clean. The rail
/// tunnel, then the galleries, west and east, from the cliff face in to
/// the tunnel. The north plateau's mirror them north to south.
const CAVES: [(Vec2, Vec2); 5] = [
    (Vec2::new(104.0, -6.0), Vec2::new(116.0, 58.0)),
    (Vec2::new(76.0, 20.0), Vec2::new(104.0, 30.0)),
    (Vec2::new(76.0, 30.0), Vec2::new(86.0, 58.0)),
    (Vec2::new(116.0, 20.0), Vec2::new(144.0, 30.0)),
    (Vec2::new(134.0, 30.0), Vec2::new(144.0, 58.0)),
];
/// Each cave's roof (min, max): the cave and some of the rock round it,
/// tiled so no two overlap, ending at the cliff's lip.
const ROOFS: [(Vec2, Vec2); 5] = [
    (Vec2::new(102.0, -6.0), Vec2::new(118.0, 51.0)),
    (Vec2::new(74.0, 18.0), Vec2::new(102.0, 32.0)),
    (Vec2::new(74.0, 32.0), Vec2::new(88.0, 51.0)),
    (Vec2::new(118.0, 18.0), Vec2::new(146.0, 32.0)),
    (Vec2::new(132.0, 32.0), Vec2::new(146.0, 51.0)),
];

/// Sandstone, in a few shades; the desert floor.
const STONE: Vec3 = Vec3::new(0.58, 0.34, 0.22);
const STONE_DARK: Vec3 = Vec3::new(0.4, 0.23, 0.15);
const STONE_PALE: Vec3 = Vec3::new(0.7, 0.5, 0.34);
const SAND: Vec3 = Vec3::new(0.66, 0.46, 0.3);
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

/// Mirror a point east to west.
fn mirror(p: Vec2) -> Vec2 {
    Vec2::new(SIZE - p.x, p.y)
}

/// Mirror a point north to south (the north plateau from the south one).
fn flip(p: Vec2) -> Vec2 {
    Vec2::new(p.x, SIZE - p.y)
}

/// Signed distance to the pit's rim outline (negative inside).
fn pit_sdf(p: Vec2) -> f32 {
    let q = (p - CENTER).abs() - (PIT_HALF - Vec2::splat(PIT_CORNER));
    q.max(Vec2::ZERO).length() + q.x.max(q.y).min(0.0) - PIT_CORNER
}

/// The pit's ground: 0 at the rim, down its walls to the floor, and the
/// haul roads down the west and east walls.
fn pit_ground(p: Vec2) -> f32 {
    let d = pit_sdf(p);
    if d > 0.0 {
        return 0.0;
    }
    let h = -PIT_DEPTH * smoothstep(0.0, PIT_WALL, -d);
    // The haul road (west; the east one mirrors it): a shelf cut into the
    // wall, falling at a steady grade from the rim to the floor.
    let w = Vec2::new(p.x.min(SIZE - p.x), p.y);
    let across = 1.0 - smoothstep(ROAD_X.1, ROAD_X.1 + 2.0, w.x);
    let t = ((ROAD_Z.0 - w.y) / (ROAD_Z.0 - ROAD_Z.1)).clamp(0.0, 1.0);
    let road = -PIT_DEPTH * t;
    h + (road.max(h) - h) * across
}

/// How high the plateau stands at a point, before the caves are cut out:
/// 0..1 of its full height.
fn plateau_mask(p: Vec2) -> f32 {
    let z = p.y.min(SIZE - p.y);
    let across = 1.0 - smoothstep(CLIFF.0, CLIFF.1, z);
    let t = (((p.x - MID).abs() - PLATEAU_FLAT) / PLATEAU_RAMP).clamp(0.0, 1.0);
    across * (1.0 - t)
}

/// Distance from a point to the nearest cave (0 inside one).
fn cave_distance(p: Vec2) -> f32 {
    let z = Vec2::new(p.x, p.y.min(SIZE - p.y));
    CAVES
        .iter()
        .map(|&(a, b)| {
            let q = (z - (a + b) * 0.5).abs() - (b - a) * 0.5;
            q.max(Vec2::ZERO).length()
        })
        .fold(f32::INFINITY, f32::min)
}

/// Play ground: low dunes on the desert floor, the pit sunk in the middle,
/// the plateaus raised north and south with their caves cut out.
pub fn ground(x: f32, z: f32) -> f32 {
    let p = Vec2::new(x, z);
    let mask = plateau_mask(p);
    let dunes =
        0.4 * (0.061 * x + 0.3).sin() * (0.047 * z).cos() + 0.22 * (0.13 * x - 0.09 * z).sin();
    // Flat along the railroad, round the pit's rim and on the plateaus.
    let flat =
        smoothstep(7.0, 14.0, (x - MID).abs()) * smoothstep(2.0, 10.0, pit_sdf(p)) * (1.0 - mask);
    let rock = PLATEAU_H * mask * smoothstep(0.0, 2.0, cave_distance(p));
    dunes * flat + rock + pit_ground(p)
}

/// Desert out past the edge: the plateaus run on as ridges, and the floor
/// rises into broken hills.
fn scenery_ground(x: f32, z: f32) -> f32 {
    let p = Vec2::new(x, z);
    let q = (p - CENTER).abs() - Vec2::splat(MID);
    let d = q.max(Vec2::ZERO).length() + q.x.max(q.y).min(0.0);
    if d < -0.01 {
        return -10.0;
    }
    // The edge's ground carried on out, the plateaus solid (their tunnels
    // run on under the rock).
    let edge = p.clamp(Vec2::ZERO, Vec2::splat(SIZE));
    let base = ground(edge.x, edge.y).max(PLATEAU_H * plateau_mask(edge));
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
            // the horizon, sand blowing off the plateaus.
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
            },
            bases: Map::no_bases(&sides),
            factions: sides.to_vec(),
            repair_spots: Vec::new(),
            hold_site: Some(Vec2::new(MID - 17.0, MID - 16.0)),
            cores: Vec::new(),
            crystals: Vec::new(),
            glow_paths: Vec::new(),
            fall_y: None,
            hold_name: "SEAM",
            routes: Vec::new(),
        };
        let mut terrain = Terrain::from_fn(Vec2::ZERO, 2.0, 111, 111, ground);
        terrain.color = SAND;
        terrain.material = Material::Sand;
        map.terrain = Some(terrain);
        let mut rng = Rng::new(0x5ea3_0194);

        map.pit_walls(&mut rng);
        map.seam(&mut rng);
        map.cutting_rigs();
        map.rail_bridge();
        map.railroad();
        for north in [false, true] {
            map.caves(&mut rng, north);
            map.cliff_face(&mut rng, north);
            map.headframes(north);
        }
        map.ore_train(&mut rng);
        map.tipple();
        map.plateau_cover(&mut rng);
        map.lane_cover(&mut rng);
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
    /// still running, the stopped train ticking in the heat.
    fn mine_sounds(&mut self) {
        let mut found: Vec<(Ambient, Vec3)> = self
            .crystals
            .iter()
            .filter(|c| c.lit && c.radius > 0.6)
            .map(|c| (Ambient::Crystal, c.base + c.axis * 0.35))
            .collect();
        for x in [MID - 16.0, MID + 16.0] {
            found.push((Ambient::Machine, Vec3::new(x, -4.0, MID + 10.0)));
        }
        for (x, z) in [(MID, 68.0), (MID + 7.0, 163.0)] {
            let g = self.ground_height(x, z);
            found.push((Ambient::Hull, Vec3::new(x, g + 2.5, z)));
        }
        found.push((Ambient::Shrine, Vec3::new(SIZE - 32.0, 2.0, MID)));
        found.push((Ambient::Core, Vec3::new(32.0, 10.0, MID)));
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
        let g = self.ground_height(at.x, at.y);
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

    /// Blocks of rock along the pit's steep walls, and spoil at their foot.
    /// The haul roads stay clear.
    fn pit_walls(&mut self, rng: &mut Rng) {
        let n = 64;
        for k in 0..n {
            let a = k as f32 / n as f32 * TAU;
            // Walk out from the middle to the rim at this angle.
            let dir = Vec2::new(a.cos(), a.sin());
            let mut r = 10.0;
            while pit_sdf(CENTER + dir * r) < -PIT_WALL * 0.5 {
                r += 0.25;
            }
            let at = CENTER + dir * r;
            let w = Vec2::new(at.x.min(SIZE - at.x), at.y);
            let on_road = w.x < ROAD_X.1 + 4.0 && w.y > ROAD_Z.1 - 4.0;
            let under_bridge = (at.x - MID).abs() < DECK_HALF + 4.0;
            if on_road || under_bridge || k % 2 == 1 && rng.chance(0.4) {
                continue;
            }
            let h = rng.range(3.5, 6.5);
            self.stone(
                at,
                -a + rng.range(-0.2, 0.2),
                Vec2::new(rng.range(1.2, 2.0), rng.range(2.0, 3.2)),
                -PIT_DEPTH - 0.5,
                -PIT_DEPTH + h,
                STONE * rng.range(0.85, 1.1),
                0.35,
            );
            if let Some(o) = self.oboxes.last_mut() {
                o.broken = true;
            }
            // Spoil fallen off the bench.
            if rng.chance(0.5) {
                let s = rng.range(0.5, 1.1);
                let p = at - dir * rng.range(3.0, 5.0);
                self.boulder(rng, p, s, false);
            }
        }
    }

    /// The seam across the pit floor, east to west under the bridge: raw
    /// crystal breaking out of the rock, cut back in steps where the rigs
    /// have been at it, a river of plasma running along the vein.
    fn seam(&mut self, rng: &mut Rng) {
        let floor = -PIT_DEPTH;
        let mut pts = Vec::new();
        let mut widths = Vec::new();
        let x0 = MID - PIT_HALF.x + PIT_WALL + 2.0;
        let x1 = SIZE - x0;
        let n = 30;
        for k in 0..=n {
            let x = x0 + (x1 - x0) * k as f32 / n as f32;
            let z = MID + 1.5 * (x * 0.15).sin();
            pts.push(Vec3::new(x, floor + 0.04, z));
            widths.push(0.9 + 0.4 * (x * 0.3).sin().abs());
        }
        self.glow_paths.push(GlowPath {
            points: pts,
            widths,
            color: PLASMA * 0.9,
        });
        // Crystal along the vein, mirrored: big outcrops for cover with
        // small shards round them. Clear of the piers and the haul road.
        let outcrops = [(85.0, 101.0, 3.2), (93.0, 110.0, 4.2), (101.0, 106.0, 2.8)];
        for (x, z, h) in outcrops {
            for p in [Vec2::new(x, z), mirror(Vec2::new(x, z))] {
                let lean_dir = Vec2::new(rng.range(-1.0, 1.0), rng.range(-1.0, 1.0));
                self.shard(
                    rng.next_u64(),
                    Vec3::new(p.x, floor, p.y),
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
                    let out = q - p;
                    self.shard(
                        rng.next_u64(),
                        Vec3::new(q.x, floor, q.y),
                        rng.range(0.8, 1.8),
                        0.35,
                        out,
                        rng.range(0.3, 0.7),
                        rng.chance(0.4),
                        false,
                    );
                }
            }
        }
        // Cut faces: low steps of rock either side of the vein where the
        // rigs have cut it back.
        for (x, z) in [(84.0, 92.0), (100.0, 97.0)] {
            for p in [Vec2::new(x, z), mirror(Vec2::new(x, z))] {
                self.stone(
                    p,
                    0.0,
                    Vec2::new(3.0, 1.6),
                    floor - 0.5,
                    floor + 1.4,
                    STONE_DARK,
                    0.25,
                );
            }
        }
    }

    /// The cutting rigs, either side of the seam: a crystal-hulled body on
    /// hover pads, a boom reaching out to the cut, a plasma head glowing at
    /// its end.
    fn cutting_rigs(&mut self) {
        let floor = -PIT_DEPTH;
        for east in [false, true] {
            let at = Vec2::new(MID - 16.0, MID + 10.0);
            let at = if east { mirror(at) } else { at };
            // Boom reaches toward the vein (south) and in toward the middle.
            let yaw = if east { PI + 0.5 } else { PI - 0.5 };
            let rig = Rig {
                pos: Vec3::new(at.x, floor, at.y),
                rot: Quat::from_rotation_y(yaw),
            };
            // Hover pads: low dark blocks with a plasma glow under them.
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
            // Body and cab.
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
            // The boom: two arms out over the front, down to the head.
            let root = Vec3::new(0.0, 2.9, 2.6);
            let elbow = Vec3::new(0.0, 5.2, 6.5);
            let head = Vec3::new(0.0, 1.2, 9.5);
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
            // The cutting head: a crystal disc wheel ringed in plasma.
            let hc = rig.point(head);
            let axle = rig.rot * Vec3::X * 0.35;
            self.disc(hc, axle, 1.3, GLASS * 1.5, Material::Vhaal);
            self.disc(hc, axle * 1.25, 0.6, PLASMA, Material::Glow);
        }
    }

    /// The rail bridge over the pit: a deck on piers of worked crystal, low
    /// steel girders along its edges, a truss over them, the track on top.
    fn rail_bridge(&mut self) {
        let (z0, z1) = DECK_Z;
        let len = z1 - z0;
        let mid_z = (z0 + z1) * 0.5;
        // The deck: drivable, rails and all.
        self.oboxes.push(OBox {
            center: Vec2::new(MID, mid_z),
            half: Vec2::new(DECK_HALF, len * 0.5),
            y0: DECK_TOP - DECK_T,
            y1: DECK_TOP,
            color: STEEL * 1.2,
            side_mat: Material::Metal,
            top_mat: Material::Metal,
            ramp: true,
            ..OBox::default()
        });
        // Short ramps up onto it from the rim at each end.
        for (zmin, zmax, rise) in [(z0 - 3.0, z0, Rise::PosZ), (z1, z1 + 3.0, Rise::NegZ)] {
            self.wedges.push(Wedge {
                min: Vec2::new(MID - DECK_HALF, zmin),
                max: Vec2::new(MID + DECK_HALF, zmax),
                base_y: -0.3,
                height: DECK_TOP + 0.3,
                rise,
                color: STEEL,
                material: Some(Material::Metal),
            });
        }
        // Girders along each edge: low enough to shoot over, high enough
        // to keep a tank on the deck. A truss of posts and diagonals over
        // them, open.
        for side in [-1.0f32, 1.0] {
            let x = MID + side * (DECK_HALF - 0.25);
            self.oboxes.push(OBox {
                center: Vec2::new(x, mid_z),
                half: Vec2::new(0.25, len * 0.5),
                y0: DECK_TOP - 0.01,
                y1: DECK_TOP + 0.9,
                color: RUST,
                side_mat: Material::Rust,
                top_mat: Material::Rust,
                ..OBox::default()
            });
            let n = (len / 6.0) as usize;
            for k in 0..=n {
                let z = z0 + len * k as f32 / n as f32;
                let foot = Vec3::new(x, DECK_TOP + 0.9, z);
                self.segment(
                    foot,
                    foot + Vec3::Y * 2.6,
                    0.12,
                    RUST,
                    Material::Rust,
                    false,
                );
                if k < n {
                    let z2 = z0 + len * (k + 1) as f32 / n as f32;
                    let (a, b) = if k % 2 == 0 {
                        (foot, Vec3::new(x, DECK_TOP + 3.5, z2))
                    } else {
                        (foot + Vec3::Y * 2.6, Vec3::new(x, DECK_TOP + 0.9, z2))
                    };
                    self.segment(a, b, 0.08, RUST, Material::Rust, false);
                }
            }
            self.segment(
                Vec3::new(x, DECK_TOP + 3.5, z0),
                Vec3::new(x, DECK_TOP + 3.5, z1),
                0.14,
                RUST,
                Material::Rust,
                false,
            );
        }
        // Piers: a crystal column under each rail, a cap beam across their
        // tops, a seam of plasma up the faces.
        let floor = -PIT_DEPTH;
        for &z in &PIERS {
            for side in [-1.0f32, 1.0] {
                let at = Vec2::new(MID + side * 2.4, z);
                self.oboxes.push(OBox {
                    center: at,
                    yaw: PI / 4.0,
                    half: Vec2::splat(0.8),
                    y0: floor - 0.5,
                    y1: DECK_TOP - DECK_T,
                    color: GLASS,
                    side_mat: Material::Vhaal,
                    top_mat: Material::Vhaal,
                    ..OBox::default()
                });
                // Plasma up the outside corner.
                self.oboxes.push(OBox {
                    center: at + Vec2::new(side * 1.1, 0.0),
                    half: Vec2::new(0.06, 0.12),
                    y0: floor + 0.3,
                    y1: DECK_TOP - DECK_T - 0.4,
                    color: PLASMA,
                    side_mat: Material::Glow,
                    top_mat: Material::Glow,
                    solid: false,
                    ..OBox::default()
                });
            }
            self.oboxes.push(OBox {
                center: Vec2::new(MID, z),
                half: Vec2::new(DECK_HALF + 0.3, 0.9),
                y0: DECK_TOP - DECK_T - 0.9,
                y1: DECK_TOP - DECK_T + 0.01,
                color: GLASS * 1.2,
                side_mat: Material::Vhaal,
                top_mat: Material::Vhaal,
                ..OBox::default()
            });
            // A brace between the columns, out of the way overhead.
            self.segment(
                Vec3::new(MID - 2.4, floor + 3.0, z),
                Vec3::new(MID + 2.4, DECK_TOP - DECK_T - 0.9, z),
                0.1,
                STEEL,
                Material::Metal,
                false,
            );
        }
    }

    /// The track north to south down the middle, through both tunnels and
    /// over the bridge: two rails on sleepers. Visual only: tanks roll over it.
    fn railroad(&mut self) {
        let (z0, z1) = (-4.0f32, SIZE + 4.0);
        let step = 1.6;
        let n = ((z1 - z0) / step) as usize;
        let on_deck = |z: f32| z > DECK_Z.0 && z < DECK_Z.1;
        let y_at = |m: &Map, z: f32| {
            if on_deck(z) {
                DECK_TOP
            } else {
                m.ground_height(MID, z)
                    .max(m.ground_height(MID - 1.5, z))
                    .max(m.ground_height(MID + 1.5, z))
            }
        };
        for k in 0..n {
            let z = z0 + step * (k as f32 + 0.5);
            let y = y_at(self, z);
            self.oboxes.push(OBox {
                center: Vec2::new(MID, z),
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
        // Rails, in runs of 8 m following the ground.
        let run = 8.0;
        let runs = ((z1 - z0) / run) as usize;
        for k in 0..runs {
            let (a, b) = (z0 + run * k as f32, z0 + run * (k + 1) as f32);
            let (ya, yb) = (y_at(self, a) + 0.16, y_at(self, b) + 0.16);
            for side in [-1.0f32, 1.0] {
                let x = MID + side * GAUGE * 0.5;
                self.segment(
                    Vec3::new(x, ya, a),
                    Vec3::new(x, yb, b),
                    0.08,
                    STEEL * 1.4,
                    Material::Metal,
                    false,
                );
            }
        }
    }

    /// One plateau's caves: the cut corridors walled in rock, roofed over
    /// with slabs flush with the top, lit by plasma lamps, crystal veins in
    /// the walls, timber portals at the cliff face, and ore carts.
    fn caves(&mut self, rng: &mut Rng, north: bool) {
        let place = |p: Vec2| if north { flip(p) } else { p };
        let rect = |(a, b): (Vec2, Vec2)| {
            let (p, q) = (place(a), place(b));
            (p.min(q), p.max(q))
        };
        let caves: Vec<(Vec2, Vec2)> = CAVES.iter().map(|&r| rect(r)).collect();
        let inside_any = |p: Vec2| {
            caves
                .iter()
                .any(|(a, b)| p.x > a.x && p.x < b.x && p.y > a.y && p.y < b.y)
        };
        let lip = |z: f32| {
            if north {
                z > SIZE - CLIFF.1 + 1.5
            } else {
                z < CLIFF.1 - 1.5
            }
        };
        // Walls: along every edge of every cave, where the other side is
        // rock (not another cave, not the open air past the cliff).
        let step = 2.0;
        for &(a, b) in &caves {
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
                        !inside_any(p) && lip(p.y) && (0.0..SIZE).contains(&p.y)
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
        // Roofs: one slab per cave, flush with the top, tiled so none
        // overlap.
        for &r in &ROOFS {
            let (a, b) = rect(r);
            let (a, b) = (a.max(Vec2::splat(-6.0)), b.min(Vec2::splat(SIZE + 6.0)));
            self.oboxes.push(OBox {
                center: (a + b) * 0.5,
                half: (b - a) * 0.5,
                y0: ROOF.0,
                y1: ROOF.1,
                color: STONE * 0.95,
                side_mat: Material::Rubble,
                top_mat: Material::Rubble,
                ramp: true,
                ..OBox::default()
            });
        }
        // Lamps: plasma in a crystal cage on the walls every 12 m, and
        // veins of raw crystal breaking out of them.
        for &(a, b) in &caves {
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
                    if !lip(p.y) || inside_any(p + out * 1.0) || !(2.0..SIZE - 2.0).contains(&p.y) {
                        continue;
                    }
                    if (k + side as usize).is_multiple_of(2) {
                        let at = p - out * 0.15;
                        self.oboxes.push(OBox {
                            center: at,
                            half: Vec2::splat(0.22),
                            y0: 3.9,
                            y1: 4.6,
                            color: PLASMA,
                            side_mat: Material::Glow,
                            top_mat: Material::Glow,
                            solid: false,
                            ..OBox::default()
                        });
                        self.oboxes.push(OBox {
                            center: at,
                            half: Vec2::splat(0.34),
                            y0: 4.6,
                            y1: 4.75,
                            color: GLASS,
                            side_mat: Material::Vhaal,
                            top_mat: Material::Vhaal,
                            solid: false,
                            ..OBox::default()
                        });
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
        // Portals at the cliff face: a timber frame round each mouth.
        for &(a, b) in &caves {
            let mouth_z = if north { a.y } else { b.y };
            let at_face = if north {
                mouth_z < SIZE - CLIFF.0
            } else {
                mouth_z > CLIFF.0
            };
            if !at_face {
                continue;
            }
            let z = if north {
                SIZE - CLIFF.0 - 0.6
            } else {
                CLIFF.0 + 0.6
            };
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
                center: Vec2::new((a.x + b.x) * 0.5, z),
                half: Vec2::new((b.x - a.x) * 0.5 + 0.4, 0.4),
                y0: ROOF.0 - 0.7,
                y1: ROOF.0 + 0.02,
                color: TIMBER * 0.9,
                side_mat: Material::Wood,
                top_mat: Material::Wood,
                ..OBox::default()
            });
        }
        // Ore carts left in the galleries (not the rail tunnel).
        for &(x, z) in &[
            (90.0f32, 22.4f32),
            (78.4, 44.0),
            (130.0, 22.4),
            (141.6, 44.0),
        ] {
            let p = place(Vec2::new(x, z));
            let yaw = if (x - 78.4).abs() < 1.0 || (x - 141.6).abs() < 1.0 {
                0.0
            } else {
                FRAC_PI_2
            };
            self.ore_cart(rng, p, yaw);
        }
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

    /// Rock along one plateau's cliff face, so it reads as stone, not
    /// sand. Clear of the cave mouths.
    fn cliff_face(&mut self, rng: &mut Rng, north: bool) {
        let mouths: Vec<(f32, f32)> = CAVES
            .iter()
            .filter(|(_, b)| b.y > CLIFF.1)
            .map(|(a, b)| (a.x, b.x))
            .collect();
        let mut x = MID - PLATEAU_FLAT - 6.0;
        while x < MID + PLATEAU_FLAT + 6.0 {
            let w = rng.range(2.0, 3.2);
            let c = x + w;
            x += w * 2.0 - 0.4;
            if mouths
                .iter()
                .any(|&(a, b)| c + w > a - 0.6 && c - w < b + 0.6)
            {
                continue;
            }
            let full = plateau_mask(Vec2::new(c, CLIFF.0 - 1.0));
            if full < 0.3 {
                continue;
            }
            let zc = CLIFF.0 + 1.2 + rng.range(-0.3, 0.5);
            let p = Vec2::new(c, zc);
            let p = if north { flip(p) } else { p };
            let top = PLATEAU_H * full + rng.range(-1.5, 0.8);
            self.stone(
                p,
                rng.range(-0.08, 0.08),
                Vec2::new(w, rng.range(1.6, 2.4)),
                -0.6,
                top,
                [STONE, STONE_DARK, STONE_PALE][rng.index(3)] * rng.range(0.85, 1.05),
                0.45,
            );
            if let Some(o) = self.oboxes.last_mut() {
                o.broken = true;
            }
            if rng.chance(0.35) {
                let q = p + Vec2::new(
                    rng.range(-2.0, 2.0),
                    if north { -1.0 } else { 1.0 } * rng.range(3.5, 5.0),
                );
                let s = rng.range(0.5, 1.0);
                self.boulder(rng, q, s, false);
            }
        }
    }

    /// A headframe over a shaft on each end of one plateau's top: four
    /// legs and two back braces of steel, a sheave wheel at the top, the
    /// hoist house behind, the shaft collar capped with crystal.
    fn headframes(&mut self, north: bool) {
        for east in [false, true] {
            let mut at = Vec2::new(MID - 20.0, 10.0);
            if east {
                at = mirror(at);
            }
            if north {
                at = flip(at);
            }
            let g = self.ground_height(at.x, at.y);
            let top = g + 16.0;
            let (hx, hz) = (2.6, 2.6);
            for (sx, sz) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                let foot = Vec3::new(at.x + sx * hx, g - 0.2, at.y + sz * hz);
                let head = Vec3::new(at.x + sx * 0.9, top, at.y + sz * 0.9);
                self.segment(foot, head, 0.22, RUST, Material::Rust, true);
            }
            // Cross-ties at three heights.
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
            // Back braces out toward the hoist house (toward the map edge).
            let back = if north { 1.0 } else { -1.0 };
            for sx in [-1.0f32, 1.0] {
                let foot = Vec3::new(at.x + sx * 2.0, g - 0.2, at.y + back * 9.0);
                self.segment(
                    foot,
                    Vec3::new(at.x + sx * 0.9, top - 1.0, at.y),
                    0.18,
                    RUST,
                    Material::Rust,
                    true,
                );
            }
            // The sheave: a wheel on top, turned to face the hoist.
            let sheave = Vec3::new(at.x, top + 1.4, at.y);
            self.disc(
                sheave,
                Vec3::new(0.15, 0.0, 0.0),
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
            // Hoist cable down the shaft, and back to the hoist house.
            let house = Vec2::new(at.x, at.y + back * 6.5);
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
                Vec3::new(house.x, g + 4.0, house.y),
                0.04,
                STEEL * 0.6,
                Material::Rust,
                false,
            );
            // Collar: a crystal cap over the shaft, low enough to drive over.
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
            // The hoist house: a squat crystal block with a plasma slit.
            let house = house + Vec2::new(0.0, back * 3.0);
            self.oboxes.push(OBox {
                center: house,
                half: Vec2::new(3.2, 2.2),
                y0: g - 0.3,
                y1: g + 4.2,
                color: GLASS * 1.15,
                side_mat: Material::Vhaal,
                top_mat: Material::Vhaal,
                ..OBox::default()
            });
            self.oboxes.push(OBox {
                center: house - Vec2::new(0.0, back * 2.21),
                half: Vec2::new(2.4, 0.02),
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
    }

    /// One rail car centered at `at` (on the track, along z), `yaw` off the
    /// line, rolled `roll` onto its side (a derailed car). A hopper of raw
    /// crystal, or the locomotive (`loco`), its crystal prow toward +z.
    fn rail_car(&mut self, rng: &mut Rng, at: Vec2, yaw: f32, roll: f32, loco: bool) {
        let g = self.ground_height(at.x, at.y);
        let len: f32 = if loco { 13.0 } else { 9.0 };
        let half_w = 1.7;
        // Rolled about its long axis round the low edge it tipped over on.
        let pivot = Vec3::new(-half_w * roll.signum(), 0.0, 0.0);
        let rot = Quat::from_rotation_y(yaw) * Quat::from_rotation_z(roll);
        let base = Vec3::new(at.x, g + 0.2, at.y);
        let rig = Rig {
            pos: base + Quat::from_rotation_y(yaw) * pivot - rot * pivot,
            rot,
        };
        let id = Quat::IDENTITY;
        // Bogies and wheels.
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
        // Underframe.
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
            // A long crystal hood, the cab raised at the back, plasma vents
            // down its flanks, a crystal prow out front.
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
            // The hopper: crystal sides flaring out, full of raw crystal.
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

    /// The ore train: the locomotive stopped in the south lane short of the
    /// bridge (room to drive onto it past its prow), a hopper left at the
    /// end of the line in the tunnel. In the north lane, past the tipple,
    /// a hopper knocked off the rails onto its side, its load spilled, and
    /// another still on them.
    fn ore_train(&mut self, rng: &mut Rng) {
        self.rail_car(rng, Vec2::new(MID, 66.0), 0.0, 0.0, true);
        // A hopper left at the end of the line, in the tunnel's dead end.
        self.rail_car(rng, Vec2::new(MID, 12.0), 0.0, 0.0, false);
        // The north lane: a car on its side beside the line, another on it.
        self.rail_car(rng, Vec2::new(MID + 7.0, 163.0), 0.25, -1.5, false);
        self.rail_car(rng, Vec2::new(MID, 160.0), 0.0, 0.0, false);
        // Its load, strewn over the sand.
        for _ in 0..9 {
            let p = Vec2::new(MID + rng.range(9.0, 16.0), rng.range(152.0, 164.0));
            let g = self.ground_height(p.x, p.y);
            let dir = Vec2::new(rng.range(-1.0, 1.0), rng.range(-1.0, 1.0));
            self.shard(
                rng.next_u64(),
                Vec3::new(p.x, g, p.y),
                rng.range(0.4, 1.2),
                0.3,
                dir,
                rng.range(0.6, 1.3),
                rng.chance(0.3),
                false,
            );
        }
    }

    /// The loading tipple over the track in the north lane: a crystal bin
    /// on four legs high enough to drive a train (or a tank) under, its
    /// chute over the rails, and two conveyors climbing to it out of the
    /// pit, west and east.
    fn tipple(&mut self) {
        let at = Vec2::new(MID, 152.0);
        let g = self.ground_height(at.x, at.y);
        let (bin0, bin1) = (g + 7.5, g + 11.0);
        for (sx, sz) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
            self.oboxes.push(OBox {
                center: at + Vec2::new(sx * 5.4, sz * 3.6),
                half: Vec2::splat(0.4),
                y0: g - 0.3,
                y1: bin0,
                color: RUST,
                side_mat: Material::Rust,
                top_mat: Material::Rust,
                ..OBox::default()
            });
        }
        self.oboxes.push(OBox {
            center: at,
            half: Vec2::new(5.8, 4.0),
            y0: bin0,
            y1: bin1,
            color: GLASS * 1.1,
            side_mat: Material::Vhaal,
            top_mat: Material::Vhaal,
            ..OBox::default()
        });
        self.oboxes.push(OBox {
            center: at,
            half: Vec2::new(1.4, 1.4),
            y0: bin0 - 1.6,
            y1: bin0 + 0.01,
            color: STEEL,
            side_mat: Material::Metal,
            top_mat: Material::Metal,
            solid: false,
            ..OBox::default()
        });
        self.oboxes.push(OBox {
            center: at,
            half: Vec2::new(5.85, 4.05),
            y0: bin1 - 0.8,
            y1: bin1 - 0.6,
            color: PLASMA,
            side_mat: Material::Glow,
            top_mat: Material::Glow,
            solid: false,
            ..OBox::default()
        });
        // Conveyors from the pit floor by the rigs, up over the rim, to the
        // bin's sides. Visual, but for the trestles that hold them up.
        for east in [false, true] {
            let start = Vec2::new(MID - 18.0, MID + 17.0);
            let end = Vec2::new(MID - 6.0, 152.0);
            let (start, end) = if east {
                (mirror(start), mirror(end))
            } else {
                (start, end)
            };
            let a = Vec3::new(start.x, -PIT_DEPTH + 1.2, start.y);
            let b = Vec3::new(end.x, bin1 - 0.3, end.y);
            self.segment(a, b, 0.5, STEEL, Material::Metal, false);
            if let Some(o) = self.oboxes.last_mut() {
                o.half.x = 0.9;
            }
            // The belt, running glow up its middle.
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
            // Trestles: pairs of legs at intervals, down to the ground.
            for f in [0.22f32, 0.45, 0.68, 0.86] {
                let p = a.lerp(b, f);
                let gp = self.ground_height(p.x, p.z);
                let side = (b - a).cross(Vec3::Y).normalize() * 1.0;
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
    }

    /// Cover on the plateau tops: boulders, ore bins and a few low cut
    /// blocks, clear of the headframes and the cave roofs' lips.
    fn plateau_cover(&mut self, rng: &mut Rng) {
        let spots = [
            (66.0, 40.0, 2.0),
            (72.0, 12.0, 1.6),
            (100.0, 8.0, 1.2),
            (52.0, 26.0, 1.8),
        ];
        for (x, z, s) in spots {
            for p in [
                Vec2::new(x, z),
                mirror(Vec2::new(x, z)),
                flip(Vec2::new(x, z)),
                flip(mirror(Vec2::new(x, z))),
            ] {
                self.boulder(rng, p, s, true);
            }
        }
        // Stacks of cut crystal blocks waiting for the train, on the roofs
        // over the galleries.
        for (x, z) in [(90.0, 36.0), (60.0, 14.0)] {
            for p in [
                Vec2::new(x, z),
                mirror(Vec2::new(x, z)),
                flip(Vec2::new(x, z)),
                flip(mirror(Vec2::new(x, z))),
            ] {
                let g = self.ground_height(p.x, p.y);
                for k in 0..3 {
                    let off = Vec2::new((k as f32 - 1.0) * 1.9, 0.0);
                    let h = if k == 1 { 3.0 } else { 1.5 };
                    self.oboxes.push(OBox {
                        center: p + off,
                        half: Vec2::splat(0.9),
                        y0: g - 0.2,
                        y1: g + h,
                        color: GLASS * rng.range(1.0, 1.4),
                        side_mat: Material::Vhaal,
                        top_mat: Material::Vhaal,
                        ..OBox::default()
                    });
                }
            }
        }
    }

    /// Cover in the lanes between the plateaus and the pit, and on the
    /// open ground round the pit's ends: boulders and spoil heaps.
    fn lane_cover(&mut self, rng: &mut Rng) {
        let spots = [
            (40.0, 64.0, 1.8),
            (62.0, 72.0, 1.5),
            (84.0, 62.0, 2.0),
            (96.0, 74.0, 1.2),
            (34.0, 156.0, 1.6),
            (58.0, 148.0, 2.0),
            (80.0, 160.0, 1.6),
            (54.0, 96.0, 1.8),
            (50.0, 124.0, 1.6),
            (40.0, 110.0, 1.2),
        ];
        for (x, z, s) in spots {
            for p in [Vec2::new(x, z), mirror(Vec2::new(x, z))] {
                self.boulder(rng, p, s, true);
                for _ in 0..2 {
                    let q = p + Vec2::new(rng.range(-4.0, 4.0), rng.range(-4.0, 4.0));
                    let s = rng.range(0.3, 0.6);
                    self.boulder(rng, q, s, false);
                }
            }
        }
    }

    /// The Ithessar's yard at the west edge: a plasma core held over it, as
    /// at every Ithessar place, and worked crystal standing round.
    fn mine_ithessar_base(&mut self, rng: &mut Rng) {
        let at = Vec2::new(27.0, MID);
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
    /// raised round a cleared ring, facing the seam they call holy.
    fn mine_resonant_base(&mut self, rng: &mut Rng) {
        let at = Vec2::new(SIZE - 32.0, MID);
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
    /// it meets (the desert floor, the plateau tops, the tunnel floors).
    fn edge_rocks(&mut self, rng: &mut Rng) {
        let shades = [STONE, STONE_DARK, STONE_PALE];
        for (p, inward) in self.edge_points(3.2, 2.0) {
            let at = p + inward * rng.range(0.4, 1.0);
            let q = at.clamp(Vec2::splat(0.5), Vec2::splat(SIZE - 0.5));
            let g = self.ground_height(q.x, q.y);
            // Lower inside a tunnel, under its roof.
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

    /// The ways round for bots, on every level: over the bridge and under
    /// it, through the caves under each plateau (in at a gallery, along
    /// the rail tunnel, out at the other gallery), over each plateau's top.
    fn mine_routes(&mut self) {
        let floor = -PIT_DEPTH;
        let mut ways = vec![
            // Over the deck, rim to rim: crossing from the lane south of
            // the pit on one side to the one north of it on the other, and
            // the other way about.
            vec![
                Vec3::new(MID - 12.0, 0.0, DECK_Z.0 - 5.0),
                Vec3::new(MID, DECK_TOP, DECK_Z.0 + 5.0),
                Vec3::new(MID, DECK_TOP, DECK_Z.1 - 5.0),
                Vec3::new(MID + 14.0, 0.0, DECK_Z.1 + 11.0),
            ],
            vec![
                Vec3::new(MID + 12.0, 0.0, DECK_Z.0 - 5.0),
                Vec3::new(MID, DECK_TOP, DECK_Z.0 + 5.0),
                Vec3::new(MID, DECK_TOP, DECK_Z.1 - 5.0),
                Vec3::new(MID - 14.0, 0.0, DECK_Z.1 + 11.0),
            ],
            // Across the pit floor, down one haul road and up the other,
            // between the bridge's piers.
            vec![
                Vec3::new(ROAD_X.0 + 6.0, floor * 0.5, MID + 8.0),
                Vec3::new(ROAD_X.1 + 2.0, floor, MID - 8.0),
                Vec3::new(MID, floor, MID),
                Vec3::new(SIZE - ROAD_X.1 - 2.0, floor, MID - 8.0),
                Vec3::new(SIZE - ROAD_X.0 - 6.0, floor * 0.5, MID + 8.0),
            ],
        ];
        for north in [false, true] {
            let place = |x: f32, y: f32, z: f32| {
                let p = if north {
                    flip(Vec2::new(x, z))
                } else {
                    Vec2::new(x, z)
                };
                Vec3::new(p.x, y, p.y)
            };
            // Under the plateau: in at the west gallery's mouth, across to
            // the tunnel, out along the east one.
            ways.push(vec![
                place(81.0, 0.0, 56.0),
                place(81.0, 0.0, 34.0),
                place(96.0, 0.0, 25.0),
                place(MID, 0.0, 25.0),
                place(SIZE - 96.0, 0.0, 25.0),
                place(SIZE - 81.0, 0.0, 34.0),
                place(SIZE - 81.0, 0.0, 56.0),
            ]);
            // Over its top, end to end.
            ways.push(vec![
                place(MID - PLATEAU_FLAT + 4.0, PLATEAU_H, 30.0),
                place(MID, PLATEAU_H, 36.0),
                place(MID + PLATEAU_FLAT - 4.0, PLATEAU_H, 30.0),
            ]);
        }
        self.routes = ways;
    }

    fn mine_spawns(&mut self) {
        for (side, xs) in [(0, [11.0, 18.0]), (1, [SIZE - 11.0, SIZE - 18.0])] {
            for x in xs {
                for k in 0..10 {
                    self.bases[side].push(Vec2::new(x, MID - 28.0 + k as f32 * 6.2));
                }
            }
        }
        let mut spots = Vec::new();
        // The lanes, the pit floor and the plateau tops, west half.
        for &(x, z) in &[
            (30.0, 66.0),
            (52.0, 62.0),
            (74.0, 68.0),
            (30.0, 154.0),
            (52.0, 158.0),
            (72.0, 152.0),
            (48.0, 110.0),
            (60.0, 88.0),
            (60.0, 132.0),
            (90.0, 96.0),
            (94.0, 124.0),
            (64.0, 26.0),
            (96.0, 14.0),
            (64.0, 194.0),
            (96.0, 206.0),
            (81.0, 40.0),
            (95.0, 25.0),
        ] {
            spots.push(Vec2::new(x, z));
        }
        let mirrored: Vec<Vec2> = spots.iter().map(|&p| mirror(p)).collect();
        spots.extend(mirrored);
        self.open_spots = spots
            .into_iter()
            .filter(|&p| self.mine_clear(p, TANK_CLEAR))
            .collect();
        self.repair_spots = vec![
            Vec2::new(62.0, MID),
            Vec2::new(SIZE - 62.0, MID),
            Vec2::new(MID - 14.0, 42.0),
            Vec2::new(MID + 14.0, SIZE - 42.0),
        ];
        for side in 0..2 {
            let base = std::mem::take(&mut self.bases[side]);
            self.bases[side] = base
                .into_iter()
                .filter(|&p| self.mine_clear(p, TANK_CLEAR))
                .collect();
        }
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
        // Over the bridge: the deck, and the pit floor under it.
        let p = Vec2::new(MID, MID - 10.0);
        assert!(
            (land_on(&w, p, 30.0) - DECK_TOP).abs() < 0.1,
            "deck at {}",
            land_on(&w, p, 30.0)
        );
        assert!(
            (land_on(&w, p, -1.0) + PIT_DEPTH).abs() < 0.3,
            "pit floor at {}",
            land_on(&w, p, -1.0)
        );
        // Over the rail tunnel and a gallery: the plateau top, and the cave floor.
        for p in [
            Vec2::new(MID, 30.0),
            Vec2::new(96.0, 25.0),
            Vec2::new(MID, SIZE - 30.0),
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
        // The plateau's top elsewhere.
        assert!((land_on(&w, Vec2::new(66.0, 30.0), 30.0) - PLATEAU_H).abs() < 0.3);
    }

    #[test]
    fn every_spawn_reaches_the_pit_and_both_lanes() {
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
            Vec2::new(MID - 17.0, MID - 16.0),
            Vec2::new(MID + 17.0, MID - 16.0),
            Vec2::new(70.0, 66.0),
            Vec2::new(70.0, 154.0),
            Vec2::new(66.0, 26.0),
        ];
        for &p in m.open_spots.iter().chain(m.bases.iter().flatten()) {
            for g in goals {
                assert!(!nav.path(p, g).is_empty(), "no path from {p} to {g}");
            }
        }
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

    /// Every hull drives up onto the bridge and across the pit on its deck.
    #[test]
    fn tanks_cross_the_bridge() {
        for hull in HULLS {
            let (end, low, _) = drive(hull, Vec2::new(MID, DECK_Z.0 - 5.0), 0.0, 12.0);
            assert!(low > -1.5, "{hull:?} fell into the pit ({low})");
            assert!(end.z > MID + 10.0, "{hull:?} only got to {end}");
        }
    }

    /// Every hull drives in at a gallery's mouth and down it, under the
    /// plateau's top (past the ore cart against its west wall).
    #[test]
    fn tanks_drive_into_the_caves() {
        for hull in HULLS {
            let (end, _, _) = drive(hull, Vec2::new(83.0, CLIFF.1 + 6.0), PI, 5.0);
            assert!(end.z < 40.0, "{hull:?} only got to {end}");
            assert!(end.y < 2.0, "{hull:?} is up at {end}");
        }
    }

    /// Every hull climbs a plateau's ramp to its top.
    #[test]
    fn tanks_climb_onto_the_plateau() {
        for hull in HULLS {
            let (end, _, high) = drive(hull, Vec2::new(20.0, 30.0), FRAC_PI_2, 10.0);
            assert!(end.x > MID - PLATEAU_FLAT, "{hull:?} only got to {end}");
            assert!(high > PLATEAU_H - 0.5, "{hull:?} only got {high} m up");
        }
    }

    /// Routes keep to the level they're on: along the deck over the pit,
    /// across the pit floor under it, through the caves under a plateau,
    /// over the plateau's top above them.
    #[test]
    fn routes_keep_to_their_level() {
        let nav = NavGrid::build(&Map::mine());
        let floor = -PIT_DEPTH;
        let cases = [
            (
                Vec3::new(MID, DECK_TOP, DECK_Z.0 + 6.0),
                Vec3::new(MID, DECK_TOP, DECK_Z.1 - 6.0),
                DECK_TOP,
            ),
            (
                Vec3::new(MID - 10.0, floor, MID + 12.0),
                Vec3::new(MID + 10.0, floor, MID + 12.0),
                floor,
            ),
            (Vec3::new(81.0, 0.0, 34.0), Vec3::new(MID, 0.0, 25.0), 0.0),
            (
                Vec3::new(66.0, PLATEAU_H, 26.0),
                Vec3::new(SIZE - 66.0, PLATEAU_H, 26.0),
                PLATEAU_H,
            ),
        ];
        for (a, b, y) in cases {
            let r = nav.route(a, b);
            assert!(!r.is_empty(), "no route from {a} to {b}");
            let end = *r.last().unwrap();
            assert!(end.distance(b) < 3.0, "route from {a} to {b} ends at {end}");
            assert!(
                r.iter().all(|p| (p.y - y).abs() < 1.0),
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

    /// After a tank on the pit floor under the bridge, a bot coming up
    /// the lane to the bridge doesn't drive out over it on the deck: it
    /// goes down a haul road and in under.
    #[test]
    fn a_bot_gets_under_the_bridge() {
        let d = chase(Vec2::new(MID, 68.0), 0.0, Vec2::new(MID, MID + 10.0), 40.0);
        assert!(d < 12.0, "got no nearer than {d:.1} m");
    }

    /// After a tank in the rail tunnel, a bot on the plateau's top over it
    /// finds the way down and in.
    #[test]
    fn a_bot_gets_into_the_tunnel_under_it() {
        let d = chase(Vec2::new(66.0, 26.0), FRAC_PI_2, Vec2::new(MID, 30.0), 40.0);
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

    #[test]
    fn the_halves_mirror() {
        let m = Map::mine();
        let solid: Vec<&OBox> = m
            .oboxes
            .iter()
            .filter(|o| {
                o.solid
                    && o.center.x > -10.0
                    && o.center.x < SIZE + 10.0
                    && o.center.y > -10.0
                    && o.center.y < SIZE + 10.0
            })
            .collect();
        // Rock is strewn at random, and the wrecked car in the north lane
        // and each side's camp are one of a kind; everything else built is
        // placed in pairs.
        let camp = |p: Vec2| (p.x - MID).abs() > MID - 40.0 && (p.y - MID).abs() < 8.0;
        let wreck = |p: Vec2| p.x > MID + 4.5 && p.x < MID + 16.0 && (158.5..168.9).contains(&p.y);
        let inner: Vec<&&OBox> = solid
            .iter()
            .filter(|o| o.center.min_element() > 6.0 && o.center.max_element() < SIZE - 6.0)
            .filter(|o| o.side_mat != Material::Rubble && !wreck(o.center) && !camp(o.center))
            .collect();
        for o in &inner {
            let p = mirror(o.center);
            assert!(
                inner.iter().any(|q| q.center.distance(p) < 0.6),
                "{} has no twin",
                o.center
            );
        }
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
    const LEVELS: [&str; 6] = ["deck", "under bridge", "pit", "caves", "plateau", "lanes"];

    /// Which of `LEVELS` a tank at `p` is on.
    fn level(p: Vec3) -> usize {
        let q = Vec2::new(p.x, p.z);
        if (p.x - MID).abs() < DECK_HALF && (DECK_Z.0..DECK_Z.1).contains(&p.z) && p.y > -2.0 {
            0
        } else if (p.x - MID).abs() < DECK_HALF + 3.0 && pit_sdf(q) < 0.0 && p.y < -4.0 {
            1
        } else if pit_sdf(q) < -1.0 && p.y < -2.0 {
            2
        } else if cave_distance(q) == 0.0 && plateau_mask(q) > 0.3 && p.y < 4.0 {
            3
        } else if p.y > 5.0 {
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
                    if drop > 3.0 && drop > across * 0.6 && across > 1.0 {
                        if f.arrived > 3.0 && !f.is_husk() && launched[i] < now - 2.0 {
                            falls += 1;
                            eprintln!("t={now:.0} tank {i} fell {drop:.1} m: {q} to {p}");
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

    /// Bots get round the whole mine and fight on every level of it: over
    /// the bridge and under it, in the pit, the caves, on the plateaus and
    /// in the lanes. They don't fall off things, sit stuck, or all crowd
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
        // and a fair share of it off the lanes.
        for (l, name) in LEVELS.iter().enumerate() {
            assert!(fighting[l] > 0.002, "nobody fought {name}");
        }
        assert!(
            fighting[5] < 0.9,
            "{:.0}% of the fighting in the lanes",
            fighting[5] * 100.0
        );
        assert!(time[3] > 0.02 && time[4] > 0.03, "caves {time:?}");
    }
}
