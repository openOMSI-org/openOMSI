//! The microphone: opened only while it is needed, mono at 48 kHz whatever the device does.
//!
//! The stream lives on the thread that made it (some platforms' streams cannot move), so
//! the voice thread opens and closes it. Nothing is recorded: samples go from the device's
//! callback to the encoder through a short queue and are gone.

use crate::codec::RATE;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::Arc;

/// The queue between the device and the voice thread holds at most this much (s); a thread
/// that does not keep up loses the oldest samples rather than the memory.
const QUEUE_SECS: usize = 1;

/// Names of the input devices, for the settings.
pub fn input_devices() -> Vec<String> {
    let host = cpal::default_host();
    host.input_devices().map(|d| d.filter_map(|d| d.name().ok()).collect()).unwrap_or_default()
}

/// Downmix to mono and bring a device's rate to 48 kHz (linear interpolation up, averaging
/// down: speech needs no better).
pub struct Converter {
    ratio: f64,
    phase: f64,
    prev: f32,
    acc: f32,
    acc_n: u32,
    channels: usize,
}

impl Converter {
    pub fn new(rate: u32, channels: usize) -> Converter {
        Converter { ratio: rate.max(1) as f64 / RATE as f64, phase: 0.0, prev: 0.0, acc: 0.0, acc_n: 0, channels: channels.max(1) }
    }

    /// Interleaved samples in, mono 48 kHz samples appended to `out`.
    pub fn feed(&mut self, input: &[f32], out: &mut Vec<f32>) {
        for frame in input.chunks_exact(self.channels) {
            let mono = frame.iter().sum::<f32>() / self.channels as f32;
            if self.ratio > 1.0 {
                // (the device is faster: average the input samples that fall in one output sample)
                self.acc += mono;
                self.acc_n += 1;
                self.phase += 1.0;
                if self.phase >= self.ratio {
                    self.phase -= self.ratio;
                    out.push(self.acc / self.acc_n as f32);
                    self.acc = 0.0;
                    self.acc_n = 0;
                }
            } else {
                // (the device is slower or the same: interpolate between its samples)
                while self.phase < 1.0 {
                    out.push(self.prev + (mono - self.prev) * self.phase as f32);
                    self.phase += self.ratio;
                }
                self.phase -= 1.0;
                self.prev = mono;
            }
        }
    }
}

/// An open microphone.
pub struct Capture {
    _stream: cpal::Stream,
    queue: Arc<Mutex<VecDeque<f32>>>,
    pub device: String,
}

fn build<T: SizedSample + Send + 'static>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    queue: Arc<Mutex<VecDeque<f32>>>,
) -> Result<cpal::Stream, cpal::BuildStreamError>
where
    f32: FromSample<T>,
{
    let mut conv = Converter::new(config.sample_rate.0, config.channels as usize);
    let mut scratch = Vec::new();
    let mut floats = Vec::new();
    device.build_input_stream(
        config,
        move |data: &[T], _| {
            floats.clear();
            floats.extend(data.iter().map(|s| s.to_sample::<f32>()));
            scratch.clear();
            conv.feed(&floats, &mut scratch);
            let mut q = queue.lock();
            q.extend(scratch.iter().copied());
            let cap = RATE as usize * QUEUE_SECS;
            if q.len() > cap {
                let drop = q.len() - cap;
                q.drain(..drop);
            }
        },
        |e| log::warn!("voice: microphone error: {e}"),
        None,
    )
}

impl Capture {
    /// Open the microphone called `name` (empty or not found: the default one).
    pub fn open(name: &str) -> Result<Capture, String> {
        let host = cpal::default_host();
        let device = (!name.is_empty())
            .then(|| host.input_devices().ok().and_then(|mut d| d.find(|d| d.name().ok().as_deref() == Some(name))))
            .flatten()
            .or_else(|| host.default_input_device())
            .ok_or_else(|| "no microphone found".to_string())?;
        let label = device.name().unwrap_or_default();
        let supported = device.default_input_config().map_err(|e| format!("microphone {label} failed: {e}"))?;
        let format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();
        let queue = Arc::new(Mutex::new(VecDeque::new()));
        let q = queue.clone();
        let stream = match format {
            SampleFormat::F32 => build::<f32>(&device, &config, q),
            SampleFormat::I16 => build::<i16>(&device, &config, q),
            SampleFormat::U16 => build::<u16>(&device, &config, q),
            SampleFormat::I32 => build::<i32>(&device, &config, q),
            SampleFormat::I8 => build::<i8>(&device, &config, q),
            other => return Err(format!("microphone {label}: sample format {other:?} is not supported")),
        }
        .map_err(|e| format!("microphone {label} failed: {e}"))?;
        stream.play().map_err(|e| format!("microphone {label} failed: {e}"))?;
        log::info!("voice: microphone {label} open ({} Hz, {} channel(s))", config.sample_rate.0, config.channels);
        Ok(Capture { _stream: stream, queue, device: label })
    }

    /// Everything captured since the last call (mono, 48 kHz).
    pub fn take(&self, out: &mut Vec<f32>) {
        out.extend(self.queue.lock().drain(..));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rates_are_brought_to_48k_and_stereo_to_mono() {
        for (rate, channels) in [(48_000u32, 1usize), (44_100, 2), (16_000, 1), (96_000, 2), (22_050, 1)] {
            let mut c = Converter::new(rate, channels);
            let n = rate as usize; // one second
            let input: Vec<f32> = (0..n * channels).map(|i| 0.4 * (2.0 * std::f32::consts::PI * 440.0 * (i / channels) as f32 / rate as f32).sin()).collect();
            let mut out = Vec::new();
            for chunk in input.chunks(480 * channels) {
                c.feed(chunk, &mut out);
            }
            assert!((out.len() as i64 - 48_000).abs() <= 4, "{rate} Hz x{channels}: {} samples", out.len());
            // the tone is still a tone at the same pitch: count zero crossings
            let crossings = out.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
            assert!((crossings as i64 - 440).abs() <= 3, "{rate} Hz x{channels}: {crossings} cycles");
            let peak = out.iter().fold(0f32, |m, v| m.max(v.abs()));
            assert!(peak > 0.3 && peak < 0.45, "{rate} Hz x{channels}: peak {peak}");
        }
    }
}
