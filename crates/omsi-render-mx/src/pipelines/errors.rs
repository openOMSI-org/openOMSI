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
            if matches!(e, mx::Error::OutOfMemory(_)) {
                oom.store(true, std::sync::atomic::Ordering::Relaxed);
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
            if rt_on {
                // (the ray tracing goes first: the likelier cause, and the dearer feature)
                if !rt_flag.swap(true, std::sync::atomic::Ordering::Relaxed) {
                    log::error!(
                        "GPU error with ray tracing (Enhanced+ draws as Enhanced from now on): {text}"
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
