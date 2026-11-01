//! On-demand, server-owned vehicle packages for one selected model and repaint.
//! Separate from share_maps and from bus uploads offered by other players.
use super::*;
use crate::server_cfg::SERVER_VEHICLES;

type Package = Arc<(Manifest, Vec<PathBuf>)>;
type Cached = Arc<std::sync::OnceLock<Result<Package, String>>>;
pub(super) type Cache = Mutex<HashMap<String, (Instant, Cached)>>;

/// Bound memory while keeping every request cached, including negative answers.
/// One builder per key runs outside the map lock, so case variants and simultaneous
/// requests cannot repeatedly load and hash the same vehicle.
pub(super) fn cached(cache: &Cache, key: String, build: impl FnOnce() -> Result<(Manifest, Vec<PathBuf>), String>) -> Result<Package, String> {
    let entry = {
        let mut map = cache.lock().unwrap_or_else(|e| e.into_inner());
        map.retain(|_, (at, value)| at.elapsed() < Duration::from_secs(if value.get().is_some_and(Result::is_err) { 30 } else { 600 }));
        if let Some((at, hit)) = map.get_mut(&key) {
            *at = Instant::now();
            hit.clone()
        } else {
            if map.len() >= 128 {
                if let Some(oldest) = map.iter().min_by_key(|(_, (at, _))| *at).map(|(key, _)| key.clone()) { map.remove(&oldest); }
            }
            let entry = Arc::new(std::sync::OnceLock::new());
            map.insert(key, (Instant::now(), entry.clone()));
            entry
        }
    };
    entry.get_or_init(|| build().map(Arc::new)).clone()
}

pub(super) fn cache_key(exact: bool, bus: &str, paint: &str) -> String {
    format!("{}:{}\u{1f}{}", exact, omsi_cfg::name_key(&bus.replace('\\', "/")), omsi_cfg::name_key(paint))
}

// A name inside Vehicles/ or another content folder is not enough to make a
// disk file safe to serve: a mod can include a symlink into a private directory.
// Check the resolved file against the *declared* content root, not just its
// textual path. Virtual entries in sealed stores/archives do not have a normal
// on-disk path and remain subject to the manifest and quarantine checks.
pub(super) fn trusted_source(root: &Path, source: &Path) -> bool {
    let mut roots = omsi_cfg::content_roots();
    roots.push(root.to_path_buf());
    let Some(declared) = roots
        .iter()
        .filter(|r| source.starts_with(r))
        .max_by_key(|r| r.components().count())
    else {
        return false;
    };
    // Physical files, including symlinks, must resolve inside their declared
    // content root. Do not let an existing symlink use the virtual-file bypass.
    if std::fs::symlink_metadata(source).is_ok() {
        let (Ok(file), Ok(base)) = (source.canonicalize(), declared.canonicalize()) else {
            return false;
        };
        return file.is_file() && file.starts_with(base);
    }
    omsi_cfg::vfs::is_virtual(source)
}

/// The selected bus is offered only when its canonical path is explicitly configured.
pub(super) fn package(root: &Path, base: &(Manifest, Vec<PathBuf>), bus: &str, paint: &str) -> Result<(Manifest, Vec<PathBuf>), String> {
    let bus = omsi_net::vehicle_path(bus).ok_or("invalid bus path")?;
    let Some(allowed) = SERVER_VEHICLES.get().filter(|v| !v.is_empty()) else {
        return Err("fleet not configured".into());
    };
    if !allowed.iter().any(|v| omsi_net::vehicle_path(v).is_some_and(|b| b.eq_ignore_ascii_case(&bus))) {
        return Err("that bus is not in the dedicated server's vehicles list".into());
    }
    if refuse_path(&bus).is_some() { return Err("refused vehicle path".into()); }
    let path = omsi_cfg::find_in_roots(&bus).map(|(_, p)| p).unwrap_or_else(|| root.join(&bus));
    if !trusted_source(root, &path) {
        return Err("the selected bus resolves outside the trusted content roots".into());
    }
    let ty = omsi_sim::VehicleType::load(root, &path).map_err(|e| format!("{bus}: {e}"))?;
    if !paint.is_empty() && !ty.paint_schemes.iter().take(128).any(|p| p.name.eq_ignore_ascii_case(paint)) {
        return Err("the selected repaint is not offered by the server".into());
    }
    let (manifest, sources) = model_manifest(root, &bus, &ty)?;
    let entries = manifest.entries.clone();
    // Add the selected version on top of the optional map manifest. Keep GET indexes
    // aligned with the source paths; prefer this exact vehicle over map dependencies.
    let mut combined: HashMap<String, (Entry, PathBuf)> = base.0.entries.iter().cloned()
        .zip(base.1.iter().cloned()).map(|(e,p)| (omsi_cfg::name_key(&e.path), (e,p))).collect();
    for (entry, source) in entries.into_iter().zip(sources) {
        combined.insert(omsi_cfg::name_key(&entry.path), (entry, source));
    }
    let mut combined: Vec<_> = combined.into_values().collect();
    combined.sort_by(|a,b| a.0.path.cmp(&b.0.path));
    let total: u64 = combined.iter().map(|(e,_)| e.size).sum();
    if combined.len() > MAX_FILES || total > MAX_TOTAL { return Err("the combined package exceeds session limits".into()); }
    let (entries, sources) = combined.into_iter().unzip();
    log::info!("LAN mods: server fleet offers {bus} ({paint}), {} files ({:.1} MB)", manifest.entries.len(), manifest.size() as f64 / 1e6);
    Ok((Manifest { map: base.0.map.clone(), bus, entries }, sources))
}

/// The installed complete model fingerprint, with no network downloads or mounts.
/// This permits a local-only join when the mod transport is unavailable.
pub fn fingerprint(root: &Path, bus: &str) -> Result<String, String> {
    let bus = omsi_net::vehicle_path(bus).ok_or("invalid bus path")?;
    let path = omsi_cfg::find_in_roots(&bus).map(|(_, p)| p).unwrap_or_else(|| root.join(&bus));
    if !trusted_source(root, &path) { return Err("untrusted bus path".into()); }
    let ty = omsi_sim::VehicleType::load(root, &path).map_err(|e| e.to_string())?;
    model_manifest(root, &bus, &ty).map(|(m, _)| m.fingerprint())
}

fn model_manifest(root: &Path, bus: &str, ty: &omsi_sim::VehicleType) -> Result<(identity::VehicleManifest, Vec<PathBuf>), String> {
    // The server offers the *complete configured model*, including the default
    // and all advertised repaint dependencies. A player with the same bus but
    // without a server repaint must obtain that repaint without replacing OMSI 2.
    // BTreeSet deduplicates shared textures, models, scripts and sounds.
    let mut all = std::collections::BTreeSet::new();
    for scheme in std::iter::once(None).chain((0..ty.paint_schemes.len().min(128)).map(Some)) {
        all.extend(bus_files::of_type(root, &ty, scheme, None));
    }
    if all.iter().any(|path| !trusted_source(root, path)) {
        return Err("selected vehicle dependencies resolve outside the trusted content roots".into());
    }
    let files = upload::listed(root, all.into_iter().collect());
    let entries = upload::hashed(&files);
    // Hashing skips files it cannot safely read. Never zip a shortened hash
    // list with the original paths: that could serve the wrong bytes for a hash.
    if entries.len() != files.len() {
        return Err(format!("{} of {} selected vehicle files could not be verified", files.len() - entries.len(), files.len()));
    }
    let manifest = identity::VehicleManifest { bus: bus.to_string(), entries }.validated()?;
    if manifest.entries.len() > MAX_FILES || manifest.size() > MAX_TOTAL {
        return Err("server vehicle package exceeds the download limits".into());
    }
    if let Some(e) = manifest.entries.iter().find(|e| refuse_path(&e.path).is_some() || e.size > MAX_FILE) {
        return Err(format!("server vehicle contains a refused file: {}", e.path));
    }
    Ok((manifest, files.into_iter().map(|(_, p)| p).collect()))
}

/// The exact bus and selected paint used for remote-player fingerprint checks.
/// Unlike LISTBUS (which offers every advertised repaint for the local driver),
/// this must contain only the selected paint's dependencies. A different version
/// must never be passed off as another player's fingerprint.
pub(super) fn exact(root: &Path, bus: &str, paint: &str) -> Result<(Manifest, Vec<PathBuf>), String> {
    let bus = omsi_net::vehicle_path(bus).ok_or("invalid bus path")?;
    let Some(allowed) = SERVER_VEHICLES.get().filter(|v| !v.is_empty()) else {
        return Err("fleet not configured".into());
    };
    if !allowed.iter().any(|v| omsi_net::vehicle_path(v).is_some_and(|b| b.eq_ignore_ascii_case(&bus))) {
        return Err("that bus is not in the dedicated server's vehicles list".into());
    }
    if refuse_path(&bus).is_some() { return Err("refused vehicle path".into()); }
    let path = omsi_cfg::find_in_roots(&bus).map(|(_, p)| p).unwrap_or_else(|| root.join(&bus));
    if !trusted_source(root, &path) {
        return Err("the selected bus resolves outside the trusted content roots".into());
    }
    let ty = omsi_sim::VehicleType::load(root, &path).map_err(|e| format!("{bus}: {e}"))?;
    let scheme = if paint.is_empty() { None } else {
        Some(ty.paint_schemes.iter().take(128).position(|p| p.name.eq_ignore_ascii_case(paint))
            .ok_or("the selected repaint is not offered by the server")?)
    };
    let dependencies = bus_files::of_type(root, &ty, scheme, None);
    if dependencies.iter().any(|path| !trusted_source(root, path)) {
        return Err("selected vehicle dependencies resolve outside the trusted content roots".into());
    }
    let files = upload::listed(root, dependencies);
    let entries = upload::hashed(&files);
    if entries.len() != files.len() {
        return Err(format!("{} of {} selected vehicle files could not be verified", files.len() - entries.len(), files.len()));
    }
    let m = identity::VehicleManifest { bus: bus.clone(), entries }.validated()?;
    if m.entries.len() > MAX_FILES || m.size() > MAX_TOTAL {
        return Err("server vehicle exceeds the transfer limits".into());
    }
    if let Some(e) = m.entries.iter().find(|e| refuse_path(&e.path).is_some() || e.size > MAX_FILE) {
        return Err(format!("server vehicle contains a refused file: {}", e.path));
    }
    Ok((Manifest { map: String::new(), bus, entries: m.entries }, files.into_iter().map(|(_, p)| p).collect()))
}

/// A bounded ASCII request: paths and paint names are hex-encoded UTF-8 to preserve spaces.
pub(super) fn hex_text(text: &str) -> String {
    if text.is_empty() { return "-".to_string(); }
    text.as_bytes().iter().map(|b| format!("{b:02x}")).collect()
}
pub(super) fn unhex_text(text: &str) -> Option<String> {
    if text == "-" { return Some(String::new()); }
    if text.len() > 1024 || text.len() % 2 != 0 || !text.bytes().all(|c| c.is_ascii_hexdigit()) { return None; }
    let bytes: Option<Vec<u8>> = text.as_bytes().chunks(2).map(|pair| {
        u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()
    }).collect();
    String::from_utf8(bytes?).ok()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cache_deduplicates_case_variants_and_negative_results() {
        let cache = Cache::default();
        let first = cache_key(true, "Vehicles/Bus.bus", "Blue");
        let second = cache_key(true, "vehicles\\bus.BUS", "BLUE");
        assert_eq!(first, second);
        assert!(cached(&cache, first, || Err("not offered".into())).is_err());
        assert_eq!(cached(&cache, second, || panic!("a failed request must be cached")).unwrap_err(), "not offered");
    }

    #[test]
    fn cache_keeps_new_entries_after_reaching_its_capacity() {
        let cache = Cache::default();
        for i in 0..140 {
            let _ = cached(&cache, i.to_string(), || Err("not offered".into()));
        }
        assert_eq!(cache.lock().unwrap().len(), 128);
        assert!(cached(&cache, "139".into(), || panic!("new entries must still be cached")).is_err());
    }

    #[test]
    fn refuses_untrusted_files_outside_content_roots() {
        let base = std::env::temp_dir().join(format!("openomsi-fleet-sources-{}", std::process::id()));
        let root = base.join("game");
        let inside = root.join("Vehicles").join("Bus");
        std::fs::create_dir_all(&inside).unwrap();
        let regular = inside.join("safe.dds");
        std::fs::write(&regular, b"safe texture").unwrap();
        let outside = base.join("private.dds");
        std::fs::write(&outside, b"private contents").unwrap();
        assert!(trusted_source(&root, &regular));
        assert!(!trusted_source(&root, &outside));
        #[cfg(unix)]
        {
            let link = inside.join("linked.dds");
            std::os::unix::fs::symlink(&outside, &link).unwrap();
            assert!(
                !trusted_source(&root, &link),
                "a mod symlink must not expose a private file"
            );
            std::fs::remove_file(link).unwrap();
        }
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn selected_names_round_trip_and_reject_malformed_encoding() {
        for text in ["", "Vehicles/Marcopolo Torino 1999/torino.bus", "Bahia - São João"] {
            assert_eq!(unhex_text(&hex_text(text)).as_deref(), Some(text));
        }
        assert_eq!(unhex_text("f"), None);
        assert_eq!(unhex_text("gg"), None);
        assert_eq!(unhex_text("ff"), None);
    }
}
