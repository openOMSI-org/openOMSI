use super::*;
#[test]
fn optional_passenger_preference_survives_launcher_saves() {
    let key = "passenger_connections";
    for phone in [false, true] {
        assert_eq!(settings_from_text_on(None, phone)[key], false);
        assert_eq!(
            settings_from_text_on(Some("version=1\n"), phone)[key],
            false
        );
        for on in [false, true, false] {
            let old = format!("{key}=0\n{key}=1\n");
            let mut settings = settings_from_text_on(Some(&old), phone);
            settings[key] = json!(on);
            let text = settings_to_text(&settings, Some(&old));
            assert_eq!(
                text.lines()
                    .filter(|line| line.starts_with(&format!("{key}=")))
                    .count(),
                1
            );
            assert_eq!(settings_from_text_on(Some(&text), phone)[key], on);
        }
    }
}

#[test]
fn connection_chance_round_trips_without_enabling_connections() {
    for input in ["-10", "0", "35", "100", "150", "NaN", "inf"] {
        let settings = settings_from_text(Some(&format!("passenger_transfer_percent={input}\n")));
        let chance = settings["passenger_transfer_percent"].as_f64().unwrap();
        assert!(chance.is_finite() && (0.0..=100.0).contains(&chance));
        let text = settings_to_text(&settings, None);
        let read = settings_from_text(Some(&text));
        assert_eq!(
            read["passenger_transfer_percent"],
            settings["passenger_transfer_percent"]
        );
        assert_eq!(read["passenger_connections"], false);
    }
}
