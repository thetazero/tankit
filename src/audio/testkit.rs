//! Helpers the audio tests share: a listener, levels, a mixer left to settle.

use glam::Vec3;

use super::dsp::Svf;
use super::mixer::Mixer;
use super::sounds::beam::BurnParams;
use super::tank::TankParams;
use super::voice::{Layer, Voice, length, reference};
use super::{Camera, Listener};

/// At the origin, looking down -Z.
pub fn ears() -> Listener {
    Listener::of(&Camera {
        pos: Vec3::ZERO,
        target: Vec3::NEG_Z,
        fov_y: 1.0,
    })
}

pub fn rms(buf: &[f32]) -> f32 {
    (buf.iter().map(|x| x * x).sum::<f32>() / buf.len() as f32).sqrt()
}

/// Energy of `x` between `lo` and `hi` Hz (two 2-pole filters: a rough band).
pub fn band(x: &[f32], lo: f32, hi: f32, sr: f32) -> f32 {
    let (mut a, mut b) = (Svf::default(), Svf::default());
    x.iter()
        .map(|&s| {
            let y = b.run(a.run(s, lo, 0.7, sr).high, hi, 0.7, sr).low;
            y * y
        })
        .sum()
}

/// Level above 600 Hz, which small speakers actually play.
pub fn mids(buf: &[f32]) -> f32 {
    let mut f = Svf::default();
    let hp: Vec<f32> = buf
        .chunks_exact(2)
        .map(|lr| f.run(lr[0] + lr[1], 600.0, 0.7, 48_000.0).high)
        .collect();
    rms(&hp[hp.len() / 2..])
}

/// A tank close up, driving at `rpm` (0..1).
pub fn driving(rpm: f32) -> TankParams {
    TankParams {
        gain: 1.0,
        air: 16_000.0,
        rpm,
        load: rpm,
        tread: rpm * 10.0,
        ..TankParams::default()
    }
}

/// A second of `m`; the last tenth is returned.
pub fn settle(m: &mut Mixer) -> Vec<f32> {
    let mut buf = vec![0.0; 2 * 4800];
    for _ in 0..10 {
        m.render(&mut buf);
    }
    buf
}

/// One tank and its burn, alone in a 48 kHz mix, settled.
pub fn alone(tank: TankParams, burn: BurnParams) -> Vec<f32> {
    let mut m = Mixer::new(48_000.0);
    m.set_master(1.0);
    m.set_world(1.0);
    m.set_tanks(&[tank]);
    m.set_burns(&[burn]);
    settle(&mut m)
}

/// Every one of `patches` renders finite, audible, bounded, and actually ends.
pub fn sane(patches: &[(&str, &'static [Layer])]) {
    let sr = 48_000.0;
    for &(name, layers) in patches {
        let mut v = Voice::new(layers, 1.0, 1.0, (1.0, 1.0), 1.0, 99, sr);
        let len = length(layers);
        let frames = (len * sr) as usize + 64;
        let mut buf = vec![0.0; frames * 2];
        v.render(&mut buf);
        assert!(v.done(), "{name} never ends");
        let peak = buf.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!(buf.iter().all(|x| x.is_finite()), "{name} not finite");
        assert!(peak > 0.02, "{name} silent: {peak}");
        assert!(peak < 4.0, "{name} too hot: {peak}");
        assert!(len < 12.0, "{name} too long: {len}");
        same_as_reference(name, layers, &buf, sr);
    }
}

/// `played` (stereo, from a [`Voice`] at unit gain and pan, seed 99)
/// sounds like the patch as [`reference`] plays it. Sample for sample over
/// the first 100 ms the quick maths is out by under -70 dB; after that a
/// tone's phase drifts a hair (as the reference's own rounding drifts from
/// the exact maths, by far more), so from there on it's the loudness, every
/// 10 ms, that must agree, to 0.1 dB, down to 40 dB under its loudest.
fn same_as_reference(name: &str, layers: &'static [Layer], played: &[f32], sr: f32) {
    let want = reference(layers, 99, sr, played.len() / 2);
    // The reference stops where the voice ends; `played` runs on a little.
    let got: Vec<f32> = played.iter().step_by(2).take(want.len()).copied().collect();
    let early = (0.1 * sr) as usize;
    let early = early.min(want.len());
    let err: Vec<f32> = want[..early].iter().zip(&got).map(|(a, b)| a - b).collect();
    // (Some patches are silent at first: no error is fine then too.)
    let (err, signal) = (rms(&err), rms(&want[..early]));
    assert!(
        err <= signal * 10f32.powf(-70.0 / 20.0),
        "{name} is {:.1} dB off the reference",
        20.0 * (err / signal).log10()
    );
    let window = (0.01 * sr) as usize;
    let loud = want.chunks(window).map(rms).fold(0.0, f32::max);
    for (i, (w, g)) in want.chunks(window).zip(got.chunks(window)).enumerate() {
        let (w, g) = (rms(w), rms(g));
        // (Far down the tail, layers under -80 dB are skipped: not counted.)
        if w > loud * 1e-2 {
            let db = 20.0 * (g / w).log10();
            assert!(
                db.abs() < 0.1,
                "{name} at {} ms is {db:.2} dB off the reference",
                i * 10
            );
        }
    }
}
