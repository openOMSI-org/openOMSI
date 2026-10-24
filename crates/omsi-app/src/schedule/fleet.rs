//! The vehicles of the timetable on the GPU: the vehicle sets of the next departures (which
//! bus runs one is omsi-sim's `ScheduleSim::choose`) read and uploaded ahead.

use super::*;
use omsi_render::{Renderer, Scene};

/// How far ahead (s) the timetable reads and uploads the vehicles of its next departures: a
/// tour's bus missing on its layover stands at its first stop up to half an hour early.
pub(super) const FLEET_AHEAD: f64 = 35.0 * 60.0;

/// [`FLEET_AHEAD`], or `OMSI_FLEET_AHEAD` minutes (for tests).
pub(super) fn fleet_ahead() -> f64 {
    omsi_cfg::flags::OMSI_FLEET_AHEAD
        .parse::<f64>()
        .map(|m| m * 60.0)
        .unwrap_or(FLEET_AHEAD)
}

/// A vehicle set nobody has drawn for this long, and that no departure of the next
/// minutes wants, leaves the GPU.
pub(super) const FLEET_IDLE: std::time::Duration = std::time::Duration::from_secs(90);

impl Schedule {
    /// Upload the vehicles of the buses on the road at `day_time` and of the departures of the
    /// next minutes before the first frame, so that they spawn without a hitch; the rest of
    /// the fleet follows as its departures come near (see [`Schedule::tick`]). Uploading the
    /// whole fleet in every paint scheme up front took 109 sets and 640 MB on Ahlheim.
    ///
    /// Every type of the fleet is started once, which reads the files its scripts and
    /// displays need: done on the first bus of a type that comes along, those were frames of
    /// 60 to 170 ms in the middle of a drive.
    pub fn precache(
        &mut self,
        world: &World,
        renderer: &Renderer,
        scene: &mut Scene,
        traffic: Option<&mut crate::traffic::Traffic>,
        day_time: f64,
    ) {
        let t0 = std::time::Instant::now();
        let Some(t) = traffic else { return };
        let sets = self.upcoming_sets(world, t, day_time);
        // a few sets at a time: read on the workers, uploaded, and the copies let go
        let mut most_held = 0usize;
        for chunk in sets.chunks(3) {
            most_held = most_held.max(world.prefetch_vehicle_sets(renderer, chunk));
            for (ty, scheme) in chunk {
                world.precache_vehicle(renderer, scene, ty, *scheme);
            }
        }
        let t1 = std::time::Instant::now();
        let mut seen = std::collections::HashSet::new();
        for (ty, _, hof) in self.sim.depot_vehicles() {
            if !seen.insert(ty.def.path.clone()) {
                continue;
            }
            crate::traffic::warm_up(world, ty, hof.clone());
            t.prime_pull_out_room(ty, true);
            for (tr, _) in t.trailer_chain(ty) {
                if seen.insert(tr.def.path.clone()) {
                    crate::traffic::warm_up(world, &tr, None);
                }
            }
        }
        let pooled: Vec<Arc<VehicleType>> = self.sim.pooled_types();
        for ty in &pooled {
            if seen.insert(ty.def.path.clone()) {
                let hof = self.sim.pool_hof(ty.def.dir(), &*world.data());
                crate::traffic::warm_up(world, ty, hof);
            }
        }
        // what was read ahead and not used (textures of variants the AI never shows)
        world.forget_prefetched();
        crate::release_free_memory();
        self.fleet_check = day_time;
        log::info!("timetable fleet: {} vehicle/paint sets of the first {:.0} minutes read and uploaded in {:.1} s (at most {:.0} MB read ahead at once), a first start of every type in {:.1} s", sets.len(), fleet_ahead() / 60.0, (t1 - t0).as_secs_f32(), most_held as f64 / 1e6, t1.elapsed().as_secs_f32());
    }

    /// Whether every vehicle set of `c` is on the GPU. Where one is not (the clock was set
    /// far ahead, a type nothing read ahead), the sets are asked for and false is said: the
    /// departure is tried again once they are up, where building them on the frame it
    /// appears on cost 120 to 300 ms (the vehicle's meshes and textures all at once).
    pub(super) fn sets_ready(&mut self, c: &Choice, world: &World, traffic: &mut Traffic) -> bool {
        let missing: Vec<_> = Self::choice_sets(c, traffic)
            .into_iter()
            .filter(|(t, s)| !world.has_vehicle_set(&(t.def.path.clone(), *s)))
            .collect();
        if missing.is_empty() {
            return true;
        }
        for m in missing {
            if !self.fleet_wanted.iter().any(|w| w.0.def.path == m.0.def.path && w.1 == m.1) {
                self.fleet_wanted.push(m);
            }
        }
        false
    }

    /// The vehicle sets a choice is drawn with: the vehicle, its rear sections, a train's
    /// further cars.
    pub(super) fn choice_sets(c: &Choice, traffic: &mut Traffic) -> Vec<(Arc<VehicleType>, Option<usize>)> {
        let mut out = vec![(c.ty.clone(), c.scheme)];
        for (t, _) in traffic.trailer_chain(&c.ty) {
            let s = c.scheme.filter(|i| *i < t.paint_schemes.len());
            out.push((t, s));
        }
        if let Some(cars) = &c.train {
            // Every vehicle of the train as `spawn_new_bus` and `turn_train` draw it: the
            // further cars with the vehicles of their units (bogies, the second half of a
            // twin car) unpainted, and - for a trip that runs the train turned round, or
            // when it is turned for its next trip - its last vehicle leading in the train's
            // paint. Only the lead and its own chain were read ahead, so an S-Bahn's other
            // halves were built on the frame it appeared, and again after every quiet
            // spell had let them go (#2365, #2464).
            let mut all: Vec<Arc<VehicleType>> = vec![c.ty.clone()];
            all.extend(traffic.trailer_chain(&c.ty).into_iter().map(|(t, _)| t));
            for (t, rev) in cars.iter().skip(1) {
                all.extend(traffic.coupled_chain(t, *rev, false).into_iter().map(|(t, _)| t));
                all.push(t.clone());
                all.extend(traffic.coupled_chain(t, *rev, true).into_iter().map(|(t, _)| t));
            }
            for t in all {
                out.push((t.clone(), None));
                if c.scheme.is_some() {
                    out.push((t, c.scheme));
                }
            }
        }
        out
    }

    /// The vehicle sets of the trips on the road at `day_time` and of the departures of the
    /// next [`FLEET_AHEAD`] seconds.
    pub(super) fn upcoming_sets(
        &mut self,
        world: &World,
        traffic: &mut Traffic,
        day_time: f64,
    ) -> Vec<(Arc<VehicleType>, Option<usize>)> {
        let mut out = Vec::new();
        let mut seen: HashSet<crate::scene::VehicleKey> = HashSet::new();
        for i in self.sim.upcoming(day_time, fleet_ahead()) {
            let Some(c) = self.sim.choose(i, &*world.data()) else {
                continue;
            };
            for (ty, scheme) in Self::choice_sets(&c, traffic) {
                if seen.insert((ty.def.path.clone(), scheme)) {
                    out.push((ty, scheme));
                }
            }
        }
        out
    }

    /// Keep the GPU's fleet to the vehicles of the next minutes: read the sets of the
    /// coming departures on the workers and upload them (one a frame) well before they are
    /// due, and let go of the sets nobody uses any more.
    pub(super) fn fleet(
        &mut self,
        world: &World,
        traffic: &mut Traffic,
        renderer: &Renderer,
        scene: &mut Scene,
        day_time: f64,
    ) {
        let ready = self.fleet_ready.lock().pop();
        if let Some(key) = ready {
            if let Some(ty) = self.fleet_reading.remove(&key) {
                let t = std::time::Instant::now();
                world.precache_vehicle(renderer, scene, &ty, key.1);
                if omsi_cfg::flags::OMSI_PROFILE.is_set() {
                    log::info!(
                        "timetable fleet: {} (scheme {:?}) uploaded ahead in {:.1} ms",
                        ty.def
                            .path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy(),
                        key.1,
                        t.elapsed().as_secs_f64() * 1000.0
                    );
                }
            }
        }
        if (day_time - self.fleet_check).abs() < 5.0 && self.fleet_wanted.is_empty() {
            return;
        }
        self.fleet_check = day_time;
        // (the sets a departure found missing go first: they are what the next tries wait for)
        let mut sets = std::mem::take(&mut self.fleet_wanted);
        sets.extend(self.upcoming_sets(world, traffic, day_time));
        let keep: HashSet<crate::scene::VehicleKey> = sets
            .iter()
            .map(|(t, s)| (t.def.path.clone(), *s))
            .chain(self.fleet_reading.keys().cloned())
            .chain(traffic.random_sets().into_iter().map(|(t, s)| (t.def.path.clone(), s)))
            .collect();
        // (OMSI_FLEET_IDLE=<s> shortens the wait, for tests)
        let idle = omsi_cfg::flags::OMSI_FLEET_IDLE
            .parse::<f32>()
            .map(std::time::Duration::from_secs_f32)
            .unwrap_or(FLEET_IDLE);
        if world.trim_vehicle_sets(renderer, scene, &keep, idle) > 0 {
            crate::release_free_memory();
        }
        let prefetch = world.vehicle_prefetch(renderer);
        for (ty, scheme) in sets {
            let key = (ty.def.path.clone(), scheme);
            if self.fleet_reading.contains_key(&key) || world.has_vehicle_set(&key) {
                continue;
            }
            // two at a time: a set's repaints are compressed from pictures of tens of
            // megabytes (the rest follows at the next look, five seconds on)
            if self.fleet_reading.len() >= 2 {
                break;
            }
            self.fleet_reading.insert(key.clone(), ty.clone());
            let (p, ready) = (prefetch.clone(), self.fleet_ready.clone());
            // (off the frame's pool: see `threads`)
            crate::threads::background_pool().spawn(move || {
                p.prefetch(&ty, scheme);
                ready.lock().push(key);
            });
        }
    }
}
