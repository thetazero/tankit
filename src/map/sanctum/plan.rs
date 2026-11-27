//! What one Sanctum is: the shape of its mesa, the proportions of its
//! church, what stands round it, the land past it. All of it comes of a
//! seed; seed 0 is the hand-made one every other is a variation on. Whatever
//! kind of world it's on, a seed is the same Sanctum: the land's `Palette`
//! is only looks.

use std::f32::consts::{FRAC_PI_2, PI, TAU};
use std::ops::RangeInclusive;

use glam::{Vec2, Vec3};

use super::{CENTER, HALF_W, LOW, SIZE, smoothstep};
use crate::map::procgen::foundry::Ground;
use crate::map::strata::Rock;
use crate::render::Atmosphere;
use crate::render::material::Material;
use crate::rng::Rng;

/// A nave bay's length, m.
pub const BAY: f32 = 9.0;
/// Where the nave meets the crossing.
pub const NAVE_END: f32 = 138.0;

/// The land's looks.
pub struct Palette {
    pub land: Vec3,
    pub material: Material,
    /// The land where it's bare (worn, or blown clear), in patches; `None`:
    /// it's all one.
    pub bare: Option<Vec3>,
    pub stone: Vec3,
    pub roof: Vec3,
    pub rock: Rock,
    /// Loose blocks off the cliffs.
    pub blocks: [Vec3; 3],
}

impl Palette {
    pub fn of(ground: Ground) -> Palette {
        match ground {
            Ground::Desert => Palette {
                land: Vec3::new(0.78, 0.66, 0.5),
                material: Material::Sand,
                bare: None,
                stone: Vec3::new(0.82, 0.76, 0.64),
                roof: Vec3::new(0.3, 0.28, 0.3),
                rock: Rock::SANDSTONE,
                blocks: [
                    Vec3::new(0.66, 0.44, 0.29),
                    Vec3::new(0.55, 0.34, 0.22),
                    Vec3::new(0.74, 0.56, 0.39),
                ],
            },
            // Dark gneiss under snow; the church a cold pale grey.
            Ground::Tundra => {
                let snow = Vec3::new(0.7, 0.73, 0.78);
                Palette {
                    land: snow,
                    material: Material::Rubble,
                    bare: Some(Vec3::new(0.3, 0.3, 0.3)),
                    stone: Vec3::new(0.44, 0.45, 0.48),
                    roof: Vec3::new(0.5, 0.54, 0.61),
                    rock: Rock {
                        dust: snow,
                        ground: Material::Rubble,
                        lying: 0.85,
                        shades: [
                            Vec3::new(0.20, 0.21, 0.23),
                            Vec3::new(0.14, 0.15, 0.17),
                            Vec3::new(0.24, 0.25, 0.27),
                            Vec3::new(0.17, 0.18, 0.20),
                            Vec3::new(0.18, 0.19, 0.20),
                            Vec3::new(0.22, 0.24, 0.26),
                        ],
                        bleach: Vec3::new(0.33, 0.35, 0.39),
                        soft: Vec3::new(0.47, 0.48, 0.51),
                    },
                    blocks: [
                        Vec3::new(0.20, 0.21, 0.23),
                        Vec3::new(0.15, 0.16, 0.18),
                        Vec3::new(0.24, 0.25, 0.27),
                    ],
                }
            }
            // Grey limestone standing out of grassland, grass on its ledges.
            Ground::Grassland => {
                let grass = Vec3::new(0.18, 0.23, 0.095);
                Palette {
                    land: grass,
                    material: Material::Rubble,
                    bare: Some(Vec3::new(0.31, 0.27, 0.17)),
                    stone: Vec3::new(0.68, 0.67, 0.6),
                    roof: Vec3::new(0.27, 0.23, 0.2),
                    rock: Rock {
                        dust: grass,
                        ground: Material::Rubble,
                        lying: 0.85,
                        shades: [
                            Vec3::new(0.5, 0.49, 0.44),
                            Vec3::new(0.36, 0.36, 0.33),
                            Vec3::new(0.57, 0.56, 0.5),
                            Vec3::new(0.42, 0.42, 0.37),
                            Vec3::new(0.46, 0.44, 0.38),
                            Vec3::new(0.53, 0.52, 0.48),
                        ],
                        bleach: Vec3::new(0.7, 0.69, 0.64),
                        soft: Vec3::new(0.85, 0.88, 0.8),
                    },
                    blocks: [
                        Vec3::new(0.5, 0.49, 0.44),
                        Vec3::new(0.38, 0.38, 0.34),
                        Vec3::new(0.56, 0.55, 0.49),
                    ],
                }
            }
        }
    }
}

/// A graveyard: its corner, and its two tombs (offset from its middle, a
/// mausoleum or a plain tomb).
pub struct Yard {
    pub at: Vec2,
    pub tombs: [(Vec2, bool); 2],
}

/// Resonant crystals breaking the surface.
pub struct Cluster {
    pub at: Vec2,
    pub n: usize,
    pub width: f32,
    pub height: f32,
}

/// Cells in a row along x: the first one's middle, how many, and which way
/// the church isn't (+1 north of it, -1 south).
pub struct Cells {
    pub first: Vec2,
    pub n: usize,
    pub side: f32,
}

/// A broken cloister along x, likewise.
pub struct Cloister {
    pub first: Vec2,
    pub n: usize,
    pub side: f32,
}

pub struct Plan {
    pub seed: u64,
    pub ground: Ground,
    pub colors: Palette,

    /// The rim's waves: how far it's drawn out east and west, and the size
    /// and phase of the five- and ninefold ones.
    stretch: f32,
    waves: [(f32, f32); 2],
    /// Bays cut back into the mesa: (angle, half-width in radians, depth in
    /// m). Kept clear of the bases (east and west) and of everything built.
    bays: Vec<(f32, f32, f32)>,
    /// Spurs jutting out of it.
    spurs: Vec<(f32, f32, f32)>,
    /// Notches broken out of the top edge, where rock has fallen away.
    pub notches: Vec<(f32, f32, f32)>,
    /// Buttes broken away from the mesa, down on the floor: (angle, how far
    /// out from the edge, radius, height over the floor).
    pub buttes: Vec<(f32, f32, f32, f32)>,
    /// Mesas and buttes on the horizon: (center, radius, height over the
    /// floor).
    pub hills: Vec<(Vec2, f32, f32)>,

    /// How many bays the nave has, and which one's wall has fallen, on
    /// which side (+1 north).
    pub nave_bays: usize,
    pub breach: (usize, f32),
    /// The transepts reach this far north and south of the axis.
    pub arm: f32,
    /// Where the chancel ends and the apse goes round.
    pub chancel_x: f32,
    /// Which of the apse's five segments have fallen in.
    pub fallen: RangeInclusive<usize>,
    /// The west front's towers, south then north: how tall, and whether the
    /// spire still stands.
    pub towers: [(f32, bool); 2],
    /// How far the crossing tower's walls stand over the roof, and its
    /// spire's height.
    pub crossing: (f32, f32),
    /// Which aisle a pillar's down across (+1 north).
    pub pillar_side: f32,

    pub chapels: Vec<(Vec2, Vec2)>,
    pub bell: Vec2,
    pub cells: Option<Cells>,
    pub cloister: Option<Cloister>,
    pub yards: Vec<Yard>,
    pub clusters: Vec<Cluster>,
    /// Low walls on each side's approach: (middle, yaw, half-length).
    pub walls: Vec<(Vec2, f32, f32)>,
    pub repairs: Vec<Vec2>,
}

/// `d` radians wrapped to -PI..PI.
fn wrap(d: f32) -> f32 {
    (d + PI).rem_euclid(TAU) - PI
}

/// Sum of bumps at angle `a`: (center, half-width, size) each.
pub fn bumps(a: f32, list: &[(f32, f32, f32)]) -> f32 {
    list.iter()
        .map(|&(c, w, h)| h * (-(wrap(a - c) / w).powi(2)).exp())
        .sum()
}

/// The cone of scree round a rock `r` across and `h` tall, at distance `d`
/// from its middle.
fn talus(d: f32, r: f32, h: f32) -> f32 {
    0.35 * h * smoothstep(r * 2.2 + 6.0, r * 0.9, d).powf(1.6)
}

/// Ground already spoken for: a middle and half extents.
type Plot = (Vec2, Vec2);

impl Plan {
    pub fn new(ground: Ground, seed: u64) -> Plan {
        let mut plan = Plan::hand_made(ground);
        if seed != 0 {
            plan.seed = seed;
            let mut rng = Rng::new(seed ^ 0x5A4C_70AD);
            plan.vary_mesa(&mut rng);
            plan.vary_church(&mut rng);
            plan.vary_precinct(&mut rng);
        }
        plan
    }

    fn hand_made(ground: Ground) -> Plan {
        let v = Vec2::new;
        let yard = |x, z, tombs| Yard { at: v(x, z), tombs };
        let cluster = |x, z, n, width, height| Cluster {
            at: v(x, z),
            n,
            width,
            height,
        };
        Plan {
            seed: 0,
            ground,
            colors: Palette::of(ground),
            stretch: 6.0,
            waves: [(4.0, 0.7), (1.5, 2.0)],
            bays: vec![
                (0.85, 0.16, 11.0),
                (2.25, 0.15, 9.0),
                (-0.95, 0.18, 15.0),
                (-2.25, 0.17, 15.0),
            ],
            spurs: vec![
                (0.45, 0.06, 9.0),
                (1.6, 0.07, 12.0),
                (-0.4, 0.05, 8.0),
                (-2.78, 0.07, 11.0),
            ],
            notches: vec![
                (0.8, 0.025, 3.0),
                (1.35, 0.03, 4.0),
                (1.95, 0.02, 2.5),
                (2.75, 0.03, 3.5),
                (-0.6, 0.025, 3.0),
                (-1.5, 0.03, 4.0),
                (-2.6, 0.02, 2.5),
                (3.4, 0.03, 3.0),
            ],
            buttes: vec![(2.5, 30.0, 8.0, 30.0), (-0.15, 34.0, 6.0, 24.0)],
            hills: vec![
                (v(-260.0, 120.0), 90.0, 46.0),
                (v(520.0, -80.0), 120.0, 38.0),
                (v(420.0, 520.0), 70.0, 52.0),
                (v(-150.0, 520.0), 60.0, 30.0),
                (v(150.0, -330.0), 100.0, 44.0),
                (v(-200.0, -220.0), 18.0, 48.0),
                (v(520.0, 260.0), 14.0, 40.0),
                (v(60.0, 560.0), 16.0, 58.0),
                (v(-330.0, 360.0), 12.0, 34.0),
            ],
            nave_bays: 4,
            breach: (2, 1.0),
            arm: 36.0,
            chancel_x: 172.0,
            fallen: 1..=2,
            towers: [(27.0, true); 2],
            crossing: (14.0, 16.0),
            pillar_side: -1.0,
            chapels: vec![
                (v(98.0, 196.0), v(7.0, 5.0)),
                (v(202.0, 104.0), v(7.0, 5.0)),
                (v(208.0, 192.0), v(5.0, 6.0)),
                (v(92.0, 108.0), v(5.0, 6.0)),
            ],
            bell: v(82.0, 186.0),
            cells: Some(Cells {
                first: v(122.0, 80.0),
                n: 6,
                side: -1.0,
            }),
            cloister: Some(Cloister {
                first: v(120.0, 222.0),
                n: 9,
                side: 1.0,
            }),
            yards: vec![
                yard(78.0, 128.0, [(v(-4.6, -3.6), true), (v(4.6, -3.6), false)]),
                yard(110.0, 186.0, [(v(4.6, 3.6), true), (v(-1.0, -4.4), false)]),
                yard(184.0, 116.0, [(v(-4.6, 3.6), true), (v(4.6, -3.6), false)]),
            ],
            clusters: vec![
                cluster(110.0, 130.0, 4, 1.6, 3.5),
                cluster(190.0, 172.0, 4, 1.6, 3.5),
                cluster(150.0, 112.0, 3, 1.2, 2.6),
                cluster(150.0, 192.0, 3, 1.2, 2.6),
                cluster(75.0, 152.0, 3, 1.4, 3.0),
                cluster(226.0, 148.0, 3, 1.4, 3.0),
                cluster(176.0, 220.0, 4, 1.8, 4.0),
                cluster(130.0, 72.0, 4, 1.8, 4.0),
            ],
            walls: vec![
                (v(88.0, 140.0), 0.2, 6.0),
                (v(86.0, 162.0), -0.3, 5.0),
                (v(212.0, 160.0), 0.2, 6.0),
                (v(214.0, 138.0), -0.3, 5.0),
            ],
            repairs: vec![
                CENTER + v(0.0, 58.0),
                CENTER + v(0.0, -58.0),
                CENTER + v(-66.0, 22.0),
                CENTER + v(62.0, -22.0),
            ],
        }
    }

    fn vary_mesa(&mut self, rng: &mut Rng) {
        self.stretch = rng.range(5.0, 9.0);
        self.waves = [
            (rng.range(2.5, 4.5), rng.range(0.0, TAU)),
            (rng.range(0.8, 1.8), rng.range(0.0, TAU)),
        ];
        // Bays only on the diagonals, clear of the bases and of what's
        // built to the north and south.
        self.bays.clear();
        for q in 0..4 {
            if rng.chance(0.8) {
                let at = FRAC_PI_2 * (q as f32 + 0.5) + rng.range(-0.14, 0.14);
                self.bays
                    .push((at, rng.range(0.12, 0.2), rng.range(7.0, 16.0)));
            }
        }
        // Spread round: one to a stretch of rim.
        let round = |rng: &mut Rng, n: i32| -> Vec<f32> {
            let spin = rng.range(0.0, TAU);
            (0..n)
                .map(|k| spin + (k as f32 + rng.range(-0.3, 0.3)) * TAU / n as f32)
                .collect()
        };
        let n = rng.int(2, 5);
        self.spurs = round(rng, n)
            .into_iter()
            .map(|a| {
                // (Less of them east and west, where the rim's far out.)
                let most = 12.0 - 4.0 * a.cos().abs();
                (a, rng.range(0.05, 0.08), rng.range(5.0, most))
            })
            .collect();
        let n = rng.int(5, 10);
        self.notches = round(rng, n)
            .into_iter()
            .map(|a| (a, rng.range(0.02, 0.032), rng.range(2.3, 4.0)))
            .collect();
        // In the arena's corners, where there's room for them.
        let first = rng.index(4);
        self.buttes = (0..rng.int(1, 3) as usize)
            .map(|k| {
                let a = FRAC_PI_2 * ((first + k) as f32 + 0.5) + rng.range(-0.2, 0.2);
                let r = rng.range(5.0, 9.0);
                (a, rng.range(28.0, 38.0), r, rng.range(18.0, 34.0))
            })
            .collect();
        self.hills.clear();
        let n = rng.int(8, 12);
        for a in round(rng, n) {
            let great = rng.chance(0.55);
            let (r, h) = if great {
                (rng.range(55.0, 110.0), rng.range(28.0, 52.0))
            } else {
                (rng.range(11.0, 20.0), rng.range(32.0, 60.0))
            };
            let far = rng.range(300.0, 440.0) + if great { r } else { 0.0 };
            let c = CENTER + Vec2::from_angle(a) * far;
            if self
                .hills
                .iter()
                .all(|&(o, s, _)| o.distance(c) > (r + s) * 1.3)
            {
                self.hills.push((c, r, h));
            }
        }
    }

    fn vary_church(&mut self, rng: &mut Rng) {
        let side = |rng: &mut Rng| if rng.chance(0.5) { 1.0 } else { -1.0 };
        self.nave_bays = rng.int(3, 5) as usize;
        // Not the bay at either end: the towers and the transepts are there.
        self.breach = (rng.int(1, self.nave_bays as i32 - 2) as usize, side(rng));
        self.arm = [28.0, 36.0, 42.0][rng.index(3)];
        self.chancel_x = [168.0, 172.0, 178.0][rng.index(3)];
        self.fallen = [1..=2, 2..=3, 1..=3][rng.index(3)].clone();
        let pair = rng.range(23.0, 32.0);
        self.towers = [(pair, true); 2];
        // One of the pair's down, more often than not.
        if rng.chance(0.6) {
            self.towers[rng.index(2)] = (rng.range(20.5, 23.0), false);
        }
        self.crossing = (rng.range(8.0, 16.0), rng.range(11.0, 22.0));
        self.pillar_side = side(rng);
    }

    /// Where the ground's taken before anything's put round the church: the
    /// church, the ways to its doors, and the bases.
    fn taken(&self) -> Vec<Plot> {
        let rect = |x0: f32, x1: f32, z0: f32, z1: f32| {
            let (a, b) = (Vec2::new(x0, z0), Vec2::new(x1, z1));
            (CENTER + (a + b) * 0.5, (b - a) * 0.5)
        };
        let nx = self.nave_x() - CENTER.x;
        let ch = self.chancel_x - CENTER.x;
        let arm = self.arm + 2.0;
        let mut out = vec![
            rect(nx - 7.0, 0.0, -19.0, 19.0),
            rect(-14.0, 14.0, -arm, arm),
            rect(12.0, ch + 13.0, -13.0, 13.0),
            // The doors, and the breaches.
            rect(-74.0, nx, -8.0, 8.0),
            rect(ch, 74.0, -8.0, 8.0),
            rect(-6.0, 6.0, -arm - 9.0, arm + 9.0),
        ];
        let x = self.breach_x() - CENTER.x;
        let s = self.breach.1;
        let (a, b) = (s * HALF_W, s * (HALF_W + 14.0));
        out.push(rect(x - 5.0, x + 5.0, a.min(b), a.max(b)));
        for s in [-1.0, 1.0] {
            let (a, b) = (s * 98.0f32, s * 73.0f32);
            out.push(rect(a.min(b), a.max(b), -28.0, 28.0));
        }
        out
    }

    /// Whether what's `half` across fits at `c`: on the top, well back from
    /// the edge, `gap` clear of all that's `taken`.
    fn fits(&self, taken: &[Plot], c: Vec2, half: Vec2, gap: f32) -> bool {
        let back = (-2..=2).all(|i| {
            (-2..=2).all(|j| {
                let at = Vec2::new(i as f32, j as f32) * 0.5;
                self.past_edge(c + half * at) < -13.0
            })
        });
        back && taken.iter().all(|(o, h)| {
            let d = (c - *o).abs() - half - *h;
            d.x > gap || d.y > gap
        })
    }

    /// The first place tried that fits, now taken.
    fn place(
        &self,
        taken: &mut Vec<Plot>,
        half: Vec2,
        gap: f32,
        mut at: impl FnMut() -> Vec2,
    ) -> Option<Vec2> {
        let c = (0..400)
            .map(|_| at())
            .find(|&c| self.fits(taken, c, half, gap))?;
        taken.push((c, half));
        Some(c)
    }

    fn vary_precinct(&mut self, rng: &mut Rng) {
        const GAP: f32 = 6.0;
        let v = Vec2::new;
        let mut taken = self.taken();
        let anywhere = |rng: &mut Rng| {
            let r = rng.range(24.0, 86.0);
            CENTER + Vec2::from_angle(rng.range(0.0, TAU)) * r
        };
        let side = if rng.chance(0.5) { 1.0 } else { -1.0 };

        // Repair drops first: one each way round the church.
        self.repairs.clear();
        for (d, wide) in [
            (v(27.0, 47.0), 7.0),
            (v(-27.0, -47.0), 7.0),
            (v(-64.0, 26.0), 10.0),
            (v(62.0, -26.0), 10.0),
        ] {
            let d = d * v(1.0, side);
            let at = || CENTER + d + v(rng.range(-wide, wide), rng.range(-3.0, 8.0) * d.y.signum());
            if let Some(c) = self.place(&mut taken, Vec2::splat(3.0), 1.0, at) {
                self.repairs.push(c);
            }
        }
        // The cells one side of the church, the cloister the other. Shorter
        // if the rim won't have them whole.
        self.cells = (3..=rng.int(4, 7)).rev().find_map(|n| {
            let half = v(5.5 * n as f32, 4.2);
            let at = || CENTER + v(rng.range(-22.0, 22.0), side * rng.range(60.0, 72.0));
            let c = self.place(&mut taken, half, GAP, at)?;
            let first = c - v(5.5 * (n - 1) as f32, 0.0);
            Some(Cells {
                first,
                n: n as usize,
                side,
            })
        });
        self.cloister = (4..=rng.int(6, 9)).rev().find_map(|n| {
            let half = v(3.75 * n as f32 + 1.0, 4.0);
            let at = || CENTER + v(rng.range(-22.0, 22.0), -side * rng.range(60.0, 72.0));
            let c = self.place(&mut taken, half, GAP, at)?;
            let first = c + v(-3.75 * (n - 1) as f32, -side * 2.5);
            Some(Cloister {
                first,
                n: n as usize,
                side: -side,
            })
        });
        // The bell tower, round the west front or the chancel.
        let end = if rng.chance(0.5) { 1.0 } else { -1.0 };
        let at = || {
            let s = if rng.chance(0.5) { 1.0 } else { -1.0 };
            CENTER + v(end * rng.range(40.0, 72.0), s * rng.range(24.0, 56.0))
        };
        if let Some(c) = self.place(&mut taken, Vec2::splat(4.5), GAP, at) {
            self.bell = c;
        } else {
            // (There's always room in the open quarters; the tests see to it.)
            self.bell = CENTER + v(-68.0, 40.0);
            taken.push((self.bell, Vec2::splat(4.5)));
        }
        self.chapels.clear();
        for _ in 0..rng.int(2, 5) {
            let half = if rng.chance(0.5) {
                v(7.0, 5.0)
            } else {
                v(5.0, 6.0)
            };
            let c = self.place(&mut taken, half + Vec2::splat(0.6), GAP, || anywhere(rng));
            self.chapels.extend(c.map(|c| (c, half)));
        }
        self.yards.clear();
        for _ in 0..rng.int(2, 4) {
            let at = self.place(&mut taken, v(6.8, 7.2), 4.0, || anywhere(rng));
            let Some(c) = at else { continue };
            let flip = if rng.chance(0.5) { 1.0 } else { -1.0 };
            let tombs = [
                [(v(-4.6, -3.6), true), (v(4.6, -3.6), false)],
                [(v(4.6, 3.6), true), (v(-1.0, -4.4), false)],
                [(v(-4.6, 3.6), true), (v(4.6, -3.6), false)],
            ][rng.index(3)]
            .map(|(off, m)| (off * v(flip, 1.0), m));
            self.yards.push(Yard {
                at: c - v(4.5, 5.2),
                tombs,
            });
        }
        // Low walls on the approaches, two a side.
        self.walls.clear();
        for s in [-1.0, 1.0] {
            for t in [-1.0, 1.0] {
                let len = rng.range(4.5, 6.5);
                let yaw = rng.range(-0.35, 0.35);
                let half = v(len, 0.6 + len * yaw.abs());
                let at = || CENTER + v(s * rng.range(56.0, 68.0), t * rng.range(10.0, 30.0));
                let c = self.place(&mut taken, half, 4.0, at);
                self.walls.extend(c.map(|c| (c, yaw, len)));
            }
        }
        self.clusters.clear();
        for _ in 0..rng.int(6, 10) {
            let width = rng.range(1.2, 1.9);
            let half = Vec2::splat(width + 0.6);
            let at = self.place(&mut taken, half, 4.0, || anywhere(rng));
            self.clusters.extend(at.map(|at| Cluster {
                at,
                n: rng.int(3, 5) as usize,
                width,
                height: width * rng.range(2.0, 2.4),
            }));
        }
    }

    /// The nave's west end.
    pub fn nave_x(&self) -> f32 {
        NAVE_END - BAY * self.nave_bays as f32
    }

    /// The middle of the breach in the nave wall.
    pub fn breach_x(&self) -> f32 {
        self.nave_x() - super::WALL + BAY * self.breach.0 as f32 + 7.0
    }

    /// Edge of the mesa top, distance from the center at angle `a`.
    pub fn rim(&self, a: f32) -> f32 {
        let [(w5, p5), (w9, p9)] = self.waves;
        102.0
            + self.stretch * (2.0 * a).cos()
            + w5 * (5.0 * a + p5).sin()
            + w9 * (9.0 * a + p9).sin()
            - bumps(a, &self.bays)
            + bumps(a, &self.spurs)
    }

    /// How far `p` is outside the mesa's edge (negative: on top).
    pub fn past_edge(&self, p: Vec2) -> f32 {
        let d = p - CENTER;
        d.length() - self.rim(d.y.atan2(d.x))
    }

    /// The mesa's top: flat round the church, a gentle roll further out, and
    /// broken down at the edge where a notch has fallen away.
    pub fn top(&self, x: f32, z: f32) -> f32 {
        let p = Vec2::new(x, z);
        let r = p.distance(CENTER);
        let a = (z - CENTER.y).atan2(x - CENTER.x);
        let roll = 0.3 * (x * 0.07).sin() * (z * 0.05).cos() * smoothstep(60.0, 80.0, r);
        roll - bumps(a, &self.notches) * smoothstep(-8.0, -1.0, self.past_edge(p))
    }

    pub fn butte_center(&self, a: f32, out: f32) -> Vec2 {
        CENTER + Vec2::new(a.cos(), a.sin()) * (self.rim(a) + out)
    }

    pub fn height(&self, x: f32, z: f32) -> f32 {
        let p = Vec2::new(x, z);
        let d = self.past_edge(p);
        let top = self.top(x, z);
        // The floor: scree banked against the cliff's foot, fans of it under
        // the notches where rock came down, dunes beyond.
        let a = (z - CENTER.y).atan2(x - CENTER.x);
        let fans = self
            .notches
            .iter()
            .map(|&(c, w, _)| 7.0 * (-(wrap(a - c) / (w * 2.5)).powi(2)).exp())
            .sum::<f32>();
        let scree = (10.0 + fans) * (-(d - 4.0).max(0.0) / (12.0 + fans)).exp();
        let dunes = 1.5 * (x * 0.04).sin() * (z * 0.05 + 1.0).cos();
        let cones = self
            .buttes
            .iter()
            .map(|&(a, out, r, h)| talus(p.distance(self.butte_center(a, out)), r, h))
            .fold(0.0, f32::max);
        let floor = LOW + (scree + dunes).max(cones);
        // The drop itself is steep, behind the rock face (see `strata`).
        let t = smoothstep(-1.5, 0.3, d);
        top * (1.0 - t) + floor * t
    }

    /// Beyond the arena: the open floor (the mesas out there are sculpted,
    /// see `sanctum_horizon`). Sunk out of sight under the arena.
    pub fn scenery_height(&self, x: f32, z: f32) -> f32 {
        let p = Vec2::new(x, z);
        let edge = p.clamp(Vec2::ZERO, Vec2::splat(SIZE));
        if edge == p {
            // Under the arena's own ground, meeting it at the edge, and well
            // down under the mesa: its cells are coarse, and would cut out
            // through the cliff's face.
            let inside = x.min(z).min(SIZE - x).min(SIZE - z);
            return self.height(x, z).min(LOW + 6.0) - 0.6 * smoothstep(0.0, 14.0, inside);
        }
        // Carry on from the arena's edge, easing into the open land.
        let dunes =
            2.0 * (x * 0.02).sin() * (z * 0.025 + 1.0).cos() + 1.0 * (x * 0.05 + z * 0.03).sin();
        let cones = self
            .hills
            .iter()
            .map(|&(c, r, h)| talus(p.distance(c), r, h))
            .fold(0.0, f32::max);
        let at = self.height(edge.x, edge.y);
        at + (LOW + dunes.max(cones) - at) * smoothstep(0.0, 40.0, p.distance(edge))
    }

    pub fn atmosphere(&self) -> Atmosphere {
        // Late afternoon: a low warm sun out of the west-southwest, so it
        // comes in at the great door and the south windows.
        let mut sky = Atmosphere {
            sun_dir: Vec3::new(-0.6, 0.42, -0.45),
            sun_color: Vec3::new(2.3, 1.8, 1.35),
            sky_zenith: Vec3::new(0.3, 0.45, 0.68),
            sky_horizon: Vec3::new(0.86, 0.72, 0.58),
            ground_color: Vec3::new(0.45, 0.33, 0.24),
            fog_density: 0.0014,
            ..Atmosphere::default()
        };
        if self.seed != 0 {
            // Another hour: still from the west half of the sky or the
            // east, in at one end of the church.
            let mut rng = Rng::new(self.seed ^ 0x50A2);
            let az = rng.range(-0.9, 0.9) + if rng.chance(0.5) { PI } else { 0.0 };
            sky.sun_dir = Vec3::new(az.cos() * 0.75, rng.range(0.42, 0.62), az.sin() * 0.75);
            sky.fog_density = rng.range(0.0011, 0.0016);
        }
        match self.ground {
            Ground::Desert => sky,
            // Thin cold light, the snow throwing it back up.
            Ground::Tundra => Atmosphere {
                sun_color: Vec3::new(1.6, 1.52, 1.46),
                sky_zenith: Vec3::new(0.28, 0.4, 0.58),
                sky_horizon: Vec3::new(0.64, 0.7, 0.78),
                ground_color: Vec3::new(0.26, 0.28, 0.33),
                fog_density: sky.fog_density * 1.25,
                blowing_sand: 0.4,
                dust: Vec3::new(0.78, 0.81, 0.86),
                settle: Vec3::new(0.66, 0.69, 0.74),
                ..sky
            },
            // Clear and broad.
            Ground::Grassland => Atmosphere {
                sun_color: Vec3::new(2.15, 1.96, 1.66),
                sky_zenith: Vec3::new(0.2, 0.37, 0.66),
                sky_horizon: Vec3::new(0.58, 0.65, 0.7),
                ground_color: Vec3::new(0.2, 0.23, 0.14),
                fog_density: sky.fog_density * 0.8,
                dust: Vec3::new(0.32, 0.28, 0.2),
                settle: Vec3::new(0.18, 0.21, 0.1),
                ..sky
            },
        }
    }
}
