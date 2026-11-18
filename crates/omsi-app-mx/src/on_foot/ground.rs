//! The ground under a walker's feet: the step up, the slope, a people's path's height.

use super::*;

/// The ground under the feet at `next` (the step about to be taken from `f.pos`): the floor the
/// feet come to, a step up to 45 cm, steep ground climbed no more than a step above the last
/// footing, a people's path's height where the ground disagrees with it; a step too high stays
/// where it was (`next` is put back, the speed lost).
pub(super) fn settle_on_ground(f: &mut OnFoot, w: &crate::scene::World, net: Option<&omsi_sim::traffic::Network>, z: f64, next: &mut DVec2) {
    // Steep ground (a bank, a wall's face the ground finds) is climbed no more than a
    // step above where the walker last stood on walkable ground: out of the bus under
    // a covered terminus the walker went up the bank beside it onto the deck, a few
    // centimetres a frame, each a step of its own. Stepping out of a bus starts the
    // limit afresh from the ground stepped onto.
    let footing = *f.footing.get_or_insert(z);
    let ground = w.walk_height_reach(next.x, next.y, z + f.lift, 1.0);
    // (on a people's path the floor is the path's where the ground differs from it:
    // the stairs of an object whose steps are no surface, a landing under a roof)
    let on_path = net.and_then(|n| n.footing_on_path(*next, z + f.lift, PATH_REACH));
    let ground = match on_path {
        Some(p) if ground.is_none_or(|g| (g - p.z).abs() > 0.2) => Some(p.z),
        _ => ground,
    };
    let on_path = on_path.is_some();
    let gradient_at = |g: f64| ground_gradient(|x, y| w.walk_height_reach(x, y, g, 0.5), *next, g);
    // (the gradient is only needed for ground above a step over the footing)
    let blocked = !on_path && ground.is_some_and(|g| g > footing + STEP && too_steep_to_climb(g, gradient_at(g), footing));
    match ground {
        Some(g) if blocked || (!on_path && g - (z + f.lift) > STEP) => {
            // too high a step: stay (and lose the speed into it)
            *next = f.pos.truncate();
            f.vel *= 0.3;
        }
        Some(g) => {
            let feet = z + f.lift;
            if on_path || gradient_at(g) <= MAX_WALK_GRADIENT {
                f.footing = Some(g);
            }
            if g >= feet - 0.02 || f.grounded() && feet - g < 0.4 {
                // on the ground (down a kerb the feet follow it)
                f.pos.z = g;
                if f.vz <= 0.0 {
                    f.lift = 0.0;
                } else {
                    f.lift = (feet - g).max(0.0);
                }
            } else {
                // stepped off something high: falling
                f.pos.z = g;
                f.lift = feet - g;
            }
        }
        None => {}
    }
}
