//! The map's timetable for the game menu's lines and tours: the running one, or the book read
//! for a session without timetable traffic (`SessionState::timetable_book`).

use crate::App;

/// The map's timetable as the menu reads it: the running one, else the book of a session
/// without timetable traffic (`SessionState::timetable_book`).
pub(crate) fn timetable(app: &App) -> Option<&crate::schedule::Schedule> {
    app.session.schedule.as_ref().or(app.session.timetable_book.as_ref())
}

/// A duty picked from the timetable book: from now on the timetable runs for the player's
/// duty alone - no timetable buses, as the launcher's "Disable timetable buses" with a line
/// chosen.
pub(crate) fn timetable_for_duty(app: &mut App) {
    if app.session.schedule.is_some() {
        return;
    }
    if let Some(book) = app.session.timetable_book.take() {
        if let Some(t) = app.session.traffic.as_mut() {
            t.no_timetable_buses = true;
        }
        app.session.schedule = Some(book);
    }
}
