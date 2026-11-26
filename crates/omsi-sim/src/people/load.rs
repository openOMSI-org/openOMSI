//! What the people aboard a bus weigh (openOMSI, the `passenger_mass` setting, #4014): each
//! rider's weight and where they stand or sit, by the bus's sections, for the physics to
//! carry. Off by default: the bus then weighs what its file says, whoever rides.

use super::cabin::CabinPart;
use super::model::{BusId, PeopleSim, Person, Place};
use glam::Vec3;

/// How a passenger's weight is taken (the `passenger_mass` setting).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum PassengerWeight {
    /// Not at all: the bus weighs what its file says (the default).
    #[default]
    Off,
    /// The `.hum`'s `[mass]` (tonnes, as the vehicles' `[mass]`); `kg` for a type without
    /// one. Each person's weight varies by up to `spread` (a share, 0..0.5) either way.
    Files { kg: f32, spread: f32 },
    /// `kg` for everybody, varied by up to `spread` either way.
    Fixed { kg: f32, spread: f32 },
}

/// The height of a person's centre of gravity over the point the cabin keeps for them (their
/// feet, or the seat's place on the floor), m.
const BODY_COG: f32 = 0.8;

impl PassengerWeight {
    /// Whether people weigh anything at all.
    pub fn on(&self) -> bool {
        *self != PassengerWeight::Off
    }

    /// What `p` weighs (kg).
    pub fn of(&self, p: &Person) -> f32 {
        let (base, spread) = match *self {
            PassengerWeight::Off => return 0.0,
            PassengerWeight::Files { kg, spread } => {
                let t = p.ty.def.mass;
                (
                    if t.is_finite() && t > 0.0 {
                        t * 1000.0
                    } else {
                        kg
                    },
                    spread,
                )
            }
            PassengerWeight::Fixed { kg, spread } => (kg, spread),
        };
        (base * (1.0 + spread.clamp(0.0, 0.5) * person_noise(p.id))).max(0.0)
    }
}

/// A number in -1..1 that is the same for one person every time it is asked for: their weight
/// stays theirs while they ride.
fn person_noise(id: u32) -> f32 {
    let mut x = (id as u64).wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    (x >> 40) as f32 / (1u64 << 23) as f32 - 1.0
}

/// The people aboard one bus by its sections, front first: each one's weight (kg) and centre
/// of gravity in the section's own model frame (x right, y forward, z up, m).
pub type SectionLoads = Vec<Vec<(f32, Vec3)>>;

/// The section of `parts` (front first, see `Cabin::parts`) a point of the cabin's unfolded
/// frame lies in, and the point in that section's own frame.
pub fn section_point(parts: &[CabinPart], p: Vec3) -> (usize, Vec3) {
    let k = parts
        .iter()
        .enumerate()
        .skip(1)
        .filter(|(_, s)| p.y < s.joint_y)
        .map(|(k, _)| k)
        .last()
        .unwrap_or(0);
    let Some(s) = parts.get(k) else {
        return (0, p);
    };
    let q = p - s.offset;
    (
        k,
        if s.turned {
            Vec3::new(-q.x, -q.y, q.z)
        } else {
            q
        },
    )
}

impl PeopleSim {
    /// What the people aboard `bus` weigh, by its sections (`sections`: how many, at least
    /// one; `parts` the cabin's, empty for a bus of one section).
    pub fn loads(
        &self,
        bus: BusId,
        parts: &[CabinPart],
        sections: usize,
        weight: PassengerWeight,
    ) -> SectionLoads {
        let mut out: SectionLoads = vec![Vec::new(); sections.max(1)];
        if !weight.on() {
            return out;
        }
        for p in &self.people {
            let Place::Bus(b, local) = p.place else {
                continue;
            };
            if b != bus {
                continue;
            }
            let kg = weight.of(p);
            if kg <= 0.0 {
                continue;
            }
            let (k, at) = section_point(parts, local);
            let k = k.min(out.len() - 1);
            out[k].push((kg, at + Vec3::Z * BODY_COG));
        }
        out
    }

    /// The player's bus (see [`PeopleSim::loads`]).
    pub fn player_loads(&self, sections: usize, weight: PassengerWeight) -> SectionLoads {
        let parts = self
            .player_cabin
            .as_ref()
            .map(|c| c.parts.clone())
            .unwrap_or_default();
        self.loads(BusId::Player, &parts, sections, weight)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_stays_in_range_and_is_steady() {
        for id in 0..5000 {
            let n = person_noise(id);
            assert!((-1.0..=1.0).contains(&n), "{id}: {n}");
            assert_eq!(n, person_noise(id));
        }
        // (and is not one value for all)
        assert!((0..100).map(person_noise).any(|n| n > 0.3));
        assert!((0..100).map(person_noise).any(|n| n < -0.3));
    }

    #[test]
    fn points_go_to_the_section_they_are_in() {
        let parts = [
            CabinPart {
                offset: Vec3::ZERO,
                joint_y: f32::INFINITY,
                turned: false,
            },
            CabinPart {
                offset: Vec3::new(0.0, -9.0, 0.0),
                joint_y: -6.0,
                turned: false,
            },
        ];
        assert_eq!(
            section_point(&parts, Vec3::new(0.5, 2.0, 0.1)),
            (0, Vec3::new(0.5, 2.0, 0.1))
        );
        assert_eq!(
            section_point(&parts, Vec3::new(0.5, -8.0, 0.1)),
            (1, Vec3::new(0.5, 1.0, 0.1))
        );
        // a bus of one section
        assert_eq!(section_point(&[], Vec3::new(1.0, -8.0, 0.0)).0, 0);
    }
}
