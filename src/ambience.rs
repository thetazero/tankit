//! Ambient motion: the world moving on its own. Visual only, stepped with the
//! match so it runs the same in play and in screenshots.
//!
//! - Wind-blown sand skittering low across the ground (maps with blowing sand).
//! - Slow, faint dust drifting through the air.
//! - Sand trickling off broken wall tops, in on-and-off bursts.
//! - Dust kicked up behind tanks' tracks, by speed.

use glam::{Vec2, Vec3};

use crate::game::DT;
use crate::map::Map;
use crate::render::Particle;
use crate::rng::Rng;
use crate::sim::Tank;

/// How far from the focus (the camera) ambient particles live.
const RADIUS: f32 = 55.0;
const MAX_PARTICLES: usize = 2500;

#[derive(Clone, Copy)]
struct P {
    pos: Vec3,
    vel: Vec3,
    age: f32,
    life: f32,
    size0: f32,
    size1: f32,
    alpha: f32,
    color: Vec3,
    /// Fraction of gravity applied.
    gravity: f32,
    /// How fast it slows down (air drag).
    drag: f32,
    /// Drawn this many times longer than wide, along its motion (0 = round).
    stretch: f32,
    /// > 0: skims the ground at this height, following the terrain.
    hover: f32,
}

/// A broken top sand can trickle off: a point on the edge and the outward direction.
#[derive(Clone, Copy)]
struct Ledge {
    at: Vec3,
    out: Vec3,
    /// Along the edge.
    along: Vec3,
    len: f32,
}

pub struct Ambience {
    particles: Vec<P>,
    ledges: Vec<Ledge>,
    /// Ledges currently trickling and for how much longer.
    active: Vec<(usize, f32)>,
    rng: Rng,
    /// Where the camera is: ambient effects happen around it.
    pub focus: Vec3,
    wind: Vec2,
    blowing: f32,
    dust: Vec3,
    sand_budget: f32,
    mote_budget: f32,
}

impl Ambience {
    pub fn new(map: &Map) -> Ambience {
        let a = &map.atmosphere;
        let ledges = map
            .oboxes
            .iter()
            .filter(|o| o.broken && o.solid && o.y1 > 2.0 && o.half.x.max(o.half.y) > 0.4)
            .map(|o| {
                let (ax, az) = o.axes();
                // Sand spills off the long side.
                let (along, out, len) = if o.half.x >= o.half.y {
                    (ax, az, o.half.x)
                } else {
                    (az, ax, o.half.y)
                };
                let short = o.half.x.min(o.half.y);
                Ledge {
                    at: Vec3::new(
                        o.center.x + out.x * short,
                        o.y1 - 0.25,
                        o.center.y + out.y * short,
                    ),
                    out: Vec3::new(out.x, 0.0, out.y),
                    along: Vec3::new(along.x, 0.0, along.y),
                    len,
                }
            })
            .collect();
        Ambience {
            particles: Vec::new(),
            ledges,
            active: Vec::new(),
            rng: Rng::new(0xa3b1_e7c5),
            focus: Vec3::ZERO,
            wind: a.wind,
            blowing: a.blowing_sand,
            dust: a.dust,
            sand_budget: 0.0,
            mote_budget: 0.0,
        }
    }

    fn push(&mut self, p: P) {
        if self.particles.len() < MAX_PARTICLES {
            self.particles.push(p);
        }
    }

    /// One fixed step. `tanks` with whether each is alive.
    pub fn step(&mut self, map: &Map, time: f32, tanks: &[(&Tank, bool)]) {
        let wind3 = Vec3::new(self.wind.x, 0.0, self.wind.y);
        let focus2 = Vec2::new(self.focus.x, self.focus.z);

        // Wind-blown sand: low, fast streamers.
        if self.blowing > 0.0 {
            self.sand_budget += 900.0 * self.blowing * DT;
            while self.sand_budget >= 1.0 {
                self.sand_budget -= 1.0;
                let r = RADIUS * self.rng.f32().sqrt();
                let a = self.rng.range(0.0, std::f32::consts::TAU);
                let p2 = focus2 + Vec2::new(a.cos(), a.sin()) * r;
                // Gusts: sand moves in patches that sweep downwind.
                let gust = gust(p2, self.wind, time);
                if self.rng.f32() > gust {
                    continue;
                }
                let g = map.ground_height(p2.x, p2.y);
                let speed = self.rng.range(0.8, 1.4);
                // Lighter than the ground: grains in the air catch the sun.
                let sand = self.dust * self.rng.range(1.3, 1.6);
                let p = P {
                    pos: Vec3::new(p2.x, g, p2.y),
                    vel: wind3 * speed,
                    age: 0.0,
                    life: self.rng.range(0.8, 1.6),
                    size0: self.rng.range(0.06, 0.12),
                    size1: self.rng.range(0.1, 0.18),
                    alpha: self.rng.range(0.5, 0.85),
                    color: sand,
                    gravity: 0.0,
                    drag: 0.0,
                    stretch: self.rng.range(8.0, 16.0),
                    hover: self.rng.range(0.08, 0.45),
                };
                self.push(p);
            }
        }

        // Dust drifting through the air: big, slow, barely there.
        self.mote_budget += (6.0 + 14.0 * self.blowing) * DT;
        while self.mote_budget >= 1.0 {
            self.mote_budget -= 1.0;
            let r = RADIUS * self.rng.f32().sqrt();
            let a = self.rng.range(0.0, std::f32::consts::TAU);
            let p2 = focus2 + Vec2::new(a.cos(), a.sin()) * r;
            let g = map.ground_height(p2.x, p2.y);
            let p = P {
                pos: Vec3::new(p2.x, g + self.rng.range(0.5, 8.0), p2.y),
                vel: wind3 * self.rng.range(0.3, 0.6) + Vec3::Y * self.rng.range(-0.1, 0.2),
                age: 0.0,
                life: self.rng.range(5.0, 9.0),
                size0: self.rng.range(2.0, 4.0),
                size1: self.rng.range(4.0, 8.0),
                alpha: self.rng.range(0.03, 0.06),
                color: self.dust,
                gravity: 0.0,
                drag: 0.0,
                stretch: 0.0,
                hover: 0.0,
            };
            self.push(p);
        }

        // Sand trickling off broken tops near the camera, in bursts.
        self.active.retain_mut(|(_, t)| {
            *t -= DT;
            *t > 0.0
        });
        if self.blowing > 0.0
            && self.active.len() < 10
            && self.rng.chance(2.0 * DT)
            && !self.ledges.is_empty()
        {
            for _ in 0..12 {
                let i = self.rng.index(self.ledges.len());
                let l = self.ledges[i];
                if l.at.distance(self.focus) < RADIUS * 0.8
                    && !self.active.iter().any(|&(j, _)| j == i)
                {
                    let dur = self.rng.range(2.0, 6.0);
                    self.active.push((i, dur));
                    break;
                }
            }
        }
        for k in 0..self.active.len() {
            let l = self.ledges[self.active[k].0];
            for _ in 0..2 {
                let at = l.at + l.along * self.rng.range(-0.3, 0.3) * l.len + l.out * 0.05;
                let p = P {
                    pos: at,
                    vel: l.out * self.rng.range(0.1, 0.4) + wind3 * 0.05,
                    age: 0.0,
                    life: 1.6,
                    size0: self.rng.range(0.04, 0.07),
                    size1: 0.1,
                    alpha: 0.7,
                    color: self.dust * 1.05,
                    gravity: 0.6,
                    drag: 0.4,
                    stretch: 3.0,
                    hover: 0.0,
                };
                self.push(p);
            }
        }

        // Tracks throw up dust when tanks move.
        for &(t, alive) in tanks {
            let speed = t.speed().abs();
            if !alive || speed < 1.0 || t.wheels_down == 0 {
                continue;
            }
            let n = (speed * 0.5 * DT * 6.0).min(1.0);
            if !self.rng.chance(n * 3.0) {
                continue;
            }
            let c = t.chassis;
            for side in [-1.0f32, 1.0] {
                let local = Vec3::new(side * c.track_x, 0.25, c.rear.0 - 0.2);
                let at = t.pos + t.rot * local;
                let back = -(t.rot * Vec3::Z);
                let p = P {
                    pos: at,
                    vel: back * speed * 0.15 + Vec3::Y * self.rng.range(0.6, 1.4) + wind3 * 0.2,
                    age: 0.0,
                    life: self.rng.range(1.0, 1.8),
                    size0: self.rng.range(0.6, 1.0),
                    size1: self.rng.range(2.4, 3.6),
                    alpha: (0.22 + speed * 0.012).min(0.4),
                    color: self.dust * self.rng.range(0.9, 1.05),
                    gravity: -0.05,
                    drag: 1.2,
                    stretch: 0.0,
                    hover: 0.0,
                };
                self.push(p);
            }
        }

        // Move and age everything; drop what's dead or wandered off.
        let focus = self.focus;
        self.particles.retain_mut(|p| {
            p.age += DT;
            p.vel.y -= 9.8 * p.gravity * DT;
            p.vel *= 1.0 - p.drag * DT;
            p.pos += p.vel * DT;
            if p.hover > 0.0 {
                p.pos.y = map.ground_height(p.pos.x, p.pos.z) + p.hover;
            }
            p.age < p.life && p.pos.distance(focus) < RADIUS * 1.6
        });
    }

    /// What to draw this frame.
    pub fn particles(&self) -> Vec<Particle> {
        self.particles
            .iter()
            .map(|p| {
                let t = p.age / p.life;
                // Fade in fast, out slow.
                let fade = (t * 8.0).min(1.0) * (1.0 - t).powf(1.5);
                let dir = p.vel.normalize_or_zero();
                Particle {
                    pos: p.pos.to_array(),
                    size: p.size0 + (p.size1 - p.size0) * t,
                    color: p.color.extend(p.alpha * fade).to_array(),
                    stretch: (dir * p.stretch).to_array(),
                    glow: 0.0,
                    billow: 0.0,
                }
            })
            .collect()
    }

    #[cfg(test)]
    fn count(&self) -> usize {
        self.particles.len()
    }
}

/// 0..1: how gusty the wind is here, now. Patches sweep downwind.
fn gust(p: Vec2, wind: Vec2, time: f32) -> f32 {
    let q = (p - wind * time) * 0.03;
    let n = (q.x * 1.7).sin() * (q.y * 1.3 + 0.5).cos() + (q.x * 0.6 + q.y * 0.9).sin() * 0.5;
    (0.35 + n * 0.45).clamp(0.05, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::Map;

    #[test]
    fn desert_has_blowing_sand_and_trickles() {
        let map = Map::reservoir(0);
        let mut a = Ambience::new(&map);
        assert!(!a.ledges.is_empty(), "no broken ledges to trickle sand off");
        a.focus = Vec3::new(70.0, 2.0, 120.0);
        for _ in 0..(4.0 / DT) as usize {
            a.step(&map, 0.0, &[]);
        }
        let n = a.count();
        assert!(n > 100 && n <= MAX_PARTICLES, "{n} particles");
        assert!(
            a.particles()
                .iter()
                .all(|p| p.color[3] >= 0.0 && p.color[3] <= 1.0 && p.size > 0.0)
        );
    }
}
