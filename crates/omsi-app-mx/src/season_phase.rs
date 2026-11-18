//! The season and its phase in the game: `--season` setting the session's date and the
//! launcher's choice read as the game reads it. The season itself is
//! `omsi_weather::season_phase`.

use super::*;
pub(crate) use omsi_weather::season_phase::*;

/// A season chosen with `--season` (and its phase) sets the session's date: the year and
/// the time of day stay the player's (see [`phase_date`]); a saved situation keeps its own
/// date. The season is written back as [`SeasonChoice::word`] (what LAN players are told).
pub fn apply_season_date(args: &mut Args) {
    let Some(choice) = args.season.as_deref().and_then(SeasonChoice::parse) else { return };
    args.season = Some(choice.word());
    if args.situation.is_some() {
        return;
    }
    let lat = map_latitude(&args.root, &args.map);
    let c = start_clock(args);
    let code = c.date_code();
    let now = (code / 10000, (code / 100 % 100) as u32, (code % 100) as u32);
    let (y, m, d) = phase_date(choice, now, lat);
    if (y, m, d) != now {
        args.date = Some(format!("{y:04}-{m:02}-{d:02}"));
        args.day_of_year = None;
    }
    log::info!(
        "season {}: date {y:04}-{m:02}-{d:02} ({} hemisphere, latitude {lat:.1}), texture season kind {}, plants mixed {:?}",
        choice.word(),
        if lat < 0.0 { "southern" } else { "northern" },
        choice.kind(),
        choice.mix().map(|m| (m.from, m.to, m.share))
    );
}

