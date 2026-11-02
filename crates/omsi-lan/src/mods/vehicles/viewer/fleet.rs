//! Remote-player requests for a bus from the dedicated server's official fleet.
//! Runs off the simulation thread; no preview or model is loaded before validation.

use super::*;

impl Viewer {
    /// Request the server-owned exact bus and repaint, even when the dedicated
    /// server rejects player uploads. Do not download anything until its
    /// validated content fingerprint agrees with the remote player's announcement.
    /// Files are kept in the existing content-addressed store, never in OMSI 2.
    pub(super) fn fetch_official(&self, fp: &str, bus: &str, paint: &str) -> Result<(), Failure> {
        let (host, session) = crate::mods::upload::host().ok_or(Failure::NoHost)?;
        let _slot = self.take_slot();
        if !self.driven(fp) { return Err(Failure::NotDriven); }
        let (raw, mut out, mut input) = failure::open(host, session)?;
        writeln!(out, "FLEETINFO {} {}", crate::mods::server_fleet::hex_text(bus), crate::mods::server_fleet::hex_text(paint))
            .and_then(|_| out.flush()).map_err(|e| e.to_string())?;
        let len = failure::reply(&mut input)?.ok_or("no official server fleet list")?;
        if len > 256 << 20 { return Err("the list is too large".into()); }
        let mut body = vec![0u8; len as usize];
        input.read_exact(&mut body).map_err(|e| e.to_string())?;
        let manifest: VehicleManifest = serde_json::from_slice(&body)
            .map_err(|e| format!("bad official fleet manifest: {e}"))?;
        let manifest = manifest.validated()?;
        if manifest.bus.to_ascii_lowercase() != omsi_net::vehicle_path(bus).ok_or("invalid bus path")?.to_ascii_lowercase() {
            return Err("the server offered another vehicle".into());
        }
        if manifest.fingerprint() != fp {
            return Err("the server's bus or repaint differs from this player's version".into());
        }
        if let Some(bad) = manifest.entries.iter().find(|e| crate::mods::refuse_path(&e.path).is_some() || e.size > crate::mods::MAX_FILE) {
            return Err(format!("{}: refused", bad.path).into());
        }
        let missing = missing(&manifest);
        let total = missing.iter().try_fold(0u64, |n, &k| n.checked_add(manifest.entries[k].size))
            .ok_or("server bus is too large")?;
        if total > MAX_BUS_DOWNLOAD { return Err("server bus exceeds the download limit".into()); }
        use std::sync::atomic::Ordering;
        if self.fetched.fetch_add(total, Ordering::AcqRel).saturating_add(total) > MAX_SESSION_DOWNLOAD {
            self.fetched.fetch_sub(total, Ordering::AcqRel);
            return Err("too much server vehicle content requested this session".into());
        }
        let mut reserved = Reserved(&self.fetched, total, false);
        self.set(fp, Wanted::Pending(0, total));
        let progress = Progress::new(self, fp, 0, total);
        for &k in &missing {
            let e = &manifest.entries[k];
            if let Some(bytes) = buses().own_copy(e) {
                put(e, &bytes)?;
                progress.add(e.size);
                continue;
            }
            writeln!(out, "GET {k}").and_then(|_| out.flush()).map_err(|err| err.to_string())?;
            let size = failure::reply(&mut input)?.ok_or("no file size")?;
            if size != e.size { return Err(format!("{}: the server sent {size} bytes instead of {}", e.path, e.size).into()); }
            let mut bytes = vec![0u8; size as usize];
            input.read_exact(&mut bytes).map_err(|err| format!("{}: {err}", e.path))?;
            if crate::mods::sha256_of(&bytes) != e.sha256 {
                return Err(format!("{}: file hash differs from the signed session manifest", e.path).into());
            }
            crate::mods::quarantine::screen(&e.path, &bytes)?;
            put(e, &bytes)?;
            progress.add(e.size);
        }
        let _ = writeln!(out, "BYE").and_then(|_| out.flush());
        let _ = raw.shutdown(std::net::Shutdown::Both);
        commit(manifest)?;
        reserved.2 = true;
        log::info!("vehicles: the official server's remote bus {bus} ({paint}) is verified ({:.1} MB)", total as f64 / 1e6);
        Ok(())
    }

}
