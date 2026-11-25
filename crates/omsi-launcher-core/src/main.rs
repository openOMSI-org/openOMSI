//! `openomsi-launcher`: the launcher's commands for a terminal (`--cli <command> [json]`), and
//! otherwise the game binary beside this one with the same arguments (a plain start of it
//! opens the desktop launcher, `openomsi-launcher-app`).

fn main() {
    omsi_cfg::migrate_legacy_data_dir();
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(|a| a == "--cli").unwrap_or(false) {
        let cmd = args.get(2).cloned().unwrap_or_default();
        let arg = args.get(3).cloned().unwrap_or_else(|| "{}".into());
        match omsi_launcher_lib::cli(&cmd, &arg) {
            Ok(v) => println!("{}", serde_json::to_string_pretty(&v).unwrap()),
            Err(e) => {
                eprintln!("error: {e:#}");
                std::process::exit(1);
            }
        }
        return;
    }
    let game = omsi_launcher_lib::load_config().game;
    let game = if game.trim().is_empty() {
        std::env::current_exe()
            .ok()
            .and_then(|e| {
                e.parent().map(|d| {
                    d.join(if cfg!(windows) {
                        "openomsi.exe"
                    } else {
                        "openomsi"
                    })
                })
            })
            .unwrap_or_default()
    } else {
        std::path::PathBuf::from(game)
    };
    match std::process::Command::new(&game).args(&args[1..]).status() {
        Ok(s) => std::process::exit(s.code().unwrap_or(0)),
        Err(e) => {
            eprintln!("starting {}: {e}", game.display());
            std::process::exit(1);
        }
    }
}
