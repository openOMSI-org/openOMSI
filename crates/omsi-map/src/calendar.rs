//! `Holidays.txt` and `timezone.txt` (`TMap.loadGlobalFile` - "Load Calendar").

use omsi_cfg::CfgFile;
use std::path::Path;

/// The day as the in-use rules ask about it.
#[derive(Clone, Copy, Debug, Default)]
pub struct DayKind {
    pub workday: bool,
    pub holiday: bool,
    pub school_holiday: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct HolidayRange {
    /// YYYYMMDD
    pub start: i32,
    pub end: i32,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Holiday {
    pub date: i32,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Dst {
    pub start: i32,
    pub end: i32,
    /// The hour summer time starts on `start` and ends on `end`, and the hours the clocks
    /// go forward (OMSI keeps them as floats).
    pub params: [f32; 3],
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TimeZone {
    pub offset_hours: f32,
    pub dst: Vec<Dst>,
    pub location: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Calendar {
    pub ranges: Vec<HolidayRange>,
    pub holidays: Vec<Holiday>,
}

impl Calendar {
    pub fn load(path: &Path) -> Result<Calendar, omsi_cfg::CfgError> {
        let f = CfgFile::read(path)?;
        Ok(Self::parse(&f))
    }
    pub fn parse(f: &CfgFile) -> Calendar {
        let mut c = Calendar::default();
        let mut r = f.reader();
        while let Some(k) = r.next_keyword() {
            match k.as_str() {
                "holidays" => {
                    let start = r.i32();
                    let end = r.i32();
                    let name = r.str().to_string();
                    c.ranges.push(HolidayRange { start, end, name });
                }
                "holiday" => {
                    let date = r.i32();
                    let name = r.str().to_string();
                    c.holidays.push(Holiday { date, name });
                }
                _ => {}
            }
        }
        c
    }
    /// Is `yyyymmdd` a holiday (single day or within a school-holiday range)?
    pub fn is_holiday(&self, date: i32) -> bool {
        self.holidays.iter().any(|h| h.date == date)
    }
    pub fn in_holiday_range(&self, date: i32) -> bool {
        self.ranges.iter().any(|h| date >= h.start && date <= h.end)
    }
}

impl TimeZone {
    /// Latitude and longitude from `[location]` (north and east positive).
    pub fn lat_lon(&self) -> Option<(f64, f64)> {
        let lat = self.location.first().map(|s| omsi_cfg::parse_f64(s))?;
        let lon = self.location.get(1).map(|s| omsi_cfg::parse_f64(s))?;
        Some((lat, lon))
    }

    pub fn load(path: &Path) -> Result<TimeZone, omsi_cfg::CfgError> {
        let f = CfgFile::read(path)?;
        Ok(Self::parse(&f))
    }
    pub fn parse(f: &CfgFile) -> TimeZone {
        let mut t = TimeZone::default();
        let mut r = f.reader();
        while let Some(k) = r.next_keyword() {
            match k.as_str() {
                "timezone" => t.offset_hours = r.f32(),
                "dst" => {
                    let start = r.i32();
                    let end = r.i32();
                    let params = [r.f32(), r.f32(), r.f32()];
                    t.dst.push(Dst { start, end, params });
                }
                "location" => {
                    t.location = r
                        .rest_of_block()
                        .into_iter()
                        .map(|s| s.to_string())
                        .collect()
                }
                _ => {}
            }
        }
        t
    }
}
