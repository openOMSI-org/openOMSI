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
    @State private var confirmReset = false

    private var phone: Bool { store.platform == "ios" }
    private var store_build: Bool { store.storeBuild }

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
        case "sound": radio
        case "gameplay": simulation
        default: updates
        }
    }

    private func patch(_ values: [String: Any]) {
        store.act("settings", ["values": values])
    }

    private func openControls() {
        store.page = .controls
        store.phoneTab = "more"
        store.phoneMore = .controls
    }

    private func button(_ label: LocalizedStringKey, _ icon: String, _ action: @escaping () -> Void) -> some View {
        HStack {
            Button(action: action) { Label(label, systemImage: icon) }.buttonStyle(.secondary)
            Spacer()
        }
    }

    private func note(_ text: LocalizedStringKey) -> some View {
        Text(text).font(.inter(12.5)).foregroundStyle(Theme.muted).fixedSize(horizontal: false, vertical: true)
    }

    // --- graphics --------------------------------------------------------------------------

    private var graphics: some View {
        let mode = store.setting("graphics")?.string ?? "vanilla_plus"
        let classic = mode == "vanilla"
        let enhanced = mode.hasPrefix("enhanced")
        // (Real-Time Ray Tracing is a switch of Enhanced; a file of the days of Enhanced+ names it as a mode)
        let traced = enhanced && (mode == "enhanced_plus" || store.flag("ray_tracing", false))
        return Card("Graphics") {
            SettingSelect(label: "Graphics", key: "graphics", options: [("vanilla", "Vanilla (as OMSI 2)"), ("vanilla_plus", "Vanilla+"), ("enhanced", "Enhanced")])
            if enhanced {
                SettingToggle(label: "Real-Time Ray Tracing", key: "ray_tracing", fallback: false)
            }
            PresetRow(mode: mode)
            OptimiseBlock()
            SettingSelect(label: "Anti-aliasing", key: "msaa", options: Choices.msaa, heavy: { $0 == "8" && !traced })
            SettingSelect(label: "Upscaling", key: "upscaler", options: [("default", "Standard (Catmull-Rom)"), ("fsr1", "AMD FSR 1 (experimental)")])
            if store.setting("upscaler")?.string == "fsr1" {
                SettingSelect(label: "FSR quality", key: "fsr_quality", options: [("ultra", "Ultra Quality (77%)"), ("quality", "Quality (67%)"), ("balanced", "Balanced (59%)"), ("performance", "Performance (50%)"), ("auto", "Automatic (keeps the frame rate)"), ("custom", "Use render scale")])
                SettingSlider(label: "FSR sharpness", key: "fsr_sharpness", range: 0 ... 1, step: 0.05, fallback: 0.8) { "\(Int($0 * 100))%" }
            }
            SettingSelect(label: "Render scale", key: "render_scale", options: Choices.renderScale)
            SettingSelect(label: "Anisotropic", key: "anisotropy", options: [("1", "Off"), ("2", "2x"), ("4", "4x"), ("8", "8x"), ("16", "16x")])
            if !classic {
                SettingSelect(label: "Shadow map", key: "shadow_size", options: Choices.shadowSize)
                if !traced {
                    SettingToggle(label: "Ambient occlusion", key: "ssao", fallback: true)
                    SettingToggle(label: "Sun shadows", key: "shadows", fallback: true)
                }
                SettingSelect(label: "Shadows cast by", key: "shadow_casters", options: [("all", "Every solid mesh"), ("omsi", "[shadow] meshes, as OMSI")])
                SettingToggle(label: "Detail texturing up close", key: "detail_textures", fallback: true)
            }
            if enhanced {
                SettingSlider(label: "Night brightness", key: "night_brightness", range: 0 ... 3, step: 0.25, fallback: 0) { $0 < 0.01 ? off : String(format: "+%.2f", $0) }
                SettingSlider(label: "LED glow", key: "led_glow", range: 0 ... 15, step: 1, fallback: 6) { $0 < 0.5 ? off : "\(Int($0))" }
                SettingSlider(label: "LED mip strength", key: "led_mips", range: 0 ... 4, step: 0.05, fallback: 1.3) { $0 < 0.005 ? off : String(format: "%.2f", $0) }
            }
            SettingToggle(label: "OMSI's shadow meshes (under vehicles)", key: "shadow_blobs", fallback: true)
            if !traced { SettingSlider(label: "Reflective objects reflect", key: "reflections", range: 0 ... 1, step: 0.05, fallback: 1) { $0 < 0.005 ? off : pct($0) } }
            SettingToggle(label: "Clouds", key: "clouds", fallback: true)
            if enhanced && store.flag("clouds", true) {
                SettingSelect(label: "Cloud quality", key: "cloud_quality", options: Choices.cloudQuality)
            }
            SettingSelect(label: "Rain quality", key: "rain_quality", options: Choices.rainQuality)
            if enhanced && !traced {
                SettingSelect(label: "Street light", key: "street_light", options: [("auto", "Auto"), ("on", "On"), ("off", "Off")])
            }
            if enhanced {
                SettingSelect(label: "Light has shadows", key: "light_shadows", options: [("off", "Off"), ("street", "Street lamps and vehicles"), ("all", "Every light")])
                SettingToggle(label: "Cloud shadows and light shafts", key: "cloud_shadows", fallback: true)
                SettingToggle(label: "Glow of lights", key: "light_glow", fallback: true)
            }
            SettingSelect(label: "Vehicle smoke", key: "vehicle_smoke", options: [("all", "All vehicles"), ("player", "Player only"), ("off", "Off")], help: "Controls the vehicles' smoke particles, not scenery effects or glowing particles.")
            if (store.setting("reflections")?.double ?? 1) > 0.005 {
                SettingSelect(label: enhanced ? "Real-Time Reflections" : "Puddles", key: "puddles", options: [("high", "High"), ("low", "Low"), ("off", "Off")])
            }
            SettingToggle(label: "Windy trees", key: "windy_trees", fallback: true)
        }
    }

    private var display: some View {
        let enhanced = (store.setting("graphics")?.string ?? "vanilla_plus").hasPrefix("enhanced")
        let mirrors = store.setting("mirror_size")?.string ?? "256"
        return Card("Display") {
            if !phone {
                SettingToggle(label: "Fullscreen", key: "fullscreen")
                SettingSelect(label: "Window size", key: "resolution", options: [("auto", "Automatic"), ("1280x720", "1280 x 720"), ("1280x800", "1280 x 800 (Steam Deck)"), ("1366x768", "1366 x 768"), ("1600x900", "1600 x 900"), ("1920x1080", "1920 x 1080"), ("1920x1200", "1920 x 1200"), ("2560x1440", "2560 x 1440"), ("3840x2160", "3840 x 2160")])
                SettingToggle(label: "V-sync", key: "vsync", fallback: true)
            }
            SettingToggle(label: "Simulation on its own thread", key: "sim_thread")
            SettingSelect(label: "Frame limit", key: "max_fps", options: phone ? [("0", "Screen refresh rate"), ("30", "30 fps"), ("45", "45 fps"), ("60", "60 fps"), ("120", "120 fps (ProMotion)")] : Choices.maxFps)
            SectionTitle("World & memory").padding(.top, 8)
            SettingSelect(label: "View distance", key: "view_distance", options: Choices.viewDistance, heavy: { (Double($0) ?? 0) >= 2000 })
            SettingSelect(label: "Object distance", key: "max_obj_dist", options: Choices.objectDistance, heavy: { (Double($0) ?? 0) >= 1500 })
            SettingSelect(label: "Small objects", key: "min_obj_size", options: Choices.smallObjects)
            SettingSelect(label: "Mirrors", key: "mirror_size", options: Choices.mirrorSize)
            SettingSelect(label: "Bus mirror refresh", key: "mirror_refresh", options: Choices.mirrorRefresh, fallback: "full", heavy: { $0 == "every" && mirrors != "0" })
            if enhanced {
                SettingSelect(label: "Mirror quality", key: "mirror_quality", options: [("standard", "Standard"), ("enhanced", "As the picture (Enhanced)")])
            }
            SettingToggle(label: "Mirror panels every frame", key: "mirror_hud_every_frame")
            SettingSelect(label: "Video memory", key: "video_memory", options: [("0", "Automatic"), ("1000", "1 GB"), ("1500", "1.5 GB"), ("2000", "2 GB"), ("3000", "3 GB"), ("4000", "4 GB"), ("6000", "6 GB"), ("8000", "8 GB"), ("12000", "12 GB"), ("16000", "16 GB")], help: "Everything the game keeps on the graphics card: textures, models and the picture itself. Automatic is nine tenths of the card.")
            SettingToggle(label: "Compress textures on loading", key: "texture_compression", fallback: true)
            SettingToggle(label: "DXT/BC textures stay compressed on the GPU", key: "gpu_texture_compression", fallback: true)
            SectionTitle("Profiles").padding(.top, 8)
            GraphicsProfiles()
        }
    }

    // --- driving ---------------------------------------------------------------------------

    private var driving: some View {
        Card("Keyboard & mouse") {
            if phone {
                SectionTitle("Touch controls")
                SettingSelect(label: "Door buttons", key: "touch_door_mode", options: [("standard", "Standard (physical doors)"), ("direct", "Direct OMSI triggers")])
                SettingSlider(label: "Pedal length", key: "touch_pedal_length", range: 0.8 ... 1.6, step: 0.05, fallback: 1, round: 0.01) { pct($0) }
                SettingSlider(label: "Pedal rest zone (a finger at the foot presses nothing)", key: "touch_pedal_deadzone", range: 0 ... 0.4, step: 0.01, fallback: 0.15, round: 0.01) { pct($0) }
                SectionTitle("Keyboard & mouse").padding(.top, 8)
            }
            SettingSelect(label: "Driving keys", key: "drive_keys", options: [("omsi", "Custom controls (Controls page)"), ("simple", "W A S D + arrows"), ("wasd", "W A S D only"), ("arrows", "Arrow keys only")], fallback: "simple")
            SettingToggle(label: "Steering linearity (keys at OMSI's steady pace)", key: "steering_linear")
            SettingToggle(label: "Old Steering (the wheel stays, turn it back yourself)", key: "old_steering")
            SettingToggle(label: "Dynamic steering (slower keys at speed, OMSI's redSteerSpd)", key: "red_steer_spd")
            if !phone {
                SettingSlider(label: "Mouse steering sensitivity (O)", key: "mouse_sens", range: 0.1 ... 3, step: 0.05, fallback: 1, round: 0.01) { omsiPct($0) }
                SettingSlider(label: "Mouse pedal strength", key: "mouse_pedal_strength", range: 0.5 ... 2, step: 0.05, fallback: 1, round: 0.01) { omsiPct($0) }
                SettingToggle(label: "Smooth mouse steering (off: the wheel follows the cursor at once, as in OMSI)", key: "mouse_smooth", fallback: true)
                SettingToggle(label: "The mouse works the pedals", key: "mouse_pedals", fallback: true, help: "Up the throttle, down the brake, as in OMSI; off: the mouse only steers, the pedals stay on the keys")
                SettingToggle(label: "Hold the cursor while the mouse steers (off: the crosshair stays free, the window's edges are the lock)", key: "mouse_hold", fallback: true)
                SettingToggle(label: "A right click ends the mouse steering (as in OMSI)", key: "mouse_right_off")
            }
            SettingToggle(label: "Indicators cancel themselves (as the bus's script does)", key: "blinker_cancel", fallback: true)
            SettingToggle(label: "The keyboard brake stays on until the throttle (as in OMSI)", key: "brake_hold", fallback: true)
            SettingSlider(label: "Pedal keys speed (100 % = OMSI)", key: "key_pedal_speed", range: 0.25 ... 2, step: 0.05, fallback: 1, round: 0.01) { omsiPct($0) }
            SettingToggle(label: "Automatic clutch (manual gearboxes)", key: "auto_clutch", fallback: true)
            SettingToggle(label: "Automated manual gearbox (shifts a manual gearbox for you by the engine speed)", key: "auto_shift")
            button("Change the keys", "keyboard", openControls)
        }
    }

    private var controllers: some View {
        Card("Game controllers") {
            if !phone { button("Set up a wheel or pedals", "gamecontroller", openControls) }
            SectionTitle("Wheel and pedals")
            SettingSlider(label: "Wheel rotation", key: "wheel_range", range: 180 ... 1800, step: 30, fallback: 900, round: 1) { deg($0) }
            if !phone {
                SettingSlider(label: "Full lock at", key: "wheel_lock", range: 0 ... 1800, step: 30, fallback: 0, round: 1, zeroBelow: 45) { $0 < 45 ? "OMSI" : deg($0) }
                SettingSlider(label: "Throttle pedal strength", key: "pedal_throttle", range: 0.5 ... 2, step: 0.05, fallback: 1, round: 0.01) { pedal($0) }
                SettingSlider(label: "Brake pedal strength", key: "pedal_brake", range: 0.5 ... 2, step: 0.05, fallback: 1, round: 0.01) { pedal($0) }
            }
            SettingToggle(label: "H-pattern shifter: return to neutral when the gear is released", key: "momentary_gears")
            if !phone {
                SettingToggle(label: "Arrow keys switch the cameras with a wheel too (no glance)", key: "arrows_switch_cams", fallback: true)
                SettingToggle(label: "Keep driving in the background", key: "ctrl_background")
            }
            SectionTitle("Gamepad").padding(.top, 8)
            SettingSelect(label: "Gamepad type", key: "pad_type", options: [("auto", "Automatic"), ("xbox", "Xbox"), ("ps4", "PlayStation 4"), ("ps5", "PlayStation 5")])
            SettingToggle(label: "Default gamepad buttons (indicators, doors, views, gears, menu, pause)", key: "pad_buttons", fallback: true)
            SettingSlider(label: "Stick steering speed", key: "pad_steer_speed", range: 0.8 ... 5, step: 0.1, fallback: 2, round: 0.1) { String(format: "%.1f s", $0) }
            SettingSlider(label: "Stick steering smoothing", key: "pad_steer_smooth", range: 0 ... 300, step: 10, fallback: 120, round: 1) { offMs($0) }
            SettingSlider(label: "Stick dead zone", key: "pad_deadzone", range: 0 ... 0.4, step: 0.01, fallback: 0.08, round: 0.01) { String(format: "%.0f %%", $0 * 100) }
            SettingToggle(label: "Stick steers like a wheel (a wheel seen as a gamepad)", key: "pad_steer_linear")
            SectionTitle("Force feedback").padding(.top, 8)
            SettingToggle(label: "Force feedback and vibration", key: "ff_enabled", fallback: true)
            SettingToggle(label: "Invert force feedback by default", key: "ff_invert")
            if !phone { note("Wheels with a saved direction use their own setting under Controls → Game controllers.") }
            SettingSlider(label: "Road texture vibration", key: "ff_road_vib", range: 0 ... 4, step: 0.05, fallback: 1) { vibration($0) }
            SettingSlider(label: "Engine vibration", key: "ff_engine_vib", range: 0 ... 4, step: 0.05, fallback: 1) { vibration($0) }
            SettingSlider(label: "Vibration fade-out", key: "ff_fade", range: 0 ... 1.5, step: 0.05, fallback: 0.28, round: 0.01) { $0 < 0.01 ? off : "\(Int(($0 * 1000).rounded())) ms" }
            button("Reset wheel settings", "arrow.counterclockwise") {
                patch(["wheel_range": 900, "wheel_lock": 0, "ff_invert": false, "ff_enabled": true, "ff_road_vib": 1, "ff_engine_vib": 1, "ff_fade": 0.28])
            }
        }
    }

    // --- camera ----------------------------------------------------------------------------

    private var camera: some View {
        let fovKey = !phone && store.flag("triple_screen") ? "triple_fov_deg" : "fov"
        return Card("Driver's view") {
            SectionTitle("Seat position")
            SettingSlider(label: "Seat forward / back", key: "seat_y", range: -0.6 ... 0.6, step: 0.01, fallback: 0, round: 0.01) { signed($0 * 100, " cm") }
            SettingSlider(label: "Seat up / down", key: "seat_z", range: -0.6 ... 0.6, step: 0.01, fallback: 0, round: 0.01) { signed($0 * 100, " cm") }
            SettingSlider(label: "Seat right / left", key: "seat_x", range: -0.6 ... 0.6, step: 0.01, fallback: 0, round: 0.01) { signed($0 * 100, " cm") }
            SettingSlider(label: "Head pitch", key: "seat_pitch_deg", range: -45 ... 45, step: 1, fallback: 0, round: 1) { signed($0, "°") }
            button("Reset the seat position", "arrow.counterclockwise") { patch(["seat_x": 0, "seat_y": 0, "seat_z": 0, "seat_pitch_deg": 0]) }
            SettingSlider(label: "Field of view", key: fovKey, range: 0 ... 120, step: 1, fallback: 0, round: 1, zeroBelow: 20) { $0 < 20 ? String(localized: "Default") : deg($0) }
                .id(fovKey)
            SettingSlider(label: "Mouse look sensitivity", key: "look_sens", range: 0.1 ... 2, step: 0.05, fallback: 1, round: 0.01) { omsiPct($0) }
            SettingToggle(label: "Right stick turns the view", key: "right_stick_look", fallback: true)
            SettingSlider(label: "Smooth the mouse look", key: "look_smoothing_ms", range: 0 ... 200, step: 10, fallback: 0, round: 1) { offMs($0) }
            SectionTitle("A head at rest").padding(.top, 8)
            SettingSlider(label: "Head sway at a standstill", key: "head_idle", range: 0 ... 1, step: 0.05, fallback: 0, round: 0.01) { $0 <= 0 ? off : pct($0) }
            SettingSlider(label: "Sway pace", key: "head_idle_pace", range: 0.5 ... 2, step: 0.05, fallback: 1, round: 0.01) { pct($0) }
            SettingToggle(label: "Driver's view turns with the steering", key: "steer_look")
            SettingSlider(label: "Steering view angle", key: "steer_look_angle", range: 0 ... 60, step: 1, fallback: 30) { deg($0) }
            SettingSlider(label: "Steering view response", key: "steer_look_response", range: 0.05 ... 1, step: 0.05, fallback: 0.25) { String(format: "%.0f ms", $0 * 1000) }
            SettingToggle(label: "Head moves with the bus", key: "head_movement", fallback: true)
            SettingToggle(label: "Camera glides between viewpoints", key: "driverview_smooth", fallback: true)
            SettingToggle(label: "Driver's hands in the cab view", key: "hands_in_cab")
            if !phone {
                SettingToggle(label: "Right mouse button turns the view, Shift+right zooms (off: right zooms as in OMSI, the wheel button turns)", key: "alt_view", fallback: true)
            }
            SettingSelect(label: "Mirror panels on the screen", key: "mirror_hud", options: [("0", "Off"), ("2", "Left mirror"), ("1", "Right mirror"), ("3", "Left and right"), ("7", "Left, right and front")])
            if !phone {
                SettingToggle(label: "Precision mouse zoom (FOV curve instead of the linear way)", key: "precision_zoom")
            }
        }
    }

    private static let trackAxes = ["yaw", "pitch", "roll", "x", "y", "z"]

    private func trackSlider(_ axis: String, _ label: LocalizedStringKey) -> some View {
        SettingSlider(label: label, key: "head_tracking_\(axis)_sens", range: 0 ... 100, step: 1, fallback: 100, round: 1) { $0 <= 0 ? off : "\(Int($0))%" }
    }

    private var outside: some View {
        Card("Outside views") {
            SettingToggle(label: "Camera collisions (outside view)", key: "camera_collision", fallback: true)
            SettingToggle(label: "Driver at the wheel (outside views)", key: "driver", fallback: true)
            if !phone {
                SectionTitle("Head tracking").padding(.top, 8)
                SettingToggle(label: "Head tracking (native TrackIR / OpenTrack)", key: "head_tracking")
                SectionTitle("Rotation").padding(.top, 8)
                trackSlider("yaw", "Yaw sensitivity (left / right)")
                trackSlider("pitch", "Pitch sensitivity (up / down)")
                trackSlider("roll", "Roll sensitivity")
                SettingToggle(label: "Invert yaw", key: "head_tracking_invert_yaw")
                SettingToggle(label: "Invert pitch", key: "head_tracking_invert_pitch")
                SettingToggle(label: "Invert roll", key: "head_tracking_invert_roll")
                SectionTitle("Position").padding(.top, 8)
                trackSlider("x", "X sensitivity (left / right)")
                trackSlider("y", "Y sensitivity (up / down)")
                trackSlider("z", "Z sensitivity (forward / back)")
                SettingToggle(label: "Invert X", key: "head_tracking_invert_x")
                SettingToggle(label: "Invert Y", key: "head_tracking_invert_y")
                SettingToggle(label: "Invert Z", key: "head_tracking_invert_z")
                button("Reset head-tracking axes", "arrow.counterclockwise") {
                    var v: [String: Any] = [:]
                    for a in Self.trackAxes {
                        v["head_tracking_\(a)_sens"] = 15
                        v["head_tracking_invert_\(a)"] = false
                    }
                    patch(v)
                }
                SectionTitle("Triple screen").padding(.top, 8)
                SettingToggle(label: "Three screen projections", key: "triple_screen")
                SettingToggle(label: "Span three monitors at startup", key: "triple_span", fallback: true)
                SettingToggle(label: "HUD on centre screen", key: "triple_hud_center", fallback: true)
                if store.flag("triple_screen") {
                    note("Three equal screens in a horizontal row. OpenXR takes priority.")
                    SettingSlider(label: "Visible width of one panel", key: "triple_width_mm", range: 200 ... 2000, step: 10, fallback: 600) { mm($0) }
                    SettingSlider(label: "Eye to centre screen", key: "triple_distance_mm", range: 200 ... 3000, step: 10, fallback: 650, also: ["triple_fov_deg": 0]) { mm($0) }
                    SettingSlider(label: "Both frames at each join", key: "triple_bezel_mm", range: 0 ... 100, step: 1, fallback: 0) { mm($0) }
                    SettingSlider(label: "Left screen inward angle", key: "triple_left_angle_deg", range: 0 ... 90, step: 1, fallback: 45) { deg($0) }
                    SettingSlider(label: "Right screen inward angle", key: "triple_right_angle_deg", range: 0 ... 90, step: 1, fallback: 45) { deg($0) }
                    SettingSlider(label: "Eye above screen centre", key: "triple_eye_height_mm", range: -500 ... 500, step: 1, fallback: 0) { mm($0) }
                }
            }
        }
    }

    // --- sound -----------------------------------------------------------------------------

    private func volume(_ label: LocalizedStringKey, _ key: String, _ fallback: Double) -> some View {
        SettingSlider(label: label, key: key, range: 0 ... 1, step: 0.05, fallback: fallback, round: 0.01) { pct($0) }
    }

    private var sound: some View {
        Card("Volume") {
            volume("Volume", "volume", 0.6)
            volume("Traffic", "vol_ai", 1)
            volume("Surroundings", "vol_scenery", 1)
            SettingToggle(label: "Doppler effect", key: "doppler", fallback: true)
            SettingToggle(label: "Ambience (places, weather, nature, town)", key: "ambient", fallback: true)
            volume("Ambience volume", "vol_ambient", 0.8)
            SettingSelect(label: "Passenger voices", key: "pax_voices", options: [("all", "Greetings and tickets"), ("tickets", "Only the ticket asked for"), ("off", "Silent")])
            volume("Radio: small speaker", "radio_speaker", 0)
            volume("Radio: FM hiss", "radio_hiss", 0)
            volume("Radio: signal dropouts", "radio_dropouts", 0)
        }
    }

    private var radio: some View {
        Card("Radio stations") {
            note("A radio's station button n plays the n-th station, a cassette player the first; Shift+R steps through them. An address is an MP3, AAC or Ogg stream or an .m3u/.pls playlist.")
            RadioStations()
        }
    }

    // --- gameplay --------------------------------------------------------------------------

    private var passengers: some View {
        Card("Passengers") {
            SettingSelect(label: "Boarding", key: "boarding", options: [("auto", "Pay and take the ticket"), ("pay", "The driver sells the ticket"), ("walk", "Just walk in")])
            SettingSelect(label: "Standing passengers", key: "standing_chance", options: [("0", "Never"), ("5", "5%"), ("10", "10%"), ("25", "25%"), ("50", "50%"), ("100", "Always")], fallback: "5")
            SettingSelect(label: "Passenger animations", key: "passenger_animation", options: [("original", "Original OMSI"), ("enhanced", "Natural (procedural)")])
            SettingSelect(label: "Character models", key: "character_models", options: [("original", "Original"), ("improved", "High quality (improved)")], help: "High quality draws the people with the Microsoft Rocketbox avatars and their living faces, on this computer only: servers and other players keep their own. They are downloaded once (about 310 MB).")
            SettingToggle(label: "Passengers pay the exact fare", key: "exact_fare", fallback: true)
            SettingToggle(label: "Passengers react to bad driving", key: "pax_bad_driving", fallback: true)
            SettingSlider(label: "How many passengers", key: "pax_density", range: 0 ... 2, step: 0.1, fallback: 1, round: 0.01) { pct($0) }
            SettingToggle(label: "Ability to get up (Ctrl+Shift+G)", key: "get_up")
            SettingToggle(label: "Start with a full tank", key: "full_tank", fallback: true)
            SettingToggle(label: "Trip summary", key: "trip_summary", fallback: true)
            SectionTitle("Traffic").padding(.top, 8)
            SettingSelect(label: "Random traffic", key: "ai_unsched_factor", options: [("25", "25%"), ("50", "50%"), ("75", "75%"), ("100", "100%"), ("150", "150%"), ("200", "200%")], fallback: "100")
            SettingSelect(label: "Timetable vehicles", key: "ai_max_scheduled", options: [("0", "All"), ("10", "At most 10"), ("25", "At most 25"), ("50", "At most 50")])
            SettingSelect(label: "Parked cars", key: "ai_max_parked", options: [("-1", "None"), ("0", "Every space"), ("35", "At most 35"), ("100", "At most 100"), ("250", "At most 250")], fallback: "0")
            SettingSelect(label: "People at once", key: "ai_max_humans", options: [("50", "50"), ("100", "100"), ("200", "200"), ("400", "400"), ("720", "720"), ("1000", "1000")], fallback: "200")
            SettingToggle(label: "Timetable buses ahead of time wait only at timed stops", key: "ai_wait_timed_stops_only")
        }
    }

    private var simulation: some View {
        Card("Simulation") {
            SettingSelect(label: "Maintenance", key: "maintenance", options: [("0", "Infinite (no wear)"), ("1", "Very bad"), ("2", "Bad"), ("3", "Normal"), ("4", "Good")])
            SettingToggle(label: "Collisions with vehicles", key: "collision_vehicles", fallback: true)
            SettingToggle(label: "Collisions with objects (walls, poles)", key: "collision_objects", fallback: true)
            SettingToggle(label: "Collisions with people", key: "collision_pedestrians", fallback: true)
            SettingToggle(label: "Start at the real time", key: "use_real_time")
            SettingToggle(label: "Start on today's date", key: "use_real_date")
            SettingToggle(label: "Sync the clock with the real time (locks the time)", key: "time_sync")
            SettingToggle(label: "Sync the weather with METAR (locks the weather)", key: "metar_sync")
            SettingSelect(label: "Time speed (not in multiplayer or with the real-time sync)", key: "time_speed", options: [("1", "Real time"), ("2", "x2"), ("4", "x4"), ("8", "x8"), ("15", "x15"), ("30", "x30")])
        }
    }

    // --- general ---------------------------------------------------------------------------

    private var interface: some View {
        Card("Interface & online") {
            if !languages.isEmpty {
                SettingSelect(label: "Language", key: "language", options: languages)
            }
            if !phone {
                SettingToggle(label: "Translate the remaining texts automatically (offline, downloads 620 MB once)", key: "machine_translation")
                SettingToggle(label: "The launcher rests while a game runs (gives the graphics card to the game)", key: "launcher_rest", fallback: true)
                SettingToggle(label: "Discord Rich Presence", key: "discord_status", fallback: true, help: "Shows the launcher or your map, bus, line and multiplayer status in Discord.")
                SettingToggle(label: "Voice chat in multiplayer", key: "voice_enabled", fallback: false, help: "Hold the nearby (V) or radio (B) key to talk. The microphone is used only while a key is held; nothing is recorded")
            }
            SettingSlider(label: "Game interface size", key: "ui_scale", range: 0.5 ... 2, step: 0.05, fallback: 1, round: 0.01) { pct($0) }
            SettingToggle(label: "Interface grows with the window", key: "ui_scale_window", fallback: true)
            SettingSlider(label: "Interface opacity", key: "ui_opacity", range: 0.2 ... 1, step: 0.05, fallback: 0.85, round: 0.01) { pct($0) }
            if !phone { SettingToggle(label: "Name of the button under the mouse", key: "tooltips", fallback: true) }
            SettingToggle(label: "Frame rate in the corner", key: "show_fps")
            SettingSelect(label: "Performance details", key: "perf_overlay", options: [("off", "Off"), ("compact", "Compact"), ("detailed", "Detailed")])
            SettingToggle(label: "Notes in the top-left corner", key: "notes", fallback: true)
            SettingToggle(label: "Chat in online games", key: "chat", fallback: true)
            SettingSlider(label: "Chat size", key: "chat_size", range: 0.5 ... 3, step: 0.1, fallback: 1, round: 0.1) { pct($0) }
            SettingToggle(label: "Other players' names above their buses", key: "name_tags", fallback: true)
            SettingSelect(label: "Photo mode", key: "photo_mode", options: [("simple", "Simple"), ("advanced", "Advanced (for photographers)")], help: "Simple: the lens, the look and the frame. Advanced: a camera body - sensor size, P/A/S/M exposure with ISO and metering, white balance in kelvin, tilt-shift, histogram, focus peaking, bracketing and 16-bit PNG.")
            SectionTitle("Navigator").padding(.top, 8)
            SettingToggle(label: "Navigator (Shift+N: map, schedule, off)", key: "navigator", fallback: true)
            SettingToggle(label: "Route arrows (as in OMSI 2)", key: "nav_arrows")
            SettingToggle(label: "AI vehicles on the map", key: "nav_ai", fallback: true)
            SettingToggle(label: "Map surface under the roads", key: "nav_surface", fallback: true)
            SettingToggle(label: "Light map", key: "nav_light")
            SettingToggle(label: "3D buildings on the map", key: "nav_3d", fallback: true)
            SettingToggle(label: "Navigator as a widget (drag it anywhere, size it at its corner)", key: "nav_widget", fallback: true)
            SettingSlider(label: "Navigator size", key: "nav_size", range: 0.6 ... 2, step: 0.05, fallback: 1) { String(format: "%.0f %%", $0 * 100) }
            CornerRow(fallback: phone ? "top-center" : "bottom-left", phone: phone)
        }
    }

    @ViewBuilder
    private var updates: some View {
        Card(store_build ? "Online" : "Updates") {
            if !store_build {
                SettingToggle(label: "Look for updates when the launcher starts", key: "update_check", fallback: true)
                SettingSelect(label: "Update channel", key: "update_channel", options: [("stable", "Stable"), ("beta", "Beta (for testers)")], help: "Beta: every new version as soon as it is built, before it is tested. Back on Stable, the beta you have stays until a newer stable version comes out.")
                SettingToggle(label: "Install updates without asking", key: "update_auto")
                SettingToggle(label: "Tell me about a new version during a session", key: "update_notify", fallback: true)
            }
            SettingToggle(label: "Count me in the website's \"playing now\" (anonymous)", key: "presence", fallback: true)
            Text("openOMSI \(store.version) (\(store.build))").font(.inter(12)).foregroundStyle(Theme.muted)
            SectionTitle("Reset").padding(.top, 8)
            HStack {
                Button { confirmReset = true } label: { Label("Reset all settings...", systemImage: "arrow.counterclockwise") }
                    .buttonStyle(.destructive)
                    .confirmationDialog("Reset every setting?", isPresented: $confirmReset) {
                        Button("Reset", role: .destructive, action: resetAll)
                    } message: {
                        Text("Graphics, sound, controllers and game settings go back to how they came. The language, the drivers, the key bindings and the game folder stay.")
                    }
                Spacer()
            }
            SectionTitle("Support").padding(.top, 8)
            HStack {
                Button("Export diagnostics") {
                    if let path = (try? JSONSerialization.jsonObject(with: Bridge.call("act", ["do": "diagnostics"]), options: [.fragmentsAllowed])) as? String {
                        store.say(tr("Support package saved: {path}", ["path": path]), error: false)
                        Platform.reveal(path)
                    }
                }
                .buttonStyle(.secondary)
                Spacer()
            }
        }
    }

    private func resetAll() {
        do {
            guard var defaults = try JSONSerialization.jsonObject(with: Bridge.api("settings_defaults")) as? [String: Any] else { return }
            if let l = store.setting("language") { defaults["language"] = l.any }
            store.act("settings", ["values": defaults, "replace": true])
            store.act("save_settings")
            store.say(String(localized: "Every setting is back to how it came."), error: false)
        } catch {
            store.say(error.localizedDescription, error: true)
        }
    }
}

private var off: String { String(localized: "Off") }
private func pct(_ v: Double) -> String { String(format: "%.0f%%", v * 100) }
private func omsiPct(_ v: Double) -> String { abs(v - 1) < 0.01 ? "OMSI" : pct(v) }
private func offMs(_ v: Double) -> String { v <= 0 ? off : String(format: "%.0f ms", v) }
private func deg(_ v: Double) -> String { String(format: "%.0f°", v) }
private func mm(_ v: Double) -> String { String(format: "%.0f mm", v) }
private func signed(_ n: Double, _ unit: String) -> String {
    let r = Int(n.rounded())
    return "\(r >= 0 ? "+" : "")\(r)\(unit)"
}
private func pedal(_ v: Double) -> String {
    if abs(v - 1) < 0.01 { return String(localized: "Normal") }
    return (v < 1 ? String(localized: "Softer") : String(localized: "Stronger")) + String(format: " x%.2f", v)
}
private func vibration(_ v: Double) -> String { v < 0.01 ? off : abs(v - 1) < 0.01 ? String(localized: "Normal") : pct(v) }

enum Choices {
    static let maxFps = [("0", "Screen refresh rate"), ("30", "30 fps"), ("45", "45 fps"), ("60", "60 fps"), ("120", "120 fps"), ("144", "144 fps"), ("1000", "Unlimited")]
    static let msaa = [("1", "Off"), ("2", "2x MSAA"), ("4", "4x MSAA"), ("8", "8x MSAA")]
    static let renderScale = [("auto", "Auto"), ("1", "100%"), ("0.85", "85%"), ("0.75", "75%"), ("0.67", "67%"), ("0.5", "50%")]
    static let shadowSize = [("1024", "1024"), ("2048", "2048"), ("4096", "4096")]
    static let cloudQuality = [("high", "High"), ("low", "Low")]
    static let rainQuality = [("high", "High"), ("medium", "Medium"), ("low", "Low")]
    static let viewDistance = [("auto", "Default (1200 m)"), ("600", "600 m - fastest"), ("900", "900 m"), ("1200", "1200 m"), ("1500", "1500 m"), ("2000", "2000 m"), ("2500", "2500 m")]
    static let objectDistance = [("auto", "Automatic"), ("500", "500 m"), ("750", "750 m"), ("900", "900 m"), ("1500", "1500 m"), ("3000", "3000 m")]
    static let smallObjects = [("0.005", "All"), ("0.013", "Normal"), ("0.02", "Fewer (faster)"), ("0.03", "Few (fastest)")]
    static let mirrorSize = [("0", "Off"), ("128", "Low (128)"), ("256", "Normal (256)"), ("512", "High (512)"), ("1024", "Very high (1024)")]
    static let mirrorRefresh = [("off", "None (frozen picture)"), ("eco", "Economical"), ("full", "Full"), ("every", "Every frame (smoothest)")]

    static let rows: [String: (label: String, options: [(String, String)])] = [
        "max_fps": ("Frame limit", maxFps), "msaa": ("Anti-aliasing", msaa), "render_scale": ("Render scale", renderScale), "shadow_size": ("Shadow map", shadowSize),
        "cloud_quality": ("Cloud quality", cloudQuality), "rain_quality": ("Rain quality", rainQuality), "view_distance": ("View distance", viewDistance),
        "max_obj_dist": ("Object distance", objectDistance), "min_obj_size": ("Small objects", smallObjects), "mirror_size": ("Mirrors", mirrorSize),
        "mirror_refresh": ("Bus mirror refresh", mirrorRefresh), "ssao": ("Ambient occlusion", []), "shadows": ("Sun shadows", []),
        "sim_thread": ("Simulation on its own thread", []),
    ]
}

struct CornerRow: View {
    @Environment(LauncherStore.self) private var store
    var fallback: String
    var phone = false

    var body: some View {
        let cur = store.setting("navigator_corner")?.string ?? fallback
        LabeledRow("Corner") {
            HStack {
                Spacer(minLength: 0)
                Grid(horizontalSpacing: 6, verticalSpacing: 6) {
                    if phone {
                        GridRow { corner("top-left", "Top left", cur); corner("top-center", "Centre", cur); corner("top-right", "Top right", cur) }
                        GridRow { corner("bottom-left", "Bottom left", cur); Color.clear; corner("bottom-right", "Bottom right", cur) }
                    } else {
                        GridRow { corner("top-left", "Top left", cur); corner("top-right", "Top right", cur) }
                        GridRow { corner("bottom-left", "Bottom left", cur); corner("bottom-right", "Bottom right", cur) }
                    }
                }
                .padding(5)
                .frame(width: phone ? 150 : 110, height: 64)
                .background(Theme.surface2, in: RoundedRectangle(cornerRadius: 6, style: .continuous))
            }
        }
    }

    private func corner(_ value: String, _ label: LocalizedStringKey, _ cur: String) -> some View {
        Button { store.set("navigator_corner", value) } label: {
            RoundedRectangle(cornerRadius: 3, style: .continuous).fill(cur == value ? Theme.accent : Theme.surface3)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(Text(label))
        .accessibilityAddTraits(cur == value ? .isSelected : [])
        .help(Text(label))
    }
}

/// The quality presets, Low to Ultra (`graphics_presets`; each sets the graphics mode too): the one
/// the settings match now, else "Custom".
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
