//! Saltmarket's big set pieces, built properly instead of as blockout boxes:
//! the pier, the dam, the pipeline and the stranded boats. Each is made of
//! parts you'd recognize (planks, piles, bracing, buttresses, flanges, hull
//! strakes), with damage that makes sense.
//!
//! Collision stays simple: one or a few solid pieces per thing, the detail
//! visual only.

use glam::{Vec2, Vec3};

use super::{Map, OBox};
use crate::render::material::Material;
use crate::rng::Rng;

const TIMBER: Vec3 = Vec3::new(0.44, 0.34, 0.24);
const STEEL: Vec3 = Vec3::new(0.34, 0.22, 0.14);
const CONCRETE: Vec3 = Vec3::new(0.62, 0.6, 0.56);

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

    /// A timber pier from `root` (on the shore, deck at `deck_y`) running
    /// `len` along `dir`, `width` wide. Planks with gaps on two stringers,
    /// round piles in pairs down to the ground with cross-bracing, a railing
    /// with bays missing, bollards; the far end has collapsed, its planks
    /// hanging into the bed.
    pub(super) fn pier(
        &mut self,
        rng: &mut Rng,
        root: Vec2,
        dir: Vec2,
        len: f32,
        width: f32,
        deck_y: f32,
    ) {
        let side = Vec2::new(-dir.y, dir.x);
        let at = |m: &Map, t: f32, s: f32, y: f32| {
            let p = root + dir * t + side * s;
            (Vec3::new(p.x, y, p.y), m.ground_height(p.x, p.y))
        };
        let hw = width * 0.5;
        let intact = len * rng.range(0.72, 0.82);
        let yaw = dir.x.atan2(dir.y);
        // Stringers: two steel beams the planks sit on (solid: they carry tanks).
        for s in [-hw + 0.4, hw - 0.4] {
            let (a, _) = at(self, 0.0, s, deck_y - 0.35);
            let (b, _) = at(self, intact, s, deck_y - 0.35);
            self.segment(a, b, 0.22, STEEL, Material::Rust, true);
        }
        // Deck: one solid slab under the planks, the planks on top visual,
        // with some missing; wheels roll over the slab.
        let (mid, _) = at(self, intact * 0.5, 0.0, deck_y - 0.09);
        self.oboxes.push(OBox {
            center: Vec2::new(mid.x, mid.z),
            yaw,
            half: Vec2::new(hw - 0.05, intact * 0.5),
            y0: deck_y - 0.16,
            y1: deck_y - 0.06,
            color: TIMBER * 0.5,
            side_mat: Material::Wood,
            top_mat: Material::Wood,
            ..OBox::default()
        });
        let pitch = 0.32;
        let n = (len / pitch) as usize;
        for i in 0..n {
            let t = i as f32 * pitch + pitch * 0.5;
            if rng.chance(0.06) {
                continue; // a missing plank
            }
            let (c, g) = at(self, t, 0.0, deck_y);
            let past = t - intact;
            // Past the break, planks hang from the last stringer end, more
            // steeply the further out they were, some lying in the bed.
            let (y0, tilt, solid) = if past > 0.0 {
                if rng.chance(0.35) {
                    continue;
                }
                let droop = (past / (len - intact)).min(1.0);
                let tilt = 0.25 + 0.9 * droop;
                let y = (deck_y - past * tilt.sin() * 1.1).max(g + 0.1);
                (y, tilt, false)
            } else {
                (deck_y - 0.06, 0.0, false)
            };
            let skew = rng.range(-0.04, 0.04);
            let plank_w = rng.range(0.24, 0.29);
            self.oboxes.push(OBox {
                center: Vec2::new(c.x, c.z),
                yaw: yaw + std::f32::consts::FRAC_PI_2 + skew,
                half: Vec2::new(hw + rng.range(-0.15, 0.1), plank_w * 0.5),
                y0,
                y1: y0 + 0.07,
                tilt: tilt * if rng.chance(0.5) { 1.0 } else { -1.0 } * 0.3,
                roll: rng.range(-0.03, 0.03),
                color: TIMBER * rng.range(0.75, 1.1),
                side_mat: Material::Wood,
                top_mat: Material::Wood,
                solid,
                ..OBox::default()
            });
        }
        // Piles in pairs every 4 m, braced.
        let mut t = 2.0;
        while t < len - 0.5 {
            let mut tops = Vec::new();
            for s in [-hw + 0.25, hw - 0.25] {
                let (top, g) = at(self, t, s, deck_y - 0.4);
                if g > deck_y - 0.6 {
                    continue; // still on the shore
                }
                // Past the break the piles stand, some snapped short.
                let top = if t > intact && rng.chance(0.5) {
                    Vec3::new(top.x, g + rng.range(0.8, 2.5), top.z)
                } else {
                    top
                };
                self.pipe(
                    Vec3::new(top.x, g - 0.4, top.z),
                    top,
                    0.17,
                    TIMBER * 0.8,
                    Material::Wood,
                    true,
                );
                tops.push((top, g));
            }
            if tops.len() == 2 && t < intact {
                let ((a, ga), (b, gb)) = (tops[0], tops[1]);
                let lo = |p: Vec3, g: f32| Vec3::new(p.x, g + (p.y - g) * 0.25, p.z);
                self.segment(
                    lo(a, ga),
                    b - Vec3::Y * 0.2,
                    0.06,
                    TIMBER * 0.75,
                    Material::Wood,
                    false,
                );
                self.segment(
                    lo(b, gb),
                    a - Vec3::Y * 0.2,
                    0.06,
                    TIMBER * 0.75,
                    Material::Wood,
                    false,
                );
            }
            t += 4.0;
        }
        // Railing: posts and a top rail, bays missing.
        for s in [-hw, hw] {
            let mut t = 0.3;
            while t < intact - 0.3 {
                let next = (t + 2.0).min(intact - 0.3);
                if rng.chance(0.75) {
                    let (p, _) = at(self, t, s, deck_y);
                    self.segment(
                        p,
                        p + Vec3::Y * 1.0,
                        0.05,
                        TIMBER * 0.85,
                        Material::Wood,
                        false,
                    );
                    if rng.chance(0.8) {
                        let (q, _) = at(self, next, s, deck_y + 0.95);
                        self.segment(
                            p + Vec3::Y * 0.95,
                            q,
                            0.04,
                            TIMBER * 0.85,
                            Material::Wood,
                            false,
                        );
                    }
                }
                t = next;
            }
        }
        // Bollards near the end.
        for s in [-hw + 0.35, hw - 0.35] {
            let (p, _) = at(self, intact - 1.5, s, deck_y);
            self.pipe(
                p,
                p + Vec3::Y * 0.5,
                0.16,
                STEEL * 0.9,
                Material::Rust,
                false,
            );
        }
    }

    /// A concrete gravity dam across `x0..x1` at `z0..z1` (`z0` faces the
    /// town), `h` tall, breached over `gap`. Buttresses on the town face, a
    /// parapet along both edges of the crest walkway (broken in places), lamp
    /// posts, a gatehouse at the east end, broken edges and rubble at the
    /// breach. Returns nothing; the wall blocks are solid.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn concrete_dam(
        &mut self,
        rng: &mut Rng,
        x0: f32,
        x1: f32,
        z0: f32,
        z1: f32,
        h: f32,
        gap: (f32, f32),
    ) {
        let block = |m: &mut Map, lo: Vec3, hi: Vec3, color: Vec3, chip: f32, solid: bool| {
            m.oboxes.push(OBox {
                center: Vec2::new(lo.x + hi.x, lo.z + hi.z) * 0.5,
                half: Vec2::new(hi.x - lo.x, hi.z - lo.z) * 0.5,
                y0: lo.y,
                y1: hi.y,
                color,
                side_mat: Material::Concrete,
                top_mat: Material::Concrete,
                chip,
                solid,
                ..OBox::default()
            });
        };
        // The wall: cast in 8 m lifts, each a touch different, so the joints show.
        for (a, b) in [(x0, gap.0), (gap.1, x1)] {
            let mut x = a;
            while x < b - 0.1 {
                let nx = (x + 8.0).min(b);
                let c = CONCRETE * rng.range(0.9, 1.04);
                let broken = (nx - gap.0).abs() < 0.1 || (x - gap.1).abs() < 0.1;
                block(
                    self,
                    Vec3::new(x + 0.03, 0.0, z0),
                    Vec3::new(nx - 0.03, h, z1),
                    c,
                    0.35,
                    true,
                );
                if broken && let Some(o) = self.oboxes.last_mut() {
                    o.broken = true;
                }
                x = nx;
            }
        }
        // Buttresses on the town face, stepping down.
        let mut x = x0 + 4.0;
        while x < x1 - 2.0 {
            if x < gap.0 - 1.5 || x > gap.1 + 1.5 {
                for (k, (d, hh)) in [(1.6f32, h * 0.85), (3.0, h * 0.5)].iter().enumerate() {
                    let w = 0.9 - 0.15 * k as f32;
                    block(
                        self,
                        Vec3::new(x - w, 0.0, z0 - d),
                        Vec3::new(x + w, *hh, z0 + 0.1),
                        CONCRETE * 0.95,
                        0.3,
                        k == 0,
                    );
                }
            }
            x += 8.0;
        }
        // Parapets along the crest walkway, with gaps; lamp posts leaning.
        for (a, b) in [(x0, gap.0), (gap.1, x1)] {
            for z in [z0 + 0.25, z1 - 0.25] {
                let mut x = a + 0.3;
                while x < b - 0.3 {
                    let nx = (x + rng.range(2.5, 5.0)).min(b - 0.3);
                    if rng.chance(0.82) {
                        block(
                            self,
                            Vec3::new(x, h, z - 0.22),
                            Vec3::new(nx, h + 0.9, z + 0.22),
                            CONCRETE * 0.97,
                            0.12,
                            false,
                        );
                    }
                    x = nx + rng.range(0.0, 0.8);
                }
            }
            let mut x = a + 6.0;
            while x < b - 3.0 {
                let lean =
                    Vec3::new(rng.range(-0.25, 0.25), 1.0, rng.range(-0.15, 0.15)).normalize();
                let base = Vec3::new(x, h, z1 - 0.6);
                let top = base + lean * 4.5;
                self.pipe(base, top, 0.08, STEEL, Material::Rust, false);
                self.segment(
                    top,
                    top + Vec3::new(0.0, -0.1, -0.9),
                    0.06,
                    STEEL,
                    Material::Rust,
                    false,
                );
                x += 16.0;
            }
        }
        // Gatehouse at the east end: a small concrete hut with a flat roof.
        let gx = x1 - 6.0;
        block(
            self,
            Vec3::new(gx - 2.5, h, z0 + 1.2),
            Vec3::new(gx + 2.5, h + 3.0, z1 - 1.2),
            CONCRETE * 0.9,
            0.25,
            true,
        );
        block(
            self,
            Vec3::new(gx - 2.8, h + 3.0, z0 + 0.9),
            Vec3::new(gx + 2.8, h + 3.3, z1 - 0.9),
            CONCRETE * 0.85,
            0.2,
            false,
        );
        // The breach: big broken blocks tipped into the gap.
        for (k, side) in [(0, gap.0), (1, gap.1)] {
            for _ in 0..3 {
                let dx = if k == 0 {
                    rng.range(0.5, 3.0)
                } else {
                    -rng.range(0.5, 3.0)
                };
                let s = rng.range(0.8, 1.6);
                let z = rng.range(z0 - 2.0, z1 + 1.0);
                self.oboxes.push(OBox {
                    center: Vec2::new(side + dx, z),
                    yaw: rng.range(0.0, 3.0),
                    half: Vec2::new(s, s * rng.range(0.6, 1.0)),
                    y0: -0.4,
                    y1: s * rng.range(0.8, 1.3),
                    tilt: rng.range(-0.4, 0.4),
                    roll: rng.range(-0.3, 0.3),
                    color: CONCRETE * rng.range(0.85, 1.0),
                    side_mat: Material::Concrete,
                    top_mat: Material::Concrete,
                    chip: 0.4,
                    solid: false,
                    ..OBox::default()
                });
            }
        }
    }

    /// A pipeline segment `len` long along +z from `start`, `r` radius,
    /// half sunk: flanged joints every 6 m, the far joint sometimes split
    /// with the next run knocked askew.
    pub(super) fn pipeline_run(&mut self, rng: &mut Rng, start: Vec2, len: f32, r: f32) {
        let g = self.ground_height(start.x, start.y + len * 0.5);
        let sink = r * rng.range(0.3, 0.6);
        let yaw = rng.range(-0.05, 0.05);
        let dir = Vec3::new(yaw.sin(), 0.0, yaw.cos());
        let a = Vec3::new(start.x, g + r - sink, start.y);
        let b = a + dir * len;
        let rust = STEEL * rng.range(0.85, 1.15);
        self.pipe(a, b, r, rust, Material::Rust, true);
        // Flanges at both ends.
        for p in [a + dir * 0.15, b - dir * 0.15] {
            self.pipe(
                p - dir * 0.12,
                p + dir * 0.12,
                r * 1.18,
                rust * 0.9,
                Material::Rust,
                false,
            );
        }
        // Support saddles.
        for t in [0.3f32, 0.7] {
            let p = a.lerp(b, t);
            self.oboxes.push(OBox {
                center: Vec2::new(p.x, p.z),
                yaw,
                half: Vec2::new(r * 1.1, 0.25),
                y0: g - 0.4,
                y1: p.y - r * 0.4,
                color: CONCRETE * 0.85,
                side_mat: Material::Concrete,
                top_mat: Material::Concrete,
                chip: 0.12,
                solid: false,
                ..OBox::default()
            });
        }
    }

    /// A small boat stranded in the mud, listing: a hull of strakes tapering
    /// to a bow, a cabin with dark windows, a snapped mast. `dir` points
    /// bow-ward. One solid box for collision.
    pub(super) fn boat(&mut self, rng: &mut Rng, at: Vec2, dir: Vec2, len: f32) {
        let g = self.ground_height(at.x, at.y);
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
