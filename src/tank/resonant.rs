//! Resonant kit: the Vigil hull. The Resonant don't build tanks; they build a
//! vessel for a crystal, by rite. The crystal sits in a shrine at its heart
//! where an engine would be, blue light runs from it in veins to the running
//! gear, the side plates float off the frame held by that light, and a bell
//! (a relic of some fallen machine) hangs off the back.

use glam::{Vec2, Vec3, Vec4};

use super::model::{
    CARBON, CARBON_LIGHT, CRYSTAL, HullModel, PAINT, Running, Skin, at_x, at_y, at_z, block, ngon,
    plate, round, shard, solid, trap, v,
};
use super::track::Wheel;
use crate::render::material::Material;
use crate::render::mesh::MeshData;

/// Crystal light, for the parts drawn as emissive instances (unlit, color
/// times the instance's glow strength), and its white-hot core.
pub const CRYSTAL_LIGHT: Vec3 = Vec3::new(0.2, 0.5, 1.0);
pub const CRYSTAL_LIGHT_HOT: Vec3 = Vec3::new(0.7, 0.88, 1.0);

/// Old gilt: the shrine, its posts and the bell.
pub(crate) const GILT: Skin = Skin::new(
    Vec4::new(0.5, 0.36, 0.16, Material::Metal.id()),
    Vec4::new(0.72, 0.56, 0.3, Material::Metal.id()),
);

fn light(c: Vec3) -> Skin {
    let c = c.extend(0.0);
    Skin::new(c, c)
}

/// How far the side plates float off the frame, m.
pub const PLATE_GAP: f32 = 0.1;

pub struct VigilModel {
    pub hull: HullModel,
    /// The crystal, its veins and the light bridges: drawn emissive, pulsing,
    /// flickering out on a husk.
    pub light: MeshData,
    /// The floating side plates, [left (+x), right (-x)], at rest.
    pub plates: [MeshData; 2],
    /// The bell, hanging from its pivot at the origin.
    pub bell: MeshData,
    pub bell_pivot: Vec3,
}

/// A square bar from `a` to `b`, `r` across.
pub(crate) fn bar(m: &mut MeshData, a: Vec3, b: Vec3, r: f32, skin: Skin) {
    let d = (b - a).normalize();
    let side = if d.y.abs() > 0.9 { Vec3::X } else { Vec3::Y };
    let u = d.cross(side).normalize() * r * 0.5;
    let w = d.cross(u).normalize() * r * 0.5;
    let ring = |c: Vec3| vec![c - u - w, c + u - w, c + u + w, c - u + w];
    solid(m, &[ring(a), ring(b)], 0.0, skin);
}

pub fn vigil() -> VigilModel {
    let mut m = MeshData::default();
    let inside = v(0.0, 0.8, 0.0);

    // Frame: Lancer's tub and deck, bare dark composite.
    solid(
        &mut m,
        &[
            at_z(&trap(0.74, 0.82, 0.45, 1.02, 0.08), -2.38),
            at_z(&trap(0.86, 0.9, 0.3, 1.1, 0.1), -1.9),
            at_z(&trap(0.86, 0.9, 0.3, 1.1, 0.1), 1.6),
            at_z(&trap(0.66, 0.76, 0.5, 0.92, 0.08), 2.45),
        ],
        0.04,
        CARBON,
    );
    solid(
        &mut m,
        &[
            at_z(&trap(1.2, 1.1, 0.98, 1.3, 0.06), -2.42),
            at_z(&trap(1.3, 1.18, 0.96, 1.4, 0.08), -1.95),
            at_z(&trap(1.3, 1.18, 0.96, 1.4, 0.08), 0.95),
            at_z(&trap(1.16, 0.92, 0.92, 1.25, 0.07), 1.95),
            at_z(&trap(0.8, 0.52, 0.86, 1.02, 0.05), 2.55),
        ],
        0.04,
        CARBON,
    );
    // Ribs over the tracks the plates float off: the frame's bones, showing.
    for z in [-1.9f32, -1.0, -0.1, 0.8, 1.7] {
        for s in [1.0f32, -1.0] {
            let (a, b) = (s * 1.04, s * 1.56);
            block(
                &mut m,
                v(a.min(b), 1.12, z - 0.07),
                v(a.max(b), 1.36, z + 0.07),
                0.02,
                CARBON_LIGHT,
            );
        }
    }

    // Prow: a painted plate like a lectern, the brow above it.
    plate(
        &mut m,
        [
            v(-0.62, 1.4, 1.0),
            v(0.62, 1.4, 1.0),
            v(0.44, 1.1, 2.38),
            v(-0.44, 1.1, 2.38),
        ],
        0.06,
        inside,
        PAINT,
    );
    plate(
        &mut m,
        [
            v(-0.66, 1.0, 2.5),
            v(0.66, 1.0, 2.5),
            v(0.5, 0.62, 2.62),
            v(-0.5, 0.62, 2.62),
        ],
        0.06,
        inside,
        PAINT,
    );

    // The shrine, where an engine would be: a gilt plinth and four posts
    // joined by a frame, open so the crystal shows.
    let (sz0, sz1) = (-2.38, -1.62);
    let sc = (sz0 + sz1) * 0.5;
    block(&mut m, v(-0.6, 1.36, sz0), v(0.6, 1.5, sz1), 0.03, GILT);
    for (x, z) in [
        (0.5, sz0 + 0.06),
        (-0.5, sz0 + 0.06),
        (0.5, sz1 - 0.06),
        (-0.5, sz1 - 0.06),
    ] {
        block(
            &mut m,
            v(x - 0.05, 1.5, z - 0.05),
            v(x + 0.05, 1.92, z + 0.05),
            0.015,
            GILT,
        );
    }
    for (a, b) in [
        (v(0.5, 1.92, sz0 + 0.06), v(0.5, 1.92, sz1 - 0.06)),
        (v(-0.5, 1.92, sz0 + 0.06), v(-0.5, 1.92, sz1 - 0.06)),
        (v(-0.5, 1.92, sz0 + 0.06), v(0.5, 1.92, sz0 + 0.06)),
        (v(-0.5, 1.92, sz1 - 0.06), v(0.5, 1.92, sz1 - 0.06)),
    ] {
        bar(&mut m, a, b, 0.07, GILT);
    }

    // The bell's yoke: an arm out over the stern from the shrine's back.
    let bell_pivot = v(0.0, 1.84, -2.78);
    block(
        &mut m,
        v(-0.05, 1.84, -2.84),
        v(0.05, 1.93, sz0 + 0.02),
        0.015,
        GILT,
    );

    // --- Light ---
    let mut glow = MeshData::default();
    let core = light(CRYSTAL_LIGHT);
    let hot = light(CRYSTAL_LIGHT_HOT);
    // The crystal: one tall shard and two leaning beside it.
    shard(
        &mut glow,
        v(0.0, 1.48, sc),
        v(0.0, 1.0, 0.0),
        0.62,
        0.15,
        hot,
    );
    shard(
        &mut glow,
        v(0.14, 1.48, sc + 0.08),
        v(0.35, 1.0, 0.1),
        0.36,
        0.09,
        core,
    );
    shard(
        &mut glow,
        v(-0.12, 1.48, sc - 0.1),
        v(-0.3, 1.0, -0.15),
        0.34,
        0.09,
        core,
    );
    for s in [1.0f32, -1.0] {
        // Veins from the shrine along the deck, down the prow to the
        // sprockets, and down the stern to the idlers.
        let x = s * 0.86;
        let pts = [
            v(s * 0.55, 1.42, sz1),
            v(x, 1.42, -1.2),
            v(x, 1.42, 0.95),
            v(s * 0.7, 1.26, 1.95),
            v(s * 0.86, 1.0, 2.3),
            v(s * 0.92, 0.66, 2.05),
        ];
        for w in pts.windows(2) {
            bar(&mut glow, w[0], w[1], 0.06, core);
        }
        bar(
            &mut glow,
            v(s * 0.55, 1.4, sz0),
            v(s * 0.8, 1.1, -2.46),
            0.06,
            core,
        );
        bar(
            &mut glow,
            v(s * 0.8, 1.1, -2.46),
            v(s * 0.92, 0.62, -2.1),
            0.06,
            core,
        );
        // Hubs where the light drives the wheels.
        round(
            &mut glow,
            &[
                at_x(&ngon(8, 0.13, Vec2::new(2.05, 0.66)), s * 0.9),
                at_x(&ngon(8, 0.13, Vec2::new(2.05, 0.66)), s * 0.95),
            ],
            0.0,
            hot,
        );
        round(
            &mut glow,
            &[
                at_x(&ngon(8, 0.12, Vec2::new(-2.1, 0.62)), s * 0.9),
                at_x(&ngon(8, 0.12, Vec2::new(-2.1, 0.62)), s * 0.95),
            ],
            0.0,
            hot,
        );
        // Bridges of light holding each plate off its rib.
        for z in [-1.9f32, -1.0, -0.1, 0.8, 1.7] {
            bar(
                &mut glow,
                v(s * 1.5, 1.36, z),
                v(s * (1.5 + PLATE_GAP + 0.05), 1.36 + PLATE_GAP, z),
                0.05,
                core,
            );
        }
    }
    // A line of scripture across the prow.
    let mut z = 1.2;
    let mut k = 0;
    while z < 2.15 {
        let len = [0.12f32, 0.05, 0.08, 0.04, 0.1, 0.06][k % 6];
        let f = (z - 1.0) / 1.38;
        let y = 1.4 + (1.1 - 1.4) * f + 0.035;
        bar(
            &mut glow,
            v(0.0, y, z),
            v(0.0, y - 0.3 * len / 1.38, z + len),
            0.035,
            core,
        );
        z += len + 0.05;
        k += 1;
    }

    // --- Floating plates ---
    let plates = [1.0f32, -1.0].map(|s| {
        let mut p = MeshData::default();
        let g = PLATE_GAP;
        let x = |x: f32| s * (x + g);
        let pin = v(0.0, 0.8, 0.0);
        // Fenders, lifted off the ribs.
        for (z0, z1) in [(-2.3f32, -1.0), (-0.95, 0.65), (0.7, 2.1)] {
            plate(
                &mut p,
                [
                    v(x(0.98), 1.46 + g, z0),
                    v(x(1.66), 1.27 + g, z0),
                    v(x(1.66), 1.27 + g, z1),
                    v(x(0.98), 1.46 + g, z1),
                ],
                0.07,
                pin,
                PAINT,
            );
        }
        // Skirts, standing out from the tracks, edged in scripture.
        for (z0, z1, lo0, lo1) in [
            (-2.3f32, -0.8, 1.0, 0.78),
            (-0.75, 0.75, 0.74, 0.74),
            (0.8, 2.0, 0.78, 1.08),
        ] {
            let sk = [
                v(x(1.66), 1.25, z0),
                v(x(1.66), 1.25, z1),
                v(x(1.74), lo1, z1),
                v(x(1.74), lo0, z0),
            ];
            plate(&mut p, sk, 0.05, pin, PAINT);
            plate(
                &mut p,
                [
                    v(x(1.72), 1.16, z0 + 0.12),
                    v(x(1.72), 1.16, z1 - 0.12),
                    v(x(1.725), 1.12, z1 - 0.12),
                    v(x(1.725), 1.12, z0 + 0.12),
                ],
                0.012,
                pin,
                CRYSTAL,
            );
        }
        p
    });

    // --- The bell ---
    let mut bell = MeshData::default();
    let c = |r: f32| ngon(12, r, Vec2::ZERO);
    round(
        &mut bell,
        &[
            at_y(&c(0.05), -0.02),
            at_y(&c(0.07), -0.08),
            at_y(&c(0.13), -0.12),
            at_y(&c(0.16), -0.26),
            at_y(&c(0.19), -0.38),
            at_y(&c(0.25), -0.46),
            at_y(&c(0.26), -0.5),
        ],
        0.02,
        GILT,
    );
    round(
        &mut bell,
        &[at_y(&c(0.2), -0.5), at_y(&c(0.12), -0.36)],
        0.0,
        CARBON,
    );

    VigilModel {
        hull: HullModel {
            body: m,
            running: Running {
                track_x: 1.22,
                track_width: 0.56,
                sprocket: Wheel {
                    center: Vec2::new(2.05, 0.66),
                    radius: 0.36,
                },
                idler: Wheel {
                    center: Vec2::new(-2.1, 0.62),
                    radius: 0.32,
                },
                road: [-1.35f32, -0.45, 0.45, 1.35]
                    .iter()
                    .map(|&z| Wheel {
                        center: Vec2::new(z, 0.38),
                        radius: 0.3,
                    })
                    .collect(),
            },
            mount: v(0.0, 1.4, -0.3),
        },
        light: glow,
        plates,
        bell,
        bell_pivot,
    }
}
