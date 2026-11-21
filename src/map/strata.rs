//! Layered sandstone, for the Sanctum's mesa, the buttes broken off it and
//! the mesas on its horizon.
//!
//! A tower of rock is beds stacked up at fixed heights, level across the
//! whole tower (and across the buttes, which share the mesa's beds). Hard
//! beds stand out and soft ones have weathered back under them, so each
//! hard bed overhangs the one below. Joints break every bed into slabs:
//! long joints that run straight down through many beds, and shorter ones
//! that break the soft beds up more finely. Each slab is a block of its own,
//! its edges rounded off and its faces lumpy, set a little in or out of its
//! neighbours and leaning a little; here and there one has fallen out,
//! leaving a notch under the bed above, and lies at the foot. Behind them
//! all a core of rock, dark, closes off the cracks between them.
//!
//! All of it is looks: what you drive on is the map's ground.

use std::f32::consts::{PI, TAU};

use glam::{Quat, Vec2, Vec3, Vec3Swizzles};

use crate::render::material::Material;
use crate::render::mesh::{MeshData, Vertex};
use crate::rng::Rng;

/// Loose sand, for tops and ledges.
const SAND: Vec3 = Vec3::new(0.74, 0.62, 0.47);

/// Sandstone, bed by bed: pale buff to deep red-brown.
const SHADES: [Vec3; 6] = [
    Vec3::new(0.72, 0.5, 0.33),
    Vec3::new(0.52, 0.31, 0.2),
    Vec3::new(0.8, 0.64, 0.46),
    Vec3::new(0.6, 0.37, 0.24),
    Vec3::new(0.68, 0.44, 0.28),
    Vec3::new(0.76, 0.58, 0.4),
];

/// One bed of rock, from `y0` up to `y1`.
#[derive(Clone, Copy, Debug)]
pub struct Bed {
    pub y0: f32,
    pub y1: f32,
    /// Hard beds stand out and break into big slabs; soft ones are
    /// weathered back and break small.
    pub hard: bool,
    /// How far out of the core its slabs stand, m (before `relief`).
    set: f32,
    color: Vec3,
}

/// Beds from `top` down to `bottom`, each `scale` times the usual thickness.
/// A hard caprock first.
pub fn beds(top: f32, bottom: f32, scale: f32, seed: u64) -> Vec<Bed> {
    let mut rng = Rng::new(seed);
    let mut out: Vec<Bed> = Vec::new();
    let mut y = top;
    let mut hard = true;
    while y > bottom {
        let thick = if out.is_empty() {
            rng.range(3.2, 4.2)
        } else if hard && rng.chance(0.5) {
            // A massive bed, the cliff's sheer walls.
            rng.range(5.5, 10.0)
        } else if hard {
            rng.range(2.4, 4.5)
        } else {
            rng.range(0.6, 1.7)
        } * scale;
        let set = if out.is_empty() {
            rng.range(0.9, 1.2)
        } else if hard {
            rng.range(1.1, 2.0)
        } else {
            rng.range(0.0, 0.3)
        };
        let shade = SHADES[rng.index(SHADES.len())];
        // Hard beds bleach paler; soft ones are redder, and darker for the
        // shade they sit in.
        let color = if hard {
            shade.lerp(Vec3::new(0.84, 0.7, 0.54), 0.08)
        } else {
            shade * Vec3::new(0.95, 0.88, 0.85)
        } * rng.range(0.94, 1.06);
        out.push(Bed {
            y0: y - thick,
            y1: y,
            hard,
            set,
            color,
        });
        y -= thick;
        // Mostly in turn, now and then two of a kind.
        hard = if hard {
            rng.chance(0.2)
        } else {
            rng.chance(0.85)
        };
    }
    out
}

/// A tower of rock to build.
pub struct Tower<'a> {
    pub center: Vec2,
    /// Its edge (where the top's ground ends): distance from the center at
    /// an angle.
    pub rim: &'a dyn Fn(f32) -> f32,
    /// Height of the ground on its top at a point; the beds stop under it.
    pub lip: &'a dyn Fn(Vec2) -> f32,
    /// Height of the ground round its foot at a point.
    pub floor: &'a dyn Fn(Vec2) -> f32,
    pub beds: &'a [Bed],
    /// How far out of the rim the core stands at the top, m.
    pub core: f32,
    /// How much further out it stands at the foot than at the top, m.
    pub spread: f32,
    /// How far the slabs stand in and out, 1 for a full-size mesa.
    pub relief: f32,
    /// Size of the beds and slabs, 1 for the mesa.
    pub scale: f32,
    /// Size of the mesh's cells, 1 up close; more for rock only ever seen
    /// from far off.
    pub cell: f32,
    /// Its top is ground you play on: sand reaches this far in from the
    /// rim (negative), tucked under that ground. `None`: the top is closed
    /// over, out of reach.
    pub ring_cap: Option<f32>,
    pub seed: u64,
}

/// A slab that fell out of the face: where it was (angle, height) and how
/// big it was.
pub struct Fallen {
    pub angle: f32,
    pub size: Vec3,
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

impl Tower<'_> {
    fn top(&self) -> f32 {
        self.beds.first().map_or(0.0, |b| b.y1)
    }

    fn bottom(&self) -> f32 {
        self.beds.last().map_or(0.0, |b| b.y0)
    }

    /// How far out of the rim the core stands at angle `a`, height `y`:
    /// near sheer at the top, sloping out over the softer beds lower down.
    fn core_at(&self, a: f32, y: f32) -> f32 {
        let s = ((self.top() - y) / (self.top() - self.bottom())).clamp(0.0, 1.0);
        // Never in from `core`: the ground's own drop at the edge is in
        // behind it.
        let wobble = 0.3 * self.relief * (1.0 + noise(Vec3::new(a * 9.0, y * 0.05, 0.0), 3));
        self.core
            + self.spread * (0.47 * s + 0.53 * smoothstep(0.55, 1.0, s))
            + wobble * smoothstep(0.0, 6.0, self.top() - y)
    }

    fn at(&self, a: f32, out: f32, y: f32) -> Vec3 {
        let r = (self.rim)(a);
        let p = self.center + Vec2::new(a.cos(), a.sin()) * (r + out);
        // The beds aren't ruled lines: they dip and rise gently along the
        // face (not at the top, which is the ground's).
        let k = self.seed as u32;
        let warp = (0.9 * noise(Vec3::new(a * r / 45.0, y / 40.0, 0.5), k + 70)
            + 0.3 * noise(Vec3::new(a * r / 12.0, y / 15.0, 0.5), k + 71))
            * self.scale
            * smoothstep(0.0, 5.0, self.top() - y);
        Vec3::new(p.x, y + warp, p.y)
    }

    /// The floor out in front of the face at angle `a`, standing `out` out
    /// of the rim; in a bay that can be back over the top, which isn't.
    fn ground_before(&self, a: f32, out: f32) -> f32 {
        let g = (self.floor)(self.at(a, out + 4.0, 0.0).xz());
        if g > self.top() - 10.0 { f32::MIN } else { g }
    }

    fn lip_at(&self, a: f32) -> f32 {
        let edge = (self.rim)(a);
        (self.lip)(self.center + Vec2::new(a.cos(), a.sin()) * (edge - 1.6).max(0.0))
    }

    /// Angles of the long joints round the tower: one every 5-12 m of rim.
    fn joints(&self, rng: &mut Rng) -> Vec<f32> {
        let start = rng.range(0.0, 0.1);
        let mut out = Vec::new();
        let mut a = start;
        while a < start + TAU - 0.01 {
            out.push(a);
            a += rng.range(3.5, 9.0) * self.scale / (self.rim)(a).max(3.0);
        }
        // The last slab mustn't be a sliver.
        if out.len() > 2 && start + TAU - out[out.len() - 1] < 2.0 * self.scale / (self.rim)(0.0) {
            out.pop();
        }
        out.push(start + TAU);
        out
    }
}

/// One block of rock: an arc of a bed, between two joints.
struct Slab {
    a0: f32,
    a1: f32,
    y0: f32,
    y1: f32,
    /// How far out of the rim its front and back stand at the middle.
    front: f32,
    back: f32,
    /// How much further out its front stands at its top than its foot.
    lean: f32,
    color: Vec3,
    soft: bool,
    key: u32,
}

/// Build `t` into `m`. Returns the slabs that fell out.
pub fn build(m: &mut MeshData, t: &Tower) -> Vec<Fallen> {
    let mut rng = Rng::new(t.seed);
    let long = t.joints(&mut rng);
    let mut fallen = Vec::new();
    let mut slabs = Vec::new();
    for (b, bed) in t.beds.iter().enumerate() {
        // Through-going joints, most of them reaching this bed; then breaks
        // to keep its slabs to size.
        let mut cuts: Vec<f32> = long
            .iter()
            .enumerate()
            .filter(|&(i, _)| {
                i == 0 || i + 1 == long.len() || rng.chance(if bed.hard { 0.8 } else { 0.45 })
            })
            .map(|(_, &a)| a)
            .collect();
        let most = if bed.hard { 13.0 } else { 9.0 } * t.scale;
        let mut k = 0;
        while k + 1 < cuts.len() {
            let (a0, a1) = (cuts[k], cuts[k + 1]);
            let len = (a1 - a0) * (t.rim)(a0);
            if len > most {
                let f = rng.range(0.35, 0.65);
                cuts.insert(k + 1, a0 + (a1 - a0) * f);
            } else {
                k += 1;
            }
        }
        for w in cuts.windows(2) {
            let r = (t.rim)(w[0]).max(3.0);
            let len = (w[1] - w[0]) * r;
            let mid = (w[0] + w[1]) * 0.5;
            // The joint's open a little either side.
            let gap = |rng: &mut Rng| rng.range(0.05, 0.22) * t.scale / r;
            let (a0, a1) = (w[0] + gap(&mut rng), w[1] - gap(&mut rng));
            let floor = (t.floor)(t.at(mid, t.core_at(mid, bed.y1) + 1.0, 0.0).xz());
            // Buried in the sand at the foot, or above the top: nothing to see.
            let lip = t.lip_at(w[0]).max(t.lip_at(mid)).max(t.lip_at(w[1]));
            if bed.y1 < floor - 1.5 || bed.y0 + 0.2 > lip {
                continue;
            }
            // A thick bed sometimes parts in two along a slab.
            let thick = bed.y1 - bed.y0;
            let split = if thick > 4.5 * t.scale && rng.chance(0.15) {
                bed.y0 + thick * rng.range(0.3, 0.7)
            } else {
                f32::NAN
            };
            // Whole stretches of a bed have weathered further back than
            // others.
            let retreat = noise(
                Vec3::new(mid * r / (18.0 * t.scale), b as f32 * 3.1, 0.5),
                40,
            );
            let set = bed.set + 0.7 * retreat + rng.range(-0.3, 0.3);
            let pieces = if split.is_nan() {
                vec![(bed.y0, bed.y1)]
            } else {
                vec![(bed.y0, split), (split, bed.y1)]
            };
            for (y0, y1) in pieces {
                let part = |rng: &mut Rng| rng.range(0.03, 0.1) * t.scale;
                let (y0, y1) = (y0 + part(&mut rng), y1 - part(&mut rng));
                let core = t.core_at(mid, (y0 + y1) * 0.5);
                let mut front = core + (set + rng.range(-0.2, 0.2)).max(0.2) * t.relief;
                let mut hollow = false;
                // Now and then one has come away, leaving a notch: under
                // the caprock, not in it, and not where it's buried.
                let chance = if bed.hard { 0.07 } else { 0.03 };
                if b > 0 && len < 10.0 * t.scale && y0 > floor + 2.0 && rng.chance(chance) {
                    fallen.push(Fallen {
                        angle: mid,
                        size: Vec3::new(len, y1 - y0, front - core),
                    });
                    front = core + 0.12 * t.relief;
                    hollow = true;
                }
                slabs.push(Slab {
                    a0,
                    a1,
                    y0,
                    y1,
                    front,
                    back: core - 0.25,
                    lean: rng.range(-0.25, 0.3) * t.relief,
                    // Down in a hollow, out of the light.
                    color: bed.color * rng.range(0.92, 1.07) * if hollow { 0.7 } else { 1.0 },
                    soft: !bed.hard,
                    key: rng.next_u64() as u32,
                });
            }
        }
    }
    for s in &slabs {
        slab(m, t, s);
    }
    core(m, t);
    fallen
}

/// The rock's color at `p`, its own `color` there, facing `n`: sand on what
/// faces up and where the sand has drifted against the foot, the ground in
/// front of it being at `ground`.
fn tint(p: Vec3, n: Vec3, ground: f32, color: Vec3, k: u32) -> Vec3 {
    let blotch = 0.92 + 0.14 * noise(p * 0.18, k) + 0.05 * noise(p * 0.7, k + 1);
    let c = color * blotch * if n.y < -0.3 { 0.82 } else { 1.0 };
    // Sand lies in patches on what faces up, bare rock between.
    let patches = smoothstep(
        -0.25,
        0.35,
        noise(p * 0.3, k + 7) + 0.5 * noise(p * 0.9, k + 8),
    );
    let dust = smoothstep(0.6, 0.92, n.y) * 0.5 * patches;
    let drift = smoothstep(5.0, 0.8, p.y - ground);
    c.lerp(SAND, dust.max(drift))
}

fn push(
    m: &mut MeshData,
    pos: &[Vec3],
    nrm: &[Vec3],
    col: &[Vec3],
    mat: &[Material],
    tris: &[[usize; 3]],
) {
    let base = m.verts.len() as u32;
    for k in 0..pos.len() {
        m.verts.push(Vertex {
            pos: pos[k].to_array(),
            normal: nrm[k].normalize_or_zero().to_array(),
            color: col[k].extend(mat[k].id()).to_array(),
        });
    }
    for t in tris {
        m.indices.extend(t.iter().map(|&k| base + k as u32));
    }
}

/// Smooth normals over `tris`.
fn normals(pos: &[Vec3], tris: &[[usize; 3]]) -> Vec<Vec3> {
    let mut nrm = vec![Vec3::ZERO; pos.len()];
    for t in tris {
        let f = (pos[t[1]] - pos[t[0]]).cross(pos[t[2]] - pos[t[0]]);
        for &k in t {
            nrm[k] += f;
        }
    }
    nrm
}

/// Triangles of a `cols` x `rows` grid (column-major), each wound to face
/// along `want` at its first corner.
fn grid(pos: &[Vec3], want: &[Vec3], cols: usize, rows: usize, tris: &mut Vec<[usize; 3]>) {
    let id = |c: usize, r: usize| c * rows + r;
    for c in 0..cols - 1 {
        for r in 0..rows - 1 {
            let q = [id(c, r), id(c + 1, r), id(c + 1, r + 1), id(c, r + 1)];
            let pair = if (c + r) % 2 == 0 {
                [[q[0], q[1], q[2]], [q[0], q[2], q[3]]]
            } else {
                [[q[0], q[1], q[3]], [q[1], q[2], q[3]]]
            };
            for mut t in pair {
                let w = want[q[0]] + want[q[2]];
                let f = (pos[t[1]] - pos[t[0]]).cross(pos[t[2]] - pos[t[0]]);
                if f.length_squared() < 1e-10 {
                    continue;
                }
                if f.dot(w) < 0.0 {
                    t.swap(1, 2);
                }
                tris.push(t);
            }
        }
    }
}

/// A slab: a grid round its section (back along its top, round the front
/// corner, down its face, round the foot and back under it), swept along
/// its arc, its ends rounding in to nothing at the joints.
fn slab(m: &mut MeshData, t: &Tower, s: &Slab) {
    let sc = t.scale;
    let r_mid = (t.rim)((s.a0 + s.a1) * 0.5).max(3.0);
    let len = (s.a1 - s.a0) * r_mid;
    let thick = s.y1 - s.y0;
    let depth = s.front - s.back;
    if len < 0.3 || thick < 0.2 || depth < 0.1 {
        return;
    }
    // Rounding: of the corners in section, and of the ends in plan.
    // Soft rock weathers round; hard rock keeps its edges.
    let round = if s.soft { 0.45 } else { 0.24 };
    let rc = (round * sc).min(thick * 0.4).min(depth * 0.45);
    let re = (0.45 * sc).min(len * 0.4).min(depth);
    // Across the slab: closely round the ends, then evenly.
    let fine: &[f32] = if t.cell > 1.5 {
        &[0.0, 0.4, 1.0]
    } else {
        &[0.0, 0.1, 0.32, 0.62, 1.0]
    };
    let mut us: Vec<f32> = fine.iter().map(|f| f * re).collect();
    let cell = sc * t.cell;
    let inner = ((len - 2.0 * re) / (1.3 * cell)).ceil().max(1.0) as usize;
    for i in 1..inner {
        us.push(re + (len - 2.0 * re) * i as f32 / inner as f32);
    }
    let ends: Vec<f32> = us.iter().rev().map(|u| len - u).collect();
    us.extend(ends);
    // Round its section: (how far back from the front, height, normal),
    // the front at 0, the back at `depth`.
    let n_top = ((depth - rc) / (1.6 * cell)).ceil().max(1.0) as usize;
    let n_face = ((thick - 2.0 * rc) / (0.9 * cell)).ceil().max(1.0) as usize;
    let steps = if t.cell > 1.5 { 2 } else { 3 };
    let mut sec: Vec<(f32, f32, Vec2)> = Vec::new();
    for i in 0..n_top {
        let f = i as f32 / n_top as f32;
        sec.push((depth + (rc - depth) * f, s.y1, Vec2::Y));
    }
    let arc = |sec: &mut Vec<(f32, f32, Vec2)>, cy: f32, from: f32| {
        for i in 0..=steps {
            let th = from - i as f32 / steps as f32 * PI * 0.5;
            let n = Vec2::new(th.cos(), th.sin());
            sec.push((rc - rc * n.x, cy + rc * n.y, n));
        }
    };
    arc(&mut sec, s.y1 - rc, PI * 0.5);
    for i in 1..n_face {
        let f = i as f32 / n_face as f32;
        sec.push((0.0, s.y1 - rc - (thick - 2.0 * rc) * f, Vec2::X));
    }
    arc(&mut sec, s.y0 + rc, 0.0);
    for i in 1..=n_top {
        let f = i as f32 / n_top as f32;
        sec.push((rc + (depth - rc) * f, s.y0, -Vec2::Y));
    }
    let rows = sec.len();
    let mut pos = Vec::with_capacity(us.len() * rows);
    let mut want = Vec::with_capacity(us.len() * rows);
    let mut col = Vec::with_capacity(us.len() * rows);
    for &u in &us {
        let a = s.a0 + u / r_mid;
        // The ends round in: how much of its depth stands at this column.
        let e = (u.min(len - u) / re).min(1.0);
        let stand = (1.0 - (1.0 - e) * (1.0 - e)).sqrt();
        let side = if u < len * 0.5 { -1.0 } else { 1.0 };
        let dir = Vec3::new(a.cos(), 0.0, a.sin());
        let along = Vec3::new(-a.sin(), 0.0, a.cos());
        // Where the top's ground cuts it (in a notch the caprock is gone).
        let ceiling = t.lip_at(a) + 0.15;
        let y1 = s.y1.min(ceiling);
        let cut = smoothstep(0.15, 0.7, y1 - s.y0);
        // The face bows out a little across it.
        let bow = 0.05 * sc * (PI * u / len).sin();
        // Its top and foot are broken, not ruled: chipped down (and up)
        // along it.
        let chip = |k: u32| {
            let n = noise(Vec3::new(u / (1.6 * sc), 0.5, 0.5), s.key + k);
            (n * 1.4 - 0.2).max(0.0) * (0.3 * sc).min(thick * 0.18)
        };
        let (chip_top, chip_foot) = (chip(5), chip(6) * 0.6);
        let mid_y = s.y0 + thick * 0.5;
        for &(back, y, n2) in &sec {
            let y = if y > mid_y {
                y - (s.y1 - y1)
            } else {
                y.min(y1 - 0.02)
            };
            let y = if y > mid_y {
                y - chip_top * ((y - mid_y) / (thick * 0.5)).clamp(0.0, 1.0)
            } else {
                y + chip_foot * ((mid_y - y) / (thick * 0.5)).clamp(0.0, 1.0)
            };
            let h = ((y - s.y0) / thick).clamp(0.0, 1.0) - 0.5;
            let frac = 1.0 - back / depth;
            let out = s.back + (depth - back + (bow + s.lean * h) * frac) * stand * cut;
            let n = (dir * n2.x + Vec3::Y * n2.y) * stand + along * side * (1.0 - stand);
            let p = t.at(a, out, y);
            // Lumps: big ones over the whole slab, small ones on its faces.
            let k = s.key;
            let q = p / sc;
            let lump = 0.24 * noise(q * 0.2, k)
                + 0.14 * noise(q * 0.55, k + 2)
                + 0.07 * noise(q * 1.2, k + 1);
            // Less on its top and underside, which are bedding planes.
            let lump = lump * (1.0 - 0.6 * n2.y.abs());
            let p = p + n.normalize_or_zero() * lump * sc * stand.min(frac + 0.2);
            pos.push(p);
            want.push(n);
            let ground = t.ground_before(a, out);
            col.push(tint(p, n, ground, s.color, k));
        }
    }
    let mut tris = Vec::new();
    grid(&pos, &want, us.len(), rows, &mut tris);
    let nrm = normals(&pos, &tris);
    let mats = vec![Material::Cliff; pos.len()];
    push(m, &pos, &nrm, &col, &mats, &tris);
}

/// The core behind the slabs, dark in the cracks, with the top's sand over
/// it: a ring tucked under the top's ground, or a closed cap.
fn core(m: &mut MeshData, t: &Tower) {
    let sc = t.scale * t.cell;
    let circ = TAU * (t.rim)(0.0).max(3.0);
    let n = ((circ / (2.0 * sc)).ceil() as usize).max(24);
    let mut ys = Vec::new();
    let mut y = t.bottom();
    while y < t.top() {
        ys.push(y);
        y += 1.6 * sc;
    }
    let mut pos = Vec::new();
    let mut want = Vec::new();
    let mut col = Vec::new();
    let mut mat = Vec::new();
    for i in 0..=n {
        let a = i as f32 / n as f32 * TAU;
        let dir = Vec3::new(a.cos(), 0.0, a.sin());
        let lip = t.lip_at(a);
        let mut row = |p: Vec3, w: Vec3, c: Vec3, s: Material| {
            pos.push(p);
            want.push(w);
            col.push(c);
            mat.push(s);
        };
        for &y in &ys {
            let y = y.min(lip - 0.1);
            let p = t.at(a, t.core_at(a, y), y);
            let ground = t.ground_before(a, t.core_at(a, y));
            row(
                p,
                dir,
                tint(p, dir, ground, SHADES[1] * 0.55, 9),
                Material::Cliff,
            );
        }
        let top = lip + 0.05;
        row(
            t.at(a, t.core_at(a, top), top),
            Vec3::Y,
            SAND,
            Material::Sand,
        );
        // Back across the top, to the ground on it or to the middle.
        match t.ring_cap {
            Some(inner) => {
                let p = t.at(a, inner, 0.0);
                let p = Vec3::new(p.x, (t.lip)(p.xz()) + 0.03, p.z);
                row(p, Vec3::Y, SAND, Material::Sand);
            }
            None => {
                let c = t.center;
                row(
                    Vec3::new(c.x, top + 0.2, c.y),
                    Vec3::Y,
                    SAND,
                    Material::Sand,
                );
            }
        }
    }
    let rows = pos.len() / (n + 1);
    let mut tris = Vec::new();
    grid(&pos, &want, n + 1, rows, &mut tris);
    let mut nrm = normals(&pos, &tris);
    // The seam: the last column is the first again.
    for r in 0..rows {
        let sum = nrm[r] + nrm[n * rows + r];
        nrm[r] = sum;
        nrm[n * rows + r] = sum;
    }
    // Sand lies flat; the shading break at its edge is the rock's lip.
    for k in 0..pos.len() {
        if mat[k] == Material::Sand {
            nrm[k] = Vec3::Y;
        }
    }
    push(m, &pos, &nrm, &col, &mat, &tris);
}

/// A block of rock lying loose: a rounded box `half` across, lumpy, turned
/// by `rot`, at `at`.
pub fn block(
    m: &mut MeshData,
    at: Vec3,
    half: Vec3,
    rot: Quat,
    color: Vec3,
    floor: &dyn Fn(Vec2) -> f32,
    key: u32,
) {
    let k = 5;
    let r = (half.min_element() * 0.45).min(0.5);
    let core = half - Vec3::splat(r);
    let mut pos = Vec::new();
    let mut want = Vec::new();
    let mut tris = Vec::new();
    // Six faces of a cube, each a grid.
    let faces = [
        (Vec3::X, Vec3::Y, Vec3::Z),
        (-Vec3::X, Vec3::Z, Vec3::Y),
        (Vec3::Y, Vec3::Z, Vec3::X),
        (-Vec3::Y, Vec3::X, Vec3::Z),
        (Vec3::Z, Vec3::X, Vec3::Y),
        (-Vec3::Z, Vec3::Y, Vec3::X),
    ];
    for (nf, u, v) in faces {
        let base = pos.len();
        for i in 0..=k {
            for j in 0..=k {
                let c = nf
                    + u * (i as f32 / k as f32 * 2.0 - 1.0)
                    + v * (j as f32 / k as f32 * 2.0 - 1.0);
                let q = c * half;
                let inner = q.clamp(-core, core);
                let n = (q - inner).normalize_or(nf);
                let lp = inner + n * r;
                let lump = 0.12 * half.min_element() * noise(lp * 0.9, key)
                    + 0.04 * noise(lp * 2.5, key + 1);
                let n = rot * n;
                pos.push(at + rot * lp + n * lump);
                want.push(n);
            }
        }
        let mut local = Vec::new();
        grid(&pos[base..], &want[base..], k + 1, k + 1, &mut local);
        tris.extend(local.into_iter().map(|t| t.map(|i| i + base)));
    }
    // The faces' edges meet: share their normals.
    let mut nrm = normals(&pos, &tris);
    let mut shared: std::collections::HashMap<[i32; 3], Vec3> = Default::default();
    let key_of = |p: Vec3| (p * 1000.0).round().as_ivec3().to_array();
    let flat: Vec<Vec3> = pos.iter().map(|&p| p - at).collect();
    for (p, n) in flat.iter().zip(&nrm) {
        *shared.entry(key_of(*p)).or_default() += *n;
    }
    for (p, n) in flat.iter().zip(nrm.iter_mut()) {
        *n = shared[&key_of(*p)];
    }
    let col: Vec<Vec3> = pos
        .iter()
        .zip(&nrm)
        .map(|(&p, &n)| tint(p, n.normalize_or_zero(), floor(p.xz()), color, key))
        .collect();
    let mats = vec![Material::Cliff; pos.len()];
    push(m, &pos, &nrm, &col, &mats, &tris);
}
