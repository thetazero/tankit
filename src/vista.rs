//! Where the menus park your tank on each map, and how they look at it.

use glam::{Vec2, Vec3};

use crate::MapKind;
use crate::map::Map;
use crate::render::Camera;
use crate::sim::{Tank, World};

/// A spot on a map to show the tank off from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vista {
    /// Where the tank parks (x, z).
    pub at: Vec2,
    /// The camera's yaw around the tank (see [`camera`]).
    pub view: f32,
}

/// Spots picked from `tests::vista_search`'s best and checked by eye:
/// open sky, the map's landmarks behind the tank, the tank in the sun.
/// One per side, so each contract's sides show the map differently.
pub(crate) fn spots(kind: MapKind) -> &'static [(f32, f32, f32)] {
    match kind {
        // The church and its bell tower; a chapel's gable. (Generated ones
        // pick their own.)
        MapKind::Sanctum(_) => &[(68.0, 113.0, 3.40), (78.0, 203.0, 4.71)],
        // Generated: each city picks its own (`Map::vistas`).
        MapKind::City(..) | MapKind::ShieldSite(..) => &[],
        // Generated: each foundry picks its own (by each side's spawns,
        // looking at the great hall).
        MapKind::Foundry(..) | MapKind::Depot(..) | MapKind::Hoard(_) => &[],
        // Generated: each mine picks its own (the Face, the adit's mouth
        // and the bridge out of it, from either side of the bridge's end).
        MapKind::Mine(..) => &[],
        // Generated: each desert picks its own (by each side's spawns,
        // looking at the Needles).
        MapKind::Desert | MapKind::Reservoir | MapKind::ShieldCamp(..) => &[],
    }
}

/// The spot the menus show `map` (of kind `kind`) from while side `side`
/// is picked: the map's own, if it picked them, else `spots`.
pub fn of(map: &Map, kind: MapKind, side: usize) -> Vista {
    if !map.vistas.is_empty() {
        let (at, view) = map.vistas[side % map.vistas.len()];
        return Vista { at, view };
    }
    let s = spots(kind);
    let (x, z, view) = s[side % s.len()];
    Vista {
        at: Vec2::new(x, z),
        view,
    }
}

/// How far the menu camera swings either side of the vista's view, radians,
/// and how fast (radians of phase a second): slow, and never far enough
/// round to lose the scene.
const SWAY: f32 = 0.22;
const SWAY_RATE: f32 = 0.12;

/// The camera's yaw `t` seconds into the menu, swaying about `view`.
pub fn sway(view: f32, t: f32) -> f32 {
    view + SWAY * (t * SWAY_RATE).sin()
}

/// Parks tank 0 at `v`, front three quarters toward the camera.
pub fn park(world: &mut World, v: Vista) {
    world.teleport(0, v.at, heading(v.view));
}

/// Distance from the tank's middle to the camera, m.
const DIST: f32 = 10.0;
/// Camera height over the tank's middle, m: low, so the scene rises behind it.
const HEIGHT: f32 = 1.4;
/// How far up the camera aims above the tank's middle, m: puts the horizon
/// low in the frame, with sky over it.
const LIFT: f32 = 1.3;

/// The tank's yaw for the camera at `view` to see its front three quarters.
pub fn heading(view: f32) -> f32 {
    view - 0.65
}

/// The menu camera: low, at yaw `angle` around `tank`, the tank right of
/// the panel and the scene over it.
pub fn camera(world: &World, tank: &Tank, angle: f32, aspect: f32) -> Camera {
    camera_at(world, tank.pos, angle, aspect)
}

fn camera_at(world: &World, at: Vec3, angle: f32, aspect: f32) -> Camera {
    let focus = at + Vec3::Y * 1.0;
    let dir = Vec3::new(angle.sin() * DIST, HEIGHT, angle.cos() * DIST);
    let len = dir.length();
    let hit = world.raycast(focus, dir / len, len);
    let pos = focus + dir / len * (hit - 0.6).max(3.0).min(len);
    let right = (focus - pos).cross(Vec3::Y).normalize_or_zero();
    let shift = (focus - pos).length() * 0.28 * (aspect / (16.0 / 9.0)).clamp(0.6, 1.2);
    Camera {
        pos,
        target: focus - right * shift + Vec3::Y * LIFT,
        fov_y: 50f32.to_radians(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::make_map;

    /// Distance along a ray to static geometry or a visual-only box
    /// (arcades, awnings: what the physics doesn't see but the camera does).
    fn cast(world: &World, origin: Vec3, dir: Vec3, max: f32) -> f32 {
        let mut best = world.raycast(origin, dir, max);
        for b in &world.map.boxes {
            let inv = dir.recip();
            let t0 = (b.min - origin) * inv;
            let t1 = (b.max - origin) * inv;
            let near = t0.min(t1).max_element();
            let far = t0.max(t1).min_element();
            if near <= far && far > 0.0 {
                best = best.min(near.max(0.0));
            }
        }
        best
    }

    /// Why a tank parked at `p` (ground height `g`), seen from yaw `view`
    /// (and either end of the sway), would look wrong, if it would: debris
    /// under it, or the ground between it and the camera dipping or rising
    /// (a dune's shaded side across the foreground).
    fn fault(world: &World, p: Vec2, g: f32, view: f32) -> Option<String> {
        let map = &world.map;
        let mut under = vec![p];
        for k in 0..8 {
            let a = k as f32 * std::f32::consts::TAU / 8.0;
            for r in [1.5, 3.0] {
                under.push(p + Vec2::new(a.sin(), a.cos()) * r);
            }
        }
        for q in &under {
            for b in &map.oboxes {
                let d = *q - b.center;
                let (s, c) = b.yaw.sin_cos();
                // Into the box's frame (the inverse of its rotation_y).
                let (lx, lz) = (d.x * c - d.y * s, d.x * s + d.y * c);
                let hit = lx.abs() < b.half.x + 0.3 && lz.abs() < b.half.y + 0.3;
                if hit && b.y1 > g + 0.05 && b.y0 < g + 3.0 {
                    return Some(format!("on a box at {:?}", b.center));
                }
            }
            for b in &map.boxes {
                if q.x > b.min.x - 0.3
                    && q.x < b.max.x + 0.3
                    && q.y > b.min.z - 0.3
                    && q.y < b.max.z + 0.3
                    && b.max.y > g + 0.05
                    && b.min.y < g + 3.0
                {
                    return Some(format!("on a box at {:?}", b.min));
                }
            }
        }
        let sun = map.atmosphere.sun_dir.normalize();
        let lit = |q: Vec2| {
            let e = 0.5;
            let dx = map.ground_height(q.x + e, q.y) - map.ground_height(q.x - e, q.y);
            let dz = map.ground_height(q.x, q.y + e) - map.ground_height(q.x, q.y - e);
            Vec3::new(-dx, 2.0 * e, -dz).normalize().dot(sun) / sun.y
        };
        for v in [view - SWAY, view, view + SWAY] {
            let to_cam = Vec2::new(v.sin(), v.cos());
            let side = Vec2::new(to_cam.y, -to_cam.x);
            let mut shaded = 0;
            for i in 0..=12 {
                for j in -2..=2 {
                    let q = p + to_cam * (i as f32 * (DIST + 4.0) / 12.0) + side * (j as f32 * 2.0);
                    let h = map.ground_height(q.x, q.y);
                    if (h - g).abs() > 2.0 {
                        return Some(format!("foreground ground at {h:.1} vs {g:.1} ({q:?})"));
                    }
                    if lit(q) < 0.6 {
                        shaded += 1;
                    }
                }
            }
            // A bump here and there is fine; a slope across the view isn't.
            if shaded > 6 {
                return Some(format!(
                    "{shaded} of 65 foreground samples in a dune's shade"
                ));
            }
        }
        None
    }

    /// Every picked spot stands clear and level, in the sun, under open sky.
    #[test]
    fn picked_spots_are_clear() {
        let mut bad = Vec::new();
        for (kind, seed) in [
            (MapKind::Reservoir, 0),
            (MapKind::Reservoir, 1),
            (
                MapKind::Sanctum(crate::map::procgen::foundry::Ground::Desert),
                0,
            ),
            (
                MapKind::Sanctum(crate::map::procgen::foundry::Ground::Desert),
                1,
            ),
            (
                MapKind::Sanctum(crate::map::procgen::foundry::Ground::Tundra),
                2,
            ),
            (
                MapKind::Sanctum(crate::map::procgen::foundry::Ground::Grassland),
                3,
            ),
            (
                MapKind::Sanctum(crate::map::procgen::foundry::Ground::Tundra),
                4,
            ),
            (
                MapKind::Sanctum(crate::map::procgen::foundry::Ground::Grassland),
                5,
            ),
            (
                MapKind::City(crate::map::procgen::Look::Imperium, Default::default()),
                0,
            ),
            (
                MapKind::City(crate::map::procgen::Look::Imperium, Default::default()),
                1,
            ),
            (
                MapKind::City(crate::map::procgen::Look::Imperium, Default::default()),
                2,
            ),
            (
                MapKind::City(
                    crate::map::procgen::Look::Imperium,
                    crate::map::procgen::setting::Setting::Tundra,
                ),
                0,
            ),
            (
                MapKind::City(
                    crate::map::procgen::Look::Resonant,
                    crate::map::procgen::setting::Setting::Grassland,
                ),
                1,
            ),
            (
                MapKind::Foundry(
                    crate::map::procgen::Look::Uthenn,
                    crate::map::procgen::foundry::Ground::Tundra,
                ),
                0,
            ),
            (
                MapKind::Foundry(
                    crate::map::procgen::Look::Imperium,
                    crate::map::procgen::foundry::Ground::Desert,
                ),
                1,
            ),
            (
                MapKind::Hoard(crate::map::procgen::foundry::Ground::Tundra),
                0,
            ),
            (
                MapKind::Depot(
                    crate::map::procgen::Look::Imperium,
                    crate::map::procgen::foundry::Ground::Desert,
                ),
                0,
            ),
            (
                MapKind::Depot(
                    crate::map::procgen::Look::Ithessar,
                    crate::map::procgen::foundry::Ground::Desert,
                ),
                1,
            ),
            (
                MapKind::Depot(
                    crate::map::procgen::Look::Resonant,
                    crate::map::procgen::foundry::Ground::Tundra,
                ),
                2,
            ),
            (
                MapKind::Depot(
                    crate::map::procgen::Look::Uthenn,
                    crate::map::procgen::foundry::Ground::Grassland,
                ),
                3,
            ),
            (
                MapKind::Mine(
                    crate::map::procgen::Look::Ithessar,
                    crate::map::procgen::foundry::Ground::Desert,
                ),
                0,
            ),
            (
                MapKind::Mine(
                    crate::map::procgen::Look::Imperium,
                    crate::map::procgen::foundry::Ground::Tundra,
                ),
                2,
            ),
            (
                MapKind::Mine(
                    crate::map::procgen::Look::Uthenn,
                    crate::map::procgen::foundry::Ground::Grassland,
                ),
                3,
            ),
            (MapKind::Desert, 0),
            (MapKind::Desert, 1),
        ]
        .into_iter()
        // (The mine is slow to build unoptimized: `cargo test --release`.)
        .filter(|&(kind, _)| !cfg!(debug_assertions) || !matches!(kind, MapKind::Mine(..)))
        {
            let world = World::new(make_map(kind, seed), 0);
            let sun = world.map.atmosphere.sun_dir.normalize();
            let n = spots(kind).len().max(world.map.vistas.len());
            for side in 0..n {
                let v = of(&world.map, kind, side);
                let g = world.map.ground_height(v.at.x, v.at.y);
                let at = Vec3::new(v.at.x, g + 0.7, v.at.y);
                let why = fault(&world, v.at, g, v.view)
                    .or_else(|| {
                        (cast(&world, at + Vec3::Y * 1.2, sun, 300.0) < 300.0)
                            .then(|| "the tank is in shadow".to_string())
                    })
                    .or_else(|| {
                        (cast(&world, at + Vec3::Y * 0.5, Vec3::Y, 60.0) < 60.0)
                            .then(|| "something overhead".to_string())
                    });
                if let Some(why) = why {
                    bad.push(format!("{kind:?} (seed {seed}) side {side}: {why}"));
                }
            }
        }
        assert!(bad.is_empty(), "{}", bad.join("\n"));
    }

    /// The fraction of the frame's scene part (right of the panel) that's
    /// sky, the median distance to what the rest hits, and the fraction
    /// that hits something within `near` m of the camera.
    fn look(world: &World, cam: &Camera, aspect: f32) -> (f32, f32, f32) {
        let fwd = (cam.target - cam.pos).normalize();
        let right = fwd.cross(Vec3::Y).normalize();
        let up = right.cross(fwd);
        let t = (cam.fov_y * 0.5).tan();
        let (mut sky, mut near, mut d) = (0, 0, Vec::new());
        let (nx, ny) = (24, 14);
        for j in 0..ny {
            for i in 0..nx {
                // x from 0.35 (just right of the panel) to 1.
                let sx = -0.3 + 1.3 * (i as f32 + 0.5) / nx as f32;
                let sy = 1.0 - 2.0 * (j as f32 + 0.5) / ny as f32;
                let dir = (fwd + right * sx * t * aspect + up * sy * t).normalize();
                let max = 400.0;
                let hit = cast(world, cam.pos, dir, max);
                if hit >= max {
                    sky += 1;
                } else {
                    if hit < 14.0 && dir.y > -0.05 {
                        near += 1;
                    }
                    d.push(hit);
                }
            }
        }
        d.sort_by(|a, b| a.total_cmp(b));
        let n = (nx * ny) as f32;
        let med = d.get(d.len() / 2).copied().unwrap_or(400.0);
        (sky as f32 / n, med, near as f32 / n)
    }

    /// Searches each map for spots to show the tank from, best first.
    /// `cargo test vista_search -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn vista_search() {
        for kind in [MapKind::Sanctum(
            crate::map::procgen::foundry::Ground::Desert,
        )] {
            let world = World::new(make_map(kind, 0), 0);
            let s = world.map.size;
            let sun = world.map.atmosphere.sun_dir.normalize();
            let mut found = Vec::new();
            let step = 5.0;
            let mut z = 8.0;
            while z < s - 8.0 {
                let mut x = 8.0;
                while x < s - 8.0 {
                    let p = Vec2::new(x, z);
                    x += step;
                    // Stand on what's there; it must be level and clear.
                    let top = 120.0;
                    let ground = |q: Vec2| {
                        top - world.raycast(Vec3::new(q.x, top, q.y), -Vec3::Y, top + 50.0)
                    };
                    let g = ground(p);
                    let level = [(2.5, 0.0), (-2.5, 0.0), (0.0, 2.5), (0.0, -2.5)]
                        .iter()
                        .all(|&(a, b)| (ground(p + Vec2::new(a, b)) - g).abs() < 0.35);
                    if !level || g < -20.0 || (g - world.map.ground_height(p.x, p.y)).abs() > 0.3 {
                        continue;
                    }
                    if let Some(fall) = world.map.fall_y
                        && g < fall + 1.0
                    {
                        continue;
                    }
                    let at = Vec3::new(p.x, g + 0.7, p.y);
                    let open = (0..8).all(|k| {
                        let a = k as f32 * std::f32::consts::TAU / 8.0;
                        let dir = Vec3::new(a.sin(), 0.0, a.cos());
                        cast(&world, at + Vec3::Y * 0.5, dir, 4.5) >= 4.5
                    });
                    // Nothing overhead.
                    if cast(&world, at + Vec3::Y * 0.5, Vec3::Y, 60.0) < 60.0 {
                        continue;
                    }
                    if !open {
                        continue;
                    }
                    // The tank in sunlight.
                    if cast(&world, at + Vec3::Y * 1.2, sun, 300.0) < 300.0 {
                        continue;
                    }
                    for k in 0..24 {
                        let view = k as f32 * std::f32::consts::TAU / 24.0;
                        if fault(&world, p, g, view).is_some() {
                            continue;
                        }
                        let aspect = 16.0 / 9.0;
                        let cam = camera_at(&world, at, view, aspect);
                        let focus = at + Vec3::Y;
                        let back = cam.pos - focus;
                        if (cam.pos - at).length() < DIST - 0.5
                            || cast(&world, focus, back.normalize(), back.length()) < back.length()
                            || cast(&world, cam.pos, Vec3::Y, 60.0) < 60.0
                        {
                            continue;
                        }
                        // Lit from the camera's side.
                        let to_cam = Vec3::new(view.sin(), 0.0, view.cos());
                        let lit = to_cam.dot(Vec3::new(sun.x, 0.0, sun.z).normalize());
                        let (sky, med, near) = look(&world, &cam, aspect);
                        let score = -(sky - 0.35).abs() * 4.0 + (med.min(150.0) / 30.0).ln()
                            - near * 6.0
                            + lit.clamp(-0.5, 0.7);
                        found.push((score, p, view, sky, med, near, lit));
                    }
                }
                z += step;
            }
            found.sort_by(|a, b| b.0.total_cmp(&a.0));
            // Best few, spread apart.
            let mut kept: Vec<(f32, Vec2, f32, f32, f32, f32, f32)> = Vec::new();
            for f in found {
                if kept.iter().all(|k| k.1.distance(f.1) > 25.0) {
                    kept.push(f);
                }
                if kept.len() == 8 {
                    break;
                }
            }
            println!("{kind:?} (size {s}):");
            for side in 0..spots(kind).len() {
                let v = of(&world.map, kind, side);
                let ok =
                    |q: Vec2| fault(&world, q, world.map.ground_height(q.x, q.y), v.view).is_none();
                let mut best = None;
                for r in 0..=8 {
                    for k in 0..16 {
                        let a = k as f32 * std::f32::consts::TAU / 16.0;
                        let q = v.at + Vec2::new(a.sin(), a.cos()) * r as f32;
                        if best.is_none() && ok(q) {
                            best = Some(q);
                        }
                    }
                }
                println!("  pick {side} at {:?}: nearest clear {best:?}", v.at);
            }
            for (score, p, view, sky, med, near, lit) in kept {
                println!(
                    "  {score:5.2}  at ({:.0}, {:.0}) view {view:.2}  sky {sky:.2} median {med:.0} near {near:.2} lit {lit:.2}",
                    p.x, p.y
                );
            }
        }
    }
}
