//! What the device reports on its own: errors nobody caught and its loss.

use crate::*;

/// Set by `report`, read by `render_inner` and `Renderer::device_lost`.
pub(crate) struct ErrorFlags {
    pub gpu_error: Arc<std::sync::atomic::AtomicBool>,
    pub rt_error: Arc<std::sync::atomic::AtomicBool>,
    pub out_of_memory: Arc<std::sync::atomic::AtomicBool>,
    pub device_lost: Arc<std::sync::Mutex<Option<String>>>,
    pub report: Report,
    pub log: Arc<std::sync::Mutex<ErrorLog>>,
}

/// The last errors `report` saw, numbered, for [`crate::ErrorWatch`].
#[derive(Default)]
pub(crate) struct ErrorLog {
    pub next: u64,
    pub entries: std::collections::VecDeque<(u64, String)>,
}

impl ErrorLog {
    fn push(&mut self, text: String) {
        self.entries.push_back((self.next, text));
        self.next += 1;
        if self.entries.len() > 256 {
            self.entries.pop_front();
        }
    }
}

/// Where a GPU error that nothing else handles goes: the device's polls, a frame mX refused.
/// (mX returns its errors where they happen - wgpu called a handler for them instead.)
pub(crate) type Report = Arc<dyn Fn(&mx::Error) + Send + Sync>;

/// Errors about invalid GPU resources within five seconds that mean the device is gone: a frame
/// with one bad texture makes one or two (60 a second); a dead device makes hundreds a second.
const INVALID_FLOOD: u64 = 1500;

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
    device: &mx::Device,
    format: mx::TextureFormat,
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
    let log: Arc<std::sync::Mutex<ErrorLog>> = Default::default();
    let report: Report = {
        let log = log.clone();
        let flag = gpu_error.clone();
        let rt_flag = rt_error.clone();
        let rt_on = options.ray_tracing;
        let oom = out_of_memory.clone();
        let count = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let lost = device_lost.clone();
        let flood: Arc<std::sync::Mutex<(std::time::Instant, u64)>> =
            Arc::new(std::sync::Mutex::new((std::time::Instant::now(), 0)));
        let rt_blame = std::sync::Mutex::new(RtBlame::default());
        Arc::new(move |e: &mx::Error| {
            log.lock()
                .unwrap_or_else(|p| p.into_inner())
                .push(gpu_error_text(e));
            if let mx::Error::DeviceLost(why) = e {
                let mut l = lost.lock().unwrap_or_else(|p| p.into_inner());
                if l.is_none() {
                    log::error!("the graphics device was lost: {why}");
                    *l = Some(why.clone());
                }
                return;
            }
            let out_of_memory = matches!(e, mx::Error::OutOfMemory(_));
            if out_of_memory {
                oom.store(true, std::sync::atomic::Ordering::Relaxed);
                note_out_of_memory();
            }
            let n = count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let text = gpu_error_text(e);
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
                        log::error!(
                            "the graphics device is taken for lost: {} errors about invalid GPU resources in {:.1} s (the first of them: {text})",
                            w.1,
                            now.duration_since(w.0).as_secs_f32()
                        );
                        *l = Some(format!(
                            "Unknown: {} errors about invalid GPU resources in a few seconds",
                            w.1
                        ));
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
                    log::error!(
                        "GPU error with ray tracing (Enhanced+ draws as Enhanced from now on): {text}"
                    );
                }
            } else if rt_on {
                if n < 20 || n % 1000 == 0 {
                    log::error!(
                        "GPU error #{} with ray tracing, put down to the card being out of memory (the game goes on): {text}",
                        n + 1
                    );
                }
            } else if blames_msaa(e, &text, msaa)
                && !flag.swap(true, std::sync::atomic::Ordering::Relaxed)
            {
                log::error!("GPU error with {msaa}x MSAA (drawing without it from now on): {text}");
            } else if n < 20 || n % 1000 == 0 {
                log::error!("GPU error #{} (the game goes on): {text}", n + 1);
            }
        })
    };
    let _ = device;
    #[cfg(not(feature = "test-hooks"))]
    let _ = format;
    #[cfg(feature = "test-hooks")]
    if omsi_cfg::flags::OMSI_FAKE_GPU_ERROR.var() == Some("build") && msaa > 1 {
        // test hook for the fallback in `new_with`: a sample count no device takes
        let made = device.create_texture(&mx::TextureDescriptor {
            label: Some("invalid"),
            size: mx::Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 3,
            dimension: mx::TextureDimension::D2,
            format,
            usage: mx::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        if let Err(e) = made {
            report(&e);
        }
    }
    ErrorFlags {
        gpu_error,
        rt_error,
        out_of_memory,
        device_lost,
        report,
        log,
    }
}

fn blames_msaa(e: &mx::Error, text: &str, msaa: u32) -> bool {
    msaa > 1 && !matches!(e, mx::Error::OutOfMemory(_)) && !text.contains("label is invalid")
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
}

#[cfg(test)]
mod msaa_blame_tests {
    use super::blames_msaa;

    #[test]
    fn out_of_memory_and_its_invalid_resources_keep_msaa() {
        let oom = mx::Error::OutOfMemory("a texture".into());
        assert!(!blames_msaa(&oom, "Out of Memory", 4));
        let invalid = mx::Error::Device("v".into());
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
