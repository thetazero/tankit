//! Dressing a foundry's skeleton: the ground stained, what every works has
//! (walls, girders, rails, wagons, slag, castings) built in the style's
//! colours, the rest filled in by the style. And the gameplay: the places
//! bots head for, repair drops, the menus' vistas.

use std::f32::consts::{FRAC_PI_2, TAU};

use glam::{Quat, Vec2, Vec3};

use super::ground::{Ground, Land};
use super::skeleton::*;
use super::style::*;
use crate::ai::nav::NavGrid;
use crate::map::landmarks::Rig;
use crate::map::procgen::skeleton::smoothstep;
use crate::map::valley::noise2;
use crate::map::{Ambient, GlowPath, Map, OBox, Terrain, rocks};
use crate::render::PointLight;
use crate::render::material::Material;
use crate::rng::Rng;

const TIMBER: Vec3 = Vec3::new(0.3, 0.23, 0.17);
const RAIL: Vec3 = Vec3::new(0.36, 0.34, 0.33);
/// The rails' half gauge.
const GAUGE: f32 = 0.75;
/// How far past the edge the tracks run on.
const RUN_ON: f32 = 120.0;

pub fn dress(map: &mut Map, sk: &Skeleton, st: &dyn Works, ground: Ground, rng: &mut Rng) {
    let pal = st.palette();
    let land = ground.land();
    map.terrain = Some(terrain(sk, &land, ground));
    let mut s = Shop {
        map,
        sk,
        st,
        pal,
        ground,
        land,
    };
    s.halls(rng);
    for c in &sk.stacks {
        round_solid(s.map, c.at, c.r, (-0.3, 6.0));
        st.chimney(s.map, c);
    }
    for f in &sk.furnaces {
        s.furnace(f, rng);
    }
    s.crane();
    for g in &sk.gantries {
        s.gantry(g);
    }
    s.tracks();
    for w in &sk.wagons {
        s.wagon(w, rng);
    }
    for k in &sk.stock {
        s.stock(k, rng);
    }
    s.slag(rng);
    s.fence();
    s.bases();
    s.growth(rng);
    s.beyond(rng);
    s.map.separate_coplanar_faces();
    s.play();
    let views = crate::map::procgen::sunlit_views(s.map, &sk.bases, sk.hold);
    s.map.vistas = views;
}

/// The ground: sand, black with soot round the works, ballast under the
/// tracks, the slag.
fn terrain(sk: &Skeleton, land: &Land, ground: Ground) -> Terrain {
    let mut t = sk.terrain();
    t.color = Vec3::ONE;
    t.material = land.material;
    t.paint(|x, z, h, _| {
        let p = Vec2::new(x, z);
        let mottle = 0.9 + 0.16 * noise2(x / 6.0, z / 6.0, 210) + 0.06 * noise2(x, z, 211);
        let hall = sk
            .halls
            .iter()
            .map(|b| ((p - b.center).abs() - b.half).max(Vec2::ZERO).length())
            .fold(f32::MAX, f32::min);
        let stain = 1.0 - smoothstep(10.0, 72.0, hall + 16.0 * noise2(x / 22.0, z / 22.0, 212));
        // Snow lies thick or not at all; grass wears through to mud.
        let patch = noise2(x / 13.0, z / 13.0, 215);
        let bare = match ground {
            Ground::Desert => 0.5 * patch,
            Ground::Tundra => smoothstep(0.4, 0.62, patch + 0.25 * stain),
            Ground::Grassland => smoothstep(0.5, 0.75, patch + 0.3 * stain),
        };
        let open = land.open.lerp(land.low, bare);
        let (least, most) = match ground {
            Ground::Desert => (0.3, 1.0),
            _ => (0.0, 0.85),
        };
        let mut c = open.lerp(land.soot, least + (most - least) * stain);
        let track = sk
            .tracks
            .iter()
            .map(|k| (z - k).abs())
            .fold(f32::MAX, f32::min);
        c = c.lerp(land.ballast, (1.0 - smoothstep(2.0, 4.0, track)) * 0.9);
        c = c.lerp(land.slag, smoothstep(0.05, 1.4, h));
        if ground == Ground::Tundra {
            // Snow lodged on the slag.
            let lodged = smoothstep(0.5, 0.7, noise2(x / 8.0, z / 8.0, 216));
            c = c.lerp(land.open, lodged * 0.8 * smoothstep(0.5, 2.0, h));
        }
        c * mottle
    });
    t
}

struct Shop<'a> {
    map: &'a mut Map,
    sk: &'a Skeleton,
    st: &'a dyn Works,
    pal: Palette,
    ground: Ground,
    land: Land,
}

impl Shop<'_> {
    fn slab(
        &mut self,
        c: Vec2,
        half: Vec2,
        y: (f32, f32),
        color: Vec3,
        mat: Material,
        solid: bool,
    ) {
        slab(self.map, c, half, y, color, mat, solid);
    }

    fn beam(&mut self, a: Vec3, b: Vec3, r: f32) {
        let (c, m) = (self.pal.steel, self.pal.steel_mat);
        self.map.segment(a, b, r, c, m, false);
    }

    /// The halls: floors, walls, lintels over the doors, girders, roofs.
    fn halls(&mut self, rng: &mut Rng) {
        let sk = self.sk;
        for hall in &sk.halls {
            let (floor, dark) = (self.pal.floor, self.pal.dark);
            let (cells, inner) = ((hall.half * 2.0 / 8.0).round(), hall.half * 2.0);
            let tile = inner / cells;
            for i in 0..cells.x as i32 {
                for j in 0..cells.y as i32 {
                    let c = hall.center - hall.half + tile * (Vec2::new(i as f32, j as f32) + 0.5);
                    let shade = 0.9 + 0.1 * noise2(c.x, c.y, 213);
                    self.slab(
                        c,
                        tile * 0.5 - Vec2::splat(0.05),
                        (-0.2, 0.02),
                        floor * shade,
                        Material::Concrete,
                        false,
                    );
                }
            }
            for (k, w) in hall.panels.iter().enumerate() {
                self.panel(w, k, rng);
            }
            for d in &hall.doors {
                let (o, c) = (out(d.side), hall.door(d));
                let along = o.perp().abs();
                let half = along * d.half + o.abs() * WALL_T * 0.5;
                let (wall, mat, trim) = (self.pal.wall, self.pal.wall_mat, self.pal.trim);
                self.slab(c, half, (DOOR_H + 0.7, hall.h), wall, mat, true);
                // A painted girder over the way through, posts at its sides.
                self.slab(
                    c,
                    half + o.abs() * 0.2 + along * 0.4,
                    (DOOR_H, DOOR_H + 0.7),
                    trim,
                    Material::Metal,
                    true,
                );
                for s in [-1.0f32, 1.0] {
                    self.slab(
                        c + along * s * (d.half + 0.3),
                        along * 0.28 + o.abs() * (WALL_T * 0.5 + 0.22),
                        (-0.2, DOOR_H + 0.2),
                        dark,
                        Material::Concrete,
                        false,
                    );
                }
                let lamp = c + o * (WALL_T * 0.5 + 0.35);
                let molten = self.pal.molten;
                self.slab(
                    lamp,
                    Vec2::splat(0.25),
                    (DOOR_H + 0.15, DOOR_H + 0.55),
                    molten,
                    Material::Glow,
                    false,
                );
            }
            self.roof(hall);
        }
    }

    fn panel(&mut self, w: &Panel, k: usize, rng: &mut Rng) {
        let along = w.along();
        let (wall, mat, dark) = (self.pal.wall, self.pal.wall_mat, self.pal.dark);
        self.slab(
            w.center,
            w.half + w.out.abs() * 0.05,
            (-0.3, 1.3),
            dark,
            Material::Concrete,
            true,
        );
        let shade = 0.94 + 0.06 * noise2(w.center.x, w.center.y, 214);
        let o = slab(
            self.map,
            w.center,
            w.half,
            (1.3, w.top),
            wall * shade,
            mat,
            true,
        );
        o.broken = w.broken;
        // A pilaster at its start; a coping over a whole one.
        let rib = along * 0.35 + w.out.abs() * (WALL_T * 0.5 + 0.18);
        self.slab(
            w.center - along * (w.reach() - 0.4),
            rib,
            (-0.2, w.top - 0.1),
            dark * 1.1,
            Material::Concrete,
            false,
        );
        if !w.broken {
            self.slab(
                w.center,
                w.half + w.out.abs() * 0.22,
                (w.top - 0.5, w.top + 0.15),
                dark,
                Material::Concrete,
                false,
            );
            self.st.panel(self.map, w, k);
            return;
        }
        // What fell off it, at its foot.
        for _ in 0..rng.int(3, 6) {
            let s = rng.range(0.3, 0.9);
            let q =
                w.center + along * rng.range(-1.0, 1.0) * w.reach() + w.out * rng.range(-3.2, 3.2);
            self.map.oboxes.push(OBox {
                solid: false,
                tilt: rng.range(-0.3, 0.3),
                ..rocks::rubble(
                    q,
                    rng.range(0.0, TAU),
                    Vec2::new(s, s * 0.7),
                    -0.25,
                    s * 0.7,
                    wall * rng.range(0.75, 0.95),
                )
            });
        }
    }

    /// Girders over a hall at each bay, the roof on the bays that still
    /// have it.
    fn roof(&mut self, hall: &Hall) {
        let n = hall.bays.len();
        let seg = (hall.half.x - WALL_T) * 2.0 / n as f32;
        let x0 = hall.center.x - hall.half.x + WALL_T;
        let (z, y) = (hall.half.y - WALL_T, hall.h - 0.7);
        for k in 1..n {
            let x = x0 + seg * k as f32;
            let p = |t: f32, y: f32| Vec3::new(x, y, hall.center.y + t * z);
            self.beam(p(-1.0, y), p(1.0, y), 0.22);
            self.beam(p(-0.9, y - 1.8), p(0.9, y - 1.8), 0.14);
            let m = 8;
            for j in 0..m {
                let t = -0.9 + 1.8 * j as f32 / m as f32;
                let u = t + 1.8 / m as f32;
                let (a, b) = if j % 2 == 0 {
                    (y - 1.8, y)
                } else {
                    (y, y - 1.8)
                };
                self.beam(p(t, a), p(u, b), 0.08);
            }
        }
        for (k, &whole) in hall.bays.iter().enumerate() {
            if whole {
                let a = x0 + seg * k as f32;
                self.st.bay(self.map, hall, a, a + seg);
            }
        }
        self.st.gables(self.map, hall);
    }

    /// A furnace, the runner from its mouth down the casting floor to a bed
    /// of moulds, its light.
    fn furnace(&mut self, f: &Round, rng: &mut Rng) {
        round_solid(self.map, f.at, f.r, (-0.3, f.h));
        self.st.furnace(self.map, f, rng);
        let molten = self.pal.molten;
        let floor = self.sk.hold.y;
        let end = floor - 1.0 + (f.at.x - self.sk.hold.x).abs() * 0.1;
        let n = 10;
        let points: Vec<Vec3> = (0..=n)
            .map(|k| {
                let t = k as f32 / n as f32;
                let z = (f.at.y - f.r - 0.1) + (end - (f.at.y - f.r - 0.1)) * t;
                Vec3::new(f.at.x + (t * 7.0).sin() * 0.25, 0.06, z)
            })
            .collect();
        self.map.glow_paths.push(GlowPath {
            widths: vec![0.45; points.len()],
            points,
            color: molten,
        });
        // The moulds: hulls' undersides, filling.
        for s in [-1.0f32, 0.0, 1.0] {
            let c = Vec2::new(f.at.x + s * 2.6, end - 1.6);
            self.map.glow_paths.push(GlowPath {
                points: vec![at(c - Vec2::Y * 1.5, 0.05), at(c + Vec2::Y * 1.5, 0.05)],
                widths: vec![0.8, 0.8],
                color: molten * 0.6,
            });
        }
        self.map.glow_paths.push(GlowPath {
            points: vec![
                Vec3::new(f.at.x - 3.0, 0.07, end),
                Vec3::new(f.at.x + 3.0, 0.07, end),
            ],
            widths: vec![0.35; 2],
            color: molten,
        });
        let mouth = at(f.at - Vec2::Y * (f.r + 1.2), 2.2);
        self.map.lights.push(PointLight {
            pos: mouth,
            color: molten * 9.0,
            radius: 17.0,
        });
        self.map.lights.push(PointLight {
            pos: at(Vec2::new(f.at.x, end), 1.6),
            color: molten * 4.0,
            radius: 10.0,
        });
        self.map.sound(Ambient::Furnace, mouth);
        // The flue, up and over the wall to its chimney.
        let top = f.h + 1.5;
        let stack = Vec2::new(
            f.at.x,
            self.sk.halls[0].center.y + self.sk.halls[0].half.y + 3.6,
        );
        let (steel, m) = (self.pal.steel * 0.8, self.pal.steel_mat);
        self.map
            .pipe(at(f.at, f.h - 0.6), at(f.at, top), 0.75, steel, m, false);
        self.map
            .pipe(at(f.at, top), at(stack, top + 1.5), 0.7, steel, m, false);
    }

    /// The great hall's travelling crane: a bridge across it, a ladle hung
    /// over the casting floor, brim full.
    fn crane(&mut self) {
        let hall = &self.sk.halls[0];
        let hold = self.sk.hold;
        let x = hold.x + hall.half.x * 0.24;
        let y = hall.h - 4.2;
        let z = hall.half.y - WALL_T;
        for s in [-1.0f32, 1.0] {
            // The rails it runs on, along both walls.
            let w = hall.center.y + s * (z - 0.5);
            let l = hall.half.x - WALL_T;
            self.beam(
                Vec3::new(hall.center.x - l, y - 0.7, w),
                Vec3::new(hall.center.x + l, y - 0.7, w),
                0.22,
            );
            self.beam(
                Vec3::new(x + s * 1.1, y, hall.center.y - z + 0.3),
                Vec3::new(x + s * 1.1, y, hall.center.y + z - 0.3),
                0.4,
            );
        }
        let c = Vec2::new(x, hold.y + 1.0);
        let (trim, dark) = (self.pal.trim, self.pal.dark);
        self.slab(
            c,
            Vec2::new(1.8, 1.3),
            (y + 0.3, y + 1.5),
            trim,
            Material::Painted,
            false,
        );
        for s in [-1.0f32, 1.0] {
            let q = c + Vec2::X * s * 1.3;
            self.map.segment(
                at(q, 8.6),
                at(q, y + 0.4),
                0.06,
                dark,
                Material::Metal,
                false,
            );
        }
        self.beam(at(c - Vec2::X * 1.8, 8.6), at(c + Vec2::X * 1.8, 8.6), 0.2);
        for s in [-1.0f32, 1.0] {
            let q = c + Vec2::X * s * 1.65;
            self.beam(at(q, 6.4), at(q, 8.6), 0.1);
        }
        let steel = self.pal.steel * 0.7;
        self.map
            .pipe(at(c, 5.4), at(c, 7.6), 1.5, steel, Material::Metal, false);
        self.map
            .disc(at(c, 5.45), Vec3::Y * 0.05, 1.35, steel, Material::Metal);
        let molten = self.pal.molten;
        self.map
            .disc(at(c, 7.3), Vec3::Y * 0.04, 1.35, molten, Material::Glow);
        self.map.lights.push(PointLight {
            pos: at(c, 8.2),
            color: molten * 4.0,
            radius: 9.0,
        });
        self.map.sound(Ambient::Machine, at(c, 6.0));
    }

    /// A gantry: its runway, legs and bracing, the hoist and what hangs
    /// from it; its beam's the style's.
    fn gantry(&mut self, g: &Gantry) {
        let (steel, mat, dark, trim) = (
            self.pal.steel,
            self.pal.steel_mat,
            self.pal.dark,
            self.pal.trim,
        );
        for l in g.legs() {
            // The runway's rail, flush with the ground.
            self.slab(
                l,
                Vec2::new(g.run, 0.12),
                (-0.2, 0.06),
                RAIL,
                Material::Metal,
                false,
            );
            // The bogie, and a pair of posts leaning in to the top.
            self.slab(l, LEG, (0.15, 1.3), trim, Material::Painted, true);
            for s in [-1.0f32, 1.0] {
                let foot = l + Vec2::X * s * (LEG.x - 0.35);
                let head = l + Vec2::X * s * 0.7;
                self.map
                    .segment(at(foot, 1.2), at(head, g.h + 0.2), 0.3, steel, mat, true);
                let wheel = l + Vec2::X * s * (LEG.x - 0.55);
                self.map.disc(
                    at(wheel, 0.42),
                    Vec3::Z * (LEG.y - 0.1),
                    0.36,
                    dark,
                    Material::Metal,
                );
            }
            for (a, b) in [(1.3, 4.6), (4.6, 8.0)] {
                let k = |y: f32| 1.2 - 0.5 * y / g.h;
                self.beam(at(l - Vec2::X * k(a), a), at(l + Vec2::X * k(b), b), 0.1);
                self.beam(at(l - Vec2::X * k(b), b), at(l + Vec2::X * k(b), b), 0.13);
            }
        }
        self.st.gantry(self.map, g);
        let c = g.at + Vec2::Y * g.span * g.hoist;
        self.slab(
            c,
            Vec2::new(1.9, 1.4),
            (g.h + 1.65, g.h + 3.0),
            trim,
            Material::Painted,
            false,
        );
        let hook = g.h * 0.55;
        for s in [-1.0f32, 1.0] {
            let q = c + Vec2::X * s * 0.35;
            self.map.segment(
                at(q, hook),
                at(q, g.h + 1.7),
                0.05,
                dark,
                Material::Metal,
                false,
            );
        }
        self.slab(
            c,
            Vec2::splat(0.5),
            (hook - 0.6, hook),
            trim,
            Material::Painted,
            false,
        );
        self.map.sound(Ambient::Machine, at(c, g.h * 0.5));
    }

    /// Rails on sleepers, out past the edge both ways.
    fn tracks(&mut self) {
        let size = self.sk.size;
        for &z in &self.sk.tracks {
            let (from, to) = (-RUN_ON, size + RUN_ON);
            let n = ((to - from) / 1.8) as usize;
            for k in 0..n {
                let x = from + 1.8 * (k as f32 + 0.5);
                self.slab(
                    Vec2::new(x, z),
                    Vec2::new(0.17, GAUGE + 0.65),
                    (-0.15, 0.07),
                    TIMBER,
                    Material::Wood,
                    false,
                );
            }
            for s in [-1.0f32, 1.0] {
                let mut x = from;
                while x < to {
                    let half = ((to - x) * 0.5).min(10.0);
                    self.slab(
                        Vec2::new(x + half, z + s * GAUGE),
                        Vec2::new(half, 0.07),
                        (0.0, 0.2),
                        RAIL,
                        Material::Metal,
                        false,
                    );
                    x += 20.0;
                }
            }
        }
    }

    /// A wagon: a frame on two bogies, a stake-sided tub, castings or slag
    /// in it.
    fn wagon(&mut self, w: &Wagon, rng: &mut Rng) {
        let (paint, dark) = (self.pal.wagon * rng.range(0.85, 1.1), self.pal.dark);
        let c = w.at;
        // All that stops a tank.
        slab(
            self.map,
            c,
            WAGON,
            (-0.1, WAGON_H),
            Vec3::ZERO,
            Material::Plain,
            true,
        )
        .hidden = true;
        self.slab(
            c,
            Vec2::new(WAGON.x - 0.1, 0.9),
            (0.75, 1.1),
            dark,
            Material::Metal,
            false,
        );
        for s in [-1.0f32, 1.0] {
            for t in [-0.65f32, 0.65] {
                let p = c + Vec2::X * (s * 2.9 + t);
                self.map.disc(
                    at(p, 0.55),
                    Vec3::Z * (GAUGE + 0.1),
                    0.42,
                    dark * 0.8,
                    Material::Metal,
                );
            }
            // Buffers, the tub's ends.
            for t in [-0.8f32, 0.8] {
                self.slab(
                    c + Vec2::new(s * (WAGON.x + 0.1), t),
                    Vec2::new(0.3, 0.16),
                    (0.8, 1.1),
                    dark,
                    Material::Metal,
                    false,
                );
            }
            self.slab(
                c + Vec2::X * s * (WAGON.x - 0.2),
                Vec2::new(0.1, WAGON.y - 0.08),
                (1.1, 2.6),
                paint,
                Material::Painted,
                false,
            );
            let side = c + Vec2::Y * s * (WAGON.y - 0.12);
            self.slab(
                side,
                Vec2::new(WAGON.x - 0.1, 0.08),
                (1.1, 2.45),
                paint,
                Material::Painted,
                false,
            );
            for k in 0..5 {
                let x = -3.8 + 1.9 * k as f32;
                self.slab(
                    side + Vec2::new(x, s * 0.08),
                    Vec2::new(0.09, 0.07),
                    (0.9, 2.65),
                    dark,
                    Material::Metal,
                    false,
                );
            }
        }
        self.slab(
            c,
            Vec2::new(WAGON.x - 0.3, WAGON.y - 0.2),
            (1.08, 1.2),
            dark,
            Material::Metal,
            false,
        );
        if !w.loaded {
            return;
        }
        if rng.chance(0.5) {
            for s in [-1.0f32, 1.0] {
                self.casting(c + Vec2::X * s * 2.1, 1.2, 0.58, false);
            }
        } else {
            for k in 0..7 {
                let p = c + Vec2::new(-3.3 + 1.1 * k as f32, rng.range(-0.3, 0.3));
                let s = rng.range(0.7, 0.95);
                self.map.oboxes.push(OBox {
                    solid: false,
                    ..rocks::rubble(
                        p,
                        rng.range(0.0, TAU),
                        Vec2::new(s, s * 0.8),
                        1.1,
                        1.1 + s * 1.8,
                        self.land.slag * rng.range(1.2, 1.6),
                    )
                });
            }
        }
    }

    /// A hull as it comes out of the mould, on `y`, `k` full size: the tub,
    /// its glacis, the sponsons, the turret ring.
    fn casting(&mut self, c: Vec2, y: f32, k: f32, solid: bool) {
        let color = self.pal.casting;
        let m = Material::Metal;
        self.slab(
            c,
            Vec2::new(2.5, 1.1) * k,
            (y, y + 1.15 * k),
            color,
            m,
            solid,
        );
        for s in [-1.0f32, 1.0] {
            self.slab(
                c + Vec2::Y * s * 1.4 * k,
                Vec2::new(2.95, 0.38) * k,
                (y + 0.25 * k, y + 1.0 * k),
                color * 0.8,
                m,
                solid,
            );
        }
        let rig = Rig {
            pos: at(c, y),
            rot: Quat::IDENTITY,
        };
        for (x, turn) in [(2.45, -0.75), (-2.5, 0.95)] {
            rig.part(
                self.map,
                Vec3::new(x, 0.62, 0.0) * k,
                Vec3::new(0.62, 0.16, 1.08) * k,
                Quat::from_rotation_z(turn),
                color * 0.92,
                m,
                false,
                0.0,
            );
        }
        self.map.pipe(
            at(c - Vec2::X * 0.2 * k, y + 1.05 * k),
            at(c - Vec2::X * 0.2 * k, y + 1.4 * k),
            0.85 * k,
            color * 0.85,
            m,
            false,
        );
    }

    fn stock(&mut self, s: &Stock, rng: &mut Rng) {
        let g = self.sk.ground(s.at);
        let dark = self.pal.dark;
        match s.kind {
            StockKind::Hull => {
                // On a cradle.
                for x in [-1.6f32, 1.6] {
                    self.slab(
                        s.at + Vec2::X * x,
                        Vec2::new(0.35, 1.8),
                        (g - 0.6, g + 0.7),
                        dark,
                        Material::Wood,
                        true,
                    );
                }
                self.casting(s.at, g + 0.7, 1.0, true);
            }
            StockKind::Billets => self.st.billets(self.map, s, g, rng),
            StockKind::Pot => {
                let steel = self.pal.steel * rng.range(0.6, 0.8);
                round_solid(self.map, s.at, 1.65, (g - 0.6, g + 2.3));
                self.map.pipe(
                    at(s.at, g - 0.5),
                    at(s.at, g + 1.0),
                    1.35,
                    steel,
                    Material::Rust,
                    false,
                );
                self.map.pipe(
                    at(s.at, g + 1.0),
                    at(s.at, g + 2.3),
                    1.65,
                    steel,
                    Material::Rust,
                    false,
                );
                // Some still hot.
                let hot = rng.chance(0.35);
                let (c, m) = if hot {
                    (self.pal.molten * 0.7, Material::Glow)
                } else {
                    (self.land.slag, Material::Rubble)
                };
                self.map.disc(at(s.at, g + 2.05), Vec3::Y * 0.05, 1.5, c, m);
            }
        }
    }

    /// Lumps on the slag heaps and round their feet: nothing to stop a
    /// tank.
    fn slag(&mut self, rng: &mut Rng) {
        for s in &self.sk.slag {
            for _ in 0..(s.radius * 2.6) as i32 {
                let p = s.at
                    + Vec2::from_angle(rng.range(0.0, TAU)) * s.radius * rng.f32().sqrt() * 1.1;
                if p.min_element() < 4.0 || p.max_element() > self.sk.size - 4.0 {
                    continue;
                }
                let g = self.sk.ground(p);
                let r = rng.range(0.25, 0.8);
                self.map.oboxes.push(OBox {
                    solid: false,
                    ..rocks::rubble(
                        p,
                        rng.range(0.0, TAU),
                        Vec2::new(r, r * 0.8),
                        g - r * 0.5,
                        g + r * 0.6,
                        self.land.slag * rng.range(0.9, 1.7),
                    )
                });
            }
        }
    }

    /// The works' wall: panels between posts, gates where the tracks run
    /// out.
    fn fence(&mut self) {
        let sk = self.sk;
        let (wall, mat, dark, steel) = (
            self.pal.wall,
            self.pal.wall_mat,
            self.pal.dark,
            self.pal.steel,
        );
        for (k, (c, half, inward, gate)) in sk.fence().into_iter().enumerate() {
            let (y0, y1) = sk.fence_y(c, half);
            let along = inward.perp().abs();
            if gate {
                // Shut.
                self.slab(c, half, (y0, y1), steel, Material::Rust, true);
                for t in [0.3, 0.7] {
                    let y = y0 + (y1 - y0) * t;
                    self.slab(
                        c,
                        half + inward.abs() * 0.12,
                        (y, y + 0.3),
                        dark,
                        Material::Metal,
                        false,
                    );
                }
            } else {
                self.slab(c, half, (y0, y1), wall * 0.82, mat, true);
                self.slab(
                    c,
                    half + inward.abs() * 0.15,
                    (y1 - 0.1, y1 + 0.3),
                    dark,
                    Material::Concrete,
                    false,
                );
            }
            let post = c - along * half.dot(along);
            self.slab(
                post,
                Vec2::splat(FENCE_T + 0.25),
                (y0, y1 + 0.7),
                dark * 1.1,
                Material::Concrete,
                false,
            );
            if !gate {
                self.st.fence(self.map, c, inward, y1, k);
            }
        }
    }

    /// Each side's ground: a cast pad, its stores stacked behind it, its
    /// colours flying.
    fn bases(&mut self) {
        let sk = self.sk;
        for (side, &b) in sk.bases.iter().enumerate() {
            let Some(&who) = self.map.factions.get(side) else {
                continue;
            };
            let floor = self.pal.floor;
            for i in 0..5 {
                for j in 0..5 {
                    let c = b + Vec2::new(i as f32 - 2.0, j as f32 - 2.0) * 6.0;
                    self.slab(
                        c,
                        Vec2::splat(2.92),
                        (-0.2, 0.02),
                        floor * (0.86 + 0.04 * ((i * 3 + j * 5) % 3) as f32),
                        Material::Concrete,
                        false,
                    );
                }
            }
            let back = if b.x < sk.size * 0.5 { -1.0 } else { 1.0 };
            let color = who.paint() * 0.8 + Vec3::splat(0.05);
            for (z, y) in [(-6.5, 0.0), (0.0, 0.0), (6.5, 0.0), (-3.2, 2.5)] {
                self.slab(
                    b + Vec2::new(back * 18.2, z),
                    Vec2::new(1.25, 2.9),
                    (y - 0.1, y + 2.4),
                    color,
                    Material::Painted,
                    true,
                );
            }
            for s in [-1.0f32, 1.0] {
                let p = b + Vec2::new(back * 14.5, s * 15.0);
                let dark = self.pal.dark;
                self.map.pipe(
                    at(p, -0.2),
                    at(p, 11.0),
                    0.16,
                    dark,
                    Material::Painted,
                    true,
                );
                let face = Vec2::X * -back;
                self.map.side_banner(
                    at(p + face * 0.1, 10.7),
                    face,
                    2.4,
                    6.0,
                    who.paint(),
                    who == crate::faction::Faction::Imperium,
                );
            }
        }
    }

    /// What the land's put there itself, inside the wall: stones and drifted
    /// snow on tundra, tufts on grassland. Nothing that stops a tank.
    fn growth(&mut self, rng: &mut Rng) {
        let (n, color) = match self.ground {
            Ground::Desert => (90, self.land.rock),
            Ground::Tundra => (200, self.land.open),
            Ground::Grassland => (900, self.land.open),
        };
        for _ in 0..n {
            let s = self.sk.size;
            let p = Vec2::new(rng.range(4.0, s - 4.0), rng.range(4.0, s - 4.0));
            let g = self.sk.ground(p);
            let paved = self.sk.halls.iter().any(|h| h.contains(p, 2.0))
                || self
                    .sk
                    .bases
                    .iter()
                    .any(|b| (*b - p).abs().max_element() < PAD)
                || self.sk.tracks.iter().any(|z| (z - p.y).abs() < 2.5)
                || g > 0.3;
            if paved {
                continue;
            }
            let yaw = rng.range(0.0, TAU);
            match self.ground {
                Ground::Desert => {
                    let r = rng.range(0.2, 0.55);
                    let o = rocks::boulder(p, yaw, Vec2::splat(r), g - r * 0.5, g + r * 0.7, color);
                    self.map.oboxes.push(OBox { solid: false, ..o });
                }
                Ground::Tundra => {
                    // A low bank of snow.
                    let half = Vec2::new(rng.range(1.2, 3.2), rng.range(0.8, 1.6));
                    let o = rocks::boulder(p, yaw, half, g - 0.6, g + rng.range(0.12, 0.35), color);
                    self.map.oboxes.push(OBox { solid: false, ..o });
                }
                Ground::Grassland => {
                    for k in 0..4 {
                        let a = yaw + k as f32 * 1.7;
                        let lean = Vec2::from_angle(a) * rng.range(0.1, 0.3);
                        let h = rng.range(0.3, 0.7);
                        self.map.segment(
                            at(p, g - 0.05),
                            at(p + lean, g + h),
                            0.035,
                            color * rng.range(0.8, 1.35),
                            Material::Grass,
                            false,
                        );
                    }
                }
            }
        }
    }

    /// A tree past the wall: a trunk and a few rounds of leaf.
    fn tree(&mut self, rng: &mut Rng, p: Vec2, g: f32) {
        let h = rng.range(4.0, 7.0);
        self.map.segment(
            at(p, g - 1.0),
            at(p, g + h),
            0.3,
            TIMBER,
            Material::Wood,
            false,
        );
        for _ in 0..4 {
            let r = rng.range(1.5, 2.7);
            let c = p + Vec2::from_angle(rng.range(0.0, TAU)) * rng.range(0.0, 1.6);
            let y = g + h * rng.range(0.75, 1.1);
            let color = Vec3::new(0.11, 0.18, 0.06) * rng.range(0.8, 1.4);
            let o = rocks::boulder(
                c,
                rng.range(0.0, TAU),
                Vec2::new(r, r * 0.9),
                y - r * 0.7,
                y + r * 0.8,
                color,
            );
            self.map.oboxes.push(OBox { solid: false, ..o });
        }
    }

    /// Past the edge: the land rising into hills, level along the tracks;
    /// more of the works, seen from afar.
    fn beyond(&mut self, rng: &mut Rng) {
        let sk = self.sk;
        let s = sk.size;
        let cell = 6.0f32;
        let n = ((s + 1040.0) / cell).round() as usize + 1;
        let reach = ((n - 1) as f32 * cell - s) * 0.5;
        let land = |x: f32, z: f32| {
            let p = Vec2::new(x, z);
            let q = (p - Vec2::splat(s * 0.5)).abs() - Vec2::splat(s * 0.5);
            let d = q.max(Vec2::ZERO).length() + q.x.max(q.y).min(0.0);
            if d < -0.01 {
                return -10.0;
            }
            let base =
                sk.ground(p.clamp(Vec2::ZERO, Vec2::splat(s))) * (1.0 - smoothstep(0.0, 50.0, d));
            let hills = 5.0 * ((x * 0.011).sin() * (z * 0.013).cos()).abs()
                + 4.0 * (1.0 + (x * 0.021 + z * 0.017).sin())
                + 12.0
                    * smoothstep(
                        0.5,
                        0.9,
                        ((x * 0.004 + 1.0).sin() * (z * 0.005).cos()).abs(),
                    );
            let track = sk
                .tracks
                .iter()
                .map(|k| (z - k).abs())
                .fold(f32::MAX, f32::min);
            let level = smoothstep(10.0, 34.0, track).max(smoothstep(RUN_ON, RUN_ON + 90.0, d));
            base + hills * smoothstep(8.0, 90.0, d) * level - 0.06 * smoothstep(0.0, 4.0, d)
        };
        let mut t = Terrain::from_fn(Vec2::splat(-reach), cell, n, n, land);
        t.color = self.land.far;
        t.material = self.land.far_material;
        self.map.scenery_terrain = Some(t);
        let (wall, mat, dark) = (self.pal.wall, self.pal.wall_mat, self.pal.dark);
        let mut put = 0;
        for _ in 0..200 {
            if put >= 16 {
                break;
            }
            let dir = Vec2::from_angle(rng.range(0.0, TAU));
            let edge = s * 0.5 / dir.x.abs().max(dir.y.abs());
            let c = Vec2::splat(s * 0.5) + dir * (edge + rng.range(34.0, 120.0));
            let half = Vec2::new(rng.range(8.0, 18.0), rng.range(7.0, 12.0));
            if sk.tracks.iter().any(|z| (z - c.y).abs() < half.y + 7.0) {
                continue;
            }
            put += 1;
            let g = land(c.x, c.y);
            let h = rng.range(8.0, 14.0);
            self.slab(c, half, (g - 8.0, g + h), wall * 0.8, mat, false);
            self.slab(
                c,
                half + Vec2::splat(0.4),
                (g + h, g + h + 0.8),
                dark,
                Material::Concrete,
                false,
            );
            if rng.chance(0.6) {
                self.st.chimney(
                    self.map,
                    &Round {
                        at: c + Vec2::X * (half.x + 4.0),
                        r: 1.6,
                        h: g + rng.range(20.0, 32.0),
                    },
                );
            }
        }
        if self.ground == Ground::Grassland {
            // Copses on the hills.
            for _ in 0..40 {
                let dir = Vec2::from_angle(rng.range(0.0, TAU));
                let edge = s * 0.5 / dir.x.abs().max(dir.y.abs());
                let c = Vec2::splat(s * 0.5) + dir * (edge + rng.range(22.0, 150.0));
                for _ in 0..rng.int(2, 6) {
                    let p = c + Vec2::new(rng.range(-9.0, 9.0), rng.range(-9.0, 9.0));
                    let q = (p - Vec2::splat(s * 0.5)).abs().max_element() - s * 0.5;
                    let on_track = sk.tracks.iter().any(|z| (z - p.y).abs() < 5.0);
                    if q > 12.0 && !on_track {
                        self.tree(rng, p, land(p.x, p.y));
                    }
                }
            }
        }
        // Buttes on the desert, crags on the tundra, none on grassland.
        let crags = if self.ground == Ground::Grassland {
            0
        } else {
            26
        };
        for _ in 0..crags {
            let dir = Vec2::from_angle(rng.range(0.0, TAU));
            let w = rng.range(12.0, 30.0);
            let edge = s * 0.5 / dir.x.abs().max(dir.y.abs());
            let c = Vec2::splat(s * 0.5) + dir * (edge + w + rng.range(140.0, 300.0));
            let g = land(c.x, c.y) - 2.0;
            self.map.oboxes.push(OBox {
                solid: false,
                broken: true,
                ..rocks::boulder(
                    c,
                    rng.range(0.0, TAU),
                    Vec2::new(w, w * rng.range(0.5, 0.9)),
                    g,
                    g + rng.range(12.0, 36.0),
                    self.land.rock * rng.range(0.85, 1.05),
                )
            });
        }
    }

    /// The places bots head for, the ways round, and the repair drops that
    /// came down clear.
    fn play(&mut self) {
        let sk = self.sk;
        let clear = |m: &Map, p: Vec2| {
            let g = m.ground_height(p.x, p.y);
            !m.oboxes.iter().any(|o| {
                let (lo, hi) = o.y_range();
                o.solid && lo < g + 3.0 && hi > g + 0.3 && o.distance(p) < TANK_CLEAR
            }) && !m
                .crystals
                .iter()
                .any(|c| c.solid && c.distance(p) < TANK_CLEAR)
        };
        for side in 0..self.map.bases.len().min(sk.bases.len()) {
            let mut b = std::mem::take(&mut self.map.bases[side]);
            b.retain(|&p| clear(self.map, p));
            self.map.bases[side] = b;
        }
        let nav = NavGrid::build(self.map);
        let reach = |p: Vec2| !nav.path(sk.bases[0], p).is_empty();
        let n = (sk.size / 10.0) as i32;
        let spots: Vec<Vec2> = (0..n * n)
            .map(|k| Vec2::new((k % n) as f32 + 0.5, (k / n) as f32 + 0.5) * 10.0)
            .filter(|&p| clear(self.map, p) && reach(p))
            .collect();
        self.map.open_spots = spots;
        self.map.repair_spots = sk
            .repair_spots
            .iter()
            .copied()
            .filter(|&p| clear(self.map, p) && reach(p))
            .collect();
        // Each route's points, off whatever's been put down on them.
        let off = |q: Vec3| {
            (0..160).find_map(|k| {
                let p = Vec2::new(q.x, q.z) + Vec2::from_angle(k as f32 * 2.4) * (k as f32 * 0.08);
                let open = !nav.is_blocked(p) && reach(p);
                open.then(|| at(p, sk.ground(p)))
            })
        };
        self.map.routes = sk
            .routes()
            .into_iter()
            .map(|r| r.into_iter().filter_map(off).collect())
            .collect();
    }
}

/// Turned so its local z runs along the map's x.
pub const ALONG_X: f32 = FRAC_PI_2;
