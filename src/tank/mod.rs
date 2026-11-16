//! Tank visuals: the modular kit (hulls x turrets) uploaded once, and the
//! per-frame instance list for an animated tank.

pub mod model;
pub mod track;

use glam::{Mat4, Quat, Vec3, Vec4};

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
        KitMeshes { hulls: vec![hull(model::lancer()), hull(model::bastion())], turrets: vec![model::lance(), model::maul()] }
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
    belt: Belt,
    links: usize,
    mount: Vec3,
}

struct TurretArt {
    head: MeshId,
    mantlet: MeshId,
    barrel: MeshId,
    pivot: Vec3,
    barrels: Vec<Vec3>,
    muzzle: f32,
    recoil: f32,
}

/// The tank kit on the GPU.
pub struct TankArt {
    hulls: Vec<HullArt>,
    turrets: Vec<TurretArt>,
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
                HullArt {
                    body: r.add_mesh(&h.body),
                    link: r.add_mesh(&link),
                    road: r.add_mesh(&road),
                    sprocket: r.add_mesh(&sprocket),
                    idler: r.add_mesh(&idler),
                    running: run,
                    belt,
                    links,
                    mount: h.mount,
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
            })
            .collect();
        TankArt { hulls, turrets }
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

        // Running gear stays on the ground; the body above rides the suspension.
        let base = Mat4::from_translation(t.pos3()) * Mat4::from_rotation_y(t.yaw);
        let speed = t.speed().abs();
        let rumble = ((t.track_dist[0] + t.track_dist[1]) * 7.0).sin() * 0.008 * (speed / 15.0).min(1.0);
        let sprung = base
            * Mat4::from_translation(Vec3::new(0.0, 0.85 + rumble, 0.0))
            * Mat4::from_rotation_x(t.pitch)
            * Mat4::from_rotation_z(t.roll)
            * Mat4::from_translation(Vec3::NEG_Y * 0.85);
        out.push((hull.body, Instance::new(sprung, tint)));

        let run = &hull.running;
        for (side, sign) in [(0usize, 1.0f32), (1, -1.0)] {
            let dist = t.track_dist[side];
            // Mirror the -x side so wheel hubs and link details face outward.
            let flip = Mat4::from_scale(Vec3::new(sign, 1.0, 1.0));
            let x = sign * run.track_x;
            let wheel = |w: &Wheel| {
                base * Mat4::from_translation(Vec3::new(x, w.center.y, w.center.x)) * flip * Mat4::from_rotation_x(dist / w.radius)
            };
            for w in &run.road {
                out.push((hull.road, Instance::new(wheel(w), tint)));
            }
            out.push((hull.sprocket, Instance::new(wheel(&run.sprocket), tint)));
            out.push((hull.idler, Instance::new(wheel(&run.idler), tint)));

            let pitch = hull.belt.length() / hull.links as f32;
            for i in 0..hull.links {
                let p = hull.belt.at(i as f32 * pitch + dist);
                // Link frame: x across, y = outward normal, z = along the belt.
                let ny = Vec3::new(0.0, p.normal.y, p.normal.x);
                let nz = Vec3::X.cross(ny);
                let m = Mat4::from_cols(Vec4::X, ny.extend(0.0), nz.extend(0.0), Vec3::new(x, p.pos.y, p.pos.x).extend(1.0));
                out.push((hull.link, Instance::new(base * m * flip, plain)));
            }
        }

        // Turret yaws on the hull's ring; the gun pitches about its trunnion.
        let tm = sprung * Mat4::from_translation(hull.mount) * Mat4::from_rotation_y(t.turret_rel);
        out.push((turret.head, Instance::new(tm, tint)));
        let gun = tm * Mat4::from_translation(turret.pivot) * Mat4::from_rotation_x(-t.gun_pitch);
        out.push((turret.mantlet, Instance::new(gun, tint)));

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
    fn recoil_snaps_back_then_returns() {
        assert_eq!(recoil_curve(0.0).0, 0.0);
        assert!((recoil_curve(0.05).0 - 1.0).abs() < 1e-5);
        assert!(recoil_curve(1.0).0 < 0.05);
        assert_eq!(recoil_curve(0.5).1, 0.0);
    }
}
