//! Timetable state retained with a parked vehicle across ownership changes.

#[derive(Default)]
pub struct SavedDuty {
    pub duty: Option<crate::timetable_run::PlayerDuty>,
    pub reservation: Option<crate::timetable_run::DutyReservation>,
}
