//! Saltmarket's edge: what boxes the town in. The reservoir was dammed in a
//! dry valley, so sandstone cliffs close the north, east and south sides; the
//! west, where the valley opens and the main road ran out, was walled off in
//! the war with a barricade of stacked shipping containers.
//!
//! The cliff is one sculpted rock face (like the Sanctum's mesa) whose
//! colliders follow it; the containers are solid boxes. Both are too tall to
//! climb and leave no gaps, so a tank driving at any edge stops against
//! something it can see. The arena's own boundary still stands, hidden
//! inside them, as a backstop.

use std::f32::consts::{PI, TAU};

use glam::{Vec2, Vec3};

use super::{Map, OBox};
use crate::render::material::Material;
use crate::render::mesh::{MeshData, Vertex};
use crate::rng::Rng;

/// Where the cliff's foot stands, m in from the arena's edge (the face
/// wanders a little either side of it).
const FOOT: f32 = 1.4;

/// Sandstone strata.
const STRATA: [Vec3; 5] = [
    Vec3::new(0.7, 0.5, 0.34),
    Vec3::new(0.52, 0.34, 0.23),
    Vec3::new(0.8, 0.64, 0.46),
    Vec3::new(0.46, 0.3, 0.21),
    Vec3::new(0.66, 0.44, 0.29),
];

const SAND: Vec3 = Vec3::new(0.72, 0.62, 0.45);

/// Faded container paint.
const CONTAINERS: [Vec3; 5] = [
    Vec3::new(0.42, 0.2, 0.14),
    Vec3::new(0.24, 0.31, 0.38),
    Vec3::new(0.3, 0.35, 0.25),
    Vec3::new(0.52, 0.4, 0.22),
    Vec3::new(0.46, 0.45, 0.42),
];

/// One side of the arena: its edge line runs from `origin` along `along`;
/// `inward` points into the arena.
struct Side {
    origin: Vec2,
    along: Vec2,
    inward: Vec2,
}

impl Side {
    fn yaw(&self) -> f32 {
        // OBox local +x is (cos yaw, -sin yaw) on the ground.
        (-self.along.y).atan2(self.along.x)
    }

    /// How far a box reaches past the edge line into the arena.
    fn reach(&self, o: &OBox) -> f32 {
        o.corners()
            .iter()
            .map(|c| (*c - self.origin).dot(self.inward))
            .fold(f32::MIN, f32::max)
    }

    /// Slides the box across the edge so its inner face is `inset` inside.
    fn seat(&self, o: &mut OBox, inset: f32) {
        let r = self.reach(o);
        o.center += self.inward * (inset - r);
    }

    /// Slides the box across the edge so even the shallower end of its
    /// (slightly turned) inner face is `inset` inside: it covers the edge
    /// line all along its length.
    fn seat_whole(&self, o: &mut OBox, inset: f32) {
        let mut r: Vec<f32> = o
            .corners()
            .iter()
            .map(|c| (*c - self.origin).dot(self.inward))
            .collect();
        r.sort_by(f32::total_cmp);
        o.center += self.inward * (inset - r[2]);
    }
}

impl Map {
    /// Cliffs on the north, east and south; the container barricade on the
    /// west. `ground` is the play ground's height, `scenery` the ground
    /// beyond the arena (the plateau the cliffs hold up).
    pub(super) fn valley_edge(
        &mut self,
        rng: &mut Rng,
        ground: fn(f32, f32) -> f32,
        scenery: fn(f32, f32) -> f32,
    ) {
        self.cliff(ground, scenery);
        self.barricade(rng, ground);
    }

    /// The valley wall: one sculpted sandstone face (see `sculpt_cliff`) from
    /// behind the barricade's south end, along the south, east and north
    /// edges with rounded corners, to behind its north end. It collides
    /// through thin invisible slabs that follow the face at tank height, so a
    /// tank stops where the rock is.
    fn cliff(&mut self, ground: fn(f32, f32) -> f32, scenery: fn(f32, f32) -> f32) {
        let s = self.size;
        let (lo, hi) = (FOOT, s - FOOT);
        // Corners: (point, radius). The two ends turn away behind the
        // barricade, so the face's ends are out of sight.
        let path = rounded(
            &[
                (Vec2::new(-34.0, -18.0), 0.0),
                (Vec2::new(-10.0, lo), 10.0),
                (Vec2::new(hi, lo), 18.0),
                (Vec2::new(hi, hi), 18.0),
                (Vec2::new(-10.0, hi), 10.0),
                (Vec2::new(-34.0, s + 18.0), 0.0),
            ],
            0.6,
        );
        let inside = move |p: Vec2| p.cmpge(Vec2::ZERO).all() && p.cmple(Vec2::splat(s)).all();
        let floor = move |p: Vec2| {
            if inside(p) {
                ground(p.x, p.y)
            } else {
                scenery(p.x, p.y)
            }
        };
        let face = sculpt_cliff(&mut self.sculpted, &path, &floor, scenery, s);
        // Colliders: pairs of columns, each a slab from the face's most
        // forward point at tank height back into the rock.
        for k in (0..path.len() - 1).step_by(2) {
            let j = (k + 2).min(path.len() - 1);
            let (a, b) = (face[k], face[j]);
            let (na, nb) = (path[k].1, path[j].1);
            let pts = [a.0, b.0, a.0 - na * 30.0, b.0 - nb * 30.0];
            let min = pts.iter().fold(Vec2::splat(f32::MAX), |m, p| m.min(*p));
            let max = pts.iter().fold(Vec2::splat(f32::MIN), |m, p| m.max(*p));
            let (y0, y1) = (a.1.min(b.1) - 12.0, a.2.max(b.2) + 2.0);
            self.solids.push(super::Solid {
                min: Vec3::new(min.x, y0, min.y),
                max: Vec3::new(max.x, y1, max.y),
            });
        }
    }

    /// The west barricade across the valley mouth: 40-foot shipping
    /// containers end to end along the edge, stacked two high and three in
    /// places, a few shoved askew; steel tank traps where the roads ran out.
    fn barricade(&mut self, rng: &mut Rng, ground: fn(f32, f32) -> f32) {
        let s = self.size;
        let side = Side {
            origin: Vec2::ZERO,
            along: Vec2::Y,
            inward: Vec2::X,
        };
        let (wide, tall) = (2.44f32, 2.6f32);
        // Lowest and highest ground under a container.
        let foot = |z0: f32, z1: f32| {
            (0..=4)
                .map(|k| ground(0.0, (z0 + (z1 - z0) * k as f32 / 4.0).clamp(0.0, s)))
                .fold((f32::MAX, f32::MIN), |(lo, hi), g| (lo.min(g), hi.max(g)))
        };
        // Sun-faded paint, or rusted through to bare steel.
        let paint = |rng: &mut Rng| {
            if rng.chance(0.3) {
                (
                    Vec3::new(0.42, 0.27, 0.17) * rng.range(0.8, 1.1),
                    Material::Rust,
                )
            } else {
                let c = CONTAINERS[rng.index(CONTAINERS.len())] * rng.range(0.8, 1.05);
                (c, Material::Painted)
            }
        };
        let mut z = -6.0;
        while z < s + 4.0 {
            // Bottom row: tight end to end, sunk a little into the sand.
            // 40-foot containers, or 20-foot ones where the ground climbs
            // too fast under a long one (up onto the north ridge).
            let mut len = 12.2;
            let (mut g, mut high) = foot(z, z + len);
            if high - g > 0.6 {
                len = 6.06;
                (g, high) = foot(z, z + len);
            }
            let (color, side_mat) = paint(rng);
            let mut base = OBox {
                center: Vec2::new(0.0, z + len * 0.5),
                yaw: side.yaw() + rng.range(-0.03, 0.03),
                half: Vec2::new(len * 0.5, wide * 0.5),
                y0: g - 0.15,
                y1: g - 0.15 + tall,
                color,
                side_mat,
                top_mat: Material::Rust,
                ..OBox::default()
            };
            side.seat_whole(&mut base, rng.range(0.5, 1.0));
            self.oboxes.push(base);
            // A second one stacked on top (always, so the wall is too tall
            // to climb), often shifted along and turned a touch.
            let (color, side_mat) = paint(rng);
            let mut upper = OBox {
                center: base.center + Vec2::new(0.0, rng.range(-0.4, 0.4)),
                yaw: base.yaw + rng.range(-0.08, 0.08),
                y0: base.y1,
                y1: base.y1 + tall,
                color,
                side_mat,
                ..base
            };
            side.seat_whole(&mut upper, rng.range(0.2, 0.8));
            self.oboxes.push(upper);
            // A third here and there, and always where the ground rises
            // under the stack (toward the north ridge) and two would leave it
            // low on the uphill end.
            let steep = high - g > 0.3;
            if steep || rng.chance(0.3) {
                let (color, side_mat) = paint(rng);
                let shift = if steep { 0.4 } else { len * 0.25 };
                let mut third = OBox {
                    center: upper.center + Vec2::new(-0.3, rng.range(-shift, shift)),
                    yaw: upper.yaw + rng.range(-0.15, 0.15) * shift / 3.0,
                    y0: upper.y1,
                    y1: upper.y1 + tall,
                    color,
                    side_mat,
                    ..upper
                };
                if steep {
                    side.seat_whole(&mut third, rng.range(0.0, 0.5));
                } else {
                    side.seat(&mut third, rng.range(-0.5, 0.8));
                }
                self.oboxes.push(third);
            }
            // Jammed end to end: no gap a shell (or a sand-drift climb) gets through.
            z += len - rng.range(0.1, 0.3);
        }
        // Tank traps (three crossed girders) where the roads met the wall.
        for (cz, n) in [(108.0f32, 3), (56.0, 2), (22.0, 2)] {
            for k in 0..n {
                let c = Vec2::new(
                    rng.range(4.0, 6.0),
                    cz + (k as f32 - (n - 1) as f32 * 0.5) * 4.5,
                );
                self.hedgehog(rng, c, ground(c.x, c.y));
            }
        }
    }

    /// A Czech hedgehog: three steel girders crossed at their middles.
    fn hedgehog(&mut self, rng: &mut Rng, at: Vec2, g: f32) {
        let steel = Vec3::new(0.2, 0.16, 0.13);
        let mid = Vec3::new(at.x, g + 0.75, at.y);
        let turn = rng.range(0.0, std::f32::consts::TAU);
        let axes = [
            Vec3::new(1.0, 0.75, 0.0),
            Vec3::new(-0.5, 0.75, 0.87),
            Vec3::new(-0.5, 0.75, -0.87),
        ];
        for a in axes {
            let a = glam::Quat::from_rotation_y(turn) * a.normalize() * 1.1;
            self.segment(mid - a, mid + a, 0.12, steel, Material::Rust, true);
        }
    }
}

/// A straight run from start to end, or an arc round (center, radius).
type Piece = (Vec2, Vec2, Option<(Vec2, f32)>);

/// A path through `corners` (point, radius: the first and last radius are
/// ignored), each corner rounded into an arc, sampled every `step` m:
/// (point, unit normal to its left).
fn rounded(corners: &[(Vec2, f32)], step: f32) -> Vec<(Vec2, Vec2)> {
    // Straight runs and arcs, as (start, end, Option<(center, radius)>).
    let mut pieces: Vec<Piece> = Vec::new();
    let mut from = corners[0].0;
    for w in corners.windows(3) {
        let (a, (b, r), c) = (w[0].0, w[1], w[2].0);
        let (d0, d1) = ((b - a).normalize(), (c - b).normalize());
        let turn = d0.angle_to(d1);
        let cut = r * (turn.abs() * 0.5).tan();
        let (p0, p1) = (b - d0 * cut, b + d1 * cut);
        let left = d0.perp() * turn.signum();
        pieces.push((from, p0, None));
        pieces.push((p0, p1, Some((p0 + left * r, r))));
        from = p1;
    }
    pieces.push((from, corners[corners.len() - 1].0, None));
    let mut out = Vec::new();
    for (a, b, arc) in pieces {
        match arc {
            None => {
                let len = a.distance(b);
                let n = (len / step).ceil().max(1.0) as usize;
                let nrm = (b - a).normalize().perp();
                for i in 0..n {
                    out.push((a.lerp(b, i as f32 / n as f32), nrm));
                }
            }
            Some((c, r)) => {
                let (t0, t1) = ((a - c).to_angle(), (b - c).to_angle());
                let mut dt = t1 - t0;
                if dt > PI {
                    dt -= TAU;
                } else if dt < -PI {
                    dt += TAU;
                }
                let n = (r * dt.abs() / step).ceil().max(1.0) as usize;
                for i in 0..n {
                    let t = t0 + dt * i as f32 / n as f32;
                    let p = c + Vec2::from_angle(t) * r;
                    // The left normal of a left turn points at the center.
                    let nrm = (c - p).normalize() * dt.signum();
                    out.push((p, nrm));
                }
            }
        }
    }
    let last = corners[corners.len() - 1].0;
    let nrm = out.last().map(|q| q.1).unwrap_or(Vec2::Y);
    out.push((last, nrm));
    out
}

/// A cheap hash of two integers to 0..1.
fn hash(i: u32, k: u32) -> f32 {
    let h = i.wrapping_mul(0x9E37_79B9) ^ k.wrapping_mul(0x85EB_CA6B);
    let h = (h ^ (h >> 15)).wrapping_mul(0x2C1B_3C6D);
    (h ^ (h >> 12)) as f32 / u32::MAX as f32
}

/// Smooth value noise along a line, one bump per unit of `x`, seeded by `k`.
fn noise(x: f32, k: u32) -> f32 {
    let x = x + 1000.0;
    let (i, f) = (x.floor(), x.fract());
    let f = f * f * (3.0 - 2.0 * f);
    hash(i as u32, k) * (1.0 - f) + hash(i as u32 + 1, k) * f
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Sculpt the cliff along `path` (foot line, normal toward the open side)
/// into `m`, as one smooth-shaded surface like the Sanctum's mesa: a lip at
/// the plateau's edge with a cap running back under the plateau's sand;
/// uneven fluted columns down the face; bands of harder rock set in or out,
/// with ledges along their feet; the face leaning back a little, and its
/// foot buried in drifted sand. Returns, per path point, the face's most
/// forward point at tank height and the floor and lip heights there.
fn sculpt_cliff(
    m: &mut MeshData,
    path: &[(Vec2, Vec2)],
    floor: &dyn Fn(Vec2) -> f32,
    scenery: fn(f32, f32) -> f32,
    size: f32,
) -> Vec<(Vec2, f32, f32)> {
    const BAND: f32 = 5.5;
    const STEP: f32 = 0.7;
    // Back from the foot line to where the lip's height is read, and where
    // the cap tucks under the plateau.
    const BACK: f32 = 20.0;
    // How far forward of (positive) or back from the foot line the face
    // stands, `u` m along the path and `depth` m down from the lip, on a
    // face `tall` m from lip to floor.
    let face = |u: f32, depth: f32, tall: f32| {
        let up = (tall - depth).max(0.0);
        let phase = u * TAU / 3.2 + 6.0 * (u * 0.031).sin() + 3.0 * (u * 0.077 + 1.0).sin();
        let groove = (0.5 + 0.5 * (phase + 0.6 * (depth * 0.13 + u * 0.4).sin()).cos()).powf(0.6);
        // Flutes deepen up the face; the sand-scoured foot is smoother.
        let flute = (groove * (0.4 + 1.2 * noise(u / 14.0, 7))
            + 0.25 * (u * 2.3 + depth * 0.07).sin())
            * (0.35 + 0.65 * smoothstep(1.0, 6.0, up));
        let band = (depth / BAND).floor() as u32;
        let f = (depth / BAND).fract();
        let set = 0.9 * (noise(u / 18.0, band + 20) - 0.5);
        let ledge =
            0.9 * (1.4 * noise(u / 9.0, band + 40) - 0.35).max(0.0) * smoothstep(0.75, 0.97, f);
        // The caprock overhangs a little, the face leans back as it rises.
        let undercut = 0.9 * smoothstep(0.3, 1.5, depth) * (1.0 - smoothstep(3.0, 7.0, depth));
        let lean = -0.12 * (up - 2.0);
        let flare = 0.8 * smoothstep(1.0, -2.0, up);
        -0.6 + flute + set + ledge + lean + flare - undercut
    };
    let shade = |u: f32, depth: f32| {
        let b = depth / BAND + 0.2 * (noise(u / 25.0, 3) - 0.5);
        let (i, f) = (b.floor().max(0.0) as usize, b.fract());
        let blend = smoothstep(0.85, 1.0, f);
        let band = STRATA[(i + 3) % STRATA.len()].lerp(STRATA[(i + 4) % STRATA.len()], blend);
        let streak = 0.9 + 0.14 * noise(u / 2.5, 5) + 0.06 * noise(u / 9.0, 9);
        band * streak
    };

    let n = path.len();
    // Arc length along the path.
    let mut along = vec![0.0f32; n];
    for i in 1..n {
        along[i] = along[i - 1] + path[i].0.distance(path[i - 1].0);
    }
    // The lip's height: the plateau's, read the same distance out past the
    // arena's edge all round (round a corner that's farther back), so the
    // corners don't sag.
    let lip_at = |p: Vec2, nrm: Vec2| {
        let out = |q: Vec2| (q - q.clamp(Vec2::ZERO, Vec2::splat(size))).length();
        let mut t = BACK;
        while out(p - nrm * t) < BACK - FOOT && t < 3.0 * BACK {
            t += 0.5;
        }
        let q = p - nrm * t;
        scenery(q.x, q.y)
    };
    let lips: Vec<f32> = path.iter().map(|&(p, nrm)| lip_at(p, nrm)).collect();
    let floors: Vec<f32> = path.iter().map(|&(p, _)| floor(p)).collect();
    // Rows: two for the cap and lip, then down the face to below the floor.
    let deepest = (0..n).map(|i| lips[i] - floors[i]).fold(0.0, f32::max) + 3.0;
    let mut depths = Vec::new();
    let mut d = 0.6;
    while d < deepest {
        depths.push(d);
        d += STEP;
    }
    let rows = 2 + depths.len();
    let mut pos = vec![Vec3::ZERO; n * rows];
    let mut col = vec![Vec3::ZERO; n * rows];
    let mut sandy = vec![false; n * rows];
    let mut fronts = Vec::with_capacity(n);
    for i in 0..n {
        let (p, nrm) = path[i];
        let u = along[i];
        let (lip, fl) = (lips[i], floors[i]);
        let tall = lip - fl;
        let mut front = (f32::MIN, p);
        for r in 0..rows {
            let jd = if r < 2 {
                0.0
            } else {
                hash(i as u32, r as u32 * 2 + 1) - 0.5
            };
            let (y, out, depth) = match r {
                0 => {
                    let q = p - nrm * BACK;
                    (scenery(q.x, q.y) + 0.05, -BACK, 0.0)
                }
                1 => (lip + 0.15, face(u, 0.0, tall), 0.0),
                _ => {
                    // Rows run below the floor where the face is short, so
                    // the foot is always buried.
                    let dp = (depths[r - 2] + jd * STEP * 0.5).max(0.3);
                    (lip - dp, face(u, dp, tall), dp)
                }
            };
            let q = p + nrm * out;
            pos[i * rows + r] = Vec3::new(q.x, y, q.y);
            if y > fl + 0.4 && y < fl + 4.5 && out > front.0 {
                front = (out, q);
            }
            sandy[i * rows + r] = r == 0;
            let drift = smoothstep(4.0, 0.8, y - fl);
            col[i * rows + r] = if r == 0 {
                SAND * 0.95
            } else {
                shade(u, depth.max(0.01)).lerp(SAND * 0.95, drift)
            };
        }
        fronts.push((front.1, fl, lip));
    }
    // The cap takes its own copy of the lip row, so the edge stays crisp.
    let first_dup = pos.len();
    for i in 0..n {
        pos.push(pos[i * rows + 1]);
        col.push(SAND * 0.95);
        sandy.push(true);
    }
    let mut tris: Vec<[usize; 3]> = Vec::new();
    for i in 0..n - 1 {
        let nm = (path[i].1 + path[i + 1].1).normalize_or_zero();
        let out = Vec3::new(nm.x, 0.0, nm.y);
        for r in 0..rows - 1 {
            let cap = r == 0;
            let v = |i: usize, r: usize| {
                if cap && r == 1 {
                    first_dup + i
                } else {
                    i * rows + r
                }
            };
            let q = [v(i, r), v(i + 1, r), v(i + 1, r + 1), v(i, r + 1)];
            let pair = if (i + r) % 2 == 0 {
                [[q[0], q[1], q[2]], [q[0], q[2], q[3]]]
            } else {
                [[q[0], q[1], q[3]], [q[1], q[2], q[3]]]
            };
            let want = if cap { Vec3::Y } else { out };
            tris.extend(pair.into_iter().map(|t| orient(&pos, t, want)));
        }
    }
    let mut nrm = vec![Vec3::ZERO; pos.len()];
    for t in &tris {
        let f = (pos[t[1]] - pos[t[0]]).cross(pos[t[2]] - pos[t[0]]);
        for &k in t {
            nrm[k] += f;
        }
    }
    let base = m.verts.len() as u32;
    for k in 0..pos.len() {
        let mat = if sandy[k] {
            Material::Sand
        } else {
            Material::Rubble
        };
        m.verts.push(Vertex {
            pos: pos[k].to_array(),
            normal: nrm[k].normalize_or_zero().to_array(),
            color: col[k].extend(mat.id()).to_array(),
        });
    }
    for t in tris {
        m.indices.extend(t.iter().map(|&k| base + k as u32));
    }
    fronts
}

/// Triangle `t` wound to face along `want`.
fn orient(pos: &[Vec3], mut t: [usize; 3], want: Vec3) -> [usize; 3] {
    if (pos[t[1]] - pos[t[0]])
        .cross(pos[t[2]] - pos[t[0]])
        .dot(want)
        < 0.0
    {
        t.swap(1, 2);
    }
    t
}
