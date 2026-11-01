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
        let announced = info.fleet.iter().find(|f| f.bus.eq_ignore_ascii_case(&bus.replace('\\', "/")))?;
        if !paint.is_empty() && !announced.paints.iter().any(|p| p.eq_ignore_ascii_case(&paint)) { return None; }
        if !omsi_net::is_fingerprint(&announced.content) { return None; }
        let local = crate::mods::fleet_fingerprint(&root, &bus).ok()?;
        Some(local == announced.content)
    });
    while !worker.is_finished() {
        lan.keepalive(0.05, &planned);
        std::thread::sleep(Duration::from_millis(50));
    }
    worker.join().ok().flatten() == Some(true)
}
