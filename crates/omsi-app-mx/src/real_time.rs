//! Real-time sync in the game: the arguments that start a session at this device's date
//! and time. The clock itself is `omsi_career::real_time`.

pub(crate) use omsi_career::real_time::*;

/// Make the arguments start the game at this device's date and time.
pub(crate) fn start_at_now(args: &mut crate::cli::Args) {
    let Some(n) = now() else { return };
    args.date = Some(format!("{:04}-{:02}-{:02}", n.year, n.month, n.day));
    args.day_of_year = None;
    let s = n.secs as u64;
    args.time = format!("{:02}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60);
    args.start_synced = true;
}
