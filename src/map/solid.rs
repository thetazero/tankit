//! Ground as a solid: rock wherever a signed distance is negative, meshed
//! into one surface that's drawn, driven on and asked for its floors. Unlike
//! a `Terrain` it can stand sheer, overhang and be dug into, at any angle.

use glam::{IVec3, Vec2, Vec3};

/// What's rock: negative in it, positive in the air, about the distance (m)
/// to the surface near it.
pub trait Rock {
    /// The distances up the column over `p`, at each of `ys`.
    fn column(&self, p: Vec2, ys: &[f32], out: &mut [f32]);
}

/// The surface of a solid, as a mesh.
#[derive(Debug, Default)]
pub struct Surface {
    pub verts: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    /// As it's drawn.
    pub tris: Vec<[u32; 3]>,
    /// As it's driven on: the same surface to within `SLACK`, in far fewer
    /// triangles.
    pub driven: (Vec<Vec3>, Vec<[u32; 3]>),
    /// The color of its rock, where it stands steep.
    pub rock: Vec3,
    /// Which floors (triangles near enough level) are over each square of
    /// the ground.
    lo: Vec2,
    nx: usize,
    nz: usize,
    over: Vec<Vec<u32>>,
}

/// How far what's driven on may be from what's drawn, m.
const SLACK: f32 = 0.02;
/// The squares triangles are sorted into, m.
const SQUARE: f32 = 1.0;
/// A floor isn't steeper than this (its normal's y).
const FLOOR: f32 = 0.35;

impl Surface {
    /// Meshes `rock` from `lo` to `hi` in cubes `cell` across.
    pub fn of(rock: &dyn Rock, lo: Vec3, hi: Vec3, cell: f32) -> Surface {
        let n = ((hi - lo) / cell).ceil().as_ivec3() + 1;
        let ys: Vec<f32> = (0..n.y).map(|y| lo.y + y as f32 * cell).collect();
        let id = |c: IVec3| ((c.z * n.x + c.x) * n.y + c.y) as usize;
        let mut d = vec![0.0f32; (n.x * n.y * n.z) as usize];
        for z in 0..n.z {
            for x in 0..n.x {
                let p = Vec2::new(lo.x, lo.z) + Vec2::new(x as f32, z as f32) * cell;
                let at = id(IVec3::new(x, 0, z));
                rock.column(p, &ys, &mut d[at..at + n.y as usize]);
            }
        }
        let mut s = Surface::default();
        // A vertex in every cube the surface passes through, in the middle
        // of where it crosses the cube's edges.
        let mut ids = vec![u32::MAX; d.len()];
        let corner = |k: i32| IVec3::new(k & 1, (k >> 1) & 1, (k >> 2) & 1);
        for z in 0..n.z - 1 {
            for x in 0..n.x - 1 {
                for y in 0..n.y - 1 {
                    let c = IVec3::new(x, y, z);
                    let (mut sum, mut hits) = (Vec3::ZERO, 0.0);
                    for k in 0..8 {
                        for bit in [1, 2, 4] {
                            if k & bit != 0 {
                                continue;
                            }
                            let (a, b) = (corner(k), corner(k | bit));
                            let (da, db) = (d[id(c + a)], d[id(c + b)]);
                            if (da < 0.0) != (db < 0.0) {
                                sum += a.as_vec3().lerp(b.as_vec3(), da / (da - db));
                                hits += 1.0;
                            }
                        }
                    }
                    if hits > 0.0 {
                        ids[id(c)] = s.verts.len() as u32;
                        s.verts.push(lo + (c.as_vec3() + sum / hits) * cell);
                    }
                }
            }
        }
        // A face across every grid edge the surface crosses, between the
        // four cubes round it.
        for z in 0..n.z - 1 {
            for x in 0..n.x - 1 {
                for y in 0..n.y - 1 {
                    let c = IVec3::new(x, y, z);
                    for (axis, u, v) in [
                        (IVec3::X, IVec3::Y, IVec3::Z),
                        (IVec3::Y, IVec3::Z, IVec3::X),
                        (IVec3::Z, IVec3::X, IVec3::Y),
                    ] {
                        if c.dot(u) < 1 || c.dot(v) < 1 {
                            continue;
                        }
                        let (da, db) = (d[id(c)], d[id(c + axis)]);
                        if (da < 0.0) == (db < 0.0) {
                            continue;
                        }
                        let q = [c - u - v, c - v, c, c - u].map(|c| ids[id(c)]);
                        if q.contains(&u32::MAX) {
                            continue;
                        }
                        let q = if da < 0.0 {
                            q
                        } else {
                            [q[0], q[3], q[2], q[1]]
                        };
                        s.tris.push([q[0], q[1], q[2]]);
                        s.tris.push([q[0], q[2], q[3]]);
                    }
                }
            }
        }
        s.finish(lo, hi);
        s
    }

    /// Its normals, and which triangles are over each square.
    fn finish(&mut self, lo: Vec3, hi: Vec3) {
        self.normals = vec![Vec3::ZERO; self.verts.len()];
        for t in &self.tris {
            let [a, b, c] = t.map(|k| self.verts[k as usize]);
            let n = (b - a).cross(c - a);
            for &k in t {
                self.normals[k as usize] += n;
            }
        }
        for n in &mut self.normals {
            *n = n.normalize_or(Vec3::Y);
        }
        self.driven = self.thinned();
        self.lo = Vec2::new(lo.x, lo.z);
        self.nx = ((hi.x - lo.x) / SQUARE).ceil() as usize + 1;
        self.nz = ((hi.z - lo.z) / SQUARE).ceil() as usize + 1;
        self.over = vec![Vec::new(); self.nx * self.nz];
        for i in 0..self.driven.1.len() {
            let [a, b, c] = self.driven_at(i as u32);
            let n = (b - a).cross(c - a);
            if n.y <= n.length() * FLOOR {
                continue;
            }
            let p = [a, b, c].map(|v| Vec2::new(v.x, v.z));
            let a = self.square(p[0].min(p[1]).min(p[2]));
            let b = self.square(p[0].max(p[1]).max(p[2]));
            for z in a.1..=b.1 {
                for x in a.0..=b.0 {
                    self.over[z * self.nx + x].push(i as u32);
                }
            }
        }
    }

    fn driven_at(&self, i: u32) -> [Vec3; 3] {
        self.driven.1[i as usize].map(|k| self.driven.0[k as usize])
    }

    /// The surface in as few triangles as keep to it within `SLACK`.
    fn thinned(&self) -> (Vec<Vec3>, Vec<[u32; 3]>) {
        let positions: Vec<[f32; 3]> = self.verts.iter().map(|v| v.to_array()).collect();
        let bytes = bytemuck::cast_slice(&positions);
        let adapter = meshopt::VertexDataAdapter::new(bytes, 12, 0).expect("positions");
        let thin = meshopt::simplify(
            self.tris.as_flattened(),
            &adapter,
            0,
            SLACK,
            meshopt::SimplifyOptions::ErrorAbsolute | meshopt::SimplifyOptions::LockBorder,
            None,
        );
        let mut at = vec![u32::MAX; self.verts.len()];
        let mut verts = Vec::new();
        let ids: Vec<u32> = thin
            .into_iter()
            .map(|k| {
                let slot = &mut at[k as usize];
                if *slot == u32::MAX {
                    *slot = verts.len() as u32;
                    verts.push(self.verts[k as usize]);
                }
                *slot
            })
            .collect();
        (
            verts,
            ids.chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect(),
        )
    }

    fn square(&self, p: Vec2) -> (usize, usize) {
        let q = ((p - self.lo) / SQUARE).max(Vec2::ZERO);
        (
            (q.x as usize).min(self.nx - 1),
            (q.y as usize).min(self.nz - 1),
        )
    }

    /// Every floor over `p`, in no order (one on a shared edge may come
    /// twice).
    fn over(&self, p: Vec2) -> impl Iterator<Item = f32> + '_ {
        let (x, z) = self.square(p);
        self.over[z * self.nx + x]
            .iter()
            .filter_map(move |&i| self.floor_of(i, p))
    }

    /// Every floor over `p`, the highest first.
    pub fn floors(&self, p: Vec2) -> Vec<f32> {
        let mut out: Vec<f32> = self.over(p).collect();
        out.sort_by(|a, b| b.total_cmp(a));
        out.dedup_by(|a, b| (*a - *b).abs() < 1e-3);
        out
    }

    fn floor_of(&self, i: u32, p: Vec2) -> Option<f32> {
        let [a, b, c] = self.driven_at(i);
        let flat = |v: Vec3| Vec2::new(v.x, v.z) - p;
        let (pa, pb, pc) = (flat(a), flat(b), flat(c));
        // (Every floor winds the same way seen from above.)
        let (u, v, w) = (pb.perp_dot(pc), pc.perp_dot(pa), pa.perp_dot(pb));
        let area = u + v + w;
        let e = 1e-4 * area;
        (u <= -e && v <= -e && w <= -e).then(|| (a.y * u + b.y * v + c.y * w) / area)
    }

    /// The highest floor over `p`.
    pub fn top(&self, p: Vec2) -> Option<f32> {
        self.over(p).reduce(f32::max)
    }

    /// The floor over `p` nearest height `y`.
    pub fn near(&self, p: Vec2, y: f32) -> Option<f32> {
        self.over(p)
            .min_by(|a, b| (a - y).abs().total_cmp(&(b - y).abs()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A plain with a cliff 10 m up along a line at 30 degrees, and a tunnel
    /// into it square to the cliff.
    struct Dug;

    const DIR: f32 = std::f32::consts::PI / 6.0;

    impl Dug {
        /// (Along the cliff, into it).
        fn local(p: Vec2) -> Vec2 {
            Vec2::from_angle(-DIR).rotate(p)
        }
        fn at(u: f32, v: f32) -> Vec2 {
            Vec2::from_angle(DIR).rotate(Vec2::new(u, v))
        }
    }

    impl Rock for Dug {
        fn column(&self, p: Vec2, ys: &[f32], out: &mut [f32]) {
            let q = Dug::local(p);
            for (y, d) in ys.iter().zip(out) {
                let rock = (-q.y).max(y - 10.0).min(*y);
                let hall = (q.x.abs() - 4.0).max((y - 2.5).abs() - 2.5).max(q.y - 15.0);
                *d = rock.max(-hall);
            }
        }
    }

    fn dug() -> Surface {
        Surface::of(
            &Dug,
            Vec3::new(-30.0, -3.0, -30.0),
            Vec3::new(30.0, 14.0, 30.0),
            0.5,
        )
    }

    #[test]
    fn a_dig_has_a_floor_under_its_roof() {
        let s = dug();
        let floors = |u: f32, v: f32| s.floors(Dug::at(u, v));
        let is = |f: Vec<f32>, want: &[f32]| {
            assert_eq!(f.len(), want.len(), "{f:?} want {want:?}");
            for (a, b) in f.iter().zip(want) {
                assert!((a - b).abs() < 0.3, "{f:?} want {want:?}");
            }
        };
        is(floors(0.0, -10.0), &[0.0]);
        is(floors(12.0, 10.0), &[10.0]);
        is(floors(0.0, 8.0), &[10.0, 0.0]);
        is(floors(0.0, 20.0), &[10.0]);
        assert!(s.near(Dug::at(0.0, 8.0), 1.0).unwrap() < 0.3);
        assert!(s.top(Dug::at(0.0, 8.0)).unwrap() > 9.7);
    }

    /// Its faces look out of the rock, and no two meet wrongly: inside the
    /// box it's meshed in, every edge is between two of them.
    #[test]
    fn the_surface_is_closed_and_faces_out() {
        let s = dug();
        let mut edges = std::collections::HashMap::new();
        for t in &s.tris {
            let v = t.map(|k| s.verts[k as usize]);
            let n = (v[1] - v[0]).cross(v[2] - v[0]).normalize_or_zero();
            let mid = (v[0] + v[1] + v[2]) / 3.0;
            let mut d = [0.0; 2];
            for (k, side) in [0.6, -0.6].into_iter().enumerate() {
                let p = mid + n * side;
                Dug.column(Vec2::new(p.x, p.z), &[p.y], &mut d[k..k + 1]);
            }
            assert!(n == Vec3::ZERO || d[0] > d[1], "{mid} faces {n}: {d:?}");
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                *edges.entry((a.min(b), a.max(b))).or_insert(0) += 1;
            }
        }
        for ((a, b), n) in edges {
            let v = s.verts[a as usize].lerp(s.verts[b as usize], 0.5);
            let inside = v.x.abs().max(v.z.abs()) < 28.0;
            assert!(!inside || n % 2 == 0, "an open edge at {v}");
        }
    }
}
