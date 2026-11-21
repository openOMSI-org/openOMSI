//! Regression and integration tests for others_detail setting (#2538).
use super::Settings;

#[test]
fn default_is_off_for_desktop_and_phone() {
    assert!(!Settings::desktop().others_detail);
    assert!(!Settings::phone().others_detail);
    assert!(!Settings::default().others_detail);
    for src in [
        "",
        "version=1\n",
        "others_detail=0\n",
        "others_detail=false\n",
    ] {
        let s = Settings::from_text(src);
        assert!(!s.others_detail);
    }
}

#[test]
fn both_settings_writers_round_trip_others_detail() {
    let key = "others_detail";
    for phone in [false, true] {
        assert_eq!(
            omsi_launcher_lib::settings_from_text_on(None, phone)[key],
            false
        );
        for on in [false, true, false] {
            let mut settings = Settings::default();
            settings.others_detail = on;
            let native = settings.to_text();
            let launcher = omsi_launcher_lib::settings_from_text_on(Some(&native), phone);
            assert_eq!(launcher[key], on);
            let text = omsi_launcher_lib::settings_to_text(&launcher, Some(&native));
            let restored = Settings::from_text(&text);
            assert_eq!(restored.others_detail, on);
            assert_eq!(
                text.lines()
                    .filter(|line| line.starts_with(&format!("{key}=")))
                    .count(),
                1
            );
        }
    }
}

#[test]
fn others_detail_integration_off_and_on() {
    use omsi_lan::others::{Budget, Detail, LOW_FPS, RECOVERED_FPS, REDUCE_BEYOND, detail};

    // When the setting is off:
    let off_settings = Settings {
        others_detail: false,
        ..Default::default()
    };
    let mut off_budget = Budget::new();
    let off_struggling = off_budget.step(15.0);
    assert!(off_struggling);
    // Remote buses at all distances stay in full detail because the setting is disabled
    assert_eq!(
        detail(off_settings.others_detail, 0.0, false, off_struggling),
        Detail::Full
    );
    assert_eq!(
        detail(
            off_settings.others_detail,
            REDUCE_BEYOND + 50.0,
            false,
            off_struggling
        ),
        Detail::Full
    );
    assert_eq!(
        detail(off_settings.others_detail, 500.0, true, off_struggling),
        Detail::Full
    );

    // When the setting is on:
    let on_settings = Settings {
        others_detail: true,
        ..Default::default()
    };
    let mut on_budget = Budget::new();

    // Fast FPS (> 26)
    let fast_struggling = on_budget.step(60.0);
    assert!(!fast_struggling);
    // Nearby bus is in full detail
    assert_eq!(
        detail(on_settings.others_detail, 50.0, false, fast_struggling),
        Detail::Full
    );
    // Distant bus beyond 120 m is reduced
    assert_eq!(
        detail(
            on_settings.others_detail,
            REDUCE_BEYOND + 1.0,
            false,
            fast_struggling
        ),
        Detail::Reduced
    );
    // Bus we ride in is never thinned
    assert_eq!(
        detail(
            on_settings.others_detail,
            REDUCE_BEYOND + 1.0,
            true,
            fast_struggling
        ),
        Detail::Full
    );

    // Frame rate drops to struggling (<= 20 fps)
    let slow_struggling = on_budget.step(LOW_FPS);
    assert!(slow_struggling);
    // Even nearby buses are now reduced
    assert_eq!(
        detail(on_settings.others_detail, 10.0, false, slow_struggling),
        Detail::Reduced
    );
    // Bus we ride in is still never thinned
    assert_eq!(
        detail(on_settings.others_detail, 10.0, true, slow_struggling),
        Detail::Full
    );

    // Frame rate recovers (> 26 fps)
    let recovered_struggling = on_budget.step(RECOVERED_FPS + 5.0);
    assert!(!recovered_struggling);
    // Nearby bus returns to full detail
    assert_eq!(
        detail(on_settings.others_detail, 10.0, false, recovered_struggling),
        Detail::Full
    );

    // Repeated transitions across 120 m boundary and FPS thresholds:
    for i in 0..50 {
        let distance = if i % 2 == 0 {
            REDUCE_BEYOND - 10.0
        } else {
            REDUCE_BEYOND + 10.0
        };
        let expected = if i % 2 == 0 {
            Detail::Full
        } else {
            Detail::Reduced
        };
        assert_eq!(
            detail(on_settings.others_detail, distance, false, false),
            expected
        );
    }

    for i in 0..50 {
        let struggling = if i % 2 == 0 {
            on_budget.step(60.0);
            false
        } else {
            on_budget.step(LOW_FPS - 1.0);
            true
        };
        let expected = if struggling {
            Detail::Reduced
        } else {
            Detail::Full
        };
        assert_eq!(
            detail(on_settings.others_detail, 50.0, false, struggling),
            expected
        );
    }
}
