//! The round's flow: the horn that starts it, the clock ticking down its
//! last seconds, the horn that ends it, and the sting that says how it went.
//! Heard wherever you are, like the HUD.
//!
//! The end horn and the outcome sting are played together; the sting's
//! layers wait (`AFTER_HORN`) until the horn has had its say.

use super::Play;
use crate::audio::voice::{Filt, Layer, noise, ring, sine};
use crate::combat::RoundCue;

/// The last few seconds tick higher and harder than the rest.
const FINAL: u8 = 3;

pub fn cue(cue: RoundCue) -> Play {
    Play::everywhere(match cue {
        RoundCue::Start => START,
        RoundCue::Count(n) if n <= FINAL => FINAL_TICK,
        RoundCue::Count(_) => TICK,
        RoundCue::End => END,
        RoundCue::Won => WON,
        RoundCue::Draw => DRAW,
        RoundCue::Lost => LOST,
    })
}

/// A brassy note: a fundamental and two overtones with a soft attack,
/// starting `delay` in.
const fn horn(f: f32, delay: f32, decay: f32, gain: f32) -> [Layer; 3] {
    [
        Layer {
            delay,
            attack: 0.03,
            ..sine(f, f, 1.0, decay, gain)
        },
        Layer {
            delay,
            attack: 0.04,
            ..sine(f * 2.0, f * 2.0, 1.0, decay * 0.8, gain * 0.45)
        },
        Layer {
            delay,
            attack: 0.05,
            ..sine(f * 3.0, f * 3.0, 1.0, decay * 0.6, gain * 0.2)
        },
    ]
}

/// A clean bell-like tone and its octave, starting `delay` in.
const fn chime(f: f32, delay: f32, decay: f32, gain: f32) -> [Layer; 2] {
    [
        Layer {
            delay,
            attack: 0.005,
            ..sine(f, f, 1.0, decay, gain)
        },
        Layer {
            delay,
            attack: 0.005,
            ..sine(f * 2.0, f * 2.0, 1.0, decay * 0.5, gain * 0.3)
        },
    ]
}

/// Round start: a two-note horn call rising a fourth (A to D) over a drum
/// hit: go.
pub const START: &[Layer] = &{
    let a = horn(220.0, 0.0, 0.12, 0.2);
    let d = horn(294.0, 0.18, 0.45, 0.24);
    [
        ring(70.0, 0.3, 0.3),
        noise(Filt::Low, 900.0, 200.0, 0.08, 0.12, 0.25),
        a[0],
        a[1],
        a[2],
        d[0],
        d[1],
        d[2],
    ]
};

/// One of the last ten seconds: a dry clock tick and a short beep.
pub const TICK: &[Layer] = &[
    noise(Filt::Band, 2500.0, 2500.0, 0.1, 0.012, 0.35),
    Layer {
        attack: 0.003,
        ..sine(1175.0, 1175.0, 1.0, 0.06, 0.22)
    },
];

/// One of the last three seconds: higher, longer and harder than a tick.
pub const FINAL_TICK: &[Layer] = &[
    noise(Filt::Band, 3200.0, 3200.0, 0.1, 0.015, 0.3),
    Layer {
        attack: 0.003,
        ..sine(1760.0, 1760.0, 1.0, 0.16, 0.28)
    },
    Layer {
        attack: 0.003,
        ..sine(3520.0, 3520.0, 1.0, 0.08, 0.06)
    },
];

/// Round over: a long low buzzing horn (odd overtones, like a klaxon)
/// over a deep gong.
pub const END: &[Layer] = &[
    ring(73.0, 1.2, 0.3),
    ring(110.0, 0.8, 0.12),
    noise(Filt::Low, 600.0, 150.0, 0.3, 0.3, 0.15),
    Layer {
        attack: 0.04,
        ..sine(147.0, 145.0, 1.0, 0.7, 0.24)
    },
    Layer {
        attack: 0.05,
        ..sine(441.0, 435.0, 1.0, 0.6, 0.1)
    },
    Layer {
        attack: 0.06,
        ..sine(735.0, 725.0, 1.0, 0.5, 0.05)
    },
];

/// Seconds the outcome sting waits for the end horn.
const AFTER_HORN: f32 = 0.9;

/// Your side won: a horn fanfare in D major, a fifth, the third, then the
/// whole chord held high over a low D.
pub const WON: &[Layer] = &{
    let t = AFTER_HORN;
    let a = horn(587.0, t, 0.12, 0.14);
    let b = horn(880.0, t + 0.13, 0.12, 0.14);
    let c = horn(740.0, t + 0.26, 0.12, 0.14);
    let d = horn(1175.0, t + 0.42, 0.7, 0.16);
    let e = horn(880.0, t + 0.42, 0.7, 0.1);
    let f = horn(147.0, t + 0.42, 0.7, 0.2);
    [
        a[0], a[1], a[2], b[0], b[1], b[2], c[0], c[1], c[2], d[0], d[1], d[2], e[0], e[1], e[2],
        f[0], f[1], f[2],
    ]
};

/// A draw: an open fifth (D and A, neither major nor minor) chimed twice,
/// the same both times.
pub const DRAW: &[Layer] = &{
    let t = AFTER_HORN;
    let a = chime(294.0, t, 0.4, 0.18);
    let b = chime(440.0, t, 0.4, 0.14);
    let c = chime(294.0, t + 0.45, 0.6, 0.18);
    let d = chime(440.0, t + 0.45, 0.6, 0.14);
    [a[0], a[1], b[0], b[1], c[0], c[1], d[0], d[1]]
};

/// `l`, sagging flat as it sounds.
const fn sag(l: Layer) -> Layer {
    Layer {
        freq: (l.freq.0, l.freq.0 * 0.94),
        glide: 0.6,
        ..l
    }
}

/// Your side lost: a slow fall down a D minor chord (A, F, D), each note
/// sagging flat, onto a low D and F and a dull thud.
pub const LOST: &[Layer] = &{
    let t = AFTER_HORN;
    let a = horn(440.0, t, 0.22, 0.14);
    let b = horn(349.0, t + 0.28, 0.22, 0.14);
    let c = horn(294.0, t + 0.56, 0.9, 0.16);
    let d = horn(175.0, t + 0.56, 0.9, 0.12);
    [
        sag(a[0]),
        sag(a[1]),
        sag(a[2]),
        sag(b[0]),
        sag(b[1]),
        sag(b[2]),
        sag(c[0]),
        sag(c[1]),
        sag(c[2]),
        sag(d[0]),
        sag(d[1]),
        sag(d[2]),
        Layer {
            delay: t + 0.56,
            ..ring(60.0, 0.4, 0.3)
        },
    ]
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::testkit::{ears, sane};
    use crate::audio::{Audio, wav};
    use crate::combat::Sound;

    #[test]
    fn every_sound_is_sane() {
        sane(&[
            ("start", START),
            ("tick", TICK),
            ("final_tick", FINAL_TICK),
            ("end", END),
            ("won", WON),
            ("draw", DRAW),
            ("lost", LOST),
        ]);
    }

    /// `cues` played together, as heard in the match, `secs` long.
    fn heard(cues: &[RoundCue], secs: f32) -> Vec<f32> {
        const RATE: u32 = 48_000;
        let mut audio = Audio::offline(RATE);
        audio.set_volume(crate::profile::Settings::default().volume());
        // Let the master volume settle first, as it has in a match: a tick is
        // over before it would have.
        audio.render(&mut vec![0.0; 2 * RATE as usize / 2]);
        let shots: Vec<_> = cues
            .iter()
            .map(|&c| audio.one_shot(&Sound::Round(c), &ears()).unwrap())
            .collect();
        audio.with(|m| {
            m.set_wind(0.0);
            for s in shots {
                m.play(s);
            }
        });
        let mut out = vec![0.0; 2 * (secs * RATE as f32) as usize];
        audio.render(&mut out);
        out
    }

    fn peak(x: &[f32]) -> f32 {
        x.iter().fold(0.0f32, |m, x| m.max(x.abs()))
    }

    /// Every cue is clearly audible and under a kill's peak, even the end
    /// horn and sting on top of each other; and each moment has its own patch.
    #[test]
    fn round_cues_are_heard_and_tell_apart() {
        for (cues, name) in [
            (&[RoundCue::Start][..], "start"),
            (&[RoundCue::Count(9)], "tick"),
            (&[RoundCue::Count(2)], "final tick"),
            (&[RoundCue::End, RoundCue::Won], "won"),
            (&[RoundCue::End, RoundCue::Draw], "draw"),
            (&[RoundCue::End, RoundCue::Lost], "lost"),
        ] {
            let p = peak(&heard(cues, 3.0));
            assert!(p > 0.05 && p < 0.9, "{name} peaks at {p}");
        }
        let all = [START, TICK, FINAL_TICK, END, WON, DRAW, LOST];
        for i in 0..all.len() {
            for j in i + 1..all.len() {
                assert!(all[i] != all[j]);
            }
        }
        // The sting waits for the horn: nothing of it in the first moments.
        let early = &heard(&[RoundCue::Won], 0.5);
        assert!(peak(early) < 1e-4);
    }

    /// Writes `docs/audio/round-*.wav`: `cargo test record_round_cues -- --ignored`.
    #[test]
    #[ignore]
    fn record_round_cues() {
        use RoundCue::*;
        for (cues, name, secs) in [
            (&[Start][..], "start", 1.5),
            (&[Count(9)], "tick", 0.5),
            (&[Count(2)], "final-tick", 0.5),
            (&[End, Won], "won", 3.0),
            (&[End, Draw], "draw", 3.0),
            (&[End, Lost], "lost", 3.0),
        ] {
            let path = format!("docs/audio/round-{name}.wav");
            std::fs::write(&path, wav(&heard(cues, secs), 48_000)).unwrap();
        }
    }

    /// Writes `docs/audio/round-finish.wav`: a real match's last eleven
    /// seconds, the countdown into the end horn and a win.
    /// `cargo test record_round_finish -- --ignored`.
    #[test]
    #[ignore]
    fn record_round_finish() {
        let mut game = crate::combat::Match::new(crate::map::Map::reservoir(0), 0, 0, 7);
        game.start();
        game.time_left = 11.0;
        game.fighters[0].kills = 1;
        let out = crate::audio::record(&mut game, 14.0, 48_000, 0.8, |_| {
            crate::sim::Command::default()
        });
        std::fs::write("docs/audio/round-finish.wav", wav(&out, 48_000)).unwrap();
    }
}
