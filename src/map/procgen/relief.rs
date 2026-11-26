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
    /// Along a line through `at`, `dir`: `height` (a valley's is negative)
    /// at the middle, gone `reach` m to either side, its line swaying.
    Ridge {
        at: Vec2,
        dir: Vec2,
        height: f32,
        reach: f32,
        sway: f32,
    },
    Hill {
        at: Vec2,
        height: f32,
        reach: f32,
    },
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
            Shape::Ridge {
                at,
                dir,
                height,
                reach,
                sway,
            } => {
                let q = p - at;
                let t = q.dot(dir);
                let off = q.perp_dot(dir) + (t / 90.0).sin() * sway;
                height * bell(off / reach) * (1.0 - 0.35 * (t / 130.0).sin().abs())
            }
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
    /// The lie of a city `size` across: often some, now and then none.
    pub fn new(rng: &mut Rng, size: f32) -> Relief {
        let mut shapes = Vec::new();
        let mid = Vec2::splat(size * 0.5);
        let any = |rng: &mut Rng| Vec2::from_angle(rng.range(0.0, TAU));
        if rng.chance(0.15) {
            return Relief::default();
        }
        if rng.chance(0.45) {
            let dir = any(rng);
            let grade = rng.range(0.02, 0.06);
            shapes.push(Shape::Tilt {
                from: mid,
                dir,
                grade,
            });
        }
        if rng.chance(0.5) {
            let at = mid + any(rng) * size * rng.range(0.0, 0.3);
            let up = if rng.chance(0.5) { 1.0 } else { -1.0 };
            let reach = rng.range(45.0, 90.0);
            shapes.push(Shape::Ridge {
                at,
                dir: any(rng),
                height: up * reach * rng.range(0.1, 0.22),
                reach,
                sway: rng.range(5.0, 22.0),
            });
        }
        for _ in 0..rng.int(1, 4) {
            // Anywhere, over the edge and past it too.
            let at = Vec2::new(rng.range(-0.15, 1.15), rng.range(-0.15, 1.15)) * size;
            let reach = rng.range(40.0, 110.0);
            shapes.push(Shape::Hill {
                at,
                height: reach * rng.range(0.07, 0.2),
                reach,
            });
        }
        Relief {
            shapes,
            levels: Vec::new(),
        }
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
