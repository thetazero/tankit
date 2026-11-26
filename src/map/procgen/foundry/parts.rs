//! What a works is made of, each as the solids it stands as: big buildings
//! that are fights of their own, the medium things between them, cover. A
//! style dresses them; what's here is what's driven into, under and round.

use glam::{Vec2, Vec3};

use crate::map::{Map, OBox};
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

    /// Stands over what's under it, whatever its level.
    pub fn over(self) -> bool {
        matches!(
            self,
            Part::Portal | Part::PipeBridge | Part::Conveyor | Part::Wagons
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
            Part::Conveyor | Part::Portal | Part::PipeBridge | Part::Crane => {
                Vec3::new(0.75, 0.68, 0.3)
            }
            Part::Wagons => Vec3::new(0.3, 0.35, 0.5),
            p if Part::SMALL.contains(&p) => Vec3::new(0.35, 0.5, 0.35),
            _ => Vec3::new(0.55, 0.55, 0.58),
        }
    }
}

/// One being put up: its footprint, its ground's height, and its dice (its
/// own, so a part's the same whatever else is on the map).
struct Site<'a> {
    map: &'a mut Map,
    f: OBox,
    g: f32,
    color: Vec3,
    rng: Rng,
}

impl Site<'_> {
    /// A box about (`x`, `z`) on the site, from `y0` to `y1` over its ground.
    fn put(&mut self, x: f32, z: f32, half: (f32, f32), y: (f32, f32)) -> &mut OBox {
        let (ax, az) = self.f.axes();
        let shade = self.rng.range(0.92, 1.04);
        self.map.oboxes.push(OBox {
            center: self.f.center + ax * x + az * z,
            yaw: self.f.yaw,
            half: Vec2::new(half.0, half.1),
            y0: self.g + y.0,
            y1: self.g + y.1,
            color: self.color * shade,
            side_mat: Material::Concrete,
            top_mat: Material::Concrete,
            ..OBox::default()
        });
        self.map.oboxes.last_mut().unwrap()
    }

    /// Something round: two boxes, an eighth apart.
    fn round(&mut self, x: f32, z: f32, r: f32, y: (f32, f32)) {
        let h = r * std::f32::consts::FRAC_1_SQRT_2;
        self.put(x, z, (h, h), y);
        let o = self.put(x, z, (h, h), y);
        o.yaw += std::f32::consts::FRAC_PI_4;
        o.y1 -= 0.1;
    }

    /// A wall along z (else x), from `a` to `b`, `at` across, cut by `gates`
    /// (where, half how wide), some panels broken down; lintels over.
    fn wall(&mut self, along_z: bool, at: f32, (a, b): (f32, f32), h: f32, gates: &[(f32, f32)]) {
        let mut gates = gates.to_vec();
        gates.sort_by(|p, q| p.0.total_cmp(&q.0));
        let mut cut = |s: &mut Self, t0: f32, t1: f32, y: (f32, f32)| {
            if t1 - t0 < 0.3 {
                return;
            }
            let (m, l) = ((t0 + t1) * 0.5, (t1 - t0) * 0.5);
            if along_z {
                s.put(at, m, (0.55, l), y);
            } else {
                s.put(m, at, (l, 0.55), y);
            }
        };
        let mut from = a;
        for &(c, w) in gates.iter().chain([&(b + 100.0, 100.0)]) {
            let to = (c - w).min(b);
            let n = ((to - from) / 6.5).round().max(1.0);
            for k in 0..n as i32 {
                let t = |k: f32| from + (to - from) * k / n;
                let top = if self.rng.chance(0.14) {
                    self.rng.range(1.2, h * 0.6)
                } else {
                    h
                };
                cut(self, t(k as f32), t(k as f32 + 1.0), (-1.2, top));
            }
            if c < b && h > 8.2 {
                cut(self, c - w, c + w, (7.0, h));
            }
            from = c + w;
            if from >= b {
                break;
            }
        }
    }

    /// A roof over `half` about (`x`, `z`), in bays along z, some gone.
    fn roof(&mut self, x: f32, z: f32, half: (f32, f32), y: f32, gone: f32) {
        let n = (half.1 * 2.0 / 7.0).round().max(1.0);
        let l = half.1 / n;
        for k in 0..n as i32 {
            if self.rng.chance(gone) {
                continue;
            }
            let c = z - half.1 + l * (2.0 * k as f32 + 1.0);
            self.put(x, c, (half.0, l - 0.05), (y, y + 0.5));
        }
    }

    /// Posts at the corners of `half` about the middle, from the ground (or
    /// well into it) to `h`.
    fn legs(&mut self, half: (f32, f32), r: f32, h: f32) {
        for sx in [-1.0f32, 1.0] {
            for sz in [-1.0f32, 1.0] {
                self.put(sx * half.0, sz * half.1, (r, r), (-1.5, h));
            }
        }
    }
}

/// Puts `part` up on `f`, the ground `g` high.
pub fn build(map: &mut Map, f: &OBox, part: Part, g: f32) {
    let seed = (f.center.x.to_bits() as u64) << 32 | f.center.y.to_bits() as u64;
    let mut s = Site {
        map,
        f: *f,
        g,
        color: part.color(),
        rng: Rng::new(seed),
    };
    let (w, l) = (f.half.x, f.half.y);
    let pick = s.rng.index(3);
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
                s.round(-w + 6.2, z, 4.2, (-0.5, 9.5));
                if k % 2 == pick as i32 % 2 {
                    s.round(-w + 3.0, z, 1.7, (9.5, h + 13.0));
                }
                let edge = -l + (2.0 * l) * k as f32 / n as f32;
                if k > 0 {
                    s.put(w - 0.8, edge, (0.7, 0.7), (-1.0, h));
                }
            }
            s.put(w - 0.8, 0.0, (0.5, l), (h - 1.2, h));
            s.roof(0.0, 0.0, (w, l), h, 0.4);
        }
        Part::Casting => {
            let h = s.rng.range(9.0, 11.5);
            let n = (l / 4.5).round().max(2.0) as i32;
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
                s.put(
                    0.0,
                    (k as f32 - 1.0) * l * 0.55,
                    (w * 0.3, l * 0.1),
                    (-0.5, 1.5),
                );
            }
            s.roof(0.0, 0.0, (w, l), h, 0.35);
            // A half-height annex on an end.
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
                s.put(-w + 2.6, z, (1.5, 2.6), (-0.5, 2.6));
            }
            s.roof(0.0, 0.0, (w, l), h, 0.3);
            // A clerestory down part of it.
            s.put(0.0, (a + b) * 0.5, (w * 0.35, l * 0.35), (h + 0.5, h + 2.6));
        }
        Part::Power => {
            let h = s.rng.range(12.0, 16.0);
            s.put(0.0, -l * 0.25, (w, l * 0.75), (-1.5, h));
            s.put(0.0, l * 0.72, (w * 0.8, l * 0.28), (-1.5, h * 0.55));
            for k in 0..=pick as i32 + 1 {
                let z = -l * 0.75 + k as f32 * l * 0.42;
                s.round(w * 0.3, z, 2.0, (h, h + 17.0 + k as f32 * 2.0));
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
            s.put(w * 0.55, l * 0.4, (2.0, 3.2), (-0.5, 2.0));
        }
        Part::Ore => {
            let n = (l / 5.0).round().max(2.0) as i32;
            for k in 0..=n {
                let z = -l + 1.0 + (2.0 * l - 2.0) * k as f32 / n as f32;
                for sx in [-1.0f32, 1.0] {
                    s.put(sx * (w - 1.0), z, (0.8, 0.8), (-1.5, 7.0));
                }
            }
            s.put(0.0, 0.0, (w, l), (6.6, 14.0));
            s.put(0.0, l * 0.5, (w * 0.5, l * 0.4), (14.0, 18.5));
        }
        Part::Cooling => {
            let r = w - 0.2;
            for e in [-1.0f32, 1.0] {
                let z = e * (l - r);
                s.round(0.0, z, r, (-1.5, 6.0));
                s.round(0.0, z, r * 0.84, (6.0, 15.0));
                s.round(0.0, z, r * 0.9, (15.0, 20.0));
            }
        }
        Part::Tanks => {
            let n = (l / w).floor().max(1.0) as i32;
            for k in 0..n {
                let z = -l + 2.0 * l * (k as f32 + 0.5) / n as f32;
                let h = s.rng.range(4.0, 8.0);
                s.round(0.0, z, w * 0.86, (-1.0, h));
            }
        }
        Part::Bunker => {
            for e in [-1.0f32, 1.0] {
                s.put(e * (w - 0.4), 0.0, (0.4, l), (-1.0, 3.2));
            }
            let n = (l / 2.8).round().max(2.0) as i32;
            for k in 0..=n {
                let z = -l + 0.4 + (2.0 * l - 0.8) * k as f32 / n as f32;
                s.put(0.0, z, (w, 0.35), (-1.0, 3.6));
            }
        }
        Part::WaterTower => {
            s.legs((w - 0.5, w - 0.5), 0.4, 9.0);
            s.round(0.0, 0.0, w, (9.0, 14.0));
        }
        Part::Office => {
            let h = 3.4 * (1 + pick) as f32;
            s.put(0.0, 0.0, (w, l), (-1.2, h));
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
            s.put(0.0, 0.0, (w, l), (-1.2, 1.3));
            s.put(-w * 0.4, l * 0.4, (w * 0.5, l * 0.35), (1.3, 3.4));
        }
        Part::Crane => {
            let h = s.rng.range(9.0, 12.0);
            s.legs((w - 0.6, l - 0.6), 0.5, h);
            s.put(0.0, 0.0, (w + 2.0, l * 0.5), (h, h + 1.6));
            s.put(w * 0.3, 0.0, (1.5, l * 0.6), (h + 1.6, h + 3.4));
        }
        Part::Gatehouse => {
            s.put(0.0, 0.0, (w, l), (-1.0, 3.2));
            s.put(0.0, 0.0, (w + 0.5, l + 0.5), (3.2, 3.6));
        }
        Part::Wall => {
            let h = s.rng.range(2.4, 4.2);
            s.wall(true, 0.0, (-l, l), h, &[]);
        }
        Part::Rack => {
            for k in 0..3 {
                let h = 1.4 + 0.7 * k as f32;
                s.put((k as f32 - 1.0) * w * 0.6, 0.0, (w * 0.25, l), (-0.8, h));
            }
        }
        Part::Portal => {
            s.legs((w - 0.7, l - 0.7), 0.6, 8.5);
            s.put(0.0, 0.0, (w, l * 0.4), (8.5, 10.2));
        }
        Part::PipeBridge => {
            for e in [-1.0f32, 1.0] {
                s.put(e * (w - 0.6), 0.0, (0.5, l), (-1.5, 7.5));
            }
            s.put(0.0, 0.0, (w, l * 0.8), (7.0, 8.4));
        }
        Part::Conveyor => {
            let n = (l / 6.0).floor().max(1.0) as i32;
            for k in 0..n {
                let z = -l + 2.0 * l * (k as f32 + 0.5) / n as f32;
                s.put(0.0, z, (0.5, 0.5), (-3.0, 8.0));
            }
            s.put(0.0, 0.0, (w, l), (8.0, 10.6));
        }
        Part::Wagons => {
            let n = (l / 4.6).round().max(1.0) as i32;
            for k in 0..n {
                let z = -l + 2.0 * l * (k as f32 + 0.5) / n as f32;
                s.put(0.0, z, (w, l / n as f32 - 0.35), (-0.4, 3.0));
            }
        }
        Part::Hulls => {
            s.put(0.0, 0.0, (w * 0.7, l), (-0.5, 1.9));
            s.put(0.0, 0.0, (w, l * 0.9), (0.7, 1.5));
        }
        Part::Billets => {
            s.put(0.0, 0.0, (w, l), (-0.5, 1.1));
            s.put(w * 0.1, 0.0, (w * 0.8, l * 0.8), (1.1, 2.0));
        }
        Part::Pot => s.round(0.0, 0.0, w, (-0.6, 2.3)),
        Part::Rubble => {
            s.put(0.0, 0.0, (w, l), (-0.6, 1.4)).tilt = 0.08;
            s.put(w * 0.4, 0.3, (w * 0.5, l * 0.5), (0.2, 2.2)).tilt = -0.2;
        }
    }
}
