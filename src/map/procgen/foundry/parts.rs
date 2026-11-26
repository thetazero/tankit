//! What a works is made of, each as the solids it stands as: big buildings
//! that are fights of their own, the medium things between them, cover. A
//! style dresses them; what's here is what's driven into, under and round.

use std::f32::consts::{FRAC_PI_4, TAU};

use glam::{Vec2, Vec3};

use super::style::{Palette, Works};
use crate::map::rocks;

use crate::map::{Ambient, GlowPath, Map, OBox};
use crate::render::PointLight;
use crate::render::material::Material;
use crate::rng::Rng;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    // Big.
    /// A furnace hall, open down one side, furnaces in a row.
    Smelter,
    /// A shed on columns, its side walls mostly gone.
    Casting,
    /// A long hall, driven through end to end.
    Mill,
    /// Solid, stepped, chimneys out of it.
    Power,
    /// A hall with a wing off it round a yard.
    Assembly,
    /// A tower of ore bins on legs: driven under.
    Ore,
    /// Two great round towers.
    Cooling,
    // Medium.
    Tanks,
    Bunker,
    WaterTower,
    Office,
    Shed,
    Dock,
    Crane,
    Gatehouse,
    Wall,
    Rack,
    Portal,
    PipeBridge,
    Conveyor,
    Wagons,
    /// Freight cars of any kind, coupled, on the rails.
    Cars,
    /// A travelling crane over tracks: driven under.
    Gantry,
    // Small.
    Hulls,
    Billets,
    Pot,
    Rubble,
}

impl Part {
    /// Along the roads.
    pub const YARD: &[Part] = &[
        Part::Tanks,
        Part::Bunker,
        Part::WaterTower,
        Part::Office,
        Part::Office,
        Part::Shed,
        Part::Shed,
        Part::Gatehouse,
        Part::Wall,
        Part::Wall,
        Part::Rack,
        Part::Crane,
        Part::Hulls,
        Part::Billets,
    ];
    pub const SMALL: &[Part] = &[Part::Hulls, Part::Billets, Part::Pot, Part::Rubble];

    pub fn big(self) -> bool {
        matches!(
            self,
            Part::Smelter
                | Part::Casting
                | Part::Mill
                | Part::Power
                | Part::Assembly
                | Part::Ore
                | Part::Cooling
        )
    }

    /// Floored: driven into.
    fn floored(self) -> bool {
        matches!(
            self,
            Part::Smelter | Part::Casting | Part::Mill | Part::Assembly | Part::Ore | Part::Shed
        )
    }

    /// Stands over what's under it, whatever its level.
    pub fn over(self) -> bool {
        matches!(
            self,
            Part::Portal
                | Part::PipeBridge
                | Part::Conveyor
                | Part::Wagons
                | Part::Cars
                | Part::Gantry
        )
    }

    /// Half how wide and long one is.
    pub fn half(self, rng: &mut Rng) -> Vec2 {
        let (w, l): ((f32, f32), (f32, f32)) = match self {
            Part::Smelter => ((15.0, 18.0), (22.0, 29.0)),
            Part::Casting => ((11.0, 15.0), (16.0, 24.0)),
            Part::Mill => ((8.5, 10.5), (30.0, 44.0)),
            Part::Power => ((9.0, 12.0), (12.0, 17.0)),
            Part::Assembly => ((18.0, 23.0), (18.0, 25.0)),
            Part::Ore => ((7.5, 9.0), (10.0, 18.0)),
            Part::Cooling => ((8.5, 10.0), (19.0, 23.0)),
            Part::Tanks => ((4.0, 5.0), (8.0, 13.0)),
            Part::Bunker => ((3.0, 4.0), (5.0, 9.0)),
            Part::WaterTower => ((2.8, 3.4), (2.8, 3.4)),
            Part::Office => ((3.6, 5.5), (5.0, 9.0)),
            Part::Shed => ((5.5, 7.5), (7.5, 11.0)),
            Part::Dock => ((2.6, 3.6), (7.0, 12.0)),
            Part::Crane => ((6.5, 8.5), (3.0, 4.0)),
            Part::Gatehouse => ((2.4, 3.2), (3.0, 4.0)),
            Part::Wall => ((0.6, 0.7), (5.0, 11.0)),
            Part::Rack => ((1.4, 1.8), (6.0, 12.0)),
            Part::Portal | Part::PipeBridge => ((8.0, 8.0), (1.2, 1.2)),
            Part::Conveyor => ((1.3, 1.3), (10.0, 20.0)),
            Part::Wagons => ((1.5, 1.5), (4.5, 9.0)),
            Part::Cars => ((1.55, 1.55), (5.0, 15.0)),
            Part::Gantry => ((9.0, 11.0), (3.6, 3.6)),
            Part::Hulls => ((1.9, 2.1), (3.2, 3.6)),
            Part::Billets => ((1.4, 2.0), (2.4, 3.2)),
            Part::Pot => ((1.7, 1.7), (1.7, 1.7)),
            Part::Rubble => ((1.5, 2.4), (1.8, 3.0)),
        };
        Vec2::new(rng.range(w.0, w.1), rng.range(l.0, l.1))
    }

    fn color(self) -> Vec3 {
        match self {
            Part::Smelter => Vec3::new(0.75, 0.35, 0.2),
            Part::Casting => Vec3::new(0.7, 0.55, 0.25),
            Part::Mill => Vec3::new(0.35, 0.45, 0.65),
            Part::Power => Vec3::new(0.55, 0.3, 0.55),
            Part::Assembly => Vec3::new(0.3, 0.55, 0.5),
            Part::Ore => Vec3::new(0.5, 0.4, 0.3),
            Part::Cooling => Vec3::new(0.6, 0.65, 0.7),
            Part::Conveyor | Part::Portal | Part::PipeBridge | Part::Crane | Part::Gantry => {
                Vec3::new(0.75, 0.68, 0.3)
            }
            Part::Wagons | Part::Cars => Vec3::new(0.3, 0.35, 0.5),
            p if Part::SMALL.contains(&p) => Vec3::new(0.35, 0.5, 0.35),
            _ => Vec3::new(0.55, 0.55, 0.58),
        }
    }
}

/// What a piece is, for its colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Wall,
    Dark,
    Roof,
    Steel,
    Trim,
    Cast,
    Rust,
}

/// One being put up: its footprint, its ground's height, and its dice (its
/// own, so a part's the same whatever else is on the map). Undressed (no
/// style), it's its solids in plain grey.
pub struct Site<'a> {
    pub map: &'a mut Map,
    f: OBox,
    pub g: f32,
    grey: Vec3,
    pub rng: Rng,
    /// The style's dice: what it draws never shifts what's solid.
    pub dice: Rng,
    st: Option<&'a dyn Works>,
    pub pal: Option<Palette>,
    role: Role,
}

impl Site<'_> {
    /// A point on the site, `y` over its ground.
    pub fn at(&self, x: f32, y: f32, z: f32) -> Vec3 {
        let (ax, az) = self.f.axes();
        let p = self.f.center + ax * x + az * z;
        Vec3::new(p.x, self.g + y, p.y)
    }

    /// A direction on the site, on the ground.
    pub fn dir(&self, x: f32, z: f32) -> Vec2 {
        let (ax, az) = self.f.axes();
        ax * x + az * z
    }

    fn look(&self, role: Role) -> (Vec3, Material) {
        let Some(p) = &self.pal else {
            return (self.grey, Material::Concrete);
        };
        match role {
            Role::Wall => (p.wall, p.wall_mat),
            Role::Dark => (p.dark, Material::Concrete),
            Role::Roof => (p.roof, p.roof_mat),
            Role::Steel => (p.steel, p.steel_mat),
            Role::Trim => (p.trim, Material::Painted),
            Role::Cast => (p.casting, Material::Metal),
            Role::Rust => (p.steel * 0.75, Material::Rust),
        }
    }

    /// A box about (`x`, `z`) on the site, from `y0` to `y1` over its ground.
    fn put(&mut self, x: f32, z: f32, half: (f32, f32), y: (f32, f32)) -> &mut OBox {
        let shade = self.rng.range(0.93, 1.04);
        let (c, m) = self.look(self.role);
        let p = self.at(x, 0.0, z);
        self.map.oboxes.push(OBox {
            center: Vec2::new(p.x, p.z),
            yaw: self.f.yaw,
            half: Vec2::new(half.0, half.1),
            y0: self.g + y.0,
            y1: self.g + y.1,
            color: c * shade,
            side_mat: m,
            top_mat: m,
            ..OBox::default()
        });
        self.map.oboxes.last_mut().unwrap()
    }

    /// Dressing: a box that stops nothing.
    pub fn deck(
        &mut self,
        x: f32,
        z: f32,
        half: (f32, f32),
        y: (f32, f32),
        color: Vec3,
        mat: Material,
    ) -> &mut OBox {
        let p = self.at(x, 0.0, z);
        self.map.oboxes.push(OBox {
            center: Vec2::new(p.x, p.z),
            yaw: self.f.yaw,
            half: Vec2::new(half.0, half.1),
            y0: self.g + y.0,
            y1: self.g + y.1,
            color,
            side_mat: mat,
            top_mat: mat,
            solid: false,
            ..OBox::default()
        });
        self.map.oboxes.last_mut().unwrap()
    }

    /// A beam between two points of the site (x, y, z).
    pub fn beam(&mut self, a: (f32, f32, f32), b: (f32, f32, f32), r: f32, c: Vec3, m: Material) {
        let (p, q) = (self.at(a.0, a.1, a.2), self.at(b.0, b.1, b.2));
        self.map.segment(p, q, r, c, m, false);
    }

    pub fn pipe(&mut self, a: (f32, f32, f32), b: (f32, f32, f32), r: f32, c: Vec3, m: Material) {
        let (p, q) = (self.at(a.0, a.1, a.2), self.at(b.0, b.1, b.2));
        self.map.pipe(p, q, r, c, m, false);
    }

    pub fn disc(&mut self, a: (f32, f32, f32), r: f32, c: Vec3, m: Material) {
        let p = self.at(a.0, a.1, a.2);
        self.map.disc(p, Vec3::Y * 0.05, r, c, m);
    }

    pub fn light(&mut self, a: (f32, f32, f32), color: Vec3, radius: f32) {
        let pos = self.at(a.0, a.1, a.2);
        self.map.lights.push(PointLight { pos, color, radius });
    }

    pub fn glow(&mut self, pts: &[(f32, f32, f32)], w: f32, color: Vec3) {
        let points: Vec<Vec3> = pts.iter().map(|p| self.at(p.0, p.1, p.2)).collect();
        self.map.glow_paths.push(GlowPath {
            widths: vec![w; points.len()],
            points,
            color,
        });
    }

    /// Solid and round: two boxes, an eighth apart. Dressed, they're unseen
    /// and `skin` (a round pipe) is what shows, unless the style has it.
    fn round(&mut self, x: f32, z: f32, r: f32, y: (f32, f32), skin: bool) {
        let h = r * std::f32::consts::FRAC_1_SQRT_2;
        let hide = self.st.is_some();
        self.put(x, z, (h, h), y).hidden = hide;
        let o = self.put(x, z, (h, h), y);
        o.yaw += FRAC_PI_4;
        o.y1 -= 0.1;
        o.hidden = hide;
        if hide && skin {
            let (c, m) = self.look(self.role);
            self.pipe((x, y.0, z), (x, y.1, z), r, c, m);
            self.disc((x, y.1 - 0.12, z), r * 0.95, c * 0.85, m);
        }
    }

    fn furnace(&mut self, x: f32, z: f32, r: f32, h: f32) {
        self.role = Role::Steel;
        self.round(x, z, r, (-0.5, h), false);
        let Some(st) = self.st else { return };
        st.furnace(self, x, z, r, h);
        let molten = self.pal.as_ref().map_or(Vec3::ONE, |p| p.molten);
        // A runner out along the floor to moulds.
        let end = x + r + 8.0;
        let mid = (x + r + end) * 0.5;
        self.glow(
            &[(x + r + 0.1, 0.07, z), (mid, 0.07, z + 0.3), (end, 0.07, z)],
            0.4,
            molten,
        );
        for s in [-1.0f32, 1.0] {
            let c = z + s * 2.0;
            self.glow(
                &[(end - 1.4, 0.05, c), (end + 1.4, 0.05, c)],
                0.75,
                molten * 0.6,
            );
        }
        self.light((x + r + 1.2, 2.2, z), molten * 8.0, 16.0);
        let p = self.at(x + r, 2.0, z);
        self.map.sound(Ambient::Furnace, p);
    }

    fn chimney(&mut self, x: f32, z: f32, r: f32, y: (f32, f32)) {
        self.role = Role::Wall;
        match self.st {
            None => self.round(x, z, r, y, false),
            Some(st) => st.chimney(self, x, z, r, y),
        }
    }

    /// A wall along z (else x), from `a` to `b`, `at` across, cut by `gates`
    /// (where, half how wide), some panels broken down; lintels over. Its
    /// outside's toward `at`'s sign.
    fn wall(&mut self, along_z: bool, at: f32, (a, b): (f32, f32), h: f32, gates: &[(f32, f32)]) {
        let mut gates = gates.to_vec();
        gates.sort_by(|p, q| p.0.total_cmp(&q.0));
        let spot = |t: f32| if along_z { (at, t) } else { (t, at) };
        let across = |l: f32, w: f32| if along_z { (w, l) } else { (l, w) };
        let mut from = a;
        let mut count = 0;
        for &(c, w) in gates.iter().chain([&(b + 100.0, 100.0)]) {
            let to = (c - w).min(b);
            let n = ((to - from) / 6.0).round().max(1.0);
            for k in 0..n as i32 {
                if to - from < 0.3 {
                    break;
                }
                let l = (to - from) / n * 0.5;
                let m = from + l * (2.0 * k as f32 + 1.0);
                let broken = self.rng.chance(0.14);
                let top = if broken {
                    self.rng.range(1.2, h * 0.6)
                } else {
                    h
                };
                self.role = Role::Wall;
                let (x, z) = spot(m);
                // (Each a hair thicker or thinner than the next.)
                let thick = 0.53 + 0.02 * (count % 3) as f32;
                self.put(x, z, across(l, thick), (-1.2, top)).broken = broken;
                if self.st.is_some() {
                    self.panel(along_z, at, m, l, top, broken, count);
                }
                count += 1;
            }
            if c < b && h > 8.2 {
                let (x, z) = spot(c);
                self.role = Role::Wall;
                self.put(x, z, across(w, 0.55), (7.6, h));
                self.role = Role::Trim;
                self.put(x, z, across(w + 0.3, 0.83), (7.0, 7.6));
                if let Some(p) = &self.pal {
                    let molten = p.molten;
                    let side = at.signum() * 0.95;
                    let (x, z) = if along_z {
                        (at + side, c)
                    } else {
                        (c, at + side)
                    };
                    self.deck(x, z, (0.22, 0.22), (7.1, 7.5), molten, Material::Glow);
                }
            }
            from = c + w;
            if from >= b {
                break;
            }
        }
    }

    /// A wall panel's dressing: plinth, pilaster, coping, then the style's.
    #[allow(clippy::too_many_arguments)]
    fn panel(&mut self, along_z: bool, at: f32, m: f32, l: f32, top: f32, broken: bool, k: usize) {
        let (dark, wall) = match &self.pal {
            Some(p) => (p.dark, p.wall),
            None => return,
        };
        let spot = |t: f32| if along_z { (at, t) } else { (t, at) };
        let across = |l: f32, w: f32| if along_z { (w, l) } else { (l, w) };
        let cast = Material::Concrete;
        let (x, z) = spot(m);
        if top > 1.6 {
            self.deck(x, z, across(l - 0.07, 0.63), (-1.1, 1.0), dark, cast);
        }
        let (px, pz) = spot(m - l + 0.35);
        self.deck(
            px,
            pz,
            across(0.32, 0.74),
            (-1.0, top - 0.15),
            dark * 1.1,
            cast,
        );
        if !broken {
            self.deck(
                x,
                z,
                across(l - 0.05, 0.78),
                (top - 0.45, top + 0.14),
                dark,
                cast,
            );
            if let Some(st) = self.st {
                let w = Panel {
                    along_z,
                    at,
                    mid: m,
                    reach: l,
                    top,
                };
                st.panel(self, &w, k);
            }
            return;
        }
        for _ in 0..4 {
            let s = self.dice.range(0.3, 0.8);
            let off = self.dice.range(-2.6, 2.6);
            let (x, z) = spot(m + self.dice.range(-1.0, 1.0) * l);
            let p = if along_z {
                self.at(x + off, 0.0, z)
            } else {
                self.at(x, 0.0, z + off)
            };
            let yaw = self.dice.range(0.0, TAU);
            self.map.oboxes.push(OBox {
                solid: false,
                ..rocks::rubble(
                    Vec2::new(p.x, p.z),
                    yaw,
                    Vec2::new(s, s * 0.7),
                    p.y - 0.25,
                    p.y + s * 0.7,
                    wall * 0.85,
                )
            });
        }
    }

    /// A roof over `half` about (`x`, `z`), in bays along z, some gone;
    /// trusses under it.
    fn roof(&mut self, x: f32, z: f32, half: (f32, f32), y: f32, gone: f32) {
        let n = (half.1 * 2.0 / 7.0).round().max(1.0);
        let l = half.1 / n;
        for k in 0..n as i32 {
            let c = z - half.1 + l * (2.0 * k as f32 + 1.0);
            if k > 0
                && let Some(p) = &self.pal
            {
                let (c0, steel, m) = (c - l, p.steel, p.steel_mat);
                let (x0, x1) = (x - half.0 + 0.3, x + half.0 - 0.3);
                self.beam((x0, y - 0.5, c0), (x1, y - 0.5, c0), 0.2, steel, m);
                self.beam((x0, y - 1.9, c0), (x1, y - 1.9, c0), 0.13, steel, m);
                let w = (x1 - x0) / 6.0;
                for j in 0..6 {
                    let (u, v) = (x0 + w * j as f32, x0 + w * (j + 1) as f32);
                    let (p, q) = if j % 2 == 0 {
                        (y - 1.9, y - 0.5)
                    } else {
                        (y - 0.5, y - 1.9)
                    };
                    self.beam((u, p, c0), (v, q, c0), 0.08, steel, m);
                }
            }
            if self.rng.chance(gone) {
                continue;
            }
            self.role = Role::Roof;
            // (The style's own shape stands on it.)
            self.put(x, c, (half.0, l - 0.05), (y, y + 0.28));
            if let Some(st) = self.st {
                st.bay(self, x, c, (half.0, l - 0.05), y + 0.28);
            }
        }
    }

    /// Posts at the corners of `half` about the middle, from well into the
    /// ground to `h`, braced across.
    fn legs(&mut self, half: (f32, f32), r: f32, h: f32) {
        self.role = Role::Steel;
        for sx in [-1.0f32, 1.0] {
            for sz in [-1.0f32, 1.0] {
                self.put(sx * half.0, sz * half.1, (r, r), (-1.5, h));
            }
        }
        if let Some(p) = &self.pal {
            let (c, m) = (p.steel, p.steel_mat);
            for sx in [-1.0f32, 1.0] {
                let (x, z) = (sx * half.0, half.1);
                self.beam((x, h * 0.45, -z), (x, h * 0.95, z), 0.1, c, m);
                self.beam((x, h * 0.95, -z), (x, h * 0.45, z), 0.1, c, m);
                self.beam((x, h * 0.45, -z), (x, h * 0.45, z), 0.12, c, m);
            }
        }
    }

    /// Windows up a block's four sides.
    fn windows(&mut self, x: f32, z: f32, (w, l): (f32, f32), (y0, y1): (f32, f32)) {
        let Some(p) = &self.pal else { return };
        let (lit, dark) = (p.molten * 0.5, Vec3::new(0.07, 0.08, 0.1));
        let floors = ((y1 - y0) / 3.4).floor() as i32;
        for f in 0..floors {
            let y = y0 + 3.4 * f as f32 + 1.3;
            for (along_z, half, out) in [(true, l, w), (false, w, l)] {
                let n = (half / 1.4) as i32;
                for k in 0..n {
                    let t = (k as f32 - (n - 1) as f32 * 0.5) * 2.5;
                    let on = self.dice.chance(0.2);
                    let (c, m) = if on {
                        (lit, Material::Glow)
                    } else {
                        (dark, Material::Metal)
                    };
                    for e in [-1.0f32, 1.0] {
                        let (px, pz, h) = if along_z {
                            (x + e * out, z + t, (0.06, 0.6))
                        } else {
                            (x + t, z + e * out, (0.6, 0.06))
                        };
                        self.deck(px, pz, h, (y, y + 1.3), c, m);
                    }
                }
            }
        }
    }

    /// Their floor, on the ground.
    fn floor(&mut self) {
        let Some(p) = &self.pal else { return };
        let (c, (w, l)) = (p.floor, (self.f.half.x, self.f.half.y));
        let (nx, nz) = ((w / 4.0).round().max(1.0), (l / 4.0).round().max(1.0));
        for i in 0..nx as i32 {
            for j in 0..nz as i32 {
                let x = -w + w / nx * (2.0 * i as f32 + 1.0);
                let z = -l + l / nz * (2.0 * j as f32 + 1.0);
                let shade = self.dice.range(0.9, 1.03);
                self.deck(
                    x,
                    z,
                    (w / nx - 0.05, l / nz - 0.05),
                    (-0.3, 0.03),
                    c * shade,
                    Material::Concrete,
                );
            }
        }
    }
}

/// A stretch of a hall's wall: along z (else x), `at` across, `reach` m
/// either side of `mid`, standing to `top`.
pub struct Panel {
    pub along_z: bool,
    pub at: f32,
    pub mid: f32,
    pub reach: f32,
    pub top: f32,
}

impl Panel {
    /// The place `t` along it from its middle, `off` outward from its middle.
    pub fn spot(&self, t: f32, off: f32) -> (f32, f32) {
        let a = self.at + off * self.at.signum();
        if self.along_z {
            (a, self.mid + t)
        } else {
            (self.mid + t, a)
        }
    }

    /// Half-sizes `l` along it, `w` across.
    pub fn half(&self, l: f32, w: f32) -> (f32, f32) {
        if self.along_z { (w, l) } else { (l, w) }
    }

    /// Outward, on the site.
    pub fn out(&self) -> (f32, f32) {
        let s = self.at.signum();
        if self.along_z { (s, 0.0) } else { (0.0, s) }
    }
}

/// Puts `part` up on `f`, the ground `g` high, in `st`'s style (plain grey
/// without one).
pub fn build(map: &mut Map, f: &OBox, part: Part, g: f32, st: Option<&dyn Works>) {
    let seed = (f.center.x.to_bits() as u64) << 32 | f.center.y.to_bits() as u64;
    let mut s = Site {
        map,
        f: *f,
        g,
        grey: part.color(),
        rng: Rng::new(seed),
        dice: Rng::new(seed ^ 0xD1CE),
        st,
        pal: st.map(|st| st.palette()),
        role: Role::Wall,
    };
    let (w, l) = (f.half.x, f.half.y);
    let pick = s.rng.index(3);
    let molten = s.pal.as_ref().map_or(Vec3::ONE, |p| p.molten);
    if part.floored() {
        s.floor();
    }
    match part {
        Part::Smelter => {
            let h = s.rng.range(13.0, 17.0);
            // Furnaces down the back, a lean-to behind them or a charging
            // ramp's tower at one end; the front open but for columns.
            s.wall(true, -w + 0.6, (-l, l), h, &[]);
            let gate = [(w * 0.3, 5.2)];
            s.wall(false, -l + 0.6, (-w, w), h, &gate);
            s.wall(false, l - 0.6, (-w, w), h, &gate);
            let n = (l / 7.0).round() as i32;
            for k in 0..n {
                let z = -l + (2.0 * l) * (k as f32 + 0.5) / n as f32;
                s.furnace(-w + 6.2, z, 4.2, 9.5);
                if k % 2 == pick as i32 % 2 {
                    s.chimney(-w + 3.0, z, 1.7, (9.5, h + 13.0));
                }
                let edge = -l + (2.0 * l) * k as f32 / n as f32;
                if k > 0 {
                    s.role = Role::Steel;
                    s.put(w - 0.8, edge, (0.7, 0.7), (-1.0, h));
                }
            }
            s.role = Role::Trim;
            s.put(w - 0.8, 0.0, (0.5, l), (h - 1.2, h));
            if s.pal.is_some() {
                // The ladle, on its crane over the floor.
                let (x, y) = (w * 0.25, h - 3.4);
                let (c, m) = s.look(Role::Steel);
                for e in [-1.0f32, 1.0] {
                    s.beam((-w + 1.0, y, e), (w - 1.0, y, e), 0.35, c, m);
                    s.beam((x, 8.0, e * 1.2), (x, y, e * 1.2), 0.06, c, m);
                }
                s.pipe((x, 5.4, 0.0), (x, 7.6, 0.0), 1.5, c * 0.7, m);
                s.disc((x, 5.5, 0.0), 1.35, c * 0.7, m);
                s.disc((x, 7.3, 0.0), 1.35, molten, Material::Glow);
                s.light((x, 8.2, 0.0), molten * 4.0, 10.0);
                let p = s.at(x, 7.0, 0.0);
                s.map.sound(Ambient::Machine, p);
            }
            s.roof(0.0, 0.0, (w, l), h, 0.4);
        }
        Part::Casting => {
            let h = s.rng.range(9.0, 11.5);
            let n = (l / 4.5).round().max(2.0) as i32;
            s.role = Role::Steel;
            for k in 0..=n {
                let z = -l + 0.8 + (2.0 * l - 1.6) * k as f32 / n as f32;
                for sx in [-1.0f32, 1.0] {
                    s.put(sx * (w - 0.8), z, (0.6, 0.6), (-1.0, h));
                }
            }
            // A stretch of wall on one side, the pits down the middle.
            let side = if pick == 0 { 1.0 } else { -1.0 };
            s.wall(
                true,
                side * (w - 0.6),
                (-l * 0.5, l * 0.6),
                h,
                &[(0.0, 4.6)],
            );
            for k in 0..3 {
                s.role = Role::Dark;
                let z = (k as f32 - 1.0) * l * 0.55;
                let hot = pick as i32 == k;
                let (c, m) = if hot {
                    (molten * 0.7, Material::Glow)
                } else {
                    (Vec3::splat(0.12), Material::Rubble)
                };
                if s.pal.is_some() {
                    s.deck(0.0, z, (w * 0.26, l * 0.08), (1.4, 1.56), c, m);
                    if hot {
                        s.light((0.0, 3.0, z), molten * 4.0, 11.0);
                    }
                }
                s.put(
                    0.0,
                    (k as f32 - 1.0) * l * 0.55,
                    (w * 0.3, l * 0.1),
                    (-0.5, 1.5),
                );
            }
            s.roof(0.0, 0.0, (w, l), h, 0.35);
            // A half-height annex on an end.
            s.role = Role::Wall;
            s.put(-side * w * 0.5, l - 3.0, (w * 0.4, 3.0), (-1.0, 5.0));
        }
        Part::Mill => {
            let h = s.rng.range(9.5, 12.0);
            let a = s.rng.range(-0.6, -0.1) * l;
            let b = s.rng.range(0.1, 0.6) * l;
            s.wall(true, -w + 0.6, (-l, l), h, &[(a, 4.8)]);
            s.wall(true, w - 0.6, (-l, l), h, &[(b, 4.8)]);
            for e in [-1.0f32, 1.0] {
                s.wall(false, e * (l - 0.6), (-w, w), h, &[(0.0, 5.0)]);
            }
            // The rolling line, off to one side of the way through.
            let n = (l / 11.0) as i32;
            for k in 0..n {
                let z = -l + 2.0 * l * (k as f32 + 0.5) / n as f32;
                s.role = Role::Rust;
                s.put(-w + 2.6, z, (1.5, 2.6), (-0.5, 2.6));
                if s.pal.is_some() {
                    let (c, m) = s.look(Role::Steel);
                    s.pipe((-w + 1.2, 2.9, z), (-w + 4.0, 2.9, z), 0.5, c, m);
                    s.glow(
                        &[(-w + 4.5, 0.4, z - 3.5), (-w + 4.5, 0.4, z + 3.5)],
                        0.25,
                        molten,
                    );
                }
            }
            s.roof(0.0, 0.0, (w, l), h, 0.3);
            // A clerestory down part of it.
            s.role = Role::Roof;
            s.put(0.0, (a + b) * 0.5, (w * 0.35, l * 0.35), (h + 0.5, h + 2.6));
        }
        Part::Power => {
            let h = s.rng.range(12.0, 16.0);
            s.put(0.0, -l * 0.25, (w, l * 0.75), (-1.5, h));
            s.put(0.0, l * 0.72, (w * 0.8, l * 0.28), (-1.5, h * 0.55));
            s.windows(0.0, -l * 0.25, (w, l * 0.75), (3.0, h - 1.0));
            if let Some(p) = &s.pal {
                let (dark, trim) = (p.dark, p.trim);
                for (y, c) in [(h - 0.5, dark), (2.2, trim)] {
                    s.deck(
                        0.0,
                        -l * 0.25,
                        (w + 0.14, l * 0.75 + 0.14),
                        (y, y + 0.6),
                        c,
                        Material::Concrete,
                    );
                }
                let (c, m) = s.look(Role::Steel);
                for e in [-1.0f32, 1.0] {
                    let x = e * w * 0.5;
                    s.pipe(
                        (x, h * 0.55 + 1.0, l * 0.5),
                        (x, h * 0.55 + 1.0, l),
                        0.5,
                        c,
                        m,
                    );
                }
            }
            for k in 0..=pick as i32 + 1 {
                let z = -l * 0.75 + k as f32 * l * 0.42;
                s.chimney(w * 0.3, z, 2.0, (h, h + 17.0 + k as f32 * 2.0));
            }
        }
        Part::Assembly => {
            let h = s.rng.range(10.0, 13.0);
            let t = w * 0.45;
            // The main hall down one side, a wing across an end; the corner
            // between them is a yard.
            let x = -w + t;
            s.wall(true, -w + 0.6, (-l, l), h, &[(l * 0.3, 5.0)]);
            s.wall(
                true,
                x + t,
                (-l + 2.0 * t, l),
                h,
                &[(0.0, 5.0), (l * 0.6, 4.6)],
            );
            s.wall(false, l - 0.6, (-w, x + t), h, &[(x, 5.0)]);
            s.wall(false, -l + 0.6, (-w, w), h, &[(w * 0.5, 4.8)]);
            s.wall(false, -l + 2.0 * t, (x + t, w), h * 0.75, &[(w * 0.5, 4.6)]);
            s.wall(true, w - 0.6, (-l, -l + 2.0 * t), h * 0.75, &[]);
            s.roof(x, t, (t, l - t), h, 0.4);
            s.roof(0.0, -l + t, (w, t), h + 0.6, 0.5);
            s.role = Role::Cast;
            s.put(w * 0.55, l * 0.4, (2.0, 3.2), (-0.5, 2.0));
        }
        Part::Ore => {
            let n = (l / 5.0).round().max(2.0) as i32;
            s.role = Role::Steel;
            for k in 0..=n {
                let z = -l + 1.0 + (2.0 * l - 2.0) * k as f32 / n as f32;
                for sx in [-1.0f32, 1.0] {
                    s.put(sx * (w - 1.0), z, (0.8, 0.8), (-1.5, 7.0));
                }
            }
            if let Some(p) = &s.pal {
                let (trim, dark) = (p.trim, p.dark);
                for y in [6.5, 10.0, 13.6] {
                    s.deck(
                        0.0,
                        0.0,
                        (w + 0.15, l + 0.15),
                        (y, y + 0.45),
                        dark,
                        Material::Metal,
                    );
                }
                for k in 0..n {
                    let z = -l + 1.0 + (2.0 * l - 2.0) * (k as f32 + 0.5) / n as f32;
                    s.deck(0.0, z, (1.4, 1.2), (5.4, 6.7), trim, Material::Painted);
                }
            }
            s.role = Role::Wall;
            s.put(0.0, 0.0, (w, l), (6.6, 14.0));
            s.role = Role::Roof;
            s.put(0.0, l * 0.5, (w * 0.5, l * 0.4), (14.0, 18.5));
        }
        Part::Cooling => {
            s.role = Role::Wall;
            let r = w - 0.2;
            for e in [-1.0f32, 1.0] {
                let z = e * (l - r);
                s.round(0.0, z, r, (-1.5, 6.0), true);
                s.round(0.0, z, r * 0.84, (6.0, 15.0), true);
                s.round(0.0, z, r * 0.9, (15.0, 20.0), true);
                if let Some(p) = &s.pal {
                    let trim = p.trim;
                    s.pipe(
                        (0.0, 17.4, z),
                        (0.0, 18.4, z),
                        r * 0.91,
                        trim,
                        Material::Painted,
                    );
                }
            }
        }
        Part::Tanks => {
            let n = (l / w).floor().max(1.0) as i32;
            for k in 0..n {
                let z = -l + 2.0 * l * (k as f32 + 0.5) / n as f32;
                let h = s.rng.range(4.0, 8.0);
                s.role = Role::Wall;
                s.round(0.0, z, w * 0.86, (-1.0, h), true);
                if let Some(p) = &s.pal {
                    let (trim, (c, m)) = (p.trim, s.look(Role::Steel));
                    let r = w * 0.86;
                    s.pipe(
                        (0.0, h - 1.4, z),
                        (0.0, h - 0.9, z),
                        r + 0.04,
                        trim,
                        Material::Painted,
                    );
                    s.pipe((r + 0.3, 0.6, z), (r + 0.3, 0.6, z + 3.0), 0.2, c, m);
                }
            }
        }
        Part::Bunker => {
            for e in [-1.0f32, 1.0] {
                s.role = Role::Dark;
                s.put(e * (w - 0.4), 0.0, (0.4, l), (-1.0, 3.2));
            }
            let n = (l / 2.8).round().max(2.0) as i32;
            for k in 0..=n {
                let z = -l + 0.4 + (2.0 * l - 0.8) * k as f32 / n as f32;
                s.put(0.0, z, (w, 0.35), (-1.0, 3.6));
                if k < n && s.pal.is_some() {
                    // Ore, heaped in it.
                    let step = (2.0 * l - 0.8) / n as f32;
                    let p = s.at(0.0, 0.0, z + step * 0.5);
                    let yaw = s.dice.range(0.0, TAU);
                    let c = Vec3::new(0.2, 0.15, 0.12) * s.dice.range(0.8, 1.4);
                    s.map.oboxes.push(OBox {
                        solid: false,
                        ..rocks::rubble(
                            Vec2::new(p.x, p.z),
                            yaw,
                            Vec2::new(step * 0.6, step * 0.55),
                            p.y - 0.8,
                            p.y + 1.8,
                            c,
                        )
                    });
                }
            }
        }
        Part::WaterTower => {
            s.legs((w - 0.5, w - 0.5), 0.4, 9.0);
            s.role = Role::Rust;
            s.round(0.0, 0.0, w, (9.0, 14.0), true);
        }
        Part::Office => {
            let h = 3.4 * (1 + pick) as f32;
            s.put(0.0, 0.0, (w, l), (-1.2, h));
            s.windows(0.0, 0.0, (w, l), (0.4, h));
            if let Some(p) = &s.pal {
                let dark = p.dark;
                s.deck(
                    0.0,
                    0.0,
                    (w + 0.2, l + 0.2),
                    (h - 0.3, h + 0.15),
                    dark,
                    Material::Concrete,
                );
            }
            s.role = Role::Roof;
            s.put(w * 0.3, l * 0.4, (w * 0.3, l * 0.25), (h, h + 2.0));
        }
        Part::Shed => {
            let h = s.rng.range(6.0, 7.5);
            s.legs((w - 0.4, l - 0.4), 0.4, h);
            s.wall(true, -w + 0.4, (-l, l), h, &[]);
            if pick > 0 {
                s.wall(false, -l + 0.4, (-w, w), h, &[]);
            }
            s.roof(0.0, 0.0, (w, l), h, 0.25);
        }
        Part::Dock => {
            s.role = Role::Dark;
            s.put(0.0, 0.0, (w, l), (-1.2, 1.3));
            s.role = Role::Cast;
            s.put(-w * 0.4, l * 0.4, (w * 0.5, l * 0.35), (1.3, 3.4));
        }
        Part::Crane => {
            let h = s.rng.range(9.0, 12.0);
            s.legs((w - 0.6, l - 0.6), 0.5, h);
            s.role = Role::Trim;
            s.put(0.0, 0.0, (w + 2.0, l * 0.5), (h, h + 1.6));
            if let Some(st) = s.st {
                st.girder(&mut s, (w + 2.0, l * 0.5), (h, h + 1.6));
                let (c, m) = s.look(Role::Dark);
                for e in [-0.3f32, 0.3] {
                    s.beam(
                        (w * 0.3 + e, h * 0.5, 0.0),
                        (w * 0.3 + e, h, 0.0),
                        0.05,
                        c,
                        m,
                    );
                }
                s.deck(w * 0.3, 0.0, (0.5, 0.5), (h * 0.5 - 0.6, h * 0.5), c, m);
            }
            s.role = Role::Steel;
            s.put(w * 0.3, 0.0, (1.5, l * 0.6), (h + 1.6, h + 3.4));
        }
        Part::Gatehouse => {
            s.put(0.0, 0.0, (w, l), (-1.0, 3.2));
            s.windows(0.0, 0.0, (w, l), (0.0, 3.4));
            s.role = Role::Roof;
            s.put(0.0, 0.0, (w + 0.5, l + 0.5), (3.2, 3.6));
        }
        Part::Wall => {
            let h = s.rng.range(2.4, 4.2);
            s.wall(true, 0.0, (-l, l), h, &[]);
        }
        Part::Rack => {
            for k in 0..3 {
                let h = 1.4 + 0.7 * k as f32;
                s.role = Role::Cast;
                s.put((k as f32 - 1.0) * w * 0.6, 0.0, (w * 0.25, l), (-0.8, h));
            }
        }
        Part::Portal => {
            s.legs((w - 0.7, l - 0.7), 0.6, 8.5);
            s.role = Role::Trim;
            s.put(0.0, 0.0, (w, l * 0.4), (8.5, 10.2));
            if let Some(st) = s.st {
                st.girder(&mut s, (w, l * 0.4), (8.5, 10.2));
            }
        }
        Part::PipeBridge => {
            for e in [-1.0f32, 1.0] {
                s.put(e * (w - 0.6), 0.0, (0.5, l), (-1.5, 7.5));
            }
            let hide = s.st.is_some();
            s.put(0.0, 0.0, (w, l * 0.8), (7.0, 8.4)).hidden = hide;
            if hide {
                let (c, m) = s.look(Role::Steel);
                for (z, y, r) in [(-0.55, 7.4, 0.4), (0.4, 7.5, 0.5), (0.0, 8.1, 0.28)] {
                    s.pipe((-w, y, z), (w, y, z), r, c, m);
                }
                for e in [-1.0f32, 1.0] {
                    for z in [-0.55, 0.4] {
                        s.pipe((e * w, 7.5, z), (e * w, -0.5, z), 0.4, c, m);
                    }
                }
                s.deck(0.0, 0.0, (w, 0.9), (6.75, 7.0), c * 0.8, m);
            }
        }
        Part::Conveyor => {
            let n = (l / 6.0).floor().max(1.0) as i32;
            s.role = Role::Steel;
            for k in 0..n {
                let z = -l + 2.0 * l * (k as f32 + 0.5) / n as f32;
                s.put(0.0, z, (0.5, 0.5), (-3.0, 8.0));
            }
            s.role = Role::Wall;
            s.put(0.0, 0.0, (w, l), (8.0, 10.6));
            if let Some(p) = &s.pal {
                let (dark, trim) = (p.dark, p.trim);
                s.deck(
                    0.0,
                    0.0,
                    (w + 0.06, l - 0.3),
                    (9.2, 9.8),
                    Vec3::splat(0.07),
                    Material::Metal,
                );
                s.deck(
                    0.0,
                    0.0,
                    (w + 0.15, l),
                    (10.4, 10.75),
                    dark,
                    Material::Metal,
                );
                s.deck(0.0, 0.0, (w + 0.1, l), (7.9, 8.3), trim, Material::Painted);
            }
        }
        Part::Wagons => {
            let n = (l / 4.6).round().max(1.0) as i32;
            for k in 0..n {
                let z = -l + 2.0 * l * (k as f32 + 0.5) / n as f32;
                let len = l / n as f32 - 0.35;
                let hide = s.st.is_some();
                s.put(0.0, z, (w, len), (-0.4, 3.0)).hidden = hide;
                if hide {
                    s.wagon(z, w, len);
                }
            }
        }
        Part::Cars => {
            let n = (l / 5.0).round().max(1.0) as i32;
            for k in 0..n {
                let z = -l + 2.0 * l * (k as f32 + 0.5) / n as f32;
                let len = l / n as f32 - 0.4;
                let kind = s.rng.index(4);
                let hide = s.st.is_some();
                // A box car, a gondola, a tanker, a flat with a crate on it.
                let h = [3.9, 2.9, 3.5, 1.25][kind];
                s.put(0.0, z, (w, len), (-0.4, h)).hidden = hide;
                let crate_at = s.rng.range(-0.3, 0.3) * len;
                if kind == 3 {
                    s.put(0.0, z + crate_at, (w * 0.8, len * 0.4), (h, h + 1.7))
                        .hidden = hide;
                }
                if hide {
                    match kind {
                        1 => s.wagon(z, w, len),
                        k => s.car(k, (z, crate_at), w, len),
                    }
                }
            }
        }
        Part::Gantry => {
            let h = s.rng.range(8.5, 10.0);
            s.legs((w - 0.7, l - 0.7), 0.5, h);
            s.role = Role::Trim;
            s.put(0.0, 0.0, (w + 1.0, l * 0.35), (h, h + 1.6));
            if let Some(st) = s.st {
                st.girder(&mut s, (w + 1.0, l * 0.35), (h, h + 1.6));
                let x = s.dice.range(-0.5, 0.5) * w;
                let (c, m) = s.look(Role::Dark);
                for e in [-0.4f32, 0.4] {
                    s.beam((x + e, h * 0.62, 0.0), (x + e, h, 0.0), 0.05, c, m);
                }
                s.deck(x, 0.0, (0.9, 0.9), (h * 0.62 - 0.5, h * 0.62), c, m);
            }
            s.role = Role::Steel;
            s.put(0.0, 0.0, (1.4, l * 0.5), (h + 1.6, h + 3.2));
        }
        Part::Hulls => {
            let hide = s.st.is_some();
            s.role = Role::Cast;
            s.put(0.0, 0.0, (w * 0.7, l), (-0.5, 1.9)).hidden = hide;
            s.put(0.0, 0.0, (w, l * 0.9), (0.7, 1.5)).hidden = hide;
            if hide {
                s.casting(0.0, 0.0, 0.6, w / 2.0);
                for z in [-1.6f32, 1.6] {
                    let c = Vec3::new(0.3, 0.23, 0.17);
                    s.deck(0.0, z, (1.7, 0.3), (-0.6, 0.65), c, Material::Wood);
                }
            }
        }
        Part::Billets => {
            let hide = s.st.is_some();
            s.role = Role::Cast;
            s.put(0.0, 0.0, (w, l), (-0.5, 1.1)).hidden = hide;
            s.put(w * 0.1, 0.0, (w * 0.8, l * 0.8), (1.1, 2.0)).hidden = hide;
            if let Some(st) = s.st {
                st.billets(&mut s, w, l);
            }
        }
        Part::Pot => {
            s.role = Role::Rust;
            s.round(0.0, 0.0, w, (-0.6, 2.3), true);
            if s.pal.is_some() && pick == 0 {
                s.disc((0.0, 2.3, 0.0), w * 0.85, molten * 0.7, Material::Glow);
            }
        }
        Part::Rubble => {
            s.role = Role::Wall;
            s.put(0.0, 0.0, (w, l), (-0.6, 1.4)).tilt = 0.08;
            s.put(w * 0.4, 0.3, (w * 0.5, l * 0.5), (0.2, 2.2)).tilt = -0.2;
        }
    }
}

impl Site<'_> {
    /// A hull as it comes out of the mould, on `y`, `k` full size.
    fn casting(&mut self, x: f32, z: f32, y: f32, k: f32) {
        let (c, m) = self.look(Role::Cast);
        let d = |s: &mut Self, dx: f32, dz: f32, h: (f32, f32), t: (f32, f32), f: f32| {
            s.deck(
                x + dx * k,
                z + dz * k,
                (h.0 * k, h.1 * k),
                (y + t.0 * k, y + t.1 * k),
                c * f,
                m,
            )
            .yaw += 0.0;
        };
        d(self, 0.0, 0.0, (1.1, 2.6), (0.0, 1.15), 1.0);
        for s in [-1.0f32, 1.0] {
            d(self, s * 1.4, 0.0, (0.38, 2.95), (0.25, 1.0), 0.8);
        }
        for (dz, turn) in [(2.5f32, 0.75f32), (-2.55, -0.9)] {
            let o = self.deck(
                x,
                z + dz * k,
                (1.05 * k, 0.6 * k),
                (y + 0.45 * k, y + 0.77 * k),
                c * 0.92,
                m,
            );
            o.tilt = turn;
        }
        self.pipe(
            (x, y + 1.05 * k, z + 0.2 * k),
            (x, y + 1.4 * k, z + 0.2 * k),
            0.85 * k,
            c * 0.85,
            m,
        );
    }

    /// A car's frame, on its bogies.
    fn bogies(&mut self, z: f32, w: f32, l: f32, dark: Vec3) {
        let metal = Material::Metal;
        self.deck(0.0, z, (0.9, l - 0.1), (0.75, 1.08), dark, metal);
        self.deck(0.0, z, (w - 0.15, l - 0.2), (1.06, 1.2), dark, metal);
        for e in [-1.0f32, 1.0] {
            for t in [0.5f32, 0.78] {
                let c = self.at(0.0, 0.55, z + e * l * t);
                let axle = self.dir(1.0, 0.0);
                let a = Vec3::new(axle.x, 0.0, axle.y) * 0.85;
                self.map.disc(c, a, 0.42, dark * 0.8, metal);
            }
        }
    }

    /// A freight car `kind` (as `Part::Cars` draws them) at `z`, its crate
    /// `crate_at` from its middle.
    fn car(&mut self, kind: usize, (z, crate_at): (f32, f32), w: f32, l: f32) {
        let Some(p) = &self.pal else { return };
        let (paint, dark, trim) = (p.wagon * self.dice.range(0.8, 1.2), p.dark, p.trim);
        let (metal, painted) = (Material::Metal, Material::Painted);
        self.bogies(z, w, l, dark);
        match kind {
            0 => {
                self.deck(0.0, z, (w - 0.1, l - 0.12), (1.2, 3.7), paint, painted);
                self.deck(0.0, z, (w - 0.02, l - 0.04), (3.68, 3.9), dark, metal);
                for e in [-1.0f32, 1.0] {
                    // Its door, and the ribs either side.
                    self.deck(e * (w - 0.08), z, (0.07, 1.1), (1.25, 3.5), trim, painted);
                    for t in [-0.85f32, -0.55, 0.55, 0.85] {
                        let at = z + t * l;
                        self.deck(e * (w - 0.07), at, (0.06, 0.08), (1.22, 3.66), dark, metal);
                    }
                }
            }
            2 => {
                let (a, b) = ((0.0, 2.25, z - l + 0.5), (0.0, 2.25, z + l - 0.5));
                self.pipe(a, b, w * 0.72, paint, painted);
                for e in [-1.0f32, 1.0] {
                    let c = self.at(0.0, 2.25, z + e * (l - 0.5));
                    let out = self.dir(0.0, e);
                    let d = Vec3::new(out.x, 0.0, out.y) * 0.12;
                    self.map.disc(c, d, w * 0.7, paint * 0.9, painted);
                    let hoop = z + e * l * 0.45;
                    self.pipe(
                        (0.0, 2.25, hoop - 0.12),
                        (0.0, 2.25, hoop + 0.12),
                        w * 0.745,
                        dark,
                        metal,
                    );
                }
                self.pipe((0.0, 3.1, z), (0.0, 3.5, z), 0.45, dark, metal);
            }
            _ => {
                let at = z + crate_at;
                self.deck(
                    0.0,
                    at,
                    (w * 0.8, l * 0.4),
                    (1.2, 2.93),
                    trim * 0.8,
                    painted,
                );
                for t in [-0.3f32, 0.3] {
                    let b = at + t * l;
                    self.deck(0.0, b, (w * 0.8 + 0.04, 0.12), (1.2, 2.97), dark, metal);
                }
            }
        }
    }

    /// A wagon on its wheels, its load in it.
    fn wagon(&mut self, z: f32, w: f32, l: f32) {
        let Some(p) = &self.pal else { return };
        let (paint, dark) = (p.wagon * self.dice.range(0.85, 1.1), p.dark);
        let (metal, painted) = (Material::Metal, Material::Painted);
        self.deck(0.0, z, (0.9, l - 0.1), (0.75, 1.1), dark, metal);
        self.deck(0.0, z, (w - 0.2, l - 0.3), (1.08, 1.2), dark, metal);
        for e in [-1.0f32, 1.0] {
            for t in [0.45f32, 0.75] {
                let c = self.at(0.0, 0.55, z + e * l * t);
                let axle = self.dir(1.0, 0.0);
                let a = Vec3::new(axle.x, 0.0, axle.y) * 0.85;
                self.map.disc(c, a, 0.42, dark * 0.8, metal);
            }
            self.deck(
                0.0,
                z + e * (l - 0.1),
                (w - 0.08, 0.1),
                (1.1, 2.6),
                paint,
                painted,
            );
            self.deck(
                e * (w - 0.12),
                z,
                (0.08, l - 0.1),
                (1.1, 2.45),
                paint,
                painted,
            );
            for k in 0..4 {
                let t = z - l * 0.75 + l * 0.5 * k as f32;
                self.deck(e * (w - 0.03), t, (0.07, 0.09), (0.9, 2.65), dark, metal);
            }
        }
        if self.dice.chance(0.5) {
            self.casting(0.0, z, 1.2, 0.6);
        } else {
            for k in 0..4 {
                let q = self.at(0.0, 1.1, z - l * 0.7 + l * 0.47 * k as f32);
                let r = self.dice.range(0.7, 1.0);
                let yaw = self.dice.range(0.0, TAU);
                self.map.oboxes.push(OBox {
                    solid: false,
                    ..rocks::rubble(
                        Vec2::new(q.x, q.z),
                        yaw,
                        Vec2::new(r, r * 0.8),
                        q.y,
                        q.y + r * 1.7,
                        Vec3::new(0.22, 0.17, 0.14),
                    )
                });
            }
        }
    }
}
