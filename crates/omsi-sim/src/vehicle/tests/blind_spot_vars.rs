use super::*;

/// A bus whose `[varlist]` is `varlist` and whose script does nothing.
fn bus(varlist: &str) -> VehicleInstance {
    crate::timetable_run::tests::script_test_vehicle("{init}\n{end}\n", varlist, "")
}

#[test]
fn the_traffic_writes_the_blind_spot_into_the_script_variables() {
    let mut b =
        bus("BSM_Left\nBSM_Left_Dist\nBSM_Right\nBSM_Right_Dist\nBSM_Rear\nBSM_Rear_Dist\n");
    let spot = crate::ai_traffic::blind_spot::BlindSpot {
        left: None,
        right: Some(2.5),
        rear: Some(11.0),
    };
    assert_eq!(b.apply_blind_spot(spot), 6, "the bus declares all six");
    assert_eq!(b.var("BSM_Left"), Some(0.0));
    assert_eq!(b.var("BSM_Right"), Some(1.0));
    assert_eq!(b.var("BSM_Right_Dist"), Some(2.5));
    assert_eq!(b.var("BSM_Rear"), Some(1.0));
    assert_eq!(b.var("BSM_Rear_Dist"), Some(11.0));
    // and once the traffic has passed, the variables say so again
    let clear = b.apply_blind_spot(crate::ai_traffic::blind_spot::BlindSpot::default());
    assert_eq!(
        (clear, b.var("BSM_Right"), b.var("BSM_Right_Dist")),
        (6, Some(0.0), Some(0.0))
    );
}

#[test]
fn a_bus_that_declares_none_of_the_variables_is_untouched() {
    // an OMSI 2 add-on written before this existed: nothing to set, nothing added
    let mut b = bus("elec_busbar_main\ndoorfront\n");
    let before = b.var_index().len();
    let spot = crate::ai_traffic::blind_spot::BlindSpot {
        left: Some(1.0),
        right: Some(1.0),
        rear: Some(1.0),
    };
    assert_eq!(b.apply_blind_spot(spot), 0);
    assert_eq!(
        b.var_index().len(),
        before,
        "no variable was added to the bus"
    );
    assert_eq!(b.var("BSM_Left"), None);
    assert_eq!(
        b.var("elec_busbar_main"),
        Some(0.0),
        "its own variables are as they were"
    );
}

#[test]
fn a_bus_may_declare_only_some_of_them() {
    // a dashboard lamp for the right side only
    let mut b = bus("BSM_Right\n");
    let spot = crate::ai_traffic::blind_spot::BlindSpot {
        left: Some(1.0),
        right: Some(0.5),
        rear: None,
    };
    assert_eq!(b.apply_blind_spot(spot), 1);
    assert_eq!(b.var("BSM_Right"), Some(1.0));
    assert_eq!(b.var("BSM_Left"), None);
}
