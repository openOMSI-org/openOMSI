//! What a rail vehicle's bogies tell its scripts of the track under them, each car its own:
//! how tight the curve is, how hard the wheels press on the rail (the squeal in bends), and what
//! its contact shoes touch (the third rail, which the NF6D's PZB also reads its codes from).

use super::{TrailerPart, VehicleInstance, body_reversed};
use omsi_vehicle::Vehicle;

/// How far a wheelset moves across the track before its flange meets the rail (m): Omsi.exe
/// 0x7de770.
const FLANGE_PLAY: f32 = 0.009;

/// The track under one bogie.
#[derive(Debug, Clone, Copy)]
pub struct BogieGround<'a> {
    /// 1/m, positive turning right as the car faces.
    pub curvature: f32,
    /// The `[third_rail]`s of the path, with their `x` as the path's travel has it.
    pub third_rails: &'a [[f32; 6]],
    /// 1 where the car faces the way the path runs, -1 against it.
    pub facing: f32,
}

impl<'a> BogieGround<'a> {
    /// `curvature`: as the path runs; `along`: the car faces the same way.
    pub fn new(curvature: f32, third_rails: &'a [[f32; 6]], along: bool) -> Self {
        let facing = if along { 1.0 } else { -1.0 };
        BogieGround {
            curvature: curvature * facing,
            third_rails,
            facing,
        }
    }
}

/// How far the wheelset would run off the middle of the track per 1/m of curve: what the cones
/// of the wheels need to roll a curve (Omsi.exe 0x7cc44e, wheel radius x half the span / taper).
fn cone_offset(def: &Vehicle) -> f32 {
    match def.sinus {
        Some([radius, span, taper, _]) if taper != 0.0 => radius * (span / 2.0) / taper,
        _ => 0.0,
    }
}

/// The variables of a car on `at` (its bogies 0 and 1), as Omsi.exe sets them: the curve at
/// 0x7ddc27, the wheels at 0x7ddbb0, the shoes at 0x7df500. The wheels are the wheelset's resting
/// place: Omsi.exe also rocks it on the track's faults, which does not change a curve.
fn track_vars(def: &Vehicle, at: &[BogieGround; 2], put: &mut dyn FnMut(&str, f32)) {
    let cone = cone_offset(def);
    for (k, ground) in at.iter().enumerate() {
        let (at_limit, invradius) = if k == 0 {
            ("boogie_0_wheel_at_limit", "boogie_0_invradius")
        } else {
            ("boogie_1_wheel_at_limit", "boogie_1_invradius")
        };
        put(invradius, ground.curvature);
        put(
            at_limit,
            (-cone * ground.curvature / FLANGE_PLAY).clamp(-1.0, 1.0),
        );
    }
    for (i, shoe) in def.contact_shoes.iter().enumerate() {
        let Some(ground) = at.get(shoe[0] as usize) else {
            continue;
        };
        let name = |what: &str| format!("contactshoe_{i}_{what}");
        put(&name("volt_rail"), 0.0);
        put(&name("volt_veh"), 0.0);
        put(&name("freq"), 0.0);
        put(&name("rail_pos_x"), 0.0);
        put(&name("rail_pos_z"), 0.0);
        put(&name("rail_index"), -1.0);
        let touches = |r: &[f32; 6]| {
            (shoe[1]..=shoe[2]).contains(&(ground.facing * r[0]))
                && (shoe[3]..=shoe[4]).contains(&r[1])
        };
        if let Some((index, rail)) = ground
            .third_rails
            .iter()
            .enumerate()
            .find(|(_, r)| touches(r))
        {
            put(&name("volt_rail"), rail[3]);
            put(&name("volt_veh"), rail[3]);
            put(&name("freq"), rail[4]);
            put(&name("rail_pos_x"), rail[0]);
            put(&name("rail_pos_z"), rail[1]);
            put(&name("rail_index"), index as f32);
        }
    }
}

impl VehicleInstance {
    /// Tell the leading car what its bogies stand on.
    pub fn feel_track(&mut self, at: &[BogieGround; 2]) {
        let ty = self.ty.clone();
        track_vars(&ty.def, at, &mut |name, v| {
            self.set_var(name, v);
        });
    }

    /// Tell every car of the train what its own bogies stand on: `center_y` and `l` are where
    /// the leading vehicle's are from its origin, and `ground(rel, turned)` is the track `rel`
    /// metres ahead of the origin (behind: negative) for a car turned round on it or not.
    pub fn feel_train_track<'a>(
        &mut self,
        center_y: f64,
        l: f64,
        ground: impl Fn(f64, bool) -> BogieGround<'a>,
    ) {
        let turned = self.ty.def.boogies.is_some_and(|b| b < 0.0);
        let y0 = if turned { -l } else { l };
        self.feel_track(&[ground(center_y + y0, turned), ground(center_y - y0, turned)]);
        for t in &mut self.trailers {
            if let Some([b0, b1]) = t.bogies_behind() {
                let turned = t.turned_on_track();
                t.feel_track(&[ground(-b0, turned), ground(-b1, turned)]);
            }
        }
    }
}

impl TrailerPart {
    /// How far behind the leading vehicle's origin its bogies 0 and 1 stand (m), when it runs
    /// on rails, and whether it has scripts to tell.
    pub fn bogies_behind(&self) -> Option<[f64; 2]> {
        self.rail_bogies.filter(|_| self.run.is_some())
    }

    /// The part stands turned round on the track.
    pub fn turned_on_track(&self) -> bool {
        body_reversed(&self.ty.def, self.reversed)
    }

    /// Tell the part what its own bogies stand on. They stay its own: the car in front of it
    /// may be out of the curve already.
    pub fn feel_track(&mut self, at: &[BogieGround; 2]) {
        let ty = self.ty.clone();
        let Some(run) = self.run.as_deref_mut() else {
            return;
        };
        track_vars(&ty.def, at, &mut |name, v| {
            run.set_own(name, v);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(def: &Vehicle, at: &[BogieGround; 2]) -> hashbrown::HashMap<String, f32> {
        let mut out = hashbrown::HashMap::new();
        track_vars(def, at, &mut |n, v| {
            out.insert(n.to_string(), v);
        });
        out
    }

    /// The NF6D's: `[sinus] 0.425 1.5 0.025 0.01`, `[contact_shoe] 0 -0.1 0.1 0.01 0.536 1`.
    fn nf6d() -> Vehicle {
        Vehicle {
            sinus: Some([0.425, 1.5, 0.025, 0.01]),
            contact_shoes: vec![[0.0, -0.1, 0.1, 0.01, 0.536, 1.0]],
            ..Default::default()
        }
    }

    /// Its `Meterspur_normal_9001_PZB_Tunnel_aktiv.sli`.
    const PZB: [f32; 6] = [0.0, 0.535, 3.0, 9001.0, 0.0, 0.051];

    #[test]
    fn a_shoe_reads_the_rail_it_touches() {
        let rails = [PZB];
        let v = vars(
            &nf6d(),
            &[
                BogieGround::new(0.0, &rails, true),
                BogieGround::new(0.0, &[], true),
            ],
        );
        assert_eq!(v["contactshoe_0_volt_rail"], 9001.0);
        assert_eq!(v["contactshoe_0_volt_veh"], 9001.0);
        assert_eq!(v["contactshoe_0_rail_index"], 0.0);
        assert_eq!(v["contactshoe_0_rail_pos_z"], 0.535);
    }

    #[test]
    fn a_shoe_that_leaves_the_rail_keeps_nothing_of_it() {
        let mut v = std::collections::HashMap::new();
        let rails = [[0.0, 0.535, 3.0, 750.0, 50.0, 0.05]];
        let on = [
            BogieGround::new(0.0, &rails, true),
            BogieGround::new(0.0, &[], true),
        ];
        let off = [
            BogieGround::new(0.0, &[], true),
            BogieGround::new(0.0, &[], true),
        ];
        track_vars(&nf6d(), &on, &mut |n, x| {
            v.insert(n.to_string(), x);
        });
        assert_eq!(
            (v["contactshoe_0_freq"], v["contactshoe_0_rail_pos_z"]),
            (50.0, 0.535)
        );
        track_vars(&nf6d(), &off, &mut |n, x| {
            v.insert(n.to_string(), x);
        });
        for what in ["volt_rail", "volt_veh", "freq", "rail_pos_x", "rail_pos_z"] {
            assert_eq!(v[&format!("contactshoe_0_{what}")], 0.0, "{what}");
        }
        assert_eq!(v["contactshoe_0_rail_index"], -1.0);
    }

    #[test]
    fn a_shoe_off_the_rail_reads_nothing() {
        let v = vars(
            &nf6d(),
            &[
                BogieGround::new(0.0, &[], true),
                BogieGround::new(0.0, &[PZB], true),
            ],
        );
        assert_eq!(
            v["contactshoe_0_volt_rail"], 0.0,
            "the rail is under the other bogie"
        );
        assert_eq!(v["contactshoe_0_rail_index"], -1.0);
        let high = [[0.0, 0.6, 3.0, 750.0, 0.0, 0.05]];
        let v = vars(
            &nf6d(),
            &[
                BogieGround::new(0.0, &high, true),
                BogieGround::new(0.0, &[], true),
            ],
        );
        assert_eq!(v["contactshoe_0_volt_rail"], 0.0, "above the shoe");
    }

    #[test]
    fn a_side_shoe_finds_its_rail_by_the_way_the_car_faces() {
        // (the A3's right shoe; the rail 1.12 m to the right of the path's way)
        let a3 = Vehicle {
            contact_shoes: vec![[0.0, 1.06, 1.18, 0.17, 0.19, 1.0]],
            ..Default::default()
        };
        let rails = [[1.12, 0.18, 3.0, 750.0, 0.0, 0.05]];
        let on = |along| {
            vars(
                &a3,
                &[
                    BogieGround::new(0.0, &rails, along),
                    BogieGround::new(0.0, &[], along),
                ],
            )["contactshoe_0_volt_rail"]
        };
        assert_eq!((on(true), on(false)), (750.0, 0.0));
    }

    #[test]
    fn the_wheels_meet_the_rail_in_a_bend_and_not_on_the_straight() {
        let def = nf6d();
        let on = |curvature: f32| vars(&def, &[BogieGround::new(curvature, &[], true); 2])["boogie_0_wheel_at_limit"];
        assert_eq!(on(0.0).abs(), 0.0);
        assert!(on(1.0 / 25.0).abs() > 0.95, "a street corner");
        assert!(
            on(1.0 / 5000.0).abs() < 0.95,
            "a wide bend rolls on the cones"
        );
    }

    #[test]
    fn each_bogie_has_its_own_curve_and_it_turns_with_the_way_driven() {
        let v = vars(
            &nf6d(),
            &[
                BogieGround::new(0.04, &[], false),
                BogieGround::new(0.0, &[], false),
            ],
        );
        assert_eq!(
            (v["boogie_0_invradius"], v["boogie_1_invradius"].abs()),
            (-0.04, 0.0)
        );
    }

    #[test]
    fn no_sinus_no_squeal() {
        let v = vars(&Vehicle::default(), &[BogieGround::new(0.04, &[], true); 2]);
        assert_eq!(v["boogie_0_wheel_at_limit"].abs(), 0.0);
    }
}
