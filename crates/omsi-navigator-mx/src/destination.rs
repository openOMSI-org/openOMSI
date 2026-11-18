//! A player-selected destination, separate from the timetable and its route.

use super::*;
mod draw;
mod route;
mod waypoints;
pub(super) use route::route_to_destination;
use route::DestinationPath;

pub(super) struct EntryMarker {
    pub at: DVec2,
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DestinationStatus {
    Pending,
    Navigating,
    Unreachable,
    Arrived,
}

pub(super) struct Destination {
    pub at: DVec2,
    pub name: String,
    pub status: DestinationStatus,
    pub(super) path: Option<DestinationPath>,
    network: Option<(u64, usize)>,
    retry_in: f32,
}

impl Navigator {
    /// Choose any finite world point, without changing the player's duty.
    pub fn set_destination(&mut self, at: DVec2, name: Option<String>) {
        if !at.is_finite() {
            return;
        }
        self.waypoints.clear();
        self.activate_destination(Destination::new(at, name));
    }

    fn activate_destination(&mut self, destination: Destination) {
        let version = self.route.version + 1;
        if self.destination.is_none() {
            self.duty_route = Some(std::mem::take(&mut self.route));
        }
        self.route = Route {
            version,
            ..Default::default()
        };
        self.destination = Some(destination);
        self.city.waypoint_hits.clear();
        self.city.waypoint_remove.clear();
        self.next_turn = None;
        self.next_dist = None;
        self.dist_t = 0.0;
    }

    /// Return to the latest timetable route, including updates made during navigation.
    pub fn cancel_destination(&mut self) {
        self.waypoints.clear();
        self.city.waypoint_hits.clear();
        self.city.waypoint_remove.clear();
        if self.destination.take().is_none() {
            return;
        }
        let version = self.route.version + 1;
        self.route = self.duty_route.take().unwrap_or_default();
        self.route.version = version;
        self.route.on_route = false;
        self.route.joined = false;
        self.route.off_for = 0.0;
        self.route.retry_in = 0.0;
        self.next_turn = None;
        self.next_dist = None;
        self.dist_t = 0.0;
    }

    pub(super) fn duty_route_ref(&self) -> &Route {
        self.duty_route.as_ref().unwrap_or(&self.route)
    }
    pub(super) fn duty_route_mut(&mut self) -> &mut Route {
        self.duty_route.as_mut().unwrap_or(&mut self.route)
    }

    pub(super) fn reset_navigation_network(&mut self) {
        self.route = Route {
            version: self.route.version + 1,
            ..Default::default()
        };
        if let Some(r) = self.duty_route.as_mut() {
            *r = Route::default();
        }
        if let Some(d) = self.destination.as_mut() {
            d.path = None;
            d.network = None;
            d.status = DestinationStatus::Pending;
            d.retry_in = 0.0;
        }
        self.invalidate_waypoint_paths();
        self.next_turn = None;
        self.next_dist = None;
        self.dist_t = 0.0;
    }

    pub(super) fn read_entry_markers(&mut self, world: &omsi_scene::World) {
        self.entries = world
            .global
            .entry_points
            .iter()
            .filter_map(|ep| {
                let (p, _) = world.entry_point_place(ep)?;
                p.is_finite().then(|| EntryMarker {
                    at: p.truncate(),
                    name: ep.name.trim().to_string(),
                })
            })
            .collect();
    }

    /// Select an entry marker by its visible screen position, or the exact ground point.
    pub fn map_destination(&mut self, x: f32, y: f32) {
        let Some(at) = self.map_point(x, y) else {
            return;
        };
        let local = Vec2::new(x - self.city.rect[0], y - self.city.rect[1]);
        if self.city.buttons.iter().any(|(r, _)| r.contains(local))
            || self.city.lines.has_point(local)
        {
            return;
        }
        if self
            .city
            .waypoint_hits
            .iter()
            .any(|(r, _)| r.contains(local))
        {
            return;
        }
        if let Some(e) = self
            .city
            .entry_hits
            .iter()
            .rev()
            .find(|(rect, _)| rect.contains(local))
            .and_then(|(_, i)| self.entries.get(*i))
        {
            self.add_destination(e.at, Some(e.name.clone()));
            return;
        }
        let radius = 18.0 * self.city.pin_scale.max(0.5);
        let picked = self
            .entries
            .iter()
            .filter_map(|e| {
                let screen = self.city_screen_at(e.at)?;
                let distance = (screen - local).length();
                (distance <= radius).then_some((e, distance))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(e, _)| (e.at, e.name.clone()));
        match picked {
            Some((at, name)) => self.add_destination(at, Some(name)),
            None => self.add_destination(at, None),
        }
    }

    fn follow_active_destination(&mut self, f: &NavFrame) {
        let global = self.global.clone();
        let Some(net) = global.as_deref().or(f.traffic.map(|t| &t.net)) else {
            return;
        };
        let d = self.destination.as_mut().expect("destination navigation");
        let r = &mut self.route;
        let network = (self.global_version, net.lanes.len());
        if d.network != Some(network) {
            d.path = None;
            d.status = DestinationStatus::Pending;
            d.retry_in = 0.0;
            d.network = Some(network);
            *r = Route {
                version: r.version + 1,
                ..Default::default()
            };
        }
        if d.status == DestinationStatus::Arrived {
            return;
        }
        d.retry_in = (d.retry_in - f.dt).max(0.0);
        let best = (r.progress..r.lanes.len().min(r.progress + 60))
            .filter_map(|k| {
                let lane = net.lanes.get(r.lanes[k])?;
                let (s, distance) = lane.nearest_point(f.bus)?;
                if distance > 16.0 || angle_diff(f.heading, lane.at(s).1 as f64).abs() > 100.0 {
                    return None;
                }
                if k + 1 == r.lanes.len() && d.path.as_ref().is_some_and(|p| s > p.end_s + 20.0) {
                    return None;
                }
                Some((k, s, distance + k.saturating_sub(r.progress) as f64 * 0.05))
            })
            .min_by(|a, b| a.2.total_cmp(&b.2));
        match best {
            Some((k, s, _)) => {
                if r.progress != k {
                    r.version += 1;
                }
                r.progress = k;
                r.s = s;
                r.on_route = true;
                r.off_for = 0.0;
                if let Some(path) = d.path.as_ref() {
                    if k + 1 == r.lanes.len()
                        && s >= path.end_s - 10.0
                        && (f.bus - path.end).truncate().length() < 20.0
                    {
                        d.status = DestinationStatus::Arrived;
                        r.lanes.clear();
                        r.on_route = false;
                        r.version += 1;
                        self.next_turn = None;
                        return;
                    }
                }
            }
            None => {
                r.on_route = false;
                r.off_for += f.dt;
            }
        }
        if (d.path.is_none() || r.off_for > OFF_ROUTE_AFTER) && d.retry_in <= 0.0 {
            d.retry_in = REROUTE_EVERY * 2.0;
            d.path = route_to_destination(net, f.bus, f.heading, d.at);
            d.status = if d.path.is_some() {
                DestinationStatus::Navigating
            } else {
                DestinationStatus::Unreachable
            };
            *r = Route {
                lanes: d.path.as_ref().map(|p| p.lanes.clone()).unwrap_or_default(),
                version: r.version + 1,
                complete: true,
                ..Route::default()
            };
            if let Some(l) = r.lanes.first().and_then(|&i| net.lanes.get(i)) {
                r.s = l.nearest_point(f.bus).map(|p| p.0).unwrap_or(0.0);
            }
        }
    }

    pub(super) fn destination_distance(&self, net: &Network) -> Option<f32> {
        let d = self.destination.as_ref()?;
        if d.status == DestinationStatus::Arrived {
            return Some(0.0);
        }
        let path = d.path.as_ref()?;
        let r = &self.route;
        let mut total = -r.s;
        for (k, &i) in r.lanes.iter().enumerate().skip(r.progress) {
            total += if k + 1 == r.lanes.len() {
                path.end_s
            } else {
                net.lanes.get(i)?.length()
            };
        }
        Some(total.max(0.0))
    }
}
