//! Each car of a train rocks on its own springs (`rail::Body`): it leans out of a bend by the
//! force on it, and the rails under it tip it, each car at its own moment. Omsi.exe gives every
//! car this roll (0x7de53f: `wagenkasten_*`, from `[rail_body_osc]`).
//!
//! Only a car's body rocks. A train's file lists more parts than that, and they differ: the
//! bogies, couplings and wheel sets stay level on the rails, and the joints between two
//! cars (the NF6D's `Gelenk` parts, massless but as tall as the car) lean with the cars on
//! either side, or they would come apart from them at the bellows.

use super::TrailerPart;
use crate::rail::{Body, BodyParams, RailClass};

/// Under this a part is not a car's body (kg): the NF6D gives its joints and bogies no mass
/// at all, the Zweiachser its wheels and coupling a few kilogrammes.
const BODY_MASS: f32 = 1000.0;
/// A massless part reaching this high (m) is a joint in the car's outline; running gear
/// stays under it.
const JOINT_HEIGHT: f32 = 1.5;
/// Two parts whose origins are as near as this along the train (m) are one car in two files
/// (the NF6D's `B`, and `B_User` with the rear cab): they rock as one.
const SAME_CAR: f64 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SwayRole {
    Body,
    Joint,
    Gear,
}

fn role_of(t: &TrailerPart) -> SwayRole {
    if t.ty.def.mass_kg() >= BODY_MASS {
        SwayRole::Body
    } else if t.ty.model_box().is_some_and(|(_, hi)| hi.z >= JOINT_HEIGHT) {
        SwayRole::Joint
    } else {
        SwayRole::Gear
    }
}

#[derive(Debug, Clone)]
pub(super) struct CarSway {
    role: SwayRole,
    body: Body,
    params: Option<BodyParams>,
    last_heading: Option<f64>,
    /// The lean now (degrees).
    pub(super) roll: f32,
}

impl CarSway {
    fn new(t: &TrailerPart) -> CarSway {
        let role = role_of(t);
        let span = t.ty.def.boogies.map_or(4.0, |b| b.abs() * 2.0);
        let params =
            RailClass::of(&t.ty.def).map(|class| BodyParams::for_car(&t.ty.def, class, span));
        CarSway {
            role,
            body: Body::new(t.first_axle as u64 + 2),
            params,
            last_heading: None,
            roll: 0.0,
        }
    }
}

impl TrailerPart {
    /// What this part does when its train rocks (see the module).
    pub fn sway_role(&mut self) -> SwayRole {
        self.sway_state().role
    }

    fn sway_state(&mut self) -> &mut CarSway {
        if self.sway.is_none() {
            self.sway = Some(CarSway::new(self));
        }
        self.sway.as_mut().unwrap()
    }

    /// The lean of a car's body, for the parts around it.
    pub(super) fn body_roll(&mut self) -> Option<f32> {
        let s = self.sway_state();
        (s.role == SwayRole::Body).then_some(s.roll)
    }

    /// One step of the part's lean, at the heading it now has. `speed`: along the track.
    /// `twin`: the lean of the car this part is the other half of; `beside`: of the cars on
    /// either side of it.
    pub(super) fn sway_step(&mut self, dt: f32, speed: f32, twin: Option<f32>, beside: f32) -> f32 {
        let heading = self.heading;
        let s = self.sway_state();
        let last = s.last_heading.replace(heading);
        s.roll = match (s.role, twin, s.params) {
            (SwayRole::Gear, ..) => 0.0,
            (SwayRole::Joint, ..) => beside,
            (SwayRole::Body, Some(roll), _) => roll,
            (SwayRole::Body, None, None) => 0.0,
            (SwayRole::Body, None, Some(p)) if dt > 0.0 => {
                let turned = last.map_or(0.0, |h| {
                    ((heading - h + 180.0).rem_euclid(360.0) - 180.0).to_radians() / dt as f64
                });
                s.body
                    .step(&p, dt, speed, 0.0, (turned as f32).clamp(-1.0, 1.0))
                    .roll_deg
                    * crate::rail::ROLL_SHOWN
            }
            _ => s.roll,
        };
        s.roll
    }
}

/// Whether two origins along the train are one car.
pub(super) fn same_car(a: f64, b: f64) -> bool {
    (a - b).abs() < SAME_CAR
}
