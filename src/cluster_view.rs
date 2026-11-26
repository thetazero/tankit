//! Behind the main menu: every system in the galaxy as a star, seen from
//! just off your ship and turning slowly round it. Stars take their
//! holder's color, strongly near you and faintly far off, and burn where
//! there's fighting. A picture of the war, not a way to pick contracts.

use std::f32::consts::TAU;

use glam::{Mat4, Quat, Vec2, Vec3, Vec4};

use crate::faction::Faction;
use crate::galaxy::Galaxy;
use crate::galaxy::theater::{ship_name, system_name};
use crate::menu::Hit;
use crate::render::mesh::{Instance, MeshId};
use crate::render::{Atmosphere, Camera, Particle, Scene};
use crate::scene::project;
use crate::theater_view;
use crate::ui::{Align, Hits, Painter, design};

/// Stars this near you are your local cluster (ly): bigger, brighter, and
/// named when pointed at.
const LOCAL: f32 = 420.0;
/// Distances are squeezed into this many units, half of it by `HALF` ly: it
/// keeps every star in the right direction and inside the far plane.
const DEPTH: f32 = 430.0;
const HALF: f32 = 900.0;
/// The camera: how far off, how steeply down, its view, and how long a turn takes (s).
const DIST: f32 = 115.0;
const PITCH: f32 = 0.42;
const FOV: f32 = 0.9;
const TURN: f32 = 420.0;

/// One star.
#[derive(Clone, Debug)]
struct Star {
    /// Squeezed (see `DEPTH`), from you.
    at: Vec3,
    color: Vec3,
    /// 1 at you, 0 at the edge of your cluster and beyond.
    near: f32,
    /// 0..1, its own.
    seed: f32,
    system: usize,
    side: usize,
    war: bool,
}

/// The galaxy around you as of a tick.
#[derive(Clone, Debug)]
pub struct Cluster {
    pub tick: u64,
    pub crew: Option<u32>,
    stars: Vec<Star>,
    ship: Option<String>,
}

impl Cluster {
    pub fn of(g: &Galaxy) -> Cluster {
        let company = g.crew_company().map(|c| &g.companies[c]);
        let center = g.systems[company.map_or(0, |c| c.at as usize)].pos;
        let stars = g
            .systems
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let d = s.pos - center;
                let r = d.length();
                let near = (1.0 - r / LOCAL).max(0.0);
                let c = Faction::side(s.owner()).ui();
                let tint = Vec3::new(c[0], c[1], c[2]);
                let white = Vec3::new(0.9, 0.93, 1.0);
                // The galaxy's plane lies flat: its z is up.
                let flat = Vec3::new(d.x, d.z, -d.y);
                Star {
                    at: flat * (DEPTH / (r + HALF)),
                    color: white.lerp(tint, if near > 0.0 { 1.0 } else { 0.55 }),
                    near,
                    seed: (i as f32 * 0.618_034).fract(),
                    system: i,
                    side: s.owner(),
                    war: s.at_war(),
                }
            })
            .collect();
        Cluster {
            tick: g.tick,
            crew: g.crew,
            stars,
            ship: company.map(|c| ship_name(c.id)),
        }
    }
}

/// The view at `time`, you in the middle of what the panel leaves.
fn camera(time: f32, aspect: f32) -> Camera {
    let a = TAU * time / TURN + 0.6;
    let out = Vec3::new(a.cos() * PITCH.cos(), PITCH.sin(), a.sin() * PITCH.cos());
    let right = (-out).cross(Vec3::Y).normalize();
    // You, as a share of the screen's width right of its middle.
    let panel = (design::MARGIN * 2.0 + design::COLUMN) / (900.0 * aspect);
    let shift = right * (panel * DIST * (FOV * 0.5).tan() * aspect);
    Camera {
        pos: out * DIST - shift,
        target: -shift,
        fov_y: FOV,
    }
}

/// The stars, for a screen `aspect` wide.
pub fn scene(c: &Cluster, time: f32, aspect: f32) -> Scene {
    let camera = camera(time, aspect);
    // Black space, self-lit so no light reaches it.
    let space = Instance::new(
        Mat4::from_scale_rotation_translation(Vec3::splat(1040.0), Quat::IDENTITY, camera.pos),
        Vec4::new(0.002, 0.0025, 0.005, 1.0),
    );
    let mut particles = Vec::with_capacity(c.stars.len() * 2);
    theater_view::stars(camera.pos, &mut particles);
    for s in &c.stars {
        let twinkle = 0.85 + 0.15 * (time * (0.6 + s.seed) + s.seed * 40.0).sin();
        // Mostly faint, a few bright.
        let bright = s.seed * s.seed * s.seed;
        let (size, strength) = if s.near > 0.0 {
            (1.0 + 0.7 * bright, 1.3 + 2.0 * bright)
        } else {
            (0.8 + 0.6 * bright, 0.3 + 0.9 * bright)
        };
        let glow = |r, k| Particle::glow(s.at, r, s.color, k, Vec3::ZERO);
        particles.push(glow(size, strength * twinkle));
        if s.near > 0.0 {
            // Its holder's haze.
            particles.push(glow(11.0, 0.05 * s.near));
        }
        if s.war {
            let flare = 0.5 + 0.5 * (time * 3.0 + s.seed * 60.0).sin();
            let fire = Vec3::new(1.0, 0.3, 0.08);
            let r = if s.near > 0.0 { 2.4 } else { 2.2 };
            let k = 0.5 + 0.7 * flare;
            particles.push(Particle::glow(s.at, r, fire, k, Vec3::ZERO));
        }
    }
    let atmosphere = Atmosphere {
        fog_density: 0.0,
        blowing_sand: 0.0,
        ..Atmosphere::default()
    };
    Scene {
        camera,
        time,
        sun_dir: atmosphere.sun_dir,
        atmosphere,
        shadow_center: Vec3::new(0.0, -1.0e5, 0.0),
        shadow_radius: 1.0,
        instances: vec![(MeshId::SPHERE, space)],
        particles,
        lights: Vec::new(),
        ui: Vec::new(),
        world: false,
    }
}

/// What's written over the stars on a `w` by `h` screen: your ship, and the
/// star pointed at (`hover`).
pub fn draw(
    p: &mut Painter,
    hits: &mut Hits<Hit>,
    c: Option<&Cluster>,
    hover: Option<usize>,
    time: f32,
    w: f32,
    h: f32,
) {
    let Some(c) = c else {
        // The galaxy's still loading.
        p.set_halo(None);
        p.rect(Vec2::ZERO, Vec2::new(w, h), [0.0, 0.0, 0.0, 1.0]);
        return;
    };
    let vp = camera(time, w / h).view_proj(w / h);
    let left = design::MARGIN * 2.0 + design::COLUMN;
    let put = |at: Vec3| {
        let (x, y) = project(vp, at, w, h)?;
        // Clear of the panel and the edges, so a name fits.
        let on = x > left + 60.0 && x < w - 100.0 && y > 50.0 && y < h - 70.0;
        on.then_some(Vec2::new(x, y))
    };
    for (i, s) in c.stars.iter().enumerate().filter(|(_, s)| s.near > 0.0) {
        let Some(q) = put(s.at) else { continue };
        hits.add(p, q - Vec2::splat(11.0), Vec2::splat(22.0), Hit::Star(i));
        if hover == Some(i) {
            let holder = Faction::side(s.side);
            p.arc(q, 9.0, 0.0, TAU, 1.0, design::TEXT);
            let at = q + Vec2::new(0.0, 20.0);
            p.text(at, &system_name(s.system), &design::BODY, Align::Center);
            let style = design::SMALL.colored(holder.ui());
            let below = at + Vec2::new(0.0, 20.0);
            p.text(below, holder.name(), &style, Align::Center);
        }
    }
    if let (Some(ship), Some(q)) = (&c.ship, put(Vec3::ZERO)) {
        p.arc(q, 15.0, 0.0, TAU, 1.5, design::AMBER);
        let style = design::BODY.colored(design::AMBER);
        p.text(q - Vec2::new(0.0, 40.0), ship, &style, Align::Center);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::galaxy::Rules;

    #[test]
    fn every_star_is_in_view_range_and_you_are_clear_of_the_panel() {
        let g = Galaxy::with_size(3, 400, Vec2::new(1800.0, 1100.0), Rules::default());
        let c = Cluster::of(&g);
        assert_eq!(c.stars.len(), g.systems.len());
        assert!(c.stars.iter().any(|s| s.at == Vec3::ZERO));
        assert!(c.stars.iter().all(|s| s.at.length() < DEPTH));
        for (w, h) in [(1600.0, 900.0), (1200.0, 900.0)] {
            for time in [0.0, 90.0, 250.0] {
                let vp = camera(time, w / h).view_proj(w / h);
                let (x, y) = project(vp, Vec3::ZERO, w, h).unwrap();
                let mid = (design::MARGIN * 2.0 + design::COLUMN + w) * 0.5;
                assert!(
                    (x - mid).abs() < 2.0 && (y - h * 0.5).abs() < 2.0,
                    "{x} {y}"
                );
                let mut p = Painter::new(1.0);
                let mut hits = Hits::new();
                draw(&mut p, &mut hits, Some(&c), Some(0), time, w, h);
                assert!(!hits.is_empty());
                assert!(p.finish().iter().all(|v| {
                    let [x, y] = v.pos;
                    x > -4.0 && x < w + 4.0 && y > -4.0 && y < h + 4.0
                }));
                assert!(scene(&c, time, w / h).particles.len() > c.stars.len());
            }
        }
    }
}
