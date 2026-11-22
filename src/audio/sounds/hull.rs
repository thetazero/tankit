//! Your hull running low: an alert as it drops under [`LOW`], then a pulse
//! like a heartbeat for as long as it stays there, quickening as the hull
//! nears nothing. Only your own tank, and heard like the HUD (not placed):
//! it's a warning light you can hear.

use super::Play;
use crate::audio::voice::{Filt, Layer, noise, sine};
use crate::combat::{Match, Phase};

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
        // Once the round is over the fight is too: no warning over the results.
        let low = game.phase == Phase::Playing && f.alive && !f.is_husk() && frac < LOW;
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

/// One partial of the warning's buzzer: a steady tone that swells in over
/// `attack` and is gone within a tenth of a second or so. The swell and the
/// quick, even fall keep it a "bup" rather than a ping (a sine struck at
/// once and left to ring, falling in pitch, is sonar).
const fn tone(f: f32, attack: f32, decay: f32, gain: f32, delay: f32) -> Layer {
    Layer {
        attack,
        delay,
        ..sine(f, f, 0.1, decay, gain)
    }
}

/// Hull under a third: two falling notes of a low buzzer, a hollow
/// square-ish tone (odd harmonics) so small speakers carry it, low enough
/// not to sting.
pub const ALERT: &[Layer] = &[
    tone(392.0, 0.012, 0.07, 0.26, 0.0),
    tone(1176.0, 0.012, 0.05, 0.08, 0.0),
    tone(1960.0, 0.012, 0.04, 0.035, 0.0),
    tone(294.0, 0.012, 0.07, 0.28, 0.2),
    tone(882.0, 0.012, 0.06, 0.09, 0.2),
    tone(1470.0, 0.012, 0.05, 0.04, 0.2),
];

/// While low: a heartbeat, its two beats a dull thump with a short, soft
/// buzz on top in the buzzer's voice (the thump alone vanishes on a laptop).
pub const PULSE: &[Layer] = &[
    sine(90.0, 50.0, 0.05, 0.07, 0.45),
    noise(Filt::Low, 600.0, 200.0, 0.03, 0.03, 0.25),
    tone(196.0, 0.015, 0.04, 0.12, 0.0),
    tone(588.0, 0.015, 0.03, 0.07, 0.0),
    tone(980.0, 0.015, 0.025, 0.035, 0.0),
    Layer {
        delay: 0.17,
        ..sine(80.0, 45.0, 0.05, 0.06, 0.32)
    },
    tone(196.0, 0.015, 0.035, 0.08, 0.17),
    tone(588.0, 0.015, 0.025, 0.045, 0.17),
    tone(980.0, 0.015, 0.02, 0.022, 0.17),
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
        let mut m = Match::new(crate::map::Map::saltmarket(), 0, 5, 7);
        m.start();
        m
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

    /// The round over, the warning stops, even mid-pulse with the hull low.
    #[test]
    fn quiet_once_the_round_is_over() {
        let mut g = game();
        let mut w = Warning::default();
        hull(&mut g, 0.1);
        assert!(!run(&mut w, &g, 0.0, 2.0).is_empty());
        g.phase = Phase::Over;
        assert!(run(&mut w, &g, 2.0, 5.0).is_empty());
    }

    /// Nothing like a sonar ping: no partial rings on (each is gone, -60 dB,
    /// within half a second), and every tone above the thump swells in and
    /// holds its pitch instead of being struck and sliding down.
    #[test]
    fn not_a_ping() {
        for l in PULSE.iter().chain(ALERT) {
            assert!(l.attack + l.decay * 6.9 < 0.55, "rings: {}", l.freq.0);
            if l.freq.0 > 150.0 && l.src == crate::audio::voice::Src::Sine {
                assert_eq!(l.freq.0, l.freq.1, "slides like a ping");
                assert!(l.attack >= 0.01, "struck like a ping: {}", l.freq.0);
            }
        }
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
