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
//! contested site can be clicked (it's a row of the list beside it). A
//! generator that's down is a ghost of itself, ringed with its repairs: a
//! step a tick, those done in its holder's colour (#341).
//!
//! The scene (`SystemArt`, `scene`) and the marks over it (`draw`) share
//! one layout and camera, worked out from the theater and the screen's
//! shape alone, so the marks sit on the worlds without either needing the
//! other.

use std::f32::consts::{PI, TAU};

use glam::{Mat4, Quat, Vec2, Vec3, Vec4};

use crate::faction::Faction;
use crate::galaxy::theater::{Ship, SiteView, Theater, planet_name};
use crate::garage::{PANEL_W, PANEL_X};
use crate::menu::Hit;
use crate::planet;
use crate::render::mesh::{Instance, MeshData, MeshId};
use crate::render::{Atmosphere, Camera, Particle, PointLight, Renderer, Scene};
use crate::scene::project;
use crate::ship_model;
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
/// How high over its world your ship is parked, and how big it's drawn
/// (`ship_model` builds it `ship_model::LEN` long).
const SHIP_UP: f32 = 29.0;
const SHIP_SCALE: f32 = 1.35;
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
            ship: r.add_mesh(&ship_model::mesh()),
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

/// The system view of `t` on a screen `aspect` wide, at `time` seconds (the
/// worlds turn), or, while `transfer` plays, that.
pub fn scene(
    art: &SystemArt,
    t: &Theater,
    transfer: Option<&Transfer>,
    time: f32,
    aspect: f32,
) -> Scene {
    match transfer.and_then(|tr| tr.leg(t).map(|leg| (tr, leg))) {
        None => system_scene(art, t, time, camera(t, aspect), parked(&layout(t), time)),
        Some((tr, leg)) => transfer_scene(art, t, tr, leg, time, aspect),
    }
}

/// Where your ship is in a frame, and how hard it's driving.
#[derive(Clone, Copy, Debug)]
struct Pose {
    at: Vec3,
    /// Turned from bow along +x.
    yaw: Quat,
    /// Its drives' glow (1 parked), and how far its wake streams back
    /// behind it (0 for none).
    drive: f32,
    wake: f32,
}

/// Your ship parked over its world, bow toward the star.
fn parked(l: &Layout, time: f32) -> Option<Pose> {
    l.ship.map(|(at, _)| Pose {
        at: at + Vec3::Y * (time * 0.8).sin() * 0.3,
        yaw: parked_turn(),
        drive: 1.0,
        wake: 0.0,
    })
}

/// The system of `t`, at `time` seconds, seen by `camera`, your ship at `ship`.
fn system_scene(
    art: &SystemArt,
    t: &Theater,
    time: f32,
    camera: Camera,
    ship: Option<Pose>,
) -> Scene {
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

    // Your ship, drives aglow, and a cool fill light out toward the camera
    // and above, so the side away from the star still shows its shape.
    let lights = ship
        .map(|pose| {
            ship_parts(art, pose, &mut instances, &mut particles);
            let toward = (camera.pos - pose.at).normalize();
            PointLight {
                pos: pose.at + toward * 20.0 + Vec3::new(-6.0, 12.0, 0.0),
                color: Vec3::new(0.75, 0.82, 1.0) * 70.0,
                radius: 60.0,
            }
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

/// Your ship in the scene at `pose`: its hull, its drives' glow, and its
/// wake streaming back behind it while it's under way.
fn ship_parts(
    art: &SystemArt,
    pose: Pose,
    instances: &mut Vec<(MeshId, Instance)>,
    particles: &mut Vec<Particle>,
) {
    instances.push((
        art.ship,
        Instance::new(
            Mat4::from_scale_rotation_translation(Vec3::splat(SHIP_SCALE), pose.yaw, pose.at),
            Vec4::new(1.0, 1.0, 1.0, 0.0),
        ),
    ));
    let amber = Faction::Company.ui();
    let amber = Vec3::new(amber[0], amber[1], amber[2]);
    let back = pose.yaw * -Vec3::X;
    for &(mouth, r) in &ship_model::DRIVES {
        let d = pose.at + pose.yaw * mouth * SHIP_SCALE;
        particles.push(Particle::glow(
            d,
            r * 1.3 * SHIP_SCALE * pose.drive.sqrt(),
            amber,
            1.2 * pose.drive,
            Vec3::ZERO,
        ));
        if pose.wake > 0.0 {
            let len = ship_model::LEN * SHIP_SCALE * 0.5 * pose.wake;
            particles.push(Particle::glow(
                d + back * len,
                0.7,
                Vec3::new(1.0, 0.8, 0.55),
                0.8,
                back * (len / 0.7),
            ));
        }
    }
}

/// Moving onto your company's next contract (#348), played over the system
/// view: the ship you're aboard leaves the system it was in, jumps, and
/// arrives over the world of its new one; or, staying in the same system,
/// flies over to its new contract's world.
#[derive(Clone, Debug)]
pub struct Transfer {
    /// The theater you were in.
    pub from: Theater,
    /// Seconds it's played.
    pub age: f32,
}

/// A jump to another system: leaving, under way, arriving (seconds).
const LEAVE_S: f32 = 1.7;
const JUMP_S: f32 = 1.5;
const ARRIVE_S: f32 = 2.0;
/// A move to another world of the same system (or a new contract over the same one).
const MOVE_S: f32 = 2.6;
/// How the ship sits parked: bow toward the star, a little toward the camera.
const PARKED_YAW: Quat = Quat::from_xyzw(0.0, -0.974_794_1, 0.0, 0.223_106_4);
/// How far it banks over toward the camera while parked, so its deck shows.
const PARKED_BANK: f32 = -0.4;

/// How the ship sits parked: `PARKED_YAW`, banked.
fn parked_turn() -> Quat {
    PARKED_YAW * Quat::from_rotation_x(PARKED_BANK)
}
/// How far off into the deep it goes leaving, and comes from arriving
/// (inside the black round the camera).
const DEEP: f32 = 190.0;

/// Which part of a transfer is playing, and how far through it (0..1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Leg {
    Leave(f32),
    Jump(f32),
    Arrive(f32),
    Move(f32),
}

impl Transfer {
    pub fn new(from: Theater) -> Transfer {
        Transfer { from, age: 0.0 }
    }

    /// Going from theater `from` to `to` is a transfer: you're aboard
    /// another ship, or on another contract, or in another system.
    pub fn between(from: &Theater, to: &Theater) -> bool {
        match (&from.ship, &to.ship) {
            (Some(a), Some(b)) => a.id != b.id || a.signed != b.signed || from.system != to.system,
            _ => false,
        }
    }

    /// It jumps to another system (else it moves within the one it's in).
    pub fn jumps(&self, to: &Theater) -> bool {
        self.from.system != to.system
    }

    /// How long it plays, seconds.
    pub fn length(&self, to: &Theater) -> f32 {
        if self.jumps(to) {
            LEAVE_S + JUMP_S + ARRIVE_S
        } else {
            MOVE_S
        }
    }

    /// How far through it is (0..1).
    pub fn progress(&self, to: &Theater) -> f32 {
        (self.age / self.length(to)).clamp(0.0, 1.0)
    }

    /// What's playing, toward `to`; None once it's done.
    pub fn leg(&self, to: &Theater) -> Option<Leg> {
        let a = self.age.max(0.0);
        if a >= self.length(to) {
            return None;
        }
        Some(if !self.jumps(to) {
            Leg::Move(a / MOVE_S)
        } else if a < LEAVE_S {
            Leg::Leave(a / LEAVE_S)
        } else if a < LEAVE_S + JUMP_S {
            Leg::Jump((a - LEAVE_S) / JUMP_S)
        } else {
            Leg::Arrive((a - LEAVE_S - JUMP_S) / ARRIVE_S)
        })
    }
}

/// The theater whose worlds are on screen: the one you're leaving while
/// the ship's still in it, else `t`.
pub fn on_screen<'a>(t: &'a Theater, transfer: Option<&'a Transfer>) -> &'a Theater {
    match transfer.and_then(|tr| tr.leg(t).map(|leg| (tr, leg))) {
        Some((tr, Leg::Leave(_))) => &tr.from,
        _ => t,
    }
}

fn smooth(u: f32) -> f32 {
    let u = u.clamp(0.0, 1.0);
    u * u * (3.0 - 2.0 * u)
}

/// Away from `camera` past `at`, into the deep beyond it, leaning `side`
/// to the right (left if negative) so the ship shows its flank as it goes.
fn deep(camera: Camera, at: Vec3, side: f32) -> Vec3 {
    let ray = (at - camera.pos).normalize();
    let right = ray.cross(Vec3::Y).normalize();
    (ray + right * side).normalize()
}

/// Turned so its bow points along `dir` (level).
fn heading(dir: Vec3) -> Quat {
    Quat::from_rotation_y((-dir.z).atan2(dir.x))
}

/// A frame of `tr` toward `t`, `leg` of it.
fn transfer_scene(
    art: &SystemArt,
    t: &Theater,
    tr: &Transfer,
    leg: Leg,
    time: f32,
    aspect: f32,
) -> Scene {
    match leg {
        // It turns out of the system and goes, faster and faster, until it
        // jumps in a flash.
        Leg::Leave(u) => {
            let from = &tr.from;
            let cam = camera(from, aspect);
            let Some(mut pose) = parked(&layout(from), time) else {
                return system_scene(art, from, time, cam, None);
            };
            let out = deep(cam, pose.at, 0.3);
            pose.yaw = parked_turn().slerp(heading(out), smooth(u / 0.4));
            let go = ((u - 0.3) / 0.7).clamp(0.0, 1.0);
            pose.at += out * (go * go * go * DEEP);
            pose.drive = 1.0 + 5.0 * go;
            pose.wake = 6.0 * go * go;
            let mut s = system_scene(art, from, time, cam, Some(pose));
            flash(&mut s, pose.at, ((u - 0.82) / 0.18).clamp(0.0, 1.0));
            s
        }
        // Under way between the stars: the ship, and space streaming past.
        Leg::Jump(u) => jump_scene(art, u, time),
        // It drops in from a flash, slows, and settles over its new world.
        Leg::Arrive(u) => {
            let cam = camera(t, aspect);
            let Some(mut pose) = parked(&layout(t), time) else {
                return system_scene(art, t, time, cam, None);
            };
            let dir = -deep(cam, pose.at, -0.35);
            let k = 1.0 - smooth(u / 0.75);
            pose.at -= dir * (k * k * DEEP);
            pose.yaw = heading(dir).slerp(parked_turn(), smooth((u - 0.55) / 0.45));
            pose.drive = 1.0 + 4.0 * k;
            pose.wake = 5.0 * k * k;
            let mut s = system_scene(art, t, time, cam, Some(pose));
            flash(&mut s, pose.at, 1.0 - (u / 0.15).clamp(0.0, 1.0));
            s
        }
        // Up off its world, over to the next, and down to park.
        Leg::Move(u) => {
            let l = layout(t);
            let Some(mut pose) = parked(&l, time) else {
                return system_scene(art, t, time, camera(t, aspect), None);
            };
            let a = layout(&tr.from).ship.map_or(pose.at, |(at, _)| at);
            let b = pose.at;
            let e = smooth(u);
            let flat = Vec3::new(b.x - a.x, 0.0, b.z - a.z);
            let way = if flat.length() > 1.0 {
                pose.at = a.lerp(b, e) + Vec3::Y * (34.0 * (PI * e).sin());
                heading(flat)
            } else {
                // The same world: a lap round it, out to the right and back
                // behind it.
                let (r, ang) = (30.0, TAU * e);
                pose.at = b + Vec3::new(ang.sin(), 0.0, ang.cos() - 1.0) * r;
                heading(Vec3::new(ang.cos(), 0.0, -ang.sin()))
            };
            let turn = smooth(u / 0.2) * (1.0 - smooth((u - 0.75) / 0.25));
            pose.yaw = parked_turn().slerp(way, turn);
            let speed = (PI * e).sin();
            pose.drive = 1.0 + 2.5 * speed;
            pose.wake = 2.0 * speed;
            system_scene(art, t, time, camera(t, aspect), Some(pose))
        }
    }
}

/// A jump's flash at `at`, `k` (0..1) of its brightest.
fn flash(s: &mut Scene, at: Vec3, k: f32) {
    if k <= 0.0 {
        return;
    }
    let white = Vec3::new(0.9, 0.95, 1.0);
    s.particles.push(Particle::glow(
        at,
        6.0 + 30.0 * k,
        white,
        3.0 * k,
        Vec3::ZERO,
    ));
    s.particles
        .push(Particle::glow(at, 2.0, white, 4.0 * k, Vec3::X * 60.0 * k));
}

/// Between the stars, `u` (0..1) of the way: the ship side on, bow ahead,
/// and the stars streaming past, longest mid-jump.
fn jump_scene(art: &SystemArt, u: f32, time: f32) -> Scene {
    let camera = Camera {
        pos: Vec3::new(-40.0, 26.0, 150.0),
        target: Vec3::new(-24.0, 0.0, 0.0),
        fov_y: 0.62,
    };
    let mut instances = vec![(
        MeshId::SPHERE,
        Instance::new(
            Mat4::from_scale_rotation_translation(
                Vec3::splat(SPACE_R * 2.0),
                Quat::IDENTITY,
                camera.pos,
            ),
            Vec4::new(0.004, 0.005, 0.009, 1.0),
        ),
    )];
    let mut particles = Vec::new();
    // Up to speed, and back down.
    let speed = (PI * u).sin().max(0.0).powf(0.6);
    let mut h = 0x9E37_79B9u32;
    let mut rnd = || {
        h ^= h << 13;
        h ^= h >> 17;
        h ^= h << 5;
        (h >> 8) as f32 / 16_777_216.0
    };
    const SPAN: f32 = 900.0;
    for _ in 0..220 {
        let (a, b, c, d) = (rnd(), rnd(), rnd(), rnd());
        let x = (a * SPAN - time * 1400.0).rem_euclid(SPAN) - SPAN * 0.5;
        let at = Vec3::new(x, (b - 0.5) * 300.0, -60.0 - c * 420.0);
        let len = 1.0 + 90.0 * speed * (0.4 + d);
        let color = Vec3::new(0.8 + 0.2 * d, 0.88, 1.0);
        particles.push(Particle::glow(
            at,
            0.4 + 0.6 * d,
            color,
            0.25 + 0.7 * d,
            -Vec3::X * len,
        ));
    }
    let pose = Pose {
        at: Vec3::Y * (time * 1.7).sin() * 0.4,
        yaw: heading(Vec3::new(1.0, 0.0, 0.12)),
        drive: 3.0,
        wake: 5.0 + 4.0 * speed,
    };
    ship_parts(art, pose, &mut instances, &mut particles);
    let atmosphere = Atmosphere {
        sun_dir: Vec3::new(-0.3, 0.6, 0.75).normalize(),
        ..atmosphere(&Layout {
            star: Vec3::ZERO,
            worlds: Vec::new(),
            ship: None,
        })
    };
    // The jump's own light, cold, round the hull.
    let lights = vec![PointLight {
        pos: Vec3::new(-6.0, 14.0, 26.0),
        color: Vec3::new(0.75, 0.85, 1.0) * 5.0,
        radius: 50.0,
    }];
    let mut s = Scene {
        camera,
        time,
        sun_dir: atmosphere.sun_dir,
        atmosphere,
        shadow_center: Vec3::new(0.0, -1.0e5, 0.0),
        shadow_radius: 1.0,
        instances,
        particles,
        lights,
        ui: Vec::new(),
        world: false,
    };
    flash(
        &mut s,
        Vec3::new(30.0, 0.0, 0.0),
        1.0 - (u / 0.15).clamp(0.0, 1.0),
    );
    flash(
        &mut s,
        Vec3::new(30.0, 0.0, 0.0),
        ((u - 0.85) / 0.15).clamp(0.0, 1.0),
    );
    s
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
/// While `transfer` plays, its caption instead.
pub fn draw(
    p: &mut Painter,
    hits: &mut Hits<Hit>,
    t: &Theater,
    transfer: Option<&Transfer>,
    selected: usize,
    w: f32,
    h: f32,
) {
    let min = Vec2::new(VIEW_LEFT, VIEW_TOP);
    let max = Vec2::new(w - VIEW_RIGHT, h - VIEW_BOTTOM);
    widgets::corners(p, min, max, 14.0, theme::DIM);
    let bottom = max.y - LEGEND_H;
    if let Some((tr, leg)) = transfer.and_then(|tr| tr.leg(t).map(|leg| (tr, leg))) {
        caption(p, t, tr, leg, min, max);
        return;
    }
    let l = layout(t);
    let vp = camera(t, w / h).view_proj(w / h);
    let at = |v: Vec3| project(vp, v, w, h).map(|(x, y)| Vec2::new(x, y));
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
            && let (Some(s), Some(name)) = (
                at(ship + Vec3::Y * SHIP_SCALE * 4.5),
                t.ship.as_ref().map(|s| &s.name),
            )
        {
            let mut style = TextStyle::LABEL;
            style.color = Faction::Company.ui();
            label(p, s - Vec2::new(0.0, 22.0), name, &style);
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

    // Which system you're in, and how long you'll fight in it, over it;
    // what the marks mean under it.
    here(p, t, min);
    if let Some(ship) = &t.ship {
        term(p, ship, Vec2::new(max.x - 24.0, min.y + 22.0));
    }
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

/// Which system you're in, at the view's top left.
fn here(p: &mut Painter, t: &Theater, min: Vec2) {
    let mut style = TextStyle::LABEL;
    style.color = theme::DIM;
    p.text(
        Vec2::new(min.x + 24.0, min.y + 22.0),
        "YOU ARE IN",
        &style,
        Align::Left,
    );
    p.text(
        Vec2::new(min.x + 24.0, min.y + 38.0),
        &format!("{} SYSTEM", t.name),
        &TextStyle::value(20.0),
        Align::Left,
    );
    p.text(
        Vec2::new(min.x + 24.0, min.y + 68.0),
        &format!("CATALOGUE {:04} · TICK {}", t.system, t.tick),
        &style,
        Align::Left,
    );
}

/// How much longer your ship fights here, right-aligned at `at`: its
/// contract's ticks left (a battle each) and a bar of them, of its term.
fn term(p: &mut Painter, ship: &Ship, at: Vec2) {
    let mut style = TextStyle::LABEL;
    style.color = theme::DIM;
    p.text(at, "CONTRACT HERE", &style, Align::Right);
    let left = match ship.ticks_left {
        1 => "1 BATTLE LEFT".to_string(),
        k => format!("UP TO {k} BATTLES LEFT"),
    };
    let mut big = TextStyle::value(20.0);
    big.color = Faction::Company.ui();
    p.text(at + Vec2::new(0.0, 16.0), &left, &big, Align::Right);
    // A cell a tick, of its term: lit for those left.
    let n = ship.term.clamp(1, 60) as usize;
    let lit = (ship.ticks_left as usize).min(n);
    let (cw, gap, ch) = (6.0, 2.0, 7.0);
    let x0 = at.x - n as f32 * (cw + gap) + gap;
    for i in 0..n {
        let color = if i < lit {
            with_alpha(Faction::Company.ui(), 0.9)
        } else {
            theme::FAINT
        };
        p.rect(
            Vec2::new(x0 + i as f32 * (cw + gap), at.y + 48.0),
            Vec2::new(cw, ch),
            color,
        );
    }
    let lost = (ship.lost.round().max(0.0)) as u32;
    let cap = (ship.max_loss.round() as u32).max(1);
    p.text(
        at + Vec2::new(0.0, 64.0),
        &format!("SOONER IF THE WORLD FALLS, OR AT {cap} HULLS LOST ({lost} SO FAR)"),
        &style,
        Align::Right,
    );
}

/// What a transfer says over the view: where from and to, a route with
/// the ship along it, and the contract it's going to.
fn caption(p: &mut Painter, t: &Theater, tr: &Transfer, leg: Leg, min: Vec2, max: Vec2) {
    let amber = Faction::Company.ui();
    let mut style = TextStyle::LABEL;
    style.color = amber;
    let what = match leg {
        Leg::Leave(_) => "CONTRACT DONE · LEAVING",
        Leg::Jump(_) => "UNDER WAY",
        Leg::Arrive(_) => "ARRIVING · NEW CONTRACT",
        Leg::Move(_) => "NEW CONTRACT",
    };
    p.text(
        Vec2::new(min.x + 24.0, min.y + 22.0),
        what,
        &style,
        Align::Left,
    );
    let jumps = tr.jumps(t);
    let world = |th: &Theater| {
        th.ship
            .as_ref()
            .map_or(th.name.clone(), |s| planet_name(&th.name, s.over))
    };
    let employer = |th: &Theater| {
        th.ship.as_ref().map_or(String::new(), |s| {
            format!("{} OF {}", Faction::ALL[s.side].name(), s.realm)
        })
    };
    // The route's ends: the systems for a jump, else whose contract.
    let (a, b) = if jumps {
        (
            format!("{} SYSTEM", tr.from.name),
            format!("{} SYSTEM", t.name),
        )
    } else {
        (employer(&tr.from), employer(t))
    };
    let head = if jumps {
        format!("{a}  \u{2192}  {b}")
    } else if world(&tr.from) == world(t) {
        format!("STAYING OVER {}", world(t))
    } else {
        format!("{}  \u{2192}  {}", world(&tr.from), world(t))
    };
    p.text(
        Vec2::new(min.x + 24.0, min.y + 38.0),
        &head,
        &TextStyle::value(20.0),
        Align::Left,
    );
    let mut dim = TextStyle::LABEL;
    dim.color = theme::DIM;
    if let Some(ship) = &t.ship {
        let realm = Faction::ALL[ship.side];
        let aboard = match &tr.from.ship {
            Some(old) if old.id != ship.id => format!("ABOARD THE {} · ", ship.name),
            _ => String::new(),
        };
        p.text(
            Vec2::new(min.x + 24.0, min.y + 68.0),
            &format!(
                "{aboard}HIRED BY THE {} OF {} · UP TO {} BATTLES",
                realm.name(),
                ship.realm,
                ship.ticks_left
            ),
            &dim,
            Align::Left,
        );
    }

    // The route, along the bottom: from, to, and the ship on its way.
    let y = max.y - LEGEND_H * 0.5;
    let (x0, x1) = (min.x + 40.0, max.x - 40.0);
    let k = smooth(tr.progress(t));
    let ship_x = x0 + (x1 - x0) * k;
    p.line(
        Vec2::new(x0, y),
        Vec2::new(ship_x, y),
        2.0,
        with_alpha(amber, 0.8),
    );
    p.line(Vec2::new(ship_x, y), Vec2::new(x1, y), 1.0, theme::FAINT);
    dot(p, Vec2::new(x0, y), 4.0, theme::DIM);
    ring(p, Vec2::new(x1, y), 6.0, 1.5, amber);
    p.text(Vec2::new(x0, y - 24.0), &a, &dim, Align::Left);
    p.text(Vec2::new(x1, y - 24.0), &b, &dim, Align::Right);
    ship_glyph(p, Vec2::new(ship_x, y), 0.5);
}

/// A jump's flash over the whole screen (`w` by `h`), at the cuts between
/// its legs: under the panel, so it stays readable.
pub fn veil(p: &mut Painter, t: &Theater, transfer: Option<&Transfer>, w: f32, h: f32) {
    let Some(leg) = transfer.and_then(|tr| tr.leg(t)) else {
        return;
    };
    let k = match leg {
        Leg::Leave(u) => (u - 0.88) / 0.12,
        Leg::Jump(u) => (1.0 - u / 0.12).max((u - 0.88) / 0.12),
        Leg::Arrive(u) => 1.0 - u / 0.12,
        Leg::Move(_) => 0.0,
    }
    .clamp(0.0, 1.0);
    if k > 0.0 {
        p.rect(Vec2::ZERO, Vec2::new(w, h), [0.88, 0.92, 1.0, 0.9 * k]);
    }
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
    // A generator that's down: its repairs as steps round the marker, those
    // done in its holder's colour (none yet: a wreck).
    if let Some((done, of)) = s.repairs() {
        let n = of.max(1) as usize;
        let gap = 0.35;
        for i in 0..n {
            let a0 = -PI * 0.5 + TAU * i as f32 / n as f32 + gap * 0.5;
            let a1 = a0 + TAU / n as f32 - gap;
            let pts: Vec<Vec2> = (0..=8)
                .map(|k| a0 + (a1 - a0) * k as f32 / 8.0)
                .map(|a| at + Vec2::new(a.cos(), a.sin()) * 19.5)
                .collect();
            if i < done as usize {
                p.polyline(&pts, 2.5, owner);
            } else {
                p.polyline(&pts, 1.0, mix(BG, owner, 0.45));
            }
        }
    }
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
            "YOUR COMPANY",
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

    /// The parked heading is the one the ship always had: bow toward the star.
    #[test]
    fn parked_yaw_is_bow_to_the_star() {
        let want = Quat::from_rotation_y(PI + 0.45);
        assert!(PARKED_YAW.dot(want).abs() > 0.99999);
        let bow = PARKED_YAW * Vec3::X;
        assert!(bow.x < -0.8, "{bow}");
    }

    /// A transfer is a change of ship, contract or system, not of tick.
    #[test]
    fn what_counts_as_a_transfer() {
        let mut a = theater(2);
        a.ship = Some(Ship {
            id: 1,
            name: "TEST".into(),
            over: 0,
            crystals: 0.0,
            aboard: 0.0,
            realm: "R".into(),
            side: 0,
            upfront: 0.0,
            bonus: 0.0,
            field: 0.0,
            lost: 0.0,
            max_loss: 1.0,
            ticks_left: 10,
            term: 30,
            signed: 4,
            news: None,
        });
        let mut b = a.clone();
        b.tick += 3;
        b.ship.as_mut().unwrap().ticks_left = 7;
        assert!(!Transfer::between(&a, &b));
        let mut c = b.clone();
        c.ship.as_mut().unwrap().id = 2;
        assert!(Transfer::between(&a, &c));
        let mut d = b.clone();
        d.system = 8;
        assert!(Transfer::between(&a, &d));
        assert!(!Transfer::between(&theater(2), &d), "not from no ship");
        let tr = Transfer::new(a.clone());
        assert!(tr.jumps(&d) && !tr.jumps(&c));
        assert!(tr.length(&d) > tr.length(&c));
    }
}
