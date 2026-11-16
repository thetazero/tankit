//! Your hull running low: an alert as it drops under [`LOW`], then a pulse
//! like a heartbeat for as long as it stays there, quickening as the hull
//! nears nothing. Only your own tank, and heard like the HUD (not placed):
//! it's a warning light you can hear.

use super::Play;
use crate::audio::voice::{Filt, Layer, noise, sine};
use crate::combat::Match;

/// Below this share of its full hull a tank is low.
pub const LOW: f32 = 0.3;

/// Seconds between pulses: just under [`LOW`], and with the hull all but gone.
const SLOW: f32 = 1.3;
const FAST: f32 = 0.55;

/// Seconds between pulses with `frac` of the hull left.
pub fn period(frac: f32) -> f32 {
    let t = (frac / LOW).clamp(0.0, 1.0);
    FAST + (SLOW - FAST) * t
}

/// Whether the player's hull was low last frame, and when the next pulse is due.
#[derive(Default)]
pub struct Warning {
    low: bool,
    next: f32,
}

impl Warning {
    /// Once a live frame, `time` seconds into the match: the alert or pulse
    /// to play, if one is due.
    pub fn update(&mut self, game: &Match, time: f32) -> Option<Play> {
        let f = &game.fighters[0];
        let frac = f.health / f.max_health(&game.world.tanks[0]);
        // A husk is past its hull, and has its own bell.
        let low = f.alive && !f.is_husk() && frac < LOW;
        let was = std::mem::replace(&mut self.low, low);
        if !low {
            return None;
        }
        if !was {
            // The alert's two notes, then the first pulse.
            self.next = time + 0.6;
            return Some(Play::everywhere(ALERT));
        }
        if time >= self.next {
            self.next = time + period(frac);
            // A little harder the closer to nothing.
            let urgency = 1.0 - (frac / LOW).clamp(0.0, 1.0);
            return Some(Play {
                gain: 0.75 + 0.35 * urgency,
                ..Play::everywhere(PULSE)
            });
        }
        None
    }
}

/// Hull under a third: two falling notes of an alarm, a hollow square-ish
/// tone (odd harmonics) so small speakers carry it.
pub const ALERT: &[Layer] = &[
    sine(988.0, 988.0, 0.1, 0.1, 0.3),
    sine(2964.0, 2964.0, 0.1, 0.06, 0.08),
    Layer {
        delay: 0.17,
        ..sine(740.0, 740.0, 0.1, 0.14, 0.32)
    },
    Layer {
        delay: 0.17,
        ..sine(2220.0, 2220.0, 0.1, 0.08, 0.09)
    },
];

/// While low: a heartbeat, its two beats a dull thump with a short low bleep
/// on top (the thump alone vanishes on a laptop).
pub const PULSE: &[Layer] = &[
    sine(90.0, 50.0, 0.05, 0.07, 0.45),
    noise(Filt::Low, 600.0, 200.0, 0.03, 0.03, 0.25),
    sine(660.0, 620.0, 0.05, 0.05, 0.14),
    Layer {
        delay: 0.17,
        ..sine(80.0, 45.0, 0.05, 0.06, 0.32)
    },
    Layer {
        delay: 0.17,
        ..sine(620.0, 580.0, 0.05, 0.04, 0.09)
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::mixer::{Mixer, OneShot};
    use crate::audio::testkit::sane;
    use crate::audio::wav;

    #[test]
    fn every_sound_is_sane() {
        sane(&[("hull_alert", ALERT), ("hull_pulse", PULSE)]);
    }

    fn game() -> Match {
        Match::new(crate::map::Map::saltmarket(), 0, 5, 7)
    }

    /// Set the player's hull to `frac` of full.
    fn hull(game: &mut Match, frac: f32) {
        let max = game.fighters[0].max_health(&game.world.tanks[0]);
        game.fighters[0].health = frac * max;
    }

    /// Run `w` over `secs` of frames; which patches it played, and when.
    fn run(w: &mut Warning, game: &Match, from: f32, secs: f32) -> Vec<(f32, &'static str)> {
        let mut out = Vec::new();
        let mut t = from;
        while t < from + secs {
            if let Some(p) = w.update(game, t) {
                out.push((t, if p.layers == ALERT { "alert" } else { "pulse" }));
            }
            t += 1.0 / 60.0;
        }
        out
    }

    #[test]
    fn quiet_while_the_hull_is_sound() {
        let mut g = game();
        let mut w = Warning::default();
        hull(&mut g, 1.0);
        assert!(run(&mut w, &g, 0.0, 3.0).is_empty());
        hull(&mut g, LOW + 0.01);
        assert!(run(&mut w, &g, 3.0, 3.0).is_empty());
    }

    /// Dropping low alerts once, then pulses steadily until repaired or dead.
    #[test]
    fn low_hull_alerts_then_pulses() {
        let mut g = game();
        let mut w = Warning::default();
        hull(&mut g, 0.25);
        let heard = run(&mut w, &g, 0.0, 5.0);
        assert_eq!(heard[0], (0.0, "alert"));
        assert!(heard[1..].iter().all(|h| h.1 == "pulse"));
        assert!(heard.len() >= 4, "{heard:?}");

        // Repaired: quiet. Low again: a fresh alert.
        hull(&mut g, 0.6);
        assert!(run(&mut w, &g, 5.0, 2.0).is_empty());
        hull(&mut g, 0.2);
        assert_eq!(run(&mut w, &g, 7.0, 0.1)[0].1, "alert");

        // Dead: quiet.
        g.fighters[0].alive = false;
        hull(&mut g, 0.0);
        assert!(run(&mut w, &g, 8.0, 3.0).is_empty());
    }

    /// The nearer to nothing, the faster the heart.
    #[test]
    fn the_pulse_quickens() {
        assert!(period(0.29) > 1.2);
        assert!(period(0.01) < 0.6);
        let beats = |frac| {
            let mut g = game();
            hull(&mut g, frac);
            run(&mut Warning::default(), &g, 0.0, 6.0).len()
        };
        assert!(beats(0.03) > beats(0.28) + 3);
    }

    /// Only your own tank: a bot's hull says nothing.
    #[test]
    fn only_your_own_hull() {
        let mut g = game();
        let w = &mut Warning::default();
        for f in &mut g.fighters[1..] {
            f.health = 1.0;
        }
        assert!(run(w, &g, 0.0, 3.0).is_empty());
    }

    /// Writes `docs/audio/hull-low.wav`: your hull dropping under a third,
    /// sinking toward nothing (the pulse quickening), then repaired:
    /// `cargo test record_hull -- --ignored`.
    #[test]
    #[ignore]
    fn record_hull() {
        const RATE: u32 = 48_000;
        let volume = crate::profile::Settings::default().volume();
        let mut m = Mixer::new(RATE as f32);
        m.set_master(volume * volume);
        m.set_world(1.0);
        let mut g = game();
        let mut w = Warning::default();
        let frame = (RATE / 60) as usize * 2;
        let mut out = Vec::new();
        for i in 0..(9.0 * 60.0) as usize {
            let t = i as f32 / 60.0;
            // Full, then hit to 28% at 0.5 s, worn down to 3% by 7 s, repaired at 8 s.
            let frac = match t {
                t if t < 0.5 => 1.0,
                t if t < 7.0 => 0.28 - 0.25 * (t - 0.5) / 6.5,
                t if t < 8.0 => 0.03,
                _ => 0.6,
            };
            hull(&mut g, frac);
            if let Some(p) = w.update(&g, t) {
                m.play(OneShot {
                    layers: p.layers,
                    pitch: p.pitch,
                    gain: 0.7 * p.gain,
                    pan: 0.0,
                    air: 16_000.0,
                });
            }
            let start = out.len();
            out.resize(start + frame, 0.0);
            m.render(&mut out[start..]);
        }
        std::fs::write("docs/audio/hull-low.wav", wav(&out, RATE)).unwrap();
    }
}
