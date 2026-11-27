//! Native dedicated server command. No renderer, scene, wgpu or audio.
//!
//! Full authority is deliberately disabled pending map/vehicle/network parity.
//! Explicit --experimental-free-roam runs a restricted LAN relay for testing.
use anyhow::{Context, Result, bail};
use omsi_server::{runtime, startup};
use std::path::PathBuf;

struct ConsoleLogger;
static LOGGER: ConsoleLogger = ConsoleLogger;

impl log::Log for ConsoleLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Info
    }
    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            eprintln!("[{}] {}", record.level(), record.args());
        }
    }
    fn flush(&self) {}
}

fn main() -> Result<()> {
    // Dedicated hosts have no game window to print the normal log to.
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(log::LevelFilter::Info);
    let mut args = std::env::args().skip(1);
    let build = option_env!("OPENOMSI_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"));
    if let Some(flag) = args.next() {
        if flag == "--version" && args.next().is_none() {
            println!("{build}");
            return Ok(());
        }
        if flag == "--self-test" && args.next().is_none() {
            let exe = std::env::current_exe().context("locating headless executable")?;
            anyhow::ensure!(exe.is_file(), "headless executable is not readable");
            println!("headless {build}: self-test passed");
            return Ok(());
        }
        args = std::env::args().skip(1);
    }
    let Some(root) = args.next() else {
        bail!(
            "Usage: omsi-server <OMSI2-root> <server.cfg> (--experimental-free-roam | --experimental-simulation) [--ticks N]"
        );
    };
    let config = args.next().context("missing server.cfg path")?;
    let mut experimental = false;
    let mut simulation = false;
    let mut max_ticks = None;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--experimental-free-roam" if !experimental => experimental = true,
            "--experimental-simulation" if !simulation => simulation = true,
            "--ticks" if max_ticks.is_none() => {
                let ticks: u64 = args
                    .next()
                    .context("--ticks requires a positive integer")?
                    .parse()?;
                anyhow::ensure!((1..=60_000).contains(&ticks), "--ticks must be 1..=60000");
                max_ticks = Some(ticks);
            }
            _ => bail!("unsupported or repeated option {flag}"),
        }
    }
    if experimental == simulation {
        bail!(
            "Choose exactly one experimental mode: --experimental-free-roam or --experimental-simulation. Use legacy openomsi --server for production"
        );
    }
    let root = PathBuf::from(root);
    anyhow::ensure!(root.is_dir(), "OMSI root must be an installed directory");
    let config = PathBuf::from(config);
    anyhow::ensure!(config.is_file(), "server.cfg must already exist");
    // The graphical server registers its installed content roots before it
    // loads global.cfg. Without this, maps with no [entrypoints] list cannot
    // discover entrypoint .sco definitions via find_in_roots(), and other
    // scenery/vehicle references may be missed by the CPU map index.
    // Match the legacy graphical server's content search order:
    // user content, mounted content archives, then the OMSI installation.
    // The map and its .sco entrypoint objects may live in an overlay.
    if let Some(content) = omsi_cfg::content_dir() {
        match omsi_cfg::ensure_content_layout(&content) {
            Ok(()) => omsi_cfg::add_content_root(content.clone()),
            Err(error) => log::warn!("content folder {}: {error}", content.display()),
        }
    }
    omsi_cfg::vfs::mount_env_zips();
    if let Some(content) = omsi_cfg::content_dir() {
        omsi_cfg::vfs::mount_dir_zips(&content.join("Archives"));
    }
    omsi_cfg::add_content_root(root.clone());
    // Validate operator restrictions before initializing any administration,
    // global upload policy, gateway or console reader.
    let parsed = omsi_lan::server_cfg::ServerCfg::load(
        &config,
        omsi_server::population::MAX_EXPERIMENTAL_TRAFFIC,
    )?;
    if simulation {
        runtime::allow_simulated_roam(&parsed)?;
    } else {
        runtime::allow_free_roam(&parsed)?;
    }
    let plan = startup::prepare(
        &config,
        &root,
        None,
        omsi_server::population::MAX_EXPERIMENTAL_TRAFFIC,
        env!("CARGO_PKG_VERSION"),
        &|_, _| String::new(),
    )?;
    if simulation {
        runtime::simulated_roam(&root, &plan, max_ticks)
    } else {
        runtime::free_roam(&root, &plan, max_ticks)
    }
}
