//! The launcher's update from end to end, without a window: put this program into an
//! installed openOMSI (as `openomsi-launcher-app`, in place of the launcher), point
//! `OMSI_UPDATE_URL` at a release description, and it checks, downloads, installs and starts
//! the new program the way the launcher's page does (`update_check`, `update_install`,
//! `update_status` until "restarting", then the window's `update_relaunch`).

use serde_json::Value;
use std::time::{Duration, Instant};

struct NoWindow;

impl omsi_launcher_api::Platform for NoWindow {
    fn renderer(&self, _req: Value, _wait: Duration) -> Result<(Value, Vec<u8>), String> {
        Err("no renderer here".into())
    }
}

fn call(cmd: &str, args: Value) -> Value {
    let v = omsi_launcher_api::call(cmd, &args, &NoWindow).unwrap_or_else(|e| panic!("{cmd}: {e}"));
    println!("{cmd}: {v}");
    v
}

fn main() {
    let _ = call("update_check", serde_json::json!({ "again": true }));
    let t0 = Instant::now();
    let mut asked = false;
    loop {
        let s = call("update_status", serde_json::json!({}));
        match s["state"].as_str().unwrap_or("") {
            "available" if !asked => {
                asked = true;
                call("update_install", serde_json::json!({}));
            }
            "restarting" => break,
            "failed" => std::process::exit(1),
            _ => {}
        }
        if t0.elapsed() > Duration::from_secs(300) {
            eprintln!("no end within 5 minutes");
            std::process::exit(1);
        }
        std::thread::sleep(Duration::from_millis(700));
    }
    // (the window's `update_relaunch`)
    let place = omsi_platform::updater::install_place().expect("install place");
    let game = place.exe.with_file_name(if cfg!(windows) {
        "openomsi.exe"
    } else {
        "openomsi"
    });
    let program = if game.is_file() {
        game
    } else {
        place.exe.clone()
    };
    println!("relaunch: {}", program.display());
    let status = std::process::Command::new(&program)
        .current_dir(&place.dir)
        .status()
        .expect("start");
    println!("the new program ended with {status}");
}
