//! The driver's cabs of a coupled train: which of its parts has a cab to drive from (the
//! NF6D's rear cab car, the other end of the tram), where its driver's seat is, which of the
//! train's driver's cameras is that cab's own, and what such a part's own scripts fired.

use super::VehicleInstance;
use glam::{DVec3, Vec3};
use std::sync::Arc;

/// A vehicle type's passenger cabin, read once per type.
pub fn cabin_of(def: &omsi_vehicle::Vehicle) -> Option<Arc<omsi_vehicle::PassengerCabin>> {
    static CABINS: std::sync::Mutex<
        Option<
            std::collections::HashMap<
                std::path::PathBuf,
                Option<Arc<omsi_vehicle::PassengerCabin>>,
            >,
        >,
    > = std::sync::Mutex::new(None);
    let mut cabins = CABINS.lock().unwrap_or_else(|e| e.into_inner());
    cabins
        .get_or_insert_with(Default::default)
        .entry(def.path.clone())
        .or_insert_with(|| {
            let rel = def.passenger_cabin.as_ref()?;
            omsi_vehicle::PassengerCabin::load(&omsi_cfg::resolve_path(def.dir(), rel))
                .map_err(|e| log::warn!("driver: {e}"))
                .ok()
                .map(Arc::new)
        })
        .clone()
}

/// `a` (degrees) brought to -180..180.
fn wrap_deg(a: f32) -> f32 {
    (a + 180.0).rem_euclid(360.0) - 180.0
}

/// The driver's seat of the cab of coupled part `k` of `main` (in that part's frame), when
/// it is a cab: the first `[drivpos]` of any part of the train - its own, the leading
/// vehicle's, another's - that lies by its standard driver's camera. The NF6D's rear cab car
/// (turned round) has no cabin of its own: its seat is the one of the forward-facing
/// stand-in for section B, in the same place. None for a part that is no cab (the NF6D's
/// joints and bogies have a driver's camera each, up on the roof).
pub fn part_cab_seat(main: &VehicleInstance, k: usize) -> Option<omsi_vehicle::cabin::PassPos> {
    let t = main.trailers.get(k)?;
    let def = &t.ty.def;
    let cams = &def.cameras_driver;
    let std = cams.get(def.camera_std.min(cams.len().checked_sub(1)?))?;
    let cam = Vec3::from(std.pos);
    // (the stand-in's seat faces into its section: turned to the cab's front)
    let facing = |rot: f32| {
        if wrap_deg(rot - std.yaw).abs() > 90.0 {
            (rot + 180.0).rem_euclid(360.0)
        } else {
            rot
        }
    };
    // A cab with the same driver's cameras as another vehicle of the train is that one's cab
    // (the NF6D's two ends): its seat, in its own frame.
    let same_cab = |d: &omsi_vehicle::Vehicle| {
        d.cameras_driver.len() == cams.len()
            && d.cameras_driver
                .iter()
                .zip(cams)
                .all(|(a, b)| (Vec3::from(a.pos) - Vec3::from(b.pos)).length() < 0.01)
    };
    // (by its camera all the same: the NF6D's joints share one roof camera)
    let by_cam = |p: [f32; 3]| {
        let p = Vec3::from(p);
        (p - cam).truncate().length() < 1.5 && (p.z - cam.z).abs() < 1.8
    };
    let twin = std::iter::once(&main.ty.def)
        .chain(
            main.trailers
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != k)
                .map(|(_, p)| &p.ty.def),
        )
        .filter(|d| same_cab(d))
        .find_map(|d| cabin_of(d)?.driver_positions.first().cloned())
        .filter(|s| by_cam(s.pos));
    if twin.is_some() {
        return twin;
    }
    let (rot_k, pos_k) = (t.body_rotation(), t.position);
    let to_local = rot_k.inverse();
    let lead = std::iter::once((
        &main.ty.def,
        main.body_rotation(),
        main.position,
        main.heading,
    ));
    let parts = main
        .trailers
        .iter()
        .map(|p| (&p.ty.def, p.body_rotation(), p.position, p.body_heading()));
    lead.chain(parts)
        .filter_map(|(d, rot, at, heading)| {
            let seat = cabin_of(d)?.driver_positions.first().cloned()?;
            let world = at + rot.transform_point3(Vec3::from(seat.pos)).as_dvec3();
            let local = to_local.transform_point3((world - pos_k).as_vec3());
            let face = (seat.rot as f64 + heading - t.body_heading()).rem_euclid(360.0) as f32;
            Some(omsi_vehicle::cabin::PassPos {
                pos: local.to_array(),
                rot: facing(face),
                ..seat
            })
        })
        .filter(|s| by_cam(s.pos))
        .min_by(|a, b| {
            (Vec3::from(a.pos) - cam)
                .length()
                .total_cmp(&(Vec3::from(b.pos) - cam).length())
        })
}

impl VehicleInstance {
    /// The driver's seat (the world, the hip) of coupled part `k` when it is a driver's cab
    /// (see [`part_cab_seat`]).
    pub fn cab_seat(&self, k: usize) -> Option<DVec3> {
        let seat = Vec3::from(part_cab_seat(self, k)?.pos);
        let t = self.trailers.get(k)?;
        Some(t.position + t.body_rotation().transform_point3(seat).as_dvec3())
    }

    /// The cab of a coupled part whose driver's seat is within `reach` of `pos` (plan view):
    /// the part's index and the seat, the nearest.
    pub fn cab_near(&self, pos: DVec3, reach: f64) -> Option<(usize, DVec3)> {
        (0..self.trailers.len())
            .filter_map(|k| Some((k, self.cab_seat(k)?)))
            .map(|(k, at)| (k, at, (at - pos).truncate().length()))
            .filter(|x| x.2 < reach)
            .min_by(|a, b| a.2.total_cmp(&b.2))
            .map(|(k, at, _)| (k, at))
    }

    /// The driving ends of the train: the leading vehicle (None) and every coupled part with
    /// a cab, in their order.
    pub fn driving_ends(&self) -> Vec<Option<usize>> {
        std::iter::once(None)
            .chain(
                (0..self.trailers.len())
                    .filter(|&k| self.cab_seat(k).is_some())
                    .map(Some),
            )
            .collect()
    }

    /// The coupled part whose cab the driver works the train from (`active_cab`, where that
    /// part is a cab); None in the leading vehicle.
    pub fn driving_cab(&self) -> Option<usize> {
        self.active_cab.filter(|&k| self.cab_seat(k).is_some())
    }

    /// The file name of coupled part `part` (None: the leading vehicle), without its extension.
    pub fn part_name(&self, part: Option<usize>) -> String {
        let file = part.map_or(&self.ty.def.path, |k| &self.trailers[k].ty.def.path);
        file.file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    /// The driving end after the one driven from (before it, `back`), and its number among
    /// the train's driving ends and how many there are (see [`Self::driving_ends`]).
    pub fn next_driving_end(&self, back: bool) -> (Option<usize>, usize, usize) {
        let ends = self.driving_ends();
        let n = ends.len();
        let at = ends.iter().position(|e| *e == self.active_cab).unwrap_or(0);
        let next = if back { (at + n - 1) % n } else { (at + 1) % n };
        (ends[next], next + 1, n)
    }

    /// What the outside camera orbits: the part whose cab the driver sits in (the NF6D's
    /// rear cab car, by its own `[set_camera_outside_center]`), else the vehicle - its
    /// position, the heading its model faces and the orbit's centre in its frame.
    pub fn orbit_target(&self) -> (DVec3, f64, [f32; 3]) {
        match self.driving_cab().and_then(|k| self.trailers.get(k)) {
            Some(t) => (t.position, t.body_heading(), t.ty.def.camera_outside_center),
            None => (
                self.position,
                self.heading,
                self.ty.def.camera_outside_center,
            ),
        }
    }

    /// For driver's camera `slot` of the whole train (the leading vehicle's first, then each
    /// part's in order): the coupled part it is on and its number among that part's. None
    /// for one of the leading vehicle's.
    pub fn driver_camera_part(&self, slot: usize) -> Option<(usize, usize)> {
        let mut k = slot.checked_sub(self.ty.def.cameras_driver.len())?;
        for (part, t) in self.trailers.iter().enumerate() {
            let own = t.ty.def.cameras_driver.len();
            if k < own {
                return Some((part, k));
            }
            k -= own;
        }
        None
    }

    /// The boxes of the vehicle and of the parts coupled behind it: what stands in a walker's
    /// way.
    pub fn train_boxes(&self) -> Vec<crate::collision::Obb> {
        use crate::collision::Obb;
        let own = self
            .ty
            .def
            .bounding_box
            .map(|bb| Obb::from_box(bb, self.position, self.heading));
        let parts = self.trailers.iter().filter_map(|t| {
            t.ty.def
                .bounding_box
                .map(|bb| Obb::from_box(bb, t.position, t.body_heading()))
        });
        own.into_iter().chain(parts).collect()
    }

    /// The driver's view moved to driver's camera `part` of the train (None: one of the
    /// leading vehicle's): the keys work the cab it is in. A coupled part's camera that is
    /// no cab (the NF6D's roof cameras over its sections) leaves the driver where he sat.
    pub fn follow_driver_camera(&mut self, part: Option<usize>) {
        match part {
            None => self.active_cab = None,
            Some(k) if self.cab_seat(k).is_some() => self.active_cab = Some(k),
            Some(_) => {}
        }
    }

    /// What the coupled parts fired this frame by their own scripts (they fire anew next
    /// frame): each part's sounds - None for a part on the leading vehicle's scripts, which
    /// hears its triggers - and its `(T.F.x)` files (the announcements of the NF6D rear cab's
    /// BKK FUTAR).
    #[allow(clippy::type_complexity)]
    pub fn take_part_fired(&mut self) -> (Vec<Option<Vec<String>>>, Vec<Vec<(String, String)>>) {
        let sounds = self
            .trailers
            .iter_mut()
            .map(|t| t.run.as_deref_mut().map(|r| r.take_fired_sounds()))
            .collect();
        let files: Vec<Vec<(String, String)>> = self
            .trailers
            .iter_mut()
            .map(|t| {
                t.run
                    .as_deref_mut()
                    .map(|r| r.take_fired_files())
                    .unwrap_or_default()
            })
            .collect();
        for (i, (t, f)) in files
            .iter()
            .enumerate()
            .flat_map(|(i, fs)| fs.iter().map(move |x| (i, x)))
        {
            log::info!("announcement (part {i}): {t} -> {f}");
        }
        (sounds, files)
    }

    /// Where the driver gets out of coupled part `k`'s cab: up from its seat (the world),
    /// and out beside it on the side `look` is to (plan view), past its half width.
    pub fn cab_exit(&self, k: usize, look: glam::DVec2) -> Option<(DVec3, glam::DVec2)> {
        let t = self.trailers.get(k)?;
        let hip = self.cab_seat(k)?;
        let hd = t.body_heading().to_radians();
        let right = glam::DVec2::new(hd.cos(), -hd.sin());
        let half =
            t.ty.def
                .bounding_box
                .map(|b| b[0] as f64 * 0.5)
                .unwrap_or(1.25);
        let side = if look.dot(right) < 0.0 { -1.0 } else { 1.0 };
        Some((
            hip - DVec3::new(0.0, 0.0, 0.45),
            hip.truncate() + right * side * (half + 0.6),
        ))
    }

    /// The standard driver's camera of coupled part `k`'s cab among all the train's driver's
    /// cameras (the leading vehicle's first, then each part's in order). None where it has
    /// no driver's camera.
    pub fn cab_camera_slot(&self, k: usize) -> Option<usize> {
        let t = self.trailers.get(k)?;
        let own = t.ty.def.cameras_driver.len();
        if own == 0 {
            return None;
        }
        let before: usize = self.trailers[..k]
            .iter()
            .map(|t| t.ty.def.cameras_driver.len())
            .sum();
        Some(self.ty.def.cameras_driver.len() + before + t.ty.def.camera_std.min(own - 1))
    }
}

#[cfg(test)]
mod nf6d_cab_tests {
    use std::sync::Arc;

    /// The NF6D's rear cab car (turned round, without a cabin of its own) is a cab, its seat
    /// the one of the stand-in for its section; the stand-in and the parts with a roof
    /// camera are none.
    #[test]
    #[ignore = "needs an OMSI 2 install with the NF6D add-on (OMSI_ROOT)"]
    fn the_rear_cab_car_is_a_cab_and_the_stand_in_is_not() {
        let root = std::path::PathBuf::from(std::env::var_os("OMSI_ROOT").expect("OMSI_ROOT"));
        let dir = root.join("Vehicles/Tram_NF6D");
        let load = |f: &str| Arc::new(crate::VehicleType::load(&root, &dir.join(f)).unwrap());
        let mut v = crate::VehicleInstance::new(
            load("NF6D_A.bus"),
            crate::VehicleHost::new(Default::default()),
        );
        let parts = [
            "NF6D_Gelenk_ACV.bus",
            "NF6D_Gelenk_ACH.bus",
            "NF6D_C.bus",
            "NF6D_Gelenk_BCV.bus",
            "NF6D_Gelenk_BCH.bus",
            "NF6D_B.bus",
            "NF6D_DrehgestellV.bus",
            "NF6D_EEF_V.bus",
            "NF6D_EEF_H.bus",
            "NF6D_DrehgestellH.bus",
        ];
        for p in parts {
            v.attach_trailer_ex(load(p), false);
        }
        v.attach_trailer_ex(load("NF6D_B_User.bus"), true);
        for _ in 0..10 {
            v.update(0.02);
        }
        let rear = v.trailers.len() - 1;
        let seat = super::part_cab_seat(&v, rear).expect("the rear cab car is a cab");
        // (facing its cab's front, by its camera, as a driver sits)
        let cam = v.trailers[rear].ty.def.cameras_driver[3].pos;
        assert!(
            (seat.pos[1] - cam[1]).abs() < 0.6,
            "{:?} by {:?}",
            seat.pos,
            cam
        );
        assert!(
            seat.rot.rem_euclid(360.0) < 30.0 || seat.rot.rem_euclid(360.0) > 330.0,
            "facing forward: {}",
            seat.rot
        );
        for k in 0..rear {
            assert!(
                super::part_cab_seat(&v, k).is_none(),
                "part {k} ({}) is no cab",
                parts[k]
            );
        }
        assert_eq!(v.driving_ends(), vec![None, Some(rear)]);
    }
}
