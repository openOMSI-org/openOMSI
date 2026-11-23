use super::*;

fn drive(target: f32, start: f32, slope: f32, secs: f32) -> f32 {
    let mut c = Cruise::new(target);
    let mut v = start;
    let dt = 1.0 / 60.0;
    for _ in 0..(secs / dt) as usize {
        let (t, b) = c.pedals(v, dt);
        assert!((0.0..=1.0).contains(&t) && (0.0..=0.4).contains(&b) && (t == 0.0 || b == 0.0));
        let a = t * 6.0 - b * 25.0 - v * 0.02 - slope;
        v = (v + a * dt).max(0.0);
    }
    v
}

#[test]
fn it_holds_the_speed_on_the_flat_uphill_and_downhill() {
    for slope in [0.0, 1.5, -1.0] {
        let v = drive(50.0, 50.0, slope, 120.0);
        assert!((v - 50.0).abs() < 2.0, "slope {slope}: {v}");
    }
    assert!((drive(50.0, 40.0, 0.0, 120.0) - 50.0).abs() < 2.0);
}

#[test]
fn cresting_a_hill_releases_the_throttle() {
    let mut cruise = Cruise::new(50.0);
    for _ in 0..600 {
        cruise.pedals(40.0, 1.0 / 60.0);
    }
    assert_eq!(cruise.pedals(51.0, 1.0 / 60.0), (0.0, 0.0));
    let (throttle, brake) = cruise.pedals(55.0, 1.0 / 60.0);
    assert_eq!(throttle, 0.0);
    assert!(brake > 0.0);
}

#[test]
fn a_light_analog_brake_goes_through_and_a_firm_one_switches_it_off() {
    let mut c = Cruise::new(50.0);
    assert!(c.with_driver(40.0, 0.016, 0.0, 0.0, false).unwrap().0 > 0.0);
    assert_eq!(
        c.with_driver(40.0, 0.016, 0.0, 0.03, false),
        Some((0.0, 0.03))
    );
    assert_eq!(c.with_driver(40.0, 0.016, 0.0, 0.2, false), None);
    assert_eq!(c.with_driver(40.0, 0.016, 0.0, 0.0, true), None);
}

#[test]
fn its_pedals_shift_an_automated_manual_up() {
    use omsi_input::auto_shift::auto_shift_gear;
    let mut c = Cruise::new(50.0);
    let (t, b) = c.with_driver(47.0, 0.016, 0.0, 0.0, false).unwrap();
    assert_eq!(auto_shift_gear(2, 5, 2.8, 0.0, 0.0, 0.0, 47.0), 2);
    assert_eq!(auto_shift_gear(2, 5, 2.8, 0.0, t, b, 47.0), 3);
}

#[test]
fn its_messages_are_translated_with_the_speed_in_them() {
    let ru = |key: &str| {
        crate::_rust_i18n_try_translate("ru", key)
            .map(|t| t.into_owned())
            .unwrap_or_else(|| key.to_string())
    };
    assert_eq!(on_text(49.6, ru), "Круиз-контроль: 50 км/ч");
    assert_eq!(
        on_text(3.0, ru),
        "Круиз-контроль работает на скорости выше 5 км/ч"
    );
    assert_eq!(
        on_text(49.6, |key: &str| key.to_string()),
        "Cruise control: 50 km/h"
    );
}

#[test]
fn nonsense_in_gives_no_pedals() {
    let mut c = Cruise::new(30.0);
    assert_eq!(c.pedals(f32::NAN, 0.016), (0.0, 0.0));
    assert_eq!(c.pedals(20.0, 0.0), (0.0, 0.0));
}
