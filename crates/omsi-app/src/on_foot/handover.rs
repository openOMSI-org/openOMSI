#[cfg(test)]
mod tests {
    use glam::DVec3;
    #[test]
    fn a_real_other_vehicle_blocks_the_exit() {
        let mut v = crate::schedule::tests::script_test_vehicle("", "", "");
        std::sync::Arc::make_mut(&mut v.ty).def.bounding_box = Some([2.5, 12.0, 3.0, 0.0, 0.0, 1.5]);
        let door = DVec3::new(1.8, 5.0, 0.0);
        let blocker = omsi_sim::collision::Obb::from_box([2.5, 12.0, 3.0, 0.0, 0.0, 1.5], DVec3::new(3.0, 0.0, 0.0), 0.0);
        assert!(super::super::outside_at(None, &v, &[blocker], door).is_none());
        assert!(super::super::outside_at(None, &v, &[], door).is_some());
    }
}
