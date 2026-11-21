//! A mine's inside sculpted as smooth rock, the Seam's: walls that wander
//! in and out as they run and as they rise, arching over into a roof that
//! sags a little between them. Drawn only: the map keeps its boxes for
//! colliding, hidden behind the rock.
//!
//! The open space is given as a signed distance (negative inside), so the
//! walls follow its edge wherever it goes, corners rounded, without the map
//! having to trace them. Each wall point is worked out from where the edge
//! crosses the sampling grid alone, so neighbouring pieces of wall meet
//! exactly, with no seams.

use glam::{Vec2, Vec3};

use super::valley::noise2;
use crate::render::material::Material;
use crate::render::mesh::{MeshData, Vertex};

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Rows up a wall, from under the floor to where it starts to arch, and
/// round the arch into the roof.
const WALL_ROWS: usize = 12;
const ARCH_ROWS: usize = 7;
/// The walls start this far under the floor (hidden by it).
const BURIED: f32 = 0.6;

/// What to line, and how.
pub(super) struct Cave<'a> {
    /// Signed distance to the open space's edge (m; negative inside it).
    pub sdf: &'a dyn Fn(Vec2) -> f32,
    /// The roof's underside over a point.
    pub roof: &'a dyn Fn(Vec2) -> f32,
    /// How far the walls arch over into the roof at a wall point (m).
    pub arch: &'a dyn Fn(Vec2) -> f32,
    /// Line the wall at this edge point, the rock lying along `n`?
    pub wall_here: &'a dyn Fn(Vec2, Vec2) -> bool,
    /// Roof over this point?
    pub roof_here: &'a dyn Fn(Vec2) -> bool,
    /// The rock's colour and material at a point on it, facing `n`.
    pub shade: &'a dyn Fn(Vec3, Vec3) -> (Vec3, Material),
    /// Veins showing in the wall at an edge point, each the same one from
    /// point to point (0 thick where it pinches out): its middle's height,
    /// half its thickness, and half the thickness of the plasma thread down
    /// its middle.
    pub veins: &'a dyn Fn(Vec2) -> [Vein; 2],
}

/// A vein of crystal across a wall, at one point along it.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Vein {
    pub y: f32,
    pub half: f32,
    pub thread: f32,
}

/// Raw crystal in the walls, and the plasma threading it.
const CRYSTAL: Vec3 = Vec3::new(0.05, 0.1, 0.2);
const PLASMA: Vec3 = crate::tank::ithessar::PLASMA;

impl Cave<'_> {
    /// Out of the open space, into the rock, at `p`.
    fn out(&self, p: Vec2) -> Vec2 {
        let e = 0.05;
        let s = self.sdf;
        Vec2::new(
            s(p + Vec2::X * e) - s(p - Vec2::X * e),
            s(p + Vec2::Y * e) - s(p - Vec2::Y * e),
        )
        .normalize_or(Vec2::X)
    }

    /// How far the wall stands back into the rock (m; negative: out into
    /// the open) at edge point `c`, height `y`: a little in and out as it
    /// runs and as it rises, mostly back, so it never stands far in front
    /// of the collider at the edge.
    fn sway(c: Vec2, y: f32) -> f32 {
        let u = c.x + c.y;
        let big = noise2(u / 4.5, y / 3.2, 11);
        let small = noise2(u / 1.6, y / 1.1, 12);
        -0.15 + 0.55 * big + 0.2 * small
    }

    /// A point on the wall above edge point `c`: `v` from 0 (under the
    /// floor) to `WALL_ROWS` (where it starts to arch) and on round the arch
    /// to `WALL_ROWS + ARCH_ROWS` (into the roof, past it).
    fn wall_point(&self, c: Vec2, v: f32) -> Vec3 {
        let n = self.out(c);
        let r = (self.arch)(c);
        let top = (self.roof)(c) - r;
        let w = WALL_ROWS as f32;
        if v <= w {
            let y = -BURIED + (top + BURIED) * v / w;
            let q = c + n * Self::sway(c, y);
            Vec3::new(q.x, y, q.y)
        } else {
            // A quarter round and a little more, so it passes up through
            // the roof and the roof hides the join.
            let a = (v - w) / ARCH_ROWS as f32 * 1.75;
            let back = Self::sway(c, top) * (1.0 - smoothstep(0.0, 1.5, a));
            let (s, k) = a.min(std::f32::consts::FRAC_PI_2).sin_cos();
            let (inward, up) = if a <= std::f32::consts::FRAC_PI_2 {
                (r * (1.0 - k), r * s)
            } else {
                // Past the top of the round: on in under the roof, rising.
                let more = a - std::f32::consts::FRAC_PI_2;
                (r + more * r * 0.5, r + more * 0.25)
            };
            let q = c + n * (back - inward);
            Vec3::new(q.x, top + up, q.y)
        }
    }

    /// The wall's normal there, facing into the open.
    fn wall_normal(&self, c: Vec2, v: f32) -> Vec3 {
        let n = self.out(c);
        let t = n.perp();
        let e = 0.1;
        let ds = self.wall_point(c + t * e, v) - self.wall_point(c - t * e, v);
        let dv = self.wall_point(c, v + 0.25) - self.wall_point(c, v - 0.25);
        let f = ds.cross(dv).normalize_or_zero();
        if f.x * n.x + f.z * n.y > 0.0 { -f } else { f }
    }

    /// The roof's underside at a point, sagging a little.
    fn roof_point(&self, p: Vec2) -> Vec3 {
        let sag = 0.35 * noise2(p.x / 3.1, p.y / 3.1, 13) + 0.1 * noise2(p.x, p.y, 14);
        Vec3::new(p.x, (self.roof)(p) - sag, p.y)
    }
}

/// Line the cave between `lo` and `hi` (on the ground plane) into `m`.
pub(super) fn line(m: &mut MeshData, cave: &Cave, lo: Vec2, hi: Vec2) {
    walls(m, cave, lo, hi);
    roof(m, cave, lo, hi);
}

fn push(m: &mut MeshData, cave: &Cave, pos: Vec3, nrm: Vec3) -> u32 {
    let (color, mat) = (cave.shade)(pos, nrm);
    m.verts.push(Vertex {
        pos: pos.to_array(),
        normal: nrm.to_array(),
        color: color.extend(mat.id()).to_array(),
    });
    (m.verts.len() - 1) as u32
}

/// The walls: marching squares over the distance field finds where the
/// edge crosses each cell, and a strip of wall rises over each piece.
fn walls(m: &mut MeshData, cave: &Cave, lo: Vec2, hi: Vec2) {
    const CELL: f32 = 0.5;
    let nx = ((hi.x - lo.x) / CELL).ceil() as usize;
    let nz = ((hi.y - lo.y) / CELL).ceil() as usize;
    let at = |i: usize, j: usize| lo + Vec2::new(i as f32, j as f32) * CELL;
    let mut d = vec![0.0f32; (nx + 1) * (nz + 1)];
    for j in 0..=nz {
        for i in 0..=nx {
            // Never exactly on the edge: every crossing is between two
            // corners of opposite sign.
            let s = (cave.sdf)(at(i, j));
            d[j * (nx + 1) + i] = if s == 0.0 { 1e-4 } else { s };
        }
    }
    let val = |i: usize, j: usize| d[j * (nx + 1) + i];
    // Where the edge crosses from corner a to corner b (always the lower
    // one first, so both cells sharing the side get the same point).
    let cross = |a: (usize, usize), b: (usize, usize)| -> Option<Vec2> {
        let (sa, sb) = (val(a.0, a.1), val(b.0, b.1));
        if (sa < 0.0) == (sb < 0.0) {
            return None;
        }
        let t = sa / (sa - sb);
        Some(at(a.0, a.1).lerp(at(b.0, b.1), t))
    };
    for j in 0..nz {
        for i in 0..nx {
            let sides = [
                cross((i, j), (i + 1, j)),
                cross((i + 1, j), (i + 1, j + 1)),
                cross((i, j + 1), (i + 1, j + 1)),
                cross((i, j), (i, j + 1)),
            ];
            let pts: Vec<Vec2> = sides.iter().flatten().copied().collect();
            let pieces: Vec<(Vec2, Vec2)> = match pts.len() {
                2 => vec![(pts[0], pts[1])],
                4 => {
                    // A saddle: pair the crossings by the middle's side.
                    let mid =
                        (val(i, j) + val(i + 1, j) + val(i, j + 1) + val(i + 1, j + 1)) * 0.25;
                    let low = val(i, j) < 0.0;
                    if (mid < 0.0) == low {
                        vec![(pts[0], pts[1]), (pts[2], pts[3])]
                    } else {
                        vec![(pts[0], pts[3]), (pts[1], pts[2])]
                    }
                }
                _ => Vec::new(),
            };
            for (a, b) in pieces {
                let mid = (a + b) * 0.5;
                if a.distance_squared(b) < 1e-8 || !(cave.wall_here)(mid, cave.out(mid)) {
                    continue;
                }
                strip(m, cave, a, b);
            }
        }
    }
}

/// One strip of wall over the edge from `a` to `b`.
fn strip(m: &mut MeshData, cave: &Cave, a: Vec2, b: Vec2) {
    let rows = WALL_ROWS + ARCH_ROWS;
    let mut ids = [Vec::with_capacity(rows + 1), Vec::with_capacity(rows + 1)];
    for (k, c) in [a, b].into_iter().enumerate() {
        for r in 0..=rows {
            let v = r as f32;
            let p = cave.wall_point(c, v);
            let n = cave.wall_normal(c, v);
            ids[k].push(push(m, cave, p, n));
        }
    }
    let into = -{
        let n = cave.out((a + b) * 0.5);
        Vec3::new(n.x, 0.0, n.y)
    };
    for r in 0..rows {
        let q = [ids[0][r], ids[1][r], ids[1][r + 1], ids[0][r + 1]];
        for t in [[q[0], q[1], q[2]], [q[0], q[2], q[3]]] {
            tri(m, t, into);
        }
    }
    // The veins, laid on the wall: the crystal, and the thread on it.
    let (va, vb) = ((cave.veins)(a), (cave.veins)(b));
    for k in 0..2 {
        let (x, y) = (va[k], vb[k]);
        for (half, lift, color, mat) in [
            ((x.half, y.half), 0.03, CRYSTAL, Material::Vhaal),
            ((x.thread, y.thread), 0.06, PLASMA, Material::Glow),
        ] {
            if half.0 <= 0.0 && half.1 <= 0.0 {
                continue;
            }
            let corner = |c: Vec2, mid: f32, h: f32, up: f32| {
                let top = (cave.roof)(c) - (cave.arch)(c);
                let y = (mid + h * up).clamp(-BURIED, top);
                let v = (y + BURIED) / (top + BURIED) * WALL_ROWS as f32;
                let n = cave.wall_normal(c, v);
                (cave.wall_point(c, v) + n * lift, n)
            };
            let base = m.verts.len() as u32;
            for (c, mid, h) in [(a, x.y, half.0), (b, y.y, half.1)] {
                for up in [-1.0, 1.0] {
                    let (p, n) = corner(c, mid, h.max(0.0), up);
                    m.verts.push(Vertex {
                        pos: p.to_array(),
                        normal: n.to_array(),
                        color: color.extend(mat.id()).to_array(),
                    });
                }
            }
            for t in [[base, base + 2, base + 3], [base, base + 3, base + 1]] {
                tri(m, t, into);
            }
        }
    }
}

/// Triangle `t` (already pushed), wound to face along `want`.
fn tri(m: &mut MeshData, mut t: [u32; 3], want: Vec3) {
    let p = |k: u32| Vec3::from_array(m.verts[k as usize].pos);
    let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
    let f = (b - a).cross(c - a);
    if f.length_squared() < 1e-12 {
        return;
    }
    if f.dot(want) < 0.0 {
        t.swap(1, 2);
    }
    m.indices.extend(t);
}

/// The roof: a sheet over every roofed point in reach of the open space,
/// out past the walls' tops, facing down.
fn roof(m: &mut MeshData, cave: &Cave, lo: Vec2, hi: Vec2) {
    const CELL: f32 = 1.0;
    let nx = ((hi.x - lo.x) / CELL).ceil() as usize;
    let nz = ((hi.y - lo.y) / CELL).ceil() as usize;
    let at = |i: usize, j: usize| lo + Vec2::new(i as f32, j as f32) * CELL;
    let mut ids: Vec<Option<u32>> = vec![None; (nx + 1) * (nz + 1)];
    for j in 0..nz {
        for i in 0..nx {
            let c = at(i, j) + Vec2::splat(CELL * 0.5);
            if (cave.sdf)(c) > 1.6 || !(cave.roof_here)(c) {
                continue;
            }
            let mut q = [0u32; 4];
            for (k, (di, dj)) in [(0, 0), (1, 0), (1, 1), (0, 1)].into_iter().enumerate() {
                let slot = (j + dj) * (nx + 1) + i + di;
                q[k] = *ids[slot].get_or_insert_with(|| {
                    let p = at(i + di, j + dj);
                    let e = 0.25;
                    let h = |d: Vec2| cave.roof_point(p + d).y;
                    let n = Vec3::new(
                        h(Vec2::X * e) - h(-Vec2::X * e),
                        -2.0 * e,
                        h(Vec2::Y * e) - h(-Vec2::Y * e),
                    )
                    .normalize();
                    push(m, cave, cave.roof_point(p), n)
                });
            }
            for t in [[q[0], q[1], q[2]], [q[0], q[2], q[3]]] {
                tri(m, t, -Vec3::Y);
            }
        }
    }
}

/// The distance to a rectangle's edge (negative inside), its corners
/// rounded over `r`.
pub(super) fn rect_sdf((a, b): (Vec2, Vec2), r: f32, p: Vec2) -> f32 {
    let q = (p - (a + b) * 0.5).abs() - ((b - a) * 0.5 - Vec2::splat(r));
    q.max(Vec2::ZERO).length() + q.x.max(q.y).min(0.0) - r
}

/// Smooth minimum, blending over `k` (a fillet where two spaces meet).
pub(super) fn smin(a: f32, b: f32, k: f32) -> f32 {
    let h = (0.5 + 0.5 * (b - a) / k).clamp(0.0, 1.0);
    b + (a - b) * h - k * h * (1.0 - h)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tunnel() -> (Vec2, Vec2) {
        (Vec2::new(0.0, 0.0), Vec2::new(10.0, 30.0))
    }

    fn lined() -> MeshData {
        let mut m = MeshData::default();
        let sdf = |p: Vec2| rect_sdf(tunnel(), 1.0, p);
        let cave = Cave {
            sdf: &sdf,
            roof: &|_| 7.0,
            arch: &|_| 2.0,
            wall_here: &|_, _| true,
            roof_here: &|p| sdf(p) < 2.0,
            shade: &|_, _| (Vec3::ONE, Material::Cliff),
            veins: &|_| [Vein::default(); 2],
        };
        line(&mut m, &cave, Vec2::splat(-4.0), Vec2::new(14.0, 34.0));
        m
    }

    /// The lining closes the tunnel in: its walls stand at the edge (a
    /// little back, never far out into the open), every face looks into
    /// the tunnel, and the roof is over it.
    #[test]
    fn the_lining_closes_the_tunnel_in() {
        let m = lined();
        assert!(!m.indices.is_empty());
        let (a, b) = tunnel();
        let mid = (a + b) * 0.5;
        for t in m.indices.chunks(3) {
            let p: Vec<Vec3> = t
                .iter()
                .map(|&k| Vec3::from_array(m.verts[k as usize].pos))
                .collect();
            let f = (p[1] - p[0]).cross(p[2] - p[0]).normalize();
            let c = (p[0] + p[1] + p[2]) / 3.0;
            let to_mid = Vec3::new(mid.x, 3.0, mid.y) - c;
            // Walls face in, the roof faces down.
            assert!(f.dot(to_mid) > -0.05 || f.y < -0.5, "{c} faces {f}");
        }
        for v in &m.verts {
            let p = Vec3::from_array(v.pos);
            if p.y > 0.0 && p.y < 4.0 {
                let s = rect_sdf(tunnel(), 1.0, Vec2::new(p.x, p.z));
                assert!(s > -0.3 && s < 1.0, "wall point {p} is {s} m off the edge");
            }
        }
    }

    /// Neighbouring strips meet exactly: every wall point is shared, at
    /// the same place, by the strips either side of it.
    #[test]
    fn the_walls_have_no_seams() {
        let m = lined();
        let mut seen: std::collections::HashMap<[u32; 3], usize> = Default::default();
        for v in &m.verts {
            let key = v.pos.map(|x| (x * 1000.0).round() as i32 as u32);
            *seen.entry(key).or_default() += 1;
        }
        let shared = seen.values().filter(|&&n| n >= 2).count();
        assert!(
            shared * 3 > seen.len(),
            "{shared} of {} points shared",
            seen.len()
        );
    }
}
