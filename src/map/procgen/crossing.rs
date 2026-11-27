//! Where a road meets a valley or a ridge: a bridge over the one, its deck
//! level from rim to rim, the valley's floor open underneath; a tunnel
//! through the other, dug out of it (`Land::bores`) level with the land either
//! side. Two floors to fight on either way.

use glam::{Vec2, Vec3};

use super::civic::Stone;
use super::land::Bore;
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
    /// The deck's top (or the ground's over the tunnel).
    pub top: f32,
    /// The tunnel's floor.
    pub floor: f32,
    /// The roofed stretch of a tunnel, along it.
    pub roofed: (f32, f32),
}

/// How far past its road a tunnel's kept clear of buildings.
pub const ROOF: f32 = 7.0;
/// A deck's thickness.
const DECK: f32 = 0.9;
/// How much rock there is over a tunnel.
pub const COVER: f32 = 1.5;

impl Crossing {
    pub fn dir(&self) -> Vec2 {
        (self.to - self.from).normalize()
    }

    pub fn len(&self) -> f32 {
        self.from.distance(self.to)
    }

    /// How high a tunnel is inside.
    fn clear(&self) -> f32 {
        self.top - self.floor - COVER
    }

    /// A tunnel, as it's dug.
    pub fn bore(&self) -> Bore {
        Bore {
            from: self.from,
            to: self.to,
            half: self.half,
            floor: self.floor,
            roofed: self.roofed,
            clear: self.clear(),
        }
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
                let roof = self.floor + self.clear();
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
                        (roof - 0.15, self.top + 0.95),
                        s.trim * 0.95,
                        true,
                    );
                }
            }
        }
        for &k in &floors {
            map.oboxes[k].ramp = true;
        }
    }
}
