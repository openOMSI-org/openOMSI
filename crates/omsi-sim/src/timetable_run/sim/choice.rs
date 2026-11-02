//! The vehicles of the timetable: which bus runs a departure, from the depots, the trains
//! and the plain `[aigroup_2]` pools.

use super::*;

impl ScheduleSim {
    /// The vehicle departure `i` is driven with (see [`Choice`]).
    pub fn choose(&mut self, i: usize, world: &dyn TimetableWorld) -> Option<Choice> {
        let group = self.departures[i].ai_group.to_ascii_lowercase();
        let key = self.tour_key(i);
        // the draw: the tour mixed with the session (#2250); `key` names the tour's choice
        let h = if self.vehicle_seed == 0 { key } else { mix(key ^ self.vehicle_seed) };
        // trains: the group lists .zug files instead of depot vehicles
        let train = self
            .trains
            .get(&group)
            .and_then(|t| t.get((h % t.len().max(1) as u64) as usize).cloned());
        let (ty, numbers, hof): (
            Arc<VehicleType>,
            Vec<omsi_map::DepotEntry>,
            Option<Arc<omsi_vehicle::Hof>>,
        ) = match (&train, self.depots.get(&group).filter(|v| !v.is_empty())) {
            (Some(cars), _) => (cars[0].0.clone(), Vec::new(), None),
            (None, Some(vehicles)) => {
                // The depot's types come out in proportion to their fleets: a typgroup
                // listing 40 fleet numbers appears eight times as often as one with
                // 5, as in OMSI - a plain round robin gave the single MB O305 of a
                // depot the same share as the whole SD200 fleet.
                // A tour's vehicle for the day: the one `car_use` gives it, else one drawn now
                // and kept (a fleet number no other tour has, while there are any left).
                let (k, j) = match self.tour_vehicle.get(&key) {
                    Some(&kj) => kj,
                    None => {
                        let weights: Vec<usize> = vehicles.iter().map(|(_, n, _)| n.len().max(1)).collect();
                        let total: usize = weights.iter().sum();
                        let mut x = (h % total.max(1) as u64) as usize;
                        let mut k = 0usize;
                        for (i, w) in weights.iter().enumerate() {
                            if x < *w {
                                k = i;
                                break;
                            }
                            x -= w;
                        }
                        let nums = &vehicles[k].1;
                        let start = ((h >> 21) % nums.len().max(1) as u64) as usize;
                        let free = (0..nums.len())
                            .map(|o| (start + o) % nums.len())
                            .find(|&j| !self.used_numbers.contains(&(group.clone(), nums[j].number.trim().to_string())));
                        let j = free.unwrap_or(start);
                        if let Some(n) = nums.get(j) {
                            self.used_numbers.insert((group.clone(), n.number.trim().to_string()));
                        }
                        self.tour_vehicle.insert(key, (k, j));
                        (k, j)
                    }
                };
                let (ty, numbers, hof) = &vehicles[k.min(vehicles.len() - 1)];
                let numbers = numbers.get(j).cloned().into_iter().collect::<Vec<_>>();
                (ty.clone(), numbers, hof.clone())
            }
            _ => {
                // a plain [aigroup_2] flies/drives its own vehicles (the Tegel approach)
                let root = world.root().to_path_buf();
                let ty = {
                    let pool = self.pool(&root, world, &group);
                    pool.get((h % pool.len().max(1) as u64) as usize).cloned()?
                };
                // The group names no depot file, so Omsi.exe leaves the bus's selected-hof
                // index at 0: it runs with the depot of its own folder (see [`pool_depot`]).
                // Without one such a bus got no `SetLineTo`, no `AI_target_index` and no
                // `ai_scheduled_settarget` trigger at all - and a mod bus that switches its
                // destination picture on in that trigger drove with a blank display.
                let hof = self.pool_hof(ty.def.dir(), world);
                (ty, Vec::new(), hof)
            }
        };
        // A depot bus as Omsi.exe makes it (0x70a174): the fleet number of its ailists line;
        // the plate of that line, else - unless the bus's plates are free - the plate the bus
        // gives the number ([registration_list] / [registration_automatic]); and the repaint
        // that line names, else the model's own paint (the first repaint when the default
        // paint is "<nouse>"). Another tour's bus draws a repaint at random, as random
        // traffic does.
        let entry = numbers.first().cloned();
        let number = entry.as_ref().map(|e| {
            let plate = if !e.registration.trim().is_empty() {
                e.registration.clone()
            } else if ty.def.registration_mode != 1 {
                ty.def.plate_of_number(&e.number)
            } else {
                String::new()
            };
            (e.number.clone(), plate)
        });
        let scheme = if ty.paint_schemes.is_empty() {
            None
        } else if let Some(e) = &entry {
            ty.paint_schemes
                .iter()
                .position(|s| s.name.trim_end() == e.paint.trim_end())
                .or_else(|| (ty.def.default_paint.trim() == "<nouse>").then_some(0))
        } else {
            Some(
                ((h >> 42) % ty.paint_schemes.len().min(crate::ai_traffic::AI_SCHEMES) as u64)
                    as usize,
            )
        };
        Some(Choice {
            ty,
            number,
            hof,
            scheme,
            train,
        })
    }

    /// The departures of the trips on the road at `day_time` and of those leaving in the
    /// next `ahead` seconds that the AI drives today: whose vehicles the fleet reads and
    /// uploads ahead (omsi-app's `Schedule::upcoming_sets`).
    pub fn upcoming(&self, day_time: f64, ahead: f64) -> Vec<usize> {
        let end = self
            .departures
            .partition_point(|d| d.time <= day_time - self.day_base + ahead);
        let mut out = Vec::new();
        for i in 0..end {
            if self.is_player_tour(i) || !self.runs(i) {
                continue;
            }
            if self.dep_time(i) + self.times_of(i).duration < day_time {
                continue;
            }
            out.push(i);
        }
        out
    }

    /// The depot vehicles of every AI group.
    pub fn depot_vehicles(&self) -> impl Iterator<Item = &DepotVehicle> {
        self.depots.values().flatten()
    }

    /// The vehicles of the plain `[aigroup_2]` pools loaded so far.
    pub fn pooled_types(&self) -> Vec<Arc<VehicleType>> {
        self.pools.values().flatten().cloned().collect()
    }

    /// Vehicles of a plain `[aigroup_2]`, loaded on first use and kept.
    fn pool(&mut self, root: &Path, world: &dyn TimetableWorld, group: &str) -> &[Arc<VehicleType>] {
        if !self.pools.contains_key(group) {
            let mut out = Vec::new();
            for g in world
                .ailists()
                .groups
                .iter()
                .filter(|g| !g.is_depot && g.name.to_ascii_lowercase() == group)
            {
                for v in &g.vehicles {
                    if v.file.to_ascii_lowercase().ends_with(".zug") {
                        continue;
                    }
                    let path = omsi_cfg::resolve_path(root, &v.file);
                    match VehicleType::load_ai(root, &path) {
                        Ok(t) => out.push(Arc::new(t)),
                        Err(e) => log::warn!("AI group '{}' vehicle {}: {e}", g.name, v.file),
                    }
                }
            }
            if !out.is_empty() {
                log::info!(
                    "AI group '{group}': {} vehicles loaded for its timetable trips",
                    out.len()
                );
            }
            self.pools.insert(group.to_string(), out);
        }
        self.pools.get(group).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// The depot file a bus of a plain `[aigroup_2]` group runs with (see [`pool_depot`]),
    /// looked up once per vehicle folder.
    pub fn pool_hof(&mut self, dir: &Path, world: &dyn TimetableWorld) -> Option<Arc<omsi_vehicle::Hof>> {
        if let Some(h) = self.pool_hofs.get(dir) {
            return h.clone();
        }
        let names: Vec<&str> = world
            .ailists()
            .groups
            .iter()
            .filter_map(|g| g.hof.as_deref())
            .collect();
        let h = pool_depot(dir, &names).map(Arc::new);
        self.pool_hofs.insert(dir.to_path_buf(), h.clone());
        h
    }
}

/// The depot file of a bus whose AI group names none (a plain `[aigroup_2]` pool): the one
/// of the map's own depots where its vehicle folder has it (the group is the map author's
/// shortcut for a fleet the `[aigroup_depot]` groups name with a depot), else the folder's
/// first `.hof` - what Omsi.exe uses for such a bus, whose selected-hof index stays 0
/// ("the hofs are loaded in the order the folder lists them", [`omsi_vehicle::hof::depot_files`]).
///
/// A timetable bus is spawned with this depot file: with none it got no `SetLineTo`, no
/// `AI_target_index` and no `ai_scheduled_settarget` trigger, and a mod bus that switches
/// its destination picture on in that trigger (the HK roller blinds, the LED matrices)
/// drove with a blank display instead of its destination.
pub fn pool_depot(dir: &Path, map_depots: &[&str]) -> Option<omsi_vehicle::Hof> {
    omsi_vehicle::hof::depot_like(dir, map_depots).or_else(|| {
        let files = omsi_vehicle::hof::depot_files(dir);
        // (the first file that loads: one unreadable depot must not leave the bus without
        // the displays and the stops of the rest)
        files.iter().find_map(|p| omsi_vehicle::Hof::load(p).ok())
    })
}

/// The depot file an `[aigroup_depot]` names for a vehicle: a file of that name next to the
/// vehicle, else the one whose `[name]` it is - the stock groups name the depot
/// ("Spandau 1986"), not the file ("Spandau 86.hof"), and without it no scheduled bus had
/// termini or stops for its displays.
pub fn depot_file(
    cache: &mut HashMap<(PathBuf, String), Option<Arc<omsi_vehicle::Hof>>>,
    dir: &Path,
    name: &str,
) -> Option<Arc<omsi_vehicle::Hof>> {
    let key = (dir.to_path_buf(), name.trim().to_ascii_lowercase());
    if let Some(h) = cache.get(&key) {
        return h.clone();
    }
    // (through the content file system: the bus may be in an archive, and a mod may add
    // depot files to a stock bus folder)
    let mut found = omsi_vehicle::hof::depot_in(dir, name);
    if found.is_none() {
        // a mod bus brings only the depot of the map it was made on: the map's depot as
        // another vehicle folder has it (`omsi_vehicle::hof::depot_anywhere`)
        found = omsi_vehicle::hof::depot_anywhere(name);
        match &found {
            Some(h) => log::info!(
                "{} has no depot file '{name}'; using {}",
                dir.display(),
                h.path.display()
            ),
            None => log::debug!(
                "no depot file '{name}' next to {} or in any vehicle folder",
                dir.display()
            ),
        }
    }
    let h = found.map(Arc::new);
    cache.insert(key, h.clone());
    h
}
