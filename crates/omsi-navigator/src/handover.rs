//! Mark physical handed-over buses even without timetable or destination script variables.

use super::*;

pub fn handed_over_camera_pose(position: DVec3, heading: f64) -> Option<(DVec3, f32)> {
    if !position.is_finite() || !heading.is_finite() {
        return None;
    }
    let radians = heading.to_radians();
    Some((
        position + DVec3::new(-radians.sin() * 25.0, -radians.cos() * 25.0, 30.0),
        heading as f32,
    ))
}

impl Navigator {
    pub fn set_handed_over_buses(&mut self, ids: impl IntoIterator<Item = u64>) {
        self.handed_over_buses.clear();
        self.handed_over_buses.extend(ids);
    }

    fn handed_over_label(&self, id: u64) -> Option<String> {
        self.handed_over_buses
            .contains(&id)
            .then(|| omsi_ui::i18n::tr_fill("Handed-over bus #{id}", &[("{id}", &id.to_string())]))
    }

    fn visible_traffic(&self, id: u64, released: bool) -> bool {
        self.handed_over_buses.contains(&id) || (self.show_ai && !released)
    }

    pub(super) fn map_traffic_kind(
        &self,
        car: &omsi_sim::ai_traffic::model::AiCar,
    ) -> Option<(Color, Option<String>)> {
        if !self.visible_traffic(car.id, car.gone) {
            return None;
        }
        if let Some(label) = self.handed_over_label(car.id) {
            let (_, line) = traffic_kind(car);
            Some((BUS, line.or(Some(label))))
        } else {
            self.show_ai.then(|| traffic_kind(car))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn locator_camera_stays_behind_and_above_the_bus() {
        let p = DVec3::new(100.0, 200.0, 4.0);
        let (camera, yaw) = handed_over_camera_pose(p, 90.0).unwrap();
        assert!((camera - DVec3::new(75.0, 200.0, 34.0)).length() < 1e-6);
        assert_eq!(yaw, 90.0);
        assert!(handed_over_camera_pose(p, f64::NAN).is_none());
        assert!(handed_over_camera_pose(DVec3::NAN, 0.0).is_none());
    }

    #[test]
    fn handed_over_markers_do_not_depend_on_ai_layer_or_timetable() {
        let mut nav = Navigator::new(true, 0.85, "bottom-left");
        nav.show_ai = false;
        nav.set_handed_over_buses([7, 9]);
        assert!(
            nav.visible_traffic(7, true),
            "released buses remain physical until removed"
        );
        assert!(nav.visible_traffic(7, false));
        assert!(!nav.visible_traffic(8, false));
        assert!(!nav.visible_traffic(8, true));
        assert!(nav.handed_over_label(7).is_some());
        assert!(nav.handed_over_label(9).is_some());
        assert!(nav.handed_over_label(8).is_none());
        nav.set_handed_over_buses([9]);
        assert!(nav.handed_over_label(7).is_none());
        nav.set_handed_over_buses([]);
        assert!(nav.handed_over_label(9).is_none());
    }
    #[test]
    fn actual_unscheduled_released_bus_keeps_a_marker_until_it_is_removed() {
        use omsi_sim::ai_traffic::setup::RandomTypes;
        use omsi_sim::traffic::LaneBuilder;
        let ty = std::sync::Arc::new(omsi_sim::VehicleType {
            def: Default::default(),
            model: Default::default(),
            model_dir: Default::default(),
            program: Default::default(),
            meshes: vec![],
            lod_meshes: vec![],
            paint_schemes: vec![],
            texchanges: vec![],
            wheel_meshes: vec![],
            suspension_axles: vec![],
            missing_packs: vec![],
            mesh_bounds: vec![],
            mesh_boxes: vec![],
        });
        let vehicle =
            omsi_sim::VehicleInstance::new(ty, omsi_sim::VehicleHost::new(Default::default()));
        let lane = LaneBuilder::arc(DVec3::ZERO, 0.0, 1500.0, 0.0, 0.0, LaneKind::Street, 3.0);
        let mut net = Network {
            lanes: vec![lane],
            ..Default::default()
        };
        net.build_grid();
        let random = RandomTypes {
            types: vec![],
            groups: vec![],
            group_curves: false,
            group_uvg: vec![],
            uvg_defaults: vec![],
        };
        let mut traffic = TrafficSim::assemble(
            std::path::Path::new(""),
            net,
            random,
            vec![],
            HashMap::new(),
            (vec![], vec![]),
            vec![],
            (1.0, 0),
            0,
        );
        let id = traffic.adopt_vehicle(vehicle, 0, 0.0, None);
        traffic.release(0);
        let mut nav = Navigator::new(true, 0.85, "bottom-left");
        nav.show_ai = false;
        nav.set_handed_over_buses([id]);
        let car = &traffic.cars[0];
        assert!(car.gone && car.bus.is_none());
        assert_eq!(traffic_kind(car).0, DOT);
        let marker = nav
            .map_traffic_kind(car)
            .expect("a physical bus, even without a line or timetable");
        assert_eq!(marker.0, BUS);
        assert!(marker.1.is_some());
        nav.set_handed_over_buses([]);
        assert!(nav.map_traffic_kind(car).is_none());
    }
    #[test]
    fn handed_over_marker_label_fits_without_truncation() {
        let fonts = Fonts::new();
        let label = "Handed-over bus #123";
        let fitted = fonts.fit(label, 11.5, Weight::Bold, TRANSIT_LABEL_MAX);
        assert_eq!(fitted, label);
    }
}
