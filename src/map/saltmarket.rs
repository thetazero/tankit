//! Saltmarket: a ruined trading town on the shore of a dry reservoir, on a
//! desert world (see docs/setting.md). Attrition only.
//!
//! Blockout layout, 240 m square (+x east, +z north):
//! - Town (west two thirds): a street grid around a central market square,
//!   with the market hall (drive-through: four piers and a roof) in the middle.
//!   Lots are intact blocks, gutted shells you can drive into, low ruined
//!   stubs with sand drifted against them (ramps onto the roofs), or rubble.
//! - Reservoir bed (east third): a bowl ~6 m deep with gentle shores, so you
//!   can drive in and out anywhere. Open, long sight lines. A half-buried war
//!   machine sits in it as cover and landmark.
//! - High ground: a dune ridge along the north edge overlooking town and bowl,
//!   and the dam across the reservoir's south end (ramp up from town, ramp
//!   down into the bowl, a breach through the middle).
//!
//! The three lanes loop into each other: no dead ends.

use glam::{Vec2, Vec3};

use super::{Map, Rise, Terrain, Wedge, mat};
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

/// Streets: (lo, hi) bands along each axis. E-W avenues (z) and N-S streets (x).
const AVENUES: [(f32, f32); 4] = [(36.0, 46.0), (70.0, 79.0), (112.0, 128.0), (158.0, 166.0)];
const STREETS: [(f32, f32); 4] = [(26.0, 34.0), (52.0, 62.0), (94.0, 106.0), (128.0, 136.0)];
/// Town extents.
const TOWN_X: (f32, f32) = (12.0, 146.0);
const TOWN_Z: (f32, f32) = (16.0, 196.0);
/// Market square (open, holds the hall).
const SQUARE_MIN: Vec2 = Vec2::new(62.0, 100.0);
const SQUARE_MAX: Vec2 = Vec2::new(98.0, 140.0);

impl Map {
    pub fn saltmarket() -> Map {
        let mut map = Map {
            size: SIZE,
            solids: Vec::new(),
            boxes: Vec::new(),
            trees: Vec::new(),
            wedges: Vec::new(),
            terrain: None,
            roads: Vec::new(),
            open_spots: Vec::new(),
            spawn_yaw: None,
        };
        let mut terrain = Terrain::from_fn(Vec2::ZERO, 2.0, 121, 121, ground);
        terrain.color = SAND;
        terrain.material = mat::PLAIN;
        map.terrain = Some(terrain);

        let mut rng = Rng::new(0x5a17_3a2c);
        map.town(&mut rng);
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
        self.wedges.push(Wedge { min, max, base_y, height, rise, color });
    }

    /// Fill each town block (between streets, outside the square) with lots.
    fn town(&mut self, rng: &mut Rng) {
        // Block edges along each axis: town edge, street bands, town edge.
        let edges = |bands: &[(f32, f32)], (lo, hi): (f32, f32)| {
            let mut spans = Vec::new();
            let mut start = lo;
            for &(a, b) in bands {
                spans.push((start, a));
                start = b;
            }
            spans.push((start, hi));
            spans
        };
        let xs = edges(&STREETS, TOWN_X);
        let zs = edges(&AVENUES, TOWN_Z);
        for &(x0, x1) in &xs {
            for &(z0, z1) in &zs {
                let (bmin, bmax) = (Vec2::new(x0, z0), Vec2::new(x1, z1));
                // The square is open ground.
                let overlap = bmin.cmplt(SQUARE_MAX).all() && bmax.cmpgt(SQUARE_MIN).all();
                if overlap || (x1 - x0) < 6.0 || (z1 - z0) < 6.0 {
                    continue;
                }
                self.lots(rng, bmin + Vec2::splat(1.0), bmax - Vec2::splat(1.0));
            }
        }
    }

    /// Split a block into 1-3 lots with alleys and build a ruin on each.
    fn lots(&mut self, rng: &mut Rng, min: Vec2, max: Vec2) {
        let size = max - min;
        let along_x = size.x >= size.y;
        let long = if along_x { size.x } else { size.y };
        let n = if long > 26.0 { 3 } else if long > 14.0 { 2 } else { 1 };
        let alley = 4.5;
        let len = (long - alley * (n - 1) as f32) / n as f32;
        for i in 0..n {
            let a = i as f32 * (len + alley);
            let (lmin, lmax) = if along_x {
                (Vec2::new(min.x + a, min.y), Vec2::new(min.x + a + len, max.y))
            } else {
                (Vec2::new(min.x, min.y + a), Vec2::new(max.x, min.y + a + len))
            };
            self.ruin(rng, lmin, lmax);
        }
    }

    /// One building lot: intact, gutted shell, low stub with a sand drift, or rubble.
    fn ruin(&mut self, rng: &mut Rng, min: Vec2, max: Vec2) {
        let roll = rng.f32();
        let lo = |y: f32| Vec3::new(min.x, y, min.y);
        let hi = |y: f32| Vec3::new(max.x, y, max.y);
        if roll < 0.22 {
            // Still standing.
            let h = rng.range(8.0, 14.0);
            self.block_c(lo(0.0), hi(h), CONCRETE * rng.range(0.92, 1.05));
        } else if roll < 0.55 {
            // Gutted shell: four walls with breaches big enough to drive through.
            let (h, t) = (rng.range(4.5, 8.0), 1.0);
            let breach = rng.index(4);
            let c = RUIN * rng.range(0.92, 1.05);
            let walls = [
                (Vec3::new(min.x, 0.0, min.y), Vec3::new(max.x, h, min.y + t)),
                (Vec3::new(min.x, 0.0, max.y - t), Vec3::new(max.x, h, max.y)),
                (Vec3::new(min.x, 0.0, min.y), Vec3::new(min.x + t, h, max.y)),
                (Vec3::new(max.x - t, 0.0, min.y), Vec3::new(max.x, h, max.y)),
            ];
            for (k, (a, b)) in walls.into_iter().enumerate() {
                if k == breach || (k == (breach + 2) % 4 && rng.chance(0.5)) {
                    // Breached wall: two stubs with a 6 m gap in the middle.
                    let mid = (a + b) * 0.5;
                    let gap = 3.2;
                    if (b.x - a.x) > (b.z - a.z) {
                        self.block_c(a, Vec3::new(mid.x - gap, b.y * 0.6, b.z), c);
                        self.block_c(Vec3::new(mid.x + gap, a.y, a.z), Vec3::new(b.x, b.y * 0.8, b.z), c);
                    } else {
                        self.block_c(a, Vec3::new(b.x, b.y * 0.6, mid.z - gap), c);
                        self.block_c(Vec3::new(a.x, a.y, mid.z + gap), Vec3::new(b.x, b.y * 0.8, b.z), c);
                    }
                } else {
                    self.block_c(a, b, c);
                }
            }
        } else if roll < 0.8 {
            // Low stub with sand drifted up one side: a ramp onto the roof.
            let h = 2.2;
            self.block_c(lo(0.0), hi(h), RUIN * rng.range(0.9, 1.0));
            let w = max - min;
            let run = 7.0;
            let (rmin, rmax, rise) = match rng.index(4) {
                0 => (Vec2::new(min.x - run, min.y + w.y * 0.2), Vec2::new(min.x, max.y - w.y * 0.2), Rise::PosX),
                1 => (Vec2::new(max.x, min.y + w.y * 0.2), Vec2::new(max.x + run, max.y - w.y * 0.2), Rise::NegX),
                2 => (Vec2::new(min.x + w.x * 0.2, min.y - run), Vec2::new(max.x - w.x * 0.2, min.y), Rise::PosZ),
                _ => (Vec2::new(min.x + w.x * 0.2, max.y), Vec2::new(max.x - w.x * 0.2, max.y + run), Rise::NegZ),
            };
            self.ramp(rmin, rmax, 0.0, h, rise, SAND);
        } else {
            // Collapsed: a rubble pile (two ramps meeting at a crest) and a wall stub.
            let mid = (min + max) * 0.5;
            let h = rng.range(1.6, 2.6);
            self.ramp(Vec2::new(min.x, min.y), Vec2::new(mid.x, max.y), 0.0, h, Rise::PosX, RUBBLE);
            self.ramp(Vec2::new(mid.x, min.y), Vec2::new(max.x, max.y), 0.0, h, Rise::NegX, RUBBLE);
            self.block_c(Vec3::new(min.x, 0.0, max.y - 1.0), Vec3::new(min.x + (max.x - min.x) * 0.4, rng.range(3.0, 6.0), max.y), RUIN);
        }
    }

    /// The market hall: a big drive-through landmark in the square. Four piers
    /// carry a roof slab high enough to drive under.
    fn market_hall(&mut self) {
        let c = (SQUARE_MIN + SQUARE_MAX) * 0.5;
        let (half, pier, roof_lo, roof_hi) = (12.0, 5.0, 7.5, 9.5);
        for (sx, sz) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
            let p = c + Vec2::new(sx, sz) * (half - pier * 0.5);
            self.block_c(Vec3::new(p.x - pier * 0.5, 0.0, p.y - pier * 0.5), Vec3::new(p.x + pier * 0.5, roof_lo, p.y + pier * 0.5), CONCRETE);
        }
        self.block_c(Vec3::new(c.x - half, roof_lo, c.y - half), Vec3::new(c.x + half, roof_hi, c.y + half), CONCRETE * 1.04);
        // A smaller drum on the roof stands in for the dome.
        self.block_c(Vec3::new(c.x - 6.0, roof_hi, c.y - 6.0), Vec3::new(c.x + 6.0, roof_hi + 3.5, c.y + 6.0), CONCRETE * 0.97);
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
        self.ramp(Vec2::new(gap0 + 1.0, z0 - 6.0), Vec2::new(gap1 - 1.0, z0), 0.0, 1.0, Rise::PosZ, RUBBLE);
        // Up from the town's south edge onto the west half of the dam.
        self.ramp(Vec2::new(x0 - 14.0, z0), Vec2::new(x0, z1), 0.0, h, Rise::PosX, DAM * 0.95);
        // Down from the east half into the bowl.
        self.ramp(Vec2::new(205.0, z1), Vec2::new(217.0, z1 + 16.0), -2.5, h + 2.5, Rise::NegZ, DAM * 0.95);
    }

    fn landmarks(&mut self) {
        // Toppled water tower: the base stump, and the tank lying in the street
        // with its shaft as a ramp up onto it.
        self.block_c(Vec3::new(128.0, 0.0, 48.0), Vec3::new(134.0, 3.0, 54.0), RUST);
        self.ramp(Vec2::new(128.5, 54.0), Vec2::new(133.5, 66.0), 0.0, 4.5, Rise::PosZ, RUST * 0.9);
        self.block_c(Vec3::new(126.0, 0.0, 66.0), Vec3::new(136.0, 4.5, 73.0), RUST * 1.1);

        // A bus with its tires gone, across a street.
        self.block_c(Vec3::new(57.5, 0.0, 88.0), Vec3::new(60.5, 3.2, 99.0), Vec3::new(0.62, 0.52, 0.34));

        // The half-buried war machine in the reservoir bed: hull sunk into the
        // floor, turret on top, the gun barrel reaching west toward the town.
        let floor = -BOWL_DEPTH;
        let (cx, cz) = (200.0, 150.0);
        self.block_c(Vec3::new(cx - 8.0, floor - 2.0, cz - 5.0), Vec3::new(cx + 8.0, floor + 4.5, cz + 5.0), WRECK);
        self.block_c(Vec3::new(cx - 3.5, floor + 4.5, cz - 3.5), Vec3::new(cx + 3.5, floor + 7.5, cz + 3.5), WRECK * 1.05);
        self.block_c(Vec3::new(cx - 18.0, floor + 5.2, cz - 0.7), Vec3::new(cx - 3.5, floor + 6.6, cz + 0.7), WRECK * 0.9);
        // A sand drift banked against its west flank: the way up.
        self.ramp(Vec2::new(cx - 15.0, cz - 4.0), Vec2::new(cx - 8.0, cz + 4.0), floor, 4.5, Rise::PosX, SAND);
    }

    /// Cover in the reservoir bed, so the open lane has things to fight around:
    /// a pier out from the town shore, a half-buried pipeline, two stranded
    /// boats and rock outcrops.
    fn reservoir_cover(&mut self, rng: &mut Rng) {
        let at = |x: f32, z: f32| ground(x, z);
        // Pier: a deck at town level running out over the shore, on piles.
        let (pz0, pz1) = (136.0, 142.0);
        self.block_c(Vec3::new(146.0, -1.0, pz0), Vec3::new(176.0, 0.4, pz1), RUST * 0.8);
        for x in [154.0f32, 162.0, 170.0] {
            let g = at(x, pz0);
            self.block_c(Vec3::new(x - 0.6, g - 0.5, pz0 + 0.5), Vec3::new(x + 0.6, -1.0, pz0 + 1.7), RUST * 0.7);
            self.block_c(Vec3::new(x - 0.6, g - 0.5, pz1 - 1.7), Vec3::new(x + 0.6, -1.0, pz1 - 0.5), RUST * 0.7);
        }
        // Pipeline: segments half sunk in the bed, with gaps to drive through.
        for i in 0..6 {
            let z0 = 55.0 + i as f32 * 11.0;
            let g = at(172.0, z0 + 4.0);
            self.block_c(Vec3::new(170.5, g - 0.8, z0), Vec3::new(173.5, g + 1.4, z0 + 8.0), RUST);
        }
        // Stranded boats: hull block with a bow ramp, lying in the mud.
        for (x, z, along_x) in [(213.0f32, 70.0f32, false), (180.0, 188.0, true)] {
            let g = at(x, z);
            let (hx, hz) = if along_x { (6.0, 2.2) } else { (2.2, 6.0) };
            self.block_c(Vec3::new(x - hx, g - 0.5, z - hz), Vec3::new(x + hx, g + 2.4, z + hz), Vec3::new(0.44, 0.4, 0.36));
            if along_x {
                self.ramp(Vec2::new(x + hx, z - hz), Vec2::new(x + hx + 4.0, z + hz), g - 0.5, 2.9, Rise::NegX, SAND);
            } else {
                self.ramp(Vec2::new(x - hz, z - hx - 4.0), Vec2::new(x + hz, z - hx), g - 0.5, 2.9, Rise::PosZ, SAND);
            }
        }
        // Rock outcrops.
        for (x, z) in [(196.0f32, 92.0f32), (184.0, 120.0), (208.0, 126.0), (190.0, 60.0), (205.0, 180.0), (176.0, 160.0)] {
            let g = at(x, z);
            let (w, d, h) = (rng.range(3.0, 6.0), rng.range(3.0, 6.0), rng.range(2.5, 4.5));
            self.block_c(Vec3::new(x - w, g - 1.0, z - d), Vec3::new(x + w, g + h, z + d), Vec3::new(0.62, 0.52, 0.4));
        }
    }

    /// The dune ridge along the north edge: a bunker, a radio mast stump and rocks.
    fn ridge(&mut self) {
        let at = |x: f32, z: f32| ground(x, z);
        let g = at(150.0, 216.0);
        self.block_c(Vec3::new(142.0, g - 1.0, 212.0), Vec3::new(156.0, g + 2.8, 220.0), DAM);
        let g = at(96.0, 222.0);
        self.block_c(Vec3::new(95.0, g - 0.5, 221.0), Vec3::new(97.0, g + 11.0, 223.0), RUST);
        for (x, z, w, h) in [(30.0f32, 220.0f32, 4.0f32, 3.0f32), (70.0, 214.0, 3.0, 2.5), (126.0, 224.0, 5.0, 3.5), (190.0, 220.0, 4.0, 3.0), (222.0, 214.0, 3.0, 2.5)] {
            let g = at(x, z);
            self.block_c(Vec3::new(x - w, g - 1.0, z - w * 0.8), Vec3::new(x + w, g + h, z + w * 0.8), Vec3::new(0.62, 0.52, 0.4));
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
        for &(sx0, sx1) in &STREETS {
            for &(az0, az1) in &AVENUES {
                spots.push(Vec2::new((sx0 + sx1) * 0.5, (az0 + az1) * 0.5));
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
            }) && m.wedges.iter().all(|w| {
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
        assert!(m.open_spots.len() >= 12, "only {} spawn spots", m.open_spots.len());
        // At least one drop zone in each region: town west, ridge, reservoir/dam side.
        assert!(m.open_spots.iter().any(|p| p.x < 30.0));
        assert!(m.open_spots.iter().any(|p| p.y > 200.0));
        assert!(m.open_spots.iter().any(|p| p.x > 150.0));
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
                let s = ((ground(x + 1.0, z) - ground(x, z)).powi(2) + (ground(x, z + 1.0) - ground(x, z)).powi(2)).sqrt();
                worst = worst.max(s.atan().to_degrees());
            }
        }
        assert!(worst < 22.0, "slope of {worst} degrees");
    }
}
