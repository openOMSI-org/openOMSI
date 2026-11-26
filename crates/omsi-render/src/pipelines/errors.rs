//! What the device reports on its own: errors nobody caught and its loss.

use crate::*;

/// Set by the device's handlers, read by `render_inner` and `Renderer::device_lost`.
pub(crate) struct ErrorFlags {
    pub gpu_error: Arc<std::sync::atomic::AtomicBool>,
    pub rt_error: Arc<std::sync::atomic::AtomicBool>,
    pub out_of_memory: Arc<std::sync::atomic::AtomicBool>,
    pub device_lost: Arc<std::sync::Mutex<Option<String>>>,
    /// Errors from `Queue::submit` since the frame last looked (see `RejectedRun`).
    pub refused_submits: Arc<std::sync::atomic::AtomicU64>,
}

/// Errors about invalid GPU resources within five seconds that mean the device is gone: a frame
/// with one bad texture makes one or two (60 a second); a dead device makes hundreds a second.
const INVALID_FLOOD: u64 = 1500;

/// Frames in a row whose work the device turned away (`Queue::submit` refused it for an invalid
/// resource) that make the renderer be built again, and, the next time, the device be taken for
/// lost: about two seconds of nothing drawn. A bad texture spoils a frame or two; one of the
/// renderer's own resources that could not be made (the card was out of memory while it was
/// built) spoils every frame until it is made again - 100 errors a second stayed under
/// `INVALID_FLOOD` and the picture was gone for good.
pub(crate) const REJECTED_FRAMES: u32 = 120;

/// What to do about a run of refused frames (see `REJECTED_FRAMES`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Rejected {
    Fine,
    Rebuild,
    Lost,
}

/// Frames refused in a row (see `REJECTED_FRAMES`), counted across a rebuild of the renderer.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct RejectedRun {
    run: u32,
    rebuilt: bool,
    /// Frames drawn in a row since the last refused one.
    good: u32,
}

/// Frames drawn in a row after which a rebuild counts as having worked: a later run of
/// refused frames rebuilds again instead of taking the device for lost (one transient run
/// an hour earlier made the next one end the device).
pub(crate) const RECOVERED_FRAMES: u32 = REJECTED_FRAMES * 5;

impl RejectedRun {
    /// One frame more: whether the device refused any of its work.
    pub(crate) fn frame(&mut self, refused: bool) -> Rejected {
        if !refused {
            self.run = 0;
            self.good = self.good.saturating_add(1);
            if self.good >= RECOVERED_FRAMES {
                self.rebuilt = false;
            }
            return Rejected::Fine;
        }
        self.good = 0;
        self.run += 1;
        if self.run < REJECTED_FRAMES {
            return Rejected::Fine;
        }
        self.run = 0;
        if std::mem::replace(&mut self.rebuilt, true) {
            Rejected::Lost
        } else {
            Rejected::Rebuild
        }
    }
}

/// When the card last refused memory (ms from the first call, 0 never): the fallbacks ask
/// whether a failure came with it (`out_of_memory_within`).
static LAST_OUT_OF_MEMORY: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn ms_since_start() -> u64 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    // (1 at the start: 0 is "never")
    START
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_millis() as u64
        + 1
}

/// The card refused memory now (an error of the device, a texture or buffer that could not be made).
pub(crate) fn note_out_of_memory() {
    LAST_OUT_OF_MEMORY.store(ms_since_start(), std::sync::atomic::Ordering::Relaxed);
}

/// Whether the card refused memory within the last `d`.
pub(crate) fn out_of_memory_within(d: std::time::Duration) -> bool {
    let at = LAST_OUT_OF_MEMORY.load(std::sync::atomic::Ordering::Relaxed);
    at != 0 && ms_since_start().saturating_sub(at) <= d.as_millis() as u64
}

/// How long after the card refused memory an error is taken for its doing (a resource that
/// could not be made, used later), and the texture budget is given to lower its textures before
/// a feature is turned off for it.
pub(crate) const OUT_OF_MEMORY_SPELL: std::time::Duration = std::time::Duration::from_secs(10);

pub(crate) fn install(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    options: &RenderOptions,
) -> ErrorFlags {
    let msaa = options.msaa;
    // A GPU error while multisampling is on is logged and switches multisampling off
    // at the next frame (`render_inner`). Otherwise it is logged and the game goes on:
    // wgpu's own handler ends the process, and one call a driver refused (a limit of
    // that card, a bigger map than the last) closed the game a few seconds into the
    // drive - a wrong picture for a frame is better than no game. The first errors and
    // then every thousandth reach the log.
    let gpu_error = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let rt_error = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let out_of_memory = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let device_lost: Arc<std::sync::Mutex<Option<String>>> = Default::default();
    let refused_submits = Arc::new(std::sync::atomic::AtomicU64::new(0));
    {
        let lost = device_lost.clone();
        device.set_device_lost_callback(move |reason, message| {
            // (dropping the device at the end also calls this: that is no loss)
            if matches!(reason, wgpu::DeviceLostReason::Destroyed) {
                return;
            }
            log::error!("the graphics device was lost ({reason:?}): {message}");
            *lost.lock().unwrap_or_else(|e| e.into_inner()) =
                Some(format!("{reason:?}: {message}"));
        });
    }
    {
        let flag = gpu_error.clone();
        let rt_flag = rt_error.clone();
        let rt_on = options.ray_tracing;
        let oom = out_of_memory.clone();
        let count = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let lost = device_lost.clone();
        let refused = refused_submits.clone();
        let flood: Arc<std::sync::Mutex<(std::time::Instant, u64)>> =
            Arc::new(std::sync::Mutex::new((std::time::Instant::now(), 0)));
        let rt_blame = std::sync::Mutex::new(RtBlame::default());
        device.on_uncaptured_error(Arc::new(move |e: wgpu::Error| {
            let out_of_memory = matches!(e, wgpu::Error::OutOfMemory { .. });
            if out_of_memory {
                oom.store(true, std::sync::atomic::Ordering::Relaxed);
                note_out_of_memory();
            }
            let n = count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let text = gpu_error_text(&e);
            if text.contains("Queue::submit") {
                refused.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            // A resource that could not be made (the card ran out of memory, a driver reset
            // that never reached the lost-device callback) makes every use of it an error:
            // hundreds a second, a picture of nothing, the loading screen for good (#3232).
            // That rate is a device lost, whatever the driver called it.
            if text.contains("is invalid") {
                let mut w = flood.lock().unwrap_or_else(|p| p.into_inner());
                let now = std::time::Instant::now();
                if now.duration_since(w.0) > std::time::Duration::from_secs(5) {
                    *w = (now, 0);
                }
                w.1 += 1;
                if w.1 >= INVALID_FLOOD {
                    let mut l = lost.lock().unwrap_or_else(|p| p.into_inner());
                    if l.is_none() {
                        log::error!("the graphics device is taken for lost: {} errors about invalid GPU resources in {:.1} s (the first of them: {text})", w.1, now.duration_since(w.0).as_secs_f32());
                        *l = Some(format!("Unknown: {} errors about invalid GPU resources in a few seconds", w.1));
                    }
                }
            }
            let blame_rt = rt_on
                && rt_blame.lock().unwrap_or_else(|p| p.into_inner()).blames(
                    out_of_memory,
                    &text,
                    out_of_memory_within(OUT_OF_MEMORY_SPELL),
                    std::time::Instant::now(),
                );
            if blame_rt {
                // (the ray tracing goes first: the likelier cause, and the dearer feature)
                if !rt_flag.swap(true, std::sync::atomic::Ordering::Relaxed) {
                    log::error!("GPU error with ray tracing (Enhanced+ draws as Enhanced from now on): {text}");
                }
            } else if rt_on {
                if n < 20 || n % 1000 == 0 {
                    log::error!("GPU error #{} with ray tracing, put down to the card being out of memory (the game goes on): {text}", n + 1);
                }
            } else if blames_msaa(&e, &text, msaa) && !flag.swap(true, std::sync::atomic::Ordering::Relaxed) {
                log::error!("GPU error with {msaa}x MSAA (drawing without it from now on): {text}");
            } else if n < 20 || n % 1000 == 0 {
                log::error!("GPU error #{} (the game goes on): {text}", n + 1);
            }
        }));
    }
    #[cfg(not(feature = "test-hooks"))]
    let _ = format;
    #[cfg(feature = "test-hooks")]
    if omsi_cfg::flags::OMSI_FAKE_GPU_ERROR.var() == Some("build") && msaa > 1 {
        // test hook for the fallback in `new_with`: a sample count no device takes
        let _ = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("invalid"),
            size: wgpu::Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 3,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
    }
    ErrorFlags {
        gpu_error,
        rt_error,
        out_of_memory,
        device_lost,
        refused_submits,
    }
}

/// Whether an error with the ray tracing on is put down to it (which turns it off for the
/// session). The card running out of memory is the texture budget's first: it lowers the
/// textures (`take_out_of_memory`), and the errors of what could not be made meanwhile are
/// its doing too. Only when the card is out of memory again after the budget had its time
/// (`OUT_OF_MEMORY_SPELL`) twice over does the ray tracing, the dearer feature, go - before,
/// one Out of Memory at the end of the timetable's preloading turned it off, rebuilt the
/// renderer with the card full, and left the clouds and the sky cube invalid for good.
#[derive(Debug, Default)]
pub(crate) struct RtBlame {
    /// When the card ran out of memory, a spell apart (at most the last two).
    spells: Vec<std::time::Instant>,
}

impl RtBlame {
    pub(crate) fn blames(
        &mut self,
        out_of_memory: bool,
        text: &str,
        memory_short: bool,
        now: std::time::Instant,
    ) -> bool {
        if out_of_memory {
            if self
                .spells
                .last()
                .is_none_or(|t| now.duration_since(*t) >= OUT_OF_MEMORY_SPELL)
            {
                self.spells.push(now);
            }
            // (the third spell: twice the budget lowered the textures, and the card is
            // still full)
            return self.spells.len() >= 3;
        }
        // (a resource that could not be made for want of memory, used: no fault of the
        // ray tracing's)
        !(memory_short && text.contains("is invalid"))
    }
}

fn blames_msaa(e: &wgpu::Error, text: &str, msaa: u32) -> bool {
    msaa > 1 && !matches!(e, wgpu::Error::OutOfMemory { .. }) && !text.contains("label is invalid")
}

#[cfg(test)]
mod rt_blame_tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn out_of_memory_is_the_budget_s_until_it_comes_back_twice() {
        let t0 = Instant::now();
        let mut b = RtBlame::default();
        assert!(!b.blames(true, "Out of Memory", true, t0));
        // (more of the same spell, and the invalid resources it left)
        assert!(!b.blames(true, "Out of Memory", true, t0 + Duration::from_secs(2)));
        assert!(!b.blames(
            false,
            "Texture with 'cloud base' label is invalid",
            true,
            t0 + Duration::from_secs(3)
        ));
        assert!(!b.blames(true, "Out of Memory", true, t0 + Duration::from_secs(12)));
        assert!(b.blames(true, "Out of Memory", true, t0 + Duration::from_secs(25)));
    }

    #[test]
    fn other_errors_with_the_ray_tracing_are_its() {
        let t0 = Instant::now();
        let mut b = RtBlame::default();
        assert!(b.blames(false, "In a trace_rays command", false, t0));
        // (an invalid resource with no shortage of memory behind it)
        assert!(b.blames(false, "BindGroup with 'rt' label is invalid", false, t0));
    }

    #[test]
    fn a_run_of_refused_frames_rebuilds_then_loses_the_device() {
        let mut r = RejectedRun::default();
        for _ in 1..REJECTED_FRAMES {
            assert_eq!(r.frame(true), Rejected::Fine);
        }
        // (a frame that went through starts the count again)
        assert_eq!(r.frame(false), Rejected::Fine);
        for _ in 1..REJECTED_FRAMES {
            assert_eq!(r.frame(true), Rejected::Fine);
        }
        assert_eq!(r.frame(true), Rejected::Rebuild);
        for _ in 1..REJECTED_FRAMES {
            assert_eq!(r.frame(true), Rejected::Fine);
        }
        assert_eq!(r.frame(true), Rejected::Lost);
    }

    #[test]
    fn a_rebuild_that_worked_is_forgotten_after_a_while() {
        let mut r = RejectedRun::default();
        for _ in 1..REJECTED_FRAMES {
            r.frame(true);
        }
        assert_eq!(r.frame(true), Rejected::Rebuild);
        for _ in 0..RECOVERED_FRAMES {
            assert_eq!(r.frame(false), Rejected::Fine);
        }
        for _ in 1..REJECTED_FRAMES {
            assert_eq!(r.frame(true), Rejected::Fine);
        }
        assert_eq!(r.frame(true), Rejected::Rebuild, "rebuilt again, not lost");
    }

    #[test]
    fn a_frame_or_two_refused_is_no_matter() {
        let mut r = RejectedRun::default();
        for _ in 0..1000 {
            assert_eq!(r.frame(true), Rejected::Fine);
            assert_eq!(r.frame(false), Rejected::Fine);
        }
    }
}

#[cfg(test)]
mod msaa_blame_tests {
    use super::blames_msaa;

    #[test]
    fn out_of_memory_and_its_invalid_resources_keep_msaa() {
        let oom = wgpu::Error::OutOfMemory {
            source: Box::new(std::io::Error::other("oom")),
        };
        assert!(!blames_msaa(&oom, "Out of Memory", 4));
        let invalid = wgpu::Error::Validation {
            source: Box::new(std::io::Error::other("v")),
            description: String::new(),
        };
        assert!(!blames_msaa(
            &invalid,
            "BindGroup with 'camera' label is invalid",
            4
        ));
        assert!(blames_msaa(&invalid, "Sample count 3 is invalid", 4));
        assert!(blames_msaa(&invalid, "Color state [0] is invalid", 4));
        assert!(!blames_msaa(&invalid, "Sample count 3 is invalid", 1));
    }
}
