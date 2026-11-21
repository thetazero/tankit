//! The Kiln: Ithessar ground, and their weapons' ground. Slow plasma wants
//! walls to bank off and corners to wait behind, so this is a warren: lanes
//! winding between walls of squared stone, opening into small courts,
//! with nowhere you can see (or shoot) more than a few dozen meters down.
//! In the middle the Ithessar keep a sun: six great crystals lean in over a
//! raised court and hold a ball of plasma between their tips. The Imperium
//! has broken in at the east edge.
//!
//! 180 m square (+x east, +z north), mirrored east to west. Three ways
//! across, each its own kind of fight:
//!
//! - north: the canyon, wide and sunken, a river of plasma down its floor
//! - middle: short and straight through a court to the kiln, the fast way in
//! - south: the gallery, narrow and twisting between sheer stone walls
//!
//! Cross-cuts link the three near each base. Stone fills everything else.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use glam::{Vec2, Vec3};

use super::rocks;
use super::{Ambient, Core, Crystal, GlowPath, Map, OBox, Terrain};
use crate::faction::Faction;
use crate::render::material::Material;
use crate::rng::Rng;
use crate::tank::ithessar::PLASMA;

const SIZE: f32 = 180.0;
const MID: f32 = SIZE * 0.5;
const CENTER: Vec2 = Vec2::new(MID, MID);
/// The kiln court: open inside `COURT_R`; the dais rises inside `DAIS_R`.
const COURT_R: f32 = 27.0;
const DAIS_R: f32 = 15.0;
const DAIS_H: f32 = 2.2;
/// Great crystals round the kiln, and the core's height above the dais.
const KILN_R: f32 = 19.0;
const CORE_Y: f32 = 14.0;
const CANYON_DEPTH: f32 = 2.2;

/// A lane: (x, z) points on the west half, and its half-width. Each is
/// mirrored onto the east half; the north and south lanes meet their
/// mirrors at x = 90.
type Lane = (&'static [(f32, f32)], f32);

const CANYON: Lane = (
    &[
        (24.0, 112.0),
        (34.0, 128.0),
        (50.0, 140.0),
        (70.0, 146.0),
        (90.0, 150.0),
    ],
    8.0,
);
const MIDDLE: Lane = (
    &[(26.0, 90.0), (44.0, 86.0), (56.0, 92.0), (66.0, 90.0)],
    6.5,
);
const GALLERY: Lane = (
    &[
        (24.0, 68.0),
        (34.0, 54.0),
        (46.0, 48.0),
        (54.0, 36.0),
        (70.0, 30.0),
        (90.0, 28.0),
    ],
    5.5,
);
/// Cross-cuts: canyon to middle court to gallery, and each outer lane in
/// to the kiln court.
const CUTS: &[Lane] = &[
    (&[(34.0, 128.0), (40.0, 112.0), (44.0, 96.0)], 5.0),
    (&[(44.0, 80.0), (40.0, 66.0), (34.0, 54.0)], 5.0),
    (&[(70.0, 146.0), (74.0, 128.0), (72.0, 112.0)], 5.0),
    (&[(70.0, 30.0), (72.0, 50.0), (70.0, 66.0)], 4.5),
];
/// Small courts where lanes meet: center and radius (west half; mirrored).
const COURTS: &[((f32, f32), f32)] = &[
    ((44.0, 88.0), 10.0),
    ((34.0, 128.0), 8.0),
    ((34.0, 54.0), 7.5),
    ((90.0, 150.0), 11.0),
    ((90.0, 28.0), 9.0),
];
/// The west base: an open court against the edge.
const BASE_MIN: Vec2 = Vec2::new(3.0, 62.0);
const BASE_MAX: Vec2 = Vec2::new(27.0, 118.0);

const SLATE: Vec3 = Vec3::new(0.19, 0.2, 0.22);
/// The warren's worked stone: a cold blue-grey, a shade above the ground.
const STONE: Vec3 = Vec3::new(0.18, 0.19, 0.23);
const GLASS: Vec3 = Vec3::new(0.02, 0.065, 0.17);
const CONCRETE: Vec3 = Vec3::new(0.56, 0.57, 0.6);
const IMPERIAL: Vec3 = Vec3::new(0.78, 0.82, 0.9);
const STEEL: Vec3 = Vec3::new(0.32, 0.33, 0.36);
const FLOODLIGHT: Vec3 = Vec3::new(0.85, 0.92, 1.0);

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn lanes() -> impl Iterator<Item = &'static Lane> {
    [&CANYON, &MIDDLE, &GALLERY].into_iter().chain(CUTS.iter())
}

/// Fold a point onto the west half.
fn west(p: Vec2) -> Vec2 {
    Vec2::new(p.x.min(SIZE - p.x), p.y)
}

fn seg_distance(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

fn polyline_distance(p: Vec2, pts: &[(f32, f32)]) -> f32 {
    pts.windows(2)
        .map(|w| seg_distance(p, Vec2::new(w[0].0, w[0].1), Vec2::new(w[1].0, w[1].1)))
        .fold(f32::INFINITY, f32::min)
}

/// How far a point is inside open ground (positive: open, by that much;
/// negative: that far into the crystal).
fn openness(p: Vec2) -> f32 {
    let w = west(p);
    let mut best = COURT_R - p.distance(CENTER);
    for (pts, half) in lanes() {
        best = best.max(half - polyline_distance(w, pts));
    }
    for &((x, z), r) in COURTS {
        best = best.max(r - w.distance(Vec2::new(x, z)));
    }
    let q = (w - (BASE_MIN + BASE_MAX) * 0.5).abs() - (BASE_MAX - BASE_MIN) * 0.5;
    best.max(-q.max(Vec2::ZERO).length() - q.x.max(q.y).min(0.0))
}

/// How far outside the arena a point is (0 or less: inside).
fn edge_distance(p: Vec2) -> f32 {
    let q = (p - Vec2::splat(MID)).abs() - Vec2::splat(MID);
    q.max(Vec2::ZERO).length() + q.x.max(q.y).min(0.0)
}

/// Play ground: the dais, the sunken canyon, a little unevenness.
pub fn ground(x: f32, z: f32) -> f32 {
    let p = Vec2::new(x, z);
    let dais = DAIS_H * (1.0 - smoothstep(DAIS_R, DAIS_R + 7.0, p.distance(CENTER)));
    let d = polyline_distance(west(p), CANYON.0);
    let canyon = -CANYON_DEPTH * (1.0 - smoothstep(CANYON.1 - 2.5, CANYON.1 + 4.0, d));
    let rough = (x * 0.13).sin() * (z * 0.11).cos() * 0.25;
    dais + canyon + rough
}

/// Ash flats outside the arena, rising into a crystal-studded berm.
fn scenery_ground(x: f32, z: f32) -> f32 {
    let p = Vec2::new(x, z);
    let d = edge_distance(p);
    if d < -0.01 {
        return -10.0;
    }
    let edge = p.clamp(Vec2::ZERO, Vec2::splat(SIZE));
    let base = ground(edge.x, edge.y);
    let berm = 5.0 * smoothstep(0.0, 12.0, d) * (1.0 - 0.6 * smoothstep(20.0, 70.0, d));
    let swell = smoothstep(10.0, 60.0, d)
        * (4.0 * (x * 0.015 + (z * 0.01).sin() * 2.0).sin().abs()
            + 2.0 * ((x * 0.005 - z * 0.008).sin() + 1.0));
    base + berm + swell
}

impl Map {
    pub fn kiln() -> Map {
        static BUILT: std::sync::OnceLock<Map> = std::sync::OnceLock::new();
        Map::built_once(&BUILT, Map::build_kiln)
    }

    fn build_kiln() -> Map {
        let mut map = Map {
            size: SIZE,
            solids: Vec::new(),
            boxes: Vec::new(),
            wedges: Vec::new(),
            oboxes: Vec::new(),
            terrain: None,
            scenery_terrain: None,
            open_spots: Vec::new(),
            spawn_yaw: None,
            // A cold dusk: a low pale sun, a deep indigo sky, slate ground.
            // The only warm light on this world is the plasma.
            atmosphere: crate::render::Atmosphere {
                sun_dir: Vec3::new(-0.62, 0.42, 0.55),
                sun_color: Vec3::new(1.75, 1.8, 2.1),
                sky_zenith: Vec3::new(0.04, 0.06, 0.17),
                sky_horizon: Vec3::new(0.26, 0.32, 0.46),
                ground_color: Vec3::new(0.2, 0.22, 0.28),
                fog_density: 0.0022,
                wind: Vec2::new(-1.5, 0.8),
                blowing_sand: 0.15,
                dust: Vec3::new(0.4, 0.4, 0.46),
                settle: crate::render::SAND_SETTLE,
                ambient: 3.2,
            },
            bases: Map::no_bases(&[Faction::Ithessar, Faction::Imperium]),
            factions: [Faction::Ithessar, Faction::Imperium].to_vec(),
            repair_spots: Vec::new(),
            hold_site: Some(CENTER),
            cores: Vec::new(),
            crystals: Vec::new(),
            glow_paths: Vec::new(),
            beams: Vec::new(),
            lights: Vec::new(),
            fall_y: None,
            hold_name: "DEPOSIT",
            raid_stands: Vec::new(),
            routes: Vec::new(),
            sculpted: crate::render::mesh::MeshData::default(),
            rock_dust: None,
            sounds: Vec::new(),
        };
        let mut terrain = Terrain::from_fn(Vec2::ZERO, 2.0, 91, 91, ground);
        terrain.color = SLATE;
        terrain.material = Material::Sand;
        map.terrain = Some(terrain);
        let mut rng = Rng::new(0x1735_5a12);

        map.kiln_court(&mut rng);
        map.warren(&mut rng);
        map.river(&mut rng);
        map.ithessar_base();
        map.imperium_base(&mut rng);
        map.kiln_rim();
        map.kiln_spawns();
        map.kiln_horizon(&mut rng);
        map.kiln_sounds();
        map
    }

    /// What sounds here: every lit crystal in the arena, the cores held
    /// between crystal tips, and the plasma river down the canyon.
    fn kiln_sounds(&mut self) {
        let inside = |p: Vec3| (0.0..=SIZE).contains(&p.x) && (0.0..=SIZE).contains(&p.z);
        let mut found = Vec::new();
        for c in self.crystals.iter().filter(|c| c.lit) {
            found.push((Ambient::Crystal, c.base + c.axis * 0.35));
        }
        for c in &self.cores {
            found.push((Ambient::Core, c.at));
        }
        // Along the river, one every 14 m of its length.
        for g in self.glow_paths.iter().filter(|g| g.points.len() >= 4) {
            let mut owed = 0.0;
            for w in g.points.windows(2) {
                owed += w[0].distance(w[1]);
                if owed >= 14.0 {
                    owed = 0.0;
                    found.push((Ambient::River, w[1] + Vec3::Y * 0.5));
                }
            }
        }
        for (kind, at) in found {
            if inside(at) {
                self.sound(kind, at);
            }
        }
    }

    /// One crystal standing on the ground at `at`: `h` tall, `r` thick,
    /// tipped `lean` radians toward `dir` (a ground direction).
    #[allow(clippy::too_many_arguments)]
    fn crystal(
        &mut self,
        at: Vec2,
        h: f32,
        r: f32,
        dir: Vec2,
        lean: f32,
        lit: bool,
        rng: &mut Rng,
    ) {
        let g = self.ground_height(at.x, at.y);
        let d = dir.normalize_or(Vec2::X);
        let axis = Vec3::new(d.x * lean.sin(), lean.cos(), d.y * lean.sin()) * (h + 1.2);
        self.crystals.push(Crystal {
            base: Vec3::new(at.x, g - 1.2, at.y),
            axis,
            radius: r,
            sides: 5 + (rng.next_u64() % 3) as u32,
            seed: rng.next_u64() as u32,
            color: GLASS * rng.range(0.85, 1.2),
            lit,
            solid: true,
            raw: false,
        });
    }

    /// The kiln: six great crystals on the edge of the dais leaning in, a
    /// seam of plasma up each, holding the core between their tips. A few
    /// low crystals on the court for cover.
    fn kiln_court(&mut self, rng: &mut Rng) {
        let mut prongs = Vec::new();
        for i in 0..6 {
            let a = i as f32 / 6.0 * TAU + PI / 6.0;
            let out = Vec2::new(a.cos(), a.sin());
            let at = CENTER + out * KILN_R;
            let (h, lean) = (rng.range(21.0, 23.0), 0.55);
            self.crystal(at, h, 2.4, -out, lean, true, rng);
            let c = self.crystals.last().unwrap();
            prongs.push(c.base + c.axis * 0.95);
        }
        let g = self.ground_height(CENTER.x, CENTER.y);
        self.cores.push(Core {
            at: Vec3::new(CENTER.x, g + CORE_Y, CENTER.y),
            radius: 3.6,
            prongs,
        });
        self.ring_glow(CENTER, 7.0, g, 0.5);
        // Low cover on the court, between the great crystals: something to
        // duck behind on the way up to the dais.
        for i in 0..6 {
            let a = i as f32 / 6.0 * TAU;
            let out = Vec2::new(a.cos(), a.sin());
            let at = CENTER + out * (DAIS_R + 2.5);
            self.crystal(at, rng.range(2.6, 3.4), 1.3, out, 0.04, i % 2 == 0, rng);
            let beside = at + Vec2::new(-out.y, out.x) * 2.2;
            self.crystal(beside, rng.range(1.6, 2.2), 0.9, out, 0.06, false, rng);
        }
    }

    /// Stone everywhere that isn't a lane, court or base: worked blocks
    /// along the lane edges, plumb and flat-faced so they read as walls,
    /// block sight, and bank plasma true; sparser and bigger deeper in. Placed on the west half and
    /// mirrored, so both sides fight in the same warren.
    fn warren(&mut self, rng: &mut Rng) {
        let step = 4.2;
        for ix in 0..((MID / step) as usize + 1) {
            for iz in 0..((SIZE / step) as usize + 1) {
                let p = Vec2::new(ix as f32 * step, iz as f32 * step)
                    + Vec2::new(rng.range(-1.2, 1.2), rng.range(-1.2, 1.2));
                if p.x > MID - 0.5 || p.x < -2.0 || p.y < -2.0 || p.y > SIZE + 2.0 {
                    continue;
                }
                let depth = -openness(p);
                if depth < 2.2 {
                    continue;
                }
                // Deep inside, only now and then: a big one to break the skyline.
                if depth > 10.0 && !rng.chance(0.18) {
                    continue;
                }
                // A block of stone squared to the lane beside it: its face
                // flat and plumb, so plasma banks off it true. Each is a
                // little wider than the grid, so they close up into a wall.
                let e = 1.0;
                let grad = Vec2::new(
                    openness(p + Vec2::X * e) - openness(p - Vec2::X * e),
                    openness(p + Vec2::Y * e) - openness(p - Vec2::Y * e),
                );
                let edge = depth < 6.0;
                let h = if edge {
                    rng.range(6.0, 9.0)
                } else {
                    rng.range(8.0, 16.0)
                };
                let shade = rng.range(0.85, 1.15);
                let normal = if grad.length() > 0.01 {
                    grad.normalize()
                } else {
                    Vec2::from_angle(rng.range(0.0, TAU))
                };
                let mirror = |q: Vec2| Vec2::new(SIZE - q.x, q.y);
                let flip = |d: Vec2| Vec2::new(-d.x, d.y);
                for (q, n) in [(p, normal), (mirror(p), flip(normal))] {
                    self.warren_block(q, n, h, shade, !edge);
                }
            }
        }
    }

    /// One block of the warren at `at`, its face turned to `normal`
    /// (along the ground), `h` tall; deep ones bigger.
    fn warren_block(&mut self, at: Vec2, normal: Vec2, h: f32, shade: f32, deep: bool) {
        let along = normal.perp();
        let g = self.ground_height(at.x, at.y);
        let size = if deep { 3.4 } else { 2.6 };
        // Local +z along the wall, local +x out of its face. A block from
        // the rocks toolkit: it collides as what's drawn, its face near
        // plumb.
        let yaw = along.x.atan2(along.y);
        self.oboxes.push(OBox {
            side_mat: Material::Rubble,
            top_mat: Material::Rubble,
            ..rocks::block(at, yaw, Vec2::new(2.4, size), g - 1.0, g + h, STONE * shade)
        });
    }

    /// Plasma showing through cracks in the canyon floor: one jagged
    /// fissure the length of the canyon, with branches splitting off it.
    fn river(&mut self, rng: &mut Rng) {
        // The whole canyon: the west half, then its mirror back out east.
        let mut ctrl: Vec<Vec2> = CANYON.0.iter().map(|&(x, z)| Vec2::new(x, z)).collect();
        let east: Vec<Vec2> = ctrl
            .iter()
            .rev()
            .skip(1)
            .map(|p| Vec2::new(SIZE - p.x, p.y))
            .collect();
        ctrl.extend(east);
        // Walk it in short straight steps, kinked sideways: a crack, not a curve.
        let mut line = Vec::new();
        for w in ctrl.windows(2) {
            let steps = (w[0].distance(w[1]) / 1.8).ceil() as usize;
            for k in 0..steps {
                let p = w[0].lerp(w[1], k as f32 / steps as f32);
                let d = (w[1] - w[0]).normalize();
                line.push(p + Vec2::new(-d.y, d.x) * rng.range(-0.7, 0.7));
            }
        }
        let width = |rng: &mut Rng, base: f32| base * rng.range(0.6, 1.3);
        let main_w: Vec<f32> = line.iter().map(|_| width(rng, 0.16)).collect();
        self.crack(&line, main_w);
        // Branches: every so often a smaller crack splits off at an angle,
        // narrowing to nothing.
        let mut i = 4;
        while i + 2 < line.len() {
            let d = (line[i + 1] - line[i - 1]).normalize();
            let side = if rng.chance(0.5) { 1.0 } else { -1.0 };
            let mut dir = (d * 0.6 + Vec2::new(-d.y, d.x) * side).normalize();
            let mut p = line[i];
            let n = 3 + (rng.next_u64() % 4) as usize;
            let mut pts = vec![p];
            for _ in 0..n {
                dir = (dir + Vec2::new(rng.range(-0.5, 0.5), rng.range(-0.5, 0.5))).normalize();
                p += dir * rng.range(1.0, 1.8);
                pts.push(p);
            }
            let ws = (0..pts.len())
                .map(|k| 0.12 * (1.0 - k as f32 / pts.len() as f32) + 0.02)
                .collect();
            self.crack(&pts, ws);
            i += 3 + (rng.next_u64() % 4) as usize;
        }
    }

    /// A glowing crack along `pts` (ground points) with half-widths `widths`.
    fn crack(&mut self, pts: &[Vec2], widths: Vec<f32>) {
        let points = pts
            .iter()
            .map(|p| Vec3::new(p.x, self.ground_height(p.x, p.y) + 0.05, p.y))
            .collect();
        self.glow_paths.push(GlowPath {
            points,
            widths,
            color: PLASMA,
        });
    }

    /// The Ithessar base: glowing drop rings, lanterns over the ways out.
    fn ithessar_base(&mut self) {
        for &p in &[
            Vec2::new(12.0, MID - 14.0),
            Vec2::new(12.0, MID + 14.0),
            Vec2::new(18.0, MID),
        ] {
            let g = self.ground_height(p.x, p.y);
            self.ring_glow(p, 3.0, g, 0.35);
        }
        for &(x, z) in &[(24.0, 112.0), (26.0, 90.0), (24.0, 68.0)] {
            let g = self.ground_height(x, z);
            self.cores.push(Core {
                at: Vec3::new(x, g + 9.0, z),
                radius: 0.9,
                prongs: Vec::new(),
            });
        }
    }

    /// The Imperium's breach on the east edge: drop pods, prefab walls,
    /// floodlights. Built in the base court, so it costs no one cover.
    fn imperium_base(&mut self, rng: &mut Rng) {
        for &(x, z) in &[(170.0, 66.0), (170.0, 114.0)] {
            let at = Vec2::new(x, z);
            let g = self.ground_height(x, z);
            for yaw in [0.0, PI / 4.0] {
                self.oboxes.push(OBox {
                    center: at,
                    yaw,
                    half: Vec2::splat(2.4),
                    y0: g - 0.6,
                    y1: g + 4.6,
                    color: IMPERIAL,
                    side_mat: Material::Painted,
                    top_mat: Material::Painted,
                    chip: 0.2,
                    ..OBox::default()
                });
            }
            self.oboxes.push(OBox {
                center: at,
                yaw: PI / 8.0,
                half: Vec2::splat(1.5),
                y0: g + 4.6,
                y1: g + 5.8,
                color: STEEL,
                side_mat: Material::Rust,
                top_mat: Material::Rust,
                ..OBox::default()
            });
        }
        for k in 0..2 {
            let z = MID - 10.0 + k as f32 * 20.0 + rng.range(-1.0, 1.0);
            let at = Vec2::new(SIZE - 22.0, z);
            let g = self.ground_height(at.x, at.y);
            self.oboxes.push(OBox {
                center: at,
                yaw: rng.range(-0.15, 0.15),
                half: Vec2::new(0.6, 3.2),
                y0: g - 0.5,
                y1: g + 2.0,
                color: CONCRETE,
                side_mat: Material::Concrete,
                top_mat: Material::Concrete,
                chip: 0.3,
                ..OBox::default()
            });
        }
        for &(x, z) in &[(177.0, 76.0), (177.0, 104.0)] {
            let g = self.ground_height(x, z);
            self.oboxes.push(OBox {
                center: Vec2::new(x, z),
                half: Vec2::splat(0.22),
                y0: g - 0.5,
                y1: g + 10.0,
                color: STEEL,
                side_mat: Material::Rust,
                top_mat: Material::Rust,
                ..OBox::default()
            });
            self.oboxes.push(OBox {
                center: Vec2::new(x - 0.4, z),
                yaw: FRAC_PI_2,
                half: Vec2::new(1.1, 0.32),
                y0: g + 9.6,
                y1: g + 10.4,
                color: FLOODLIGHT,
                side_mat: Material::Glow,
                top_mat: Material::Glow,
                solid: false,
                ..OBox::default()
            });
        }
    }

    /// The edge: the crystal grows thick all round, a palisade too tall
    /// and too close-set to get through, leaning out over the berm. Behind
    /// the Imperium's breach on the east it's knocked low and askew, leaning
    /// in where they blasted their way through, but still standing.
    fn kiln_rim(&mut self) {
        let mut rng = Rng::new(0x0e0d_9e11);
        for (p, inward) in self.edge_points(2.6, 4.0) {
            let at = p - inward * rng.range(0.3, 1.0)
                + Vec2::new(-inward.y, inward.x) * rng.range(-0.4, 0.4);
            let breach = p.x > SIZE - 0.5 && (p.y - MID).abs() < 36.0;
            let r = rng.range(2.5, 3.4);
            let jitter = Vec2::new(rng.range(-0.6, 0.6), rng.range(-0.6, 0.6));
            let (h, lean, dir) = if breach {
                (rng.range(3.5, 6.5), rng.range(0.25, 0.5), inward + jitter)
            } else {
                (
                    rng.range(8.0, 17.0),
                    rng.range(0.05, 0.25),
                    -inward + jitter,
                )
            };
            let lit = !breach && rng.chance(0.12);
            self.crystal(at, h, r, dir, lean, lit, &mut rng);
        }
    }

    /// Bases at the west and east edges; roam points in every lane and
    /// court; repair crates in the side courts and where the outer lanes meet.
    fn kiln_spawns(&mut self) {
        for (side, xs) in [(0, [9.0, 17.0]), (1, [SIZE - 9.0, SIZE - 17.0])] {
            for x in xs {
                for k in 0..9 {
                    self.bases[side].push(Vec2::new(x, MID - 24.0 + k as f32 * 6.0));
                }
            }
        }
        let mut spots = vec![CENTER + Vec2::new(0.0, 9.0), CENTER + Vec2::new(0.0, -9.0)];
        for (pts, _) in lanes() {
            for &(x, z) in pts.iter() {
                spots.push(Vec2::new(x, z));
                spots.push(Vec2::new(SIZE - x, z));
            }
        }
        spots.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
        spots.dedup_by(|a, b| a.distance(*b) < 3.0);
        let spots: Vec<Vec2> = spots.into_iter().filter(|&p| self.clear(p, 3.5)).collect();
        self.open_spots = spots;
        self.repair_spots = vec![
            Vec2::new(44.0, 88.0),
            Vec2::new(SIZE - 44.0, 88.0),
            Vec2::new(MID, 150.0),
            Vec2::new(MID, 28.0),
        ];
        for side in 0..2 {
            let base = std::mem::take(&mut self.bases[side]);
            self.bases[side] = base.into_iter().filter(|&p| self.clear(p, 3.5)).collect();
        }
    }

    /// Nothing solid within `r` of a ground point.
    fn clear(&self, p: Vec2, r: f32) -> bool {
        !self.oboxes.iter().any(|o| o.solid && o.distance(p) < r)
            && !self.crystals.iter().any(|c| c.solid && c.distance(p) < r)
    }

    /// Beyond the edge: ash flats, crystal thickets on the berm, the great
    /// spires of Ithessar country on the horizon, and another kept sun.
    fn kiln_horizon(&mut self, rng: &mut Rng) {
        let (cell, reach) = (4.0f32, 560.0f32);
        let n = ((SIZE + 2.0 * reach) / cell) as usize + 1;
        let mut t = Terrain::from_fn(Vec2::splat(-reach), cell, n, n, scenery_ground);
        t.color = SLATE * 0.9;
        t.material = Material::Sand;
        self.scenery_terrain = Some(t);

        // Thickets on the berm, so the warren seems to go on.
        for _ in 0..260 {
            let a = rng.range(0.0, TAU);
            let d = rng.range(4.0, 60.0);
            let c = CENTER + Vec2::new(a.cos(), a.sin()) * (MID * 1.15 + d);
            if edge_distance(c) < 3.0 {
                continue;
            }
            let g = scenery_ground(c.x, c.y);
            let h = rng.range(6.0, 20.0);
            let lean = rng.range(0.0, 0.3);
            let tilt = Vec3::new(
                rng.range(-1.0, 1.0) * lean,
                1.0,
                rng.range(-1.0, 1.0) * lean,
            );
            self.crystals.push(Crystal {
                base: Vec3::new(c.x, g - 1.5, c.y),
                axis: tilt.normalize() * h,
                radius: rng.range(1.5, 4.5),
                sides: 5 + (rng.next_u64() % 3) as u32,
                seed: rng.next_u64() as u32,
                color: GLASS,
                lit: rng.chance(0.15),
                solid: false,
                raw: false,
            });
        }
        // The great spires, tallest to the west.
        for k in 0..18 {
            let a = k as f32 / 18.0 * TAU + rng.range(-0.12, 0.12);
            let d = rng.range(230.0, 440.0);
            let c = CENTER + Vec2::new(a.cos(), a.sin()) * d;
            let westness = (-a.cos()).max(0.0);
            let h = rng.range(45.0, 90.0) + 80.0 * westness;
            let g = scenery_ground(c.x, c.y);
            let lean = rng.range(-0.2, 0.2);
            self.crystals.push(Crystal {
                base: Vec3::new(c.x, g - 6.0, c.y),
                axis: Vec3::new(lean, 1.0, rng.range(-0.15, 0.15)).normalize() * h,
                radius: rng.range(7.0, 14.0),
                sides: 5 + (rng.next_u64() % 3) as u32,
                seed: rng.next_u64() as u32,
                color: GLASS,
                lit: rng.chance(0.7),
                solid: false,
                raw: false,
            });
            // Some in pairs: a shorter one leaning against it.
            if rng.chance(0.5) {
                let p = c + Vec2::new(-a.sin(), a.cos()) * rng.range(14.0, 22.0);
                let to = (c - p).normalize();
                self.crystals.push(Crystal {
                    base: Vec3::new(p.x, scenery_ground(p.x, p.y) - 6.0, p.y),
                    axis: Vec3::new(to.x * 0.45, 1.0, to.y * 0.45).normalize() * h * 0.6,
                    radius: rng.range(5.0, 9.0),
                    sides: 6,
                    seed: rng.next_u64() as u32,
                    color: GLASS,
                    lit: true,
                    solid: false,
                    raw: false,
                });
            }
        }
        let far = CENTER + Vec2::new(-320.0, 250.0);
        self.cores.push(Core {
            at: Vec3::new(far.x, scenery_ground(far.x, far.y) + 150.0, far.y),
            radius: 26.0,
            prongs: Vec::new(),
        });
    }

    /// A ring of glowing segments set into the ground (visual only).
    fn ring_glow(&mut self, at: Vec2, r: f32, g: f32, width: f32) {
        let n = ((TAU * r / 1.6) as usize).max(8);
        for k in 0..n {
            let a = (k as f32 + 0.5) / n as f32 * TAU;
            let p = at + Vec2::new(a.cos(), a.sin()) * r;
            self.oboxes.push(OBox {
                center: p,
                yaw: FRAC_PI_2 - a,
                half: Vec2::new(TAU * r / n as f32 * 0.42, width * 0.5),
                y0: g - 0.3,
                y1: g + 0.04,
                color: PLASMA,
                side_mat: Material::Glow,
                top_mat: Material::Glow,
                solid: false,
                ..OBox::default()
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::NavGrid;

    #[test]
    fn every_spawn_reaches_the_kiln_and_all_three_lanes() {
        let m = Map::kiln();
        assert!(
            m.open_spots.len() >= 20,
            "{} open spots",
            m.open_spots.len()
        );
        for (side, base) in m.bases.iter().enumerate() {
            assert!(base.len() >= 14, "side {side} has {} spawns", base.len());
        }
        let nav = NavGrid::build(&m);
        let goals = [
            CENTER + Vec2::new(0.0, 9.0),
            Vec2::new(MID, 150.0),
            Vec2::new(MID, 28.0),
            Vec2::new(44.0, 88.0),
            Vec2::new(SIZE - 44.0, 88.0),
        ];
        for &p in m.open_spots.iter().chain(m.bases.iter().flatten()) {
            for g in goals {
                assert!(!nav.path(p, g).is_empty(), "no path from {p} to {g}");
            }
        }
    }

    /// Crystal closes the edge all round: nowhere can a tank drive up to
    /// out of the arena.
    #[test]
    fn the_edge_is_crystal() {
        Map::kiln().assert_edge_walled(0.5, |_| false);
    }

    #[test]
    fn lanes_are_clear_of_crystal() {
        let m = Map::kiln();
        for (pts, half) in lanes() {
            for w in pts.windows(2) {
                let (a, b) = (Vec2::new(w[0].0, w[0].1), Vec2::new(w[1].0, w[1].1));
                for k in 0..=8 {
                    let p = a.lerp(b, k as f32 / 8.0);
                    for q in [p, Vec2::new(SIZE - p.x, p.y)] {
                        assert!(m.clear(q, (half - 2.5).max(1.5)), "lane blocked at {q}");
                    }
                }
            }
        }
    }

    #[test]
    fn sightlines_are_short() {
        // From the middle lane, the gallery and the cross-cuts, most
        // directions hit crystal within 60 m: this is a close-quarters map.
        // (The canyon and the kiln court see further, on purpose.)
        let w = crate::sim::World::empty(Map::kiln());
        let mut long = 0;
        let mut total = 0;
        for (pts, _) in [&MIDDLE, &GALLERY].into_iter().chain(CUTS.iter()) {
            for &(x, z) in pts.iter() {
                let g = w.map.ground_height(x, z);
                for k in 0..16 {
                    let a = k as f32 / 16.0 * TAU;
                    let d = Vec3::new(a.cos(), 0.0, a.sin());
                    total += 1;
                    if w.raycast(Vec3::new(x, g + 2.0, z), d, 200.0) > 60.0 {
                        long += 1;
                    }
                }
            }
        }
        assert!(long * 5 < total, "{long} of {total} sightlines over 60 m");
    }

    #[test]
    fn the_core_floats_between_the_great_crystals() {
        let m = Map::kiln();
        let c = &m.cores[0];
        assert_eq!(c.prongs.len(), 6);
        for p in &c.prongs {
            let flat = Vec2::new(p.x - c.at.x, p.z - c.at.z).length();
            assert!(flat < KILN_R - 5.0, "tip {flat} m out");
            assert!(
                (p.y - c.at.y).abs() < 6.0,
                "tip at {}, core at {}",
                p.y,
                c.at.y
            );
        }
    }

    #[test]
    fn repair_spots_are_open() {
        let m = Map::kiln();
        let nav = NavGrid::build(&m);
        for &p in &m.repair_spots {
            assert!(!nav.is_blocked(p), "repair spot {p} is blocked");
        }
    }
}

#[cfg(test)]
mod play {
    use super::*;
    use crate::combat::Match;
    use crate::sim::Command;

    /// Two minutes of a 6v6 round on the Kiln: both sides score, and every
    /// bot gets out of its base and keeps moving. Slow (run with
    /// `--ignored --release`).
    #[test]
    #[ignore]
    fn bots_fight_across_the_kiln() {
        let mut m = Match::new(Map::kiln(), 0, 11, 7);
        let start: Vec<Vec2> = m
            .world
            .tanks
            .iter()
            .map(|t| Vec2::new(t.pos.x, t.pos.z))
            .collect();
        let mut furthest = vec![0.0f32; start.len()];
        let mut still = vec![0u32; start.len()];
        let mut last = start.clone();
        let steps = (120.0 / crate::game::DT) as usize;
        for k in 0..steps {
            m.step(Command::default(), false);
            if k % 120 == 0 {
                for (i, t) in m.world.tanks.iter().enumerate().skip(1) {
                    let p = Vec2::new(t.pos.x, t.pos.z);
                    furthest[i] = furthest[i].max(p.distance(start[i]));
                    if m.fighters[i].alive && p.distance(last[i]) < 0.5 {
                        still[i] += 1;
                    }
                    last[i] = p;
                }
            }
        }
        let (a, b) = (m.score(0), m.score(1));
        eprintln!("score {a} : {b}; furthest {furthest:?}; still (s) {still:?}");
        assert!(a > 0 && b > 0, "score {a} : {b}");
        for i in 1..furthest.len() {
            assert!(furthest[i] > 35.0, "bot {i} only got {} m out", furthest[i]);
            assert!(still[i] < 40, "bot {i} sat still {} s", still[i]);
        }
    }
}
