//! The Drive page's season and its phase: what the choice does to the date.
use super::state::State;
use omsi_weather::season_phase::{SeasonChoice, map_lat_deg, phase_date};

impl State {
    /// The season choice changed to `season` ("auto" = by date): a season moves the date
    /// into its phase, and "By date" gives back the date the player had before.
    pub fn set_season(&mut self, season: &str) {
        let was_auto = self.choice.season == "auto";
        self.choice.season = season.to_string();
        if season == "auto" {
            if let Some(d) = self.choice.own_date.take() {
                self.choice.date = d;
                self.load_lines();
            }
            return;
        }
        if was_auto {
            self.choice.own_date = Some(self.choice.date.clone());
        }
        self.season_chosen();
    }

    /// A season and its phase chosen: the date moves to the phase's typical day of the
    /// chosen year (half a year later on a map south of the equator), see `season_phase`.
    pub fn season_chosen(&mut self) {
        let c = &self.choice;
        if let Some(d) = launcher_date(&c.season, &c.phase, &c.date, &c.map, false) {
            self.choice.date = d;
            self.load_lines();
        }
    }
}

/// The launcher's season and phase (`autumn`, `late`) as the game reads them.
pub(crate) fn launcher_choice(season: &str, phase: &str) -> Option<SeasonChoice> {
    SeasonChoice::parse(&format!("{season}-{phase}"))
}

/// The launcher's date (`YYYY-MM-DD`) for its season and phase on `map`: the phase's
/// typical day of its year, or with `keep_in_phase` the date itself when it lies in the
/// phase already (see [`phase_date`]). None with no season chosen.
pub(crate) fn launcher_date(
    season: &str,
    phase: &str,
    date: &str,
    map: &str,
    keep_in_phase: bool,
) -> Option<String> {
    let choice = launcher_choice(season, phase)?;
    let num = |r: std::ops::Range<usize>| date.get(r).and_then(|x| x.parse::<u32>().ok());
    let lat = map_lat_deg(
        std::path::Path::new(&omsi_launcher_lib::load_config().root),
        map,
    );
    let year = num(0..4).unwrap_or(1989) as i32;
    let (y, m, d) = match (num(5..7), num(8..10)) {
        (Some(m), Some(d)) if keep_in_phase => phase_date(choice, (year, m, d), lat),
        _ => {
            let (m, d) = choice.day(lat);
            (year, m, d)
        }
    };
    Some(format!("{y:04}-{m:02}-{d:02}"))
}
