//! Why a bus does not move, and its physics in the log.

/// The player's key for `action` when `bindings` (the vehicle's `Inputs/keyboard.cfg`) bind it
/// to another key than OMSI's own file does; `None` for the stock key or no key at all.
pub fn rebound_key(bindings: &[omsi_content::KeyBinding], action: &str) -> Option<String> {
    use omsi_content::input::KEY_HOLD;
    let b = bindings
        .iter()
        .find(|b| b.action.eq_ignore_ascii_case(action) && b.scan_code != 0)?;
    let stock = crate::stock_keys::STOCK_KEYS.iter().any(|(a, s, m)| {
        a.eq_ignore_ascii_case(action)
            && *s == b.scan_code
            && (m & !KEY_HOLD) == (b.modifier & !KEY_HOLD)
    });
    (!stock).then(|| crate::keys::key_name(b.scan_code as i64, b.modifier as i64))
}

/// Why the bus is not moving although the throttle is pressed: the things a driver checks
/// first (empty when it moves or nothing is pressed). The window's HUD shows them, an
/// offscreen run logs them. `key(action)` names the player's own key for an action of
/// `Inputs/keyboard.cfg` when it is not the stock one (see [`rebound_key`]).
pub fn standing_reasons(
    v: &omsi_sim::VehicleInstance,
    key: &dyn Fn(&str) -> Option<String>,
) -> Vec<String> {
    let mut lines = Vec::new();
    let throttle = v.var("throttle").unwrap_or(0.0) > 0.05;
    let slow = v.physics.velocity_kmh().abs() < 3.0;
    if !(throttle && slow) {
        return lines;
    }
    if !omsi_sim::startup::engine_running(v) {
        let m = key("kw_m_enginestart").unwrap_or_else(|| "M".into());
        // (the electrics' key named as the player set it too, not always E - #461)
        let e = key("cp_batterietrennschalter_toggle").unwrap_or_else(|| "E".into());
        lines.push(format!("The engine is off  ({e} electrics, {m} starter; mod buses with an ignition key turn it with {e}: press again and hold. Shift+U does it all)"));
    } else if v
        .var("antrieb_getr_gangwahl")
        .map(|g| (g - 1.0).abs() < 0.1)
        .or_else(|| {
            v.var("cockpit_gangwahltaster")
                .map(|g| (g - 1.0).abs() < 0.1)
        })
        .unwrap_or(false)
    {
        // gear position 1 is N in the stock gearboxes and the mods built on them
        let d = key("automatic_D").unwrap_or_else(|| "D (Shift+D when W A S D drive)".into());
        lines.push(format!("The gearbox is in N: press {d}, or click it. Buses like the Citaro take D only with the brake held"));
    } else if v.var("antrieb_getr_gangwahl").is_none()
        && v.var("cockpit_gangwahltaster").is_none()
        && v.var("antrieb_getr_gang")
            .map(|g| g.abs() < 0.1)
            .unwrap_or(false)
    {
        // a manual gearbox (the stock F90 and T3, the Sprinters, PAZ and LAZ mods): gear 0 is
        // neutral, and the keyboard's `kw_s_1` … are the digit keys
        lines.push("Manual gearbox in neutral: 1-5 select a gear (R reverse, N neutral); the clutch works itself unless auto_clutch=0".to_string());
    }
    let parking = v
        .var("bremse_feststell")
        .or_else(|| v.var("parking_brake"))
        .unwrap_or(0.0)
        > 0.5;
    if parking {
        let k = key("parking_brake_toggle").unwrap_or_else(|| ".".into());
        lines.push(format!("Parking brake is on  ({k} releases it)"));
    }
    // a bus that stood long enough to lose its air (the stock scripts start with 4-9 bar):
    // the spring brake is released by air (`bremse_p_Brzyl_FBA`, absolute; the stock
    // brake curve still holds below 6.5 bar), so it holds until the compressor has
    // filled the tanks, whatever the parking brake lever says
    if !parking && lines.is_empty() {
        // (only where a script assigns the pressures: a non-pneumatic bus whose varlist still
        // declares them reads them as an empty system - #3030)
        let air = |name: &str| v.script_stores(name).then(|| v.var(name)).flatten();
        let fba_val = air("bremse_p_Brzyl_FBA").or_else(|| air("spring_brake_pressure"));
        if let Some(fba) = fba_val.filter(|p| *p < 6.0e5) {
            let tanks: Vec<f32> = (1..=4)
                .filter_map(|i| {
                    air(&format!("bremse_p_Tank0{i}")).or_else(|| air(&format!("air_tank_{i}")))
                })
                .collect();
            let low = tanks.iter().copied().fold(f32::MAX, f32::min);
            // (with the tanks full the spring brake is held by the bus's own parking brake,
            // whatever its script calls it, not by a want of air)
            let full = !tanks.is_empty() && low >= 6.0e5;
            // (whole sentences: the tables translate a text by its key)
            if !full && tanks.is_empty() {
                lines.push(format!("Air pressure is low (spring brake {:.1} bar): the spring brake holds until the compressor has filled the tanks - keep the engine running", fba / 1e5));
            } else if !full {
                lines.push(format!("Air pressure is low ({:.1} bar in the tanks, spring brake {:.1} bar): the spring brake holds until the compressor has filled the tanks - keep the engine running", low / 1e5, fba / 1e5));
            }
        }
    }
    if v.var("bremse_halte_sw").unwrap_or(0.0) > 0.5
        || v.var("bremse_halte").unwrap_or(0.0) > 0.5
        || v.var("bus_stop_brake").unwrap_or(0.0) > 0.5
    {
        lines.push(
            "Stop brake / door release is on: the bus stands until it is switched off".to_string(),
        );
    }
    // what the passengers are told is open (OMSI's `PAX_Entry<n>_Open` / `PAX_Exit<n>_Open`);
    // the door leaves' `door_<n>` only where a bus has none of those - mods put other things
    // in `door_<n>`, and a Hong Kong bus with its doors shut said they were open
    let pax: Vec<f32> = (0..8)
        .flat_map(|i| {
            let e = format!("PAX_Entry{i}_Open");
            let x = format!("PAX_Exit{i}_Open");
            [
                v.has_script_var(&e).then(|| v.var(&e)).flatten(),
                v.has_script_var(&x).then(|| v.var(&x)).flatten(),
            ]
        })
        .flatten()
        .collect();
    let open = if pax.is_empty() {
        (0..4).any(|i| v.var(&format!("door_{i}")).unwrap_or(0.0) > 0.05)
    } else {
        pax.iter().any(|x| *x > 0.5)
            || (0..4).any(|i| {
                let e = format!("PAX_Entry{i}_Open");
                let x = format!("PAX_Exit{i}_Open");
                (!v.has_script_var(&e) && !v.has_script_var(&x))
                    && v.var(&format!("door_{i}")).unwrap_or(0.0) > 0.05
            })
    };
    if open {
        lines.push("Doors are open".to_string());
    }
    lines
}

/// `OMSI_DEBUG_PHYSICS`: one line with the player's pose, speed, wheel contacts and crashes.
pub fn log_physics(v: &omsi_sim::VehicleInstance, t: f32) {
    let wheels = v
        .rigid
        .as_ref()
        .map(|rb| {
            rb.wheels
                .iter()
                .map(|w| {
                    format!(
                        "{:+.3}@{:+.3}{}{}{}",
                        w.compression,
                        w.ground_z - v.position.z,
                        if w.on_ground { "" } else { "!" },
                        if w.step_force.abs() > 500.0 {
                            format!(" step {:+.1} kN", w.step_force / 1000.0)
                        } else {
                            String::new()
                        },
                        if w.walls.is_empty() {
                            String::new()
                        } else {
                            format!(" against {} face(s)", w.walls.len())
                        }
                    )
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default();
    // the drift of the body: how far its travel departs from where it points (deg), and the
    // coupled parts' angles to it
    let slip = v
        .rigid
        .as_ref()
        .filter(|rb| rb.velocity.truncate().length() > 1.0)
        .map(|rb| {
            let d = (rb.velocity.x as f64)
                .atan2(rb.velocity.y as f64)
                .to_degrees()
                - v.heading;
            let d = (d + 540.0).rem_euclid(360.0) - 180.0;
            let d = if d.abs() > 90.0 {
                (d + 360.0).rem_euclid(360.0) - 180.0
            } else {
                d
            };
            format!(" slip {d:+.2}")
        })
        .unwrap_or_default();
    let joints = (0..2)
        .filter_map(|i| {
            v.var(&format!("articulation_{i}_alpha"))
                .map(|a| format!(" joint{i} {:+.2}", a))
        })
        .collect::<String>();
    log::info!(
        "physics t={t:5.2} pos ({:.2}, {:.2}, {:.3}) hdg {:.1}{slip}{joints} pitch {:+.2} bank {:+.2} v {:+.2} km/h wheels [{wheels}] crashes {} last {:.1} kJ gear {:?} n {:?} M_Wheel {:?} brake {:?} pedal {:?} abs {:?} rpm {:?}",
        v.position.x,
        v.position.y,
        v.position.z,
        v.heading,
        v.pitch,
        v.bank,
        v.physics.velocity_kmh(),
        v.crashes,
        v.last_impact / 1000.0,
        v.var("antrieb_getr_aktugang").or(v.var("gear")),
        v.var("engine_n"),
        v.var("M_Wheel"),
        v.var("Axle_Brakeforce_1_L"),
        v.var("Brake"),
        v.var("bremse_ABS_eingriff"),
        v.var("Wheel_RotationSpeed_1_L"),
    );
}

#[cfg(test)]
mod tests {
    use super::rebound_key;
    use omsi_content::KeyBinding;

    fn bind(action: &str, scan_code: i32, modifier: i32) -> KeyBinding {
        KeyBinding {
            action: action.into(),
            scan_code,
            modifier,
        }
    }

    #[test]
    fn hint_names_the_players_own_key_only() {
        // the stock D (scan 32) keeps the hint's own wording; Ctrl+D is named (#461)
        assert_eq!(
            rebound_key(&[bind("automatic_D", 32, 0)], "automatic_D"),
            None
        );
        assert_eq!(
            rebound_key(&[bind("automatic_D", 32, 4)], "automatic_D").as_deref(),
            Some("Ctrl+D")
        );
        assert_eq!(
            rebound_key(&[bind("automatic_D", 0, 0)], "automatic_D"),
            None
        );
        assert_eq!(rebound_key(&[], "automatic_D"), None);
        // the electrics' key: E is the stock one, a moved one is named
        assert_eq!(
            rebound_key(
                &[bind("cp_batterietrennschalter_toggle", 18, 0)],
                "cp_batterietrennschalter_toggle"
            ),
            None
        );
        assert_eq!(
            rebound_key(
                &[bind("cp_batterietrennschalter_toggle", 18, 4)],
                "cp_batterietrennschalter_toggle"
            )
            .as_deref(),
            Some("Ctrl+E")
        );
    }
}
