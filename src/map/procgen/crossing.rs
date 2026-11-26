//! Where a road meets a valley or a ridge: a bridge over the one, its deck
//! level from rim to rim, the valley's floor open underneath; a tunnel
//! through the other, cut down to the land either side and roofed level
//! with the crest. Two floors to fight on either way (the bots take a flat
//! slab for a floor, and the ground under it for another).

use glam::{Vec2, Vec3};

use super::civic::Stone;
use crate::map::{Map, OBox};
use crate::render::material::Material;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Bridge,
    Tunnel,
}

#[derive(Clone, Copy, Debug)]
pub struct Crossing {
    pub kind: Kind,
    pub from: Vec2,
    pub to: Vec2,
    /// Half the road's width.
    pub half: f32,
    /// The deck's (or the tunnel's roof's) top.
    pub top: f32,
    /// The tunnel's floor.
    pub floor: f32,
    /// The roofed stretch of a tunnel, along it.
    pub roofed: (f32, f32),
}

/// How thick a tunnel's walls are: the ground it's cut down into slopes
/// back up inside them, out of sight (it's on a 2 m grid).
pub const WALL: f32 = 5.6;
/// How far out a tunnel's roof reaches past the road.
pub const ROOF: f32 = 7.0;
/// A deck's or a roof's thickness.
const DECK: f32 = 0.9;

impl Crossing {
    pub fn dir(&self) -> Vec2 {
        (self.to - self.from).normalize()
    }

    pub fn len(&self) -> f32 {
        self.from.distance(self.to)
    }

    /// Distance from its center line.
    pub fn distance(&self, p: Vec2) -> f32 {
        let d = self.to - self.from;
        let t = ((p - self.from).dot(d) / d.length_squared()).clamp(0.0, 1.0);
        p.distance(self.from + d * t)
    }

    /// Builds it, `ground` the land's height.
    pub fn build(&self, map: &mut Map, s: Stone, ground: impl Fn(Vec2) -> f32) {
        let (d, side) = (self.dir(), Vec2::new(-self.dir().y, self.dir().x));
        let yaw = d.x.atan2(d.y);
        let mut put = |t: (f32, f32), off: f32, w: f32, y: (f32, f32), c: Vec3, solid: bool| {
            let at = self.from + d * (t.0 + t.1) * 0.5 + side * off;
            map.oboxes.push(OBox {
                center: at,
                yaw,
                half: Vec2::new(w, (t.1 - t.0) * 0.5),
                y0: y.0,
                y1: y.1,
                color: c,
                side_mat: s.mat,
                top_mat: s.mat,
                solid,
                chip: 0.01,
                ..OBox::default()
            });
            map.oboxes.last_mut().unwrap().side_mat = if s.mat == Material::Metal {
                Material::Concrete
            } else {
                s.mat
            };
            map.oboxes.len() - 1
        };
        let mut floors = Vec::new();
        match self.kind {
            Kind::Bridge => {
                let len = self.len();
                // The deck, a little into the rims.
                let ends = (-1.5, len + 1.5);
                floors.push(put(
                    ends,
                    0.0,
                    self.half,
                    (self.top - DECK, self.top),
                    s.wall * 0.9,
                    true,
                ));
                for e in [-1.0f32, 1.0] {
                    // Low walls along it.
                    put(
                        (1.0, len - 1.0),
                        e * (self.half - 0.3),
                        0.28,
                        (self.top, self.top + 0.95),
                        s.wall,
                        true,
                    );
                    for t in [1.0, len - 1.0] {
                        put(
                            (t - 0.7, t + 0.7),
                            e * (self.half - 0.3),
                            0.6,
                            (self.top - 2.0, self.top + 2.2),
                            s.trim,
                            true,
                        );
                    }
                }
                // Piers, in pairs, with room to drive between.
                let n = (len / 19.0).round().max(1.0) as i32;
                for k in 1..n {
                    let t = len * k as f32 / n as f32;
                    let at = self.from + d * t;
                    if self.top - ground(at) < 3.0 {
                        continue;
                    }
                    for e in [-1.0f32, 1.0] {
                        let off = e * (self.half - 1.2);
                        let g = ground(at + side * off) - 0.6;
                        put(
                            (t - 0.9, t + 0.9),
                            off,
                            0.9,
                            (g, self.top - DECK + 0.05),
                            s.wall * 0.94,
                            true,
                        );
                    }
                    put(
                        (t - 0.7, t + 0.7),
                        0.0,
                        self.half + 0.25,
                        (self.top - DECK - 0.7, self.top - DECK + 0.04),
                        s.wall * 0.86,
                        false,
                    );
                }
            }
            Kind::Tunnel => {
                let (a, b) = self.roofed;
                floors.push(put(
                    (a, b),
                    0.0,
                    self.half + ROOF,
                    (self.top - DECK, self.top + 0.03),
                    s.wall * 0.86,
                    true,
                ));
                // Its walls, stepping down outside with the land; a portal
                // at each mouth.
                let len = self.len();
                let n = (len / 4.0).ceil() as i32;
                for k in 0..n {
                    let t = (len * k as f32 / n as f32, len * (k + 1) as f32 / n as f32);
                    let mid = (t.0 + t.1) * 0.5;
                    let roofed = mid > a && mid < b;
                    for e in [-1.0f32, 1.0] {
                        let off = e * (self.half + 0.3 + WALL * 0.5);
                        let land = ground(self.from + d * mid + side * e * (self.half + 7.5));
                        let top = if roofed {
                            self.top - DECK + 0.04
                        } else {
                            (land + 0.5).min(self.top)
                        };
                        if top > self.floor + 0.6 {
                            put(t, off, WALL * 0.5, (self.floor - 0.6, top), s.wall, true);
                        }
                    }
                }
                for t in [a, b] {
                    for e in [-1.0f32, 1.0] {
                        put(
                            (t - 0.8, t + 0.8),
                            e * (self.half + 1.0),
                            1.1,
                            (self.floor - 0.6, self.top + 1.6),
                            s.trim,
                            true,
                        );
                    }
                    put(
                        (t - 0.6, t + 0.6),
                        0.0,
                        self.half + 2.0,
                        (self.top + 0.02, self.top + 0.95),
                        s.trim * 0.95,
                        true,
                    );
                }
            }
        }
        for k in floors {
            map.oboxes[k].ramp = true;
        }
    }
}
