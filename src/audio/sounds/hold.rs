//! The Hold site: taken, lost, contested, and paying out. Heard wherever you
//! are: the site is the round's objective.

use super::Play;
use crate::audio::voice::{Filt, Layer, Src, noise, ring, sine};
use crate::hold::Cue;

pub fn cue(cue: Cue) -> Play {
    Play::everywhere(match cue {
        Cue::Taken => SITE_TAKEN,
        Cue::Lost => SITE_LOST,
        Cue::Contested => SITE_CONTESTED,
        Cue::Paid => SITE_PAID,
    })
}

/// Hold site taken by your side: a bright fanfare climbing a major chord
/// (C, E, G, high C) over a soft swell, ending on a held top note.
pub const SITE_TAKEN: &[Layer] = &[
    Layer {
        attack: 0.08,
        ..sine(262.0, 262.0, 1.0, 0.5, 0.1)
    },
    sine(523.0, 523.0, 1.0, 0.16, 0.18),
    sine(1046.0, 1046.0, 1.0, 0.08, 0.05),
    Layer {
        delay: 0.1,
        ..sine(659.0, 659.0, 1.0, 0.16, 0.18)
    },
    Layer {
        delay: 0.2,
        ..sine(784.0, 784.0, 1.0, 0.18, 0.18)
    },
    Layer {
        delay: 0.3,
        attack: 0.01,
        ..sine(1046.0, 1046.0, 1.0, 0.6, 0.2)
    },
    Layer {
        delay: 0.3,
        attack: 0.01,
        ..sine(2093.0, 2093.0, 1.0, 0.3, 0.05)
    },
];

/// Hold site lost: three low notes falling down a minor chord (G, E flat,
/// C) that sag flat as they go, over a dull thud: the opposite of taking it.
pub const SITE_LOST: &[Layer] = &[
    ring(90.0, 0.25, 0.35),
    noise(Filt::Low, 400.0, 120.0, 0.1, 0.15, 0.3),
    sine(392.0, 380.0, 0.2, 0.18, 0.2),
    Layer {
        delay: 0.14,
        ..sine(311.0, 300.0, 0.2, 0.2, 0.2)
    },
    Layer {
        delay: 0.28,
        attack: 0.01,
        ..sine(262.0, 233.0, 0.5, 0.6, 0.22)
    },
    Layer {
        delay: 0.28,
        attack: 0.01,
        ..sine(131.0, 117.0, 0.5, 0.6, 0.15)
    },
];

/// A two-tone siren note: a buzzy tone (fundamental and odd overtones) with
/// a fast attack, starting `delay` in.
const fn siren(f: f32, delay: f32, gain: f32) -> [Layer; 2] {
    [
        Layer {
            delay,
            attack: 0.01,
            ..sine(f, f, 1.0, 0.12, gain)
        },
        Layer {
            delay,
            attack: 0.01,
            ..sine(f * 3.0, f * 3.0, 1.0, 0.08, gain * 0.3)
        },
    ]
}

/// Hold site contested: an alarm, a hard high-low siren played three times,
/// nothing like the chords for taking or losing it.
pub const SITE_CONTESTED: &[Layer] = &{
    let a = siren(988.0, 0.0, 0.24);
    let b = siren(740.0, 0.13, 0.24);
    let c = siren(988.0, 0.26, 0.24);
    let d = siren(740.0, 0.39, 0.24);
    let e = siren(988.0, 0.52, 0.24);
    let f = siren(740.0, 0.65, 0.2);
    [
        a[0], a[1], b[0], b[1], c[0], c[1], d[0], d[1], e[0], e[1], f[0], f[1],
    ]
};

/// Paid for holding the site: two quick coin clinks (bright inharmonic
/// partials, like struck metal) and a cash-drawer rattle.
pub const SITE_PAID: &[Layer] = &[
    ring(2637.0, 0.08, 0.12),
    ring(3951.0, 0.06, 0.08),
    ring(5870.0, 0.04, 0.04),
    Layer {
        delay: 0.08,
        ..ring(3136.0, 0.12, 0.12)
    },
    Layer {
        delay: 0.08,
        ..ring(4699.0, 0.09, 0.08)
    },
    Layer {
        delay: 0.08,
        ..ring(6980.0, 0.05, 0.04)
    },
    Layer {
        src: Src::Crackle(90.0),
        delay: 0.02,
        ..noise(Filt::High, 4000.0, 4000.0, 0.1, 0.12, 0.12)
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::testkit::{ears, sane};
    use crate::audio::{Audio, wav};
    use crate::combat::Sound;

    #[test]
    fn every_sound_is_sane() {
        sane(&[
            ("site_taken", SITE_TAKEN),
            ("site_lost", SITE_LOST),
            ("site_contested", SITE_CONTESTED),
            ("site_paid", SITE_PAID),
        ]);
    }

    /// Each Hold cue on its own, as heard in the match, `secs` long.
    fn hold_cue(cue: Cue, secs: f32) -> Vec<f32> {
        const RATE: u32 = 48_000;
        let mut audio = Audio::offline(RATE);
        audio.set_volume(crate::profile::Settings::default().volume());
        let shot = audio.one_shot(&Sound::Hold(cue), &ears()).unwrap();
        audio.with(|m| {
            m.set_wind(0.0);
            m.play(shot)
        });
        let mut out = vec![0.0; 2 * (secs * RATE as f32) as usize];
        audio.render(&mut out);
        out
    }

    const CUES: [(Cue, &str); 4] = [
        (Cue::Taken, "taken"),
        (Cue::Lost, "lost"),
        (Cue::Contested, "contested"),
        (Cue::Paid, "paid"),
    ];

    /// Every Hold cue is clearly audible, under a kill's peak, and differs from the others.
    #[test]
    fn hold_cues_are_heard_and_tell_apart() {
        let peaks: Vec<f32> = CUES
            .iter()
            .map(|&(c, _)| hold_cue(c, 1.2).iter().fold(0.0f32, |m, x| m.max(x.abs())))
            .collect();
        for (p, (_, name)) in peaks.iter().zip(CUES) {
            assert!(*p > 0.05 && *p < 0.9, "{name} peaks at {p}");
        }
        let layers = [SITE_TAKEN, SITE_LOST, SITE_CONTESTED, SITE_PAID];
        for i in 0..4 {
            for j in i + 1..4 {
                assert!(layers[i] != layers[j]);
            }
        }
    }

    /// Writes `docs/audio/hold-*.wav`: `cargo test record_hold_cues -- --ignored`.
    #[test]
    #[ignore]
    fn record_hold_cues() {
        for (cue, name) in CUES {
            let path = format!("docs/audio/hold-{name}.wav");
            std::fs::write(&path, wav(&hold_cue(cue, 1.5), 48_000)).unwrap();
        }
    }
}
