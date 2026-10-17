//! The type a remote player is drawn with: their own bus by content, a stand-in bus, or
//! a neutral box of its size (`remote_type`, split out of `game.rs`).

use super::*;

/// The type a remote player is drawn with (`remote_type`).
pub(super) struct RemoteType {
    pub(super) ty: Arc<omsi_sim::VehicleType>,
    /// A box stands in for their bus.
    pub(super) stand_in: bool,
    /// Another bus stands in for theirs (`RemoteVehicle::fallback`).
    pub(super) fallback: bool,
    /// ...because their bus (this file) is being read off the frame (`remote_load`).
    pub(super) reading: Option<std::path::PathBuf>,
    /// Their own bus is here but could not be read: this file.
    pub(super) read_failed: Option<std::path::PathBuf>,
    /// What reading their bus read (`mods::vehicles::audit`).
    pub(super) reads: Vec<std::path::PathBuf>,
}

/// The type a remote player drives - as that player has it. While it is on its way here as
/// that bus (or still being read off the frame: `remote_load`) a neutral box of its size
/// stands in for it (`mods::placeholder`); once it is not to be had as itself (given up, not
/// come for long, not readable, a game that says no fingerprint, a bus the server's
/// `vehicles` list does not allow) another bus does, as before there were boxes
/// (`fallback_type`), and the box only when there is none.
pub(super) fn remote_type(args: &LanConfig, pose: &Pose, player: Option<&omsi_sim::VehicleInstance>) -> Option<RemoteType> {
    let mut reading = None;
    let mut read_failed = None;
    let by_content = omsi_net::is_fingerprint(&pose.content);
    // (asked before it is asked for again below: `want` starts asking anew, and a bus being
    // asked for is never hopeless)
    let hopeless_by_now = by_content && crate::mods::vehicles::hopeless(&pose.content);
    let allowed = crate::server_cfg::SERVER_VEHICLES.get().filter(|l| !l.is_empty());
    let loaded = if allowed.is_none_or(|l| crate::server_cfg::allows(l, &pose.bus)) {
        // the bus as its player has it, file for file (`mods::vehicles`): this machine's
        // own copy when it is the same, else the one fetched by its content - never another
        // version of it that happens to have the same path here. (A player whose game says
        // no fingerprint, an older one: the bus of that path here, as before.)
        let path = if by_content {
            use crate::mods::vehicles::Wanted;
            match crate::mods::vehicles::want(&args.root, pose.id, &pose.content, &pose.bus, &pose.paint) {
                Wanted::Local(p) | Wanted::Mounted(p) => Ok(p),
                Wanted::Pending(..) => Err("being made sure of by its content".to_string()),
                Wanted::Failed(e) => Err(e.to_string()),
            }
        } else {
            remote_bus_file(args, &pose.bus)
        };
        // (read on a thread of its own, its paint put on the GPU there too: a heavy coach
        // read here froze the frame for a good part of a second - `remote_load`)
        path.and_then(|path| match crate::remote_load::get(&args.root, &path, &pose.paint) {
            crate::remote_load::Got::Ready(ty, reads) => Ok((ty, reads)),
            crate::remote_load::Got::Reading => {
                reading = Some(path);
                Err("being read".to_string())
            }
            crate::remote_load::Got::Failed(why) => {
                // (their own bus, here: read again after a while - see `lan::tick`)
                if by_content {
                    read_failed = Some(path);
                }
                Err(why)
            }
        })
    } else {
        Err("the server does not offer it".to_string())
    };
    let e = match loaded {
        Ok((ty, reads)) => return Some(RemoteType { ty, stand_in: false, fallback: false, reading: None, read_failed: None, reads }),
        Err(e) => e,
    };
    // not to be had as itself (for now): another bus, as before there were boxes
    let hopeless = !by_content || read_failed.is_some() || hopeless_by_now;
    if hopeless && reading.is_none() {
        match fallback_type(args, pose, player) {
            Fallback::Ready(ty, which) => {
                log::info!("LAN: player {} drives {:?}, not to be had here as that bus ({e}); {which} stands in for it", pose.id, pose.bus);
                return Some(RemoteType { ty, stand_in: false, fallback: true, reading: None, read_failed, reads: Vec::new() });
            }
            Fallback::Reading(path) => reading = Some(path),
            Fallback::None => {}
        }
    }
    // on its way (or nothing to stand in): a plain box of its size, until it is here as
    // itself (`mods::placeholder`)
    log::info!("LAN: player {} drives {:?}, not here as that bus ({e}); a box of its size stands for it", pose.id, pose.bus);
    crate::mods::placeholder::of_size(&args.root, pose.length, pose.width, pose.height, pose.box_offset).map(|ty| RemoteType { ty, stand_in: true, fallback: false, reading, read_failed, reads: Vec::new() })
}

/// What stands in for a player's bus that is not to be had as itself (`remote_type`).
enum Fallback {
    /// This bus (and which, for the log).
    Ready(Arc<omsi_sim::VehicleType>, &'static str),
    /// One is being read off the frame (`remote_load`): a box meanwhile.
    Reading(std::path::PathBuf),
    None,
}

/// The bus that stands in for a player's that is not to be had as itself - as before there
/// were boxes: this machine's bus of that path (another version of it, when theirs is known by
/// its content), else the one our player drives, else on a server the first of the buses its
/// `vehicles` list allows.
fn fallback_type(args: &LanConfig, pose: &Pose, player: Option<&omsi_sim::VehicleInstance>) -> Fallback {
    let read = |path: std::path::PathBuf| match crate::remote_load::get(&args.root, &path, &pose.paint) {
        crate::remote_load::Got::Ready(ty, _) => Some(Ok(ty)),
        crate::remote_load::Got::Reading => Some(Err(path)),
        crate::remote_load::Got::Failed(_) => None,
    };
    let allowed = crate::server_cfg::SERVER_VEHICLES.get().filter(|l| !l.is_empty());
    // (theirs by its path was tried already when theirs says no fingerprint)
    if omsi_net::is_fingerprint(&pose.content) && allowed.is_none_or(|l| crate::server_cfg::allows(l, &pose.bus)) {
        match remote_bus_file(args, &pose.bus).ok().and_then(read) {
            Some(Ok(ty)) => return Fallback::Ready(ty, "this machine's bus of that path"),
            Some(Err(path)) => return Fallback::Reading(path),
            None => {}
        }
    }
    if let Some(p) = player {
        return Fallback::Ready(p.ty.clone(), "the bus our player drives");
    }
    match allowed.and_then(|l| l.first()).and_then(|first| remote_bus_file(args, first).ok()).and_then(read) {
        Some(Ok(ty)) => Fallback::Ready(ty, "the server's first bus"),
        Some(Err(path)) => Fallback::Reading(path),
        None => Fallback::None,
    }
}
