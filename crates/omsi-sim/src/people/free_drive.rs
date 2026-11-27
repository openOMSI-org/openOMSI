//! #2712: whether the stops of the map fill in free drive, where nobody is working the
//! timetable. OMSI's passengers turned up at stops whatever the player was driving; openOMSI
//! filled a stop only where the timetable had a trip due there, so a driver out for a spin
//! found the whole map empty.

/// What the settings say about it (`pax_free_drive`): `auto` - fill the stops by their own
/// numbers when the player has no duty (free drive), or when their duty's timetable has no
/// trip due anywhere (#3841), and by the timetable's trips otherwise; `on` - fill them by
/// their own numbers always; `off` - never mind free drive, keep to the timetable's trips.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FreeDriveFill {
    #[default]
    Auto,
    On,
    Off,
}

impl FreeDriveFill {
    /// The settings' spellings; what is not one of them is `auto`, the way OMSI's own
    /// passengers behaved.
    pub fn from_setting(s: &str) -> FreeDriveFill {
        match s.trim().to_ascii_lowercase().as_str() {
            "on" | "1" | "yes" | "true" | "always" => FreeDriveFill::On,
            "off" | "0" | "no" | "false" | "never" => FreeDriveFill::Off,
            _ => FreeDriveFill::Auto,
        }
    }

    /// The settings' own spelling of it.
    pub fn setting_name(&self) -> &'static str {
        match self {
            FreeDriveFill::Auto => "auto",
            FreeDriveFill::On => "on",
            FreeDriveFill::Off => "off",
        }
    }

    /// Whether the stops fill by their own numbers now: `has_duty` is whether the player is
    /// working a duty of the timetable (not free drive).
    pub fn fills(&self, has_duty: bool) -> bool {
        match self {
            FreeDriveFill::Auto => !has_duty,
            FreeDriveFill::On => true,
            FreeDriveFill::Off => false,
        }
    }
}

/// The share of a stop's own numbers that is filled: none where the map has a timetable and
/// the player keeps to it and no trip of it is due there (or no line is listed at the stop);
/// all of it in free drive (#2712).
pub fn stop_served(timetabled: bool, free_drive: bool, has_lines: bool, trip_due: bool) -> f32 {
    if timetabled && !free_drive && (!has_lines || !trip_due) {
        0.0
    } else {
        1.0
    }
}
