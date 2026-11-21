//! The static world split into ground cells, so the camera draws only the
//! cells in view, each at its own level of detail (see [`super::lod`]).
//!
//! Splitting is quick and happens at load. The cells' coarser copies take
//! about a second for a big map, so [`WorldLod`] builds them on a
//! background thread; until they arrive every cell draws exactly.

use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

use glam::Vec3;

use super::lod::{self, FxMap, Level};
use super::mesh::{MeshData, Vertex};

/// One ground cell of the static mesh: its triangles at each level of
/// detail (finest first, the first exact), and a sphere round them.
pub struct Chunk {
    pub levels: Vec<Level>,
    pub center: Vec3,
    pub radius: f32,
}

/// Ground cell size, meters.
const CHUNK: f32 = 24.0;

/// Error targets for the cells' coarser copies, meters. At
/// [`lod::MAX_ERROR_PX`] the last is used only out near the far plane.
const ERRORS: [f32; 7] = [0.005, 0.01, 0.02, 0.04, 0.08, 0.16, 0.32];

/// `mesh` with its triangles regrouped by the ground cell their middle is
/// in, and each cell, with only its exact level so far.
pub fn split(mesh: &MeshData) -> (MeshData, Vec<Chunk>) {
    let pos = |i: u32| Vec3::from(mesh.verts[i as usize].pos);
    let mut cells: BTreeMap<(i32, i32), Vec<u32>> = BTreeMap::new();
    for t in mesh.indices.chunks_exact(3) {
        let mid = (pos(t[0]) + pos(t[1]) + pos(t[2])) / 3.0;
        let key = (
            (mid.x / CHUNK).floor() as i32,
            (mid.z / CHUNK).floor() as i32,
        );
        cells.entry(key).or_default().extend_from_slice(t);
    }
    let mut indices = Vec::with_capacity(mesh.indices.len());
    let mut chunks = Vec::with_capacity(cells.len());
    for tris in cells.values() {
        let start = indices.len() as u32;
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for &i in tris {
            lo = lo.min(pos(i));
            hi = hi.max(pos(i));
        }
        indices.extend_from_slice(tris);
        let center = (lo + hi) * 0.5;
        let radius = tris
            .iter()
            .map(|&i| pos(i).distance(center))
            .fold(0.0, f32::max);
        chunks.push(Chunk {
            levels: vec![Level {
                indices: start..indices.len() as u32,
                error: 0.0,
            }],
            center,
            radius,
        });
    }
    let out = MeshData {
        verts: mesh.verts.clone(),
        indices,
    };
    (out, chunks)
}

/// The world mesh with every cell's coarser copies added: its vertices and
/// indices (the exact ones first, unchanged), and each cell's levels.
pub struct Levels {
    pub mesh: MeshData,
    pub levels: Vec<Vec<Level>>,
}

/// Coarser copies of each cell of `mesh` (as [`split`] made it, cells'
/// exact ranges in `exact`), or `None` if `cancel` is set first.
///
/// Each cell is welded and simplified on its own. Every vertex position a
/// cell shares with another stays locked where it is, so cells at different
/// levels still meet without cracks. Each level simplifies the last one kept
/// (much quicker than starting over), so errors add up along the way. Each
/// step's error is measured ([`lod::deviation`]), not taken on trust.
pub fn build_levels(mesh: &MeshData, exact: &[Range<u32>], cancel: &AtomicBool) -> Option<Levels> {
    let tris = |r: &Range<u32>| &mesh.indices[r.start as usize..r.end as usize];
    // Which cell each vertex is used by (u32::MAX: more than one), first by
    // index, then by position, since faces have their own corners.
    let mut by_index = vec![u32::MAX - 1; mesh.verts.len()];
    for (k, r) in exact.iter().enumerate() {
        for &i in tris(r) {
            let e = &mut by_index[i as usize];
            if *e == u32::MAX - 1 {
                *e = k as u32;
            } else if *e != k as u32 {
                *e = u32::MAX;
            }
        }
    }
    // `+ 0.0` turns -0.0 into 0.0, so both sides of a border at 0 match.
    let key = |i: usize| mesh.verts[i].pos.map(|x| (x + 0.0).to_bits());
    let mut by_pos: FxMap<[u32; 3], u32> = FxMap::default();
    by_pos.reserve(mesh.verts.len());
    for (i, &k) in by_index.iter().enumerate() {
        if k == u32::MAX - 1 {
            continue;
        }
        let e = by_pos.entry(key(i)).or_insert(k);
        if *e != k {
            *e = u32::MAX;
        }
    }

    let mut out = MeshData {
        verts: mesh.verts.clone(),
        indices: mesh.indices.clone(),
    };
    let mut levels: Vec<Vec<Level>> = exact
        .iter()
        .map(|r| {
            vec![Level {
                indices: r.clone(),
                error: 0.0,
            }]
        })
        .collect();
    // Each cell welded, then simplified level by level.
    struct Cell {
        verts: Vec<Vertex>,
        locked: Vec<bool>,
        base: u32,
        tris: Vec<u32>,
        error: f32,
        radius: f32,
    }
    let mut cells = Vec::with_capacity(exact.len());
    for r in exact {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        let (verts, welded, from) = lod::weld(&mesh.verts, tris(r));
        let locked = from
            .iter()
            .map(|&i| by_pos[&key(i as usize)] == u32::MAX)
            .collect();
        let (lo, hi) = verts.iter().fold(
            (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
            |(lo, hi), v| (lo.min(v.pos.into()), hi.max(v.pos.into())),
        );
        let base = out.verts.len() as u32;
        out.verts.extend_from_slice(&verts);
        cells.push(Cell {
            verts,
            locked,
            base,
            tris: welded,
            error: 0.0,
            radius: (hi - lo).length() * 0.5,
        });
    }
    // Level by level, every cell in turn, so neighbouring cells drawn at
    // one level are one range.
    for target in ERRORS {
        for (cell, levels) in cells.iter_mut().zip(&mut levels) {
            if cancel.load(Ordering::Relaxed) {
                return None;
            }
            let kept = cell.tris.len();
            if kept <= 36 || target > cell.radius * 0.25 || target <= cell.error {
                continue;
            }
            let step = target - cell.error;
            let (t, claimed) = lod::simplify(&cell.verts, &cell.tris, step, &cell.locked);
            // A copy only costs indices, so any real saving earns one.
            if t.is_empty() || t.len() * 8 > kept * 7 {
                continue;
            }
            let measured =
                lod::deviation(&cell.verts, &cell.tris, &cell.verts, &t, step, step * 2.0);
            // Within `step` measures 0, so `step` is all that's known.
            let error = claimed.max(measured).max(step);
            if error > step * 1.5 {
                continue;
            }
            let start = out.indices.len() as u32;
            out.indices.extend(t.iter().map(|i| i + cell.base));
            cell.error += error;
            levels.push(Level {
                indices: start..out.indices.len() as u32,
                error: cell.error,
            });
            cell.tris = t;
        }
    }
    Some(Levels { mesh: out, levels })
}

/// A [`build_levels`] running on its own thread for one world mesh.
pub struct WorldLod {
    /// Which world mesh it's for (the renderer's count of meshes set).
    pub generation: u64,
    result: mpsc::Receiver<Option<Levels>>,
    cancel: Arc<AtomicBool>,
}

impl WorldLod {
    pub fn start(generation: u64, mesh: Arc<MeshData>, exact: Vec<Range<u32>>) -> WorldLod {
        let cancel = Arc::new(AtomicBool::new(false));
        let (send, result) = mpsc::channel();
        let stop = cancel.clone();
        std::thread::Builder::new()
            .name("world lod".into())
            .spawn(move || {
                let _ = send.send(build_levels(&mesh, &exact, &stop));
            })
            .expect("spawn the world LOD thread");
        WorldLod {
            generation,
            result,
            cancel,
        }
    }

    /// The levels if they're done (waiting for them if `wait`).
    pub fn poll(&self, wait: bool) -> Option<Levels> {
        if wait {
            self.result.recv().ok().flatten()
        } else {
            self.result.try_recv().ok().flatten()
        }
    }
}

impl Drop for WorldLod {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec4;

    /// Bumpy ground, 96 m square: four cells by four.
    fn ground() -> MeshData {
        let mut m = MeshData::default();
        let c = Vec4::new(0.4, 0.4, 0.35, 1.0);
        let h = |x: f32, z: f32| (x * 0.21).sin() * 0.6 + (z * 0.13).cos() * 0.4;
        let n = 96;
        for iz in 0..=n {
            for ix in 0..=n {
                let (x, z) = (ix as f32, iz as f32);
                m.verts.push(Vertex {
                    pos: [x, h(x, z), z],
                    normal: [0.0, 1.0, 0.0],
                    color: c.to_array(),
                });
            }
        }
        let w = n as u32 + 1;
        for iz in 0..n as u32 {
            for ix in 0..n as u32 {
                let (a, b) = (iz * w + ix, iz * w + ix + 1);
                m.indices.extend_from_slice(&[a, b, b + w, a, b + w, a + w]);
            }
        }
        m
    }

    #[test]
    fn cells_get_coarser_levels_that_still_meet() {
        let (mesh, chunks) = split(&ground());
        assert_eq!(chunks.len(), 16);
        let exact: Vec<_> = chunks.iter().map(|c| c.levels[0].indices.clone()).collect();
        let lv = build_levels(&mesh, &exact, &AtomicBool::new(false)).expect("not cancelled");
        assert!(lv.levels.iter().all(|l| l.len() >= 3));
        // The exact triangles are untouched.
        assert_eq!(lv.mesh.indices[..mesh.indices.len()], mesh.indices[..]);
        // Every position on the line between two cells is still used by
        // each cell at its coarsest level: so any mix of levels has no gaps.
        let used = |k: usize, l: &Level| -> std::collections::HashSet<[u32; 3]> {
            let _ = k;
            lv.mesh.indices[l.indices.start as usize..l.indices.end as usize]
                .iter()
                .map(|&i| lv.mesh.verts[i as usize].pos.map(f32::to_bits))
                .collect()
        };
        for (k, levels) in lv.levels.iter().enumerate() {
            let exact = used(k, &levels[0]);
            let coarse = used(k, levels.last().expect("levels"));
            for p in &exact {
                let (x, z) = (f32::from_bits(p[0]), f32::from_bits(p[2]));
                let on_line = (x % CHUNK == 0.0 && x > 0.0 && x < 96.0)
                    || (z % CHUNK == 0.0 && z > 0.0 && z < 96.0);
                if on_line {
                    assert!(coarse.contains(p), "cell {k} dropped ({x}, {z})");
                }
            }
            assert!(coarse.len() < exact.len());
        }
    }

    #[test]
    fn cancelling_stops_it() {
        let (mesh, chunks) = split(&ground());
        let exact: Vec<_> = chunks.iter().map(|c| c.levels[0].indices.clone()).collect();
        assert!(build_levels(&mesh, &exact, &AtomicBool::new(true)).is_none());
    }
}
