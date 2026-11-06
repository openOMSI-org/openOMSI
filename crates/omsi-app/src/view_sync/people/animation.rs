//! Opt-in restoration of the earlier procedural human poses.
//! Keep the original OMSI animation as the default for compatibility and A/B testing.

use super::*;
use glam::{Affine3A, Vec3};
use omsi_sim::human::{Pose, PoseInput, SLOTS};
use omsi_sim::people::{BusId, PoseFacts};

mod floor;

fn is_procedural_mode(value: &str) -> bool {
    matches!(value.to_ascii_lowercase().as_str(), "procedural" | "enhanced" | "1")
}

pub(super) fn enabled() -> bool {
    // Read once per game session (PeopleView::new), not once per process: on Android
    // another game is started in the launcher's existing process.
    let mode = omsi_cfg::flags::OMSI_PAX_ANIMATION.live_var()
        .unwrap_or_else(|_| crate::settings::Settings::load().passenger_animation);
    is_procedural_mode(&mode)
}

/// The previous foot-planted / IK pose system, driven by the current people's state (as
/// the frame has it: `PersonFrame::facts`). This changes only how bodies are skinned,
/// never boarding, movement or ticket logic. `st`: the person as drawn (the pose's own
/// state, the ankles for `OMSI_TRACE_PAX`).
pub(super) fn bones(p: &PersonFrame, ty: &omsi_sim::human::HumanType, st: &mut PersonView, dt: f32, world: &World, buses: &HashMap<BusId, &omsi_sim::people::cabin::BusNow>) -> [Affine3A; SLOTS] {
    let (origin, heading, frame) = match p.place {
        Place::Ground => (p.position, p.heading, 0),
        Place::Bus(bus, at) => {
            let frame = match bus {
                BusId::Player => 1,
                BusId::Ai(id) => id.wrapping_add(2),
            };
            (at.as_dvec3(), p.lheading, frame)
        }
    };
    let PoseFacts { activity, seat, reach, floor_lift, hold, seated_level } = p.facts;
    let bus = match p.place {
        Place::Bus(id, _) => buses.get(&id).copied(),
        Place::Ground => None,
    };
    // The place jumps within the bus where Omsi.exe puts a passenger straight onto the seat
    // from the aisle's nearest point (0.7 - 1.9 m): drawn as a glide at a walking pace, the
    // feet stepping along under the body, while the pose sits down - the person popped onto
    // the seat before.
    let origin = {
        // (in f64: the ground's places are world coordinates, far too large for an f32)
        // (on the ground as well: where the bus comes and the waiting take their way to the
        // door, their places jump a little; and from the ground into the bus, compared in
        // the world where they were drawn last - the bus stands then)
        // (a jump is the place's own, frame to frame: walking moves it by a few centimetres;
        // across the floor only - its height the floor and the seat's anchor decide, which
        // the pose follows by itself)
        match st.last_place {
            Some((f, last)) if f == frame => {
                let jump = ((last - origin).as_vec3()) * glam::Vec3::new(1.0, 1.0, 0.0);
                if (0.1..3.0).contains(&jump.length()) {
                    st.glide += jump;
                }
            }
            Some(_) => {
                let drawn = match bus {
                    Some(b) => b.to_local(st.last_world) - origin.as_vec3(),
                    None => (st.last_world - origin).as_vec3(),
                } * glam::Vec3::new(1.0, 1.0, 0.0);
                st.glide = if drawn.length() < 3.0 { drawn } else { glam::Vec3::ZERO };
            }
            None => st.glide = glam::Vec3::ZERO,
        }
        st.last_place = Some((frame, origin));
        let len = st.glide.length();
        if len > 0.0 {
            st.glide *= ((len - 1.2 * dt) / len).max(0.0);
        }
        st.glide_world = match bus {
            Some(b) => b.world(origin.as_vec3() + st.glide) - b.world(origin.as_vec3()),
            None => st.glide.as_dvec3(),
        };
        st.last_world = p.position + st.glide_world;
        origin + st.glide.as_dvec3()
    };
    // A seated renderer anchor is [passpos] - human [seatheight], not the floor.
    let level = match seated_level {
        Some(lift) => origin.z + lift,
        None => origin.z,
    };
    let sample = |at| floor::sample(world, bus, at, level);
    // (seated: the pelvis where the original seated pose has it - on a bus seat and on a
    // stop's bench alike - in the pose's frame, which stands `floor_lift` higher)
    let seat_pelvis = seat.map(|_| {
        let b = omsi_sim::human::slots_from_omsi(&p.anim.bones(&p.ty.omsi));
        let r = &p.ty.rig;
        let mid = (b[0].transform_point3(r.hip[0]) + b[1].transform_point3(r.hip[1])) * 0.5;
        mid + (r.pelvis - (r.hip[0] + r.hip[1]) * 0.5) - Vec3::Z * floor_lift
    });
    let input = PoseInput {
        activity,
        // The simulation's renderer still places a seated person at hip - model height.
        // The IK foot floor is hip - physical seat height, which can be higher.
        origin: origin + glam::DVec3::Z * floor_lift as f64,
        heading,
        frame,
        velocity: p.vel,
        seat,
        seat_pelvis,
        reach,
        hold,
        floor: Some(&sample),
        ..PoseInput::default()
    };
    let seed = p.id;
    let pose = st.procedural.get_or_insert_with(|| Pose::new(seed));
    pose.advance(&ty.rig, &input, dt);
    let mut posed = pose.bones(&ty.rig);
    super::avatars::clip_pose(p.id, ty, pose, &mut posed);
    // Shift the local pose back to the renderer's original anchor; no game logic moves.
    if floor_lift != 0.0 {
        let lift = Affine3A::from_translation(Vec3::Z * floor_lift);
        for bone in &mut posed.bones {
            *bone = lift * *bone;
        }
        posed.ankle = posed.ankle.map(|a| lift.transform_point3(a));
    }
    // OMSI_TRACE_PAX records the deformed ankles rather than the untouched spawn markers.
    st.ankles = posed.ankle;
    posed.bones
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enhanced_pose_requires_explicit_opt_in() {
        assert!(!is_procedural_mode(""));
        assert!(!is_procedural_mode("original"));
        assert!(!is_procedural_mode("0"));
        assert!(is_procedural_mode("procedural"));
        assert!(is_procedural_mode("enhanced"));
        assert!(is_procedural_mode("ENHANCED"));
        assert!(is_procedural_mode("1"));
    }
}
