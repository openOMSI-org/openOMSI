//! Finding and reading the overhead wire of the map being driven: `trolley.txt` beside its
//! `global.cfg`, or in the same folder of any other content root (a mod can wire an existing
//! map), or the file `OMSI_TROLLEY_FILE` names. A map with none has no trolleybus wire.

use omsi_trolley::WireNet;
use std::path::PathBuf;
use std::sync::Arc;

fn candidates(args: &crate::Args) -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(f) = omsi_cfg::flags::OMSI_TROLLEY_FILE.var().filter(|s| !s.is_empty()) {
        v.push(PathBuf::from(f));
    }
    let global = omsi_cfg::resolve_path(&args.root, &args.map);
    if let Some(dir) = global.parent() {
        v.push(omsi_cfg::resolve_path(dir, "trolley.txt"));
        if let Some(name) = dir.file_name() {
            for root in omsi_cfg::content_roots() {
                v.push(omsi_cfg::resolve_path(&root, &format!("maps/{}/trolley.txt", name.to_string_lossy())));
            }
        }
    }
    v
}

/// The wire of each map read so far in this run (including "none": a map without wire is not
/// searched again at every trolleybus placed): reading the map's tiles for its wire splines is
/// seconds on a big map. The buses of a map share one network, as they share one wire.
static READ: std::sync::Mutex<Vec<(PathBuf, Option<Arc<WireNet>>)>> = std::sync::Mutex::new(Vec::new());

/// `splines`: with no `trolley.txt`, read the wire off the map's wire splines (for a trolleybus:
/// seconds on a big map, and the map draws that wire itself).
pub(crate) fn load(args: &crate::Args, world: &omsi_scene::World, splines: bool) -> Option<Arc<WireNet>> {
    let key = omsi_cfg::resolve_path(&args.root, &args.map);
    if let Some((_, net)) = READ.lock().unwrap_or_else(|e| e.into_inner()).iter().find(|(k, _)| *k == key) {
        return net.clone();
    }
    let net = read(args, world, splines);
    // (a "none" without the splines read is not the final word for a trolleybus later)
    if splines || net.is_some() {
        READ.lock().unwrap_or_else(|e| e.into_inner()).push((key, net.clone()));
    }
    net
}

fn read(args: &crate::Args, world: &omsi_scene::World, splines: bool) -> Option<Arc<WireNet>> {
    let (mut net, path) = match candidates(args).into_iter().find(|p| p.is_file()) {
        Some(path) => {
            let bytes = std::fs::read(&path).map_err(|e| log::warn!("{}: {e}", path.display())).ok()?;
            let mut net = WireNet::parse(&omsi_cfg::decode_text(&bytes));
            net.dir = path.parent().map(|d| d.to_path_buf());
            (net, path)
        }
        None if !splines => return None,
        // none written: the map's own wire splines
        None => {
            let tiles: Vec<(i32, i32, PathBuf)> = world.map_tiles().into_iter().map(|(_, x, y, p)| (x, y, p)).collect();
            (omsi_scene::trolley_derive::derive(&args.root, &tiles, &world.chrono_dirs())?, PathBuf::from("(the map's wire splines)"))
        }
    };
    if net.is_empty() {
        log::warn!("{}: no overhead wire in it", path.display());
        return None;
    }
    if !args.online() {
        net.draw_random_switches();
    }
    if let Some(dead) = omsi_cfg::flags::OMSI_TROLLEY_DEAD.var() {
        for id in dead.split(',').filter_map(|s| s.trim().parse::<i32>().ok()) {
            if let Some(i) = net.section_index(id) {
                net.set_section_live(i, false);
            }
        }
    }
    log::info!("trolley wire {}: {} spans, {} switches, {} sections, {:.0} V", path.display(), net.spans.len(), net.switches.len(), net.sections.len(), net.nominal_volts);
    Some(Arc::new(net))
}
