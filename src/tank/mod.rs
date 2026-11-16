//! Tank visuals: the modular kit (hulls x turrets) uploaded once, and the
//! per-frame instance list for an animated tank.

pub mod model;
pub mod track;

use glam::{Mat4, Quat, Vec2, Vec3, Vec4};

use crate::render::Renderer;
use crate::render::mesh::{Instance, MeshData, MeshId};
use crate::sim::{HullKind, Tank, TurretKind};
use model::{HullModel, Running, TurretModel};
use track::{Belt, Wheel};

/// Target spacing between tread links.
const LINK_PITCH: f32 = 0.19;
/// Half the link thickness: the belt path runs through the link's middle.
const LINK_HALF: f32 = 0.04;

/// CPU-side meshes for the whole kit, before upload.
pub struct KitMeshes {
    hulls: Vec<(HullModel, [MeshData; 4])>,
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
        let mut kit = KitMeshes {
            hulls: vec![hull(model::lancer()), hull(model::bastion())],
            turrets: vec![model::lance(), model::maul()],
        };
        // Bake contact shadows into every part. Neighboring parts are separate
        // instances, so each bake also sees them as occluders: the hull sees a
        // turret on its ring and the tracks under its sides, the turret sees
        // the hull roof under it.
        let lance_head = kit.turrets[0].head.clone();
        for (h, parts) in &mut kit.hulls {
            let mut occ = MeshData::default();
            occ.append_keep(&lance_head, glam::Mat4::from_translation(h.mount));
            let r = &h.running;
            for s in [-1.0f32, 1.0] {
                let x = s * r.track_x;
                let (z0, z1) = (r.idler.center.x.min(r.sprocket.center.x) - 0.4, r.idler.center.x.max(r.sprocket.center.x) + 0.4);
                occ.add_box(glam::Vec3::new(x - r.track_width * 0.5, 0.0, z0), glam::Vec3::new(x + r.track_width * 0.5, 1.05, z1), glam::Vec4::ONE, glam::Vec4::ONE);
            }
            h.body.bake_ao_with(&occ, 0.8, 1.8, 0.15);
            for part in parts.iter_mut() {
                part.bake_ao(0.15, 1.4, 0.35);
            }
        }
        // The hull roof under every turret.
        let mut roof = MeshData::default();
        roof.add_box(glam::Vec3::new(-2.0, -0.4, -2.5), glam::Vec3::new(2.0, 0.0, 2.5), glam::Vec4::ONE, glam::Vec4::ONE);
        for t in &mut kit.turrets {
            t.head.bake_ao_with(&roof, 0.7, 1.8, 0.15);
            let mut head_in_gun = MeshData::default();
            head_in_gun.append_keep(&t.head, glam::Mat4::from_translation(-t.pivot));
            t.mantlet.bake_ao_with(&head_in_gun, 0.3, 1.4, 0.3);
            t.barrel.bake_ao(0.2, 1.4, 0.35);
        }
        kit
    }

    #[cfg(test)]
    pub fn triangle_count(&self) -> usize {
        let h: usize = self.hulls.iter().map(|(h, p)| h.body.indices.len() + p.iter().map(|m| m.indices.len()).sum::<usize>()).sum();
        let t: usize = self.turrets.iter().map(|t| t.head.indices.len() + t.mantlet.indices.len() + t.barrel.indices.len()).sum();
        (h + t) / 3
    }
}

struct HullArt {
    body: MeshId,
    link: MeshId,
    road: MeshId,
    sprocket: MeshId,
    idler: MeshId,
    running: Running,
    links: usize,
    mount: Vec3,
    suspension: Vec<Strut>,
}

/// One road wheel's exposed suspension, for the +x side (mirrored for -x).
/// The pivot and shock top ride with the hull; the hub stays on the track.
struct Strut {
    /// Hub in the ground frame.
    hub: Vec3,
    /// Arm pivot and shock top in the hull (sprung) frame.
    pivot: Vec3,
    top: Vec3,
}

impl Strut {
    fn layout(run: &Running, top_y: f32) -> Vec<Strut> {
        let x = run.track_x - run.track_width * 0.5 - 0.06;
        run.road
            .iter()
            .map(|w| {
                // Long trailing arms, all raked the same way like a row of legs.
                let hub = Vec3::new(x, w.center.y, w.center.x);
                let pivot = hub + Vec3::new(0.0, 0.36, 0.5);
                let top = Vec3::new(x, top_y, w.center.x + 0.38);
                Strut { hub, pivot, top }
            })
            .collect()
    }
}

/// Shared machinery meshes.
struct Machinery {
    arm: MeshId,
    cylinder: MeshId,
    rod: MeshId,
}

struct TurretArt {
    head: MeshId,
    mantlet: MeshId,
    barrel: MeshId,
    pivot: Vec3,
    barrels: Vec<Vec3>,
    muzzle: f32,
    recoil: f32,
    ram_anchor: Vec3,
    ram_lug: Vec3,
}

/// The tank kit on the GPU.
pub struct TankArt {
    hulls: Vec<HullArt>,
    turrets: Vec<TurretArt>,
    mech: Machinery,
}

impl TankArt {
    pub fn upload(r: &mut Renderer) -> TankArt {
        let kit = KitMeshes::build();
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
                let suspension = Strut::layout(&run, h.shock_top);
                HullArt {
                    body: r.add_mesh(&h.body),
                    link: r.add_mesh(&link),
                    road: r.add_mesh(&road),
                    sprocket: r.add_mesh(&sprocket),
                    idler: r.add_mesh(&idler),
                    running: run,
                    links,
                    mount: h.mount,
                    suspension,
                }
            })
            .collect();
        let turrets = kit
            .turrets
            .into_iter()
            .map(|t| TurretArt {
                head: r.add_mesh(&t.head),
                mantlet: r.add_mesh(&t.mantlet),
                barrel: r.add_mesh(&t.barrel),
                pivot: t.pivot,
                barrels: t.barrels,
                muzzle: t.muzzle,
                recoil: t.recoil,
                ram_anchor: t.ram_anchor,
                ram_lug: t.ram_lug,
            })
            .collect();
        let mech = Machinery {
            arm: r.add_mesh(&model::arm()),
            cylinder: r.add_mesh(&model::cylinder()),
            rod: r.add_mesh(&model::rod()),
        };
        TankArt { hulls, turrets, mech }
    }

    /// Append every instance needed to draw `t` this frame.
    pub fn instances(&self, t: &Tank, out: &mut Vec<(MeshId, Instance)>) {
        let hull = &self.hulls[match t.hull_kind {
            HullKind::Lancer => 0,
            HullKind::Bastion => 1,
        }];
        let turret = &self.turrets[match t.turret_kind {
            TurretKind::Lance => 0,
            TurretKind::Maul => 1,
        }];
        let tint = t.color.extend(0.0);
        let plain = Vec4::new(1.0, 1.0, 1.0, 0.0);

        // The whole kit hangs off the physics body (body frame = art frame, ground at y=0);
        // road wheels move on their own suspension below it.
        let base = Mat4::from_rotation_translation(t.rot, t.pos);
        let sprung = base;
        out.push((hull.body, Instance::new(sprung, tint)));

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
                    let lift = t.wheel_lift.get(side * per_side + k).copied().unwrap_or(0.0);
                    Wheel { center: w.center + Vec2::new(0.0, lift), radius: w.radius }
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
                base * Mat4::from_translation(Vec3::new(x, w.center.y, w.center.x)) * flip * Mat4::from_rotation_x(dist / w.radius)
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
                let m = Mat4::from_cols(Vec4::X, ny.extend(0.0), nz.extend(0.0), Vec3::new(x, p.pos.y, p.pos.x).extend(1.0));
                out.push((hull.link, Instance::new(base * m * flip, plain)));
            }

            // Suspension: arms swing and shocks telescope as the hull moves on its springs.
            let across = base.transform_vector3(Vec3::X);
            let mx = |v: Vec3| Vec3::new(v.x * sign, v.y, v.z);
            for (k, st) in hull.suspension.iter().enumerate() {
                // The hub follows its wheel's real suspension travel, so the arm swings with it.
                let lifted = Vec3::new(st.hub.x, road[k].center.y, st.hub.z);
                let hub = base.transform_point3(mx(lifted));
                let pivot = sprung.transform_point3(mx(st.pivot));
                let top = sprung.transform_point3(mx(st.top));
                let reach = hub - pivot;
                let arm = aim(pivot, reach, across) * Mat4::from_scale(Vec3::new(1.0, 1.0, reach.length() / model::ARM_LEN));
                out.push((self.mech.arm, Instance::new(arm, plain)));
                let low = pivot + reach * 0.55;
                self.hydraulic(top, low, across, 1.0, tint, out);
            }
        }

        // Turret yaws on the hull's ring; the gun pitches about its trunnion.
        let tm = sprung * Mat4::from_translation(hull.mount) * Mat4::from_rotation_y(t.turret_rel);
        out.push((turret.head, Instance::new(tm, tint)));
        let gun = tm * Mat4::from_translation(turret.pivot) * Mat4::from_rotation_x(-(t.gun_pitch + t.gun_sway));
        out.push((turret.mantlet, Instance::new(gun, tint)));
        // Elevation ram between the turret roof and the gun cradle.
        let across = tm.transform_vector3(Vec3::X);
        self.hydraulic(tm.transform_point3(turret.ram_anchor), gun.transform_point3(turret.ram_lug), across, 1.35, tint, out);

        let fired = t.fire_barrel % turret.barrels.len();
        let (kick, flash) = recoil_curve(t.since_fire);
        for (i, &o) in turret.barrels.iter().enumerate() {
            let back = if i == fired { kick * turret.recoil } else { 0.0 };
            out.push((turret.barrel, Instance::new(gun * Mat4::from_translation(o - Vec3::Z * back), tint)));
            if i == fired && flash > 0.0 {
                muzzle_flash(gun * Mat4::from_translation(o + Vec3::Z * (turret.muzzle - back)), flash, t.since_fire, out);
            }
        }
    }
}

impl TankArt {
    /// A cylinder anchored at `a` and its rod anchored at `b`, telescoping between them.
    fn hydraulic(&self, a: Vec3, b: Vec3, across: Vec3, scale: f32, tint: Vec4, out: &mut Vec<(MeshId, Instance)>) {
        let s = Mat4::from_scale(Vec3::splat(scale));
        out.push((self.mech.cylinder, Instance::new(aim(a, b - a, across) * s, tint)));
        out.push((self.mech.rod, Instance::new(aim(b, a - b, across) * s, tint)));
    }
}

/// Frame at `origin` whose +z points along `dir`, with x kept close to `across`.
fn aim(origin: Vec3, dir: Vec3, across: Vec3) -> Mat4 {
    let z = dir.normalize_or_zero();
    let x = (across - z * across.dot(z)).normalize_or_zero();
    let y = z.cross(x);
    Mat4::from_cols(x.extend(0.0), y.extend(0.0), z.extend(0.0), origin.extend(1.0))
}

/// (barrel travel 0..1, flash 0..1) for a time since firing.
fn recoil_curve(t: f32) -> (f32, f32) {
    let kick = if t < 0.05 { t / 0.05 } else { (-(t - 0.05) * 5.0).exp() };
    let flash = (1.0 - t / 0.12).max(0.0);
    (kick.clamp(0.0, 1.0), flash)
}

fn muzzle_flash(at: Mat4, f: f32, t: f32, out: &mut Vec<(MeshId, Instance)>) {
    let hot = Vec4::new(1.0, 0.5, 0.15, 2.6);
    let core = Vec4::new(1.0, 0.85, 0.55, 3.5);
    let grow = 0.6 + (1.0 - f) * 0.8;
    // Forward spike and three petals splayed around the muzzle.
    out.push((MeshId::GEM, Instance::new(at * Mat4::from_scale_rotation_translation(Vec3::new(0.55, 2.2 * grow, 0.55) * f.sqrt(), Quat::from_rotation_x(std::f32::consts::FRAC_PI_2), Vec3::Z * 0.9 * grow), hot)));
    for k in 0..3 {
        let a = k as f32 / 3.0 * std::f32::consts::TAU + t * 20.0;
        let q = Quat::from_rotation_z(a) * Quat::from_rotation_x(1.1);
        out.push((MeshId::GEM, Instance::new(at * Mat4::from_scale_rotation_translation(Vec3::new(0.3, 1.3 * grow, 0.3) * f, q, q * Vec3::Y * 0.45 * grow), hot)));
    }
    out.push((MeshId::SPHERE, Instance::new(at * Mat4::from_scale(Vec3::splat(0.8 * f + 0.1)), core)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kit_builds_with_sane_geometry() {
        let kit = KitMeshes::build();
        let tris = kit.triangle_count();
        assert!(tris > 2000 && tris < 200_000, "kit has {tris} triangles");
        for (h, parts) in &kit.hulls {
            for m in std::iter::once(&h.body).chain(parts.iter()) {
                assert!(m.verts.iter().all(|v| v.pos.iter().chain(v.normal.iter()).all(|x| x.is_finite())));
                assert!(m.verts.iter().all(|v| (glam::Vec3::from(v.normal).length() - 1.0).abs() < 1e-3));
            }
            // Treads: the bottom run must sit on the ground.
            let mut wheels = h.running.road.clone();
            wheels.push(h.running.sprocket);
            wheels.push(h.running.idler);
            let belt = Belt::new(&wheels, LINK_HALF);
            let lowest = (0..200).map(|i| belt.at(belt.length() * i as f32 / 200.0).pos.y).fold(f32::INFINITY, f32::min);
            assert!((lowest - LINK_HALF).abs() < 0.01, "tread bottom at {lowest}");
        }
    }

    #[test]
    fn physics_chassis_matches_the_art() {
        use crate::sim::{Chassis, HullKind};
        for (kind, h) in [(HullKind::Lancer, model::lancer()), (HullKind::Bastion, model::bastion())] {
            let c = Chassis::of(kind);
            let r = &h.running;
            assert_eq!(c.track_x, r.track_x, "{kind:?}");
            assert_eq!(c.track_width, r.track_width, "{kind:?}");
            let road: Vec<f32> = r.road.iter().map(|w| w.center.x).collect();
            assert_eq!(c.road_z, road.as_slice(), "{kind:?}");
            assert!((c.road_y - r.road[0].center.y).abs() < 1e-6 && (c.road_r - r.road[0].radius).abs() < 1e-6);
            // Front and rear end wheels (sprocket or idler, whichever sits at that end).
            let (front, rear) = if r.sprocket.center.x > 0.0 { (r.sprocket, r.idler) } else { (r.idler, r.sprocket) };
            assert_eq!(c.front, (front.center.x, front.center.y, front.radius), "{kind:?} front");
            assert_eq!(c.rear, (rear.center.x, rear.center.y, rear.radius), "{kind:?} rear");
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
