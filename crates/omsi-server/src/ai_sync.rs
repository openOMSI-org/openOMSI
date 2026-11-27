//! Renderer-free LAN traffic snapshots. This mirrors the graphical host's
//! traffic table writer, but has no camera, vehicle graphics or sound sets.
//! Population remains gated until the CPU road/collision model reaches parity.

use omsi_net::traffic_table::{Kind, Look, Meta, Row, TrafficTable};
use omsi_net::{LanSession, Role};
use omsi_sim::VehicleInstance;
use omsi_sim::ai_traffic::TrafficSim;
use std::path::Path;

pub const TRAFFIC_BYTES_PER_SECOND: f64 = 40_000.0;

#[derive(Default)]
pub struct HeadlessTrafficSync {
    pub table: TrafficTable,
    meta_time: f32,
    pub sent_bytes: u64,
}

fn car_meta(v: &VehicleInstance) -> Meta {
    let text = |name: &str| {
        v.ty.program
            .str_var(name)
            .and_then(|i| v.state.str_vars.get(i as usize))
            .map(|s| s.trim().to_string())
            .unwrap_or_default()
    };
    let target = v
        .var("AI_target_index")
        .filter(|index| *index >= 0.0)
        .map(|index| index as i32)
        .unwrap_or(-1);
    let hof = v
        .host
        .hof
        .as_ref()
        .and_then(|h| h.path.file_stem())
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    Meta {
        line: text("SetLineTo"),
        destination: format!("#{target}"),
        number: text("number"),
        ident: text("ident"),
        hof,
    }
}

fn traffic_viewer_position(pose: &omsi_net::Pose) -> Option<[f64; 3]> {
    crate::interest::client_position(pose).map(|position| position.to_array())
}

impl HeadlessTrafficSync {
    /// Write authoritative AI rows, including paint, destination, movement
    /// and stable IDs. TrafficTable handles delayed ID reuse after despawn.
    pub fn update(&mut self, traffic: &TrafficSim, root: &Path) {
        if traffic.mirror {
            self.table.clear();
            return;
        }
        let refresh = traffic.time - self.meta_time >= 1.0;
        if refresh {
            self.meta_time = traffic.time;
        }
        self.table.begin(traffic.time as f64);
        for car in &traffic.cars {
            let vehicle = &car.vehicle;
            let kind = if car.is_bus() {
                Kind::Bus
            } else if car.is_rail() {
                Kind::Rail
            } else {
                Kind::Car
            };
            let at_station = if car.at_station() {
                1
            } else if !vehicle.station_released() {
                -1
            } else {
                0
            };
            let row = Row::from_state(
                [vehicle.position.x, vehicle.position.y, vehicle.position.z],
                vehicle.heading,
                vehicle.pitch as f64,
                vehicle.bank as f64,
                car.state.speed as f64,
                car.body.steer as f64,
                car.state.blinker.clamp(0, 3) as u8,
                car.state.braking,
                traffic.night,
                at_station,
            );
            let look = || Look {
                file: omsi_lan::mirror::relative_file(&vehicle.ty.def.path, root),
                scheme: car.scheme.map(|scheme| scheme.min(254) as u8),
                paint: car
                    .scheme
                    .and_then(|scheme| vehicle.ty.paint_schemes.get(scheme))
                    .map(|paint| paint.name.clone())
                    .unwrap_or_default(),
            };
            let meta = || {
                if car.is_bus() {
                    car_meta(vehicle)
                } else {
                    Meta::default()
                }
            };
            let Some(id) = self.table.upsert(car.id, kind, look, meta, row) else {
                continue;
            };
            if car.is_bus() && refresh {
                self.table.set_meta(id, &car_meta(vehicle));
            }
        }
        self.table.finish();
    }

    /// Use the production acknowledged, bandwidth-limited traffic protocol.
    /// Never transmit AI rows from an untrusted or non-host LAN session.
    pub fn publish(&mut self, lan: &mut LanSession, dt: f32) -> usize {
        if lan.role != Role::Host || !dt.is_finite() || dt <= 0.0 {
            return 0;
        }
        // The on-foot spectator is a genuine traffic viewer too. The host's
        // population and world streaming already follow Walker coordinates;
        // excluding them here hid all AI/timetable buses until they spawned
        // a vehicle. Reuse the same validated coordinate policy.
        let viewers: Vec<_> = lan
            .peers()
            .filter(|peer| peer.has_pose && peer.has_info)
            .filter_map(|peer| {
                traffic_viewer_position(&peer.pose).map(|position| (peer.pose.id, position))
            })
            .collect();
        let bytes = lan.traffic_publish(&self.table, &viewers, dt, TRAFFIC_BYTES_PER_SECOND);
        self.sent_bytes += bytes as u64;
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_sim::ai_traffic::setup::RandomTypes;
    use omsi_sim::traffic::Network;

    #[test]
    fn walking_spectators_receive_scheduled_traffic_without_a_bus() {
        let pose = omsi_net::Pose {
            walker: Some(omsi_net::Walker {
                x: 4_000.0,
                y: 20.0,
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(!pose.has_vehicle());
        let at = traffic_viewer_position(&pose).expect("walker position");
        assert_eq!(at[0], 4_000.0);
        assert_eq!(at[1], 20.0);
        assert!(traffic_viewer_position(&omsi_net::Pose::default()).is_none());
    }

    #[test]
    fn empty_host_does_not_invent_ai_cars_or_traffic_rows() {
        let random = RandomTypes {
            types: Vec::new(),
            groups: Vec::new(),
            group_curves: false,
            group_uvg: Vec::new(),
            uvg_defaults: Vec::new(),
        };
        let mut traffic = crate::traffic::assemble(
            Path::new("."),
            Network::default(),
            random,
            Vec::new(),
            Vec::new(),
            0,
        );
        let mut sync = HeadlessTrafficSync::default();
        sync.update(&traffic, Path::new("."));
        assert!(sync.table.is_empty());
        traffic.time += 1.0;
        sync.update(&traffic, Path::new("."));
        assert_eq!(sync.table.len(), 0);
    }
}
