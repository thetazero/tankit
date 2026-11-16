//! Turns simulation state into renderer input: the static map mesh, tank
//! instances and the chase camera. The HUD lives in `hud`.

use glam::{Mat4, Quat, Vec2, Vec3, Vec4};

use crate::map::{Map, Terrain, Wedge, mat};
use crate::render::mesh::{MeshData, Vertex};
use crate::render::{Camera, Scene};
use crate::sim::{Tank, World, apron_rects, dir2, wrap_angle};
use crate::tank::TankArt;

pub const SUN_DIR: Vec3 = Vec3::new(0.55, 0.48, 0.4);

/// Static geometry for the whole arena, built once per map.
pub fn map_mesh(map: &Map) -> MeshData {
    let mut m = MeshData::default();
    let pad = 120.0;
    let asphalt = Vec4::new(0.17, 0.17, 0.18, mat::ASPHALT);
    match &map.terrain {
        Some(t) => {
            terrain_mesh(t, &mut m);
            for (min, max) in apron_rects(t.origin, t.origin + t.size(), map.size.max(pad)) {
                m.add_box(Vec3::new(min.x, -0.2, min.y), Vec3::new(max.x, 0.0, max.y), asphalt, asphalt);
            }
        }
        None => m.add_box(Vec3::new(-pad, -0.2, -pad), Vec3::new(map.size + pad, 0.0, map.size + pad), asphalt, asphalt),
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
            let material = if n.y > 0.2 { mat::PAVING } else { mat::PLAIN };
            let c = w.color.extend(material);
            if pts.len() == 4 {
                m.quad([pts[0], pts[1], pts[2], pts[3]], n, c);
            } else {
                m.tri([pts[0], pts[1], pts[2]], n, c);
            }
        }
    }

    for b in &map.boxes {
        m.add_box(b.min, b.max, b.color.extend(b.side_mat), b.color.extend(b.top_mat));
    }
    for o in &map.oboxes {
        add_obox(o, &mut m);
    }

    let sphere = MeshData::sphere(12, 8);
    for t in &map.trees {
        let s = Vec3::new(t.radius * 2.0, t.radius * 1.7, t.radius * 2.0);
        m.append(&sphere, Mat4::from_scale_rotation_translation(s, Quat::IDENTITY, t.pos), Vec4::new(0.27, 0.45, 0.2, mat::PLAIN));
    }

    road_markings(map, &mut m);
    repair_pads(map, &mut m);
    m
}

/// A box turned about +y: built as an axis-aligned box at the origin, then rotated into place.
fn add_obox(o: &crate::map::OBox, m: &mut MeshData) {
    let mut local = MeshData::default();
    let h = Vec3::new(o.half.x, (o.y1 - o.y0) * 0.5, o.half.y);
    local.add_box(-h, h, o.color.extend(o.side_mat), o.color.extend(o.top_mat));
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

/// Painted drop zones where repair crates come down: a square outline with a cross.
fn repair_pads(map: &Map, m: &mut MeshData) {
    let paint = Vec4::new(0.5, 0.78, 0.5, mat::PLAIN);
    for &c in &map.repair_spots {
        let y = map.ground_height(c.x, c.y) + 0.02;
        let mut stripe = |min: Vec2, max: Vec2| {
            m.add_box(Vec3::new(c.x + min.x, y - 0.02, c.y + min.y), Vec3::new(c.x + max.x, y, c.y + max.y), paint, paint);
        };
        let (r, t) = (3.2, 0.25);
        stripe(Vec2::new(-r, -r), Vec2::new(r, -r + t));
        stripe(Vec2::new(-r, r - t), Vec2::new(r, r));
        stripe(Vec2::new(-r, -r + t), Vec2::new(-r + t, r - t));
        stripe(Vec2::new(r - t, -r + t), Vec2::new(r, r - t));
        stripe(Vec2::new(-1.4, -0.35), Vec2::new(1.4, 0.35));
        stripe(Vec2::new(-0.35, -1.4), Vec2::new(0.35, -0.35));
        stripe(Vec2::new(-0.35, 0.35), Vec2::new(0.35, 1.4));
    }
}

fn terrain_mesh(t: &Terrain, m: &mut MeshData) {
    let h = |ix: i64, iz: i64| t.at(ix.clamp(0, t.nx as i64 - 1) as usize, iz.clamp(0, t.nz as i64 - 1) as usize);
    let base = m.verts.len() as u32;
    for iz in 0..t.nz as i64 {
        for ix in 0..t.nx as i64 {
            let p = Vec3::new(t.origin.x + ix as f32 * t.cell, h(ix, iz), t.origin.y + iz as f32 * t.cell);
            let n = Vec3::new(h(ix - 1, iz) - h(ix + 1, iz), 2.0 * t.cell, h(ix, iz - 1) - h(ix, iz + 1)).normalize();
            m.verts.push(Vertex { pos: p.to_array(), normal: n.to_array(), color: t.color.extend(t.material).to_array() });
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

/// Dashed center lines along each inner road, skipping intersections.
fn road_markings(map: &Map, m: &mut MeshData) {
    use crate::map::CELL;
    let paint = Vec4::new(0.85, 0.78, 0.45, mat::PLAIN);
    for (ri, r) in map.roads.iter().enumerate() {
        let (w, h) = (r.x1 - r.x0, r.y1 - r.y0);
        let along_x = w > h;
        let (len_cells, cross) = if along_x { (w, h) } else { (h, w) };
        if cross < 2 {
            continue;
        }
        let mid = if along_x { (r.y0 + r.y1) as f32 * 0.5 * CELL } else { (r.x0 + r.x1) as f32 * 0.5 * CELL };
        let start = if along_x { r.x0 } else { r.y0 } as f32 * CELL;
        let dashes = (len_cells as f32 * CELL / 6.0) as i32;
        for d in 0..dashes {
            let c = start + d as f32 * 6.0 + 3.0;
            let p = if along_x { Vec2::new(c, mid) } else { Vec2::new(mid, c) };
            let (cx, cy) = ((p.x / CELL) as i32, (p.y / CELL) as i32);
            let in_other = map.roads.iter().enumerate().any(|(oi, o)| {
                oi != ri && o.contains(cx, cy) && ((o.x1 - o.x0 > o.y1 - o.y0) != along_x)
            });
            if in_other {
                continue;
            }
            let half = if along_x { Vec2::new(1.4, 0.12) } else { Vec2::new(0.12, 1.4) };
            m.add_box(
                Vec3::new(p.x - half.x, 0.0, p.y - half.y),
                Vec3::new(p.x + half.x, 0.015, p.y + half.y),
                paint,
                paint,
            );
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
}

impl ChaseCam {
    pub fn new() -> ChaseCam {
        ChaseCam { yaw: 0.0, tilt: 0.35, focus: Vec3::ZERO, initialized: false }
    }

    pub fn update(&mut self, world: &World, tank: &Tank, dt: f32) -> Camera {
        let anchor = tank.pos + Vec3::Y * 2.2;
        let turret_yaw = tank.turret_yaw();
        if !self.initialized {
            self.focus = anchor;
            self.yaw = turret_yaw;
            self.initialized = true;
        }
        self.focus = self.focus.lerp(anchor, 1.0 - (-dt * 14.0).exp());
        self.yaw = wrap_angle(self.yaw + wrap_angle(turret_yaw - self.yaw) * (1.0 - (-dt * 10.0).exp()));

        let tilt = self.tilt.clamp(0.0, 1.0);
        let dist = 9.0 + tilt * 3.0;
        let height = 2.5 + tilt * 6.0;
        let back = dir2(self.yaw);
        let back3 = Vec3::new(-back.x, 0.0, -back.y);
        let desired = self.focus + back3 * dist + Vec3::Y * height;

        let to = desired - self.focus;
        let len = to.length();
        let hit = world.camera_raycast(self.focus, to / len, len);
        let pos = self.focus + to / len * (hit - 0.6).max(1.5);

        let ahead = Vec3::new(back.x, 0.0, back.y);
        // Firing kicks the camera back and up for a moment.
        let kick = (-tank.since_fire * 14.0).exp() * if tank.since_fire < 0.5 { 1.0 } else { 0.0 };
        let pos = pos - ahead * kick * 0.45 + Vec3::Y * kick * 0.2;
        Camera { pos, target: self.focus + ahead * 6.0 + Vec3::Y * (0.5 + kick * 0.3), fov_y: 65f32.to_radians() }
    }
}

/// The 3D part of a frame. `tanks` is what to draw (usually `world.tanks`,
/// interpolated for display). `ui` starts empty; the caller adds the HUD.
pub fn build(world: &World, tanks: &[Tank], art: &TankArt, camera: Camera) -> Scene {
    let mut instances = Vec::new();
    for t in tanks {
        art.instances(t, &mut instances);
    }

    Scene {
        camera,
        time: world.time,
        sun_dir: SUN_DIR,
        shadow_center: Vec3::new(world.map.size * 0.5, 0.0, world.map.size * 0.5),
        shadow_radius: world.map.size * 0.75,
        instances,
        ui: Vec::new(),
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
