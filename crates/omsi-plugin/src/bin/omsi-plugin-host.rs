//! Runs one OMSI plugin library for the game (see `omsi_plugin::Remote`): built for 32-bit
//! Windows (`i686-pc-windows-*`, shipped as `omsi-plugin-host32.exe`) it loads the 32-bit
//! DLLs OMSI's plugins are; on Windows the game starts it directly, elsewhere through Wine.
//!
//! usage: omsi-plugin-host <library>   (then the protocol of `omsi_plugin::wire` on stdio)
#[cfg(windows)]
#[path = "../steam_error_dialog.rs"]
mod steam_error_dialog;

use omsi_plugin::{wire, Library};
use std::io::{BufReader, BufWriter, Write};

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: omsi-plugin-host <library>");
        std::process::exit(2);
    };
    #[cfg(windows)]
    steam_error_dialog::watch(&path);
    let mut input = BufReader::new(std::io::stdin().lock());
    let mut out = BufWriter::new(std::io::stdout().lock());
    let mut lib = match Library::load(std::path::Path::new(&path)) {
        Ok(l) => Some(l),
        Err(e) => {
            eprintln!("omsi-plugin-host: {path}: {e}");
            None
        }
    };
    loop {
        let Ok(op) = wire::get_u8(&mut input) else { return };
        let res = match (op, &mut lib) {
            (wire::START, Some(lib)) => {
                lib.start();
                wire::put_u8(&mut out, 1).and_then(|_| wire::put_u8(&mut out, lib.procs().flags()))
            }
            (wire::START, None) => {
                let _ = wire::put_u8(&mut out, 0).and_then(|_| wire::put_u8(&mut out, 0)).and_then(|_| out.flush());
                return;
            }
            (wire::FRAME, Some(lib)) => {
                // (the script textures travel only for a library with AccessScriptTexture,
                // as the game knows from the flags of START)
                // (and the passengers' tickets only for one with AccessTicketRequest)
                let (textures, tickets) = (lib.procs().texture, lib.procs().ticket);
                let frame = wire::get_frame(&mut input, textures, lib.procs().pointer).and_then(|mut f| {
                    if tickets {
                        f.tickets = wire::get_tickets(&mut input)?;
                    }
                    Ok(f)
                });
                match frame {
                    Ok(f) => {
                        let reply = lib.frame(&f);
                        wire::put_reply(&mut out, &reply, textures).and_then(|_| if tickets { wire::put_ticket_answers(&mut out, &reply.tickets) } else { Ok(()) })
                    }
                    Err(_) => return,
                }
            }
            (wire::FINALIZE, Some(lib)) => {
                lib.finalize();
                let _ = wire::put_u8(&mut out, 1).and_then(|_| out.flush());
                return;
            }
            _ => return,
        };
        if res.and_then(|_| out.flush()).is_err() {
            return;
        }
    }
}

/// Homebrew's i686 MinGW links no unwinder the prebuilt standard library can use; built
/// with `panic=abort` nothing unwinds, and this stands in for the one symbol it names.
#[cfg(all(target_os = "windows", target_arch = "x86", target_env = "gnu"))]
#[unsafe(no_mangle)]
pub extern "C" fn _Unwind_Resume() -> ! {
    std::process::abort()
}
