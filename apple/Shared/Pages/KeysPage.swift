import GameController
import SwiftUI

struct KeyBinding: Codable, Hashable {
    var action: String
    var scan_code: Int
    var modifier: Int
}

struct KeyLists: Codable {
    var game: [KeyBinding]
    var vehicles: [KeyBinding]

    subscript(sec: String) -> [KeyBinding] {
        get { sec == "game" ? game : vehicles }
        set { if sec == "game" { game = newValue } else { vehicles = newValue } }
    }
}

enum Keys {
    static let hold = 1
    static let shift = 2
    static let ctrl = 4
    static let alt = 8
    static let chord = shift | ctrl | alt

    static let table: [(hid: Int, scan: Int, name: String)] = [
        (0x29, 1, "Escape"), (0x1E, 2, "1"), (0x1F, 3, "2"), (0x20, 4, "3"), (0x21, 5, "4"), (0x22, 6, "5"), (0x23, 7, "6"), (0x24, 8, "7"), (0x25, 9, "8"), (0x26, 10, "9"), (0x27, 11, "0"),
        (0x2D, 12, "-"), (0x2E, 13, "="), (0x2A, 14, "Backspace"), (0x2B, 15, "Tab"),
        (0x14, 16, "Q"), (0x1A, 17, "W"), (0x08, 18, "E"), (0x15, 19, "R"), (0x17, 20, "T"), (0x1C, 21, "Y"), (0x18, 22, "U"), (0x0C, 23, "I"), (0x12, 24, "O"), (0x13, 25, "P"),
        (0x2F, 26, "BracketLeft"), (0x30, 27, "BracketRight"), (0x28, 28, "Enter"), (0xE0, 29, "ControlLeft"),
        (0x04, 30, "A"), (0x16, 31, "S"), (0x07, 32, "D"), (0x09, 33, "F"), (0x0A, 34, "G"), (0x0B, 35, "H"), (0x0D, 36, "J"), (0x0E, 37, "K"), (0x0F, 38, "L"),
        (0x33, 39, ";"), (0x34, 40, "Quote"), (0x35, 41, "Backquote"), (0xE1, 42, "ShiftLeft"), (0x31, 43, "Backslash"),
        (0x1D, 44, "Z"), (0x1B, 45, "X"), (0x06, 46, "C"), (0x19, 47, "V"), (0x05, 48, "B"), (0x11, 49, "N"), (0x10, 50, "M"),
        (0x36, 51, ","), (0x37, 52, "."), (0x38, 53, "/"), (0xE5, 54, "ShiftRight"), (0x55, 55, "NumMultiply"), (0xE2, 56, "AltLeft"), (0x2C, 57, "Space"), (0x39, 58, "CapsLock"),
        (0x3A, 59, "F1"), (0x3B, 60, "F2"), (0x3C, 61, "F3"), (0x3D, 62, "F4"), (0x3E, 63, "F5"), (0x3F, 64, "F6"), (0x40, 65, "F7"), (0x41, 66, "F8"), (0x42, 67, "F9"), (0x43, 68, "F10"),
        (0x53, 69, "NumLock"), (0x47, 70, "ScrollLock"), (0x5F, 71, "Num7"), (0x60, 72, "Num8"), (0x61, 73, "Num9"), (0x56, 74, "NumSubtract"), (0x5C, 75, "Num4"), (0x5D, 76, "Num5"), (0x5E, 77, "Num6"),
        (0x57, 78, "NumAdd"), (0x59, 79, "Num1"), (0x5A, 80, "Num2"), (0x5B, 81, "Num3"), (0x62, 82, "Num0"), (0x63, 83, "NumDecimal"), (0x64, 86, "IntlBackslash"), (0x44, 87, "F11"), (0x45, 88, "F12"),
        (0x68, 100, "F13"), (0x69, 101, "F14"), (0x6A, 102, "F15"), (0x67, 141, "NumEqual"), (0x58, 156, "NumEnter"), (0xE4, 157, "ControlRight"), (0x54, 181, "NumDivide"), (0x46, 183, "PrintScreen"), (0xE6, 184, "AltRight"),
        (0x48, 197, "Pause"), (0x4A, 199, "Home"), (0x52, 200, "Up"), (0x4B, 201, "PageUp"), (0x50, 203, "Left"), (0x4F, 205, "Right"), (0x4D, 207, "End"), (0x51, 208, "Down"), (0x4E, 209, "PageDown"),
        (0x49, 210, "Insert"), (0x4C, 211, "Delete"), (0xE3, 219, "SuperLeft"), (0xE7, 220, "SuperRight"), (0x65, 221, "ContextMenu"),
    ]
    static let scanOf: [Int: Int] = Dictionary(uniqueKeysWithValues: table.map { ($0.hid, $0.scan) })
    static let nameOf: [Int: String] = Dictionary(uniqueKeysWithValues: table.map { ($0.scan, $0.name) }).merging([90: "Shift", 91: "Ctrl", 92: "Alt", 1004: "Mouse 4", 1005: "Mouse 5"]) { a, _ in a }
    static let bothSides: [Int: Int] = [0xE1: 90, 0xE5: 90, 0xE0: 91, 0xE4: 91, 0xE2: 92, 0xE6: 92]
    static let modifierBit: [Int: Int] = [0xE1: shift, 0xE5: shift, 0xE0: ctrl, 0xE4: ctrl, 0xE2: alt, 0xE6: alt, 0xE3: 0, 0xE7: 0]

    static func parts(_ scan: Int, _ modifier: Int) -> [String]? {
        guard scan != 0 else { return nil }
        var p: [String] = []
        if modifier & shift != 0 { p.append("Shift") }
        if modifier & ctrl != 0 { p.append("Ctrl") }
        if modifier & alt != 0 { p.append("Alt") }
        p.append(nameOf[scan] ?? "scan \(scan)")
        return p
    }

    static func name(_ scan: Int, _ modifier: Int) -> String {
        parts(scan, modifier)?.joined(separator: "+") ?? String(localized: "(unbound)")
    }

    static let stock: [KeyBinding] = ([
        ("debug_start_bench2", 48, 6), ("sim_pause", 25, 0), ("debug_window", 1, 2), ("open_mainmenue", 1, 0), ("exit", 16, 4),
        ("view_toggle_viewpoint", 31, 0), ("view_set_driver", 59, 0), ("view_set_passenger", 60, 0), ("view_set_outside", 61, 0), ("view_set_map", 62, 0),
        ("view_set_ego", 87, 0), ("view_set_schedule", 210, 1), ("view_set_ticketselling", 199, 1), ("view_reset_direction", 46, 0), ("view_reset_all_directions", 57, 0),
        ("view_interiorcam_minus", 203, 0), ("view_interiorcam_plus", 205, 0), ("view_toggle_informationdisplay", 21, 2), ("screenshot", 25, 6), ("toggel_ctrler", 37, 0),
        ("toggel_mouse_ctrl", 24, 0), ("open_menue", 92, 0), ("debug_threadcalc", 88, 4), ("scendes_grabmode", 34, 0), ("scendes_rotmode", 19, 0),
        ("scendes_set_x", 45, 0), ("scendes_set_y", 44, 0), ("scendes_set_z", 21, 0), ("scendes_enter", 28, 0), ("scendes_cancel", 1, 0),
        ("scendes_aitraffic_desel", 90, 3), ("scendes_aitraffic_deact", 91, 5), ("scendes_new", 49, 0), ("scendes_delete", 211, 0), ("scendes_nodechange_pri", 67, 0),
        ("scendes_rast_strg", 91, 4), ("ego_forward", 17, 1), ("quicksave", 31, 4),
        ("throttle", 72, 1), ("brake", 80, 1), ("throttle_amplify", 78, 1), ("clutch", 15, 1), ("steering_left", 75, 1), ("steering_neutral", 76, 1), ("steering_right", 77, 1),
        ("ticket_give", 20, 0), ("change_give", 20, 4), ("parking_brake_toggle", 52, 0), ("blinker_left_set", 71, 1), ("blinker_right_set", 73, 1), ("blinker_off", 83, 0),
        ("blinker_warn_toggle", 48, 0), ("kw_scheinwerfer_toggle", 38, 0), ("kw_standlicht_toggle", 38, 2), ("kw_fernlicht_toggle", 33, 0), ("kw_m_enginestart", 50, 0),
        ("kw_wipermode_up", 17, 0), ("horn", 35, 1), ("cp_microphone", 16, 1), ("cp_fahrerlicht_toggle", 7, 0), ("cp_licht_untenrechts_toggle", 8, 0),
        ("cp_licht_oberdeck_toggle", 9, 0), ("cp_licht_unterdeck_toggle", 10, 0), ("kw_s_R", 19, 0), ("kw_s_N", 49, 0), ("kw_s_1", 2, 0), ("kw_s_2", 3, 0), ("kw_s_3", 4, 0),
        ("kw_s_4", 5, 0), ("kw_s_5", 6, 0), ("kw_s_6", 7, 0), ("automatic_R", 19, 0), ("automatic_N", 49, 0), ("automatic_1", 2, 0), ("automatic_2", 3, 0), ("automatic_D", 32, 0),
        ("bus_doorfront0", 181, 0), ("bus_doorfront1", 55, 0), ("bus_dooraft", 74, 0), ("bus_linie_plus", 66, 0), ("bus_linie_minus", 63, 0), ("bus_ziel_plus", 65, 0),
        ("bus_ziel_minus", 64, 0), ("bus_rollband_setL1", 63, 0), ("bus_rollband_setL2", 64, 0), ("bus_rollband_setL3", 65, 0), ("bus_rollband_setT", 66, 0),
        ("bus_rollband_up", 201, 1), ("bus_rollband_dn", 209, 1), ("bus_rollband_up_step", 201, 0), ("bus_rollband_dn_step", 209, 0), ("IBIS_eingabe", 156, 4),
        ("IBIS_1", 79, 4), ("IBIS_2", 80, 4), ("IBIS_3", 81, 4), ("IBIS_4", 75, 4), ("IBIS_5", 76, 4), ("IBIS_6", 77, 4), ("IBIS_7", 71, 4), ("IBIS_8", 72, 4), ("IBIS_9", 73, 4),
        ("IBIS_0", 82, 4), ("IBIS_loeschen", 83, 4), ("IBIS_setmode_linie_kurs", 55, 4), ("IBIS_setmode_route", 181, 4), ("IBIS_setmode_ziel", 74, 4),
        ("cashdesk_changer_8_00", 7, 6), ("cashdesk_changer_4_00", 8, 6), ("cashdesk_changer_1_50", 9, 6), ("cashdesk_changer_0_30", 10, 6), ("cashdesk_changer_0_15", 11, 6),
        ("cashdesk_changer_2_00", 7, 4), ("cashdesk_changer_1_00", 8, 4), ("cashdesk_changer_0_50", 9, 4), ("cashdesk_changer_0_10", 10, 4), ("cashdesk_changer_0_05", 11, 4),
        ("cp_wischer_intervall_toggle", 17, 2), ("cp_wischer_wascher_button", 17, 4), ("cp_batterietrennschalter_toggle", 18, 0), ("bus_20h-switch", 70, 0),
        ("cashdesk_changer_5_00", 6, 4), ("cashdesk_changer_15_00", 6, 6), ("bus_doorfront5", 78, 0), ("taster_nebelschluss", 33, 4), ("cp_schalter_kinderwagen", 88, 0),
        ("kw_s_plus", 26, 0), ("kw_s_minus", 53, 0), ("change_take", 20, 2),
    ] as [(String, Int, Int)]).map { KeyBinding(action: $0.0, scan_code: $0.1, modifier: $0.2) }

    static var stockLists: KeyLists {
        let split = stock.firstIndex { $0.action == "throttle" } ?? 0
        return KeyLists(game: Array(stock[..<split]), vehicles: Array(stock[split...]))
    }

    static let known: [String: String] = [
        "throttle": "Throttle", "brake": "Brake", "throttle_amplify": "Throttle (full, kickdown)", "clutch": "Clutch",
        "steering_left": "Steer left", "steering_right": "Steer right", "steering_neutral": "Steering to centre", "parking_brake_toggle": "Parking brake",
        "blinker_left_set": "Indicator left", "blinker_right_set": "Indicator right", "blinker_left_toggle": "Indicator left (toggle)", "blinker_right_toggle": "Indicator right (toggle)",
        "blinker_off": "Indicators off", "blinker_warn_toggle": "Hazard lights", "gear_up": "Gear up (manual gearbox)", "gear_down": "Gear down (manual gearbox)", "horn": "Horn",
        "kw_scheinwerfer_toggle": "Headlights", "kw_standlicht_toggle": "Sidelights", "kw_fernlicht_toggle": "High beam", "kw_m_enginestart": "Starter",
        "kw_wipermode_up": "Wipers (next mode)", "cp_batterietrennschalter_toggle": "Battery / ignition", "automatic_D": "Gear D",
        "automatic_N": "Gear N (automatic gearbox)", "automatic_R": "Gear R (automatic gearbox)", "kw_s_N": "Gear N (manual gearbox)", "kw_s_R": "Gear R (manual gearbox)",
        "bus_doorfront0": "Front door (leaf 1)", "bus_doorfront1": "Front door (leaf 2)", "bus_dooraft": "Release rear doors",
        "door_1": "Door 1 (front), any bus", "door_2": "Door 2, any bus", "door_3": "Door 3, any bus", "door_4": "Door 4, any bus", "doors_all": "All doors, any bus",
        "doors_release": "Door release (passengers open), any bus", "ticket_give": "Sell the requested ticket", "view_set_driver": "Driver's view", "view_set_passenger": "Passenger view",
        "view_set_outside": "Outside view", "view_toggle_viewpoint": "Next view", "view_toggle_interior": "Cabin and outside, one key", "view_interiorcam_plus": "Next interior camera",
        "view_interiorcam_minus": "Previous interior camera", "view_reset_direction": "Reset the view direction", "view_reset_all_directions": "Reset all view directions",
        "view_look_left": "Look left (hold)", "view_look_right": "Look right (hold)", "view_look_up": "Look up (hold)", "view_look_down": "Look down (hold)",
        "vr_recenter": "VR: Reset view", "vr_toggle_desktop_mirror": "VR: Monitor preview", "vr_toggle_mode": "VR: Switch VR / desktop", "vr_toggle_navigator": "VR: Toggle navigator",
        "vr_position_navigator": "VR: Position navigator", "exit": "Quit", "pax_door_request": "Passenger: request entry / exit", "chat_open": "Multiplayer: write in the chat",
        "chat_toggle": "Multiplayer: show / hide the chat", "voice_radio": "Multiplayer voice: radio, whole session (hold)", "voice_nearby": "Multiplayer voice: nearby players (hold)", "sim_pause": "Pause", "open_menu": "Open / close the main menu",
        "open_menue": "Open / close the quick menu (OMSI: Alt)", "view_zoom_in": "Zoom in", "view_zoom_out": "Zoom out",
        "view_camera_keys_toggle": "Free camera keys on/off (off: they work the bus)", "radio_power": "Radio: on / off", "radio_next": "Radio: next station",
        "radio_previous": "Radio: previous station", "radio_volume_up": "Radio: louder", "radio_volume_down": "Radio: quieter", "screenshot": "Screenshot", "photo_mode": "Photo mode",
        "quicksave": "Quicksave", "toggel_mouse_ctrl": "Toggle mouse steering", "toggel_ctrler": "Toggle game controllers",
    ]

    static let hGears = ["R", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10"].map { "kw_s_\($0)_fest" }

    static func label(_ a: String) -> String {
        if a.hasPrefix("kw_s_"), a.hasSuffix("_fest"), a.count > 10 {
            return tr("Gear {gear} (H-pattern)", ["gear": String(a.dropFirst(5).dropLast(5))])
        }
        if let k = known[a] { return NSLocalizedString(k, comment: "") }
        return humanize(a)
    }

    static func humanize(_ name: String) -> String {
        var s = name.replacingOccurrences(of: "([a-z])([A-Z])", with: "$1 $2", options: .regularExpression)
        s = s.replacingOccurrences(of: "(\\d)(\\D)", with: "$1 $2", options: .regularExpression).replacingOccurrences(of: "(\\D)(\\d)", with: "$1 $2", options: .regularExpression)
        let words = s.split { "_- ./\\".contains($0) }.map(String.init)
            .filter { !["cp", "kw", "bus", "toggle", "mouse", "lod"].contains($0.lowercased()) }
            .map { $0 == $0.uppercased() ? $0 : $0.lowercased() }
        let out = words.isEmpty ? name : words.joined(separator: " ")
        return out.prefix(1).uppercased() + out.dropFirst()
    }

    static func gameOnly(_ a: String) -> Bool {
        a.range(of: "^(vr_|chat_|view_|door_[1-4]$|doors_all$|doors_release$|gear_(up|down)$|sim_pause$|open_menu$|screenshot$|photo_mode$|quicksave$|toggel_|exit$|voice_radio$|voice_nearby$|pax_door_request$)", options: .regularExpression) != nil
    }
}

struct KeyCaps: View {
    var scan: Int
    var modifier: Int

    var body: some View {
        if let parts = Keys.parts(scan, modifier) {
            HStack(spacing: 4) {
                ForEach(Array(parts.enumerated()), id: \.offset) { _, p in
                    Text(p).font(.inter(12, .semibold)).foregroundStyle(Theme.text)
                        .padding(.horizontal, 7).frame(minHeight: 24)
                        .background(Theme.surface3, in: RoundedRectangle(cornerRadius: 4, style: .continuous))
                }
            }
        } else {
            Text("(unbound)").font(.inter(12.5)).foregroundStyle(Theme.muted)
        }
    }
}

struct KeyCapture: Identifiable {
    var id = UUID()
    var sec: String
    var index: Int?
    var insert = 0
    var action = ""
    var modifier = 0
}

struct KeyEditor: View {
    @Environment(LauncherStore.self) private var store
    @Environment(\.compactLayout) private var compact
    @State private var kb: KeyLists?
    @State private var loadError = ""
    @State private var saveError = ""
    @State private var sec = "vehicles"
    @State private var filters = ["vehicles": "", "game": ""]
    @State private var capture: KeyCapture?
    @State private var picker = false
    @State private var confirmReset = false

    var body: some View {
        Group {
            if !loadError.isEmpty {
                VStack(alignment: .leading, spacing: 8) {
                    Notice(text: loadError, kind: .danger)
                    Button { load() } label: { Label("Try again", systemImage: "arrow.clockwise") }.buttonStyle(.launcher(.plain, small: true))
                }
            } else if let kb {
                editor(kb)
            } else {
                HStack(spacing: 8) { ProgressView().controlSize(.small); Text("Reading the keys…").foregroundStyle(Theme.muted) }
            }
        }
        .task { load() }
        .sheet(item: $capture) { c in
            let waiting = c.index.flatMap { i in kb.map { $0[c.sec][i] } } ?? KeyBinding(action: c.action, scan_code: 0, modifier: c.modifier)
            CaptureSheet(action: waiting.action, current: c.index != nil ? waiting : nil, list: kb?[c.sec] ?? [], skip: c.index) { scan, chord in
                assign(c, scan, chord)
            }
            .environment(store)
            .font(.inter(14)).foregroundStyle(Theme.text).tint(Theme.accent).preferredColorScheme(.dark)
        }
        .sheet(isPresented: $picker) {
            ActionPicker(bindings: kb ?? Keys.stockLists) { action in
                picker = false
                filters["vehicles"] = action
                capture = KeyCapture(sec: "vehicles", insert: kb?.vehicles.count ?? 0, action: action)
            }
            .font(.inter(14)).foregroundStyle(Theme.text).tint(Theme.accent).preferredColorScheme(.dark)
        }
        .confirmationDialog("Reset to defaults", isPresented: $confirmReset) {
            Button("Reset", role: .destructive) { persist(Keys.stockLists, keepLayout: true) }
        } message: {
            Text("Every key goes back to OMSI 2's own keyboard.cfg. Keys you added for a bus's own triggers are removed.")
        }
    }

    @ViewBuilder
    private func editor(_ kb: KeyLists) -> some View {
        let preset = store.setting("drive_keys")?.string ?? "simple"
        let presetName = ["wasd": "W A S D only", "arrows": "Arrow keys only"][preset] ?? "W A S D + arrows"
        let list = kb[sec]
        let filter = filters[sec] ?? ""
        let q = filter.trimmingCharacters(in: .whitespaces).lowercased()
        let counts = Dictionary(list.filter { $0.scan_code != 0 }.map { ("\($0.scan_code):\($0.modifier & Keys.chord)", 1) }, uniquingKeysWith: +)
        let all = list.enumerated().map { (b: $0.element, i: $0.offset, label: Keys.label($0.element.action)) }
            .filter { x in q.isEmpty || x.label.lowercased().contains(q) || x.b.action.lowercased().contains(q) || Keys.name(x.b.scan_code, x.b.modifier).lowercased().contains(q) || (Keys.known[x.b.action] ?? "").lowercased().contains(q) }
        let shown = sec == "game" ? all.filter { $0.b.action.hasPrefix("vr_") } + all.filter { !$0.b.action.hasPrefix("vr_") } : all
        let newAction = filter.trimmingCharacters(in: .whitespaces)
        let canAdd = shown.isEmpty && newAction.count > 1 && newAction.range(of: "^[A-Za-z0-9_]+$", options: .regularExpression) != nil
        VStack(alignment: .leading, spacing: 16) {
            if preset != "omsi" {
                VStack(alignment: .leading, spacing: 10) {
                    Notice(text: tr("Driving keys: {name} (Settings). Those keys drive the bus and win over the list below. Change any key here and your own layout (Custom controls) is used from then on.", ["name": NSLocalizedString(presetName, comment: "")]), icon: "keyboard")
                    Button {
                        store.set("drive_keys", "omsi")
                        store.say(String(localized: "Driving keys: Custom controls - the game uses the keys of this page."), error: false)
                    } label: { Label("Use these keys", systemImage: "keyboard") }.buttonStyle(.launcher(.primary, small: true))
                }
            }
            if !saveError.isEmpty { Notice(text: saveError, kind: .danger) }
            Card {
                HStack(alignment: .bottom) {
                    UnderlineTabs(items: [("vehicles", "Driving & the bus"), ("game", "The game")], selection: $sec)
                    Spacer(minLength: 8)
                    Button { confirmReset = true } label: { Label("Reset to defaults", systemImage: "arrow.clockwise") }.buttonStyle(.launcher(.ghost, small: true))
                }
                Text(sec == "game" ? "Menus, views, pausing" : "The bus's own keys").font(.inter(12.5)).foregroundStyle(Theme.muted)
                HStack(spacing: 8) {
                    SearchField(prompt: "Filter...", text: Binding(get: { filters[sec] ?? "" }, set: { filters[sec] = $0 }))
                    if sec == "vehicles" {
                        Button { picker = true } label: { Label("Add binding", systemImage: "plus") }.buttonStyle(.secondary)
                    }
                }
                if canAdd {
                    Button { capture = KeyCapture(sec: sec, insert: list.count, action: newAction) } label: {
                        Label(tr("Add \"{name}\" and give it a key", ["name": newAction]), systemImage: "plus")
                    }
                    .buttonStyle(.secondary)
                }
                if shown.isEmpty && !canAdd {
                    EmptyState(icon: "magnifyingglass", title: "No matching actions.")
                }
                LazyVStack(spacing: 0) {
                    ForEach(shown, id: \.i) { x in
                        let clash = x.b.scan_code != 0 && (counts["\(x.b.scan_code):\(x.b.modifier & Keys.chord)"] ?? 0) > 1
                        row(x.b, x.i, x.label, clash)
                        Divider().overlay(Theme.surface2)
                    }
                }
            }
        }
    }

    private func row(_ b: KeyBinding, _ i: Int, _ label: String, _ clash: Bool) -> some View {
        let keys = HStack(spacing: 8) {
            KeyCaps(scan: b.scan_code, modifier: b.modifier)
            if clash { Tag(text: "Same key", kind: .danger).help("Another entry of this list has the same key.") }
        }
        let buttons = HStack(spacing: 4) {
            Button("Edit") { capture = KeyCapture(sec: sec, index: i) }.buttonStyle(.launcher(.plain, small: true))
            Button { capture = KeyCapture(sec: sec, insert: i + 1, action: b.action, modifier: b.modifier) } label: { Image(systemName: "plus") }
                .buttonStyle(.launcher(.ghost, small: true)).help("Add another key for this action")
            Button { clear(i) } label: { Image(systemName: "trash") }
                .buttonStyle(.launcher(.ghost, small: true)).help("Clear the key (the entry stays)").disabled(b.scan_code == 0)
            Button { remove(i) } label: { Image(systemName: "xmark") }
                .buttonStyle(.launcher(.ghost, small: true)).help("Remove the entry")
        }
        let name = VStack(alignment: .leading, spacing: 2) {
            Text(label).font(.inter(14))
            if label != b.action { Text(b.action).font(.inter(12)).foregroundStyle(Theme.muted) }
        }
        return Group {
            if compact {
                VStack(alignment: .leading, spacing: 6) {
                    name
                    HStack { keys; Spacer(minLength: 4); buttons }
                }
            } else {
                HStack(spacing: 12) {
                    name.frame(maxWidth: .infinity, alignment: .leading)
                    keys.frame(width: 230, alignment: .leading)
                    buttons
                }
            }
        }
        .padding(.vertical, 8)
    }

    private func load() {
        loadError = ""
        do {
            kb = try Bridge.get(KeyLists.self, "cli", ["cmd": "keybindings", "arg": [String: Any]()])
        } catch {
            loadError = error.localizedDescription
        }
    }

    private func persist(_ next: KeyLists, vr: Bool = false, keepLayout: Bool = false) {
        kb = next
        do {
            let arg = try JSONSerialization.jsonObject(with: JSONEncoder().encode(next))
            kb = try Bridge.get(KeyLists.self, "cli", ["cmd": "save_keybindings", "arg": arg])
            saveError = ""
            let preset = store.setting("drive_keys")?.string ?? "simple"
            if !vr && !keepLayout && preset != "omsi" {
                store.set("drive_keys", "omsi")
                store.say(String(localized: "Key bindings saved; Driving keys switched to Custom controls so the game uses them."), error: false)
            } else {
                store.say(String(localized: "Key bindings saved."), error: false)
            }
        } catch {
            saveError = error.localizedDescription
            store.say(error.localizedDescription, error: true)
        }
    }

    private func clear(_ i: Int) {
        guard var next = kb else { return }
        next[sec][i].scan_code = 0
        next[sec][i].modifier = 0
        persist(next, vr: next[sec][i].action.hasPrefix("vr_"))
    }

    private func remove(_ i: Int) {
        guard var next = kb else { return }
        let vr = next[sec][i].action.hasPrefix("vr_")
        next[sec].remove(at: i)
        persist(next, vr: vr)
    }

    private func assign(_ c: KeyCapture, _ scan: Int, _ chord: Int) {
        capture = nil
        guard var next = kb else { return }
        if let i = c.index {
            let b = next[c.sec][i]
            next[c.sec][i].scan_code = scan
            next[c.sec][i].modifier = scan != 0 ? chord | (b.modifier & Keys.hold) : 0
            persist(next, vr: b.action.hasPrefix("vr_"))
        } else if scan != 0 {
            next[c.sec].insert(KeyBinding(action: c.action, scan_code: scan, modifier: chord | (c.modifier & Keys.hold)), at: min(c.insert, next[c.sec].count))
            persist(next, vr: c.action.hasPrefix("vr_"))
        }
    }
}

private struct CaptureSheet: View {
    @Environment(LauncherStore.self) private var store
    @Environment(\.dismiss) private var dismiss
    var action: String
    var current: KeyBinding?
    var list: [KeyBinding]
    var skip: Int?
    var onKey: (Int, Int) -> Void
    @State private var pending: (scan: Int, chord: Int, taken: [KeyBinding])?
    @State private var held: Set<Int> = []
    @State private var lone: Int?
    @FocusState private var focused: Bool

    var body: some View {
        let label = Keys.label(action)
        VStack(spacing: 14) {
            Text(label).font(.inter(17, .semibold))
            if label != action { Text(action).font(.inter(12.5)).foregroundStyle(Theme.muted) }
            if let p = pending {
                KeyCaps(scan: p.scan, modifier: p.chord)
                Notice(text: tr("This key is already used by: {actions}", ["actions": p.taken.map { Keys.label($0.action) }.joined(separator: ", ")]) + "\n" + String(localized: "Both will react to it. Press another key, or use it anyway."), kind: .warn)
            } else {
                Text("press a key...").font(.inter(14, .semibold)).padding(.horizontal, 18).frame(height: 40)
                    .background(Theme.surface3, in: RoundedRectangle(cornerRadius: 6, style: .continuous))
                if let c = current {
                    HStack(spacing: 6) { Text("Now:").foregroundStyle(Theme.muted); KeyCaps(scan: c.scan_code, modifier: c.modifier) }.font(.inter(12.5))
                }
            }
            Text("Hold Shift, Ctrl or Alt for a combination, or press one alone to bind it. Escape cancels; Backspace or Delete clears the key.")
                .font(.inter(12.5)).foregroundStyle(Theme.muted).multilineTextAlignment(.center).fixedSize(horizontal: false, vertical: true)
            HStack(spacing: 8) {
                Spacer()
                Button("Cancel") { dismiss() }.buttonStyle(.secondary)
                if let p = pending { Button("Use it anyway") { onKey(p.scan, p.chord) }.buttonStyle(.primary) }
            }
        }
        .padding(24)
        .frame(minWidth: 380, maxWidth: 520)
        .background(Theme.surface.ignoresSafeArea())
        .focusable()
        .focused($focused)
        .focusEffectDisabled()
        .onKeyPress { _ in .handled }
        .onAppear {
            focused = true
            listen()
        }
        .onReceive(NotificationCenter.default.publisher(for: .GCKeyboardDidConnect)) { _ in listen() }
        .onDisappear { GCKeyboard.coalesced?.keyboardInput?.keyChangedHandler = nil }
    }

    private func listen() {
        GCKeyboard.coalesced?.keyboardInput?.keyChangedHandler = { _, _, code, pressed in
            MainActor.assumeIsolated { key(code.rawValue, pressed) }
        }
    }

    private func chord(except own: Int = 0) -> Int {
        held.reduce(0) { $0 | (Keys.modifierBit[$1] ?? 0) } & ~own
    }

    private func key(_ code: Int, _ down: Bool) {
        if let bit = Keys.modifierBit[code] {
            if down {
                held.insert(code)
                lone = code
            } else {
                held.remove(code)
                if lone == code {
                    lone = nil
                    take(Keys.bothSides[code] ?? Keys.scanOf[code] ?? 0, chord(except: bit))
                }
            }
            return
        }
        guard down else { return }
        lone = nil
        switch code {
        case 0x29: dismiss()
        case 0x2A, 0x4C:
            if current != nil { onKey(0, 0) } else { dismiss() }
        default:
            guard let scan = Keys.scanOf[code] else {
                store.say(tr("{code} has no DirectInput scan code the game understands.", ["code": String(format: "0x%02X", code)]), error: true)
                return
            }
            take(scan, chord())
        }
    }

    private func take(_ scan: Int, _ chord: Int) {
        guard scan != 0 else { return }
        let taken = list.enumerated().filter { $0.offset != skip && $0.element.scan_code == scan && ($0.element.modifier & Keys.chord) == chord && $0.element.action != action }.map(\.element)
        if taken.isEmpty { onKey(scan, chord) } else { pending = (scan, chord, taken) }
    }
}

private struct ActionPicker: View {
    @Environment(\.dismiss) private var dismiss
    var bindings: KeyLists
    var pick: (String) -> Void
    @State private var search = ""
    @State private var rows = 150

    var body: some View {
        let options = all.filter { search.isEmpty || $0.find.contains(search.trimmingCharacters(in: .whitespaces).lowercased()) }
        let keys = Dictionary(bindings.vehicles.filter { $0.scan_code != 0 }.map { ($0.action.lowercased(), $0) }) { a, _ in a }
        PickerFrame(title: "Add a key binding") {
            VStack(alignment: .leading, spacing: 10) {
                Text("Driving & the bus").font(.inter(12.5)).foregroundStyle(Theme.muted)
                SearchField(prompt: "Search actions...", text: $search)
                Text(tr("{n} results", ["n": "\(options.count)"])).font(.inter(12.5)).foregroundStyle(Theme.muted)
                Scroll {
                    LazyVStack(spacing: 0) {
                        if options.isEmpty { Text("No matching actions.").foregroundStyle(Theme.muted).padding(8) }
                        ForEach(options.prefix(rows), id: \.action) { o in
                            ListItem(title: o.label, subtitle: o.action, selected: false) { pick(o.action) } trailing: {
                                if let k = keys[o.action.lowercased()] { KeyCaps(scan: k.scan_code, modifier: k.modifier) }
                            }
                        }
                        if options.count > rows {
                            HStack {
                                Text(tr("Showing the first {shown} of {n}: search to narrow the list.", ["shown": "\(rows)", "n": "\(options.count)"])).font(.inter(12.5)).foregroundStyle(Theme.muted)
                                Spacer()
                                Button("Show more") { rows += 150 }.buttonStyle(.launcher(.plain, small: true))
                            }
                            .padding(8)
                        }
                    }
                }
                Text("The installed buses' own script triggers are not listed here: type the trigger's name in the list's filter to add it.")
                    .font(.inter(12.5)).foregroundStyle(Theme.muted).fixedSize(horizontal: false, vertical: true)
            }
            .padding(.horizontal, 20)
        } foot: {
            Spacer()
            Button("Cancel") { dismiss() }.buttonStyle(.secondary)
        }
    }

    private var all: [(action: String, label: String, find: String)] {
        let split = Keys.stock.firstIndex { $0.action == "throttle" } ?? 0
        let game = Set((Keys.stock[..<split] + bindings.game).map { $0.action.lowercased() })
        var seen = Set<String>()
        var out: [(action: String, label: String, find: String)] = []
        for a in Keys.stock[split...].map(\.action) + bindings.vehicles.map(\.action) + Keys.known.keys.sorted() + Keys.hGears {
            let k = a.lowercased()
            guard !a.isEmpty, !Keys.gameOnly(a), !game.contains(k), !seen.contains(k) else { continue }
            seen.insert(k)
            let l = Keys.label(a)
            out.append((a, l, "\(l)\n\(a)".lowercased()))
        }
        return out.sorted { $0.find.localizedStandardCompare($1.find) == .orderedAscending }
    }
}
