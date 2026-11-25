//! The output device's thread: it owns the cpal stream (on some platforms a stream may not
//! leave the thread that made it, and the engine must be free to move to the simulation's
//! thread), opens it again when asked, and looks every two seconds whether the system's
//! default output changed.

use super::render::{MixerCell, Reverb, Shared};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// The engine's end of the device thread.
pub(super) struct DeviceThread {
    open: Option<mpsc::Sender<()>>,
    opened: mpsc::Receiver<Option<String>>,
    join: Option<std::thread::JoinHandle<()>>,
}

impl DeviceThread {
    /// The thread, with the stream opened on the default device: `None` when no thread
    /// could be started.
    pub(super) fn start(
        shared: Arc<Shared>,
        mixer: Arc<MixerCell>,
        reopen: Arc<AtomicBool>,
    ) -> Option<DeviceThread> {
        let (open_tx, open_rx) = mpsc::channel::<()>();
        let (done_tx, done_rx) = mpsc::channel();
        let join = std::thread::Builder::new()
            .name("audio device".into())
            .spawn(move || run(open_rx, done_tx, &shared, &mixer, &reopen))
            .ok()?;
        Some(DeviceThread {
            open: Some(open_tx),
            opened: done_rx,
            join: Some(join),
        })
    }

    /// Play on the system's default output device from now on: the name of the device
    /// played on, `None` when no stream plays.
    pub(super) fn open(&self) -> Option<String> {
        self.open.as_ref()?.send(()).ok()?;
        self.opened.recv().ok().flatten()
    }
}

impl Drop for DeviceThread {
    /// The stream is closed before the engine is gone (playback stops when it is dropped).
    fn drop(&mut self) {
        self.open.take();
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

fn run(
    open: mpsc::Receiver<()>,
    opened: mpsc::Sender<Option<String>>,
    shared: &Arc<Shared>,
    mixer: &Arc<MixerCell>,
    reopen: &Arc<AtomicBool>,
) {
    let mut stream: Option<cpal::Stream> = None;
    // The default device's name the watcher saw last (from the first stream on; no stream
    // at the start, no watching).
    let mut watched: Option<String> = None;
    let mut next_look = Instant::now() + Duration::from_secs(2);
    loop {
        match open.recv_timeout(next_look.saturating_duration_since(Instant::now())) {
            Ok(()) => {
                // (the old stream first: some drivers give a device to one stream at a time)
                stream = None;
                let name = open_default(shared, mixer, reopen).map(|(s, name)| {
                    stream = Some(s);
                    name
                });
                if watched.is_none() {
                    if let Some(n) = &name {
                        watched = Some(n.clone());
                        next_look = Instant::now() + Duration::from_secs(2);
                    }
                }
                let _ = opened.send(name);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                next_look = Instant::now() + Duration::from_secs(2);
                if let Some(current) = watched.as_mut() {
                    let name = cpal::default_host()
                        .default_output_device()
                        .and_then(|d| d.name().ok())
                        .unwrap_or_default();
                    if name != *current {
                        *current = name;
                        reopen.store(true, Ordering::Relaxed);
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    drop(stream);
}

/// A stream on the system's default output device, mixing from `mixer`, with the device's
/// name.
fn open_default(
    shared: &Arc<Shared>,
    mixer: &Arc<MixerCell>,
    reopen: &Arc<AtomicBool>,
) -> Option<(cpal::Stream, String)> {
    let host = cpal::default_host();
    let Some(dev) = host.default_output_device() else {
        log::warn!("audio: no output device");
        return None;
    };
    let name = dev.name().unwrap_or_default();
    let cfg = match dev.default_output_config() {
        Ok(c) => c,
        Err(e) => {
            log::warn!("audio: no output config on {name}: {e}");
            return None;
        }
    };
    let (rate, ch) = (cfg.sample_rate().0, cfg.channels().max(1) as usize);
    shared.sample_rate.store(rate, Ordering::Relaxed);
    shared.channels.store(ch, Ordering::Relaxed);
    let cell = mixer.clone();
    let lost = reopen.clone();
    // (the echo's delay lines for this device's format, made here: the callback allocates
    // nothing; the mixer's old ones are let go of with the stream)
    let mut spare = Reverb::ready(rate, ch);
    let stream = dev.build_output_stream(
        &cfg.config(),
        move |data: &mut [f32], _| {
            let mixed = cell.try_with(|m| {
                m.adopt_reverb(&mut spare);
                m.render(data)
            });
            if mixed.is_none() {
                data.fill(0.0);
            }
        },
        move |e| {
            // (a lost device only: a driver's hiccups are not worth a new stream)
            if matches!(e, cpal::StreamError::DeviceNotAvailable) {
                lost.store(true, Ordering::Relaxed);
            }
            log::warn!("audio stream error: {e}");
        },
        None,
    );
    match stream {
        Ok(s) => {
            if let Err(e) = s.play() {
                log::warn!("audio: cannot start stream: {e}");
            }
            log::info!(
                "audio: playing on {name} ({} Hz, {} channels)",
                rate,
                cfg.channels()
            );
            Some((s, name))
        }
        Err(e) => {
            log::warn!("audio: cannot open stream on {name}: {e}");
            None
        }
    }
}
