//! One-shot sounds: each a stack of short synth layers (a swept oscillator or
//! filtered noise under a decay envelope). Shots, impacts, hits, blasts, UI.

use std::f32::consts::TAU;

use super::dsp::{Noise, OnePole, Svf};

#[derive(Clone, Copy, PartialEq)]
pub enum Src {
    Sine,
    Noise,
    /// Random clicks, about this many per second (debris, fire crackle).
    Crackle(f32),
}

#[derive(Clone, Copy, PartialEq)]
pub enum Filt {
    Off,
    Low,
    Band,
    High,
}

/// One layer of a one-shot.
#[derive(Clone, Copy, PartialEq)]
pub struct Layer {
    pub src: Src,
    /// Oscillator pitch, start and end, Hz.
    pub freq: (f32, f32),
    /// Time constant of the pitch glide, seconds.
    pub glide: f32,
    pub filt: Filt,
    /// Filter cutoff, start and end, Hz.
    pub cutoff: (f32, f32),
    /// Time constant of the cutoff sweep, seconds.
    pub sweep: f32,
    pub q: f32,
    /// Linear rise, seconds.
    pub attack: f32,
    /// Exponential decay time constant, seconds.
    pub decay: f32,
    pub gain: f32,
    /// Start offset, seconds.
    pub delay: f32,
}

const L: Layer = Layer {
    src: Src::Sine,
    freq: (440.0, 440.0),
    glide: 0.1,
    filt: Filt::Off,
    cutoff: (1000.0, 1000.0),
    sweep: 0.1,
    q: 0.7,
    attack: 0.001,
    decay: 0.1,
    gain: 1.0,
    delay: 0.0,
};

const fn sine(f0: f32, f1: f32, glide: f32, decay: f32, gain: f32) -> Layer {
    Layer {
        freq: (f0, f1),
        glide,
        decay,
        gain,
        ..L
    }
}

const fn noise(filt: Filt, c0: f32, c1: f32, sweep: f32, decay: f32, gain: f32) -> Layer {
    Layer {
        src: Src::Noise,
        filt,
        cutoff: (c0, c1),
        sweep,
        decay,
        gain,
        ..L
    }
}

/// A metal partial (inharmonic sines make a struck plate).
const fn ring(f: f32, decay: f32, gain: f32) -> Layer {
    sine(f, f * 0.98, 0.4, decay, gain)
}

/// Imperium rail cannon: a heavy gun's report. A falling slam (with its
/// overtones, so small speakers still hear the drop), a whump of blast that
/// closes fast, only milliseconds of crack, then a long rolling rumble and its
/// echo. Kept out of the 1-5 kHz band where a snare drum's wires live.
pub const KONTOS: &[Layer] = &[
    sine(110.0, 38.0, 0.07, 0.3, 0.5),
    sine(220.0, 76.0, 0.07, 0.18, 0.35),
    sine(330.0, 114.0, 0.07, 0.1, 0.2),
    noise(Filt::Low, 3000.0, 380.0, 0.04, 0.3, 1.3),
    Layer {
        q: 0.6,
        ..noise(Filt::Band, 700.0, 300.0, 0.15, 0.35, 0.9)
    },
    noise(Filt::High, 1800.0, 1800.0, 0.1, 0.006, 0.35),
    Layer {
        attack: 0.02,
        ..noise(Filt::Low, 650.0, 180.0, 0.5, 0.9, 0.8)
    },
    Layer {
        delay: 0.18,
        attack: 0.03,
        ..noise(Filt::Low, 380.0, 120.0, 0.4, 0.7, 0.25)
    },
    sine(2600.0, 800.0, 0.08, 0.08, 0.05),
];

/// Imperium twin autocannon: the same report, shorter and punchier.
pub const PELEKYS: &[Layer] = &[
    sine(130.0, 48.0, 0.05, 0.14, 0.5),
    sine(260.0, 96.0, 0.05, 0.08, 0.3),
    noise(Filt::Low, 3200.0, 420.0, 0.03, 0.15, 1.2),
    Layer {
        q: 0.6,
        ..noise(Filt::Band, 800.0, 350.0, 0.08, 0.16, 0.7)
    },
    noise(Filt::High, 2200.0, 2200.0, 0.1, 0.005, 0.3),
    Layer {
        attack: 0.01,
        ..noise(Filt::Low, 600.0, 170.0, 0.3, 0.4, 0.5)
    },
];

/// Sorn orb launcher: a falling, hollow "thoom".
pub const WISP: &[Layer] = &[
    sine(900.0, 170.0, 0.12, 0.25, 0.42),
    sine(1350.0, 255.0, 0.12, 0.18, 0.18),
    Layer {
        q: 3.0,
        ..noise(Filt::Band, 1800.0, 500.0, 0.1, 0.17, 0.3)
    },
    sine(75.0, 60.0, 0.1, 0.15, 0.4),
];

/// Sorn three-barrel: quick plasma darts.
pub const TRIAD: &[Layer] = &[
    sine(1400.0, 520.0, 0.05, 0.07, 0.28),
    Layer {
        q: 4.0,
        ..noise(Filt::Band, 2600.0, 900.0, 0.05, 0.05, 0.18)
    },
    sine(110.0, 80.0, 0.05, 0.05, 0.2),
];

/// A shell landing on a wall or the ground: thud and debris.
pub const IMPACT: &[Layer] = &[
    sine(95.0, 45.0, 0.05, 0.1, 0.6),
    noise(Filt::Low, 2600.0, 500.0, 0.06, 0.1, 0.55),
    Layer {
        src: Src::Crackle(380.0),
        filt: Filt::Band,
        cutoff: (3000.0, 2200.0),
        q: 1.5,
        decay: 0.22,
        gain: 0.35,
        delay: 0.02,
        ..L
    },
];

/// Plasma splashing on a surface: a fizz.
pub const SPLASH: &[Layer] = &[
    Layer {
        q: 2.0,
        ..noise(Filt::Band, 3200.0, 800.0, 0.08, 0.15, 0.4)
    },
    sine(600.0, 150.0, 0.08, 0.12, 0.28),
];

/// An orb skipping off a wall.
pub const BOUNCE: &[Layer] = &[
    sine(1800.0, 700.0, 0.08, 0.12, 0.24),
    noise(Filt::High, 4000.0, 4000.0, 0.1, 0.02, 0.18),
];

/// A round striking armour: one crunch. All noise, no pitched ring: any
/// tone that hangs on, however low, reads as a ping off a can.
pub const HIT: &[Layer] = &[
    // A thump too short to have a pitch.
    sine(110.0, 45.0, 0.03, 0.06, 0.6),
    // The blow: low-passed noise that closes fast.
    noise(Filt::Low, 1800.0, 250.0, 0.04, 0.1, 1.3),
    Layer {
        q: 0.6,
        ..noise(Filt::Band, 500.0, 250.0, 0.08, 0.12, 0.8)
    },
    // The crunch: a dense burst of grinding clicks.
    Layer {
        src: Src::Crackle(2500.0),
        filt: Filt::Band,
        cutoff: (1200.0, 600.0),
        sweep: 0.08,
        q: 0.8,
        decay: 0.09,
        gain: 0.5,
        ..L
    },
    // Grit settling after it.
    Layer {
        src: Src::Crackle(600.0),
        filt: Filt::Band,
        cutoff: (2000.0, 900.0),
        q: 1.0,
        decay: 0.18,
        gain: 0.3,
        delay: 0.01,
        ..L
    },
];

/// A tank blowing up.
pub const KILL: &[Layer] = &[
    sine(85.0, 28.0, 0.3, 0.5, 1.0),
    noise(Filt::Low, 6000.0, 200.0, 0.25, 0.55, 0.9),
    Layer {
        src: Src::Crackle(140.0),
        filt: Filt::Band,
        cutoff: (1600.0, 900.0),
        sweep: 0.6,
        q: 1.0,
        decay: 0.8,
        gain: 0.55,
        delay: 0.05,
        ..L
    },
    Layer {
        attack: 0.05,
        ..noise(Filt::Low, 320.0, 80.0, 1.0, 1.3, 0.5)
    },
    Layer {
        delay: 0.08,
        ..sine(230.0, 150.0, 0.6, 0.55, 0.12)
    },
];

/// Hull mended at a repair crate.
pub const REPAIR: &[Layer] = &[
    sine(660.0, 660.0, 1.0, 0.25, 0.22),
    Layer {
        delay: 0.09,
        ..sine(990.0, 990.0, 1.0, 0.35, 0.22)
    },
    Layer {
        delay: 0.18,
        ..sine(1320.0, 1320.0, 1.0, 0.5, 0.18)
    },
    ring(140.0, 0.12, 0.3),
];

/// The beam lights: a zap that leaps up in pitch out of a crack of static
/// and a thump of the emitter charging.
pub const BEAM_ON: &[Layer] = &[
    sine(260.0, 1500.0, 0.04, 0.1, 0.3),
    sine(520.0, 3000.0, 0.04, 0.05, 0.1),
    noise(Filt::High, 2600.0, 2600.0, 0.1, 0.02, 0.6),
    Layer {
        src: Src::Crackle(900.0),
        filt: Filt::Band,
        cutoff: (3500.0, 2200.0),
        sweep: 0.08,
        q: 1.2,
        decay: 0.09,
        gain: 0.45,
        ..L
    },
    sine(95.0, 55.0, 0.05, 0.08, 0.4),
];

/// Trigger let go: the beam winds down, a falling zap and a fizz.
pub const BEAM_OFF: &[Layer] = &[
    sine(1300.0, 200.0, 0.06, 0.14, 0.22),
    Layer {
        q: 1.5,
        ..noise(Filt::Band, 3000.0, 700.0, 0.08, 0.1, 0.3)
    },
];

/// A beam strikes a hull: hit as hard as a shell (the blow, a crunch), then
/// the plate sizzles and pings as it takes the heat.
pub const SCORCH: &[Layer] = &[
    sine(120.0, 50.0, 0.03, 0.06, 0.5),
    noise(Filt::Low, 2200.0, 300.0, 0.04, 0.09, 1.0),
    Layer {
        src: Src::Crackle(1800.0),
        filt: Filt::Band,
        cutoff: (2400.0, 1200.0),
        sweep: 0.1,
        q: 0.9,
        decay: 0.12,
        gain: 0.35,
        ..L
    },
    ring(1470.0, 0.3, 0.12),
    ring(2290.0, 0.22, 0.08),
    ring(3720.0, 0.15, 0.05),
    Layer {
        attack: 0.015,
        ..noise(Filt::High, 3500.0, 3500.0, 0.1, 0.25, 0.25)
    },
];

/// The beam locks out: a steam hiss as the emitter powers down.
pub const OVERHEAT: &[Layer] = &[
    Layer {
        attack: 0.01,
        ..noise(Filt::High, 1500.0, 4500.0, 0.2, 0.5, 0.3)
    },
    sine(320.0, 110.0, 0.2, 0.25, 0.22),
];

/// Your shot killed someone.
pub const CONFIRM: &[Layer] = &[
    sine(880.0, 880.0, 1.0, 0.06, 0.16),
    Layer {
        delay: 0.07,
        ..sine(1320.0, 1320.0, 1.0, 0.12, 0.16)
    },
];

/// Remote link re-established in a fresh chassis.
pub const LINK: &[Layer] = &[
    Layer {
        attack: 0.05,
        ..sine(220.0, 440.0, 0.15, 0.3, 0.12)
    },
    Layer {
        attack: 0.05,
        delay: 0.05,
        ..sine(330.0, 660.0, 0.15, 0.3, 0.08)
    },
];

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

/// Menu cursor moved.
pub const TICK: &[Layer] = &[
    sine(1800.0, 1800.0, 1.0, 0.015, 0.09),
    noise(Filt::High, 5000.0, 5000.0, 0.1, 0.004, 0.05),
];

/// Menu item chosen.
pub const SELECT: &[Layer] = &[
    sine(1100.0, 1600.0, 0.03, 0.06, 0.12),
    sine(2200.0, 3200.0, 0.03, 0.03, 0.04),
];

/// Imperium arrival, sent down from orbit: a roar falling out of the sky
/// down the lance (0.4 s), the slam of touchdown, and a brass chord as the
/// emblem burns on the ground.
pub const DROP_IMPERIUM: &[Layer] = &[
    Layer {
        attack: 0.3,
        q: 1.5,
        ..noise(Filt::Band, 4500.0, 600.0, 0.2, 0.08, 0.35)
    },
    Layer {
        attack: 0.35,
        ..sine(1600.0, 300.0, 0.18, 0.05, 0.1)
    },
    Layer {
        delay: 0.4,
        ..sine(90.0, 30.0, 0.12, 0.45, 0.7)
    },
    Layer {
        delay: 0.4,
        ..noise(Filt::Low, 5000.0, 180.0, 0.18, 0.5, 0.55)
    },
    Layer {
        delay: 0.4,
        attack: 0.04,
        ..noise(Filt::Low, 400.0, 120.0, 0.6, 0.8, 0.35)
    },
    // The emblem's chord: a fifth and an octave, brassy with a little edge.
    Layer {
        delay: 0.5,
        attack: 0.08,
        ..sine(220.0, 220.0, 1.0, 0.5, 0.12)
    },
    Layer {
        delay: 0.5,
        attack: 0.08,
        ..sine(330.0, 330.0, 1.0, 0.45, 0.1)
    },
    Layer {
        delay: 0.5,
        attack: 0.08,
        ..sine(440.0, 440.0, 1.0, 0.4, 0.08)
    },
    Layer {
        delay: 0.5,
        attack: 0.08,
        q: 2.0,
        ..noise(Filt::Band, 1300.0, 1300.0, 0.1, 0.35, 0.05)
    },
];

/// Resonant arrival, the vessel kindling: a glassy shimmer gathering as the
/// motes spiral in, then a bell's toll at 0.7 s and its echo 0.3 s later.
pub const KINDLE_RESONANT: &[Layer] = &[
    Layer {
        attack: 0.6,
        ..sine(2400.0, 3600.0, 0.5, 0.06, 0.05)
    },
    Layer {
        attack: 0.6,
        ..sine(3100.0, 4700.0, 0.5, 0.06, 0.035)
    },
    Layer {
        attack: 0.6,
        q: 6.0,
        ..noise(Filt::Band, 5000.0, 7000.0, 0.5, 0.06, 0.06)
    },
    // The toll: a church bell's partials (hum, prime, minor third, fifth, nominal).
    Layer {
        delay: 0.7,
        ..ring(196.0, 1.3, 0.3)
    },
    Layer {
        delay: 0.7,
        ..ring(392.0, 1.0, 0.3)
    },
    Layer {
        delay: 0.7,
        ..ring(470.0, 0.8, 0.18)
    },
    Layer {
        delay: 0.7,
        ..ring(588.0, 0.6, 0.14)
    },
    Layer {
        delay: 0.7,
        ..ring(784.0, 0.45, 0.12)
    },
    Layer {
        delay: 0.7,
        ..ring(1140.0, 0.25, 0.08)
    },
    Layer {
        delay: 0.7,
        ..noise(Filt::High, 3000.0, 3000.0, 0.1, 0.01, 0.25)
    },
    // The echo, softer and duller.
    Layer {
        delay: 1.0,
        ..ring(196.0, 1.0, 0.12)
    },
    Layer {
        delay: 1.0,
        ..ring(392.0, 0.8, 0.12)
    },
    Layer {
        delay: 1.0,
        ..ring(588.0, 0.4, 0.05)
    },
];

/// The Vigil's bell, a tone under the arrival's: hum, prime, minor third,
/// fifth, nominal, twelfth, Hz.
const VIGIL_BELL: [f32; 6] = [147.0, 294.0, 353.0, 441.0, 588.0, 882.0];

/// A Vigil breaking: the crystal shell cracks (a sharp snap, a shower of
/// glass, a few high crystal rings), the light rushes out with a rising
/// breath, and the bell is struck hard and left to ring.
pub const SHATTER: &[Layer] = &[
    // The snap, and the blow that cracked it.
    noise(Filt::High, 3200.0, 3200.0, 0.1, 0.012, 0.7),
    sine(95.0, 42.0, 0.08, 0.2, 0.55),
    noise(Filt::Low, 1600.0, 220.0, 0.06, 0.18, 0.5),
    // Glass: a shower of tinkling shards and the crystal singing as it splits.
    Layer {
        src: Src::Crackle(900.0),
        filt: Filt::Band,
        cutoff: (5200.0, 3400.0),
        sweep: 0.3,
        q: 2.5,
        decay: 0.35,
        gain: 0.4,
        delay: 0.01,
        ..L
    },
    sine(2870.0, 2850.0, 0.5, 0.3, 0.07),
    sine(3910.0, 3880.0, 0.5, 0.22, 0.05),
    sine(5230.0, 5190.0, 0.5, 0.15, 0.04),
    // The light pouring out: a breath of air rising through the cracks.
    Layer {
        attack: 0.08,
        q: 3.0,
        ..noise(Filt::Band, 1800.0, 6500.0, 0.4, 0.45, 0.22)
    },
    // The bell, struck hard.
    Layer {
        delay: 0.03,
        ..ring(VIGIL_BELL[0], 1.5, 0.32)
    },
    Layer {
        delay: 0.03,
        ..ring(VIGIL_BELL[1], 1.3, 0.32)
    },
    Layer {
        delay: 0.03,
        ..ring(VIGIL_BELL[2], 1.0, 0.2)
    },
    Layer {
        delay: 0.03,
        ..ring(VIGIL_BELL[3], 0.8, 0.15)
    },
    Layer {
        delay: 0.03,
        ..ring(VIGIL_BELL[4], 0.6, 0.13)
    },
    Layer {
        delay: 0.03,
        ..ring(VIGIL_BELL[5], 0.35, 0.08)
    },
];

/// A husk's bell tolling while it runs on. The shell is cracked, so the bell
/// is too: its prime has a split twin a few Hz off, and the two beat.
pub const TOLL: &[Layer] = &[
    ring(VIGIL_BELL[0], 1.1, 0.24),
    ring(VIGIL_BELL[1], 0.9, 0.2),
    ring(VIGIL_BELL[1] + 3.5, 0.9, 0.13),
    ring(VIGIL_BELL[2], 0.7, 0.13),
    ring(VIGIL_BELL[3], 0.55, 0.1),
    ring(VIGIL_BELL[4], 0.4, 0.08),
    // The clapper, and a glint of crystal answering it.
    noise(Filt::High, 3000.0, 3000.0, 0.1, 0.008, 0.18),
    sine(2870.0, 2850.0, 0.5, 0.25, 0.03),
];

/// A husk going out: one last toll that chokes off, its partials sagging, as
/// the light drains away in a falling shimmer and the crystal falls in on
/// itself. No explosion: it doesn't burn, it goes dark.
pub const GUTTER: &[Layer] = &[
    // The choked toll.
    sine(VIGIL_BELL[0], VIGIL_BELL[0] * 0.75, 0.7, 0.8, 0.26),
    sine(VIGIL_BELL[1], VIGIL_BELL[1] * 0.75, 0.6, 0.55, 0.22),
    sine(VIGIL_BELL[2], VIGIL_BELL[2] * 0.75, 0.5, 0.35, 0.12),
    sine(VIGIL_BELL[3], VIGIL_BELL[3] * 0.75, 0.4, 0.25, 0.08),
    noise(Filt::High, 3000.0, 3000.0, 0.1, 0.008, 0.15),
    // The light draining: a shimmer falling from bright to nothing.
    Layer {
        attack: 0.05,
        ..sine(3400.0, 380.0, 0.45, 0.5, 0.07)
    },
    Layer {
        attack: 0.05,
        ..sine(4450.0, 500.0, 0.45, 0.45, 0.045)
    },
    Layer {
        attack: 0.05,
        q: 4.0,
        ..noise(Filt::Band, 6000.0, 900.0, 0.4, 0.5, 0.12)
    },
    // The crystal falling in, and the dead hull settling.
    Layer {
        src: Src::Crackle(500.0),
        filt: Filt::Band,
        cutoff: (4200.0, 1500.0),
        sweep: 0.4,
        q: 2.0,
        decay: 0.4,
        gain: 0.35,
        delay: 0.08,
        ..L
    },
    Layer {
        delay: 0.1,
        ..sine(70.0, 34.0, 0.12, 0.3, 0.5)
    },
    Layer {
        delay: 0.1,
        ..noise(Filt::Low, 900.0, 150.0, 0.2, 0.35, 0.4)
    },
];

/// Uthenn arrival, the hull made: a laser frame's hum, tungsten cubes
/// clunking into the volume, and the scan plane's rising sweep (0.9 s).
pub const BUILD_UTHENN: &[Layer] = &[
    Layer {
        attack: 0.05,
        ..sine(120.0, 120.0, 1.0, 0.5, 0.12)
    },
    Layer {
        attack: 0.05,
        q: 8.0,
        ..noise(Filt::Band, 240.0, 240.0, 1.0, 0.45, 0.12)
    },
    // Cubes landing: dense metal blocks, a dull knock with a short ring.
    Layer {
        delay: 0.1,
        ..sine(180.0, 90.0, 0.03, 0.06, 0.45)
    },
    Layer {
        delay: 0.1,
        ..ring(1900.0, 0.05, 0.08)
    },
    Layer {
        delay: 0.19,
        ..sine(210.0, 100.0, 0.03, 0.05, 0.35)
    },
    Layer {
        delay: 0.19,
        ..ring(2300.0, 0.04, 0.07)
    },
    Layer {
        delay: 0.26,
        ..sine(160.0, 80.0, 0.03, 0.06, 0.4)
    },
    Layer {
        delay: 0.33,
        ..sine(200.0, 95.0, 0.03, 0.05, 0.35)
    },
    Layer {
        delay: 0.33,
        ..ring(2100.0, 0.04, 0.07)
    },
    Layer {
        delay: 0.41,
        ..sine(170.0, 85.0, 0.03, 0.06, 0.4)
    },
    // The scan plane rising up through the hull.
    Layer {
        attack: 0.8,
        ..sine(300.0, 1500.0, 0.6, 0.08, 0.1)
    },
    Layer {
        attack: 0.8,
        ..sine(600.0, 3000.0, 0.6, 0.08, 0.04)
    },
    // Finished: a clean two-note chirp.
    Layer {
        delay: 0.92,
        ..sine(1500.0, 1500.0, 1.0, 0.05, 0.1)
    },
    Layer {
        delay: 1.0,
        ..sine(2000.0, 2000.0, 1.0, 0.08, 0.1)
    },
];

/// Ithessar arrival, lowered on plasma: the orb opening with a rising hum,
/// the stream's fizz as the tank floats down, then the orb's burst at 0.9 s.
pub const DESCEND_ITHESSAR: &[Layer] = &[
    Layer {
        attack: 0.3,
        ..sine(110.0, 220.0, 0.3, 0.5, 0.25)
    },
    Layer {
        attack: 0.3,
        ..sine(330.0, 450.0, 0.3, 0.4, 0.08)
    },
    Layer {
        attack: 0.4,
        q: 2.5,
        ..noise(Filt::Band, 1800.0, 2600.0, 0.5, 0.35, 0.12)
    },
    // The burst: a pop and a splash of droplets.
    Layer {
        delay: 0.9,
        ..sine(700.0, 140.0, 0.06, 0.15, 0.45)
    },
    Layer {
        delay: 0.9,
        q: 1.5,
        ..noise(Filt::Band, 4000.0, 900.0, 0.1, 0.2, 0.45)
    },
    Layer {
        src: Src::Crackle(220.0),
        filt: Filt::Band,
        cutoff: (5000.0, 2500.0),
        sweep: 0.3,
        q: 2.0,
        decay: 0.18,
        gain: 0.3,
        delay: 0.95,
        ..L
    },
];

#[derive(Clone, Copy, Default)]
struct LayerState {
    phase: f32,
    svf: Svf,
}

/// A one-shot playing.
pub struct Voice {
    layers: &'static [Layer],
    state: Vec<LayerState>,
    noise: Noise,
    /// Samples played.
    n: u32,
    t: f32,
    len: f32,
    pitch: f32,
    pub gain: f32,
    left: f32,
    right: f32,
    air: OnePole,
    air_coef: f32,
}

impl Voice {
    /// `pitch` scales every frequency; `air_coef` is the distance lowpass.
    pub fn new(
        layers: &'static [Layer],
        pitch: f32,
        gain: f32,
        pan: (f32, f32),
        air_coef: f32,
        seed: u32,
    ) -> Voice {
        Voice {
            layers,
            state: vec![LayerState::default(); layers.len()],
            noise: Noise::new(seed),
            n: 0,
            t: 0.0,
            len: length(layers),
            pitch,
            gain,
            left: pan.0,
            right: pan.1,
            air: OnePole::default(),
            air_coef,
        }
    }

    pub fn done(&self) -> bool {
        self.t >= self.len
    }

    /// How loud it still is, roughly (for dropping the quietest when crowded).
    pub fn level(&self) -> f32 {
        self.gain * (1.0 - self.t / self.len).max(0.0)
    }

    /// Add the next `out.len() / 2` stereo frames into `out`.
    pub fn render(&mut self, out: &mut [f32], sr: f32) {
        for frame in out.chunks_exact_mut(2) {
            if self.t >= self.len {
                return;
            }
            let mut sum = 0.0;
            for (l, s) in self.layers.iter().zip(&mut self.state) {
                sum += layer(l, s, &mut self.noise, self.t, self.pitch, sr);
            }
            let x = self.air.run(sum, self.air_coef) * self.gain;
            frame[0] += x * self.left;
            frame[1] += x * self.right;
            // Counted, not summed: adding 1/sr drifts over a long tail.
            self.n += 1;
            self.t = self.n as f32 / sr;
        }
    }
}

/// Seconds until every layer has died away (to -60 dB).
pub fn length(layers: &[Layer]) -> f32 {
    layers
        .iter()
        .map(|l| l.delay + l.attack + l.decay * 6.9)
        .fold(0.0, f32::max)
}

fn layer(l: &Layer, s: &mut LayerState, noise: &mut Noise, t: f32, pitch: f32, sr: f32) -> f32 {
    if t < l.delay {
        return 0.0;
    }
    let t = t - l.delay;
    let env = if t < l.attack {
        t / l.attack
    } else {
        (-(t - l.attack) / l.decay).exp()
    };
    let glide = |(a, b): (f32, f32), tau: f32| b + (a - b) * (-t / tau).exp();
    let x = match l.src {
        Src::Sine => {
            s.phase = (s.phase + glide(l.freq, l.glide) * pitch / sr).fract();
            (s.phase * TAU).sin()
        }
        Src::Noise => noise.next(),
        Src::Crackle(rate) => {
            if noise.unit() < rate / sr {
                noise.next() * 3.0
            } else {
                0.0
            }
        }
    };
    let y = match l.filt {
        Filt::Off => x,
        f => {
            let o = s.svf.run(x, glide(l.cutoff, l.sweep) * pitch, l.q, sr);
            match f {
                Filt::Low => o.low,
                Filt::Band => o.band,
                _ => o.high,
            }
        }
    };
    y * env * l.gain
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [&[Layer]; 25] = [
        KONTOS,
        PELEKYS,
        WISP,
        TRIAD,
        IMPACT,
        SPLASH,
        BOUNCE,
        HIT,
        SCORCH,
        BEAM_ON,
        BEAM_OFF,
        KILL,
        REPAIR,
        OVERHEAT,
        CONFIRM,
        LINK,
        TICK,
        SELECT,
        DROP_IMPERIUM,
        KINDLE_RESONANT,
        BUILD_UTHENN,
        DESCEND_ITHESSAR,
        SHATTER,
        TOLL,
        GUTTER,
    ];

    /// Every sound renders finite, audible, bounded, and actually ends.
    #[test]
    fn every_sound_is_sane() {
        let sr = 48_000.0;
        for (i, layers) in ALL.iter().enumerate() {
            let mut v = Voice::new(layers, 1.0, 1.0, (1.0, 1.0), 1.0, 99);
            let frames = (v.len * sr) as usize + 64;
            let mut buf = vec![0.0; frames * 2];
            v.render(&mut buf, sr);
            assert!(v.done(), "sound {i} never ends");
            let peak = buf.iter().fold(0.0f32, |m, x| m.max(x.abs()));
            assert!(buf.iter().all(|x| x.is_finite()), "sound {i} not finite");
            assert!(peak > 0.02, "sound {i} silent: {peak}");
            assert!(peak < 4.0, "sound {i} too hot: {peak}");
            assert!(v.len < 12.0, "sound {i} too long: {}", v.len);
        }
    }

    /// Energy of `x` between `lo` and `hi` Hz (two 2-pole filters: a rough band).
    fn band(x: &[f32], lo: f32, hi: f32, sr: f32) -> f32 {
        let (mut a, mut b) = (Svf::default(), Svf::default());
        x.iter()
            .map(|&s| {
                let y = b.run(a.run(s, lo, 0.7, sr).high, hi, 0.7, sr).low;
                y * y
            })
            .sum()
    }

    /// Issue #77: the Imperium guns sounded like a snare drum. Their slam sat
    /// below what small speakers play, leaving a burst of 1-5 kHz noise. A
    /// cannon's audible body is low-mid, and its report rolls on.
    #[test]
    fn imperium_guns_boom_not_snap() {
        let sr = 48_000.0;
        for (name, layers) in [("kontos", KONTOS), ("pelekys", PELEKYS)] {
            let mut v = Voice::new(layers, 1.0, 1.0, (1.0, 0.0), 1.0, 7);
            let mut buf = vec![0.0; 2 * (v.len * sr) as usize];
            v.render(&mut buf, sr);
            let x: Vec<f32> = buf.iter().step_by(2).copied().collect();
            let body = band(&x, 150.0, 900.0, sr);
            let snare = band(&x, 1500.0, 6000.0, sr);
            // The old patches scored 0.9 and 2.7 here.
            assert!(
                body > 4.0 * snare,
                "{name}: body {body:.2}, snare {snare:.2}"
            );
        }
        // The main gun's report rolls on (the old one was 0.05 here).
        let mut v = Voice::new(KONTOS, 1.0, 1.0, (1.0, 0.0), 1.0, 7);
        let mut buf = vec![0.0; 2 * (v.len * sr) as usize];
        v.render(&mut buf, sr);
        let x: Vec<f32> = buf.iter().step_by(2).copied().collect();
        let tail = band(&x[(0.25 * sr) as usize..], 150.0, 900.0, sr);
        let body = band(&x, 150.0, 900.0, sr);
        assert!(tail > 0.08 * body, "kontos stops dead: {:.3}", tail / body);
    }

    /// Getting hit sounded like a tin can: long bright partials above 1 kHz.
    /// A round hitting thick armour is a low crunch that dies quickly.
    #[test]
    fn hits_crunch_not_ping() {
        let sr = 48_000.0;
        let mut v = Voice::new(HIT, 1.0, 1.0, (1.0, 0.0), 1.0, 7);
        let mut buf = vec![0.0; 2 * (v.len * sr) as usize];
        v.render(&mut buf, sr);
        let x: Vec<f32> = buf.iter().step_by(2).copied().collect();
        let body = band(&x, 100.0, 900.0, sr);
        let ping = band(&x, 1000.0, 6000.0, sr);
        assert!(body > 3.0 * ping, "body {body:.2}, ping {ping:.2}");
        // No bright ring hanging on after the strike.
        let tail = band(&x[(0.15 * sr) as usize..], 1000.0, 6000.0, sr);
        assert!(tail < 0.02 * ping, "rings on: {:.3}", tail / ping);
        // A crunch, not a ping: no pitched layer lasts long enough to hear
        // as a note.
        for l in HIT {
            if l.src == Src::Sine {
                assert!(l.decay <= 0.06, "a tone rings for {} s", l.decay);
            }
        }
    }

    #[test]
    fn delayed_layers_start_silent() {
        let mut v = Voice::new(CONFIRM, 1.0, 1.0, (1.0, 1.0), 1.0, 1);
        let mut buf = vec![0.0; 2 * 10];
        v.render(&mut buf, 48_000.0);
        // Only the first blip has started; the second waits 70 ms.
        assert!(buf.iter().all(|x| x.abs() < 0.2));
    }
}
