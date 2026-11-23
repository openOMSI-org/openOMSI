//! `openbus` - the game and its launcher (the game itself is the `openbus_game` library).

// a game, not a console program: no console window opens beside it on Windows (a start
// from a terminal still prints there, see attach_parent_console)
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

/// Laptops with two graphics chips (NVIDIA Optimus, AMD's switchable graphics): these two
/// exports ask their drivers for the graphics card rather than the processor's graphics,
/// as a game asks (build.rs exports them from the executable). Without them such a driver
/// could hand the game the weaker chip, or a card it then would not open.
#[cfg(windows)]
#[unsafe(no_mangle)]
#[used]
pub static NvOptimusEnablement: u32 = 1;

#[cfg(windows)]
#[unsafe(no_mangle)]
#[used]
pub static AmdPowerXpressRequestHighPerformance: u32 = 1;

/// mimalloc in place of the system's allocator: the frame allocates and frees many small
/// and middle-sized blocks (traffic, people, scripts, the render lists), which the system
/// allocators serve slowly - Windows' heap especially, and macOS's with its large-block
/// cache turned off (see `startup::restart_with_allocator_settings`), where every big
/// block was a call into the kernel. (Not on Android: the game is a library there.)
#[cfg(not(target_os = "android"))]
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> anyhow::Result<()> {
    // Before anything that could break (see omsi-guard): `--self-test`, the start guard's
    // watcher, and safe mode's switches.
    if let Some(code) = omsi_guard::early() {
        std::process::exit(code);
    }
    omsi_guard::apply_safe_mode();
    // A launcher start (a double click: no arguments) gets its watcher, or is healed right
    // now when this version failed here before.
    if std::env::args_os().len() == 1 {
        if !omsi_guard::start(omsi_guard::journal::Kind::Launcher) {
            return Ok(());
        }
        omsi_guard::test_break();
    }
    let r = openbus_game::run();
    omsi_guard::exited(r.is_ok());
    r
}
