use super::*;

pub(super) fn vehicle_pages(app: &App) -> Vec<Page> {
    let has = app.player.is_some();
    let server = crate::input_script::on_server(&app.args);
    let mut display: Vec<(String, String)> = Vec::new();
    if has {
        let typed_number = if app.menus.menu_edit_number {
            app.menus.menu_edit.clone().unwrap_or_default()
        } else {
            app.player.as_ref().unwrap().vehicle.number()
        };
        let typed_ident = if app.menus.menu_edit_ident {
            app.menus.menu_edit.clone().unwrap_or_default()
        } else {
            app.player.as_ref().unwrap().vehicle.ident()
        };

        display.push(opens(
            "Destination display",
            "Change the current destination",
            "dest",
        ));
        display.push(opens(
            "Depot file (HOF)",
            "Change the current depot file (used for the timetable)",
            "hof",
        ));
        display.push(opens(
            "Fleet number",
            "Change the vehicle's current fleet number",
            "number",
        ));
        if app.net.lan.is_none()
            && !app
                .player
                .as_ref()
                .unwrap()
                .vehicle
                .ty
                .paint_schemes
                .is_empty()
        {
            display.push(opens(
                "Livery",
                "Repaint the vehicle you drive where it stands: same bus, same passengers",
                "livery",
            ));
        }
        display.push((
            row(
                "Custom fleet number",
                if app.menus.menu_edit_number { 'E' } else { 'e' },
                &typed_number,
                "Enter custom fleet number (not from the list)",
                None,
            ),
            "custom_number_edit".to_string(),
        ));
        display.push((
            row(
                "Custom plate",
                if app.menus.menu_edit_ident { 'E' } else { 'e' },
                &typed_ident,
                "Enter custom plate (leave free for automatic)",
                None,
            ),
            "custom_ident_edit".to_string(),
        ));
    }
    if !server {
        display.push(opens(
            "Driver",
            "Change the current driver profile",
            "driver",
        ));
    }
    let mut fleet: Vec<(String, String)> = Vec::new();
    if has || !app.session.placed.is_empty() {
        fleet.push(button(
            "Drive the next vehicle",
            "Switch",
            "Take the wheel of another vehicle standing in the world",
            "switch",
        ));
        // (one of them by name, rather than going through them one by one, #2026)
        if app.session.placed.len() > 1 {
            fleet.push(opens(
                "Drive another vehicle",
                "Choose which of the vehicles standing in the world to drive",
                "switchto",
            ));
        }
    }
    fleet.push(opens(
        "Place a vehicle",
        "Place a vehicle of your choice",
        "place",
    ));
    if has {
        fleet.push(button(
            "Couple",
            "Couple",
            "Couple the vehicle to the one in front of or behind it",
            "couple",
        ));
        fleet.push(button(
            "Uncouple",
            "Uncouple",
            "Separate the coupled vehicles",
            "uncouple",
        ));
        if app.session.on_foot.is_none() {
            fleet.push(button(
                "Get up and out",
                "Get out",
                "Step out of your car and explore the world",
                "getout",
            ));
        }
        fleet.push(button(
            "Remove this vehicle",
            "Remove",
            "Removes the current vehicle",
            "remove",
        ));
        // (#728: another bus in this one's place, or this one again with its files read
        // anew - a script or a .bus changed - without starting the game again)
        fleet.push(button(
            "Swap for another vehicle",
            "Swap",
            "Put another vehicle in this one's place and drive it",
            "swap",
        ));
        fleet.push(button("Reload this vehicle", "Reload", "Read the vehicle's files again (.bus, model and sound configuration, scripts) and drive it from here", "reload"));
    }
    if !app.session.placed.is_empty() {
        fleet.push(button(
            "Remove the placed vehicles",
            "Remove",
            "Removes all vehicles you've placed from the world",
            "clearplaced",
        ));
    }
    let mut service: Vec<(String, String)> = Vec::new();
    if has {
        service.push(button(
            "Refuel",
            "Refuel",
            "Fills the tank of the current vehicle",
            "refuel",
        ));
        service.push(button("Wash", "Wash", "Cleans the current vehicle", "wash"));
        service.push(button(
            "Repair",
            "Repair",
            "Repairs the current vehicle",
            "repair",
        ));
        service.push(button(
            "Put back on its wheels",
            "Reset",
            "Return the vehicle to an upright position",
            "reset",
        ));
        if !server && app.menus.navigator.is_some() {
            service.push(button(
                "Move on the map",
                "Pick",
                "Teleports you to any location on the map",
                "teleport",
            ));
            service.push(opens(
                "Teleport to a start point",
                "Teleport to a starting point on the map",
                "tplist",
            ));
        }
    }
    vec![
        (
            "Display and driver",
            sectioned_rows(
                display,
                "Destination and depot",
                &[("number", "Vehicle identification"), ("driver", "Driver")],
            ),
        ),
        (
            "Vehicles",
            sectioned_rows(
                fleet,
                "Switch and place vehicles",
                &[
                    ("couple", "Coupling"),
                    ("getout", "Driver"),
                    ("remove", "Replace and remove vehicles"),
                ],
            ),
        ),
        (
            "Service",
            sectioned_rows(
                service,
                "Vehicle maintenance",
                &[("reset", "Recovery and relocation")],
            ),
        ),
        (
            if tag_axle_available(app) {
                "Tag axle"
            } else {
                "Tag axle (unavailable)"
            },
            if let Some(rb) = app.player.as_ref().and_then(|p| p.vehicle.rigid.as_ref()) {
                let mut rows = Vec::new();
                if rb.wheel_axle.iter().copied().max().unwrap_or(0) >= 2 {
                    rows.extend(slider_row(app, "tag_preset", "Steering preset", "Choose a starting profile; moving an individual slider changes it to Custom", &|v| omsi_ui::tr(tag_preset_name(v)).into_owned()));
                    rows.extend(switch_row(app, "tag_steering", "Enable tag-axle override", "Change the rear axle's physical steering while driving; off follows the original bus physics"));
                    if rb.tag_axle_steering.is_some() {
                        rows.extend(slider_row(
                            app,
                            "tag_axle",
                            "Steered axle",
                            "Axle number, counting the front axle as 1",
                            &|v| omsi_ui::tr("Axle {n}").replace("{n}", &format!("{v:.0}")),
                        ));
                        rows.extend(slider_row(
                            app,
                            "tag_multiplier",
                            "Steering strength",
                            "Multiplier of rear steering at low speed",
                            &|v| format!("{v:.2}×"),
                        ));
                        rows.extend(slider_row(
                            app,
                            "tag_fade_start",
                            "Fade begins",
                            "Start reducing rear steering above this speed",
                            &|v| format!("{v:.0} km/h"),
                        ));
                        rows.extend(slider_row(
                            app,
                            "tag_fade_end",
                            "Fully straight at",
                            "No rear steering at or above this speed",
                            &|v| format!("{v:.0} km/h"),
                        ));
                    }
                }
                if rows.is_empty() {
                    rows.push((
                        row(
                            "Requires a vehicle with at least three physics axles",
                            'i',
                            "",
                            "",
                            None,
                        ),
                        "noop".to_string(),
                    ));
                }
                rows
            } else {
                vec![(
                    row("No vehicle loaded", 'i', "", "", None),
                    "noop".to_string(),
                )]
            },
        ),
    ]
}
