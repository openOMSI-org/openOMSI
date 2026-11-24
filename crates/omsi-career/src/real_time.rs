//! Real-time sync: the game's date and time follow this device's clock (the "Sync with real
//! time" setting, `time_sync`; a dedicated server's `real_time` in server.cfg).
//!
//! In a LAN session the host's clock is the one that counts: a host or server with the sync
//! on runs at real time, and the clients follow it as they always did.

use omsi_sim::SimClock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// This device's local calendar date and time of day.
#[derive(Clone, Copy, Debug)]
pub struct Now {
    pub year: i32,
    pub month: i32,
    pub day: i32,
    /// Seconds since local midnight.
    pub secs: f64,
}

pub fn now() -> Option<Now> {
    local(SystemTime::now().duration_since(UNIX_EPOCH).ok()?)
}

#[cfg(unix)]
fn local(d: Duration) -> Option<Now> {
    // SAFETY: localtime_r only writes the struct handed to it
    unsafe {
        let t = d.as_secs() as libc::time_t;
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&t, &mut tm).is_null() {
            return None;
        }
        Some(Now {
            year: tm.tm_year + 1900,
            month: tm.tm_mon + 1,
            day: tm.tm_mday,
            secs: tm.tm_hour as f64 * 3600.0
                + tm.tm_min as f64 * 60.0
                + tm.tm_sec as f64
                + d.subsec_nanos() as f64 / 1e9,
        })
    }
}

#[cfg(windows)]
fn local(_d: Duration) -> Option<Now> {
    // SAFETY: GetLocalTime only returns a value
    let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    Some(Now {
        year: t.wYear as i32,
        month: t.wMonth as i32,
        day: t.wDay as i32,
        secs: t.wHour as f64 * 3600.0
            + t.wMinute as f64 * 60.0
            + t.wSecond as f64
            + t.wMilliseconds as f64 / 1000.0,
    })
}

#[cfg(not(any(unix, windows)))]
fn local(_d: Duration) -> Option<Now> {
    None
}

/// The days from year 1 to the given day (a running number to subtract dates with).
fn day_number(year: i32, day_of_year: i32) -> i64 {
    let y = year as i64 - 1;
    365 * y + y / 4 - y / 100 + y / 400 + day_of_year as i64
}

/// The real date and time as a clock (the other fields of `base` stay).
pub fn clock_now(base: &SimClock) -> Option<SimClock> {
    let n = now()?;
    let mut c = base.clone();
    c.set_date(n.year, n.month, n.day);
    c.time = n.secs.clamp(0.0, 86399.999);
    Some(c)
}

/// Seconds `real` is ahead of `mine` (negative: behind), across midnight too.
pub fn gap(mine: &SimClock, real: &SimClock) -> f64 {
    (day_number(real.year, real.day_of_year) - day_number(mine.year, mine.day_of_year)) as f64
        * 86400.0
        + (real.time - mine.time)
}

/// A dedicated server runs on the real time (`real_time` in its server.cfg): its clock
/// cannot be set or sped up. (Process-wide: a server is one session per process, set once
/// from its server.cfg before it starts.)
static SERVER_REAL: AtomicBool = AtomicBool::new(false);

pub fn set_server_real(on: bool) {
    SERVER_REAL.store(on, Ordering::Relaxed);
}

pub fn server_real() -> bool {
    SERVER_REAL.load(Ordering::Relaxed)
}
