//! What a placed tile brings the traffic, handed over through an `Inbox` (see
//! `crate::handover`): the light programs of its crossings as they are numbered, and its
//! lanes and parked cars once it is placed for the first time.

use crate::traffic::{Lane, TrafficLightController};
use glam::DVec3;

pub enum TrafficArrival {
    /// A crossing's light program, numbered as the lanes name it (`Lane::traffic_light`):
    /// the programs come in the order of their numbers.
    Light {
        index: usize,
        object: i64,
        controller: TrafficLightController,
    },
    /// A tile placed for the first time: its lanes, and its parked cars (world position and
    /// heading in degrees) - together, so that whoever takes them has every parked car with
    /// the lanes it stands beside.
    Tile {
        tile: (i32, i32),
        lanes: Vec<Lane>,
        parked: Vec<(DVec3, f64)>,
    },
}

/// What a take of the traffic's inbox brought, gathered.
#[derive(Default)]
pub struct TrafficArrivals {
    /// The tiles' lanes in the order the tiles came (see [`TrafficArrivals::sort_lanes`]).
    pub lanes: Vec<Lane>,
    pub parked: Vec<(DVec3, f64)>,
    /// The tiles they came from.
    pub tiles: Vec<(i32, i32)>,
    /// New light programs: (number, owning object, program), by number.
    pub lights: Vec<(usize, i64, TrafficLightController)>,
}

impl TrafficArrivals {
    pub fn gather(list: Vec<TrafficArrival>) -> TrafficArrivals {
        let mut out = TrafficArrivals::default();
        for a in list {
            match a {
                TrafficArrival::Light {
                    index,
                    object,
                    controller,
                } => out.lights.push((index, object, controller)),
                TrafficArrival::Tile {
                    tile,
                    lanes,
                    parked,
                } => {
                    out.lanes.extend(lanes);
                    out.parked.extend(parked);
                    out.tiles.push(tile);
                }
            }
        }
        out
    }

    /// The lanes waiting in `list`, copied (for a reader that only draws them).
    pub fn lanes_of(list: &[TrafficArrival]) -> Vec<Lane> {
        list.iter()
            .flat_map(|a| match a {
                TrafficArrival::Tile { lanes, .. } => lanes.as_slice(),
                TrafficArrival::Light { .. } => &[],
            })
            .cloned()
            .collect()
    }
}
