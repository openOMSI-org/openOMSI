//! A wheel, a set of pedals or a joystick nobody has set up yet is set up by itself, from
//! how its axes lie while nobody touches them: the X axis steers (unless it rests at an end); an axis resting at one end
//! of its travel is a pedal (whichever end it rests at, so a pedal that reads the other way
//! round is turned), the pedals taken as throttle, brake and clutch in the order the device
//! lists them, as nearly every wheel has them; and a device with no pedal lying at an end
//! (a joystick, a wheel with its pedals on one combined axis) drives with its Y axis, pushed
//! forward to go and pulled back to brake. Before, such a device only steered and the
//! player had to set up its pedals by hand in the launcher. Gamepads keep their own layout
//! (`gamepad_profile`); whatever is guessed here is saved like a set-up made by hand and
//! can be changed in the launcher's Game controllers page.

use crate::controllers::{Connected, DeviceCfg, Func};

/// How far from its centre an axis at rest must lie to be a pedal (a pedal's spring holds it
/// at its very end; a stick or a wheel rests in the middle).
const PEDAL_REST: f32 = 0.9;
/// How near its centre the Y axis must rest to drive as a combined throttle and brake.
const CENTRE_REST: f32 = 0.2;

/// Logitech's wheels on macOS and Linux, read as their HID report has them (Windows's
/// Logitech driver reorders the pedals): the clutch on Y, the throttle on Z, the brake on Rz.
const LOGITECH: u16 = 0x046d;
const LOGITECH_HID_PEDALS: &[u16] = &[
    0xc24f, // G29 (PlayStation)
    0xc260, // G29 (PlayStation 4 mode)
    0xc262, // G920
    0xc266, // G923 (PlayStation)
    0xc267, // G923 (PlayStation, Xbox's mode)
    0xc26d, // G923 (Xbox)
    0xc26e, // G923 (Xbox)
];

/// The set-up of a device nobody has set up, from its axes at rest (DirectInput slots:
/// X, Y, Z, Rx, Ry, Rz and the two sliders, -1..1). None: nothing it could drive with.
pub fn guess(name: &str, hardware_id: Option<(u16, u16)>, ff_capable: bool, rest: &[(usize, f32)], steering_taken: bool) -> Option<DeviceCfg> {
    let at = |k: usize| rest.iter().find(|(s, _)| *s == k).map(|(_, v)| *v);
    let mut d = DeviceCfg { name: name.to_string(), second: if ff_capable { "1" } else { "0" }.into(), ..Default::default() };
    // (an X axis resting at an end is a pedal: a set of pedals of its own has its throttle there)
    let steers = (ff_capable || !steering_taken) && at(0).is_some_and(|x| x.abs() < PEDAL_REST);
    if steers {
        d.axes[0] = Some((Func::Steering, false));
    }
    let mut pedals: Vec<(usize, f32)> = rest.iter().copied().filter(|&(k, v)| k < 8 && v.abs() >= PEDAL_REST).collect();
    pedals.sort_by_key(|p| p.0);
    pedals.dedup_by_key(|p| p.0);
    let y_centred = at(1).is_some_and(|v| v.abs() <= CENTRE_REST);
    // (a joystick's throttle lever left at its end is no pedal set: the stick drives)
    if steers && y_centred && (pedals.is_empty() || (!ff_capable && pedals.len() == 1)) {
        // a joystick, or a wheel's pedals on one axis: Y forward (-1) is the throttle
        d.axes[1] = Some((Func::ThrottleBrake, true));
        return Some(d);
    }
    if !steers && pedals.is_empty() {
        return None;
    }
    let slots: Vec<usize> = pedals.iter().map(|p| p.0).collect();
    let logitech_hid = !cfg!(windows) && hardware_id.is_some_and(|(v, p)| v == LOGITECH && LOGITECH_HID_PEDALS.contains(&p)) && slots == [1, 2, 5];
    let order: &[Func] = if logitech_hid { &[Func::Clutch, Func::Throttle, Func::Brake] } else { &[Func::Throttle, Func::Brake, Func::Clutch] };
    for (&(k, v), &f) in pedals.iter().zip(order) {
        // (a pedal reads -1 up and 1 down: one resting at 1 runs the other way)
        d.axes[k] = Some((f, v > 0.0));
    }
    Some(d)
}

pub fn guess_all(fresh: &[(&Connected, &[(usize, f32)])], mut steering_taken: bool) -> Vec<DeviceCfg> {
    let mut order: Vec<_> = fresh.iter().collect();
    order.sort_by_key(|(c, _)| !c.ff_capable);
    let mut out = Vec::new();
    for (c, rest) in order {
        if let Some(d) = guess(&c.name, c.hardware_id, c.ff_capable, rest, steering_taken) {
            steering_taken |= c.ff_capable && d.steers();
            out.push(d);
        }
    }
    out
}

/// Every device's axes as they lay untouched, once it had been read a few times (a device's
/// first reports can be empty), by name.
#[derive(Default)]
pub struct Rests(Vec<(String, u32, Vec<(usize, f32)>)>);

impl Rests {
    /// Reads before the axes are believed.
    const SETTLE: u32 = 10;

    /// Read `c` once more; its axes at rest once they are known.
    pub fn watch(&mut self, c: &Connected) -> Option<&[(usize, f32)]> {
        let i = match self.0.iter().position(|r| r.0 == c.name) {
            Some(i) => i,
            None => {
                self.0.push((c.name.clone(), 0, Vec::new()));
                self.0.len() - 1
            }
        };
        let r = &mut self.0[i];
        if r.1 < Self::SETTLE {
            r.1 += 1;
            r.2 = c.axes.clone();
            return None;
        }
        Some(&r.2)
    }

    /// The axes at rest of a device read often enough.
    pub fn of(&self, name: &str) -> Option<&[(usize, f32)]> {
        self.0.iter().find(|r| r.0 == name && r.1 >= Self::SETTLE).map(|r| r.2.as_slice())
    }
}

/// A device nobody set up, read in the game: its set-up once it is used - turned, or a pedal
/// pressed; a force-feedback wheel at once. (An idle joystick beside the keyboard is not
/// taken, #1476: it would hold the steering at its centre.)
pub fn when_used(rests: &mut Rests, c: &Connected, steering_taken: bool) -> Option<DeviceCfg> {
    if c.axes.is_empty() {
        return None;
    }
    let rest = rests.watch(c)?;
    let moved = |(k, v): &(usize, f32)| rest.iter().find(|(s, _)| s == k).is_some_and(|(_, v0)| (v - v0).abs() > 0.3);
    if !c.ff_capable && !c.axes.iter().any(moved) {
        return None;
    }
    let d = guess(&c.name, c.hardware_id, c.ff_capable, rest, steering_taken)?;
    log::info!("game controller {} set up by itself from its axes at rest {rest:?}: {:?}", c.name, d.axes);
    Some(d)
}

/// What the player is told once a device was set up by itself.
pub fn notice(d: &DeviceCfg) -> String {
    format!("{} was set up by itself ({}); change it in the launcher (Controls → Game controllers)", d.name, describe(d))
}

/// What the player is told: the device and what it drives with now.
pub fn describe(d: &DeviceCfg) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for f in [Func::Steering, Func::Throttle, Func::Brake, Func::Clutch, Func::ThrottleBrake] {
        if d.axes.iter().flatten().any(|(g, _)| *g == f) {
            parts.push(match f {
                Func::Steering => "steering",
                Func::Throttle => "throttle",
                Func::Brake => "brake",
                Func::Clutch => "clutch",
                _ => "throttle and brake on one axis",
            });
        }
    }
    parts.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn funcs(d: &DeviceCfg) -> Vec<(usize, Func, bool)> {
        d.axes.iter().enumerate().filter_map(|(k, a)| a.map(|(f, r)| (k, f, r))).collect()
    }

    #[test]
    fn a_wheel_with_three_pedals_resting_at_their_ends() {
        // a G25/G29 under Windows: Y, Rz and a slider, released at the top of their travel
        let d = guess("Logitech G29 Driving Force Racing Wheel", Some((0x046d, 0xc24f)), true, &[(0, 0.01), (1, 1.0), (2, 0.0), (5, 1.0), (6, 1.0)], false).unwrap();
        assert_eq!(d.second, "1");
        if cfg!(windows) {
            assert_eq!(funcs(&d), [(0, Func::Steering, false), (1, Func::Throttle, true), (5, Func::Brake, true), (6, Func::Clutch, true)]);
        }
        // a wheel of another maker, its pedals resting at -1
        let d = guess("Thrustmaster T300RS", Some((0x044f, 0xb66e)), true, &[(0, -0.02), (1, -1.0), (5, -0.98)], false).unwrap();
        assert_eq!(funcs(&d), [(0, Func::Steering, false), (1, Func::Throttle, false), (5, Func::Brake, false)]);
    }

    #[test]
    fn logitech_pedals_as_their_hid_report_has_them_off_windows() {
        let d = guess("Logitech G29 Driving Force Racing Wheel", Some((0x046d, 0xc24f)), true, &[(0, 0.0), (1, 1.0), (2, 1.0), (5, 1.0)], false).unwrap();
        let expected = if cfg!(windows) {
            [(0, Func::Steering, false), (1, Func::Throttle, true), (2, Func::Brake, true), (5, Func::Clutch, true)]
        } else {
            [(0, Func::Steering, false), (1, Func::Clutch, true), (2, Func::Throttle, true), (5, Func::Brake, true)]
        };
        assert_eq!(funcs(&d), expected);
    }

    #[test]
    fn a_joystick_or_combined_pedals_drive_with_y() {
        let d = guess("Saitek Cyborg Joystick", None, false, &[(0, 0.0), (1, 0.05), (2, 0.0), (6, 0.3)], false).unwrap();
        assert_eq!(d.second, "0");
        assert_eq!(funcs(&d), [(0, Func::Steering, false), (1, Func::ThrottleBrake, true)]);
        assert_eq!(describe(&d), "steering, throttle and brake on one axis");
    }

    #[test]
    fn pedals_of_their_own_and_a_joystick_with_a_throttle_lever() {
        assert!(guess("Shifter", None, false, &[], false).is_none());
        assert!(guess("Button box", None, false, &[(1, 0.3)], false).is_none());
        let d = guess("Fanatec ClubSport Pedals", None, false, &[(0, -1.0), (1, -1.0), (2, -1.0)], false).unwrap();
        assert_eq!(funcs(&d), [(0, Func::Throttle, false), (1, Func::Brake, false), (2, Func::Clutch, false)]);
        let d = guess("Thrustmaster T.16000M", None, false, &[(0, 0.0), (1, 0.0), (6, 1.0)], false).unwrap();
        assert_eq!(funcs(&d), [(0, Func::Steering, false), (1, Func::ThrottleBrake, true)]);
    }

    #[test]
    fn beside_a_wheel_already_set_up_only_a_force_feedback_wheel_steers() {
        assert!(guess("Thrustmaster TH8A", None, false, &[(0, 0.0), (1, 0.02)], true).is_none());
        assert!(guess("Button box", None, false, &[(0, 0.0)], true).is_none());
        let d = guess("Fanatec ClubSport Pedals", None, false, &[(0, -1.0), (1, -1.0), (2, -1.0)], true).unwrap();
        assert_eq!(funcs(&d), [(0, Func::Throttle, false), (1, Func::Brake, false), (2, Func::Clutch, false)]);
        let d = guess("Thrustmaster T300RS", Some((0x044f, 0xb66e)), true, &[(0, -0.02), (1, -1.0), (5, -0.98)], true).unwrap();
        assert_eq!(funcs(&d), [(0, Func::Steering, false), (1, Func::Throttle, false), (5, Func::Brake, false)]);
    }

    #[test]
    fn a_shifter_listed_before_the_wheel_does_not_steer_and_without_a_motor_both_still_do() {
        let device = |name: &str, ff: bool| Connected { name: name.into(), hardware_id: None, axes: Vec::new(), gamepad: false, ff, ff_capable: ff, buttons: 0 };
        let (shifter, wheel) = (device("Thrustmaster TH8A", false), device("Thrustmaster T300RS", true));
        let rest: &[(usize, f32)] = &[(0, 0.0), (1, 0.0)];
        let autos = guess_all(&[(&shifter, rest), (&wheel, rest)], false);
        assert_eq!(autos.iter().filter(|d| d.steers()).map(|d| d.name.as_str()).collect::<Vec<_>>(), ["Thrustmaster T300RS"]);
        let wheel = device("Logitech Driving Force GT", false);
        let autos = guess_all(&[(&shifter, rest), (&wheel, rest)], false);
        assert!(autos.iter().all(|d| d.steers()));
    }
}
