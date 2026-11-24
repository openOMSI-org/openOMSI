//! What the machine has and gives back: its physical memory, and freed heap pages returned to
//! the system.

/// Bytes of memory the machine has.
pub fn physical_memory() -> Option<u64> {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        unsafe extern "C" {
            fn sysctlbyname(
                name: *const std::ffi::c_char,
                oldp: *mut std::ffi::c_void,
                oldlenp: *mut usize,
                newp: *mut std::ffi::c_void,
                newlen: usize,
            ) -> i32;
        }
        let mut v: u64 = 0;
        let mut len = std::mem::size_of::<u64>();
        // SAFETY: a u64 buffer of the size given, a nul-terminated name
        let r = unsafe {
            sysctlbyname(
                c"hw.memsize".as_ptr(),
                &mut v as *mut u64 as *mut std::ffi::c_void,
                &mut len,
                std::ptr::null_mut(),
                0,
            )
        };
        (r == 0 && v > 0).then_some(v)
    }
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        // MemTotal in kB
        let text = std::fs::read_to_string("/proc/meminfo").ok()?;
        let line = text.lines().find(|l| l.starts_with("MemTotal:"))?;
        let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
        Some(kb * 1024)
    }
    #[cfg(windows)]
    {
        use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
        let mut m = MEMORYSTATUSEX {
            dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
            ..Default::default()
        };
        // SAFETY: a MEMORYSTATUSEX with its length set, as the call wants it
        unsafe { GlobalMemoryStatusEx(&mut m) }.ok()?;
        (m.ullTotalPhys > 0).then_some(m.ullTotalPhys)
    }
    #[cfg(not(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "linux",
        target_os = "android",
        windows
    )))]
    {
        None
    }
}

/// Give the heap's free pages back to the system, on a thread of its own (the allocator
/// keeps what was freed for later, and it counts as the game's memory until then).
pub fn release_free_memory() {
    #[cfg(target_os = "macos")]
    {
        unsafe extern "C" {
            fn malloc_zone_pressure_relief(zone: *mut std::ffi::c_void, goal: usize) -> usize;
        }
        let _ = std::thread::Builder::new()
            .name("heap relief".into())
            .spawn(|| {
                let t = std::time::Instant::now();
                // SAFETY: a null zone asks every malloc zone; the call only returns free pages
                let freed = unsafe { malloc_zone_pressure_relief(std::ptr::null_mut(), 0) };
                if omsi_cfg::flags::OMSI_PROFILE.is_set() {
                    log::info!(
                        "heap: {:.0} MB of free pages given back in {:.0} ms",
                        freed as f64 / 1e6,
                        t.elapsed().as_secs_f64() * 1000.0
                    );
                }
            });
    }
}
