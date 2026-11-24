//! A panic that is caught and handled: a driver wgpu cannot use, or a controller library
//! that takes itself down, gives an error there and is not reported as the end of the game.
//! (Here, below the renderer and the input devices, so both can use the one guard the
//! game's panic hook asks.)

thread_local! {
    /// A panic on this thread now is caught and handled (see [`catching`]).
    static CATCHING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether a panic on this thread is being caught by the renderer (the game's panic hook
/// does not report it as the end of the game).
pub fn catching() -> bool {
    CATCHING.with(|c| c.get())
}

/// Run `f`; a panic in it (a driver wgpu cannot use, taking it down in a way it does not
/// turn into an error) is caught and gives None, and is not reported as the end of the game.
pub fn catch<R>(f: impl FnOnce() -> R) -> Option<R> {
    catch_unwind(f).ok()
}

/// As [`catch`], keeping the panic's payload (to look at it, or to go on with it by
/// `std::panic::resume_unwind`).
pub fn catch_unwind<R>(f: impl FnOnce() -> R) -> std::thread::Result<R> {
    let was = CATCHING.with(|c| c.replace(true));
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    CATCHING.with(|c| c.set(was));
    r
}
