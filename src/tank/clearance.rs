//! Keeping the running gear clear of the armor over it: whether any wheel
//! or tread link cuts through a hull's body, and how far each road wheel
//! may rise on its suspension before it would.

use glam::{Mat4, Vec3};

use super::model::HullModel;
use super::{Gear, running_gear};
use crate::render::mesh::MeshData;

type Tri = [Vec3; 3];

/// A mesh's triangles, placed by `at`.
fn triangles(m: &MeshData, at: Mat4) -> Vec<Tri> {
    m.indices
        .chunks(3)
        .map(|t| [0, 1, 2].map(|k| at.transform_point3(Vec3::from(m.verts[t[k] as usize].pos))))
        .collect()
}

fn bounds(t: &[Tri]) -> (Vec3, Vec3) {
    t.iter()
        .flatten()
        .fold((Vec3::MAX, Vec3::MIN), |(lo, hi), &p| {
            (lo.min(p), hi.max(p))
        })
}

/// Whether segment `p`-`q` passes through the inside of triangle `t`.
/// Faces that only touch don't count.
fn segment_hits(p: Vec3, q: Vec3, t: Tri) -> bool {
    let (e1, e2) = (t[1] - t[0], t[2] - t[0]);
    let d = q - p;
    let h = d.cross(e2);
    let a = e1.dot(h);
    if a.abs() < 1e-9 {
        return false;
    }
    let f = 1.0 / a;
    let s = p - t[0];
    let u = f * s.dot(h);
    let qv = s.cross(e1);
    let v = f * d.dot(qv);
    let w = f * e2.dot(qv);
    let eps = 1e-4;
    u > eps && v > eps && u + v < 1.0 - eps && w > eps && w < 1.0 - eps
}

fn tris_cross(a: Tri, b: Tri) -> bool {
    (0..3).any(|i| segment_hits(a[i], a[(i + 1) % 3], b) || segment_hits(b[i], b[(i + 1) % 3], a))
}

/// A hull's body and running gear meshes, ready to test poses against.
pub struct Clearance<'a> {
    hull: &'a HullModel,
    /// Link, road wheel, sprocket, idler.
    parts: &'a [MeshData; 4],
    links: usize,
    /// Body triangles with their bounds.
    body: Vec<(Tri, Vec3, Vec3)>,
}

impl<'a> Clearance<'a> {
    pub fn new(hull: &'a HullModel, parts: &'a [MeshData; 4], links: usize) -> Clearance<'a> {
        let body = triangles(&hull.body, Mat4::IDENTITY)
            .into_iter()
            .map(|t| {
                let (lo, hi) = bounds(&[t]);
                (t, lo, hi)
            })
            .collect();
        Clearance {
            hull,
            parts,
            links,
            body,
        }
    }

    /// Where the gear posed this way cuts through the body: the centre of
    /// each body triangle it crosses, and which piece crosses it. Only the
    /// pieces `keep` picks (by where they sit) are tested.
    pub fn clips(
        &self,
        stops: &[f32],
        lift: &dyn Fn(usize, usize) -> f32,
        travel: [f32; 2],
        keep: &dyn Fn(Gear, Vec3) -> bool,
    ) -> Vec<(Vec3, Gear)> {
        let mut out = Vec::new();
        for (gear, at) in running_gear(&self.hull.running, stops, self.links, lift, travel) {
            if !keep(gear, at.w_axis.truncate()) {
                continue;
            }
            let mesh = &self.parts[match gear {
                Gear::Link => 0,
                Gear::Road => 1,
                Gear::Sprocket => 2,
                Gear::Idler => 3,
            }];
            let tris = triangles(mesh, at);
            let (lo, hi) = bounds(&tris);
            for (b, blo, bhi) in &self.body {
                if blo.cmpgt(hi).any() || bhi.cmplt(lo).any() {
                    continue;
                }
                if tris.iter().any(|&t| tris_cross(*b, t)) {
                    out.push(((b[0] + b[1] + b[2]) / 3.0, gear));
                }
            }
        }
        out
    }

    /// How far each road wheel can rise, up to `max`, with the tread
    /// wrapped over it still clear of the armor: found with the others at
    /// rest and the links in one place, then backed off by `MARGIN` for
    /// the links passing over it and its neighbours rising with it.
    pub fn bump_stops(&self, max: f32) -> Vec<f32> {
        const MARGIN: f32 = 0.03;
        let run = &self.hull.running;
        run.road
            .iter()
            .enumerate()
            .map(|(k, w)| {
                // The rest of the gear is clear at rest, so only this wheel
                // and the links near it (on one side: the hulls are
                // symmetric) need testing.
                let near = |gear: Gear, at: Vec3| {
                    at.x > 0.0
                        && match gear {
                            Gear::Road => (at.z - w.center.x).abs() < 1e-3,
                            Gear::Link => (at.z - w.center.x).abs() < w.radius + 0.4,
                            Gear::Sprocket | Gear::Idler => false,
                        }
                };
                let clear = |l: f32| {
                    let lift =
                        move |side: usize, j: usize| if side == 0 && j == k { l } else { 0.0 };
                    self.clips(&[], &lift, [0.0, 0.0], &near).is_empty()
                };
                if clear(max) {
                    return max;
                }
                let (mut lo, mut hi) = (0.0, max);
                for _ in 0..7 {
                    let mid = (lo + hi) * 0.5;
                    if clear(mid) {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                (lo - MARGIN).max(0.0)
            })
            .collect()
    }
}
