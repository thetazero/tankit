//! Arena geometry. `Map::generate` builds the procedural city (BSP-split
//! blocks separated by roads, filled with buildings, plazas with cover, and
//! small parks); `Map::proving_ground` is a hand-built physics test course.
//! Shapes are boxes, wedges (ramps), and an optional heightfield ground.

use glam::{Vec2, Vec3};

use crate::rng::Rng;

mod saltmarket;

/// Size of one city-grid cell in meters.
pub const CELL: f32 = 4.0;
/// City grid is GRID x GRID cells.
pub const GRID: i32 = 42;

/// Material ids, read by the scene shader from vertex color alpha.
pub mod mat {
    pub const PLAIN: f32 = 0.0;
    pub const WALL: f32 = 1.0;
    pub const ASPHALT: f32 = 2.0;
    pub const ROOF: f32 = 3.0;
    pub const GRASS: f32 = 4.0;
    pub const PAVING: f32 = 5.0;
}

/// Blocking collider.
#[derive(Clone, Copy, Debug)]
pub struct Solid {
    pub min: Vec3,
    pub max: Vec3,
}

impl Solid {
    pub fn min2(&self) -> Vec2 {
        Vec2::new(self.min.x, self.min.z)
    }
    pub fn max2(&self) -> Vec2 {
        Vec2::new(self.max.x, self.max.z)
    }
}

/// Visual-only box (colliders are listed separately in `solids`).
#[derive(Clone, Copy, Debug)]
pub struct VisBox {
    pub min: Vec3,
    pub max: Vec3,
    pub color: Vec3,
    pub side_mat: f32,
    pub top_mat: f32,
}

/// A solid box turned about the vertical axis (buildings and walls that
/// don't line up with the world grid). Blocking and visible.
#[derive(Clone, Copy, Debug)]
pub struct OBox {
    /// Footprint center on the ground plane (x, z).
    pub center: Vec2,
    /// Rotation about +y, radians (local +x turns toward +z... standard glam rotation_y).
    pub yaw: f32,
    /// Footprint half extents along the box's local x and z.
    pub half: Vec2,
    pub y0: f32,
    pub y1: f32,
    pub color: Vec3,
    pub side_mat: f32,
    pub top_mat: f32,
}

impl OBox {
    /// Local axes on the ground plane: (right, forward), as world xz directions.
    pub fn axes(&self) -> (Vec2, Vec2) {
        // glam's rotation_y maps local +x to (cos, -sin) and +z to (sin, cos) in xz.
        let (s, c) = self.yaw.sin_cos();
        (Vec2::new(c, -s), Vec2::new(s, c))
    }

    /// World point (x, z) in the box's local footprint frame.
    pub fn to_local(&self, p: Vec2) -> Vec2 {
        let (ax, az) = self.axes();
        let d = p - self.center;
        Vec2::new(d.dot(ax), d.dot(az))
    }

    /// Distance from a ground point to the footprint (0 inside).
    pub fn distance(&self, p: Vec2) -> f32 {
        let q = self.to_local(p).abs() - self.half;
        q.max(Vec2::ZERO).length()
    }

    /// Footprint corners in world xz, counter-clockwise.
    pub fn corners(&self) -> [Vec2; 4] {
        let (ax, az) = self.axes();
        let (x, z) = (ax * self.half.x, az * self.half.y);
        [self.center - x - z, self.center + x - z, self.center + x + z, self.center - x + z]
    }

    /// Footprints overlap (separating axis test), with `margin` of required gap.
    pub fn overlaps(&self, o: &OBox, margin: f32) -> bool {
        let (a0, a1) = self.axes();
        let (b0, b1) = o.axes();
        for axis in [a0, a1, b0, b1] {
            let proj = |b: &OBox| {
                let cs = b.corners().map(|c| c.dot(axis));
                (cs.iter().copied().fold(f32::INFINITY, f32::min), cs.iter().copied().fold(f32::NEG_INFINITY, f32::max))
            };
            let (lo0, hi0) = proj(self);
            let (lo1, hi1) = proj(o);
            if hi0 + margin < lo1 || hi1 + margin < lo0 {
                return false;
            }
        }
        true
    }
}

/// Which way a wedge's slope climbs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Rise {
    PosX,
    NegX,
    PosZ,
    NegZ,
}

/// A ramp: rectangular footprint, flat bottom at `base_y`, sloped top that
/// climbs from 0 to `height` in the `rise` direction. Solid and drivable.
#[derive(Clone, Copy, Debug)]
pub struct Wedge {
    pub min: Vec2,
    pub max: Vec2,
    pub base_y: f32,
    pub height: f32,
    pub rise: Rise,
    pub color: Vec3,
}

impl Wedge {
    /// Corners: [low0, low1, high_base0, high_base1, high_top0, high_top1].
    pub fn points(&self) -> [Vec3; 6] {
        let (a, b, y0, y1) = (self.min, self.max, self.base_y, self.base_y + self.height);
        let p = |x: f32, z: f32, y: f32| Vec3::new(x, y, z);
        match self.rise {
            Rise::PosX => [p(a.x, a.y, y0), p(a.x, b.y, y0), p(b.x, a.y, y0), p(b.x, b.y, y0), p(b.x, a.y, y1), p(b.x, b.y, y1)],
            Rise::NegX => [p(b.x, a.y, y0), p(b.x, b.y, y0), p(a.x, a.y, y0), p(a.x, b.y, y0), p(a.x, a.y, y1), p(a.x, b.y, y1)],
            Rise::PosZ => [p(a.x, a.y, y0), p(b.x, a.y, y0), p(a.x, b.y, y0), p(b.x, b.y, y0), p(a.x, b.y, y1), p(b.x, b.y, y1)],
            Rise::NegZ => [p(a.x, b.y, y0), p(b.x, b.y, y0), p(a.x, a.y, y0), p(b.x, a.y, y0), p(a.x, a.y, y1), p(b.x, a.y, y1)],
        }
    }

    /// Faces as point-index polygons (convex), for rendering.
    pub const FACES: [&'static [usize]; 5] = [&[0, 1, 5, 4], &[2, 3, 5, 4], &[0, 1, 3, 2], &[0, 2, 4], &[1, 3, 5]];
}

/// Heightfield ground. Heights are stored row-major, z rows then x columns.
#[derive(Clone, Debug)]
pub struct Terrain {
    pub origin: Vec2,
    pub cell: f32,
    pub nx: usize,
    pub nz: usize,
    pub heights: Vec<f32>,
    pub color: Vec3,
    pub material: f32,
}

impl Terrain {
    pub fn from_fn(origin: Vec2, cell: f32, nx: usize, nz: usize, f: impl Fn(f32, f32) -> f32) -> Terrain {
        let mut heights = Vec::with_capacity(nx * nz);
        for iz in 0..nz {
            for ix in 0..nx {
                heights.push(f(origin.x + ix as f32 * cell, origin.y + iz as f32 * cell));
            }
        }
        Terrain { origin, cell, nx, nz, heights, color: Vec3::new(0.36, 0.5, 0.28), material: mat::GRASS }
    }

    pub fn size(&self) -> Vec2 {
        Vec2::new((self.nx - 1) as f32, (self.nz - 1) as f32) * self.cell
    }

    pub fn at(&self, ix: usize, iz: usize) -> f32 {
        self.heights[iz * self.nx + ix]
    }

    /// Bilinear height (close to, not exactly, the collider's triangles).
    pub fn height_at(&self, x: f32, z: f32) -> f32 {
        let g = (Vec2::new(x, z) - self.origin) / self.cell;
        let gx = g.x.clamp(0.0, (self.nx - 1) as f32 - 0.001);
        let gz = g.y.clamp(0.0, (self.nz - 1) as f32 - 0.001);
        let (ix, iz) = (gx as usize, gz as usize);
        let (fx, fz) = (gx - ix as f32, gz - iz as f32);
        let h0 = self.at(ix, iz) * (1.0 - fx) + self.at(ix + 1, iz) * fx;
        let h1 = self.at(ix, iz + 1) * (1.0 - fx) + self.at(ix + 1, iz + 1) * fx;
        h0 * (1.0 - fz) + h1 * fz
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Tree {
    pub pos: Vec3,
    pub radius: f32,
}

/// Axis-aligned rect in grid cells, half-open.
#[derive(Clone, Copy, Debug)]
pub struct CellRect {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

impl CellRect {
    fn w(&self) -> i32 {
        self.x1 - self.x0
    }
    fn h(&self) -> i32 {
        self.y1 - self.y0
    }
    fn world_min(&self) -> Vec2 {
        Vec2::new(self.x0 as f32, self.y0 as f32) * CELL
    }
    fn world_max(&self) -> Vec2 {
        Vec2::new(self.x1 as f32, self.y1 as f32) * CELL
    }
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x0 && x < self.x1 && y >= self.y0 && y < self.y1
    }
}

#[derive(Clone)]
pub struct Map {
    /// Arena spans [0, size] on x and z.
    pub size: f32,
    pub solids: Vec<Solid>,
    pub boxes: Vec<VisBox>,
    pub trees: Vec<Tree>,
    pub wedges: Vec<Wedge>,
    /// Rotated solid boxes (blocking and visible).
    pub oboxes: Vec<OBox>,
    pub terrain: Option<Terrain>,
    pub roads: Vec<CellRect>,
    /// Open road positions usable for spawns and pickups.
    pub open_spots: Vec<Vec2>,
    /// Fixed spawn heading; None means face the map center.
    pub spawn_yaw: Option<f32>,
}

const MIN_BLOCK: i32 = 4;
const RING: i32 = 3;
const SIDEWALK: f32 = 1.2;

const BUILDING_COLORS: [[f32; 3]; 8] = [
    [0.80, 0.74, 0.62], // sand
    [0.70, 0.71, 0.72], // concrete
    [0.74, 0.47, 0.36], // terracotta
    [0.46, 0.54, 0.62], // slate
    [0.86, 0.83, 0.74], // cream
    [0.58, 0.62, 0.52], // sage
    [0.62, 0.50, 0.44], // brick
    [0.55, 0.58, 0.66], // steel blue
];

impl Map {
    pub fn generate(seed: u64) -> Map {
        let mut rng = Rng::new(seed);
        let size = GRID as f32 * CELL;
        let mut map = Map {
            size,
            solids: Vec::new(),
            boxes: Vec::new(),
            trees: Vec::new(),
            wedges: Vec::new(),
            oboxes: Vec::new(),
            terrain: None,
            roads: Vec::new(),
            open_spots: Vec::new(),
            spawn_yaw: None,
        };

        // Ring road around the inner city.
        let inner = CellRect { x0: RING, y0: RING, x1: GRID - RING, y1: GRID - RING };
        map.roads.push(CellRect { x0: 0, y0: 0, x1: GRID, y1: RING });
        map.roads.push(CellRect { x0: 0, y0: GRID - RING, x1: GRID, y1: GRID });
        map.roads.push(CellRect { x0: 0, y0: 0, x1: RING, y1: GRID });
        map.roads.push(CellRect { x0: GRID - RING, y0: 0, x1: GRID, y1: GRID });

        let mut blocks = Vec::new();
        bsp(inner, &mut rng, &mut blocks, &mut map.roads, 0);

        for b in blocks {
            let roll = rng.f32();
            if roll < 0.68 {
                map.fill_buildings(b, &mut rng);
            } else if roll < 0.88 {
                map.fill_plaza(b, &mut rng);
            } else {
                map.fill_park(b, &mut rng);
            }
        }

        map.perimeter_walls();
        map.backdrop(&mut rng);
        map.collect_open_spots();
        map
    }

    fn fill_buildings(&mut self, b: CellRect, rng: &mut Rng) {
        // Sidewalk slab under the whole block.
        let (bmin, bmax) = (b.world_min(), b.world_max());
        self.boxes.push(VisBox {
            min: Vec3::new(bmin.x, 0.0, bmin.y),
            max: Vec3::new(bmax.x, 0.12, bmax.y),
            color: Vec3::new(0.55, 0.55, 0.54),
            side_mat: mat::PLAIN,
            top_mat: mat::PAVING,
        });

        // Split the block into lots along its long axis, sometimes with an alley.
        let mut lots = Vec::new();
        split_lots(b, rng, &mut lots, 0);

        for lot in lots {
            let (lmin, lmax) = (lot.world_min() + SIDEWALK, lot.world_max() - SIDEWALK);
            if (lmax - lmin).min_element() < 3.0 {
                continue;
            }
            let f = rng.f32();
            let height = 7.0 + 22.0 * f * f;
            let color = Vec3::from(BUILDING_COLORS[rng.index(BUILDING_COLORS.len())])
                * rng.range(0.9, 1.05);
            self.add_building(lmin, lmax, height, color);

            // Stepped upper section for skyline variety.
            if rng.chance(0.35) && (lmax - lmin).min_element() > 9.0 {
                let inset = rng.range(1.5, 3.5);
                let h2 = height + rng.range(4.0, 12.0);
                let shade = color * 0.92;
                let (umin, umax) = (lmin + inset, lmax - inset);
                self.boxes.push(VisBox {
                    min: Vec3::new(umin.x, height, umin.y),
                    max: Vec3::new(umax.x, h2, umax.y),
                    color: shade,
                    side_mat: mat::WALL,
                    top_mat: mat::ROOF,
                });
                self.solids.push(Solid {
                    min: Vec3::new(umin.x, height, umin.y),
                    max: Vec3::new(umax.x, h2, umax.y),
                });
            }

            // Rooftop clutter (visual only).
            for _ in 0..rng.int(0, 3) {
                let s = Vec2::new(rng.range(1.0, 2.5), rng.range(1.0, 2.5));
                let p = Vec2::new(
                    rng.range(lmin.x + 1.0, (lmax.x - s.x - 1.0).max(lmin.x + 1.0)),
                    rng.range(lmin.y + 1.0, (lmax.y - s.y - 1.0).max(lmin.y + 1.0)),
                );
                self.boxes.push(VisBox {
                    min: Vec3::new(p.x, height, p.y),
                    max: Vec3::new(p.x + s.x, height + rng.range(0.8, 2.0), p.y + s.y),
                    color: Vec3::splat(0.62),
                    side_mat: mat::PLAIN,
                    top_mat: mat::PLAIN,
                });
            }
        }
    }

    fn add_building(&mut self, min: Vec2, max: Vec2, height: f32, color: Vec3) {
        let (min3, max3) = (Vec3::new(min.x, 0.0, min.y), Vec3::new(max.x, height, max.y));
        self.boxes.push(VisBox {
            min: min3,
            max: max3,
            color,
            side_mat: mat::WALL,
            top_mat: mat::ROOF,
        });
        self.solids.push(Solid { min: min3, max: max3 });
    }

    fn fill_plaza(&mut self, b: CellRect, rng: &mut Rng) {
        let (bmin, bmax) = (b.world_min(), b.world_max());
        self.boxes.push(VisBox {
            min: Vec3::new(bmin.x, 0.0, bmin.y),
            max: Vec3::new(bmax.x, 0.06, bmax.y),
            color: Vec3::new(0.62, 0.58, 0.52),
            side_mat: mat::PLAIN,
            top_mat: mat::PAVING,
        });

        let mut placed: Vec<(Vec2, Vec2)> = Vec::new();
        let count = ((b.w() * b.h()) as f32 / 9.0).round() as i32 + rng.int(0, 2);
        for _ in 0..count * 4 {
            if placed.len() as i32 >= count {
                break;
            }
            let kind = rng.f32();
            let (half, height, color) = if kind < 0.45 {
                // Concrete jersey barrier.
                let along_x = rng.chance(0.5);
                let h = if along_x { Vec2::new(2.2, 0.5) } else { Vec2::new(0.5, 2.2) };
                (h, 1.3, Vec3::new(0.72, 0.72, 0.70))
            } else if kind < 0.8 {
                // Shipping crate.
                let s = rng.range(1.1, 1.6);
                (Vec2::splat(s), s * 2.0, crate_color(rng))
            } else {
                // Low wall segment.
                let along_x = rng.chance(0.5);
                let h = if along_x { Vec2::new(3.5, 0.35) } else { Vec2::new(0.35, 3.5) };
                (h, 2.2, Vec3::new(0.64, 0.56, 0.48))
            };
            let margin = 2.0;
            let c = Vec2::new(
                rng.range(bmin.x + margin + half.x, bmax.x - margin - half.x),
                rng.range(bmin.y + margin + half.y, bmax.y - margin - half.y),
            );
            // Keep a tank-wide gap between pieces of cover.
            let clear = 5.0;
            if placed.iter().any(|(pc, ph)| {
                (c.x - pc.x).abs() < half.x + ph.x + clear && (c.y - pc.y).abs() < half.y + ph.y + clear
            }) {
                continue;
            }
            placed.push((c, half));
            let min = Vec3::new(c.x - half.x, 0.0, c.y - half.y);
            let max = Vec3::new(c.x + half.x, height, c.y + half.y);
            self.boxes.push(VisBox { min, max, color, side_mat: mat::PLAIN, top_mat: mat::PLAIN });
            self.solids.push(Solid { min, max });
        }
    }

    fn fill_park(&mut self, b: CellRect, rng: &mut Rng) {
        let (bmin, bmax) = (b.world_min(), b.world_max());
        self.boxes.push(VisBox {
            min: Vec3::new(bmin.x, 0.0, bmin.y),
            max: Vec3::new(bmax.x, 0.1, bmax.y),
            color: Vec3::new(0.32, 0.48, 0.24),
            side_mat: mat::PLAIN,
            top_mat: mat::GRASS,
        });
        let mut placed: Vec<Vec2> = Vec::new();
        let count = ((b.w() * b.h()) as f32 / 8.0).round() as i32;
        for _ in 0..count * 5 {
            if placed.len() as i32 >= count {
                break;
            }
            let p = Vec2::new(rng.range(bmin.x + 3.0, bmax.x - 3.0), rng.range(bmin.y + 3.0, bmax.y - 3.0));
            if placed.iter().any(|q| q.distance(p) < 7.0) {
                continue;
            }
            placed.push(p);
            let r = rng.range(1.8, 2.8);
            let trunk = 0.35;
            self.boxes.push(VisBox {
                min: Vec3::new(p.x - trunk, 0.0, p.y - trunk),
                max: Vec3::new(p.x + trunk, 3.2, p.y + trunk),
                color: Vec3::new(0.36, 0.26, 0.18),
                side_mat: mat::PLAIN,
                top_mat: mat::PLAIN,
            });
            self.solids.push(Solid {
                min: Vec3::new(p.x - 0.6, 0.0, p.y - 0.6),
                max: Vec3::new(p.x + 0.6, 3.2 + r, p.y + 0.6),
            });
            self.trees.push(Tree { pos: Vec3::new(p.x, 3.0 + r * 0.6, p.y), radius: r });
        }
    }

    fn perimeter_walls(&mut self) {
        let s = self.size;
        let (t, h) = (2.0, 5.0);
        let walls = [
            (Vec3::new(-t, 0.0, -t), Vec3::new(s + t, h, 0.0)),
            (Vec3::new(-t, 0.0, s), Vec3::new(s + t, h, s + t)),
            (Vec3::new(-t, 0.0, 0.0), Vec3::new(0.0, h, s)),
            (Vec3::new(s, 0.0, 0.0), Vec3::new(s + t, h, s)),
        ];
        for (min, max) in walls {
            self.boxes.push(VisBox {
                min,
                max,
                color: Vec3::new(0.40, 0.40, 0.42),
                side_mat: mat::PLAIN,
                top_mat: mat::PLAIN,
            });
            self.solids.push(Solid { min, max });
        }
    }

    /// Road cell centers with enough room around them to place a tank.
    /// Visual-only buildings outside the wall so the city seems to go on.
    fn backdrop(&mut self, rng: &mut Rng) {
        let s = self.size;
        let (gap, depth) = (6.0, 60.0);
        // Four bands around the arena; corners are covered by the x-bands.
        let bands = [
            (Vec2::new(-gap - depth, -gap - depth), Vec2::new(s + gap + depth, -gap), true),
            (Vec2::new(-gap - depth, s + gap), Vec2::new(s + gap + depth, s + gap + depth), true),
            (Vec2::new(-gap - depth, -gap), Vec2::new(-gap, s + gap), false),
            (Vec2::new(s + gap, -gap), Vec2::new(s + gap + depth, s + gap), false),
        ];
        for (min, max, along_x) in bands {
            let len = if along_x { max.x - min.x } else { max.y - min.y };
            let mut t = 0.0;
            while t < len {
                let w = rng.range(10.0, 22.0).min(len - t);
                let d = rng.range(12.0, depth);
                let (bmin, bmax) = if along_x {
                    let near_wall = min.y < 0.0;
                    let (y0, y1) = if near_wall { (max.y - d, max.y) } else { (min.y, min.y + d) };
                    (Vec2::new(min.x + t, y0), Vec2::new(min.x + t + w - 1.5, y1))
                } else {
                    let near_wall = min.x < 0.0;
                    let (x0, x1) = if near_wall { (max.x - d, max.x) } else { (min.x, min.x + d) };
                    (Vec2::new(x0, min.y + t), Vec2::new(x1, min.y + t + w - 1.5))
                };
                let f = rng.f32();
                let h = 10.0 + 30.0 * f * f;
                let color = Vec3::from(BUILDING_COLORS[rng.index(BUILDING_COLORS.len())]) * rng.range(0.85, 1.0);
                self.boxes.push(VisBox {
                    min: Vec3::new(bmin.x, 0.0, bmin.y),
                    max: Vec3::new(bmax.x, h, bmax.y),
                    color,
                    side_mat: mat::WALL,
                    top_mat: mat::ROOF,
                });
                t += w;
            }
        }
    }

    fn collect_open_spots(&mut self) {
        const CLEARANCE: f32 = 3.0;
        let mut seen = vec![false; (GRID * GRID) as usize];
        for r in &self.roads {
            for y in r.y0..r.y1 {
                for x in r.x0..r.x1 {
                    let i = (y * GRID + x) as usize;
                    if seen[i] {
                        continue;
                    }
                    seen[i] = true;
                    let p = Vec2::new(x as f32 + 0.5, y as f32 + 0.5) * CELL;
                    let blocked = self.solids.iter().any(|s| p.clamp(s.min2(), s.max2()).distance(p) < CLEARANCE);
                    if !blocked {
                        self.open_spots.push(p);
                    }
                }
            }
        }
    }

    pub fn center(&self) -> Vec2 {
        Vec2::splat(self.size * 0.5)
    }

    /// Flat ground and nothing else (the showroom yard).
    pub fn empty(size: f32) -> Map {
        Map {
            size,
            solids: Vec::new(),
            boxes: Vec::new(),
            trees: Vec::new(),
            wedges: Vec::new(),
            oboxes: Vec::new(),
            terrain: None,
            roads: Vec::new(),
            open_spots: vec![Vec2::ZERO],
            spawn_yaw: Some(0.0),
        }
    }

    /// Ground height at a point (terrain if present, else flat 0).
    pub fn ground_height(&self, x: f32, z: f32) -> f32 {
        self.terrain.as_ref().map_or(0.0, |t| t.height_at(x, z))
    }

    /// Hand-built physics test course: kickers at several angles with landing
    /// ramps, a raised platform with a ramp up and a drop off, speed bumps,
    /// stairs, rolling hills and a crater. Spawns face the obstacle lanes.
    pub fn proving_ground() -> Map {
        let size = 200.0;
        let mut map = Map {
            size,
            solids: Vec::new(),
            boxes: Vec::new(),
            trees: Vec::new(),
            wedges: Vec::new(),
            oboxes: Vec::new(),
            terrain: None,
            roads: Vec::new(),
            open_spots: Vec::new(),
            // Every lane runs along +z.
            spawn_yaw: Some(0.0),
        };

        // Terrain: flat in the west half (obstacle lanes), hills and a crater in the east.
        map.terrain = Some(Terrain::from_fn(Vec2::ZERO, 2.0, 101, 101, |x, z| {
            let east = ((x - 100.0) / 20.0).clamp(0.0, 1.0);
            let hills = (x * 0.06).sin() * (z * 0.05).cos() * 3.0 + (x * 0.13 + z * 0.07).sin() * 1.2;
            let mound = 7.0 * (-((x - 160.0).powi(2) + (z - 50.0).powi(2)) / 300.0).exp();
            let crater_d = ((x - 155.0).powi(2) + (z - 150.0).powi(2)).sqrt();
            let crater = if crater_d < 22.0 {
                -5.0 * (1.0 - (crater_d / 22.0).powi(2)) + 2.0 * (-(crater_d - 22.0).powi(2) / 10.0).exp()
            } else {
                2.0 * (-(crater_d - 22.0).powi(2) / 10.0).exp()
            };
            // Fade to flat near the walls so terrain never pokes over them.
            let edge = (x.min(200.0 - x).min(z).min(200.0 - z) / 15.0).clamp(0.0, 1.0);
            east * edge * (hills.max(-1.5) + 1.5 + mound + crater)
        }));

        let concrete = Vec3::new(0.62, 0.60, 0.56);
        let orange = Vec3::new(0.86, 0.48, 0.18);
        let blue = Vec3::new(0.30, 0.46, 0.70);

        // Lanes run along +z. Each lane: a kicker, then a landing ramp after a gap.
        let lanes = [(12.0f32, 14.0f32, orange), (32.0, 22.0, orange), (52.0, 30.0, orange)];
        for (x, deg, color) in lanes {
            let h = 1.6;
            let len = h / deg.to_radians().tan();
            let z0 = 40.0;
            map.wedges.push(Wedge {
                min: Vec2::new(x - 4.0, z0),
                max: Vec2::new(x + 4.0, z0 + len),
                base_y: 0.0,
                height: h,
                rise: Rise::PosZ,
                color,
            });
            // Landing ramp, same height as the kicker, after a gap sized to about
            // 70% of the flight distance at top speed so a full-speed jump clears it
            // and lands on the down slope.
            let (v, g) = (crate::sim::HullStats::LANCER.max_speed, crate::sim::GRAVITY);
            let flight = 2.0 * v * deg.to_radians().sin() / g * v * deg.to_radians().cos();
            let land_z = z0 + len + flight * 0.7;
            map.wedges.push(Wedge {
                min: Vec2::new(x - 5.0, land_z),
                max: Vec2::new(x + 5.0, land_z + 14.0),
                base_y: 0.0,
                height: h,
                rise: Rise::NegZ,
                color: concrete,
            });
            map.open_spots.push(Vec2::new(x, 12.0));
        }

        // Raised platform: long ramp up from the south, a steep side ramp on the
        // west, a gentler ramp down on the east, and open drops everywhere else.
        let (px0, px1, pz0, pz1, ph) = (72.0, 88.0, 70.0, 100.0, 3.5);
        map.add_block(Vec3::new(px0, 0.0, pz0), Vec3::new(px1, ph, pz1), concrete);
        map.wedges.push(Wedge { min: Vec2::new(px0, pz0 - 30.0), max: Vec2::new(px1, pz0), base_y: 0.0, height: ph, rise: Rise::PosZ, color: blue });
        map.wedges.push(Wedge { min: Vec2::new(px0 - 9.0, pz0 + 4.0), max: Vec2::new(px0, pz0 + 14.0), base_y: 0.0, height: ph, rise: Rise::PosX, color: blue });
        map.wedges.push(Wedge { min: Vec2::new(px1, pz0 + 16.0), max: Vec2::new(px1 + 12.0, pz0 + 26.0), base_y: 0.0, height: ph, rise: Rise::NegX, color: blue });
        map.open_spots.push(Vec2::new(80.0, 20.0));

        // Speed bumps: back-to-back wedge pairs across a lane.
        for i in 0..6 {
            let z = 120.0 + i as f32 * 9.0;
            for (rise, z0, z1) in [(Rise::PosZ, z, z + 1.2), (Rise::NegZ, z + 1.2, z + 2.4)] {
                map.wedges.push(Wedge { min: Vec2::new(6.0, z0), max: Vec2::new(18.0, z1), base_y: 0.0, height: 0.35, rise, color: Vec3::new(0.85, 0.75, 0.2) });
            }
        }
        map.open_spots.push(Vec2::new(12.0, 105.0));

        // Stairs: 0.25 m steps up to a 2 m ledge.
        for i in 0..8 {
            let z = 120.0 + i as f32 * 2.0;
            let top = 0.25 * (i + 1) as f32;
            map.add_block(Vec3::new(28.0, 0.0, z), Vec3::new(40.0, top, 200.0 - 50.0), concrete * (0.9 + 0.02 * i as f32));
        }
        map.open_spots.push(Vec2::new(34.0, 105.0));

        // Scattered blocks to bump into.
        for (x, z, s) in [(60.0, 130.0, 1.5), (66.0, 140.0, 1.0), (56.0, 150.0, 2.0), (70.0, 160.0, 1.2)] {
            map.add_block(Vec3::new(x - s, 0.0, z - s), Vec3::new(x + s, s * 2.0, z + s), Vec3::new(0.70, 0.30, 0.22));
        }

        // Hills / crater spawns.
        map.open_spots.push(Vec2::new(120.0, 20.0));
        map.open_spots.push(Vec2::new(150.0, 110.0));

        map.perimeter_walls();
        map
    }

    fn add_block(&mut self, min: Vec3, max: Vec3, color: Vec3) {
        self.boxes.push(VisBox { min, max, color, side_mat: mat::PLAIN, top_mat: mat::PAVING });
        self.solids.push(Solid { min, max });
    }
}

fn crate_color(rng: &mut Rng) -> Vec3 {
    const C: [[f32; 3]; 4] = [[0.70, 0.30, 0.22], [0.25, 0.42, 0.62], [0.80, 0.62, 0.20], [0.32, 0.52, 0.36]];
    Vec3::from(C[rng.index(C.len())])
}

fn bsp(r: CellRect, rng: &mut Rng, blocks: &mut Vec<CellRect>, roads: &mut Vec<CellRect>, depth: u32) {
    let road_w = if depth < 2 { 3 } else { rng.int(2, 3) };
    let can_x = r.w() >= 2 * MIN_BLOCK + road_w;
    let can_y = r.h() >= 2 * MIN_BLOCK + road_w;
    let small = r.w().max(r.h()) <= 10;
    if (!can_x && !can_y) || (small && rng.chance(0.5)) {
        blocks.push(r);
        return;
    }
    let split_x = if can_x && can_y {
        if r.w() == r.h() { rng.chance(0.5) } else { r.w() > r.h() }
    } else {
        can_x
    };
    if split_x {
        let s = rng.int(r.x0 + MIN_BLOCK, r.x1 - MIN_BLOCK - road_w);
        roads.push(CellRect { x0: s, y0: r.y0, x1: s + road_w, y1: r.y1 });
        bsp(CellRect { x1: s, ..r }, rng, blocks, roads, depth + 1);
        bsp(CellRect { x0: s + road_w, ..r }, rng, blocks, roads, depth + 1);
    } else {
        let s = rng.int(r.y0 + MIN_BLOCK, r.y1 - MIN_BLOCK - road_w);
        roads.push(CellRect { x0: r.x0, y0: s, x1: r.x1, y1: s + road_w });
        bsp(CellRect { y1: s, ..r }, rng, blocks, roads, depth + 1);
        bsp(CellRect { y0: s + road_w, ..r }, rng, blocks, roads, depth + 1);
    }
}

/// Split a block into building lots, with occasional 2-cell alleys between them.
fn split_lots(r: CellRect, rng: &mut Rng, lots: &mut Vec<CellRect>, depth: u32) {
    let long = r.w().max(r.h());
    if depth >= 2 || long < 6 || rng.chance(0.3) {
        lots.push(r);
        return;
    }
    let alley = if long >= 9 && rng.chance(0.45) { 2 } else { 0 };
    let min_part = 3;
    if r.w() >= r.h() {
        if r.w() < 2 * min_part + alley {
            lots.push(r);
            return;
        }
        let s = rng.int(r.x0 + min_part, r.x1 - min_part - alley);
        split_lots(CellRect { x1: s, ..r }, rng, lots, depth + 1);
        split_lots(CellRect { x0: s + alley, ..r }, rng, lots, depth + 1);
    } else {
        if r.h() < 2 * min_part + alley {
            lots.push(r);
            return;
        }
        let s = rng.int(r.y0 + min_part, r.y1 - min_part - alley);
        split_lots(CellRect { y1: s, ..r }, rng, lots, depth + 1);
        split_lots(CellRect { y0: s + alley, ..r }, rng, lots, depth + 1);
    }
}
