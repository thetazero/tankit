//! Traces of the people who lived here: power poles and sagging cables,
//! burned-out cars, market stalls under faded cloth, dead palms. Placed by
//! the map; these helpers only know how to build each thing.
//!
//! Small and thin parts (cables, fronds, cloth) are visual only; bodies a
//! tank would hit (car shells, pole shafts, stall posts, crates) collide.

use glam::{Vec2, Vec3};

use super::{Map, OBox};
use crate::render::material::Material;
use crate::rng::Rng;

impl Map {
    /// A thin piece from `a` to `b` (any slope), `r` half-thickness.
    pub(super) fn segment(
        &mut self,
        a: Vec3,
        b: Vec3,
        r: f32,
        color: Vec3,
        material: Material,
        solid: bool,
    ) {
        let d = b - a;
        let horiz = Vec2::new(d.x, d.z);
        let len = d.length();
        if len < 1e-3 {
            return;
        }
        let mid = (a + b) * 0.5;
        self.oboxes.push(OBox {
            center: Vec2::new(mid.x, mid.z),
            // Local +z along the segment's heading, tipped to its slope.
            yaw: horiz.x.atan2(horiz.y),
            tilt: -d.y.atan2(horiz.length()),
            half: Vec2::new(r, len * 0.5),
            y0: mid.y - r,
            y1: mid.y + r,
            color,
            side_mat: material,
            top_mat: material,
            solid,
            ..OBox::default()
        });
    }

    /// A cable sagging between two points, in short straight runs.
    pub(super) fn cable(&mut self, a: Vec3, b: Vec3, sag: f32) {
        let n = 8;
        let at = |t: f32| a.lerp(b, t) - Vec3::Y * sag * 4.0 * t * (1.0 - t);
        for k in 0..n {
            let (t0, t1) = (k as f32 / n as f32, (k + 1) as f32 / n as f32);
            self.segment(
                at(t0),
                at(t1),
                0.025,
                Vec3::new(0.08, 0.08, 0.08),
                Material::Rust,
                false,
            );
        }
    }

    /// A wooden power pole with a crossbar. Returns the two cable points on
    /// the crossbar (none if it snapped off).
    pub(super) fn pole(&mut self, rng: &mut Rng, at: Vec2, along: Vec2) -> Option<[Vec3; 2]> {
        let g = self.ground_height(at.x, at.y) - 0.3;
        let wood = Vec3::new(0.42, 0.32, 0.22) * rng.range(0.85, 1.1);
        let base = Vec3::new(at.x, g, at.y);
        if rng.chance(0.12) {
            // Snapped: a stump, and the rest lying in the sand.
            let h = rng.range(1.5, 3.0);
            self.segment(base, base + Vec3::Y * h, 0.14, wood, Material::Wood, true);
            let a = rng.range(0.0, std::f32::consts::TAU);
            let fall = Vec3::new(a.cos(), 0.0, a.sin());
            let p = base + fall * 1.0 + Vec3::Y * 0.3;
            self.segment(
                p,
                p + fall * 7.0 + Vec3::Y * -0.1,
                0.14,
                wood * 0.9,
                Material::Wood,
                false,
            );
            return None;
        }
        // Most stand; some lean.
        let lean = if rng.chance(0.25) {
            rng.range(0.08, 0.22)
        } else {
            rng.range(0.0, 0.03)
        };
        let a = rng.range(0.0, std::f32::consts::TAU);
        let tip = base + Vec3::new(a.cos() * lean, 1.0, a.sin() * lean).normalize() * 8.5;
        self.segment(base, tip, 0.14, wood, Material::Wood, true);
        let across = Vec3::new(-along.y, 0.0, along.x);
        let bar = tip - Vec3::Y * 0.5;
        self.segment(
            bar - across * 0.9,
            bar + across * 0.9,
            0.07,
            wood,
            Material::Wood,
            false,
        );
        Some([
            bar - across * 0.8 + Vec3::Y * 0.09,
            bar + across * 0.8 + Vec3::Y * 0.09,
        ])
    }

    /// A burned-out car: shell, roof, bare rims. Mostly charred and rusted;
    /// some still show their paint.
    pub(super) fn car(&mut self, rng: &mut Rng, at: Vec2, yaw: f32) {
        let g = self.ground_height(at.x, at.y);
        let burnt = rng.chance(0.65);
        let (color, mat) = if burnt {
            (
                Vec3::new(0.2, 0.16, 0.13) * rng.range(0.8, 1.2),
                Material::Rust,
            )
        } else {
            let paints = [
                Vec3::new(0.62, 0.66, 0.68),
                Vec3::new(0.55, 0.22, 0.18),
                Vec3::new(0.78, 0.74, 0.62),
                Vec3::new(0.3, 0.42, 0.5),
            ];
            (paints[rng.index(paints.len())], Material::Painted)
        };
        let base = OBox {
            center: at,
            yaw,
            ..OBox::default()
        };
        let (ax, az) = base.axes();
        let part = |m: &mut Map,
                    off: Vec2,
                    half: Vec2,
                    y0: f32,
                    y1: f32,
                    c: Vec3,
                    mt: Material,
                    solid: bool| {
            m.oboxes.push(OBox {
                center: at + ax * off.x + az * off.y,
                yaw,
                half,
                y0: g + y0,
                y1: g + y1,
                color: c,
                side_mat: mt,
                top_mat: mt,
                solid,
                chip: 0.12,
                ..OBox::default()
            });
        };
        part(
            self,
            Vec2::ZERO,
            Vec2::new(0.9, 2.15),
            0.18,
            0.95,
            color,
            mat,
            true,
        );
        part(
            self,
            Vec2::new(0.0, -0.2),
            Vec2::new(0.78, 1.15),
            0.95,
            1.45,
            color * 0.9,
            mat,
            false,
        );
        // Blown-out windows: a dark interior showing through.
        part(
            self,
            Vec2::new(0.0, -0.2),
            Vec2::new(0.8, 1.17),
            1.0,
            1.35,
            Vec3::splat(0.04),
            Material::Rubble,
            false,
        );
        for (sx, sz) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
            part(
                self,
                Vec2::new(sx * 0.92, sz * 1.35),
                Vec2::new(0.08, 0.32),
                0.0,
                0.62,
                Vec3::splat(0.12),
                Material::Rust,
                false,
            );
        }
    }

    /// A dead palm: a curved trunk and a few dry fronds hanging off the top.
    /// Some have fallen over.
    pub(super) fn palm(&mut self, rng: &mut Rng, at: Vec2) {
        let g = self.ground_height(at.x, at.y) - 0.3;
        let trunk = Vec3::new(0.4, 0.33, 0.25) * rng.range(0.85, 1.1);
        let frond = Vec3::new(0.46, 0.38, 0.24) * rng.range(0.85, 1.1);
        let a = rng.range(0.0, std::f32::consts::TAU);
        let bend = Vec3::new(a.cos(), 0.0, a.sin());
        let fallen = rng.chance(0.25);
        let h = rng.range(6.0, 9.0);
        let mut p = Vec3::new(at.x, g, at.y);
        let n = 6;
        for k in 0..n {
            let t = (k + 1) as f32 / n as f32;
            let q = if fallen {
                Vec3::new(at.x, g + 0.35, at.y) + bend * h * t
            } else {
                Vec3::new(at.x, g, at.y) + Vec3::Y * h * t + bend * (t * t) * h * 0.18
            };
            self.segment(p, q, 0.2 - 0.04 * t, trunk, Material::Wood, k < 3);
            p = q;
        }
        // Dry fronds drooping from the crown (or splayed on the sand).
        let fronds = rng.int(4, 7);
        for k in 0..fronds {
            let b = k as f32 / fronds as f32 * std::f32::consts::TAU + rng.range(-0.3, 0.3);
            let out = Vec3::new(b.cos(), 0.0, b.sin());
            let len = rng.range(2.2, 3.4);
            let droop = if fallen { 0.0 } else { rng.range(0.6, 1.4) };
            let end = p + out * len - Vec3::Y * droop * len * 0.6;
            self.segment(p, end, 0.12, frond, Material::Wood, false);
        }
    }

    /// A market stall: four posts, a cloth roof sagging toward the front, a
    /// counter and a few crates. `yaw`: the front faces local +z.
    pub(super) fn stall(&mut self, rng: &mut Rng, at: Vec2, yaw: f32) {
        let g = self.ground_height(at.x, at.y);
        let wood = Vec3::new(0.45, 0.35, 0.24) * rng.range(0.85, 1.1);
        let cloths = [
            Vec3::new(0.66, 0.24, 0.18), // faded red
            Vec3::new(0.26, 0.4, 0.55),  // faded blue
            Vec3::new(0.66, 0.56, 0.3),  // mustard
            Vec3::new(0.32, 0.46, 0.34), // faded green
            Vec3::new(0.78, 0.74, 0.64), // bleached white
        ];
        let cloth = cloths[rng.index(cloths.len())];
        let base = OBox {
            center: at,
            yaw,
            ..OBox::default()
        };
        let (ax, az) = base.axes();
        let w = |x: f32, z: f32, y: f32| {
            Vec3::new(at.x, g + y, at.y)
                + Vec3::new(ax.x, 0.0, ax.y) * x
                + Vec3::new(az.x, 0.0, az.y) * z
        };
        let (hx, hz) = (rng.range(1.3, 1.8), rng.range(1.0, 1.3));
        for (sx, sz, h) in [
            (-1.0f32, -1.0f32, 2.6f32),
            (1.0, -1.0, 2.6),
            (-1.0, 1.0, 2.2),
            (1.0, 1.0, 2.2),
        ] {
            self.segment(
                w(sx * hx, sz * hz, 0.0),
                w(sx * hx, sz * hz, h),
                0.06,
                wood,
                Material::Wood,
                true,
            );
        }
        // Cloth roof: sloped from back to front, sometimes torn half away.
        let torn = rng.chance(0.3);
        let span = if torn { 0.5 } else { 1.0 };
        let (x0, x1) = (-hx - 0.2, -hx + 2.0 * hx * span + 0.2);
        {
            let (a, b) = (
                w((x0 + x1) * 0.5, -hz - 0.15, 2.65),
                w((x0 + x1) * 0.5, hz + 0.35, 2.05),
            );
            let d = b - a;
            let mid = (a + b) * 0.5;
            let horiz = Vec2::new(d.x, d.z);
            self.oboxes.push(OBox {
                center: Vec2::new(mid.x, mid.z),
                yaw: horiz.x.atan2(horiz.y),
                tilt: -d.y.atan2(horiz.length()),
                half: Vec2::new((x1 - x0) * 0.5, d.length() * 0.5),
                y0: mid.y - 0.02,
                y1: mid.y + 0.02,
                color: cloth,
                side_mat: Material::Cloth,
                top_mat: Material::Cloth,
                solid: false,
                ..OBox::default()
            });
        }
        // Counter across the front.
        let c = w(0.0, hz - 0.2, 0.0);
        self.oboxes.push(OBox {
            center: Vec2::new(c.x, c.z),
            yaw,
            half: Vec2::new(hx - 0.1, 0.3),
            y0: g,
            y1: g + 0.95,
            color: wood * 0.9,
            side_mat: Material::Wood,
            top_mat: Material::Wood,
            chip: 0.08,
            ..OBox::default()
        });
        // Crates stacked beside it.
        for k in 0..rng.int(1, 4) {
            let p = w(hx + 0.7 + rng.range(-0.2, 0.4), rng.range(-hz, hz), 0.0);
            let s = rng.range(0.3, 0.45);
            let y = if k == 3 { 2.0 * s } else { 0.0 };
            self.oboxes.push(OBox {
                center: Vec2::new(p.x, p.z),
                yaw: yaw + rng.range(-0.4, 0.4),
                half: Vec2::splat(s),
                y0: g + y,
                y1: g + y + 2.0 * s,
                color: wood * rng.range(0.8, 1.05),
                side_mat: Material::Wood,
                top_mat: Material::Wood,
                chip: 0.05,
                solid: k == 0,
                ..OBox::default()
            });
        }
    }
}
