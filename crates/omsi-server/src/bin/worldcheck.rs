//! GPU-free map diagnostics for the future native dedicated server.
//! This is intentionally NOT a multiplayer server or release artifact.
use anyhow::{Context, Result};
use omsi_server::world::HostWorld;
use std::path::{Path, PathBuf};

fn world_path(root: &Path, map: &str) -> PathBuf {
    let p = omsi_cfg::resolve_path(root, map);
    if p.extension().is_some_and(|s| s.eq_ignore_ascii_case("cfg")) {
        p
    } else {
        omsi_cfg::resolve_path(&p, "global.cfg")
    }
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let (Some(root), Some(map)) = (args.next(), args.next()) else {
        anyhow::bail!(
            "Usage: omsi-worldcheck <OMSI2-root> <map-directory-or-global.cfg> [YYYYMMDD]"
        );
    };
    let date = args.next().unwrap_or_else(|| "19890501".to_string());
    anyhow::ensure!(args.next().is_none(), "unexpected extra argument");
    let plain_date = date.replace('-', "");
    anyhow::ensure!(
        plain_date.len() == 8 && plain_date.bytes().all(|c| c.is_ascii_digit()),
        "date must be YYYYMMDD"
    );
    let root = PathBuf::from(root);
    let global_path = world_path(&root, &map);
    let world = HostWorld::open(&root, &global_path, plain_date.parse()?)
        .with_context(|| format!("CPU-only map startup failed for {}", global_path.display()))?;
    anyhow::ensure!(
        world.index.tiles_read > 0,
        "no usable OMSI tiles were indexed"
    );
    println!(
        "CPU-only world check: map='{}', tiles={}, damaged_tiles={}, indexed_objects={}, files={} (renderer/audio not initialized)",
        world.global.name,
        world.index.tiles_read,
        world.index.tiles_failed,
        world.index.objects.len(),
        world.index.files.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn global_cfg_input_is_accepted_as_file_or_directory() {
        let root = Path::new("/omsi");
        assert!(world_path(root, "maps/Map").ends_with("maps/Map/global.cfg"));
        assert!(world_path(root, "maps/Map/global.cfg").ends_with("maps/Map/global.cfg"));
    }
}
