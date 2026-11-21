//! The depot bay the garage opens in: a concrete hangar with the chassis on a
//! turntable under a skylight, maintenance arms either side, a gantry crane
//! overhead, a sealed blast door behind, lamps, racks and a link console.
//!
//! It has its own geometry (drawn as instances, the map's static mesh off),
//! its own light (sun through the skylight, dim ambient, dust in the beam)
//! and a fixed camera; the tank turns on the deck instead of the camera
//! orbiting. Coordinates are bay-local: the turntable's center is the origin.

use glam::{Mat4, Quat, Vec2, Vec3, Vec4};

use crate::render::material::Material;
use crate::render::mesh::{Instance, MeshData, MeshId};
use crate::render::{Atmosphere, Camera, Particle, Renderer, Scene};
use crate::sim::Tank;
use crate::tank::TankArt;

/// Top of the turntable deck: where the tank stands.
pub const DECK: f32 = 0.15;
const DECK_R: f32 = 4.2;
/// The room's inside: x and z extents, and the roof.
const ROOM_MIN: Vec2 = Vec2::new(-13.0, -11.0);
const ROOM_MAX: Vec2 = Vec2::new(13.0, 14.0);
const ROOF: f32 = 10.0;
/// The skylight in the roof (xz min, max), offset toward the sun so its beam lands on the deck.
const SKY_MIN: Vec2 = Vec2::new(-0.5, 0.0);
const SKY_MAX: Vec2 = Vec2::new(6.5, 8.0);
/// A second, narrow skylight toward the back: a stripe of light before the door.
const SKY2_MIN: Vec2 = Vec2::new(-9.0, -5.6);
const SKY2_MAX: Vec2 = Vec2::new(9.0, -4.2);
/// Camera: world yaw of its offset from the deck, distance and height above the focus.
pub const CAM_YAW: f32 = 0.42;
const CAM_DIST: f32 = 10.5;
const CAM_HEIGHT: f32 = 2.9;
/// Hanging lamps (bulb positions).
const LAMPS: [Vec3; 4] = [
    Vec3::new(-8.5, 5.6, -6.8),
    Vec3::new(-3.0, 5.9, -7.2),
    Vec3::new(2.5, 5.6, -6.8),
    Vec3::new(8.0, 5.9, -7.2),
];
const LAMP_LIGHT: Vec3 = Vec3::new(1.0, 0.78, 0.5);
const LINK: Vec3 = Vec3::new(0.35, 0.85, 1.0);

/// The bay's meshes, uploaded once.
pub struct BayArt {
    room: MeshId,
    deck: MeshId,
}

impl BayArt {
    pub fn upload(r: &mut Renderer) -> BayArt {
        BayArt {
            room: r.add_mesh(&room_mesh()),
            deck: r.add_mesh(&deck_mesh()),
        }
    }
}

/// The light in the bay: a warm sun through the skylight, a dim cool room.
pub fn atmosphere() -> Atmosphere {
    Atmosphere {
        sun_dir: Vec3::new(0.3, 1.0, 0.45).normalize(),
        sun_color: Vec3::new(2.6, 2.25, 1.8),
        sky_zenith: Vec3::new(0.42, 0.48, 0.56),
        sky_horizon: Vec3::new(0.2, 0.2, 0.21),
        ground_color: Vec3::new(0.24, 0.22, 0.2),
        fog_density: 0.012,
        wind: Vec2::ZERO,
        blowing_sand: 0.0,
        dust: Vec3::new(0.5, 0.48, 0.45),
        settle: crate::render::SAND_SETTLE,
    }
}

/// The fixed view of the deck, framed right of the garage panel like the
/// garage's orbit. `pull` > 1 backs it off (arriving, it eases in from 1.25).
pub fn camera(aspect: f32, pull: f32) -> Camera {
    let focus = Vec3::Y * (DECK + 1.0);
    let dist = CAM_DIST * pull;
    let pos = focus
        + Vec3::new(
            CAM_YAW.sin() * dist,
            CAM_HEIGHT * pull,
            CAM_YAW.cos() * dist,
        );
    let right = (focus - pos).cross(Vec3::Y).normalize_or_zero();
    let shift = (focus - pos).length() * 0.36 * (aspect / (16.0 / 9.0)).clamp(0.6, 1.2);
    Camera {
        pos,
        target: focus - right * shift,
        fov_y: 50f32.to_radians(),
    }
}

/// `tank` (already dressed in the kit to show) standing on the deck, turned
/// `spin` radians from its front three-quarter view, at `time` seconds.
pub fn scene(
    art: &TankArt,
    bay: &BayArt,
    tank: &Tank,
    spin: f32,
    time: f32,
    camera: Camera,
) -> Scene {
    // Facing so the garage's front three-quarter view (see `garage::front_angle`) is toward the camera.
    let yaw = CAM_YAW - 0.65 - spin;
    let mut t = tank.clone();
    t.pos = Vec3::Y * DECK;
    t.rot = Quat::from_rotation_y(yaw);
    t.vel = Vec3::ZERO;
    t.since_fire = 100.0;
    t.wheel_lift.iter_mut().for_each(|l| *l = 0.0);

    let mut instances = Vec::new();
    let mut particles = Vec::new();
    art.instances(&t, &mut instances, &mut particles);
    let white = Vec4::new(1.0, 1.0, 1.0, 0.0);
    instances.push((bay.room, Instance::new(Mat4::IDENTITY, white)));
    instances.push((bay.deck, Instance::new(Mat4::from_rotation_y(-spin), white)));
    lights(time, &mut particles);
    let atmosphere = atmosphere();
    Scene {
        camera,
        time,
        sun_dir: atmosphere.sun_dir,
        atmosphere,
        shadow_center: Vec3::new(0.0, 3.0, -1.0),
        shadow_radius: 17.0,
        instances,
        particles,
        ui: Vec::new(),
        world: false,
    }
}

/// Lamp halos, the console's glow, the deck ring, and dust turning in the sunbeam.
fn lights(time: f32, out: &mut Vec<Particle>) {
    for (i, &l) in LAMPS.iter().enumerate() {
        let flicker = 1.0
            - 0.06
                * ((time * 13.0 + i as f32 * 2.1).sin() * (time * 3.1 + i as f32).sin()).max(0.0);
        out.push(Particle::glow(
            l,
            0.9,
            LAMP_LIGHT,
            0.55 * flicker,
            Vec3::ZERO,
        ));
        out.push(Particle::glow(
            l,
            2.6,
            LAMP_LIGHT,
            0.08 * flicker,
            Vec3::ZERO,
        ));
    }
    // The link console's screen, and status lights on the door.
    out.push(Particle::glow(
        Vec3::new(7.2, 1.55, -2.6),
        1.1,
        LINK,
        0.18,
        Vec3::ZERO,
    ));
    for (i, x) in [-5.6f32, 3.6].into_iter().enumerate() {
        let blink = (0.5 + 0.5 * (time * 2.0 + i as f32 * 1.7).sin()).powi(3);
        out.push(Particle::glow(
            Vec3::new(x, 6.8, ROOM_MIN.y + 0.45),
            0.35,
            Vec3::new(1.0, 0.25, 0.12),
            0.25 + 0.6 * blink,
            Vec3::ZERO,
        ));
    }
    // A faint shaft down from the skylight.
    let sun = atmosphere().sun_dir;
    let top = Vec3::new(
        (SKY_MIN.x + SKY_MAX.x) * 0.5,
        ROOF,
        (SKY_MIN.y + SKY_MAX.y) * 0.5,
    );
    for k in 0..5 {
        let f = (k as f32 + 0.5) / 5.0;
        let at = top - sun * (ROOF - DECK) / sun.y * f;
        out.push(Particle::glow(
            at,
            2.6,
            Vec3::new(1.0, 0.92, 0.78),
            0.022,
            sun * 1.6,
        ));
    }
    // Dust motes drifting through the beam: fixed seeds, slow wander.
    let mut h = 0x9e37_79b9u32;
    let mut rnd = || {
        h ^= h << 13;
        h ^= h >> 17;
        h ^= h << 5;
        (h >> 8) as f32 / 16_777_216.0
    };
    for _ in 0..40 {
        let (a, b, c, s) = (rnd(), rnd(), rnd(), rnd());
        let y = 0.6 + ((c * 7.0 + time * (0.05 + 0.08 * s)) % 1.0) * 6.5;
        // In the beam at this height: the skylight's footprint slid along the sun.
        let off = sun * (y - ROOF) / sun.y;
        let x =
            SKY_MIN.x + a * (SKY_MAX.x - SKY_MIN.x) + off.x + (time * 0.3 + s * 6.0).sin() * 0.3;
        let z =
            SKY_MIN.y + b * (SKY_MAX.y - SKY_MIN.y) + off.z + (time * 0.23 + c * 6.0).cos() * 0.3;
        out.push(Particle::glow(
            Vec3::new(x, y, z),
            0.008 + 0.01 * s,
            Vec3::new(1.0, 0.9, 0.75),
            0.15 + 0.25 * s,
            Vec3::ZERO,
        ));
    }
}

fn col(rgb: [f32; 3], m: Material) -> Vec4 {
    Vec4::new(rgb[0], rgb[1], rgb[2], m.id())
}

fn bx(m: &mut MeshData, min: Vec3, max: Vec3, c: Vec4) {
    m.add_box(min, max, c, c);
}

/// A box of `size` centered at `at`, turned by `rot`.
fn obox(m: &mut MeshData, at: Vec3, size: Vec3, rot: Quat, c: Vec4) {
    m.append(
        &MeshData::cube(),
        Mat4::from_scale_rotation_translation(size, rot, at),
        c,
    );
}

/// An upright cylinder of radius `r` from `y0` to `y1` at `at` (xz).
fn cyl(m: &mut MeshData, at: Vec2, r: f32, y0: f32, y1: f32, segs: u32, c: Vec4) {
    m.append(
        &MeshData::cylinder(segs),
        Mat4::from_scale_rotation_translation(
            Vec3::new(r * 2.0, y1 - y0, r * 2.0),
            Quat::IDENTITY,
            Vec3::new(at.x, (y0 + y1) * 0.5, at.y),
        ),
        c,
    );
}

/// A beam between two points, `w` thick.
fn beam(m: &mut MeshData, a: Vec3, b: Vec3, w: f32, c: Vec4) {
    let d = b - a;
    let rot = Quat::from_rotation_arc(Vec3::Z, d.normalize());
    obox(m, (a + b) * 0.5, Vec3::new(w, w, d.length()), rot, c);
}

/// The turning part of the turntable: deck plate, ribs and a ring of studs.
fn deck_mesh() -> MeshData {
    let mut m = MeshData::default();
    let plate = col([0.26, 0.27, 0.28], Material::Metal);
    let rib = col([0.18, 0.18, 0.19], Material::Metal);
    let mark = col([0.78, 0.6, 0.16], Material::Painted);
    cyl(&mut m, Vec2::ZERO, DECK_R, 0.02, DECK, 40, plate);
    for i in 0..16 {
        let a = i as f32 / 16.0 * std::f32::consts::TAU;
        let d = Vec3::new(a.sin(), 0.0, a.cos());
        obox(
            &mut m,
            d * (DECK_R * 0.6) + Vec3::Y * DECK,
            Vec3::new(0.08, 0.03, DECK_R * 0.72),
            Quat::from_rotation_y(a),
            rib,
        );
    }
    // A painted arrow at the front edge, so the turning reads.
    obox(
        &mut m,
        Vec3::new(0.0, DECK + 0.005, DECK_R - 0.35),
        Vec3::new(0.7, 0.012, 0.22),
        Quat::IDENTITY,
        mark,
    );
    cyl(&mut m, Vec2::ZERO, 0.7, DECK, DECK + 0.03, 20, rib);
    m
}

/// Everything that stands still.
fn room_mesh() -> MeshData {
    let mut m = MeshData::default();
    let concrete = col([0.46, 0.45, 0.43], Material::Concrete);
    let wall = col([0.42, 0.41, 0.39], Material::Concrete);
    let steel = col([0.22, 0.25, 0.24], Material::Painted);
    let dark = col([0.12, 0.12, 0.13], Material::Metal);
    let hazard = col([0.8, 0.6, 0.12], Material::Painted);
    let black = col([0.06, 0.06, 0.06], Material::Painted);
    let rust = col([0.4, 0.27, 0.18], Material::Rust);
    let wood = col([0.5, 0.4, 0.28], Material::Wood);
    let lamp = col([LAMP_LIGHT.x, LAMP_LIGHT.y, LAMP_LIGHT.z], Material::Glow);
    let link = col([LINK.x, LINK.y, LINK.z], Material::Glow);
    let (lo, hi) = (ROOM_MIN, ROOM_MAX);
    const T: f32 = 0.6;

    // Floor, with a shallow pit for the turntable's ring.
    bx(
        &mut m,
        Vec3::new(lo.x - T, -0.4, lo.y - T),
        Vec3::new(hi.x + T, 0.0, hi.y + T),
        concrete,
    );
    cyl(&mut m, Vec2::ZERO, DECK_R + 0.35, 0.0, 0.05, 48, dark);
    // The ring of link light round the deck.
    for i in 0..48 {
        let a = i as f32 / 48.0 * std::f32::consts::TAU;
        let d = Vec3::new(a.sin(), 0.0, a.cos());
        obox(
            &mut m,
            d * (DECK_R + 0.16) + Vec3::Y * 0.06,
            Vec3::new(0.05, 0.03, 0.3),
            Quat::from_rotation_y(a),
            link,
        );
    }
    // Painted bay lines: a square round the deck and a walkway to the door.
    let r = 6.0;
    for (a, b) in [
        (Vec2::new(-r, -r), Vec2::new(r, -r + 0.12)),
        (Vec2::new(-r, r - 0.12), Vec2::new(r, r)),
        (Vec2::new(-r, -r), Vec2::new(-r + 0.12, r)),
        (Vec2::new(r - 0.12, -r), Vec2::new(r, r)),
    ] {
        bx(
            &mut m,
            Vec3::new(a.x, 0.0, a.y),
            Vec3::new(b.x, 0.01, b.y),
            hazard,
        );
    }
    // Hazard chevrons at the corners of the square.
    for (sx, sz) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        for k in 0..3 {
            let c = Vec3::new(sx * (r - 0.5 - k as f32 * 0.45), 0.006, sz * (r - 0.5));
            obox(
                &mut m,
                c,
                Vec3::new(0.18, 0.012, 0.7),
                Quat::from_rotation_y(0.6 * sx * sz),
                hazard,
            );
        }
    }

    // Walls, with steel columns and a ring beam.
    bx(
        &mut m,
        Vec3::new(lo.x - T, 0.0, lo.y - T),
        Vec3::new(hi.x + T, ROOF, lo.y),
        wall,
    );
    bx(
        &mut m,
        Vec3::new(lo.x - T, 0.0, hi.y),
        Vec3::new(hi.x + T, ROOF, hi.y + T),
        wall,
    );
    bx(
        &mut m,
        Vec3::new(lo.x - T, 0.0, lo.y),
        Vec3::new(lo.x, ROOF, hi.y),
        wall,
    );
    bx(
        &mut m,
        Vec3::new(hi.x, 0.0, lo.y),
        Vec3::new(hi.x + T, ROOF, hi.y),
        wall,
    );
    // A darker skirting band of grime and wear.
    let grime = col([0.27, 0.26, 0.25], Material::Concrete);
    bx(
        &mut m,
        Vec3::new(lo.x, 0.0, lo.y),
        Vec3::new(hi.x, 1.1, lo.y + 0.08),
        grime,
    );
    bx(
        &mut m,
        Vec3::new(lo.x, 0.0, lo.y),
        Vec3::new(lo.x + 0.08, 1.1, hi.y),
        grime,
    );
    bx(
        &mut m,
        Vec3::new(hi.x - 0.08, 0.0, lo.y),
        Vec3::new(hi.x, 1.1, hi.y),
        grime,
    );
    let mut x = lo.x + 0.4;
    while x < hi.x {
        bx(
            &mut m,
            Vec3::new(x - 0.3, 0.0, lo.y),
            Vec3::new(x + 0.3, ROOF, lo.y + 0.45),
            steel,
        );
        x += 4.4;
    }
    let mut z = lo.y + 4.0;
    while z < hi.y {
        for wx in [lo.x, hi.x - 0.45] {
            bx(
                &mut m,
                Vec3::new(wx, 0.0, z - 0.3),
                Vec3::new(wx + 0.45, ROOF, z + 0.3),
                steel,
            );
        }
        z += 4.4;
    }
    bx(
        &mut m,
        Vec3::new(lo.x, 7.6, lo.y),
        Vec3::new(hi.x, 8.1, lo.y + 0.55),
        steel,
    );
    bx(
        &mut m,
        Vec3::new(lo.x, 7.6, lo.y),
        Vec3::new(lo.x + 0.55, 8.1, hi.y),
        steel,
    );
    bx(
        &mut m,
        Vec3::new(hi.x - 0.55, 7.6, lo.y),
        Vec3::new(hi.x, 8.1, hi.y),
        steel,
    );

    // Roof: slabs round the two skylights, beams across the openings.
    let roof = col([0.3, 0.3, 0.3], Material::Concrete);
    let slab = |m: &mut MeshData, x0: f32, z0: f32, x1: f32, z1: f32| {
        if x1 > x0 && z1 > z0 {
            bx(
                m,
                Vec3::new(x0, ROOF, z0),
                Vec3::new(x1, ROOF + 0.5, z1),
                roof,
            );
        }
    };
    let (ax, bxx) = (lo.x - T, hi.x + T);
    let (az, bz) = (lo.y - T, hi.y + T);
    slab(&mut m, ax, az, bxx, SKY2_MIN.y);
    slab(&mut m, ax, SKY2_MIN.y, SKY2_MIN.x, SKY2_MAX.y);
    slab(&mut m, SKY2_MAX.x, SKY2_MIN.y, bxx, SKY2_MAX.y);
    slab(&mut m, ax, SKY2_MAX.y, bxx, SKY_MIN.y);
    slab(&mut m, ax, SKY_MIN.y, SKY_MIN.x, SKY_MAX.y);
    slab(&mut m, SKY_MAX.x, SKY_MIN.y, bxx, SKY_MAX.y);
    slab(&mut m, ax, SKY_MAX.y, bxx, bz);
    let mut x = SKY_MIN.x + 0.9;
    while x < SKY_MAX.x {
        bx(
            &mut m,
            Vec3::new(x - 0.12, ROOF - 0.5, SKY_MIN.y),
            Vec3::new(x + 0.12, ROOF, SKY_MAX.y),
            steel,
        );
        x += 1.3;
    }
    let mut x = SKY2_MIN.x + 0.8;
    while x < SKY2_MAX.x {
        bx(
            &mut m,
            Vec3::new(x - 0.1, ROOF - 0.4, SKY2_MIN.y),
            Vec3::new(x + 0.1, ROOF, SKY2_MAX.y),
            steel,
        );
        x += 1.6;
    }
    for z in [SKY_MIN.y, SKY_MAX.y, SKY2_MIN.y, SKY2_MAX.y] {
        bx(
            &mut m,
            Vec3::new(lo.x, ROOF - 0.7, z - 0.2),
            Vec3::new(hi.x, ROOF, z + 0.2),
            steel,
        );
    }

    // The blast door: frame, ribbed leaves split down the middle, hazard edging.
    let (d0, d1, dh, dz) = (-5.0f32, 3.0f32, 6.4f32, lo.y);
    bx(
        &mut m,
        Vec3::new(d0 - 0.7, 0.0, dz),
        Vec3::new(d0, dh + 0.7, dz + 0.6),
        steel,
    );
    bx(
        &mut m,
        Vec3::new(d1, 0.0, dz),
        Vec3::new(d1 + 0.7, dh + 0.7, dz + 0.6),
        steel,
    );
    bx(
        &mut m,
        Vec3::new(d0 - 0.7, dh, dz),
        Vec3::new(d1 + 0.7, dh + 0.7, dz + 0.6),
        steel,
    );
    let leaf = col([0.3, 0.32, 0.3], Material::Painted);
    bx(
        &mut m,
        Vec3::new(d0, 0.0, dz),
        Vec3::new(d1, dh, dz + 0.2),
        leaf,
    );
    let mid = (d0 + d1) * 0.5;
    bx(
        &mut m,
        Vec3::new(mid - 0.05, 0.0, dz),
        Vec3::new(mid + 0.05, dh, dz + 0.26),
        black,
    );
    let mut y = 0.9;
    while y < dh - 0.3 {
        bx(
            &mut m,
            Vec3::new(d0 + 0.2, y, dz),
            Vec3::new(mid - 0.2, y + 0.18, dz + 0.32),
            leaf,
        );
        bx(
            &mut m,
            Vec3::new(mid + 0.2, y, dz),
            Vec3::new(d1 - 0.2, y + 0.18, dz + 0.32),
            leaf,
        );
        y += 0.75;
    }
    // Hazard band along the bottom: diagonal stripes, clipped to the band.
    bx(
        &mut m,
        Vec3::new(d0, 0.0, dz),
        Vec3::new(d1, 0.6, dz + 0.24),
        black,
    );
    let mut x = d0 + 0.2;
    while x < d1 - 0.1 {
        obox(
            &mut m,
            Vec3::new(x, 0.3, dz + 0.25),
            Vec3::new(0.22, 0.62, 0.02),
            Quat::from_rotation_z(0.6),
            hazard,
        );
        x += 0.55;
    }
    bx(
        &mut m,
        Vec3::new(d0 - 0.7, 0.0, dz + 0.6),
        Vec3::new(d0, 0.6, dz + 0.62),
        hazard,
    );
    bx(
        &mut m,
        Vec3::new(d1, 0.0, dz + 0.6),
        Vec3::new(d1 + 0.7, 0.6, dz + 0.62),
        hazard,
    );
    // Warning lamps either side of the door (the glow is added per frame).
    for x in [d0 - 0.35, d1 + 0.35] {
        bx(
            &mut m,
            Vec3::new(x - 0.2, 6.6, dz + 0.6),
            Vec3::new(x + 0.2, 7.0, dz + 0.85),
            col([1.0, 0.3, 0.15], Material::Glow),
        );
    }

    // Hanging lamps: cable, shade, bulb.
    for l in LAMPS {
        bx(
            &mut m,
            l + Vec3::new(-0.02, 0.3, -0.02),
            Vec3::new(l.x + 0.02, ROOF, l.z + 0.02),
            dark,
        );
        m.append(
            &MeshData::cylinder(16),
            Mat4::from_scale_rotation_translation(
                Vec3::new(0.9, 0.3, 0.9),
                Quat::IDENTITY,
                l + Vec3::Y * 0.2,
            ),
            steel,
        );
        cyl(
            &mut m,
            Vec2::new(l.x, l.z),
            0.28,
            l.y - 0.02,
            l.y + 0.06,
            12,
            lamp,
        );
    }

    // Gantry crane: rails on the side walls, a bridge across, a hoist hanging over the deck.
    for rx in [lo.x + 0.6, hi.x - 0.9] {
        bx(
            &mut m,
            Vec3::new(rx, 8.1, lo.y + 0.6),
            Vec3::new(rx + 0.3, 8.4, hi.y),
            dark,
        );
    }
    let bz0 = -1.8;
    bx(
        &mut m,
        Vec3::new(lo.x + 0.5, 8.4, bz0 - 0.35),
        Vec3::new(hi.x - 0.5, 9.1, bz0 + 0.35),
        hazard,
    );
    bx(
        &mut m,
        Vec3::new(lo.x + 0.5, 8.3, bz0 - 0.4),
        Vec3::new(hi.x - 0.5, 8.4, bz0 + 0.4),
        dark,
    );
    let hx = -2.6;
    bx(
        &mut m,
        Vec3::new(hx - 0.5, 7.7, bz0 - 0.45),
        Vec3::new(hx + 0.5, 8.3, bz0 + 0.45),
        steel,
    );
    for dx in [-0.15, 0.15] {
        bx(
            &mut m,
            Vec3::new(hx + dx - 0.02, 6.2, bz0 - 0.02),
            Vec3::new(hx + dx + 0.02, 7.7, bz0 + 0.02),
            dark,
        );
    }
    bx(
        &mut m,
        Vec3::new(hx - 0.3, 5.8, bz0 - 0.2),
        Vec3::new(hx + 0.3, 6.2, bz0 + 0.2),
        hazard,
    );
    obox(
        &mut m,
        Vec3::new(hx, 5.55, bz0),
        Vec3::new(0.08, 0.5, 0.08),
        Quat::IDENTITY,
        dark,
    );
    obox(
        &mut m,
        Vec3::new(hx + 0.12, 5.3, bz0),
        Vec3::new(0.3, 0.08, 0.08),
        Quat::IDENTITY,
        dark,
    );

    // Maintenance arms either side of the deck, folded back, tool heads down.
    for (base, face) in [
        (Vec2::new(-5.8, -2.2), 0.9f32),
        (Vec2::new(5.5, -3.8), -0.8),
    ] {
        cyl(&mut m, base, 0.55, 0.0, 0.45, 16, dark);
        cyl(&mut m, base, 0.38, 0.45, 1.1, 16, steel);
        let dir = Vec3::new(face.sin(), 0.0, face.cos());
        let shoulder = Vec3::new(base.x, 1.2, base.y);
        let elbow = shoulder + dir * 1.1 + Vec3::Y * 2.1;
        let wrist = elbow + dir * 1.7 - Vec3::Y * 0.7;
        beam(&mut m, shoulder, elbow, 0.32, hazard);
        beam(&mut m, elbow, wrist, 0.22, hazard);
        m.append(
            &MeshData::sphere(10, 6),
            Mat4::from_scale_rotation_translation(Vec3::splat(0.42), Quat::IDENTITY, elbow),
            dark,
        );
        m.append(
            &MeshData::sphere(10, 6),
            Mat4::from_scale_rotation_translation(Vec3::splat(0.3), Quat::IDENTITY, wrist),
            dark,
        );
        beam(
            &mut m,
            wrist,
            wrist - Vec3::Y * 0.55 + dir * 0.1,
            0.14,
            dark,
        );
        cyl(
            &mut m,
            Vec2::new(wrist.x + dir.x * 0.12, wrist.z + dir.z * 0.12),
            0.05,
            wrist.y - 0.85,
            wrist.y - 0.5,
            8,
            link,
        );
        // Cable from the base to the wall.
        beam(
            &mut m,
            Vec3::new(base.x, 0.05, base.y),
            Vec3::new(base.x * 1.2, 0.05, lo.y + 0.6),
            0.1,
            black,
        );
    }

    // The link console, right of the deck: a desk, a tilted screen, a cable to the deck.
    let (cx, cz) = (7.2f32, -2.6f32);
    bx(
        &mut m,
        Vec3::new(cx - 0.9, 0.0, cz - 0.45),
        Vec3::new(cx + 0.9, 1.0, cz + 0.45),
        steel,
    );
    let tilt = Quat::from_rotation_y(-0.5) * Quat::from_rotation_x(-0.6);
    obox(
        &mut m,
        Vec3::new(cx, 1.45, cz),
        Vec3::new(1.5, 0.9, 0.08),
        tilt,
        dark,
    );
    obox(
        &mut m,
        Vec3::new(cx, 1.45, cz) + tilt * Vec3::Z * 0.05,
        Vec3::new(1.3, 0.72, 0.02),
        tilt,
        link,
    );
    beam(
        &mut m,
        Vec3::new(cx - 0.9, 0.04, cz),
        Vec3::new(DECK_R + 0.5, 0.04, -0.5),
        0.09,
        black,
    );

    // Racks along the right wall: uprights, shelves, crates and canisters.
    let rx = hi.x - 1.4;
    for z in [-9.5f32, -7.0, -4.5] {
        bx(
            &mut m,
            Vec3::new(rx - 0.05, 0.0, z - 0.05),
            Vec3::new(rx + 0.05, 4.2, z + 0.05),
            steel,
        );
        bx(
            &mut m,
            Vec3::new(hi.x - 0.15, 0.0, z - 0.05),
            Vec3::new(hi.x - 0.05, 4.2, z + 0.05),
            steel,
        );
    }
    for y in [0.15f32, 1.5, 2.85, 4.1] {
        bx(
            &mut m,
            Vec3::new(rx - 0.05, y, -9.55),
            Vec3::new(hi.x - 0.05, y + 0.08, -4.45),
            steel,
        );
    }
    let crates = [
        (Vec3::new(rx + 0.15, 0.23, -9.2), Vec3::new(0.95, 0.8, 1.1)),
        (Vec3::new(rx + 0.2, 0.23, -7.8), Vec3::new(0.9, 0.6, 1.0)),
        (Vec3::new(rx + 0.1, 1.58, -8.6), Vec3::new(1.0, 0.7, 1.4)),
        (Vec3::new(rx + 0.2, 2.93, -6.6), Vec3::new(0.9, 0.55, 0.9)),
        (Vec3::new(rx + 0.15, 0.23, -5.7), Vec3::new(1.0, 1.0, 1.0)),
    ];
    for (at, s) in crates {
        bx(&mut m, at, at + s, wood);
        bx(
            &mut m,
            at + Vec3::new(-0.01, s.y * 0.45, -0.01),
            at + s + Vec3::new(0.01, -s.y * 0.45, 0.01),
            dark,
        );
    }
    for (i, z) in [-7.0f32, -6.55, -6.1, -5.65].into_iter().enumerate() {
        let c = if i % 2 == 0 { hazard } else { steel };
        cyl(&mut m, Vec2::new(rx + 0.55, z), 0.2, 1.58, 2.3, 12, c);
    }

    // Spare parts on the left: a stack of track sections, drums, a road wheel.
    for k in 0..4 {
        let y = k as f32 * 0.22;
        bx(
            &mut m,
            Vec3::new(-11.6, y, -6.0 + k as f32 * 0.05),
            Vec3::new(-8.6, y + 0.18, -5.1 + k as f32 * 0.05),
            col([0.16, 0.15, 0.14], Material::Metal),
        );
        let mut x = -11.5;
        while x < -8.7 {
            bx(
                &mut m,
                Vec3::new(x, y + 0.18, -6.02 + k as f32 * 0.05),
                Vec3::new(x + 0.1, y + 0.21, -5.08 + k as f32 * 0.05),
                dark,
            );
            x += 0.3;
        }
    }
    for (p, c) in [
        (Vec2::new(-11.8, -2.6), rust),
        (Vec2::new(-11.1, -2.1), rust),
        (Vec2::new(-11.75, -1.6), steel),
        (Vec2::new(-10.9, -3.2), hazard),
    ] {
        cyl(&mut m, p, 0.32, 0.0, 0.95, 14, c);
        cyl(&mut m, p, 0.335, 0.3, 0.36, 14, dark);
        cyl(&mut m, p, 0.335, 0.62, 0.68, 14, dark);
    }
    m.append(
        &MeshData::cylinder(18),
        Mat4::from_scale_rotation_translation(
            Vec3::new(1.3, 0.35, 1.3),
            Quat::from_rotation_z(1.3),
            Vec3::new(-9.4, 0.62, -3.6),
        ),
        dark,
    );
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The camera stays inside the room, under the roof, looking at the deck.
    #[test]
    fn camera_is_inside_the_room() {
        for pull in [1.0, 1.25] {
            let c = camera(16.0 / 9.0, pull);
            assert!(c.pos.x > ROOM_MIN.x && c.pos.x < ROOM_MAX.x);
            assert!(c.pos.z > ROOM_MIN.y && c.pos.z < ROOM_MAX.y);
            assert!(c.pos.y > DECK && c.pos.y < ROOF - 1.0);
            assert!((c.target - c.pos).dot(-c.pos) > 0.0);
        }
    }

    /// The sun through the skylight lands on the deck.
    #[test]
    fn sunbeam_lands_on_the_deck() {
        let sun = atmosphere().sun_dir;
        let up = sun * (ROOF / sun.y);
        let at = Vec2::new(up.x, up.z);
        assert!(at.x > SKY_MIN.x && at.x < SKY_MAX.x && at.y > SKY_MIN.y && at.y < SKY_MAX.y);
    }
}
