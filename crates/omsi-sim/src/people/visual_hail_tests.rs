//! Regression coverage for optional visual-only hailing eligibility.
use super::*;

#[test]
fn request_only_for_close_approaching_buses() {
    let dir = DVec2::Y;
    assert!(PeopleSim::visual_hail_range(DVec3::new(3.0,-50.0,0.0),dir));
    assert!(!PeopleSim::visual_hail_range(DVec3::new(3.0,-100.0,0.0),dir));
    assert!(!PeopleSim::visual_hail_range(DVec3::new(35.0,-20.0,0.0),dir));
    assert!(!PeopleSim::visual_hail_range(DVec3::new(0.0,30.0,0.0),dir));
    assert!(!PeopleSim::visual_hail_range(DVec3::new(0.0,-20.0,12.0),dir));
    assert!(!PeopleSim::visual_hail_range(DVec3::new(f64::NAN,0.0,0.0),dir));
}
