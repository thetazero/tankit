//! The lie of the land under a city: one height for every point, in the
//! arena and on out to the horizon. A few big shapes (a tilt to the whole
//! place, a ridge or a valley across it, hills), then level ground wherever
//! something needs it, at whatever height the land has there: a plaza, a
//! court, a building's plot.

use std::f32::consts::TAU;

use glam::Vec2;

use super::skeleton::smoothstep;
use crate::map::OBox;
use crate::rng::Rng;

#[derive(Clone, Copy, Debug)]
enum Shape {
    /// Rising `grade` m per m along `dir`, from `from`, leveling off far out.
    Tilt {
        from: Vec2,
        dir: Vec2,
        grade: f32,
    },
    Cut(Cut),
    Hill {
        at: Vec2,
        height: f32,
        reach: f32,
    },
}

/// A valley or a ridge, right across the city: level along the middle,
/// `floor` m to either side of a line, then a bank.
#[derive(Clone, Copy, Debug)]
pub struct Cut {
    pub at: Vec2,
    pub dir: Vec2,
    /// How high (a valley's is negative).
    pub height: f32,
    pub floor: f32,
    bank: f32,
    sway: f32,
    phase: f32,
}

impl Cut {
    /// How far its line is off its straight course, `t` along it.
    fn wander(&self, t: f32) -> f32 {
        self.sway * (t / 85.0 + self.phase).sin()
    }

    /// Along it, and off its line.
    pub fn local(&self, p: Vec2) -> (f32, f32) {
        let q = p - self.at;
        let t = q.dot(self.dir);
        (t, self.dir.perp_dot(q) - self.wander(t))
    }

    /// Its line, `t` along.
    pub fn point(&self, t: f32) -> Vec2 {
        let side = Vec2::new(-self.dir.y, self.dir.x);
        self.at + self.dir * t + side * self.wander(t)
    }

    /// How wide the bank is there: steep here, easy there.
    pub fn bank(&self, t: f32) -> f32 {
        let u = 0.5 + 0.5 * (t / 47.0 + self.phase * 2.3).sin();
        self.bank * (1.0 + 1.7 * u * u)
    }

    /// 1 along the middle, 0 past the bank.
    pub fn depth(&self, p: Vec2) -> f32 {
        let (t, off) = self.local(p);
        1.0 - smoothstep(self.floor, self.floor + self.bank(t), off.abs())
    }
}

impl Shape {
    fn height(&self, p: Vec2) -> f32 {
        let bell = |u: f32| (1.0 - (u * u).min(1.0)).powi(2);
        match *self {
            Shape::Tilt { from, dir, grade } => {
                let t = (p - from).dot(dir);
                let far = 340.0;
                grade * far * (t / far).tanh()
            }
            Shape::Cut(c) => c.height * c.depth(p),
            Shape::Hill { at, height, reach } => height * bell(p.distance(at) / reach),
        }
    }
}

/// Level ground: `half` across round `at` (a round one's corners are all
/// `round`), blending into the land over `blend` m.
#[derive(Clone, Copy, Debug)]
struct Level {
    b: OBox,
    round: f32,
    blend: f32,
    height: f32,
}

#[derive(Clone, Debug, Default)]
pub struct Relief {
    shapes: Vec<Shape>,
    levels: Vec<Level>,
}

impl Relief {
    /// The lie of a city `size` across: up to two valleys or ridges right
    /// across it, kept off `avoid` (places, and how far round them), and
    /// hills and a tilt, low where those are.
    pub fn new(rng: &mut Rng, size: f32, avoid: &[(Vec2, f32)]) -> Relief {
        let mid = Vec2::splat(size * 0.5);
        let any = |rng: &mut Rng| Vec2::from_angle(rng.range(0.0, TAU));
        let want = match rng.f32() {
            x if x < 0.22 => 0,
            x if x < 0.72 => 1,
            _ => 2,
        };
        let mut cuts: Vec<Cut> = Vec::new();
        let mut tries = 0;
        while cuts.len() < want && tries < 300 {
            tries += 1;
            let up = rng.chance(0.45);
            let h = rng.range(7.5, 13.0);
            let c = Cut {
                at: mid + any(rng) * size * rng.range(0.0, 0.36),
                dir: any(rng),
                height: if up { h } else { -h },
                floor: if up {
                    rng.range(7.0, 13.0)
                } else {
                    rng.range(12.0, 20.0)
                },
                bank: h * rng.range(1.1, 1.7),
                sway: rng.range(4.0, 16.0),
                phase: rng.range(0.0, TAU),
            };
            let reach = c.floor + c.bank * 1.4;
            let ease = tries as f32 / 300.0 * 14.0;
            let clear = avoid
                .iter()
                .all(|&(p, r)| c.local(p).1.abs() > reach + r - ease)
                // Two run much the same way, well apart, or cross squarely.
                && cuts.iter().all(|o| {
                    let turn = o.dir.perp_dot(c.dir).abs();
                    turn > 0.75 || (turn < 0.35 && o.local(c.at).1.abs() > 110.0)
                });
            if clear {
                cuts.push(c);
            }
        }
        let mut shapes: Vec<Shape> = Vec::new();
        if rng.chance(0.4) {
            let grade = rng.range(0.015, 0.045);
            shapes.push(Shape::Tilt {
                from: mid,
                dir: any(rng),
                grade,
            });
        }
        let hills = if cuts.is_empty() {
            rng.int(1, 4)
        } else {
            rng.int(0, 3)
        };
        for _ in 0..hills {
            let at = Vec2::new(rng.range(-0.15, 1.15), rng.range(-0.15, 1.15)) * size;
            let reach = rng.range(40.0, 100.0);
            // Not where it would fill a valley in or bury a ridge.
            if cuts
                .iter()
                .any(|c| c.local(at).1.abs() < reach * 0.6 + c.floor + c.bank)
            {
                continue;
            }
            shapes.push(Shape::Hill {
                at,
                height: reach * rng.range(0.07, 0.18),
                reach,
            });
        }
        shapes.extend(cuts.into_iter().map(Shape::Cut));
        Relief {
            shapes,
            levels: Vec::new(),
        }
    }

    pub fn cuts(&self) -> Vec<Cut> {
        self.shapes
            .iter()
            .filter_map(|s| match s {
                Shape::Cut(c) => Some(*c),
                _ => None,
            })
            .collect()
    }

    /// How steep the land is, before any of it's leveled.
    pub fn steep(&self, p: Vec2) -> f32 {
        let e = 2.0;
        let d = |v: Vec2| (self.land(p + v) - self.land(p - v)) / (2.0 * e);
        Vec2::new(d(Vec2::X * e), d(Vec2::Y * e)).length()
    }

    /// Levels `b`'s ground and `margin` m round it, at `height`.
    pub fn level_at(&mut self, b: OBox, margin: f32, blend: f32, height: f32) {
        self.levels.push(Level {
            b,
            round: margin,
            blend,
            height,
        });
    }

    /// What shapes it's made of, for the probes.
    #[cfg(test)]
    pub fn describe(&self) -> String {
        let of = |s: &Shape| match s {
            Shape::Tilt { grade, .. } => format!("tilt {grade:.3}"),
            Shape::Cut(c) => format!("cut {:.1} m, floor {:.0} at {:.0}", c.height, c.floor, c.at),
            Shape::Hill { at, height, reach } => {
                format!("hill {height:.1} m over {reach:.0} m at {at:.0}")
            }
        };
        self.shapes.iter().map(of).collect::<Vec<_>>().join("; ")
    }

    /// The land, before any of it's leveled.
    pub fn land(&self, p: Vec2) -> f32 {
        self.shapes.iter().map(|s| s.height(p)).sum()
    }

    pub fn height(&self, p: Vec2) -> f32 {
        let mut h = self.land(p);
        for l in &self.levels {
            let reach = l.b.half.length() + l.round + l.blend;
            if l.b.center.distance_squared(p) > reach * reach {
                continue;
            }
            let d = l.b.distance(p) - l.round;
            h += (l.height - h) * (1.0 - smoothstep(0.0, l.blend, d));
        }
        h
    }

    /// Levels round ground, `r` across, at the height it has.
    pub fn level_round(&mut self, at: Vec2, r: f32, blend: f32) {
        let b = OBox {
            center: at,
            half: Vec2::splat(0.01),
            ..OBox::default()
        };
        self.push(b, r, blend);
    }

    /// Levels `b`'s ground and `margin` m round it, at the height it has.
    pub fn level_box(&mut self, b: &OBox, margin: f32, blend: f32) {
        self.push(*b, margin, blend);
    }

    fn push(&mut self, b: OBox, round: f32, blend: f32) {
        if self.shapes.is_empty() {
            return;
        }
        let height = self.height(b.center);
        self.levels.push(Level {
            b,
            round,
            blend,
            height,
        });
    }
}
