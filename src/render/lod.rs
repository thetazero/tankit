//! Level of detail: coarser copies of a mesh, built once at load, and the
//! rule for which copy to draw.
//!
//! Copies come from meshoptimizer's edge-collapse simplifier, which merges
//! vertices while keeping track of how far the surface has moved. Flat
//! ground and flat walls collapse to a few big triangles at almost no
//! error; edges, creases and color borders hold. Each copy records that
//! error, and the renderer draws the coarsest copy whose error covers less
//! than [`MAX_ERROR_PX`] of a pixel at that distance, so the switch from
//! one copy to the next can't be seen.
//!
//! Copies index the same vertices as the exact mesh: only the triangles
//! change, so a copy costs nothing but its indices.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::ops::Range;

use glam::Vec3;
use meshopt::SimplifyOptions;

use super::mesh::Vertex;

/// The most a drawn copy's error may cover on screen, in pixels.
pub const MAX_ERROR_PX: f32 = 0.5;

/// The most a shadow-casting copy's error may cover in the shadow map, in
/// texels. The shadow filter blurs over several texels, so this hides too.
pub const MAX_ERROR_TEXELS: f32 = 0.5;

/// One copy of a mesh: its index range in the shared index buffer, and how
/// far (mesh units) its surface may sit from the exact one.
#[derive(Clone, Debug, PartialEq)]
pub struct Level {
    pub indices: Range<u32>,
    pub error: f32,
}

/// The coarsest of `levels` (finest first, the first exact) whose error is
/// at most `max_error` (mesh units).
pub fn pick(levels: &[Level], max_error: f32) -> usize {
    levels
        .iter()
        .rposition(|l| l.error <= max_error)
        .unwrap_or(0)
}

/// A fast hasher for the small keys here (the Fx hash rustc uses):
/// building a map's copies hashes millions of them at load.
#[derive(Default)]
pub struct Fx(u64);

impl Hasher for Fx {
    fn write(&mut self, bytes: &[u8]) {
        for c in bytes.chunks(8) {
            let mut b = [0u8; 8];
            b[..c.len()].copy_from_slice(c);
            self.write_u64(u64::from_le_bytes(b));
        }
    }
    fn write_u32(&mut self, i: u32) {
        self.write_u64(i as u64);
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

pub type FxMap<K, V> = HashMap<K, V, BuildHasherDefault<Fx>>;

/// `tris` (indexing `verts`) over the vertices they use, with exact
/// duplicates merged: mesh builders give every face its own corners, and
/// the simplifier can only collapse what's joined. Returns the vertices,
/// the triangles over them, and each new vertex's index in `verts`.
pub fn weld(verts: &[Vertex], tris: &[u32]) -> (Vec<Vertex>, Vec<u32>, Vec<u32>) {
    let mut index: FxMap<[u32; 10], u32> = FxMap::default();
    let mut out = Vec::new();
    let mut from = Vec::new();
    let tris = tris
        .iter()
        .map(|&i| {
            let v = &verts[i as usize];
            let mut key = [0u32; 10];
            for (k, x) in v.pos.iter().chain(&v.normal).chain(&v.color).enumerate() {
                key[k] = x.to_bits();
            }
            *index.entry(key).or_insert_with(|| {
                out.push(*v);
                from.push(i);
                out.len() as u32 - 1
            })
        })
        .collect();
    (out, tris, from)
}

/// `tris` (indexing `verts`, welded) simplified as far as `max_error`
/// (mesh units) allows, never moving a vertex `locked` marks. Returns the
/// triangles, over the same vertices, and the error they have.
///
/// Collapses never cross a crease or a change of color: where corners of
/// different faces meet, the simplifier only slides them along the seam.
/// (Its permissive mode, which crosses them, lets a flat face end up using
/// a bevel's corner and so its sideways normal, and the whole face shades
/// wrong.)
pub fn simplify(
    verts: &[Vertex],
    tris: &[u32],
    max_error: f32,
    locked: &[bool],
) -> (Vec<u32>, f32) {
    let positions: Vec<[f32; 3]> = verts.iter().map(|v| v.pos).collect();
    let mut error = 0.0;
    let out = meshopt::simplify_with_locks_decoder(
        tris,
        &positions,
        locked,
        0,
        max_error,
        SimplifyOptions::ErrorAbsolute,
        Some(&mut error),
    );
    (out, error)
}

/// Where a cluster sits: a grid cube, or, for a vertex that must not move,
/// its exact position.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Cell {
    Grid(i32, i32, i32),
    Pinned([u32; 3]),
}

/// `tris` (indexing `verts`) by vertex clustering: space cut into cubes
/// `cell` wide, every vertex in a cube moved to their average, and any
/// triangle left with two corners in one cube dropped. Unlike
/// [`simplify`], this also merges small separate pieces (a tread link's
/// bolts and bars) that edge collapses can't join. Vertices `pinned` marks
/// stay where they are.
///
/// Corners in one cube share a position, but only those with similar
/// normals and the same color and material share a vertex (their normals
/// and colors averaged), so flat faces stay flat and colors don't bleed.
/// New vertices are appended to `out_verts`, and the returned indices point
/// into it. Also returns the farthest any vertex moved: the copy's error.
pub fn cluster(
    verts: &[Vertex],
    tris: &[u32],
    cell: f32,
    pinned: &[bool],
    out_verts: &mut Vec<Vertex>,
) -> (Vec<u32>, f32) {
    let cell_of = |i: u32| {
        let p = Vec3::from(verts[i as usize].pos);
        if pinned[i as usize] {
            Cell::Pinned(p.to_array().map(f32::to_bits))
        } else {
            let q = (p / cell).floor();
            Cell::Grid(q.x as i32, q.y as i32, q.z as i32)
        }
    };
    // Each cube's vertices, averaged into one point.
    let mut cells: FxMap<Cell, (u32, Vec3, u32)> = FxMap::default();
    let mut vert_cell = vec![u32::MAX; verts.len()];
    for &i in tris {
        if vert_cell[i as usize] != u32::MAX {
            continue;
        }
        let next = cells.len() as u32;
        let e = cells.entry(cell_of(i)).or_insert((next, Vec3::ZERO, 0));
        e.1 += Vec3::from(verts[i as usize].pos);
        e.2 += 1;
        vert_cell[i as usize] = e.0;
    }
    let mut points = vec![Vec3::ZERO; cells.len()];
    for (_, (id, sum, n)) in cells {
        points[id as usize] = sum / n as f32;
    }
    // Every vertex counts, kept or not: a thin strip that collapses to
    // nothing moved as far as its width.
    let moved = tris
        .iter()
        .map(|&i| {
            points[vert_cell[i as usize] as usize].distance(Vec3::from(verts[i as usize].pos))
        })
        .fold(0.0f32, f32::max);

    // Each cube's corners split by look, then averaged.
    type Look = (u32, u8, [u32; 4]);
    let look_of = |i: u32| -> Look {
        let v = &verts[i as usize];
        (
            vert_cell[i as usize],
            normal_bucket(Vec3::from(v.normal)),
            look(v),
        )
    };
    let mut looks: FxMap<Look, u32> = FxMap::default();
    let mut sums: Vec<(Vec3, [f32; 3], u32)> = Vec::new();
    let base = out_verts.len() as u32;
    let mut out_of = vec![u32::MAX; verts.len()];
    let mut out_index = |i: u32, out_verts: &mut Vec<Vertex>| -> u32 {
        if out_of[i as usize] != u32::MAX {
            return out_of[i as usize];
        }
        let key = look_of(i);
        let v = &verts[i as usize];
        let at = points[key.0 as usize];
        let id = *looks.entry(key).or_insert_with(|| {
            out_verts.push(Vertex {
                pos: at.to_array(),
                normal: [0.0; 3],
                color: v.color,
            });
            sums.push((Vec3::ZERO, [0.0; 3], 0));
            out_verts.len() as u32 - 1
        });
        let s = &mut sums[(id - base) as usize];
        s.0 += Vec3::from(v.normal);
        for k in 0..3 {
            s.1[k] += v.color[k];
        }
        s.2 += 1;
        out_of[i as usize] = id;
        id
    };

    let mut out = Vec::with_capacity(tris.len() / 2);
    let mut seen: FxMap<[u32; 3], ()> = FxMap::default();
    for t in tris.chunks_exact(3) {
        let c = [t[0], t[1], t[2]].map(|i| vert_cell[i as usize]);
        if c[0] == c[1] || c[1] == c[2] || c[0] == c[2] {
            continue;
        }
        // The same three points in the same winding: draw it once.
        let r = (0..3).min_by_key(|&k| c[k]).expect("three corners");
        if seen
            .insert([c[r], c[(r + 1) % 3], c[(r + 2) % 3]], ())
            .is_some()
        {
            continue;
        }
        for &i in t {
            let id = out_index(i, out_verts);
            out.push(id);
        }
    }
    for (k, (n, rgb, count)) in sums.into_iter().enumerate() {
        let v = &mut out_verts[base as usize + k];
        let n = n.normalize_or_zero();
        if n != Vec3::ZERO {
            v.normal = n.to_array();
        }
        for c in 0..3 {
            v.color[c] = rgb[c] / count as f32;
        }
    }
    (out, moved)
}

/// Which of 54 cones round the sphere `n` points into (about 35° across):
/// its main axis, then a 3x3 grid over the other two components.
fn normal_bucket(n: Vec3) -> u8 {
    let a = n.abs();
    let (axis, u, w, m) = if a.x >= a.y && a.x >= a.z {
        (if n.x > 0.0 { 0 } else { 1 }, n.y, n.z, a.x)
    } else if a.y >= a.z {
        (if n.y > 0.0 { 2 } else { 3 }, n.x, n.z, a.y)
    } else {
        (if n.z > 0.0 { 4 } else { 5 }, n.x, n.y, a.z)
    };
    if m == 0.0 {
        return 0;
    }
    let bin = |x: f32| ((x / m + 1.0) * 1.5).floor().clamp(0.0, 2.0) as u8;
    axis * 9 + bin(u) * 3 + bin(w)
}

/// What a surface looks like, for telling which surfaces a copy must keep
/// apart: its color (to 1/64) and material.
fn look(v: &Vertex) -> [u32; 4] {
    let q = |x: f32| (x.clamp(0.0, 4.0) * 64.0).round() as u32;
    [
        q(v.color[0]),
        q(v.color[1]),
        q(v.color[2]),
        v.color[3].to_bits(),
    ]
}

/// The closest point to `p` on triangle `t` (Ericson, Real-Time Collision
/// Detection, 5.1.5).
fn closest_on_triangle(p: Vec3, [a, b, c]: [Vec3; 3]) -> Vec3 {
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return a + ab * (d1 / (d1 - d3));
    }
    let cp = p - c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return a + ac * (d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let denom = 1.0 / (va + vb + vc);
    a + ab * (vb * denom) + ac * (vc * denom)
}

/// How far a copy strays from the surface it was made from: for every
/// corner and middle of `from`'s triangles, the distance to the nearest of
/// `to`'s triangles with the same look, the largest of them. Anything
/// within `ok` counts as 0 (so most points stop at the first match), and
/// nothing counts as more than `cap`: the search goes no farther, so pass
/// the most error that would still be any use.
///
/// This is the error a copy is drawn by. A simplifier's own estimate can
/// miss a thin colored strip collapsing away, since the strip's corners
/// slide along its flat face; measured like this, the strip has moved as far
/// as it was wide.
pub fn deviation(
    from_verts: &[Vertex],
    from: &[u32],
    to_verts: &[Vertex],
    to: &[u32],
    ok: f32,
    cap: f32,
) -> f32 {
    let p = |vs: &[Vertex], i: u32| Vec3::from(vs[i as usize].pos);
    let (lo, hi) = to.iter().fold(
        (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
        |(lo, hi), &i| (lo.min(p(to_verts, i)), hi.max(p(to_verts, i))),
    );
    if to.is_empty() {
        return if from.is_empty() { 0.0 } else { cap };
    }
    // `to`'s triangles bucketed on a grid, by look.
    let g = (ok * 2.0).max((hi - lo).max_element() / 24.0).max(1e-4);
    let cell = |q: Vec3| ((q - lo) / g).floor().as_ivec3();
    let mut grid: FxMap<(glam::IVec3, [u32; 4]), Vec<u32>> = FxMap::default();
    for (k, t) in to.chunks_exact(3).enumerate() {
        let c = [t[0], t[1], t[2]].map(|i| p(to_verts, i));
        let (a, b) = (
            cell(c[0].min(c[1]).min(c[2])),
            cell(c[0].max(c[1]).max(c[2])),
        );
        let l = look(&to_verts[t[0] as usize]);
        for x in a.x..=b.x {
            for y in a.y..=b.y {
                for z in a.z..=b.z {
                    grid.entry((glam::IVec3::new(x, y, z), l))
                        .or_default()
                        .push(k as u32);
                }
            }
        }
    }
    let rings = (cap / g).ceil().min(4.0) as i32;
    let nearest = |q: Vec3, l: [u32; 4]| -> f32 {
        let home = cell(q);
        let mut best = f32::MAX;
        for r in 0..=rings {
            for x in -r..=r {
                for y in -r..=r {
                    for z in -r..=r {
                        if x.abs().max(y.abs()).max(z.abs()) != r {
                            continue;
                        }
                        let Some(list) = grid.get(&(home + glam::IVec3::new(x, y, z), l)) else {
                            continue;
                        };
                        for &k in list {
                            let t = &to[k as usize * 3..k as usize * 3 + 3];
                            let tri = [t[0], t[1], t[2]].map(|i| p(to_verts, i));
                            best = best.min(closest_on_triangle(q, tri).distance(q));
                        }
                    }
                }
            }
            // Anything in a farther ring is at least `r * g` away.
            if best <= ok || best <= r as f32 * g {
                break;
            }
        }
        best.min(cap)
    };
    let patches = patches(from_verts, from);
    // Whether nothing of a patch's look is left anywhere over its box.
    let mut gone: FxMap<u32, bool> = FxMap::default();
    let mut is_gone = |id: u32, lo: Vec3, hi: Vec3, l: [u32; 4]| -> bool {
        *gone.entry(id).or_insert_with(|| {
            let (lo, hi) = (lo - Vec3::splat(ok), hi + Vec3::splat(ok));
            let (a, b) = (cell(lo), cell(hi));
            for x in a.x..=b.x {
                for y in a.y..=b.y {
                    for z in a.z..=b.z {
                        let Some(list) = grid.get(&(glam::IVec3::new(x, y, z), l)) else {
                            continue;
                        };
                        for &k in list {
                            let t = &to[k as usize * 3..k as usize * 3 + 3];
                            let c = [t[0], t[1], t[2]].map(|i| p(to_verts, i));
                            let (tlo, thi) = (c[0].min(c[1]).min(c[2]), c[0].max(c[1]).max(c[2]));
                            if tlo.cmple(hi).all() && thi.cmpge(lo).all() {
                                return false;
                            }
                        }
                    }
                }
            }
            true
        })
    };
    let mut worst = 0.0f32;
    // Each corner is tried once, from the first triangle that has it.
    let mut tried = vec![false; from_verts.len()];
    for (k, t) in from.chunks_exact(3).enumerate() {
        let c = [t[0], t[1], t[2]].map(|i| p(from_verts, i));
        let l = look(&from_verts[t[0] as usize]);
        let mut points = vec![(c[0] + c[1] + c[2]) / 3.0];
        for (&i, &q) in t.iter().zip(&c) {
            if !std::mem::replace(&mut tried[i as usize], true) {
                points.push(q);
            }
        }
        for q in points {
            let mut d = nearest(q, l);
            // A patch that's gone entirely counts only its width: once
            // that's under a pixel, losing it can't be seen. (One that's
            // shrunk, say a strip pulled into a dot, counts in full.)
            let (id, width, lo, hi) = patches[k];
            if d > ok && d > width && is_gone(id, lo, hi, l) {
                d = width;
            }
            if d > ok {
                worst = worst.max(d);
            }
        }
    }
    worst
}

/// For each triangle, the patch of one look it belongs to (the triangles of
/// that look joined to it through shared corners): an id, its width, and its
/// box. Width is area over the box's diagonal: about 2 cm for a strip 2 cm
/// by 90 cm, and how wide a thing is decides whether it shows.
fn patches(verts: &[Vertex], tris: &[u32]) -> Vec<(u32, f32, Vec3, Vec3)> {
    // Union-find over corners: same position and look.
    let mut corner: FxMap<([u32; 3], [u32; 4]), u32> = FxMap::default();
    let mut parent: Vec<u32> = Vec::new();
    fn root(parent: &mut [u32], mut i: u32) -> u32 {
        while parent[i as usize] != i {
            parent[i as usize] = parent[parent[i as usize] as usize];
            i = parent[i as usize];
        }
        i
    }
    let ids: Vec<u32> = tris
        .iter()
        .map(|&i| {
            let v = &verts[i as usize];
            *corner
                .entry((v.pos.map(f32::to_bits), look(v)))
                .or_insert_with(|| {
                    parent.push(parent.len() as u32);
                    parent.len() as u32 - 1
                })
        })
        .collect();
    for t in ids.chunks_exact(3) {
        let a = root(&mut parent, t[0]);
        for &b in &t[1..] {
            let b = root(&mut parent, b);
            parent[b as usize] = a;
        }
    }
    let mut patches: FxMap<u32, (Vec3, Vec3, f32)> = FxMap::default();
    for (t, c) in tris.chunks_exact(3).zip(ids.chunks_exact(3)) {
        let r = root(&mut parent, c[0]);
        let q = [t[0], t[1], t[2]].map(|i| Vec3::from(verts[i as usize].pos));
        let area = (q[1] - q[0]).cross(q[2] - q[0]).length() * 0.5;
        let lo = q[0].min(q[1]).min(q[2]);
        let hi = q[0].max(q[1]).max(q[2]);
        let p = patches.entry(r).or_insert((lo, hi, 0.0));
        *p = (p.0.min(lo), p.1.max(hi), p.2 + area);
    }
    ids.chunks_exact(3)
        .map(|t| {
            let r = root(&mut parent, t[0]);
            let (lo, hi, area) = patches[&r];
            let diagonal = lo.distance(hi);
            (r, (area / diagonal.max(1e-6)).min(diagonal), lo, hi)
        })
        .collect()
}

/// The exact mesh and its coarser copies, finest first, for a mesh of
/// `radius`. For each error from `radius / 512` up to `radius / 4` it tries
/// both [`simplify`] and [`cluster`] and keeps whichever leaves fewer
/// triangles, if that's at most 3/4 of the last copy kept and its error,
/// measured against the exact mesh with [`deviation`], is within 1.5x the
/// target. The
/// vertices go to `out_verts` and every level's triangles to `out_indices`.
pub fn build_levels(
    verts: &[Vertex],
    tris: &[u32],
    radius: f32,
    out_verts: &mut Vec<Vertex>,
    out_indices: &mut Vec<u32>,
) -> Vec<Level> {
    let (welded, tris, _) = weld(verts, tris);
    let base = out_verts.len() as u32;
    out_verts.extend_from_slice(&welded);
    let start = out_indices.len() as u32;
    out_indices.extend(tris.iter().map(|i| i + base));
    let mut levels = vec![Level {
        indices: start..out_indices.len() as u32,
        error: 0.0,
    }];
    let locked = vec![false; welded.len()];
    let mut target = radius / 512.0;
    let mut kept = tris.len();
    while target <= radius / 4.0 && kept > 12 {
        let measure = |v: &[Vertex], t: &[u32], claimed: f32| {
            claimed.max(deviation(&welded, &tris, v, t, target, target * 2.0))
        };
        let (edge, edge_claim) = simplify(&welded, &tris, target, &locked);
        // Cubes this wide move no vertex farther than `target`.
        let mut new = Vec::new();
        let (clus, clus_claim) = cluster(&welded, &tris, target / 3f32.sqrt(), &locked, &mut new);
        // Fewest triangles first; measured only if it's a worthwhile cut.
        let mut tries = [(edge, edge_claim, false), (clus, clus_claim, true)];
        tries.sort_by_key(|(t, _, _)| t.len());
        let choice = tries.into_iter().find_map(|(t, claim, own)| {
            if t.is_empty() || t.len() * 4 > kept * 3 {
                return None;
            }
            let v: &[Vertex] = if own { &new } else { &welded };
            let error = measure(v, &t, claim);
            (error <= target * 1.5).then_some((t, error, own))
        });
        if let Some((t, error, own)) = choice {
            let offset = if own {
                let o = out_verts.len() as u32;
                out_verts.extend_from_slice(&new);
                o
            } else {
                base
            };
            let start = out_indices.len() as u32;
            out_indices.extend(t.iter().map(|i| i + offset));
            levels.push(Level {
                indices: start..out_indices.len() as u32,
                error,
            });
            kept = t.len();
        }
        target *= 2.0;
    }
    levels
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::mesh::MeshData;
    use glam::Vec4;

    fn tri_count(l: &Level) -> usize {
        (l.indices.end - l.indices.start) as usize / 3
    }

    fn grid(n: i32) -> MeshData {
        let mut m = MeshData::default();
        let c = Vec4::new(0.5, 0.4, 0.3, 1.0);
        for z in 0..n {
            for x in 0..n {
                let p = |dx: i32, dz: i32| Vec3::new((x + dx) as f32, 0.0, (z + dz) as f32);
                m.quad([p(0, 0), p(0, 1), p(1, 1), p(1, 0)], Vec3::Y, c);
            }
        }
        m
    }

    #[test]
    fn coarser_levels_have_fewer_triangles_and_bigger_errors() {
        let sphere = MeshData::sphere(64, 40);
        let (mut v, mut i) = (Vec::new(), Vec::new());
        let levels = build_levels(&sphere.verts, &sphere.indices, 0.5, &mut v, &mut i);
        assert!(levels.len() >= 3, "{} levels", levels.len());
        assert_eq!(tri_count(&levels[0]), sphere.indices.len() / 3);
        for w in levels.windows(2) {
            assert!(tri_count(&w[1]) * 4 <= tri_count(&w[0]) * 3);
            assert!(w[1].error >= w[0].error);
        }
        // Each level's triangles stay near the sphere: their middles sit no
        // deeper inside it than the level's error allows (with slack for the
        // estimate).
        for l in &levels {
            for t in i[l.indices.start as usize..l.indices.end as usize].chunks_exact(3) {
                let mid = t
                    .iter()
                    .map(|&k| Vec3::from(v[k as usize].pos))
                    .sum::<Vec3>()
                    / 3.0;
                let depth = 0.5 - mid.length();
                assert!(
                    depth <= l.error * 3.0 + 2e-3,
                    "depth {depth} at error {}",
                    l.error
                );
            }
        }
    }

    #[test]
    fn a_flat_grid_collapses_to_almost_nothing() {
        let m = grid(20);
        let (v, t, _) = weld(&m.verts, &m.indices);
        assert_eq!(v.len(), 21 * 21);
        let (s, error) = simplify(&v, &t, 1e-3, &vec![false; v.len()]);
        assert!(s.len() / 3 <= 80, "{} triangles left", s.len() / 3);
        assert!(error < 1e-3);
    }

    #[test]
    fn locked_vertices_stay_in_use() {
        // A grid whose outer ring is locked keeps every outer vertex, so it
        // still meets whatever it joined on to.
        let m = grid(10);
        let (v, t, _) = weld(&m.verts, &m.indices);
        let edge = |p: [f32; 3]| p[0] == 0.0 || p[0] == 10.0 || p[2] == 0.0 || p[2] == 10.0;
        let locked: Vec<bool> = v.iter().map(|v| edge(v.pos)).collect();
        let (s, _) = simplify(&v, &t, 1e-3, &locked);
        assert!(s.len() < t.len());
        for (k, _) in v.iter().enumerate().filter(|(_, v)| edge(v.pos)) {
            assert!(s.contains(&(k as u32)), "edge vertex {k} dropped");
        }
    }

    #[test]
    fn pick_takes_the_coarsest_level_within_the_error() {
        let l = |e: f32| Level {
            indices: 0..3,
            error: e,
        };
        let levels = [l(0.0), l(0.01), l(0.02), l(0.04)];
        assert_eq!(pick(&levels, 0.0), 0);
        assert_eq!(pick(&levels, 0.015), 1);
        assert_eq!(pick(&levels, 0.03), 2);
        assert_eq!(pick(&levels, 1.0), 3);
    }
}
