//! A tall rock face sculpted as one smooth-shaded surface along a foot line:
//! the crystal mine's crags. Natural sandstone in warped strata with buttresses,
//! ledges and joints, seamed through with veins of crystal; quarried flat
//! and benched where it's been cut back, the natural rock overhanging the
//! top of the cut; hanging down only to a lintel over a tunnel's mouth.
//!
//! The face always stands forward of its foot line (toward the open side),
//! so ground rising behind the line stays hidden behind it. The map gives it
//! colliders (see `fronts`).

use glam::{Vec2, Vec3};

use super::valley::{bump, hash, noise, noise2};
use crate::render::material::Material;
use crate::render::mesh::{MeshData, Vertex};

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Red sandstone strata, bottom band first.
const STRATA: [Vec3; 6] = [
    Vec3::new(0.56, 0.33, 0.22),
    Vec3::new(0.6, 0.37, 0.24),
    Vec3::new(0.5, 0.29, 0.19),
    Vec3::new(0.62, 0.41, 0.28),
    Vec3::new(0.53, 0.31, 0.2),
    Vec3::new(0.58, 0.35, 0.23),
];
/// The top: weathered rock.
pub(super) const TOP: Vec3 = Vec3::new(0.46, 0.27, 0.18);
/// Freshly cut stone, and the scorch along a plasma cut.
const CUT: Vec3 = Vec3::new(0.74, 0.56, 0.42);
const SCORCH: Vec3 = Vec3::new(0.24, 0.14, 0.13);
/// Raw seam crystal, showing in the rock: the mine's raw crystal (cloudy
/// blue-grey, nobody's), a little darker where it's still in the stone.
const VEIN: Vec3 = Vec3::new(0.22, 0.27, 0.36);
const SAND: Vec3 = Vec3::new(0.66, 0.46, 0.3);
/// Strata band height, m.
const BAND: f32 = 3.4;
/// A quarry bench's height, m.
pub(super) const BENCH: f32 = 3.3;

/// One point on the foot line.
#[derive(Clone, Copy, Debug)]
pub(super) struct Foot {
    pub p: Vec2,
    /// Out of the rock, toward the open side.
    pub nrm: Vec2,
    /// Meters along the line.
    pub along: f32,
    /// The ground the face stands on (a tunnel's lintel: its underside).
    pub floor: f32,
    /// The rim.
    pub lip: f32,
    /// Quarried flat below this height (world y; the floor or below: not
    /// quarried).
    pub cut: f32,
    /// Over a tunnel's mouth: hangs down to `floor`, not buried.
    pub mouth: bool,
    /// How much relief (0: a flat plane, 1: full).
    pub relief: f32,
}

/// Seams of crystal through the rock, each repeating every this far up.
const VEIN_REPEAT: f32 = 30.0;

/// Seam `k` at `along`: its middle's height (one of them; it repeats every
/// `VEIN_REPEAT` m up), its half-width, and how much of it there is (0..1:
/// seams pinch out).
pub(super) fn vein_line(k: u32, along: f32) -> (f32, f32, f32) {
    let kf = k as f32;
    let dip = [0.16, -0.11, 0.07][k as usize];
    let line = [9.0, 19.0, 29.0][k as usize]
        + dip * along
        + 3.0 * (noise(along / 17.0, 40 + k) * 2.0 - 1.0)
        + 1.2 * (noise(along / 5.0, 50 + k) * 2.0 - 1.0);
    let width = 0.25 + 0.6 * noise(along / 9.0 + kf * 3.1, 60 + k);
    let present = smoothstep(0.3, 0.55, noise(along / 23.0 + kf * 5.7, 70 + k));
    (line, width, present)
}

/// Crystal veins running through the rock: how strongly (0..1) one shows at
/// `along`, height `y`. Three seams, dipping and wandering, pinching out.
pub(super) fn vein(along: f32, y: f32) -> f32 {
    let mut v = 0.0f32;
    for k in 0..3u32 {
        let (line, width, present) = vein_line(k, along);
        // Wrapped, so the dipping seams keep crossing the face.
        let d = (y - line).rem_euclid(VEIN_REPEAT);
        let d = d.min(VEIN_REPEAT - d);
        v = v.max((1.0 - smoothstep(width * 0.5, width, d)) * present);
    }
    v
}

/// Where the seams cross the face at `f`: (seam, height, half-width,
/// presence) for each, bottom to top.
pub(super) fn veins_at(f: &Foot) -> Vec<(u32, f32, f32, f32)> {
    let mut out = Vec::new();
    for k in 0..3u32 {
        let (line, width, present) = vein_line(k, f.along);
        let lo = ((f.floor - line) / VEIN_REPEAT).floor() as i32;
        let hi = ((f.lip - line) / VEIN_REPEAT).ceil() as i32;
        for j in lo..=hi {
            let y = line + j as f32 * VEIN_REPEAT;
            if y > f.floor + 0.4 && y < f.lip - 0.8 {
                out.push((k, y, width, present));
            }
        }
    }
    out.sort_by(|a, b| a.1.total_cmp(&b.1));
    out
}

/// How far forward of its foot (m, always > 0) the face stands at `f`,
/// height `y`.
pub(super) fn out(f: &Foot, y: f32) -> f32 {
    let a = f.along;
    let tall = (f.lip - f.floor).max(1.0);
    let up = y - f.floor;
    // Natural rock: buttresses and bays tens of meters across, smaller
    // bulges on them (taller than wide: the rock splits on vertical joints).
    let mass = 3.2 * noise2(a / 24.0, y / 45.0, 1)
        + 1.0 * noise2(a / 7.0, y / 16.0, 2)
        + 0.25 * noise2(a / 2.2, y / 5.0, 13);
    // Strata: harder bands set out, with ledges along their tops.
    let yw = y + 2.0 * (noise2(a / 26.0, y / 12.0, 3) * 2.0 - 1.0);
    let band = (yw / BAND).floor();
    let fr = yw / BAND - band;
    let set = 0.5 * noise(a / 21.0 + band * 7.31, 4);
    // (None at the very foot, half in the ground: a ledge there would lie
    // just under it, and show through in patches.)
    let ledge = 0.6
        * (noise(a / 9.0 + band * 3.17, 5) * 1.7 - 0.7).max(0.0)
        * smoothstep(0.72, 0.94, fr)
        * smoothstep(0.6, 1.8, y - f.floor);
    // Joints: clefts cut down from the rim.
    let cell = (a / 12.0).floor() as i32;
    let at = (cell as f32 + 0.15 + 0.7 * hash(cell, 1, 8)) * 12.0;
    let cleft = if hash(cell, 2, 8) > 0.5 {
        let reach = tall * (0.3 + 0.6 * hash(cell, 4, 8));
        -1.4 * bump(a - at, 0.5 + 0.8 * hash(cell, 3, 8))
            * (1.0 - smoothstep(reach - 3.0, reach, f.lip - y))
    } else {
        0.0
    };
    // Crystal stands proud of the softer rock round it.
    let crystal = 0.35 * vein(a, y);
    // Leaning back as it rises; the foot spreading.
    let lean = 1.3 * (1.0 - (up / tall).clamp(0.0, 1.0));
    let flare = 0.5 * smoothstep(1.5, -1.0, up);
    // Low down it stays within the colliders' reach; higher up it can
    // stand out further.
    let reach = 3.4 + 3.0 * smoothstep(6.0, 16.0, up);
    let tame = smoothstep(0.0, 1.0, (up - 1.0) / 8.0);
    let mass = mass * (0.55 + 0.45 * tame);
    let natural = (0.25 + (mass + set + ledge + cleft + crystal) * f.relief + lean + flare)
        .clamp(0.12, reach);
    // Quarried: flat benches, each set back from the one below, the cut's
    // top overhung by the rock above it.
    if y < f.cut {
        let k = ((y - f.floor) / BENCH).floor().max(0.0);
        let kerf = -0.06 * bump((a / 2.6).fract() - 0.5, 0.04);
        let quarry = 0.9 - 0.22 * k + kerf;
        let blend = smoothstep(f.cut - 0.5, f.cut, y);
        return (quarry + (natural - quarry) * blend).clamp(0.12, reach);
    }
    natural
}

/// The face's colour at `f`, height `y`.
fn shade(f: &Foot, y: f32) -> Vec3 {
    let a = f.along;
    let yw = y + 2.0 * (noise2(a / 26.0, y / 12.0, 3) * 2.0 - 1.0);
    let b = yw / BAND;
    let (i, fr) = (b.floor().max(0.0) as usize, b.fract());
    let pick = |i: usize| STRATA[(i * 7 + 3) % STRATA.len()];
    let band = pick(i).lerp(pick(i + 1), smoothstep(0.82, 1.0, fr));
    // Desert varnish: dark streaks running down from the ledges.
    let streak = noise2(a / 0.8, y / 30.0, 9) * noise(a / 12.0, 19);
    let varnish = 1.0 - 0.22 * smoothstep(0.4, 0.6, streak);
    let mottle = 0.88 + 0.22 * noise2(a / 6.0, y / 5.0, 10) + 0.08 * noise2(a / 1.5, y / 1.2, 22);
    let mut c = band * varnish * mottle;
    if y < f.cut {
        // Fresh stone, scorched along the bench lines where the plasma cut.
        let k = (y - f.floor) / BENCH;
        let line = 1.0 - smoothstep(0.0, 0.08, k.fract().min(1.0 - k.fract()));
        let fresh = CUT * (0.92 + 0.12 * noise2(a / 3.0, y / 2.0, 11));
        c = fresh.lerp(SCORCH, 0.8 * line);
        c = c.lerp(band * mottle, smoothstep(f.cut - 0.4, f.cut, y));
    }
    // The crystal showing through.
    let v = vein(a, y);
    // Raw crystal glinting in a dark selvage of crystal-stained rock.
    c = c.lerp(c * 0.55, smoothstep(0.0, 0.3, v));
    c = c.lerp(
        VEIN * (0.7 + 0.9 * noise2(a / 0.5, y / 0.5, 12)),
        smoothstep(0.45, 0.8, v),
    );
    // Sand drifted up the foot.
    if !f.mouth {
        c = c.lerp(SAND * 0.95, smoothstep(1.6, 0.2, y - f.floor));
    }
    c
}

/// Sculpt the face along `feet` into `m`. `behind` is the ground's height
/// at a point (for the cap over the rim). Returns, per foot, the face's most
/// forward point at tank height (`None` over a mouth).
pub(super) fn sculpt(
    m: &mut MeshData,
    feet: &[Foot],
    behind: &dyn Fn(Vec2) -> f32,
) -> Vec<Option<Vec2>> {
    const STEP: f32 = 0.7;
    const BACK: f32 = 9.0;
    let n = feet.len();
    let tallest = feet.iter().map(|f| f.lip - f.floor).fold(0.0, f32::max);
    let rows = 2 + (tallest / STEP).ceil() as usize + 2;
    let mut pos = vec![Vec3::ZERO; n * rows];
    let mut col = vec![Vec3::ZERO; n * rows];
    let mut fronts = Vec::with_capacity(n);
    for (i, f) in feet.iter().enumerate() {
        // Mouths hang down to the lintel; elsewhere the foot is buried.
        let bottom = if f.mouth { f.floor } else { f.floor - 1.4 };
        let mut front: Option<(f32, Vec2)> = None;
        for r in 0..rows {
            let (q, y, c) = if r == 0 {
                // The cap, back over the top.
                // Clear of the ground all the way back from the rim, not
                // just at its far edge: dipping under a rise between them
                // it'd show through in patches.
                let q = f.p - f.nrm * BACK;
                let rim = f.lip + 0.1;
                let from = f.p + f.nrm * out(f, rim);
                let y = (1..=16)
                    .map(|k| {
                        let t = k as f32 / 16.0;
                        rim + (behind(from.lerp(q, t)) + 0.05 - rim) / t
                    })
                    .fold(f.lip - 0.5, f32::max);
                (q, y, TOP)
            } else if r == 1 {
                let y = f.lip + 0.1;
                (f.p + f.nrm * out(f, y), y, shade(f, y))
            } else {
                let t = (r - 2) as f32 / (rows - 3) as f32;
                let jitter = (hash(i as i32, r as i32, 12) - 0.5) * 0.25;
                let y = (f.lip - (f.lip - bottom) * t + jitter).clamp(bottom, f.lip);
                let o = out(f, y);
                let q = f.p + f.nrm * o;
                if !f.mouth && y > f.floor + 0.4 && y < f.floor + 4.5 {
                    if front.is_none_or(|(best, _)| o > best) {
                        front = Some((o, q));
                    }
                }
                (q, y, shade(f, y))
            };
            pos[i * rows + r] = Vec3::new(q.x, y, q.y);
            col[i * rows + r] = c;
        }
        fronts.push(front.map(|(_, q)| q));
    }
    // The cap takes its own copy of the rim row, so the edge stays crisp.
    let first_dup = pos.len();
    for i in 0..n {
        pos.push(pos[i * rows + 1]);
        col.push(TOP);
    }
    let mut tris: Vec<[usize; 3]> = Vec::new();
    for i in 0..n - 1 {
        // A gap in the line (two pieces of face): no strip across it.
        if feet[i].p.distance(feet[i + 1].p) > 3.0 {
            continue;
        }
        let nm = (feet[i].nrm + feet[i + 1].nrm).normalize_or_zero();
        let outward = Vec3::new(nm.x, 0.0, nm.y);
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
            let want = if cap { Vec3::Y } else { outward };
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
        let mat = Material::Cliff;
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

/// Resample a line of corners into points about `step` m apart, rounding
/// the corners (over `round` m each side): (point, direction).
pub(super) fn resample(corners: &[Vec2], step: f32, round: f32) -> Vec<(Vec2, Vec2)> {
    // Fine points along the straight line first.
    let mut fine = Vec::new();
    for w in corners.windows(2) {
        let len = w[0].distance(w[1]);
        let k = (len / 0.1).ceil().max(1.0) as usize;
        for j in 0..k {
            fine.push(w[0].lerp(w[1], j as f32 / k as f32));
        }
    }
    fine.push(*corners.last().unwrap());
    // Round the corners: average each point with its neighbours within
    // `round` m (a moving mean), ends held.
    let reach = (round / 0.1) as usize;
    let n = fine.len();
    let smooth: Vec<Vec2> = (0..n)
        .map(|i| {
            let r = reach.min(i).min(n - 1 - i);
            let s: Vec2 = fine[i - r..=i + r].iter().copied().sum();
            s / (2 * r + 1) as f32
        })
        .collect();
    let mut pts = vec![smooth[0]];
    let mut acc = 0.0;
    for w in smooth.windows(2) {
        acc += w[0].distance(w[1]);
        if acc >= step {
            pts.push(w[1]);
            acc = 0.0;
        }
    }
    if pts.last().unwrap().distance(smooth[n - 1]) > step * 0.3 {
        pts.push(smooth[n - 1]);
    }
    let m = pts.len();
    (0..m)
        .map(|i| {
            let (a, b) = (pts[i.saturating_sub(1)], pts[(i + 1).min(m - 1)]);
            (pts[i], (b - a).normalize_or(Vec2::X))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_face_stands_forward_of_its_foot() {
        for k in 0..400 {
            let f = Foot {
                p: Vec2::ZERO,
                nrm: Vec2::X,
                along: k as f32 * 1.37,
                floor: 0.0,
                lip: 40.0,
                cut: if k % 2 == 0 { 10.0 } else { -1.0 },
                mouth: false,
                relief: 1.0,
            };
            for j in 0..60 {
                let o = out(&f, j as f32 * 0.7 - 1.0);
                let y = j as f32 * 0.7 - 1.0;
                let reach = if y < 5.0 { 3.5 } else { 6.5 };
                assert!(o > 0.1 && o < reach, "{o} at {k}, {j}");
            }
        }
    }

    #[test]
    fn veins_show_but_not_everywhere() {
        let mut hits = 0;
        let total = 200 * 60;
        for k in 0..200 {
            for j in 0..60 {
                if vein(k as f32 * 1.1, j as f32 * 0.7) > 0.5 {
                    hits += 1;
                }
            }
        }
        assert!(hits * 50 > total, "veins on {hits} of {total}");
        assert!(hits * 4 < total, "veins on {hits} of {total}");
    }
}
