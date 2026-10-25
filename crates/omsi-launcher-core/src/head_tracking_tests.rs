/// A Tobii eye tracker's settings: their defaults, a file's values kept through the page's
/// writer, and ones out of reach (or misspelt) back to the defaults.
#[test]
fn the_tobii_settings_are_kept() {
    let d = super::settings_from_text(None);
    assert_eq!((d["head_tracking_source"].as_str(), d["tobii_edges"].as_bool()), (Some("auto"), Some(false)));
    let t = super::settings_to_text(&super::settings_from_text(Some("head_tracking_source=Tobii\ntobii_edges=1\nhead_tracking_smoothing=0.83\nhead_tracking_dead_zone=0.3\n")), None);
    for line in ["head_tracking_source=tobii", "tobii_edges=1", "head_tracking_smoothing=0.85", "head_tracking_dead_zone=0.3"] {
        assert_eq!(t.lines().filter(|l| *l == line).count(), 1, "{line} in {t}");
    }
    let t = super::settings_to_text(&super::settings_from_text(Some("head_tracking_source=webcam\nhead_tracking_smoothing=9\n")), None);
    assert!(t.contains("head_tracking_source=auto\n") && t.contains("head_tracking_smoothing=0.5\n"), "{t}");
}
