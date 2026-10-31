//! A plugin with OMSI's plugin interface, for the tests of `omsi-plugin`. Its `.opl`
//! lists `[varlist] 1 throttle`, `[stringvarlist] 1 note`, `[systemvarlist] 1 Time` and
//! `[triggers] 1 horn`: it doubles `throttle`, writes "seen at <Time>" into `note`,
//! remembers `Time` as it last saw it, and holds `horn` down on odd whole seconds. With
//! `[scripttexturelist]` it paints the first entry (see `AccessScriptTexture`), and it
//! prices the passengers' tickets by the stops they ride (see `AccessTicketRequest`). A
//! negative `throttle` keeps it busy in its own code for that many seconds first, as a
//! plugin waiting on the network does.
use std::sync::atomic::{AtomicU32, Ordering};

static TIME: AtomicU32 = AtomicU32::new(0);
static STARTED: AtomicU32 = AtomicU32::new(0);

#[unsafe(no_mangle)]
pub extern "system" fn PluginStart(_owner: *mut std::ffi::c_void) {
    STARTED.fetch_add(1, Ordering::SeqCst);
}

#[unsafe(no_mangle)]
pub extern "system" fn PluginFinalize() {}

/// # Safety
/// `value` and `write` point to a Single and a Boolean, as OMSI passes them.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn AccessVariable(index: u16, value: *mut f32, write: *mut u8) {
    unsafe {
        if index == 0 {
            if *value < 0.0 {
                std::thread::sleep(std::time::Duration::from_secs_f32(-*value));
            }
            *value *= 2.0;
            *write = 1;
        }
    }
}

/// # Safety
/// As `AccessVariable`.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn AccessSystemVariable(index: u16, value: *mut f32, write: *mut u8) {
    unsafe {
        if index == 0 {
            TIME.store((*value).to_bits(), Ordering::SeqCst);
            *write = 0;
        }
    }
}

/// # Safety
/// `text` is a buffer of the text's length + 1 wide characters.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn AccessStringVariable(index: u16, text: *mut u16, write: *mut u8) {
    unsafe {
        if index != 0 {
            return;
        }
        let mut len = 0;
        while *text.add(len) != 0 {
            len += 1;
        }
        let t = f32::from_bits(TIME.load(Ordering::SeqCst));
        let msg: Vec<u16> = format!("seen at {t}").encode_utf16().collect();
        // only as much as the buffer holds, as a real plugin must
        let n = msg.len().min(len);
        std::ptr::copy_nonoverlapping(msg.as_ptr(), text, n);
        *text.add(n) = 0;
        *write = 1;
    }
}

/// # Safety
/// `active` points to a Boolean.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn AccessTrigger(index: u16, active: *mut u8) {
    unsafe {
        if index == 0 {
            let t = f32::from_bits(TIME.load(Ordering::SeqCst));
            *active = (t as u32 % 2 == 1) as u8;
        }
    }
}

static POINTER: AtomicU32 = AtomicU32::new(0);

/// openOMSI's `AccessScriptTexturePointer`: remembers the last event (x, y, kind), which
/// `AccessScriptTexture` shows in the second pixel.
#[unsafe(no_mangle)]
pub extern "system" fn AccessScriptTexturePointer(index: u16, kind: i32, x: f32, y: f32) {
    if index == 0 {
        POINTER.store(u32::from_le_bytes([x as u8, y as u8, kind as u8, 1]), Ordering::SeqCst);
    }
}

/// openOMSI's `AccessScriptTexture`: entry 0 is painted with each pixel's x and y in red
/// and green, blue 7; the first pixel's alpha counts the calls, from what the buffer kept
/// (so the buffer must persist between frames). Entry 1 is never written.
///
/// # Safety
/// `pixels` is `width` x `height` RGBA bytes, `write` points to a Boolean.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn AccessScriptTexture(index: u16, width: i32, height: i32, pixels: *mut u8, write: *mut u8) {
    unsafe {
        if index != 0 || width <= 0 || height <= 0 {
            return;
        }
        let (w, h) = (width as usize, height as usize);
        let px = std::slice::from_raw_parts_mut(pixels, w * h * 4);
        let calls = px[3].wrapping_add(1);
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) * 4;
                px[i..i + 4].copy_from_slice(&[x as u8, y as u8, 7, 255]);
            }
        }
        px[3] = calls;
        // the second pixel: the last mouse event (x, y, kind, 1), if there was one
        let p = POINTER.load(Ordering::SeqCst).to_le_bytes();
        if p[3] == 1 && px.len() >= 8 {
            px[4..8].copy_from_slice(&p);
        }
        *write = 1;
    }
}

/// openOMSI's `AccessTicketRequest`: a child (under 15) needs no ticket; a ride of `n`
/// stops of the trip costs `n` x 10, is called "<destination>, <n> stops" and asked for
/// with the voice line `Zone_<n>`; a ride the trip does not know is left as it is.
///
/// # Safety
/// `stop` and `destination` are zero-terminated wide texts; `name` and `voice` buffers of
/// 256 wide characters; the rest point to an Integer, a Single and a Boolean.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "system" fn AccessTicketRequest(
    _person: u32,
    age: f32,
    _stop: *const u16,
    destination: *const u16,
    stop_number: i32,
    destination_number: i32,
    kind: *mut i32,
    _ticket: *mut i32,
    price: *mut f32,
    name: *mut u16,
    voice: *mut u16,
    write: *mut u8,
) {
    unsafe {
        if age < 15.0 {
            *kind = 0;
            *write = 1;
            return;
        }
        if stop_number <= 0 || destination_number <= stop_number {
            return;
        }
        let mut len = 0;
        while *destination.add(len) != 0 {
            len += 1;
        }
        let dest = String::from_utf16_lossy(std::slice::from_raw_parts(destination, len));
        let n = destination_number - stop_number;
        *price = n as f32 * 10.0;
        let put = |buf: *mut u16, text: &str| {
            let t: Vec<u16> = text.encode_utf16().take(255).chain(std::iter::once(0)).collect();
            std::ptr::copy_nonoverlapping(t.as_ptr(), buf, t.len());
        };
        put(name, &format!("{dest}, {n} stops"));
        put(voice, &format!("Zone_{n}"));
        *write = 1;
    }
}

/// Homebrew's i686 MinGW links no unwinder the prebuilt standard library can use; built
/// with `panic=abort` nothing unwinds, and this stands in for the one symbol it names.
#[cfg(all(target_os = "windows", target_arch = "x86", target_env = "gnu"))]
#[unsafe(no_mangle)]
pub extern "C" fn _Unwind_Resume() -> ! {
    std::process::abort()
}
