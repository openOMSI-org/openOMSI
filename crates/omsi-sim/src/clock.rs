//! Simulation time: date, time of day and the per-frame `Timegap`.

#[derive(Debug, Clone)]
pub struct SimClock {
    pub year: i32,
    pub day_of_year: i32,
    /// Seconds since midnight.
    pub time: f64,
    /// Last frame duration in seconds.
    pub timegap: f32,
    pub paused: bool,
    /// Seconds of play since the clock started - the scripts' `GetTime` (OMSI keeps a
    /// millisecond counter of frame times, `g_859ea8`, and never wraps it at midnight: the
    /// gearbox scripts compare timestamps of it).
    pub run_time: f64,
}

impl Default for SimClock {
    fn default() -> Self {
        Self {
            year: 1989,
            day_of_year: 150,
            time: 9.0 * 3600.0,
            timegap: 1.0 / 60.0,
            paused: false,
            run_time: 0.0,
        }
    }
}

/// A time of day as the command line and the files write it (`hh[:mm[:ss]]`) in seconds
/// since midnight; a missing hour is 9, missing minutes and seconds 0.
pub fn parse_day_time(s: &str) -> f64 {
    let mut it = s.split(':');
    let h: f64 = it.next().and_then(|x| x.trim().parse().ok()).unwrap_or(9.0);
    let m: f64 = it.next().and_then(|x| x.trim().parse().ok()).unwrap_or(0.0);
    let sec: f64 = it.next().and_then(|x| x.trim().parse().ok()).unwrap_or(0.0);
    h * 3600.0 + m * 60.0 + sec
}

impl SimClock {
    /// The clock a game starts on: `date` (`yyyy-mm-dd`), else the default's; `day_of_year`
    /// over it when given; the time of day `time` (see [`parse_day_time`]).
    pub fn starting(date: Option<&str>, day_of_year: Option<i32>, time: &str) -> SimClock {
        let mut c = SimClock::default();
        if let Some(d) = date {
            let v: Vec<i32> = d.split('-').filter_map(|x| x.trim().parse().ok()).collect();
            if v.len() == 3 {
                c.set_date(v[0], v[1], v[2]);
            }
        }
        if let Some(doy) = day_of_year {
            c.day_of_year = doy.max(1);
        }
        c.time = parse_day_time(time);
        c
    }

    pub fn advance(&mut self, dt: f32) {
        self.timegap = dt;
        if !self.paused {
            self.run_time += dt as f64;
            self.time += dt as f64;
            while self.time >= 86400.0 {
                self.time -= 86400.0;
                self.next_day();
            }
        }
    }

    /// The calendar a day on (into the next year after its last day).
    pub fn next_day(&mut self) {
        self.day_of_year += 1;
        if self.day_of_year > days_in_year(self.year) {
            self.day_of_year = 1;
            self.year += 1;
        }
    }

    /// The calendar a day back (into the year before from its first day).
    pub fn prev_day(&mut self) {
        self.day_of_year -= 1;
        if self.day_of_year < 1 {
            self.year -= 1;
            self.day_of_year = days_in_year(self.year);
        }
    }

    /// Seconds from now to the same time of the same day `months` months on (or back), the
    /// day kept within the month it lands in (31 January + 1 month: 28 or 29 February).
    pub fn months_away(&self, months: i32) -> f64 {
        let (d, m) = self.day_month();
        let total = self.year * 12 + (m - 1) + months;
        let (y, m) = (total.div_euclid(12), total.rem_euclid(12) + 1);
        let mut there = self.clone();
        there.set_date(y, m, d);
        let days = |c: &SimClock| -> i64 {
            (1..c.year).map(|y| days_in_year(y) as i64).sum::<i64>() + c.day_of_year as i64
        };
        (days(&there) - days(self)) as f64 * 86400.0
    }

    pub fn hour(&self) -> f32 {
        (self.time / 3600.0) as f32
    }

    /// (day, month) of the current day of year.
    pub fn day_month(&self) -> (i32, i32) {
        let leap = days_in_year(self.year) == 366;
        let months = [
            31,
            if leap { 29 } else { 28 },
            31,
            30,
            31,
            30,
            31,
            31,
            30,
            31,
            30,
            31,
        ];
        let mut d = self.day_of_year.max(1);
        for (i, m) in months.iter().enumerate() {
            if d <= *m {
                return (d, i as i32 + 1);
            }
            d -= m;
        }
        (31, 12)
    }

    /// Day of the week, 0 = Monday … 6 = Sunday.
    pub fn weekday(&self) -> i32 {
        // days since 1970-01-01 (a Thursday)
        let mut days: i64 = 0;
        if self.year >= 1970 {
            for y in 1970..self.year {
                days += days_in_year(y) as i64;
            }
        } else {
            for y in self.year..1970 {
                days -= days_in_year(y) as i64;
            }
        }
        days += (self.day_of_year - 1) as i64;
        ((days + 3).rem_euclid(7)) as i32
    }

    /// Set the date from year, month, day.
    pub fn set_date(&mut self, year: i32, month: i32, day: i32) {
        // (a date from a file or another game may be anything: the day is kept within its
        // month and the year within reason, so the sums below cannot overflow)
        let year = year.clamp(1, 9999);
        let leap = days_in_year(year) == 366;
        let months = [
            31,
            if leap { 29 } else { 28 },
            31,
            30,
            31,
            30,
            31,
            31,
            30,
            31,
            30,
            31,
        ];
        let month = month.clamp(1, 12) as usize;
        let mut doy = day.clamp(1, months[month - 1]);
        for m in months.iter().take(month - 1) {
            doy += m;
        }
        self.year = year;
        self.day_of_year = doy;
    }

    /// `YYYYMMDD` as used by calendars and chrono events.
    pub fn date_code(&self) -> i32 {
        let (d, m) = self.day_month();
        self.year * 10000 + m * 100 + d
    }
}

pub fn days_in_year(y: i32) -> i32 {
    if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 {
        366
    } else {
        365
    }
}

#[cfg(test)]
mod date_tests {
    use super::*;

    #[test]
    fn dates_from_elsewhere_stay_in_range() {
        let mut c = SimClock::default();
        c.set_date(2024, 3, 1);
        assert_eq!((c.year, c.day_of_year), (2024, 61));
        c.set_date(2023, 2, 31);
        assert_eq!(c.day_of_year, 59, "the day kept within its month");
        c.set_date(i32::MAX, i32::MAX, i32::MAX);
        assert!(c.day_of_year <= 366 && c.year <= 9999);
    }
    /// The days and months the time menu steps the date by: over the end of a year into
    /// the next one (it used to start the same year again), and a month to the same day.
    #[test]
    fn days_and_months_step_across_the_year() {
        let mut c = SimClock::default();
        c.set_date(2023, 12, 31);
        c.next_day();
        assert_eq!((c.year, c.day_month()), (2024, (1, 1)));
        c.prev_day();
        assert_eq!((c.year, c.day_month()), (2023, (31, 12)));
        assert_eq!(c.months_away(1), 31.0 * 86400.0);
        assert_eq!(c.months_away(-1), -31.0 * 86400.0, "to 30 November");
        c.set_date(2024, 1, 31);
        assert_eq!(c.months_away(1), 29.0 * 86400.0, "to 29 February");
        c.set_date(2024, 3, 15);
        assert_eq!(c.months_away(-12), -366.0 * 86400.0);
    }
}
