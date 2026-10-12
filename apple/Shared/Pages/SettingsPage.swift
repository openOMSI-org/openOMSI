import SwiftUI

/// The Settings page: the game's settings file (`settings.cfg`), every change saved at once,
/// in the tabs and with the choices of the game's launcher (`launcher/pages.rs`). What a
/// system cannot have is left out there (a window size on an iPhone, VR on a Mac, the
/// updater of an App Store build).
struct SettingsPage: View {
    @Environment(LauncherStore.self) private var store
    @State private var tab = "graphics"
    @State private var languages: [(String, String)] = []
    /// The page's width: two columns of cards where there is room for them.
    @State private var width: CGFloat = 1000

    private var phone: Bool { store.platform == "ios" }
    private var store_build: Bool { store.platform == "ios" }

    var body: some View {
        PageScroll("Settings", subtitle: "Every change is saved at once; the game reads them when it starts.") {
            UnderlineTabs(items: [("graphics", "Graphics"), ("driving", "Driving"), ("camera", "Camera"), ("sound", "Sound"), ("gameplay", "Gameplay"), ("general", "General")], selection: $tab)
            columns
        }
        .onAppear {
            if languages.isEmpty, let l = try? JSONSerialization.jsonObject(with: Bridge.call("languages")) as? [[String: Any]] {
                languages = l.compactMap { d in (d["code"] as? String).map { ($0, d["name"] as? String ?? $0) } }
            }
        }
    }

    /// Two columns of cards side by side where there is room (as the desktop launcher's
    /// settings grid), one under the other where there is not.
    @ViewBuilder
    private var columns: some View {
        Group {
            if width >= 820 {
                HStack(alignment: .top, spacing: 20) {
                    left.frame(maxWidth: .infinity)
                    right.frame(maxWidth: .infinity)
                }
            } else {
                VStack(spacing: 20) { left; right }
            }
        }
        .frame(maxWidth: .infinity)
        .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { width = $0 }
        .id(tab)
    }

    @ViewBuilder
    private var left: some View {
        switch tab {
        case "graphics": graphics
        case "driving": driving
        case "camera": camera
        case "sound": sound
        case "gameplay": passengers
        default: interface
        }
    }

    @ViewBuilder
    private var right: some View {
        switch tab {
        case "graphics": display
        case "driving": controllers
        case "camera": outside
        case "sound": Color.clear.frame(height: 1)
        case "gameplay": simulation
        default: updates
        }
    }

    // --- graphics --------------------------------------------------------------------------

    private var graphics: some View {
        let mode = store.setting("graphics")?.string ?? "vanilla_plus"
        let classic = mode == "vanilla"
        let traced = mode == "enhanced_plus"
        return Card("Graphics") {
            SettingSelect(label: "Graphics", key: "graphics", options: [("vanilla", "Vanilla (as OMSI 2)"), ("vanilla_plus", "Vanilla+"), ("enhanced", "Enhanced"), ("enhanced_plus", "Enhanced+")])
            PresetRow(mode: mode)
            SettingSelect(label: "Anti-aliasing", key: "msaa", options: [("1", "Off"), ("2", "2x MSAA"), ("4", "4x MSAA"), ("8", "8x MSAA")])
            SettingSelect(label: "Render scale", key: "render_scale", options: [("auto", "Auto"), ("1", "100%"), ("0.85", "85%"), ("0.75", "75%"), ("0.67", "67%"), ("0.5", "50%")])
            SettingSelect(label: "Anisotropic", key: "anisotropy", options: [("1", "Off"), ("2", "2x"), ("4", "4x"), ("8", "8x"), ("16", "16x")])
            if !classic {
                SettingSelect(label: "Shadow map", key: "shadow_size", options: [("1024", "1024"), ("2048", "2048"), ("4096", "4096")])
                if !traced {
                    SettingToggle(label: "Ambient occlusion", key: "ssao", fallback: true)
                    SettingToggle(label: "Sun shadows", key: "shadows", fallback: true)
                }
                SettingSelect(label: "Shadows cast by", key: "shadow_casters", options: [("all", "Every solid mesh"), ("omsi", "[shadow] meshes, as OMSI")])
                SettingToggle(label: "Detail texturing up close", key: "detail_textures", fallback: true)
                SettingSlider(label: "Night brightness", key: "night_brightness", range: 0 ... 3, step: 0.25, fallback: 0) { $0 < 0.01 ? "Off" : String(format: "+%.2f", $0) }
                SettingSlider(label: "LED glow", key: "led_glow", range: 0 ... 15, step: 1, fallback: 6) { $0 < 0.5 ? "Off" : "\(Int($0))" }
                SettingSlider(label: "LED mip strength", key: "led_mips", range: 0 ... 4, step: 0.05, fallback: 1.3) { $0 < 0.005 ? "Off" : String(format: "%.2f", $0) }
            }
            SettingToggle(label: "OMSI's shadow meshes (under vehicles)", key: "shadow_blobs", fallback: true)
            if !traced { SettingToggle(label: "Reflection maps (paint, chrome, glass)", key: "reflections", fallback: true) }
            SettingToggle(label: "Clouds", key: "clouds", fallback: true)
            if mode.hasPrefix("enhanced") && store.flag("clouds", true) {
                SettingSelect(label: "Cloud quality", key: "cloud_quality", options: [("high", "High"), ("low", "Low")])
            }
            SettingSelect(label: "Rain quality", key: "rain_quality", options: [("high", "High"), ("medium", "Medium"), ("low", "Low")])
            SettingToggle(label: "Windy trees", key: "windy_trees", fallback: true)
        }
    }

    private var display: some View {
        Card("Display") {
            if !phone {
                SettingToggle(label: "Fullscreen", key: "fullscreen")
                SettingSelect(label: "Window size", key: "resolution", options: [("auto", "Automatic"), ("1280x720", "1280 x 720"), ("1600x900", "1600 x 900"), ("1920x1080", "1920 x 1080"), ("1920x1200", "1920 x 1200"), ("2560x1440", "2560 x 1440"), ("3840x2160", "3840 x 2160")])
                SettingToggle(label: "V-sync", key: "vsync", fallback: true)
            }
            SettingSelect(label: "Frame limit", key: "max_fps", options: phone ? [("0", "Screen refresh rate"), ("30", "30 fps"), ("45", "45 fps"), ("60", "60 fps"), ("120", "120 fps (ProMotion)")] : [("0", "Screen refresh rate"), ("30", "30 fps"), ("45", "45 fps"), ("60", "60 fps"), ("120", "120 fps"), ("144", "144 fps"), ("1000", "Unlimited")])
            SectionTitle("World & memory").padding(.top, 8)
            SettingSelect(label: "View distance", key: "view_distance", options: [("auto", "Default (1200 m)"), ("600", "600 m - fastest"), ("900", "900 m"), ("1200", "1200 m"), ("1500", "1500 m"), ("2000", "2000 m"), ("2500", "2500 m")])
            SettingSelect(label: "Object distance", key: "max_obj_dist", options: [("auto", "Automatic"), ("500", "500 m"), ("750", "750 m"), ("900", "900 m"), ("1500", "1500 m"), ("3000", "3000 m")])
            SettingSelect(label: "Small objects", key: "min_obj_size", options: [("0.005", "All"), ("0.013", "Normal"), ("0.02", "Fewer (faster)"), ("0.03", "Few (fastest)")])
            SettingSelect(label: "Mirrors", key: "mirror_size", options: [("0", "Off"), ("128", "Low (128)"), ("256", "Normal (256)"), ("512", "High (512)"), ("1024", "Very high (1024)")])
            SettingSelect(label: "Real-time reflections", key: "mirror_refresh", options: [("off", "None (frozen picture)"), ("eco", "Economical"), ("full", "Full")])
            SettingSelect(label: "Texture memory", key: "texture_memory", options: [("0", "Automatic"), ("500", "500 MB"), ("1000", "1 GB"), ("1500", "1.5 GB"), ("2000", "2 GB"), ("3000", "3 GB"), ("4000", "4 GB"), ("6000", "6 GB")])
            SettingToggle(label: "Compress textures on loading", key: "texture_compression", fallback: true)
            SettingToggle(label: "DXT/BC textures stay compressed on the GPU", key: "gpu_texture_compression", fallback: true)
            SectionTitle("Profiles").padding(.top, 8)
            GraphicsProfiles()
        }
    }

    // --- driving ---------------------------------------------------------------------------

    private var driving: some View {
        Card(phone ? "Touch & keyboard" : "Keyboard & mouse") {
            if phone {
                SettingSelect(label: "Door buttons", key: "touch_door_mode", options: [("standard", "Standard (physical doors)"), ("direct", "Direct OMSI triggers")])
            }
            SettingSelect(label: "Driving keys", key: "drive_keys", options: [("omsi", "Custom controls (Controls page)"), ("simple", "W A S D + arrows"), ("wasd", "W A S D only"), ("arrows", "Arrow keys only")])
            SettingToggle(label: "Steering linearity (keys at OMSI's steady pace)", key: "steering_linear")
            SettingToggle(label: "Old Steering (the wheel stays, turn it back yourself)", key: "old_steering")
            SettingToggle(label: "Dynamic steering (slower keys at speed, OMSI's redSteerSpd)", key: "red_steer_spd")
            if !phone {
                SettingSlider(label: "Mouse steering sensitivity (O)", key: "mouse_sens", range: 0.1 ... 3, step: 0.05, fallback: 1) { abs($0 - 1) < 0.01 ? "OMSI" : "\(Int($0 * 100))%" }
                SettingToggle(label: "Smooth mouse steering", key: "mouse_smooth")
                SettingToggle(label: "Hold the cursor while the mouse steers", key: "mouse_hold")
                SettingToggle(label: "A right click ends the mouse steering (as in OMSI)", key: "mouse_right_off")
            }
            SettingToggle(label: "Indicators cancel themselves (as the bus's script does)", key: "blinker_cancel")
            SettingToggle(label: "The keyboard brake stays on until the throttle (as in OMSI)", key: "brake_hold")
            SettingToggle(label: "Automatic clutch (manual gearboxes)", key: "auto_clutch")
            SettingToggle(label: "Automated manual gearbox", key: "auto_shift")
        }
    }

    private var controllers: some View {
        Card("Game controllers") {
            SettingToggle(label: "H-pattern shifter: return to neutral when the gear is released", key: "momentary_gears")
            SettingSlider(label: "Wheel rotation", key: "wheel_range", range: 180 ... 1800, step: 30, fallback: 900) { "\(Int($0))°" }
            SettingSlider(label: "Stick steering smoothing", key: "pad_steer_smooth", range: 0 ... 300, step: 10, fallback: 80) { "\(Int($0)) ms" }
            SettingSlider(label: "Stick steering speed", key: "pad_steer_speed", range: 0.8 ... 5, step: 0.1, fallback: 2) { String(format: "%.1f", $0) }
            SettingSlider(label: "Stick dead zone", key: "pad_deadzone", range: 0 ... 0.4, step: 0.01, fallback: 0.08) { "\(Int($0 * 100))%" }
            SettingToggle(label: "Stick steers like a wheel", key: "pad_steer_linear")
            SettingSelect(label: "Gamepad type", key: "pad_type", options: [("auto", "Automatic"), ("xbox", "Xbox"), ("ps4", "PlayStation 4"), ("ps5", "PlayStation 5")])
            SettingToggle(label: "Default gamepad buttons (indicators, doors, views, gears, menu, pause)", key: "pad_buttons", fallback: true)
            SettingToggle(label: "Force feedback and vibration", key: "ff_enabled", fallback: true)
            SettingToggle(label: "Invert force feedback", key: "ff_invert")
        }
    }

    // --- camera ----------------------------------------------------------------------------

    private var camera: some View {
        Card("Driver's view") {
            SettingSlider(label: "Head pitch", key: "seat_pitch_deg", range: -45 ... 45, step: 1, fallback: 0) { "\(Int($0))°" }
            SettingSlider(label: "Field of view", key: "fov", range: 0 ... 120, step: 1, fallback: 0) { $0 < 20 ? "Default" : "\(Int($0))°" }
            SettingSlider(label: "Mouse look sensitivity", key: "look_sens", range: 0.1 ... 2, step: 0.05, fallback: 1) { "\(Int($0 * 100))%" }
            SettingToggle(label: "Right stick turns the view", key: "right_stick_look", fallback: true)
            SettingSlider(label: "Smooth the mouse look", key: "look_smoothing_ms", range: 0 ... 200, step: 10, fallback: 0) { $0 < 1 ? "Off" : "\(Int($0)) ms" }
            SectionTitle("A head at rest").padding(.top, 8)
            SettingSlider(label: "Head sway at a standstill", key: "head_idle", range: 0 ... 1, step: 0.05, fallback: 0.35) { "\(Int($0 * 100))%" }
            SettingSlider(label: "Sway pace", key: "head_idle_pace", range: 0.5 ... 2, step: 0.05, fallback: 1) { String(format: "%.2fx", $0) }
            SettingToggle(label: "Driver's view turns with the steering", key: "steer_look", fallback: true)
            SettingToggle(label: "Head moves with the bus", key: "head_movement", fallback: true)
            SettingToggle(label: "Camera glides between viewpoints", key: "driverview_smooth", fallback: true)
            SettingToggle(label: "Driver's hands in the cab view", key: "hands_in_cab", fallback: true)
            if !phone {
                SettingToggle(label: "Right mouse button turns the view, Shift+right zooms", key: "alt_view")
                SettingToggle(label: "Precision mouse zoom", key: "precision_zoom")
            }
        }
    }

    private var outside: some View {
        Card("Outside views") {
            SettingToggle(label: "Camera collisions (outside view)", key: "camera_collision", fallback: true)
            SettingToggle(label: "Driver at the wheel (outside views)", key: "driver", fallback: true)
            if !phone {
                SectionTitle("Head tracking").padding(.top, 8)
                SettingToggle(label: "Head tracking (native TrackIR / OpenTrack)", key: "head_tracking", fallback: true)
                SettingToggle(label: "Invert yaw", key: "head_tracking_invert_yaw")
                SettingToggle(label: "Invert pitch", key: "head_tracking_invert_pitch")
                SettingToggle(label: "Invert roll", key: "head_tracking_invert_roll")
                SectionTitle("Triple screen").padding(.top, 8)
                SettingToggle(label: "Three screen projections", key: "triple_screen")
                SettingToggle(label: "Span three monitors at startup", key: "triple_span", fallback: true)
                SettingToggle(label: "HUD on centre screen", key: "triple_hud_center", fallback: true)
            }
        }
    }

    // --- sound -----------------------------------------------------------------------------

    private var sound: some View {
        Card("Volume") {
            SettingSlider(label: "Volume", key: "volume", range: 0 ... 1, step: 0.05, fallback: 0.6) { "\(Int($0 * 100))%" }
            SettingSlider(label: "Traffic", key: "vol_ai", range: 0 ... 1, step: 0.05, fallback: 1) { "\(Int($0 * 100))%" }
            SettingSlider(label: "Surroundings", key: "vol_scenery", range: 0 ... 1, step: 0.05, fallback: 1) { "\(Int($0 * 100))%" }
            SettingToggle(label: "Doppler effect", key: "doppler", fallback: true)
            SettingToggle(label: "Ambience (places, weather, nature, town)", key: "ambient", fallback: true)
            SettingSlider(label: "Ambience volume", key: "vol_ambient", range: 0 ... 1, step: 0.05, fallback: 0.8) { "\(Int($0 * 100))%" }
            SettingSelect(label: "Passenger voices", key: "pax_voices", options: [("all", "Greetings and tickets"), ("tickets", "Only the ticket asked for"), ("off", "Silent")])
        }
    }

    // --- gameplay --------------------------------------------------------------------------

    private var passengers: some View {
        Card("Passengers") {
            SettingSelect(label: "Boarding", key: "boarding", options: [("auto", "Pay and take the ticket"), ("pay", "The driver sells the ticket"), ("walk", "Just walk in")])
            SettingSelect(label: "Standing passengers", key: "standing_chance", options: [("0", "Never"), ("5", "5%"), ("10", "10%"), ("25", "25%"), ("50", "50%"), ("100", "Always")])
            SettingSelect(label: "Passenger animations", key: "passenger_animation", options: [("original", "Original OMSI"), ("enhanced", "Procedural (experimental)")])
            SettingToggle(label: "Passengers pay the exact fare", key: "exact_fare", fallback: true)
            SettingSlider(label: "How many passengers", key: "pax_density", range: 0 ... 2, step: 0.1, fallback: 1) { "\(Int($0 * 100))%" }
            SettingToggle(label: "Ability to get up (Ctrl+Shift+G)", key: "get_up")
            SectionTitle("Traffic").padding(.top, 8)
            SettingSelect(label: "Random traffic", key: "ai_unsched_factor", options: [("25", "25%"), ("50", "50%"), ("75", "75%"), ("100", "100%"), ("150", "150%"), ("200", "200%")])
            SettingSelect(label: "Timetable vehicles", key: "ai_max_scheduled", options: [("0", "All"), ("10", "At most 10"), ("25", "At most 25"), ("50", "At most 50")])
            SettingSelect(label: "Parked cars", key: "ai_max_parked", options: [("-1", "None"), ("0", "Every space"), ("35", "At most 35"), ("100", "At most 100"), ("250", "At most 250")])
            SettingToggle(label: "Timetable buses ahead of time wait only at timed stops", key: "ai_wait_timed_stops_only")
        }
    }

    private var simulation: some View {
        Card("Simulation") {
            SettingSelect(label: "Maintenance", key: "maintenance", options: [("0", "Infinite (no wear)"), ("1", "Very bad"), ("2", "Bad"), ("3", "Normal"), ("4", "Good")])
            SettingToggle(label: "Collisions with vehicles", key: "collision_vehicles", fallback: true)
            SettingToggle(label: "Collisions with objects (walls, poles)", key: "collision_objects")
            SettingToggle(label: "Collisions with people", key: "collision_pedestrians", fallback: true)
            SettingToggle(label: "Start at the real time", key: "use_real_time")
            SettingToggle(label: "Start on today's date", key: "use_real_date")
            SettingToggle(label: "Sync the clock with the real time (locks the time)", key: "time_sync")
            SettingToggle(label: "Sync the weather with METAR (locks the weather)", key: "metar_sync")
            SettingSelect(label: "Time speed", key: "time_speed", options: [("1", "Real time"), ("2", "x2"), ("4", "x4"), ("8", "x8"), ("15", "x15"), ("30", "x30")])
        }
    }

    // --- general ---------------------------------------------------------------------------

    private var interface: some View {
        Card("Interface & online") {
            if !languages.isEmpty {
                SettingSelect(label: "Language", key: "language", options: languages)
            }
            LauncherChoice()
            if !phone {
                SettingToggle(label: "The launcher rests while a game runs", key: "launcher_rest", fallback: true)
                SettingToggle(label: "Discord Rich Presence", key: "discord_status", fallback: true)
            }
            SettingSlider(label: "Game interface size", key: "ui_scale", range: 0.5 ... 2, step: 0.05, fallback: 1) { "\(Int($0 * 100))%" }
            SettingSlider(label: "Interface opacity", key: "ui_opacity", range: 0.2 ... 1, step: 0.05, fallback: 0.85) { "\(Int($0 * 100))%" }
            if !phone { SettingToggle(label: "Name of the button under the mouse", key: "tooltips", fallback: true) }
            SettingToggle(label: "Frame rate in the corner", key: "show_fps")
            SettingSelect(label: "Performance details", key: "perf_overlay", options: [("off", "Off"), ("compact", "Compact"), ("detailed", "Detailed")])
            SettingToggle(label: "Notes in the top-left corner", key: "notes", fallback: true)
            SettingToggle(label: "Chat in online games", key: "chat", fallback: true)
            SettingToggle(label: "Other players' names above their buses", key: "name_tags", fallback: true)
            SectionTitle("Navigator").padding(.top, 8)
            SettingToggle(label: "Navigator (Shift+N: map, schedule, off)", key: "navigator", fallback: true)
            SettingToggle(label: "Route arrows (as in OMSI 2)", key: "nav_arrows")
            SettingToggle(label: "AI vehicles on the map", key: "nav_ai")
        }
    }

    @ViewBuilder
    private var updates: some View {
        Card(store_build ? "Online" : "Updates") {
            if !store_build {
                SettingToggle(label: "Look for updates when the launcher starts", key: "update_check", fallback: true)
                SettingToggle(label: "Install updates without asking", key: "update_auto")
                SettingToggle(label: "Tell me about a new version during a session", key: "update_notify", fallback: true)
            }
            SettingToggle(label: "Count me in the website's \"playing now\" (anonymous)", key: "presence", fallback: true)
            HStack {
                Button("Export diagnostics") {
                    if let path = (try? JSONSerialization.jsonObject(with: Bridge.call("act", ["do": "diagnostics"]), options: [.fragmentsAllowed])) as? String {
                        store.say(String(localized: "Support package saved: \(path)"), error: false)
                        Platform.reveal(path)
                    }
                }
                .buttonStyle(.secondary)
                Spacer()
            }
            Text("openOMSI \(store.version) (\(store.build))").font(.inter(12)).foregroundStyle(Theme.muted)
        }
    }
}

/// "Cross-Platform Launcher": the launcher the game draws, as on Windows, Linux and Android,
/// in place of this one.
struct LauncherChoice: View {
    @Environment(LauncherStore.self) private var store

    var body: some View {
        LabeledRow("Cross-Platform Launcher", detail: String(localized: "The launcher of the other systems, drawn by the game, in place of this one")) {
            Toggle("", isOn: Binding(get: { store.flag("cross_platform_launcher") }, set: { on in
                store.set("cross_platform_launcher", on)
                store.act("save_settings")
                Platform.setCrossPlatform(on)
            }))
            .labelsHidden()
            .toggleStyle(.launcher)
        }
    }
}

/// The quality presets of a graphics mode (`graphics_presets_for`): the one the settings
/// match now, else "Custom".
struct PresetRow: View {
    @Environment(LauncherStore.self) private var store
    var mode: String
    @State private var presets: [(String, [String: JSON])] = []
    @State private var loadedFor = ""

    var body: some View {
        LabeledRow("Quality preset") {
            SelectField(options: presets.map { (value: $0.0, label: $0.0) } + [(value: "custom", label: String(localized: "Custom"))], selection: current, onChange: { name in
                if let p = presets.first(where: { $0.0 == name }) {
                    store.act("settings", ["values": p.1.mapValues(\.any)])
                }
            })
        }
        .onAppear(perform: load)
        .onChange(of: mode) { _, _ in load() }
    }

    private var current: String {
        presets.first { p in p.1.allSatisfy { k, v in matches(store.setting(k), v) } }?.0 ?? "custom"
    }

    private func matches(_ a: JSON?, _ b: JSON) -> Bool {
        guard let a else { return false }
        if a == b { return true }
        if let x = a.double, let y = b.double { return abs(x - y) < 1e-6 }
        return a.string == b.string
    }

    private func load() {
        guard loadedFor != mode else { return }
        loadedFor = mode
        struct P: Decodable { var name: String; var values: [String: JSON] }
        presets = ((try? Bridge.get([P].self, "graphics_presets", ["mode": mode])) ?? []).map { ($0.name, $0.values) }
    }
}

/// The graphics settings saved under a name, loaded again in one go.
struct GraphicsProfiles: View {
    @Environment(LauncherStore.self) private var store
    @State private var names: [String] = []
    @State private var selected = ""
    @State private var name = ""

    var body: some View {
        VStack(spacing: 8) {
            LabeledRow("Saved profile") {
                SelectField(options: names.isEmpty ? [(value: "", label: String(localized: "No saved profiles"))] : names.map { (value: $0, label: $0) }, selection: selected, onChange: { selected = $0; name = $0 })
            }
            HStack {
                Button { store.act("graphics_profile_load", ["name": selected]) } label: { Label("Load", systemImage: "square.and.arrow.down") }
                    .buttonStyle(.launcher(.plain, wide: true)).disabled(selected.isEmpty)
                Button { store.act("graphics_profile_delete", ["name": selected]); reload() } label: { Label("Delete", systemImage: "trash") }
                    .buttonStyle(.launcher(.danger, wide: true)).disabled(selected.isEmpty)
            }
            HStack {
                InputField(prompt: "Profile name", text: $name)
                Button { store.act("graphics_profile_save", ["name": name]); reload() } label: { Label("Save", systemImage: "square.and.arrow.down.on.square") }
                    .buttonStyle(.primary).disabled(name.trimmingCharacters(in: .whitespaces).isEmpty)
            }
        }
        .onAppear(perform: reload)
    }

    private func reload() {
        names = ((try? Bridge.get([String: JSON].self, "graphics_profiles")) ?? [:]).keys.sorted()
        if !names.contains(selected) { selected = names.first ?? "" }
    }
}
