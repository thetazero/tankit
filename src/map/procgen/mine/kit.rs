//! What every style's mine is made with, so it plays the same whoever's it
//! is: the colliders, loads and lights are here, what's drawn on them is the
//! style's. A rail car's body, a lamp's bracket, a borer's stand.

use glam::{Quat, Vec2, Vec3};

use super::dig::{Dig, MESA_TOP, ORE, RAW, STEEL, heap_height, ore_heap, ore_lump, own_rng};
use super::yaw_along;
use crate::map::landmarks::Rig;
use crate::map::{Beam, Map, OBox};
use crate::render::PointLight;
use crate::render::material::Material;
use crate::render::mesh::MeshData;
use crate::rng::Rng;
use crate::tank::model::{self, Skin};

/// How bright a lamp's light is, and how far it reaches (m).
pub(super) const LAMP_BRIGHT: f32 = 4.5;
pub(super) const LAMP_REACH: f32 = 10.0;

/// A style's rail cars, each drawn in the car's frame: its length along z,
/// rails at y = 0.
pub(super) struct Freight {
    /// A bogie centered this far along the car.
    pub bogie: fn(&mut MeshData, f32),
    /// An ore car's body, inside x 1.75, y 1.1 to 3.8, z 4.3; with the bed
    /// the load heaps on if it's loaded.
    pub tub: fn(&mut MeshData, bool),
    /// The locomotive's body, inside x 1.75, y 1.1 to 4, z -6.5 to 5.9; over
    /// it, its cab, inside x 1, y 4.9, z -4.6 to -0.2.
    pub loco: fn(&mut MeshData),
    /// Half the load's width, this far along the tub (4 m each way).
    pub load: fn(f32) -> f32,
}

/// A skin of `c`, its edges worn brighter.
pub(super) fn skin(c: Vec3, m: Material) -> Skin {
    let face = c.extend(m.id());
    if m == Material::Glow {
        return Skin::new(face, face);
    }
    Skin::new(face, (c * 1.45).extend(m.id()))
}

/// A box about `c`, `h` its half size.
pub(super) fn slab(m: &mut MeshData, c: Vec3, h: Vec3, skin: Skin) {
    model::block(m, c - h, c + h, h.min_element() * 0.25, skin);
}

/// A round bar along z, `r` its radius at each of its stations.
pub(super) fn turned(m: &mut MeshData, c: Vec2, stations: &[(f32, f32)], skin: Skin) {
    let rings: Vec<Vec<Vec3>> = stations
        .iter()
        .map(|&(z, r)| model::at_z(&model::ngon(12, r, c), z))
        .collect();
    model::round(m, &rings, 0.02, skin);
}

/// An upright round post at `c` (x, z), `r` its radius at each height.
pub(super) fn post(m: &mut MeshData, c: Vec2, stations: &[(f32, f32)], skin: Skin) {
    let rings: Vec<Vec<Vec3>> = stations
        .iter()
        .map(|&(y, r)| model::at_y(&model::ngon(8, r, c), y))
        .collect();
    model::round(m, &rings, 0.0, skin);
}

/// A bar from `a` to `b`, `r` half as thick.
pub(super) fn bar(m: &mut MeshData, a: Vec3, b: Vec3, r: f32, skin: Skin) {
    let d = (b - a).normalize();
    let side = d.any_orthonormal_vector();
    let up = d.cross(side);
    let ring = |p: Vec3| {
        [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)]
            .map(|(u, v)| p + (side * u + up * v) * r)
            .to_vec()
    };
    model::round(m, &[ring(a), ring(b)], 0.0, skin);
}

/// Four wheels on their axles, under a car at `z`.
pub(super) fn wheels(m: &mut MeshData, z: f32, tyre: Skin, hub: Skin) {
    for dz in [-0.9f32, 0.9] {
        let c = Vec2::new(z + dz, 0.6);
        let axle = model::ngon(8, 0.12, c);
        model::round(
            m,
            &[model::at_x(&axle, -1.15), model::at_x(&axle, 1.15)],
            0.0,
            tyre,
        );
        for s in [-1.0f32, 1.0] {
            let ring = |r: f32, x: f32| model::at_x(&model::ngon(12, r, c), s * x);
            model::round(m, &[ring(0.5, 1.1), ring(0.5, 1.28)], 0.03, tyre);
            model::round(m, &[ring(0.2, 1.27), ring(0.2, 1.34)], 0.01, hub);
        }
    }
}

/// A square ore tub's bed, the load heaped over it.
pub(super) fn bed(m: &mut MeshData, half: Vec2) {
    let flat = model::rect(Vec2::ZERO, half, 0.1);
    let ore = Skin::new(
        ORE.extend(Material::Rubble.id()),
        (ORE * 1.15).extend(Material::Rubble.id()),
    );
    model::solid(
        m,
        &[model::at_y(&flat, 1.6), model::at_y(&flat, 3.2)],
        0.02,
        ore,
    );
}

/// A light's colour, spread toward white.
pub(super) fn lit(color: Vec3, hot: Vec3, k: f32) -> Vec3 {
    color.lerp(hot, 0.5) * LAMP_BRIGHT * k
}

impl Dig<'_> {
    /// A rail car centered at `at`, its length along `yaw`, rolled `roll`
    /// onto its side (a derailed car): an ore car, or the locomotive
    /// (`loco`), its prow toward local +z. Drawn sculpted; a plain box round
    /// it is what tanks hit and hide behind. Returns its frame.
    pub(super) fn car(
        &mut self,
        at: Vec2,
        yaw: f32,
        roll: f32,
        loco: bool,
        style: &Freight,
    ) -> Rig {
        // Its detail comes from its own stream.
        let rng = &mut own_rng(at);
        let upright = roll.abs() < 0.3;
        let g = self.ground_height(at.x, at.y);
        let half_w = 1.75;
        let pivot = Vec3::new(-half_w * roll.signum(), 0.0, 0.0);
        let rot = Quat::from_rotation_y(yaw) * Quat::from_rotation_z(roll);
        let base = Vec3::new(at.x, g + 0.2, at.y);
        let rig = Rig {
            pos: base + Quat::from_rotation_y(yaw) * pivot - rot * pivot,
            rot,
        };
        let mut m = MeshData::default();
        let bogies: [f32; 2] = if loco { [-4.0, 3.0] } else { [-2.5, 2.5] };
        for z in bogies {
            (style.bogie)(&mut m, z);
        }
        // The collider: the body, from the bogies' frames to the rim.
        let (cz, hz, top) = if loco {
            (-0.3, 6.2, 3.95)
        } else {
            (0.0, 4.3, 3.8)
        };
        self.hidden_part(
            &rig,
            Vec3::new(0.0, (0.45 + top) * 0.5, cz),
            Vec3::new(half_w, (top - 0.45) * 0.5, hz),
        );
        if loco {
            (style.loco)(&mut m);
            self.hidden_part(&rig, Vec3::new(0.0, 4.4, -2.4), Vec3::new(1.0, 0.5, 2.2));
        } else {
            (style.tub)(&mut m, upright);
        }
        self.sculpt(&rig, &m);
        if !loco {
            if upright {
                self.ore_load(rng, &rig, style.load);
            } else {
                self.spilled_load(rng, &rig);
            }
        }
        rig
    }

    /// An ore car's load: crystal-rich rock from the Crag heaped up over
    /// the rim (`half_w` wide along it), raw crystal breaking out of it.
    fn ore_load(&mut self, rng: &mut Rng, rig: &Rig, half_w: fn(f32) -> f32) {
        self.heaped_ore(
            rng,
            rig,
            half_w,
            4.0,
            3.3,
            1.0,
            (12, 28),
            40,
            (0.16, 0.36),
            4,
        );
    }

    /// A car on its side has lost its load: the ore poured out of its
    /// mouth into a slumped fan on the ground, highest just outside the
    /// lip (a little still lying in the tub), thinning out away from it,
    /// lumps rolled out past its toe and raw crystal among them.
    fn spilled_load(&mut self, rng: &mut Rng, rig: &Rig) {
        // Out of the mouth, across the ground; the fan's frame has its
        // local +z that way, +x along the car, y up.
        let mouth = rig.rot * Vec3::Y;
        let out = Vec2::new(mouth.x, mouth.z).normalize_or(Vec2::X);
        let lip = rig.point(Vec3::new(0.0, 3.45, 0.0));
        let (half_l, rise, back) = (4.5f32, 1.5f32, 2.0f32);
        let center = Vec2::new(lip.x, lip.z) + out * (half_l - back);
        let fan = Rig {
            pos: Vec3::new(center.x, 0.0, center.y),
            rot: Quat::from_rotation_y(out.x.atan2(out.y)),
        };
        // Wide as the tub's mouth at the lip, spreading as it runs out.
        let half_w = move |z: f32| 3.6 + 1.6 * ((z + half_l) / (2.0 * half_l));
        // How high the pile stands at (u, v) over the ground: the mound
        // `ore_heap` makes, leaned so its crest is at the lip.
        let lean = move |v: f32| 1.6 - 0.65 * (v + 1.0);
        let sink = 0.12;
        let place = |m: &Map, local: Vec3| {
            let w = fan.point(Vec3::new(local.x, 0.0, local.z));
            Vec3::new(w.x, m.ground_height(w.x, w.z) - sink + local.y, w.z)
        };
        let mut m = MeshData::default();
        ore_heap(&mut m, rng, half_w, half_l, 0.0, rise, 12, 16);
        for v in &mut m.verts {
            let p = Vec3::from(v.pos);
            let y = p.y * lean(p.z / half_l);
            v.pos = place(self, Vec3::new(p.x, y, p.z)).to_array();
        }
        // Laid over the ground, each facet faces a new way.
        for t in m.verts.chunks_mut(3) {
            let p = t.iter().map(|v| Vec3::from(v.pos)).collect::<Vec<_>>();
            let mut n = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or(Vec3::Y);
            if n.y < 0.0 {
                n = -n;
            }
            for v in t {
                v.normal = n.to_array();
            }
        }
        // Loose stones on the pile, raw crystal among them, and more
        // rolled out past its toe onto the ground.
        let shades = [ORE * 0.8, ORE * 0.6, ORE * 0.95, MESA_TOP * 0.6];
        for k in 0..34 {
            let loose = k >= 24;
            let (u, v) = if loose {
                (rng.range(-1.1, 1.1), rng.range(0.9, 1.5))
            } else {
                (rng.range(-0.8, 0.8), rng.range(-0.5, 0.85))
            };
            let z = v * half_l;
            let y = if loose {
                0.0
            } else {
                heap_height(0.0, rise, u, v) * lean(v)
            };
            let at = place(self, Vec3::new(u * half_w(z), y + sink, z));
            let (color, mat, s) = if k % 8 == 7 {
                (RAW * 1.15, Material::Ore, rng.range(0.15, 0.3))
            } else {
                let c = shades[rng.index(shades.len())];
                (c, Material::Rubble, rng.range(0.18, 0.45))
            };
            ore_lump(&mut m, rng, at, s, color, mat);
        }
        let world = Rig {
            pos: Vec3::ZERO,
            rot: Quat::IDENTITY,
        };
        self.sculpt(&world, &m);
    }

    /// A lamp's steel bracket out from `at` (on a wall's face) along `out`:
    /// where the lamp hangs from.
    pub(super) fn bracket(&mut self, at: Vec3, out: Vec2, size: f32) -> Vec3 {
        let o = Vec3::new(out.x, 0.0, out.y).normalize() * 0.45 * size;
        let end = at + o;
        // The arm, a strut under it back to the wall, a plate on the wall.
        self.segment(at, end, 0.04, STEEL, Material::Metal, false);
        self.segment(
            at - Vec3::Y * 0.4,
            at + o * 0.65,
            0.025,
            STEEL,
            Material::Metal,
            false,
        );
        self.oboxes.push(OBox {
            center: Vec2::new(at.x, at.z),
            yaw: out.x.atan2(out.y),
            half: Vec2::new(0.12, 0.03),
            y0: at.y - 0.5,
            y1: at.y + 0.1,
            color: STEEL,
            side_mat: Material::Metal,
            top_mat: Material::Metal,
            solid: false,
            ..OBox::default()
        });
        end - Vec3::Y * 0.04
    }

    /// A lamp hanging `drop` m on a rod from `at`, `draw`n hanging from
    /// y = 0, 1 m tall or so, its light at its middle.
    pub(super) fn lamp_on(
        &mut self,
        at: Vec3,
        drop: f32,
        size: f32,
        light: Vec3,
        draw: impl Fn(&mut MeshData),
    ) {
        let top = at - Vec3::Y * drop;
        if drop > 0.05 {
            self.segment(at, top, 0.025, STEEL, Material::Metal, false);
        }
        self.lights.push(PointLight {
            pos: top - Vec3::Y * 0.55 * size,
            color: light * size,
            radius: LAMP_REACH * size,
        });
        let mut m = MeshData::default();
        draw(&mut m);
        for v in &mut m.verts {
            v.pos = (Vec3::from(v.pos) * size).to_array();
        }
        let rig = Rig {
            pos: top,
            rot: Quat::IDENTITY,
        };
        self.sculpt(&rig, &m);
    }

    /// What's the same of any borer: its beam (`color`, white-hot `hot`),
    /// the light it throws, and the one block a tank hits. Returns the
    /// machine's frame: its middle on the floor, the beam out along +z from
    /// z = 5.8.
    pub(super) fn borer_stand(
        &mut self,
        (from, to, radius): (Vec3, Vec3, f32),
        color: Vec3,
        hot: Vec3,
    ) -> Rig {
        self.beams.push(Beam {
            from,
            to,
            radius,
            color,
            hot,
        });
        let dir = (to - from).normalize();
        let rig = Rig {
            pos: Vec3::new(from.x - 5.8 * dir.x, 0.0, from.z - 5.8 * dir.z),
            rot: Quat::from_rotation_y(yaw_along(Vec2::new(dir.x, dir.z))),
        };
        // One block from the floor up, nothing to climb.
        let c = rig.point(Vec3::new(0.0, 0.0, -0.6));
        self.oboxes.push(OBox {
            center: Vec2::new(c.x, c.z),
            yaw: rig.rot.to_euler(glam::EulerRot::YXZ).0,
            half: Vec2::new(2.9, 4.6),
            y0: -0.2,
            y1: 3.6,
            hidden: true,
            ..OBox::default()
        });
        // The beam lights the bay: the blaze on the face, and along it.
        for (at, k) in [(to - dir * 1.2, 2.2), ((from + to) * 0.5, 1.2)] {
            self.lights.push(PointLight {
                pos: at,
                color: lit(color, hot, k),
                radius: LAMP_REACH * k,
            });
        }
        rig
    }

    /// Colliders for the two things behind a borer, where the Ithessar's
    /// monoliths stand (x 1.7 either way, z -6.2).
    pub(super) fn borer_posts(&mut self, rig: &Rig) {
        for x in [-1.7f32, 1.7] {
            let foot = rig.point(Vec3::new(x, 0.0, -6.2));
            self.oboxes.push(OBox {
                center: Vec2::new(foot.x, foot.z),
                yaw: rig.rot.to_euler(glam::EulerRot::YXZ).0,
                half: Vec2::new(0.45, 0.55),
                y0: -0.2,
                y1: 5.4,
                hidden: true,
                ..OBox::default()
            });
        }
    }
}
