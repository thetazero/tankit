//! Ground as a solid: rock wherever a signed distance is negative, meshed
//! into one surface that's drawn, driven on and asked for its floors. Unlike
//! a `Terrain` it can stand sheer, overhang and be dug into, at any angle.

use glam::{IVec3, Vec2, Vec3};

/// What's rock: negative in it, positive in the air, about the distance (m)
/// to the surface near it.
pub trait Rock {
    /// What's worked out once for the whole of a column.
    type Column;
    fn column(&self, p: Vec2) -> Self::Column;
    /// From below the lowest of its surface to above the highest: it's all
    /// rock under the one and all air over the other.
    fn span(&self, c: &Self::Column) -> (f32, f32);
    /// The distance `y` up it.
    fn distance(&self, c: &Self::Column, y: f32) -> f32;
}

/// Samples of a rock on a grid, each column's only where its own surface or
/// a neighbor's is.
struct Samples {
    nx: i32,
    nz: i32,
    /// Each column's first sample and its height (in cells), and how many.
    columns: Vec<(usize, i32, i32)>,
    d: Vec<f32>,
}

impl Samples {
    fn of<R: Rock>(rock: &R, lo: Vec2, (nx, nz): (i32, i32), cell: f32) -> Samples {
        let found: Vec<R::Column> = (0..nx * nz)
            .map(|k| rock.column(lo + Vec2::new((k % nx) as f32, (k / nx) as f32) * cell))
            .collect();
        let spans: Vec<(i32, i32)> = found
            .iter()
            .map(|c| {
                let (a, b) = rock.span(c);
                ((a / cell).floor() as i32 - 1, (b / cell).ceil() as i32 + 1)
            })
            .collect();
        let mut d = Vec::new();
        let mut columns = Vec::with_capacity(found.len());
        for (k, c) in found.iter().enumerate() {
            let (x, z) = (k as i32 % nx, k as i32 / nx);
            let (mut a, mut b) = spans[k];
            for j in (z - 1).max(0)..=(z + 1).min(nz - 1) {
                for i in (x - 1).max(0)..=(x + 1).min(nx - 1) {
                    let s = spans[(j * nx + i) as usize];
                    (a, b) = (a.min(s.0), b.max(s.1));
                }
            }
            columns.push((d.len(), a, b - a + 1));
            d.extend((a..=b).map(|y| rock.distance(c, y as f32 * cell)));
        }
        Samples { nx, nz, columns, d }
    }

    fn at(&self, c: IVec3) -> f32 {
        let (from, low, n) = self.columns[(c.z * self.nx + c.x) as usize];
        match c.y - low {
            y if y < 0 => -1.0,
            y if y >= n => 1.0,
            y => self.d[from + y as usize],
        }
    }

    /// The heights of the cubes over a square that may have surface in them.
    fn cubes(&self, x: i32, z: i32) -> std::ops::Range<i32> {
        let (mut a, mut b) = (i32::MAX, i32::MIN);
        for (x, z) in [(x, z), (x + 1, z), (x, z + 1), (x + 1, z + 1)] {
            let (_, low, n) = self.columns[(z * self.nx + x) as usize];
            (a, b) = (a.min(low), b.max(low + n - 1));
        }
        a..b
    }
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
    /// Meshes `rock` from `lo` to `hi` (all the way up and down) in cubes
    /// `cell` across.
    pub fn of<R: Rock>(rock: &R, lo: Vec2, hi: Vec2, cell: f32) -> Surface {
        let n = ((hi - lo) / cell).ceil().as_ivec2() + 1;
        let d = Samples::of(rock, lo, (n.x, n.y), cell);
        let mut s = Surface::default();
        // A vertex in every cube the surface passes through, in the middle
        // of where it crosses the cube's edges.
        let corner = |k: i32| IVec3::new(k & 1, (k >> 1) & 1, (k >> 2) & 1);
        // (Over each square: its first cube's height, and their vertices.)
        let mut ids: Vec<u32> = Vec::new();
        let mut over: Vec<(usize, std::ops::Range<i32>)> = Vec::new();
        for z in 0..d.nz - 1 {
            for x in 0..d.nx - 1 {
                let cubes = d.cubes(x, z);
                over.push((ids.len(), cubes.clone()));
                for y in cubes {
                    let c = IVec3::new(x, y, z);
                    let at: [f32; 8] = std::array::from_fn(|k| d.at(c + corner(k as i32)));
                    if at[1..].iter().all(|v| (*v < 0.0) == (at[0] < 0.0)) {
                        ids.push(u32::MAX);
                        continue;
                    }
                    let (mut sum, mut hits) = (Vec3::ZERO, 0.0);
                    for k in 0..8 {
                        for bit in [1, 2, 4] {
                            if k & bit != 0 {
                                continue;
                            }
                            let (a, b) = (corner(k), corner(k | bit));
                            let (da, db) = (at[k as usize], at[(k | bit) as usize]);
                            if (da < 0.0) != (db < 0.0) {
                                sum += a.as_vec3().lerp(b.as_vec3(), da / (da - db));
                                hits += 1.0;
                            }
                        }
                    }
                    ids.push(s.verts.len() as u32);
                    let mut q = sum / hits;
                    // (Its edges are straight: they meet other ground.)
                    if x == 0 || x == d.nx - 2 {
                        q.x = 0.5;
                    }
                    if z == 0 || z == d.nz - 2 {
                        q.z = 0.5;
                    }
                    let p = (c.as_vec3() + q) * cell;
                    s.verts.push(Vec3::new(lo.x + p.x, p.y, lo.y + p.z));
                }
            }
        }
        let vertex = |c: IVec3| {
            let (from, cubes) = &over[(c.z * (d.nx - 1) + c.x) as usize];
            if cubes.contains(&c.y) {
                ids[from + (c.y - cubes.start) as usize]
            } else {
                u32::MAX
            }
        };
        // A face across every grid edge the surface crosses, between the
        // four cubes round it.
        for z in 0..d.nz - 1 {
            for x in 0..d.nx - 1 {
                for y in d.cubes(x, z) {
                    let c = IVec3::new(x, y, z);
                    if vertex(c) == u32::MAX {
                        continue;
                    }
                    for (axis, u, v) in [
                        (IVec3::X, IVec3::Y, IVec3::Z),
                        (IVec3::Y, IVec3::Z, IVec3::X),
                        (IVec3::Z, IVec3::X, IVec3::Y),
                    ] {
                        let low = c - u - v;
                        if low.x < 0 || low.z < 0 {
                            continue;
                        }
                        let (da, db) = (d.at(c), d.at(c + axis));
                        if (da < 0.0) == (db < 0.0) {
                            continue;
                        }
                        let q = [c - u - v, c - v, c, c - u].map(vertex);
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
    fn finish(&mut self, lo: Vec2, hi: Vec2) {
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
        self.lo = lo;
        self.nx = ((hi.x - lo.x) / SQUARE).ceil() as usize + 1;
        self.nz = ((hi.y - lo.y) / SQUARE).ceil() as usize + 1;
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
    pub fn over(&self, p: Vec2) -> impl Iterator<Item = f32> + '_ {
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
        type Column = Vec2;

        fn column(&self, p: Vec2) -> Vec2 {
            Dug::local(p)
        }

        fn span(&self, _: &Vec2) -> (f32, f32) {
            (0.0, 10.0)
        }

        fn distance(&self, q: &Vec2, y: f32) -> f32 {
            let rock = (-q.y).max(y - 10.0).min(y);
            let hall = (q.x.abs() - 4.0).max((y - 2.5).abs() - 2.5).max(q.y - 15.0);
            rock.max(-hall)
        }
    }

    fn dug() -> Surface {
        Surface::of(&Dug, Vec2::splat(-30.0), Vec2::splat(30.0), 0.5)
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
            let d = [0.6, -0.6].map(|side| {
                let p = mid + n * side;
                Dug.distance(&Dug.column(Vec2::new(p.x, p.z)), p.y)
            });
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
