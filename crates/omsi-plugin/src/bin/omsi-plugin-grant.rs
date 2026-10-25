use omsi_plugin::api::process::{ProcessGrant, ProcessGrants};
use std::path::PathBuf;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = std::env::args_os().skip(1);
    let usage = "usage: omsi-plugin-grant ABSOLUTE_ENTRY ABSOLUTE_PLUGIN_FOLDER [RELATIVE_HELPER ...]; inspect code first, save stdout outside the plugin folder";
    let entry = PathBuf::from(args.next().ok_or(usage)?);
    let folder = PathBuf::from(args.next().ok_or(usage)?);
    let helpers = args
        .map(|s| {
            s.into_string()
                .map_err(|_| "helper paths must be UTF-8".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let grant = ProcessGrant::capture(&entry, &folder, &helpers)?;
    let manifest = ProcessGrants {
        grants: vec![grant],
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?
    );
    Ok(())
}
