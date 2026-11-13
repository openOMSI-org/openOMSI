//! One per-vehicle sharing policy across LAN, dedicated fleet and player uploads.
//! This is a software opt-out, not DRM against modified clients.
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub(super) fn is_descriptor(name: &str) -> bool {
    let name = name.rsplit(['/', '\\']).next().unwrap_or(name);
    name.rsplit_once('.').is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("bus") || ext.eq_ignore_ascii_case("ovh"))
}

/// Explicitly mark any on-disk content folder as non-distributable.
/// Files nested in a marked folder cannot leave as a public vehicle's dependencies.
const PRIVATE_MARKER: &str = ".openomsi-no-share";

fn marked_private(path: &Path) -> bool {
    path.parent().is_some_and(|parent| parent.ancestors().any(|dir| dir.join(PRIVATE_MARKER).exists()))
}

fn strict_mode() -> bool {
    omsi_cfg::flags::OMSI_LAN_REQUIRE_TRANSFER_OPT_IN.is_set()
}

fn policy_allows(value: Option<bool>, strict: bool) -> bool {
    value.unwrap_or(!strict)
}

/// Inspect incoming `.bus`/`.ovh` bytes before storing them. A modified or
/// outdated sender can ignore its own policy, so the receiver must also check.
/// Other assets are not independently labelled with redistribution permissions.
pub(super) fn allows_description_bytes(name: &str, bytes: &[u8]) -> bool {
    !is_descriptor(name) || (transfers_enabled()
        && policy_allows(omsi_vehicle::Vehicle::parse(&omsi_cfg::CfgFile::from_bytes(name, bytes)).lan_file_transfer, strict_mode()))
}

/// Fail closed if a referenced vehicle description cannot be read.
pub(super) fn transfers_enabled() -> bool {
    !omsi_cfg::flags::OMSI_NO_LAN_VEHICLE_TRANSFER.is_set()
}

pub(super) fn allows_file(path: &Path) -> bool {
    if !transfers_enabled() || marked_private(path) { return false; }
    match omsi_vehicle::Vehicle::load(path) {
        Ok(v) => policy_allows(v.lan_file_transfer, strict_mode()),
        Err(e) => {
            log::warn!("LAN transfer policy: {} cannot be checked: {e}", path.display());
            false
        }
    }
}

pub(super) fn allows_bus(root: &Path, bus: &str) -> bool {
    let Some(bus) = omsi_net::vehicle_path(bus) else { return false };
    let path = omsi_cfg::find_in_roots(&bus).map(|(_, p)| p).unwrap_or_else(|| root.join(&bus));
    allows_file(&path)
}

/// Also reject a protected articulated rear section or other referenced .bus/.ovh.
pub(super) fn allows_dependencies(files: &[(String, PathBuf)]) -> bool {
    if !transfers_enabled() || files.iter().any(|(_, path)| marked_private(path)) { return false; }
    let descriptions: Vec<_> = files.iter().filter(|(name, _)| is_descriptor(name)).collect();
    if strict_mode() && descriptions.is_empty() { return false; }
    descriptions.iter().all(|(_, path)| allows_file(path))
}

fn vehicle_folder(name: &str) -> Option<String> {
    let normalized = name.replace('\\', "/").to_ascii_lowercase();
    let mut parts = normalized.split('/');
    let top = parts.next()?;
    if top != "vehicles" && top != "trains" { return None; }
    let folder = parts.next()?;
    if folder.is_empty() { return None; }
    Some(format!("{top}/{folder}/"))
}

/// The host map collector normally includes entire Vehicles/<folder> subtrees.
/// Exclude the entire folder when any included vehicle descriptor forbids transfers.
pub(super) fn filter_session_files(files: &mut Vec<(String, PathBuf)>) {
    if !transfers_enabled() {
        files.retain(|(name, _)| vehicle_folder(name).is_none());
        return;
    }
    let private: HashSet<String> = files.iter()
        .filter(|(name, path)| is_descriptor(name) && !allows_file(path))
        .filter_map(|(name, _)| vehicle_folder(name))
        .collect();
    if !private.is_empty() {
        log::warn!("LAN mods: {} private vehicle folder(s) excluded from the session manifest", private.len());
    }
    files.retain(|(name, path)| {
        let name = name.replace('\\', "/").to_ascii_lowercase();
        !marked_private(path) && !private.iter().any(|folder| name.starts_with(folder))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_mode_requires_an_explicit_yes_but_legacy_mode_does_not() {
        assert!(policy_allows(None, false));
        assert!(policy_allows(Some(true), false));
        assert!(!policy_allows(Some(false), false));
        assert!(!policy_allows(None, true));
        assert!(policy_allows(Some(true), true));
        assert!(!policy_allows(Some(false), true));
    }

    #[test]
    fn private_parent_marker_blocks_shared_assets_from_an_otherwise_public_bus() {
        let dir = std::env::temp_dir().join(format!("openomsi-private-assets-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let bus_dir = dir.join("Vehicles/Public");
        let asset_dir = dir.join("Fonts/Private");
        std::fs::create_dir_all(&bus_dir).unwrap();
        std::fs::create_dir_all(&asset_dir).unwrap();
        let bus = bus_dir.join("public.bus");
        std::fs::write(&bus, "[friendlyname]\nMaker\nPublic\nDefault\n").unwrap();
        let font = asset_dir.join("secret.oft");
        std::fs::write(&font, "test").unwrap();
        std::fs::write(asset_dir.join(PRIVATE_MARKER), "").unwrap();
        let files = vec![
            ("Vehicles/Public/public.bus".into(), bus.clone()),
            ("Fonts/Private/secret.oft".into(), font.clone()),
        ];
        assert!(allows_file(&bus));
        assert!(!allows_dependencies(&files));
        let mut map_files = files;
        filter_session_files(&mut map_files);
        assert_eq!(map_files.len(), 1);
        assert_eq!(map_files[0].0, "Vehicles/Public/public.bus");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn private_vehicle_folder_is_excluded_but_public_map_is_not() {
        let dir = std::env::temp_dir().join(format!("openomsi-lan-private-policy-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let private = dir.join("private.bus");
        let public = dir.join("public.bus");
        std::fs::write(&private, "[openomsi_multiplayer]\nallow_file_transfer 0\n").unwrap();
        std::fs::write(&public, "[friendlyname]\nA\nB\nC\n").unwrap();
        assert!(!allows_file(&private));
        assert!(allows_file(&public));
        assert!(!allows_dependencies(&[("Vehicles/P/private.bus".into(), private.clone())]));
        let mut files = vec![
            ("Vehicles/P/private.bus".into(), private),
            ("Vehicles/P/texture/x.dds".into(), dir.join("x.dds")),
            ("Vehicles/Q/public.bus".into(), public),
            ("maps/Test/global.cfg".into(), dir.join("global.cfg")),
        ];
        filter_session_files(&mut files);
        assert_eq!(files.len(), 2);
        assert!(files.iter().any(|(n, _)| n == "Vehicles/Q/public.bus"));
        assert!(files.iter().any(|(n, _)| n == "maps/Test/global.cfg"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
