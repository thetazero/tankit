//! Parts things are built of (pipes, discs, whatever `Rig` places in its
//! own frame), and the stranded boat.

use glam::{Vec2, Vec3};

use super::{Ambient, Map, OBox};
use crate::render::material::Material;
use crate::rng::Rng;

const TIMBER: Vec3 = Vec3::new(0.44, 0.34, 0.24);

impl Map {
    /// A round pipe or post from `a` to `b`: a ring of thin plates around
    /// the axis (12 for thick pipes, 6 for thin posts), each rolled to face
    /// outward. An inner core box collides when `solid`.
    pub(super) fn pipe(
        &mut self,
        a: Vec3,
        b: Vec3,
        r: f32,
        color: Vec3,
        material: Material,
        solid: bool,
    ) {
        let sides = if r < 0.12 { 6 } else { 12 };
        let step = std::f32::consts::TAU / sides as f32;
        let apothem = r * (step * 0.5).cos();
        let half_side = r * (step * 0.5).sin() * 1.04;
        let thick = (r * 0.12).clamp(0.01, 0.06);
        for k in 0..sides {
            self.segment(
                a,
                b,
                thick,
                color * (0.96 + 0.04 * (k % 2) as f32),
                material,
                false,
            );
            let Some(o) = self.oboxes.last_mut() else {
                return;
            };
            o.roll = k as f32 * step;
            o.half.x = half_side;
            // Push the plate out from the axis along its own (rolled) up.
            let out = o.rotation() * Vec3::Y * (apothem - thick);
            o.center += Vec2::new(out.x, out.z);
            o.y0 += out.y;
            o.y1 += out.y;
        }
        if solid {
            self.segment(a, b, r * 0.7, color * 0.5, material, true);
            if let Some(o) = self.oboxes.last_mut() {
                o.half.x = r * 0.7;
            }
        }
    }

    /// A solid disc (wheel, hub) centered on `c`, its axle along `axis`
    /// (whose length is half the disc's thickness): six strips across it,
    /// turned 30 degrees apart, fill a 12-sided face. Their faces are staggered
    /// a few millimeters so the overlaps never share a plane.
    pub(super) fn disc(&mut self, c: Vec3, axis: Vec3, r: f32, color: Vec3, material: Material) {
        let step = std::f32::consts::PI / 6.0;
        let apothem = r * (step * 0.5).cos();
        let half_side = r * (step * 0.5).sin();
        for k in 0..6 {
            let e = axis.normalize() * (axis.length() + 0.003 * k as f32);
            self.segment(
                c - e,
                c + e,
                half_side + 0.002 * k as f32,
                color,
                material,
                false,
            );
            if let Some(o) = self.oboxes.last_mut() {
                o.roll = k as f32 * step;
                o.half.x = apothem + 0.002 * k as f32;
            }
        }
    }

    /// A small boat stranded in the mud, listing: a hull of strakes tapering
    /// to a bow, a cabin with dark windows, a snapped mast. `dir` points
    /// bow-ward. One solid box for collision.
    pub(super) fn boat(&mut self, rng: &mut Rng, at: Vec2, dir: Vec2, len: f32) {
        let g = self.ground_height(at.x, at.y);
        self.sound(Ambient::Hull, Vec3::new(at.x, g + 1.5, at.y));
        let yaw = dir.x.atan2(dir.y);
        let list = rng.range(0.12, 0.28) * if rng.chance(0.5) { 1.0 } else { -1.0 };
        let beam = len * 0.2;
        let paints = [
            Vec3::new(0.62, 0.66, 0.68),
            Vec3::new(0.66, 0.3, 0.24),
            Vec3::new(0.3, 0.44, 0.52),
        ];
        let paint = paints[rng.index(paints.len())];
        let base = OBox {
            center: at,
            yaw,
            ..OBox::default()
        };
        let (ax, az) = base.axes();
        let w = |x: f32, z: f32| at + ax * x + az * z;
        // Collision: one box for the hull.
        self.oboxes.push(OBox {
            center: at,
            yaw,
            half: Vec2::new(beam * 0.85, len * 0.36),
            y0: g - 0.5,
            y1: g + 1.4,
            color: paint,
            side_mat: Material::Painted,
            top_mat: Material::Painted,
            roll: list,
            solid: true,
            ..OBox::default()
        });
        // Hull plates: on each side a straight run aft and a bow plate
        // angled in to meet at the stem, flared outward; a transom across the
        // stern; a deck on top. Antifouling red below a paint band above.
        let (stern_z, shoulder_z, stem_z) = (-len * 0.46, len * 0.12, len * 0.5);
        let hull_h = 1.7;
        let plate =
            |m: &mut Map, from: Vec2, to: Vec2, y_mid: f32, half_h: f32, c: Vec3, flare: f32| {
                let (a, b) = (w(from.x, from.y), w(to.x, to.y));
                m.segment(
                    Vec3::new(a.x, g + y_mid, a.y),
                    Vec3::new(b.x, g + y_mid, b.y),
                    half_h,
                    c,
                    Material::Painted,
                    false,
                );
                if let Some(o) = m.oboxes.last_mut() {
                    o.half.x = 0.07;
                    o.roll = flare;
                }
            };
        let red = Vec3::new(0.4, 0.15, 0.11);
        for side in [-1.0f32, 1.0] {
            let flare = -side * 0.22 + list;
            let aft = Vec2::new(side * beam, stern_z);
            let mid = Vec2::new(side * beam, shoulder_z);
            let stem = Vec2::new(0.0, stem_z);
            for (y_mid, hh, c) in [
                (0.15, 0.45, red),
                (0.6 + hull_h * 0.25, hull_h * 0.25 + 0.05, paint),
            ] {
                plate(self, aft, mid, y_mid, hh, c, flare);
                plate(self, mid, stem, y_mid + 0.1, hh, c, flare * 0.6);
            }
        }
        plate(
            self,
            Vec2::new(-beam, stern_z),
            Vec2::new(beam, stern_z),
            0.75,
            0.8,
            paint * 0.9,
            list,
        );
        // Deck: full width aft, narrowing in steps under the bow plates.
        for k in 0..6 {
            let t0 = shoulder_z + (stem_z - shoulder_z) * k as f32 / 6.0;
            let t1 = shoulder_z + (stem_z - shoulder_z) * (k + 1) as f32 / 6.0;
            let hw = beam * (1.0 - (k as f32 + 0.5) / 6.0) * 0.95;
            let c = w(0.0, (t0 + t1) * 0.5);
            self.oboxes.push(OBox {
                center: c,
                yaw,
                half: Vec2::new(hw, (t1 - t0) * 0.5),
                y0: g + 1.45,
                y1: g + 1.55,
                roll: list,
                color: TIMBER,
                side_mat: Material::Wood,
                top_mat: Material::Wood,
                solid: false,
                ..OBox::default()
            });
        }
        let c = w(0.0, (stern_z + shoulder_z) * 0.5);
        self.oboxes.push(OBox {
            center: c,
            yaw,
            half: Vec2::new(beam * 0.95, (shoulder_z - stern_z) * 0.5),
            y0: g + 1.4,
            y1: g + 1.5,
            roll: list,
            color: TIMBER,
            side_mat: Material::Wood,
            top_mat: Material::Wood,
            solid: false,
            ..OBox::default()
        });
        // Cabin aft of midships, dark windows, a snapped mast.
        let c = w(0.0, -len * 0.15);
        self.oboxes.push(OBox {
            center: c,
            yaw,
            half: Vec2::new(beam * 0.65, len * 0.14),
            y0: g + 1.2,
            y1: g + 2.6,
            roll: list,
            color: Vec3::new(0.8, 0.77, 0.68),
            side_mat: Material::Painted,
            top_mat: Material::Painted,
            chip: 0.08,
            solid: false,
            ..OBox::default()
        });
        self.oboxes.push(OBox {
            center: c,
            yaw,
            half: Vec2::new(beam * 0.66, len * 0.1),
            y0: g + 1.9,
            y1: g + 2.3,
            roll: list,
            color: Vec3::splat(0.05),
            side_mat: Material::Rubble,
            top_mat: Material::Rubble,
            solid: false,
            ..OBox::default()
        });
        let m = w(0.0, len * 0.1);
        let top = Vec3::new(m.x, g + rng.range(3.5, 5.5), m.y) + Vec3::new(list * 2.0, 0.0, 0.0);
        self.pipe(
            Vec3::new(m.x, g + 1.4, m.y),
            top,
            0.09,
            TIMBER,
            Material::Wood,
            false,
        );
    }
}

/// Places parts in an object's own frame (any orientation): positions and
/// rotations are converted to the map's yaw/tilt/roll boxes.
pub(super) struct Rig {
    pub pos: Vec3,
    pub rot: glam::Quat,
}

impl Rig {
    pub fn point(&self, local: Vec3) -> Vec3 {
        self.pos + self.rot * local
    }

    /// A box centered at `local`, half extents `half` (x, y, z), turned by
    /// `local_rot` within the rig.
    #[allow(clippy::too_many_arguments)]
    pub fn part(
        &self,
        map: &mut Map,
        local: Vec3,
        half: Vec3,
        local_rot: glam::Quat,
        color: Vec3,
        material: Material,
        solid: bool,
        chip: f32,
    ) {
        let c = self.point(local);
        let (yaw, tilt, roll) = (self.rot * local_rot).to_euler(glam::EulerRot::YXZ);
        map.oboxes.push(OBox {
            center: Vec2::new(c.x, c.z),
            yaw,
            tilt,
            roll,
            half: Vec2::new(half.x, half.z),
            y0: c.y - half.y,
            y1: c.y + half.y,
            color,
            side_mat: material,
            top_mat: material,
            solid,
            chip,
            ..OBox::default()
        });
    }
}
