//! Shard Flats: placeholder map for the Hold contract. Open desert flats
//! where a crystal vein has broken the surface in a shallow crater; the hold
//! site is the deposit. 200 m square (+x east, +z north):
//!
//! - center: the crater, a raised rim around a sunken floor, a cluster of
//!   crystal shards jutting from the middle (the deposit, the hold ring)
//! - inner ring: rock outcrops and smaller shard clusters, cover on the way in
//! - diagonals: four wrecked extractor pads (concrete slabs with ramps)
//! - west and east edges: the two sides' bases

use crate::render::material::Material;
use std::f32::consts::TAU;

use glam::{Vec2, Vec3};

use super::{DEFAULT_SIDES, Map, OBox, Rise, Terrain, Wedge};
use crate::rng::Rng;

const SIZE: f32 = 200.0;
const CENTER: Vec2 = Vec2::new(100.0, 100.0);
/// Crater: sunken floor inside `FLOOR_R`, rim crest at `RIM_R`.
const FLOOR_R: f32 = 17.0;
const RIM_R: f32 = 24.0;
const SAND: Vec3 = Vec3::new(0.78, 0.68, 0.52);
const ROCK: Vec3 = Vec3::new(0.42, 0.37, 0.33);
const CONCRETE: Vec3 = Vec3::new(0.60, 0.58, 0.54);
const CRYSTAL: Vec3 = Vec3::new(0.55, 0.82, 0.88);

fn ground(x: f32, z: f32) -> f32 {
    let r = Vec2::new(x, z).distance(CENTER);
    let floor = -2.0 * (1.0 - (r / FLOOR_R).powi(2)).max(0.0);
    let rim = 2.2 * (-((r - RIM_R) / 5.0).powi(2)).exp();
    let dunes = (x * 0.045).sin() * (z * 0.06).cos() * 1.2 + (x * 0.09 + z * 0.05).sin() * 0.5;
    // Dunes flatten near the crater and the walls.
    let calm = ((r - RIM_R - 6.0) / 20.0).clamp(0.0, 1.0);
    let edge = (x.min(SIZE - x).min(z).min(SIZE - z) / 12.0).clamp(0.0, 1.0);
    floor + rim + dunes.max(-0.8) * calm * edge
}

impl Map {
    pub fn shard_flats() -> Map {
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
            // Midday on the same desert world: high hard sun, pale bleached
            // sky, a thin haze off the flats.
            atmosphere: crate::render::Atmosphere {
                sun_dir: Vec3::new(0.45, 0.7, 0.35),
                sun_color: Vec3::new(2.2, 1.95, 1.6),
                sky_zenith: Vec3::new(0.34, 0.5, 0.7),
                sky_horizon: Vec3::new(0.8, 0.76, 0.66),
                ground_color: Vec3::new(0.42, 0.35, 0.26),
                fog_density: 0.0024,
                ..crate::render::Atmosphere::default()
            },
            bases: Default::default(),
            factions: DEFAULT_SIDES,
            // Just outside the crater rim: a quick patch-up, in the open.
            repair_spots: vec![
                CENTER + Vec2::new(0.0, 33.0),
                CENTER + Vec2::new(28.6, -16.5),
                CENTER + Vec2::new(-28.6, -16.5),
            ],
            hold_site: Some(CENTER),
            cores: Vec::new(),
            crystals: Vec::new(),
            glow_paths: Vec::new(),
        };
        let mut terrain = Terrain::from_fn(Vec2::ZERO, 2.0, 101, 101, ground);
        terrain.color = SAND;
        terrain.material = Material::Plain;
        map.terrain = Some(terrain);
        let mut rng = Rng::new(0x5a2d_f1a7);

        // The deposit: a tight cluster of shards in the middle of the ring,
        // leaning out from a common root. Cover to fight around, room to drive.
        map.shards(CENTER, 7, 2.6, 5.5, &mut rng);

        // Inner ring: outcrops, some with crystals of their own.
        for i in 0..8 {
            let a = i as f32 / 8.0 * TAU + 0.3;
            let r = rng.range(40.0, 52.0);
            let p = CENTER + Vec2::new(a.cos(), a.sin()) * r;
            map.rock(p, rng.range(2.5, 4.5), rng.range(1.8, 3.2), &mut rng);
            if i % 2 == 0 {
                map.shards(p + Vec2::new(3.5, 2.0), 3, 1.0, 2.8, &mut rng);
            }
        }
        // Loose rocks further out.
        for _ in 0..14 {
            let a = rng.range(0.0, TAU);
            let r = rng.range(60.0, 80.0);
            let p = CENTER + Vec2::new(a.cos(), a.sin()) * r;
            map.rock(p, rng.range(1.5, 3.0), rng.range(1.0, 2.2), &mut rng);
        }

        // Wrecked extractor pads on the diagonals: a raised slab with a ramp up
        // facing the crater, and a broken mast lying across it.
        for k in 0..4 {
            let a = k as f32 / 4.0 * TAU + TAU / 8.0;
            let d = Vec2::new(a.cos(), a.sin());
            let c = CENTER + d * 62.0;
            let (half, h) = (7.0, 1.6);
            let g = ground(c.x, c.y);
            map.add_block(
                Vec3::new(c.x - half, g - 1.0, c.y - half),
                Vec3::new(c.x + half, g + h, c.y + half),
                CONCRETE,
            );
            // Ramp on the side facing the crater.
            let (min, max, rise) = if d.x < 0.0 {
                (
                    Vec2::new(c.x + half, c.y - 3.0),
                    Vec2::new(c.x + half + 8.0, c.y + 3.0),
                    Rise::NegX,
                )
            } else {
                (
                    Vec2::new(c.x - half - 8.0, c.y - 3.0),
                    Vec2::new(c.x - half, c.y + 3.0),
                    Rise::PosX,
                )
            };
            map.wedges.push(Wedge {
                min,
                max,
                base_y: g - 0.3,
                height: h + 0.3,
                rise,
                color: CONCRETE * 0.92,
                material: None,
            });
            map.oboxes.push(OBox {
                center: c + Vec2::new(0.0, 2.0),
                yaw: a + 0.6,
                half: Vec2::new(0.6, 6.5),
                y0: g + h,
                y1: g + h + 1.2,
                color: Vec3::new(0.45, 0.30, 0.22),
                ..OBox::default()
            });
        }

        // Bases: two columns of drop zones at the west and east walls. The
        // deposit sits halfway between, and both get the same run to it.
        for (side, xs) in [(0, [10.0, 18.0]), (1, [SIZE - 10.0, SIZE - 18.0])] {
            for x in xs {
                for k in 0..8 {
                    map.bases[side].push(Vec2::new(x, 58.0 + k as f32 * 12.0));
                }
            }
        }
        // Roam points around the edge and just outside the rim.
        for i in 0..10 {
            let a = i as f32 / 10.0 * TAU;
            map.open_spots
                .push(CENTER + Vec2::new(a.cos(), a.sin()) * 84.0);
        }
        for i in 0..6 {
            let a = i as f32 / 6.0 * TAU + 0.5;
            map.open_spots
                .push(CENTER + Vec2::new(a.cos(), a.sin()) * 32.0);
        }
        let blocked = |p: Vec2, m: &Map| {
            m.solids.iter().any(|s| {
                let c = p.clamp(s.min2(), s.max2());
                c.distance(p) < 4.0
            }) || m.oboxes.iter().any(|o| o.distance(p) < 4.0)
        };
        let spots = std::mem::take(&mut map.open_spots);
        map.open_spots = spots.into_iter().filter(|&p| !blocked(p, &map)).collect();
        for side in 0..2 {
            let base = std::mem::take(&mut map.bases[side]);
            map.bases[side] = base.into_iter().filter(|&p| !blocked(p, &map)).collect();
        }

        map.perimeter_walls();
        map
    }

    /// `n` crystal shards rooted around `at`, leaning outward: `width` thick,
    /// up to `height` tall.
    fn shards(&mut self, at: Vec2, n: usize, width: f32, height: f32, rng: &mut Rng) {
        for i in 0..n {
            let a = i as f32 / n as f32 * TAU + rng.range(-0.3, 0.3);
            let out = Vec2::new(a.cos(), a.sin());
            let p = at + out * width * rng.range(0.3, 0.8);
            let h = height * rng.range(0.55, 1.0);
            let g = self.ground_height(p.x, p.y);
            let w = width * rng.range(0.35, 0.55);
            self.oboxes.push(OBox {
                center: p,
                // Local +z points outward, and a negative tilt raises that end:
                // the shard leans away from the root.
                yaw: std::f32::consts::FRAC_PI_2 - a,
                half: Vec2::new(w * 0.5, w * 0.5),
                y0: g - 0.8,
                y1: g + h,
                color: CRYSTAL * rng.range(0.85, 1.1),
                tilt: rng.range(0.15, 0.4),
                ..OBox::default()
            });
        }
    }

    /// A squat rock outcrop: two overlapping boulders.
    fn rock(&mut self, at: Vec2, size: f32, height: f32, rng: &mut Rng) {
        for k in 0..2 {
            let off = Vec2::new(rng.range(-1.0, 1.0), rng.range(-1.0, 1.0)) * size * 0.4 * k as f32;
            let p = at + off;
            let g = self.ground_height(p.x, p.y);
            let s = size * if k == 0 { 1.0 } else { 0.7 };
            self.oboxes.push(OBox {
                center: p,
                yaw: rng.range(0.0, TAU),
                half: Vec2::new(s * 0.5, s * 0.4),
                y0: g - 0.6,
                y1: g + height * if k == 0 { 1.0 } else { 0.6 },
                color: ROCK * rng.range(0.85, 1.1),
                ..OBox::default()
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::NavGrid;

    #[test]
    fn every_spawn_reaches_the_deposit() {
        let m = Map::shard_flats();
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
                !nav.path(p, CENTER).is_empty(),
                "no path from {p} to the deposit"
            );
        }
    }

    #[test]
    fn repair_spots_are_open() {
        let m = Map::shard_flats();
        let nav = NavGrid::build(&m);
        for &p in &m.repair_spots {
            assert!(!nav.is_blocked(p), "repair spot {p} is blocked");
        }
    }
}
