//! Companion stars: some systems have two stars or three, the same for every
//! pilot. They're only to be looked at: in the system view (`theater_view`),
//! and in the skies over its battles (`Conditions::apply`).

use glam::{Vec2, Vec3};

use crate::render::Atmosphere;
use crate::rng::Rng;

/// The most companions a system's main star has.
pub const MAX_COMPANIONS: usize = 2;

/// What share of systems have two stars, and three.
const BINARY: f32 = 0.22;
const TERNARY: f32 = 0.08;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// Small and red.
    Dwarf,
    /// Orange, not much smaller than the main star.
    Amber,
    /// Blue-white, small and fierce.
    Blue,
}

impl Kind {
    pub const ALL: [Kind; 3] = [Kind::Dwarf, Kind::Amber, Kind::Blue];

    pub fn color(self) -> Vec3 {
        match self {
            Kind::Dwarf => Vec3::new(1.0, 0.3, 0.12),
            Kind::Amber => Vec3::new(1.0, 0.6, 0.22),
            Kind::Blue => Vec3::new(0.42, 0.65, 1.0),
        }
    }

    /// How wide it is, 1 the main star's.
    pub fn size(self) -> f32 {
        match self {
            Kind::Dwarf => 0.5,
            Kind::Amber => 0.8,
            Kind::Blue => 0.6,
        }
    }
}

/// A star beside a system's main one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Companion {
    pub kind: Kind,
    /// Where it stands in a world's sky: degrees to the right of the main
    /// star and above it.
    pub aside: i8,
    pub above: i8,
}

/// A system's companion stars: none, one, or two.
pub type Companions = [Option<Companion>; MAX_COMPANIONS];

/// Those of system `system`, the same for every pilot.
pub fn companions(system: usize) -> Companions {
    let mut r = Rng::new(system as u64 ^ 0x0B1A_4157);
    let n = match r.f32() {
        x if x < TERNARY => 2,
        x if x < TERNARY + BINARY => 1,
        _ => 0,
    };
    let first = if r.f32() < 0.5 { -1.0 } else { 1.0 };
    std::array::from_fn(|i| {
        (i < n).then(|| {
            // One a close pair with the main star; the next wider, on its
            // other side.
            let (side, aside) = match i {
                0 => (first, r.range(8.0, 16.0)),
                _ => (-first, r.range(16.0, 28.0)),
            };
            Companion {
                kind: Kind::ALL[r.index(Kind::ALL.len())],
                aside: (side * aside) as i8,
                above: r.range(-4.0, 9.0) as i8,
            }
        })
    })
}

/// A companion in a map's sky, as it's drawn.
#[derive(Clone, Copy, Debug, Default)]
pub struct SkyStar {
    /// Direction toward it.
    pub dir: Vec3,
    /// Its color times intensity (HDR); black for none.
    pub color: Vec3,
    /// How small its disc is: its brightness falls off as this power of the
    /// cosine of the angle from its middle.
    pub sharp: f32,
}

/// How low in the sky a companion may stand: the sine of its elevation.
const LOWEST: f32 = 0.17;

/// Puts companions in sky `a`, beside its sun. By night the stars are down,
/// but their companions are far enough off to be brilliant points in the sky.
pub fn light_up(companions: &Companions, night: bool, a: &mut Atmosphere) {
    let sun = a.sun_dir.normalize();
    let right = Vec3::new(-sun.z, 0.0, sun.x).normalize_or(Vec3::X);
    let up = right.cross(sun);
    // As dim as dust has made the sun.
    let clear = a.sun_color.dot(Vec3::new(0.2126, 0.7152, 0.0722));
    let bright = match night {
        true if a.stars == 0.0 => 0.0,
        true => 0.4,
        false => (clear / 1.5).min(1.0),
    };
    for (sky, c) in a.companions.iter_mut().zip(companions) {
        *sky = SkyStar::default();
        let Some(c) = c else { continue };
        let tan = |deg: i8| (deg as f32).to_radians().tan();
        let d = (sun + right * tan(c.aside) + up * tan(c.above)).normalize();
        let y = d.y.max(LOWEST);
        let flat = Vec2::new(d.x, d.z).normalize_or(Vec2::X) * (1.0 - y * y).sqrt();
        let size = c.kind.size();
        *sky = SkyStar {
            dir: Vec3::new(flat.x, y, flat.y),
            color: c.kind.color() * bright,
            sharp: match night {
                true => 40_000.0,
                false => 2400.0 / (size * size),
            },
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Most systems have one star, some two, a few three.
    #[test]
    fn some_systems_have_more_stars() {
        let mut of = [0; 3];
        for s in 0..10_000 {
            let c = companions(s);
            assert_eq!(c, companions(s));
            of[c.iter().flatten().count()] += 1;
        }
        assert!((6700..7300).contains(&of[0]), "{of:?}");
        assert!((1950..2450).contains(&of[1]), "{of:?}");
        assert!((650..950).contains(&of[2]), "{of:?}");
    }

    /// In the sky, each stands clear of the sun and of each other, over the
    /// horizon, however high the sun.
    #[test]
    fn companions_stand_apart_in_the_sky() {
        for y in [0.17f32, 0.24, 0.5, 0.8] {
            for s in 0..2000 {
                let mut a = Atmosphere {
                    sun_dir: Vec3::new((1.0 - y * y).sqrt(), y, 0.0),
                    ..Default::default()
                };
                let c = companions(s);
                light_up(&c, false, &mut a);
                let mut up = vec![a.sun_dir];
                for (k, sky) in a.companions.iter().enumerate() {
                    assert_eq!(c[k].is_some(), sky.color != Vec3::ZERO);
                    if c[k].is_some() {
                        assert!(sky.dir.is_normalized() && sky.dir.y >= LOWEST - 1e-4);
                        for d in &up {
                            let apart = d.angle_between(sky.dir).to_degrees();
                            assert!(apart > 6.5, "{apart} in system {s}");
                        }
                        up.push(sky.dir);
                    }
                }
            }
        }
    }
}
