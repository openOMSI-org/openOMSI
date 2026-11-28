//! Explicit, restart-only consent for content distribution from the
//! experimental native host. Old server.cfg files default both switches ON,
//! so looking only at ServerCfg booleans would expose map and bus files.

use anyhow::{Context, Result, ensure};
use omsi_lan::server_cfg::ServerCfg;
use std::path::Path;

/// Match ServerCfg's last-definition-wins parsing (including case and
/// whitespace). An absent key is never considered consent to send files.
fn explicit_enabled(text: &str, key: &str) -> bool {
    let mut selected = None;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some((name, value)) = line.split_once('=') {
            if name.trim().eq_ignore_ascii_case(key) {
                selected = Some(value.trim().to_ascii_lowercase());
            }
        }
    }
    selected
        .as_deref()
        .is_some_and(|v| matches!(v, "1" | "true" | "yes" | "on"))
}

pub fn verify_explicit_opt_in(cfg: &ServerCfg, path: &Path) -> Result<()> {
    if !cfg.accept_uploads && !cfg.share_maps {
        return Ok(());
    }
    let text = std::fs::read_to_string(path).with_context(|| {
        format!(
            "reading the experimental mod-sharing policy from {}",
            path.display()
        )
    })?;
    ensure!(
        !cfg.accept_uploads || explicit_enabled(&text, "accept_uploads"),
        "experimental client-bus uploads require an explicit accept_uploads=1 in server.cfg"
    );
    ensure!(
        !cfg.share_maps || explicit_enabled(&text, "share_maps"),
        "experimental map distribution requires an explicit share_maps=1 in server.cfg"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(text: &str) -> (ServerCfg, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "omsi-native-mod-consent-{}-{:?}.cfg",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::write(&path, text).unwrap();
        let cfg = ServerCfg::load(&path, 256).unwrap();
        (cfg, path)
    }

    #[test]
    fn transfer_disabled_is_the_default_for_experimental_example() {
        let (cfg, path) = config(include_str!(
            "../../../scripts/server-headless/server-test.cfg"
        ));
        assert!(!cfg.share_maps);
        assert!(!cfg.accept_uploads);
        assert!(verify_explicit_opt_in(&cfg, &path).is_ok());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn legacy_missing_switches_cannot_auto_export_mods() {
        let (cfg, path) = config("map = Maps/Grundorf/global.cfg\n");
        assert!(cfg.share_maps && cfg.accept_uploads);
        assert!(verify_explicit_opt_in(&cfg, &path).is_err());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn independent_map_and_bus_upload_opt_ins_are_respected() {
        for (share, upload) in [(false, false), (true, false), (false, true), (true, true)] {
            let (cfg, path) = config(&format!(
                "share_maps = {}\naccept_uploads = {}\nparked_buses = 0\n",
                u8::from(share),
                u8::from(upload)
            ));
            assert_eq!(cfg.share_maps, share);
            assert_eq!(cfg.accept_uploads, upload);
            assert!(verify_explicit_opt_in(&cfg, &path).is_ok());
            std::fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn last_setting_wins_and_comments_do_not_count_as_consent() {
        assert!(!explicit_enabled("# share_maps = 1\n", "share_maps"));
        assert!(!explicit_enabled(
            "share_maps = 1\nshare_maps = 0\n",
            "share_maps"
        ));
        assert!(explicit_enabled(
            "share_maps = 0\nSHARE_MAPS = On\n",
            "share_maps"
        ));
        assert!(!explicit_enabled(
            "accept_uploads = off\n",
            "accept_uploads"
        ));
    }
}
