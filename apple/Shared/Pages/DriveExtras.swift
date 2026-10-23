import SwiftUI

func favKey(_ file: String) -> String { file.replacingOccurrences(of: "\\", with: "/").lowercased() }

func busFile(_ file: String) -> String {
    let n = favKey(file)
    guard let r = n.range(of: "vehicles/", options: .backwards), r.lowerBound > n.startIndex else { return n }
    return String(n[r.lowerBound...])
}

func longDate(_ iso: String) -> String {
    let f = DateFormatter()
    f.dateFormat = "yyyy-MM-dd"
    f.locale = Locale(identifier: "en_US_POSIX")
    guard let d = f.date(from: iso) else { return iso }
    return d.formatted(date: .long, time: .omitted)
}

func entryName(_ e: EntryInfo) -> String {
    let n = e.name.trimmingCharacters(in: .whitespaces)
    return n.isEmpty ? tr("Entry point {n}", ["n": "\(e.index + 1)"]) : n
}

extension LauncherStore {
    var serverBuses: Set<String>? {
        guard choice.lan_mode == "join", let v = snapshot.host_vehicles, !v.isEmpty else { return nil }
        return Set(v.map(busFile))
    }

    var offeredVehicles: [VehicleInfo] {
        guard let s = serverBuses else { return lists.vehicles }
        return lists.vehicles.filter { s.contains(busFile($0.file)) }
    }

    var noServerBus: Bool { serverBuses != nil && !lists.vehicles.isEmpty && offeredVehicles.isEmpty }

    var busOffered: Bool { serverBuses?.contains(busFile(choice.bus)) ?? true }
}

@MainActor
@Observable
final class DriveData {
    static let shared = DriveData()
    var favourites: [String] = []
    var notes: [String: String] = [:]
    @ObservationIgnored private var loaded = false
    @ObservationIgnored private var ready = false
    @ObservationIgnored private var saving: [String: Task<Void, Never>] = [:]

    private struct Boot: Decodable { var favourites: [String]? }

    func load() {
        guard !loaded else { return }
        loaded = true
        Task { @MainActor in
            if let b = try? await offMain({ try Bridge.api(Boot.self, "bootstrap") }) {
                favourites = b.favourites ?? []
                ready = true
            }
            if let n = try? await offMain({ try Bridge.api([String: String].self, "notes_get") }) { notes = n.merging(notes) { _, mine in mine } }
        }
    }

    func starred(_ file: String) -> Bool { favourites.contains(favKey(file)) }

    func toggle(_ file: String) {
        guard ready else { return }
        let k = favKey(file)
        favourites = starred(file) ? favourites.filter { $0 != k } : favourites + [k]
        _ = try? Bridge.api("favourites_save", ["list": favourites])
    }

    func setNote(_ key: String, _ text: String) {
        notes[key] = text
        saving[key]?.cancel()
        saving[key] = Task {
            try? await Task.sleep(nanoseconds: 600_000_000)
            if Task.isCancelled { return }
            _ = try? Bridge.api("notes_save", ["key": key, "text": text])
        }
    }
}

struct NotesBox: View {
    var kind: String
    var file: String
    @State private var data = DriveData.shared
    @State private var open = false

    var body: some View {
        let key = "\(kind):\(favKey(file))"
        let text = data.notes[key] ?? ""
        Group {
            if open || !text.isEmpty {
                TextEditor(text: Binding(get: { data.notes[key] ?? "" }, set: { data.setNote(key, $0) }))
                    .font(.inter(14))
                    .foregroundStyle(Theme.text)
                    .scrollContentBackground(.hidden)
                    .frame(minHeight: 64, maxHeight: 120)
                    .padding(.horizontal, 6).padding(.vertical, 4)
                    .fieldBackground()
                    .overlay(alignment: .topLeading) {
                        if text.isEmpty {
                            Text("Your own note, kept on this computer").font(.inter(14)).foregroundStyle(Theme.muted)
                                .padding(.horizontal, 11).padding(.vertical, 12).allowsHitTesting(false)
                        }
                    }
            } else {
                Button { open = true } label: { Label("Add a note", systemImage: "note.text") }
                    .buttonStyle(.launcher(.ghost, small: true))
            }
        }
        .onAppear { data.load(); open = !text.isEmpty }
        .onChange(of: key) { open = !(data.notes[key] ?? "").isEmpty }
        .onChange(of: text.isEmpty) { _, empty in if !empty { open = true } }
    }
}

struct StarButton: View {
    var file: String
    @State private var data = DriveData.shared

    var body: some View {
        let on = data.starred(file)
        Button { data.toggle(file); Haptics.tap() } label: {
            Image(systemName: on ? "star.fill" : "star").font(.system(size: 14, weight: .semibold))
                .foregroundStyle(on ? Theme.accent : Theme.muted)
                .frame(width: 34, height: 34).contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(LocalizedStringKey(on ? "Remove from the favourites" : "Add to the favourites"))
        .onAppear { data.load() }
    }
}

struct MultiplayerDutyNotice: View {
    @Environment(LauncherStore.self) private var store
    @Environment(\.compactLayout) private var compact

    var body: some View {
        let c = store.choice
        if c.lan_mode == "host" || c.lan_mode == "join" {
            let host = c.lan_mode == "host"
            let info = HStack(alignment: .center, spacing: 12) {
                Image(systemName: "person.2.fill").font(.system(size: 15)).foregroundStyle(Theme.accent)
                VStack(alignment: .leading, spacing: 3) {
                    Text(LocalizedStringKey(host ? "Host my next duty" : "Choose a bus and start")).font(.inter(14, .semibold))
                    Text(LocalizedStringKey(host ? "Start the duty to create your session, then return to Multiplayer to copy the code." : "The session is selected, not connected yet. Choose your bus and press Join and drive."))
                        .font(.inter(13)).foregroundStyle(Theme.muted).fixedSize(horizontal: false, vertical: true)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            let buttons = Group {
                Button("Multiplayer") { store.page = .multiplayer; store.phoneTab = "multiplayer" }
                    .buttonStyle(.launcher(.plain, small: true))
                Button("Cancel") { store.act("lan", ["mode": "off", "addr": ""]) }
                    .buttonStyle(.launcher(.ghost, small: true))
            }
            Group {
                if compact {
                    VStack(alignment: .leading, spacing: 10) {
                        info
                        HStack(spacing: 8) { buttons }
                    }
                } else {
                    HStack(alignment: .center, spacing: 12) { info; buttons }
                }
            }
            .padding(.horizontal, 14).padding(.vertical, 12)
            .background(Theme.surface2, in: RoundedRectangle(cornerRadius: Theme.radiusSm, style: .continuous))
        }
    }
}

struct SaveDelete: ViewModifier {
    @Environment(LauncherStore.self) private var store
    @Binding var item: SavedSituation?

    func body(content: Content) -> some View {
        content.confirmationDialog("Delete the save", isPresented: Binding(get: { item != nil }, set: { if !$0 { item = nil } }), presenting: item) { x in
            Button("Delete", role: .destructive) {
                do {
                    if try !JSONDecoder().decode(Bool.self, from: Bridge.call("act", ["do": "delete_situation", "file": x.file])) { store.say(String(localized: "Nothing found"), error: true) }
                } catch {
                    store.say(error.localizedDescription, error: true)
                }
                store.tick()
            }
            Button("Cancel", role: .cancel) {}
        } message: { x in
            Text(tr("Delete \"{name}\"? It cannot be brought back.", ["name": String(localized: String.LocalizationValue(x.name))]))
        }
    }
}

struct DeleteSaveButton: View {
    var action: () -> Void
    var body: some View {
        Button(action: action) {
            Image(systemName: "trash").font(.system(size: 13)).foregroundStyle(Theme.muted)
                .frame(width: 34, height: 34).contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help("Delete")
    }
}
