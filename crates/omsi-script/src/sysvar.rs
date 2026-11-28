//! System variables (`L.S.` / `S.S.`), from `program/varlist_system.txt`.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum SysVar {
    Timegap,
    GetTime,
    NoSound,
    Pause,
    Time,
    Day,
    Month,
    Year,
    DayOfYear,
    MouseX,
    MouseY,
    PrecipType,
    PrecipRate,
    CollPosX,
    CollPosY,
    CollPosZ,
    CollEnergy,
    WeatherTemperature,
    WeatherAbsHum,
    WearLifespan,
    AutoClutch,
    SunAlt,
}

impl SysVar {
    pub const ALL: [SysVar; 22] = [
        SysVar::Timegap,
        SysVar::GetTime,
        SysVar::NoSound,
        SysVar::Pause,
        SysVar::Time,
        SysVar::Day,
        SysVar::Month,
        SysVar::Year,
        SysVar::DayOfYear,
        SysVar::MouseX,
        SysVar::MouseY,
        SysVar::PrecipType,
        SysVar::PrecipRate,
        SysVar::CollPosX,
        SysVar::CollPosY,
        SysVar::CollPosZ,
        SysVar::CollEnergy,
        SysVar::WeatherTemperature,
        SysVar::WeatherAbsHum,
        SysVar::WearLifespan,
        SysVar::AutoClutch,
        SysVar::SunAlt,
    ];

    pub fn name(self) -> &'static str {
        match self {
            SysVar::Timegap => "Timegap",
            SysVar::GetTime => "GetTime",
            SysVar::NoSound => "NoSound",
            SysVar::Pause => "Pause",
            SysVar::Time => "Time",
            SysVar::Day => "Day",
            SysVar::Month => "Month",
            SysVar::Year => "Year",
            SysVar::DayOfYear => "DayOfYear",
            SysVar::MouseX => "mouse_x",
            SysVar::MouseY => "mouse_y",
            SysVar::PrecipType => "PrecipType",
            SysVar::PrecipRate => "PrecipRate",
            SysVar::CollPosX => "coll_pos_x",
            SysVar::CollPosY => "coll_pos_y",
            SysVar::CollPosZ => "coll_pos_z",
            SysVar::CollEnergy => "coll_energy",
            SysVar::WeatherTemperature => "Weather_Temperature",
            SysVar::WeatherAbsHum => "Weather_AbsHum",
            SysVar::WearLifespan => "wearlifespan",
            SysVar::AutoClutch => "AutoClutch",
            SysVar::SunAlt => "SunAlt",
        }
    }

    pub fn from_name(s: &str) -> Option<SysVar> {
        SysVar::ALL
            .iter()
            .copied()
            .find(|v| v.name().eq_ignore_ascii_case(s))
    }
}
