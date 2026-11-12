//! One per-vehicle sharing policy across LAN, dedicated fleet and player uploads.
//! This is a software opt-out, not DRM against modified clients.
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub(super) fn is_descriptor(name: &str) -> bool {
    let name = name.rsplit(['/', '\\']).next().unwrap_or(name);
    name.rsplit_once('.').is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("bus") || ext.eq_ignore_ascii_case("ovh"))
}

/// Inspect incoming `.bus`/`.ovh` bytes before storing them. A modified or
/// outdated sender can ignore its own policy, so the receiver must also check.
/// Other assets are not independently labelled with redistribution permissions.
pub(super) fn allows_description_bytes(name: &str, bytes: &[u8]) -> bool {
    !is_descriptor(name) || omsi_vehicle::Vehicle::parse(&omsi_cfg::CfgFile::from_bytes(name, bytes)).allows_file_transfer()
}

/// Fail closed if a referenced vehicle description cannot be read.
pub(super) fn transfers_enabled() -> bool {
    !omsi_cfg::flags::OMSI_NO_LAN_VEHICLE_TRANSFER.is_set()
}

pub(super) fn allows_file(path: &Path) -> bool {
    if !transfers_enabled() { return false; }
    match omsi_vehicle::Vehicle::load(path) {
        Ok(v) => v.allows_file_transfer(),
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
    if !transfers_enabled() { return false; }
    files.iter().filter(|(name, _)| is_descriptor(name)).all(|(_, path)| allows_file(path))
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
    if private.is_empty() { return; }
    log::warn!("LAN mods: {} private vehicle folder(s) excluded from the session manifest", private.len());
    files.retain(|(name, _)| {
        let name = name.replace('\\', "/").to_ascii_lowercase();
        !private.iter().any(|folder| name.starts_with(folder))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

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
