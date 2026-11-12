/// What plays for one of the overhead line's sounds: the game's own, nothing, or a sound file
/// beside `trolley.txt`.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum SoundChoice {
    #[default]
    Builtin,
    Off,
    File(String),
}

impl SoundChoice {
    pub fn parse(v: &[&str]) -> SoundChoice {
        match v.first().map(|s| s.to_ascii_lowercase()).as_deref() {
            None | Some("on" | "1" | "yes" | "true" | "builtin" | "default") => SoundChoice::Builtin,
            Some("off" | "0" | "no" | "false" | "none") => SoundChoice::Off,
            Some(_) => SoundChoice::File(v.join(" ")),
        }
    }
}

/// The overhead line's contact, crossing and switch sounds. Vehicle equipment uses sound.cfg.
#[derive(Debug, Clone, PartialEq)]
pub struct WireSounds {
    pub frog: SoundChoice,
    pub switch: SoundChoice,
}

impl Default for WireSounds {
    fn default() -> Self {
        Self { frog: SoundChoice::Builtin, switch: SoundChoice::Off }
    }
}