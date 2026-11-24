//! How much random traffic the map makes now, of which kinds, and the dice.

use super::*;

impl TrafficSim {
    pub fn prime_pull_out_room(&mut self, ty: &VehicleType, bus: bool) {
        let (front, rear, half_width) = extents(ty, if bus { 12.0 } else { 4.5 });
        self.pull_out_room(ty, front, rear, half_width);
    }

    /// How far behind something standing a vehicle of `ty` stops so that it can pull out
    /// round it later (`crate::ai_motion::pull_out_room` against a standing bus with the
    /// oncoming lane 3.3 m over; by vehicle file).
    pub fn pull_out_room(
        &mut self,
        ty: &VehicleType,
        front: f32,
        rear: f32,
        half_width: f32,
    ) -> f32 {
        if let Some(&r) = self.pull_out_rooms.get(&ty.def.path) {
            return r;
        }
        // (a bus 2.5 m wide that stands up to 0.35 m further over than the car: bus and car
        // are rarely both in the middle of the lane)
        let r = crate::ai_motion::pull_out_room(&ty.def, (front, rear, half_width), 1.6, 3.3);
        if omsi_cfg::flags::OMSI_DEBUG_TRAFFIC.is_set() {
            log::info!(
                "pull-out room of {}: {r:.2} m (front {front:.2}, half width {half_width:.2})",
                ty.def
                    .path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
            );
        }
        self.pull_out_rooms.insert(ty.def.path.clone(), r);
        r
    }

    pub fn rand(&mut self) -> u64 {
        let mut x = self.rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    pub fn rand_f(&mut self) -> f64 {
        (self.rand() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// How much traffic group `g` makes now: its `unsched_trafficdens.txt` factor times its
    /// curve for this day of the week (+1 Monday to Friday, +2 Saturday, +4 Sunday, 0 every
    /// day).
    pub fn group_density(&self, g: usize) -> f32 {
        let Some(gr) = self.groups.get(g) else {
            return 0.0;
        };
        if !self.group_curves {
            return 1.0;
        }
        let bit = match self.weekday {
            0..=4 => 1,
            5 => 2,
            _ => 4,
        };
        let hour = (self.day_time.rem_euclid(86400.0) / 3600.0) as f32;
        match gr
            .densities
            .iter()
            .find(|(mask, _)| *mask == 0 || mask & bit != 0)
        {
            Some((_, curve)) => gr.factor * omsi_map::global::curve_at(curve, hour).max(0.0),
            None => 0.0,
        }
    }

    /// The street traffic density now, 1 = the map's normal level, times the options' share
    /// of random traffic.
    pub fn street_density(&self) -> f32 {
        self.street_density_map() * self.unsched_factor
    }

    pub fn street_density_map(&self) -> f32 {
        if !self.group_curves {
            return omsi_map::global::curve_at(
                &self.density_curve,
                (self.day_time.rem_euclid(86400.0) / 3600.0) as f32,
            )
            .clamp(0.0, 2.0);
        }
        let groups: Vec<usize> = (0..self.groups.len())
            .filter(|&g| {
                self.types
                    .iter()
                    .any(|t| t.3 == g && t.2 == LaneKind::Street)
            })
            .collect();
        let factors: f32 = groups.iter().map(|&g| self.groups[g].factor).sum();
        if factors <= 0.0 {
            return 0.0;
        }
        (groups.iter().map(|&g| self.group_density(g)).sum::<f32>() / factors).clamp(0.0, 2.0)
    }

    /// How much of group `g`'s traffic `lane` carries: the path's `[rule] trafficdensity`
    /// for the group, else the group's default (see `uvg_defaults`).
    pub fn lane_group_density(&self, lane: &crate::traffic::Lane, g: usize) -> f32 {
        match self.group_uvg.get(g).copied().flatten() {
            Some(u) => lane.pool_density(&self.uvg_defaults, u),
            None => lane.density,
        }
    }

    /// How much random traffic a street lane brings (`Lane::density`) where its paths set
    /// densities per group: each group's density there (its `[rule] trafficdensity`, or what
    /// it inherits) times how much traffic the map makes of that group relative to its
    /// first group (the `unsched_trafficdens.txt` factors, never more than the lane's
    /// own), the largest over the groups. A lane open to some traffic stays a little above
    /// 0. Taking the rarest group's rule as it was, BRT Berlin's Kurfürstendamm bus lanes,
    /// open only to ambulances (3.0 at a factor of 0.0002), spawned queues of ambulances.
    /// None: the lane keeps the density its paths gave it (no rules per group, or the map
    /// has no `unsched_trafficdens.txt`).
    pub fn spawn_density(&self, lane: &crate::traffic::Lane) -> Option<f32> {
        if lane.kind != LaneKind::Street || lane.group_density.is_empty() || !self.group_curves {
            return None;
        }
        let reference = self.groups.first().map(|g| g.factor).filter(|f| *f > 0.0)?;
        let mut busiest: Option<f32> = None;
        for (g, group) in self.groups.iter().enumerate() {
            // (a group with no street vehicles brings nobody here)
            let on_streets = self
                .types
                .iter()
                .any(|t| t.3 == g && t.2 == LaneKind::Street);
            let Some(rules) = self
                .group_uvg
                .get(g)
                .copied()
                .flatten()
                .filter(|_| on_streets)
            else {
                continue;
            };
            let density_here = lane.pool_density(&self.uvg_defaults, rules);
            if density_here > 0.0 {
                let weighed = density_here * (group.factor / reference).clamp(0.0, 1.0);
                busiest = Some(busiest.map_or(weighed, |b| b.max(weighed)));
            }
        }
        Some(busiest.map_or(0.0, |d| d.max(SPAWN_DENSITY_FLOOR)))
    }

    /// `spawn_density` for the lanes `lanes` (those of a new tile, or all at the start).
    pub fn set_spawn_densities(&mut self, lanes: std::ops::Range<usize>) {
        for l in lanes {
            if let Some(d) = self
                .net
                .lanes
                .get(l)
                .and_then(|lane| self.spawn_density(lane))
            {
                self.net.lanes[l].density = d;
            }
        }
    }

    /// A random vehicle type for a lane of `kind`: on a street `lane`, of the groups that
    /// lane carries, as much as it carries of each.
    pub fn pick_type(&mut self, kind: LaneKind, lane: Option<usize>) -> Option<Arc<VehicleType>> {
        // a vehicle's share: its weight within its group times what the group makes now
        let group_weight: Vec<f32> = (0..self.groups.len())
            .map(|g| {
                self.types
                    .iter()
                    .filter(|t| t.3 == g && t.2 == kind)
                    .map(|t| t.1)
                    .sum::<f32>()
            })
            .collect();
        let dens: Vec<f32> = (0..self.groups.len())
            .map(|g| {
                if kind == LaneKind::Street {
                    let here = lane
                        .and_then(|i| self.net.lanes.get(i))
                        .map(|l| self.lane_group_density(l, g))
                        .unwrap_or(1.0);
                    self.group_density(g) * here
                } else {
                    1.0
                }
            })
            .collect();
        // (and only the vehicles the lane is open to: Grundorf's trucks where it says
        // `trucks`, Omsi.exe 0x71d714)
        let barred: Vec<*const VehicleType> = match lane
            .filter(|_| kind == LaneKind::Street)
            .and_then(|i| self.net.lanes.get(i))
        {
            Some(l) => self
                .types
                .iter()
                .filter(|t| !l.allows(t.0.def.ai_veh_type))
                .map(|t| Arc::as_ptr(&t.0))
                .collect(),
            None => Vec::new(),
        };
        let weight = |t: &(Arc<VehicleType>, f32, LaneKind, usize)| -> f32 {
            let gw = group_weight.get(t.3).copied().unwrap_or(0.0);
            if gw <= 0.0 || barred.contains(&Arc::as_ptr(&t.0)) {
                0.0
            } else {
                t.1 / gw * dens.get(t.3).copied().unwrap_or(0.0)
            }
        };
        let total: f32 = self.types.iter().filter(|t| t.2 == kind).map(weight).sum();
        if total <= 0.0 {
            return None;
        }
        let mut x = self.rand_f() as f32 * total;
        for t in self.types.iter().filter(|t| t.2 == kind) {
            let w = weight(t);
            if x < w {
                return Some(t.0.clone());
            }
            x -= w;
        }
        self.types
            .iter()
            .filter(|t| t.2 == kind)
            .next_back()
            .map(|t| t.0.clone())
    }
}

/// The least `spawn_density` of a lane open to some random traffic.
pub const SPAWN_DENSITY_FLOOR: f32 = 0.05;

/// How many times the base street target the neighbourhood asks for, from the trafficdensity
/// of each street lane starting near the player (0 for a no_cars lane): the count of lanes
/// per about 250 (1 to 4) times their mean density (0 to 2).
pub fn road_scale(near_density: &[f32]) -> f32 {
    let road = (near_density.len() as f32 / 250.0).clamp(1.0, 4.0);
    if near_density.is_empty() {
        return road;
    }
    let mean = near_density.iter().sum::<f32>() / near_density.len() as f32;
    road * mean.clamp(0.0, 2.0)
}
