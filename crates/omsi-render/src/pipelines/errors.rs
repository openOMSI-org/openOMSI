//! What the device reports on its own: errors nobody caught and its loss.

use crate::*;

/// Set by the device's handlers, read by `render_inner` and `Renderer::device_lost`.
pub(crate) struct ErrorFlags {
    pub gpu_error: Arc<std::sync::atomic::AtomicBool>,
    pub rt_error: Arc<std::sync::atomic::AtomicBool>,
    pub out_of_memory: Arc<std::sync::atomic::AtomicBool>,
    pub device_lost: Arc<std::sync::Mutex<Option<String>>>,
}

/// Errors about invalid GPU resources within five seconds that mean the device is gone: a frame
/// with one bad texture makes one or two (60 a second); a dead device makes hundreds a second.
const INVALID_FLOOD: u64 = 1500;

pub(crate) fn install(device: &wgpu::Device, format: wgpu::TextureFormat, options: &RenderOptions) -> ErrorFlags {
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
    {
        let lost = device_lost.clone();
        device.set_device_lost_callback(move |reason, message| {
            // (dropping the device at the end also calls this: that is no loss)
            if matches!(reason, wgpu::DeviceLostReason::Destroyed) {
                return;
            }
            log::error!("the graphics device was lost ({reason:?}): {message}");
            *lost.lock().unwrap_or_else(|e| e.into_inner()) = Some(format!("{reason:?}: {message}"));
        });
    }
    {
        let flag = gpu_error.clone();
        let rt_flag = rt_error.clone();
        let rt_on = options.ray_tracing;
        let oom = out_of_memory.clone();
        let count = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let lost = device_lost.clone();
        let flood: Arc<std::sync::Mutex<(std::time::Instant, u64)>> = Arc::new(std::sync::Mutex::new((std::time::Instant::now(), 0)));
        device.on_uncaptured_error(Arc::new(move |e: wgpu::Error| {
            if matches!(e, wgpu::Error::OutOfMemory { .. }) {
                oom.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            let n = count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let text = gpu_error_text(&e);
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
            if rt_on {
                // (the ray tracing goes first: the likelier cause, and the dearer feature)
                if !rt_flag.swap(true, std::sync::atomic::Ordering::Relaxed) {
                    log::error!("GPU error with ray tracing (Enhanced+ draws as Enhanced from now on): {text}");
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
    ErrorFlags { gpu_error, rt_error, out_of_memory, device_lost }
}

fn blames_msaa(e: &wgpu::Error, text: &str, msaa: u32) -> bool {
    msaa > 1 && !matches!(e, wgpu::Error::OutOfMemory { .. }) && !text.contains("label is invalid")
}

#[cfg(test)]
mod msaa_blame_tests {
    use super::blames_msaa;

    #[test]
    fn out_of_memory_and_its_invalid_resources_keep_msaa() {
        let oom = wgpu::Error::OutOfMemory { source: Box::new(std::io::Error::other("oom")) };
        assert!(!blames_msaa(&oom, "Out of Memory", 4));
        let invalid = wgpu::Error::Validation { source: Box::new(std::io::Error::other("v")), description: String::new() };
        assert!(!blames_msaa(&invalid, "BindGroup with 'camera' label is invalid", 4));
        assert!(blames_msaa(&invalid, "Sample count 3 is invalid", 4));
        assert!(blames_msaa(&invalid, "Color state [0] is invalid", 4));
        assert!(!blames_msaa(&invalid, "Sample count 3 is invalid", 1));
    }
}
