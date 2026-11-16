//! Uthenn kit: the Gravis hull. Big, bulky and slow: one smooth pale shell
//! over the whole running gear, armored pods over the tracks, a chisel prow
//! with a green visor, and a bank of radiator fins across the rear deck.
//! Green light runs in its seams.

use glam::{Vec2, Vec3, Vec4};

use super::model::{
    CARBON, CARBON_LIGHT, HullModel, LASER_GLOW, METAL, Running, Skin, at_z, block, block2, ngon,
    plate, plate2, round, solid, trap, v,
};
use super::track::Wheel;
use crate::render::material::Material;
use crate::render::mesh::MeshData;

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
        &mut m,
        [
            v(1.3, 1.98, 1.25),
            v(1.36, 1.98, 1.25),
            v(1.36, 1.64, 2.6),
            v(1.3, 1.64, 2.6),
        ],
        0.015,
        inside,
        LASER_GLOW,
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
        &mut m,
        [
            v(-1.0, 1.12, 3.19),
            v(1.0, 1.12, 3.19),
            v(1.0, 1.06, 3.19),
            v(-1.0, 1.06, 3.19),
        ],
        0.015,
        inside,
        LASER_GLOW,
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
        &mut m,
        [
            v(2.0, 0.98, -2.4),
            v(2.0, 0.98, 2.2),
            v(2.005, 0.93, 2.2),
            v(2.005, 0.93, -2.4),
        ],
        0.015,
        inside,
        LASER_GLOW,
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
            &mut m,
            [
                v(1.83, 1.72, z),
                v(1.83, 1.72, z + 0.18),
                v(1.87, 1.36, z + 0.06),
                v(1.87, 1.36, z - 0.12),
            ],
            0.02,
            inside,
            LASER_GLOW,
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
        &mut m,
        [
            v(-1.25, 1.96, -2.85),
            v(1.25, 1.96, -2.85),
            v(1.25, 1.96, -1.45),
            v(-1.25, 1.96, -1.45),
        ],
        0.02,
        inside - Vec3::Y,
        LASER_GLOW,
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
            &mut m,
            [
                v(s * 0.7, 1.6, -3.13),
                v(s * 1.25, 1.6, -3.13),
                v(s * 1.25, 1.54, -3.13),
                v(s * 0.7, 1.54, -3.13),
            ],
            0.015,
            inside,
            LASER_GLOW,
        );
    }

    HullModel {
        body: m,
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
