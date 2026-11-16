//! The Kiln: Ithessar ground. A plain of pale ash glassed black in long
//! flows, where the Ithessar keep a sun: six crystal monoliths lean in over
//! the middle and hold a ball of plasma in the air between them. The
//! Imperium has dug in at the east edge to take it. 200 m square (+x east,
//! +z north):
//!
//! - center: the kiln, monoliths in a ring round the floating core
//! - around it: spire clusters, cover on the way in
//! - west: the Ithessar gate, two spires leaning together
//! - east: the Imperium foothold, drop pods and barricades

use std::f32::consts::{PI, TAU};

use glam::{Vec2, Vec3};

use super::{Core, Map, OBox, Terrain};
use crate::faction::Faction;
use crate::render::material::Material;
use crate::rng::Rng;
use crate::tank::ithessar::PLASMA;

const SIZE: f32 = 200.0;
const CENTER: Vec2 = Vec2::new(100.0, 100.0);
/// Monolith ring radius, and the core's height above the ground.
const KILN_R: f32 = 13.0;
const CORE_Y: f32 = 12.0;
const ASH: Vec3 = Vec3::new(0.6, 0.59, 0.6);
const GLASS: Vec3 = Vec3::new(0.07, 0.055, 0.08);
const CONCRETE: Vec3 = Vec3::new(0.56, 0.57, 0.6);
const IMPERIAL: Vec3 = Vec3::new(0.78, 0.82, 0.9);

fn ground(x: f32, z: f32) -> f32 {
    let r = Vec2::new(x, z).distance(CENTER);
    // A shallow bowl under the kiln, long low swells elsewhere.
    let bowl = -1.2 * (1.0 - (r / 30.0).powi(2)).max(0.0);
    let swell =
        (x * 0.03 + 0.4).sin() * (z * 0.041).cos() * 0.9 + (x * 0.07 - z * 0.05).sin() * 0.35;
    let calm = ((r - 26.0) / 18.0).clamp(0.0, 1.0);
    let edge = (x.min(SIZE - x).min(z).min(SIZE - z) / 12.0).clamp(0.0, 1.0);
    bowl + swell * calm * edge
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
            roads: Vec::new(),
            open_spots: Vec::new(),
            spawn_yaw: None,
            // An alien dusk: a low rose sun, an indigo sky, violet haze.
            atmosphere: crate::render::Atmosphere {
                sun_dir: Vec3::new(-0.7, 0.4, 0.45),
                sun_color: Vec3::new(2.3, 1.85, 1.7),
                sky_zenith: Vec3::new(0.1, 0.11, 0.27),
                sky_horizon: Vec3::new(0.6, 0.5, 0.6),
                ground_color: Vec3::new(0.3, 0.27, 0.3),
                fog_density: 0.0017,
                ..Default::default()
            },
            bases: Default::default(),
            factions: [Faction::Ithessar, Faction::Imperium],
            repair_spots: vec![
                CENTER + Vec2::new(-40.0, 30.0),
                CENTER + Vec2::new(-40.0, -30.0),
                CENTER + Vec2::new(40.0, 30.0),
                CENTER + Vec2::new(40.0, -30.0),
            ],
            hold_site: Some(CENTER),
            cores: Vec::new(),
        };
        let mut terrain = Terrain::from_fn(Vec2::ZERO, 2.0, 101, 101, ground);
        terrain.color = ASH;
        terrain.material = Material::Sand;
        map.terrain = Some(terrain);
        let mut rng = Rng::new(0x1735_5a12);

        // The kiln: six monoliths leaning in, a seam of plasma up each inner
        // face, holding the core between their tips.
        let mut prongs = Vec::new();
        for i in 0..6 {
            let a = i as f32 / 6.0 * TAU + PI / 6.0;
            let out = Vec2::new(a.cos(), a.sin());
            let at = CENTER + out * KILN_R;
            let g = map.ground_height(at.x, at.y);
            let (h, lean) = (rng.range(16.0, 18.0), -0.5);
            map.monolith(at, a, Vec2::new(1.7, 1.3), g - 1.0, g + h, lean, true);
            // Where its tip ends up: the box tips about its middle.
            let mid = (h + 1.0) * 0.5;
            let tip = Vec3::new(at.x, g - 1.0 + mid, at.y)
                + Vec3::new(out.x, 0.0, out.y) * (mid * lean.sin())
                + Vec3::Y * mid * lean.cos();
            prongs.push(tip);
        }
        map.cores.push(Core {
            at: Vec3::new(
                CENTER.x,
                map.ground_height(CENTER.x, CENTER.y) + CORE_Y,
                CENTER.y,
            ),
            radius: 3.4,
            prongs,
        });

        // Spire clusters on the way in: tight groups leaning off a root.
        for i in 0..6 {
            let a = i as f32 / 6.0 * TAU + rng.range(-0.2, 0.2);
            let p = CENTER + Vec2::new(a.cos(), a.sin()) * rng.range(42.0, 52.0);
            map.spires(p, 3 + i % 2, 3.0, rng.range(6.0, 9.0), &mut rng);
        }
        for _ in 0..10 {
            let a = rng.range(0.0, TAU);
            let p = CENTER + Vec2::new(a.cos(), a.sin()) * rng.range(62.0, 82.0);
            if (p.x - 15.0).abs() < 18.0 || (p.x - 185.0).abs() < 18.0 {
                continue;
            }
            map.spires(p, 2, 2.0, rng.range(3.5, 6.0), &mut rng);
        }

        // West: the Ithessar gate, two tall spires leaning into an arch.
        for s in [-1.0f32, 1.0] {
            let at = Vec2::new(24.0, CENTER.y + s * 7.0);
            let g = map.ground_height(at.x, at.y);
            let a = if s > 0.0 { PI / 2.0 } else { -PI / 2.0 };
            map.monolith(at, a, Vec2::new(1.2, 1.0), g - 1.0, g + 13.0, -0.42, true);
        }

        // East: the Imperium foothold. Drop pods (squat cylinders built as
        // boxes), and lines of concrete barricade facing the kiln.
        for k in 0..3 {
            let at = Vec2::new(
                172.0 + (k % 2) as f32 * 8.0,
                CENTER.y - 30.0 + k as f32 * 30.0,
            );
            let g = map.ground_height(at.x, at.y);
            for (yaw, half) in [
                (0.0f32, Vec2::new(2.6, 2.6)),
                (PI / 4.0, Vec2::new(2.6, 2.6)),
            ] {
                map.oboxes.push(OBox {
                    center: at,
                    yaw,
                    half,
                    y0: g - 0.5,
                    y1: g + 5.5,
                    color: IMPERIAL,
                    side_mat: Material::Painted,
                    top_mat: Material::Painted,
                    ..OBox::default()
                });
            }
        }
        for k in 0..5 {
            let z = 40.0 + k as f32 * 30.0;
            let at = Vec2::new(150.0 + rng.range(-3.0, 3.0), z);
            let g = map.ground_height(at.x, at.y);
            map.oboxes.push(OBox {
                center: at,
                yaw: rng.range(-0.25, 0.25),
                half: Vec2::new(0.8, 4.5),
                y0: g - 0.5,
                y1: g + 1.6,
                color: CONCRETE,
                side_mat: Material::Concrete,
                top_mat: Material::Concrete,
                chip: 0.3,
                ..OBox::default()
            });
        }

        // Bases: the Ithessar behind their gate, the Imperium round its pods.
        for (side, xs) in [(0, [10.0, 17.0]), (1, [SIZE - 10.0, SIZE - 17.0])] {
            for x in xs {
                for k in 0..8 {
                    map.bases[side].push(Vec2::new(x, 58.0 + k as f32 * 12.0));
                }
            }
        }
        for i in 0..10 {
            let a = i as f32 / 10.0 * TAU;
            map.open_spots
                .push(CENTER + Vec2::new(a.cos(), a.sin()) * 70.0);
        }
        for i in 0..6 {
            let a = i as f32 / 6.0 * TAU;
            map.open_spots
                .push(CENTER + Vec2::new(a.cos(), a.sin()) * 30.0);
        }
        let blocked = |p: Vec2, m: &Map| m.oboxes.iter().any(|o| o.solid && o.distance(p) < 4.0);
        let spots = std::mem::take(&mut map.open_spots);
        map.open_spots = spots.into_iter().filter(|&p| !blocked(p, &map)).collect();
        for side in 0..2 {
            let base = std::mem::take(&mut map.bases[side]);
            map.bases[side] = base.into_iter().filter(|&p| !blocked(p, &map)).collect();
        }

        map.perimeter_walls();
        map
    }

    /// A worked-crystal monolith at `at` facing `a` (its local +z points that
    /// way): footprint `half`, from `y0` to `y1`, tipped by `lean` (negative
    /// leans its top back toward -z, i.e. inward when `a` points out). With
    /// `seam`, a band of plasma runs up its face.
    #[allow(clippy::too_many_arguments)]
    fn monolith(&mut self, at: Vec2, a: f32, half: Vec2, y0: f32, y1: f32, lean: f32, seam: bool) {
        let yaw = std::f32::consts::FRAC_PI_2 - a;
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
            self.oboxes.push(OBox {
                center: at,
                yaw,
                half: Vec2::new(half.x + 0.06, 0.16),
                y0: y0 + 2.0,
                y1: y1 - 1.2,
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::NavGrid;

    #[test]
    fn every_spawn_reaches_the_kiln() {
        let m = Map::kiln();
        assert!(
            m.open_spots.len() >= 10,
            "{} open spots",
            m.open_spots.len()
        );
        for (side, base) in m.bases.iter().enumerate() {
            assert!(base.len() >= 12, "side {side} has {} spawns", base.len());
        }
        let nav = NavGrid::build(&m);
        for &p in m.open_spots.iter().chain(m.bases.iter().flatten()) {
            assert!(
                !nav.path(p, CENTER + Vec2::new(0.0, 8.0)).is_empty(),
                "no path from {p} to the kiln"
            );
        }
    }

    #[test]
    fn the_core_floats_between_the_monoliths() {
        let m = Map::kiln();
        let c = &m.cores[0];
        assert_eq!(c.prongs.len(), 6);
        for p in &c.prongs {
            // Tips lean in toward the core and reach up to about its height.
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
    fn repair_spots_are_open() {
        let m = Map::kiln();
        let nav = NavGrid::build(&m);
        for &p in &m.repair_spots {
            assert!(!nav.is_blocked(p), "repair spot {p} is blocked");
        }
    }
}
