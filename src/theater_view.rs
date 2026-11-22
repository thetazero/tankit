//! The missions screen's picture of the theater (#311): the system you're
//! posted to, in 3D (#346). Its star on the left, its worlds on their
//! orbits, each turning in the star's light under a haze of its holder's
//! colour, and your company's ship (#327) parked over the world it's
//! fighting for.
//!
//! Over it, in the UI's hairlines, each world's sites on a ring round it:
//! shield generators as domes, foundries as squares, crystal mines as
//! diamonds, in their holder's colour. Contested sites are ringed; the ones
//! your side fights for this tick are ringed hot and filled. Every
//! contested site can be clicked (it's a row of the list beside it).
//!
//! The scene (`SystemArt`, `scene`) and the marks over it (`draw`) share
//! one layout and camera, worked out from the theater and the screen's
//! shape alone, so the marks sit on the worlds without either needing the
//! other.

use std::f32::consts::{PI, TAU};

use glam::{Mat4, Quat, Vec2, Vec3, Vec4};

use crate::faction::Faction;
use crate::galaxy::theater::{SiteView, Theater, planet_name};
use crate::garage::{PANEL_W, PANEL_X};
use crate::menu::Hit;
use crate::planet;
use crate::render::material::Material;
use crate::render::mesh::{Instance, MeshData, MeshId};
use crate::render::{Atmosphere, Camera, Particle, PointLight, Renderer, Scene};
use crate::scene::project;
use crate::ui::widgets;
use crate::ui::{Align, Hits, Painter, Rgba, TextStyle, measure, theme, with_alpha};

const STAR: Rgba = [1.0, 0.86, 0.62, 1.0];
/// The fill behind the marks and labels: the black of space.
const BG: Rgba = [0.025, 0.025, 0.03, 1.0];
/// Height of the legend strip along the bottom.
const LEGEND_H: f32 = 46.0;
/// The UI's reference height (logical px), as the menu's.
const REF_HEIGHT: f32 = 900.0;
/// The view's left edge and right margin, logical px: right of the panel.
pub const VIEW_LEFT: f32 = PANEL_X + PANEL_W + 70.0;
const VIEW_RIGHT: f32 = 56.0;
/// Its top and bottom margins.
const VIEW_TOP: f32 = 60.0;
const VIEW_BOTTOM: f32 = 60.0;

/// The most worlds a system has.
pub const MAX_PLANETS: usize = 3;
/// The star's radius, and each world's.
const STAR_R: f32 = 11.0;
const WORLD_R: f32 = 9.0;
/// The first world's orbit, and how much wider each next one's is.
const ORBIT0: f32 = 64.0;
const ORBIT_STEP: f32 = 50.0;
/// How far round its orbit each world is from the camera's right (radians,
/// positive away from the camera), so they don't stand in a flat row.
const PHASE: [f32; MAX_PLANETS] = [0.2, -0.12, 0.06];
/// Your ship: how high over its world it's parked, and its length.
const SHIP_UP: f32 = 27.0;
const SHIP_LEN: f32 = 24.0;
/// How far off the camera is.
const DIST: f32 = 300.0;
/// How steeply the camera looks down on the orbits (radians).
const PITCH: f32 = 0.36;
/// The sphere of black round it all (inside the camera's far plane).
const SPACE_R: f32 = 520.0;
const STARS: usize = 600;

/// Where everything is.
pub struct Layout {
    pub star: Vec3,
    pub worlds: Vec<Vec3>,
    /// Your ship, and the world it's over.
    pub ship: Option<(Vec3, usize)>,
}

pub fn layout(t: &Theater) -> Layout {
    let n = (t.planets as usize).clamp(1, MAX_PLANETS);
    let worlds = (0..n)
        .map(|i| {
            let r = ORBIT0 + ORBIT_STEP * i as f32;
            let a = PHASE[i];
            Vec3::new(r * a.cos(), 0.0, -r * a.sin())
        })
        .collect::<Vec<_>>();
    let ship = t.ship.as_ref().map(|s| {
        let i = (s.over as usize).min(n - 1);
        (worlds[i] + Vec3::Y * SHIP_UP, i)
    });
    Layout {
        star: Vec3::ZERO,
        worlds,
        ship,
    }
}

/// The view for a screen `aspect` wide: the star and every world framed in
/// the space right of the panel, looking down a little on their orbits.
pub fn camera(t: &Theater, aspect: f32) -> Camera {
    let l = layout(t);
    let last = l.worlds.last().map_or(0.0, |w| w.x);
    // From the star's glow to past the last world's ring of sites.
    let (lo, hi) = (-STAR_R * 2.6, last + WORLD_R + 28.0);
    let span = (hi - lo).max(170.0);
    let center = Vec3::new((lo + hi) * 0.5, 0.0, -10.0);
    let w = REF_HEIGHT * aspect;
    let (left, right) = (VIEW_LEFT / w, 1.0 - VIEW_RIGHT / w);
    let (fx, frac) = ((left + right) * 0.5, (right - left).max(0.2));
    // A fixed distance (well inside the black round it all), the view
    // widened until the span fills the view's width, and its height.
    let dist = DIST;
    let half = (span * 0.5 / (frac * aspect * dist))
        .max(span * 0.32 / (dist * (1.0 - (VIEW_TOP + VIEW_BOTTOM) / REF_HEIGHT)));
    // Slid left, so the system lands in the middle of the view.
    let shift = Vec3::X * (2.0 * fx - 1.0) * dist * half * aspect;
    let back = Vec3::new(0.0, PITCH.sin(), PITCH.cos()) * dist;
    Camera {
        pos: center + back - shift,
        target: center - shift,
        fov_y: 2.0 * half.atan(),
    }
}

/// A world's look from orbit, the same for every pilot: one of the kinds
/// the galaxy has, by its system and place.
fn world_look(system: usize, planet: usize) -> (u64, planet::Look) {
    let seed = (system as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (planet as u64 + 1) * 0x5851;
    let looks = [
        planet::DRY,
        crate::map::procgen::city::PLANET,
        planet::ICE,
        planet::DRY,
    ];
    (seed, looks[(seed >> 7) as usize % looks.len()])
}

/// The system view's meshes: its worlds, rebuilt when the theater changes,
/// and your ship.
pub struct SystemArt {
    worlds: [MeshId; MAX_PLANETS],
    shown: [Option<(u64, planet::Look)>; MAX_PLANETS],
    ship: MeshId,
}

impl SystemArt {
    pub fn upload(r: &mut Renderer) -> SystemArt {
        let blank = MeshData::sphere(8, 4);
        SystemArt {
            worlds: std::array::from_fn(|_| r.add_mesh(&blank)),
            shown: [None; MAX_PLANETS],
            ship: r.add_mesh(&ship_mesh()),
        }
    }

    /// Build the worlds of `t` (only those that changed).
    pub fn show(&mut self, r: &mut Renderer, t: &Theater) {
        for i in 0..(t.planets as usize).min(MAX_PLANETS) {
            let want = world_look(t.system, i);
            if self.shown[i] != Some(want) {
                r.replace_mesh(self.worlds[i], &planet::mesh(want.0, &want.1));
                self.shown[i] = Some(want);
            }
        }
    }
}

/// The light: the star's, from the left and a little toward the camera,
/// so each world shows a lit crescent and a dark side.
fn atmosphere(l: &Layout) -> Atmosphere {
    let mid = l.worlds.iter().copied().sum::<Vec3>() / l.worlds.len().max(1) as f32;
    let sun_dir = (l.star - mid + Vec3::new(0.0, 25.0, 30.0)).normalize();
    Atmosphere {
        sun_dir,
        sun_color: Vec3::new(2.2, 2.05, 1.85),
        sky_zenith: Vec3::new(0.05, 0.055, 0.07),
        sky_horizon: Vec3::new(0.03, 0.03, 0.04),
        ground_color: Vec3::new(0.02, 0.02, 0.025),
        fog_density: 0.0,
        wind: Vec2::ZERO,
        blowing_sand: 0.0,
        dust: Vec3::splat(0.2),
        settle: Vec3::splat(0.2),
        ambient: 0.35,
    }
}

/// The system of `t`, at `time` seconds (the worlds turn), seen by `camera`.
pub fn scene(art: &SystemArt, t: &Theater, time: f32, camera: Camera) -> Scene {
    let l = layout(t);
    let mut instances = Vec::new();
    let mut particles = Vec::new();
    // Black space round the camera, self-lit so no light reaches it.
    instances.push((
        MeshId::SPHERE,
        Instance::new(
            Mat4::from_scale_rotation_translation(
                Vec3::splat(SPACE_R * 2.0),
                Quat::IDENTITY,
                camera.pos,
            ),
            Vec4::new(0.004, 0.005, 0.009, 1.0),
        ),
    ));
    stars(camera.pos, &mut particles);

    // The star: a white-hot ball in its own light, and its glow.
    let star = Vec3::from_slice(&STAR[..3]);
    instances.push((
        MeshId::SPHERE,
        Instance::new(
            Mat4::from_scale_rotation_translation(
                Vec3::splat(STAR_R * 2.0),
                Quat::IDENTITY,
                l.star,
            ),
            Vec4::new(1.0, 0.88, 0.66, 6.0),
        ),
    ));
    particles.push(Particle::glow(
        l.star,
        STAR_R * 1.1,
        Vec3::new(1.0, 0.95, 0.8),
        2.5,
        Vec3::ZERO,
    ));
    particles.push(Particle::glow(l.star, STAR_R * 1.7, star, 1.2, Vec3::ZERO));
    particles.push(Particle::glow(l.star, STAR_R * 3.2, star, 0.3, Vec3::ZERO));

    // Orbits: hairline rings of faint light round the star.
    for w in &l.worlds {
        let r = (w.x * w.x + w.z * w.z).sqrt();
        let n = (r * 1.6) as usize;
        for k in 0..n {
            let a = TAU * k as f32 / n as f32;
            let at = l.star + Vec3::new(a.cos(), 0.0, a.sin()) * r;
            particles.push(Particle::glow(
                at,
                0.34,
                Vec3::new(0.7, 0.74, 0.8),
                0.5,
                Vec3::ZERO,
            ));
        }
    }

    let holders = holders(t);
    for (i, &w) in l.worlds.iter().enumerate() {
        instances.push((
            art.worlds[i],
            Instance::new(
                Mat4::from_scale_rotation_translation(
                    Vec3::splat(WORLD_R * 2.0),
                    Quat::from_rotation_y(time * 0.05 + i as f32 * 2.0)
                        * Quat::from_rotation_z(0.25),
                    w,
                ),
                Vec4::new(1.0, 1.0, 1.0, 0.0),
            ),
        ));
        // Its air, lit in its holder's colour, round its outline.
        let tint = Faction::ALL[holders[i]].ui();
        let tint = Vec3::new(tint[0], tint[1], tint[2]);
        let view = (w - camera.pos).normalize();
        let u = view.cross(Vec3::Y).normalize();
        let v = u.cross(view);
        for k in 0..160 {
            let a = TAU * k as f32 / 160.0;
            let at = w + (u * a.cos() + v * a.sin()) * WORLD_R * 1.03;
            particles.push(Particle::glow(at, 0.7, tint, 0.22, Vec3::ZERO));
        }
    }

    // Your ship, parked over its world, bow toward the star, drives aglow.
    if let Some((at, _)) = l.ship {
        let yaw = Quat::from_rotation_y(PI + 0.45);
        let bob = Vec3::Y * (time * 0.8).sin() * 0.3;
        instances.push((
            art.ship,
            Instance::new(
                Mat4::from_scale_rotation_translation(Vec3::splat(SHIP_LEN), yaw, at + bob),
                Vec4::new(1.0, 1.0, 1.0, 0.0),
            ),
        ));
        let amber = Faction::Company.ui();
        let amber = Vec3::new(amber[0], amber[1], amber[2]);
        for dz in [-0.17, 0.17] {
            let d = at + bob + yaw * (Vec3::new(-0.54, 0.0, dz) * SHIP_LEN);
            particles.push(Particle::glow(d, 0.9, amber, 1.2, Vec3::ZERO));
        }
    }

    // A work light off its bow, toward the camera, so its far side from
    // the star still shows its shape.
    let lights = l
        .ship
        .map(|(at, _)| PointLight {
            pos: at + Vec3::new(-4.0, 6.0, 14.0),
            color: Vec3::new(0.8, 0.85, 1.0) * 10.0,
            radius: 24.0,
        })
        .into_iter()
        .collect();
    let atmosphere = atmosphere(&l);
    Scene {
        camera,
        time,
        sun_dir: atmosphere.sun_dir,
        atmosphere,
        // No shadows: the sun's light is one direction, so an inner world
        // would throw a band of shadow out across the system. The shadow
        // map covers a patch far off, and nothing here falls in it.
        shadow_center: Vec3::new(0.0, -1.0e5, 0.0),
        shadow_radius: 1.0,
        instances,
        particles,
        lights,
        ui: Vec::new(),
        world: false,
    }
}

/// Who holds most of each world's sites (index into `Faction::ALL`).
fn holders(t: &Theater) -> Vec<usize> {
    (0..(t.planets as usize).max(1))
        .map(|pl| {
            let mut held = [0usize; 4];
            for s in t.sites.iter().filter(|s| s.planet as usize == pl) {
                held[s.owner] += 1;
            }
            (0..4).max_by_key(|&f| held[f]).unwrap_or(0)
        })
        .collect()
}

/// Stars all round, far out (fixed, so they don't swim as the view changes).
fn stars(eye: Vec3, out: &mut Vec<Particle>) {
    let mut h = 0x2545_f491u32;
    let mut rnd = || {
        h ^= h << 13;
        h ^= h >> 17;
        h ^= h << 5;
        (h >> 8) as f32 / 16_777_216.0
    };
    for _ in 0..STARS {
        let (a, b, c, d) = (rnd(), rnd(), rnd(), rnd());
        let z = a * 2.0 - 1.0;
        let s = (1.0 - z * z).sqrt();
        let dir = Vec3::new(s * (TAU * b).cos(), z, s * (TAU * b).sin());
        let bright = c * c * c;
        let color = Vec3::new(0.85 + 0.15 * d, 0.88, 1.0 - 0.15 * d);
        out.push(Particle::glow(
            eye + dir * (SPACE_R * 0.92),
            0.6 + 1.0 * bright,
            color,
            0.25 + 1.6 * bright,
            Vec3::ZERO,
        ));
    }
}

/// A Company ship, a unit long along +x (bow forward), centered: a long
/// grey hull, an amber band, a bridge on top, two drive pods at the stern.
fn ship_mesh() -> MeshData {
    let mut m = MeshData::default();
    let col = |c: [f32; 3], mat: Material| Vec4::new(c[0], c[1], c[2], mat.id());
    let hull = col([0.24, 0.25, 0.27], Material::Painted);
    let dark = col([0.16, 0.16, 0.17], Material::Metal);
    let a = Faction::Company.ui();
    let amber = col([a[0], a[1], a[2]], Material::Painted);
    let glow = col([a[0], a[1], a[2]], Material::Glow);
    let bx = |m: &mut MeshData, min: [f32; 3], max: [f32; 3], c: Vec4| {
        m.add_box(Vec3::from(min), Vec3::from(max), c, c)
    };
    // The spine, and the bow tapering off it.
    bx(&mut m, [-0.42, -0.07, -0.1], [0.28, 0.07, 0.1], hull);
    bx(&mut m, [0.28, -0.055, -0.075], [0.4, 0.055, 0.075], hull);
    bx(&mut m, [0.4, -0.035, -0.045], [0.5, 0.035, 0.045], hull);
    // The band, a keel, the bridge with its lit windows.
    bx(&mut m, [0.08, -0.075, -0.105], [0.16, 0.075, 0.105], amber);
    bx(&mut m, [-0.36, -0.1, -0.04], [0.2, -0.07, 0.04], dark);
    bx(&mut m, [-0.22, 0.07, -0.06], [-0.04, 0.12, 0.06], hull);
    bx(&mut m, [-0.05, 0.09, -0.04], [-0.035, 0.11, 0.04], glow);
    // The drive pods, out on a strut, their nozzles aglow.
    bx(&mut m, [-0.34, -0.02, -0.2], [-0.28, 0.02, 0.2], dark);
    for (z0, z1) in [(-0.22, -0.12), (0.12, 0.22)] {
        bx(&mut m, [-0.5, -0.05, z0], [-0.2, 0.05, z1], hull);
        bx(
            &mut m,
            [-0.42, 0.05, z0 + 0.02],
            [-0.3, 0.052, z1 - 0.02],
            amber,
        );
        bx(
            &mut m,
            [-0.52, -0.035, z0 + 0.015],
            [-0.5, 0.035, z1 - 0.015],
            glow,
        );
    }
    m
}

/// Text on a backing of space's colour.
fn label(p: &mut Painter, at: Vec2, text: &str, style: &TextStyle) {
    let w = measure(text, style);
    p.rect(
        Vec2::new(at.x - w * 0.5 - 6.0, at.y - 3.0),
        Vec2::new(w + 12.0, style.size + 6.0),
        with_alpha(BG, 0.7),
    );
    p.text(at, text, style, Align::Center);
}

/// `a` over `b`, `k` of the way.
fn mix(b: Rgba, a: Rgba, k: f32) -> Rgba {
    [
        b[0] + (a[0] - b[0]) * k,
        b[1] + (a[1] - b[1]) * k,
        b[2] + (a[2] - b[2]) * k,
        b[3] + (a[3] - b[3]) * k,
    ]
}

/// A full circle, smooth at any size.
fn ring(p: &mut Painter, c: Vec2, r: f32, width: f32, color: Rgba) {
    let n = ((r * 1.2) as usize).clamp(20, 160);
    let pts: Vec<Vec2> = (0..=n)
        .map(|i| TAU * i as f32 / n as f32)
        .map(|a| c + Vec2::new(a.cos(), a.sin()) * r)
        .collect();
    p.polyline(&pts, width, color);
}

/// A solid disc with a crisp edge.
fn dot(p: &mut Painter, c: Vec2, r: f32, color: Rgba) {
    p.disc(c, r, color, color);
    ring(p, c, r - 0.5, 1.0, color);
}

/// The marks over the system view of `t`, on a screen `w` by `h` logical
/// px, contested site `selected` marked. Each contested site's marker is a
/// `Hit::Row` of its index in the list.
pub fn draw(p: &mut Painter, hits: &mut Hits<Hit>, t: &Theater, selected: usize, w: f32, h: f32) {
    let l = layout(t);
    let vp = camera(t, w / h).view_proj(w / h);
    let at = |v: Vec3| project(vp, v, w, h).map(|(x, y)| Vec2::new(x, y));
    let min = Vec2::new(VIEW_LEFT, VIEW_TOP);
    let max = Vec2::new(w - VIEW_RIGHT, h - VIEW_BOTTOM);
    widgets::corners(p, min, max, 14.0, theme::DIM);
    let bottom = max.y - LEGEND_H;
    let holders = holders(t);
    // How big a world looks: its radius, projected.
    let right = {
        let c = camera(t, w / h);
        (c.target - c.pos).cross(Vec3::Y).normalize()
    };

    for (pl, &wpos) in l.worlds.iter().enumerate() {
        let (Some(c), Some(edge)) = (at(wpos), at(wpos + right * WORLD_R)) else {
            continue;
        };
        let r = (edge - c).length();
        let tint = Faction::ALL[holders[pl]].ui();
        let sites: Vec<(usize, &SiteView)> = t
            .sites
            .iter()
            .enumerate()
            .filter(|(_, s)| s.planet as usize == pl)
            .collect();
        let mut held = [0usize; 4];
        for (_, s) in &sites {
            held[s.owner] += 1;
        }

        // Its sites on a ring round it, from the top.
        // (Closer in where the worlds are drawn small, so rings don't meet.)
        let ring_r = r + 30.0 * (r / 45.0).clamp(0.55, 1.0);
        let k = sites.len().max(1);
        for (j, &(i, s)) in sites.iter().enumerate() {
            let a = -PI * 0.5 + TAU * j as f32 / k as f32;
            let dir = Vec2::new(a.cos(), a.sin());
            let m = c + dir * ring_r;
            p.line(
                c + dir * (r + 4.0),
                c + dir * (ring_r - 15.0),
                1.0,
                with_alpha(tint, 0.5),
            );
            let row = t.contested.iter().position(|&x| x == i);
            site_marker(p, m, s, row == Some(selected));
            if let Some(row) = row {
                hits.add(p, m - Vec2::splat(18.0), Vec2::splat(36.0), Hit::Row(row));
            }
        }

        // Your ship's name over it.
        if let Some((ship, _)) = l.ship.filter(|&(_, i)| i == pl)
            && let (Some(s), Some(name)) = (at(ship), t.ship.as_ref().map(|s| &s.name))
        {
            let mut style = TextStyle::LABEL;
            style.color = Faction::Company.ui();
            label(p, s - Vec2::new(0.0, 46.0), name, &style);
        }

        // Its name, and who holds what.
        let below = c.y + ring_r + 30.0;
        label(
            p,
            Vec2::new(c.x, below),
            &planet_name(&t.name, pl as u8),
            &TextStyle::value(13.0),
        );
        let parts: Vec<String> = (0..4)
            .filter(|&f| held[f] > 0)
            .map(|f| format!("{} {}", Faction::ALL[f].name(), held[f]))
            .collect();
        let mut style = TextStyle::LABEL;
        style.color = theme::DIM;
        label(p, Vec2::new(c.x, below + 22.0), &parts.join(" · "), &style);
    }

    // The theater's name over it, and what the marks mean under it.
    p.text(
        Vec2::new(min.x + 24.0, min.y + 22.0),
        &format!("{} SYSTEM", t.name),
        &TextStyle::value(14.0),
        Align::Left,
    );
    let mut style = TextStyle::LABEL;
    style.color = theme::DIM;
    p.text(
        Vec2::new(min.x + 24.0, min.y + 44.0),
        &format!("CATALOGUE {:04} · TICK {}", t.system, t.tick),
        &style,
        Align::Left,
    );
    p.line(
        Vec2::new(min.x + 16.0, bottom),
        Vec2::new(max.x - 16.0, bottom),
        1.0,
        mix(BG, theme::FG, 0.12),
    );
    legend(
        p,
        Vec2::new(min.x + 24.0, bottom + 17.0),
        max.x - min.x - 48.0,
    );
}

/// A company ship at `at`, `k` times its size: a long hull, bow to the
/// right, its drives aglow at the stern, in the Company's amber.
fn ship_glyph(p: &mut Painter, at: Vec2, k: f32) {
    let amber = Faction::Company.ui();
    let pts: Vec<Vec2> = [
        (30.0, 0.0),
        (16.0, -6.0),
        (-6.0, -6.0),
        (-12.0, -11.0),
        (-22.0, -11.0),
        (-20.0, -5.0),
        (-24.0, -5.0),
        (-24.0, 5.0),
        (-20.0, 5.0),
        (-22.0, 11.0),
        (-12.0, 11.0),
        (-6.0, 6.0),
        (16.0, 6.0),
        (30.0, 0.0),
    ]
    .iter()
    .map(|&(x, y)| at + Vec2::new(x, y) * k)
    .collect();
    // Drives, then the hull's fill and outline.
    for dy in [-2.5, 2.5] {
        let d = at + Vec2::new(-27.0, dy) * k;
        p.disc(d, 6.0 * k, with_alpha(amber, 0.5), with_alpha(amber, 0.0));
    }
    p.rect(
        at + Vec2::new(-20.0, -4.5) * k,
        Vec2::new(36.0, 9.0) * k,
        mix(BG, amber, 0.25),
    );
    p.polyline(&pts, 1.5, amber);
    p.line(
        at + Vec2::new(-6.0, 0.0) * k,
        at + Vec2::new(14.0, 0.0) * k,
        1.0,
        mix(BG, amber, 0.6),
    );
}

/// What kind of site it is, as a small glyph in `color` at `at`.
fn glyph(p: &mut Painter, at: Vec2, s: &SiteView, color: Rgba) {
    if s.is_shield() {
        // A dome on its base (smooth: `Painter::arc` is coarse this small).
        let base = at + Vec2::new(0.0, 3.5);
        let dome: Vec<Vec2> = (0..=16)
            .map(|i| PI + PI * i as f32 / 16.0)
            .map(|a| base + Vec2::new(a.cos(), a.sin()) * 7.0)
            .collect();
        p.polyline(&dome, 1.6, color);
        p.line(
            base + Vec2::new(-9.0, 0.0),
            base + Vec2::new(9.0, 0.0),
            1.6,
            color,
        );
    } else if s.kind_name() == "CRYSTAL MINE" {
        p.polyline(
            &[
                at + Vec2::new(0.0, -7.0),
                at + Vec2::new(5.5, 0.0),
                at + Vec2::new(0.0, 7.0),
                at + Vec2::new(-5.5, 0.0),
                at + Vec2::new(0.0, -7.0),
            ],
            1.5,
            color,
        );
    } else {
        p.rect_outline(at - Vec2::splat(5.5), Vec2::splat(11.0), 1.5, color);
    }
}

fn site_marker(p: &mut Painter, at: Vec2, s: &SiteView, selected: bool) {
    let owner = Faction::ALL[s.owner].ui();
    // A downed generator is a ghost of itself.
    let color = if s.down().is_some() {
        mix(BG, owner, 0.4)
    } else {
        owner
    };
    if s.committed {
        dot(p, at, 15.0, mix(BG, theme::HOT, 0.3));
        ring(p, at, 15.0, 2.0, theme::HOT);
    } else {
        dot(p, at, 15.0, BG);
        if s.contested {
            ring(p, at, 15.0, 1.3, mix(BG, theme::HOT, 0.75));
        }
    }
    glyph(p, at, s, color);
    if selected {
        widgets::corners(
            p,
            at - Vec2::splat(22.0),
            at + Vec2::splat(22.0),
            7.0,
            theme::FG,
        );
    }
}

/// The key to the marks, in a row along the bottom.
fn legend(p: &mut Painter, at: Vec2, width: f32) {
    let mut style = TextStyle::LABEL;
    style.color = theme::DIM;
    let shield = SiteView::example(true);
    let mine = SiteView::example(false);
    let entries: [(&str, Box<dyn Fn(&mut Painter, Vec2)>); 6] = [
        ("SHIELD", Box::new(|p, c| glyph(p, c, &shield, theme::DIM))),
        (
            "FOUNDRY",
            Box::new(|p, c| {
                p.rect_outline(c - Vec2::splat(5.5), Vec2::splat(11.0), 1.5, theme::DIM)
            }),
        ),
        (
            "CRYSTAL MINE",
            Box::new(|p, c| glyph(p, c, &mine, theme::DIM)),
        ),
        (
            "CONTESTED",
            Box::new(|p, c| ring(p, c, 8.0, 1.2, mix(BG, theme::HOT, 0.75))),
        ),
        (
            "YOUR SIDE FIGHTS HERE",
            Box::new(|p, c| {
                dot(p, c, 8.0, mix(BG, theme::HOT, 0.3));
                ring(p, c, 8.0, 2.0, theme::HOT);
            }),
        ),
        (
            "YOUR SHIP",
            Box::new(|p, c| ship_glyph(p, c + Vec2::new(-1.0, 0.0), 0.36)),
        ),
    ];
    let mut x = at.x;
    for (name, mark) in entries {
        let w = crate::ui::measure(name, &style);
        if x + 25.0 + w > at.x + width {
            break;
        }
        mark(p, Vec2::new(x + 9.0, at.y + 5.0));
        p.text(Vec2::new(x + 25.0, at.y), name, &style, Align::Left);
        x += 25.0 + w + 26.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theater(planets: u8) -> Theater {
        Theater {
            system: 7,
            name: "TEST".into(),
            employer: 0,
            planets,
            hulls: [0.0; 4],
            held: [0; 4],
            sites: Vec::new(),
            contested: Vec::new(),
            tick: 0,
            ship: None,
        }
    }

    /// The star and every world, with room for their rings of sites, land
    /// in the view right of the panel, on any screen shape.
    #[test]
    fn the_system_fills_the_view_on_any_screen() {
        for planets in 1..=MAX_PLANETS as u8 {
            let t = theater(planets);
            let l = layout(&t);
            for aspect in [5.0 / 4.0, 16.0 / 10.0, 16.0 / 9.0, 21.0 / 9.0, 32.0 / 9.0] {
                let (w, h) = (REF_HEIGHT * aspect, REF_HEIGHT);
                let cam = camera(&t, aspect);
                let vp = cam.view_proj(aspect);
                // Everything inside the black round the camera.
                for &p in l.worlds.iter().chain([&l.star]) {
                    assert!((p - cam.pos).length() < SPACE_R * 0.8);
                }
                let (sx, _) = project(vp, l.star, w, h).unwrap();
                assert!(sx > VIEW_LEFT + 60.0, "star at {sx} ({planets}, {aspect})");
                for &p in &l.worlds {
                    let (x, y) = project(vp, p, w, h).unwrap();
                    assert!(
                        x > VIEW_LEFT + 60.0 && x < w - VIEW_RIGHT - 60.0,
                        "world at x {x} ({planets} worlds, aspect {aspect})"
                    );
                    assert!(y > VIEW_TOP + 100.0 && y < h - VIEW_BOTTOM - LEGEND_H - 100.0);
                }
            }
        }
    }
}
