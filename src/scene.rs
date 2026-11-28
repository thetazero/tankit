//! Turns simulation state into renderer input: the static map mesh, tank
//! instances and the chase camera. The HUD lives in `hud`.

use crate::render::material::Material;
use glam::{Mat4, Vec2, Vec3, Vec4};

use crate::map::rocks::{Dust, Rock};
use crate::map::{Hash, Map, Terrain, Wedge};
use crate::render::mesh::{MeshData, Vertex};
use crate::render::{Camera, Scene};
use crate::sim::{HullKind, Tank, TurretKind, World, apron_rects, dir2, wrap_angle};
use crate::tank::TankArt;

/// Static geometry for the whole arena, built once per map.
pub fn map_mesh(map: &Map) -> MeshData {
    let mut m = MeshData::default();
    let pad = 120.0;
    let asphalt = Vec4::new(0.17, 0.17, 0.18, Material::Asphalt.id());
    if let Some(st) = &map.scenery_terrain {
        terrain_mesh(st, &mut m);
    }
    match &map.terrain {
        Some(t) => {
            match &map.solid {
                Some(s) => surface_mesh(s, t, &mut m),
                None => terrain_mesh(t, &mut m),
            }
            let rects = if map.scenery_terrain.is_some() {
                Vec::new()
            } else {
                apron_rects(t.origin, t.origin + t.size(), map.size.max(pad)).to_vec()
            };
            for (min, max) in rects {
                m.add_box(
                    Vec3::new(min.x, -0.2, min.y),
                    Vec3::new(max.x, 0.0, max.y),
                    asphalt,
                    asphalt,
                );
            }
        }
        None => m.add_box(
            Vec3::new(-pad, -0.2, -pad),
            Vec3::new(map.size + pad, 0.0, map.size + pad),
            asphalt,
            asphalt,
        ),
    }
    for w in &map.wedges {
        let p = w.points();
        let center = p.iter().copied().sum::<Vec3>() / 6.0;
        for face in Wedge::FACES {
            let pts: Vec<Vec3> = face.iter().map(|&i| p[i]).collect();
            let mut n = (pts[1] - pts[0]).cross(pts[2] - pts[0]).normalize_or_zero();
            let face_center = pts.iter().copied().sum::<Vec3>() / pts.len() as f32;
            if n.dot(face_center - center) < 0.0 {
                n = -n;
            }
            // Slope faces get the paving pattern, everything else is plain.
            let material = w.material.unwrap_or(if n.y > 0.2 {
                Material::Paving
            } else {
                Material::Plain
            });
            let c = w.color.extend(material.id());
            if pts.len() == 4 {
                m.quad([pts[0], pts[1], pts[2], pts[3]], n, c);
            } else {
                m.tri([pts[0], pts[1], pts[2]], n, c);
            }
        }
    }

    // Boxes overlap all over the maps (wall runs, stacked panels, cornices)
    // with faces in one plane, and their colors differ, so they'd z-fight.
    // Each is drawn grown by its own fraction of a millimetre instead, never
    // the same as any box it touches: the depth buffer tells them apart at
    // any range, and nothing visibly moves.
    let oboxes: Vec<&crate::map::OBox> = map.oboxes.iter().filter(|o| !o.hidden).collect();
    let bounds: Vec<(Vec3, Vec3)> = map
        .boxes
        .iter()
        .map(|b| (b.min, b.max))
        .chain(oboxes.iter().map(|o| {
            // Loose: wide enough for any yaw, tilt or roll.
            let r = o.half.length() + (o.y1 - o.y0);
            let c = o.center3();
            (c - Vec3::splat(r), c + Vec3::splat(r))
        }))
        .collect();
    let mut grow = growths(&bounds).into_iter();
    for (b, g) in map.boxes.iter().zip(&mut grow) {
        m.add_box(
            b.min - g,
            b.max + g,
            b.color.extend(b.side_mat.id()),
            b.color.extend(b.top_mat.id()),
        );
    }
    let ground = |p: Vec2| map.ground_height(p.x, p.y);
    let dust = map.rock_dust.map(|color| Dust {
        color,
        ground: &ground,
    });
    for (o, g) in oboxes.into_iter().zip(&mut grow) {
        add_obox(o, g, dust, &mut m);
    }
    for c in &map.crystals {
        c.mesh(&mut m);
    }
    let base = m.verts.len() as u32;
    m.verts.extend_from_slice(&map.sculpted.verts);
    m.indices
        .extend(map.sculpted.indices.iter().map(|i| i + base));
    for g in &map.glow_paths {
        g.mesh(&mut m);
    }
    for r in map.rivers.iter() {
        r.mesh(&mut m);
    }

    repair_pads(map, &mut m);
    m
}

/// The steps map boxes are grown in (see `map_mesh`), m. The depth buffer
/// resolves far less than this even at the far plane.
const GROW_STEP: f32 = 0.0005;

/// How much to grow each box with these bounds so no two that touch grow
/// the same: a greedy coloring of the boxes by contact, each color a step.
fn growths(bounds: &[(Vec3, Vec3)]) -> Vec<f32> {
    const CELL: f32 = 8.0;
    let cells = |&(lo, hi): &(Vec3, Vec3)| {
        let (lo, hi) = (
            ((lo - 0.01) / CELL).floor().as_ivec3(),
            ((hi + 0.01) / CELL).floor().as_ivec3(),
        );
        (lo.x..=hi.x).flat_map(move |x| {
            (lo.y..=hi.y).flat_map(move |y| (lo.z..=hi.z).map(move |z| glam::IVec3::new(x, y, z)))
        })
    };
    let touch = |a: &(Vec3, Vec3), b: &(Vec3, Vec3)| {
        (a.0 - 0.01).cmple(b.1).all() && (b.0 - 0.01).cmple(a.1).all()
    };
    let mut grid: std::collections::HashMap<glam::IVec3, Vec<usize>> = Default::default();
    let mut level: Vec<u32> = Vec::with_capacity(bounds.len());
    let mut taken = Vec::new();
    for (i, b) in bounds.iter().enumerate() {
        taken.clear();
        for c in cells(b) {
            let here = grid.entry(c).or_default();
            taken.extend(
                here.iter()
                    .filter(|&&j| touch(b, &bounds[j]))
                    .map(|&j| level[j]),
            );
            here.push(i);
        }
        taken.sort_unstable();
        taken.dedup();
        let free = (0..).find(|l| taken.binary_search(l).is_err()).unwrap();
        level.push(free);
    }
    level
        .into_iter()
        .map(|l| GROW_STEP * (1 + l) as f32)
        .collect()
}

/// A box turned about +y: built as an axis-aligned box at the origin, grown
/// by `grow` (see `map_mesh`), then rotated into place.
fn add_obox(o: &crate::map::OBox, grow: f32, dust: Option<Dust>, m: &mut MeshData) {
    let mut local = MeshData::default();
    let h = Vec3::new(o.half.x, (o.y1 - o.y0) * 0.5, o.half.y);
    if let Some(rock) = Rock::of(o) {
        rock.mesh(&mut local, o.center3(), o.rotation(), dust, false);
    } else if o.chip > 0.0 || o.broken {
        worn_box(o, &mut local);
    } else {
        local.add_box(
            -h,
            h,
            o.color.extend(o.side_mat.id()),
            o.color.extend(o.top_mat.id()),
        );
    }
    let xf = Mat4::from_rotation_translation(o.rotation(), o.center3());
    let base = m.verts.len() as u32;
    for v in &local.verts {
        let mut v = *v;
        // Grow: each corner out by `grow` along every axis it's off center
        // on, so every face (chips too) moves out by at least `grow`.
        let p = Vec3::from(v.pos);
        let out = Vec3::select(p.abs().cmpgt(Vec3::splat(1e-5)), p.signum(), Vec3::ZERO);
        v.pos = xf.transform_point3(p + out * grow).to_array();
        v.normal = (o.rotation() * Vec3::from(v.normal)).to_array();
        m.verts.push(v);
    }
    m.indices.extend(local.indices.iter().map(|i| i + base));
}

/// A box that's been knocked about (box-local coordinates, centered): chips
/// bitten out of its vertical edges (some chamfered, some notched), corners of
/// the top knocked down, and if `broken`, a jagged top that steps and slopes
/// along its length. Chip faces and broken tops show raw masonry.
fn worn_box(o: &crate::map::OBox, m: &mut MeshData) {
    let mut rng = Hash::of(Vec3::new(o.center.x, o.y0 + o.y1, o.center.y));
    let (hx, hz) = (o.half.x, o.half.y);
    let hy = (o.y1 - o.y0) * 0.5;
    let face = o.color.extend(o.side_mat.id());
    let top_c = o.color.extend(o.top_mat.id());
    // Broken edges expose the core: rubble-textured, a darker masonry tone.
    let raw = (o.color * Vec3::new(0.78, 0.72, 0.66)).extend(Material::Rubble.id());
    let chip = o.chip.min(0.45 * hx.min(hz)).max(0.0);

    // Footprint ring, counter-clockwise, with per-point top drop and whether
    // the side face after this point is a chip face.
    let corners = [
        Vec2::new(-hx, -hz),
        Vec2::new(hx, -hz),
        Vec2::new(hx, hz),
        Vec2::new(-hx, hz),
    ];
    let mut ring: Vec<(Vec2, f32, bool)> = Vec::new();
    for i in 0..4 {
        let (prev, c, next) = (corners[(i + 3) % 4], corners[i], corners[(i + 1) % 4]);
        let (u, w) = ((prev - c).normalize(), (next - c).normalize());
        let roll = rng.f();
        if chip < 0.02 || roll < 0.25 {
            ring.push((c, 0.0, false));
        } else if roll < 0.7 {
            let a = chip * (0.35 + 0.65 * rng.f());
            let drop = a * rng.f();
            ring.push((c + u * a, drop * 0.5, true));
            ring.push((c + w * a, drop * 0.5, false));
        } else {
            let (d1, d2) = (chip * (0.5 + 0.7 * rng.f()), chip * (0.5 + 0.7 * rng.f()));
            let drop = chip * (0.5 + rng.f());
            ring.push((c + u * d1, drop * 0.3, true));
            ring.push((c + u * d1 + w * d2, drop, true));
            ring.push((c + w * d2, drop * 0.3, false));
        }
        // Points along the edge to the next corner, so a broken top can step.
        if o.broken {
            let len = c.distance(next);
            let n = (len / 0.45) as usize;
            for k in 1..n {
                ring.push((c + (next - c) * (k as f32 / n as f32), 0.0, false));
            }
        }
    }
    // Broken top: a smooth random profile with steps, never more than a
    // third of the height or 0.9 m.
    let depth = (hy * 2.0 * 0.33).min(0.9);
    if o.broken {
        let phase = rng.f() * 10.0;
        let n = ring.len();
        for (k, p) in ring.iter_mut().enumerate() {
            let t = k as f32 / n as f32 * std::f32::consts::TAU;
            let wave = 0.5 + 0.3 * (t * 2.0 + phase).sin() + 0.2 * (t * 5.0 + phase * 1.7).sin();
            let step = if rng.f() < 0.3 { 0.25 } else { 0.0 };
            p.1 = (p.1 + depth * (wave * 0.7 + step + 0.15 * rng.f())).min(depth);
        }
    }

    let inside = Vec3::ZERO;
    let n = ring.len();
    let bot = |p: Vec2| Vec3::new(p.x, -hy, p.y);
    let top = |p: Vec2, d: f32| Vec3::new(p.x, hy - d, p.y);
    for i in 0..n {
        let (a, da, chip_face) = ring[i];
        let (b, db, _) = ring[(i + 1) % n];
        let c = if chip_face { raw } else { face };
        m.facet([bot(a), bot(b), top(b, db)], inside, c);
        m.facet([bot(a), top(b, db), top(a, da)], inside, c);
    }
    // Top and bottom: the ring cut into triangles straight across the box,
    // not fanned from its center: on a long block a fan is a starburst of
    // slivers, each tipped its own way by the jagged edge (#443).
    let tc = if o.broken { raw } else { top_c };
    let footprint: Vec<Vec2> = ring.iter().map(|p| p.0).collect();
    for [i, j, k] in triangulate(&footprint) {
        let (a, b, c) = (ring[i], ring[j], ring[k]);
        m.facet(
            [top(a.0, a.1), top(b.0, b.1), top(c.0, c.1)],
            Vec3::new(0.0, -hy, 0.0),
            tc,
        );
        m.facet([bot(a.0), bot(b.0), bot(c.0)], Vec3::ZERO, face);
    }
}

/// A simple polygon cut into triangles (indices into `poly`), every cut as
/// short as it can be: ears clipped shortest diagonal first. On a long strip
/// that cuts straight across it, so no triangle runs the length of it.
fn triangulate(poly: &[Vec2]) -> Vec<[usize; 3]> {
    let cross = |o: Vec2, a: Vec2, b: Vec2| (a - o).perp_dot(b - o);
    // Counter-clockwise, so a convex corner turns left.
    let area: f32 = (0..poly.len())
        .map(|i| poly[i].perp_dot(poly[(i + 1) % poly.len()]))
        .sum();
    let turn = if area < 0.0 { -1.0 } else { 1.0 };
    let mut left: Vec<usize> = (0..poly.len()).collect();
    let mut tris = Vec::with_capacity(poly.len().saturating_sub(2));
    // The ear at `left[k]`, as the length of the cut that clips it, if it's one.
    let ear = |left: &[usize], k: usize| -> Option<f32> {
        let n = left.len();
        let (u, v, w) = (
            poly[left[(k + n - 1) % n]],
            poly[left[k]],
            poly[left[(k + 1) % n]],
        );
        let size = (u - v).abs().max((w - v).abs()).max_element().max(1e-3);
        if cross(u, v, w) * turn <= 1e-6 * size * size {
            return None;
        }
        // No other corner in it or on its edges.
        let eps = -1e-5 * size * size;
        let blocked = left.iter().any(|&q| {
            let p = poly[q];
            p != u
                && p != v
                && p != w
                && cross(u, v, p) * turn >= eps
                && cross(v, w, p) * turn >= eps
                && cross(w, u, p) * turn >= eps
        });
        (!blocked).then(|| u.distance(w))
    };
    while left.len() > 3 {
        let n = left.len();
        let best = (0..n)
            .filter_map(|k| ear(&left, k).map(|d| (k, d)))
            .min_by(|a, b| a.1.total_cmp(&b.1));
        // Numerically stuck (it never should be): fan what's left.
        let Some((k, _)) = best else {
            for k in 1..n - 1 {
                tris.push([left[0], left[k], left[k + 1]]);
            }
            return tris;
        };
        tris.push([left[(k + n - 1) % n], left[k], left[(k + 1) % n]]);
        left.remove(k);
    }
    if left.len() == 3 {
        tris.push([left[0], left[1], left[2]]);
    }
    tris
}

/// Drop zones where repair crates come down, glowing in the crate cross's
/// green: a dashed landing ring round a plus. Laid on the ground like a
/// decal, following the terrain, so nothing sinks into a slope.
fn repair_pads(map: &Map, m: &mut MeshData) {
    let ring = Vec3::new(0.12, 0.42, 0.2).extend(Material::Glow.id());
    let plus = Vec3::new(0.16, 0.55, 0.26).extend(Material::Glow.id());
    for &c in &map.repair_spots {
        let mut quad = |p: [Vec2; 4], col: Vec4| {
            for t in [[p[0], p[1], p[2]], [p[0], p[2], p[3]]] {
                decal_tri(map, m, t.map(|q| c + q), col);
            }
        };
        // Ring: dashes round the edge, where the crate's footprint will be ringed.
        let (r0, r1, dashes) = (3.0f32, 3.4f32, 16);
        let step = std::f32::consts::TAU / dashes as f32;
        for i in 0..dashes {
            let a0 = i as f32 * step;
            let at = |r: f32, a: f32| Vec2::new(a.cos(), a.sin()) * r;
            // Each dash is built from short pieces so it bends round the circle.
            let pieces = 3;
            let span = step * 0.6;
            for k in 0..pieces {
                let a = a0 + span * k as f32 / pieces as f32;
                let b = a0 + span * (k + 1) as f32 / pieces as f32;
                quad([at(r0, a), at(r0, b), at(r1, b), at(r1, a)], ring);
            }
        }
        // Plus, matching the cross on the crate.
        let (l, w) = (1.5f32, 0.4f32);
        quad(
            [
                Vec2::new(-l, -w),
                Vec2::new(l, -w),
                Vec2::new(l, w),
                Vec2::new(-l, w),
            ],
            plus,
        );
        for s in [-1.0f32, 1.0] {
            let (a, b) = (w * s, l * s);
            quad(
                [
                    Vec2::new(-w, a),
                    Vec2::new(w, a),
                    Vec2::new(w, b),
                    Vec2::new(-w, b),
                ],
                plus,
            );
        }
    }
}

/// A flat triangle (in xz) laid on the ground: split until its pieces are
/// small next to the terrain's bumps, each corner lifted just off the ground.
fn decal_tri(map: &Map, m: &mut MeshData, p: [Vec2; 3], c: Vec4) {
    const MAX_EDGE: f32 = 0.5;
    const LIFT: f32 = 0.05;
    let longest = (0..3)
        .max_by(|&i, &j| {
            let li = p[i].distance_squared(p[(i + 1) % 3]);
            let lj = p[j].distance_squared(p[(j + 1) % 3]);
            li.total_cmp(&lj)
        })
        .unwrap();
    let (a, b, o) = (p[longest], p[(longest + 1) % 3], p[(longest + 2) % 3]);
    if a.distance(b) > MAX_EDGE {
        let mid = (a + b) * 0.5;
        decal_tri(map, m, [a, mid, o], c);
        decal_tri(map, m, [mid, b, o], c);
        return;
    }
    let lay = |q: Vec2| Vec3::new(q.x, map.ground_height(q.x, q.y) + LIFT, q.y);
    m.facet(
        [lay(a), lay(b), lay(o)],
        lay((a + b + o) / 3.0) - Vec3::Y,
        c,
    );
}

/// How dark a bed of rock is at a point of it: beds a few meters thick,
/// warped a little as they run.
fn strata(v: Vec3) -> f32 {
    let y = v.y + 0.8 * (v.x * 0.05).sin() + 0.8 * (v.z * 0.043).cos();
    [1.0, 1.12, 0.84, 1.06, 0.92][(y / 2.6).floor().rem_euclid(5.0) as usize]
}

/// A solid's surface, colored as `t`'s ground is.
fn surface_mesh(s: &crate::map::solid::Surface, t: &Terrain, m: &mut MeshData) {
    let base = m.verts.len() as u32;
    for (v, n) in s.verts.iter().zip(&s.normals) {
        m.verts.push(Vertex {
            pos: v.to_array(),
            normal: n.to_array(),
            color: (t.color * t.tint_at(v.x, v.z))
                .extend(t.material.id())
                .to_array(),
        });
    }
    // Steep faces are rock, each with corners of its own (a material can't
    // blend across a face).
    for t in &s.tris {
        let v = t.map(|k| s.verts[k as usize]);
        let n = (v[1] - v[0]).cross(v[2] - v[0]).normalize_or(Vec3::Y);
        let mid = (v[0] + v[1] + v[2]) / 3.0;
        // What's dug out is rock all round.
        let under = s
            .top(Vec2::new(mid.x, mid.z))
            .is_some_and(|top| top > mid.y + 2.0);
        if n.y > 0.6 && !under {
            m.indices.extend(t.map(|k| base + k));
            continue;
        }
        let at = m.verts.len() as u32;
        for k in t {
            m.verts.push(Vertex {
                pos: s.verts[*k as usize].to_array(),
                normal: s.normals[*k as usize].to_array(),
                color: (s.rock * if under { 0.7 } else { 1.0 } * strata(s.verts[*k as usize]))
                    .extend(Material::Cliff.id())
                    .to_array(),
            });
        }
        m.indices.extend([at, at + 1, at + 2]);
    }
}

fn terrain_mesh(t: &Terrain, m: &mut MeshData) {
    let h = |ix: i64, iz: i64| {
        t.at(
            ix.clamp(0, t.nx as i64 - 1) as usize,
            iz.clamp(0, t.nz as i64 - 1) as usize,
        )
    };
    let base = m.verts.len() as u32;
    for iz in 0..t.nz as i64 {
        for ix in 0..t.nx as i64 {
            let p = Vec3::new(
                t.origin.x + ix as f32 * t.cell,
                h(ix, iz),
                t.origin.y + iz as f32 * t.cell,
            );
            let n = t.normal(ix as usize, iz as usize);
            m.verts.push(Vertex {
                pos: p.to_array(),
                normal: n.to_array(),
                color: (t.color
                    * t.tint
                        .get((iz * t.nx as i64 + ix) as usize)
                        .copied()
                        .unwrap_or(Vec3::ONE))
                .extend(t.material.id())
                .to_array(),
            });
        }
    }
    let nx = t.nx as u32;
    for iz in 0..t.nz as u32 - 1 {
        for ix in 0..nx - 1 {
            let (a, b) = (base + iz * nx + ix, base + iz * nx + ix + 1);
            let (c, d) = (a + nx, b + nx);
            m.indices.extend_from_slice(&[a, b, d, a, d, c]);
        }
    }
}

/// Third-person chase camera, Tanki-style: it sits behind the turret and
/// swings with it, follows with a little lag, and pulls in when something is
/// in the way. If pulling in would put it inside its own tank (backed up to
/// a wall, tipped, flipped), it climbs higher over the tank, or swings round
/// to a side that's open, and eases back once the way is clear.
pub struct ChaseCam {
    yaw: f32,
    /// 0 = low behind the tank, 1 = high overhead-ish.
    pub tilt: f32,
    focus: Vec3,
    initialized: bool,
    /// Recoil kick: an impulse each shot adds to (sized by the gun's recoil),
    /// decaying; `kick_shown` eases after it so rapid fire reads as a steady
    /// shove instead of the camera popping on every round.
    kick: f32,
    kick_shown: f32,
    /// The tank's shot counter at the last update, to spot new shots.
    shots_seen: usize,
    /// How far the boom is swung (yaw, rad) and raised (pitch, rad) off its
    /// usual place to keep out of the tank, as shown.
    dodge: Vec2,
}

/// Boom raises (rad above its usual pitch) and swings (rad off behind the
/// turret) the chase camera tries, in order, to keep out of its own tank.
const DODGE_RAISES: [f32; 7] = [0.0, 0.2, 0.4, 0.6, 0.8, 1.0, 1.25];
const DODGE_SWINGS: [f32; 9] = [0.0, 0.6, -0.6, 1.2, -1.2, 1.9, -1.9, 2.6, -2.6];
/// The chase camera keeps this far from the static world (and 0.6 m short
/// of a wall straight behind it): its near plane (0.2 m out, its corners
/// about 0.3 m) clear of it at any angle.
const CAM_BALL: f32 = 0.4;
/// The steepest the boom goes: not quite straight down at the tank.
const BOOM_STEEPEST: f32 = 1.4;
/// The chase camera's usual tilt, where it starts and where a reset eases it back.
pub const TILT_HOME: f32 = 0.35;

impl ChaseCam {
    pub fn new() -> ChaseCam {
        ChaseCam {
            yaw: 0.0,
            tilt: TILT_HOME,
            focus: Vec3::ZERO,
            initialized: false,
            kick: 0.0,
            kick_shown: 0.0,
            shots_seen: 0,
            dodge: Vec2::ZERO,
        }
    }

    pub fn update(&mut self, world: &World, tank: &Tank, dt: f32) -> Camera {
        let anchor = tank.pos + Vec3::Y * 2.2;
        let turret_yaw = tank.turret_yaw();
        if !self.initialized {
            self.focus = anchor;
            self.yaw = turret_yaw;
            self.shots_seen = tank.fire_barrel;
            self.initialized = true;
        }
        self.focus = self.focus.lerp(anchor, 1.0 - (-dt * 14.0).exp());
        self.yaw =
            wrap_angle(self.yaw + wrap_angle(turret_yaw - self.yaw) * (1.0 - (-dt * 10.0).exp()));

        let tilt = self.tilt.clamp(0.0, 1.0);
        let dist = 9.0 + tilt * 3.0;
        let height = 2.5 + tilt * 6.0;
        let pitch = height.atan2(dist);
        let len = dist.hypot(height);
        let (focus, yaw) = (self.focus, self.yaw);
        // The boom swung and raised by `dodge`, pulled in short of walls; and
        // how deep that puts the camera in its own tank (<= 0: clear).
        let place = |dodge: Vec2| {
            let back = dir2(yaw + dodge.x);
            let p = (pitch + dodge.y).min(BOOM_STEEPEST);
            let dir = Vec3::new(-back.x * p.cos(), p.sin(), -back.y * p.cos());
            // A ball, not a ray: a lump of rock beside the boom keeps out of
            // the near plane too, not only one straight along it.
            let ray = world.raycast(focus, dir, len) - 0.6;
            let ball = world.ballcast(focus, dir, CAM_BALL, len);
            let pos = focus + dir * ray.min(ball).max(0.0);
            (pos, depth_in_tank(tank, pos))
        };
        // The least dodge that keeps out of the tank (or, boxed in, the one
        // that gets least far into it).
        let mut want = (Vec2::ZERO, f32::MAX);
        'search: for swing in DODGE_SWINGS {
            for raise in DODGE_RAISES {
                let d = Vec2::new(swing, raise);
                let depth = place(d).1;
                if depth < want.1 {
                    want = (d, depth);
                }
                if depth <= 0.0 {
                    break 'search;
                }
            }
        }
        // Ease back toward the wanted dodge, unless easing leaves the camera
        // deeper in than it has to be: then jump straight there.
        let ease = 1.0 - (-dt * 4.0).exp();
        self.dodge = Vec2::new(
            wrap_angle(self.dodge.x + wrap_angle(want.0.x - self.dodge.x) * ease),
            self.dodge.y + (want.0.y - self.dodge.y) * ease,
        );
        let (mut pos, depth) = place(self.dodge);
        if depth > want.1.max(0.0) {
            self.dodge = want.0;
            pos = place(want.0).0;
        }

        let back = dir2(self.yaw);
        let ahead = Vec3::new(back.x, 0.0, back.y);
        // Firing kicks the camera back and up for a moment.
        let shots = tank.fire_barrel.saturating_sub(self.shots_seen).min(4);
        self.shots_seen = tank.fire_barrel;
        let per_shot = tank.turret_kind.recoil() / TurretKind::Kontos.recoil();
        self.kick = (self.kick * (-dt * 14.0).exp() + shots as f32 * per_shot).min(1.0);
        self.kick_shown += (self.kick - self.kick_shown) * (1.0 - (-dt * 40.0).exp());
        let kick = self.kick_shown;
        let kicked = pos - ahead * kick * 0.45 + Vec3::Y * kick * 0.2;
        // Swung round in front, the kick would shove the camera into the tank.
        if depth_in_tank(tank, kicked) <= 0.0 {
            pos = kicked;
        }
        Camera {
            pos,
            target: self.focus + ahead * 6.0 + Vec3::Y * (0.5 + kick * 0.3),
            fov_y: 65f32.to_radians(),
        }
    }
}

/// How much room the chase camera keeps from its own tank's art (m): the
/// near plane's corners (0.2 m out, ~0.25 m to the side) and a little more.
const CAM_CLEARANCE: f32 = 0.45;

/// An axis-aligned box in some frame of the tank's.
#[derive(Clone, Copy, Debug)]
struct Bulk {
    min: Vec3,
    max: Vec3,
}

impl Bulk {
    const EMPTY: Bulk = Bulk {
        min: Vec3::splat(f32::MAX),
        max: Vec3::splat(f32::MIN),
    };

    fn add(&mut self, p: Vec3) {
        self.min = self.min.min(p);
        self.max = self.max.max(p);
    }

    fn add_mesh(&mut self, m: &MeshData, at: Vec3) {
        for v in &m.verts {
            self.add(Vec3::from(v.pos) + at);
        }
    }

    /// How far `p` (in this box's frame) is inside it, padded by `pad`; zero
    /// or less outside.
    fn depth(&self, p: Vec3, pad: f32) -> f32 {
        let out = (self.min - pad - p).max(p - self.max - pad);
        -out.max_element()
    }
}

/// The boxes a tank's art fits in: the hull in the body frame, the turret
/// head in the turret frame, the mantlet and barrels in the gun frame.
struct KitBulk {
    hulls: Vec<(HullKind, Bulk)>,
    /// Same order as `TurretKind::ALL`: (head, gun).
    turrets: Vec<(Bulk, Bulk)>,
}

fn kit_bulk() -> &'static KitBulk {
    use crate::tank::{ithessar, model, resonant, uthenn};
    static BULK: std::sync::OnceLock<KitBulk> = std::sync::OnceLock::new();
    BULK.get_or_init(|| {
        let hull = |kind: HullKind, meshes: &[(&MeshData, Vec3)], points: &[Vec3]| {
            let mut b = Bulk::EMPTY;
            for p in crate::sim::Chassis::of(kind).collider_points() {
                b.add(p);
            }
            for &(m, at) in meshes {
                b.add_mesh(m, at);
            }
            for &p in points {
                b.add(p);
            }
            (kind, b)
        };
        let tracked = |kind: HullKind, h: &model::HullModel| {
            let r = &h.running;
            // Wheels and belt: the running gear's outline on both sides.
            let mut gear = Vec::new();
            for w in r.road.iter().chain([&r.sprocket, &r.idler]) {
                for x in [-r.track_x, r.track_x] {
                    let half = r.track_width * 0.5;
                    // Links stand off the wheels by about their thickness.
                    let rad = w.radius + 0.1;
                    gear.push(Vec3::new(x - half, w.center.y - rad, w.center.x - rad));
                    gear.push(Vec3::new(x + half, w.center.y + rad, w.center.x + rad));
                }
            }
            hull(kind, &[(&h.body, Vec3::ZERO)], &gear)
        };
        let hover = ithessar::sethuun();
        let shards: Vec<Vec3> = (hover.shards.iter())
            .flat_map(|&(at, len)| [at - Vec3::splat(len * 0.5), at + Vec3::splat(len * 0.5)])
            .map(|p| p + Vec3::Y * ithessar::RIDE)
            .collect();
        let vigil = resonant::vigil();
        let g = &vigil.extras;
        let mut extras = vec![
            (&g.light, Vec3::ZERO),
            (&g.heart, g.heart_at),
            (&g.bell, g.bell_pivot),
            (&vigil.hull.body, Vec3::ZERO),
        ];
        extras.extend(g.plates.iter().map(|(m, at)| (m, *at)));
        let mut hulls = vec![
            tracked(HullKind::Kataphrakt, &model::kataphrakt()),
            tracked(HullKind::Kastron, &model::kastron()),
            tracked(HullKind::Gravis, &uthenn::gravis()),
            hull(
                HullKind::Sethuun,
                &[
                    (&hover.body, Vec3::Y * ithessar::RIDE),
                    (&hover.glow, Vec3::Y * ithessar::RIDE),
                ],
                &shards,
            ),
        ];
        let (kind, mut b) = tracked(HullKind::Vigil, &vigil.hull);
        for (m, at) in extras {
            b.add_mesh(m, at);
        }
        hulls.push((kind, b));

        let turrets = TurretKind::ALL
            .iter()
            .map(|&k| {
                let m = crate::tank::turret_model(k);
                let mut head = Bulk::EMPTY;
                head.add_mesh(&m.head, Vec3::ZERO);
                let mut gun = Bulk::EMPTY;
                gun.add_mesh(&m.mantlet, Vec3::ZERO);
                for &o in &m.barrels {
                    gun.add_mesh(&m.barrel, o);
                }
                (head, gun)
            })
            .collect();
        KitBulk { hulls, turrets }
    })
}

/// How deep `p` is inside `tank`'s art padded by [`CAM_CLEARANCE`]: zero or
/// less when the camera is clear of it.
pub fn depth_in_tank(tank: &Tank, p: Vec3) -> f32 {
    let bulk = kit_bulk();
    let body = Mat4::from_rotation_translation(tank.rot, tank.pos);
    let local = |frame: Mat4| frame.inverse().transform_point3(p);
    // The hover hull leans on its cushion.
    let body = if tank.hull_kind == HullKind::Sethuun {
        body * Mat4::from_rotation_z(tank.lean)
    } else {
        body
    };
    let hull = (bulk.hulls.iter())
        .find(|(k, _)| *k == tank.hull_kind)
        .map_or(f32::MIN, |(_, b)| b.depth(local(body), CAM_CLEARANCE));
    let i = (TurretKind::ALL.iter())
        .position(|&k| k == tank.turret_kind)
        .unwrap_or(0);
    let (head, gun) = &bulk.turrets[i];
    let head = head.depth(local(tank.turret_frame()), CAM_CLEARANCE);
    let gun = gun.depth(local(tank.gun_frame()), CAM_CLEARANCE);
    hull.max(head).max(gun)
}

/// The 3D part of a frame. `tanks` is what to draw (usually `world.tanks`,
/// interpolated for display). `ui` starts empty; the caller adds the HUD.
pub fn build(world: &World, tanks: &[Tank], art: &TankArt, camera: Camera) -> Scene {
    let mut instances = Vec::new();
    let mut particles = Vec::new();
    // An empty seat has no tank to draw.
    for t in tanks.iter().filter(|t| !t.empty) {
        art.instances(t, &mut instances, &mut particles);
    }

    Scene {
        camera,
        time: world.time,
        sun_dir: world.map.atmosphere.sun_dir,
        atmosphere: world.map.atmosphere,
        shadow_center: Vec3::new(world.map.size * 0.5, 0.0, world.map.size * 0.5),
        shadow_radius: world.map.size * 0.75,
        instances,
        particles,
        lights: world.map.lights.clone(),
        rivers: world.map.rivers.clone(),
        ui: Vec::new(),
        world: true,
    }
}

/// World point to screen pixels, or `None` if it's behind the camera.
pub fn project(vp: Mat4, p: Vec3, w: f32, h: f32) -> Option<(f32, f32)> {
    let c = vp * p.extend(1.0);
    if c.w <= 0.01 {
        return None;
    }
    let ndc = c.truncate() / c.w;
    Some(((ndc.x * 0.5 + 0.5) * w, (0.5 - ndc.y * 0.5) * h))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::zfight::{Fight, Soup};

    /// Where each map's static geometry has coplanar faces that show.
    /// As `map_fights`, but only of what's round the generator in the middle
    /// of a city: the rest is `maps_hardly_z_fight`'s city, block for block.
    fn generator_fights(name: &str, map: &Map) -> Vec<Fight> {
        let at = map.skeleton.as_ref().unwrap().landmarks[0].center;
        let mut mesh = map_mesh(map);
        let near = |i: &u32| {
            let p = mesh.verts[*i as usize].pos;
            Vec2::new(p[0], p[2]).distance(at) < 60.0
        };
        let kept = mesh.indices.chunks_exact(3).filter(|t| t.iter().any(near));
        let kept = kept.flatten().copied().collect();
        mesh.indices = kept;
        mesh_fights(name, map, &mesh)
    }

    fn map_fights(name: &str, map: &Map) -> Vec<Fight> {
        mesh_fights(name, map, &map_mesh(map))
    }

    fn mesh_fights(name: &str, map: &Map, mesh: &MeshData) -> Vec<Fight> {
        let ground = map.clone();
        let mut soup = Soup::map(move |x, z| ground.ground_height(x, z));
        soup.add(name, mesh, Mat4::IDENTITY, Vec4::new(1.0, 1.0, 1.0, 0.0));
        soup.fights()
    }

    /// No map z-fights beyond a few slivers: chip cuts on stacked worn
    /// blocks that land, by chance, within a hair of each other. Takes a few
    /// seconds optimized, minutes not, so it runs under `--release` only.
    #[test]
    #[cfg_attr(
        debug_assertions,
        ignore = "slow unoptimized: run with cargo test --release"
    )]
    fn maps_hardly_z_fight() {
        hardly_z_fight([
            ("dam town", Map::reservoir(0)),
            ("well town", Map::reservoir(1)),
            ("sanctum", Map::sanctum()),
            ("city", Map::city(0)),
            ("mine", Map::mine(0)),
        ]);
    }

    /// The same for generated Sanctums, one of each ground.
    #[test]
    #[cfg_attr(
        debug_assertions,
        ignore = "slow unoptimized: run with cargo test --release"
    )]
    fn sanctums_hardly_z_fight() {
        use crate::map::procgen::foundry::Ground;
        hardly_z_fight([
            ("sanctum desert", Map::sanctum_on(Ground::Desert, 1)),
            ("sanctum tundra", Map::sanctum_on(Ground::Tundra, 2)),
            ("sanctum grassland", Map::sanctum_on(Ground::Grassland, 3)),
        ]);
    }

    /// An Ithessar city, on its own: they're the biggest.
    #[test]
    #[cfg_attr(
        debug_assertions,
        ignore = "slow unoptimized: run with cargo test --release"
    )]
    fn an_ithessar_city_hardly_z_fights() {
        use crate::map::procgen::Look;
        let kind = crate::MapKind::City(Look::Ithessar, Default::default());
        hardly_z_fight([("ithessar city", crate::make_map(kind, 0))]);
    }

    /// The same for other sides' cities, and each side's generator in one
    /// (a test of their own: the suite runs the two side by side).
    #[test]
    #[cfg_attr(
        debug_assertions,
        ignore = "slow unoptimized: run with cargo test --release"
    )]
    fn cities_hardly_z_fight() {
        use crate::faction::Faction;
        hardly_z_fight([(
            "resonant city",
            crate::make_map(
                crate::MapKind::City(crate::map::procgen::Look::Resonant, Default::default()),
                0,
            ),
        )]);
        generators_hardly_z_fight([
            (
                "shield-site imperium",
                Map::shield_city(0, Faction::Imperium),
            ),
            (
                "shield-site resonant",
                Map::shield_city(0, Faction::Resonant),
            ),
            ("shield-site uthenn", Map::shield_city(0, Faction::Uthenn)),
            (
                "shield-site ithessar",
                Map::shield_city(0, Faction::Ithessar),
            ),
        ]);
    }

    /// The same for cities on tundra and grassland.
    #[test]
    #[cfg_attr(
        debug_assertions,
        ignore = "slow unoptimized: run with cargo test --release"
    )]
    fn snowy_and_green_cities_hardly_z_fight() {
        use crate::map::procgen::{Look, setting::Setting};
        let city = |seed, l, s| crate::make_map(crate::MapKind::City(l, s), seed);
        hardly_z_fight([
            ("city tundra", city(0, Look::Imperium, Setting::Tundra)),
            (
                "city grassland",
                city(1, Look::Resonant, Setting::Grassland),
            ),
        ]);
    }

    /// The same for an Ithessar city in the snow.
    #[test]
    #[cfg_attr(
        debug_assertions,
        ignore = "slow unoptimized: run with cargo test --release"
    )]
    fn a_snowy_ithessar_city_hardly_z_fights() {
        use crate::map::procgen::{Look, setting::Setting};
        let kind = crate::MapKind::City(Look::Ithessar, Setting::Tundra);
        hardly_z_fight([("ithessar city tundra", crate::make_map(kind, 0))]);
    }

    /// The same for shield sites on tundra and grassland, in cities and out
    /// in the open.
    #[test]
    #[cfg_attr(
        debug_assertions,
        ignore = "slow unoptimized: run with cargo test --release"
    )]
    fn snowy_and_green_shield_sites_hardly_z_fight() {
        use crate::MapKind::{ShieldCamp, ShieldSite};
        use crate::faction::Faction;
        use crate::map::procgen::{Look, foundry::Ground, setting::Setting};
        let site = |seed, l, s, f| crate::make_map(ShieldSite(l, s, crate::Generator::up(f)), seed);
        let camp = |seed, g, f| crate::make_map(ShieldCamp(g, crate::Generator::up(f)), seed);
        hardly_z_fight([
            (
                "shield-site tundra",
                site(0, Look::Imperium, Setting::Tundra, Faction::Uthenn),
            ),
            (
                "shield-site grassland",
                site(1, Look::Resonant, Setting::Grassland, Faction::Imperium),
            ),
            (
                "shield-camp tundra imperium",
                camp(0, Ground::Tundra, Faction::Imperium),
            ),
            (
                "shield-camp tundra resonant",
                camp(1, Ground::Tundra, Faction::Resonant),
            ),
            (
                "shield-camp tundra uthenn",
                camp(2, Ground::Tundra, Faction::Uthenn),
            ),
            (
                "shield-camp tundra ithessar",
                camp(7, Ground::Tundra, Faction::Ithessar),
            ),
            (
                "shield-camp grassland",
                camp(2, Ground::Grassland, Faction::Resonant),
            ),
        ]);
    }

    /// The same for each faction's hull foundry.
    #[test]
    #[cfg_attr(
        debug_assertions,
        ignore = "slow unoptimized: run with cargo test --release"
    )]
    fn foundries_hardly_z_fight() {
        use crate::map::procgen::{Look, foundry::Ground};
        let foundry = |seed, l, g| crate::make_map(crate::MapKind::Foundry(l, g), seed);
        hardly_z_fight([
            (
                "foundry imperium",
                foundry(0, Look::Imperium, Ground::Desert),
            ),
            (
                "foundry resonant",
                foundry(1, Look::Resonant, Ground::Tundra),
            ),
            (
                "foundry uthenn",
                foundry(2, Look::Uthenn, Ground::Grassland),
            ),
            (
                "foundry ithessar",
                foundry(3, Look::Ithessar, Ground::Desert),
            ),
        ]);
    }

    /// The same for rail depots.
    #[test]
    #[cfg_attr(
        debug_assertions,
        ignore = "slow unoptimized: run with cargo test --release"
    )]
    fn depots_hardly_z_fight() {
        use crate::map::procgen::{Look, foundry::Ground};
        let depot = |seed, l, g| crate::make_map(crate::MapKind::Depot(l, g), seed);
        hardly_z_fight([
            ("depot imperium", depot(0, Look::Imperium, Ground::Desert)),
            ("depot resonant", depot(1, Look::Resonant, Ground::Tundra)),
            ("depot uthenn", depot(2, Look::Uthenn, Ground::Grassland)),
            ("depot ithessar", depot(3, Look::Ithessar, Ground::Tundra)),
        ]);
    }

    /// The same for the other factions' crystal mines.
    #[test]
    #[cfg_attr(
        debug_assertions,
        ignore = "slow unoptimized: run with cargo test --release"
    )]
    fn mines_hardly_z_fight() {
        use crate::map::procgen::{Look, foundry::Ground};
        let mine = |l, g| crate::make_map(crate::MapKind::Mine(l, g), 0);
        hardly_z_fight([
            ("mine imperium", mine(Look::Imperium, Ground::Desert)),
            ("mine resonant", mine(Look::Resonant, Ground::Tundra)),
            ("mine uthenn", mine(Look::Uthenn, Ground::Grassland)),
        ]);
    }

    /// The same for the open desert, whose big rocks once folded over
    /// themselves (#701): an underside in seed 3, a top in seed 5.
    #[test]
    #[cfg_attr(
        debug_assertions,
        ignore = "slow unoptimized: run with cargo test --release"
    )]
    fn open_deserts_hardly_z_fight() {
        use crate::faction::Faction;
        use crate::map::procgen::foundry::Ground;
        let camp = |seed| {
            let made = crate::Generator::up(Faction::Imperium);
            crate::make_map(crate::MapKind::ShieldCamp(Ground::Desert, made), seed)
        };
        hardly_z_fight([
            ("desert", Map::desert(3)),
            ("desert 9", Map::desert(9)),
            ("shield-camp desert 3", camp(3)),
            ("shield-camp desert 5", camp(5)),
        ]);
    }

    fn hardly_z_fight<const N: usize>(maps: [(&str, Map); N]) {
        for (name, map) in maps {
            assert_hardly(name, map_fights(name, &map));
        }
    }

    fn generators_hardly_z_fight<const N: usize>(maps: [(&str, Map); N]) {
        for (name, map) in maps {
            assert_hardly(name, generator_fights(name, &map));
        }
    }

    fn assert_hardly(name: &str, fights: Vec<Fight>) {
        let area: f32 = fights.iter().map(|f| f.area).sum();
        let report: Vec<String> = fights
            .iter()
            .take(10)
            .map(|f| format!("{:.3} m² at {:.2} (faces {:.2?})", f.area, f.at, f.faces))
            .collect();
        assert!(
            area < 0.4 && fights.iter().all(|f| f.area < 0.1),
            "{name}: {} z-fights, {area:.2} m² in all; the biggest:\n{}",
            fights.len(),
            report.join("\n")
        );
    }

    /// Each side's generator down (#341): wrecked, or halfway back up (one
    /// of each per side would double the optimized suite's slowest test).
    #[test]
    #[cfg_attr(
        debug_assertions,
        ignore = "slow unoptimized: run with cargo test --release"
    )]
    fn downed_generators_hardly_z_fight() {
        use crate::map::procgen::shield::Condition;
        for (k, owner) in crate::faction::Faction::ALL.into_iter().enumerate() {
            let done = if k % 2 == 0 { 0 } else { 2 };
            let name = format!("shield-site {owner:?} {done}/4 repaired");
            let map = Map::shield_city_in(0, owner, Condition::Down { done, of: 4 });
            assert_hardly(&name, generator_fights(&name, &map));
        }
    }

    /// A worn or broken box's top, from above: its triangles (xz) and the
    /// footprint ring they should tile.
    fn worn_top(o: &crate::map::OBox) -> Vec<[Vec2; 3]> {
        let mut m = MeshData::default();
        worn_box(o, &mut m);
        let hy = (o.y1 - o.y0) * 0.5;
        m.indices
            .chunks(3)
            .map(|t| [0, 1, 2].map(|k| Vec3::from(m.verts[t[k] as usize].pos)))
            // Every side and bottom triangle has a corner on the base.
            .filter(|t| t.iter().all(|p| p.y > -hy * 0.99))
            .map(|t| t.map(|p| Vec2::new(p.x, p.z)))
            .collect()
    }

    fn worn_boxes() -> Vec<crate::map::OBox> {
        let mut out = Vec::new();
        for k in 0..200 {
            let f = |s: u32| Hash::of(Vec3::new(k as f32, s as f32, 7.0)).f();
            out.push(crate::map::OBox {
                center: Vec2::new(k as f32 * 13.0, f(1) * 50.0),
                half: Vec2::new(0.2 + f(2) * 6.0, 0.2 + f(3) * 1.5),
                y0: 0.0,
                y1: 0.5 + f(4) * 4.0,
                chip: f(5) * 0.4,
                broken: f(6) < 0.7,
                ..Default::default()
            });
        }
        out
    }

    /// The top tiles the footprint: no triangle flipped, none overlapping,
    /// nothing left uncovered (#443).
    #[test]
    fn worn_tops_tile_their_footprint() {
        for o in worn_boxes() {
            let tris = worn_top(&o);
            let signed = |t: &[Vec2; 3]| (t[1] - t[0]).perp_dot(t[2] - t[0]) * 0.5;
            let pos: f32 = tris.iter().map(signed).filter(|a| *a > 0.0).sum();
            let neg: f32 = -tris.iter().map(signed).filter(|a| *a < 0.0).sum::<f32>();
            // All one way round (the other way would be a fold), and their area
            // what the box's minus its chips: at most a few chips' worth less.
            let (one, other) = (pos.max(neg), pos.min(neg));
            let full = o.half.x * o.half.y * 4.0;
            assert!(other < 1e-4 * full, "{o:?}: folded, {other} against {one}");
            assert!(
                one <= full * 1.0001 && one >= full - 4.0 * o.chip * o.chip * 1.5 - 1e-3,
                "{o:?}: covers {one} of {full}"
            );
        }
    }

    /// On a long broken block no top triangle runs along it: a fan from the
    /// middle made a starburst of slivers there (#443).
    #[test]
    fn long_broken_tops_have_no_slivers() {
        for o in worn_boxes() {
            let (long, short) = (o.half.max_element() * 2.0, o.half.min_element() * 2.0);
            if !o.broken || long < short * 3.0 {
                continue;
            }
            for t in worn_top(&o) {
                let edge = (0..3)
                    .map(|i| t[i].distance(t[(i + 1) % 3]))
                    .fold(0.0, f32::max);
                assert!(
                    edge < short + 1.0,
                    "{o:?}: a {edge:.2} m edge on a {short:.2} m wide top"
                );
            }
        }
    }

    /// Map boxes that touch grow by different amounts.
    #[test]
    fn touching_boxes_grow_apart() {
        let b = |x: f32| (Vec3::new(x, 0.0, 0.0), Vec3::new(x + 1.0, 1.0, 1.0));
        // A row of overlapping boxes, and one far off.
        let g = growths(&[b(0.0), b(0.5), b(0.9), b(1.4), b(50.0)]);
        for w in g[..4].windows(2) {
            assert!((w[0] - w[1]).abs() >= GROW_STEP * 0.99, "{g:?}");
        }
        assert!((g[0] - g[2]).abs() >= GROW_STEP * 0.99, "{g:?}");
        assert_eq!(g[4], GROW_STEP, "a box touching nothing grows the least");
    }

    /// Backed up to a wall, tipped, flipped or under a low roof, whatever
    /// the hull and turret, the chase camera never ends up in its own tank.
    #[test]
    fn chase_cam_keeps_out_of_its_own_tank() {
        use crate::sim::tests_support::ledge_world;
        use glam::Quat;
        use rapier3d::prelude::ColliderBuilder;
        use std::f32::consts::{FRAC_PI_2, PI};

        // A 6 m block whose near face is at z = 110; and the same with a cave
        // roof 5.5 m up over the ground in front of it.
        let open = ledge_world(6.0);
        let mut cave = ledge_world(6.0);
        cave.physics.insert_collider(
            ColliderBuilder::cuboid(15.0, 0.5, 10.0).translation(Vec3::new(100.0, 6.0, 100.0)),
            None,
        );
        cave.physics.step();
        let hulls = [
            HullKind::Kataphrakt,
            HullKind::Kastron,
            HullKind::Sethuun,
            HullKind::Vigil,
            HullKind::Gravis,
        ];
        let mut failures = Vec::new();
        for (place, mut world) in [("open", open), ("cave", cave)] {
            for hull in hulls {
                world.set_hull(0, hull);
                for turret in [TurretKind::Kontos, TurretKind::Knell, TurretKind::Lumen] {
                    let mut tank = world.tanks[0].clone();
                    tank.turret_kind = turret;
                    tank.turret_rel = 0.0;
                    // Facing away from the block, so the boom points into it.
                    {
                        for gap in [0.2, 1.0, 2.5, 4.0] {
                            for (name, lean) in [
                                ("level", Quat::IDENTITY),
                                ("nose up", Quat::from_rotation_x(-0.6)),
                                ("tipped", Quat::from_rotation_z(1.1)),
                                ("on its side", Quat::from_rotation_z(FRAC_PI_2)),
                                ("flipped", Quat::from_rotation_z(PI)),
                            ] {
                                tank.rot = Quat::from_rotation_y(PI) * lean;
                                let lift = if name == "flipped" { 3.0 } else { 1.5 };
                                tank.pos = Vec3::new(100.0, lift, 110.0 - 3.6 - gap);
                                let mut cam = ChaseCam::new();
                                for tilt in [0.0, 0.35, 1.0] {
                                    cam.tilt = tilt;
                                    for _ in 0..30 {
                                        let c = cam.update(&world, &tank, 1.0 / 60.0);
                                        let depth = depth_in_tank(&tank, c.pos);
                                        if depth > 0.0 {
                                            failures.push(format!(
                                            "{place}: {hull:?}/{turret:?} {name} {gap} m off the \
                                             wall, tilt {tilt}: {depth:.2} m in"
                                        ));
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    /// With nothing in the way the camera sits where it always has.
    #[test]
    fn chase_cam_in_the_open_is_unmoved() {
        let world = crate::sim::tests_support::flat_world();
        let tank = &world.tanks[0];
        let mut cam = ChaseCam::new();
        let c = cam.update(&world, tank, 1.0 / 60.0);
        // The usual boom, 0.6 m short of its full length (the wall margin).
        let back = dir2(tank.turret_yaw());
        let boom =
            Vec3::new(-back.x, 0.0, -back.y) * (9.0 + 0.35 * 3.0) + Vec3::Y * (2.5 + 0.35 * 6.0);
        let want = tank.pos + Vec3::Y * 2.2 + boom.normalize() * (boom.length() - 0.6);
        assert!(c.pos.distance(want) < 1e-3, "{} vs {want}", c.pos);
    }
}
