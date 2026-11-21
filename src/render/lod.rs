//! Level of detail: coarser copies of a mesh's triangles, over the same
//! vertices, for drawing it far away.
//!
//! The kit and the maps are flat-shaded: every face has its own copies of its
//! corners, so to a simplifier every crease is a seam it may not collapse
//! across, and it can't remove anything. So the simplifier sees the mesh
//! welded, one vertex per position and color (color boundaries stay seams,
//! normal creases don't), and each corner of each triangle it keeps goes back
//! to the copy of that vertex whose normal best fits the triangle: the
//! original vertex buffer serves every level, and creases stay sharp.
//!
//! Each level knows how far its surface strays from the full mesh, in the
//! mesh's own units. The renderer picks, per instance and per ground cell,
//! the coarsest level that strays less than about half a pixel on screen
//! (or a fraction of a texel in the shadow map), so a switch never shows.

use std::collections::HashMap;

use glam::Vec3;
use meshopt::SimplifyOptions;

use super::mesh::MeshData;

static DISABLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Draw everything whole from now on (`--no-lod`), for comparing against.
pub fn disable() {
    DISABLED.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Whether `--no-lod` turned levels off.
pub fn disabled() -> bool {
    DISABLED.load(std::sync::atomic::Ordering::Relaxed)
}

/// How far a drawn level may stray from the full mesh, in screen pixels.
pub const PIXEL_ERROR: f32 = 0.5;

/// How far a level drawn into the shadow map may stray, in shadow texels.
pub const TEXEL_ERROR: f32 = 0.5;

/// How much a full change of albedo (0 to 1 in a channel) counts against a
/// level, as if its surface had moved this many meters.
const COLOR_WEIGHT: f32 = 0.25;

/// The same for a change of material id.
const MATERIAL_WEIGHT: f32 = 0.25;

/// At most this many levels below the full mesh.
const MAX_LEVELS: usize = 6;

/// A level must keep at most this share of the one above it to be worth it.
const MIN_SAVING: f32 = 0.8;

/// Below this many triangles a mesh isn't simplified further.
const MIN_TRIANGLES: usize = 24;

/// Positions closer than one over this (meters) weld.
const WELD: f32 = 1e4;

/// A coarser copy of an index list, and the most its surface strays from the
/// full mesh's (mesh units).
pub struct Level {
    pub indices: Vec<u32>,
    pub error: f32,
}

/// A mesh's vertices welded by position and color, the input the simplifier
/// sees.
pub struct Welded {
    positions: Vec<[f32; 3]>,
    /// Each welded vertex's color and material, weighted for the simplifier.
    attributes: Vec<[f32; 4]>,
    /// Each original vertex's welded one.
    of: Vec<u32>,
    /// Each welded vertex's original copies: `copies[start[w]..start[w + 1]]`.
    start: Vec<u32>,
    copies: Vec<u32>,
    /// No vertex is locked (the simplifier wants a flag for each).
    unlocked: Vec<bool>,
}

impl Welded {
    pub fn new(mesh: &MeshData) -> Welded {
        let mut ids: HashMap<([u32; 3], [u32; 4]), u32> = HashMap::with_capacity(mesh.verts.len());
        let mut positions = Vec::new();
        let mut attributes = Vec::new();
        let of: Vec<u32> = mesh
            .verts
            .iter()
            .map(|v| {
                // Corners built by different sums land a rounding error
                // apart, so positions meet on a tenth-of-a-millimetre grid.
                let p = v.pos.map(|x| (x * WELD).round() as i32 as u32);
                let c = v.color.map(|x| (x + 0.0).to_bits());
                let key = (p, c);
                *ids.entry(key).or_insert_with(|| {
                    positions.push(v.pos);
                    attributes.push(v.color);
                    positions.len() as u32 - 1
                })
            })
            .collect();
        let mut start = vec![0u32; positions.len() + 1];
        for &w in &of {
            start[w as usize + 1] += 1;
        }
        for k in 1..start.len() {
            start[k] += start[k - 1];
        }
        let mut fill = start.clone();
        let mut copies = vec![0u32; of.len()];
        for (i, &w) in of.iter().enumerate() {
            copies[fill[w as usize] as usize] = i as u32;
            fill[w as usize] += 1;
        }
        Welded {
            unlocked: vec![false; positions.len()],
            positions,
            attributes,
            of,
            start,
            copies,
        }
    }

    /// `indices` over welded vertices, without the triangles welding
    /// collapses, each wound to face the way its normals point. (Nothing is
    /// back-face culled, so the kit's winding is arbitrary; the simplifier
    /// needs neighbours wound alike to see a surface.)
    fn weld(&self, mesh: &MeshData, indices: &[u32]) -> Vec<u32> {
        let mut out = Vec::with_capacity(indices.len());
        for t in indices.chunks_exact(3) {
            let w = [t[0], t[1], t[2]].map(|i| self.of[i as usize]);
            if w[0] == w[1] || w[1] == w[2] || w[0] == w[2] {
                continue;
            }
            let p = [t[0], t[1], t[2]].map(|i| Vec3::from(mesh.verts[i as usize].pos));
            let n: Vec3 = t
                .iter()
                .map(|&i| Vec3::from(mesh.verts[i as usize].normal))
                .sum();
            if (p[1] - p[0]).cross(p[2] - p[0]).dot(n) < 0.0 {
                out.extend_from_slice(&[w[0], w[2], w[1]]);
            } else {
                out.extend_from_slice(&w);
            }
        }
        out
    }

    /// Welded triangles back onto original vertices: each corner takes the
    /// copy whose normal best fits its triangle.
    fn unweld(&self, mesh: &MeshData, welded: &[u32]) -> Vec<u32> {
        let mut out = Vec::with_capacity(welded.len());
        for t in welded.chunks_exact(3) {
            let p = [t[0], t[1], t[2]].map(|w| Vec3::from(self.positions[w as usize]));
            let face = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero();
            for &w in t {
                let copies = &self.copies
                    [self.start[w as usize] as usize..self.start[w as usize + 1] as usize];
                let best = copies
                    .iter()
                    .copied()
                    .max_by(|&a, &b| {
                        let fit = |i: u32| Vec3::from(mesh.verts[i as usize].normal).dot(face);
                        fit(a).total_cmp(&fit(b))
                    })
                    .expect("every welded vertex has a copy");
                out.push(best);
            }
        }
        out
    }
}

/// The levels below the full `indices` (a subset of `mesh`'s triangles),
/// each about half the one above, finest first. With `lock_border`, the
/// subset's open edges don't move, so neighbouring subsets drawn at
/// different levels still meet without cracks.
pub fn levels(mesh: &MeshData, welded: &Welded, indices: &[u32], lock_border: bool) -> Vec<Level> {
    // Collapses may cross color boundaries (a bevel's edge color is a strip
    // a few centimetres wide), but what they change counts as error.
    let mut opts =
        SimplifyOptions::ErrorAbsolute | SimplifyOptions::Sparse | SimplifyOptions::Permissive;
    if lock_border {
        opts |= SimplifyOptions::LockBorder;
    }
    let positions = meshopt::VertexDataAdapter::new(
        bytemuck::cast_slice(&welded.positions),
        std::mem::size_of::<[f32; 3]>(),
        0,
    )
    .expect("tightly packed positions");
    let attributes: &[f32] = bytemuck::cast_slice(&welded.attributes);
    let weights = [COLOR_WEIGHT, COLOR_WEIGHT, COLOR_WEIGHT, MATERIAL_WEIGHT];
    let unlocked = &welded.unlocked;
    let mut out = Vec::new();
    let mut current = welded.weld(mesh, indices);
    let mut error = 0.0;
    let mut count = indices.len();
    while out.len() < MAX_LEVELS && current.len() / 3 >= MIN_TRIANGLES {
        let target = current.len() / 6 * 3;
        let mut step = 0.0;
        let next = meshopt::simplify_with_attributes_and_locks(
            &current,
            &positions,
            attributes,
            &weights,
            std::mem::size_of::<[f32; 4]>(),
            unlocked,
            target,
            f32::MAX,
            opts,
            Some(&mut step),
        );
        if next.len() as f32 > count as f32 * MIN_SAVING || next.is_empty() {
            break;
        }
        // Each step is measured from the one before, so the errors add up
        // to a bound on the distance from the full mesh.
        error += step;
        count = next.len();
        out.push(Level {
            indices: welded.unweld(mesh, &next),
            error,
        });
        current = next;
    }
    out
}

/// Screen pixels one meter covers at `distance`, for a view `height` pixels
/// tall with vertical field of view `fov_y`.
pub fn pixels_per_meter(height: f32, fov_y: f32, distance: f32) -> f32 {
    height / (2.0 * (fov_y * 0.5).tan() * distance.max(1e-3))
}

/// The coarsest level (0 is the full mesh) whose error, times `scale`, stays
/// within `allowed` world units. `errors` are the levels below the full mesh,
/// finest (smallest) first.
pub fn pick(errors: impl IntoIterator<Item = f32>, scale: f32, allowed: f32) -> usize {
    errors
        .into_iter()
        .take_while(|&e| e * scale <= allowed)
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::mesh::CpuMeshes;

    fn all_levels(m: &MeshData) -> Vec<Level> {
        levels(m, &Welded::new(m), &m.indices, false)
    }

    #[test]
    fn a_bevelled_block_simplifies() {
        let mut m = MeshData::default();
        crate::tank::model::block(
            &mut m,
            Vec3::ZERO,
            Vec3::new(1.0, 0.5, 2.0),
            0.05,
            crate::tank::model::METAL,
        );
        let l = all_levels(&m);
        assert!(
            !l.is_empty(),
            "nothing came off {} triangles",
            m.indices.len() / 3
        );
        let last = l.last().unwrap();
        assert!(last.indices.len() < m.indices.len() / 2);
        // Bevels are what goes first: the block's still about a block.
        assert!(l[0].error <= 0.06, "first level strays {}", l[0].error);
    }

    #[test]
    fn levels_keep_original_vertices_and_winding() {
        let mut store = CpuMeshes::new();
        let _art = crate::tank::TankArt::upload(&mut store);
        let (mut tris, mut flipped) = (0, 0);
        for m in &store.0 {
            for l in all_levels(m) {
                assert!(l.indices.iter().all(|&i| (i as usize) < m.verts.len()));
                // Kept triangles face the way their corners' normals do.
                for t in l.indices.chunks_exact(3) {
                    let p = [t[0], t[1], t[2]].map(|i| Vec3::from(m.verts[i as usize].pos));
                    let face = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero();
                    let n: Vec3 = t
                        .iter()
                        .map(|&i| Vec3::from(m.verts[i as usize].normal))
                        .sum();
                    tris += 1;
                    if face.dot(n.normalize_or_zero()) < -0.1 {
                        flipped += 1;
                    }
                }
            }
        }
        assert!(
            flipped * 100 < tris,
            "{flipped} of {tris} triangles flipped"
        );
    }

    #[test]
    fn pick_takes_the_coarsest_that_fits() {
        let e = [0.01, 0.04, 0.2];
        assert_eq!(pick(e, 1.0, 0.0), 0);
        assert_eq!(pick(e, 1.0, 0.05), 2);
        assert_eq!(pick(e, 1.0, 1.0), 3);
        assert_eq!(pick(e, 10.0, 0.05), 0);
        assert_eq!(pick(std::iter::empty(), 1.0, 1.0), 0);
    }

    #[test]
    #[ignore]
    fn survey() {
        let mut store = CpuMeshes::new();
        let _art = crate::tank::TankArt::upload(&mut store);
        for (k, m) in store.0.iter().enumerate() {
            let n = m.indices.len() / 3;
            let rs: Vec<String> = all_levels(m)
                .iter()
                .map(|l| format!("{} e{:.3}", l.indices.len() / 3, l.error))
                .collect();
            println!("mesh {k}: {n} tris  {}", rs.join("  "));
        }
    }
}
