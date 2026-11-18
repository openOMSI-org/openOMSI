//! Artificial object paths.
use super::*;
use omsi_sim::ai_motion::{AiBody, MotionKind};
use omsi_sim::traffic::{AiState, Network};
use omsi_vehicle::{Axle, Vehicle};

fn path(
    keyword: &str,
    start: DVec3,
    heading: f64,
    radius: f64,
    length: f64,
    gradients: [f64; 2],
    direction: i32,
) -> String {
    let extra = if keyword == "path_2" { "0\n0\n" } else { "" };
    format!("[{keyword}]\n{}\n{}\n{}\n{heading}\n{radius}\n{length}\n{}\n{}\n0\n3\n{direction}\n0\n{extra}", start.x, start.y, start.z, gradients[0], gradients[1])
}

fn lanes(text: &str, pos: DVec3, heading: f64) -> Vec<Lane> {
    let sco = SceneryObject::parse(&omsi_cfg::CfgFile::from_str("artificial.sco", text));
    object_lanes(&sco, pos, [heading, 0.0, 0.0], None, (0, 0), 1, &[])
}

#[test]
fn object_path_gradients_preserve_heights_and_reverse_samples() {
    // The expected heights are independent hand-calculated checkpoints, including
    // a crest whose end alone cannot distinguish a straight line from its profile.
    for keyword in ["path", "path_2"] {
        for (gradients, expected) in [
            ([0.0, 0.0], [5.0, 5.0, 5.0]),
            ([0.04, 0.04], [5.0, 5.4, 5.8]),
            ([-0.03, -0.03], [5.0, 4.7, 4.4]),
            ([0.06, -0.04], [5.0, 5.35, 5.2]),
        ] {
            for radius in [0.0, 40.0, -40.0] {
                let text = path(
                    keyword,
                    DVec3::new(2.0, 3.0, 5.0),
                    25.0,
                    radius,
                    20.0,
                    gradients,
                    2,
                );
                let ls = lanes(&text, DVec3::new(100.0, -80.0, 7.0), 63.0);
                assert_eq!(ls.len(), 2);
                for (i, z) in [(0, expected[0]), (5, expected[1]), (10, expected[2])] {
                    assert!(
                        (ls[0].points[i].z - (z + 7.0)).abs() < 1e-6,
                        "{keyword} {gradients:?} radius {radius}: {:?}",
                        ls[0].points[i]
                    );
                }
                let h = 63f64.to_radians();
                let start = DVec3::new(
                    100.0 + 2.0 * h.cos() + 3.0 * h.sin(),
                    -80.0 - 2.0 * h.sin() + 3.0 * h.cos(),
                    12.0,
                );
                let horizontal =
                    LaneBuilder::arc(start, 88.0, 20.0, radius, 0.0, LaneKind::Street, 3.0);
                for (p, q) in ls[0].points.iter().zip(&horizontal.points) {
                    assert!((p.truncate() - q.truncate()).length() < 1e-6);
                }
                assert!(ls[1].reversed);
                assert_eq!(
                    ls[1].points,
                    ls[0].points.iter().rev().copied().collect::<Vec<_>>()
                );
            }
        }
    }
}

#[test]
fn descending_object_path_joins_do_not_lift_the_rear_axle() {
    for keyword in ["path", "path_2"] {
        for reverse in [false, true] {
            // Long paths alternate with short ones. The real road descends smoothly,
            // but treating a gradient as a height change leaves the long lane far above it.
            let mut text = String::new();
            let mut y = 0.0;
            for length in [48.0, 4.0, 36.0, 4.0, 36.0] {
                text.push_str(&path(
                    keyword,
                    DVec3::new(0.0, y, 10.0 - 0.03 * y),
                    0.0,
                    0.0,
                    length,
                    [-0.03, -0.03],
                    reverse as i32,
                ));
                y += length;
            }
            let mut net = Network {
                lanes: lanes(&text, DVec3::ZERO, 0.0),
                ..Default::default()
            };
            if reverse {
                net.lanes.reverse();
            }
            for i in 0..net.lanes.len() - 1 {
                net.lanes[i].next = vec![i + 1];
            }
            let mut state = AiState::new(0, 5.0, 1);
            state.plan_next(&net);
            let mut def = Vehicle {
                mass: 1.2,
                moment_of_inertia: [1.6, 0.5, 1.8],
                cog_height: 0.45,
                rot_pnt_long: -1.5,
                ..Default::default()
            };
            def.axles = [1.5, -1.5]
                .into_iter()
                .map(|long| Axle {
                    long,
                    max_width: 1.6,
                    spring: 45.0,
                    damper: 3.0,
                    wheel_diameter: 0.6,
                    ..Default::default()
                })
                .collect();
            let ground = |_x: f64, y: f64, top: f64| {
                let z = 10.0 - 0.03 * y;
                omsi_sim::rigid::GroundProbe {
                    below: (z <= top).then_some(z),
                    above: None,
                }
            };
            let mut body = AiBody::new(&def, MotionKind::Road);
            body.place(&|d| state.way_point(&net, d), None, Some(&ground), 6.0);
            let mut worst_gap = 0.0f64;
            let mut worst_pitch = 0.0f32;
            for _ in 0..550 {
                state.s += 0.2;
                if state.s >= net.lanes[state.lane].length() {
                    state.s -= net.lanes[state.lane].length();
                    state.prev_lane = Some(state.lane);
                    state.lane += 1;
                    state.planned_next = None;
                    state.ahead.clear();
                    state.plan_next(&net);
                }
                body.step(
                    1.0 / 30.0,
                    6.0,
                    &|d| state.way_point(&net, d),
                    None,
                    Some(&ground),
                );
                let slope = if reverse { 0.03f32 } else { -0.03f32 };
                worst_pitch = worst_pitch.max((body.pitch_deg - slope.atan().to_degrees()).abs());
                for (i, a) in def.axles.iter().enumerate() {
                    let wheel_y = body.position.y + body.heading.to_radians().cos() * a.long as f64;
                    let contact = body.position.z
                        + (a.long * body.pitch_deg.to_radians().tan() - body.suspension[i][0])
                            as f64;
                    worst_gap = worst_gap.max((contact - (10.0 - 0.03 * wheel_y)).abs());
                }
            }
            assert!(
                worst_gap < 0.02,
                "{keyword}: axle contact left the road by {worst_gap:.3} m"
            );
            assert!(
                worst_pitch < 0.5,
                "{keyword}: pitch departed from the slope by {worst_pitch:.2} degrees"
            );
        }
    }
}
