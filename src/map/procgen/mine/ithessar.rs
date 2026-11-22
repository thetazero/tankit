//! The Ithessar's crystal mine: what's theirs in it. Plasma lamps in
//! crystal cages, the plasma borer in the bay, their freight of worked
//! crystal over dark alloy, their lens on the warehouse, the core over
//! their yard. Their spawns' side has the yard; the other side's camp is
//! just cleared ground (the mine is the Ithessar's, the fight over it
//! anyone's).

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use glam::{Quat, Vec2, Vec3, Vec4};

use super::dig::{Dig, MESA_TOP, ORE, RAW, STEEL, heap_height, ore_heap, ore_lump, own_rng};
use super::*;
use crate::map::landmarks::Rig;
use crate::map::{Core, Crystal, OBox};
use crate::render::material::Material;
use crate::render::mesh::MeshData;
use crate::rng::Rng;
use crate::tank::ithessar::{PLASMA, PLASMA_HOT};
use crate::tank::model::{self, Skin};

/// Ithessar worked crystal: their piers, cars, rigs and lamps.
const GLASS: Vec3 = Vec3::new(0.03, 0.07, 0.16);
/// How bright a plasma lamp's light is, and how far it reaches (m).
const LAMP_BRIGHT: f32 = 4.5;
const LAMP_REACH: f32 = 10.0;

/// The Ithessar.
pub struct Ithessar;

impl MineStyle for Ithessar {
    fn hold_name(&self) -> &'static str {
        "SEAM"
    }

    fn wall_lamp(&self, d: &mut Dig, at: Vec3, out: Vec2, size: f32) {
        d.plasma_wall_lamp(at, out, size);
    }

    fn hanging_lamp(&self, d: &mut Dig, at: Vec3, drop: f32, size: f32) {
        d.plasma_lamp(at, drop, size);
    }

    fn borer(&self, d: &mut Dig, rng: &mut Rng) {
        d.plasma_borer(rng);
    }

    fn rail_car(&self, d: &mut Dig, at: Vec2, yaw: f32, roll: f32, loco: bool) {
        d.ithessar_car(at, yaw, roll, loco);
    }

    fn emblem(&self, d: &mut Dig, rig: &Rig, at: Vec3, r: f32) {
        d.lens_emblem(rig, at, r);
    }

    fn base(&self, d: &mut Dig, side: usize, at: Vec2, rng: &mut Rng) {
        // Their yard has their core over it; the other side's camp is
        // cleared ground, nobody's marker.
        if d.factions.get(side) == Some(&crate::faction::Faction::Ithessar) {
            d.ithessar_yard(at, rng);
        }
    }
}

impl Dig<'_> {
    /// The plasma borer, braced on the bay's floor, its beam held on into
    /// the bore it's cutting; the power it draws held in a core behind it.
    fn plasma_borer(&mut self, rng: &mut Rng) {
        let lay = self.l;
        let (from, to, radius) = lay.beam;
        self.beams.push(crate::map::Beam { from, to, radius });
        let dir = (to - from).normalize();
        let rig = Rig {
            pos: Vec3::new(from.x - 5.8 * dir.x, 0.0, from.z - 5.8 * dir.z),
            rot: Quat::from_rotation_y(yaw_along(Vec2::new(dir.x, dir.z))),
        };
        let axis_y = from.y;
        // Hover pads under it, glowing on the floor.
        for (x, z) in [
            (-2.1, -3.6),
            (2.1, -3.6),
            (-2.3, 0.0),
            (2.3, 0.0),
            (-2.1, 3.4),
            (2.1, 3.4),
        ] {
            rig.part(
                self,
                Vec3::new(x, 0.42, z),
                Vec3::new(0.75, 0.3, 1.0),
                Quat::IDENTITY,
                STEEL,
                Material::Metal,
                false,
                0.0,
            );
            rig.part(
                self,
                Vec3::new(x, 0.1, z),
                Vec3::new(0.6, 0.04, 0.85),
                Quat::IDENTITY,
                PLASMA,
                Material::Glow,
                false,
                0.0,
            );
        }
        // What a tank hits: one block from the floor up, nothing to climb.
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
        // The body: worked crystal, a plasma seam round it.
        rig.part(
            self,
            Vec3::new(0.0, 1.45, -0.6),
            Vec3::new(2.5, 0.75, 4.4),
            Quat::IDENTITY,
            GLASS,
            Material::Vhaal,
            true,
            0.0,
        );
        rig.part(
            self,
            Vec3::new(0.0, 2.24, -0.6),
            Vec3::new(2.55, 0.05, 4.45),
            Quat::IDENTITY,
            PLASMA,
            Material::Glow,
            false,
            0.0,
        );
        rig.part(
            self,
            Vec3::new(0.0, 2.9, -1.4),
            Vec3::new(2.0, 0.65, 3.4),
            Quat::IDENTITY,
            GLASS * 1.25,
            Material::Vhaal,
            true,
            0.0,
        );
        // Heat fins along its back, plasma glowing between them.
        for k in 0..6 {
            let z = -4.0 + k as f32 * 1.05;
            rig.part(
                self,
                Vec3::new(0.0, 3.95, z),
                Vec3::new(1.5, 0.42, 0.12),
                Quat::IDENTITY,
                STEEL * 0.8,
                Material::Metal,
                false,
                0.0,
            );
        }
        rig.part(
            self,
            Vec3::new(0.0, 3.6, -1.4),
            Vec3::new(1.4, 0.06, 2.9),
            Quat::IDENTITY,
            PLASMA,
            Material::Glow,
            false,
            0.0,
        );
        // The operator's cab on its left, a slit of light for a window.
        rig.part(
            self,
            Vec3::new(-2.0, 3.6, 1.6),
            Vec3::new(0.8, 0.7, 1.0),
            Quat::IDENTITY,
            GLASS * 1.5,
            Material::Vhaal,
            true,
            0.0,
        );
        rig.part(
            self,
            Vec3::new(-2.0, 3.8, 2.62),
            Vec3::new(0.6, 0.12, 0.03),
            Quat::IDENTITY,
            PLASMA,
            Material::Glow,
            false,
            0.0,
        );
        // The barrel, out of the body's front to the emitter, ringed with
        // focusing crystal.
        let barrel0 = rig.point(Vec3::new(0.0, axis_y, 1.5));
        let barrel1 = rig.point(Vec3::new(0.0, axis_y, 5.0));
        self.pipe(barrel0, barrel1, 1.05, GLASS * 1.1, Material::Vhaal, true);
        for (k, z) in [2.3f32, 3.4, 4.5].into_iter().enumerate() {
            let c = rig.point(Vec3::new(0.0, axis_y, z));
            let ax = dir * 0.16;
            self.disc(c, ax, 1.55 - 0.1 * k as f32, GLASS * 1.6, Material::Vhaal);
            self.disc(c, ax * 0.5, 1.65 - 0.1 * k as f32, PLASMA, Material::Glow);
        }
        // The emitter: four crystal claws reaching in round the beam.
        for k in 0..4 {
            let a = k as f32 * FRAC_PI_2 + PI * 0.25;
            let (sa, ca) = a.sin_cos();
            let root = rig.point(Vec3::new(ca * 1.0, axis_y + sa * 1.0, 4.9));
            let knee = rig.point(Vec3::new(ca * 1.35, axis_y + sa * 1.35, 5.6));
            let tip = rig.point(Vec3::new(ca * 0.62, axis_y + sa * 0.62, 6.4));
            self.segment(root, knee, 0.2, GLASS * 1.4, Material::Vhaal, false);
            self.segment(knee, tip, 0.14, GLASS * 1.6, Material::Vhaal, false);
        }
        let mouth = rig.point(Vec3::new(0.0, axis_y, 5.25));
        self.disc(mouth, dir * 0.25, 0.75, PLASMA, Material::Glow);
        // Jacks braced down to the floor against its kick.
        for (x, z) in [(-2.4, 2.8), (2.4, 2.8), (-2.4, -4.6), (2.4, -4.6)] {
            let top = rig.point(Vec3::new(x, 1.6, z));
            let foot = rig.point(Vec3::new(x * 1.45, 0.0, z + 0.9 * z.signum()));
            self.segment(top, foot, 0.17, STEEL, Material::Metal, false);
            let f = Vec2::new(foot.x, foot.z);
            self.oboxes.push(OBox {
                center: f,
                yaw: rng.range(0.0, 0.2),
                half: Vec2::splat(0.45),
                y0: -0.1,
                y1: 0.12,
                color: STEEL,
                side_mat: Material::Metal,
                top_mat: Material::Metal,
                solid: false,
                ..OBox::default()
            });
        }
        // What it draws on: a core held between two monoliths behind it.
        let mut prongs = Vec::new();
        for x in [-1.7f32, 1.7] {
            let foot = rig.point(Vec3::new(x, 0.0, -6.2));
            self.oboxes.push(OBox {
                center: Vec2::new(foot.x, foot.z),
                yaw: rig.rot.to_euler(glam::EulerRot::YXZ).0,
                half: Vec2::new(0.45, 0.55),
                y0: -0.2,
                y1: 5.4,
                color: GLASS * 1.3,
                side_mat: Material::Vhaal,
                top_mat: Material::Vhaal,
                chip: 0.12,
                ..OBox::default()
            });
            prongs.push(rig.point(Vec3::new(x * 0.8, 5.5, -6.2)));
        }
        // The beam lights the bay: the blaze on the face, and along it.
        for (at, k) in [(to - dir * 1.2, 2.2), ((from + to) * 0.5, 1.2)] {
            self.lights.push(crate::render::PointLight {
                pos: at,
                color: PLASMA.lerp(PLASMA_HOT, 0.5) * LAMP_BRIGHT * k,
                radius: LAMP_REACH * k,
            });
        }
        self.cores.push(Core {
            at: rig.point(Vec3::new(0.0, 4.4, -6.2)),
            radius: 0.85,
            prongs,
        });
        // Conduits of their dark glass along the floor, from the core to
        // the body.
        for x in [-0.9f32, 0.9] {
            let a = rig.point(Vec3::new(x, 0.12, -5.6));
            let b = rig.point(Vec3::new(x, 0.12, -4.4));
            self.segment(a, b, 0.12, GLASS * 1.2, Material::Vhaal, false);
        }
    }

    /// One rail car centered at `at`, its length along `yaw`, rolled `roll`
    /// onto its side (a derailed car): an ore car, or the locomotive (`loco`),
    /// its prow toward local +z. Ithessar freight runs on rails, but it's
    /// built like the rest of their machines: knapped crystal over dark
    /// alloy running gear, each body held a hand's width off its bogies by
    /// the field, plasma showing through its seams. The hulls are drawn
    /// sculpted; a plain box round each is what tanks hit and hide behind.
    fn ithessar_car(&mut self, at: Vec2, yaw: f32, roll: f32, loco: bool) {
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
            bogie(&mut m, z);
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
            locomotive(&mut m);
            self.hidden_part(&rig, Vec3::new(0.0, 4.4, -2.4), Vec3::new(1.0, 0.5, 2.2));
            // Its drive: a knot of plasma held between two crystal horns
            // behind the canopy.
            let core = rig.point(Vec3::new(0.0, 5.25, -5.4));
            let prongs = [-1.0f32, 1.0]
                .map(|s| rig.point(Vec3::new(s * 0.75, 5.35, -5.4)))
                .to_vec();
            self.cores.push(Core {
                at: core,
                radius: 0.32,
                prongs,
            });
        } else {
            ore_tub(&mut m, upright);
        }
        self.sculpt(&rig, &m);
        if !loco {
            if upright {
                self.ore_load(rng, &rig);
            } else {
                self.spilled_load(rng, &rig);
            }
        }
    }

    /// An ore car's load: crystal-rich rock from the Crag heaped up over
    /// the rim, raw crystal breaking out of it.
    fn ore_load(&mut self, rng: &mut Rng, rig: &Rig) {
        // The heap, highest down the middle, filling the tub's lens to
        // just inside the rim (see `ore_tub`), a few lumps of raw crystal
        // in it.
        let half_w = |z: f32| 1.55 * ((4.0 - z.abs()) / 1.1).clamp(0.05, 1.0);
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

    /// A plasma lamp hanging `drop` m on a rod from `at`: the Ithessar's
    /// light everywhere they work, their emblem made real. An orb of plasma
    /// held in an almond-shaped cage of crystal prongs, pinched to a point
    /// below and gathered into a crystal cap above.
    fn plasma_lamp(&mut self, at: Vec3, drop: f32, size: f32) {
        let top = at - Vec3::Y * drop;
        if drop > 0.05 {
            self.segment(at, top, 0.025, STEEL, Material::Metal, false);
        }
        // The cap the cage hangs from: a short hexagonal collar.
        self.pipe(
            top,
            top - Vec3::Y * 0.12 * size,
            0.11 * size,
            GLASS * 1.3,
            Material::Vhaal,
            false,
        );
        let (h, r) = (0.82 * size, 0.24 * size);
        let crown = top - Vec3::Y * 0.12 * size;
        let tip = crown - Vec3::Y * h;
        let mid = (crown + tip) * 0.5;
        // The orb: a glowing core, haloed.
        let orb = mid + Vec3::Y * 0.03 * size;
        let core = PLASMA * 1.6;
        self.lights.push(crate::render::PointLight {
            pos: orb,
            color: PLASMA.lerp(PLASMA_HOT, 0.45) * LAMP_BRIGHT * size,
            radius: LAMP_REACH * size,
        });
        for k in 0..3 {
            let a = PI / 3.0 * k as f32;
            let axis = Vec3::new(a.cos(), 0.0, a.sin());
            self.disc(orb, axis * 0.1 * size, 0.18 * size, core, Material::Glow);
        }
        self.disc(orb, Vec3::Y * 0.1 * size, 0.18 * size, core, Material::Glow);
        // The lens: five prongs bowed out round it, swept a little like
        // grown crystal, meeting top and bottom.
        let n = 4;
        for k in 0..n {
            let a = TAU * k as f32 / n as f32;
            let swirl = 0.35;
            let bow = |t: f32, twist: f32| {
                let ang = a + twist;
                let w = r * (PI * t).sin();
                let y = crown.y + (tip.y - crown.y) * t;
                Vec3::new(mid.x + ang.cos() * w, y, mid.z + ang.sin() * w)
            };
            let pts = [
                bow(0.0, 0.0),
                bow(0.22, swirl * 0.3),
                bow(0.5, swirl * 0.6),
                bow(0.78, swirl * 0.85),
                bow(1.0, swirl),
            ];
            for w in pts.windows(2) {
                self.segment(
                    w[0],
                    w[1],
                    0.014 * size,
                    GLASS * 3.0,
                    Material::Vhaal,
                    false,
                );
            }
        }
        // The point it's pinched to, below, a bead of plasma on it.
        self.segment(
            tip + Vec3::Y * 0.04 * size,
            tip - Vec3::Y * 0.14 * size,
            0.03 * size,
            GLASS * 1.3,
            Material::Vhaal,
            false,
        );
        self.disc(
            tip - Vec3::Y * 0.16,
            Vec3::Y * 0.025,
            0.04,
            PLASMA,
            Material::Glow,
        );
    }

    /// A plasma lamp on a wall, `size` times the usual: a steel bracket out from `at` (on the
    /// wall's face) along `out`, the lamp hanging off its end.
    fn plasma_wall_lamp(&mut self, at: Vec3, out: Vec2, size: f32) {
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
        self.plasma_lamp(end - Vec3::Y * 0.04, 0.0, size);
    }

    /// The Ithessar emblem on a wall, in `rig`'s frame at `at`, facing its
    /// -z: an almond-shaped lens of crystal `r` m across each way from
    /// the middle, a plasma orb in it.
    fn lens_emblem(&mut self, rig: &Rig, at: Vec3, r: f32) {
        // As the HUD draws it: two arcs meeting in points either side.
        let h = 0.58 * r;
        let off = (r * r - h * h) / (2.0 * h);
        let rad = off + h;
        let a = (off / r).atan();
        let n = 10;
        for (cy, from, to) in [(off, PI + a, TAU - a), (-off, a, PI - a)] {
            let p = |k: usize| {
                let t = from + (to - from) * k as f32 / n as f32;
                rig.point(at + Vec3::new(t.cos() * rad, cy + t.sin() * rad, 0.0))
            };
            for k in 0..n {
                self.segment(p(k), p(k + 1), 0.05, GLASS * 1.3, Material::Vhaal, false);
            }
        }
        let c = rig.point(at);
        self.disc(
            c,
            rig.rot * Vec3::Z * -0.03,
            0.3 * r,
            PLASMA,
            Material::Glow,
        );
    }

    /// The Ithessar's yard at the west edge: a plasma core held over it, as
    /// at every Ithessar place, and worked crystal standing round.
    fn ithessar_yard(&mut self, at: Vec2, rng: &mut Rng) {
        let g = self.ground_height(at.x, at.y);
        let mut prongs = Vec::new();
        for k in 0..3 {
            let a = k as f32 / 3.0 * TAU + 0.3;
            let p = at + Vec2::new(a.cos(), a.sin()) * 5.0;
            let lean = -Vec2::new(a.cos(), a.sin());
            let h = 8.0;
            let axis = Vec3::new(lean.x * 0.35, 1.0, lean.y * 0.35).normalize() * (h + 1.0);
            let base = Vec3::new(p.x, g - 1.0, p.y);
            self.crystals.push(Crystal {
                base,
                axis,
                radius: 0.9,
                sides: 6,
                seed: rng.next_u64() as u32,
                color: GLASS * 1.2,
                lit: true,
                solid: true,
                raw: false,
            });
            prongs.push(base + axis);
        }
        self.cores.push(Core {
            at: Vec3::new(at.x, g + 10.0, at.y),
            radius: 1.0,
            prongs,
        });
    }
}

/// Ithessar freight: worked crystal over dark alloy.
const HULL: Skin = Skin::new(
    Vec4::new(0.02, 0.05, 0.13, Material::Vhaal.id()),
    Vec4::new(0.05, 0.12, 0.28, Material::Vhaal.id()),
);

/// The locomotive's canopy: a lens of clearer crystal.
const LENS: Skin = Skin::new(
    Vec4::new(0.06, 0.14, 0.3, Material::Vhaal.id()),
    Vec4::new(0.12, 0.24, 0.45, Material::Vhaal.id()),
);

const GEAR: Skin = Skin::new(
    Vec4::new(0.12, 0.11, 0.13, Material::Metal.id()),
    Vec4::new(0.22, 0.2, 0.23, Material::Metal.id()),
);

/// Plasma showing through the seams.
const SEAM: Skin = Skin::new(
    Vec4::new(PLASMA.x, PLASMA.y, PLASMA.z, Material::Glow.id()),
    Vec4::new(PLASMA.x, PLASMA.y, PLASMA.z, Material::Glow.id()),
);

/// Loose crystal held in the field, lit rose by the plasma in it.
const LOOSE: Skin = Skin::new(
    Vec4::new(0.42, 0.13, 0.3, Material::Glow.id()),
    Vec4::new(0.6, 0.22, 0.42, Material::Glow.id()),
);

/// A thin glowing bar through the points of `line` (each a center in the
/// car's frame and its half size across there).
fn seam_bar(m: &mut MeshData, line: &[(Vec3, Vec2)]) {
    let rings: Vec<Vec<Vec3>> = line
        .iter()
        .map(|(c, h)| model::at_z(&model::rect(Vec2::new(c.x, c.y), *h, 0.005), c.z))
        .collect();
    model::solid(m, &rings, 0.0, SEAM);
}

/// A bogie centered `z` along the car: a pointed alloy frame on four
/// wheels, plasma in their hubs, and the field pad on top that holds the
/// body up.
fn bogie(m: &mut MeshData, z: f32) {
    let full = model::trap(0.95, 0.7, 0.4, 0.95, 0.08);
    let end = model::trap(0.45, 0.3, 0.55, 0.9, 0.05);
    model::solid(
        m,
        &[
            model::at_z(&end, z - 1.8),
            model::at_z(&full, z - 1.3),
            model::at_z(&full, z + 1.3),
            model::at_z(&end, z + 1.8),
        ],
        0.04,
        GEAR,
    );
    for x in [-1.2f32, 1.2] {
        for dz in [-0.9, 0.9] {
            let c = Vec2::new(z + dz, 0.6);
            let wheel = model::ngon(10, 0.5, c);
            model::round(
                m,
                &[model::at_x(&wheel, x - 0.09), model::at_x(&wheel, x + 0.09)],
                0.03,
                GEAR,
            );
            let hub = model::ngon(6, 0.17, c);
            let o = x + x.signum() * 0.1;
            model::round(
                m,
                &[
                    model::at_x(&hub, o),
                    model::at_x(&hub, o + x.signum() * 0.03),
                ],
                0.0,
                SEAM,
            );
        }
    }
    seam_bar(
        m,
        &[
            (Vec3::new(0.0, 1.02, z - 1.0), Vec2::new(0.55, 0.03)),
            (Vec3::new(0.0, 1.02, z + 1.0), Vec2::new(0.55, 0.03)),
        ],
    );
}

/// An ore car's tub: open-topped, flared, pointed at both ends like a
/// lens, two bands of crystal with plasma glowing between them, on a keel.
/// A blade stands up off each point.
fn ore_tub(m: &mut MeshData, loaded: bool) {
    // Plan outline (x, z), going round: the flanks, then a point.
    let hex = |hw: f32, hz: f32, tip: f32| {
        [
            Vec2::new(hw, -hz),
            Vec2::new(hw, hz),
            Vec2::new(0.0, tip),
            Vec2::new(-hw, hz),
            Vec2::new(-hw, -hz),
            Vec2::new(0.0, -tip),
        ]
    };
    let (bot, rim) = (hex(1.2, 2.7, 3.7), hex(1.75, 3.25, 4.45));
    let (y0, y1) = (1.4, 3.45);
    let at = |i: usize, t: f32, inset: f32| {
        let p = bot[i % 6].lerp(rim[i % 6], t) * inset;
        Vec3::new(p.x, y0 + (y1 - y0) * t, p.y)
    };
    let inside = Vec3::new(0.0, 2.4, 0.0);
    for i in 0..6 {
        for (t0, t1) in [(0.0, 0.46), (0.54, 1.0)] {
            model::plate(
                m,
                [
                    at(i, t0, 1.0),
                    at(i + 1, t0, 1.0),
                    at(i + 1, t1, 1.0),
                    at(i, t1, 1.0),
                ],
                0.12,
                inside,
                HULL,
            );
        }
        model::plate(
            m,
            [
                at(i, 0.44, 0.97),
                at(i + 1, 0.44, 0.97),
                at(i + 1, 0.56, 0.97),
                at(i, 0.56, 0.97),
            ],
            0.04,
            inside,
            SEAM,
        );
    }
    let keel: Vec<Vec2> = bot.iter().map(|p| *p * Vec2::new(0.35, 0.8)).collect();
    model::solid(
        m,
        &[model::at_y(&keel, 1.12), model::at_y(&bot, y0 + 0.02)],
        0.03,
        HULL,
    );
    // Inside, the floor of the load (the lumps heap up on it). Emptied,
    // the tub shows its bare floor.
    if loaded {
        let bed: Vec<Vec2> = rim.iter().map(|p| *p * 0.9).collect();
        let floor: Vec<Vec2> = bot.iter().map(|p| *p * 0.92).collect();
        model::solid(
            m,
            &[model::at_y(&floor, y0 + 0.1), model::at_y(&bed, 3.2)],
            0.02,
            Skin::new(
                ORE.extend(Material::Rubble.id()),
                (ORE * 1.15).extend(Material::Rubble.id()),
            ),
        );
    }
    for s in [-1.0f32, 1.0] {
        model::shard(
            m,
            Vec3::new(0.0, 2.7, s * 4.15),
            Vec3::new(0.0, 1.0, s * 0.6),
            1.7,
            0.17,
            HULL,
        );
    }
}

/// The locomotive's body, prow toward +z: two long blades of crystal split
/// down the spine, the plasma that drives it glowing up through the split,
/// raking down to a knife-edged prow; a lens of clearer crystal over the
/// spine aft, and loose crystals riding the field along the prow.
fn locomotive(m: &mut MeshData) {
    // One blade's section (x, y), the right one; the left mirrors it.
    let blade = [
        Vec2::new(0.12, 1.25),
        Vec2::new(1.2, 1.3),
        Vec2::new(1.72, 2.3),
        Vec2::new(1.45, 3.3),
        Vec2::new(0.32, 4.0),
        Vec2::new(0.12, 3.75),
    ];
    // Stations along it: (z, width, height) as fractions of the full section.
    let stations = [
        (-6.5, 0.85, 0.82),
        (-6.0, 1.0, 1.0),
        (1.6, 1.0, 1.0),
        (4.4, 0.72, 0.62),
        (6.3, 0.26, 0.24),
        (6.9, 0.06, 0.1),
    ];
    let foot = 1.55;
    for side in [-1.0f32, 1.0] {
        let rings: Vec<Vec<Vec3>> = stations
            .iter()
            .map(|&(z, sx, sy)| {
                blade
                    .iter()
                    .map(|p| Vec3::new(side * p.x * sx, foot + (p.y - foot) * sy, z))
                    .collect()
            })
            .collect();
        model::solid(m, &rings, 0.03, HULL);
        // Plasma along the chine, where the flank turns in.
        seam_bar(
            m,
            &[
                (Vec3::new(side * 1.74, 2.3, -6.0), Vec2::new(0.03, 0.05)),
                (Vec3::new(side * 1.74, 2.3, 1.6), Vec2::new(0.03, 0.05)),
                (
                    Vec3::new(side * 1.26, foot + 0.75 * 0.62, 4.4),
                    Vec2::new(0.03, 0.04),
                ),
            ],
        );
    }
    // The plasma in the split.
    seam_bar(
        m,
        &[
            (Vec3::new(0.0, 2.65, -6.0), Vec2::new(0.1, 1.1)),
            (Vec3::new(0.0, 2.65, 1.6), Vec2::new(0.1, 1.1)),
            (Vec3::new(0.0, 2.2, 4.4), Vec2::new(0.07, 0.75)),
            (Vec3::new(0.0, 1.8, 6.2), Vec2::new(0.025, 0.25)),
        ],
    );
    // The canopy lens, over the spine aft, raked forward.
    let lens = [
        Vec2::new(-0.9, 3.6),
        Vec2::new(0.9, 3.6),
        Vec2::new(1.05, 4.25),
        Vec2::new(0.5, 4.85),
        Vec2::new(-0.5, 4.85),
        Vec2::new(-1.05, 4.25),
    ];
    let lens_at = |z: f32, s: f32, drop: f32| -> Vec<Vec3> {
        lens.iter()
            .map(|p| Vec3::new(p.x * s, 3.6 + (p.y - 3.6) * s - drop, z))
            .collect()
    };
    model::solid(
        m,
        &[
            lens_at(-4.9, 0.55, 0.0),
            lens_at(-4.4, 1.0, 0.0),
            lens_at(-1.8, 1.0, 0.0),
            lens_at(0.4, 0.35, 0.1),
        ],
        0.03,
        LENS,
    );
    for s in [-1.0f32, 1.0] {
        seam_bar(
            m,
            &[
                (Vec3::new(s * 1.07, 4.25, -4.3), Vec2::new(0.02, 0.04)),
                (Vec3::new(s * 1.07, 4.25, -1.9), Vec2::new(0.02, 0.04)),
            ],
        );
        // The horns that hold the drive's plasma, aft of the canopy.
        model::shard(
            m,
            Vec3::new(s * 0.55, 3.7, -5.4),
            Vec3::new(s * 0.35, 1.0, 0.0),
            1.75,
            0.16,
            HULL,
        );
        // Loose crystals riding the field along the prow, points forward.
        for k in 0..2 {
            let z = 2.0 + k as f32 * 1.6;
            let x = 2.15 - (z - 1.6) * 0.2;
            model::shard(
                m,
                Vec3::new(s * x, 2.7 - k as f32 * 0.3, z),
                Vec3::new(s * 0.1, -0.06, 1.0),
                0.8 - k as f32 * 0.15,
                0.07,
                LOOSE,
            );
        }
    }
}
