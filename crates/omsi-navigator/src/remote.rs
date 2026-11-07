use super::*;

const AREA: f64 = 900.0;
const AHEAD: f32 = 2000.0;

pub struct RemoteNav {
    pub anchor: [f64; 2],
    pub route: Vec<[f32; 2]>,
    pub turn: Option<(i32, f32, f32, Option<String>)>,
    pub stop: Option<(String, f32)>,
    pub stops: Vec<[f32; 2]>,
    pub street: Option<String>,
    pub limit: Option<f32>,
    pub on_route: bool,
}

impl Navigator {
    pub fn forget_remote_area(&mut self) {
        self.remote_area = None;
    }

    pub fn remote_roads(&mut self, bus: DVec3) -> Option<([f64; 2], Vec<(Vec<[f32; 2]>, f32)>)> {
        let net = self.global.clone()?;
        let at = DVec2::new(bus.x, bus.y);
        if self.remote_area.is_some_and(|(a, v)| (a - at).length() < AREA / 3.0 && v == self.global_version) {
            return None;
        }
        self.remote_area = Some((at, self.global_version));
        let mut seen = hashbrown::HashSet::new();
        let mut roads = Vec::new();
        for i in roads::lanes_near(&net, at, AREA) {
            let l = &net.lanes[i];
            if l.kind != LaneKind::Street || l.invisible || l.points.len() < 2 || l.key.is_some_and(|k| !seen.insert((k, l.source))) {
                continue;
            }
            let pts: Vec<Vec3> = l.points.iter().map(|p| Vec3::new((p.x - at.x) as f32, (p.y - at.y) as f32, 0.0)).collect();
            roads.push((roads::simplify(&pts, 1.0).iter().map(|p| [p.x, p.y]).collect(), l.width));
        }
        Some(([at.x, at.y], roads))
    }

    pub fn remote_info(&self) -> Option<RemoteNav> {
        let net = self.global.as_deref()?;
        let anchor = self.remote_area?.0;
        let r = &self.route;
        let mut pts: Vec<Vec3> = Vec::new();
        let mut length = 0.0;
        'lanes: for (k, &l) in r.lanes.iter().enumerate().skip(r.progress) {
            let lane = net.lanes.get(l)?;
            for (j, p) in lane.points.iter().enumerate() {
                if k == r.progress && lane.dist.get(j).is_some_and(|d| *d < r.s) {
                    continue;
                }
                let q = Vec3::new((p.x - anchor.x) as f32, (p.y - anchor.y) as f32, 0.0);
                length += pts.last().map_or(0.0, |last| (q - *last).length());
                pts.push(q);
                if length > AHEAD {
                    break 'lanes;
                }
            }
        }
        Some(RemoteNav {
            anchor: [anchor.x, anchor.y],
            route: roads::simplify(&pts, 0.8).iter().map(|p| [p.x, p.y]).collect(),
            turn: self.next_turn.clone().map(|(direction, angle, metres, street)| (direction, angle, metres as f32, street)),
            stop: self.stop_spots.first().map(|s| (s.1.clone(), self.next_stop_distance_for(s.3).unwrap_or(0.0) as f32)),
            stops: self.stop_spots.iter().map(|s| [(s.0.x - anchor.x) as f32, (s.0.y - anchor.y) as f32]).collect(),
            street: self.street_here.clone(),
            limit: r.lanes.get(r.progress).and_then(|&l| net.lanes.get(l)).map(|l| l.speed_limit_kmh).filter(|v| *v > 0.0),
            on_route: r.on_route,
        })
    }
}
