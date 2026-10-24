//! The pages of the options, vehicle and world windows, and their titles.

use super::*;

pub(super) fn options_pages(app: &App) -> Vec<Page> {
    let s = &app.settings;
    let file = settings_file();
    let pick = |key: &str, name: &str, desc: &str| select_row(&file, key, name, desc);
    let pct = |v: f32| format!("{:.0} %", v * 100.0);
    let cm = |v: f32| format!("{:+.0} cm", v * 100.0);
    let later = "Takes effect when the game starts the next time";
    let mode = session_mode(app);
    let shown = |key: &str| shown_in_mode(mode, key);
    let game: Vec<(String, String)> = vec![
        switch_row(app, "navigator", "Navigator", "Enables/Disables the Minimap"),
        switch_row(app, "nav_ai", "AI vehicles on the map", "Shows/hides the other (AI) vehicles on the Minimap and the city map"),
        switch_row(app, "nav_surface", "Map surface under the roads", "Shows the map's ground, water, woods, houses and paved surfaces on the Minimap and the city map (and the map's package, when it has one)"),
        switch_row(app, "nav_light", "Light map", "The Minimap and the city map in a light look"),
        switch_row(app, "nav_3d", "3D buildings on the map", "The houses stand in 3D with their shadows on the Minimap, and the city map tilts into 3D when zoomed in"),
        switch_row(app, "nav_widget", "Navigator as a widget", "Drag the Minimap anywhere and change its size at its bottom right corner"),
        switch_row(app, "nav_arrows", "Route arrows (as in OMSI 2)", "Shows OMSI 2's route arrows over the road"),
        pick("navigator_corner", "Corner", later),
        switch_row(app, "get_up", "Ability to get up (Ctrl+Shift+G)", "Allows you to get out of the car and explore the world"),
        switch_row(app, "coll_objects", "Collisions with objects", "Enables/disables collisions with objects such as buildings, streetlights, etc."),
        switch_row(app, "coll_vehicles", "Collisions with vehicles", "Enables/Disables Collisions with Other Vehicles"),
        switch_row(app, "realistic_road", "Realistic road physics", "Tyres that slip, grip and roughness by road surface, wind on the bus; off, the buses drive as in OMSI 2"),
        switch_row(app, "collision_pedestrians", "Collisions with people", "Enables/disables knocking down people"),
        switch_row(app, "timetable_win", "Timetable window (Insert)", "Displays a list of all stops (only when a tour is active)"),
        switch_row(app, "trip_summary", "Trip summary", "At the end of each trip: its stops with the times planned and driven, to copy or to save"),
        switch_row(app, "info_bar", "Information bar (Shift+Y)", "Displays information such as the time, speed, and other details at the top of the screen"),
        switch_row(app, "exact_fare", "Passengers pay the exact fare", "No change is given at the cash desk"),
        switch_row(app, "full_tank", "Start with a full tank", "Each new bus starts with its tank full (many maps have no petrol station); a saved situation keeps its own"),
        switch_row(app, "pax_bad_driving", "Passengers react to bad driving", "They complain about rough driving and may get off at the next stop"),
        switch_row(app, "auto_announce", "Arrival announcements at the stop", "On depot files that keep two recordings per stop (the Chinese maps), the arrival one is played as the bus reaches the stop: the IBIS's next stop key, pressed as the driver would"),
        pick("boarding", "Boarding", "How passengers get their tickets"),
        pick("standing_chance", "Standing passengers", "The chance a passenger stands although a seat is free"),
        pick("maintenance", "Maintenance", later),
        pick("ai_unsched_factor", "Random traffic", later),
        pick("ai_max_scheduled", "Timetable vehicles", later),
        pick("ai_max_parked", "Parked cars", later),
        pick("ai_max_humans", "People at once", later),
    ]
        .into_iter()
        .flatten()
        .collect();
    let mut graphics: Vec<(String, String)> = vec![
        pick("graphics", "Graphics", later),
        preset_row(&file, "Quality preset", "Sets the graphics mode and most of the options at once, from Low (Vanilla) to Ultra (Enhanced with everything on)"),
        app.can_optimise().then(|| button("Optimise for my PC", "Start", "Measures while you drive and adjusts the graphics for a smooth frame rate; your settings are saved first", "optimise")),
        omsi_launcher_lib::optimise::backup().is_some().then(|| button("Restore my previous settings", "Restore", "The graphics settings as they were before the last optimisation", "optimise_restore")),
        pick("msaa", "Anti-aliasing", later),
        pick("render_scale", "Render scale", if s.upscaler == "fsr1" { "With FSR 1 it changes at once" } else { later }),
        pick("anisotropy", "Anisotropic", later),
        switch_row(app, "ray_tracing", "Real-Time Ray Tracing", "Enhanced: the sun's shadows, the ambient occlusion and the reflections traced with hardware ray queries, where the card has them; the heaviest option (takes effect when the game starts the next time)").filter(|_| shown("ray_tracing")),
        switch_row(app, "shadows", "Sun shadows", "Enables/Disables shadows").filter(|_| shown("shadows")),
        pick("shadow_size", "Shadow map", later).filter(|_| shown("shadow_size")),
        switch_row(app, "ssao", "Ambient occlusion", later).filter(|_| shown("ssao")),
        pick("shadow_casters", "Shadows cast by", later).filter(|_| shown("shadow_casters")),
        switch_row(app, "detail_textures", "Detail texturing up close", "The ground and large walls get fine grain when close").filter(|_| shown("detail_textures")),
        slider_row(app, "night_brightness", "Night brightness", "Enhanced graphics: how much brighter the night is shown", &|v| if v < 0.01 { "Off".to_string() } else { format!("+{v:.2}") }).filter(|_| shown("night_brightness")),
        slider_row(app, "led_glow", "LED glow", "How strongly the dots of LED destination displays glow", &|v| format!("{}/15", v as i64)).filter(|_| shown("led_glow")),
        slider_row(app, "led_mips", "LED mip strength", "Lower: sharper LED dots that shimmer from a distance; higher: calmer, softer dots", &|v| format!("{v:.2}")).filter(|_| shown("led_mips")),
        slider_row(app, "reflections", "Reflective objects reflect", "Changes show at once; turned up from Off, they come back when the game starts the next time", &|v| if v < 0.005 { "Off".to_string() } else { format!("{:.0}%", v * 100.0) }).filter(|_| shown("reflections")),
        switch_row(app, "clouds", "Clouds", later),
        pick("cloud_quality", "Cloud quality", "Enhanced: the volumetric clouds marched in fewer steps - faster, a little grainier").filter(|_| shown("cloud_quality") && app.settings.clouds),
        pick("street_light", "Street light", "Enhanced: shade over a whole street and light bounced between surfaces; Auto turns it off on a small or shared graphics chip (about half a millisecond a frame)").filter(|_| shown("street_light")),
        pick("light_shadows", "Light has shadows", "Enhanced: which lights cast shadows - the street lamps and the vehicles' headlamps, or every light: spots along their own axis and the lamps that shine every way in cube maps. Cached maps, a few passes a frame").filter(|_| shown("light_shadows")),
        switch_row(app, "cloud_shadows", "Cloud shadows and light shafts", "Enhanced: the clouds' shadow moving over the land and the sun's shafts through the gaps of a broken sky").filter(|_| shown("cloud_shadows")), switch_row(app, "light_glow", "Glow of lights", "Enhanced: the glare round bright lights and the lamps' halo in the night air").filter(|_| shown("light_glow")),
        pick("rain_quality", "Rain quality", "Lower is faster: the drops on the glass painted less often, less spray and fewer streaks; Low shows OMSI 2's own rain on the glass (from the next bus loaded)"),
        pick("vehicle_smoke", "Vehicle smoke", "Controls the vehicles' smoke particles, not scenery effects or glowing particles."),
        pick("puddles", if shown("ray_tracing") { "Real-Time Reflections" } else { "Puddles" }, "The wet roads' reflections in the rain: Low only near the bus and in fewer rays, much cheaper; Off none (the reflections on paint and glass stay)").filter(|_| app.settings.reflections > 0.0),
        switch_row(app, "windy_trees", "Windy trees", "The trees' leaves bend and sway in the wind and its gusts; with no wind they stand still"),
    ]
        .into_iter()
        .flatten()
        .collect();
    let at = graphics.iter().position(|r| r.1 == "sel render_scale").map_or(graphics.len(), |k| k + 1);
    graphics.splice(at..at, upscaler_rows(app, &file));
    let mut display = vec![
        switch_row(app, "fullscreen", "Fullscreen", "Switches the window between windowed and fullscreen"),
        pick("resolution", "Window size", later),
        switch_row(
            app,
            "triple_screen",
            "Triple screen",
            "Three physical screen projections; OpenXR takes priority",
        ),
    ];
    // (the rig's own settings only while it is on)
    let triple = vec![
        switch_row(
            app,
            "triple_hud_center",
            "HUD on centre screen",
            "Keep the navigator, menus and information on the centre screen",
        ),
        switch_row(
            app,
            "triple_span",
            "Span three monitors at startup",
            "Borderless across three equal monitors in one horizontal row; restart required",
        ),
        slider_row(
            app,
            "triple_width_mm",
            "Visible panel width",
            "Width of one screen without its frame",
            &|v| format!("{v:.0} mm"),
        ),
        slider_row(
            app,
            "triple_distance_mm",
            "Eye distance",
            "Eye to the centre screen",
            &|v| format!("{v:.0} mm"),
        ),
        slider_row(
            app,
            "triple_bezel_mm",
            "Frame width at each join",
            "Combined width of both adjacent frames",
            &|v| format!("{v:.0} mm"),
        ),
        slider_row(
            app,
            "triple_left_angle_deg",
            "Left screen angle",
            "Inward angle from a flat row",
            &|v| format!("{v:.0}°"),
        ),
        slider_row(
            app,
            "triple_right_angle_deg",
            "Right screen angle",
            "Inward angle from a flat row",
            &|v| format!("{v:.0}°"),
        ),
        slider_row(
            app,
            "triple_eye_height_mm",
            "Eye above screen centre",
            "Vertical eye offset",
            &|v| format!("{v:.0} mm"),
        ),
    ];
    if app.settings.triple.enabled {
        display.extend(triple);
    }
    let display: Vec<(String, String)> = display.into_iter().chain([
        switch_row(app, "vsync", "V-sync", "Waits for the screen's refresh"),
        pick("max_fps", "Frame limit", "Frames a second at most"),
        switch_row(app, "fps", "Frame rate", "Show the frames per second in the top right corner"),
        pick("perf_overlay", "Performance details", "Where each frame's time goes, how busy the processor is and what slows the game down, with live graphs"),
        switch_row(app, "sim_thread", "Simulation on its own thread", "The AI traffic, the people and the scenery scripts run beside the drawing, on another processor core (the AI cars one frame later); from the next start"),
        pick("view_distance", "View distance", later),
        pick("max_obj_dist", "Object distance", later),
        pick("min_obj_size", "Small objects", later),
        pick("mirror_size", "Mirrors", later),
        pick("mirror_refresh", "Bus mirror refresh", "How often the mirrors are drawn: every frame, the street moves through them as smoothly as through the windscreen"),
        pick("mirror_quality", "Mirror quality", "In Enhanced and Enhanced+: the mirrors shaded as the picture is, with its light and its sky, instead of the plain light").filter(|_| shown("mirror_quality")),
        switch_row(app, "mirror_shadows", "Shadows in the mirrors", "The mirrors show the sun's shadows (they look up the picture's shadow maps; no pass of their own)"),
        switch_row(app, "mirror_particles", "Smoke and spray in the mirrors", "The mirrors show exhaust and the spray of the wheels, as OMSI 2 does without its \"No PS in reflections\""),
        switch_row(app, "mirror_hud_every_frame", "Mirror panels every frame", "The mirror panels in the cab (Ctrl+M) are drawn every frame, as in ETS2, instead of taking turns with the other mirrors"),
        pick("video_memory", "Video memory", later),
        switch_row(app, "texture_compression", "Compress textures on loading", later),
        (!omsi_launcher_lib::graphics_profiles().is_empty()).then(|| opens("Load graphics profile", "Applies a graphics profile saved in the launcher", "gfxprofile")),
    ])
        .flatten()
        .collect();
    let sound: Vec<(String, String)> = vec![
        slider_row(app, "volume", "Volume", "Set how loud the game should be", &pct),
        slider_row(app, "vol_ai", "Traffic", "How loud the other vehicles are", &pct),
        switch_row(app, "ambient", "Ambience", "The place, the weather, the birds of the season and the town's life, from real recordings"),
        slider_row(app, "vol_ambient", "Ambience volume", "How loud the ambience is", &pct),
        slider_row(app, "vol_scenery", "Surroundings", "How loud the sounds of the scenery are", &pct),
        switch_row(app, "doppler", "Doppler effect", "Approaching sounds higher, receding ones lower"),
        pick("pax_voices", "Passenger voices", "What passengers say"),
        switch_row(app, "voice_enabled", "Voice chat in multiplayer", "Hold the nearby (V) or radio (B) key to talk. The microphone is used only while a key is held; nothing is recorded"),
        switch_row(app, "voice_all_off", "Everybody else's voice chat off", "Single players are muted in the player list"),
        switch_row(app, "voice_talker_list", "Show who is talking", "A line for each player you hear"),
        slider_row(app, "voice_volume", "Voices: volume", "How loud the other players' voices are", &pct),
        slider_row(app, "voice_mic_gain", "Microphone gain", "How much your microphone is amplified", &pct),
    ]
        .into_iter()
        .chain(crate::radio::option_rows(app, &pct))
        .flatten()
        .collect();
    let mut camera: Vec<(String, String)> = vec![
        switch_row(app, "head", "Head movement", "The view moves with the vehicle's acceleration"),
        switch_row(app, "cam_smooth", "Smooth viewpoint changes", "Enables a smooth transition between camera perspectives"),
        switch_row(app, "camcoll", "Camera collisions", "The outside camera cannot pass through objects"),
        switch_row(app, "steer_look", "View turns with steering", "Camera turns with the steering wheel (cockpit only)"),
        slider_row(app, "steer_look_angle", "Steering view angle", "How far the view turns at full steering lock", &|v| format!("{v:.0}°")),
        slider_row(app, "steer_look_response", "Steering view response", "How quickly the view follows the steering", &|v| format!("{:.0} ms", v * 1000.0)),
        slider_row(app, "head_idle", "Head sway at a standstill", "How much the view sways on its own when nothing is done to it - a head at rest is never quite still, most of it seen while the bus waits at a stop", &|v| if v <= 0.0 { "Off".to_string() } else { format!("{:.0}%", v * 100.0) }),
        slider_row(app, "head_idle_pace", "Sway pace", "How fast that sway moves (100% is the pace it is designed at)", &|v| format!("{:.0}%", v * 100.0)),
        switch_row(app, "hands_in_cab", "Driver's hands in the cab view", "Shows the driver's hand on the steering wheel (Cockpit only)"),
        switch_row(app, "driver", "Driver at the wheel (outside views)", "Shows the driver in the outside views and in the mirrors"),
    ]
        .into_iter()
        .chain(head_tracking_rows(app, &pct))
        .chain(vec![
        slider_row(app, "look_sens", "Mouse look sensitivity", "How fast the view turns when looking round with the mouse (100% is OMSI's)", &pct),
        slider_row(app, "look_smoothing_ms", "Smooth the mouse look", "How long the view takes to come round to where the mouse or the stick turned it (off: at once, as OMSI)", &|v| if v <= 0.0 { "Off".to_string() } else { format!("{v:.0} ms") }),
        switch_row(app, "alt_view", "Right mouse button turns the view", "Shift+right zooms; off: right zooms as in OMSI, the wheel button turns"),
        switch_row(app, "precision_zoom", "Precision mouse zoom", "The mouse zoom follows the FOV curve instead of OMSI's linear way"),
        slider_row(app, "fov", "Field of view", "Vertical field of view; in triple screen Default uses physical measurements, an override moves the virtual eye", &|v| if v < 20.0 { "Default".to_string() } else { format!("{v:.0}°") }),
        slider_row(app, "seat 1", "Seat forward and back", "Adjust the driver's seat position forward or backward", &cm),
        slider_row(app, "seat 2", "Seat height", "Adjust the driver's seat height", &cm),
        slider_row(app, "seat 0", "Seat left and right", "Adjust the driver's seat position from side to side", &cm),
        slider_row(app, "seat_pitch", "Head pitch", "Set the driver's neutral head tilt up or down, independent of the display setup", &|v| format!("{v:+.0}°")),
        ])
        .flatten()
        .collect();
    camera.push(button("Reset the seat position", "Reset", "Put the seat back where the vehicle has it.", "seat_reset"));
    camera.extend(pick("mirror_hud", "Mirror panels on the screen", "Copies of the bus's mirrors over the picture in the cab, as in ETS2 (Ctrl+M shows or hides them); a bus whose panels you arranged keeps its own"));
    if app.player.is_some() && !app.vr_active() && s.mirror_size != 0 { camera.push(button("Arrange the mirror panels", "Open", "Drag them where you want them and change their size, saved for this vehicle (Ctrl+Shift+M).", "mirror_hud_edit")); }
    if app.player.is_some() && !app.vr_active() { camera.push(button("Edit the view and mirrors", "Open", "A panel at the side of the driver's view: the eye, the direction, the field of view and each mirror, saved for this vehicle.", "view_editor")); }
    if cfg!(windows) {
        camera.extend(
            vec![
                switch_row(app, "vr", "Use OpenXR headset", later),
                if s.vr { pick("vr_scale", "Eye resolution", later) } else { None },
                if s.vr { pick("vr_head_smoothing_ms", "Head tracking smoothing", later) } else { None },
                if s.vr { pick("vr_mirror_rate", "Bus mirror refresh", later) } else { None },
                if s.vr { switch_row(app, "vr_desktop_mirror", "Show headset picture on monitor", later) } else { None },
            ]
                .into_iter()
                .flatten(),
        );
    }
    let controls: Vec<(String, String)> = vec![
        pick("drive_keys", "Driving keys", "Which keys drive the vehicle"),
        switch_row(app, "mouse", "Steering with the mouse", "Steer and control the pedals using the mouse"),
        switch_row(app, "mouse_right", "A right click ends the mouse steering", "As in OMSI; off: the right button only looks round"),
        slider_row(app, "mouse_sens", "Mouse steering sensitivity", "Adjust how much the steering wheel turns based on mouse movement", &pct),
        slider_row(app, "mouse_pedal_strength", "Mouse pedal strength", "Adjust how much mouse travel is needed to reach full throttle or braking", &|v| if (v - 1.0).abs() < 0.01 { "OMSI".to_string() } else { format!("{:.0}%", v * 100.0) }),
        switch_row(app, "mouse_smooth", "Smooth mouse steering", "The wheel eases after the cursor; off: it follows at once, as in OMSI"),
        switch_row(app, "mouse_pedals", "The mouse works the pedals", "Up the throttle, down the brake, as in OMSI; off: the mouse only steers, the pedals stay on the keys"),
        switch_row(app, "mouse_hold", "Hold the cursor while steering", "The wheel reaches its lock past the window's edges; off: the crosshair stays free and without delay, as in OMSI"),
        slider_row(app, "key_pedal_speed", "Pedal keys speed", "How fast the throttle and brake keys press their pedals; slower is finer", &|v| if (v - 1.0).abs() < 0.01 { "OMSI".to_string() } else { format!("{:.0}%", v * 100.0) }),
        switch_row(app, "steering_linear", "Steering linearity (keys at OMSI's steady pace)", "Keyboard steering at OMSI's steady pace"),
        switch_row(app, "old_steering", "Old Steering (the wheel stays, turn it back yourself)", "The wheel stays where the keys left it"),
        switch_row(app, "red_steer_spd", "Dynamic steering (slower keys at speed, OMSI's redSteerSpd)", "The steering keys act slower at speed"),
        switch_row(app, "ff", "Force feedback and vibration", "Enable force feedback for the steering wheel and vibration for controllers"),
        switch_row(app, "ff_invert", "Invert force feedback by default", "For wheels without a saved direction"),
        slider_row(app, "wheel_range", "Wheel rotation", "The steering wheel's own rotation, lock to lock", &|v| format!("{v:.0}°")),
        slider_row(app, "wheel_lock", "Full lock at", "How far the wheel turns for the vehicle's full lock", &|v| if v < 45.0 { "OMSI".to_string() } else { format!("{v:.0}°") }),
        slider_row(app, "pedal_t", "Throttle pedal strength", "Adjust how strongly pedal input affects the throttle", &|v| format!("x{v}")),
        slider_row(app, "pedal_b", "Brake pedal strength", "Adjust how strongly pedal input affects the brake", &|v| format!("x{v}")),
        switch_row(app, "blinker_cancel", "Indicators cancel themselves", "The bus's script turns the indicator off after a turn; off: it stays on until you turn it off"),
        switch_row(app, "brake_hold", "Keyboard brake stays on", "Keep the brake applied until the throttle is pressed"),
        switch_row(app, "auto_clutch", "Automatic clutch", "Automatically operate the clutch for you"),
        switch_row(app, "momentary_gears", "H-pattern shifter: return to neutral when the gear is released", "For a manual shifter without a neutral button; releasing a gear selects neutral immediately"),
        switch_row(app, "auto_shift", "Automated manual gearbox", "Shift a manual gearbox's gears for you by the engine speed"),
    ]
        .into_iter()
        .flatten()
        .collect();
    let interface: Vec<(String, String)> = vec![
        pick("language", "Language", "The language of the game's interface"),
        pick("units", "Units", "Speeds, distances and temperatures in the game's own texts"),
        switch_row(app, "machine_translation", "Translate the remaining texts automatically (offline, downloads 620 MB once)", "Translates texts nobody has translated, on this machine"),
        slider_row(app, "ui_scale", "Game interface size", "The size of the texts, the menu, the timetable and the navigator", &pct),
        switch_row(app, "ui_scale_window", "Interface grows with the window", "On a window taller than 1080p the interface grows with it"),
        slider_row(app, "ui_opacity", "Interface opacity", "How much of the interface's backgrounds shows", &pct),
        switch_row(app, "tooltips", "Name of the button under the mouse", "Shows the name of what the cursor points at"),
        switch_row(app, "notes", "Driving hints", "Why the vehicle does not move, the change due, what a service did: as short notifications"),
        switch_row(app, "chat", "Chat in online games", "Shows the chat of a LAN session"),
        slider_row(app, "chat_size", "Chat size", "The chat's texts on top of the interface size (also Ctrl + the mouse wheel over the chat)", &pct),
        switch_row(app, "name_tags", "Other players' names above their buses", "Shows the names of the other players"),
        Some(opens("Reset all settings...", "Everything but the language, the key bindings and the game folder goes back to how it came", "reset")),
    ]
        .into_iter()
        .flatten()
        .collect();
    let mut pages = vec![("Gameplay", game), ("Graphics", graphics), ("Display and memory", display), ("Sound", sound), ("Camera", camera), ("Controls", controls), ("Interface", interface)];
    if app.vr_active() && app.player.is_some() {
        let desc = "Navigator position (this bus)";
        let mut rows = vec![
            switch_row(app, "navigator", "Navigator", desc).unwrap(),
            button("Move and rotate with the mouse...", "Open", desc, "vr_nav_edit"),
        ];
        for (id, label) in [("x", "Position right / left"), ("y", "Position forward / back"), ("z", "Position up / down"), ("width", "Display width")] {
            rows.extend(slider_row(app, &format!("vr_nav_{id}"), label, desc, &cm));
        }
        for (id, label) in [("yaw", "Display rotation"), ("tilt", "Display tilt"), ("roll", "Display roll")] {
            rows.extend(slider_row(app, &format!("vr_nav_{id}"), label, desc, &|v| format!("{v:.0}°")));
        }
        rows.extend(slider_row(app, "vr_nav_opacity", "Interface opacity", desc, &pct));
        rows.push(button("Reset navigator position", "Reset", desc, "vr_nav_reset"));
        pages.push(("VR", rows));
    }
    pages
}

pub(super) fn vehicle_pages(app: &App) -> Vec<Page> {
    let has = app.player.is_some();
    let server = crate::input_script::on_server(&app.args);
    let mut display: Vec<(String, String)> = Vec::new();
    if has {
        let typed_number = if app.menus.menu_edit_number {
            app.menus.menu_edit.clone().unwrap_or_default()
        } else { app.player.as_ref().unwrap().vehicle.number() };
        let typed_ident = if app.menus.menu_edit_ident {
            app.menus.menu_edit.clone().unwrap_or_default()
        } else { app.player.as_ref().unwrap().vehicle.ident() };

        display.push(opens("Destination display", "Change the current destination", "dest"));
        display.push(opens("Depot file (HOF)", "Change the current depot file (used for the timetable)", "hof"));
        display.push(opens("Fleet number", "Change the vehicle's current fleet number", "number"));
        display.push((row("Custom fleet number", if app.menus.menu_edit_number{'E'}else{'e'}, &typed_number, "Enter custom fleet number (not from the list)", None), "custom_number_edit".to_string()));
        display.push((row("Custom plate", if app.menus.menu_edit_ident{'E'}else{'e'}, &typed_ident, "Enter custom plate (leave free for automatic)", None), "custom_ident_edit".to_string()));
    }
    if !server {
        display.push(opens("Driver", "Change the current driver profile", "driver"));
    }
    let mut fleet: Vec<(String, String)> = Vec::new();
    if has || !app.session.placed.is_empty() {
        fleet.push(button("Drive the next vehicle", "Switch", "Take the wheel of another vehicle standing in the world", "switch"));
        // (one of them by name, rather than going through them one by one, #2026)
        if app.session.placed.len() > 1 {
            fleet.push(opens("Drive another vehicle", "Choose which of the vehicles standing in the world to drive", "switchto"));
        }
    }
    fleet.push(opens("Place a vehicle", "Place a vehicle of your choice", "place"));
    if has {
        fleet.push(button("Couple", "Couple", "Couple the vehicle to the one in front of or behind it", "couple"));
        fleet.push(button("Uncouple", "Uncouple", "Separate the coupled vehicles", "uncouple"));
        if app.session.on_foot.is_none() {
            fleet.push(button("Get up and out", "Get out", "Step out of your car and explore the world", "getout"));
        }
        fleet.push(button("Remove this vehicle", "Remove", "Removes the current vehicle", "remove"));
        // (#728: another bus in this one's place, or this one again with its files read
        // anew - a script or a .bus changed - without starting the game again)
        fleet.push(button("Swap for another vehicle", "Swap", "Put another vehicle in this one's place and drive it", "swap"));
        fleet.push(button("Reload this vehicle", "Reload", "Read the vehicle's files again (.bus, model and sound configuration, scripts) and drive it from here", "reload"));
    }
    if !app.session.placed.is_empty() {
        fleet.push(button("Remove the placed vehicles", "Remove", "Removes all vehicles you've placed from the world", "clearplaced"));
    }
    let mut service: Vec<(String, String)> = Vec::new();
    if has {
        service.push(button("Refuel", "Refuel", "Fills the tank of the current vehicle", "refuel"));
        service.push(button("Wash", "Wash", "Cleans the current vehicle", "wash"));
        service.push(button("Repair", "Repair", "Repairs the current vehicle", "repair"));
        service.push(button("Put back on its wheels", "Reset", "Return the vehicle to an upright position", "reset"));
        if !server && app.menus.navigator.is_some() {
            service.push(button("Move on the map", "Pick", "Teleports you to any location on the map", "teleport"));
            service.push(opens("Teleport to a start point", "Teleport to a starting point on the map", "tplist"));
        }
    }
    vec![
        ("Display and driver", sectioned_rows(display, "Destination and depot", &[("number", "Vehicle identification"), ("driver", "Driver")])),
        ("Vehicles", sectioned_rows(fleet, "Switch and place vehicles", &[("couple", "Coupling"), ("getout", "Driver"), ("remove", "Replace and remove vehicles")])),
        ("Service", sectioned_rows(service, "Vehicle maintenance", &[("reset", "Recovery and relocation")])),
    ]
}

pub(super) fn world_pages(app: &App) -> Vec<Page> {
    let client = app.net.lan.as_ref().is_some_and(|l| l.role == omsi_net::Role::Client);
    let pct = |v: f32| format!("{:.0} %", v * 100.0);
    let mut time: Vec<(String, String)> = Vec::new();
    let mut weather: Vec<(String, String)> = Vec::new();
    let mut climate: Vec<(String, String)> = Vec::new();
    let mut tools: Vec<(String, String)> = Vec::new();
    if !client {
        let t = app.clock.time;
        let now = format!("{:02}:{:02}", ((t / 3600.0) as i64).rem_euclid(24), ((t / 60.0) as i64) % 60);
        time.extend(switch_row(app, "time_sync", "Real-time sync", "The game follows your device's date and time"));
        if app.real_time_locked() {
            let (d, m) = app.clock.day_month();
            let text = format!("{:04}-{m:02}-{d:02}  {}:{:02}", app.clock.year, now, (t as i64) % 60);
            time.push((row("Date and time", 'i', &text, "Synchronized with the real time", None), "noop".to_string()));
        } else {
            // the exact time: typed as hours, minutes and seconds
            match app.menus.menu_edit.as_ref() {
                Some(d) => {
                    let mut c: Vec<char> = d.chars().collect();
                    c.resize(6, '_');
                    let typed = format!("{}{}:{}{}:{}{}", c[0], c[1], c[2], c[3], c[4], c[5]);
                    time.push((row("Exact time", 'E', &typed, "Press Enter to change, Esc to cancel", None), "time_edit".to_string()));
                }
                None => {
                    let secs = format!("{}:{:02}", now, (t as i64) % 60);
                    time.push((row("Exact time", 'e', &secs, "Change the current time (Press Enter to change)", None), "time_edit".to_string()));
                }
            }
            time.extend(slider_row(app, "hour", "Hour", "Set the hour of the day directly", &|v| format!("{:02}", v as i64)));
            time.extend(slider_row(app, "minute", "Minute", "Set the minute directly", &|v| format!("{:02}", v as i64)));
            // (forwards and back by a tap: the typed time wants Enter and the held keys Ctrl+Shift+Page Up,
            // neither of which a phone has)
            for (name, text, secs) in [("Clock -1 hour", "-1 h", -3600), ("Clock -10 minutes", "-10 min", -600), ("Clock +10 minutes", "+10 min", 600), ("Clock +1 hour", "+1 h", 3600)] {
                time.push(button(name, text, "Move the clock.", &format!("clock_shift {secs}")));
            }
            for (name, hm, secs) in [("Morning", "06:00", 6 * 3600), ("Noon", "12:00", 12 * 3600), ("Evening", "18:00", 18 * 3600), ("Night", "23:00", 23 * 3600)] {
                time.push(button(name, hm, "Jump to this time of day.", &format!("clock_set {secs}")));
            }
            // the date: by a day, a week or a month, the time of day kept (the timetable's
            // day, the season and the map's chrono changes follow it as at midnight)
            let (d, m) = app.clock.day_month();
            let day = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"][app.clock.weekday().rem_euclid(7) as usize];
            let date = format!("{:04}-{m:02}-{d:02}  {}", app.clock.year, omsi_ui::tr(day));
            time.push((row("Date", 'i', &date, "The day the timetable, the season and the map's changes follow", None), "noop".to_string()));
            for (name, text, secs) in [
                ("Date -1 month", "-1 month", app.clock.months_away(-1)),
                ("Date -1 week", "-1 week", -7.0 * 86400.0),
                ("Date -1 day", "-1 day", -86400.0),
                ("Date +1 day", "+1 day", 86400.0),
                ("Date +1 week", "+1 week", 7.0 * 86400.0),
                ("Date +1 month", "+1 month", app.clock.months_away(1)),
            ] {
                time.push(button(name, text, "Move the date.", &format!("clock_shift {secs}")));
            }
            if let (Some(_), Some(p)) = (app.session.duty.as_ref(), app.player.as_ref()) {
                let d = p.vehicle.host.tt_delay as f64;
                if d.abs() >= 1.0 {
                    let text = format!("{}{}:{:02}", if d < 0.0 { "-" } else { "+" }, (d.abs() / 60.0) as i64, d.abs() as i64 % 60);
                    time.push(button("On time with the timetable", &text, "Move the clock so that the vehicle is on time", "clock_ontime"));
                }
            }
            if app.net.lan.is_none() {
                time.extend(slider_row(app, "speed", "Time speed", "How fast the world's clock runs", &|v| format!("x{v}")));
            }
        }
        weather.extend(switch_row(app, "metar_sync", "METAR sync", "The weather follows the real METAR report"));
        let src = if app.settings.metar_station.is_empty() { format!("{} ({})", app.metar_station(), omsi_ui::tr("automatic")) } else { app.metar_station() };
        weather.push((row("METAR source", 'o', &src, "The airport used for real weather.", None), "metar_src".to_string()));
        let typed=if app.menus.menu_edit_icao{
            let mut s=app.menus.menu_edit.clone().unwrap_or_default(); while s.len()<4{s.push('_');} format!("{s}  ({})", omsi_ui::tr("typing"))
        }else{app.metar_station()};
        weather.push((row("ICAO",if app.menus.menu_edit_icao{'E'}else{'e'},&typed,"Enter any 4-letter ICAO station.",None),"metar_icao_edit".to_string()));
        if app.metar_locked() {
            weather.push(button("METAR report", "Refresh now", "Fetch the selected station again without waiting for the next automatic update.", "metar_refresh"));
        } else {
            weather.push(button("METAR report", "Load once", "Load the selected station once without enabling continuous METAR sync.", "metar_once"));
        }
        weather.push((row("Preset", 'o', &weather_shown(app), "A ready-made weather", None), "weather".to_string()));
        if !app.metar_locked() {
            weather.push(button("Custom weather", "Edit current", "Freeze the weather currently in force and edit it as a custom weather.", "weather_custom"));
        }
        // how much of the sky is covered: a slider, as the launcher's panel has it (the five
        // cloud types are the points it runs between)
        weather.extend(slider_row(app, "cloud_cover", "Cloud cover", "How much of the sky the clouds cover.", &|v| if v <= 0.0 { omsi_ui::tr("Clear sky").into_owned() } else { format!("{:.0} %", v * 100.0) }));
        weather.extend(slider_row(app, "visibility", "Visibility", "How far one can see; less is fog.", &|v| if v >= 1000.0 { format!("{:.1} km", v / 1000.0) } else { format!("{} m", v as i64) }));
        weather.extend(slider_row(app,"brightness","Brightness","Brightness of the custom weather lighting.",&|v|format!("{:.0} %",v*100.0)));
        let kind = app.session.weather.as_ref().map(|w| (w.precip.first().copied().unwrap_or(0.0).max(0.0) as usize).min(PRECIP_KINDS.len() - 1)).unwrap_or(0);
        weather.push((row("Precipitation", 'o', PRECIP_KINDS[kind], "Rain or snow.", None), "precipkind".to_string()));
        weather.extend(slider_row(app, "rain_amt", "Precipitation strength", "How hard it rains or snows.", &pct));
        weather.extend(slider_row(app, "wet", "Wet roads", "How wet the roads are now (they dry in the sun, wet in the rain).", &pct));
        weather.extend(switch_row(app,"snow_cover","Snow cover","Snow lying on the world and ground."));
        weather.extend(switch_row(app,"snow_road","Snow on road","Treat the road surface as snow-covered."));
        climate.extend(slider_row(app, "temp", "Temperature", "The air temperature.", &|v| format!("{} °C", v as i64)));
        let dew_temp=app.session.weather.as_ref().map(|w|w.temp.0).unwrap_or(15.0);
        climate.extend(slider_row(app,"humidity","Humidity","Relative humidity of the air.",&|v|format!("{:.0} % | {}",v,omsi_ui::tr("dew point {t} °C").replace("{t}",&format!("{:.0}",crate::weather_setup::dew_point_c(dew_temp,v))))));
        climate.extend(slider_row(app, "wind_speed", "Wind speed", "How fast the wind blows; it drives the clouds.", &|v| format!("{} m/s", v as i64)));
        climate.extend(slider_row(app, "wind_dir", "Wind direction", "The direction of the wind in degrees (0 is north).", &|v| format!("{}°", v as i64)));
        // the METAR sync on: only its own rows stay (the weather is the report's)
        if app.metar_locked() {
            weather.retain(|r| matches!(r.1.as_str(), "metar_sync" | "metar_src" | "metar_icao_edit" | "metar_refresh"));
            climate.clear();
        }
        tools.push(button("Object editor", "Open", "Place and move objects in the world.", "editor"));
    }
    let mut people: Vec<(String, String)> = Vec::new();
    people.extend(slider_row(app, "traffic", "Traffic", "How many vehicles drive around the map.", &|v| omsi_ui::tr("{n} vehicles").replace("{n}", &(v as i64).to_string())));
    if !client && app.session.traffic.is_some() {
        people.push(button("Clear AI traffic", "Clear", "Remove the current AI cars from the road; random traffic will return automatically.", "traffic_clear"));
    }
    people.extend(slider_row(app, "pax", "Passengers", "How many passengers wait at the stops and ride.", &pct));
    vec![
        ("Time", sectioned_rows(time, "Clock and time speed", &[("clock_shift -3600", "Quick time changes"), ("clock_set 21600", "Time presets"), ("noop", "Date"), ("clock_ontime", "Timetable"), ("speed", "Time speed")])),
        ("Weather", weather_rows(weather, climate)),
        ("Traffic and people", sectioned_rows(people, "Traffic", &[("pax", "Passengers")])),
        ("Tools", tools),
    ]
}

/// The pages of the settings window `kind` (empty ones left out) and the one shown.
pub(super) fn pages_of(app: &App, kind: &ListKind) -> Option<(Vec<Page>, usize)> {
    let (pages, tab) = match kind {
        ListKind::Options(t) => (options_pages(app), *t),
        ListKind::Vehicle(t) => (vehicle_pages(app), *t),
        ListKind::World(t) => (world_pages(app), *t),
        ListKind::Controls => (vec![("Controls", items(app, kind))], 0),
        ListKind::Keyboard(tab) => (keyboard_pages(app), *tab),
        ListKind::ControllerDevices(_) | ListKind::Controller(..) | ListKind::ControllerAxis(..) | ListKind::ControllerButtonSettings(..) => crate::game_controller_menu::pages(app, kind)?,
        _ => return None,
    };
    let pages: Vec<Page> = pages.into_iter().filter(|p| !p.1.is_empty()).collect();
    let tab = tab.min(pages.len().saturating_sub(1));
    Some((pages, tab))
}

fn shown_in_mode(mode: &str, key: &str) -> bool {
    let mode = crate::settings::graphics_mode(mode);
    match key {
        "shadows" | "ssao" => !matches!(mode, "vanilla" | "enhanced_plus"),
        "shadow_size" | "shadow_casters" | "detail_textures" => mode != "vanilla",
        "reflections" => mode != "enhanced_plus",
        "night_brightness" | "led_glow" | "led_mips" | "mirror_quality" | "cloud_quality" => matches!(mode, "enhanced" | "enhanced_plus"),
        // (the ray-traced picture has its own occlusion)
        "street_light" => mode == "enhanced",
        "ray_tracing" | "light_shadows" | "cloud_shadows" | "light_glow" => matches!(mode, "enhanced" | "enhanced_plus"),
        _ => true,
    }
}

/// Which tab of a window (`kind`: Options, World ...) is the one titled `title` (the first if none is).
pub(crate) fn tab_of(app: &App, kind: ListKind, title: &str) -> usize {
    pages_of(app, &kind).and_then(|(pages, _)| pages.iter().position(|p| p.0 == title)).unwrap_or(0)
}

/// The page titles of the settings window last asked for: its kind, whether the VR
/// navigator was offered, when, and the answer (`MenuState::page_titles`).
pub(crate) type TitlesCache = Option<(ListKind, bool, std::time::Instant, (Vec<String>, usize))>;

pub(crate) fn forget_page_titles(app: &App) {
    *app.menus.page_titles.borrow_mut() = None;
}

/// The titles of the pages of an open settings window and the one shown.
///
/// Asked every frame while a window is open, and building the pages is the work of
/// all their rows: the answer is kept for a moment.
pub(crate) fn page_titles(app: &App, kind: &ListKind) -> Option<(Vec<String>, usize)> {
    let vr_nav_available = app.vr_active() && app.player.is_some();
    if let Some(hit) = app.menus.page_titles.borrow().as_ref().filter(|(k, vr, t, _)| k == kind && *vr == vr_nav_available && t.elapsed().as_millis() < 5000).map(|(_, _, _, r)| r.clone()) {
        return Some(hit);
    }
    let (pages, tab) = pages_of(app, kind)?;
    let r = (pages.iter().map(|p| p.0.to_string()).collect::<Vec<_>>(), tab);
    *app.menus.page_titles.borrow_mut() = Some((kind.clone(), vr_nav_available, std::time::Instant::now(), r.clone()));
    Some(r)
}

#[cfg(test)]
mod tests {
    use super::shown_in_mode;
    use crate::game_lists::{mode_of_session, text_in_mode};
    use crate::startup::SessionSettings;

    #[test]
    fn graphics_rows_follow_the_sessions_mode() {
        let keys = ["shadows", "shadow_size", "ssao", "shadow_casters", "detail_textures", "night_brightness", "led_glow", "led_mips", "reflections", "mirror_quality", "cloud_quality", "street_light", "msaa", "ray_tracing", "light_shadows", "cloud_shadows", "light_glow"];
        let shown = |mode: &str| keys.iter().filter(|k| shown_in_mode(mode, k)).copied().collect::<Vec<_>>();
        assert_eq!(shown("vanilla"), ["reflections", "msaa"]);
        assert_eq!(shown("vanilla_plus"), ["shadows", "shadow_size", "ssao", "shadow_casters", "detail_textures", "reflections", "msaa"]);
        assert_eq!(shown("enhanced"), keys);
        assert_eq!(shown("Enhanced+"), ["shadow_size", "shadow_casters", "detail_textures", "night_brightness", "led_glow", "led_mips", "mirror_quality", "cloud_quality", "msaa", "ray_tracing", "light_shadows", "cloud_shadows", "light_glow"]);
    }
    #[test]
    fn the_running_sessions_mode_decides_what_it_forces() {
        let file = "graphics=vanilla\nshadows=1\nssao=1\ndetail_textures=1\n";
        assert!(!crate::settings::Settings::from_text(file).shadows);
        let s = text_in_mode(file, "vanilla_plus");
        assert_eq!(s.graphics, "vanilla_plus");
        assert!(s.shadows && s.ssao && s.detail_textures && !s.classic());
        let s = text_in_mode("graphics=enhanced_plus\nshadows=0\nssao=0\nreflections=0\n", "vanilla");
        assert!(!s.ray_tracing() && !s.enhanced && !s.shadows && !s.ssao && s.reflections == 0.0);
        let s = text_in_mode("graphics=vanilla\nshadows=0\n", "enhanced_plus");
        assert!(s.ray_tracing() && s.enhanced && s.shadows && s.ssao && s.reflections == 1.0);
    }

    #[test]
    fn the_sessions_mode_comes_from_how_the_game_was_started() {
        let session = |enhanced_plus, enhanced, classic| SessionSettings { enhanced_plus, enhanced, classic, ..Default::default() };
        assert_eq!(mode_of_session(&session(false, false, true), "vanilla"), "vanilla");
        assert_eq!(mode_of_session(&session(false, false, false), "vanilla_plus"), "vanilla_plus");
        assert_eq!(mode_of_session(&session(false, true, false), "enhanced"), "enhanced");
        assert_eq!(mode_of_session(&session(false, true, false), "enhanced_plus"), "enhanced_plus");
        assert_eq!(mode_of_session(&session(false, true, false), "vanilla"), "enhanced");
        assert_eq!(mode_of_session(&session(true, true, false), "vanilla"), "enhanced_plus");
        assert_eq!(mode_of_session(&session(false, false, false), "enhanced"), "vanilla_plus");
    }
}

/// The upscaler's rows of the graphics page (after the render scale's): the upscaler, and
/// FSR 1's quality and sharpening while it is chosen.
fn upscaler_rows(app: &App, file: &serde_json::Value) -> Vec<(String, String)> {
    let fsr = app.settings.upscaler == "fsr1";
    let pct = |v: f32| format!("{:.0} %", v * 100.0);
    [
        select_row(file, "upscaler", "Upscaling", "Standard Catmull-Rom or experimental FSR 1; changes show at once"),
        select_row(file, "fsr_quality", "FSR quality", &fsr_size(app)).filter(|_| fsr),
        slider_row(app, "fsr_sharpness", "FSR sharpness", "RCAS sharpening strength", &pct).filter(|_| fsr),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// What FSR 1's quality means now: the size the 3D picture is drawn at, and the window's.
fn fsr_size(app: &App) -> String {
    let sizes = app.gfx.surface.as_ref().zip(app.renderer.as_ref()).map(|(sf, r)| ((sf.config.width, sf.config.height), r.scene_size(sf.config.width, sf.config.height)));
    match sizes {
        Some(((ww, wh), (w, h))) => omsi_ui::tr("The 3D picture is drawn at {} and scaled up to {}").replacen("{}", &format!("{w}×{h}"), 1).replacen("{}", &format!("{ww}×{wh}"), 1),
        None => omsi_ui::tr("Internal render resolution").into_owned(),
    }
}

/// Keep all atmospheric controls together after permission and METAR filtering.
fn weather_rows(mut weather: Vec<(String, String)>, climate: Vec<(String, String)>) -> Vec<(String, String)> {
    weather.extend(climate);
    sectioned_rows(weather, "Live weather", &[("weather", "Weather preset and clouds"), ("temp", "Temperature and humidity"), ("wind_speed", "Wind")])
}

/// Insert display-only headings while preserving every command and its relative order.
fn sectioned_rows(rows: Vec<(String, String)>, first: &str, starts: &[(&str, &str)]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (label, action) in rows {
        let key = action.strip_suffix(ADJUST).unwrap_or(&action);
        let title = starts.iter().find(|(id, _)| *id == key).map(|(_, title)| *title).or_else(|| out.is_empty().then_some(first));
        if let Some(title) = title {
            out.push((row(title, 'i', "", "", None), HEADING.to_string()));
        }
        out.push((label, action));
    }
    out
}

#[cfg(test)]
mod pause_sections_tests {
    use super::*;

    #[test]
    fn pause_nested_sections_keep_actions_in_original_order() {
        let rows = vec![button("A", "A", "", "a"), button("B", "B", "", "b"), button("C", "C", "", "c")];
        let sections = sectioned_rows(rows.clone(), "First", &[("b", "Second")]);
        assert_eq!(sections.iter().filter(|r| r.1 != HEADING).cloned().collect::<Vec<_>>(), rows);
        assert_eq!(sections.iter().filter(|r| r.1 == HEADING).count(), 2);
    }

    #[test]
    fn pause_nested_sections_omit_missing_groups_and_match_adjustable_controls() {
        assert!(sectioned_rows(Vec::new(), "First", &[]).is_empty());
        let sections = sectioned_rows(vec![button("Wind", "", "", &format!("wind_speed{ADJUST}"))], "First", &[("wind_speed", "Wind")]);
        assert_eq!(sections.len(), 2);
        assert!(sections[0].0.starts_with("Wind"));
        assert_eq!(sections[1].1, format!("wind_speed{ADJUST}"));
    }
}

#[cfg(test)]
mod pause_weather_tests {
    use super::*;

    #[test]
    fn pause_weather_merges_temperature_humidity_and_wind_without_losing_commands() {
        let weather = vec![button("Preset", "", "", "weather")];
        let climate = ["temp", "humidity", "wind_speed", "wind_dir"].into_iter()
            .map(|id| button(id, "", "", &format!("{id}{ADJUST}"))).collect::<Vec<_>>();
        let expected: Vec<_> = weather.iter().chain(&climate).cloned().collect();
        let rows = weather_rows(weather, climate);
        assert_eq!(rows.iter().filter(|r| r.1 != HEADING).cloned().collect::<Vec<_>>(), expected);
        assert_eq!(rows.iter().filter(|r| r.1 == HEADING).count(), 3);
    }

    #[test]
    fn pause_weather_filtered_climate_adds_no_empty_sections() {
        let rows = weather_rows(vec![button("METAR sync", "", "", "metar_sync")], Vec::new());
        assert_eq!(rows.len(), 2);
        assert!(weather_rows(Vec::new(), Vec::new()).is_empty());
    }

    #[test]
    fn pause_weather_wind_has_traditional_chinese_translation() {
        assert_eq!(crate::_rust_i18n_try_translate("zh-tw", "Wind").map(|value| value.into_owned()), Some("風".to_string()));
    }
}

/// The head tracking rows of the camera page: native TrackIR / OpenTrack, a Tobii eye tracker,
/// the sensitivities and the inversions.
fn head_tracking_rows(app: &App, pct: &dyn Fn(f32) -> String) -> Vec<Option<(String, String)>> {
    let s = &app.settings;
    let pick = |key: &str, name: &str, desc: &str| select_row(&settings_file(), key, name, desc);
    vec![
        switch_row(app, "headtrack", "Head tracking (native TrackIR / OpenTrack)", &omsi_ui::tr("Use native TrackIR on Windows, or OpenTrack on UDP port {}")
            .into_owned().replace("{}", &s.head_tracking_port.to_string())),
        // (a Tobii eye tracker read natively: its source, its look at the edges, its smoothing)
        pick("head_tracking_source", "Head tracking with", "Automatic: opentrack while it sends, else a Tobii eye tracker (Windows, with the Tobii Experience installed)").filter(|_| s.head_tracking),
        switch_row(app, "tobii_edges", "Look further at the screen's edges (Tobii)", "A look at the left or right edge of the screen turns the view further that way").filter(|_| s.head_tracking && s.head_tracking_source != "opentrack"),
        slider_row(app, "head_tracking_smoothing", "Smoothing", "Steadies the view against the tracker's trembling; big turns still follow at once (0 %: it only glides between the tracker's poses). For opentrack, choose its Accela filter there", pct).filter(|_| s.head_tracking && s.head_tracking_source != "opentrack"),
        slider_row(app, "head_tracking_dead_zone", "Dead zone", "Head movements smaller than this leave the view where it is, wherever you look", &|v| format!("{v:.2}°")).filter(|_| s.head_tracking && s.head_tracking_source != "opentrack"),
        slider_row(app, "head_tracking_yaw_sens", "TrackIR yaw sensitivity", "Left / right head movement. 100% is 1:1; the low end is deliberately finer.", &|v| format!("{v:.0}%")),
        slider_row(app, "head_tracking_pitch_sens", "TrackIR pitch sensitivity", "Up / down head movement. 100% is 1:1; the low end is deliberately finer.", &|v| format!("{v:.0}%")),
        slider_row(app, "head_tracking_roll_sens", "TrackIR roll sensitivity", "Head tilt. 100% is 1:1; the low end is deliberately finer.", &|v| format!("{v:.0}%")),
        slider_row(app, "head_tracking_x_sens", "TrackIR X sensitivity", "Move your head left / right. 100% is 1:1; the low end is deliberately finer.", &|v| format!("{v:.0}%")),
        slider_row(app, "head_tracking_y_sens", "TrackIR Y sensitivity", "Move your head up / down. 100% is 1:1; the low end is deliberately finer.", &|v| format!("{v:.0}%")),
        slider_row(app, "head_tracking_z_sens", "TrackIR Z sensitivity", "Move your head forward / back. 100% is 1:1; the low end is deliberately finer.", &|v| format!("{v:.0}%")),
        switch_row(app, "head_tracking_invert_yaw", "Invert TrackIR yaw", "Reverse left / right head rotation"),
        switch_row(app, "head_tracking_invert_pitch", "Invert TrackIR pitch", "Reverse up / down head rotation"),
        switch_row(app, "head_tracking_invert_roll", "Invert TrackIR roll", "Reverse head tilt"),
        switch_row(app, "head_tracking_invert_x", "Invert TrackIR X", "Reverse left / right head movement"),
        switch_row(app, "head_tracking_invert_y", "Invert TrackIR Y", "Reverse up / down head movement"),
        switch_row(app, "head_tracking_invert_z", "Invert TrackIR Z", "Reverse forward / back head movement"),
    ]
}
