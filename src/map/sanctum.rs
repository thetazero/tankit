//! The Sanctum: a Resonant holy site, the Hold map. A great mesa standing
//! 40 m over the desert floor, sheer on every side, and on its top a church
//! built around the crystal on its altar. Driving off the edge is a fall to
//! your death (a Vigil runs on at the bottom a while, as it does). 300 m
//! square (+x east, +z north), the mesa ~200 m across in the middle:
//!
//! - center: the church, roofed. A long nave from the great west door to the
//!   crossing, transepts north and south with a door at each end, a chancel
//!   and a rounded apse to the east whose wall has fallen in. Arcades of
//!   pillars down the nave; the altar and its crystal at the crossing (the
//!   hold site). A crossing tower over the roof.
//! - around it: a bell tower, chapels, a row of cells, a broken cloister,
//!   graves and Resonant crystals, cover on the way in
//! - west and east edges of the top: the two sides' bases, the Imperium at
//!   the great door, the Resonant behind the apse
//!
//! The top sits at y = 0 and the desert floor at -40, so everything on the
//! top works like any flat map.

use std::f32::consts::{FRAC_PI_2, TAU};

use glam::{Quat, Vec2, Vec3};

use super::strata;
use super::{Ambient, DEFAULT_SIDES, GlowPath, Map, OBox, Terrain};
use crate::render::material::Material;
use crate::rng::Rng;

const SIZE: f32 = 300.0;
const CENTER: Vec2 = Vec2::new(150.0, 150.0);
/// The desert floor under the mesa.
const LOW: f32 = -40.0;
/// Below this you've gone over the edge: dead when you hit the floor.
pub const FALL_Y: f32 = -10.0;

const SAND: Vec3 = Vec3::new(0.78, 0.66, 0.5);
const STONE: Vec3 = Vec3::new(0.82, 0.76, 0.64);
const SLATE: Vec3 = Vec3::new(0.3, 0.28, 0.3);
const GILT: Vec3 = Vec3::new(0.62, 0.47, 0.22);
/// Resonant crystal, drawn glowing.
const CRYSTAL: Vec3 = Vec3::new(0.25, 0.55, 1.0);

// The church (inner faces of its walls).
/// Nave: from the west door to the crossing.
const NAVE_X: (f32, f32) = (102.0, 138.0);
/// Half-width of the nave and the transepts.
const HALF_W: f32 = 12.0;
/// The transepts reach this far north and south of the axis.
const ARM: f32 = 36.0;
/// Chancel east of the crossing, then the apse round its end.
const CHANCEL_X: f32 = 172.0;
const CHANCEL_HALF: f32 = 10.0;
const APSE_R: f32 = 10.0;
const WALL: f32 = 1.6;
const WALL_H: f32 = 18.0;
const ROOF_T: f32 = 1.2;

/// Bays cut back into the mesa: (angle, half-width in radians, depth in m).
/// Kept clear of the bases (east and west) and of everything built.
const BAYS: [(f32, f32, f32); 4] = [
    (0.85, 0.16, 11.0),
    (2.25, 0.15, 9.0),
    (-0.95, 0.18, 15.0),
    (-2.25, 0.17, 15.0),
];
/// Spurs jutting out of it.
const SPURS: [(f32, f32, f32); 4] = [
    (0.45, 0.06, 9.0),
    (1.6, 0.07, 12.0),
    (-0.4, 0.05, 8.0),
    (-2.78, 0.07, 11.0),
];
/// Notches broken out of the top edge, where rock has fallen away.
const NOTCHES: [(f32, f32, f32); 8] = [
    (0.8, 0.025, 3.0),
    (1.35, 0.03, 4.0),
    (1.95, 0.02, 2.5),
    (2.75, 0.03, 3.5),
    (-0.6, 0.025, 3.0),
    (-1.5, 0.03, 4.0),
    (-2.6, 0.02, 2.5),
    (3.4, 0.03, 3.0),
];

/// `d` radians wrapped to -PI..PI.
fn wrap(d: f32) -> f32 {
    (d + std::f32::consts::PI).rem_euclid(TAU) - std::f32::consts::PI
}

/// Sum of bumps at angle `a`: (center, half-width, size) each.
fn bumps(a: f32, list: &[(f32, f32, f32)]) -> f32 {
    list.iter()
        .map(|&(c, w, h)| h * (-(wrap(a - c) / w).powi(2)).exp())
        .sum()
}

/// Edge of the mesa top, distance from the center at angle `a`.
fn rim(a: f32) -> f32 {
    102.0 + 6.0 * (2.0 * a).cos() + 4.0 * (5.0 * a + 0.7).sin() + 1.5 * (9.0 * a + 2.0).sin()
        - bumps(a, &BAYS)
        + bumps(a, &SPURS)
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How far `p` is outside the mesa's edge (negative: on top).
fn past_edge(p: Vec2) -> f32 {
    let d = p - CENTER;
    d.length() - rim(d.y.atan2(d.x))
}

/// The mesa's top: flat round the church, a gentle roll further out, and
/// broken down at the edge where a notch has fallen away.
fn mesa_top(x: f32, z: f32) -> f32 {
    let p = Vec2::new(x, z);
    let r = p.distance(CENTER);
    let a = (z - CENTER.y).atan2(x - CENTER.x);
    let roll = 0.3 * (x * 0.07).sin() * (z * 0.05).cos() * smoothstep(60.0, 80.0, r);
    roll - bumps(a, &NOTCHES) * smoothstep(-8.0, -1.0, past_edge(p))
}

fn ground(x: f32, z: f32) -> f32 {
    let p = Vec2::new(x, z);
    let d = past_edge(p);
    let top = mesa_top(x, z);
    // The floor: scree banked against the cliff's foot, fans of it under
    // the notches where rock came down, dunes beyond.
    let a = (z - CENTER.y).atan2(x - CENTER.x);
    let fans = NOTCHES
        .iter()
        .map(|&(c, w, _)| 7.0 * (-(wrap(a - c) / (w * 2.5)).powi(2)).exp())
        .sum::<f32>();
    let scree = (10.0 + fans) * (-(d - 4.0).max(0.0) / (12.0 + fans)).exp();
    let dunes = 1.5 * (x * 0.04).sin() * (z * 0.05 + 1.0).cos();
    let cones = BUTTES
        .iter()
        .map(|&(a, out, r, h)| talus(p.distance(butte_center(a, out)), r, h))
        .fold(0.0, f32::max);
    let floor = LOW + (scree + dunes).max(cones);
    // The drop itself is steep, behind the rock face (see `strata`).
    let t = smoothstep(-1.5, 0.3, d);
    top * (1.0 - t) + floor * t
}

/// Beyond the arena: the open desert floor (the mesas out there are
/// sculpted, see `sanctum_horizon`). Sunk out of sight under the arena.
fn scenery_ground(x: f32, z: f32) -> f32 {
    let p = Vec2::new(x, z);
    let edge = p.clamp(Vec2::ZERO, Vec2::splat(SIZE));
    if edge == p {
        // Under the arena's own ground, meeting it at the edge, and well
        // down under the mesa: its cells are coarse, and would cut out
        // through the cliff's face.
        let inside = x.min(z).min(SIZE - x).min(SIZE - z);
        return ground(x, z).min(LOW + 6.0) - 0.6 * smoothstep(0.0, 14.0, inside);
    }
    // Carry on from the arena's edge, easing into the open desert.
    let dunes =
        2.0 * (x * 0.02).sin() * (z * 0.025 + 1.0).cos() + 1.0 * (x * 0.05 + z * 0.03).sin();
    let cones = HILLS
        .iter()
        .map(|&(c, r, h)| talus(p.distance(c), r, h))
        .fold(0.0, f32::max);
    ground(edge.x, edge.y)
        + (LOW + dunes.max(cones) - ground(edge.x, edge.y))
            * smoothstep(0.0, 40.0, p.distance(edge))
}

/// Mesas and buttes on the horizon: (center, radius, height over the floor).
const HILLS: [(Vec2, f32, f32); 9] = [
    (Vec2::new(-260.0, 120.0), 90.0, 46.0),
    (Vec2::new(520.0, -80.0), 120.0, 38.0),
    (Vec2::new(420.0, 520.0), 70.0, 52.0),
    (Vec2::new(-150.0, 520.0), 60.0, 30.0),
    (Vec2::new(150.0, -330.0), 100.0, 44.0),
    (Vec2::new(-200.0, -220.0), 18.0, 48.0),
    (Vec2::new(520.0, 260.0), 14.0, 40.0),
    (Vec2::new(60.0, 560.0), 16.0, 58.0),
    (Vec2::new(-330.0, 360.0), 12.0, 34.0),
];

/// Buttes broken away from the mesa, down on the floor: (angle, how far out
/// from the edge, radius, height over the floor).
const BUTTES: [(f32, f32, f32, f32); 2] = [(2.5, 30.0, 8.0, 30.0), (-0.15, 34.0, 6.0, 24.0)];

fn butte_center(a: f32, out: f32) -> Vec2 {
    CENTER + Vec2::new(a.cos(), a.sin()) * (rim(a) + out)
}

/// The cone of scree round a rock `r` across and `h` tall, at distance `d`
/// from its middle.
fn talus(d: f32, r: f32, h: f32) -> f32 {
    0.35 * h * smoothstep(r * 2.2 + 6.0, r * 0.9, d).powf(1.6)
}

impl Map {
    pub fn sanctum() -> Map {
        static BUILT: std::sync::OnceLock<Map> = std::sync::OnceLock::new();
        Map::built_once(&BUILT, Map::build_sanctum)
    }

    fn build_sanctum() -> Map {
        let mut map = Map {
            size: SIZE,
            solids: Vec::new(),
            boxes: Vec::new(),
            wedges: Vec::new(),
            oboxes: Vec::new(),
            terrain: None,
            scenery_terrain: None,
            open_spots: Vec::new(),
            spawn_yaw: None,
            // Late afternoon: a low warm sun out of the west-southwest, so it
            // comes in at the great door and the south windows.
            atmosphere: crate::render::Atmosphere {
                sun_dir: Vec3::new(-0.6, 0.42, -0.45),
                sun_color: Vec3::new(2.3, 1.8, 1.35),
                sky_zenith: Vec3::new(0.3, 0.45, 0.68),
                sky_horizon: Vec3::new(0.86, 0.72, 0.58),
                ground_color: Vec3::new(0.45, 0.33, 0.24),
                fog_density: 0.0014,
                ..crate::render::Atmosphere::default()
            },
            bases: Map::no_bases(&DEFAULT_SIDES),
            factions: DEFAULT_SIDES.to_vec(),
            // In the yards round the church, out from under its roof.
            repair_spots: vec![
                CENTER + Vec2::new(0.0, 58.0),
                CENTER + Vec2::new(0.0, -58.0),
                CENTER + Vec2::new(-66.0, 22.0),
                CENTER + Vec2::new(62.0, -22.0),
            ],
            hold_site: Some(CENTER),
            cores: Vec::new(),
            crystals: Vec::new(),
            glow_paths: Vec::new(),
            beams: Vec::new(),
            fall_y: Some(FALL_Y),
            hold_name: "ALTAR",
            routes: Vec::new(),
            sculpted: crate::render::mesh::MeshData::default(),
            sounds: Vec::new(),
        };
        let mut terrain = Terrain::from_fn(Vec2::ZERO, 2.0, 151, 151, ground);
        terrain.color = SAND;
        terrain.material = Material::Sand;
        map.terrain = Some(terrain);
        let mut rng = Rng::new(0x5a4c_7c11);

        map.cliffs();
        // The church and the precinct draw from `rng` where the cliffs'
        // boulders used to leave it (1760 draws), so they stand where they
        // always have.
        for _ in 0..1760 {
            rng.next_u64();
        }
        map.church(&mut rng);
        map.precinct(&mut rng);
        map.sanctum_horizon();

        // Bases: two columns of drop zones near the west and east edges.
        for (side, xs) in [
            (0, [56.0, 64.0, 72.0]),
            (1, [SIZE - 56.0, SIZE - 64.0, SIZE - 72.0]),
        ] {
            for x in xs {
                for k in 0..7 {
                    map.bases[side].push(Vec2::new(x, 126.0 + k as f32 * 8.0));
                }
            }
        }
        // Roam points about the top.
        for (n, r) in [(10, 48.0f32), (14, 78.0)] {
            for i in 0..n {
                let a = i as f32 / n as f32 * TAU + 0.2;
                map.open_spots
                    .push(CENTER + Vec2::new(a.cos(), a.sin()) * r);
            }
        }
        let blocked = |p: Vec2, m: &Map| {
            past_edge(p) > -10.0 || m.oboxes.iter().any(|o| o.solid && o.distance(p) < 4.0)
        };
        let spots = std::mem::take(&mut map.open_spots);
        map.open_spots = spots.into_iter().filter(|&p| !blocked(p, &map)).collect();
        for side in 0..2 {
            let base = std::mem::take(&mut map.bases[side]);
            map.bases[side] = base.into_iter().filter(|&p| !blocked(p, &map)).collect();
        }

        // Down on the desert floor, a ring of boulders closes the arena (a
        // Vigil runs on a while after the fall).
        map.floor_rim();
        map
    }

    /// Boulders along the arena's edge down on the desert floor, shoulder
    /// to shoulder: sandstone come down off the mesas long ago, too tall and
    /// steep to climb.
    fn floor_rim(&mut self) {
        let mut rng = Rng::new(0x0b01_de55);
        let boulder = |m: &mut Map, rng: &mut Rng, at: Vec2, s: f32, h: f32, solid: bool| {
            let g = m.ground_height(at.x, at.y);
            m.oboxes.push(OBox {
                center: at,
                yaw: rng.range(0.0, TAU),
                half: Vec2::new(s, s * rng.range(0.7, 1.0)),
                y0: g - 1.5,
                y1: g + h,
                tilt: rng.range(-0.15, 0.15),
                roll: rng.range(-0.15, 0.15),
                color: STONE * rng.range(0.72, 0.92),
                side_mat: Material::Rubble,
                top_mat: Material::Rubble,
                solid,
                chip: 0.75,
                ..OBox::default()
            });
        };
        for (p, inward) in self.edge_points(2.6, 4.0) {
            let side = Vec2::new(-inward.y, inward.x);
            let at = p + inward * rng.range(-0.8, 0.4) + side * rng.range(-0.5, 0.5);
            // Mostly big, now and then a great one.
            let (s, h) = if rng.chance(0.15) {
                (rng.range(5.0, 6.5), rng.range(9.0, 14.0))
            } else {
                let s = rng.range(2.8, 4.6);
                (s, s * rng.range(1.0, 2.0))
            };
            boulder(self, &mut rng, at, s, h, true);
            // Smaller ones tumbled out in front of it, and some behind.
            if rng.chance(0.3) {
                let s = rng.range(0.8, 2.0);
                let at = p + inward * rng.range(3.5, 9.0) + side * rng.range(-1.5, 1.5);
                let h = s * rng.range(0.6, 1.1);
                boulder(self, &mut rng, at, s, h, s > 1.4);
            }
            if rng.chance(0.3) {
                let s = rng.range(2.0, 4.0);
                let at = p - inward * rng.range(4.0, 9.0) + side * rng.range(-1.5, 1.5);
                let h = s * rng.range(0.8, 1.6);
                boulder(self, &mut rng, at, s, h, false);
            }
        }
    }

    /// The cliff face round the mesa (layered sandstone, see `strata`),
    /// slabs fallen from it and from its notches lying at its foot, and two
    /// buttes broken away from it, of the same beds.
    fn cliffs(&mut self) {
        let rng = &mut Rng::new(0xc11f_f5ab);
        let top = |p: Vec2| mesa_top(p.x, p.y);
        let floor = |p: Vec2| ground(p.x, p.y);
        let beds = strata::beds(0.6, LOW - 8.0, 1.0, 0x5a4c_bed5);
        let fallen = strata::build(
            &mut self.sculpted,
            &strata::Tower {
                center: CENTER,
                rim: &rim,
                lip: &top,
                floor: &floor,
                beds: &beds,
                core: 0.8,
                spread: 9.5,
                relief: 1.0,
                scale: 1.0,
                cell: 1.0,
                ring_cap: Some(-4.0),
                seed: 1,
            },
        );
        let shades = [
            Vec3::new(0.66, 0.44, 0.29),
            Vec3::new(0.55, 0.34, 0.22),
            Vec3::new(0.74, 0.56, 0.39),
        ];
        // A block of rock lying on the floor `out` past the rim at angle `a`.
        let block = |m: &mut Map, a: f32, out: f32, half: Vec3, rng: &mut Rng| {
            let p = CENTER + Vec2::new(a.cos(), a.sin()) * (rim(a) + out);
            let g = ground(p.x, p.y);
            let rot = Quat::from_rotation_y(rng.range(0.0, TAU))
                * Quat::from_rotation_x(rng.range(-0.45, 0.45))
                * Quat::from_rotation_z(rng.range(-0.45, 0.45));
            let color = shades[rng.index(shades.len())] * rng.range(0.9, 1.08);
            strata::block(
                &mut m.sculpted,
                Vec3::new(p.x, g + half.y * 0.35, p.y),
                half,
                rot,
                color,
                &floor,
                rng.next_u64() as u32,
            );
        };
        // Slabs that came out of the face, at its foot below where they were.
        for f in &fallen {
            let half = (f.size * Vec3::new(0.45, 0.5, 0.5)).min(Vec3::new(3.0, 2.0, 2.0));
            block(self, f.angle, rng.range(10.0, 16.0), half, rng);
        }
        // Under the notches, where the edge came down: big slabs and their
        // pieces.
        for &(c, w, depth) in &NOTCHES {
            for k in 0..(depth as usize + 3) {
                let a = c + rng.range(-2.5, 2.5) * w;
                let s = if k < 2 {
                    rng.range(2.2, 3.4)
                } else {
                    rng.range(0.9, 2.2)
                };
                let half = Vec3::new(s, s * rng.range(0.3, 0.55), s * rng.range(0.55, 0.85));
                block(self, a, rng.range(9.0, 22.0), half, rng);
            }
        }
        // And loose rock all round the foot.
        for _ in 0..60 {
            let s = rng.range(0.5, 1.6);
            let half = Vec3::new(s, s * rng.range(0.4, 0.8), s * rng.range(0.6, 0.9));
            block(self, rng.range(0.0, TAU), rng.range(10.0, 40.0), half, rng);
        }
        // Two buttes standing off the mesa, broken away from it long ago:
        // the same beds, where they stand.
        for (k, (a, out, r, h)) in BUTTES.into_iter().enumerate() {
            let c = butte_center(a, out);
            let sd = k as f32 * 2.3;
            let brim =
                move |t: f32| r + 1.5 * (3.0 * t + sd).sin() + 0.8 * (7.0 * t + 2.0 * sd).sin();
            let btop = move |_: Vec2| LOW + h;
            let fallen = strata::build(
                &mut self.sculpted,
                &strata::Tower {
                    center: c,
                    rim: &brim,
                    lip: &btop,
                    floor: &floor,
                    beds: &beds,
                    core: 0.0,
                    spread: r * 0.25,
                    relief: 0.6,
                    scale: 0.8,
                    cell: 1.2,
                    ring_cap: None,
                    seed: 10 + k as u64,
                },
            );
            for f in &fallen {
                let half = (f.size * Vec3::new(0.45, 0.5, 0.5)).min(Vec3::new(2.0, 1.5, 1.5));
                let d = rng.range(r + 3.0, r + 8.0);
                let p = c + Vec2::new(f.angle.cos(), f.angle.sin()) * d;
                let g = ground(p.x, p.y);
                strata::block(
                    &mut self.sculpted,
                    Vec3::new(p.x, g + half.y * 0.35, p.y),
                    half,
                    Quat::from_rotation_y(rng.range(0.0, TAU))
                        * Quat::from_rotation_x(rng.range(-0.4, 0.4)),
                    shades[k % shades.len()],
                    &floor,
                    rng.next_u64() as u32,
                );
            }
        }
    }

    /// A straight stone wall from `a` to `b`, `thick` through and `height`
    /// tall, with `openings` cut through it: (from, to) along it in meters,
    /// and (bottom, top) heights. Lintels and sills stay.
    fn stone_wall(
        &mut self,
        a: Vec2,
        b: Vec2,
        thick: f32,
        height: f32,
        openings: &[(f32, f32, f32, f32)],
    ) {
        let len = a.distance(b);
        let dir = (b - a) / len;
        let yaw = (-dir.y).atan2(dir.x);
        let piece = |m: &mut Map, u0: f32, u1: f32, y0: f32, y1: f32| {
            if u1 - u0 < 0.05 || y1 - y0 < 0.05 {
                return;
            }
            m.oboxes.push(OBox {
                center: a + dir * (u0 + u1) * 0.5,
                yaw,
                half: Vec2::new((u1 - u0) * 0.5, thick * 0.5),
                y0: if y0 <= 0.0 { -1.0 } else { y0 },
                y1,
                color: STONE,
                side_mat: Material::Plaster,
                top_mat: Material::Rubble,
                chip: 0.12,
                ..OBox::default()
            });
        };
        // Split the wall wherever an opening starts or ends; in each stretch,
        // stone fills every height no opening there leaves open.
        let mut cuts: Vec<f32> = vec![0.0, len];
        for &(u0, u1, _, _) in openings {
            cuts.extend([u0.clamp(0.0, len), u1.clamp(0.0, len)]);
        }
        cuts.sort_by(f32::total_cmp);
        cuts.dedup();
        for w in cuts.windows(2) {
            let (u0, u1) = (w[0], w[1]);
            let mid = (u0 + u1) * 0.5;
            let mut gaps: Vec<(f32, f32)> = openings
                .iter()
                .filter(|o| o.0 <= mid && mid <= o.1)
                .map(|o| (o.2, o.3))
                .collect();
            gaps.sort_by(|p, q| p.0.total_cmp(&q.0));
            let mut y = 0.0;
            for (g0, g1) in gaps {
                piece(self, u0, u1, y, g0);
                y = y.max(g1);
            }
            piece(self, u0, u1, y, height);
        }
    }

    /// A flat roof slab over `min`..`max` at `y`, and a pitched roof of
    /// slate on it, its ridge along x (`along_x`) or z, rising `rise`.
    fn roof(&mut self, min: Vec2, max: Vec2, y: f32, along_x: bool, rise: f32) {
        let c = (min + max) * 0.5;
        let h = (max - min) * 0.5;
        self.oboxes.push(OBox {
            center: c,
            half: h,
            y0: y,
            y1: y + ROOF_T,
            color: STONE * 0.9,
            side_mat: Material::Plaster,
            top_mat: Material::Rubble,
            ..OBox::default()
        });
        // Two planes from the eaves to the ridge.
        let (len, span) = if along_x { (h.x, h.y) } else { (h.y, h.x) };
        let slope = (rise / span).atan();
        let run = (span * span + rise * rise).sqrt() * 0.5;
        let mid = y + ROOF_T + rise * 0.5;
        for s in [1.0f32, -1.0] {
            let off = span * 0.5 * s;
            let (center, yaw) = if along_x {
                (c + Vec2::new(0.0, off), 0.0)
            } else {
                (c + Vec2::new(off, 0.0), FRAC_PI_2)
            };
            self.oboxes.push(OBox {
                center,
                yaw,
                half: Vec2::new(len + 0.6, run + 0.4),
                y0: mid - 0.3,
                y1: mid + 0.3,
                color: SLATE,
                side_mat: Material::Roof,
                top_mat: Material::Roof,
                tilt: slope * s,
                ..OBox::default()
            });
        }
        // Gable ends: stone stepped up under the planes.
        for k in 0..4 {
            let f = (k as f32 + 0.5) / 4.0;
            let w = span * (1.0 - f);
            let (y0, y1) = (
                y + ROOF_T + rise * k as f32 / 4.0,
                y + ROOF_T + rise * (k + 1) as f32 / 4.0,
            );
            for end in [-1.0f32, 1.0] {
                let (center, half) = if along_x {
                    (c + Vec2::new(end * (h.x - 0.4), 0.0), Vec2::new(0.4, w))
                } else {
                    (c + Vec2::new(0.0, end * (h.y - 0.4)), Vec2::new(w, 0.4))
                };
                self.oboxes.push(OBox {
                    center,
                    half,
                    y0,
                    y1,
                    color: STONE,
                    side_mat: Material::Plaster,
                    top_mat: Material::Rubble,
                    ..OBox::default()
                });
            }
        }
    }

    /// A square pillar.
    fn pillar(&mut self, at: Vec2, half: f32, height: f32) {
        self.oboxes.push(OBox {
            center: at,
            half: Vec2::splat(half),
            y0: -1.0,
            y1: height,
            color: STONE * 0.96,
            side_mat: Material::Plaster,
            top_mat: Material::Rubble,
            chip: 0.15,
            ..OBox::default()
        });
    }

    /// A cluster of `n` glowing crystals rooted at `at`, leaning out.
    fn resonant_crystals(
        &mut self,
        at: Vec2,
        base_y: f32,
        n: usize,
        width: f32,
        height: f32,
        rng: &mut Rng,
    ) {
        for i in 0..n {
            let a = i as f32 / n as f32 * TAU + rng.range(-0.3, 0.3);
            let out = Vec2::new(a.cos(), a.sin());
            let p = at + out * width * rng.range(0.2, 0.6);
            let w = width * rng.range(0.25, 0.4);
            self.oboxes.push(OBox {
                center: p,
                yaw: FRAC_PI_2 - a,
                half: Vec2::splat(w * 0.5),
                y0: base_y - 0.5,
                y1: base_y + height * rng.range(0.55, 1.0),
                color: CRYSTAL * rng.range(0.85, 1.1),
                side_mat: Material::Glow,
                top_mat: Material::Glow,
                tilt: rng.range(0.1, 0.35),
                ..OBox::default()
            });
        }
    }

    /// A grave slab set into the ground at `p` (ground height `g`), long
    /// along z: flat enough to drive over.
    fn grave_slab(&mut self, p: Vec2, g: f32, rng: &mut Rng) {
        self.oboxes.push(OBox {
            center: p,
            yaw: rng.range(-0.06, 0.06),
            half: Vec2::new(0.5, 0.9),
            y0: g - 0.3,
            y1: g + rng.range(0.06, 0.12),
            color: STONE * rng.range(0.78, 0.9),
            side_mat: Material::Plaster,
            top_mat: Material::Rubble,
            chip: 0.06,
            solid: false,
            ..OBox::default()
        });
    }

    /// `n` small Resonant shards grown up out of the soil at `p`, glowing
    /// faintly: a grave's marker, small enough to drive through.
    fn grave_shards(&mut self, p: Vec2, g: f32, n: usize, rng: &mut Rng) {
        for _ in 0..n {
            let a = rng.range(0.0, TAU);
            let at = p + Vec2::new(a.cos(), a.sin()) * rng.range(0.05, 0.3);
            let w = rng.range(0.09, 0.15);
            self.oboxes.push(OBox {
                center: at,
                yaw: FRAC_PI_2 - a,
                half: Vec2::splat(w),
                y0: g - 0.3,
                y1: g + rng.range(0.35, 0.75),
                color: CRYSTAL * rng.range(0.3, 0.4),
                side_mat: Material::Glow,
                top_mat: Material::Glow,
                tilt: rng.range(0.1, 0.4),
                solid: false,
                ..OBox::default()
            });
        }
    }

    /// A solid tomb at `p`, long along z: a chest tomb on a plinth, or a
    /// small mausoleum with a stepped roof and a door at its south end.
    fn tomb(&mut self, p: Vec2, mausoleum: bool, rng: &mut Rng) {
        let g = self.ground_height(p.x, p.y);
        let half = if mausoleum {
            Vec2::new(1.5, 2.1)
        } else {
            Vec2::new(1.1, 1.8)
        };
        let shade = rng.range(0.86, 0.95);
        let block = |m: &mut Map, half: Vec2, y0: f32, y1: f32, color: Vec3, chip: f32| {
            m.oboxes.push(OBox {
                center: p,
                half,
                y0,
                y1,
                color,
                side_mat: Material::Plaster,
                top_mat: Material::Rubble,
                chip,
                ..OBox::default()
            });
        };
        // The plinth, then the body.
        block(
            self,
            half + Vec2::splat(0.3),
            g - 0.6,
            g + 0.35,
            STONE * shade * 0.9,
            0.1,
        );
        if mausoleum {
            block(self, half, g, g + 3.0, STONE * shade, 0.12);
            // A cornice, and a roof stepped up from it.
            block(
                self,
                half + Vec2::splat(0.25),
                g + 3.0,
                g + 3.35,
                STONE * shade * 0.92,
                0.0,
            );
            for k in 0..3 {
                let f = k as f32 / 3.0;
                let y = g + 3.35 + k as f32 * 0.4;
                block(self, half * (1.0 - f * 0.55), y, y + 0.4, SLATE, 0.0);
            }
            // The door: dark, flush with the south face.
            self.oboxes.push(OBox {
                center: p - Vec2::new(0.0, half.y + 0.02),
                half: Vec2::new(0.55, 0.05),
                y0: g + 0.35,
                y1: g + 2.2,
                color: SLATE * 0.6,
                side_mat: Material::Metal,
                top_mat: Material::Metal,
                solid: false,
                ..OBox::default()
            });
        } else {
            block(self, half, g, g + 1.5, STONE * shade, 0.1);
            // The lid, overhanging.
            block(
                self,
                half + Vec2::splat(0.15),
                g + 1.5,
                g + 1.8,
                STONE * shade * 0.95,
                0.05,
            );
        }
        // Shards grown up against its foot.
        let corner = Vec2::new(half.x + 0.45, half.y * rng.range(-0.6, 0.6));
        let side = if rng.range(0.0, 1.0) < 0.5 { -1.0 } else { 1.0 };
        self.grave_shards(p + corner * Vec2::new(side, 1.0), g, 2, rng);
    }

    fn church(&mut self, rng: &mut Rng) {
        let (z0, z1) = (CENTER.y - HALF_W, CENTER.y + HALF_W);
        let (cx0, cx1) = (CENTER.x - HALF_W, CENTER.x + HALF_W);
        let (nx0, _) = NAVE_X;
        let t = WALL;
        let h = WALL_H;
        let window = |u: f32| (u - 1.5, u + 1.5, 8.0, 14.5);
        let window_top = |u: f32| (u - 0.8, u + 0.8, 14.5, 15.5);

        // West front: the great door, a rose window over it.
        self.stone_wall(
            Vec2::new(nx0 - t * 0.5, z0 - t),
            Vec2::new(nx0 - t * 0.5, z1 + t),
            t,
            h,
            &[
                // The great door, its head stepped to a point.
                (7.6, 19.6, 0.0, 10.0),
                (8.6, 18.6, 10.0, 11.0),
                (10.1, 17.1, 11.0, 12.0),
                (11.8, 15.4, 12.0, 12.8),
                // The rose window.
                (11.1, 16.1, 14.0, 17.0),
                (12.1, 15.1, 13.4, 17.6),
            ],
        );
        // Nave walls, windows down both; a breach in the north wall.
        let nave_len = cx0 - nx0 + t;
        for (z, breach) in [(z0 - t * 0.5, false), (z1 + t * 0.5, true)] {
            let mut open: Vec<(f32, f32, f32, f32)> = [6.0, 15.0, 33.0]
                .iter()
                .flat_map(|&u| [window(u), window_top(u)])
                .collect();
            if breach {
                open.push((21.0, 29.0, 0.0, 10.5));
            } else {
                open.extend([window(24.0), window_top(24.0)]);
            }
            self.stone_wall(
                Vec2::new(nx0 - t, z),
                Vec2::new(nx0 - t + nave_len, z),
                t,
                h,
                &open,
            );
        }
        // The breach's rubble, spilled in and out.
        for k in 0..7 {
            let p = Vec2::new(nx0 + 21.0 + rng.range(0.0, 10.0), z1 + rng.range(-5.0, 6.0));
            self.oboxes.push(OBox {
                center: p,
                yaw: rng.range(0.0, TAU),
                half: Vec2::new(rng.range(0.4, 1.2), rng.range(0.3, 0.8)),
                y0: -0.4,
                y1: rng.range(0.3, 0.9),
                color: STONE * rng.range(0.85, 1.0),
                side_mat: Material::Rubble,
                top_mat: Material::Rubble,
                tilt: rng.range(-0.3, 0.3),
                solid: k == 0,
                chip: 0.2,
                ..OBox::default()
            });
        }

        // Transepts: side walls with windows, an end wall with a door and a
        // window over it.
        for s in [1.0f32, -1.0] {
            let inner = CENTER.y + s * HALF_W;
            let end = CENTER.y + s * (ARM + t * 0.5);
            for x in [cx0 - t * 0.5, cx1 + t * 0.5] {
                self.stone_wall(
                    Vec2::new(x, inner + s * t * 0.5),
                    Vec2::new(x, end + s * t * 0.5),
                    t,
                    h,
                    &[window(8.0), window_top(8.0), window(18.0), window_top(18.0)],
                );
            }
            self.stone_wall(
                Vec2::new(cx0 - t, end),
                Vec2::new(cx1 + t, end),
                t,
                h,
                &[
                    (8.6, 18.6, 0.0, 9.0),
                    (9.6, 17.6, 9.0, 10.0),
                    (11.1, 16.1, 10.0, 10.8),
                    (11.6, 15.6, 12.5, 15.5),
                    (12.6, 14.6, 15.5, 16.3),
                ],
            );
        }

        // Chancel walls, then the apse in segments round its end, one fallen in.
        let (cz0, cz1) = (CENTER.y - CHANCEL_HALF, CENTER.y + CHANCEL_HALF);
        for z in [cz0 - t * 0.5, cz1 + t * 0.5] {
            self.stone_wall(
                Vec2::new(cx1, z),
                Vec2::new(CHANCEL_X, z),
                t,
                h,
                &[window(5.0), window_top(5.0)],
            );
        }
        // Fill between the transepts' east walls and the narrower chancel.
        for (za, zb) in [(z0 - t, cz0), (cz1, z1 + t)] {
            self.oboxes.push(OBox {
                center: Vec2::new(cx1 + t * 0.5, (za + zb) * 0.5),
                half: Vec2::new(t * 0.5, (zb - za).abs() * 0.5),
                y0: -1.0,
                y1: h,
                color: STONE,
                side_mat: Material::Plaster,
                top_mat: Material::Rubble,
                ..OBox::default()
            });
        }
        let apse = Vec2::new(CHANCEL_X, CENTER.y);
        let segs = 5;
        let fallen = 1..=2;
        for i in 0..segs {
            let a0 = -FRAC_PI_2 + i as f32 / segs as f32 * std::f32::consts::PI;
            let a1 = -FRAC_PI_2 + (i + 1) as f32 / segs as f32 * std::f32::consts::PI;
            let r = APSE_R + t * 0.5;
            let p = |a: f32| apse + Vec2::new(a.cos(), a.sin()) * r;
            let (pa, pb) = (p(a0), p(a1));
            let ext = (pb - pa).normalize() * 0.5;
            if fallen.contains(&i) {
                // Fallen in: a low heap of its stones, half in, half out.
                for _ in 0..9 {
                    let f = rng.range(0.0, 1.0);
                    let c =
                        pa.lerp(pb, f) + (pa.lerp(pb, f) - apse).normalize() * rng.range(-4.0, 5.0);
                    self.oboxes.push(OBox {
                        center: c,
                        yaw: rng.range(0.0, TAU),
                        half: Vec2::new(rng.range(0.5, 1.4), rng.range(0.4, 0.9)),
                        y0: -0.4,
                        y1: rng.range(0.3, 1.0),
                        color: STONE * rng.range(0.85, 1.0),
                        side_mat: Material::Rubble,
                        top_mat: Material::Rubble,
                        tilt: rng.range(-0.3, 0.3),
                        solid: false,
                        chip: 0.2,
                        ..OBox::default()
                    });
                }
                // The wall's stumps either side, broken off.
                let ends = [(i == *fallen.start(), pa, pb), (i == *fallen.end(), pb, pa)];
                for (q, toward) in ends.into_iter().filter(|e| e.0).map(|e| (e.1, e.2)) {
                    let d = (toward - q).normalize();
                    self.oboxes.push(OBox {
                        center: q + d * 0.5,
                        yaw: (-d.y).atan2(d.x),
                        half: Vec2::new(0.5, t * 0.5),
                        y0: -1.0,
                        y1: rng.range(4.0, 8.0),
                        color: STONE,
                        side_mat: Material::Plaster,
                        top_mat: Material::Rubble,
                        broken: true,
                        ..OBox::default()
                    });
                }
                // Over it, the wall still hangs from the roof.
                self.stone_wall(pa - ext, pb + ext, t, h, &[(0.0, 10.0, 0.0, 11.0)]);
                continue;
            }
            self.stone_wall(pa - ext, pb + ext, t, h, &[]);
        }

        // Stone floor throughout: nave, crossing and transepts, chancel and apse.
        for (min, max) in [
            (Vec2::new(nx0 - t, z0 - t), Vec2::new(cx1, z1 + t)),
            (
                Vec2::new(cx0 - t, CENTER.y - ARM - t),
                Vec2::new(cx1 + t, CENTER.y + ARM + t),
            ),
            (
                Vec2::new(cx1, cz0 - t),
                Vec2::new(CHANCEL_X + APSE_R, cz1 + t),
            ),
        ] {
            self.oboxes.push(OBox {
                center: (min + max) * 0.5,
                half: (max - min) * 0.5,
                y0: -0.6,
                y1: 0.04,
                color: STONE * 0.8,
                side_mat: Material::Paving,
                top_mat: Material::Paving,
                solid: false,
                ..OBox::default()
            });
        }

        // Twin towers flanking the west front, their spires stepping up.
        for s in [-1.0f32, 1.0] {
            let c = Vec2::new(nx0 - 2.5, CENTER.y + s * (HALF_W + 2.5));
            self.oboxes.push(OBox {
                center: c,
                half: Vec2::splat(3.6),
                y0: -1.0,
                y1: 27.0,
                color: STONE,
                side_mat: Material::Plaster,
                top_mat: Material::Rubble,
                chip: 0.15,
                ..OBox::default()
            });
            for k in 0..5 {
                let f = k as f32 / 5.0;
                self.oboxes.push(OBox {
                    center: c,
                    yaw: TAU / 8.0 * (k % 2) as f32,
                    half: Vec2::splat(3.8 * (1.0 - f)),
                    y0: 27.0 + f * 12.0,
                    y1: 27.0 + (f + 0.2) * 12.0,
                    color: SLATE,
                    side_mat: Material::Roof,
                    top_mat: Material::Roof,
                    ..OBox::default()
                });
            }
        }

        // Roofs: nave, transepts, chancel and apse; the crossing tower.
        let ry = h;
        self.roof(
            Vec2::new(nx0 - t, z0 - t),
            Vec2::new(cx0, z1 + t),
            ry,
            true,
            7.0,
        );
        self.roof(
            Vec2::new(cx0 - t, CENTER.y - ARM - t),
            Vec2::new(cx1 + t, CENTER.y + ARM + t),
            ry,
            false,
            7.0,
        );
        self.roof(
            Vec2::new(cx1, cz0 - t),
            Vec2::new(CHANCEL_X + APSE_R + t, cz1 + t),
            ry - 2.0,
            true,
            5.0,
        );
        // The crossing tower, and its spire.
        let tower = 7.5;
        for s in [1.0f32, -1.0] {
            for (a, b) in [
                (Vec2::new(-tower, s * tower), Vec2::new(tower, s * tower)),
                (Vec2::new(s * tower, -tower), Vec2::new(s * tower, tower)),
            ] {
                let base = ry + ROOF_T + 4.0;
                let len = a.distance(b);
                self.oboxes.push(OBox {
                    center: CENTER + (a + b) * 0.5,
                    yaw: if a.x == b.x { FRAC_PI_2 } else { 0.0 },
                    half: Vec2::new(len * 0.5 + 0.6, 0.6),
                    y0: ry,
                    y1: base + 10.0,
                    color: STONE,
                    side_mat: Material::Plaster,
                    top_mat: Material::Rubble,
                    chip: 0.12,
                    ..OBox::default()
                });
            }
        }
        let spire0 = ry + ROOF_T + 14.0;
        for k in 0..6 {
            let f = k as f32 / 6.0;
            let w = (tower + 0.6) * (1.0 - f);
            self.oboxes.push(OBox {
                center: CENTER,
                yaw: TAU / 8.0 * (k % 2) as f32,
                half: Vec2::splat(w),
                y0: spire0 + f * 16.0,
                y1: spire0 + (f + 1.0 / 6.0) * 16.0,
                color: SLATE,
                side_mat: Material::Roof,
                top_mat: Material::Roof,
                ..OBox::default()
            });
        }
        self.oboxes.push(OBox {
            center: CENTER,
            half: Vec2::splat(0.25),
            y0: spire0 + 16.0,
            y1: spire0 + 20.0,
            color: GILT,
            side_mat: Material::Metal,
            top_mat: Material::Metal,
            ..OBox::default()
        });

        // Inside: arcades down the nave, the crossing's piers.
        for x in [108.0f32, 116.0, 124.0, 132.0] {
            for z in [CENTER.y - 5.5, CENTER.y + 5.5] {
                self.pillar(Vec2::new(x, z), 0.8, h);
            }
        }
        for (dx, dz) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
            self.pillar(
                CENTER + Vec2::new(dx * (HALF_W - 1.2), dz * (HALF_W - 1.2)),
                1.2,
                h,
            );
        }
        // A pillar down across the south aisle: cover.
        self.oboxes.push(OBox {
            center: Vec2::new(119.0, CENTER.y - 8.0),
            yaw: 0.25,
            half: Vec2::new(5.5, 0.8),
            y0: 0.0,
            y1: 1.6,
            color: STONE * 0.94,
            side_mat: Material::Plaster,
            top_mat: Material::Rubble,
            chip: 0.25,
            ..OBox::default()
        });
        // Choir stalls down the chancel.
        for z in [CENTER.y - CHANCEL_HALF + 1.4, CENTER.y + CHANCEL_HALF - 1.4] {
            self.oboxes.push(OBox {
                center: Vec2::new(CHANCEL_X - 3.0, z),
                half: Vec2::new(3.5, 0.6),
                y0: 0.0,
                y1: 1.3,
                color: Vec3::new(0.38, 0.26, 0.16),
                side_mat: Material::Wood,
                top_mat: Material::Wood,
                ..OBox::default()
            });
        }

        // The altar at the crossing, and the crystal grown up out of it.
        self.oboxes.push(OBox {
            center: CENTER,
            half: Vec2::new(1.8, 1.1),
            y0: -0.5,
            y1: 1.3,
            color: STONE * 1.05,
            side_mat: Material::Plaster,
            top_mat: Material::Rubble,
            ..OBox::default()
        });
        self.oboxes.push(OBox {
            center: CENTER,
            half: Vec2::new(2.0, 1.3),
            y0: 1.3,
            y1: 1.5,
            color: GILT,
            side_mat: Material::Metal,
            top_mat: Material::Metal,
            ..OBox::default()
        });
        self.resonant_crystals(CENTER, 1.5, 6, 2.0, 4.5, rng);
        self.sound(Ambient::Altar, Vec3::new(CENTER.x, 4.0, CENTER.y));
        // A line of light down the nave from the door to the altar.
        let path: Vec<Vec3> = (0..=12)
            .map(|k| {
                Vec3::new(
                    nx0 + 1.0 + k as f32 * (CENTER.x - 3.0 - nx0) / 12.0,
                    0.05,
                    CENTER.y,
                )
            })
            .collect();
        let widths = vec![0.5; path.len()];
        self.glow_paths.push(GlowPath {
            points: path,
            widths,
            color: CRYSTAL,
        });
    }

    /// Everything round the church: bell tower, chapels, cells, a cloister,
    /// graves, crystals.
    fn precinct(&mut self, rng: &mut Rng) {
        // A small roofed building, its door on the side facing `door`.
        let chapel = |m: &mut Map, c: Vec2, half: Vec2, door: Vec2| {
            let (x0, x1, z0, z1) = (c.x - half.x, c.x + half.x, c.y - half.y, c.y + half.y);
            let ht = 7.0;
            let w = 1.0;
            let walls = [
                (Vec2::new(x0, z0), Vec2::new(x1, z0), Vec2::new(0.0, -1.0)),
                (Vec2::new(x1, z1), Vec2::new(x0, z1), Vec2::new(0.0, 1.0)),
                (Vec2::new(x0, z1), Vec2::new(x0, z0), Vec2::new(-1.0, 0.0)),
                (Vec2::new(x1, z0), Vec2::new(x1, z1), Vec2::new(1.0, 0.0)),
            ];
            for (a, b, n) in walls {
                let len = a.distance(b);
                let open = if n.dot(door) > 0.5 {
                    vec![(len * 0.5 - 2.5, len * 0.5 + 2.5, 0.0, 5.0)]
                } else {
                    vec![(len * 0.5 - 0.8, len * 0.5 + 0.8, 3.0, 5.0)]
                };
                m.stone_wall(a, b, w, ht, &open);
            }
            m.roof(
                Vec2::new(x0 - 0.5, z0 - 0.5),
                Vec2::new(x1 + 0.5, z1 + 0.5),
                ht,
                half.x > half.y,
                2.5,
            );
        };
        // Chapels on the four quarters, doors facing the church.
        for (c, half) in [
            (Vec2::new(98.0, 196.0), Vec2::new(7.0, 5.0)),
            (Vec2::new(202.0, 104.0), Vec2::new(7.0, 5.0)),
            (Vec2::new(208.0, 192.0), Vec2::new(5.0, 6.0)),
            (Vec2::new(92.0, 108.0), Vec2::new(5.0, 6.0)),
        ] {
            let to = CENTER - c;
            let door = if to.x.abs() > to.y.abs() {
                Vec2::new(to.x.signum(), 0.0)
            } else {
                Vec2::new(0.0, to.y.signum())
            };
            chapel(self, c, half, door);
        }

        // The bell tower, north-west of the great door.
        let bt = Vec2::new(82.0, 186.0);
        let half = 4.0;
        self.oboxes.push(OBox {
            center: bt,
            half: Vec2::splat(half),
            y0: -1.0,
            y1: 22.0,
            color: STONE,
            side_mat: Material::Plaster,
            top_mat: Material::Rubble,
            chip: 0.15,
            ..OBox::default()
        });
        for (dx, dz) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
            self.oboxes.push(OBox {
                center: bt + Vec2::new(dx, dz) * (half - 0.6),
                half: Vec2::splat(0.6),
                y0: 22.0,
                y1: 28.0,
                color: STONE,
                side_mat: Material::Plaster,
                top_mat: Material::Rubble,
                ..OBox::default()
            });
        }
        self.oboxes.push(OBox {
            center: bt,
            half: Vec2::splat(half + 0.4),
            y0: 28.0,
            y1: 29.0,
            color: STONE * 0.9,
            side_mat: Material::Plaster,
            top_mat: Material::Rubble,
            ..OBox::default()
        });
        for k in 0..4 {
            let f = k as f32 / 4.0;
            self.oboxes.push(OBox {
                center: bt,
                half: Vec2::splat((half + 0.2) * (1.0 - f)),
                y0: 29.0 + f * 7.0,
                y1: 29.0 + (f + 0.25) * 7.0,
                color: SLATE,
                side_mat: Material::Roof,
                top_mat: Material::Roof,
                ..OBox::default()
            });
        }
        // The bell in the belfry.
        self.sound(Ambient::Bell, Vec3::new(bt.x, 25.5, bt.y));
        for (r, y0, y1) in [(0.6f32, 26.2, 27.4), (1.0, 24.8, 26.2), (1.25, 24.4, 24.8)] {
            self.oboxes.push(OBox {
                center: bt,
                half: Vec2::splat(r),
                y0,
                y1,
                color: GILT,
                side_mat: Material::Metal,
                top_mat: Material::Metal,
                solid: false,
                ..OBox::default()
            });
        }

        // A row of cells along the south, each open to the north.
        for k in 0..6 {
            let c = Vec2::new(122.0 + k as f32 * 11.0, 80.0);
            let (x0, x1, z0, z1) = (c.x - 4.5, c.x + 4.5, c.y - 3.5, c.y + 3.5);
            self.stone_wall(Vec2::new(x0, z0), Vec2::new(x1, z0), 1.0, 5.0, &[]);
            self.stone_wall(Vec2::new(x0, z0), Vec2::new(x0, z1), 1.0, 5.0, &[]);
            if k == 5 {
                self.stone_wall(Vec2::new(x1, z0), Vec2::new(x1, z1), 1.0, 5.0, &[]);
            }
            self.oboxes.push(OBox {
                center: c + Vec2::new(0.0, -1.0),
                half: Vec2::new(5.0, 2.8),
                y0: 5.0,
                y1: 5.6,
                color: STONE * 0.9,
                side_mat: Material::Plaster,
                top_mat: Material::Rubble,
                ..OBox::default()
            });
        }
        // Their mirror on the north: a broken cloister, low walls and stumps.
        for k in 0..9 {
            let x = 120.0 + k as f32 * 7.5;
            let z = 222.0;
            let ht = if k % 3 == 1 {
                rng.range(1.2, 2.0)
            } else {
                rng.range(3.0, 6.0)
            };
            self.oboxes.push(OBox {
                center: Vec2::new(x, z),
                half: Vec2::new(rng.range(1.8, 3.2), 0.6),
                y0: -1.0,
                y1: ht,
                color: STONE * rng.range(0.9, 1.0),
                side_mat: Material::Plaster,
                top_mat: Material::Rubble,
                broken: true,
                chip: 0.2,
                ..OBox::default()
            });
            if k % 2 == 0 {
                self.pillar(Vec2::new(x + 3.7, z - 5.0), 0.6, rng.range(2.0, 5.0));
            }
        }

        // Graveyards round the church: rows of graves you drive over (flat
        // slabs set in the ground, and Resonant shards grown up out of the
        // soil marking them), and two solid tombs in each, spaced out so
        // there's open ground to steer through between them, and kept off
        // the walls and chapels near by so they don't make dead ends.
        // Tombs: (offset from the yard's middle, a mausoleum or a plain tomb).
        let w = Vec2::new;
        for (gx, gz, tombs) in [
            (
                78.0f32,
                128.0f32,
                [(w(-4.6, -3.6), true), (w(4.6, -3.6), false)],
            ),
            (110.0, 186.0, [(w(4.6, 3.6), true), (w(-1.0, -4.4), false)]),
            (184.0, 116.0, [(w(-4.6, 3.6), true), (w(4.6, -3.6), false)]),
        ] {
            let c = Vec2::new(gx + 4.5, gz + 5.2);
            for &(off, mausoleum) in &tombs {
                self.tomb(c + off, mausoleum, rng);
            }
            for i in 0..4 {
                for j in 0..5 {
                    let p = Vec2::new(gx + i as f32 * 3.0, gz + j as f32 * 2.6)
                        + Vec2::new(rng.range(-0.2, 0.2), rng.range(-0.2, 0.2));
                    if tombs
                        .iter()
                        .any(|&(off, _)| (p - (c + off)).abs().cmplt(Vec2::new(2.6, 3.0)).all())
                    {
                        continue;
                    }
                    let g = self.ground_height(p.x, p.y);
                    if rng.range(0.0, 1.0) < 0.6 {
                        self.grave_slab(p, g, rng);
                        // A shard at its head, now and then.
                        if rng.range(0.0, 1.0) < 0.5 {
                            self.grave_shards(p + Vec2::new(0.0, 1.15), g, 1, rng);
                        }
                    } else {
                        let n = 2 + rng.index(2);
                        self.grave_shards(p, g, n, rng);
                    }
                }
            }
        }

        // Crystals breaking the surface about the top: cover and light.
        for (p, n, w, ht) in [
            (Vec2::new(110.0, 130.0), 4, 1.6, 3.5),
            (Vec2::new(190.0, 172.0), 4, 1.6, 3.5),
            (Vec2::new(150.0, 112.0), 3, 1.2, 2.6),
            (Vec2::new(150.0, 192.0), 3, 1.2, 2.6),
            (Vec2::new(75.0, 152.0), 3, 1.4, 3.0),
            (Vec2::new(226.0, 148.0), 3, 1.4, 3.0),
            (Vec2::new(176.0, 220.0), 4, 1.8, 4.0),
            (Vec2::new(130.0, 72.0), 4, 1.8, 4.0),
        ] {
            let g = self.ground_height(p.x, p.y);
            self.resonant_crystals(p, g, n, w, ht, rng);
            self.sound(Ambient::Shrine, Vec3::new(p.x, g + ht * 0.5, p.y));
        }
        // Low stone walls on each side's approach: cover on the run in.
        for (c, yaw, len) in [
            (Vec2::new(88.0, 140.0), 0.2, 6.0),
            (Vec2::new(86.0, 162.0), -0.3, 5.0),
            (Vec2::new(212.0, 160.0), 0.2, 6.0),
            (Vec2::new(214.0, 138.0), -0.3, 5.0),
        ] {
            self.oboxes.push(OBox {
                center: c,
                yaw,
                half: Vec2::new(len, 0.6),
                y0: -1.0,
                y1: 1.8,
                color: STONE * 0.92,
                side_mat: Material::Plaster,
                top_mat: Material::Rubble,
                broken: true,
                chip: 0.2,
                ..OBox::default()
            });
        }
    }

    fn sanctum_horizon(&mut self) {
        let (cell, reach) = (6.0f32, 700.0f32);
        let n = ((SIZE + 2.0 * reach) / cell) as usize + 1;
        let mut t = Terrain::from_fn(Vec2::splat(-reach), cell, n, n, scenery_ground);
        t.color = SAND * 0.95;
        t.material = Material::Sand;
        self.scenery_terrain = Some(t);

        // Mesas and buttes on the horizon, layered like this one, each with
        // its own bays and its own beds; seen only from far off, so built
        // coarser.
        let floor = |p: Vec2| scenery_ground(p.x, p.y);
        for (k, (c, r, h)) in HILLS.into_iter().enumerate() {
            let sd = k as f32 * 1.7 + 0.5;
            let bays = [
                (sd, 0.12, r * 0.18),
                (sd + 2.4, 0.1, r * 0.12),
                (sd + 4.1, 0.08, r * 0.15),
            ];
            let hrim = move |a: f32| {
                r * (1.0
                    + 0.08 * (2.0 * a + sd).sin()
                    + 0.06 * (5.0 * a + 2.0 * sd).sin()
                    + 0.03 * (11.0 * a + sd).sin())
                    - if r > 30.0 { bumps(a, &bays) } else { 0.0 }
            };
            let htop = move |_: Vec2| LOW + h;
            let scale = if r > 30.0 { 2.0 } else { 1.4 };
            let beds = strata::beds(LOW + h + 0.5, LOW - 6.0, scale, 0x0b0e_d500 + k as u64);
            strata::build(
                &mut self.sculpted,
                &strata::Tower {
                    center: c,
                    rim: &hrim,
                    lip: &htop,
                    floor: &floor,
                    beds: &beds,
                    core: 0.0,
                    spread: (r * 0.2).min(9.5),
                    relief: (r / 20.0).clamp(0.6, 1.6),
                    scale,
                    cell: 2.2,
                    ring_cap: None,
                    seed: 20 + k as u64,
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::NavGrid;

    #[test]
    fn every_spawn_reaches_the_altar() {
        let m = Map::sanctum();
        assert!(
            m.open_spots.len() >= 12,
            "{} open spots",
            m.open_spots.len()
        );
        for (side, base) in m.bases.iter().enumerate() {
            assert!(base.len() >= 12, "side {side} has {} spawns", base.len());
        }
        let nav = NavGrid::build(&m);
        for &p in m.open_spots.iter().chain(m.bases.iter().flatten()) {
            assert!(
                !nav.path(p, CENTER + Vec2::new(-6.0, 0.0)).is_empty(),
                "no path from {p} to the altar"
            );
        }
    }

    #[test]
    fn the_top_is_level_and_the_floor_is_far_below() {
        let m = Map::sanctum();
        for p in m
            .bases
            .iter()
            .flatten()
            .chain(&m.repair_spots)
            .chain([&CENTER])
        {
            assert!(
                m.ground_height(p.x, p.y).abs() < 0.5,
                "{p} isn't on the top"
            );
        }
        assert!(m.ground_height(5.0, 5.0) < FALL_Y - 15.0);
        assert!(m.ground_height(150.0, 295.0) < FALL_Y - 15.0);
    }

    /// Boulders close the desert floor's edge all round: a Vigil running
    /// on after the fall never drives out of the arena.
    #[test]
    fn boulders_close_the_floor() {
        Map::sanctum().assert_edge_walled(0.5, |_| false);
    }

    #[test]
    fn repair_spots_are_open() {
        let m = Map::sanctum();
        let nav = NavGrid::build(&m);
        for &p in &m.repair_spots {
            assert!(!nav.is_blocked(p), "repair spot {p} is blocked");
        }
    }

    /// The edge is off-limits to bots' paths.
    #[test]
    fn nav_keeps_off_the_edge() {
        let m = Map::sanctum();
        let nav = NavGrid::build(&m);
        for i in 0..64 {
            let a = i as f32 / 64.0 * TAU;
            let p = CENTER + Vec2::new(a.cos(), a.sin()) * (rim(a) - 2.0);
            assert!(nav.is_blocked(p), "{p} at the edge is open");
        }
    }
}

#[cfg(test)]
mod breach {
    use super::*;
    /// The fallen apse wall is a way in: a path from outside it to the altar
    /// runs through it, not round by the transept doors.
    #[test]
    fn the_apse_breach_is_a_way_in() {
        let m = Map::sanctum();
        let nav = crate::ai::NavGrid::build(&m);
        let a = -std::f32::consts::FRAC_PI_2 + 2.0 / 5.0 * std::f32::consts::PI;
        let out = Vec2::new(CHANCEL_X, CENTER.y) + Vec2::new(a.cos(), a.sin()) * (APSE_R + 9.0);
        let path = nav.path(out, CENTER + Vec2::new(8.0, 0.0));
        assert!(!path.is_empty());
        let len: f32 = path.windows(2).map(|w| w[0].distance(w[1])).sum();
        assert!(len < 45.0, "path round is {len:.0} m");
    }
}
