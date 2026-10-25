//! Report and dismiss a known fatal Steam API dialog created by a native OMSI plugin.
//!
//! Native `.opl` DLLs may show a modal MessageBox from their own initialization code.
//! Such a window is not connected to the game's logger and can block PluginStart
//! forever. This monitor runs inside the isolated 32-bit host, before LoadLibrary.
//!
//! Do not close arbitrary plugin windows (including licensing/configuration dialogs):
//! only the specific unrecoverable "Unable to initialize Steam API" error is handled.

use std::collections::HashSet;
use std::time::Duration;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::Threading::GetCurrentProcessId;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, EnumWindows, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, PostMessageW, WM_CLOSE,
};

unsafe extern "system" fn collect_window(hwnd: HWND, data: isize) -> i32 {
    // SAFETY: EnumWindows and EnumChildWindows call this synchronously while the
    // Vec given as lparam is still alive on the calling thread.
    unsafe { &mut *(data as *mut Vec<HWND>) }.push(hwnd);
    1
}

fn windows() -> Vec<HWND> {
    let mut out = Vec::new();
    // SAFETY: collect_window only uses the pointer during this synchronous call.
    unsafe { EnumWindows(Some(collect_window), &mut out as *mut Vec<HWND> as isize) };
    out
}

fn children(hwnd: HWND) -> Vec<HWND> {
    let mut out = Vec::new();
    // SAFETY: collect_window only uses the pointer during this synchronous call.
    unsafe { EnumChildWindows(hwnd, Some(collect_window), &mut out as *mut Vec<HWND> as isize) };
    out
}

fn window_text(hwnd: HWND) -> String {
    // A MessageBox stores its message in a child Static control, not its title.
    let len = unsafe { GetWindowTextLengthW(hwnd) };
    if len <= 0 {
        return String::new();
    }
    let mut utf16 = vec![0u16; (len as usize).min(2048) + 1];
    let count = unsafe { GetWindowTextW(hwnd, utf16.as_mut_ptr(), utf16.len() as i32) };
    String::from_utf16_lossy(&utf16[..count.max(0) as usize])
}

fn is_steam_api_error(text: &str) -> bool {
    text.to_ascii_lowercase()
        .contains("unable to initialize steam api")
}

/// Start before loading the DLL: its DllMain might show the dialog, too.
pub fn watch(dll: &str) {
    let dll = dll.to_owned();
    let _ = std::thread::Builder::new()
        .name("plugin steam error monitor".into())
        .spawn(move || {
            let pid = unsafe { GetCurrentProcessId() };
            let mut handled = HashSet::new();
            loop {
                for hwnd in windows() {
                    let mut owner = 0;
                    unsafe { GetWindowThreadProcessId(hwnd, &mut owner) };
                    if owner != pid || handled.contains(&(hwnd as usize)) {
                        continue;
                    }
                    let mut message = window_text(hwnd);
                    for child in children(hwnd) {
                        let label = window_text(child);
                        if !label.is_empty() {
                            message.push(' ');
                            message.push_str(&label);
                        }
                    }
                    if !is_steam_api_error(&message) {
                        continue;
                    }
                    handled.insert(hwnd as usize);
                    eprintln!(
                        "omsi-plugin-host: {dll}: native plugin Steam API error dialog: {}; dismissing it",
                        message.trim()
                    );
                    // SAFETY: only windows of this helper's own process are touched.
                    // WM_CLOSE allows the DLL to handle the failure normally, rather
                    // than terminating the process or modifying plugin code.
                    if unsafe { PostMessageW(hwnd, WM_CLOSE, 0, 0) } == 0 {
                        eprintln!(
                            "omsi-plugin-host: {dll}: could not dismiss Steam API error dialog: {}",
                            std::io::Error::last_os_error()
                        );
                    }
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        });
}

#[cfg(test)]
mod tests {
    use super::is_steam_api_error;

    #[test]
    fn matches_the_reported_steam_error_only() {
        assert!(is_steam_api_error(
            "Error Unable to initialize Steam API. Code: 00000012 0000007E"
        ));
        assert!(is_steam_api_error("UNABLE TO INITIALIZE STEAM API"));
        assert!(!is_steam_api_error("Plugin settings"));
        assert!(!is_steam_api_error("Enter your Steam license key"));
        assert!(!is_steam_api_error("Unable to initialize the graphics API"));
    }
}
