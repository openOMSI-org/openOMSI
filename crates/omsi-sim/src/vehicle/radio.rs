//! The bus radio's station and song on the vehicle's text displays.

use super::*;

impl VehicleInstance {
    /// The station and the song on a radio whose display is a text of its script. OMSI has
    /// no radio of its own: these radios show names from a list in the script, and a radio
    /// plugin writes what it really plays into a string of theirs. Two kinds are known:
    ///
    /// - a script that reads `Snd_Radio_Text` (the plugin's variable) and puts it behind
    ///   its frequency: the text goes there;
    /// - Dmitrij's "Magnitola" (the radio of P3ta's SOR buses and others): the playlist
    ///   writes `frequency@station` into `mp3_display_track_name` every frame and the
    ///   display `magnitola_1` shows it - `@` is the line break, ten characters a line.
    ///   While the display shows that, its second line is replaced.
    ///
    /// The frequency in front is the script's too, one of its list. Where the station's
    /// own is known (`radio_frequency`: a map says which frequency its stations are on,
    /// and where) that one stands there instead.
    pub(super) fn show_radio_text(&mut self) {
        let Some(text) = self.radio_text.as_ref() else {
            return;
        };
        let frequency = self.radio_frequency.as_deref();
        let p = *self.program_ids();
        if let Some(i) = p.snd_radio_text {
            if self.state.str_vars[i as usize] != *text {
                self.state.str_vars[i as usize] = text.clone();
            }
            // (this kind keeps its frequency apart, `90.9 MHz@` in `mp3_freq`, and the
            // display begins with it)
            if let (Some(frequency), Some(display), Some(own)) =
                (frequency, p.magnitola, p.mp3_freq)
            {
                let own = &self.state.str_vars[own as usize];
                if let Some(shown) =
                    own_frequency(own, &self.state.str_vars[display as usize], frequency)
                {
                    self.state.str_vars[display as usize] = shown;
                }
            }
            return;
        }
        let (Some(display), Some(track)) = (p.magnitola, p.mp3_track) else {
            return;
        };
        if let Some(shown) = magnitola_line(
            &self.state.str_vars[track as usize],
            &self.state.str_vars[display as usize],
            text,
            frequency,
        ) {
            self.state.str_vars[display as usize] = shown;
        }
    }
}

/// The "Magnitola" radio's display with `text` as its second line: `track` is what the
/// playlist wrote (`90.9 MHz@R-ZURNAL`), `shown` what the display holds. None while the
/// display shows something else (its welcome, the volume), while the radio is stopped (no
/// station behind the `@`) or off (`text` empty). `own` is the frequency the station is
/// really on, where that is known: it stands for the script's.
pub(super) fn magnitola_line(
    track: &str,
    shown: &str,
    text: &str,
    own: Option<&str>,
) -> Option<String> {
    if text.is_empty() || shown != track {
        return None;
    }
    let (frequency, station) = track.split_once('@')?;
    (!station.trim().is_empty()).then(|| format!("{}@{text}", own.unwrap_or(frequency)))
}

/// A display that begins with the script's frequency (`script`: `90.9 MHz@`), with the
/// station's own in its place. None while it shows something else, and for the script's
/// `STOPPED@`, which is no frequency.
pub(super) fn own_frequency(script: &str, shown: &str, own: &str) -> Option<String> {
    if !script.ends_with('@') || !script.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    let rest = shown.strip_prefix(script)?;
    Some(format!("{own}@{rest}"))
}

#[cfg(test)]
mod radio_text_tests {
    use super::{magnitola_line, own_frequency};

    #[test]
    fn the_station_line_is_replaced_while_the_display_shows_it() {
        assert_eq!(
            magnitola_line("90.9 MHz@R-ZURNAL", "90.9 MHz@R-ZURNAL", "Radio 1   ", None).as_deref(),
            Some("90.9 MHz@Radio 1   ")
        );
        // its welcome and the volume are the display's own
        assert_eq!(
            magnitola_line("90.9 MHz@R-ZURNAL", " WELCOME  ", "Radio 1", None),
            None
        );
        assert_eq!(
            magnitola_line("90.9 MHz@R-ZURNAL", "VOLUME@ 15", "Radio 1", None),
            None
        );
        // stopped, and a radio that plays nothing
        assert_eq!(
            magnitola_line("STOPPED@", "STOPPED@", "Radio 1", None),
            None
        );
        assert_eq!(
            magnitola_line("90.9 MHz@R-ZURNAL", "90.9 MHz@R-ZURNAL", "", None),
            None
        );
    }

    #[test]
    fn the_stations_own_frequency_stands_for_the_scripts() {
        assert_eq!(
            magnitola_line(
                "90.9 MHz@R-ZURNAL",
                "90.9 MHz@R-ZURNAL",
                "Radio 1   ",
                Some("94.6 MHz")
            )
            .as_deref(),
            Some("94.6 MHz@Radio 1   ")
        );
        assert_eq!(
            magnitola_line("STOPPED@", "STOPPED@", "Radio 1", Some("94.6 MHz")),
            None
        );
        // the kind that keeps its frequency apart
        assert_eq!(
            own_frequency("90.9 MHz@", "90.9 MHz@Radio 1   ", "94.6 MHz").as_deref(),
            Some("94.6 MHz@Radio 1   ")
        );
        assert_eq!(
            own_frequency("90.9 MHz@", "94.6 MHz@Radio 1   ", "94.6 MHz"),
            None
        );
        assert_eq!(own_frequency("90.9 MHz@", " WELCOME  ", "94.6 MHz"), None);
        assert_eq!(own_frequency("STOPPED@", "STOPPED@", "94.6 MHz"), None);
    }
}
