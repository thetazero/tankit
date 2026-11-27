//! Finding z-fighting in tests: pairs of triangles that lie in the same plane
//! and overlap, so the depth test can't tell which one is in front and the
//! two flicker through each other.
//!
//! Only overlaps that can be seen count:
//! - faces back to back are a solid set flush on a surface, and the solid
//!   itself always stands between the camera and the shared patch;
//! - faces that shade the same (same color, same normal) look the same
//!   whichever one wins;
//! - a patch buried inside a solid, under the ground, or deep in a crevice
//!   too tight to see into never shows.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

use glam::{IVec3, Mat4, Vec2, Vec3, Vec4};

use super::mesh::{Instance, MeshData, MeshId};

/// Faces closer to each other than this count as one plane. The camera's
/// depth buffer (reverse Z, 32-bit float) resolves about d × 1.2e-7: 0.07 mm
/// at the far plane, 600 m off, and a few hundredths of a millimetre at
/// fighting range.
pub const PLANE_TOL: f32 = 0.0001;
/// Overlaps smaller than this (m²) are hairline seams, not visible fights.
pub const MIN_AREA: f32 = 2e-5;
/// How far a probe ray from an overlap must get to call it seen.
const OPEN: f32 = 1.0;

/// A cheap hasher for the buckets (small integer keys, millions of lookups):
/// std's SipHash dominated the run time. Multiply-rotate per word, as in FxHash.
#[derive(Default)]
struct Fx(u64);

impl Hasher for Fx {
    fn write(&mut self, bytes: &[u8]) {
        for c in bytes.chunks(8) {
            let mut w = [0u8; 8];
            w[..c.len()].copy_from_slice(c);
            self.write_u64(u64::from_le_bytes(w));
        }
    }

    fn write_u32(&mut self, i: u32) {
        self.write_u64(i as u64);
    }

    fn write_i32(&mut self, i: i32) {
        self.write_u64(i as u32 as u64);
    }

    fn write_usize(&mut self, i: usize) {
        self.write_u64(i as u64);
    }

    fn write_u64(&mut self, i: u64) {
        self.0 = (self.0.rotate_left(5) ^ i).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

type FastMap<K, V> = HashMap<K, V, BuildHasherDefault<Fx>>;

struct Tri {
    p: [Vec3; 3],
    n: Vec3,
    d: f32,
    /// The shading normal (the vertex normal, which lighting uses).
    shade: Vec3,
    /// Which part this came from.
    part: usize,
    /// Shaded color: vertex albedo and material times the instance tint
    /// (w: material id + 100 × glow).
    color: Vec4,
}

/// Every triangle of a scene, in world space.
pub struct Soup {
    tris: Vec<Tri>,
    parts: Vec<String>,
    /// Size of the spatial buckets triangles are sorted into, m: about the
    /// size of the scene's smaller parts.
    space: f32,
    /// Report fights per cube of this size (m) as well as per pair of
    /// parts; 0 lumps each pair together. A map is one part, so it needs this.
    cell: f32,
    /// Ground height at (x, z), if the scene has ground: nothing below it
    /// is ever seen.
    ground: Option<Box<dyn Fn(f32, f32) -> f32>>,
}

/// Two parts' coplanar faces overlapping where they can be seen.
#[derive(Debug, Clone)]
pub struct Fight {
    pub a: String,
    pub b: String,
    /// Visible overlapping area summed over the triangle pairs, m².
    pub area: f32,
    /// Where one of the overlaps is, in world space.
    pub at: Vec3,
    /// The two faces there: color (see `Tri::color`) and shading normal.
    pub faces: [(Vec4, Vec3); 2],
}

impl Soup {
    /// A scene the size of a tank: fights reported per pair of parts.
    pub fn new() -> Soup {
        Soup {
            tris: Vec::new(),
            parts: Vec::new(),
            space: 0.5,
            cell: 0.0,
            ground: None,
        }
    }

    /// A map, with ground at `ground(x, z)`: fights reported per 4 m cube.
    pub fn map(ground: impl Fn(f32, f32) -> f32 + 'static) -> Soup {
        Soup {
            space: 4.0,
            cell: 4.0,
            ground: Some(Box::new(ground)),
            ..Soup::new()
        }
    }

    /// Add `mesh` drawn with `model` and `tint` (as an instance would be).
    pub fn add(&mut self, name: &str, mesh: &MeshData, model: Mat4, tint: Vec4) {
        let part = self.parts.len();
        let nm = glam::Mat3::from_mat4(model).inverse().transpose();
        self.parts.push(name.to_string());
        for t in mesh.indices.chunks_exact(3) {
            let v = |i: u32| &mesh.verts[i as usize];
            let p = [0, 1, 2].map(|k| model.transform_point3(Vec3::from(v(t[k]).pos)));
            let n = (p[1] - p[0]).cross(p[2] - p[0]);
            if n.length() < 1e-7 {
                continue;
            }
            let n = n.normalize();
            let c = Vec4::from(v(t[0]).color);
            let color = (c.truncate() * tint.truncate()).extend(c.w + 100.0 * tint.w);
            self.tris.push(Tri {
                p,
                n,
                shade: (nm * Vec3::from(v(t[0]).normal)).normalize_or_zero(),
                d: n.dot(p[0]),
                part,
                color,
            });
        }
    }

    /// Add a frame's instances, naming each part by `name(mesh)`.
    pub fn add_instances(
        &mut self,
        meshes: &[MeshData],
        instances: &[(MeshId, Instance)],
        name: impl Fn(MeshId) -> String,
    ) {
        for (id, inst) in instances {
            let model = Mat4::from_cols_array_2d(&inst.model);
            self.add(
                &name(*id),
                &meshes[id.0 as usize],
                model,
                Vec4::from(inst.tint),
            );
        }
    }

    /// Every pair of parts with coplanar faces overlapping where it shows,
    /// largest overlap first.
    pub fn fights(&self) -> Vec<Fight> {
        // Every triangle in each spatial cube its bounds touch, also for the
        // probe rays. Then, a cube at a time, in each plane bucket of that
        // cube it's near: by normal (either way round) and by the cube
        // center's distance from its plane. Two coplanar triangles share
        // the bucket of either one.
        const N_CELL: f32 = 0.05;
        const N_TOL: f32 = 0.01;
        const D_CELL: f32 = 0.25;
        let d_tol = PLANE_TOL + N_TOL * self.space * 1.8;
        // Buckets centered on round values, so the common normals (axis
        // aligned) sit in the middle of one rather than astride two.
        let bucket = |x: f32, cell: f32| (x / cell + 0.5).floor() as i32;
        let span = |x: f32, tol: f32, cell: f32| bucket(x - tol, cell)..=bucket(x + tol, cell);
        let center = |c: IVec3| (c.as_vec3() + 0.5) * self.space;
        // Faces are looked up with their normal turned to lean this way, so
        // they need only be kept the ways round that do (or might, within
        // the tolerance).
        let lean = Vec3::new(1.0, 0.37, 0.11);
        // A bucket as one number.
        let key = |x: i32, y: i32, z: i32, k: i32| {
            [x, y, z, k]
                .into_iter()
                .fold(0u64, |h, v| (h << 16) | (v as u16) as u64)
        };
        // Faces that don't fight, wherever they are.
        let harmless = |a: &Tri, b: &Tri| {
            let facing = a.shade.dot(b.shade);
            facing < 0.0 || (facing > 0.995 && (a.color - b.color).abs().max_element() < 0.01)
        };
        let mut space: FastMap<IVec3, Vec<usize>> = FastMap::default();
        for (i, t) in self.tris.iter().enumerate() {
            for c in cubes(&t.p, PLANE_TOL, self.space) {
                space.entry(c).or_default().push(i);
            }
        }
        // Pairs that may fight, each found in every cube both are in.
        let mut pairs: Vec<(u32, u32)> = Vec::new();
        let mut planes: Vec<(u64, u32)> = Vec::new();
        // The first face looked up in each bucket (by its key, hashed): a
        // face is only ever found by one before it, so it need only be kept
        // where one is. Few are, and sorting them all was most of the work.
        const SLOTS: usize = 1 << 12;
        let mut first = vec![u32::MAX; SLOTS];
        let slot = |k: u64| (k.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 52) as usize;
        let mut asked: Vec<u64> = Vec::new();
        for (&c, here) in &space {
            if here.len() < 2 {
                continue;
            }
            planes.clear();
            for &k in &asked {
                first[slot(k)] = u32::MAX;
            }
            asked.clear();
            for &i in here {
                let a = &self.tris[i];
                let (n, d) = if a.n.dot(lean) < 0.0 {
                    (-a.n, -a.d)
                } else {
                    (a.n, a.d)
                };
                let k = key(
                    bucket(n.x, N_CELL),
                    bucket(n.y, N_CELL),
                    bucket(n.z, N_CELL),
                    bucket(n.dot(center(c)) - d, D_CELL),
                );
                asked.push(k);
                let f = &mut first[slot(k)];
                *f = (*f).min(i as u32);
            }
            for &i in here {
                let t = &self.tris[i];
                for (n, d) in [(t.n, t.d), (-t.n, -t.d)] {
                    if n.dot(lean) < -2.0 * N_TOL {
                        continue;
                    }
                    let dist = n.dot(center(c)) - d;
                    for x in span(n.x, N_TOL, N_CELL) {
                        for y in span(n.y, N_TOL, N_CELL) {
                            for z in span(n.z, N_TOL, N_CELL) {
                                for k in span(dist, d_tol, D_CELL) {
                                    let k = key(x, y, z, k);
                                    if first[slot(k)] < i as u32 {
                                        planes.push((k, i as u32));
                                    }
                                }
                            }
                        }
                    }
                }
            }
            planes.sort_unstable();
            for (&i, &k) in here.iter().zip(&asked) {
                let a = &self.tris[i];
                // (The other of each pair is later in the list.)
                let from = planes.partition_point(|&e| e <= (k, i as u32));
                for &(_, j) in planes[from..].iter().take_while(|e| e.0 == k) {
                    let b = &self.tris[j as usize];
                    if !harmless(a, b) && may_overlap(a, b) {
                        pairs.push((i as u32, j));
                    }
                }
            }
        }
        pairs.sort_unstable();
        pairs.dedup();
        let mut found: FastMap<(usize, usize, IVec3), Fight> = FastMap::default();
        for (i, j) in pairs {
            let (a, b) = (&self.tris[i as usize], &self.tris[j as usize]);
            let Some((area, mid)) = overlap(a, b) else {
                continue;
            };
            if area < 1e-7 || !self.seen(mid, (a.shade + b.shade).normalize_or_zero(), &space) {
                continue;
            }
            let (pa, pb) = if a.part <= b.part {
                (a.part, b.part)
            } else {
                (b.part, a.part)
            };
            let cell = if self.cell > 0.0 {
                (mid / self.cell).floor().as_ivec3()
            } else {
                IVec3::ZERO
            };
            let f = found.entry((pa, pb, cell)).or_insert_with(|| Fight {
                a: self.parts[pa].clone(),
                b: self.parts[pb].clone(),
                area: 0.0,
                at: mid,
                faces: [(a.color, a.shade), (b.color, b.shade)],
            });
            f.area += area;
        }
        let mut out: Vec<Fight> = found.into_values().filter(|f| f.area >= MIN_AREA).collect();
        out.sort_by(|x, y| y.area.total_cmp(&x.area));
        out
    }

    /// Whether a point on a surface facing `n` can be seen from anywhere:
    /// probe rays fanned out from just in front of it, and it shows if one
    /// of them gets clear before hitting anything, unless it's underground
    /// or inside a solid. From outside every solid no ray can reach the
    /// inside of a face first, so a few that do mean it's inside one (a
    /// few, not one, for rays that graze an edge).
    fn seen(&self, p: Vec3, n: Vec3, space: &FastMap<IVec3, Vec<usize>>) -> bool {
        let q = p + n * 0.01;
        if let Some(g) = &self.ground
            && q.y < g(q.x, q.z) - 0.005
        {
            return false;
        }
        let near: Vec<usize> = {
            let mut v: Vec<usize> = cubes(&[q, q, q], OPEN, self.space)
                .filter_map(|c| space.get(&c))
                .flatten()
                .copied()
                .collect();
            v.sort_unstable();
            v.dedup();
            v
        };
        let u = n.any_orthonormal_vector();
        let w = n.cross(u);
        let mut inside = 0;
        let mut open = false;
        for (tilt, k) in [(0.0f32, 1), (0.5, 6), (1.0, 8), (1.35, 8)] {
            for i in 0..k {
                let a = i as f32 / k as f32 * std::f32::consts::TAU + tilt;
                let dir = n * tilt.cos() + (u * a.cos() + w * a.sin()) * tilt.sin();
                let hit = near
                    .iter()
                    .filter_map(|&j| {
                        let t = &self.tris[j];
                        ray_tri(q, dir, &t.p).map(|d| (d, t.shade.dot(dir) > 0.0))
                    })
                    .min_by(|x, y| x.0.total_cmp(&y.0));
                match hit {
                    None => open = true,
                    Some((d, back)) => {
                        if back {
                            inside += 1;
                        } else if d > OPEN {
                            open = true;
                        }
                    }
                }
            }
        }
        open && inside < 3
    }
}

/// The `size` cubes of space the bounds of `p`, grown by `pad`, touch.
fn cubes(p: &[Vec3; 3], pad: f32, size: f32) -> impl Iterator<Item = IVec3> {
    let (lo, hi) = bounds(p);
    let lo = ((lo - pad) / size).floor().as_ivec3();
    let hi = ((hi + pad) / size).floor().as_ivec3();
    (lo.x..=hi.x).flat_map(move |x| {
        (lo.y..=hi.y).flat_map(move |y| (lo.z..=hi.z).map(move |z| IVec3::new(x, y, z)))
    })
}

/// Distance along `dir` (unit) from `o` to triangle `p`, if it hits.
fn ray_tri(o: Vec3, dir: Vec3, p: &[Vec3; 3]) -> Option<f32> {
    let (e1, e2) = (p[1] - p[0], p[2] - p[0]);
    let h = dir.cross(e2);
    let det = e1.dot(h);
    if det.abs() < 1e-9 {
        return None;
    }
    let s = o - p[0];
    let u = s.dot(h) / det;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = dir.dot(q) / det;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = e2.dot(q) / det;
    (t > 0.0 && t < OPEN).then_some(t)
}

/// Whether `a` and `b` lie in one plane, their bounds touching.
fn may_overlap(a: &Tri, b: &Tri) -> bool {
    if a.n.dot(b.n).abs() < 0.99996 {
        return false;
    }
    let off = |t: &Tri, p: Vec3| (t.n.dot(p) - t.d).abs();
    if b.p.iter().any(|&p| off(a, p) > PLANE_TOL) || a.p.iter().any(|&p| off(b, p) > PLANE_TOL) {
        return false;
    }
    let (amin, amax) = bounds(&a.p);
    let (bmin, bmax) = bounds(&b.p);
    !(amin.cmpgt(bmax + PLANE_TOL).any() || bmin.cmpgt(amax + PLANE_TOL).any())
}

/// Area where `a` and `b` overlap and the middle of it, if they lie in one
/// plane.
fn overlap(a: &Tri, b: &Tri) -> Option<(f32, Vec3)> {
    if !may_overlap(a, b) {
        return None;
    }
    // Into the plane's 2D frame, both wound counter-clockwise, and clip.
    let u = a.n.any_orthonormal_vector();
    let w = a.n.cross(u);
    let flat = |t: &Tri| {
        let mut q: Vec<Vec2> = t.p.iter().map(|&p| Vec2::new(p.dot(u), p.dot(w))).collect();
        if signed_area(&q) < 0.0 {
            q.reverse();
        }
        q
    };
    let mut poly = flat(b);
    let clip = flat(a);
    for i in 0..3 {
        let (e0, e1) = (clip[i], clip[(i + 1) % 3]);
        let side = |p: Vec2| (e1 - e0).perp_dot(p - e0);
        let mut next = Vec::with_capacity(poly.len() + 1);
        for k in 0..poly.len() {
            let (p, q) = (poly[k], poly[(k + 1) % poly.len()]);
            let (sp, sq) = (side(p), side(q));
            if sp >= 0.0 {
                next.push(p);
            }
            if (sp >= 0.0) != (sq >= 0.0) {
                next.push(p + (q - p) * (sp / (sp - sq)));
            }
        }
        poly = next;
        if poly.len() < 3 {
            return None;
        }
    }
    let mid = poly.iter().copied().sum::<Vec2>() / poly.len() as f32;
    Some((signed_area(&poly).abs(), u * mid.x + w * mid.y + a.n * a.d))
}

fn bounds(p: &[Vec3; 3]) -> (Vec3, Vec3) {
    (p[0].min(p[1]).min(p[2]), p[0].max(p[1]).max(p[2]))
}

fn signed_area(p: &[Vec2]) -> f32 {
    let mut s = 0.0;
    for i in 0..p.len() {
        s += p[i].perp_dot(p[(i + 1) % p.len()]);
    }
    s * 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: Vec4 = Vec4::new(1.0, 0.0, 0.0, 0.0);
    const BLUE: Vec4 = Vec4::new(0.0, 0.0, 1.0, 0.0);

    /// A unit square at height `y` from x0 to x1, facing `n` (±Y).
    fn quad(y: f32, x0: f32, x1: f32, n: Vec3, color: Vec4) -> MeshData {
        let mut m = MeshData::default();
        m.quad(
            [
                Vec3::new(x0, y, 0.0),
                Vec3::new(x1, y, 0.0),
                Vec3::new(x1, y, 1.0),
                Vec3::new(x0, y, 1.0),
            ],
            n,
            color,
        );
        m
    }

    fn fights(quads: &[MeshData]) -> Vec<Fight> {
        let mut s = Soup::new();
        for (i, m) in quads.iter().enumerate() {
            s.add(
                &format!("{i}"),
                m,
                Mat4::IDENTITY,
                Vec4::new(1.0, 1.0, 1.0, 0.0),
            );
        }
        s.fights()
    }

    #[test]
    fn finds_overlapping_coplanar_faces_only() {
        // Half overlapping in one plane: a fight of 0.5 m².
        let f = fights(&[
            quad(0.0, 0.0, 1.0, Vec3::Y, RED),
            quad(0.0, 0.5, 1.5, Vec3::Y, BLUE),
        ]);
        assert_eq!(f.len(), 1);
        assert!((f[0].area - 0.5).abs() < 1e-3, "{f:?}");
        // Side by side, sharing an edge.
        assert!(
            fights(&[
                quad(0.0, 0.0, 1.0, Vec3::Y, RED),
                quad(0.0, 1.0, 2.0, Vec3::Y, BLUE)
            ])
            .is_empty()
        );
        // A centimetre apart.
        assert!(
            fights(&[
                quad(0.0, 0.0, 1.0, Vec3::Y, RED),
                quad(0.01, 0.0, 1.0, Vec3::Y, BLUE)
            ])
            .is_empty()
        );
        // Same color, same way up: the same whichever wins.
        assert!(
            fights(&[
                quad(0.0, 0.0, 1.0, Vec3::Y, RED),
                quad(0.0, 0.5, 1.5, Vec3::Y, RED)
            ])
            .is_empty()
        );
        // Back to back: one solid sitting on another.
        assert!(
            fights(&[
                quad(0.0, 0.0, 1.0, Vec3::Y, RED),
                quad(0.0, 0.5, 1.5, -Vec3::Y, BLUE)
            ])
            .is_empty()
        );
    }

    #[test]
    fn hidden_overlaps_dont_count() {
        // Two faces fighting inside a closed box can't be seen.
        let mut s = Soup::new();
        let white = Vec4::new(1.0, 1.0, 1.0, 0.0);
        s.add(
            "0",
            &quad(0.0, 0.0, 1.0, Vec3::Y, RED),
            Mat4::IDENTITY,
            white,
        );
        s.add(
            "1",
            &quad(0.0, 0.5, 1.5, Vec3::Y, BLUE),
            Mat4::IDENTITY,
            white,
        );
        let mut cube = MeshData::default();
        cube.add_box(
            Vec3::new(-0.2, -0.5, -0.2),
            Vec3::new(1.7, 0.5, 1.2),
            white,
            white,
        );
        s.add("box", &cube, Mat4::IDENTITY, white);
        assert!(s.fights().is_empty());
        // Nor under the ground.
        let mut s = Soup::map(|_, _| 1.0);
        s.add(
            "0",
            &quad(0.0, 0.0, 1.0, Vec3::Y, RED),
            Mat4::IDENTITY,
            white,
        );
        s.add(
            "1",
            &quad(0.0, 0.5, 1.5, Vec3::Y, BLUE),
            Mat4::IDENTITY,
            white,
        );
        assert!(s.fights().is_empty());
    }
}
