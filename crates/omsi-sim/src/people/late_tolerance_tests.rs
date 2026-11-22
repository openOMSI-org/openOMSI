use super::model::LATE_TOLERANCE;
use super::ticket_hook_tests::{boarding, human_type, player_bus};
use super::*;

fn pack() -> omsi_content::tickets::TicketPack {
    let t = |name: &str, value: f32| omsi_content::tickets::Ticket {
        name: name.into(),
        age_min: 14,
        age_max: 200,
        value,
        probability: 1.0,
        ..Default::default()
    };
    omsi_content::tickets::TicketPack {
        whinge_prop: 1.0,
        chattiness: 0.0,
        tickets: vec![t("Fahrschein", 2.7)],
        ..Default::default()
    }
}

fn stepping_in(tag: &str, delay: f64, tolerance: Option<f64>) -> (PeopleSim, BusNow) {
    let ty = human_type(tag);
    let bus = player_bus(tag);
    let mut h = PeopleSim::new(Path::new("/nonexistent"), 200);
    h.tickets = Some(Arc::new(pack()));
    h.delay = delay;
    if let Some(t) = tolerance {
        h.late_tolerance = t;
    }
    h.people = vec![boarding(&ty, 1, Task::WalkingToBus)];
    (h, bus)
}

#[test]
fn five_minutes_late_is_still_omis_complaint() {
    let (mut h, bus) = stepping_in("default-late", 320.0, None);
    assert_eq!(h.late_tolerance, LATE_TOLERANCE);
    h.greet_or_complain(0, &bus);
    let said = h.dialog.clone().unwrap_or_default();
    assert!(said.starts_with("TooLate_"), "said {said:?}");
    assert_eq!(h.content, 0);
}

#[test]
fn a_longer_tolerance_keeps_the_complaint_quiet() {
    let (mut h, bus) = stepping_in("tolerant", 360.0, Some(600.0));
    h.greet_or_complain(0, &bus);
    assert_eq!(h.dialog, None);
    assert_eq!(h.content, 1);

    let (mut h, bus) = stepping_in("tolerant-late", 660.0, Some(600.0));
    h.greet_or_complain(0, &bus);
    assert!(
        h.dialog.clone().unwrap_or_default().starts_with("TooLate_"),
        "{:?}",
        h.dialog
    );
}

#[test]
fn a_tolerance_of_zero_turns_the_complaint_off() {
    let (mut h, bus) = stepping_in("no-complaint", 3_600.0, Some(0.0));
    h.greet_or_complain(0, &bus);
    assert_eq!(h.dialog, None);
    assert_eq!(h.content, 1);
}
