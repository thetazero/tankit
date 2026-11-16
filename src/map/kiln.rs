//! The Kiln: Ithessar ground, under attack. The Ithessar keep a sun here:
//! six crystal monoliths lean in over a raised dais and hold a ball of
//! plasma in the air between them. The Imperium has landed at the east edge
//! to take it. 220 m square (+x east, +z north), mirrored west to east so
//! neither side has the better ground:
//!
//! - center: the dais, a low plateau with the kiln on it (the hold site)
//! - north: the glass canyon, a sunken lane with a river of plasma down its
//!   floor and crystal walls along both rims, broken by ways in
//! - south: the ridge, high ground along the south edge with a pass cut
//!   through the middle; fallen monoliths along its crest for cover
//! - between bases and the dais: spire groves, cover for the approach
//! - west: the Ithessar gate, arches of leaning spires, lanterns
//! - east: the Imperium foothold, drop pods, prefab walls, floodlights, and
//!   a dropship that came down short in the field
//! - beyond the edge: ash dunes, and the Ithessar's great spires on the
//!   horizon with another kept sun hanging among them

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use glam::{Vec2, Vec3};

use super::{Core, Map, OBox, Terrain};
use crate::faction::Faction;
use crate::render::material::Material;
use crate::rng::Rng;
use crate::tank::ithessar::PLASMA;

const SIZE: f32 = 220.0;
const CENTER: Vec2 = Vec2::new(110.0, 110.0);
/// The dais: flat top inside `DAIS_R`, sloping down to the plain by `DAIS_FOOT`.
const DAIS_R: f32 = 21.0;
const DAIS_FOOT: f32 = 33.0;
const DAIS_H: f32 = 3.0;
/// Monolith ring radius, and the core's height above the dais.
const KILN_R: f32 = 14.0;
const CORE_Y: f32 = 13.0;
/// The canyon: floor half-width, rim half-width, depth.
const CANYON_FLOOR: f32 = 6.5;
const CANYON_RIM: f32 = 13.0;
const CANYON_DEPTH: f32 = 2.8;
/// The ridge: crest line and height.
const RIDGE_Z: f32 = 36.0;
const RIDGE_H: f32 = 6.0;

const ASH: Vec3 = Vec3::new(0.46, 0.43, 0.45);
const GLASS: Vec3 = Vec3::new(0.07, 0.055, 0.08);
const DEAD_GLASS: Vec3 = Vec3::new(0.11, 0.1, 0.11);
const CONCRETE: Vec3 = Vec3::new(0.56, 0.57, 0.6);
const IMPERIAL: Vec3 = Vec3::new(0.78, 0.82, 0.9);
const STEEL: Vec3 = Vec3::new(0.32, 0.33, 0.36);
const FLOODLIGHT: Vec3 = Vec3::new(0.85, 0.92, 1.0);

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The canyon's center line (z) at `x`: it bends.
fn canyon_z(x: f32) -> f32 {
    180.0 + 5.0 * (x * 0.034 + 0.6).sin()
}

/// How far outside the arena a point is (0 or less: inside).
fn edge_distance(p: Vec2) -> f32 {
    let q = (p - Vec2::splat(SIZE * 0.5)).abs() - Vec2::splat(SIZE * 0.5);
    q.max(Vec2::ZERO).length() + q.x.max(q.y).min(0.0)
}

/// Play ground: the dais, the canyon, the ridge, low swells.
pub fn ground(x: f32, z: f32) -> f32 {
    let p = Vec2::new(x, z);
    let dais = DAIS_H * (1.0 - smoothstep(DAIS_R, DAIS_FOOT, p.distance(CENTER)));
    let dz = (z - canyon_z(x)).abs();
    let canyon = -CANYON_DEPTH * (1.0 - smoothstep(CANYON_FLOOR, CANYON_RIM, dz));
    // Gentle up the north face, steep down the south face to the edge, and a
    // pass cut through the middle.
    let face = if z >= RIDGE_Z {
        1.0 - smoothstep(RIDGE_Z, RIDGE_Z + 30.0, z)
    } else {
        smoothstep(RIDGE_Z - 16.0, RIDGE_Z, z)
    };
    let pass = 1.0 - 0.7 * (-((x - CENTER.x) / 12.0).powi(2)).exp();
    let ridge = RIDGE_H * face * pass * (0.9 + 0.1 * (x * 0.09).sin());
    // Low swells, calm on the dais and in the canyon.
    let calm = smoothstep(DAIS_FOOT, DAIS_FOOT + 10.0, p.distance(CENTER))
        * smoothstep(CANYON_RIM, CANYON_RIM + 8.0, dz);
    let swell = ((x * 0.045 + 0.3).sin() * (z * 0.052).cos() * 0.7
        + (x * 0.11 - z * 0.07).sin() * 0.25)
        * calm;
    dais + canyon + ridge + swell
}

/// Ash dunes outside the arena: meets the play ground at the edge and rolls
/// away. Inside, it hides under the play terrain.
fn scenery_ground(x: f32, z: f32) -> f32 {
    let p = Vec2::new(x, z);
    let d = edge_distance(p);
    if d < -0.01 {
        return -10.0;
    }
    let edge = p.clamp(Vec2::ZERO, Vec2::splat(SIZE));
    let base = ground(edge.x, edge.y);
    let along = edge.x * 0.8 + edge.y * 1.2;
    let berm = 6.0
        * (0.6 + 0.4 * (along * 0.05).sin())
        * smoothstep(0.0, 14.0, d)
        * (1.0 - 0.5 * smoothstep(20.0, 60.0, d));
    let dunes = smoothstep(10.0, 50.0, d)
        * (5.0 * (x * 0.017 + (z * 0.011).sin() * 2.0).sin().abs()
            + 2.5 * ((x * 0.006 - z * 0.009).sin() + 1.0));
    base + berm + dunes
}

impl Map {
    pub fn kiln() -> Map {
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
            // Alien dusk: a low rose sun, an indigo sky, a little ash on the wind.
            atmosphere: crate::render::Atmosphere {
                sun_dir: Vec3::new(-0.7, 0.36, 0.5),
                sun_color: Vec3::new(2.6, 1.8, 1.55),
                sky_zenith: Vec3::new(0.07, 0.07, 0.22),
                sky_horizon: Vec3::new(0.55, 0.36, 0.45),
                ground_color: Vec3::new(0.22, 0.18, 0.22),
                fog_density: 0.0014,
                wind: Vec2::new(-2.0, 1.2),
                blowing_sand: 0.25,
                dust: Vec3::new(0.62, 0.58, 0.62),
            },
            bases: Default::default(),
            factions: [Faction::Ithessar, Faction::Imperium],
            repair_spots: vec![
                // The canyon floor, the ridge pass, and the two groves.
                Vec2::new(CENTER.x, canyon_z(CENTER.x)),
                Vec2::new(CENTER.x, RIDGE_Z + 2.0),
                Vec2::new(64.0, 140.0),
                Vec2::new(156.0, 80.0),
            ],
            hold_site: Some(CENTER),
            cores: Vec::new(),
        };
        let mut terrain = Terrain::from_fn(Vec2::ZERO, 2.0, 111, 111, ground);
        terrain.color = ASH;
        terrain.material = Material::Sand;
        map.terrain = Some(terrain);
        let mut rng = Rng::new(0x1735_5a12);

        map.kiln_ring(&mut rng);
        map.canyon(&mut rng);
        map.ridge_crest(&mut rng);
        map.groves(&mut rng);
        map.ithessar_gate(&mut rng);
        map.imperium_foothold(&mut rng);
        map.kiln_spawns();
        map.perimeter(false);
        map.kiln_horizon(&mut rng);
        map
    }

    /// The kiln: six monoliths on the dais, leaning in, a seam of plasma up
    /// each, holding the core between their tips. Low shards ring the dais
    /// edge as cover on the way up.
    fn kiln_ring(&mut self, rng: &mut Rng) {
        let mut prongs = Vec::new();
        for i in 0..6 {
            let a = i as f32 / 6.0 * TAU + PI / 6.0;
            let out = Vec2::new(a.cos(), a.sin());
            let at = CENTER + out * KILN_R;
            let g = self.ground_height(at.x, at.y);
            let (h, lean) = (rng.range(17.0, 19.0), -0.5);
            self.monolith(at, a, Vec2::new(1.7, 1.3), g - 1.0, g + h, lean, true);
            // Where its tip ends up: the box turns about its middle.
            let mid = (h + 1.0) * 0.5;
            let tip = Vec3::new(at.x, g - 1.0 + mid, at.y)
                + Vec3::new(out.x, 0.0, out.y) * (mid * lean.sin())
                + Vec3::Y * mid * lean.cos();
            prongs.push(tip);
        }
        let g = self.ground_height(CENTER.x, CENTER.y);
        self.cores.push(Core {
            at: Vec3::new(CENTER.x, g + CORE_Y, CENTER.y),
            radius: 3.6,
            prongs,
        });
        // A glowing ring set into the dais floor under the core.
        self.ring_glow(CENTER, 6.0, g, 0.5);
        // Breastworks of short shards round the dais rim, between the
        // monoliths, with gaps to drive through.
        for i in 0..6 {
            let a = i as f32 / 6.0 * TAU;
            let at = CENTER + Vec2::new(a.cos(), a.sin()) * (DAIS_R - 1.0);
            self.spires(at, 2, 2.4, rng.range(2.5, 3.5), rng);
        }
    }

    /// The glass canyon: a plasma river down the floor, crystal walls along
    /// both rims with breaks every so often, and a few shards on the floor.
    fn canyon(&mut self, rng: &mut Rng) {
        // The river: short glowing segments following the bend.
        let mut x = 6.0;
        while x < SIZE - 6.0 {
            let (z0, z1) = (canyon_z(x), canyon_z(x + 5.0));
            let mid = Vec2::new(x + 2.5, (z0 + z1) * 0.5);
            let dir = Vec2::new(5.0, z1 - z0);
            let g = self.ground_height(mid.x, mid.y);
            self.oboxes.push(OBox {
                center: mid,
                yaw: dir.x.atan2(dir.y),
                half: Vec2::new(0.8, dir.length() * 0.5 + 0.05),
                y0: g - 0.3,
                y1: g + 0.03,
                color: PLASMA * 0.8,
                side_mat: Material::Glow,
                top_mat: Material::Glow,
                solid: false,
                ..OBox::default()
            });
            x += 5.0;
        }
        // Rim walls: runs of leaning slabs, broken for ways in. The breaks
        // line up with the groves and the dais so every lane can drop in.
        let breaks = [30.0, 68.0, 110.0, 152.0, 190.0];
        for side in [-1.0f32, 1.0] {
            let mut x = 8.0;
            while x < SIZE - 8.0 {
                if breaks.iter().any(|b| (x - b).abs() < 8.0) {
                    x += 3.0;
                    continue;
                }
                let z = canyon_z(x) + side * (CANYON_RIM + 1.0);
                // Lean out over the canyon a little: walls, not fences.
                let a = if side > 0.0 { FRAC_PI_2 } else { -FRAC_PI_2 };
                let at = Vec2::new(x, z);
                let g = self.ground_height(at.x, at.y);
                let h = if rng.chance(0.2) {
                    rng.range(8.0, 11.0)
                } else {
                    rng.range(3.0, 6.5)
                };
                let seam = h > 7.0 || rng.chance(0.25);
                self.monolith(
                    at,
                    a + rng.range(-0.15, 0.15),
                    Vec2::new(rng.range(1.8, 3.4), rng.range(0.7, 1.1)),
                    g - 0.8,
                    g + h,
                    -rng.range(0.08, 0.2),
                    seam,
                );
                x += rng.range(5.0, 9.0);
            }
        }
        // A few broken shards on the floor to weave round.
        for k in 0..6 {
            let x = 22.0 + k as f32 * 35.0 + rng.range(-4.0, 4.0);
            let at = Vec2::new(x, canyon_z(x) + rng.range(-3.0, 3.0));
            self.spires(at, 2, 1.6, rng.range(2.5, 4.0), rng);
        }
    }

    /// Fallen monoliths along the ridge crest: long, low cover with a view
    /// over the field. Dead ones: no seam, the crystal gone grey.
    fn ridge_crest(&mut self, rng: &mut Rng) {
        for x in [28.0f32, 62.0, 158.0, 192.0] {
            let at = Vec2::new(x + rng.range(-3.0, 3.0), RIDGE_Z + rng.range(1.0, 4.0));
            let g = self.ground_height(at.x, at.y);
            self.oboxes.push(OBox {
                center: at,
                yaw: FRAC_PI_2 + rng.range(-0.3, 0.3),
                half: Vec2::new(1.3, rng.range(6.0, 8.0)),
                y0: g - 0.8,
                y1: g + 1.9,
                color: DEAD_GLASS,
                side_mat: Material::Vhaal,
                top_mat: Material::Vhaal,
                ..OBox::default()
            });
            // Its broken-off tip, standing where it fell.
            let tip = at + Vec2::new(rng.range(-9.0, 9.0), rng.range(3.0, 6.0));
            let gt = self.ground_height(tip.x, tip.y);
            self.oboxes.push(OBox {
                center: tip,
                yaw: rng.range(0.0, TAU),
                half: Vec2::new(1.0, 0.9),
                y0: gt - 0.6,
                y1: gt + 2.4,
                color: DEAD_GLASS,
                side_mat: Material::Vhaal,
                top_mat: Material::Vhaal,
                ..OBox::default()
            });
        }
        // The pass: two spires flanking it, still lit.
        for s in [-1.0f32, 1.0] {
            let at = Vec2::new(CENTER.x + s * 13.0, RIDGE_Z);
            let g = self.ground_height(at.x, at.y);
            let a = if s > 0.0 { 0.0 } else { PI };
            self.monolith(at, a, Vec2::new(1.3, 1.0), g - 0.8, g + 10.0, -0.25, true);
        }
    }

    /// Spire groves between each base and the dais (mirrored), and loose
    /// spires across the field.
    fn groves(&mut self, rng: &mut Rng) {
        for (cx, flip) in [(62.0f32, 1.0f32), (158.0, -1.0)] {
            let spots = [
                (0.0, 0.0),
                (10.0, 14.0),
                (-8.0, -16.0),
                (14.0, -6.0),
                (-12.0, 10.0),
            ];
            let mut r = Rng::new(0x9e0e);
            for (dx, dz) in spots {
                let at = Vec2::new(cx + dx * flip, CENTER.y + dz * flip);
                self.spires(at, 3, 3.2, r.range(5.0, 8.5), &mut r);
            }
        }
        // Loose spires, placed in mirrored pairs.
        let mut placed = 0;
        let mut tries = 0;
        while placed < 4 && tries < 200 {
            tries += 1;
            let at = Vec2::new(rng.range(40.0, 100.0), rng.range(52.0, 165.0));
            let clear = at.distance(CENTER) > DAIS_FOOT + 4.0
                && (at.y - canyon_z(at.x)).abs() > CANYON_RIM + 6.0
                && !self.oboxes.iter().any(|o| o.solid && o.distance(at) < 9.0);
            if !clear {
                continue;
            }
            let h = rng.range(3.5, 6.0);
            let mirror = Vec2::new(SIZE - at.x, SIZE - at.y);
            for p in [at, mirror] {
                let mut r = Rng::new(placed as u64 * 31 + 7);
                self.spires(p, 2, 2.0, h, &mut r);
            }
            placed += 1;
        }
    }

    /// The Ithessar side: two arches of leaning spires over the way out of
    /// the base, a crescent of tall slabs behind it, glowing pads to drop
    /// onto, lanterns.
    fn ithessar_gate(&mut self, rng: &mut Rng) {
        // Arches over the two ways out (north-east and south-east of base).
        for z in [CENTER.y + 24.0, CENTER.y - 24.0] {
            for s in [-1.0f32, 1.0] {
                let at = Vec2::new(34.0, z + s * 6.5);
                let g = self.ground_height(at.x, at.y);
                let a = if s > 0.0 { FRAC_PI_2 } else { -FRAC_PI_2 };
                self.monolith(at, a, Vec2::new(1.1, 0.9), g - 1.0, g + 12.0, -0.42, true);
            }
            let g = self.ground_height(34.0, z);
            self.lantern(Vec3::new(34.0, g + 12.5, z), 1.0);
        }
        // The crescent: tall slabs along the west edge, backing the base.
        for k in 0..9 {
            let z = CENTER.y - 48.0 + k as f32 * 12.0;
            let at = Vec2::new(5.0 + (k as f32 - 4.0).powi(2) * 0.25, z);
            let g = self.ground_height(at.x, at.y);
            let h = rng.range(9.0, 15.0) - (k as f32 - 4.0).abs();
            self.monolith(
                at,
                PI,
                Vec2::new(1.4, 1.0),
                g - 1.0,
                g + h,
                -0.18,
                k % 2 == 0,
            );
        }
        // Screens: low walls of crystal across the field, where the Imperium
        // put its prefab walls on the other side.
        for k in 0..6 {
            let z = SIZE - (50.0 + k as f32 * 24.0) + rng.range(-3.0, 3.0);
            let at = Vec2::new(SIZE - 172.0 - (k % 2) as f32 * 6.0, z);
            let g = self.ground_height(at.x, at.y);
            self.oboxes.push(OBox {
                center: at,
                yaw: rng.range(-0.2, 0.2),
                half: Vec2::new(0.7, 5.0),
                y0: g - 0.5,
                y1: g + 2.2,
                color: GLASS,
                side_mat: Material::Vhaal,
                top_mat: Material::Vhaal,
                ..OBox::default()
            });
            // A seam along its top.
            self.oboxes.push(OBox {
                center: at,
                half: Vec2::new(0.76, 4.4),
                y0: g + 1.7,
                y1: g + 1.85,
                color: PLASMA,
                side_mat: Material::Glow,
                top_mat: Material::Glow,
                solid: false,
                ..self.oboxes[self.oboxes.len() - 1]
            });
        }
        // Drop pads: glowing rings where pilots come in.
        for &p in &[
            Vec2::new(14.0, CENTER.y - 12.0),
            Vec2::new(14.0, CENTER.y + 12.0),
            Vec2::new(20.0, CENTER.y),
        ] {
            let g = self.ground_height(p.x, p.y);
            self.ring_glow(p, 3.0, g, 0.35);
        }
    }

    /// The Imperium side: drop pods, prefab walls, floodlight masts, and a
    /// dropship that came down short, out in the field (cover for both).
    fn imperium_foothold(&mut self, rng: &mut Rng) {
        // Drop pods: squat octagons (two crossed boxes) with a cap.
        for &(x, z) in &[(198.0, 56.0), (198.0, 164.0), (184.0, 110.0)] {
            let at = Vec2::new(x, z);
            let g = self.ground_height(x, z);
            for yaw in [0.0, PI / 4.0] {
                self.oboxes.push(OBox {
                    center: at,
                    yaw,
                    half: Vec2::splat(2.6),
                    y0: g - 0.6,
                    y1: g + 5.0,
                    color: IMPERIAL,
                    side_mat: Material::Painted,
                    top_mat: Material::Painted,
                    chip: 0.2,
                    ..OBox::default()
                });
            }
            self.oboxes.push(OBox {
                center: at,
                yaw: PI / 8.0,
                half: Vec2::splat(1.6),
                y0: g + 5.0,
                y1: g + 6.3,
                color: STEEL,
                side_mat: Material::Rust,
                top_mat: Material::Rust,
                ..OBox::default()
            });
        }
        // Prefab walls in short staggered runs, facing the dais.
        for k in 0..6 {
            let z = 50.0 + k as f32 * 24.0 + rng.range(-3.0, 3.0);
            let at = Vec2::new(172.0 + (k % 2) as f32 * 6.0, z);
            let g = self.ground_height(at.x, at.y);
            self.oboxes.push(OBox {
                center: at,
                yaw: rng.range(-0.2, 0.2),
                half: Vec2::new(0.7, 5.0),
                y0: g - 0.5,
                y1: g + 2.2,
                color: CONCRETE,
                side_mat: Material::Concrete,
                top_mat: Material::Concrete,
                chip: 0.3,
                ..OBox::default()
            });
        }
        // Floodlight masts: a steel pole with a lamp head lit toward the field.
        for &(x, z) in &[(216.0, 92.0), (216.0, 128.0), (188.0, 66.0), (188.0, 154.0)] {
            let g = self.ground_height(x, z);
            self.oboxes.push(OBox {
                center: Vec2::new(x, z),
                half: Vec2::splat(0.25),
                y0: g - 0.5,
                y1: g + 11.0,
                color: STEEL,
                side_mat: Material::Rust,
                top_mat: Material::Rust,
                ..OBox::default()
            });
            self.oboxes.push(OBox {
                center: Vec2::new(x - 0.4, z),
                yaw: FRAC_PI_2,
                half: Vec2::new(1.2, 0.35),
                y0: g + 10.6,
                y1: g + 11.5,
                color: FLOODLIGHT,
                side_mat: Material::Glow,
                top_mat: Material::Glow,
                solid: false,
                ..OBox::default()
            });
        }
        // The dropship: down short of the foothold, nose ploughed in, one
        // wing torn off and lying beside it.
        let at = Vec2::new(150.0, 150.0);
        let yaw: f32 = 0.55;
        let g = self.ground_height(at.x, at.y);
        let fwd = Vec2::new(yaw.sin(), yaw.cos());
        let right = Vec2::new(fwd.y, -fwd.x);
        self.oboxes.push(OBox {
            center: at,
            yaw,
            half: Vec2::new(3.2, 11.0),
            y0: g - 1.5,
            y1: g + 4.0,
            color: IMPERIAL * 0.85,
            side_mat: Material::Painted,
            top_mat: Material::Painted,
            tilt: 0.08,
            chip: 0.4,
            ..OBox::default()
        });
        self.oboxes.push(OBox {
            center: at - fwd * 9.0,
            yaw,
            half: Vec2::new(2.4, 2.5),
            y0: g + 1.0,
            y1: g + 6.0,
            color: STEEL,
            side_mat: Material::Rust,
            top_mat: Material::Rust,
            ..OBox::default()
        });
        self.oboxes.push(OBox {
            center: at + right * 6.5 - fwd * 2.0,
            yaw: yaw + 0.2,
            half: Vec2::new(4.0, 2.2),
            y0: g - 0.3,
            y1: g + 0.6,
            color: IMPERIAL * 0.8,
            side_mat: Material::Painted,
            top_mat: Material::Painted,
            chip: 0.5,
            ..OBox::default()
        });
        self.oboxes.push(OBox {
            center: at - right * 10.0 + fwd * 3.0,
            yaw: yaw + 1.1,
            half: Vec2::new(4.2, 2.0),
            y0: g - 0.4,
            y1: g + 0.8,
            color: IMPERIAL * 0.75,
            side_mat: Material::Painted,
            top_mat: Material::Painted,
            tilt: 0.12,
            chip: 0.6,
            ..OBox::default()
        });
        // Its twin on the Ithessar side: a great spire, fallen across the
        // field the same way (same cover, mirrored).
        let at = Vec2::new(SIZE - 150.0, SIZE - 150.0);
        let g = self.ground_height(at.x, at.y);
        self.oboxes.push(OBox {
            center: at,
            yaw,
            half: Vec2::new(2.6, 12.0),
            y0: g - 1.2,
            y1: g + 3.6,
            color: DEAD_GLASS,
            side_mat: Material::Vhaal,
            top_mat: Material::Vhaal,
            tilt: -0.06,
            ..OBox::default()
        });
        for k in 0..3 {
            let p = at + right * (5.5 + k as f32 * 1.5) + fwd * (k as f32 * 4.0 - 4.0);
            let gp = self.ground_height(p.x, p.y);
            self.oboxes.push(OBox {
                center: p,
                yaw: rng.range(0.0, TAU),
                half: Vec2::new(0.9, 0.8),
                y0: gp - 0.5,
                y1: gp + 1.4 + k as f32 * 0.5,
                color: DEAD_GLASS,
                side_mat: Material::Vhaal,
                top_mat: Material::Vhaal,
                ..OBox::default()
            });
        }
    }

    /// Bases at the west and east edges, and places for bots to roam to:
    /// the dais, the canyon, the ridge, the groves.
    fn kiln_spawns(&mut self) {
        for (side, xs) in [(0, [12.0, 20.0]), (1, [SIZE - 12.0, SIZE - 20.0])] {
            for x in xs {
                for k in 0..9 {
                    self.bases[side].push(Vec2::new(x, CENTER.y - 40.0 + k as f32 * 10.0));
                }
            }
        }
        let mut spots = vec![CENTER + Vec2::new(0.0, 8.0), CENTER + Vec2::new(0.0, -8.0)];
        for x in [40.0, 80.0, 110.0, 140.0, 180.0] {
            spots.push(Vec2::new(x, canyon_z(x)));
            spots.push(Vec2::new(x, RIDGE_Z + 6.0));
        }
        for (dx, dz) in [
            (-48.0, 0.0),
            (48.0, 0.0),
            (-30.0, 34.0),
            (30.0, -34.0),
            (-30.0, -34.0),
            (30.0, 34.0),
        ] {
            spots.push(CENTER + Vec2::new(dx, dz));
        }
        let blocked = |p: Vec2, m: &Map| m.oboxes.iter().any(|o| o.solid && o.distance(p) < 4.5);
        self.open_spots = spots.into_iter().filter(|&p| !blocked(p, self)).collect();
        for side in 0..2 {
            let base = std::mem::take(&mut self.bases[side]);
            self.bases[side] = base.into_iter().filter(|&p| !blocked(p, self)).collect();
        }
    }

    /// Beyond the edge: ash dunes, the Ithessar's great spires standing on
    /// the horizon with seams lit, and another kept sun hanging among them.
    fn kiln_horizon(&mut self, rng: &mut Rng) {
        let (cell, reach) = (4.0f32, 560.0f32);
        let n = ((SIZE + 2.0 * reach) / cell) as usize + 1;
        let mut t = Terrain::from_fn(Vec2::splat(-reach), cell, n, n, scenery_ground);
        t.color = ASH * 0.95;
        t.material = Material::Sand;
        self.scenery_terrain = Some(t);

        let first = self.oboxes.len();
        // Great spires in a loose ring, tallest to the west (Ithessar country).
        for k in 0..22 {
            let a = k as f32 / 22.0 * TAU + rng.range(-0.1, 0.1);
            let d = rng.range(260.0, 480.0);
            let c = CENTER + Vec2::new(a.cos(), a.sin()) * d;
            let west = (-a.cos()).max(0.0);
            let h = rng.range(40.0, 80.0) + 70.0 * west;
            let g = scenery_ground(c.x, c.y);
            let w = rng.range(5.0, 11.0);
            let lean = rng.range(-0.18, 0.18);
            let seam = rng.chance(0.6);
            self.monolith(c, a, Vec2::new(w, w * 0.8), g - 5.0, g + h, lean, seam);
            // Some stand in pairs, a shorter one leaning in against the first.
            if rng.chance(0.45) {
                let side = Vec2::new(-a.sin(), a.cos()) * w * 2.6;
                let h2 = h * rng.range(0.45, 0.7);
                self.monolith(
                    c + side,
                    a + FRAC_PI_2,
                    Vec2::new(w * 0.7, w * 0.6),
                    g - 5.0,
                    g + h2,
                    -0.35,
                    true,
                );
            }
        }
        // Low crystal outcrops nearer in, half drowned in ash.
        for _ in 0..26 {
            let a = rng.range(0.0, TAU);
            let d = rng.range(30.0, 200.0);
            let c = CENTER + Vec2::new(a.cos(), a.sin()) * (SIZE * 0.72 + d);
            if edge_distance(c) < 16.0 {
                continue;
            }
            let h = rng.range(4.0, 14.0);
            let mut r = Rng::new(rng.next_u64());
            self.spires(c, 3, 4.0, h, &mut r);
        }
        for o in &mut self.oboxes[first..] {
            o.solid = false;
        }
        // The other sun, far off to the north-west, among the tall spires.
        let far = CENTER + Vec2::new(-330.0, 260.0);
        self.cores.push(Core {
            at: Vec3::new(far.x, scenery_ground(far.x, far.y) + 140.0, far.y),
            radius: 26.0,
            prongs: Vec::new(),
        });
    }

    /// A worked-crystal monolith at `at` facing `a` (its local +z points that
    /// way): footprint `half`, from `y0` to `y1`, tipped by `lean` (negative
    /// leans its top back toward -z). With `seam`, a band of plasma runs up
    /// its sides.
    #[allow(clippy::too_many_arguments)]
    fn monolith(&mut self, at: Vec2, a: f32, half: Vec2, y0: f32, y1: f32, lean: f32, seam: bool) {
        let yaw = FRAC_PI_2 - a;
        self.oboxes.push(OBox {
            center: at,
            yaw,
            half,
            y0,
            y1,
            color: GLASS,
            side_mat: Material::Vhaal,
            top_mat: Material::Vhaal,
            tilt: lean,
            ..OBox::default()
        });
        if seam {
            // Slightly proud of the faces across x, inset at the ends.
            let h = y1 - y0;
            self.oboxes.push(OBox {
                center: at,
                yaw,
                half: Vec2::new(half.x + 0.06, (half.y * 0.15).max(0.12)),
                y0: y0 + (h * 0.15).min(2.0),
                y1: y1 - (h * 0.08).min(1.2),
                color: PLASMA,
                side_mat: Material::Glow,
                top_mat: Material::Glow,
                tilt: lean,
                solid: false,
                ..OBox::default()
            });
        }
    }

    /// `n` spires rooted round `at`, leaning outward: `width` thick, up to
    /// `height` tall. Every other one carries a seam.
    fn spires(&mut self, at: Vec2, n: usize, width: f32, height: f32, rng: &mut Rng) {
        for i in 0..n {
            let a = i as f32 / n as f32 * TAU + rng.range(-0.4, 0.4);
            let out = Vec2::new(a.cos(), a.sin());
            let p = at + out * width * rng.range(0.3, 0.7);
            let g = self.ground_height(p.x, p.y);
            let w = width * rng.range(0.3, 0.45);
            let h = height * rng.range(0.6, 1.0);
            self.monolith(
                p,
                a,
                Vec2::new(w * 0.6, w * 0.45),
                g - 0.8,
                g + h,
                rng.range(0.12, 0.3),
                i % 2 == 0,
            );
        }
    }

    /// A ring of glowing segments set into the ground (visual only).
    fn ring_glow(&mut self, at: Vec2, r: f32, g: f32, width: f32) {
        let n = ((TAU * r / 1.6) as usize).max(8);
        for k in 0..n {
            let a = (k as f32 + 0.5) / n as f32 * TAU;
            let p = at + Vec2::new(a.cos(), a.sin()) * r;
            self.oboxes.push(OBox {
                center: p,
                yaw: FRAC_PI_2 - a,
                half: Vec2::new(TAU * r / n as f32 * 0.42, width * 0.5),
                y0: g - 0.3,
                y1: g + 0.04,
                color: PLASMA,
                side_mat: Material::Glow,
                top_mat: Material::Glow,
                solid: false,
                ..OBox::default()
            });
        }
    }

    /// A small kept sun on top of something: a lantern.
    fn lantern(&mut self, at: Vec3, radius: f32) {
        self.cores.push(Core {
            at,
            radius,
            prongs: Vec::new(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::NavGrid;

    #[test]
    fn every_spawn_reaches_the_dais_and_both_lanes() {
        let m = Map::kiln();
        assert!(
            m.open_spots.len() >= 12,
            "{} open spots",
            m.open_spots.len()
        );
        for (side, base) in m.bases.iter().enumerate() {
            assert!(base.len() >= 14, "side {side} has {} spawns", base.len());
        }
        let nav = NavGrid::build(&m);
        let goals = [
            CENTER + Vec2::new(0.0, 8.0),
            Vec2::new(CENTER.x, canyon_z(CENTER.x)),
            Vec2::new(CENTER.x, RIDGE_Z + 4.0),
        ];
        for &p in m.open_spots.iter().chain(m.bases.iter().flatten()) {
            for g in goals {
                assert!(!nav.path(p, g).is_empty(), "no path from {p} to {g}");
            }
        }
    }

    #[test]
    fn the_core_floats_between_the_monoliths() {
        let m = Map::kiln();
        let c = &m.cores[0];
        assert_eq!(c.prongs.len(), 6);
        for p in &c.prongs {
            let flat = Vec2::new(p.x - c.at.x, p.z - c.at.z).length();
            assert!(flat < KILN_R - 4.0, "tip {flat} m out");
            assert!(
                (p.y - c.at.y).abs() < 6.0,
                "tip at {}, core at {}",
                p.y,
                c.at.y
            );
        }
    }

    #[test]
    fn the_play_ground_is_drivable_where_it_matters() {
        // Dais slopes and canyon banks are climbable; only the ridge's back
        // drop is steep.
        let slope = |x: f32, z: f32| {
            let e = 0.5;
            let gx = (ground(x + e, z) - ground(x - e, z)) / (2.0 * e);
            let gz = (ground(x, z + e) - ground(x, z - e)) / (2.0 * e);
            Vec2::new(gx, gz).length()
        };
        for k in 0..64 {
            let a = k as f32 / 64.0 * TAU;
            for r in [DAIS_R + 2.0, (DAIS_R + DAIS_FOOT) * 0.5, DAIS_FOOT - 2.0] {
                let p = CENTER + Vec2::new(a.cos(), a.sin()) * r;
                assert!(slope(p.x, p.y) < 0.5, "dais too steep at {p}");
            }
        }
        for x in (10..210).step_by(10) {
            let x = x as f32;
            for dz in [-10.0, -8.0, 8.0, 10.0] {
                assert!(
                    slope(x, canyon_z(x) + dz) < 0.65,
                    "canyon bank too steep at {x}"
                );
            }
            assert!(
                slope(x, RIDGE_Z + 15.0) < 0.4,
                "ridge face too steep at {x}"
            );
        }
    }

    #[test]
    fn both_halves_have_about_the_same_cover() {
        let m = Map::kiln();
        let area = |west: bool| -> f32 {
            m.oboxes
                .iter()
                .filter(|o| o.solid && (o.center.x < CENTER.x) == west)
                // The field, not the bases (those differ on purpose).
                .filter(|o| o.center.x > 38.0 && o.center.x < SIZE - 38.0)
                .map(|o| o.half.x * o.half.y * 4.0)
                .sum()
        };
        let (w, e) = (area(true), area(false));
        assert!((w - e).abs() / w.max(e) < 0.3, "west {w} vs east {e}");
    }

    #[test]
    fn repair_spots_are_open() {
        let m = Map::kiln();
        let nav = NavGrid::build(&m);
        for &p in &m.repair_spots {
            assert!(!nav.is_blocked(p), "repair spot {p} is blocked");
        }
    }
}

#[cfg(test)]
mod play {
    use super::*;
    use crate::combat::Match;
    use crate::sim::Command;

    /// A full 6v6 round's opening on the Kiln: both sides score, and every
    /// bot gets out of its base and into the fight. Slow (run with
    /// `--ignored --release`).
    #[test]
    #[ignore]
    fn bots_fight_across_the_kiln() {
        let mut m = Match::new(Map::kiln(), 0, 11, 7);
        let start: Vec<Vec2> = m
            .world
            .tanks
            .iter()
            .map(|t| Vec2::new(t.pos.x, t.pos.z))
            .collect();
        let mut furthest = vec![0.0f32; start.len()];
        let mut stuck_time = vec![0.0f32; start.len()];
        let mut last = start.clone();
        let steps = (90.0 / crate::game::DT) as usize;
        for k in 0..steps {
            m.step(Command::default(), false);
            if k % 120 == 0 {
                for (i, t) in m.world.tanks.iter().enumerate().skip(1) {
                    let p = Vec2::new(t.pos.x, t.pos.z);
                    furthest[i] = furthest[i].max(p.distance(start[i]));
                    if m.fighters[i].alive && p.distance(last[i]) < 0.5 {
                        stuck_time[i] += 1.0;
                    }
                    last[i] = p;
                }
            }
        }
        let (a, b) = (m.score(0), m.score(1));
        eprintln!("score {a} : {b}; furthest {furthest:?}; still {stuck_time:?}");
        assert!(a > 0 && b > 0, "score {a} : {b}");
        for i in 1..furthest.len() {
            assert!(furthest[i] > 40.0, "bot {i} only got {} m out", furthest[i]);
        }
    }
}
