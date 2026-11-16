//! Turns simulation state into renderer input: the static map mesh, tank
//! instances, the chase camera, and the HUD.

use glam::{Mat4, Quat, Vec2, Vec3, Vec4};

use crate::map::{Map, mat};
use crate::render::mesh::MeshData;
use crate::render::{Camera, Scene, UiVertex};
use crate::sim::{Tank, World, dir2, wrap_angle};
use crate::tank::TankArt;

pub const SUN_DIR: Vec3 = Vec3::new(0.55, 0.48, 0.4);

/// Static geometry for the whole arena, built once per map.
pub fn map_mesh(map: &Map) -> MeshData {
    let mut m = MeshData::default();
    let pad = 120.0;
    let asphalt = Vec4::new(0.17, 0.17, 0.18, mat::ASPHALT);
    m.add_box(Vec3::new(-pad, -0.2, -pad), Vec3::new(map.size + pad, 0.0, map.size + pad), asphalt, asphalt);

    for b in &map.boxes {
        m.add_box(b.min, b.max, b.color.extend(b.side_mat), b.color.extend(b.top_mat));
    }

    let sphere = MeshData::sphere(12, 8);
    for t in &map.trees {
        let s = Vec3::new(t.radius * 2.0, t.radius * 1.7, t.radius * 2.0);
        m.append(&sphere, Mat4::from_scale_rotation_translation(s, Quat::IDENTITY, t.pos), Vec4::new(0.27, 0.45, 0.2, mat::PLAIN));
    }

    road_markings(map, &mut m);
    m
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
        let anchor = tank.pos3() + Vec3::Y * 2.2;
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
        Camera { pos, target: self.focus + ahead * 6.0 + Vec3::Y * 0.5, fov_y: 65f32.to_radians() }
    }
}

pub fn build(world: &World, art: &TankArt, camera: Camera, width: f32, height: f32) -> Scene {
    let mut instances = Vec::new();
    for t in &world.tanks {
        art.instances(t, &mut instances);
    }

    let mut ui = Vec::new();
    let player = &world.tanks[0];
    let vp = camera.view_proj(width / height);

    // Crosshair sits where the turret actually points, so turret lag is visible.
    let aim_dir = dir2(player.turret_yaw());
    let aim_point = player.pos3() + Vec3::new(aim_dir.x, 0.0, aim_dir.y) * 40.0 + Vec3::Y * 1.7;
    if let Some((x, y)) = project(vp, aim_point, width, height) {
        crosshair(&mut ui, x, y);
    }

    // Speed bar, bottom center.
    let frac = (player.speed().abs() / player.hull.max_speed).min(1.0);
    let (bw, bh) = (220.0, 8.0);
    let (bx, by) = (width * 0.5 - bw * 0.5, height - 40.0);
    push_rect(&mut ui, bx - 2.0, by - 2.0, bw + 4.0, bh + 4.0, [0.0, 0.0, 0.0, 0.45]);
    push_rect(&mut ui, bx, by, bw * frac, bh, [0.95, 0.85, 0.35, 0.9]);

    Scene {
        camera,
        time: world.time,
        sun_dir: SUN_DIR,
        shadow_center: Vec3::new(world.map.size * 0.5, 0.0, world.map.size * 0.5),
        shadow_radius: world.map.size * 0.75,
        instances,
        ui,
    }
}

fn project(vp: Mat4, p: Vec3, w: f32, h: f32) -> Option<(f32, f32)> {
    let c = vp * p.extend(1.0);
    if c.w <= 0.01 {
        return None;
    }
    let ndc = c.truncate() / c.w;
    Some(((ndc.x * 0.5 + 0.5) * w, (0.5 - ndc.y * 0.5) * h))
}

fn crosshair(ui: &mut Vec<UiVertex>, x: f32, y: f32) {
    let (shadow, white) = ([0.0, 0.0, 0.0, 0.5], [1.0, 1.0, 1.0, 0.9]);
    for (c, o) in [(shadow, 1.0), (white, 0.0)] {
        for (dx, dy, w, h) in [(-14.0, -1.0, 9.0, 2.0), (5.0, -1.0, 9.0, 2.0), (-1.0, -14.0, 2.0, 9.0), (-1.0, 5.0, 2.0, 9.0)] {
            push_rect(ui, x + dx + o, y + dy + o, w, h, c);
        }
    }
}

pub fn push_rect(ui: &mut Vec<UiVertex>, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
    let v = |px, py| UiVertex { pos: [px, py], color };
    ui.extend_from_slice(&[v(x, y), v(x + w, y), v(x + w, y + h), v(x, y), v(x + w, y + h), v(x, y + h)]);
}
