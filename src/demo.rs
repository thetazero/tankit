//! Renderer showcase: a small lit test yard with an orbiting camera.
//! Replaced by the real arena in the next step; kept tiny on purpose.

use glam::{Mat4, Quat, Vec3, Vec4};

use crate::render::mesh::{Instance, MeshData, MeshId};
use crate::render::{Camera, Scene, UiVertex};

pub fn static_mesh() -> MeshData {
    let mut m = MeshData::default();
    // Asphalt ground (material 2), paving patch (5), grass (4).
    m.add_box(Vec3::new(-80.0, -0.1, -80.0), Vec3::new(80.0, 0.0, 80.0), Vec4::new(0.17, 0.17, 0.18, 2.0), Vec4::new(0.17, 0.17, 0.18, 2.0));
    m.add_box(Vec3::new(-14.0, 0.0, -14.0), Vec3::new(14.0, 0.06, 14.0), Vec4::new(0.6, 0.57, 0.52, 0.0), Vec4::new(0.62, 0.58, 0.52, 5.0));
    m.add_box(Vec3::new(18.0, 0.0, -10.0), Vec3::new(34.0, 0.1, 10.0), Vec4::new(0.3, 0.45, 0.22, 0.0), Vec4::new(0.32, 0.48, 0.24, 4.0));
    // Buildings with window walls (1) and roofs (3).
    let buildings = [
        (Vec3::new(-40.0, 0.0, -30.0), Vec3::new(-22.0, 22.0, -12.0), Vec3::new(0.80, 0.74, 0.62)),
        (Vec3::new(-40.0, 0.0, 6.0), Vec3::new(-24.0, 12.0, 30.0), Vec3::new(0.74, 0.47, 0.36)),
        (Vec3::new(-10.0, 0.0, -44.0), Vec3::new(12.0, 30.0, -24.0), Vec3::new(0.46, 0.54, 0.62)),
        (Vec3::new(20.0, 0.0, 20.0), Vec3::new(40.0, 16.0, 38.0), Vec3::new(0.86, 0.83, 0.74)),
    ];
    for (min, max, c) in buildings {
        m.add_box(min, max, c.extend(1.0), c.extend(3.0));
    }
    // Cover.
    for (i, x) in [-8.0f32, -3.0, 2.0, 7.0].iter().enumerate() {
        let z = if i % 2 == 0 { 6.0 } else { -6.0 };
        m.add_box(Vec3::new(x - 2.2, 0.0, z - 0.5), Vec3::new(x + 2.2, 1.3, z + 0.5), Vec4::new(0.72, 0.72, 0.70, 0.0), Vec4::new(0.72, 0.72, 0.70, 0.0));
    }
    // Trees in the park.
    let sphere = MeshData::sphere(16, 10);
    for p in [Vec3::new(22.0, 0.0, -5.0), Vec3::new(29.0, 0.0, 3.0), Vec3::new(25.0, 0.0, 6.0)] {
        m.add_box(p - Vec3::new(0.35, 0.0, 0.35), p + Vec3::new(0.35, 3.2, 0.35), Vec4::new(0.36, 0.26, 0.18, 0.0), Vec4::new(0.36, 0.26, 0.18, 0.0));
        m.append(&sphere, Mat4::from_scale_rotation_translation(Vec3::splat(4.6), Quat::IDENTITY, p + Vec3::Y * 4.4), Vec4::new(0.28, 0.46, 0.2, 0.0));
    }
    m
}

pub fn scene(time: f32, width: f32, height: f32) -> Scene {
    let mut instances = Vec::new();
    let ins = |id, m, c: Vec4| (id, Instance::new(m, c));

    // A tank-shaped stack of primitives, slowly turning, to exercise instancing.
    let root = Mat4::from_rotation_translation(Quat::from_rotation_y(time * 0.4), Vec3::ZERO);
    let green = Vec4::new(0.30, 0.62, 0.30, 0.0);
    instances.push(ins(MeshId::Cube, root * Mat4::from_scale_rotation_translation(Vec3::new(3.0, 0.9, 4.6), Quat::IDENTITY, Vec3::new(0.0, 0.85, 0.0)), green));
    for x in [-1.45f32, 1.45] {
        instances.push(ins(MeshId::Cube, root * Mat4::from_scale_rotation_translation(Vec3::new(0.7, 0.9, 4.9), Quat::IDENTITY, Vec3::new(x, 0.5, 0.0)), Vec4::new(0.12, 0.12, 0.12, 0.0)));
    }
    instances.push(ins(MeshId::Cube, root * Mat4::from_scale_rotation_translation(Vec3::new(1.8, 0.7, 2.2), Quat::IDENTITY, Vec3::new(0.0, 1.65, -0.2)), green * 0.9));
    instances.push(ins(MeshId::Cylinder, root * Mat4::from_scale_rotation_translation(Vec3::new(0.28, 2.6, 0.28), Quat::from_rotation_x(std::f32::consts::FRAC_PI_2), Vec3::new(0.0, 1.7, 2.2)), Vec4::new(0.2, 0.2, 0.2, 0.0)));

    // Spinning crystal (emissive) and a glowing orb.
    let bob = (time * 2.0).sin() * 0.25;
    instances.push(ins(MeshId::Gem, Mat4::from_scale_rotation_translation(Vec3::new(1.0, 1.4, 1.0), Quat::from_rotation_y(time * 2.0), Vec3::new(6.0, 1.4 + bob, 3.0)), Vec4::new(0.3, 0.9, 1.0, 1.6)));
    instances.push(ins(MeshId::Sphere, Mat4::from_scale_rotation_translation(Vec3::splat(1.2), Quat::IDENTITY, Vec3::new(-6.0, 1.0, -2.0)), Vec4::new(1.0, 0.6, 0.2, 4.0)));
    instances.push(ins(MeshId::Sphere, Mat4::from_scale_rotation_translation(Vec3::splat(2.0), Quat::IDENTITY, Vec3::new(-6.0, 1.0, 8.0)), Vec4::new(0.85, 0.85, 0.88, 0.0)));

    let a = time * 0.15 + 0.6;
    let camera = Camera {
        pos: Vec3::new(a.sin() * 26.0, 11.0, a.cos() * 26.0),
        target: Vec3::new(0.0, 1.5, 0.0),
        fov_y: 60f32.to_radians(),
    };

    // HUD smoke test: crosshair at the center.
    let (cx, cy) = (width * 0.5, height * 0.5);
    let mut ui = Vec::new();
    let white = [1.0, 1.0, 1.0, 0.85];
    for (x, y, w, h) in [(cx - 12.0, cy - 1.0, 8.0, 2.0), (cx + 4.0, cy - 1.0, 8.0, 2.0), (cx - 1.0, cy - 12.0, 2.0, 8.0), (cx - 1.0, cy + 4.0, 2.0, 8.0)] {
        push_rect(&mut ui, x, y, w, h, white);
    }

    Scene {
        camera,
        time,
        sun_dir: Vec3::new(0.55, 0.48, 0.4),
        shadow_center: Vec3::ZERO,
        shadow_radius: 70.0,
        instances,
        ui,
    }
}

pub fn push_rect(ui: &mut Vec<UiVertex>, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
    let v = |px, py| UiVertex { pos: [px, py], color };
    ui.extend_from_slice(&[v(x, y), v(x + w, y), v(x + w, y + h), v(x, y), v(x + w, y + h), v(x, y + h)]);
}
