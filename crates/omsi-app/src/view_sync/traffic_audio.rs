//! The AI vehicles' sounds: the view sync's side of the traffic that is heard.

use crate::traffic::Traffic;
use glam::DVec3;
use hashbrown::HashMap;
use std::sync::Arc;

/// A vehicle definition's `[sound_ai]` (else `[sound]`) configuration, the folder its clips
/// are named from, and those clips' files.
pub(crate) struct SoundPlan {
    cfg: Arc<omsi_vehicle::SoundCfg>,
    dir: std::path::PathBuf,
    clips: Vec<std::path::PathBuf>,
}

fn own_horn(cfg: &omsi_vehicle::SoundCfg) -> Option<&str> {
    cfg.sounds.iter().map(|s| s.file.trim()).find(|f| {
        let f = f.to_ascii_lowercase();
        f.contains("hupe") || f.contains("horn")
    })
}

/// The `SoundPlan` of `def`, made the first time it is asked for (None, remembered too: the
/// definition names no sound, or its configuration does not load).
fn sound_plan(
    plans: &mut HashMap<std::path::PathBuf, Option<Arc<SoundPlan>>>,
    cfgs: &mut HashMap<std::path::PathBuf, Option<Arc<omsi_vehicle::SoundCfg>>>,
    def: &omsi_vehicle::Vehicle,
) -> Option<Arc<SoundPlan>> {
    if let Some(plan) = plans.get(def.path.as_path()) {
        return plan.clone();
    }
    let plan = def.sound_ai.as_ref().or(def.sound.as_ref()).and_then(|rel| {
        let path = omsi_cfg::resolve_path(def.dir(), rel);
        let cfg = cfgs
            .entry(path.clone())
            .or_insert_with(|| {
                omsi_vehicle::SoundCfg::load(&path)
                    .map_err(|e| log::warn!("{e}"))
                    .ok()
                    .map(Arc::new)
            })
            .clone()?;
        let dir = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        let clips = omsi_audio::SoundSet::clip_paths(&cfg, &dir);
        Some(Arc::new(SoundPlan { cfg, dir, clips }))
    });
    plans.insert(def.path.clone(), plan.clone());
    plan
}

impl Traffic {
    pub(crate) fn has_sound_set(&self, id: u64) -> bool {
        self.sounds.contains_key(&id) || self.riding_sounds.as_ref().is_some_and(|r| r.0 == id)
    }

    /// Play the `[sound_ai]` sets of the cars near `listener` (others are silenced).
    /// `street_cond` is the state of the road (see `VehicleHost::street_cond`): the stock
    /// AI sound configuration fades `WetLane_1`/`WetLane_2` in with it, which is what a car
    /// driving past through the wet sounds like. `muffled`: the listener (the player) sits in
    /// a cabin right now, so every AI car's sound is heard through that bodywork and glass -
    /// a passing car's horn does not simply sound like the street outside once the windows
    /// are shut. `riding`: the AI bus the player rides in on foot - heard from inside it.
    /// `volume`: the traffic's volume (OMSI's `sound_ai`).
    pub fn update_audio(
        &mut self,
        audio: &omsi_audio::AudioEngine,
        listener: DVec3,
        street_cond: f32,
        muffled: bool,
        riding: Option<u64>,
        volume: f32,
    ) {
        self.update_riding_audio(audio, riding, volume);
        let freed = audio.trim_clips(std::time::Duration::from_secs(60));
        if freed > 0 && omsi_cfg::flags::OMSI_PROFILE.is_set() {
            log::info!(
                "sound clips: {:.1} MB nobody used for a minute let go",
                freed as f64 / 1e6
            );
        }
        for mut s in self.orphan_sounds.drain(..) {
            s.stop_all(audio);
        }
        let near = 250.0;
        // what the sounds read of the cars, taken from the traffic (`SoundFrame`)
        let mut frame = std::mem::take(&mut self.sound_frame);
        self.sim.snapshot_sounds(&mut frame, listener, near * 1.2, street_cond, riding);
        let mut triggers = Vec::new();
        for c in frame.cars() {
            // (the bus ridden in is heard from inside, by `update_riding_audio`)
            if !c.heard {
                if let Some(mut s) = self.sounds.remove(&c.id) {
                    s.stop_all(audio);
                }
                continue;
            }
            let d = (c.position - listener).length();
            if !self.sounds.contains_key(&c.id) && d < near {
                let plan = sound_plan(&mut self.sound_plans, &mut self.sound_cfgs, &c.ty.def);
                if let Some(plan) = plan {
                    // an articulated bus's rear section sounds too (its engine, on a pusher)
                    let mut parts = Vec::new();
                    for (i, t) in c.part_types.iter().enumerate() {
                        if let Some(part) = sound_plan(&mut self.sound_plans, &mut self.sound_cfgs, &t.def) {
                            parts.push((i, part));
                        }
                    }
                    // the clips are read in the background the first time; silent till then
                    let ready = audio.clips_ready(&plan.clips)
                        && parts.iter().all(|(_, part)| audio.clips_ready(&part.clips));
                    if ready {
                        let number = &c.number;
                        let mut ss = omsi_audio::SoundSet::new_exterior(audio, &plan.cfg.chosen_for(number), &plan.dir);
                        for (i, part) in &parts {
                            ss.add_part(*i, omsi_audio::SoundSet::new_exterior(audio, &part.cfg.chosen_for(number), &part.dir));
                        }
                        ss.master = volume;
                        triggers.push((c.id, ss.curve_triggers().into_iter().collect()));
                        self.sounds.insert(c.id, ss);
                    }
                }
            }
            if let Some(ss) = self.sounds.get_mut(&c.id) {
                ss.set_muffled(muffled);
                let xf = c.world;
                let vars = frame.vars(c);
                let var = |n: &str| c.var_slot(n).and_then(|i| vars.get(i).copied());
                let at_fire = |t: &str, n: &str| -> Option<f32> {
                    let vals = &c.fired_vars.iter().rev().find(|(k, _)| k.eq_ignore_ascii_case(t))?.1;
                    c.var_slot(n).and_then(|i| vals.get(i).copied())
                };
                ss.update_fired(audio, &var, &xf, &c.fired, &at_fire);
                let parts = frame.parts(c);
                ss.update_parts(audio, &var, &|i| parts.get(i).copied(), &c.fired, &at_fire);
                for (t, f) in &c.fired_files {
                    ss.play_file_trigger(audio, t, f, &var, &xf);
                }
            }
        }
        // (a new sound set tells its car which triggers to keep the variables of)
        for (id, t) in triggers {
            self.sim.set_sound_triggers(id, t);
        }
        self.play_horns(audio, &frame, listener, muffled, volume);
        self.sound_frame = frame;
    }

    /// The horns of the cars that honked (`TrafficSim::honked`, in the `SoundFrame`): a car
    /// whose own sound set answers `ev_AI_Horn` was heard by it already; every other one
    /// sounds OMSI's own AI horn (`Sounds/AI_Cars`, the lorry's for the heavy ones) where it
    /// stands - the stock cars have nothing on that trigger, and the AI's honking was never heard.
    fn play_horns(&mut self, audio: &omsi_audio::AudioEngine, frame: &omsi_sim::view_events::SoundFrame, listener: DVec3, muffled: bool, volume: f32) {
        for (heavy, position, ty) in &frame.honks {
            if (*position - listener).length() > 250.0 {
                continue;
            }
            let plan = sound_plan(&mut self.sound_plans, &mut self.sound_cfgs, &ty.def);
            if plan.as_ref().is_some_and(|p| p.cfg.sounds.iter().any(|s| s.triggers.iter().any(|t| t.trim().eq_ignore_ascii_case("ev_AI_Horn")))) {
                continue;
            }
            let file = if *heavy { "Sounds/AI_Cars/Hupe_Lkw.wav" } else { "Sounds/AI_Cars/hupe.wav" };
            let path = plan
                .as_ref()
                .and_then(|p| own_horn(&p.cfg).map(|f| omsi_cfg::resolve_path(&p.dir, f)))
                .filter(|p| omsi_cfg::vfs::is_file(p))
                .unwrap_or_else(|| omsi_cfg::resolve_path(&self.sim.root, file));
            let Some(clip) = audio.load_clip(&path) else { continue };
            let at = *position + DVec3::Z * 0.8;
            audio.play(
                clip,
                omsi_audio::mixer::VoiceParams {
                    gain: volume,
                    pitch: 1.0,
                    looping: false,
                    position: Some(at.as_vec3()),
                    doppler: true,
                    range: 12.0,
                    lowpass_hz: if muffled { 1500.0 } else { 0.0 },
                    important: false,
                },
            );
        }
    }

    /// The AI bus ridden in on foot: its full `[sound]` set from inside ([viewpoint] 2, the
    /// outside sounds through what it has open - its own `Snd_OutsideVol`), instead of its
    /// `[sound_ai]` heard as from the street (#1286: a passenger heard next to nothing).
    fn update_riding_audio(&mut self, audio: &omsi_audio::AudioEngine, riding: Option<u64>, volume: f32) {
        if self.riding_sounds.as_ref().map(|r| r.0) != riding {
            if let Some((_, mut s)) = self.riding_sounds.take() {
                s.stop_all(audio);
            }
        }
        let Some(id) = riding else { return };
        let Some(c) = self.sim.cars.iter_mut().find(|c| c.id == id) else {
            if let Some((_, mut s)) = self.riding_sounds.take() {
                s.stop_all(audio);
            }
            return;
        };
        if let Some(mut s) = self.sounds.remove(&id) {
            s.stop_all(audio);
        }
        if self.riding_sounds.is_none() {
            let cfgs = &mut self.sound_cfgs;
            let mut load = |def: &omsi_vehicle::Vehicle| {
                let rel = def.sound.clone().or_else(|| def.sound_ai.clone())?;
                let path = omsi_cfg::resolve_path(def.dir(), &rel);
                let cfg = cfgs.entry(path.clone()).or_insert_with(|| omsi_vehicle::SoundCfg::load(&path).map_err(|e| log::warn!("{e}")).ok().map(Arc::new)).clone()?;
                Some((cfg, path.parent().map(|p| p.to_path_buf()).unwrap_or_default()))
            };
            let Some((cfg, dir)) = load(&c.vehicle.ty.def) else { return };
            let parts: Vec<_> = c.vehicle.trailers.iter().enumerate().filter_map(|(i, t)| load(&t.ty.def).map(|p| (i, p))).collect();
            if !audio.clips_ready(&omsi_audio::SoundSet::clip_paths(&cfg, &dir)) || !parts.iter().all(|(_, (c, d))| audio.clips_ready(&omsi_audio::SoundSet::clip_paths(c, d))) {
                return;
            }
            let number = c.vehicle.number();
            let mut ss = omsi_audio::SoundSet::new(audio, &cfg.chosen_for(&number), &dir);
            for (i, (pc, pd)) in &parts {
                ss.add_part(*i, omsi_audio::SoundSet::new(audio, &pc.chosen_for(&number), pd));
            }
            ss.master = volume;
            self.riding_sounds = Some((id, ss));
        }
        if let Some((_, ss)) = self.riding_sounds.as_mut() {
            let fired = std::mem::take(&mut c.vehicle.host.fired_triggers);
            let fired_vars = std::mem::take(&mut c.vehicle.host.fired_trigger_vars);
            let fired_files = std::mem::take(&mut c.vehicle.host.fired_file_triggers);
            let v = &c.vehicle;
            ss.set_inside(true);
            ss.set_muffled(true);
            ss.set_listener_vehicle(true);
            // (how open this bus is to the street, for its own outside sounds and the traffic's)
            omsi_audio::soundset::set_outside_open(Some(v.var("Snd_OutsideVol").unwrap_or(0.0)));
            let xf = v.world_transform();
            let at_fire = |t: &str, n: &str| -> Option<f32> {
                let vals = &fired_vars.iter().rev().find(|(k, _)| k.eq_ignore_ascii_case(t))?.1;
                v.var_slot(n).and_then(|i| vals.get(i).copied())
            };
            ss.update_fired(audio, &|n| v.var(n), &xf, &fired, &at_fire);
            ss.update_parts(audio, &|n| v.var(n), &|i| v.trailers.get(i).map(|t| t.world_transform()), &fired, &at_fire);
            for (t, f) in &fired_files {
                ss.play_file_trigger(audio, t, f, &|n| v.var(n), &xf);
            }
        }
    }
}

#[cfg(test)]
mod horn_tests {
    #[test]
    fn an_ai_bus_honks_with_the_horn_of_its_own_sound_set() {
        let dir = std::env::temp_dir().join(format!("openomsi_horn_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let cfg = dir.join("sound_AI.cfg");
        std::fs::write(&cfg, "[sound]\nengine.wav\n1\n\n[sound]\nSD_hupe_lang.wav\n1\n\n[volcurve]\ncockpit_hupe_volume\n").unwrap();
        let loaded = omsi_vehicle::SoundCfg::load(&cfg).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(super::own_horn(&loaded), Some("SD_hupe_lang.wav"));
    }
}
