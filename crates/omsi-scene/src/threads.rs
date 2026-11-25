//! Where the game's work runs besides the frame itself. The frame waits for the AI scripts
//! and the culling on rayon's shared pool; everything that may take tens of milliseconds and
//! is not needed this frame (preparing tiles, compressing textures, reading the timetable's
//! next vehicles) runs on pools of its own at a lower priority. On the shared pool such a job
//! held up the frame's parallel work until it was done (a worker that picks up a long job
//! cannot come back for the frame's), and at the same priority it took the cores the frame's
//! workers were waiting for - the 40-60 ms frames that came every few seconds while driving.

/// Tell the scheduler that the calling thread's work can wait for the frame's: macOS's
/// "utility" quality of service, Windows' below-normal priority, a higher nice value on
/// Linux. The thread still gets every core nobody else wants.
pub fn lower_thread_priority() {
    #[cfg(target_os = "macos")]
    {
        unsafe extern "C" {
            fn pthread_set_qos_class_self_np(qos_class: u32, relative_priority: i32) -> i32;
        }
        const QOS_CLASS_UTILITY: u32 = 0x11;
        // SAFETY: plain call on the current thread with valid constants
        unsafe {
            pthread_set_qos_class_self_np(QOS_CLASS_UTILITY, 0);
        }
    }
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetCurrentThread() -> isize;
            fn SetThreadPriority(thread: isize, priority: i32) -> i32;
        }
        const THREAD_PRIORITY_BELOW_NORMAL: i32 = -1;
        // SAFETY: the pseudo handle of the current thread and a valid priority
        unsafe {
            SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
        }
    }
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        unsafe extern "C" {
            fn setpriority(which: i32, who: u32, prio: i32) -> i32;
        }
        // (PRIO_PROCESS with 0 names the calling thread on Linux)
        // SAFETY: plain call with valid arguments
        unsafe {
            setpriority(0, 0, 5);
        }
    }
}

/// The pool for background jobs that are not tiles: texture compression (the upgrades of
/// textures first uploaded plain) and the timetable's fleet read ahead. A quarter of the
/// cores, at least one, at a lower priority (see `lower_thread_priority`).
pub fn background_pool() -> &'static rayon::ThreadPool {
    static POOL: std::sync::OnceLock<rayon::ThreadPool> = std::sync::OnceLock::new();
    POOL.get_or_init(|| {
        let n = (std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            / 4)
        .max(1);
        rayon::ThreadPoolBuilder::new()
            .num_threads(n)
            .thread_name(|i| format!("background {i}"))
            .start_handler(|_| lower_thread_priority())
            .build()
            .expect("background pool")
    })
}
