//! Traces of the people who lived here: power poles and sagging cables,
//! burned-out cars, market stalls under faded cloth, dead palms. Placed by
//! the map; these helpers only know how to build each thing.
//!
//! Small and thin parts (cables, fronds, cloth) are visual only; bodies a
//! tank would hit (car shells, pole shafts, stall posts, crates) collide.

use glam::{Vec2, Vec3};

use super::{Ambient, Map, OBox};
use crate::render::material::Material;
use crate::rng::Rng;

impl Map {
    /// A thin piece from `a` to `b` (any slope), `r` half-thickness.
    pub(super) fn segment(
        &mut self,
        a: Vec3,
        b: Vec3,
        r: f32,
        color: Vec3,
        material: Material,
        solid: bool,
    ) {
        let d = b - a;
        let horiz = Vec2::new(d.x, d.z);
        let len = d.length();
        if len < 1e-3 {
            return;
        }
        let mid = (a + b) * 0.5;
        self.oboxes.push(OBox {
            center: Vec2::new(mid.x, mid.z),
            // Local +z along the segment's heading, tipped to its slope.
            yaw: horiz.x.atan2(horiz.y),
            tilt: -d.y.atan2(horiz.length()),
            half: Vec2::new(r, len * 0.5),
            y0: mid.y - r,
            y1: mid.y + r,
            color,
            side_mat: material,
            top_mat: material,
            solid,
            ..OBox::default()
        });
    }

    /// A cable sagging between two points, in short straight runs.
    pub(super) fn cable(&mut self, a: Vec3, b: Vec3, sag: f32) {
        let n = 8;
        let at = |t: f32| a.lerp(b, t) - Vec3::Y * sag * 4.0 * t * (1.0 - t);
        // Long spans sing in the wind; a short drop to a roof doesn't.
        if a.distance(b) > 6.0 {
            self.sound(Ambient::Cable, at(0.5));
        }
        for k in 0..n {
            let (t0, t1) = (k as f32 / n as f32, (k + 1) as f32 / n as f32);
            self.segment(
                at(t0),
                at(t1),
                0.025,
                Vec3::new(0.08, 0.08, 0.08),
                Material::Rust,
                false,
            );
        }
    }

    /// A wooden power pole with a crossbar. Returns the two cable points on
    /// the crossbar (none if it snapped off).
    pub(super) fn pole(&mut self, rng: &mut Rng, at: Vec2, along: Vec2) -> Option<[Vec3; 2]> {
        let g = self.ground_height(at.x, at.y) - 0.3;
        let wood = Vec3::new(0.42, 0.32, 0.22) * rng.range(0.85, 1.1);
        let base = Vec3::new(at.x, g, at.y);
        if rng.chance(0.12) {
            // Snapped: a stump, and the rest lying in the sand.
            let h = rng.range(1.5, 3.0);
            self.segment(base, base + Vec3::Y * h, 0.14, wood, Material::Wood, true);
            let a = rng.range(0.0, std::f32::consts::TAU);
            let fall = Vec3::new(a.cos(), 0.0, a.sin());
            let p = base + fall * 1.0 + Vec3::Y * 0.3;
            self.segment(
                p,
                p + fall * 7.0 + Vec3::Y * -0.1,
                0.14,
                wood * 0.9,
                Material::Wood,
                false,
            );
            return None;
        }
        // Most stand; some lean.
        let lean = if rng.chance(0.25) {
            rng.range(0.08, 0.22)
        } else {
            rng.range(0.0, 0.03)
        };
        let a = rng.range(0.0, std::f32::consts::TAU);
        let tip = base + Vec3::new(a.cos() * lean, 1.0, a.sin() * lean).normalize() * 8.5;
        self.segment(base, tip, 0.14, wood, Material::Wood, true);
        let across = Vec3::new(-along.y, 0.0, along.x);
        let bar = tip - Vec3::Y * 0.5;
        self.segment(
            bar - across * 0.9,
            bar + across * 0.9,
            0.07,
            wood,
            Material::Wood,
            false,
        );
        Some([
            bar - across * 0.8 + Vec3::Y * 0.09,
            bar + across * 0.8 + Vec3::Y * 0.09,
        ])
    }

    /// An abandoned sedan: a low body with bumpers and lamps, a narrower
    /// cabin under a roof with raked, glassless windscreens, dark wheel
    /// arches and round wheels, sand banked against its flanks. Most are
    /// burned out, rusted and sitting on bare rims; the rest keep faded
    /// paint and their tires. The front faces local +z.
    pub(super) fn car(&mut self, rng: &mut Rng, at: Vec2, yaw: f32) {
        use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};
        const DUST: Vec3 = Vec3::new(0.72, 0.62, 0.45);
        let g = self.ground_height(at.x, at.y);
        let burnt = rng.chance(0.65);
        let (color, mat) = if burnt {
            (
                Vec3::new(0.2, 0.16, 0.13) * rng.range(0.8, 1.2),
                Material::Rust,
            )
        } else {
            let paints = [
                Vec3::new(0.62, 0.66, 0.68),
                Vec3::new(0.55, 0.22, 0.18),
                Vec3::new(0.3, 0.42, 0.5),
                Vec3::new(0.4, 0.5, 0.36),
            ];
            // Sun-faded and dusty.
            (
                paints[rng.index(paints.len())].lerp(DUST, rng.range(0.15, 0.3)),
                Material::Painted,
            )
        };
        let dark = Vec3::splat(0.035);
        let base = OBox {
            center: at,
            yaw,
            ..OBox::default()
        };
        let (ax, az) = base.axes();
        let world = |x: f32, z: f32| at + ax * x + az * z;
        // A burned car sits low on its bare rims.
        let (rim, sink) = if burnt { (0.27, 0.1) } else { (0.34, 0.0) };
        let lo = 0.3 - sink;
        let belt = 0.88 - sink;
        let top = 1.38 - sink;

        struct Part {
            x: f32,
            z: f32,
            half: Vec2,
            y0: f32,
            y1: f32,
            color: Vec3,
            mat: Material,
            turn: f32,
            tilt: f32,
            roll: f32,
            solid: bool,
        }
        let part = |m: &mut Map, p: Part| {
            m.oboxes.push(OBox {
                center: world(p.x, p.z),
                yaw: yaw + p.turn,
                half: p.half,
                y0: g + p.y0,
                y1: g + p.y1,
                color: p.color,
                side_mat: p.mat,
                top_mat: p.mat,
                tilt: p.tilt,
                roll: p.roll,
                solid: p.solid,
                chip: if p.solid { 0.08 } else { 0.0 },
                ..OBox::default()
            });
        };
        let flat = |x: f32, z: f32, hx: f32, hz: f32, y0: f32, y1: f32, color: Vec3, mat| Part {
            x,
            z,
            half: Vec2::new(hx, hz),
            y0,
            y1,
            color,
            mat,
            turn: 0.0,
            tilt: 0.0,
            roll: 0.0,
            solid: false,
        };

        // Body (the collider): hood, doors, trunk.
        part(
            self,
            Part {
                solid: true,
                ..flat(0.0, 0.0, 0.9, 2.2, lo, belt, color, mat)
            },
        );
        // Wheel arches: dark recesses the wheels sit in.
        for sz in [-1.0f32, 1.0] {
            part(
                self,
                flat(
                    0.0,
                    sz * 1.35,
                    0.91,
                    0.42,
                    lo,
                    lo + rim + 0.12,
                    dark,
                    Material::Rubble,
                ),
            );
        }
        // Bumpers (some lost) and the grille between the headlamps.
        let trim = if burnt {
            Vec3::splat(0.13)
        } else {
            Vec3::splat(0.42)
        };
        for sz in [-1.0f32, 1.0] {
            if rng.chance(0.15) {
                continue;
            }
            // Some hang off at one end.
            let roll = if rng.chance(0.2) {
                rng.range(-0.12, 0.12)
            } else {
                0.0
            };
            part(
                self,
                Part {
                    roll,
                    ..flat(0.0, sz * 2.26, 0.93, 0.08, lo, lo + 0.2, trim, mat)
                },
            );
        }
        part(
            self,
            flat(
                0.0,
                2.205,
                0.5,
                0.02,
                lo + 0.22,
                belt - 0.12,
                dark,
                Material::Rubble,
            ),
        );
        // Lamps: pale headlamps (empty sockets when burned), red tail lamps.
        let head = if burnt {
            dark
        } else {
            Vec3::new(0.82, 0.8, 0.7)
        };
        let tail = if burnt {
            dark
        } else {
            Vec3::new(0.45, 0.08, 0.06)
        };
        for sx in [-1.0f32, 1.0] {
            part(
                self,
                flat(
                    sx * 0.68,
                    2.21,
                    0.15,
                    0.02,
                    belt - 0.24,
                    belt - 0.1,
                    head,
                    Material::Plain,
                ),
            );
            part(
                self,
                flat(
                    sx * 0.7,
                    -2.21,
                    0.13,
                    0.02,
                    belt - 0.26,
                    belt - 0.1,
                    tail,
                    Material::Plain,
                ),
            );
        }

        // The cabin: windows long gone, a dark interior showing through
        // between the pillars.
        let (front, back) = (0.25f32, -0.95f32); // roof edges
        let (shield, rear) = (0.85f32, -1.6f32); // where the screens meet the body
        part(
            self,
            flat(
                0.0,
                (front + back) * 0.5,
                0.68,
                (front - back) * 0.5,
                belt,
                top - 0.05,
                dark,
                Material::Rubble,
            ),
        );
        let screen = |m: &mut Map, z0: f32, z1: f32| {
            // A slab from (z0, belt) up to (z1, top).
            let (run, rise) = (z1 - z0, top - belt);
            let len = (run * run + rise * rise).sqrt();
            let ym = (belt + top) * 0.5;
            part(
                m,
                Part {
                    tilt: -(rise / run).atan(),
                    ..flat(
                        0.0,
                        (z0 + z1) * 0.5,
                        0.66,
                        len * 0.5,
                        ym - 0.03,
                        ym + 0.03,
                        dark,
                        Material::Rubble,
                    )
                },
            );
        };
        screen(self, shield, front);
        screen(self, rear, back);
        // Roof, and the pillars holding it up.
        let roof = color * 0.92;
        part(
            self,
            flat(
                0.0,
                (front + back) * 0.5,
                0.72,
                (front - back) * 0.5 + 0.04,
                top - 0.06,
                top + 0.02,
                roof,
                mat,
            ),
        );
        for sx in [-1.0f32, 1.0] {
            let pillar = |m: &mut Map, z0: f32, z1: f32, r: f32| {
                let a = world(sx * 0.69, z0);
                let b = world(sx * 0.69, z1);
                m.segment(
                    Vec3::new(a.x, g + belt, a.y),
                    Vec3::new(b.x, g + top - 0.03, b.y),
                    r,
                    roof,
                    mat,
                    false,
                );
            };
            pillar(self, shield, front, 0.05);
            pillar(self, rear, back, 0.09);
            part(
                self,
                flat(sx * 0.69, -0.38, 0.035, 0.06, belt, top - 0.03, roof, mat),
            );
        }

        // Wheels: four bars rolled 45° apart make an octagon, axle along
        // local x.
        let (tire, tire_mat) = if burnt {
            (Vec3::new(0.24, 0.17, 0.13), Material::Rust)
        } else {
            (Vec3::splat(0.07), Material::Rubble)
        };
        let hub = if burnt {
            Vec3::new(0.3, 0.22, 0.16)
        } else {
            Vec3::splat(0.5)
        };
        for (sx, sz) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
            let wheel = |m: &mut Map, x: f32, r: f32, w: f32, c: Vec3, mt: Material| {
                let k = r * (FRAC_PI_4 * 0.5).tan();
                for roll in [0.0, FRAC_PI_4, FRAC_PI_2, 3.0 * FRAC_PI_4] {
                    part(
                        m,
                        Part {
                            half: Vec2::new(r, w),
                            turn: FRAC_PI_2,
                            roll,
                            ..flat(x, sz * 1.35, 0.0, 0.0, rim - k, rim + k, c, mt)
                        },
                    );
                }
            };
            wheel(self, sx * 0.82, rim, 0.12, tire, tire_mat);
            wheel(self, sx * 0.95, rim * 0.55, 0.015, hub, Material::Painted);
        }

        // Sand blown up against one or both flanks.
        for sx in [-1.0f32, 1.0] {
            if !rng.chance(0.6) {
                continue;
            }
            let h = rng.range(0.15, 0.35);
            part(
                self,
                Part {
                    roll: -sx * rng.range(0.28, 0.4),
                    ..flat(
                        sx * 1.2,
                        rng.range(-0.6, 0.6),
                        0.8,
                        rng.range(1.2, 1.9),
                        -0.5,
                        h,
                        DUST * rng.range(0.95, 1.05),
                        Material::Sand,
                    )
                },
            );
        }
    }

    /// A dead palm: a curved trunk and a few dry fronds hanging off the top.
    /// Some have fallen over.
    pub(super) fn palm(&mut self, rng: &mut Rng, at: Vec2) {
        let g = self.ground_height(at.x, at.y) - 0.3;
        let trunk = Vec3::new(0.4, 0.33, 0.25) * rng.range(0.85, 1.1);
        let frond = Vec3::new(0.46, 0.38, 0.24) * rng.range(0.85, 1.1);
        let a = rng.range(0.0, std::f32::consts::TAU);
        let bend = Vec3::new(a.cos(), 0.0, a.sin());
        let fallen = rng.chance(0.25);
        let h = rng.range(6.0, 9.0);
        let mut p = Vec3::new(at.x, g, at.y);
        let n = 6;
        for k in 0..n {
            let t = (k + 1) as f32 / n as f32;
            let q = if fallen {
                Vec3::new(at.x, g + 0.35, at.y) + bend * h * t
            } else {
                Vec3::new(at.x, g, at.y) + Vec3::Y * h * t + bend * (t * t) * h * 0.18
            };
            self.segment(
                p,
                q,
                0.2 - 0.04 * t,
                trunk,
                Material::Wood,
                k < 3 && !fallen,
            );
            p = q;
        }
        // Dry fronds drooping from the crown (or splayed on the sand).
        let fronds = rng.int(4, 7);
        for k in 0..fronds {
            let b = k as f32 / fronds as f32 * std::f32::consts::TAU + rng.range(-0.3, 0.3);
            let out = Vec3::new(b.cos(), 0.0, b.sin());
            let len = rng.range(2.2, 3.4);
            let droop = if fallen { 0.0 } else { rng.range(0.6, 1.4) };
            let end = p + out * len - Vec3::Y * droop * len * 0.6;
            self.segment(p, end, 0.12, frond, Material::Wood, false);
        }
    }

    /// A market stall: four posts, a cloth roof sagging toward the front, a
    /// counter and a few crates. `yaw`: the front faces local +z.
    pub(super) fn stall(&mut self, rng: &mut Rng, at: Vec2, yaw: f32) {
        let g = self.ground_height(at.x, at.y);
        self.sound(Ambient::Cloth, Vec3::new(at.x, g + 2.2, at.y));
        let wood = Vec3::new(0.45, 0.35, 0.24) * rng.range(0.85, 1.1);
        let cloths = [
            Vec3::new(0.66, 0.24, 0.18), // faded red
            Vec3::new(0.26, 0.4, 0.55),  // faded blue
            Vec3::new(0.66, 0.56, 0.3),  // mustard
            Vec3::new(0.32, 0.46, 0.34), // faded green
            Vec3::new(0.78, 0.74, 0.64), // bleached white
        ];
        let cloth = cloths[rng.index(cloths.len())];
        let base = OBox {
            center: at,
            yaw,
            ..OBox::default()
        };
        let (ax, az) = base.axes();
        let w = |x: f32, z: f32, y: f32| {
            Vec3::new(at.x, g + y, at.y)
                + Vec3::new(ax.x, 0.0, ax.y) * x
                + Vec3::new(az.x, 0.0, az.y) * z
        };
        let (hx, hz) = (rng.range(1.3, 1.8), rng.range(1.0, 1.3));
        for (sx, sz, h) in [
            (-1.0f32, -1.0f32, 2.6f32),
            (1.0, -1.0, 2.6),
            (-1.0, 1.0, 2.2),
            (1.0, 1.0, 2.2),
        ] {
            self.segment(
                w(sx * hx, sz * hz, 0.0),
                w(sx * hx, sz * hz, h),
                0.06,
                wood,
                Material::Wood,
                false,
            );
        }
        // Cloth roof: sloped from back to front, sometimes torn half away.
        let torn = rng.chance(0.3);
        let span = if torn { 0.5 } else { 1.0 };
        let (x0, x1) = (-hx - 0.2, -hx + 2.0 * hx * span + 0.2);
        {
            let (a, b) = (
                w((x0 + x1) * 0.5, -hz - 0.15, 2.65),
                w((x0 + x1) * 0.5, hz + 0.35, 2.05),
            );
            let d = b - a;
            let mid = (a + b) * 0.5;
            let horiz = Vec2::new(d.x, d.z);
            self.oboxes.push(OBox {
                center: Vec2::new(mid.x, mid.z),
                yaw: horiz.x.atan2(horiz.y),
                tilt: -d.y.atan2(horiz.length()),
                half: Vec2::new((x1 - x0) * 0.5, d.length() * 0.5),
                y0: mid.y - 0.02,
                y1: mid.y + 0.02,
                color: cloth,
                side_mat: Material::Cloth,
                top_mat: Material::Cloth,
                solid: false,
                ..OBox::default()
            });
        }
        // Counter across the front.
        let c = w(0.0, hz - 0.2, 0.0);
        self.oboxes.push(OBox {
            center: Vec2::new(c.x, c.z),
            yaw,
            half: Vec2::new(hx - 0.1, 0.3),
            y0: g,
            y1: g + 0.95,
            color: wood * 0.9,
            side_mat: Material::Wood,
            top_mat: Material::Wood,
            chip: 0.08,
            solid: false,
            ..OBox::default()
        });
        // Crates stacked beside it.
        for k in 0..rng.int(1, 4) {
            let p = w(hx + 0.7 + rng.range(-0.2, 0.4), rng.range(-hz, hz), 0.0);
            let s = rng.range(0.3, 0.45);
            let y = if k == 3 { 2.0 * s } else { 0.0 };
            self.oboxes.push(OBox {
                center: Vec2::new(p.x, p.z),
                yaw: yaw + rng.range(-0.4, 0.4),
                half: Vec2::splat(s),
                y0: g + y,
                y1: g + y + 2.0 * s,
                color: wood * rng.range(0.8, 1.05),
                side_mat: Material::Wood,
                top_mat: Material::Wood,
                chip: 0.05,
                solid: false,
                ..OBox::default()
            });
        }
    }
}

impl Map {
    /// Rooftop clutter on an intact patch of flat roof at `at` (the roof's
    /// top surface), `room` meters clear around it: water tanks, a satellite
    /// dish, AC units, a solar water heater, a washing line. Visual only.
    pub(super) fn roof_clutter(&mut self, rng: &mut Rng, at: Vec3, yaw: f32, room: f32) {
        use super::landmarks::Rig;
        use glam::Quat;
        let rig = Rig {
            pos: at,
            rot: Quat::from_rotation_y(yaw + rng.range(-0.2, 0.2)),
        };
        let id = Quat::IDENTITY;
        let steel = Vec3::new(0.42, 0.4, 0.37);
        let up = rig.rot * Vec3::Y;
        // Only what fits on this slab (`room`: clear half-width around `at`).
        let fits = [0.8, 0.65, 0.75, 1.0, 1.0];
        let options: Vec<usize> = (0..5).filter(|&k| fits[k] <= room).collect();
        // Water tanks and dishes most of all: they're what shows against
        // the sky from the street.
        let weight = [4.0f32, 2.5, 1.0, 1.5, 1.0];
        let total: f32 = options.iter().map(|&k| weight[k]).sum();
        if total <= 0.0 {
            return;
        }
        let mut pick = rng.range(0.0, total);
        let mut kind = options[0];
        for &k in &options {
            if pick < weight[k] {
                kind = k;
                break;
            }
            pick -= weight[k];
        }
        match kind {
            0 => {
                // Water tanks on a steel stand: faded black, white or blue.
                let tanks = [
                    Vec3::splat(0.13),
                    Vec3::new(0.78, 0.76, 0.7),
                    Vec3::new(0.26, 0.38, 0.5),
                ];
                let color = tanks[rng.index(tanks.len())];
                let n = if room > 2.4 { rng.int(1, 3) } else { 1 };
                for k in 0..n {
                    let off = Vec3::new((k as f32 - (n - 1) as f32 * 0.5) * 1.9, 0.0, 0.0);
                    for (sx, sz) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                        let leg = rig.point(off + Vec3::new(sx * 0.6, 0.0, sz * 0.6));
                        self.segment(leg, leg + up * 1.0, 0.04, steel, Material::Painted, false);
                    }
                    rig.part(
                        self,
                        off + Vec3::new(0.0, 1.03, 0.0),
                        Vec3::new(0.75, 0.03, 0.75),
                        id,
                        steel,
                        Material::Painted,
                        false,
                        0.0,
                    );
                    let base = rig.point(off + Vec3::new(0.0, 1.06, 0.0));
                    let h = rng.range(1.2, 1.6);
                    self.pipe(base, base + up * h, 0.7, color, Material::Painted, false);
                    self.disc(
                        base + up * (h - 0.02),
                        up * 0.03,
                        0.66,
                        color * 0.95,
                        Material::Painted,
                    );
                }
            }
            1 => {
                // Satellite dish on a short post, aimed at the southern sky.
                let foot = rig.point(Vec3::ZERO);
                self.segment(foot, foot + up * 1.1, 0.04, steel, Material::Painted, false);
                let aim = (Vec3::new(0.0, 0.75, -0.66)).normalize();
                let c = foot + up * 1.25;
                self.disc(
                    c,
                    aim * 0.03,
                    0.5,
                    Vec3::new(0.8, 0.79, 0.75),
                    Material::Painted,
                );
                self.segment(c, c + aim * 0.55, 0.02, steel, Material::Painted, false);
            }
            2 => {
                // A row of AC units with dark grilles, a drip stain below.
                let n = ((room / 0.55) as i32).clamp(1, 3);
                for k in 0..rng.int(1, n + 1) {
                    let x = (k as f32 - 1.0) * 0.5 * (n > 1) as i32 as f32;
                    rig.part(
                        self,
                        Vec3::new(x, 0.32, 0.0),
                        Vec3::new(0.45, 0.32, 0.3),
                        id,
                        Vec3::new(0.74, 0.72, 0.66) * rng.range(0.85, 1.0),
                        Material::Painted,
                        false,
                        0.03,
                    );
                    rig.part(
                        self,
                        Vec3::new(x, 0.32, 0.31),
                        Vec3::new(0.3, 0.24, 0.02),
                        id,
                        Vec3::splat(0.08),
                        Material::Rust,
                        false,
                        0.0,
                    );
                }
            }
            3 => {
                // Solar water heater: a dark panel tilted on a frame, its
                // tank along the top edge.
                let tilt = Quat::from_rotation_x(-0.6);
                rig.part(
                    self,
                    Vec3::new(0.0, 0.7, 0.0),
                    Vec3::new(0.95, 0.03, 0.75),
                    tilt,
                    Vec3::new(0.08, 0.1, 0.14),
                    Material::Painted,
                    false,
                    0.0,
                );
                for sx in [-0.85f32, 0.85] {
                    let back = rig.point(Vec3::new(sx, 0.0, 0.55));
                    self.segment(
                        back,
                        back + up * 1.15,
                        0.03,
                        steel,
                        Material::Painted,
                        false,
                    );
                }
                let (a, b) = (
                    rig.point(Vec3::new(-1.0, 1.35, 0.6)),
                    rig.point(Vec3::new(1.0, 1.35, 0.6)),
                );
                self.pipe(
                    a,
                    b,
                    0.25,
                    Vec3::new(0.7, 0.69, 0.64),
                    Material::Painted,
                    false,
                );
            }
            _ => {
                // Washing line between two T posts, a few things pegged out.
                let span = (room * 1.8).clamp(2.0, 4.0);
                let mut ends = Vec::new();
                for sx in [-0.5f32, 0.5] {
                    let p = rig.point(Vec3::new(sx * span, 0.0, 0.0));
                    self.segment(p, p + up * 1.7, 0.03, steel, Material::Painted, false);
                    ends.push(p + up * 1.7);
                }
                self.cable(ends[0], ends[1], 0.12);
                let cloths = [
                    Vec3::new(0.66, 0.24, 0.18),
                    Vec3::new(0.26, 0.4, 0.55),
                    Vec3::new(0.82, 0.8, 0.72),
                    Vec3::new(0.66, 0.56, 0.3),
                ];
                for k in 0..rng.int(2, 5) {
                    let t = 0.15 + 0.7 * k as f32 / 4.0;
                    let x = (t - 0.5) * span;
                    let sag = 0.12 * 4.0 * t * (1.0 - t);
                    let (w, h) = (rng.range(0.25, 0.45), rng.range(0.4, 0.7));
                    rig.part(
                        self,
                        Vec3::new(x, 1.7 - sag - h * 0.5, 0.0),
                        Vec3::new(w, h * 0.5, 0.01),
                        Quat::from_rotation_x(rng.range(-0.15, 0.15)),
                        cloths[rng.index(cloths.len())],
                        Material::Cloth,
                        false,
                        0.0,
                    );
                }
            }
        }
    }
}
