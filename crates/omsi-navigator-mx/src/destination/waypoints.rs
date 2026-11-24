//! Ordered free-drive waypoints and the cached road segments between them.
use super::*;

impl Destination {
    pub(super) fn new(at: DVec2, name: Option<String>) -> Self {
        Self {
            at,
            name: name
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| omsi_ui::tr("Destination").into_owned()),
            status: DestinationStatus::Pending,
            path: None,
            network: None,
            retry_in: 0.0,
        }
    }
}

impl Navigator {
    /// Append a point without interrupting the leg currently being driven.
    pub fn add_destination(&mut self, at: DVec2, name: Option<String>) {
        if !at.is_finite()
            || self
                .destination
                .iter()
                .chain(&self.waypoints)
                .any(|d| (d.at - at).length() < 1.0)
        {
            return;
        }
        if self.destination.is_none() {
            self.activate_destination(Destination::new(at, name));
        } else {
            self.waypoints.push(Destination::new(at, name));
            self.route.version += 1;
        }
    }

    /// Index zero is the active point; later indices follow the displayed numbering.
    pub fn remove_destination(&mut self, index: usize) {
        if index == 0 {
            if self.waypoints.is_empty() {
                self.cancel_destination();
            } else {
                let next = self.waypoints.remove(0);
                self.activate_destination(next);
                if let Some(d) = self.destination.as_mut() {
                    d.path = None;
                    d.network = None;
                    d.status = DestinationStatus::Pending;
                    d.retry_in = 0.0;
                }
            }
        } else if index <= self.waypoints.len() {
            self.waypoints.remove(index - 1);
            self.route.version += 1;
        } else {
            return;
        }
        self.invalidate_waypoint_paths();
        self.city.waypoint_hits.clear();
        self.city.waypoint_remove.clear();
    }

    pub(super) fn invalidate_waypoint_paths(&mut self) {
        for d in &mut self.waypoints {
            d.path = None;
            d.status = DestinationStatus::Pending;
        }
    }

    pub(crate) fn follow_destination(&mut self, f: &NavFrame) {
        // Keep the final arrival status for one frame before resuming the timetable.
        if self
            .destination
            .as_ref()
            .is_some_and(|d| d.status == DestinationStatus::Arrived)
        {
            self.remove_destination(0);
            if self.destination.is_none() {
                return;
            }
        }
        let endpoint = self.destination.as_ref().and_then(|d| {
            d.path
                .as_ref()
                .map(|p| (d.network, p.end, p.lanes.last().copied()))
        });
        self.follow_active_destination(f);
        let current = self.destination.as_ref().and_then(|d| {
            d.path
                .as_ref()
                .map(|p| (d.network, p.end, p.lanes.last().copied()))
        });
        if endpoint != current {
            self.invalidate_waypoint_paths();
        }
        if !self.waypoints.is_empty()
            && self
                .destination
                .as_ref()
                .is_some_and(|d| d.status == DestinationStatus::Arrived)
        {
            self.remove_destination(0);
            self.follow_active_destination(f);
        }
        self.plan_waypoint_paths(f);
    }

    fn plan_waypoint_paths(&mut self, f: &NavFrame) {
        let Some(net) = self.global.as_deref().or(f.traffic.map(|t| &t.net)) else {
            return;
        };
        let mut previous = self.destination.as_ref().and_then(|d| d.path.as_ref());
        let mut changed = false;
        for d in &mut self.waypoints {
            let Some(path) = previous else {
                changed |= d.path.take().is_some();
                d.status = DestinationStatus::Pending;
                continue;
            };
            if d.status == DestinationStatus::Pending {
                let heading = path
                    .lanes
                    .last()
                    .and_then(|&i| net.lanes.get(i))
                    .map(|l| l.at(path.end_s).1 as f64)
                    .unwrap_or(f.heading);
                d.path = route_to_destination(net, path.end, heading, d.at);
                d.status = if d.path.is_some() {
                    DestinationStatus::Navigating
                } else {
                    DestinationStatus::Unreachable
                };
                changed = true;
            }
            previous = d.path.as_ref();
        }
        if changed {
            self.route.version += 1;
        }
    }

    pub(crate) fn draw_waypoint_routes(
        &self,
        painter: &mut Painter,
        net: &Network,
        anchor: DVec2,
        style: &RouteStyle,
        pal: &style::Palette,
    ) {
        for path in self.waypoints.iter().filter_map(|d| d.path.as_ref()) {
            mapmesh::roads::build_route_to(
                painter,
                net,
                &path.lanes,
                anchor,
                path.start_s,
                &self.route_jam,
                style,
                0,
                pal,
                Some(path.end_s),
            );
        }
    }
}
