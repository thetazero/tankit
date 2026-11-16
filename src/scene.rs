//! Turns simulation state into renderer input: the static map mesh, tank
//! instances and the chase camera. The HUD lives in `hud`.

use crate::render::material::Material;
use glam::{Mat4, Vec2, Vec3, Vec4};

use crate::map::{Map, Terrain, Wedge};
use crate::render::mesh::{MeshData, Vertex};
use crate::render::{Camera, Scene};
use crate::sim::{Tank, TurretKind, World, apron_rects, dir2, wrap_angle};
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
            terrain_mesh(t, &mut m);
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

    for b in &map.boxes {
        m.add_box(
            b.min,
            b.max,
            b.color.extend(b.side_mat.id()),
            b.color.extend(b.top_mat.id()),
        );
    }
    for o in &map.oboxes {
        add_obox(o, &mut m);
    }
    for c in &map.crystals {
        c.mesh(&mut m);
    }
    for g in &map.glow_paths {
        g.mesh(&mut m);
    }

    repair_pads(map, &mut m);
    m
}

/// A box turned about +y: built as an axis-aligned box at the origin, then rotated into place.
fn add_obox(o: &crate::map::OBox, m: &mut MeshData) {
    let mut local = MeshData::default();
    let h = Vec3::new(o.half.x, (o.y1 - o.y0) * 0.5, o.half.y);
    if o.chip > 0.0 || o.broken {
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
        v.pos = xf.transform_point3(Vec3::from(v.pos)).to_array();
        v.normal = xf.transform_vector3(Vec3::from(v.normal)).to_array();
        m.verts.push(v);
    }
    m.indices.extend(local.indices.iter().map(|i| i + base));
}

/// Cheap deterministic random stream for visual-only detail.
struct Hash(u32);

impl Hash {
    fn of(p: Vec3) -> Hash {
        let mut h = (p.x * 73.1).to_bits()
            ^ (p.y * 19.7).to_bits().rotate_left(11)
            ^ (p.z * 41.3).to_bits().rotate_left(22);
        h ^= h >> 15;
        Hash(h.wrapping_mul(0x2c1b_3c6d) | 1)
    }
    fn f(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 >> 8) as f32 / 16_777_216.0
    }
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
    // Rocks (heavily chipped rubble): an irregular outline, tapering to a
    // lumpy top, instead of a chipped rectangle.
    if o.chip >= 0.6 && o.side_mat == Material::Rubble {
        let n = 7 + (rng.f() * 3.0) as usize;
        let phase = rng.f() * 6.28;
        let ring: Vec<(Vec2, f32)> = (0..n)
            .map(|k| {
                let a = k as f32 / n as f32 * std::f32::consts::TAU + phase + (rng.f() - 0.5) * 0.4;
                let rad = 0.75 + 0.25 * rng.f();
                (Vec2::new(a.cos() * hx, a.sin() * hz) * rad, rng.f())
            })
            .collect();
        let taper = 0.55 + 0.2 * rng.f();
        let bot = |p: Vec2| Vec3::new(p.x, -hy, p.y);
        let top = |p: Vec2, d: f32| Vec3::new(p.x * taper, hy - d * hy * 0.5, p.y * taper);
        let peak = Vec3::new((rng.f() - 0.5) * hx * 0.3, hy, (rng.f() - 0.5) * hz * 0.3);
        for i in 0..n {
            let (a, da) = ring[i];
            let (b, db) = ring[(i + 1) % n];
            m.facet([bot(a), bot(b), top(b, db)], Vec3::ZERO, face);
            m.facet([bot(a), top(b, db), top(a, da)], Vec3::ZERO, face);
            m.facet(
                [peak, top(a, da), top(b, db)],
                Vec3::new(0.0, -hy, 0.0),
                top_c,
            );
        }
        return;
    }
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
    // Top: a fan from a center point at the mean height.
    let mean = ring.iter().map(|p| p.1).sum::<f32>() / n as f32;
    let center_top = Vec3::new(0.0, hy - mean * if o.broken { 0.8 } else { 1.0 }, 0.0);
    let tc = if o.broken { raw } else { top_c };
    for i in 0..n {
        let (a, da, _) = ring[i];
        let (b, db, _) = ring[(i + 1) % n];
        m.facet(
            [center_top, top(a, da), top(b, db)],
            Vec3::new(0.0, -hy, 0.0),
            tc,
        );
        m.facet([Vec3::new(0.0, -hy, 0.0), bot(b), bot(a)], Vec3::ZERO, face);
    }
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
            let n = Vec3::new(
                h(ix - 1, iz) - h(ix + 1, iz),
                2.0 * t.cell,
                h(ix, iz - 1) - h(ix, iz + 1),
            )
            .normalize();
            m.verts.push(Vertex {
                pos: p.to_array(),
                normal: n.to_array(),
                color: t.color.extend(t.material.id()).to_array(),
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
/// in the way.
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
}

impl ChaseCam {
    pub fn new() -> ChaseCam {
        ChaseCam {
            yaw: 0.0,
            tilt: 0.35,
            focus: Vec3::ZERO,
            initialized: false,
            kick: 0.0,
            kick_shown: 0.0,
            shots_seen: 0,
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
        let back = dir2(self.yaw);
        let back3 = Vec3::new(-back.x, 0.0, -back.y);
        let desired = self.focus + back3 * dist + Vec3::Y * height;

        let to = desired - self.focus;
        let len = to.length();
        let hit = world.raycast(self.focus, to / len, len);
        let pos = self.focus + to / len * (hit - 0.6).max(1.5);

        let ahead = Vec3::new(back.x, 0.0, back.y);
        // Firing kicks the camera back and up for a moment.
        let shots = tank.fire_barrel.saturating_sub(self.shots_seen).min(4);
        self.shots_seen = tank.fire_barrel;
        let per_shot = tank.turret_kind.recoil() / TurretKind::Kontos.recoil();
        self.kick = (self.kick * (-dt * 14.0).exp() + shots as f32 * per_shot).min(1.0);
        self.kick_shown += (self.kick - self.kick_shown) * (1.0 - (-dt * 40.0).exp());
        let kick = self.kick_shown;
        let pos = pos - ahead * kick * 0.45 + Vec3::Y * kick * 0.2;
        Camera {
            pos,
            target: self.focus + ahead * 6.0 + Vec3::Y * (0.5 + kick * 0.3),
            fov_y: 65f32.to_radians(),
        }
    }
}

/// The 3D part of a frame. `tanks` is what to draw (usually `world.tanks`,
/// interpolated for display). `ui` starts empty; the caller adds the HUD.
pub fn build(world: &World, tanks: &[Tank], art: &TankArt, camera: Camera) -> Scene {
    let mut instances = Vec::new();
    let mut particles = Vec::new();
    for t in tanks {
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
