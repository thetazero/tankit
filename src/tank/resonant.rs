//! Resonant kit: the Vigil hull. The Resonant don't build tanks; they build a
//! vessel for a crystal, by rite. A narrow keel runs between bare tracks; its
//! armor is six plates floating well clear of it on shafts of light; a
//! pointed-arch prow stands at the front like a lectern; and over the stern a
//! gilt arch holds the bell (a relic of some fallen machine), with the crystal
//! floating beneath it, turning.

use glam::{Vec2, Vec3, Vec4};

use super::model::{
    CARBON, CRYSTAL, HullModel, PAINT, Running, Skin, at_x, at_y, at_z, ngon, plate, round, shard,
    solid, trap, v,
};
use super::track::Wheel;
use crate::render::material::Material;
use crate::render::mesh::MeshData;

/// Crystal light, for the parts drawn as emissive instances (unlit, color
/// times the instance's glow strength), and its white-hot core.
pub const CRYSTAL_LIGHT: Vec3 = Vec3::new(0.2, 0.5, 1.0);
pub const CRYSTAL_LIGHT_HOT: Vec3 = Vec3::new(0.7, 0.88, 1.0);

/// Old gilt: the bell, its arch, the trim.
pub(crate) const GILT: Skin = Skin::new(
    Vec4::new(0.5, 0.36, 0.16, Material::Metal.id()),
    Vec4::new(0.72, 0.56, 0.3, Material::Metal.id()),
);

fn light(c: Vec3) -> Skin {
    let c = c.extend(0.0);
    Skin::new(c, c)
}

/// Everything of the Vigil drawn apart from its tracked body.
pub struct VigilExtras {
    /// Veins, the shafts holding the plates, the prow's slit: drawn emissive,
    /// breathing, guttering on a husk.
    pub light: MeshData,
    /// The crystal, around its middle, and where it floats.
    pub heart: MeshData,
    pub heart_at: Vec3,
    /// The floating plates, each built around its own middle, and where that
    /// is; left side (+x) first.
    pub plates: Vec<(MeshData, Vec3)>,
    /// The bell, hanging from its pivot at the origin.
    pub bell: MeshData,
    pub bell_pivot: Vec3,
}

pub struct VigilModel {
    pub hull: HullModel,
    pub extras: VigilExtras,
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

/// The plates' spans along the hull, (from z, to z).
const PLATE_SPANS: [(f32, f32); 3] = [(-2.1, -0.85), (-0.75, 0.55), (0.65, 1.95)];
/// Where a plate floats: its middle's height and distance out from the keel.
const PLATE_Y: f32 = 1.6;
const PLATE_X: f32 = 1.3;

pub fn vigil() -> VigilModel {
    let mut m = MeshData::default();
    let core = light(CRYSTAL_LIGHT);
    let hot = light(CRYSTAL_LIGHT_HOT);
    let mut glow = MeshData::default();

    // The keel: narrow, between the tracks, which it leaves bare.
    solid(
        &mut m,
        &[
            at_z(&trap(0.55, 0.42, 0.5, 1.28, 0.06), -2.38),
            at_z(&trap(0.78, 0.62, 0.38, 1.38, 0.08), -1.9),
            at_z(&trap(0.78, 0.62, 0.38, 1.38, 0.08), 1.6),
            at_z(&trap(0.5, 0.36, 0.55, 1.12, 0.06), 2.42),
        ],
        0.04,
        PAINT,
    );
    // Its belly and flanks in dark composite under the paint.
    solid(
        &mut m,
        &[
            at_z(&trap(0.8, 0.8, 0.36, 0.9, 0.04), -1.95),
            at_z(&trap(0.8, 0.8, 0.36, 0.9, 0.04), 1.65),
        ],
        0.03,
        CARBON,
    );
    // Gilt trim along the deck's edges.
    for s in [1.0f32, -1.0] {
        bar(
            &mut m,
            v(s * 0.62, 1.39, -1.9),
            v(s * 0.62, 1.39, 1.6),
            0.05,
            GILT,
        );
        bar(
            &mut m,
            v(s * 0.62, 1.39, 1.6),
            v(s * 0.36, 1.13, 2.42),
            0.05,
            GILT,
        );
    }

    // The prow: a pointed arch standing at the front, leaning out like a
    // lectern, rimmed in gilt, a slit of light down its face.
    let lean = |p: Vec2, z: f32| Vec3::new(p.x, p.y, z + (p.y - 0.7) * 0.35);
    let arch = [
        Vec2::new(-0.5, 0.7),
        Vec2::new(0.5, 0.7),
        Vec2::new(0.5, 1.25),
        Vec2::new(0.0, 1.6),
        Vec2::new(-0.5, 1.25),
    ];
    solid(
        &mut m,
        &[
            arch.iter().map(|&p| lean(p, 2.28)).collect(),
            arch.iter().map(|&p| lean(p, 2.4)).collect(),
        ],
        0.03,
        PAINT,
    );
    for i in 1..arch.len() {
        let (a, b) = (arch[i], arch[(i + 1) % arch.len()]);
        bar(&mut m, lean(a, 2.42), lean(b, 2.42), 0.06, GILT);
    }
    bar(
        &mut glow,
        lean(Vec2::new(0.0, 0.85), 2.43),
        lean(Vec2::new(0.0, 1.42), 2.43),
        0.05,
        hot,
    );

    // Over the stern, the bell's arch: two gilt posts and a pointed top.
    let tz = -2.15;
    for s in [1.0f32, -1.0] {
        bar(&mut m, v(s * 0.5, 1.3, tz), v(s * 0.5, 2.35, tz), 0.1, GILT);
        bar(&mut m, v(s * 0.5, 2.35, tz), v(0.0, 2.78, tz), 0.09, GILT);
    }
    let bell_pivot = v(0.0, 2.64, tz);
    // A gilt bowl the crystal floats over.
    let bowl = |r: f32, y: f32| at_y(&ngon(8, r, Vec2::new(0.0, tz)), y);
    round(
        &mut m,
        &[bowl(0.16, 1.36), bowl(0.24, 1.44), bowl(0.26, 1.5)],
        0.02,
        GILT,
    );
    let heart_at = v(0.0, 1.82, tz);

    // The crystal: one tall shard and two leaning off it, around its middle.
    let mut heart = MeshData::default();
    shard(
        &mut heart,
        v(0.0, -0.3, 0.0),
        v(0.0, 1.0, 0.0),
        0.62,
        0.15,
        hot,
    );
    shard(
        &mut heart,
        v(0.1, -0.22, 0.06),
        v(0.35, 1.0, 0.1),
        0.38,
        0.09,
        core,
    );
    shard(
        &mut heart,
        v(-0.1, -0.22, -0.08),
        v(-0.3, 1.0, -0.15),
        0.36,
        0.09,
        core,
    );
    shard(
        &mut heart,
        v(0.0, -0.22, 0.0),
        v(0.0, -1.0, 0.0),
        0.2,
        0.12,
        core,
    );

    for s in [1.0f32, -1.0] {
        // Veins from the crystal's bowl down the keel's flanks to the
        // sprockets and idlers: what drives the tracks.
        let side = s * 0.74;
        for w in [
            [v(s * 0.2, 1.38, tz + 0.2), v(side, 1.1, -1.7)],
            [v(side, 1.1, -1.7), v(side, 1.0, 1.5)],
            [v(side, 1.0, 1.5), v(s * 0.92, 0.66, 2.05)],
            [v(side, 1.1, -1.7), v(s * 0.92, 0.62, -2.1)],
        ] {
            bar(&mut glow, w[0], w[1], 0.06, core);
        }
        for (z, y, r) in [(2.05f32, 0.66f32, 0.13f32), (-2.1, 0.62, 0.12)] {
            round(
                &mut glow,
                &[
                    at_x(&ngon(8, r, Vec2::new(z, y)), s * 0.9),
                    at_x(&ngon(8, r, Vec2::new(z, y)), s * 0.95),
                ],
                0.0,
                hot,
            );
        }
        // Shafts of light holding each plate up off the keel.
        for &(z0, z1) in &PLATE_SPANS {
            for z in [z0 + 0.3, z1 - 0.3] {
                bar(
                    &mut glow,
                    v(s * 0.78, 1.12, z),
                    v(s * (PLATE_X - 0.15), PLATE_Y - 0.08, z),
                    0.08,
                    core,
                );
            }
        }
    }

    // The plates: shields floating over the bare tracks, tipped outward,
    // each with a drop at its outer edge lined with scripture.
    let mut plates = Vec::new();
    for s in [1.0f32, -1.0] {
        for &(z0, z1) in &PLATE_SPANS {
            let mut p = MeshData::default();
            let h = (z1 - z0) * 0.5;
            let below = v(0.0, -0.6, 0.0);
            plate(
                &mut p,
                [
                    v(-s * 0.4, 0.1, -h),
                    v(s * 0.38, -0.1, -h),
                    v(s * 0.38, -0.1, h),
                    v(-s * 0.4, 0.1, h),
                ],
                0.07,
                below,
                PAINT,
            );
            let inward = v(0.0, -0.3, 0.0) - v(s * 0.5, 0.0, 0.0);
            plate(
                &mut p,
                [
                    v(s * 0.4, -0.08, -h + 0.04),
                    v(s * 0.4, -0.08, h - 0.04),
                    v(s * 0.45, -0.28, h - 0.1),
                    v(s * 0.45, -0.28, -h + 0.1),
                ],
                0.05,
                inward,
                PAINT,
            );
            plate(
                &mut p,
                [
                    v(s * 0.445, -0.16, -h + 0.18),
                    v(s * 0.445, -0.16, h - 0.18),
                    v(s * 0.45, -0.2, h - 0.18),
                    v(s * 0.45, -0.2, -h + 0.18),
                ],
                0.012,
                inward,
                CRYSTAL,
            );
            // A gilt boss where the light holds it.
            round(
                &mut p,
                &[
                    at_y(&ngon(6, 0.09, Vec2::new(0.0, 0.0)), 0.05),
                    at_y(&ngon(6, 0.06, Vec2::new(0.0, 0.0)), 0.1),
                ],
                0.01,
                GILT,
            );
            plates.push((p, v(s * PLATE_X, PLATE_Y, (z0 + z1) * 0.5)));
        }
    }

    // The bell.
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
            heat_light: MeshData::default(),
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
            mount: v(0.0, 1.38, -0.3),
        },
        extras: VigilExtras {
            light: glow,
            heart,
            heart_at,
            plates,
            bell,
            bell_pivot,
        },
    }
}
