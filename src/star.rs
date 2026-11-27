//! How a star looks by its brightness (`galaxy::System::starlight`, 1 the
//! galaxy's average): a dim one small and red, an ordinary one a warm white,
//! a very bright one big and blue-white. The menus draw it by this, and the
//! skies of the battles fought under it are made over by it.

use glam::Vec3;

use crate::render::Atmosphere;

/// Starlight at which a star's as big and blue as they get (the galaxy's
/// brightest), and its inverse, as small and red.
const BRIGHTEST: f32 = 8.0;

/// A star's own color: the dimmest's, an ordinary one's, the brightest's.
const DIM: Vec3 = Vec3::new(1.0, 0.36, 0.13);
pub const ORDINARY: Vec3 = Vec3::new(1.0, 0.86, 0.62);
const BRIGHT: Vec3 = Vec3::new(0.45, 0.66, 1.0);

#[derive(Clone, Copy, Debug)]
pub struct Star {
    /// -1 the dimmest, 0 an ordinary star, 1 the brightest.
    k: f32,
}

impl Star {
    pub fn of(starlight: f32) -> Star {
        let k = starlight.max(1e-3).ln() / BRIGHTEST.ln();
        Star {
            k: k.clamp(-1.0, 1.0),
        }
    }

    /// `dim`, `ordinary` or `bright`, or between.
    fn blend(self, dim: f32, ordinary: f32, bright: f32) -> f32 {
        self.blend3(Vec3::splat(dim), Vec3::splat(ordinary), Vec3::splat(bright))
            .x
    }

    fn blend3(self, dim: Vec3, ordinary: Vec3, bright: Vec3) -> Vec3 {
        match self.k {
            k if k < 0.0 => ordinary.lerp(dim, -k),
            k => ordinary.lerp(bright, k),
        }
    }

    /// How bright it is among stars: 0 the dimmest, 1 the brightest.
    pub fn rank(self) -> f32 {
        (self.k + 1.0) * 0.5
    }

    pub fn color(self) -> Vec3 {
        self.blend3(DIM, ORDINARY, BRIGHT)
    }

    /// The color of its light, against an ordinary star's, as bright.
    fn hue(self) -> Vec3 {
        let one = Vec3::ONE;
        let c = self.blend3(Vec3::new(1.45, 0.83, 0.5), one, Vec3::new(0.72, 1.0, 1.55));
        match self.k {
            0.0 => one,
            _ => c / c.dot(Vec3::new(0.2126, 0.7152, 0.0722)),
        }
    }

    /// Its radius in the menus, against an ordinary star's.
    pub fn size(self) -> f32 {
        self.blend(0.62, 1.0, 1.4)
    }

    /// How strongly it glows in the menus, against an ordinary star.
    pub fn glow(self) -> f32 {
        self.blend(0.7, 1.0, 1.9)
    }

    /// Its light on the worlds in the menus, against an ordinary star's.
    pub fn light(self) -> Vec3 {
        self.hue() * self.blend(0.55, 1.0, 1.6)
    }

    /// Makes sky `a` over, the sun in it this star (`up`; at night it's the
    /// moon that's up, in this star's light). The light in the shade stays
    /// as bright, so tanks read as well under any star.
    pub fn shine(self, a: &mut Atmosphere, up: bool) {
        let shade = a.fill();
        a.sun_color *= self.hue() * self.blend(0.78, 1.0, 1.12);
        // The sky takes its hue too, less of it.
        let sky = Vec3::ONE.lerp(self.hue(), 0.55) * self.blend(0.8, 1.0, 1.12);
        a.sky_zenith *= sky;
        a.sky_horizon *= sky;
        if self.k != 0.0 {
            a.ambient *= shade / a.fill().max(1e-4);
        }
        if up {
            a.sun_size *= self.blend(0.6, 1.0, 2.6);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An average star leaves a sky as it was; a brighter one's is bigger
    /// and bluer, a dimmer one's smaller and redder, the shade as bright.
    #[test]
    fn skies_differ_by_starlight() {
        let own = Atmosphere::default();
        let under = |light| {
            let mut a = own;
            Star::of(light).shine(&mut a, true);
            a
        };
        assert_eq!(format!("{:?}", under(1.0)), format!("{own:?}"));
        let skies = [0.1, 0.4, 1.0, 2.5, 8.0].map(under);
        for w in skies.windows(2) {
            let blue = |a: &Atmosphere| a.sun_color.z / a.sun_color.x;
            let sky = |a: &Atmosphere| a.sky_horizon.z / a.sky_horizon.x;
            assert!(w[0].sun_size < w[1].sun_size);
            assert!(blue(&w[0]) < blue(&w[1]) && sky(&w[0]) < sky(&w[1]));
            assert!((w[0].fill() - w[1].fill()).abs() < 1e-3);
        }
        // Nothing but the light changes: not how far you can see.
        assert!(skies.iter().all(|a| a.sight() == own.sight()));
    }
}
