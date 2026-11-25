use super::*;
#[test]
fn optional_passenger_preference_survives_launcher_saves() {
    let key = "passenger_hailing";
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
fn optional_full_bus_preference_survives_launcher_saves() {
    let key = "passenger_full_bus_reaction";
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
fn passenger_mass_preferences_survive_launcher_saves() {
    for phone in [false, true] {
        let fresh = settings_from_text_on(None, phone);
        assert_eq!(fresh["passenger_mass"], false);
        assert_eq!(fresh["passenger_weight_kg"], 75.0);
        let old = "passenger_mass=1\npassenger_weight_kg=82\npassenger_weight_spread=10\n";
        let settings = settings_from_text_on(Some(old), phone);
        assert_eq!(settings["passenger_mass"], true);
        let text = settings_to_text(&settings, Some(old));
        let again = settings_from_text_on(Some(&text), phone);
        assert_eq!(again["passenger_mass"], true);
        assert_eq!(again["passenger_weight_kg"], 82.0);
        assert_eq!(again["passenger_weight_spread"], 10.0);
        assert_eq!(again["passenger_weight_fixed"], false);
    }
}
