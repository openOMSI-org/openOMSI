//! The other LAN players' vehicles as they are drawn and heard here: their renders (with
//! their rear sections), their driver figures and their sounds. LAN play (`lan`) runs the
//! vehicles and says what changed (`LanGame::drawn`); this follows it after each tick, in
//! the order it happened, so that the renderer and the mixer get the calls they got when
//! the session made them itself.

use crate::lan::{LanGame, RemoteEvent, RemoteVehicle};
use crate::scene::{self, World};
use glam::DVec3;
use omsi_render::{Renderer, Scene};
use std::sync::Arc;

/// Other players' buses are heard within this distance (m), and fall silent beyond 1.2 times it.
mod freetex;

const HEAR_RANGE: f64 = 250.0;

/// The view of one other player's vehicle.
struct RemoteView {
    pixels: freetex::RemotePixels,
    render: scene::VehicleRender,
    trailer_renders: Vec<scene::VehicleRender>,
    sounds: Vec<omsi_audio::SoundSet>,
    /// Its `[sound]` heard from inside, while we ride in it (`SyncTable::interior`).
    inside_sounds: Option<omsi_audio::SoundSet>,
    /// The driver at the wheel (their bus stood empty here), hidden while they walk about.
    driver: Option<crate::driver::DriverFigure>,
    /// The figure (`Pose::figure`) the driver was made for: made again when another comes.
    /// (Made once, it kept the figure guessed before their INFO named theirs - one of the
    /// map's drivers -, and on foot they were a passenger picked by their id.)
    driver_for: Option<String>,
}

/// The other players' vehicles as drawn and heard, by player id.
#[derive(Default)]
pub(crate) struct LanView {
    pixels: freetex::Pixels,
    remotes: hashbrown::HashMap<u32, RemoteView>,
    /// The world's reader of vehicle sets, lent to `omsi_lan::remote_load` while this lives.
    prefetching: Option<omsi_lan::remote_load::Lent>,
}

impl LanView {
    /// The figure player `id` is drawn as: the one at their wheel, which their avatar on
    /// foot wears too (`sync_remote_walkers`).
    pub(crate) fn figure_type(&self, id: u32) -> Option<Arc<omsi_sim::human::HumanType>> {
        self.remotes.get(&id).and_then(|v| v.driver.as_ref()).map(|d| d.human_type())
    }
}

/// Follow what the last LAN tick did to the other players' vehicles: make and let go of
/// their renders, play their sounds, place their drivers and draw them where they are.
pub(crate) fn apply_lan(
    game: &mut LanGame,
    view: &mut LanView,
    w: &World,
    r: &Renderer,
    scene: &mut Scene,
    audio: Option<&omsi_audio::AudioEngine>,
    frame: &crate::lan::LanInput,
    dt: f32,
) {
    // (other players' buses are read off the frame, their paint put on the GPU there too)
    if view.prefetching.is_none() {
        let p = w.vehicle_prefetch(r);
        view.prefetching = Some(omsi_lan::remote_load::lend_prefetcher(Arc::new(move |ty: &omsi_sim::VehicleType, scheme: Option<usize>| p.prefetch(ty, scheme))));
    }
    let changes = std::mem::take(&mut game.drawn);
    for e in changes.events {
        match e {
            RemoteEvent::Gone(id) => {
                if let Some(v) = view.remotes.remove(&id) {
                    release(r, scene, w, audio, v);
                }
            }
            RemoteEvent::Made(id) => {
                if let Some(rv) = game.remotes.get(&id) {
                    view.remotes.insert(id, made(w, r, scene, rv));
                }
            }
        }
    }
    for (id, inside) in changes.heard {
        if view.remotes.get(&id).is_some_and(|v| !v.sounds.is_empty() || v.inside_sounds.is_some()) {
            game.apply_player_sounds(id);
        }
        if let (Some(rv), Some(v)) = (game.remotes.get_mut(&id), view.remotes.get_mut(&id)) {
            sound_remote(rv, v, audio, frame.listener, frame.muffled, inside);
        }
    }
    if let Some(a) = audio {
        for id in changes.debug_sounds {
            if let Some(v) = view.remotes.get(&id) {
                let playing: Vec<String> = v
                    .sounds
                    .iter()
                    .flat_map(|s| s.playing(a))
                    .filter(|p| p.1 > 1.0e-4)
                    .map(|(f, g, pitch)| format!("{} {:.3}x{:.2}", f.rsplit(['\\', '/']).next().unwrap_or(&f), g, pitch))
                    .take(10)
                    .collect();
                log::info!("LAN sounds of player {} ({} set(s)): {:?}", id, v.sounds.len(), playing);
            }
        }
    }
    // draw them like AI traffic
    for (id, rv) in game.remotes.iter_mut() {
        let Some(v) = view.remotes.get_mut(id) else { continue };
        if v.driver_for.as_deref() != Some(rv.last.figure.as_str()) && !rv.stand_in {
            v.driver_for = Some(rv.last.figure.clone());
            if let Some(mut d) = v.driver.take() {
                d.hide(r, scene);
            }
            // their own figure at the wheel (a figure picked by their id otherwise)
            v.driver = crate::driver::DriverFigure::new_named(w, r, scene, rv.vehicle(), omsi_net::human_path(&rv.last.figure).as_deref(), 1000 + *id as u64);
        }
        if let Some(d) = v.driver.as_mut() {
            d.update(r, scene, rv.vehicle(), &v.render, dt.max(1.0 / 120.0), rv.last.walker.is_none(), false);
        }
        // drawn as the own bus is: its outside meshes from outside, its inside ones to
        // whoever stands in it (the outside and the AI meshes together fought over the
        // same surfaces - the flicker and the black patches in the saloon), opaque slots
        // never faded by an alpha variable the AI scripts left at 0, the bellows skinned
        let inside = frame.inside_of == Some(*id);
        crate::player::sync_vehicle_transforms(r, scene, rv.vehicle_mut(), &mut v.render, &mut v.trailer_renders, inside);
    }
}

/// The renders of a vehicle just made: its body, then its rear sections in order, each in
/// its paint scheme where it has it.
fn made(w: &World, r: &Renderer, scene: &mut Scene, rv: &RemoteVehicle) -> RemoteView {
    let vehicle = rv.vehicle();
    let scheme = vehicle.host.paint_scheme.flatten();
    let render = w.add_vehicle_shared(r, scene, &vehicle.ty, scheme, None);
    let trailer_renders = vehicle
        .trailers
        .iter()
        .map(|t| w.add_vehicle_shared(r, scene, &t.ty, scheme.filter(|i| *i < t.ty.paint_schemes.len()), Some(&render)))
        .collect();
    let mut view = RemoteView { pixels: Default::default(), render, trailer_renders, sounds: Vec::new(), inside_sounds: None, driver: None, driver_for: None };
    if !rv.stand_in && !rv.fallback { freetex::prepare(&mut view); }
    view
}

/// A remote vehicle has gone: its sounds stop and its instances go back to the world.
fn release(r: &Renderer, scene: &mut Scene, world: &World, audio: Option<&omsi_audio::AudioEngine>, v: RemoteView) {
    if let Some(a) = audio {
        for mut s in v.sounds {
            s.stop_all(a);
        }
    }
    if let Some(mut d) = v.driver {
        d.hide(r, scene);
    }
    world.release_vehicle(r, scene, v.render);
    freetex::release(r, scene, v.pixels);
    for t in v.trailer_renders {
        world.release_vehicle(r, scene, t);
    }
}

/// The outside sounds of a remote bus: made when it comes into earshot, dropped beyond.
fn sound_remote(
    rv: &mut RemoteVehicle,
    view: &mut RemoteView,
    audio: Option<&omsi_audio::AudioEngine>,
    listener: Option<DVec3>,
    muffled: bool,
    inside: bool,
) {
    let fired: Vec<String> = std::mem::take(&mut rv.vehicle_mut().host.fired_triggers);
    let fired_vars = std::mem::take(&mut rv.vehicle_mut().host.fired_trigger_vars);
    let fired_files: Vec<(String, String)> = std::mem::take(&mut rv.vehicle_mut().host.fired_file_triggers);
    let at_fire = |t: &str, n: &str| -> Option<f32> {
        let vals = &fired_vars.iter().rev().find(|(k, _)| k.eq_ignore_ascii_case(t))?.1;
        rv.vehicle().var_slot(n).and_then(|i| vals.get(i).copied())
    };
    let (Some(audio), Some(at)) = (audio, listener) else {
        return;
    };
    let table = rv.table();
    // Riding in it: its whole `[sound]` heard from inside, as our own bus is heard from its
    // cab - not its outside sounds muffled, which a passenger heard as if standing in the
    // street beside it (the exterior engine, the tyres, the doors from outside).
    let interior = inside.then_some(()).and(table.interior.clone());
    if let Some((cfg, dir)) = interior {
        for mut s in view.sounds.drain(..) {
            s.stop_all(audio);
        }
        if view.inside_sounds.is_none() && audio.clips_ready(&omsi_audio::SoundSet::clip_paths(&cfg, &dir)) {
            let number = rv.vehicle().number();
            view.inside_sounds = Some(omsi_audio::SoundSet::new(audio, &cfg.chosen_for(&number), &dir));
        }
        if let Some(ss) = view.inside_sounds.as_mut() {
            let v = rv.vehicle();
            let xf = v.world_transform();
            ss.set_inside(true);
            ss.set_muffled(true);
            ss.set_listener_vehicle(true);
            // its own outside sounds come in through what this bus has open - its
            // `Snd_OutsideVol`, not the walker's own bus's (or 1 without one): riding along
            // the exterior engine drowned the cab (#1759)
            omsi_audio::soundset::set_outside_open(Some(v.var("Snd_OutsideVol").unwrap_or(0.0)));
            ss.update_fired(audio, &|n| v.var(n), &xf, &fired, &at_fire);
            for (t, f) in &fired_files {
                ss.play_file_trigger(audio, t, f, &|n| v.var(n), &xf);
            }
        }
        return;
    }
    if let Some(mut s) = view.inside_sounds.take() {
        s.stop_all(audio);
    }
    let d = (rv.vehicle().position - at).length();
    if d > HEAR_RANGE * 1.2 {
        for mut s in view.sounds.drain(..) {
            s.stop_all(audio);
        }
        return;
    }
    if view.sounds.is_empty() && d < HEAR_RANGE && !table.sounds.is_empty() {
        // the clips are read in the background the first time; silent till then
        let ready = table
            .sounds
            .iter()
            .map(|(cfg, dir)| (cfg, dir))
            .chain(table.part_sounds.iter().map(|(_, cfg, dir)| (cfg, dir)))
            .all(|(cfg, dir)| audio.clips_ready(&omsi_audio::SoundSet::clip_paths(cfg, dir)));
        if ready {
            let number = rv.vehicle().number();
            view.sounds = table
                .sounds
                .iter()
                .map(|(cfg, dir)| omsi_audio::SoundSet::new_exterior(audio, &cfg.chosen_for(&number), dir))
                .collect();
            // the rear sections' sounds ride on the first set, each at its section
            if let Some(first) = view.sounds.first_mut() {
                for (k, cfg, dir) in &table.part_sounds {
                    first.add_part(*k, omsi_audio::SoundSet::new_exterior(audio, &cfg.chosen_for(&number), dir));
                }
            }
        }
    }
    let v = rv.vehicle();
    let xf = v.world_transform();
    for ss in view.sounds.iter_mut() {
        ss.set_muffled(muffled);
        ss.update_fired(audio, &|n| v.var(n), &xf, &fired, &at_fire);
        ss.update_parts(audio, &|n| v.var(n), &|i| v.trailers.get(i).map(|t| t.world_transform()), &fired, &at_fire);
        for (t, f) in &fired_files {
            ss.play_file_trigger(audio, t, f, &|n| v.var(n), &xf);
        }
    }
}
