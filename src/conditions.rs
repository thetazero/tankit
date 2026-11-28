//! What a map's fought in: the time of day, the weather, and the stars in its sky. A layer over
//! any map: the map is built as ever, with its own sky (its day), and the
//! conditions then make that sky over (`Conditions::apply`). Nothing on the
//! map moves, so its layout, cover and vistas are the same in any of them.
//!
//! A site's conditions come of its map seed, so it keeps them each visit.

use glam::{Vec2, Vec3};

use crate::companions::{self, Companions};
use crate::map::Map;
use crate::render::{Atmosphere, HIDDEN};
use crate::rng::Rng;
use crate::star::Star;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TimeOfDay {
    /// The map's own sky.
    #[default]
    Day,
    /// The sun low and orange, the sky going dark.
    Dusk,
    /// Moonlight and stars.
    Night,
}

impl TimeOfDay {
    pub const ALL: [TimeOfDay; 3] = [TimeOfDay::Day, TimeOfDay::Dusk, TimeOfDay::Night];

    pub fn name(self) -> &'static str {
        match self {
            TimeOfDay::Day => "DAY",
            TimeOfDay::Dusk => "DUSK",
            TimeOfDay::Night => "NIGHT",
        }
    }

    pub fn by_name(name: &str) -> Option<TimeOfDay> {
        TimeOfDay::ALL
            .into_iter()
            .find(|t| t.name().eq_ignore_ascii_case(name))
    }
}

/// The weather, on top of the time of day.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Weather {
    /// The map's own air.
    #[default]
    Clear,
    /// Sand blown so thick that nothing's made out past `STORM_SIGHT`.
    Sandstorm,
}

impl Weather {
    pub const ALL: [Weather; 2] = [Weather::Clear, Weather::Sandstorm];

    pub fn name(self) -> &'static str {
        match self {
            Weather::Clear => "CLEAR",
            Weather::Sandstorm => "SANDSTORM",
        }
    }

    pub fn by_name(name: &str) -> Option<Weather> {
        Weather::ALL
            .into_iter()
            .find(|w| w.name().eq_ignore_ascii_case(name))
    }
}

/// How far off a tank can be made out in a sandstorm, m.
pub const STORM_SIGHT: f32 = 65.0;
/// How hard a sandstorm blows, m/s.
const STORM_WIND: f32 = 15.0;

/// What a map's fought in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Conditions {
    pub time: TimeOfDay,
    pub weather: Weather,
    /// How bright the system's star is, 1 the galaxy's average
    /// (`galaxy::System::starlight`).
    pub starlight: f32,
    /// The companions of its system's star.
    pub companions: Companions,
}

impl Default for Conditions {
    fn default() -> Conditions {
        Conditions {
            time: TimeOfDay::Day,
            weather: Weather::Clear,
            starlight: 1.0,
            companions: Companions::default(),
        }
    }
}

impl Conditions {
    /// The conditions at the site whose map seed this is: half of them are
    /// fought over by day, a quarter at dusk, a quarter at night; a quarter
    /// of those on sand (`sandy`) in a sandstorm, whatever the hour; under
    /// a star as bright as `starlight`.
    pub fn of_site(map_seed: u64, sandy: bool, starlight: f32) -> Conditions {
        // Salted: what else is drawn from the seed stays as it was.
        let time = match Rng::new(map_seed ^ 0x0DA7_0D05).index(4) {
            0 => TimeOfDay::Dusk,
            1 => TimeOfDay::Night,
            _ => TimeOfDay::Day,
        };
        let storm = sandy && Rng::new(map_seed ^ 0x5A2D_5702).index(4) == 0;
        let weather = match storm {
            true => Weather::Sandstorm,
            false => Weather::Clear,
        };
        Conditions {
            time,
            weather,
            starlight,
            companions: Companions::default(),
        }
    }

    /// Makes `map`'s sky over, from its own.
    pub fn apply(self, map: &mut Map) {
        let sky = match self.time {
            TimeOfDay::Day => None,
            TimeOfDay::Dusk => Some(&DUSK),
            TimeOfDay::Night => Some(&NIGHT),
        };
        if let Some(sky) = sky {
            sky.apply(&mut map.atmosphere);
        }
        let up = self.time != TimeOfDay::Night;
        Star::of(self.starlight).shine(&mut map.atmosphere, up);
        match self.weather {
            Weather::Clear => {}
            Weather::Sandstorm => storm(&mut map.atmosphere),
        }
        let night = self.time == TimeOfDay::Night;
        companions::light_up(&self.companions, night, &mut map.atmosphere);
    }
}

/// A time's sky, before it takes on the map's hues.
struct Sky {
    /// How high the sun (or the moon) may stand: the sine of its elevation.
    /// The map's own is brought into this range.
    height: (f32, f32),
    sun: Vec3,
    zenith: Vec3,
    horizon: Vec3,
    /// How bright the ground's bounce light is, of its own.
    ground: f32,
    /// How bright the light in the shade comes out: dark, but tanks in it
    /// still read.
    fill: f32,
    /// On a map with dark shade of its own, at most this share of it.
    share: f32,
    stars: f32,
}

const DUSK: Sky = Sky {
    height: (0.17, 0.24),
    sun: Vec3::new(1.85, 1.0, 0.52),
    zenith: Vec3::new(0.1, 0.12, 0.26),
    horizon: Vec3::new(0.56, 0.31, 0.21),
    ground: 0.55,
    fill: 0.36,
    share: 0.85,
    stars: 0.0,
};

const NIGHT: Sky = Sky {
    height: (0.45, 0.7),
    sun: Vec3::new(0.3, 0.38, 0.56),
    zenith: Vec3::new(0.007, 0.011, 0.027),
    horizon: Vec3::new(0.035, 0.05, 0.085),
    ground: 0.12,
    fill: 0.22,
    share: 0.75,
    stars: 1.0,
};

/// Blows a sandstorm up in `a`: the sky one wall of dust, as bright as the
/// horizon was, the sun a dim smear behind it, and the haze thick.
fn storm(a: &mut Atmosphere) {
    let shade = a.fill();
    let dust = a.dust / luma(a.dust).max(1e-4);
    // Keeps some of the hour's own hue (orange at dusk, blue by moonlight).
    // At night the dust holds what moonlight there is.
    let light = (luma(a.sky_horizon) * 0.8).max(0.1);
    let hue = (a.sky_horizon / luma(a.sky_horizon).max(1e-4)).lerp(dust, 0.75);
    a.sky_horizon = hue * light;
    a.sky_zenith = a.sky_horizon * 0.8;
    a.sun_color = a.sun_color * hue * 0.3;
    a.stars = 0.0;
    // The light the sun lost comes from all round instead.
    a.ambient *= shade / a.fill().max(1e-4) * 1.15;
    a.fog_density = -HIDDEN.ln() / STORM_SIGHT;
    a.wind = a.wind.normalize_or(Vec2::X) * STORM_WIND;
    a.blowing_sand = 1.0;
}

/// Whether a sandstorm's blowing in `a`.
pub fn storming(a: &Atmosphere) -> bool {
    a.sight() < 1.5 * STORM_SIGHT
}

/// How much of the map's own hues its sky keeps.
const KEEP_HUE: f32 = 0.3;

impl Sky {
    fn apply(&self, a: &mut Atmosphere) {
        let shade = self.fill.min(a.fill() * self.share);
        let sun = a.sun_dir.normalize();
        let y = sun.y.clamp(self.height.0, self.height.1);
        let flat = Vec2::new(sun.x, sun.z).normalize_or(Vec2::X) * (1.0 - y * y).sqrt();
        a.sun_dir = Vec3::new(flat.x, y, flat.y);
        let tinted = |c: Vec3, own: Vec3| {
            let hue = own / luma(own).max(1e-4);
            c * Vec3::ONE.lerp(hue, KEEP_HUE)
        };
        a.sun_color = tinted(self.sun, a.sun_color);
        a.sky_zenith = tinted(self.zenith, a.sky_zenith);
        a.sky_horizon = tinted(self.horizon, a.sky_horizon);
        a.ground_color *= self.ground;
        a.ambient = 1.0;
        a.ambient = shade / a.fill().max(1e-4);
        a.stars = self.stars;
    }
}

fn luma(c: Vec3) -> f32 {
    c.dot(Vec3::new(0.2126, 0.7152, 0.0722))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::faction::Faction;
    use crate::map::procgen::Look;
    use crate::map::procgen::setting::Setting;
    use crate::{Generator, MapKind, make_map};

    /// Every kind of map can be fought at any time of day: each darker than
    /// the one before, in the shade and out of it, but never black, and the
    /// sun up and on the side it was.
    #[test]
    fn any_map_has_a_day_a_dusk_and_a_night() {
        let g = Generator::up(Faction::Imperium);
        // (Maps the z-fighting tests build too: they take seconds each.)
        use crate::map::procgen::foundry::Ground;
        let up = Generator::up;
        for (kind, seed) in [
            (MapKind::Reservoir, 0),
            (MapKind::Sanctum(Ground::Desert), 1),
            (MapKind::Sanctum(Ground::Tundra), 2),
            (MapKind::Sanctum(Ground::Grassland), 3),
            (MapKind::Foundry(Look::Imperium, Ground::Desert), 0),
            (MapKind::City(Look::Imperium, Default::default()), 0),
            (MapKind::City(Look::Resonant, Default::default()), 0),
            (MapKind::City(Look::Resonant, Setting::Grassland), 1),
            (
                MapKind::ShieldSite(Look::Imperium, Default::default(), g),
                0,
            ),
            (MapKind::Depot(Look::Imperium, Ground::Desert), 0),
            (MapKind::Depot(Look::Resonant, Ground::Tundra), 1),
            (MapKind::Depot(Look::Uthenn, Ground::Grassland), 2),
            (MapKind::Mine(Look::Imperium, Ground::Desert), 0),
            (MapKind::Mine(Look::Resonant, Ground::Tundra), 0),
            (MapKind::Mine(Look::Uthenn, Ground::Grassland), 0),
            (MapKind::Desert, 3),
            (
                MapKind::ShieldSite(Look::Imperium, Setting::Tundra, up(Faction::Uthenn)),
                0,
            ),
            (
                MapKind::ShieldSite(Look::Resonant, Setting::Grassland, g),
                1,
            ),
            (MapKind::ShieldCamp(Ground::Desert, g), 3),
            (MapKind::ShieldCamp(Ground::Tundra, g), 0),
            (
                MapKind::ShieldCamp(Ground::Grassland, up(Faction::Resonant)),
                2,
            ),
            (MapKind::Hoard(Ground::Tundra), 1),
        ] {
            let own = make_map(kind, seed);
            let mut last = (f32::INFINITY, f32::INFINITY);
            for time in TimeOfDay::ALL {
                let mut m = own.clone();
                Conditions {
                    time,
                    ..Default::default()
                }
                .apply(&mut m);
                let a = &m.atmosphere;
                let light = (luma(a.sun_color), a.fill());
                assert!(
                    light.0 < last.0 && light.1 < last.1,
                    "{kind:?} at {time:?}: {light:?}, after {last:?}"
                );
                assert!(light.0 > 0.2 && light.1 > 0.15, "{kind:?} at {time:?}");
                last = light;
                let was = own.atmosphere.sun_dir.normalize();
                let sun = a.sun_dir.normalize();
                assert!(sun.y > 0.15, "{kind:?} at {time:?}: the sun's down");
                let side = Vec2::new(was.x, was.z).normalize();
                assert!(Vec2::new(sun.x, sun.z).normalize().dot(side) > 0.999);
                if time == TimeOfDay::Day {
                    assert_eq!(format!("{a:?}"), format!("{:?}", own.atmosphere));
                }
            }
        }
    }

    /// Sites come in every time of day, day the most.
    #[test]
    fn sites_are_fought_over_at_all_hours() {
        let count = |t| {
            (0..4000)
                .filter(|&s| Conditions::of_site(s, true, 1.0).time == t)
                .count()
        };
        let [day, dusk, night] = TimeOfDay::ALL.map(count);
        assert!((1800..2200).contains(&day), "{day}");
        assert!((850..1150).contains(&dusk), "{dusk}");
        assert!((850..1150).contains(&night), "{night}");
    }

    /// A quarter of the sites on sand are fought over in a sandstorm, and no
    /// others; the hour's the same either way.
    #[test]
    fn storms_blow_on_sand() {
        let storms = (0..4000)
            .filter(|&s| {
                let (on, off) = (
                    Conditions::of_site(s, true, 1.0),
                    Conditions::of_site(s, false, 1.0),
                );
                assert_eq!(on.time, off.time);
                assert_eq!(off.weather, Weather::Clear);
                on.weather == Weather::Sandstorm
            })
            .count();
        assert!((850..1150).contains(&storms), "{storms}");
    }

    /// A sandstorm cuts sight to `STORM_SIGHT` at any hour; without one,
    /// haze hides nothing a bot could see.
    #[test]
    fn a_sandstorm_cuts_sight() {
        for kind in [
            MapKind::Desert,
            MapKind::Mine(Look::Ithessar, crate::map::procgen::foundry::Ground::Desert),
        ] {
            let own = make_map(kind, 1);
            assert!(own.atmosphere.sight() > 2.0 * crate::ai::SIGHT);
            assert!(!storming(&own.atmosphere));
            for time in TimeOfDay::ALL {
                let mut m = own.clone();
                Conditions {
                    time,
                    weather: Weather::Sandstorm,
                    ..Default::default()
                }
                .apply(&mut m);
                let a = &m.atmosphere;
                assert!((a.sight() - STORM_SIGHT).abs() < 0.1);
                assert!(storming(a) && kind.sandy());
                assert!(luma(a.sun_color) > 0.05 && a.fill() > 0.15, "{time:?}");
            }
        }
    }
}
