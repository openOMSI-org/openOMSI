//! What the weather does to the vehicle: the cabin air's humidity and the road's grip.

/// The relative humidity (a fraction, 1 = saturated) of air at `t` °C holding `abs_hum` g/m³,
/// as OMSI computes `Cabinair_relHum` (the original, Magnus over water with 7.5/237.3,
/// over ice with 7.6/240.7 below 0 °C). It was written as a percentage: 100 times what the
/// scripts and the passengers (a humid saloon at over 0.9) expect.
pub fn relative_humidity(t: f32, abs_hum: f32) -> f32 {
    let (a, b) = if t >= 0.0 { (7.5, 237.3) } else { (7.6, 240.7) };
    let sat = 1323.480_3 * 10f32.powf(a * t / (b + t)) / (273.15 + t);
    if sat > 0.0 && sat.is_finite() {
        (abs_hum / sat).max(0.0)
    } else {
        0.0
    }
}

/// The tyres' friction coefficient on the road under them: `street_cond` as the weather sets
/// it (0 dry … 1 wet, 1 … 2 snow from packed to fresh) and the air temperature (°C), below
/// which a wet road freezes (the stock "Ueberfrierende Naesse" weather). Dry asphalt 0.85,
/// wet 0.6, snow 0.3, black ice 0.12 - textbook values for truck tyres; OMSI keeps its
/// friction inside ode.dll and only hands `StreetCond` to the scripts, so these are not
/// read off it.
pub fn road_grip(street_cond: f32, temperature: f32) -> f32 {
    const DRY: f32 = 0.85;
    const WET: f32 = 0.6;
    const SNOW: f32 = 0.3;
    const ICE: f32 = 0.12;
    let c = street_cond.clamp(0.0, 2.0);
    if c <= 1.0 {
        let wet = DRY + (WET - DRY) * c;
        // a wet road at or below freezing turns to ice as the film freezes; a damp one is
        // black ice all over (the stock "Ueberfrierende Naesse": -6 degC, groundwet 0.31)
        if temperature <= 0.0 && c > 0.05 {
            let frozen = ((0.5 - temperature) / 3.0).clamp(0.0, 1.0) * (c / 0.25).min(1.0);
            wet + (ICE - wet) * frozen
        } else {
            wet
        }
    } else {
        WET + (SNOW - WET) * (c - 1.0)
    }
}

/// How fast the rain or snow wets a road vehicle's windscreen (its `precipRate` per second)
/// while something falls, as Omsi.exe works it out every frame (0x7d22d8): the fall's
/// velocity against the vehicle's, onto a pane facing up and a tenth forward, times 0.2 and
/// the strength squared. The fall (`precipVec`, 0x755724) comes down at 2 + 3 x strength m/s
/// in rain (`kind` 1) and 1.11 m/s otherwise, carried by `wind`; the pane is
/// (0, 0.985, 0.174) in the original's model frame (x right, y up, z forward; set in
/// `TRoadVehicle.LoadFromFile`, 0x7cb6c8) turned by the body's `rotation` (model ->
/// world, x east y north z up). `strength` is 0..1 (of 255), `velocity` the body's (m/s,
/// world). Rain from behind a fast bus leaves it dry: never below 0.
pub fn glass_wetting(
    kind: i32,
    strength: f32,
    wind: glam::Vec3,
    velocity: glam::Vec3,
    rotation: glam::Mat4,
) -> f32 {
    let fall = if kind == 1 {
        2.0 + 3.0 * strength
    } else {
        10.0 / 9.0
    };
    let precip = wind - glam::Vec3::Z * fall;
    let pane = rotation
        .transform_vector3(glam::Vec3::new(0.0, 0.174, 0.985))
        .normalize();
    let hitting = (-(precip - velocity).dot(pane)).max(0.0);
    0.2 * hitting * strength * strength
}

#[cfg(test)]
mod glass_tests {
    use super::glass_wetting;
    use glam::{Mat4, Vec3};

    #[test]
    fn rain_wets_the_glass_as_omsi_works_it_out() {
        let n = Vec3::new(0.0, 0.174, 0.985).normalize();
        // a standing bus in full rain, no wind: 5 m/s down onto the pane
        let r = glass_wetting(1, 1.0, Vec3::ZERO, Vec3::ZERO, Mat4::IDENTITY);
        assert!((r - 0.2 * 5.0 * n.z).abs() < 1e-5, "{r}");
        // half the strength: 3.5 m/s, a quarter of the factor
        let r = glass_wetting(1, 0.5, Vec3::ZERO, Vec3::ZERO, Mat4::IDENTITY);
        assert!((r - 0.2 * 0.25 * 3.5 * n.z).abs() < 1e-5, "{r}");
        // snow falls at 1.11 m/s whatever its strength
        let r = glass_wetting(2, 1.0, Vec3::ZERO, Vec3::ZERO, Mat4::IDENTITY);
        assert!((r - 0.2 * (10.0 / 9.0) * n.z).abs() < 1e-5, "{r}");
        // driving forward (+y) meets more of it, backing away less
        let ahead = glass_wetting(1, 1.0, Vec3::ZERO, Vec3::Y * 10.0, Mat4::IDENTITY);
        let back = glass_wetting(1, 1.0, Vec3::ZERO, -Vec3::Y * 10.0, Mat4::IDENTITY);
        assert!(
            (ahead - 0.2 * (10.0 * n.y + 5.0 * n.z)).abs() < 1e-4,
            "{ahead}"
        );
        assert!(back < 1.0 && back < ahead);
        // a tailwind as fast as the bus is the same as standing in still air
        let still = glass_wetting(1, 1.0, Vec3::ZERO, Vec3::ZERO, Mat4::IDENTITY);
        let carried = glass_wetting(1, 1.0, Vec3::Y * 8.0, Vec3::Y * 8.0, Mat4::IDENTITY);
        assert!((still - carried).abs() < 1e-5);
        // the pane turns with the body: heading east, the forward tilt faces east
        let east = Mat4::from_rotation_z(-std::f32::consts::FRAC_PI_2);
        let r = glass_wetting(1, 1.0, Vec3::ZERO, Vec3::X * 10.0, east);
        assert!((r - ahead).abs() < 1e-4, "{r} {ahead}");
        // reversing fast out of light snow: nothing reaches it, and no drying either
        assert_eq!(
            glass_wetting(2, 0.2, Vec3::ZERO, -Vec3::Y * 30.0, Mat4::IDENTITY),
            0.0
        );
    }
}

#[cfg(test)]
mod grip_tests {
    use super::road_grip;

    #[test]
    fn wet_snowy_and_frozen_roads_hold_less() {
        assert_eq!(road_grip(0.0, 15.0), 0.85);
        assert!((road_grip(1.0, 15.0) - 0.6).abs() < 1e-6);
        assert!((road_grip(2.0, -5.0) - 0.3).abs() < 1e-6);
        assert!(road_grip(1.0, -3.0) < 0.2, "black ice");
        assert_eq!(road_grip(0.0, -10.0), 0.85, "a dry road does not freeze");
    }
}
