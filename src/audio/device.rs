//! The sound card: an output stream on the default device. The mixer lives
//! on its thread; the game reaches it only through an [`Inbox`].

use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use super::mixer::{Inbox, Mixer};

/// Start an output stream on the default device, with its own mixer taking
/// orders from the inbox returned.
pub fn open() -> anyhow::Result<(Arc<Mutex<Inbox>>, cpal::Stream)> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| anyhow::anyhow!("no output device"))?;
    let supported = device.default_output_config()?;
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let channels = config.channels as usize;
    let mixer = Mixer::new(config.sample_rate as f32);
    let inbox = Arc::new(Mutex::new(Inbox::default()));
    let err = |e| log::warn!("audio stream: {e}");
    let stream = match format {
        cpal::SampleFormat::F32 => {
            stream::<f32>(&device, config, mixer, inbox.clone(), channels, err)?
        }
        cpal::SampleFormat::I16 => {
            stream::<i16>(&device, config, mixer, inbox.clone(), channels, err)?
        }
        cpal::SampleFormat::U16 => {
            stream::<u16>(&device, config, mixer, inbox.clone(), channels, err)?
        }
        cpal::SampleFormat::I32 => {
            stream::<i32>(&device, config, mixer, inbox.clone(), channels, err)?
        }
        other => anyhow::bail!("unsupported sample format {other:?}"),
    };
    stream.play()?;
    Ok((inbox, stream))
}

fn stream<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    mut mixer: Mixer,
    inbox: Arc<Mutex<Inbox>>,
    channels: usize,
    err: impl FnMut(cpal::Error) + Send + 'static,
) -> anyhow::Result<cpal::Stream>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    let mut stereo = Vec::new();
    let stream = device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            let frames = data.len() / channels.max(1);
            stereo.resize(frames * 2, 0.0);
            mixer.collect(&inbox);
            mixer.render(&mut stereo);
            for (out, lr) in data.chunks_mut(channels).zip(stereo.chunks_exact(2)) {
                for (c, s) in out.iter_mut().enumerate() {
                    // Mono gets both sides; extra channels (surround) stay quiet.
                    let v = match (channels, c) {
                        (1, _) => (lr[0] + lr[1]) * 0.5,
                        (_, 0) => lr[0],
                        (_, 1) => lr[1],
                        _ => 0.0,
                    };
                    *s = T::from_sample(v);
                }
            }
        },
        err,
        None,
    )?;
    Ok(stream)
}
