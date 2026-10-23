import GameController
import SwiftUI
import UniformTypeIdentifiers

// The launcher's other pages, as the desktop launcher has them (`launcher/ui/js/pages/*.js`).

/// Two cards side by side where there is room, one under the other where there is not.
struct TwoColumns<Left: View, Right: View>: View {
    var rightWidth: CGFloat? = nil
    @ViewBuilder var left: Left
    @ViewBuilder var right: Right
    @Environment(\.compactLayout) private var compact

    @State var width: CGFloat = 1000

    var body: some View {
        Group {
            if !compact && width >= 780 {
                HStack(alignment: .top, spacing: 20) {
                    VStack(spacing: 16) { left }.frame(maxWidth: .infinity)
                    VStack(spacing: 16) { right }.frame(maxWidth: rightWidth ?? .infinity)
                }
            } else {
                VStack(spacing: 16) { left; right }
            }
        }
        .frame(maxWidth: .infinity)
        .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { width = $0 }
    }
}

// --- Multiplayer -----------------------------------------------------------------------------

struct MultiplayerPage: View {
    @Environment(LauncherStore.self) private var store
    @State private var tab = "code"
    @State private var address = ""
    @State private var name = ""

    var body: some View {
        PageScroll("Multiplayer", subtitle: "Drive with friends by a code, or on servers that are always on.") {
            UnderlineTabs(items: [("code", "Connect by Code"), ("servers", "Servers")], selection: $tab)
            if tab == "code" { byCode } else { servers }
        }
        .onAppear { for s in store.snapshot.servers { store.act("server_ask", ["address": s.address, "every": 10]) } }
    }

    @State private var codeMode = ""

    private var byCode: some View {
        let mode = codeMode.isEmpty ? (runningHost(store.snapshot.instances) != nil || store.choice.lan_mode == "host" ? "host" : "join") : codeMode
        return VStack(alignment: .leading, spacing: 16) {
            Segmented(items: [("join", "Join friends"), ("host", "Invite friends")], selection: mode) { codeMode = $0 }
            if mode == "join" { JoinByCode() } else { HostByCode() }
        }
        .frame(maxWidth: 760, alignment: .leading)
    }

    private var servers: some View {
        VStack(spacing: 16) {
            Card("Add server") {
                ViewThatFits(in: .horizontal) {
                    HStack(spacing: 8) { nameField.frame(width: 180); addressField; addButton }
                    VStack(spacing: 8) { nameField; addressField; addButton.frame(maxWidth: .infinity, alignment: .trailing) }
                }
            }
            if store.snapshot.servers.isEmpty {
                Card { EmptyState(icon: "globe", text: "No servers yet. Add a server by the address its owner gives you.") }
            }
            ForEach(store.snapshot.servers) { s in
                ServerCard(server: s, joined: store.snapshot.joined_server == s.address)
            }
        }
    }

    private var nameField: some View { InputField(prompt: "Name (optional)", text: $name) }
    private var addressField: some View { InputField(prompt: "Address: https://… or host:port", text: $address) }
    private var addButton: some View {
        Button {
            let a = address.trimmingCharacters(in: .whitespaces)
            if store.snapshot.servers.contains(where: { $0.address == a }) { store.say(String(localized: "That server is in the list already"), error: true); return }
            store.act("server_add", ["name": name, "address": a]); address = ""; name = ""
        } label: { Label("Add server", systemImage: "plus") }
            .buttonStyle(.secondary).disabled(address.trimmingCharacters(in: .whitespaces).isEmpty)
    }
}

struct ServerCard: View {
    @Environment(LauncherStore.self) private var store
    var server: Server
    var joined: Bool
    @State private var status: HostStatus?

    var body: some View {
        HStack(alignment: .top, spacing: 14) {
            icon
            .frame(width: 48, height: 48).background(Theme.surface2, in: RoundedRectangle(cornerRadius: 8, style: .continuous))
            .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 8) {
                    Text(server.info?.name?.isEmpty == false ? server.info!.name! : (server.name.isEmpty ? server.address : server.name)).font(.inter(15, .semibold)).lineLimit(1)
                    if joined { Tag(text: "Joined", kind: .accent) }
                    if server.info?.password == true { Tag(text: "Password") }
                }
                if let i = server.info {
                    if i.ok {
                        Text([i.map.map { $0.replacingOccurrences(of: "\\", with: "/").replacingOccurrences(of: "/global.cfg", with: "").replacingOccurrences(of: "maps/", with: "") } ?? "", tr("{n} of {max} players", ["n": "\(i.players ?? 0)", "max": "\(i.max_players ?? 0)"]), i.time ?? "", i.weather ?? ""].filter { !$0.isEmpty }.joined(separator: " - ")).font(.inter(12.5)).foregroundStyle(Theme.muted).lineLimit(1)
                        if let m = i.motd, !m.isEmpty { Text(m).font(.inter(13)).foregroundStyle(Theme.text2).lineLimit(2) }
                    } else {
                        Text(tr("Can't reach the server: {err}", ["err": i.error ?? String(localized: "No answer")])).font(.inter(12.5)).foregroundStyle(Theme.danger).lineLimit(2)
                    }
                } else {
                    HStack(spacing: 6) {
                        ProgressView().controlSize(.mini).tint(Theme.muted)
                        Text("Asking the server...").font(.inter(12.5)).foregroundStyle(Theme.muted)
                    }
                }
                SelectField(options: [("auto", autoLabel), ("udp", "UDP"), ("websocket", "WebSocket")], selection: server.protocol ?? "auto", onChange: { store.act("server_protocol", ["address": server.address, "proto": $0]) })
                    .frame(width: 130).padding(.top, 4)
            }
            Spacer(minLength: 8)
            if joined {
                Button("Leave") { store.act("server_leave") }.buttonStyle(.launcher(.plain, small: true))
            } else {
                Button("Join") { store.act("server_join", ["address": server.address]); store.page = .drive; store.phoneTab = "drive" }.buttonStyle(.launcher(.primary, small: true))
                    .disabled(server.info?.ok != true)
            }
            Button { store.act("server_ask", ["address": server.address, "every": 10]) } label: { Image(systemName: "arrow.clockwise") }
                .buttonStyle(.launcher(.ghost, small: true)).help("Refresh")
            if server.official != true {
                Button { store.act("server_remove", ["address": server.address]) } label: { Image(systemName: "trash") }
                    .buttonStyle(.launcher(.ghost, small: true)).help("Remove from the list")
            }
        }
        .padding(.horizontal, 20).padding(.vertical, 18)
        .surface()
        .task(id: server.address) {
            let a = server.address
            status = try? await offMain { try Bridge.api(HostStatus.self, "server_query", ["address": a]) }
        }
    }

    @ViewBuilder private var icon: some View {
        if let s = status?.icon, let comma = s.firstIndex(of: ","), let data = Data(base64Encoded: String(s[s.index(after: comma)...])), let picture = picture(data) {
            picture.resizable().scaledToFill()
        } else {
            Image(systemName: server.official == true ? "checkmark.seal.fill" : "globe").font(.system(size: 20)).foregroundStyle(joined ? Theme.accent : Theme.muted)
        }
    }

    private func picture(_ data: Data) -> Image? {
        #if os(iOS)
        UIImage(data: data).map { Image(uiImage: $0) }
        #else
        NSImage(data: data).map { Image(nsImage: $0) }
        #endif
    }

    private var autoLabel: String {
        guard let s = status else { return String(localized: "Auto") }
        return "\(String(localized: "Auto")) (\(udpByDefault(server.address, reachedAt: s.reached_at) ? "UDP" : "WS"))"
    }
}

func udpByDefault(_ address: String, reachedAt: String?) -> Bool {
    let a = address.trimmingCharacters(in: .whitespaces)
    let host = a.split(whereSeparator: { "/?#\\".contains($0) }).first.map(String.init)?.split(separator: "@").last.map(String.init)?.replacingOccurrences(of: ":[0-9]*$", with: "", options: .regularExpression) ?? ""
    let web = a.range(of: "^(https?|wss?)://", options: .regularExpression) != nil || a.lowercased() == "openomsi"
        || host.range(of: "^[a-z0-9-]+(\\.[a-z0-9-]+)*\\.trycloudflare\\.com$", options: [.regularExpression, .caseInsensitive]) != nil
    guard !web, let m = a.range(of: "^(?:[^:\\s/]+|\\[[^\\]\\s]+\\]):([0-9]+)$", options: .regularExpression) else { return false }
    let port = a[m].split(separator: ":").last.flatMap { Int($0) } ?? 99999
    return port <= 65535 && reachedAt != "http://\(a)"
}

// --- Profile ---------------------------------------------------------------------------------

struct ProfilePage: View {
    @Environment(LauncherStore.self) private var store
    @Environment(\.compactLayout) private var compact
    @State private var newName = ""
    @State private var sex = "M"
    @State private var confirmDelete = false

    var body: some View {
        PageScroll("Profile", subtitle: "The driver whose personnel file the game writes: hours, kilometres, punctuality, tickets.") {
            driver
            if let p = store.snapshot.profile {
                Card {
                    HStack(spacing: 18) {
                        Avatar(name: p.name, size: 64)
                        VStack(alignment: .leading, spacing: 4) {
                            HStack(spacing: 8) {
                                Text(p.name).font(.inter(22, .semibold))
                                if !p.exists { Text("(no personnel file yet)").font(.inter(12.5)).foregroundStyle(Theme.muted) }
                            }
                            Text(tr("Level {n}", ["n": "\(p.level)"]) + " - " + tr("{xp} XP, {left} to level {next}", ["xp": "\(p.xp)", "left": "\(max(0, p.next_level_xp - p.xp))", "next": "\(p.level + 1)"])).font(.inter(14)).foregroundStyle(Theme.muted)
                            ProgressBar(value: progress(p)).frame(maxWidth: 420).padding(.top, 6)
                        }
                    }
                }
                LazyVGrid(columns: [GridItem(.adaptive(minimum: compact ? 140 : 150), spacing: 12)], spacing: 12) {
                    StatTile(icon: "clock.fill", value: String(format: "%d h %02d min", Int(p.hours), Int((p.hours * 60).truncatingRemainder(dividingBy: 60))), label: "hours driven")
                    StatTile(icon: "point.topleft.down.to.point.bottomright.curvepath.fill", value: String(format: "%.1f km", p.km), label: "distance")
                    StatTile(icon: "mappin.circle.fill", value: "\(p.stops)", label: "stops served")
                    StatTile(icon: "clock.arrow.circlepath", value: "\(p.early) / \(p.late)", label: "early / late")
                    StatTile(icon: "ticket.fill", value: "\(Int(p.tickets))", label: "tickets sold")
                    StatTile(icon: "banknote.fill", value: String(format: "%.2f", p.cash), label: "takings")
                    StatTile(icon: "exclamationmark.triangle.fill", value: "\(p.crashes)", label: "crashes")
                    StatTile(icon: "figure.walk", value: "\(p.hurt)", label: "pedestrians hurt")
                    StatTile(icon: "bus.fill", value: "\(Int(p.rating_driving)) %", label: "driving")
                    StatTile(icon: "person.2.fill", value: "\(Int(p.rating_comfort)) %", label: "comfort")
                    StatTile(icon: "ticket.fill", value: "\(Int(p.rating_tickets)) %", label: "ticket selling")
                    StatTile(icon: "gamecontroller.fill", value: "\(p.sessions.count)", label: "runs")
                }
                runs(p.sessions)
            } else {
                Card { EmptyState(icon: "person.fill", title: "No driver yet", text: "Create a driver to start a personnel file.") }
            }
        }
    }

    private var driver: some View {
        Card("Driver") {
            let columns = compact ? [GridItem(.flexible())] : [GridItem(.flexible(), spacing: 16), GridItem(.flexible(), spacing: 16)]
            LazyVGrid(columns: columns, alignment: .leading, spacing: 12) {
                HStack(spacing: 8) {
                    SelectField(options: store.snapshot.profiles.map { (value: $0, label: $0) }, selection: store.snapshot.config.profile, onChange: { store.act("profile", ["name": $0]) }, placeholder: "Choose a driver")
                    Button { confirmDelete = true } label: { Image(systemName: "trash") }.buttonStyle(.destructive).help("Delete this driver")
                        .disabled(store.snapshot.config.profile.isEmpty)
                        .confirmationDialog("Delete this driver?", isPresented: $confirmDelete) {
                            Button("Delete", role: .destructive) { store.act("profile_delete", ["name": store.snapshot.config.profile]) }
                        } message: {
                            Text(tr("The personnel file of {name} is deleted with all its figures.", ["name": store.snapshot.config.profile]))
                        }
                }
                HStack(spacing: 8) {
                    InputField(prompt: "New driver's name", text: $newName) { create() }
                    Segmented(items: [("M", "Male"), ("F", "Female")], selection: sex) { sex = $0 }
                    Button { create() } label: { Label("Create", systemImage: "plus") }
                        .buttonStyle(.secondary).disabled(newName.trimmingCharacters(in: .whitespaces).isEmpty)
                }
            }
        }
    }

    private func create() {
        guard !newName.trimmingCharacters(in: .whitespaces).isEmpty else { return }
        store.act("profile_create", ["name": newName, "sex": sex])
        newName = ""
    }

    private func runs(_ sessions: [Session]) -> some View {
        Card("Recent runs") {
            if sessions.isEmpty {
                EmptyState(icon: "clock.arrow.circlepath", text: "No runs yet. Drive a duty and it shows up here.")
            } else if compact {
                VStack(alignment: .leading, spacing: 12) {
                    ForEach(sessions.prefix(30), id: \.self) { s in
                        HStack(spacing: 10) {
                            if let l = s.line { LineBadge(line: l) }
                            VStack(alignment: .leading, spacing: 2) {
                                Text("\(s.bus.stem) - \(mapName(s.map))").font(.inter(14, .medium)).lineLimit(1)
                                Text("\(ago(s.time)) - \(duration(s.seconds)) - \(String(format: "%.1f", s.metres / 1000)) km - " + tr("{n} stops", ["n": "\(s.stops)"])).font(.inter(12.5)).foregroundStyle(Theme.muted).lineLimit(1)
                            }
                        }
                    }
                }
            } else {
                Grid(alignment: .leading, horizontalSpacing: 16, verticalSpacing: 0) {
                    GridRow {
                        ForEach(["When", "Duty", "Bus", "Time", "Distance", "Stops", "Tickets", "Crashes"], id: \.self) { h in
                            Text(LocalizedStringKey(h)).font(.inter(12, .semibold)).foregroundStyle(Theme.muted).padding(.vertical, 8)
                        }
                    }
                    ForEach(Array(sessions.prefix(50).enumerated()), id: \.offset) { i, s in
                        GridRow {
                            Text(ago(s.time)).foregroundStyle(Theme.muted)
                            HStack(spacing: 6) {
                                if let l = s.line { LineBadge(line: l); if let t = s.tour { Text("/ \(t)") } } else { Text("Free drive") }
                                Text(mapName(s.map)).foregroundStyle(Theme.muted)
                            }
                            Text(s.bus.stem).lineLimit(1)
                            Text(duration(s.seconds)).monospacedDigit()
                            Text(String(format: "%.1f km", s.metres / 1000)).monospacedDigit()
                            Text("\(s.stops)").monospacedDigit()
                            Text("\(s.tickets)").monospacedDigit()
                            Text("\(s.crashes)").monospacedDigit()
                        }
                        .font(.inter(14))
                        .padding(.vertical, 8)
                        .background(i % 2 == 1 ? Color.white.opacity(0.018) : .clear)
                    }
                }
            }
        }
    }

    private func mapName(_ m: String) -> String {
        m.replacingOccurrences(of: "\\", with: "/").replacingOccurrences(of: "/global.cfg", with: "").replacingOccurrences(of: "maps/", with: "")
    }

    private func duration(_ s: Double) -> String {
        String(format: "%d:%02d", Int(s) / 3600, (Int(s) % 3600) / 60)
    }

    private func ago(_ t: UInt64) -> String {
        Date(timeIntervalSince1970: TimeInterval(t)).formatted(.relative(presentation: .named))
    }

    private func progress(_ p: Profile) -> Double {
        guard p.next_level_xp > 0 else { return 0 }
        let prev = (p.level - 1) * (p.level - 1) * 250
        return min(1, max(0, Double(p.xp - prev) / Double(max(1, p.next_level_xp - prev))))
    }
}

// --- Controls --------------------------------------------------------------------------------

struct ControlsPage: View {
    @Environment(LauncherStore.self) private var store
    @State private var keyboard = GCKeyboard.coalesced != nil

    var body: some View {
        let touch = store.platform == "ios"
        let keys = !touch || keyboard
        PageScroll("Controls", subtitle: touch ? "Touch, tilt and game controllers." : "Keys, wheels and gamepads.") {
            TwoColumns {
                if touch {
                    Card("Touch controls") {
                        SettingSelect(label: "Door buttons", key: "touch_door_mode", options: [("standard", "Standard (physical doors)"), ("direct", "Direct OMSI triggers")])
                        Text("Tilt steering is switched in the game (the steering wheel button). A game controller (Xbox, PlayStation, MFi) works as soon as it is connected.").font(.inter(13)).foregroundStyle(Theme.muted)
                    }
                }
                Card("Key bindings") {
                    if !keys {
                        Notice(text: String(localized: "The keys of the game are OMSI's Inputs/keyboard.cfg: the openOMSI launcher of Windows and Linux sets them on its Controls page, which reads and writes the same file."))
                    }
                    SettingSelect(label: "Driving keys", key: "drive_keys", options: [("omsi", "Custom controls (Controls page)"), ("simple", "W A S D + arrows"), ("wasd", "W A S D only"), ("arrows", "Arrow keys only")])
                }
            } right: {
                Card("Game controllers") {
                    SettingSelect(label: "Gamepad type", key: "pad_type", options: [("auto", "Automatic"), ("xbox", "Xbox"), ("ps4", "PlayStation 4"), ("ps5", "PlayStation 5")])
                    SettingToggle(label: "Default gamepad buttons (indicators, doors, views, gears, menu, pause)", key: "pad_buttons", fallback: true)
                    SettingToggle(label: "Force feedback and vibration", key: "ff_enabled", fallback: true)
                }
            }
            if keys {
                KeyEditor()
            }
        }
        .onReceive(NotificationCenter.default.publisher(for: .GCKeyboardDidConnect)) { _ in keyboard = true }
        .onReceive(NotificationCenter.default.publisher(for: .GCKeyboardDidDisconnect)) { _ in keyboard = GCKeyboard.coalesced != nil }
    }
}

// --- Running games ---------------------------------------------------------------------------

struct GamesPage: View {
    @Environment(LauncherStore.self) private var store
    @State private var log: [String] = []
    @State private var logPid: UInt32?
    @State private var stopPid: UInt32?

    var body: some View {
        PageScroll("Running games", subtitle: "Any number of games can run at once; each writes its own log.", maxWidth: 980) {
            if store.snapshot.instances.isEmpty {
                Card { EmptyState(icon: "gamecontroller.fill", title: "No game is running", text: "Start one on the Drive page.") }
            }
            ForEach(store.snapshot.instances) { i in
                Card {
                    HStack(spacing: 12) {
                        Circle().fill(i.running == true ? Theme.ok : Theme.muted).frame(width: 8, height: 8)
                        VStack(alignment: .leading, spacing: 3) {
                            HStack(spacing: 8) {
                                if let l = i.line { LineBadge(line: l) }
                                Text("\(i.bus.stem) - \(i.map.replacingOccurrences(of: "/global.cfg", with: "").replacingOccurrences(of: "maps/", with: ""))").font(.inter(15, .semibold)).lineLimit(1)
                            }
                            Text(tr("Process {pid}", ["pid": "\(i.pid)"]) + " - \(i.profile) - \(i.lan)").font(.inter(12.5)).foregroundStyle(Theme.muted)
                            if let last = i.last_line, !last.isEmpty {
                                Text(last).font(.inter(12.5)).foregroundStyle(Theme.muted).lineLimit(1).truncationMode(.tail).help(last)
                            }
                        }
                        Spacer()
                        Button {
                            if logPid == i.pid { logPid = nil; return }
                            log = []
                            logPid = i.pid
                        } label: { Label(logPid == i.pid ? "Hide log" : "Show log", systemImage: "doc.text") }
                            .buttonStyle(.launcher(.plain, small: true))
                        Button { Platform.reveal(i.log) } label: { Image(systemName: "folder") }
                            .buttonStyle(.launcher(.ghost, small: true)).help("Show the log file")
                        if i.running == true {
                            Button("Stop") { stopPid = i.pid }.buttonStyle(.launcher(.danger, small: true))
                                .disabled(i.stopping != nil)
                        }
                    }
                    if let code = i.lan_status?["code"]?.string, i.lan_status?["role"]?.string == "host", !code.isEmpty {
                        SessionCode(code: code)
                    }
                    if logPid == i.pid { LogBlock(text: log.joined(separator: "\n")) }
                }
            }
        }
        .task(id: logPid) {
            while let pid = logPid, !Task.isCancelled {
                let lines = try? await offMain { try Bridge.api([String].self, "game_log", ["pid": pid, "lines": 120]) }
                if logPid == pid, let lines { log = lines }
                try? await Task.sleep(for: .seconds(2))
            }
        }
        .confirmationDialog("Stop this game?", isPresented: Binding(get: { stopPid != nil }, set: { if !$0 { stopPid = nil } })) {
            Button("Stop", role: .destructive) { if let p = stopPid { store.act("stop", ["pid": p]) }; stopPid = nil }
        } message: {
            Text("The game ends as if Escape and Quit were chosen; an unsaved session is lost.")
        }
    }
}

// --- Mods ------------------------------------------------------------------------------------

struct ModsPage: View {
    @Environment(LauncherStore.self) private var store
    @Environment(\.compactLayout) private var compact
    @State private var search = ""
    @State private var kind = "all"
    @State private var mode = "auto"
    @State private var open: String?
    @State private var removing: ModEntry?
    @State private var choosingHofs = false
    @State private var lastPath = ""
    @State private var dropOver = false

    var body: some View {
        PageScroll("Mods", subtitle: "A bus, a map, scenery, a whole OMSI folder - as a folder or a .zip, .7z or .rar. The original OMSI 2 folder is never written to.") {
            TwoColumns(rightWidth: 360) {
                installed
                if let w = store.snapshot.mods?.waiting, !w.isEmpty {
                    Card("Waiting for their bus", hint: "Packs installed before the bus they belong to: they go in once it is there.") {
                        ForEach(w, id: \.self) { Text($0).font(.inter(14)).foregroundStyle(Theme.text2) }
                    }
                }
                if let h = store.snapshot.mods?.hof_warnings, !h.isEmpty {
                    Notice(text: ([String(localized: "Map HOF issue"), String(localized: "Some map HOF files could not be added. Check launcher.log for details.")] + h.prefix(10).map { "- \($0)" }).joined(separator: "\n"), kind: .warn)
                }
                if let a = store.snapshot.mods?.archives, !a.isEmpty {
                    Card("Archives", hint: "Read in place by the game, without unpacking.") {
                        ForEach(a.indices, id: \.self) { i in
                            HStack {
                                Text(a[i].first?.string ?? "").font(.inter(14)).lineLimit(1).truncationMode(.middle)
                                Spacer()
                                Text(formatBytes(UInt64(a[i].last?.double ?? 0))).font(.inter(13).monospacedDigit()).foregroundStyle(Theme.muted)
                            }
                        }
                    }
                }
            } right: {
                install
                jobs
                if let m = store.snapshot.mods {
                    Card("The Mods folder") {
                        Text("Anything put into this folder is installed by itself once it has finished copying.").font(.inter(13.5)).foregroundStyle(Theme.muted)
                        HStack(spacing: 8) {
                            Text(m.inbox).font(.inter(12.5)).foregroundStyle(Theme.text2).lineLimit(1).truncationMode(.middle).textSelection(.enabled)
                            Spacer(minLength: 4)
                            Button { Platform.reveal(m.inbox) } label: { Image(systemName: "folder") }.buttonStyle(.launcher(.plain, small: true)).help("Open")
                        }
                        if !m.inbox_items.isEmpty {
                            Text(tr("In it now: {}", ["": m.inbox_items.joined(separator: ", ")])).font(.inter(12.5)).foregroundStyle(Theme.muted)
                        }
                    }
                }
            }
        }
        .onAppear { store.act("mods") }
        .onDrop(of: dropTypes, isTargeted: $dropOver, perform: drop)
        .fileImporter(isPresented: $choosingHofs, allowedContentTypes: [UTType(filenameExtension: "hof") ?? .data], allowsMultipleSelection: true, onCompletion: addHofs)
        .confirmationDialog("Delete this mod?", isPresented: Binding(get: { removing != nil }, set: { if !$0 { removing = nil } }), presenting: removing) { m in
            Button("Delete", role: .destructive) { store.act("mod_remove", ["id": m.id]) }
        } message: { m in
            Text(tr("{name} and its files are deleted from the content folder. Switch it off instead to keep it.", ["name": m.name]))
        }
    }

    private var install: some View {
        Card("Install a mod") {
            HStack(spacing: 8) {
                Button { pick(folder: true) } label: { Label("Folder…", systemImage: "folder.fill") }.buttonStyle(.launcher(.primary, wide: true))
                Button { pick(folder: false) } label: { Label("Archive", systemImage: "archivebox.fill") }.buttonStyle(.launcher(.plain, wide: true))
            }
            Button { choosingHofs = true } label: { Label("Add HOF files to buses", systemImage: "doc.fill") }.buttonStyle(.launcher(.plain, wide: true))
            Field("Archive install mode") {
                Segmented(items: [("auto", "Auto"), ("extract", "Unpacked"), ("inplace", "Used in place")], selection: mode, full: true) { mode = $0 }
            }
            if !lastPath.isEmpty {
                VStack(alignment: .leading, spacing: 4) {
                    Text(lastPath).font(.system(size: 12, design: .monospaced)).foregroundStyle(Theme.text2).lineLimit(1).truncationMode(.middle).textSelection(.enabled)
                    if let r = store.snapshot.mod_info {
                        if let e = r.error {
                            Text(e).font(.inter(12.5)).foregroundStyle(Theme.danger)
                        } else if let i = r.info, i.is_archive {
                            Text(archiveInfo(i)).font(.inter(12.5)).foregroundStyle(i.fits ? Theme.muted : Theme.warn)
                        }
                    }
                }
            }
            if !compact {
                VStack(spacing: 6) {
                    Image(systemName: "square.and.arrow.down").font(.system(size: 24))
                    Text("Drop a folder or an archive here").font(.inter(13))
                }
                .foregroundStyle(dropOver ? Theme.accent : Theme.muted)
                .frame(maxWidth: .infinity).padding(.vertical, 18)
                .overlay(RoundedRectangle(cornerRadius: Theme.radiusSm, style: .continuous).strokeBorder(dropOver ? Theme.accent : Theme.surface3, style: StrokeStyle(lineWidth: 1.5, dash: [6, 4])))
            }
            if let m = store.snapshot.mods {
                Text(tr("Free space: {n}", ["n": formatBytes(m.free_bytes)])).font(.inter(12.5)).foregroundStyle(Theme.muted)
            }
        }
    }

    private func archiveInfo(_ i: ModSourceInfo) -> String {
        let size = tr("{size} archive, {files} files, {unpacked} unpacked", ["size": formatBytes(i.archive_bytes), "files": "\(i.files)", "unpacked": formatBytes(i.unpacked_bytes)])
        let fit = i.fits ? tr("fits ({free} free)", ["free": formatBytes(i.free_bytes)]) : tr("does not fit: needs {needed}, {free} free", ["needed": formatBytes(i.needed_bytes), "free": formatBytes(i.free_bytes)])
        let place = i.in_place_ok ? " - \(String(localized: "can be used in place"))" : (i.in_place.isEmpty ? "" : " - \(i.in_place)")
        return "\(size) - \(fit)\(place)"
    }

    private var jobs: some View {
        let finished = store.snapshot.jobs.contains { !running($0) }
        return Card("Installs", actions: finished ? AnyView(Button("Clear finished") { _ = try? Bridge.api("mods_clear_finished"); store.act("mods") }.buttonStyle(.launcher(.ghost, small: true))) : nil) {
            if store.snapshot.jobs.isEmpty {
                Text("Nothing installed since the launcher started. Big archives are checked against the free disk space before anything is unpacked; a cancelled or failed install leaves nothing behind.").font(.inter(13.5)).foregroundStyle(Theme.muted)
            }
            ForEach(store.snapshot.jobs) { j in
                VStack(alignment: .leading, spacing: 6) {
                    HStack(spacing: 8) {
                        if running(j) { ProgressView().controlSize(.small).tint(Theme.accent) }
                        Text(j.name).font(.inter(14, .medium)).lineLimit(1)
                        Spacer()
                        Tag(text: j.state, kind: j.state == "failed" ? .danger : (j.state == "done" ? .ok : (j.state == "cancelled" ? .plain : .accent)))
                        if running(j) {
                            Button { cancel(j) } label: { Image(systemName: "xmark") }.buttonStyle(.launcher(.ghost, small: true)).help("Cancel")
                        }
                    }
                    if running(j) && (j.bytes_total > 0 || j.files_total > 0) {
                        ProgressBar(value: j.bytes_total > 0 ? Double(j.bytes_done) / Double(j.bytes_total) : Double(j.files_done) / Double(max(1, j.files_total)))
                        Text([j.files_total > 0 ? tr("{done} / {total} files", ["done": "\(j.files_done)", "total": "\(j.files_total)"]) : "", j.bytes_total > 0 ? "\(formatBytes(j.bytes_done)) / \(formatBytes(j.bytes_total))" : ""].filter { !$0.isEmpty }.joined(separator: " - "))
                            .font(.inter(12.5).monospacedDigit()).foregroundStyle(Theme.muted)
                    }
                    if !j.message.isEmpty { Text(j.message).font(.inter(12.5)).foregroundStyle(j.state == "failed" ? Theme.danger : Theme.muted).lineLimit(3) }
                    if !j.warnings.isEmpty { Text(j.warnings.joined(separator: " - ")).font(.inter(12.5)).foregroundStyle(Theme.warn) }
                }
            }
        }
    }

    private func running(_ j: InstallJob) -> Bool { !j.state.isEmpty && !["done", "failed", "cancelled"].contains(j.state) }

    private func cancel(_ j: InstallJob) {
        do { _ = try Bridge.api("mod_cancel", ["id": j.id]) } catch { store.say(error.localizedDescription, error: true) }
        store.act("mods")
    }

    private func category(_ m: ModEntry) -> String {
        let k = (m.kind ?? "other").lowercased()
        return ["bus", "map", "archive"].contains(k) ? k : "other"
    }

    private var installed: some View {
        let mods = store.snapshot.mods?.installed ?? []
        var counts = ["all": mods.count, "bus": 0, "map": 0, "archive": 0, "other": 0, "off": 0]
        for m in mods {
            counts[category(m), default: 0] += 1
            if !m.enabled { counts["off", default: 0] += 1 }
        }
        let shownKind = (counts[kind] ?? 0) > 0 ? kind : "all"
        let shown = mods.filter { m in
            (search.isEmpty || "\(m.name) \(m.paths.joined(separator: " "))".localizedCaseInsensitiveContains(search))
                && (shownKind == "all" || (shownKind == "off" ? !m.enabled : category(m) == shownKind))
        }
        .sorted { a, b in a.enabled != b.enabled ? a.enabled : a.name.localizedCompare(b.name) == .orderedAscending }
        let tabs: [(String, String)] = [("all", "All"), ("bus", "Buses"), ("map", "Maps"), ("archive", "Archives"), ("other", "Other"), ("off", "Off")]
        return Card("Installed mods", hint: "\(tr("{n} mods, {on} on", ["n": "\(mods.count)", "on": "\(mods.count - (counts["off"] ?? 0))"])) - \(tr("Free space: {n}", ["n": formatBytes(store.snapshot.mods?.free_bytes ?? 0)]))") {
            SearchField(prompt: "Search mods...", text: $search)
            if !mods.isEmpty {
                Scroll(.horizontal) {
                    Segmented(items: tabs.filter { $0.0 == "all" || (counts[$0.0] ?? 0) > 0 }.map { ($0.0, LocalizedStringKey("\(String(localized: String.LocalizationValue($0.1))) \(counts[$0.0] ?? 0)")) }, selection: shownKind) { kind = $0 }
                }
                .scrollIndicators(.never)
            }
            if shown.isEmpty {
                EmptyState(icon: "puzzlepiece.extension.fill", text: search.isEmpty && shownKind == "all" ? "No mods yet. Choose a folder or an archive." : "No mod matches.")
            }
            VStack(spacing: 0) {
                ForEach(shown) { m in row(m) }
            }
            if let dir = store.snapshot.mods?.content_dir {
                HStack {
                    Text(dir).font(.inter(12)).foregroundStyle(Theme.muted).lineLimit(1).truncationMode(.middle)
                    Spacer()
                    Button { Platform.reveal(dir) } label: { Label("Open", systemImage: "arrow.up.forward.square") }.buttonStyle(.launcher(.ghost, small: true))
                }
            }
        }
    }

    private func row(_ m: ModEntry) -> some View {
        let tree = modTree(m.paths)
        let holds = m.kind == "archive" ? "" : tree.filter { !$0.0.isEmpty }.map { "\($0.0) \($0.1.count)" }.joined(separator: ", ")
        let kindLabel = ["bus": "Bus", "map": "Map", "archive": "Archive"][category(m)] ?? "Other"
        let sub = [String(localized: String.LocalizationValue(kindLabel)), holds, formatBytes(m.bytes ?? 0), m.installed.map { Date(timeIntervalSince1970: TimeInterval($0)).formatted(.relative(presentation: .named)) } ?? (m.noted == true ? "" : String(localized: "found in the content folder"))]
        return VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 12) {
                Image(systemName: icon(m)).font(.system(size: 15)).foregroundStyle(m.enabled ? Theme.text2 : Theme.muted).frame(width: 22)
                VStack(alignment: .leading, spacing: 2) {
                    Text(m.name).font(.inter(14, .medium)).foregroundStyle(m.enabled ? Theme.text : Theme.muted).lineLimit(1)
                    Text(sub.filter { !$0.isEmpty }.joined(separator: " - ")).font(.inter(12.5)).foregroundStyle(Theme.muted).lineLimit(1)
                }
                Spacer()
                if store.snapshot.mod_busy == m.id {
                    ProgressView().controlSize(.small).tint(Theme.accent)
                } else {
                    Toggle("", isOn: Binding(get: { m.enabled }, set: { store.act("mod_toggle", ["id": m.id, "on": $0]) })).labelsHidden().toggleStyle(.launcher)
                    Button { removing = m } label: { Image(systemName: "trash") }
                        .buttonStyle(.launcher(.ghost, small: true)).help("Delete this mod (asks first; switch it off instead to keep it)")
                }
            }
            .padding(.horizontal, 4).padding(.vertical, 8)
            .contentShape(Rectangle())
            .onTapGesture { open = open == m.id ? nil : m.id }
            if open == m.id {
                VStack(alignment: .leading, spacing: 6) {
                    ForEach(tree.indices, id: \.self) { i in
                        HStack(alignment: .firstTextBaseline, spacing: 12) {
                            Text(tree[i].0).font(.inter(12.5, .semibold)).foregroundStyle(Theme.muted).frame(minWidth: 96, alignment: .leading)
                            Text(tree[i].1.joined(separator: ", ")).font(.inter(12.5)).foregroundStyle(Theme.text2).fixedSize(horizontal: false, vertical: true)
                        }
                    }
                }
                .textSelection(.enabled)
                .padding(.leading, 38).padding(.trailing, 12).padding(.bottom, 12)
            }
        }
    }

    private func icon(_ m: ModEntry) -> String {
        switch (m.kind ?? "").lowercased() {
        case "bus": "bus.fill"
        case "map": "map.fill"
        case "archive": "archivebox.fill"
        default: "puzzlepiece.extension.fill"
        }
    }

    private func pick(folder: Bool) {
        Platform.pick(folder: folder, types: folder ? [] : ["zip", "7z", "rar"]) { url in
            guard let url else { return }
            Task { @MainActor in installFrom(url) }
        }
    }

    private func installFrom(_ url: URL) {
        let path = Platform.importForInstall(url)
        lastPath = path
        store.act("mod_install", ["path": path, "mode": mode])
    }

    private func addHofs(_ result: Result<[URL], Error>) {
        guard case .success(let urls) = result, !urls.isEmpty else { return }
        let open = urls.filter { $0.startAccessingSecurityScopedResource() }
        defer { open.forEach { $0.stopAccessingSecurityScopedResource() } }
        do {
            let added = try JSONDecoder().decode(Int.self, from: Bridge.api("add_hofs", ["files": urls.map(\.path)]))
            store.say(added > 0 ? String(localized: "HOF files added for every bus.") : String(localized: "These HOF files were already there."), error: false)
        } catch {
            store.say(error.localizedDescription, error: true)
        }
        store.act("mods")
        store.act("reload")
    }

    private var dropTypes: [UTType] {
        #if os(macOS)
        [.fileURL]
        #else
        [.data]
        #endif
    }

    private func drop(_ providers: [NSItemProvider]) -> Bool {
        guard let p = providers.first else { return false }
        #if os(macOS)
        _ = p.loadObject(ofClass: URL.self) { url, _ in
            guard let url else { return }
            Task { @MainActor in installFrom(url) }
        }
        #else
        guard let inbox = store.snapshot.mods?.inbox, !inbox.isEmpty else { return false }
        _ = p.loadFileRepresentation(forTypeIdentifier: UTType.data.identifier) { url, error in
            let failure: String?
            if let url {
                do {
                    try FileManager.default.copyItem(at: url, to: URL(fileURLWithPath: inbox).appendingPathComponent(url.lastPathComponent))
                    failure = nil
                } catch { failure = error.localizedDescription }
            } else {
                failure = error?.localizedDescription
            }
            Task { @MainActor in
                if let failure { store.say(failure, error: true) }
                store.act("mods")
            }
        }
        #endif
        return true
    }
}

func modTree(_ paths: [String]) -> [(String, [String])] {
    var out: [(String, [String])] = []
    for p in paths {
        let parts = p.split(separator: "/", maxSplits: 1).map(String.init)
        let (folder, rest) = parts.count == 2 && !parts[0].isEmpty ? (parts[0], parts[1]) : ("", p)
        if let i = out.firstIndex(where: { $0.0.lowercased() == folder.lowercased() }) { out[i].1.append(rest) } else { out.append((folder, [rest])) }
    }
    return out
}

// --- Tutorials -------------------------------------------------------------------------------

struct TutorialsPage: View {
    @Environment(LauncherStore.self) private var store
    @Environment(\.compactLayout) private var compact

    var body: some View {
        PageScroll("Tutorials", subtitle: "OMSI 2's lessons: driving the bus, the stops, the ticket printer and the timetable.") {
            if store.tutorials.isEmpty {
                Card { EmptyState(icon: "book.fill", title: "No tutorial found", text: "The tutorials come with OMSI 2 (its Tutorials folder).") }
            }
            LazyVGrid(columns: compact ? [GridItem(.flexible())] : [GridItem(.flexible(), spacing: 16), GridItem(.flexible(), spacing: 16)], alignment: .leading, spacing: 16) {
                ForEach(store.tutorials) { t in
                    Card("\(t.n). \(t.title)") {
                        Text(t.text).font(.inter(14)).foregroundStyle(Theme.text2).lineLimit(6).frame(maxWidth: .infinity, minHeight: 60, alignment: .topLeading)
                        Button { store.tutorial(t.n) } label: { Label("Start", systemImage: "play.fill") }.buttonStyle(.primary)
                            .disabled(store.snapshot.in_game || store.starting)
                    }
                }
            }
        }
        .onAppear { store.loadTutorials() }
    }
}

// --- Setup -----------------------------------------------------------------------------------

struct SetupPage: View {
    @Environment(LauncherStore.self) private var store
    @State private var root = ""

    var body: some View {
        PageScroll("Setup", subtitle: "openOMSI plays the content of an OMSI 2 installation: the original game must be installed (Steam or another copy).", maxWidth: 900) {
            Card("Folders") {
                if !store.snapshot.root_ok {
                    Notice(text: store.snapshot.root_problem.isEmpty ? String(localized: "The original OMSI 2 was not found.") : store.snapshot.root_problem, kind: .danger)
                }
                Field("OMSI 2 folder", help: Platform.isPhone
                    ? String(localized: "Copy a complete OMSI 2 (the folder with Omsi.exe, maps and Vehicles) into openOMSI in the Files app (On My iPhone / iPad), or from a Mac's Finder over a cable, and choose it here. A copy on iCloud Drive or a USB drive can be chosen as well.")
                    : String(localized: "The OMSI 2 folder is the one with maps and Vehicles in it (any complete installation).")) {
                    HStack(spacing: 8) {
                        InputField(prompt: "/path/to/OMSI 2", text: $root) { store.act("root", ["path": root]) }
                        Button("Browse") {
                            Platform.pick(folder: true) { url in
                                guard let url else { return }
                                Task { @MainActor in
                                    root = Platform.keepAccess(url)
                                    store.act("root", ["path": root])
                                }
                            }
                        }
                        .buttonStyle(.secondary)
                        Button { store.act("root", ["path": root]) } label: { Label("Save", systemImage: "checkmark") }.buttonStyle(.primary)
                    }
                }
            }
            Card("Where things are") {
                Facts(rows: [
                    ("OMSI 2", store.snapshot.config.root.isEmpty ? "-" : store.snapshot.config.root),
                    ("Content folder", store.snapshot.content_dir ?? "-"),
                    ("Settings and logs", store.snapshot.data_dir.isEmpty ? "-" : store.snapshot.data_dir),
                    ("Driver", store.snapshot.config.profile.isEmpty ? "-" : store.snapshot.config.profile),
                    ("Version", "\(store.version) (\(store.build))"),
                ])
                HStack(spacing: 8) {
                    if let c = store.snapshot.content_dir {
                        Button { Platform.reveal(c) } label: { Label("Show", systemImage: "folder") }.buttonStyle(.secondary)
                        Button { store.act("reload") } label: { Label("Read the content again", systemImage: "arrow.clockwise") }.buttonStyle(.secondary)
                    }
                    #if os(macOS)
                    if !store.snapshot.data_dir.isEmpty {
                        Button { NSWorkspace.shared.open(URL(fileURLWithPath: store.snapshot.data_dir)) } label: { Label("Open the settings folder", systemImage: "folder") }.buttonStyle(.secondary)
                    }
                    #endif
                }
            }
        }
        .onAppear { root = store.snapshot.config.root }
    }
}
