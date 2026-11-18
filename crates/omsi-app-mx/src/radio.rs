//! The bus radio in the game's options. The radio itself is `omsi_soundscape::radio`.

pub(crate) use omsi_soundscape::radio::*;

/// The options' rows of the radio's effects (the Sound tab), with their percentages.
pub(crate) fn option_rows(app: &crate::App, pct: &dyn Fn(f32) -> String) -> [Option<(String, String)>; 4] {
    use crate::game_lists::{slider_row, switch_row};
    [
        switch_row(app, "milezero", "MileZero integration", "The MileZero radio over the navigator: what plays, a play button (paused until you press it), song requests; the stream quality follows your connection"),
        slider_row(app, "radio_speaker", "Radio: small speaker", "The bus radio through a small dashboard speaker: no deep bass, no bright treble", pct),
        slider_row(app, "radio_hiss", "Radio: FM hiss", "The hiss and crackle of a weak FM signal", pct),
        slider_row(app, "radio_dropouts", "Radio: signal dropouts", "Now and then the signal is lost for a moment", pct),
    ]
}
