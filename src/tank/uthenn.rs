//! Uthenn kit: the Gravis hull. Big, bulky and slow: one smooth pale shell
//! over the whole running gear, armored pods over the tracks, a chisel prow
//! with a green visor, and a bank of radiator fins across the rear deck.
//! Green light runs in its seams. And the Lumen turret, in the same style.

use glam::{Vec2, Vec3, Vec4};

use super::model::{
    CARBON, CARBON_LIGHT, HullModel, METAL, Rig, Running, Skin, TurretModel, at_x, at_z, block,
    block2, ngon, plate, plate2, rect, round, solid, trap, turret_ring, v,
};
use super::track::Wheel;
use crate::render::material::Material;
use crate::render::mesh::MeshData;

/// Uthenn laser light: the beam, the muzzle flare, and their glow when cool.
pub const LASER_LIGHT: Vec3 = Vec3::new(0.2, 1.0, 0.35);
/// The heat lights at rest (deeper) and near the limit (full, saturated
/// green: pushing it toward white just reads as faded in the desert sun).
const HEAT_COOL: Vec3 = Vec3::new(0.06, 0.55, 0.18);
const HEAT_HOT: Vec3 = Vec3::new(0.25, 1.0, 0.38);
/// Heat lights are white in the mesh; the instance tints them (see `heat_glow`).
const HEAT: Skin = Skin::new(Vec4::new(1.0, 1.0, 1.0, 0.0), Vec4::new(1.0, 1.0, 1.0, 0.0));
/// Lumen is built a size up from the other turrets.
const LUMEN_SCALE: f32 = 1.2;

/// The tint (rgb) and glow strength (a) of a tank's heat lights: deep green
/// at rest, brighter as the beam heats, guttering while it's locked
/// out hot, dark on a wreck.
pub fn heat_glow(heat: f32, overheated: bool, dead: bool, time: f32) -> Vec4 {
    if dead {
        return Vec4::new(0.05, 0.07, 0.06, 0.0);
    }
    let h = heat.clamp(0.0, 1.0);
    let color = HEAT_COOL.lerp(HEAT_HOT, h);
    let mut strength = 1.6 + 0.8 * h * h;
    if overheated {
        let flick = ((time * 31.0).sin() * (time * 17.0).sin()).abs();
        strength *= 0.25 + 0.75 * flick;
    }
    color.extend(strength.max(1.0))
}

/// The shell: pale, cool grey ceramic, clean to the edges.
pub(crate) const SHELL: Skin = Skin::new(
    Vec4::new(0.6, 0.64, 0.64, Material::Metal.id()),
    Vec4::new(0.72, 0.76, 0.76, Material::Metal.id()),
);
/// The darker panels set into it.
pub(crate) const SHELL_DARK: Skin = Skin::new(
    Vec4::new(0.2, 0.23, 0.24, Material::Metal.id()),
    Vec4::new(0.3, 0.33, 0.34, Material::Metal.id()),
);

pub fn gravis() -> HullModel {
    let mut m = MeshData::default();
    let mut light = MeshData::default();
    let inside = v(0.0, 1.0, 0.0);

    // Lower tub between the tracks.
    solid(
        &mut m,
        &[
            at_z(&trap(0.9, 0.98, 0.42, 1.25, 0.08), -2.95),
            at_z(&trap(1.0, 1.04, 0.32, 1.3, 0.1), -2.5),
            at_z(&trap(1.0, 1.04, 0.32, 1.3, 0.1), 2.4),
            at_z(&trap(0.9, 0.98, 0.45, 1.2, 0.08), 2.95),
        ],
        0.04,
        CARBON,
    );
    // The shell: one smooth mass the full width of the tracks, sloping down
    // into a long glacis and a chisel prow.
    solid(
        &mut m,
        &[
            at_z(&trap(1.62, 1.45, 1.2, 1.8, 0.12), -3.1),
            at_z(&trap(1.9, 1.74, 1.14, 1.95, 0.16), -2.75),
            at_z(&trap(1.9, 1.74, 1.14, 1.95, 0.16), 1.2),
            at_z(&trap(1.88, 1.5, 1.1, 1.62, 0.12), 2.55),
            at_z(&trap(1.6, 1.0, 1.0, 1.18, 0.08), 3.15),
        ],
        0.05,
        SHELL,
    );
    // A dark band down the glacis, from the turret ring to the prow, lying
    // on the shell, with a green seam along each edge.
    plate(
        &mut m,
        [
            v(-0.42, 1.99, 1.0),
            v(0.42, 1.99, 1.0),
            v(0.36, 1.66, 2.55),
            v(-0.36, 1.66, 2.55),
        ],
        0.04,
        inside,
        SHELL_DARK,
    );
    plate2(
        &mut light,
        [
            v(1.3, 1.98, 1.25),
            v(1.36, 1.98, 1.25),
            v(1.36, 1.64, 2.6),
            v(1.3, 1.64, 2.6),
        ],
        0.015,
        inside,
        HEAT,
    );
    // The visor: a dark band across the prow's face with a green bar in it.
    plate(
        &mut m,
        [
            v(-1.3, 1.17, 3.17),
            v(1.3, 1.17, 3.17),
            v(1.4, 1.01, 3.17),
            v(-1.4, 1.01, 3.17),
        ],
        0.03,
        inside,
        SHELL_DARK,
    );
    plate(
        &mut light,
        [
            v(-1.0, 1.12, 3.19),
            v(1.0, 1.12, 3.19),
            v(1.0, 1.06, 3.19),
            v(-1.0, 1.06, 3.19),
        ],
        0.015,
        inside,
        HEAT,
    );

    // Track pods: a sloped armor skirt down each side, with a green seam
    // along its length.
    plate2(
        &mut m,
        [
            v(1.92, 1.2, -2.95),
            v(1.92, 1.2, 2.7),
            v(1.98, 0.6, 2.55),
            v(1.98, 0.6, -2.85),
        ],
        0.1,
        inside,
        SHELL,
    );
    plate2(
        &mut light,
        [
            v(2.0, 0.98, -2.4),
            v(2.0, 0.98, 2.2),
            v(2.005, 0.93, 2.2),
            v(2.005, 0.93, -2.4),
        ],
        0.015,
        inside,
        HEAT,
    );
    // Dark inset panels on the pods, between the seam and the shell.
    for i in 0..3 {
        let z0 = -2.5 + i as f32 * 1.6;
        plate2(
            &mut m,
            [
                v(1.97, 1.15, z0),
                v(1.97, 1.15, z0 + 1.3),
                v(1.99, 1.04, z0 + 1.3),
                v(1.99, 1.04, z0),
            ],
            0.02,
            inside,
            SHELL_DARK,
        );
    }
    // Side vents in the shell's flanks: three slanted green slots a side.
    for k in 0..3 {
        let z = -0.9 + k as f32 * 0.42;
        plate2(
            &mut light,
            [
                v(1.83, 1.72, z),
                v(1.83, 1.72, z + 0.18),
                v(1.87, 1.36, z + 0.06),
                v(1.87, 1.36, z - 0.12),
            ],
            0.02,
            inside,
            HEAT,
        );
    }

    // Turret ring plinth.
    round(
        &mut m,
        &[at_y_ring(1.15, 1.94), at_y_ring(1.1, 2.0)],
        0.02,
        CARBON_LIGHT,
    );

    // Radiators: a bank of fins across the rear deck over a green-lit well,
    // the heat it soaks up showing between them.
    plate(
        &mut light,
        [
            v(-1.25, 1.96, -2.85),
            v(1.25, 1.96, -2.85),
            v(1.25, 1.96, -1.45),
            v(-1.25, 1.96, -1.45),
        ],
        0.02,
        inside - Vec3::Y,
        HEAT,
    );
    for k in 0..9 {
        let z = -2.8 + k as f32 * 0.16;
        block(
            &mut m,
            v(-1.2, 1.96, z),
            v(1.2, 2.24, z + 0.05),
            0.01,
            METAL,
        );
    }
    block2(
        &mut m,
        v(1.25, 1.94, -2.9),
        v(1.42, 2.28, -1.4),
        0.03,
        SHELL_DARK,
    );
    // Rear: a dark tail plate with two green running lights.
    plate(
        &mut m,
        [
            v(-1.4, 1.75, -3.11),
            v(1.4, 1.75, -3.11),
            v(1.4, 1.3, -3.11),
            v(-1.4, 1.3, -3.11),
        ],
        0.04,
        inside,
        SHELL_DARK,
    );
    for s in [-1.0f32, 1.0] {
        plate(
            &mut light,
            [
                v(s * 0.7, 1.6, -3.13),
                v(s * 1.25, 1.6, -3.13),
                v(s * 1.25, 1.54, -3.13),
                v(s * 0.7, 1.54, -3.13),
            ],
            0.015,
            inside,
            HEAT,
        );
    }

    HullModel {
        body: m,
        heat_light: light,
        running: Running {
            track_x: 1.45,
            track_width: 0.78,
            sprocket: Wheel {
                center: Vec2::new(-2.85, 0.76),
                radius: 0.42,
            },
            idler: Wheel {
                center: Vec2::new(2.85, 0.74),
                radius: 0.38,
            },
            road: [-2.2f32, -1.32, -0.44, 0.44, 1.32, 2.2]
                .iter()
                .map(|&z| Wheel {
                    center: Vec2::new(z, 0.42),
                    radius: 0.34,
                })
                .collect(),
        },
        mount: v(0.0, 2.0, -0.1),
    }
}

/// A ring of radius `r` around the turret ring's axis, at height `y`.
fn at_y_ring(r: f32, y: f32) -> Vec<Vec3> {
    super::model::at_y(&ngon(16, r, Vec2::new(0.0, -0.1)), y)
}

/// Uthenn laser: one faceted housing in the hull's pale shell, sloping down
/// to the gun, green seams along its shoulders, radiator fins over a
/// green-lit well at its back, and a smooth focusing rod ringed in green
/// ending in the lens.
pub fn lumen() -> TurretModel {
    let mut head = MeshData::default();
    let mut head_light = MeshData::default();
    let mut barrel_light = MeshData::default();
    let inside = v(0.0, 0.42, -0.2);
    turret_ring(&mut head);
    let sec = |hw: f32, hh: f32, y: f32| rect(Vec2::new(0.0, y), Vec2::new(hw, hh), hh * 0.55);
    solid(
        &mut head,
        &[
            at_z(&sec(0.6, 0.27, 0.42), -1.3),
            at_z(&sec(0.74, 0.36, 0.44), -0.95),
            at_z(&sec(0.74, 0.36, 0.44), 0.35),
            at_z(&sec(0.52, 0.24, 0.42), 1.0),
        ],
        0.03,
        SHELL,
    );
    // Dark spine down the top.
    plate(
        &mut head,
        [
            v(-0.22, 0.81, -0.95),
            v(0.22, 0.81, -0.95),
            v(0.22, 0.81, 0.35),
            v(-0.22, 0.81, 0.35),
        ],
        0.03,
        inside,
        SHELL_DARK,
    );
    // Green seams along the shoulders, from the back to where it slopes.
    plate2(
        &mut head_light,
        [
            v(0.5, 0.79, -0.9),
            v(0.5, 0.79, 0.3),
            v(0.55, 0.77, 0.3),
            v(0.55, 0.77, -0.9),
        ],
        0.012,
        inside,
        HEAT,
    );
    // Side slots, like the hull's.
    for k in 0..2 {
        let z = -0.55 + k as f32 * 0.3;
        plate2(
            &mut head_light,
            [
                v(0.76, 0.6, z),
                v(0.76, 0.6, z + 0.12),
                v(0.76, 0.3, z + 0.04),
                v(0.76, 0.3, z - 0.08),
            ],
            0.012,
            inside,
            HEAT,
        );
    }
    // The back: a green-lit well behind a stack of radiator fins.
    plate(
        &mut head_light,
        [
            v(-0.5, 0.62, -1.31),
            v(0.5, 0.62, -1.31),
            v(0.5, 0.22, -1.31),
            v(-0.5, 0.22, -1.31),
        ],
        0.012,
        inside,
        HEAT,
    );
    for k in 0..5 {
        let y = 0.24 + k as f32 * 0.085;
        block(
            &mut head,
            v(-0.54, y, -1.42),
            v(0.54, y + 0.035, -1.28),
            0.008,
            METAL,
        );
    }
    block2(
        &mut head,
        v(0.54, 0.18, -1.44),
        v(0.6, 0.68, -1.26),
        0.015,
        SHELL_DARK,
    );

    let mut mantlet = MeshData::default();
    solid(
        &mut mantlet,
        &[
            at_z(&sec(0.28, 0.2, 0.0), -0.3),
            at_z(&sec(0.24, 0.17, 0.0), 0.4),
        ],
        0.03,
        SHELL_DARK,
    );
    round(
        &mut mantlet,
        &[
            at_x(&ngon(10, 0.1, Vec2::ZERO), -0.36),
            at_x(&ngon(10, 0.1, Vec2::ZERO), 0.36),
        ],
        0.02,
        CARBON,
    );

    let mut barrel = MeshData::default();
    // A sleeve narrowing into the rod.
    round(
        &mut barrel,
        &[
            at_z(&ngon(10, 0.17, Vec2::ZERO), 0.3),
            at_z(&ngon(10, 0.15, Vec2::ZERO), 1.0),
            at_z(&ngon(10, 0.09, Vec2::ZERO), 1.25),
        ],
        0.02,
        SHELL,
    );
    round(
        &mut barrel,
        &[
            at_z(&ngon(10, 0.075, Vec2::ZERO), 1.2),
            at_z(&ngon(10, 0.075, Vec2::ZERO), 2.9),
        ],
        0.0,
        SHELL_DARK,
    );
    // Focusing rings, glowing.
    for z in [1.55f32, 1.95, 2.35] {
        round(
            &mut barrel_light,
            &[
                at_z(&ngon(10, 0.1, Vec2::ZERO), z),
                at_z(&ngon(10, 0.1, Vec2::ZERO), z + 0.06),
            ],
            0.01,
            HEAT,
        );
    }
    // Lens housing and the green lens.
    round(
        &mut barrel,
        &[
            at_z(&ngon(10, 0.12, Vec2::ZERO), 2.82),
            at_z(&ngon(10, 0.17, Vec2::ZERO), 3.0),
            at_z(&ngon(10, 0.17, Vec2::ZERO), 3.15),
        ],
        0.02,
        SHELL,
    );
    round(
        &mut barrel_light,
        &[
            at_z(&ngon(10, 0.115, Vec2::ZERO), 3.1),
            at_z(&ngon(10, 0.115, Vec2::ZERO), 3.16),
        ],
        0.0,
        HEAT,
    );

    // A size up from the kit's other turrets, to sit on the Gravis.
    for m in [
        &mut head,
        &mut head_light,
        &mut mantlet,
        &mut barrel,
        &mut barrel_light,
    ] {
        for vert in &mut m.verts {
            vert.pos = (Vec3::from(vert.pos) * LUMEN_SCALE).to_array();
        }
    }

    TurretModel {
        head,
        head_light,
        barrel_light,
        mantlet,
        barrel,
        pivot: v(0.0, 0.42, 0.92) * LUMEN_SCALE,
        barrels: vec![Vec3::ZERO],
        muzzle: 3.2 * LUMEN_SCALE,
        rig: Rig::Recoil,
        recoil: 0.0,
    }
}
