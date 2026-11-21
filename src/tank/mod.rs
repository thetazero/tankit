//! Tank visuals: the modular kit (hulls x turrets) uploaded once, and the
//! per-frame instance list for an animated tank.

pub mod ithessar;
pub mod model;
pub mod resonant;
pub mod track;
pub mod uthenn;

use std::f32::consts::FRAC_PI_2;

use glam::{Mat4, Quat, Vec2, Vec3, Vec4};

use crate::render::Particle;
use crate::render::mesh::{Instance, MeshData, MeshId, MeshStore};
use crate::sim::{HullKind, Tank, TurretKind};
use ithessar::HoverModel;
use model::{HullModel, Rig, Running, TurretModel};
use resonant::{VigilExtras, VigilModel};
use track::{Belt, Wheel};

/// Target spacing between tread links.
const LINK_PITCH: f32 = 0.19;
/// Half the link thickness: the belt path runs through the link's middle.
const LINK_HALF: f32 = 0.04;

/// CPU-side meshes for the whole kit, before upload.
pub struct KitMeshes {
    hulls: Vec<(HullModel, [MeshData; 4])>,
    hover: HoverModel,
    /// Vigil's extras (its body and running gear are `hulls[2]`).
    vigil: VigilExtras,
    turrets: Vec<TurretModel>,
}

impl KitMeshes {
    pub fn build() -> KitMeshes {
        let hull = |h: HullModel| {
            let r = &h.running;
            let parts = [
                model::link(r.track_width, LINK_PITCH),
                model::road_wheel(r.road[0].radius, r.track_width),
                model::sprocket(r.sprocket.radius, r.track_width),
                model::idler(r.idler.radius, r.track_width),
            ];
            (h, parts)
        };
        let VigilModel {
            hull: vigil_hull,
            extras: vigil,
        } = resonant::vigil();
        KitMeshes {
            hulls: vec![
                hull(model::kataphrakt()),
                hull(model::kastron()),
                hull(vigil_hull),
                hull(uthenn::gravis()),
            ],
            hover: ithessar::sethuun(),
            vigil,
            // Same order as `TurretKind::ALL`.
            turrets: TurretKind::ALL.map(turret_model).into(),
        }
    }

    #[cfg(test)]
    pub fn triangle_count(&self) -> usize {
        let h: usize = self
            .hulls
            .iter()
            .map(|(h, p)| h.body.indices.len() + p.iter().map(|m| m.indices.len()).sum::<usize>())
            .sum();
        let o = &self.hover;
        let h = h + [&o.body, &o.glow, &o.emitter, &o.lens, &o.shard]
            .iter()
            .map(|m| m.indices.len())
            .sum::<usize>();
        let g = &self.vigil;
        let h = h + [&g.light, &g.heart, &g.bell]
            .into_iter()
            .chain(g.plates.iter().map(|(m, _)| m))
            .map(|m| m.indices.len())
            .sum::<usize>();
        let t: usize = self
            .turrets
            .iter()
            .map(|t| t.head.indices.len() + t.mantlet.indices.len() + t.barrel.indices.len())
            .sum();
        (h + t) / 3
    }
}

/// `kind`'s turret art (also what its hitbox is shaped from).
pub fn turret_model(kind: TurretKind) -> TurretModel {
    match kind {
        TurretKind::Kontos => model::kontos(),
        TurretKind::Pelekys => model::pelekys(),
        TurretKind::Ammeth => ithessar::ammeth(),
        TurretKind::Ossrin => ithessar::ossrin(),
        TurretKind::Lumen => uthenn::lumen(),
        TurretKind::Hymn => model::hymn(),
        TurretKind::Knell => resonant::knell(),
    }
}

struct HullArt {
    body: MeshId,
    /// Uthenn heat lights, if the hull has any.
    heat_light: Option<MeshId>,
    link: MeshId,
    road: MeshId,
    sprocket: MeshId,
    idler: MeshId,
    running: Running,
    links: usize,
}

struct HoverArt {
    body: MeshId,
    glow: MeshId,
    emitter: MeshId,
    lens: MeshId,
    shard: MeshId,
    shards: Vec<(Vec3, f32)>,
    emitter_y: f32,
}

struct VigilArt {
    light: MeshId,
    heart: MeshId,
    heart_at: Vec3,
    plates: Vec<(MeshId, Vec3)>,
    bell: MeshId,
    bell_pivot: Vec3,
}

struct TurretArt {
    kind: TurretKind,
    head: MeshId,
    head_light: Option<MeshId>,
    barrel_light: Option<MeshId>,
    mantlet: MeshId,
    barrel: MeshId,
    barrels: Vec<Vec3>,
    muzzle: f32,
    recoil: f32,
    rig: Rig,
}

/// The tank kit on the GPU.
pub struct TankArt {
    hulls: Vec<HullArt>,
    hover: HoverArt,
    vigil: VigilArt,
    turrets: Vec<TurretArt>,
}

impl TankArt {
    pub fn upload(r: &mut impl MeshStore) -> TankArt {
        let kit = KitMeshes::build();
        let o = &kit.hover;
        let hover = HoverArt {
            body: r.add_mesh(&o.body),
            glow: r.add_mesh(&o.glow),
            emitter: r.add_mesh(&o.emitter),
            lens: r.add_mesh(&o.lens),
            shard: r.add_mesh(&o.shard),
            shards: o.shards.clone(),
            emitter_y: o.emitter_y,
        };
        let g = &kit.vigil;
        let vigil = VigilArt {
            light: r.add_mesh(&g.light),
            heart: r.add_mesh(&g.heart),
            heart_at: g.heart_at,
            plates: g
                .plates
                .iter()
                .map(|(m, at)| (r.add_mesh(m), *at))
                .collect(),
            bell: r.add_mesh(&g.bell),
            bell_pivot: g.bell_pivot,
        };
        let hulls = kit
            .hulls
            .into_iter()
            .map(|(h, [link, road, sprocket, idler])| {
                let run = h.running;
                let mut wheels: Vec<Wheel> = run.road.clone();
                wheels.push(run.sprocket);
                wheels.push(run.idler);
                let belt = Belt::new(&wheels, LINK_HALF);
                let links = (belt.length() / LINK_PITCH).round() as usize;
                HullArt {
                    body: r.add_mesh(&h.body),
                    heat_light: light_mesh(r, &h.heat_light),
                    link: r.add_mesh(&link),
                    road: r.add_mesh(&road),
                    sprocket: r.add_mesh(&sprocket),
                    idler: r.add_mesh(&idler),
                    running: run,
                    links,
                }
            })
            .collect();
        let turrets = kit
            .turrets
            .into_iter()
            .zip(TurretKind::ALL)
            .map(|(t, kind)| TurretArt {
                kind,
                rig: t.rig,
                head: r.add_mesh(&t.head),
                head_light: light_mesh(r, &t.head_light),
                barrel_light: light_mesh(r, &t.barrel_light),
                mantlet: r.add_mesh(&t.mantlet),
                barrel: r.add_mesh(&t.barrel),
                barrels: t.barrels,
                muzzle: t.muzzle,
                recoil: t.recoil,
            })
            .collect();
        TankArt {
            hulls,
            hover,
            vigil,
            turrets,
        }
    }

    /// Append every instance needed to draw `t` this frame, and its plasma
    /// light (charging orbs, muzzle flashes) to `glow`.
    pub fn instances(&self, t: &Tank, out: &mut Vec<(MeshId, Instance)>, glow: &mut Vec<Particle>) {
        let turret = &self.turrets[TurretKind::ALL
            .iter()
            .position(|&k| k == t.turret_kind)
            .unwrap_or(0)];
        // The whole kit hangs off the physics body (body frame = art frame, ground at y=0).
        let base = Mat4::from_rotation_translation(t.rot, t.pos);
        match t.hull_kind {
            HullKind::Kataphrakt => self.tracked(&self.hulls[0], t, base, out),
            HullKind::Kastron => self.tracked(&self.hulls[1], t, base, out),
            HullKind::Sethuun => self.hovering(t, base, out),
            HullKind::Gravis => self.tracked(&self.hulls[3], t, base, out),
            HullKind::Vigil => {
                self.tracked(&self.hulls[2], t, base, out);
                self.vigil(t, base, out);
            }
        }
        // The turret hangs where the sim says the gun is, so shots and the
        // beam leave the muzzle that's drawn.
        self.turret(turret, t, out, glow);
    }

    /// A tracked hull: body, wheels on their suspension, and the belt of
    /// links wrapped round them.
    fn tracked(&self, hull: &HullArt, t: &Tank, base: Mat4, out: &mut Vec<(MeshId, Instance)>) {
        let tint = t.color.extend(0.0);
        let plain = Vec4::new(1.0, 1.0, 1.0, 0.0);
        let sprung = base;
        out.push((hull.body, Instance::new(sprung, tint)));
        if let Some(light) = hull.heat_light {
            out.push((light, Instance::new(sprung, heat_tint(t))));
        }

        let run = &hull.running;
        let per_side = run.road.len();
        for (side, sign) in [(0usize, 1.0f32), (1, -1.0)] {
            let dist = t.track_dist[side];
            // Road wheels ride their suspension; the belt wraps whatever shape that makes.
            let road: Vec<Wheel> = run
                .road
                .iter()
                .enumerate()
                .map(|(k, w)| {
                    let lift = t
                        .wheel_lift
                        .get(side * per_side + k)
                        .copied()
                        .unwrap_or(0.0);
                    Wheel {
                        center: w.center + Vec2::new(0.0, lift),
                        radius: w.radius,
                    }
                })
                .collect();
            let mut wheels = road.clone();
            wheels.push(run.sprocket);
            wheels.push(run.idler);
            let belt = Belt::new(&wheels, LINK_HALF);

            // Mirror the -x side so wheel hubs and link details face outward.
            let flip = Mat4::from_scale(Vec3::new(sign, 1.0, 1.0));
            let x = sign * run.track_x;
            let wheel = |w: &Wheel| {
                base * Mat4::from_translation(Vec3::new(x, w.center.y, w.center.x))
                    * flip
                    * Mat4::from_rotation_x(dist / w.radius)
            };
            for w in &road {
                out.push((hull.road, Instance::new(wheel(w), tint)));
            }
            out.push((hull.sprocket, Instance::new(wheel(&run.sprocket), tint)));
            out.push((hull.idler, Instance::new(wheel(&run.idler), tint)));

            let pitch = belt.length() / hull.links as f32;
            for i in 0..hull.links {
                let p = belt.at(i as f32 * pitch + dist);
                // Link frame: x across, y = outward normal, z = along the belt.
                let ny = Vec3::new(0.0, p.normal.y, p.normal.x);
                let nz = Vec3::X.cross(ny);
                let m = Mat4::from_cols(
                    Vec4::X,
                    ny.extend(0.0),
                    nz.extend(0.0),
                    Vec3::new(x, p.pos.y, p.pos.x).extend(1.0),
                );
                out.push((hull.link, Instance::new(base * m * flip, plain)));
            }
        }
    }

    /// The Vigil's light, floating crystal and plates, and bell. The light
    /// breathes and burns brighter under throttle and the crystal turns; on a
    /// husk the light gutters and fades and the plates sag crooked; on a wreck
    /// it's out, the crystal has dropped into its bowl and the plates lie on
    /// the tracks.
    fn vigil(&self, t: &Tank, base: Mat4, out: &mut Vec<(MeshId, Instance)>) {
        let g = &self.vigil;
        let tint = t.color.extend(0.0);
        let time = t.age;
        let effort = (t.belt[0].abs().max(t.belt[1].abs()) / t.hull.max_speed).min(1.0);
        // Light strength, and how far the plates have let go (0 held .. 1 fallen).
        let (glow, slack) = match (t.dead, t.husk) {
            (true, _) => (0.0, 1.0),
            (false, Some(s)) => {
                // Gutters: stutters on and off, weaker as it fades.
                let flick = ((time * 23.0).sin() * (time * 37.0).sin()).abs();
                (s * (1.2 + 2.6 * flick), 0.3 + 0.4 * (1.0 - s))
            }
            (false, None) => (2.2 + 0.4 * (time * 2.2).sin() + effort * 1.0, 0.0),
        };
        let lit = |k: f32| {
            if glow > 0.0 {
                Vec4::new(1.0, 1.0, 1.0, glow * k)
            } else {
                Vec4::new(0.1, 0.1, 0.12, 0.0)
            }
        };
        out.push((g.light, Instance::new(base, lit(1.0))));

        // The crystal floats and turns; it slows as a husk fades, and drops
        // into its bowl when it's out.
        let heart = if t.dead {
            Mat4::from_translation(g.heart_at - Vec3::Y * 0.22) * Mat4::from_rotation_z(0.5)
        } else {
            let s = t.husk.unwrap_or(1.0);
            let bob = 0.06 * (time * 1.3).sin() * s;
            Mat4::from_translation(g.heart_at + Vec3::Y * bob)
                * Mat4::from_rotation_y(time * (0.6 + 0.9 * effort) * s)
        };
        out.push((g.heart, Instance::new(base * heart, lit(1.3))));

        for (k, &(mesh, at)) in g.plates.iter().enumerate() {
            let side = at.x.signum();
            let ph = k as f32 * 1.7;
            // Held: each drifts on its own, a slow float and a little roll.
            let held = 1.0 - slack;
            let bob = 0.05 * (time * 1.1 + ph).sin() * held;
            let roll = 0.05 * (time * 0.8 + ph * 1.3).sin() * held;
            // Let go: sagging out and down, tipped over, each its own way.
            let fall = if t.dead { 0.45 } else { 0.3 * slack };
            let at = at + Vec3::new(side * 0.15 * slack, bob - fall, 0.0);
            let m = Mat4::from_translation(at)
                * Mat4::from_rotation_z(side * (roll - 0.45 * slack))
                * Mat4::from_rotation_x(0.12 * slack * (ph * 2.3).sin());
            out.push((mesh, Instance::new(base * m, tint)));
        }

        // The bell swings back as it pulls away and sways as it goes.
        let local_v = t.rot.inverse() * t.vel;
        let lean = (local_v.z / t.hull.max_speed).clamp(-1.0, 1.0) * 0.35;
        let sway = (time * 3.1).sin() * (0.05 + 0.12 * effort);
        let roll = (local_v.x * 0.05).clamp(-0.3, 0.3) + (time * 2.3).cos() * 0.04;
        let hang = if t.dead { 0.0 } else { 1.0 };
        let swing = Mat4::from_translation(g.bell_pivot)
            * Mat4::from_rotation_x(-(lean + sway) * hang)
            * Mat4::from_rotation_z(roll * hang);
        out.push((
            g.bell,
            Instance::new(base * swing, Vec4::new(1.0, 1.0, 1.0, 0.0)),
        ));
    }

    /// The Ithessar hover hull: the carapace banked into its turn, six
    /// emitters gimballing with the thrust and firing plasma at the ground,
    /// and loose crystals bobbing in the field.
    fn hovering(&self, t: &Tank, base: Mat4, out: &mut Vec<(MeshId, Instance)>) {
        let h = &self.hover;
        let c = t.chassis;
        let tint = t.color.extend(0.0);
        let plain = Vec4::new(1.0, 1.0, 1.0, 0.0);
        let alive = t.color.max_element() > 0.2;
        let body =
            base * Mat4::from_rotation_z(t.lean) * Mat4::from_translation(Vec3::Y * ithessar::RIDE);
        out.push((h.body, Instance::new(body, tint)));
        if !alive {
            // A wreck: the field is gone, the crystals lie where they fell.
            for (i, &(at, len)) in h.shards.iter().enumerate() {
                let fall = Vec3::new(at.x * 0.8, 0.35 + i as f32 * 0.05, at.z * 0.9);
                let q = Quat::from_rotation_z(FRAC_PI_2 * if at.x > 0.0 { -1.0 } else { 1.0 });
                out.push((
                    h.shard,
                    Instance::new(
                        body * Mat4::from_scale_rotation_translation(Vec3::splat(len), q, fall),
                        Vec4::new(0.2, 0.2, 0.2, 0.0),
                    ),
                ));
            }
            for i in 0..c.wheel_count() {
                let m = c.mount(i);
                let at = Vec3::new(m.x, h.emitter_y, m.z);
                out.push((
                    h.emitter,
                    Instance::new(body * Mat4::from_translation(at), plain),
                ));
            }
            return;
        }

        let time = t.age;
        // The core breathes, and burns brighter under throttle.
        let effort = (t.belt[0].abs() / t.hull.max_speed).min(1.0);
        let pulse = 2.4 + 0.5 * (time * 2.6).sin() + effort * 1.2;
        out.push((h.glow, Instance::new(body, Vec4::new(1.0, 1.0, 1.0, pulse))));

        // Emitters tilt against the motion (thrust vectoring); each lens burns
        // brighter the harder it's pushing.
        let local_v = t.rot.inverse() * t.vel;
        let tilt_x = (local_v.z * 0.025).clamp(-0.35, 0.35);
        let tilt_z = (-local_v.x * 0.04).clamp(-0.3, 0.3);
        for i in 0..c.wheel_count() {
            let m = c.mount(i);
            let at = Vec3::new(m.x, h.emitter_y, m.z);
            let gimbal = Mat4::from_translation(at)
                * Mat4::from_rotation_x(tilt_x)
                * Mat4::from_rotation_z(tilt_z);
            out.push((h.emitter, Instance::new(body * gimbal, plain)));
            let lift = t.wheel_lift.get(i).copied().unwrap_or(0.0);
            let load = ((lift + ithessar_sag()) / ithessar_sag()).clamp(0.0, 2.0);
            let flicker = 0.85 + 0.15 * (time * 37.0 + i as f32 * 2.3).sin();
            out.push((
                h.lens,
                Instance::new(
                    body * gimbal,
                    Vec4::new(1.0, 1.0, 1.0, 2.5 + load * 1.5 * flicker),
                ),
            ));
        }

        // Loose crystals: each bobs on its own beat and turns slowly; at
        // speed they trail back a little, held on a spring.
        for (i, &(at, len)) in h.shards.iter().enumerate() {
            let ph = i as f32 * 1.7;
            let bob = Vec3::Y * (time * 1.4 + ph).sin() * 0.09;
            let drag = Vec3::new(-local_v.x, 0.0, -local_v.z) * 0.012;
            let spin = Quat::from_rotation_y(time * (0.4 + i as f32 * 0.13) + ph)
                * Quat::from_rotation_x(0.2 * (time * 0.9 + ph).sin());
            out.push((
                h.shard,
                Instance::new(
                    body * Mat4::from_scale_rotation_translation(
                        Vec3::splat(len),
                        spin,
                        at + bob + drag - Vec3::Y * len * 0.5,
                    ),
                    plain,
                ),
            ));
        }
    }

    /// Turret yaws on the hull's ring; the gun pitches about its trunnion.
    fn turret(
        &self,
        turret: &TurretArt,
        t: &Tank,
        out: &mut Vec<(MeshId, Instance)>,
        glow: &mut Vec<Particle>,
    ) {
        let tint = t.color.extend(0.0);
        let alive = t.color.max_element() > 0.2;
        let tm = t.turret_frame();
        out.push((turret.head, Instance::new(tm, tint)));
        if let Some(light) = turret.head_light {
            out.push((light, Instance::new(tm, heat_tint(t))));
        }
        let gun = t.gun_frame();
        out.push((turret.mantlet, Instance::new(gun, tint)));

        if turret.kind == TurretKind::Knell && alive {
            knell_charge(t, tm, gun, out, glow);
        }
        let plasma = turret.kind.maker() == crate::faction::Faction::Ithessar;
        let fired = t.fire_barrel % turret.barrels.len();
        let (kick, flash) = recoil_curve(t.since_fire);
        match turret.rig {
            Rig::Recoil => {
                for (i, &o) in turret.barrels.iter().enumerate() {
                    let back = if i == fired {
                        kick * turret.recoil
                    } else {
                        0.0
                    };
                    let bm = gun * Mat4::from_translation(o - Vec3::Z * back);
                    out.push((turret.barrel, Instance::new(bm, tint)));
                    if let Some(light) = turret.barrel_light {
                        out.push((light, Instance::new(bm, heat_tint(t))));
                    }
                    if t.beaming && !t.dead {
                        let at = bm.transform_point3(Vec3::Z * turret.muzzle);
                        beam_flare(at, t, glow);
                    }
                    if i == fired && flash > 0.0 {
                        let at = gun * Mat4::from_translation(o + Vec3::Z * (turret.muzzle - back));
                        if plasma {
                            plasma_flash(at, flash, out, glow);
                        } else {
                            muzzle_flash(at, flash, t.since_fire, out);
                        }
                    }
                }
            }
            Rig::Cradle { orb, size } => {
                // Claws spring open as the orb leaves, then settle; while the
                // gun recharges the orb gathers between them.
                let reload = crate::combat::gun(turret.kind).reload;
                let charge = (t.since_fire / reload).clamp(0.0, 1.0);
                let open = 0.42 * kick + 0.03 * (t.age * 3.0).sin();
                for &o in &turret.barrels {
                    let out_dir = Vec3::new(o.x, o.y, 0.0).normalize_or_zero();
                    let roll = out_dir.y.atan2(out_dir.x) - FRAC_PI_2;
                    let axis = Vec3::Z.cross(out_dir).normalize_or_zero();
                    let m = gun
                        * Mat4::from_translation(o)
                        * Mat4::from_axis_angle(axis, -open)
                        * Mat4::from_rotation_z(roll);
                    out.push((turret.barrel, Instance::new(m, tint)));
                }
                if alive && t.since_fire > 0.04 {
                    let c = gun * Mat4::from_translation(Vec3::Z * orb);
                    let full = charge >= 1.0;
                    let s = size
                        * (0.2 + 0.8 * charge.powf(0.6))
                        * if full {
                            1.0 + 0.06 * (t.age * 13.0).sin()
                        } else {
                            1.0
                        };
                    // The orb is light around a small white heart.
                    let at = c.transform_point3(Vec3::ZERO);
                    glow.push(Particle::glow(
                        at,
                        s * 1.7,
                        ithessar::PLASMA,
                        1.0 + 0.8 * charge,
                        Vec3::ZERO,
                    ));
                    glow.push(Particle::glow(
                        at,
                        s * 0.8,
                        ithessar::PLASMA_HOT,
                        1.6 + charge,
                        Vec3::ZERO,
                    ));
                    out.push((
                        MeshId::SPHERE,
                        Instance::new(
                            c * Mat4::from_scale(Vec3::splat(s * 0.25)),
                            ithessar::PLASMA_HOT.extend(6.0),
                        ),
                    ));
                    // Arcs from each claw's tip into the orb, flickering.
                    for (k, &o) in turret.barrels.iter().enumerate() {
                        let beat = (t.age * 23.0 + k as f32 * 2.1).sin();
                        if beat < 0.1 - charge {
                            continue;
                        }
                        let tip = o * 0.9 + Vec3::Z * (orb + 0.15);
                        let to = Vec3::Z * orb - tip;
                        let len = to.length();
                        let q = Quat::from_rotation_arc(Vec3::Y, to / len);
                        out.push((
                            MeshId::GEM,
                            Instance::new(
                                gun * Mat4::from_scale_rotation_translation(
                                    Vec3::new(0.06, len, 0.06),
                                    q,
                                    tip + to * 0.5,
                                ),
                                ithessar::PLASMA_HOT.extend(4.0),
                            ),
                        ));
                    }
                }
            }
            Rig::Spinner => {
                let turn = Mat4::from_rotation_z(t.spin);
                for (i, &o) in turret.barrels.iter().enumerate() {
                    out.push((
                        turret.barrel,
                        Instance::new(gun * turn * Mat4::from_translation(o), tint),
                    ));
                    // Each needle's tip glows with the spin.
                    if alive && t.spool > 0.02 {
                        let tip = gun * turn * Mat4::from_translation(o + Vec3::Z * turret.muzzle);
                        let hot = if i == fired { flash } else { 0.0 };
                        out.push((
                            MeshId::SPHERE,
                            Instance::new(
                                tip * Mat4::from_scale(Vec3::splat(
                                    0.1 + 0.12 * t.spool + 0.3 * hot,
                                )),
                                ithessar::PLASMA_HOT.extend(2.0 + 3.0 * t.spool),
                            ),
                        ));
                    }
                }
                if flash > 0.0 {
                    let o = turret.barrels[fired];
                    let at = gun * turn * Mat4::from_translation(o + Vec3::Z * turret.muzzle);
                    plasma_flash(at, flash * 0.7, out, glow);
                }
            }
        }
    }
}

/// A Knell ringing up: its pipes light one by one, back to front, the
/// crystal in its crown brightens, and light gathers in the bell's mouth.
/// The pipes breathe faintly even at rest.
fn knell_charge(
    t: &Tank,
    tm: Mat4,
    gun: Mat4,
    out: &mut Vec<(MeshId, Instance)>,
    glow: &mut Vec<Particle>,
) {
    use resonant::{
        CRYSTAL_LIGHT, CRYSTAL_LIGHT_HOT, KNELL_CROWN, KNELL_LIP, KNELL_PIPE_BASE, KNELL_PIPE_R,
        KNELL_PIPE_Z, KNELL_PIPES,
    };
    let c = t.charge;
    let n = KNELL_PIPES.len() as f32;
    let full = c > 0.97;
    let beat = if full {
        1.0 + 0.2 * (t.age * 40.0).sin()
    } else {
        1.0
    };
    for (k, &(x, h)) in KNELL_PIPES.iter().enumerate() {
        let lit = (c * n - k as f32).clamp(0.0, 1.0);
        let rest = 0.15 + 0.1 * (t.age * 1.3 + k as f32 * 0.7).sin();
        let strength = (rest + 3.5 * lit) * beat;
        // A slit of light up the pipe, front and back.
        for side in [1.0, -1.0] {
            let mid = Vec3::new(
                x,
                KNELL_PIPE_BASE + h * 0.55,
                KNELL_PIPE_Z + side * KNELL_PIPE_R * 0.9,
            );
            out.push((
                MeshId::CYLINDER,
                Instance::new(
                    tm * Mat4::from_scale_rotation_translation(
                        Vec3::new(0.05, h * 0.8, 0.05),
                        Quat::IDENTITY,
                        mid,
                    ),
                    CRYSTAL_LIGHT.extend(strength),
                ),
            ));
        }
        if lit > 0.0 {
            let top = tm.transform_point3(Vec3::new(x, KNELL_PIPE_BASE + h + 0.1, KNELL_PIPE_Z));
            glow.push(Particle::glow(
                top,
                0.3 + 0.3 * lit,
                CRYSTAL_LIGHT,
                1.4 * lit,
                Vec3::ZERO,
            ));
        }
    }
    if c <= 0.0 {
        return;
    }
    let crown = gun.transform_point3(Vec3::Z * (KNELL_CROWN - 0.35));
    glow.push(Particle::glow(
        crown,
        0.4 + 0.8 * c,
        CRYSTAL_LIGHT,
        2.0 * c * beat,
        Vec3::ZERO,
    ));
    let mouth = gun.transform_point3(Vec3::Z * (crate::sim::TurretKind::Knell.reach() + 0.15));
    glow.push(Particle::glow(
        mouth,
        KNELL_LIP * (0.4 + 1.4 * c) * beat,
        CRYSTAL_LIGHT,
        2.2 * c,
        Vec3::ZERO,
    ));
    glow.push(Particle::glow(
        mouth,
        KNELL_LIP * 0.7 * c,
        CRYSTAL_LIGHT_HOT,
        2.6 * c * c,
        Vec3::ZERO,
    ));
}

fn ithessar_sag() -> f32 {
    crate::sim::HOVER_SAG
}

/// Plasma leaving a crystal: a hot white bead in a rose bloom, and a short
/// spike forward. No smoke, no fire.
fn plasma_flash(at: Mat4, f: f32, out: &mut Vec<(MeshId, Instance)>, glow: &mut Vec<Particle>) {
    // A flash of light at the muzzle, a jet of it thrown forward, and a
    // small white-hot heart.
    let p = at.transform_point3(Vec3::ZERO);
    let fwd = at.transform_vector3(Vec3::Z).normalize_or_zero();
    glow.push(Particle::glow(
        p,
        0.5 + 1.0 * f,
        ithessar::PLASMA,
        2.2 * f,
        Vec3::ZERO,
    ));
    glow.push(Particle::glow(
        p,
        0.3 + 0.3 * f,
        ithessar::PLASMA_HOT,
        2.8 * f,
        Vec3::ZERO,
    ));
    glow.push(Particle::glow(
        p + fwd * 0.7 * f,
        0.25 * f,
        ithessar::PLASMA_HOT,
        2.4 * f,
        fwd * 3.0,
    ));
    out.push((
        MeshId::SPHERE,
        Instance::new(
            at * Mat4::from_scale(Vec3::splat(0.15 * f + 0.03)),
            ithessar::PLASMA_HOT.extend(6.0),
        ),
    ));
}

/// (barrel travel 0..1, flash 0..1) for a time since firing.
fn recoil_curve(t: f32) -> (f32, f32) {
    let kick = if t < 0.05 {
        t / 0.05
    } else {
        (-(t - 0.05) * 5.0).exp()
    };
    let flash = (1.0 - t / 0.12).max(0.0);
    (kick.clamp(0.0, 1.0), flash)
}

fn muzzle_flash(at: Mat4, f: f32, t: f32, out: &mut Vec<(MeshId, Instance)>) {
    let hot = Vec4::new(1.0, 0.5, 0.15, 2.6);
    let core = Vec4::new(1.0, 0.85, 0.55, 3.5);
    let grow = 0.6 + (1.0 - f) * 0.8;
    // Forward spike and three petals splayed around the muzzle.
    out.push((
        MeshId::GEM,
        Instance::new(
            at * Mat4::from_scale_rotation_translation(
                Vec3::new(0.55, 2.2 * grow, 0.55) * f.sqrt(),
                Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
                Vec3::Z * 0.9 * grow,
            ),
            hot,
        ),
    ));
    for k in 0..3 {
        let a = k as f32 / 3.0 * std::f32::consts::TAU + t * 20.0;
        let q = Quat::from_rotation_z(a) * Quat::from_rotation_x(1.1);
        out.push((
            MeshId::GEM,
            Instance::new(
                at * Mat4::from_scale_rotation_translation(
                    Vec3::new(0.3, 1.3 * grow, 0.3) * f,
                    q,
                    q * Vec3::Y * 0.45 * grow,
                ),
                hot,
            ),
        ));
    }
    out.push((
        MeshId::SPHERE,
        Instance::new(at * Mat4::from_scale(Vec3::splat(0.8 * f + 0.1)), core),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every tank, each hull with each turret, standing with its turret
    /// turned to `yaw`: where coplanar faces show.
    fn tank_fights(yaw: f32) -> Vec<String> {
        use crate::render::mesh::CpuMeshes;
        use crate::render::zfight::Soup;
        use crate::sim::World;
        let mut meshes = CpuMeshes::new();
        let art = TankArt::upload(&mut meshes);
        let names = mesh_names(&art);
        let mut out = Vec::new();
        for hull in [
            HullKind::Kataphrakt,
            HullKind::Kastron,
            HullKind::Sethuun,
            HullKind::Vigil,
            HullKind::Gravis,
        ] {
            for turret in TurretKind::ALL {
                let mut w = World::empty(crate::map::Map::empty(40.0));
                w.spawn_tank(Vec3::ONE, Vec2::splat(20.0), 0.0, hull, turret);
                let mut t = w.tanks[0].clone();
                t.turret_rel = yaw;
                t.since_fire = 100.0;
                let mut inst = Vec::new();
                art.instances(&t, &mut inst, &mut Vec::new());
                let mut soup = Soup::new();
                soup.add_instances(&meshes.0, &inst, |id| {
                    names
                        .get(&id)
                        .cloned()
                        .unwrap_or_else(|| format!("mesh {}", id.0))
                });
                for f in soup.fights() {
                    let at = f.at - t.pos;
                    out.push(format!(
                        "{hull:?}+{turret:?}: {} / {}: {:.4} m² at ({:.2}, {:.2}, {:.2})",
                        f.a, f.b, f.area, at.x, at.y, at.z
                    ));
                }
            }
        }
        out
    }

    /// No tank has coplanar faces that show. The turret is turned off the
    /// hull's axes, so nothing lines up with it by accident.
    #[test]
    fn tanks_dont_z_fight() {
        let f = tank_fights(0.7);
        assert!(f.is_empty(), "{}", f.join("\n"));
    }

    fn mesh_names(art: &TankArt) -> std::collections::HashMap<MeshId, String> {
        let mut n = std::collections::HashMap::new();
        for (id, s) in [
            (MeshId::CUBE, "cube"),
            (MeshId::CYLINDER, "cylinder"),
            (MeshId::SPHERE, "sphere"),
            (MeshId::GEM, "gem"),
            (MeshId::CANOPY, "canopy"),
        ] {
            n.insert(id, s.to_string());
        }
        for (i, h) in art.hulls.iter().enumerate() {
            n.insert(h.body, format!("hull{i} body"));
            if let Some(l) = h.heat_light {
                n.insert(l, format!("hull{i} heat light"));
            }
            n.insert(h.link, format!("hull{i} link"));
            n.insert(h.road, format!("hull{i} road wheel"));
            n.insert(h.sprocket, format!("hull{i} sprocket"));
            n.insert(h.idler, format!("hull{i} idler"));
        }
        let o = &art.hover;
        for (id, s) in [
            (o.body, "body"),
            (o.glow, "glow"),
            (o.emitter, "emitter"),
            (o.lens, "lens"),
            (o.shard, "shard"),
        ] {
            n.insert(id, format!("sethuun {s}"));
        }
        let g = &art.vigil;
        n.insert(g.light, "vigil light".into());
        n.insert(g.heart, "vigil heart".into());
        n.insert(g.bell, "vigil bell".into());
        for (i, (p, _)) in g.plates.iter().enumerate() {
            n.insert(*p, format!("vigil plate{i}"));
        }
        for t in &art.turrets {
            let k = t.kind;
            n.insert(t.head, format!("{k:?} head"));
            if let Some(l) = t.head_light {
                n.insert(l, format!("{k:?} head light"));
            }
            if let Some(l) = t.barrel_light {
                n.insert(l, format!("{k:?} barrel light"));
            }
            n.insert(t.mantlet, format!("{k:?} mantlet"));
            n.insert(t.barrel, format!("{k:?} barrel"));
        }
        n
    }

    #[test]
    fn kit_builds_with_sane_geometry() {
        let kit = KitMeshes::build();
        let tris = kit.triangle_count();
        assert!(tris > 2000 && tris < 200_000, "kit has {tris} triangles");
        for (h, parts) in &kit.hulls {
            for m in std::iter::once(&h.body).chain(parts.iter()) {
                assert!(
                    m.verts.iter().all(|v| v
                        .pos
                        .iter()
                        .chain(v.normal.iter())
                        .all(|x| x.is_finite()))
                );
                assert!(
                    m.verts
                        .iter()
                        .all(|v| (glam::Vec3::from(v.normal).length() - 1.0).abs() < 1e-3)
                );
            }
            // Treads: the bottom run must sit on the ground.
            let mut wheels = h.running.road.clone();
            wheels.push(h.running.sprocket);
            wheels.push(h.running.idler);
            let belt = Belt::new(&wheels, LINK_HALF);
            let lowest = (0..200)
                .map(|i| belt.at(belt.length() * i as f32 / 200.0).pos.y)
                .fold(f32::INFINITY, f32::min);
            assert!(
                (lowest - LINK_HALF).abs() < 0.01,
                "tread bottom at {lowest}"
            );
        }
    }

    #[test]
    fn physics_chassis_matches_the_art() {
        use crate::sim::{Chassis, HullKind};
        for (kind, h) in [
            (HullKind::Kataphrakt, model::kataphrakt()),
            (HullKind::Kastron, model::kastron()),
            (HullKind::Gravis, uthenn::gravis()),
        ] {
            let c = Chassis::of(kind);
            let r = &h.running;
            assert_eq!(c.track_x, r.track_x, "{kind:?}");
            assert_eq!(c.track_width, r.track_width, "{kind:?}");
            let road: Vec<f32> = r.road.iter().map(|w| w.center.x).collect();
            assert_eq!(c.road_z, road.as_slice(), "{kind:?}");
            assert!(
                (c.road_y - r.road[0].center.y).abs() < 1e-6
                    && (c.road_r - r.road[0].radius).abs() < 1e-6
            );
            // Front and rear end wheels (sprocket or idler, whichever sits at that end).
            let (front, rear) = if r.sprocket.center.x > 0.0 {
                (r.sprocket, r.idler)
            } else {
                (r.idler, r.sprocket)
            };
            assert_eq!(
                c.front,
                (front.center.x, front.center.y, front.radius),
                "{kind:?} front"
            );
            assert_eq!(
                c.rear,
                (rear.center.x, rear.center.y, rear.radius),
                "{kind:?} rear"
            );
        }
    }

    /// The sim's muzzle (where shots and the beam start) is the tip of the
    /// barrel that's drawn, for every turret.
    #[test]
    fn muzzle_is_at_the_barrel_tip() {
        let kit = KitMeshes::build();
        for (t, kind) in kit.turrets.iter().zip(TurretKind::ALL) {
            assert_eq!(t.muzzle, kind.reach(), "{kind:?}");
            if t.rig != Rig::Recoil {
                // Prongs and spinning needles: the muzzle is on the gun's
                // axis, out by the prong tips, not on any one prong.
                continue;
            }
            let tip = t
                .barrel
                .verts
                .iter()
                .map(|v| v.pos[2])
                .fold(f32::NEG_INFINITY, f32::max);
            assert!(
                (tip - kind.reach()).abs() < 0.12,
                "{kind:?}: barrel tip at {tip}, muzzle at {}",
                kind.reach()
            );
        }
    }

    #[test]
    fn recoil_snaps_back_then_returns() {
        assert_eq!(recoil_curve(0.0).0, 0.0);
        assert!((recoil_curve(0.05).0 - 1.0).abs() < 1e-5);
        assert!(recoil_curve(1.0).0 < 0.05);
        assert_eq!(recoil_curve(0.5).1, 0.0);
    }
}

/// A model's heat lights on the GPU, if it has any.
fn light_mesh(r: &mut impl MeshStore, m: &MeshData) -> Option<MeshId> {
    (!m.indices.is_empty()).then(|| r.add_mesh(m))
}

/// Tint and glow strength for a tank's heat lights.
fn heat_tint(t: &Tank) -> Vec4 {
    uthenn::heat_glow(t.heat, t.overheated, t.dead, t.age)
}

/// Light gathered at the lens while the beam is on: a shimmering green
/// halo round a white-hot core, hotter as the gun heats.
fn beam_flare(at: Vec3, t: &Tank, glow: &mut Vec<Particle>) {
    let shimmer = 1.0 + 0.18 * (t.age * 53.0).sin() * (t.age * 29.0).sin();
    let hot = 1.0 + t.heat;
    glow.push(Particle::glow(
        at,
        0.75 * shimmer,
        uthenn::LASER_LIGHT,
        1.6 * hot,
        Vec3::ZERO,
    ));
    glow.push(Particle::glow(
        at,
        0.3 * shimmer,
        Vec3::new(0.85, 1.0, 0.9),
        2.4 * hot,
        Vec3::ZERO,
    ));
}
