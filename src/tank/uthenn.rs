//! Uthenn kit: the Gravis hull. Big, bulky and slow: a smooth pale shell
//! over long running gear (seven road wheels a side, a high sprocket), a
//! tall raked citadel stepped up on top of it carrying the turret, armored
//! pods over the tracks, a chisel prow with a green visor, and a bank of
//! radiator fins on the rear deck behind the citadel.
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
    // Inside the citadel, for what's on its faces.
    let core = v(0.0, 2.3, -0.6);

    // Lower tub between the tracks.
    solid(
        &mut m,
        &[
            at_z(&trap(1.0, 1.08, 0.5, 1.4, 0.08), -3.4),
            at_z(&trap(1.1, 1.12, 0.36, 1.45, 0.1), -2.9),
            at_z(&trap(1.1, 1.12, 0.36, 1.45, 0.1), 2.9),
            at_z(&trap(1.0, 1.08, 0.5, 1.3, 0.08), 3.5),
        ],
        0.04,
        CARBON,
    );
    // The lower shell: one smooth mass the full width of the tracks, a deck
    // at the back for the radiators, sloping down at the front into a long
    // glacis and a chisel prow.
    solid(
        &mut m,
        &[
            at_z(&trap(1.85, 1.7, 1.42, 1.95, 0.12), -3.68),
            at_z(&trap(2.1, 2.0, 1.38, 2.05, 0.16), -3.35),
            at_z(&trap(2.1, 2.0, 1.38, 2.05, 0.16), 1.8),
            at_z(&trap(2.1, 1.9, 1.34, 1.8, 0.14), 3.1),
            at_z(&trap(1.8, 1.5, 1.2, 1.5, 0.08), 3.75),
        ],
        0.05,
        SHELL,
    );
    // The citadel: a second, narrower tier standing on the shell, its sides
    // raked in and its brow sloping down to the glacis. The turret sits on it.
    solid(
        &mut m,
        &[
            at_z(&trap(1.55, 1.3, 1.95, 2.7, 0.1), -2.45),
            at_z(&trap(1.65, 1.4, 1.95, 2.8, 0.14), -2.2),
            at_z(&trap(1.65, 1.4, 1.95, 2.8, 0.14), 0.9),
            at_z(&trap(1.6, 1.2, 1.95, 2.25, 0.1), 1.7),
        ],
        0.05,
        SHELL,
    );
    // The brow: a dark band across the citadel's sloping front, a green
    // slit in it.
    plate(
        &mut m,
        [
            v(-1.1, 2.67, 1.15),
            v(1.1, 2.67, 1.15),
            v(1.15, 2.46, 1.45),
            v(-1.15, 2.46, 1.45),
        ],
        0.04,
        core,
        SHELL_DARK,
    );
    plate(
        &mut light,
        [
            v(-0.8, 2.62, 1.25),
            v(0.8, 2.62, 1.25),
            v(0.8, 2.565, 1.33),
            v(-0.8, 2.565, 1.33),
        ],
        0.015,
        core,
        HEAT,
    );
    // A dark band down the glacis, from the citadel to the prow, lying on
    // the shell, with a green seam along each edge.
    plate(
        &mut m,
        [
            v(-0.5, 2.09, 1.8),
            v(0.5, 2.09, 1.8),
            v(0.42, 1.84, 3.1),
            v(-0.42, 1.84, 3.1),
        ],
        0.04,
        inside,
        SHELL_DARK,
    );
    plate2(
        &mut light,
        [
            v(1.5, 2.08, 1.85),
            v(1.56, 2.08, 1.85),
            v(1.56, 1.83, 3.1),
            v(1.5, 1.83, 3.1),
        ],
        0.015,
        inside,
        HEAT,
    );
    // The visor: a dark band across the prow's face with a green bar in it.
    plate(
        &mut m,
        [
            v(-1.45, 1.46, 3.77),
            v(1.45, 1.46, 3.77),
            v(1.6, 1.24, 3.77),
            v(-1.6, 1.24, 3.77),
        ],
        0.03,
        inside,
        SHELL_DARK,
    );
    plate(
        &mut light,
        [
            v(-1.15, 1.38, 3.79),
            v(1.15, 1.38, 3.79),
            v(1.15, 1.32, 3.79),
            v(-1.15, 1.32, 3.79),
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
            v(2.12, 1.42, -3.5),
            v(2.12, 1.42, 3.3),
            v(2.18, 0.75, 3.1),
            v(2.18, 0.75, -3.35),
        ],
        0.1,
        inside,
        SHELL,
    );
    plate2(
        &mut light,
        [
            v(2.2, 1.12, -2.9),
            v(2.2, 1.12, 2.7),
            v(2.205, 1.07, 2.7),
            v(2.205, 1.07, -2.9),
        ],
        0.015,
        inside,
        HEAT,
    );
    // Dark inset panels on the pods, between the seam and the shell.
    for i in 0..4 {
        let z0 = -3.0 + i as f32 * 1.55;
        plate2(
            &mut m,
            [
                v(2.17, 1.37, z0),
                v(2.17, 1.37, z0 + 1.25),
                v(2.19, 1.22, z0 + 1.25),
                v(2.19, 1.22, z0),
            ],
            0.02,
            inside,
            SHELL_DARK,
        );
    }
    // Vents in the citadel's raked flanks: five slanted green slots a side.
    for k in 0..5 {
        let z = -1.75 + k as f32 * 0.42;
        plate2(
            &mut light,
            [
                v(1.475, 2.62, z),
                v(1.475, 2.62, z + 0.18),
                v(1.62, 2.12, z + 0.06),
                v(1.62, 2.12, z - 0.12),
            ],
            0.02,
            core,
            HEAT,
        );
    }
    // Dark armor cheeks on the shell's shoulders beside the citadel.
    plate2(
        &mut m,
        [
            v(1.72, 2.09, -2.0),
            v(1.72, 2.09, 1.4),
            v(2.0, 2.09, 1.25),
            v(2.0, 2.09, -1.85),
        ],
        0.04,
        inside,
        SHELL_DARK,
    );

    // Turret ring plinth, standing proud of the citadel's roof (flush, its
    // top would z-fight with the roof).
    round(
        &mut m,
        &[at_y_ring(1.15, 2.76), at_y_ring(1.1, 2.84)],
        0.02,
        CARBON_LIGHT,
    );

    // Radiators: a bank of fins across the rear deck behind the citadel,
    // over a green-lit well, the heat it soaks up showing between them.
    plate(
        &mut light,
        [
            v(-1.5, 2.07, -3.5),
            v(1.5, 2.07, -3.5),
            v(1.5, 2.07, -2.5),
            v(-1.5, 2.07, -2.5),
        ],
        0.02,
        inside - Vec3::Y,
        HEAT,
    );
    for k in 0..7 {
        let z = -3.45 + k as f32 * 0.14;
        block(
            &mut m,
            v(-1.45, 2.06, z),
            v(1.45, 2.42, z + 0.05),
            0.01,
            METAL,
        );
    }
    block2(
        &mut m,
        v(1.5, 2.04, -3.58),
        v(1.68, 2.46, -2.45),
        0.03,
        SHELL_DARK,
    );
    // The citadel's back: a dark panel over the radiators.
    plate(
        &mut m,
        [
            v(-1.1, 2.6, -2.47),
            v(1.1, 2.6, -2.47),
            v(1.2, 2.15, -2.47),
            v(-1.2, 2.15, -2.47),
        ],
        0.03,
        core,
        SHELL_DARK,
    );
    // Rear: a dark tail plate with two green running lights.
    plate(
        &mut m,
        [
            v(-1.6, 1.85, -3.69),
            v(1.6, 1.85, -3.69),
            v(1.6, 1.45, -3.69),
            v(-1.6, 1.45, -3.69),
        ],
        0.04,
        inside,
        SHELL_DARK,
    );
    for s in [-1.0f32, 1.0] {
        plate(
            &mut light,
            [
                v(s * 0.8, 1.7, -3.71),
                v(s * 1.4, 1.7, -3.71),
                v(s * 1.4, 1.64, -3.71),
                v(s * 0.8, 1.64, -3.71),
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
            track_x: 1.6,
            track_width: 0.86,
            // A high sprocket at the back, so the track climbs to it.
            sprocket: Wheel {
                center: Vec2::new(-3.45, 0.85),
                radius: 0.46,
            },
            idler: Wheel {
                center: Vec2::new(3.5, 0.82),
                radius: 0.42,
            },
            road: [-2.7f32, -1.8, -0.9, 0.0, 0.9, 1.8, 2.7]
                .iter()
                .map(|&z| Wheel {
                    center: Vec2::new(z, 0.46),
                    radius: 0.38,
                })
                .collect(),
        },
    }
}

/// Where the turret ring sits along the hull: on the citadel, back of center.
const MOUNT_Z: f32 = -0.4;

/// A ring of radius `r` around the turret ring's axis, at height `y`.
fn at_y_ring(r: f32, y: f32) -> Vec<Vec3> {
    super::model::at_y(&ngon(16, r, Vec2::new(0.0, MOUNT_Z)), y)
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
        barrels: vec![Vec3::ZERO],
        muzzle: crate::sim::TurretKind::Lumen.reach(),
        rig: Rig::Recoil,
        recoil: 0.0,
    }
}
