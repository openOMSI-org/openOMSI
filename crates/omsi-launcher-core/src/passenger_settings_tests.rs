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

use super::*;
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
