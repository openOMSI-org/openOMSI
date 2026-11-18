//! The developer tools' hot reload: the player's bus's files looked at once a second while
//! the tools are open, and each one that changed read again and taken over by the bus as
//! it drives (see `omsi_sim::VehicleInstance::reload_definition` and the others), or on a
//! button. What cannot be taken over is said, and the bus goes on as it was.

use std::path::PathBuf;

use super::DevTools;
use crate::App;

/// What is read again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// paths.cfg and `[passengercabin]`.
    Paths,
    Sound,
    /// The `.bus` file: physics, cameras, mirrors, bounding box.
    Bus,
    /// model.cfg, softly.
    Model,
    Constfiles,
    /// A script or a variable list: not read again (the bus must be placed anew), only said.
    Scripts,
}

impl Kind {
    pub const BUTTONS: [(Kind, &'static str); 5] = [
        (Kind::Paths, "paths.cfg and passenger cabin"),
        (Kind::Sound, "sound.cfg"),
        (Kind::Bus, ".bus (physics, cameras, mirrors, box)"),
        (Kind::Model, "model.cfg (animations, lights)"),
        (Kind::Constfiles, "constfiles"),
    ];

    pub fn from_name(s: &str) -> Option<Kind> {
        Some(match s {
            "paths" | "cabin" => Kind::Paths,
            "sound" => Kind::Sound,
            "bus" => Kind::Bus,
            "model" => Kind::Model,
            "const" | "constfiles" => Kind::Constfiles,
            _ => return None,
        })
    }
}

/// The definitions of the bus's parts, front first.
pub(crate) fn part_defs(v: &omsi_sim::VehicleInstance) -> Vec<&omsi_vehicle::Vehicle> {
    std::iter::once(&v.ty.def).chain(v.trailers.iter().map(|t| &t.ty.def)).collect()
}

fn modified(p: &std::path::Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

/// The files of the bus that are looked at for changes (loose files: one in an archive does
/// not change while the game runs).
pub(crate) fn watched_files(v: &omsi_sim::VehicleInstance) -> Vec<(Kind, PathBuf, Option<std::time::SystemTime>)> {
    let mut out: Vec<(Kind, PathBuf)> = Vec::new();
    for def in part_defs(v) {
        let at = |rel: &Option<String>| rel.as_ref().map(|r| omsi_cfg::resolve_path(def.dir(), r));
        out.push((Kind::Bus, def.path.clone()));
        out.extend(at(&def.paths).map(|p| (Kind::Paths, p)));
        out.extend(at(&def.passenger_cabin).map(|p| (Kind::Paths, p)));
        out.extend(at(&def.sound).map(|p| (Kind::Sound, p)));
        out.extend(at(&def.model).map(|p| (Kind::Model, p)));
    }
    let s = &v.ty.def.scripts;
    out.extend(s.constfiles.iter().map(|p| (Kind::Constfiles, p.clone())));
    out.extend(s.scripts.iter().chain(&s.varlists).chain(&s.stringvarlists).map(|p| (Kind::Scripts, p.clone())));
    out.sort_by(|a, b| a.1.cmp(&b.1));
    out.dedup_by(|a, b| a.1 == b.1);
    out.into_iter().filter(|(_, p)| p.is_file()).map(|(k, p)| { let t = modified(&p); (k, p, t) }).collect()
}

impl App {
    /// Once a second while the tools are open: the files that changed, read again.
    pub(crate) fn devtools_watch_files(&mut self) {
        let Some(d) = self.devtools.as_mut() else { return };
        if d.last_check.elapsed().as_secs_f32() < 1.0 {
            return;
        }
        d.last_check = std::time::Instant::now();
        let mut changed: Vec<Kind> = Vec::new();
        for (kind, path, when) in d.files.iter_mut() {
            let now = modified(path);
            if now != *when {
                *when = now;
                if !changed.contains(kind) {
                    changed.push(*kind);
                }
            }
        }
        if !d.auto_reload || changed.is_empty() {
            return;
        }
        // (the constfiles are compiled with the scripts: changed scripts make both wait)
        if changed.contains(&Kind::Scripts) {
            d.scripts_changed = true;
            d.say("a script or a variable list changed: place the bus anew to run it");
            changed.retain(|k| *k != Kind::Constfiles && *k != Kind::Scripts);
        }
        for k in changed {
            self.devtools_reload(k);
        }
    }

    /// Read `kind` of the player's bus's files again and give it to the bus.
    pub(crate) fn devtools_reload(&mut self, kind: Kind) {
        let msg = match self.player.as_mut() {
            None => "no bus of one's own".to_string(),
            Some(p) => {
                let at = std::time::Instant::now();
                let r = match kind {
                    Kind::Paths => reload_cabin(p, self.session.humans.as_mut(), self.devtools.as_deref_mut()),
                    Kind::Sound => match self.sound.audio.as_ref() {
                        Some(a) => {
                            if let Some(mut s) = p.sounds.take() {
                                s.stop_all(a);
                            }
                            p.load_sounds(a);
                            Ok(format!("{} sounds", p.sounds.as_ref().map(|s| s.len()).unwrap_or(0)))
                        }
                        None => Err("no sound".to_string()),
                    },
                    Kind::Bus => omsi_vehicle::Vehicle::load(&p.vehicle.ty.def.path)
                        .map_err(|e| e.to_string())
                        .and_then(|def| p.vehicle.reload_definition(def))
                        .map(|()| {
                            p.mirrors_dirty = true;
                            format!("{} mirrors, {} driver views", p.vehicle.ty.def.cameras_reflexion.len(), p.vehicle.ty.def.cameras_driver.len())
                        }),
                    Kind::Model => {
                        let def = &p.vehicle.ty.def;
                        match def.model.as_ref().map(|m| omsi_cfg::resolve_path(def.dir(), m)) {
                            Some(path) => omsi_model::Model::load(&path).map_err(|e| e.to_string()).and_then(|m| p.vehicle.reload_model(m)).map(|()| "animations, lights and visibility taken over".to_string()),
                            None => Err("the bus has no model".to_string()),
                        }
                    }
                    Kind::Constfiles if self.devtools.as_ref().is_some_and(|d| d.scripts_changed) => Err("a script changed since: place the bus anew".to_string()),
                    Kind::Constfiles => {
                        let program = p.vehicle.recompile(&self.args.root);
                        p.vehicle.reload_program(program).map(|()| "constants and curves taken over".to_string())
                    }
                    Kind::Scripts => Err("scripts are not read again: place the bus anew".to_string()),
                };
                let name = Kind::BUTTONS.iter().find(|b| b.0 == kind).map(|b| b.1).unwrap_or("Scripts");
                match r {
                    Ok(what) => format!("Reloaded {name} in {:.0} ms: {what}", at.elapsed().as_secs_f64() * 1000.0),
                    Err(why) => format!("Reload failed for {name}: {why}"),
                }
            }
        };
        // (the overlays and the list of files follow what the bus has now)
        if let (Some(d), Some(p)) = (self.devtools.as_mut(), self.player.as_ref()) {
            if kind == Kind::Bus || kind == Kind::Paths {
                d.parts = part_defs(&p.vehicle).iter().map(|def| super::overlay::PartFiles::load(def)).collect();
            }
            d.files = watched_files(&p.vehicle);
        }
        let msg = omsi_ui::tr(&msg).into_owned();
        match self.devtools.as_mut() {
            Some(d) => d.say(msg.clone()),
            None => log::info!("developer tools: {msg}"),
        }
        self.service_msg = Some((msg, 4.0));
    }
}

/// paths.cfg and `[passengercabin]` of the bus (and its coupled parts) read again: the
/// people's cabin of the bus made anew. While people are aboard a cabin with fewer places,
/// doors or path points than before is not taken (they stand on what would be gone).
fn reload_cabin(p: &mut crate::player::Player, humans: Option<&mut crate::humans::Humans>, d: Option<&mut DevTools>) -> Result<String, String> {
    if let Some(d) = d {
        d.parts = part_defs(&p.vehicle).iter().map(|def| super::overlay::PartFiles::load(def)).collect();
    }
    let Some(h) = humans else { return Ok("Cabin drawn anew (no passengers in this game)".into()) };
    let key: Vec<PathBuf> = part_defs(&p.vehicle).iter().map(|d| d.path.clone()).collect();
    let old = h.cabins.remove(&key).flatten();
    let new = h.cabin_for(&p.vehicle);
    let aboard = h.people_in(omsi_sim::people::BusId::Player);
    let fewer = match (&old, &new) {
        (Some(o), Some(n)) => n.seats.len() < o.seats.len() || n.entries.len() < o.entries.len() || n.exits.len() < o.exits.len() || n.graph.points.len() < o.graph.points.len(),
        (Some(_), None) => true,
        _ => false,
    };
    if aboard > 0 && fewer {
        h.cabins.insert(key, old);
        return Err(format!("Cannot reload cabin: {aboard} people aboard and the new cabin has fewer places, doors or points - let them off first"));
    }
    let Some(c) = new else {
        h.player_cabin = None;
        return Ok("no passenger cabin now".into());
    };
    let seats = h.seats.entry(omsi_sim::people::BusId::Player).or_default();
    seats.resize(c.seats.len(), false);
    h.entry_req.resize(c.entries.len().max(1), false);
    h.exit_req.resize(c.exits.len().max(1), false);
    h.entry_busy.resize(c.entries.len(), false);
    h.exit_busy.resize(c.exits.len(), false);
    let what = format!("Cabin reloaded: {} path points, {} places, {} entries, {} exits", c.graph.points.len(), c.seats.len(), c.entries.len(), c.exits.len());
    h.player_cabin = Some(c);
    Ok(what)
}
