//! The rocks toolkit: every loose rock in the maps (boulders, talus, fallen
//! blocks, outcrops, worked stone) is shaped here, so a new map reuses these
//! rocks instead of building its own out of boxes.
//!
//! A rock fills a box. Its surface is a lattice over the box's six faces,
//! closer together near the edges, pulled round into a rounded box: well
//! rounded for a boulder, only at the edges for a block. A boulder is then
//! broken by a few fracture faces, their edges worn round, and its underside
//! flattened where it beds into the ground. Lumps of noise go over it all,
//! and it's scaled to fill its box (a boulder 82% across). It's one welded, smooth-shaded
//! surface, its color varied over it: blotches, darker in hollows and
//! underneath, and dust lying on what faces up.
//!
//! On the map a rock is an `OBox` with `rock` set (see `boulder`, `block`
//! and `rubble`). Every rock collides as the convex hull of the surface
//! drawn, so tanks, shots and the camera meet the rock they see (a block's
//! faces are kept near flat and plumb, so plasma still banks off it nearly
//! true). `Rock::draw` puts
//! a rock straight into a mesh when it's only looks.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use glam::{Quat, Vec2, Vec3};

use super::{Hash, OBox};
use crate::render::material::Material;
use crate::render::mesh::{MeshData, Vertex};

/// What kind of rock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A lump of natural rock: rounded, broken by fracture faces, bedded
    /// flat underneath. Collides as its own hull.
    Boulder,
    /// A block of stone: square, edges worn round, faces nearly flat.
    /// Collides as its own hull.
    Block,
    /// A broken chunk (concrete, masonry, ore): square-ish, sharply broken
    /// across at odd angles. Collides as its own hull.
    Rubble,
}

/// A boulder filling this box, its foot at `y0` (bury it a little), its top
/// at `y1`. Sandstone (the Cliff material), solid; change either, or add
/// tilt or roll, with struct update syntax (`solid: false` for one a tank
/// rolls over).
pub fn boulder(center: Vec2, yaw: f32, half: Vec2, y0: f32, y1: f32, color: Vec3) -> OBox {
    OBox {
        center,
        yaw,
        half,
        y0,
        y1,
        color,
        side_mat: Material::Cliff,
        top_mat: Material::Cliff,
        rock: Some(Kind::Boulder),
        ..OBox::default()
    }
}

/// A block of stone filling this box (see `boulder`). `broken` on the
/// result gives it a ragged top.
pub fn block(center: Vec2, yaw: f32, half: Vec2, y0: f32, y1: f32, color: Vec3) -> OBox {
    OBox {
        rock: Some(Kind::Block),
        ..boulder(center, yaw, half, y0, y1, color)
    }
}

/// A broken chunk filling this box (see `boulder`): concrete, masonry. The
/// Rubble material.
pub fn rubble(center: Vec2, yaw: f32, half: Vec2, y0: f32, y1: f32, color: Vec3) -> OBox {
    OBox {
        side_mat: Material::Rubble,
        top_mat: Material::Rubble,
        rock: Some(Kind::Rubble),
        ..boulder(center, yaw, half, y0, y1, color)
    }
}

/// One rock, in its own frame: centered, filling the box `half` across.
#[derive(Clone, Copy, Debug)]
pub struct Rock {
    pub half: Vec3,
    pub kind: Kind,
    /// Seeds its shape and its blotches.
    pub key: u32,
    pub color: Vec3,
    pub material: Material,
    /// Broken off on top: a block's top ragged, a boulder's flat (a butte).
    pub broken: bool,
}

/// Lattice spacing, m: about this far apart over a face.
const CELL: f32 = 0.7;

/// A rock reaching more than this to the side of its middle, m, is scenery:
/// none tanks fight round are so wide.
const HUGE: f32 = 8.0;

/// The surface: welded points (rock frame), and triangles facing out.
struct Surface {
    pos: Vec<Vec3>,
    nrm: Vec<Vec3>,
    /// How far each point was lumped out (+) or in (-), relative to the
    /// lumps' size.
    lump: Vec<f32>,
    tris: Vec<[u32; 3]>,
}

/// Where the lattice points sit along one axis of a box `h` across from its
/// middle, rounded `r` at its edges: closer near the ends, where it turns
/// round the edge.
fn stations(h: f32, r: f32) -> Vec<f32> {
    let core = (h - r).max(0.0);
    // Capped, so a mesa on the horizon isn't a million points.
    let n = ((2.0 * core / CELL).ceil() as usize).clamp(1, 14);
    let mut out = vec![-h];
    if r > 0.01 {
        out.push(-h + r * 0.35);
    }
    for i in 0..=n {
        out.push(-core + 2.0 * core * i as f32 / n as f32);
    }
    if r > 0.01 {
        out.push(h - r * 0.35);
    }
    out.push(h);
    out.dedup_by(|a, b| (*a - *b).abs() < 1e-4);
    out
}

/// A soft max(e, 0): 0 below `-w`, `e` above `w`, a smooth curve between.
fn soft_excess(e: f32, w: f32) -> f32 {
    if e <= -w {
        0.0
    } else if e >= w {
        e
    } else {
        (e + w) * (e + w) / (4.0 * w)
    }
}

impl Rock {
    /// The rock an `OBox` is drawn as, if it's one.
    pub fn of(o: &OBox) -> Option<Rock> {
        let kind = o.rock?;
        let mut h = Hash::of(Vec3::new(o.center.x, o.y0 + o.y1, o.center.y));
        Some(Rock {
            half: Vec3::new(o.half.x, (o.y1 - o.y0) * 0.5, o.half.y),
            kind,
            key: (h.f() * 16_777_216.0) as u32,
            color: o.color,
            material: o.side_mat,
            broken: o.broken,
        })
    }

    /// How far across its box it reaches, along each axis.
    fn reach(&self) -> Vec3 {
        match self.kind {
            Kind::Boulder => Vec3::new(0.82, 1.0, 0.82),
            Kind::Rubble => Vec3::new(0.9, 1.0, 0.9),
            Kind::Block => Vec3::ONE,
        }
    }

    fn surface(&self) -> Surface {
        let h = self.half.max(Vec3::splat(0.05));
        let small = h.min_element();
        let mut rng = Hash(self.key.wrapping_mul(0x9E37_79B9) | 1);
        let r = match self.kind {
            Kind::Boulder => small * (0.45 + 0.2 * rng.f()),
            Kind::Block => small * (0.14 + 0.08 * rng.f()),
            Kind::Rubble => small * (0.06 + 0.06 * rng.f()),
        };
        let core = h - Vec3::splat(r);
        // A horizon's rock is cut straight in toward its middle, so that
        // however its cuts meet, its surface can't fold over itself (and
        // z-fight). The rest are cut square to each face, which keeps the
        // surface closer to its hull, what they collide as.
        let huge = h.x.max(h.z) > HUGE;
        // Fracture faces, in the box's unit frame: a normal, and how far out
        // along it the rock is cut.
        let mut cuts: Vec<(Vec3, f32, f32)> = Vec::new();
        match self.kind {
            Kind::Boulder => {
                // Spread round it, so every side has one.
                let n = 4 + (rng.f() * 4.0) as usize;
                let spin = rng.f() * std::f32::consts::TAU;
                for k in 0..n {
                    let a = spin
                        + (k as f32 + 0.6 * (rng.f() - 0.5)) / n as f32 * std::f32::consts::TAU;
                    let y = rng.f() * 1.4 - 0.3;
                    let n = Vec3::new(a.cos(), y, a.sin()).normalize();
                    cuts.push((n, 0.62 + 0.22 * rng.f(), 0.05));
                }
                // Its corners in plan broken off too: it stands on about as
                // much ground as an ellipse in its box, no box corners.
                for (x, z) in [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)] {
                    let n = Vec3::new(x, (rng.f() - 0.5) * 0.6, z).normalize();
                    cuts.push((n, 0.86 + 0.1 * rng.f(), 0.05));
                }
                // Bedded flat underneath.
                cuts.push((-Vec3::Y, 0.72, 0.08));
                // Broken off flat on top (a butte, a mesa).
                if self.broken {
                    let tip = Vec3::new(rng.f() - 0.5, 0.0, rng.f() - 0.5) * 0.15;
                    cuts.push(((Vec3::Y + tip).normalize(), 0.62 + 0.15 * rng.f(), 0.05));
                }
            }
            Kind::Block => {
                let sign = |rng: &mut Hash| if rng.f() < 0.5 { -1.0 } else { 1.0 };
                // Corners knocked off, and an edge or two broken back.
                for _ in 0..2 + (rng.f() * 3.0) as usize {
                    let n = Vec3::new(
                        sign(&mut rng),
                        sign(&mut rng) * (0.5 + 0.5 * rng.f()),
                        sign(&mut rng),
                    );
                    let (n, len) = (n.normalize(), n.length());
                    cuts.push((n, len * (0.85 + 0.09 * rng.f()), 0.06));
                }
                for _ in 0..1 + (rng.f() * 2.0) as usize {
                    let n = match (rng.f() * 3.0) as usize {
                        0 => Vec3::new(sign(&mut rng), sign(&mut rng), 0.0),
                        1 => Vec3::new(0.0, sign(&mut rng), sign(&mut rng)),
                        _ => Vec3::new(sign(&mut rng), 0.0, sign(&mut rng)),
                    };
                    cuts.push((n.normalize(), 1.41 * (0.86 + 0.08 * rng.f()), 0.06));
                }
                if self.broken {
                    for _ in 0..2 {
                        let a = rng.f() * std::f32::consts::TAU;
                        let n = Vec3::new(a.cos() * 0.6, 1.0, a.sin() * 0.6).normalize();
                        cuts.push((n, 0.8 + 0.12 * rng.f(), 0.06));
                    }
                }
            }
            Kind::Rubble => {
                // Broken across every which way, sharp.
                for _ in 0..3 + (rng.f() * 3.0) as usize {
                    let n = Vec3::new(rng.f() - 0.5, rng.f() - 0.5, rng.f() - 0.5);
                    let n = n.normalize_or(Vec3::Y);
                    // How far a corner of the box sticks out along it.
                    let corner = n.abs().element_sum();
                    cuts.push((n, corner * (0.55 + 0.3 * rng.f()), 0.03));
                }
            }
        }
        // Narrower at the top than the foot, a little.
        let taper = match self.kind {
            Kind::Boulder => 0.1 + 0.15 * rng.f(),
            // Squared stone: a face stays near plumb.
            Kind::Block => 0.03 * rng.f(),
            Kind::Rubble => 0.0,
        };
        let (amp, freq) = match self.kind {
            Kind::Boulder => ((small * 0.09).min(0.35), 1.1 / (small + 0.3)),
            Kind::Block => (small * 0.05, 1.4 / (small + 0.3)),
            Kind::Rubble => (small * 0.04, 1.6 / (small + 0.3)),
        };
        let key = self.key;

        // The lattice: every station point on the box's surface, once.
        let st = [stations(h.x, r), stations(h.y, r), stations(h.z, r)];
        let dims = [st[0].len(), st[1].len(), st[2].len()];
        let mut index = vec![u32::MAX; dims[0] * dims[1] * dims[2]];
        let at = |i: usize, j: usize, k: usize| (i * dims[1] + j) * dims[2] + k;
        let mut pos = Vec::new();
        let mut lump = Vec::new();
        let mut lat = Vec::new();
        for i in 0..dims[0] {
            for j in 0..dims[1] {
                for k in 0..dims[2] {
                    let edge = i == 0
                        || j == 0
                        || k == 0
                        || i + 1 == dims[0]
                        || j + 1 == dims[1]
                        || k + 1 == dims[2];
                    if !edge {
                        continue;
                    }
                    let q = Vec3::new(st[0][i], st[1][j], st[2][k]);
                    let inner = q.clamp(-core, core);
                    let n = (q - inner).normalize_or(q.normalize_or(Vec3::Y));
                    let mut u = (inner + n * r) / h;
                    let narrow = 1.0 - taper * (u.y + 1.0) * 0.5;
                    u.x *= narrow;
                    u.z *= narrow;
                    let mut cut = 0.0f32;
                    for &(cn, d, w) in &cuts {
                        let out = u.dot(cn);
                        let e = soft_excess(out - d, w);
                        if huge && e > 0.0 {
                            u *= (out - e) / out;
                        } else {
                            u -= cn * e;
                        }
                        cut = cut.max((e / w).min(1.0));
                    }
                    let p = u * h;
                    let l = noise(p * freq, key) + 0.45 * noise(p * freq * 2.7, key + 1);
                    // Fracture faces are fresher: less lumpy.
                    let l = l * (1.0 - 0.7 * cut);
                    index[at(i, j, k)] = pos.len() as u32;
                    pos.push(p + n * amp * l);
                    lump.push(l);
                    lat.push(q);
                }
            }
        }
        // Fill its box: a block exactly; a boulder top to bottom, and most
        // of the way across (as much ground as an ellipse in the box).
        let ext = pos.iter().fold(Vec3::splat(1e-4), |m, p| m.max(p.abs()));
        let fit = h * self.reach() / ext;
        for p in &mut pos {
            *p *= fit;
        }

        // Each face's quads, wound to face out.
        let mut tris = Vec::new();
        for axis in 0..3 {
            let (b, c) = ((axis + 1) % 3, (axis + 2) % 3);
            for side in [0, dims[axis] - 1] {
                for s in 0..dims[b] - 1 {
                    for t in 0..dims[c] - 1 {
                        let id = |s: usize, t: usize| {
                            let mut ijk = [0usize; 3];
                            ijk[axis] = side;
                            ijk[b] = s;
                            ijk[c] = t;
                            index[at(ijk[0], ijk[1], ijk[2])]
                        };
                        let q = [id(s, t), id(s + 1, t), id(s + 1, t + 1), id(s, t + 1)];
                        let pair = if (s + t) % 2 == 0 {
                            [[q[0], q[1], q[2]], [q[0], q[2], q[3]]]
                        } else {
                            [[q[0], q[1], q[3]], [q[1], q[2], q[3]]]
                        };
                        for mut tri in pair {
                            let l = tri.map(|v| lat[v as usize]);
                            let out = (l[1] - l[0]).cross(l[2] - l[0]);
                            if out.dot(l[0] + l[1] + l[2]) < 0.0 {
                                tri.swap(1, 2);
                            }
                            tris.push(tri);
                        }
                    }
                }
            }
        }
        let mut nrm = vec![Vec3::ZERO; pos.len()];
        for t in &tris {
            let p = t.map(|v| pos[v as usize]);
            let f = (p[1] - p[0]).cross(p[2] - p[0]);
            for &v in t {
                nrm[v as usize] += f;
            }
        }
        for n in &mut nrm {
            *n = n.normalize_or(Vec3::Y);
        }
        Surface {
            pos,
            nrm,
            lump,
            tris,
        }
    }

    /// What tells this rock's shape apart from every other's.
    pub fn id(&self) -> [u32; 6] {
        [
            self.half.x.to_bits(),
            self.half.y.to_bits(),
            self.half.z.to_bits(),
            self.key,
            self.kind as u32,
            self.broken as u32,
        ]
    }

    /// Every point of its surface, in its own frame: their hull is the rock.
    /// Worked out once per rock: colliders and the bots' map ask for them
    /// every time they're built.
    pub fn points(&self) -> Vec<Vec3> {
        type Cache = Mutex<HashMap<[u32; 6], Arc<Vec<Vec3>>>>;
        static CACHE: LazyLock<Cache> = LazyLock::new(Default::default);
        let id = self.id();
        let hit = CACHE.lock().unwrap().get(&id).cloned();
        let pts = hit.unwrap_or_else(|| {
            let pts = Arc::new(self.surface().pos);
            CACHE.lock().unwrap().insert(id, pts.clone());
            pts
        });
        pts.to_vec()
    }

    /// Its color at world point `p`, its normal there `n` (world), lumped
    /// `lump` there; `dust` lies on what faces up, and drifts against its
    /// foot where the ground (`ground`) is close below.
    fn tint(&self, p: Vec3, n: Vec3, lump: f32, dust: Option<Dust>) -> Vec3 {
        let k = self.key;
        let blotch = 0.9 + 0.16 * noise(p * 0.35, k + 3) + 0.07 * noise(p * 1.3, k + 4);
        // Hollows and the underside hold shadow; what stands out catches
        // the weather, a little paler.
        let hollow = 1.0 + 0.12 * lump.clamp(-1.0, 1.0);
        let under = if n.y < -0.2 {
            0.8
        } else {
            1.0 + 0.05 * n.y.max(0.0)
        };
        let c = self.color * blotch * hollow * under;
        let Some(d) = dust else { return c };
        let patches = smoothstep(
            -0.2,
            0.4,
            noise(p * 0.45, k + 7) + 0.5 * noise(p * 1.2, k + 8),
        );
        let lying = smoothstep(0.55, 0.9, n.y) * 0.55 * patches;
        let drift = smoothstep(0.7, 0.05, p.y - (d.ground)(Vec2::new(p.x, p.z))) * 0.7;
        c.lerp(d.color, lying.max(drift))
    }

    /// Its mesh, placed at `at` turned by `rot`, into `m` in world space.
    pub fn draw(&self, m: &mut MeshData, at: Vec3, rot: Quat, dust: Option<Dust>) {
        self.mesh(m, at, rot, dust, true);
    }

    /// Its mesh into `m`: in world space if `world`, else in its own frame
    /// (colored as it stands at `at`, turned by `rot`).
    pub fn mesh(&self, m: &mut MeshData, at: Vec3, rot: Quat, dust: Option<Dust>, world: bool) {
        let s = self.surface();
        let base = m.verts.len() as u32;
        for k in 0..s.pos.len() {
            let (wp, wn) = (at + rot * s.pos[k], rot * s.nrm[k]);
            let c = self.tint(wp, wn, s.lump[k], dust);
            let (p, n) = if world {
                (wp, wn)
            } else {
                (s.pos[k], s.nrm[k])
            };
            m.verts.push(Vertex {
                pos: p.to_array(),
                normal: n.to_array(),
                color: c.extend(self.material.id()).to_array(),
            });
        }
        m.indices
            .extend(s.tris.iter().flat_map(|t| t.map(|v| base + v)));
    }
}

/// Dust or sand lying on rocks: its color, and the ground it drifts up from.
#[derive(Clone, Copy)]
pub struct Dust<'a> {
    pub color: Vec3,
    pub ground: &'a dyn Fn(Vec2) -> f32,
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn hash3(x: i32, y: i32, z: i32, k: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8DA6_B343)
        ^ (y as u32).wrapping_mul(0xD816_3841)
        ^ (z as u32).wrapping_mul(0xCB1A_B31F)
        ^ k.wrapping_mul(0x9E37_79B9);
    h = (h ^ (h >> 15)).wrapping_mul(0x2C1B_3C6D);
    h = (h ^ (h >> 12)).wrapping_mul(0x297A_2D39);
    (h ^ (h >> 15)) as f32 / u32::MAX as f32
}

/// Value noise in 3D, -1..1.
fn noise(p: Vec3, k: u32) -> f32 {
    let i = p.floor();
    let f = p - i;
    let u = f * f * (Vec3::splat(3.0) - 2.0 * f);
    let (x, y, z) = (i.x as i32, i.y as i32, i.z as i32);
    let at = |dx, dy, dz| hash3(x + dx, y + dy, z + dz, k);
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let a = lerp(
        lerp(at(0, 0, 0), at(1, 0, 0), u.x),
        lerp(at(0, 1, 0), at(1, 1, 0), u.x),
        u.y,
    );
    let b = lerp(
        lerp(at(0, 0, 1), at(1, 0, 1), u.x),
        lerp(at(0, 1, 1), at(1, 1, 1), u.x),
        u.y,
    );
    lerp(a, b, u.z) * 2.0 - 1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rock(kind: Kind, half: Vec3, key: u32) -> Rock {
        Rock {
            half,
            kind,
            key,
            color: Vec3::splat(0.5),
            material: Material::Rubble,
            broken: key.is_multiple_of(2),
        }
    }

    /// Every rock fills its box, never through a face: top to bottom, and
    /// across (a boulder most of the way).
    #[test]
    fn a_rock_fills_its_box() {
        for key in 0..40 {
            for kind in [Kind::Boulder, Kind::Block, Kind::Rubble] {
                let half = Vec3::new(0.4 + key as f32 * 0.1, 0.3 + (key % 7) as f32 * 0.4, 1.2);
                let r = rock(kind, half, key);
                let ext = r.points().iter().fold(Vec3::ZERO, |m, p| m.max(p.abs()));
                assert!(
                    (ext - half * r.reach()).abs().max_element() < 1e-3,
                    "{kind:?} {half}: reaches {ext}"
                );
            }
        }
    }

    /// One closed surface facing out: every edge shared by exactly two
    /// triangles, running opposite ways, and it encloses the right volume.
    #[test]
    fn a_rock_is_closed_and_faces_out() {
        for key in 0..20 {
            for kind in [Kind::Boulder, Kind::Block, Kind::Rubble] {
                let half = Vec3::new(2.0, 1.0 + key as f32 * 0.2, 1.4);
                let s = rock(kind, half, key).surface();
                let mut edges = std::collections::HashMap::new();
                for t in &s.tris {
                    for e in 0..3 {
                        *edges.entry((t[e], t[(e + 1) % 3])).or_insert(0) += 1;
                    }
                }
                for (&(a, b), &n) in &edges {
                    assert_eq!(n, 1, "{kind:?}: edge {a}-{b} used {n} times one way");
                    assert_eq!(edges.get(&(b, a)), Some(&1), "{kind:?}: edge {a}-{b} open");
                }
                let vol: f32 = s
                    .tris
                    .iter()
                    .map(|t| {
                        let p = t.map(|v| s.pos[v as usize]);
                        p[0].dot(p[1].cross(p[2])) / 6.0
                    })
                    .sum();
                let full = 8.0 * half.x * half.y * half.z;
                let (lo, hi) = match kind {
                    Kind::Boulder => (0.18, 0.7),
                    Kind::Block => (0.7, 1.0),
                    Kind::Rubble => (0.3, 0.9),
                };
                assert!(
                    vol > lo * full && vol < hi * full,
                    "{kind:?}: {vol:.2} m³ of a {full:.2} m³ box"
                );
            }
        }
    }

    /// A horizon's rock never folds back over itself: no two triangles
    /// sharing an edge face nearly opposite ways (#701: undersides and tops
    /// did, and z-fought).
    #[test]
    fn a_huge_rock_doesnt_fold_over_itself() {
        for key in 0..60u32 {
            let w = 10.0 + (key % 21) as f32;
            let half = Vec3::new(w, 6.0 + (key % 15) as f32, w * 0.7);
            for kind in [Kind::Boulder, Kind::Block, Kind::Rubble] {
                let s = rock(kind, half, key * 7919 + 1).surface();
                let mut sides = HashMap::new();
                for t in &s.tris {
                    let p = t.map(|v| s.pos[v as usize]);
                    let n = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero();
                    for e in 0..3 {
                        sides.insert((t[e], t[(e + 1) % 3]), n);
                    }
                }
                for (&(a, b), n) in &sides {
                    assert!(
                        n.dot(sides[&(b, a)]) > -0.9,
                        "{kind:?} {half} key {key}: folds at {}",
                        s.pos[a as usize]
                    );
                }
            }
        }
    }

    /// A block's faces stay close to its box (squared stone), and
    /// a boulder is round, nothing like one.
    #[test]
    fn blocks_are_square_and_boulders_round() {
        let half = Vec3::new(2.5, 2.0, 3.0);
        let mid_face = |r: &Rock| {
            // How far out the surface is straight out from the middle of
            // each side face, as a share of the box.
            let s = r.surface();
            [Vec3::X, Vec3::Z, -Vec3::X, -Vec3::Z]
                .iter()
                .map(|&d| {
                    s.pos
                        .iter()
                        .filter(|p| (*p / half).normalize().dot(d) > 0.97)
                        .map(|p| p.dot(d) / half.dot(d.abs()))
                        .fold(0.0, f32::max)
                })
                .collect::<Vec<_>>()
        };
        let corner = |r: &Rock| {
            r.points()
                .iter()
                .map(|p| (*p / half).abs().min_element())
                .fold(0.0, f32::max)
        };
        for key in 0..10 {
            let b = rock(Kind::Block, half, key * 2 + 1);
            assert!(mid_face(&b).iter().all(|&f| f > 0.9), "block {key}");
            assert!(corner(&b) > 0.75, "block {key}: corners worn right off");
            let r = rock(Kind::Boulder, half, key);
            assert!(corner(&r) < 0.8, "boulder {key}: has a box's corners");
        }
    }

    /// What collides is what's drawn: the hull of a rock's surface (its
    /// collider) is nowhere far off the surface itself, whatever the kind
    /// (#284: blocks collided as their box, out past their worn corners).
    #[test]
    fn a_rock_collides_where_its_drawn() {
        use rapier3d::parry::query::{PointQuery, Ray, RayCast};
        use rapier3d::parry::shape::{SharedShape, TriMesh};
        for (kind, half) in [
            (Kind::Boulder, Vec3::new(3.5, 4.0, 3.2)),
            (Kind::Boulder, Vec3::new(1.0, 0.8, 0.8)),
            (Kind::Block, Vec3::new(2.4, 3.0, 3.4)),
            (Kind::Block, Vec3::new(1.0, 0.6, 1.5)),
            (Kind::Rubble, Vec3::new(1.0, 0.7, 0.8)),
        ] {
            // Collides as its hull (see `rock_hull` in `sim`), not its box.
            assert!(
                OBox {
                    rock: Some(kind),
                    ..OBox::default()
                }
                .is_rock(),
                "{kind:?}"
            );
            for key in 0..10u32 {
                let r = rock(kind, half, key);
                let s = r.surface();
                let drawn = TriMesh::new(s.pos.clone(), s.tris.clone()).unwrap();
                let hull = SharedShape::convex_hull(&r.points()).unwrap();
                // Points all over the hull, each this far from the surface.
                let n = 1500;
                let worst = (0..n)
                    .map(|i| {
                        let y = 1.0 - 2.0 * (i as f32 + 0.5) / n as f32;
                        let a = i as f32 * 2.399_963;
                        let w = (1.0 - y * y).sqrt();
                        let d = Vec3::new(a.cos() * w, y, a.sin() * w);
                        let ray = Ray::new(d * 50.0, -d);
                        // (A ray skimming a hull's edge can slip past it.)
                        hull.cast_local_ray(&ray, 100.0, true).map_or(0.0, |t| {
                            drawn.distance_to_local_point(ray.point_at(t), true)
                        })
                    })
                    .fold(0.0, f32::max);
                assert!(
                    worst < 0.25,
                    "{kind:?} {half} key {key}: collider {worst:.2} m off the rock"
                );
            }
        }
    }
}
