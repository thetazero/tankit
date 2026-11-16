//! The Foundry: an Uthenn tungsten cube factory, wrecked. The Uthenn make
//! tungsten cubes; this is where they made them, until something broke it
//! open.
//!
//! 200 m square (+x east, +z north), mirrored east to west. In the middle
//! the foundry hall, its walls blown open and its roof mostly gone. At its
//! heart the smelter, a dead block of pale shell. Three production lines run
//! out of it each way, east and west: low conveyors (cover) still carrying
//! cubes, presses standing over them or fallen across. Out past the hall's
//! broken walls the stockpile yards, stacks of finished cubes in rows; north
//! and south of the hall, open ground with great cubes lying where they
//! fell. Bases at the west and east edges.

use std::f32::consts::{FRAC_PI_2, TAU};

use glam::{Vec2, Vec3};

use super::{Map, OBox, Terrain};
use crate::faction::Faction;
use crate::render::material::Material;
use crate::rng::Rng;

const SIZE: f32 = 200.0;
const MID: f32 = SIZE * 0.5;
const CENTER: Vec2 = Vec2::new(MID, MID);

/// The hall: walls from `HALL_MIN` to `HALL_MAX`, `HALL_H` tall.
const HALL_MIN: Vec2 = Vec2::new(56.0, 52.0);
const HALL_MAX: Vec2 = Vec2::new(144.0, 148.0);
const HALL_H: f32 = 13.0;
const WALL_T: f32 = 1.0;
/// The smelter at the heart: half extent and height.
const SMELTER_HALF: f32 = 9.0;
const SMELTER_H: f32 = 14.0;
/// Production lines run along x at these z, from the smelter out to the walls.
const LINES: [f32; 3] = [76.0, MID, 124.0];
/// Breaches in the end walls (z, half-width): where the lanes come in.
const BREACHES: [(f32, f32); 3] = [(64.0, 6.0), (MID - 12.0, 5.0), (136.0, 6.0)];

/// Tungsten: dark, dense grey.
const TUNGSTEN: Vec3 = Vec3::new(0.3, 0.31, 0.33);
/// The Uthenn's pale shell, on the hall and the smelter.
const SHELL: Vec3 = Vec3::new(0.6, 0.63, 0.63);
const SHELL_DARK: Vec3 = Vec3::new(0.2, 0.22, 0.23);
const FLOOR: Vec3 = Vec3::new(0.3, 0.31, 0.31);
const ASH: Vec3 = Vec3::new(0.17, 0.175, 0.175);
const STEEL: Vec3 = Vec3::new(0.34, 0.35, 0.37);
/// Uthenn green: a few lights still on in the wreck.
const GREEN: Vec3 = Vec3::new(0.15, 0.9, 0.35);

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Mirror a point east to west.
fn mirror(p: Vec2) -> Vec2 {
    Vec2::new(SIZE - p.x, p.y)
}

fn in_hall(p: Vec2) -> bool {
    p.x > HALL_MIN.x && p.x < HALL_MAX.x && p.y > HALL_MIN.y && p.y < HALL_MAX.y
}

/// Shell craters outside the hall: center and radius (west half; mirrored).
const CRATERS: [((f32, f32), f32); 4] = [
    ((38.0, 36.0), 7.0),
    ((44.0, 168.0), 6.0),
    ((86.0, 26.0), 5.5),
    ((100.0, 178.0), 6.5),
];

/// Play ground: the hall floor flat, ash outside, cratered.
fn ground(x: f32, z: f32) -> f32 {
    let p = Vec2::new(x, z);
    if in_hall(p) {
        return 0.0;
    }
    let w = Vec2::new(x.min(SIZE - x), z);
    let mut h = (x * 0.09).sin() * (z * 0.07).cos() * 0.3;
    for ((cx, cz), r) in CRATERS {
        let d = w.distance(Vec2::new(cx, cz));
        h -= 1.4 * (1.0 - smoothstep(r * 0.4, r, d));
        h += 0.6 * smoothstep(r * 0.8, r, d) * (1.0 - smoothstep(r, r * 1.4, d));
    }
    let to_hall = {
        let q = (p - (HALL_MIN + HALL_MAX) * 0.5).abs() - (HALL_MAX - HALL_MIN) * 0.5;
        q.max(Vec2::ZERO).length()
    };
    h * smoothstep(0.0, 6.0, to_hall)
}

/// Ash flats outside the arena, and spoil heaps.
fn scenery_ground(x: f32, z: f32) -> f32 {
    let p = Vec2::new(x, z);
    let q = (p - Vec2::splat(MID)).abs() - Vec2::splat(MID);
    let d = q.max(Vec2::ZERO).length() + q.x.max(q.y).min(0.0);
    if d < -0.01 {
        return -10.0;
    }
    let edge = p.clamp(Vec2::ZERO, Vec2::splat(SIZE));
    let base = ground(edge.x, edge.y);
    let heaps = smoothstep(8.0, 50.0, d)
        * (3.5 * ((x * 0.021).sin() * (z * 0.017).cos()).abs()
            + 1.5 * ((x * 0.006 + z * 0.004).sin() + 1.0));
    base + heaps
}

impl Map {
    pub fn foundry() -> Map {
        let sides = [Faction::Uthenn, Faction::Imperium];
        let mut map = Map {
            size: SIZE,
            solids: Vec::new(),
            boxes: Vec::new(),
            wedges: Vec::new(),
            oboxes: Vec::new(),
            terrain: None,
            scenery_terrain: None,
            sculpted: crate::render::mesh::MeshData::default(),
            open_spots: Vec::new(),
            spawn_yaw: None,
            // Overcast and smoky: a flat pale sun through grey haze.
            atmosphere: crate::render::Atmosphere {
                sun_dir: Vec3::new(0.4, 0.62, -0.45),
                sun_color: Vec3::new(1.6, 1.62, 1.6),
                sky_zenith: Vec3::new(0.14, 0.16, 0.17),
                sky_horizon: Vec3::new(0.34, 0.36, 0.35),
                ground_color: Vec3::new(0.15, 0.16, 0.16),
                fog_density: 0.003,
                wind: Vec2::new(1.2, -0.6),
                blowing_sand: 0.2,
                dust: Vec3::new(0.3, 0.31, 0.3),
            },
            bases: Map::no_bases(&sides),
            factions: sides.to_vec(),
            repair_spots: Vec::new(),
            hold_site: Some(CENTER + Vec2::new(0.0, 17.0)),
            cores: Vec::new(),
            crystals: Vec::new(),
            glow_paths: Vec::new(),
            fall_y: None,
            hold_name: "SMELTER",
        };
        let mut terrain = Terrain::from_fn(Vec2::ZERO, 2.0, 101, 101, ground);
        terrain.color = ASH;
        terrain.material = Material::Sand;
        map.terrain = Some(terrain);
        let mut rng = Rng::new(0x007c_0be5);

        map.hall(&mut rng);
        map.smelter(&mut rng);
        map.production_lines(&mut rng);
        map.stockpiles(&mut rng);
        map.fallen_cubes(&mut rng);
        map.foundry_spawns();
        map.perimeter(false);
        map.foundry_horizon(&mut rng);
        map
    }

    /// A plain box, on both sides of the map (mirrored) unless it sits on
    /// the middle line.
    #[allow(clippy::too_many_arguments)]
    fn twin(
        &mut self,
        center: Vec2,
        half: Vec2,
        y0: f32,
        y1: f32,
        color: Vec3,
        mat: Material,
        solid: bool,
    ) -> [usize; 2] {
        let o = OBox {
            center,
            half,
            y0,
            y1,
            color,
            side_mat: mat,
            top_mat: mat,
            solid,
            chip: if mat == Material::Concrete { 0.2 } else { 0.0 },
            ..OBox::default()
        };
        self.oboxes.push(o);
        let a = self.oboxes.len() - 1;
        if (center.x - MID).abs() < 0.01 {
            return [a, a];
        }
        self.oboxes.push(OBox {
            center: mirror(center),
            ..o
        });
        [a, self.oboxes.len() - 1]
    }

    /// The same edit to a mirrored pair, mirroring yaw.
    fn edit_twin(&mut self, pair: [usize; 2], f: impl Fn(&mut OBox)) {
        f(&mut self.oboxes[pair[0]]);
        if pair[1] != pair[0] {
            let yaw = self.oboxes[pair[0]].yaw;
            f(&mut self.oboxes[pair[1]]);
            self.oboxes[pair[1]].yaw = -yaw;
        }
    }

    /// One tungsten cube, `s` on a side, sitting on `y`: turned `yaw`,
    /// tipped `tilt`. Visual only unless `solid`.
    #[allow(clippy::too_many_arguments)]
    fn cube(&mut self, at: Vec2, y: f32, s: f32, yaw: f32, tilt: f32, solid: bool, twin: bool) {
        let o = OBox {
            center: at,
            yaw,
            half: Vec2::splat(s * 0.5),
            y0: y,
            y1: y + s,
            color: TUNGSTEN,
            side_mat: Material::Metal,
            top_mat: Material::Metal,
            tilt,
            solid,
            ..OBox::default()
        };
        self.oboxes.push(o);
        if twin {
            self.oboxes.push(OBox {
                center: mirror(at),
                yaw: -yaw,
                ..o
            });
        }
    }

    /// The hall: pale walls on a dark plinth, blown open where the lanes
    /// come in and stepping down ragged either side of each breach; column
    /// stumps and a few roof girders still spanning; the floor slab.
    fn hall(&mut self, rng: &mut Rng) {
        let c = (HALL_MIN + HALL_MAX) * 0.5;
        let h = (HALL_MAX - HALL_MIN) * 0.5;
        // Floor slab, a shade paler than the ash outside.
        self.oboxes.push(OBox {
            center: c,
            half: h,
            y0: -0.2,
            y1: 0.02,
            color: FLOOR,
            side_mat: Material::Asphalt,
            top_mat: Material::Asphalt,
            solid: false,
            ..OBox::default()
        });

        // End walls (west, mirrored east), broken at the breaches.
        let mut cuts: Vec<(f32, f32)> = BREACHES.to_vec();
        cuts.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut z = HALL_MIN.y;
        let x = HALL_MIN.x + WALL_T * 0.5;
        for (bz, bw) in cuts.iter().copied().chain([(HALL_MAX.y + 100.0, 0.0)]) {
            let end = (bz - bw).min(HALL_MAX.y);
            let ragged = [z > HALL_MIN.y, end < HALL_MAX.y];
            self.wall_run(rng, Vec2::new(x, z), Vec2::new(x, end), true, ragged);
            z = bz + bw;
            if z >= HALL_MAX.y {
                break;
            }
        }
        // Long walls (south and north): a blasted gap in each half; the
        // run meets its mirror whole in the middle.
        for wz in [HALL_MIN.y + WALL_T * 0.5, HALL_MAX.y - WALL_T * 0.5] {
            let gaps = [(78.0, 5.5), (MID, 0.0)];
            let mut x0 = HALL_MIN.x;
            for (gx, gw) in gaps {
                let ragged = [x0 > HALL_MIN.x, gw > 0.0];
                self.wall_run(rng, Vec2::new(x0, wz), Vec2::new(gx - gw, wz), true, ragged);
                x0 = gx + gw;
            }
        }

        // Columns: a grid of stumps, broken off at different heights, some
        // still standing full height with a girder across.
        let cols_x = [72.0, 88.0];
        let cols_z = [62.0, 88.0, 112.0, 138.0];
        for &cx in &cols_x {
            for &cz in &cols_z {
                let at = Vec2::new(cx, cz);
                let full = rng.chance(0.35);
                let top = if full { HALL_H } else { rng.range(2.5, 8.0) };
                let pair = self.twin(
                    at,
                    Vec2::splat(0.55),
                    0.0,
                    top,
                    SHELL,
                    Material::Painted,
                    true,
                );
                if !full {
                    self.edit_twin(pair, |o| o.broken = true);
                }
            }
        }
        // Girders: a few still spanning the hall north to south, high up.
        for gx in [72.0, 88.0] {
            if rng.chance(0.6) {
                self.twin(
                    Vec2::new(gx, MID),
                    Vec2::new(0.4, (HALL_MAX.y - HALL_MIN.y) * 0.5),
                    HALL_H - 0.9,
                    HALL_H,
                    STEEL,
                    Material::Rust,
                    false,
                );
            }
        }
        // A girder that came down: one end on a wall top, the other in the
        // floor, across a line.
        let fall = self.twin(
            Vec2::new(80.0, 88.0),
            Vec2::new(0.4, 9.0),
            2.6,
            3.4,
            STEEL,
            Material::Rust,
            true,
        );
        self.edit_twin(fall, |o| {
            o.yaw = 0.5;
            o.tilt = 0.32;
        });
        // Roof slabs that fell in, lying broken on the floor.
        for (at, yaw) in [
            ((64.0, 112.0), 0.3),
            ((84.0, 132.0), -0.6),
            ((66.0, 90.0), 1.1),
        ] {
            let s = self.twin(
                Vec2::new(at.0, at.1),
                Vec2::new(3.2, 2.2),
                0.0,
                0.45,
                SHELL_DARK,
                Material::Concrete,
                true,
            );
            let tilt = rng.range(0.05, 0.12);
            self.edit_twin(s, |o| {
                o.yaw = yaw;
                o.tilt = tilt;
                o.broken = true;
            });
        }
    }

    /// A wall from `a` to `b` (axis-aligned), in panels: a dark plinth and
    /// pale shell above. Panels toward a `ragged` end (where it meets a
    /// breach) step down broken; a seam of green runs along the plinth,
    /// lit in places. `mirrored` adds the east twin.
    fn wall_run(&mut self, rng: &mut Rng, a: Vec2, b: Vec2, mirrored: bool, ragged: [bool; 2]) {
        let len = a.distance(b);
        if len < 0.5 {
            return;
        }
        let dir = (b - a) / len;
        let n = (len / 6.0).ceil() as usize;
        let seg = len / n as f32;
        for k in 0..n {
            let mid = a + dir * (seg * (k as f32 + 0.5));
            let half = if dir.x.abs() > 0.5 {
                Vec2::new(seg * 0.5, WALL_T * 0.5)
            } else {
                Vec2::new(WALL_T * 0.5, seg * 0.5)
            };
            // Height: full, but stepping down ragged toward a broken end.
            let from_end = match ragged {
                [true, true] => k.min(n - 1 - k),
                [true, false] => k,
                [false, true] => n - 1 - k,
                [false, false] => n,
            } as f32;
            let top = if from_end < 1.0 {
                rng.range(1.6, 4.5)
            } else if from_end < 2.0 {
                rng.range(5.0, 9.0)
            } else if rng.chance(0.12) {
                rng.range(8.0, 11.5)
            } else {
                HALL_H
            };
            let broken = top < HALL_H;
            let put = |m: &mut Map, o: OBox| {
                m.oboxes.push(o);
                if mirrored {
                    m.oboxes.push(OBox {
                        center: mirror(o.center),
                        ..o
                    });
                }
            };
            put(
                self,
                OBox {
                    center: mid,
                    half,
                    y0: 0.0,
                    y1: 1.2,
                    color: SHELL_DARK,
                    side_mat: Material::Concrete,
                    top_mat: Material::Concrete,
                    ..OBox::default()
                },
            );
            put(
                self,
                OBox {
                    center: mid,
                    half,
                    y0: 1.2,
                    y1: top,
                    color: SHELL,
                    side_mat: Material::Painted,
                    top_mat: Material::Painted,
                    broken,
                    chip: 0.15,
                    ..OBox::default()
                },
            );
            // The green seam, proud of the wall on both faces; dark where
            // it's broken.
            let lit = !broken && rng.chance(0.5);
            let seam = OBox {
                center: mid,
                half: half + Vec2::splat(0.06) * Vec2::new(dir.y.abs(), dir.x.abs()),
                y0: 1.25,
                y1: 1.4,
                color: if lit { GREEN } else { SHELL_DARK },
                side_mat: if lit {
                    Material::Glow
                } else {
                    Material::Concrete
                },
                top_mat: if lit {
                    Material::Glow
                } else {
                    Material::Concrete
                },
                solid: false,
                ..OBox::default()
            };
            put(self, seam);
            // Rubble at the foot of a broken panel.
            if broken && top < 6.0 {
                for _ in 0..rng.int(2, 4) {
                    let off = Vec2::new(rng.range(-2.0, 2.0), rng.range(-2.5, 2.5));
                    let s = rng.range(0.5, 1.2);
                    let o = OBox {
                        center: mid + off,
                        yaw: rng.range(0.0, TAU),
                        half: Vec2::splat(s * 0.5),
                        y0: -0.2,
                        y1: s * 0.6,
                        color: SHELL * rng.range(0.85, 1.0),
                        side_mat: Material::Painted,
                        top_mat: Material::Painted,
                        tilt: rng.range(-0.4, 0.4),
                        solid: false,
                        ..OBox::default()
                    };
                    put(self, o);
                }
            }
        }
    }

    /// The smelter: an eight-sided furnace tower at the hall's heart, pale
    /// shell on a dark plinth, banded, with a hopper on top and two stacks,
    /// one snapped. Its north face is blown out and the crucible inside
    /// still glows green; cubes have spilled out through the breach. Chutes
    /// run down from its flanks to the middle production lines.
    fn smelter(&mut self, rng: &mut Rng) {
        // Wall heights by face, from east (0) round through north (2):
        // the north face is gone, the faces either side broken low. Faces
        // mirror (0 and 4, 1 and 3, 5 and 7), so the halves match.
        let heights = [
            SMELTER_H, 6.5, 0.0, 6.5, SMELTER_H, SMELTER_H, SMELTER_H, SMELTER_H,
        ];
        self.octagon(CENTER, SMELTER_HALF, 0.0, &heights, true);
        // The hopper: a narrower ring on top, its north side broken away.
        let hopper = [5.0, 3.0, 0.0, 3.0, 5.0, 5.0, 5.0, 5.0];
        self.octagon(
            CENTER + Vec2::new(0.0, -1.0),
            4.6,
            SMELTER_H,
            &hopper,
            false,
        );
        // The roof between them, broken open to the north.
        self.oboxes.push(OBox {
            center: CENTER + Vec2::new(0.0, -2.0),
            half: Vec2::new(5.8, 4.2),
            y0: SMELTER_H - 0.6,
            y1: SMELTER_H,
            color: SHELL_DARK,
            side_mat: Material::Concrete,
            top_mat: Material::Concrete,
            solid: false,
            broken: true,
            ..OBox::default()
        });
        // Two stacks on the roof, one snapped off.
        for (x, h, broken) in [(-5.0f32, 15.0, false), (5.0, 5.5, true)] {
            let i = self.twin(
                CENTER + Vec2::new(x, -4.5),
                Vec2::splat(1.2),
                SMELTER_H,
                SMELTER_H + h,
                SHELL_DARK * 1.3,
                Material::Concrete,
                false,
            );
            self.edit_twin(i, |o| o.broken = broken);
            if !broken {
                self.twin(
                    CENTER + Vec2::new(x, -4.5),
                    Vec2::splat(1.3),
                    SMELTER_H + h - 2.0,
                    SMELTER_H + h - 1.6,
                    GREEN,
                    Material::Glow,
                    false,
                );
            }
        }
        // The crucible inside, still hot: a green glow you see through the
        // breach, under a dark lid.
        self.oboxes.push(OBox {
            center: CENTER,
            half: Vec2::splat(4.2),
            yaw: std::f32::consts::FRAC_PI_4,
            y0: 0.0,
            y1: 3.2,
            color: GREEN,
            side_mat: Material::Glow,
            top_mat: Material::Glow,
            solid: false,
            ..OBox::default()
        });
        self.oboxes.push(OBox {
            center: CENTER,
            half: Vec2::splat(5.0),
            yaw: std::f32::consts::FRAC_PI_4,
            y0: 3.2,
            y1: 4.0,
            color: SHELL_DARK * 0.7,
            side_mat: Material::Concrete,
            top_mat: Material::Concrete,
            solid: false,
            ..OBox::default()
        });
        // Chutes from its flanks down to the middle lines (west, mirrored east).
        let chute = self.twin(
            Vec2::new(MID - SMELTER_HALF - 2.4, MID),
            Vec2::new(0.7, 3.0),
            4.2,
            4.6,
            SHELL_DARK,
            Material::Metal,
            false,
        );
        self.edit_twin(chute, |o| {
            o.yaw = -FRAC_PI_2;
            o.tilt = 0.55;
        });
        // Spilled out through the breach: a heap of cubes.
        let h = SMELTER_HALF;
        for _ in 0..40 {
            let at = CENTER
                + Vec2::new(
                    rng.range(-6.0, 6.0),
                    h - 2.0 + rng.range(0.5, 7.0) * rng.f32(),
                );
            let s = rng.range(0.5, 0.9);
            self.cube(
                at,
                -0.1,
                s,
                rng.range(0.0, TAU),
                rng.range(-0.3, 0.3),
                false,
                false,
            );
        }
        // Two bigger ones either side of the breach, cover beside the heap.
        for x in [-5.5f32, 5.5] {
            self.cube(
                CENTER + Vec2::new(x, h + 2.5),
                -0.1,
                1.6,
                rng.range(-0.4, 0.4),
                0.0,
                true,
                false,
            );
        }
    }

    /// An eight-sided wall ring round `c`, `r` from center to face: each
    /// face a panel of pale shell on a dark plinth from `y0`, `heights[i]`
    /// tall (0: no panel), face 0 facing east and counting round through
    /// north. A panel shorter than the tallest is broken off; `bands` adds
    /// a dark band round each full panel.
    fn octagon(&mut self, c: Vec2, r: f32, y0: f32, heights: &[f32; 8], bands: bool) {
        let side = 2.0 * r * (std::f32::consts::PI / 8.0).tan() + 0.3;
        let full = heights.iter().copied().fold(0.0, f32::max);
        for (i, &h) in heights.iter().enumerate() {
            if h <= 0.0 {
                continue;
            }
            let a = i as f32 * std::f32::consts::FRAC_PI_4;
            let out = Vec2::new(a.cos(), a.sin());
            // Local +z faces out.
            let yaw = out.x.atan2(out.y);
            let at = c + out * r;
            let panel = |y0: f32, y1: f32, color: Vec3, mat: Material, solid: bool| OBox {
                center: at,
                yaw,
                half: Vec2::new(side * 0.5, 0.6),
                y0,
                y1,
                color,
                side_mat: mat,
                top_mat: mat,
                solid,
                ..OBox::default()
            };
            let plinth = (y0 + 1.4).min(y0 + h);
            self.oboxes
                .push(panel(y0, plinth, SHELL_DARK, Material::Concrete, true));
            let mut wall = panel(plinth, y0 + h, SHELL, Material::Painted, true);
            wall.broken = h < full;
            wall.chip = 0.15;
            self.oboxes.push(wall);
            if bands && h >= full {
                let mut band = panel(y0 + 8.4, y0 + 9.0, SHELL_DARK, Material::Concrete, false);
                band.half += Vec2::new(0.0, 0.08);
                self.oboxes.push(band);
            }
        }
    }

    /// Three production lines each side, from the smelter out to the end
    /// walls: low conveyors with cubes still on the belt, a press over each,
    /// broken at a gap so you can cross between them.
    fn production_lines(&mut self, rng: &mut Rng) {
        let (x0, x1) = (HALL_MIN.x + 5.0, MID - SMELTER_HALF - 1.0);
        for &lz in &LINES {
            // The middle line comes out of the smelter; the outer ones start
            // beside it.
            let gaps: &[(f32, f32)] = &[(70.0, 3.5), (84.5, 3.0)];
            let mut start = x0;
            for &(gx, gw) in gaps.iter().chain(&[(x1 + 100.0, 0.0)]) {
                let end = (gx - gw).min(x1);
                if end - start > 1.0 {
                    self.conveyor(rng, start, end, lz);
                }
                start = gx + gw;
                if start >= x1 {
                    break;
                }
            }
            self.press(rng, Vec2::new(77.0, lz));
        }
    }

    /// A conveyor along x from `x0` to `x1` at `z`: a dark frame on legs, a
    /// tungsten belt, cubes riding it at a fixed pitch.
    fn conveyor(&mut self, rng: &mut Rng, x0: f32, x1: f32, z: f32) {
        let c = Vec2::new((x0 + x1) * 0.5, z);
        let half = Vec2::new((x1 - x0) * 0.5, 1.2);
        self.twin(c, half, 0.0, 1.0, SHELL_DARK, Material::Concrete, true);
        self.twin(
            c,
            half - Vec2::new(0.1, 0.25),
            1.0,
            1.08,
            STEEL * 0.5,
            Material::Metal,
            false,
        );
        let n = ((x1 - x0) / 1.5) as usize;
        for k in 0..n {
            if rng.chance(0.25) {
                continue;
            }
            let x = x0 + 0.75 + k as f32 * 1.5;
            self.cube(
                Vec2::new(x, z + rng.range(-0.2, 0.2)),
                1.08,
                0.75,
                rng.range(-0.15, 0.15),
                0.0,
                false,
                true,
            );
        }
        // Cubes knocked off the belt onto the floor.
        for _ in 0..rng.int(2, 5) {
            let x = rng.range(x0, x1);
            let side = if rng.chance(0.5) { 1.0 } else { -1.0 };
            self.cube(
                Vec2::new(x, z + side * rng.range(1.6, 3.0)),
                -0.05,
                0.6,
                rng.range(0.0, TAU),
                rng.range(-0.5, 0.5),
                false,
                true,
            );
        }
    }

    /// A press over a line: two legs and a head. Some stand; on others the
    /// head has come down beside the belt.
    fn press(&mut self, rng: &mut Rng, at: Vec2) {
        for s in [-1.0f32, 1.0] {
            self.twin(
                at + Vec2::new(0.0, s * 2.0),
                Vec2::new(0.7, 0.5),
                0.0,
                6.5,
                SHELL,
                Material::Painted,
                true,
            );
        }
        if rng.chance(0.5) {
            self.twin(
                at,
                Vec2::new(1.4, 2.6),
                5.0,
                7.2,
                SHELL_DARK,
                Material::Painted,
                true,
            );
            self.twin(
                at,
                Vec2::new(1.0, 1.2),
                4.2,
                5.0,
                TUNGSTEN,
                Material::Metal,
                false,
            );
        } else {
            let head = self.twin(
                at + Vec2::new(-1.5, 4.2),
                Vec2::new(1.4, 2.6),
                0.0,
                2.2,
                SHELL_DARK,
                Material::Painted,
                true,
            );
            self.edit_twin(head, |o| {
                o.yaw = 0.4;
                o.tilt = 0.15;
            });
        }
    }

    /// Outside the hall's end walls, the stockpile yards: rows of finished
    /// cubes stacked on pallets, the rows broken up by the lanes.
    fn stockpiles(&mut self, rng: &mut Rng) {
        for x in [34.0f32, 44.0] {
            for k in 0..9 {
                let z = 56.0 + k as f32 * 11.0;
                // Keep the lanes in from the bases clear.
                if BREACHES.iter().any(|&(bz, bw)| (z - bz).abs() < bw + 2.5) {
                    continue;
                }
                if rng.chance(0.15) {
                    continue;
                }
                let at = Vec2::new(x + rng.range(-1.0, 1.0), z);
                self.stack(rng, at);
            }
        }
    }

    /// A stack of 1 m cubes on a pallet: 2 or 3 square, 1 to 3 high, the top
    /// layer sometimes knocked askew.
    fn stack(&mut self, rng: &mut Rng, at: Vec2) {
        let n = rng.int(2, 3) as f32;
        let high = rng.int(1, 3) as f32;
        // Pallet.
        self.twin(
            at,
            Vec2::splat(n * 0.5 + 0.15),
            0.0,
            0.18,
            STEEL * 0.8,
            Material::Rust,
            true,
        );
        // The block of cubes (one collider), with the seams between cubes
        // left as dark lines on its faces.
        self.twin(
            at,
            Vec2::splat(n * 0.5),
            0.18,
            0.18 + high,
            TUNGSTEN,
            Material::Metal,
            true,
        );
        for k in 1..(n as i32) {
            let off = -n * 0.5 + k as f32;
            for line in [
                (Vec2::new(off, 0.0), Vec2::new(0.02, n * 0.5 + 0.01)),
                (Vec2::new(0.0, off), Vec2::new(n * 0.5 + 0.01, 0.02)),
            ] {
                self.twin(
                    at + line.0,
                    line.1,
                    0.18,
                    0.18 + high + 0.005,
                    SHELL_DARK * 0.4,
                    Material::Concrete,
                    false,
                );
            }
        }
        // A few loose on top, or fallen off.
        for _ in 0..rng.int(0, 3) {
            let off = Vec2::new(rng.range(-n, n), rng.range(-n, n)) * 0.45;
            let on_top = rng.chance(0.5);
            let (p, y) = if on_top {
                (at + off, 0.18 + high)
            } else {
                (at + off.normalize_or(Vec2::X) * (n * 0.5 + 1.0), -0.05)
            };
            self.cube(
                p,
                y,
                1.0,
                rng.range(-0.6, 0.6),
                if on_top { 0.0 } else { rng.range(-0.3, 0.3) },
                !on_top,
                true,
            );
        }
    }

    /// North and south of the hall, open ground with the great cubes, 4 to 7
    /// m on a side, lying where they fell: some square, some tipped onto an
    /// edge, one half sunk in a crater.
    fn fallen_cubes(&mut self, rng: &mut Rng) {
        let spots = [
            (Vec2::new(66.0, 34.0), 5.0),
            (Vec2::new(84.0, 40.0), 4.0),
            (Vec2::new(60.0, 166.0), 6.0),
            (Vec2::new(82.0, 160.0), 4.5),
            (Vec2::new(96.0, 22.0), 7.0),
            (Vec2::new(24.0, 22.0), 4.0),
            (Vec2::new(26.0, 178.0), 4.5),
        ];
        for (at, s) in spots {
            let g = self.ground_height(at.x, at.y);
            let tilt = if rng.chance(0.4) {
                rng.range(0.2, 0.45)
            } else {
                0.0
            };
            let twin = (at.x - MID).abs() > 1.0;
            self.cube(at, g - 0.6, s, rng.range(0.0, TAU), tilt, true, twin);
        }
    }

    /// Bases at the west and east edges; roam points along the lanes and
    /// in the hall; repair crates in the hall and the north and south yards.
    fn foundry_spawns(&mut self) {
        for (side, xs) in [(0, [8.0, 15.0]), (1, [SIZE - 8.0, SIZE - 15.0])] {
            for x in xs {
                for k in 0..10 {
                    self.bases[side].push(Vec2::new(x, MID - 30.0 + k as f32 * 6.5));
                }
            }
        }
        let mut spots = Vec::new();
        for &(bz, _) in &BREACHES {
            for x in [26.0, 52.0, 62.0] {
                spots.push(Vec2::new(x, bz));
            }
        }
        for &lz in &LINES {
            for x in [66.0, 80.0] {
                spots.push(Vec2::new(x, lz + 6.0));
            }
        }
        for z in [30.0, 170.0] {
            for x in [40.0, 74.0, MID] {
                spots.push(Vec2::new(x, z));
            }
        }
        let mirrored: Vec<Vec2> = spots.iter().map(|&p| mirror(p)).collect();
        spots.extend(mirrored);
        spots.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
        spots.dedup_by(|a, b| a.distance(*b) < 3.0);
        self.open_spots = spots
            .into_iter()
            .filter(|&p| self.foundry_clear(p, 3.5))
            .collect();
        self.repair_spots = vec![
            Vec2::new(MID, 70.0),
            Vec2::new(MID, 132.0),
            Vec2::new(MID, 34.0),
            Vec2::new(MID, 166.0),
        ];
        for side in 0..2 {
            let base = std::mem::take(&mut self.bases[side]);
            self.bases[side] = base
                .into_iter()
                .filter(|&p| self.foundry_clear(p, 3.5))
                .collect();
        }
    }

    /// Nothing solid within `r` of a ground point.
    fn foundry_clear(&self, p: Vec2, r: f32) -> bool {
        !self.oboxes.iter().any(|o| o.solid && o.distance(p) < r)
    }

    /// Beyond the edge: spoil heaps, more of the works (sheds and stacks,
    /// mostly dark), and towers of cubes on the skyline.
    fn foundry_horizon(&mut self, rng: &mut Rng) {
        let (cell, reach) = (4.0f32, 520.0f32);
        let n = ((SIZE + 2.0 * reach) / cell) as usize + 1;
        let mut t = Terrain::from_fn(Vec2::splat(-reach), cell, n, n, scenery_ground);
        t.color = ASH * 0.95;
        t.material = Material::Sand;
        self.scenery_terrain = Some(t);

        for _ in 0..70 {
            let a = rng.range(0.0, TAU);
            let r = rng.range(170.0, 420.0);
            let at = CENTER + Vec2::new(a.cos(), a.sin()) * r;
            let g = scenery_ground(at.x, at.y) - 1.0;
            match rng.int(0, 2) {
                // A shed: long, pale, roof gone in places.
                0 => {
                    let half = Vec2::new(rng.range(12.0, 30.0), rng.range(8.0, 16.0));
                    let h = rng.range(8.0, 16.0);
                    self.oboxes.push(OBox {
                        center: at,
                        yaw: rng.range(0.0, TAU),
                        half,
                        y0: g,
                        y1: g + h,
                        color: SHELL * rng.range(0.75, 0.95),
                        side_mat: Material::Painted,
                        top_mat: Material::Painted,
                        solid: false,
                        broken: rng.chance(0.5),
                        ..OBox::default()
                    });
                }
                // A stack: a tall square chimney, a green light near the
                // top on a few.
                1 => {
                    let w = rng.range(2.0, 3.5);
                    let h = rng.range(25.0, 55.0);
                    self.oboxes.push(OBox {
                        center: at,
                        half: Vec2::splat(w),
                        y0: g,
                        y1: g + h,
                        color: SHELL_DARK * 1.4,
                        side_mat: Material::Concrete,
                        top_mat: Material::Concrete,
                        solid: false,
                        broken: rng.chance(0.4),
                        ..OBox::default()
                    });
                    if rng.chance(0.35) {
                        self.oboxes.push(OBox {
                            center: at,
                            half: Vec2::splat(w + 0.1),
                            y0: g + h - 3.0,
                            y1: g + h - 2.4,
                            color: GREEN,
                            side_mat: Material::Glow,
                            top_mat: Material::Glow,
                            solid: false,
                            ..OBox::default()
                        });
                    }
                }
                // A tower of cubes: great cubes stacked, each turned a little.
                _ => {
                    let s = rng.range(6.0, 12.0);
                    let mut y = g;
                    let mut yaw = rng.range(0.0, TAU);
                    for _ in 0..rng.int(2, 5) {
                        self.cube(at, y, s, yaw, 0.0, false, false);
                        y += s;
                        yaw += rng.range(-0.3, 0.3);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::NavGrid;

    #[test]
    fn every_spawn_reaches_the_smelter_and_both_yards() {
        let m = Map::foundry();
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
            CENTER + Vec2::new(0.0, SMELTER_HALF + 6.0),
            Vec2::new(MID, 34.0),
            Vec2::new(MID, 166.0),
            Vec2::new(70.0, MID + 12.0),
            Vec2::new(SIZE - 70.0, MID + 12.0),
        ];
        for &p in m.open_spots.iter().chain(m.bases.iter().flatten()) {
            for g in goals {
                assert!(!nav.path(p, g).is_empty(), "no path from {p} to {g}");
            }
        }
    }

    #[test]
    fn repair_spots_and_the_hold_site_are_open() {
        let m = Map::foundry();
        let nav = NavGrid::build(&m);
        for &p in m.repair_spots.iter().chain(m.hold_site.iter()) {
            assert!(!nav.is_blocked(p), "{p} is blocked");
        }
    }

    #[test]
    fn the_halves_mirror() {
        let m = Map::foundry();
        let solid: Vec<&OBox> = m.oboxes.iter().filter(|o| o.solid).collect();
        for o in &solid {
            let p = mirror(o.center);
            assert!(
                solid.iter().any(|q| q.center.distance(p) < 0.01),
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

    /// Two minutes of a 6v6 round in the Foundry: both sides score, and
    /// every bot gets out of its base. Slow (run with `--ignored --release`).
    #[test]
    #[ignore]
    fn bots_fight_through_the_foundry() {
        let mut m = Match::new(Map::foundry(), 0, 11, 7);
        let start: Vec<Vec2> = m
            .world
            .tanks
            .iter()
            .map(|t| Vec2::new(t.pos.x, t.pos.z))
            .collect();
        let mut furthest = vec![0.0f32; start.len()];
        let steps = (120.0 / crate::game::DT) as usize;
        for k in 0..steps {
            m.step(Command::default(), false);
            if k % 120 == 0 {
                for (i, t) in m.world.tanks.iter().enumerate().skip(1) {
                    let p = Vec2::new(t.pos.x, t.pos.z);
                    furthest[i] = furthest[i].max(p.distance(start[i]));
                }
            }
        }
        let (a, b) = (m.score(0), m.score(1));
        eprintln!("score {a} : {b}; furthest {furthest:?}");
        assert!(a > 0 && b > 0, "score {a} : {b}");
        for (i, d) in furthest.iter().enumerate().skip(1) {
            assert!(*d > 35.0, "bot {i} only got {d} m out");
        }
    }
}
