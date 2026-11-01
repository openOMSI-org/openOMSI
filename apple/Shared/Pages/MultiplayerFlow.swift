import SwiftUI
#if os(iOS)
import UIKit
#else
import AppKit
#endif

func tr(_ key: String, _ args: [String: String] = [:]) -> String {
    args.reduce(String(localized: String.LocalizationValue(key))) { $0.replacingOccurrences(of: "{\($1.key)}", with: $1.value) }
}

func copyText(_ text: String) {
    #if os(iOS)
    UIPasteboard.general.string = text
    #else
    NSPasteboard.general.clearContents()
    NSPasteboard.general.setString(text, forType: .string)
    #endif
}

struct HostStatus: Decodable {
    var name = ""
    var map = ""
    var players = 0
    var max_players = 0
    var time: String?
    var weather: String?
    var icon: String?
    var reached_at: String?
}

struct JoinCheck: Decodable {
    var ok: Bool
    var text: String
}

func offMain<T: Sendable>(_ work: @escaping @Sendable () throws -> T) async throws -> T {
    try await Task.detached { try work() }.value
}

struct FlowSteps: View {
    var labels: [LocalizedStringKey]
    var current: Int

    var body: some View {
        ViewThatFits(in: .horizontal) {
            HStack(spacing: 14) { steps }
            VStack(alignment: .leading, spacing: 8) { steps }
        }
        .accessibilityElement(children: .combine)
        .accessibilityLabel(Text("Connection progress"))
    }

    private var steps: some View {
        ForEach(labels.indices, id: \.self) { i in
            HStack(spacing: 8) {
                Text(verbatim: i < current ? "✓" : "\(i + 1)").font(.inter(12, .semibold))
                    .foregroundStyle(i <= current ? Theme.accentText : Theme.muted)
                    .frame(width: 22, height: 22)
                    .background(i <= current ? Theme.accent : Theme.surface3, in: Circle())
                Text(labels[i]).font(.inter(13.5, i == current ? .semibold : .regular)).foregroundStyle(i <= current ? Theme.text : Theme.muted).lineLimit(1)
            }
        }
    }
}

struct FirstTimeHelp: View {
    var steps: [LocalizedStringKey]
    @State private var open = false

    var body: some View {
        Card {
            DisclosureGroup(isExpanded: $open) {
                VStack(alignment: .leading, spacing: 6) {
                    ForEach(steps.indices, id: \.self) { i in
                        HStack(alignment: .top, spacing: 8) {
                            Text(verbatim: "\(i + 1).").font(.inter(13)).foregroundStyle(Theme.muted)
                            Text(steps[i]).font(.inter(13)).foregroundStyle(Theme.muted).fixedSize(horizontal: false, vertical: true)
                        }
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.top, 8)
            } label: {
                Text("First time here?").font(.inter(14, .medium)).foregroundStyle(Theme.text)
            }
            .tint(Theme.muted)
        }
    }
}

struct JoinByCode: View {
    @Environment(LauncherStore.self) private var store
    @State private var code = ""
    @State private var result: Result?
    @State private var request = 0

    struct Result {
        var code: String
        var busy = false
        var validated = false
        var error: String?
        var unavailable: String?
        var host: HostStatus?
    }

    var body: some View {
        let current = result?.code == code.trimmingCharacters(in: .whitespacesAndNewlines) ? result : nil
        let host = current?.host
        let full = host.map { $0.max_players > 0 && $0.players >= $0.max_players } ?? false
        let ready = host != nil || (current?.validated == true && current?.busy == false)
        let mapFile = host?.map.replacingOccurrences(of: "\\", with: "/") ?? ""
        let installed = store.lists.maps.first { $0.file.lowercased() == mapFile.lowercased() }
        VStack(spacing: 16) {
            Card(padded: false) {
                FlowSteps(labels: ["Enter session code", "Confirm session", "Choose a bus and start"], current: ready ? 2 : 0)
                    .padding(.horizontal, 20).padding(.top, 18)
                VStack(alignment: .leading, spacing: 14) {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("Join friends").font(.inter(17, .semibold))
                        Text("Confirm the session first, then choose your bus on the Drive page. The game has not connected yet.").font(.inter(13)).foregroundStyle(Theme.muted)
                        Text("Sessions and servers are run by players, not by openOMSI. The host receives your driver name, network address, chat and voice, and may keep logs.").font(.inter(13)).foregroundStyle(Theme.muted)
                    }
                    Field("Session code", help: String(localized: "Paste the complete code from your friend. Leading and trailing spaces are removed.")) {
                        HStack(spacing: 8) {
                            InputField(prompt: "OMSI-XXXX-XXXX-...", text: $code, mono: true) { confirm() }
                            Button { confirm() } label: { Label(host != nil ? "Check again" : "Confirm session", systemImage: "magnifyingglass") }
                                .buttonStyle(.launcher(host != nil || current?.unavailable != nil ? .plain : .primary))
                                .disabled(code.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || current?.busy == true)
                        }
                    }
                    if current?.busy == true {
                        HStack(spacing: 8) {
                            ProgressView().controlSize(.small).tint(Theme.accent)
                            Text("Checking the session...").font(.inter(13)).foregroundStyle(Theme.muted)
                        }
                        Text("You can choose a bus without waiting for session details. The game connects when you start driving.").font(.inter(13)).foregroundStyle(Theme.muted)
                    }
                    if let e = current?.error {
                        Notice(text: "\(String(localized: "Could not validate the code"))\n\(e)", kind: .danger)
                    }
                    if let e = current?.unavailable {
                        Notice(text: "\(String(localized: "Could not read the session details"))\n\(String(localized: "Ask your friend to keep the game running and send the latest code. Retry, or continue without session details."))", kind: .warn)
                        DisclosureGroup {
                            Text(e).font(.inter(12.5)).foregroundStyle(Theme.muted).textSelection(.enabled).frame(maxWidth: .infinity, alignment: .leading)
                        } label: {
                            Text("Technical details").font(.inter(13)).foregroundStyle(Theme.text2)
                        }
                        .tint(Theme.muted)
                    }
                    if let h = host {
                        VStack(alignment: .leading, spacing: 10) {
                            HStack(spacing: 8) {
                                Image(systemName: "person.2.fill").foregroundStyle(Theme.muted)
                                Text(h.name).font(.inter(15, .semibold)).lineLimit(1)
                                Tag(text: tr("{n} of {max} players", ["n": "\(h.players)", "max": "\(h.max_players)"]))
                            }
                            Facts(rows: [("Map", installed?.title ?? (h.map.isEmpty ? String(localized: "Unknown") : h.map))]
                                + (h.time.map { [("Time", $0)] } ?? []) + (h.weather.map { [("Weather", $0)] } ?? []))
                            Text("Choose your bus next. The game connects only when you start driving.").font(.inter(13)).foregroundStyle(Theme.muted)
                        }
                        .accessibilityElement(children: .contain)
                        .accessibilityLabel(Text("Session details"))
                        .padding(14)
                        .background(Theme.surface2, in: RoundedRectangle(cornerRadius: Theme.radiusSm, style: .continuous))
                    }
                    if full {
                        Notice(text: String(localized: "The session is full. Ask your friend to make room, then confirm again."), kind: .warn)
                    }
                    if !mapFile.isEmpty && installed == nil {
                        Notice(text: tr("The server plays {}, which is not installed here: it is fetched from the server on joining.", ["": host?.map ?? ""]), kind: .warn)
                    }
                    if !full, let c = current, host != nil || c.unavailable != nil || c.validated {
                        Button { join(c, map: installed?.file) } label: { Label(ready ? "Choose a bus" : "Choose a bus without session details", systemImage: "bus.fill") }
                            .buttonStyle(.launcher(ready ? .primary : .plain))
                    }
                }
                .padding(20)
            }
            FirstTimeHelp(steps: ["Ask your friend to start a hosted duty and copy the session code.", "Paste the code here and confirm the session.", "Choose your bus on the Drive page, then press Join and drive."])
        }
        .onAppear { if code.isEmpty && store.choice.lan_mode == "join" { code = store.choice.lan_addr } }
        .onChange(of: code) { request += 1; result = nil }
        .onDisappear { request += 1 }
    }

    private func confirm() {
        let asked = code.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !asked.isEmpty, result?.busy != true else { return }
        request += 1
        let id = request
        result = Result(code: asked, busy: true)
        Task { @MainActor in
            do {
                let check = try await offMain { try Bridge.api(JoinCheck.self, "check_join", ["text": asked]) }
                guard id == request else { return }
                if !check.ok { result = Result(code: asked, error: check.text); return }
                if asked.range(of: "^OMSI-", options: [.regularExpression, .caseInsensitive]) == nil { result = Result(code: asked, validated: true); return }
                result = Result(code: asked, busy: true, validated: true)
                do {
                    let host = try await offMain { try Bridge.api(HostStatus.self, "server_query", ["address": asked]) }
                    if id == request { result = Result(code: asked, host: host) }
                } catch {
                    if id == request { result = Result(code: asked, unavailable: error.localizedDescription) }
                }
            } catch {
                if id == request { result = Result(code: asked, error: error.localizedDescription) }
            }
        }
    }

    private func join(_ c: Result, map: String?) {
        request += 1
        result = nil
        if let map { store.act("select_map", ["file": map]) }
        store.act("lan", ["mode": "join", "addr": c.code])
        if c.code.contains(".") || c.code.contains(":") { store.act("server_ask", ["address": c.code, "every": 10]) }
        store.page = .drive
        store.phoneTab = "drive"
    }
}

struct HostByCode: View {
    @Environment(LauncherStore.self) private var store
    #if os(iOS)
    private static let copyStep: LocalizedStringKey = "While driving, open the game menu and copy the server code there."
    #else
    private static let copyStep: LocalizedStringKey = "Return to Multiplayer to copy your running session code."
    #endif

    var body: some View {
        let session = runningHost(store.snapshot.instances)
        let code = session?["code"]?.string ?? ""
        let hosting = store.choice.lan_mode == "host"
        VStack(spacing: 16) {
            Card(padded: false) {
                FlowSteps(labels: ["Set up a hosted duty", "Start the duty", "Copy code"], current: !code.isEmpty ? 2 : hosting ? 1 : 0)
                    .padding(.horizontal, 20).padding(.top, 18)
                VStack(alignment: .leading, spacing: 14) {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("Invite friends").font(.inter(17, .semibold))
                        Text("Start your hosted duty first. A session code appears only after the game starts.").font(.inter(13)).foregroundStyle(Theme.muted)
                        Text("As the host, your game receives your players' driver names, network addresses, chat and voice. You are responsible for what you keep of it, such as logs and bans.").font(.inter(13)).foregroundStyle(Theme.muted)
                    }
                    if !code.isEmpty {
                        SessionCode(code: code)
                        Notice(text: session?["tunnel"]?.bool == true ? String(localized: "Friends can join from anywhere") : String(localized: "Getting ready for friends on the internet..."), kind: session?["tunnel"]?.bool == true ? .info : .warn)
                        Button { store.page = .games; store.phoneTab = "more"; store.phoneMore = .games } label: { Label("Running games", systemImage: "gamecontroller.fill") }
                            .buttonStyle(.secondary)
                    } else {
                        Text("Choose your map, bus and duty on the Drive page. No game starts until you press Start the duty.").font(.inter(13)).foregroundStyle(Theme.muted)
                        HStack(spacing: 8) {
                            Button { prepare() } label: { Label("Set up a hosted duty", systemImage: "bus.fill") }.buttonStyle(.primary)
                            if hosting {
                                Button("Cancel") { store.act("lan", ["mode": "off"]) }.buttonStyle(.ghost)
                            }
                        }
                    }
                }
                .padding(20)
            }
            FirstTimeHelp(steps: ["Choose your map, bus and duty, then start the game as host.", Self.copyStep, "Send the complete code to your friends; they choose Join friends."])
        }
    }

    private func prepare() {
        store.act("lan", ["mode": "host"])
        store.page = .drive
        store.phoneTab = "drive"
        #if os(macOS)
        Task.detached {
            if (try? Bridge.api("prepare_tunnel")) == nil {
                await MainActor.run { store.say(String(localized: "Internet preparation could not be started. You can still set up a LAN game."), error: true) }
            }
        }
        #endif
    }
}

struct SessionCode: View {
    @Environment(LauncherStore.self) private var store
    var code: String

    var body: some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 4) {
                SectionTitle("Session code")
                Text(code).font(.system(size: 15, weight: .semibold, design: .monospaced)).foregroundStyle(Theme.text).textSelection(.enabled)
                Text("Your friends paste it into Multiplayer → Connect by Code.").font(.inter(12.5)).foregroundStyle(Theme.muted)
            }
            Spacer(minLength: 8)
            Button { copyText(code); store.say(String(localized: "Session code copied"), error: false) } label: { Label("Copy code", systemImage: "doc.on.doc") }
                .buttonStyle(.primary)
        }
        .padding(14)
        .background(Theme.surface2, in: RoundedRectangle(cornerRadius: Theme.radiusSm, style: .continuous))
    }
}

func runningHost(_ instances: [Instance]) -> JSON? {
    instances.filter { $0.running != false }.compactMap(\.lan_status).first { $0["role"]?.string == "host" }
}
