//! A matching installed model needs no mod transport. Never substitute by name alone.
use super::*;

pub(super) fn verified(args: &LanConfig, lan: &mut LanSession) -> bool {
    let Some(bus) = args.bus.clone().filter(|_| args.allow_server_fleet) else { return false };
    let Some(target) = args.lan_join.clone() else { return false };
    let root = args.root.clone();
    let paint = args.paint.clone().unwrap_or_default();
    let planned = Pose { bus: bus.clone(), ..Default::default() };
    let worker = std::thread::spawn(move || {
        let info = omsi_net::ws::query(&target, false).ok()?;
        Some(matches_announced(&info.fleet, &bus, &paint, || {
            crate::mods::fleet_fingerprint(&root, &bus).ok()
        }))
    });
    while !worker.is_finished() {
        lan.keepalive(0.05, &planned);
        std::thread::sleep(Duration::from_millis(50));
    }
    worker.join().ok().flatten() == Some(true)
}
/// How long a game whose bus must come from the server's fleet waits to be let in. Over a
/// tunnel the first wait is 6 s, but a busy server admits a few players at a time (a join
/// on a real one took two minutes): without the welcome the bus is never asked for, and the
/// game went on without it, stuck loading until the server dropped it.
const FLEET_WELCOME_WAIT: Duration = Duration::from_secs(120);

/// The bus this game must take from the server's fleet: asked for, and not here.
pub(super) fn needs_fleet_bus(args: &LanConfig) -> bool {
    args.allow_server_fleet && args.bus.as_deref().is_some_and(|b| !installed(&args.root, b))
}

/// Wait (up to `FLEET_WELCOME_WAIT`) for the host's welcome when the game's bus may come from
/// the server's fleet: one installed here can still be another version than the server's.
pub(super) fn wait_for_welcome(args: &LanConfig, lan: &mut LanSession) {
    if !args.allow_server_fleet || args.bus.is_none() || lan.welcome.is_some() || lan.rejected.is_some() {
        return;
    }
    log::info!("LAN: waiting to be let in: this game's bus comes from the server");
    lan.warnings.push("Waiting for the server to let this game in (it admits a few players at a time)...".into());
    write_status(lan, &Default::default(), None);
    let planned = Pose { bus: args.bus.clone().unwrap_or_default(), ..Default::default() };
    let t0 = Instant::now();
    while t0.elapsed() < FLEET_WELCOME_WAIT && lan.welcome.is_none() && lan.rejected.is_none() {
        lan.keepalive(0.05, &planned);
        std::thread::sleep(Duration::from_millis(50));
    }
    lan.warnings.retain(|w| !w.starts_with("Waiting for the server"));
    match (&lan.welcome, &lan.rejected) {
        (Some(_), _) => log::info!("LAN: let in after {:.0} s more", t0.elapsed().as_secs_f32()),
        (None, Some(why)) => log::warn!("LAN: turned away: {why}"),
        (None, None) => log::warn!("LAN: not let in within {} s more", FLEET_WELCOME_WAIT.as_secs()),
    }
}

pub(super) fn installed(root: &std::path::Path, bus: &str) -> bool {
    omsi_net::vehicle_path(bus).is_some_and(|bus| omsi_cfg::find_in_roots(&bus).map(|(_, p)| p).unwrap_or_else(|| root.join(&bus)).is_file())
}

/// A selected server fleet vehicle requires a download unless its installed bytes
/// were verified against the server's complete model fingerprint.
pub(super) fn requires_server_download(allowed: bool, selected: bool, verified: bool) -> bool {
    allowed && selected && !verified
}

/// Check the announced model *before* hashing the local installation. Matching a
/// filename alone never grants a bypass when the mod channel is unavailable.
fn matches_announced(
    fleet: &[omsi_net::ws::FleetPaints],
    bus: &str,
    paint: &str,
    local_fingerprint: impl FnOnce() -> Option<String>,
) -> bool {
    let normalized = bus.replace('\\', "/");
    let Some(announced) = fleet.iter().find(|f| f.bus.eq_ignore_ascii_case(&normalized)) else {
        return false;
    };
    if !paint.is_empty() && !announced.paints.iter().any(|p| p.eq_ignore_ascii_case(paint)) {
        return false;
    }
    if !omsi_net::is_fingerprint(&announced.content) {
        return false;
    }
    local_fingerprint().is_some_and(|local| local == announced.content)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fleet() -> Vec<omsi_net::ws::FleetPaints> {
        vec![omsi_net::ws::FleetPaints {
            bus: "Vehicles/Test Bus/test.bus".into(),
            paints: vec!["Bahia Blue".into()],
            content: format!("v2-{}", "a".repeat(64)),
        }]
    }

    #[test]
    fn only_a_fleet_bus_that_is_not_here_waits_for_the_welcome() {
        let root = std::env::temp_dir().join(format!("omsi-fleet-wait-{}", std::process::id()));
        std::fs::create_dir_all(root.join("Vehicles/Here")).unwrap();
        std::fs::write(root.join("Vehicles/Here/here.bus"), "[name]\nx\n").unwrap();
        let args = |bus: &str, allow: bool| LanConfig { root: root.clone(), bus: Some(bus.into()), allow_server_fleet: allow, ..Default::default() };
        assert!(needs_fleet_bus(&args("Vehicles/Away/away.bus", true)));
        assert!(!needs_fleet_bus(&args("Vehicles/Here/here.bus", true)), "an installed bus is checked as before");
        assert!(!needs_fleet_bus(&args("Vehicles/Away/away.bus", false)), "without the fleet opt-in nothing changes");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn matching_local_model_can_join_without_a_reachable_mod_port() {
        let offered = fleet();
        let hash = offered[0].content.clone();
        // Simulate a server with UDP/status reachable but TCP mods/relay blocked.
        let mod_port_reachable = false;
        let verified = matches_announced(
            &offered, "vehicles\\Test Bus\\test.bus", "bahia blue",
            || Some(hash),
        );
        assert!(verified);
        assert!(
            !requires_server_download(true, true, verified) && !mod_port_reachable,
            "matching local files must not trigger a fatal download requirement"
        );
    }

    #[test]
    fn different_bytes_or_missing_repaint_must_not_bypass_download() {
        let offered = fleet();
        let other = format!("v2-{}", "b".repeat(64));
        assert!(!matches_announced(&offered, "Vehicles/Test Bus/test.bus", "Bahia Blue", || Some(other)));
        assert!(requires_server_download(true, true, false), "different bytes require an authenticated transfer");

        assert!(!matches_announced(&offered, "Vehicles/Test Bus/test.bus", "Missing paint", || {
            panic!("unadvertised repaint must be rejected before hashing files")
        }));
        assert!(!matches_announced(&offered, "Vehicles/Other/test.bus", "", || {
            panic!("wrong bus must not be accepted by fingerprint alone")
        }));
    }

    #[test]
    fn only_a_bus_missing_here_is_server_only() {
        let root = std::env::temp_dir().join(format!("omsi-local-fleet-{}", std::process::id()));
        std::fs::create_dir_all(root.join("Vehicles/Here")).unwrap();
        std::fs::write(root.join("Vehicles/Here/here.bus"), b"").unwrap();
        assert!(installed(&root, "Vehicles\\Here\\here.bus"));
        assert!(!installed(&root, "Vehicles/Gone/gone.bus"));
        assert!(!installed(&root, "../outside.bus"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn invalid_catalogue_hash_and_disabled_fleet_do_not_force_a_download() {
        let mut offered = fleet();
        offered[0].content = "not-a-fingerprint".into();
        assert!(!matches_announced(&offered, "Vehicles/Test Bus/test.bus", "", || {
            panic!("untrusted catalogue hashes must be discarded before reading local files")
        }));
        assert!(!requires_server_download(false, true, false));
        assert!(!requires_server_download(true, false, false));
        assert!(!requires_server_download(true, true, true));
    }
}
