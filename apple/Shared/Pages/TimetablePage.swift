import Observation
import SwiftUI

struct TTData: Codable {
    var folder: String
    var lines: [TTLine]
    var trips: [TTTripInfo]
}

struct TTLine: Codable {
    var name: String
    var user_allowed: Bool
    var priority: Int
    var tours: [TTTour]
}

struct TTTour: Codable {
    var number: String
    var ai_group: String
    var extra: String
    var trips: [TTTrip]
}

struct TTTrip: Codable {
    var trip: String
    var profile: Int
    var departure: Double
}

struct TTTripInfo: Codable {
    var name: String
    var line: String
    var terminus: String
    var profiles: [String]
}

func ttTime(_ min: Double) -> String {
    let s = Int((min * 60).rounded())
    let hm = "\(s / 3600):\(String(format: "%02d", (s / 60) % 60))"
    return s % 60 == 0 ? hm : "\(hm):\(String(format: "%02d", s % 60))"
}

func ttParse(_ text: String) -> Double? {
    let parts = text.trimmingCharacters(in: .whitespaces).split(separator: ":", omittingEmptySubsequences: false).flatMap { $0.split(separator: ".", omittingEmptySubsequences: false) }
    guard (2...3).contains(parts.count) else { return nil }
    let n = parts.compactMap { p -> Double? in
        let t = p.trimmingCharacters(in: .whitespaces)
        return !t.isEmpty && t.allSatisfy(\.isASCII) && t.allSatisfy(\.isNumber) ? Double(t) : nil
    }
    guard n.count == parts.count else { return nil }
    let s = n.count == 3 ? n[2] : 0
    return n[0] < 48 && n[1] < 60 && s < 60 ? n[0] * 60 + n[1] + s / 60 : nil
}

private func ttSame(_ a: String, _ b: String) -> Bool {
    a.trimmingCharacters(in: .whitespaces).lowercased() == b.trimmingCharacters(in: .whitespaces).lowercased()
}

private func nextNumber(_ tours: [TTTour]) -> String {
    let nums = tours.compactMap { Int($0.number.trimmingCharacters(in: .whitespaces)) }
    return String((nums.max() ?? 0) + 1)
}

private func fileNameOk(_ name: String) -> Bool {
    let bad = CharacterSet(charactersIn: "/\\:*?\"<>|").union(.controlCharacters)
    let stem = (name.split(separator: ".", omittingEmptySubsequences: false).first.map(String.init) ?? "").replacingOccurrences(of: "\\s+$", with: "", options: .regularExpression).uppercased()
    let reserved = stem.range(of: "^(CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])$", options: .regularExpression) != nil
    return !name.isEmpty && name.rangeOfCharacter(from: bad) == nil && !name.hasSuffix(".") && !name.hasSuffix(" ") && !reserved
}

@MainActor
@Observable
final class TimetableEditor {
    static let shared = TimetableEditor()

    var map: String?
    var data: TTData?
    var error: String?
    var line: String?
    var tour = 0
    var dirty: [String] = []
    var ver: [String: Int] = [:]
    var busy = false
    var drafts: [String: String] = [:]
    var offset = "20"
    var until = "22:00"
    var newLine = ""
    var armed = false
    var level = 0

    var lineIndex: Int? { data?.lines.firstIndex { $0.name == line } }

    func read(_ m: String) {
        map = m
        data = nil
        error = nil
        line = nil
        tour = 0
        dirty = []
        ver = [:]
        drafts = [:]
        armed = false
        level = 0
        Task { @MainActor in
            do {
                let d = try await offMain { try Bridge.api(TTData.self, "timetable", ["map": m]) }
                guard map == m else { return }
                data = d
                line = d.lines.first?.name
            } catch {
                if map == m { self.error = error.localizedDescription }
            }
        }
    }

    func touch(_ name: String) {
        ver[name, default: 0] += 1
        if !dirty.contains(name) { dirty.append(name) }
    }

    func saveAll(_ store: LauncherStore) async -> Bool {
        guard !busy, let m = map, let d = data else { return false }
        if drafts.values.contains(where: { ttParse($0) == nil }) {
            store.say(String(localized: "A departure is not a time (h:mm)."), error: true)
            return false
        }
        let lines = d.lines.filter { dirty.contains($0.name) }
        let sent = ver
        busy = true
        defer { busy = false }
        do {
            let body = try JSONEncoder().encode(lines)
            let reply = try await offMain { try Bridge.api("timetable_save", ["map": m, "lines": JSONSerialization.jsonObject(with: body)]) }
            let r = try JSONSerialization.jsonObject(with: reply) as? [String: Any] ?? [:]
            let saved = r["saved"] as? [String] ?? []
            let done = { (n: String) in saved.contains(n) && self.ver[n] == sent[n] }
            if map == m, data != nil {
                for i in data!.lines.indices where done(data!.lines[i].name) {
                    for k in data!.lines[i].tours.indices { data!.lines[i].tours[k].trips.sort { $0.departure < $1.departure } }
                }
                if lines.allSatisfy({ ver[$0.name] == sent[$0.name] }) { drafts = [:] }
                dirty.removeAll(where: done)
            }
            if !saved.isEmpty { store.act("lines") }
            if let e = r["error"] as? String {
                store.say(tr("Not saved: {e}", ["e": e]), error: true)
                return false
            }
            store.say(tr("{n} line(s) saved", ["n": "\(saved.count)"]), error: false)
            return true
        } catch {
            store.say(tr("Not saved: {e}", ["e": error.localizedDescription]), error: true)
            return false
        }
    }

    func switchMap(_ m: String, _ store: LauncherStore) {
        guard !busy else { return }
        Task { @MainActor in
            if !dirty.isEmpty, !(await saveAll(store)) { return }
            read(m)
        }
    }

    func reset(_ store: LauncherStore) {
        guard !busy, let m = map else { return }
        if !armed {
            armed = true
            store.say(String(localized: "Press \"Reset timetable\" again to put the map's own timetable back (the changes made here are lost)"), error: false)
            return
        }
        armed = false
        busy = true
        Task { @MainActor in
            do {
                let reply = try await offMain { try Bridge.api("timetable_reset", ["map": m]) }
                busy = false
                let r = try JSONSerialization.jsonObject(with: reply) as? [String: Any] ?? [:]
                let folder = r["folder"] as? String ?? ""
                let restored = r["restored"] as? Int ?? 0
                if r["removed"] as? Bool == true {
                    store.say(tr("The timetable of {map_folder} is the map's own again (the edited copy was removed)", ["map_folder": folder]), error: false)
                } else if restored > 0 {
                    store.say(tr("{n} line(s) of {map_folder} put back as they were", ["n": "\(restored)", "map_folder": folder]), error: false)
                } else {
                    store.say(tr("The timetable of {map_folder} has not been changed here", ["map_folder": folder]), error: true)
                    return
                }
                store.act("lines")
                if map == m { read(m) }
            } catch {
                busy = false
                store.say(error.localizedDescription, error: true)
            }
        }
    }

    func addLine(_ store: LauncherStore) {
        let name = newLine.trimmingCharacters(in: .whitespaces)
        guard data != nil else { return }
        if !fileNameOk(name) {
            store.say(String(localized: "Type the new line's name (it becomes the file name, so no / \\ : * ? \" < > |)"), error: true)
            return
        }
        if data!.lines.contains(where: { ttSame($0.name, name) }) {
            store.say(tr("Line {name} is there already", ["name": name]), error: true)
            return
        }
        data!.lines.append(TTLine(name: name, user_allowed: true, priority: 0, tours: []))
        data!.lines.sort { $0.name.lowercased() < $1.name.lowercased() }
        line = name
        tour = 0
        newLine = ""
        drafts = [:]
        touch(name)
        store.say(tr("Line {name} added: give it tours, then save", ["name": name]), error: false)
    }

    func choices(_ l: TTLine) -> [String] {
        guard let d = data else { return [] }
        let named = d.trips.filter { ttSame($0.line, l.name) }.map(\.name)
        var names = named.isEmpty ? d.trips.map(\.name) : named
        for t in l.tours { for x in t.trips where !names.contains(where: { ttSame($0, x.trip) }) { names.append(x.trip) } }
        return names.sorted { $0.lowercased() < $1.lowercased() }
    }

    func editLine(_ f: (inout TTLine) -> Void) {
        guard let i = lineIndex, data!.lines.indices.contains(i) else { return }
        f(&data!.lines[i])
        touch(data!.lines[i].name)
    }

    func pickTour(_ i: Int) {
        tour = i
        drafts = [:]
    }
}

struct TimetablePage: View {
    @Environment(LauncherStore.self) private var store
    @Environment(\.compactLayout) private var compact
    @State private var width: CGFloat = 1000
    private var tt: TimetableEditor { TimetableEditor.shared }

    var body: some View {
        let maps = store.lists.maps
        PageScroll("Timetable", subtitle: "A map's lines: their tours and when each trip leaves. Saved as the line's .ttl (in the content folder; OMSI 2's own files stay as they are).") {
            if tt.data != nil {
                let n = tt.dirty.count
                Button { Task { await tt.saveAll(store) } } label: {
                    Label(n == 0 ? String(localized: "Saved") : n == 1 ? String(localized: "Save") : tr("Save all ({n} lines)", ["n": "\(n)"]), systemImage: "square.and.arrow.down")
                }
                .buttonStyle(n > 0 ? .primary : .secondary).disabled(n == 0 || tt.busy)
            }
        } content: {
            if maps.isEmpty {
                Card { EmptyState(icon: "map", text: "No maps found.") }
            } else {
                let li = tt.lineIndex
                let line = li.map { tt.data!.lines[$0] }
                let tourIndex = line.map { min(tt.tour, max(0, $0.tours.count - 1)) } ?? 0
                let hasTour = line.map { !$0.tours.isEmpty } ?? false
                Group {
                    if compact {
                        switch (tt.level, line) {
                        case (1, .some(let l)):
                            back(String(localized: "Lines")) { tt.level = 0 }
                            ToursCard(line: l, tour: tourIndex) { tt.level = 2 }
                        case (2, .some(let l)) where hasTour:
                            back(tr("Line {name}", ["name": l.name])) { tt.level = 1 }
                            TripsCard(line: l, tour: tourIndex)
                        default:
                            LinesCard(maps: maps) { tt.level = 1 }
                        }
                    } else if width >= 1050 {
                        HStack(alignment: .top, spacing: 16) {
                            LinesCard(maps: maps).frame(width: 270)
                            if let l = line { ToursCard(line: l, tour: tourIndex).frame(width: 260) }
                            if let l = line, hasTour { TripsCard(line: l, tour: tourIndex) } else { Spacer(minLength: 0) }
                        }
                    } else {
                        HStack(alignment: .top, spacing: 16) {
                            VStack(spacing: 16) {
                                LinesCard(maps: maps)
                                if let l = line { ToursCard(line: l, tour: tourIndex) }
                            }
                            .frame(width: 280)
                            if let l = line, hasTour { TripsCard(line: l, tour: tourIndex) } else { Spacer(minLength: 0) }
                        }
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { width = $0 }
            }
        }
        .task(id: maps.map(\.file)) {
            let want = tt.map.flatMap { m in !tt.dirty.isEmpty || maps.contains { $0.file == m } ? m : nil }
                ?? maps.first { $0.file == store.choice.map }?.file ?? maps.first?.file
            if let want, want != tt.map || (tt.data == nil && tt.error == nil) { tt.read(want) }
        }
    }

    private func back(_ title: String, _ action: @escaping () -> Void) -> some View {
        Button(action: action) {
            HStack(spacing: 6) {
                Image(systemName: "chevron.left").font(.system(size: 17, weight: .semibold))
                Text(title).font(.inter(15))
            }
            .foregroundStyle(Theme.text2)
            .frame(minHeight: 36)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

private struct LinesCard: View {
    @Environment(LauncherStore.self) private var store
    var maps: [MapInfo]
    var opened: (() -> Void)? = nil
    private var tt: TimetableEditor { TimetableEditor.shared }

    var body: some View {
        @Bindable var tt = tt
        Card("Lines") {
            Field("Map") {
                SelectField(options: maps.map { (value: $0.file, label: $0.title) }, selection: tt.map ?? "", onChange: { tt.switchMap($0, store) }).disabled(tt.busy)
            }
            if let e = tt.error { Notice(text: e, kind: .danger) }
            if let d = tt.data {
                if d.lines.isEmpty {
                    Text("This map has no timetable (no TTData lines).").font(.inter(12.5)).foregroundStyle(Theme.muted)
                }
                VStack(spacing: 0) {
                    ForEach(d.lines, id: \.name) { l in
                        ListItem(title: l.name, subtitle: tr("{n} tours", ["n": "\(l.tours.count)"]), selected: opened == nil && l.name == tt.line) {
                            tt.line = l.name
                            tt.pickTour(0)
                            opened?()
                        } trailing: {
                            if tt.dirty.contains(l.name) { Circle().fill(Theme.accent).frame(width: 8, height: 8) }
                            if opened != nil { Image(systemName: "chevron.right").font(.system(size: 13, weight: .semibold)).foregroundStyle(Theme.muted) }
                        }
                    }
                }
                HStack(spacing: 6) {
                    InputField(prompt: "New line (name)", text: $tt.newLine) { tt.addLine(store) }
                    Button { tt.addLine(store) } label: { Label("Add", systemImage: "plus") }.buttonStyle(.secondary)
                }
                Button { tt.reset(store) } label: {
                    Label(tt.armed ? "Press again to reset" : "Reset timetable", systemImage: "arrow.clockwise")
                }
                .buttonStyle(.launcher(tt.armed ? .danger : .plain, wide: true))
            } else if tt.error == nil && tt.map != nil {
                ProgressView().controlSize(.small).tint(Theme.accent)
            }
        }
    }
}

private struct ToursCard: View {
    @Environment(LauncherStore.self) private var store
    var line: TTLine
    var tour: Int
    var opened: (() -> Void)? = nil
    private var tt: TimetableEditor { TimetableEditor.shared }

    var body: some View {
        @Bindable var tt = tt
        let off = Double(tt.offset.trimmingCharacters(in: .whitespaces)).flatMap { $0.isFinite ? $0 : nil }
        let until = ttParse(tt.until)
        let base = line.tours.indices.contains(tour) ? line.tours[tour] : nil
        Card {
            Text(tr("Line {name}", ["name": line.name])).font(.inter(15, .semibold)).lineLimit(1)
            VStack(spacing: 0) {
                ForEach(Array(line.tours.enumerated()), id: \.offset) { i, x in
                    ListItem(title: tr("Tour {n}", ["n": "\(x.number)"]), subtitle: [x.trips.first.map { ttTime($0.departure) } ?? "", tr("{n} trips", ["n": "\(x.trips.count)"])].filter { !$0.isEmpty }.joined(separator: " - "),
                             selected: opened == nil && i == tour) {
                        tt.pickTour(i)
                        opened?()
                    } trailing: {
                        if opened != nil { Image(systemName: "chevron.right").font(.system(size: 13, weight: .semibold)).foregroundStyle(Theme.muted) }
                    }
                }
            }
            Button { newTour() } label: { Label("New tour", systemImage: "plus") }.buttonStyle(.launcher(.plain, wide: true))
            HStack(spacing: 6) {
                InputField(prompt: "minutes", text: $tt.offset, mono: true)
                Button { copy(base!, off!) } label: { Label("Copy +min", systemImage: "doc.on.doc") }.buttonStyle(.secondary)
                    .disabled(base == nil || off == nil)
            }
            HStack(spacing: 6) {
                InputField(prompt: "until h:mm", text: $tt.until, mono: true)
                    .overlay(RoundedRectangle(cornerRadius: Theme.radiusSm).stroke(until == nil ? Theme.danger : .clear))
                Button { repeatTour(base!, off!, until!) } label: { Label("Repeat", systemImage: "arrow.clockwise") }.buttonStyle(.secondary)
                    .disabled(base == nil || (off ?? 0) < 1 || until == nil)
            }
            Button { remove() } label: { Label("Delete tour", systemImage: "trash") }.buttonStyle(.launcher(.danger, wide: true)).disabled(base == nil)
        }
    }

    private func newTour() {
        let c = tt.choices(line)
        tt.editLine { l in
            let last = l.tours.last
            l.tours.append(TTTour(number: nextNumber(l.tours), ai_group: last?.ai_group ?? "Busses", extra: last?.extra ?? "", trips: [TTTrip(trip: c.first ?? "", profile: 0, departure: 360)]))
        }
        tt.pickTour(line.tours.count)
    }

    private func copy(_ base: TTTour, _ off: Double) {
        tt.editLine { l in
            var t = base
            t.number = nextNumber(l.tours)
            t.trips = base.trips.map { var x = $0; x.departure = max(0, x.departure + off); return x }
            l.tours.append(t)
        }
        tt.pickTour(line.tours.count)
    }

    private func repeatTour(_ base: TTTour, _ every: Double, _ until: Double) {
        let first = base.trips.first?.departure ?? 0
        var made = 0
        tt.editLine { l in
            var k = 1.0
            while first + every * k <= until + 1e-3 && made < 500 {
                var t = base
                t.number = nextNumber(l.tours)
                t.trips = base.trips.map { var x = $0; x.departure += every * k; return x }
                l.tours.append(t)
                made += 1
                k += 1
            }
        }
        if made > 0, let i = tt.lineIndex { tt.pickTour(tt.data!.lines[i].tours.count - 1) }
        store.say(tr("{made} tour(s) made, every {every} min up to {until}", ["made": "\(made)", "every": tt.offset.trimmingCharacters(in: .whitespaces), "until": ttTime(until)]), error: false)
    }

    private func remove() {
        let i = tour
        tt.editLine { if $0.tours.indices.contains(i) { $0.tours.remove(at: i) } }
        tt.pickTour(max(0, i - 1))
    }
}

private struct TripsCard: View {
    var line: TTLine
    var tour: Int
    private var tt: TimetableEditor { TimetableEditor.shared }

    var body: some View {
        let t = line.tours[tour]
        let choices = tt.choices(line)
        Card {
            HStack(spacing: 8) {
                Text(tr("Tour {n}", ["n": "\(t.number)"]) + " - \(t.ai_group)").font(.inter(15, .semibold)).lineLimit(1)
                Spacer(minLength: 8)
                Button { addTrip(choices) } label: { Label("Add trip", systemImage: "plus") }.buttonStyle(.launcher(.plain, small: true))
                Button("-1 min") { shift(-1) }.buttonStyle(.launcher(.plain, small: true)).disabled(t.trips.isEmpty)
                Button("+1 min") { shift(1) }.buttonStyle(.launcher(.plain, small: true)).disabled(t.trips.isEmpty)
            }
            VStack(spacing: 8) {
                ForEach(Array(t.trips.enumerated()), id: \.offset) { i, x in
                    TripRow(trip: x, key: "\(line.name)|\(tour)|\(i)", choices: choices, info: tt.data?.trips.first { ttSame($0.name, x.trip) }) { f in
                        tt.editLine { if $0.tours.indices.contains(tour), $0.tours[tour].trips.indices.contains(i) { f(&$0.tours[tour].trips[i]) } }
                    } remove: {
                        tt.editLine { if $0.tours.indices.contains(tour), $0.tours[tour].trips.indices.contains(i) { $0.tours[tour].trips.remove(at: i) } }
                        tt.drafts = [:]
                    }
                }
            }
        }
    }

    private func addTrip(_ choices: [String]) {
        tt.editLine { l in
            guard l.tours.indices.contains(tour) else { return }
            let trips = l.tours[tour].trips
            let n = trips.count
            let x: TTTrip
            if n == 0 { x = TTTrip(trip: choices.first ?? "", profile: 0, departure: 360) }
            else if n == 1 { x = TTTrip(trip: trips[0].trip, profile: trips[0].profile, departure: trips[0].departure + 30) }
            else { x = TTTrip(trip: trips[n - 2].trip, profile: trips[n - 2].profile, departure: trips[n - 1].departure + max(1, trips[n - 1].departure - trips[n - 2].departure)) }
            l.tours[tour].trips.append(x)
        }
    }

    private func shift(_ d: Double) {
        tt.editLine { l in
            guard l.tours.indices.contains(tour) else { return }
            for k in l.tours[tour].trips.indices { l.tours[tour].trips[k].departure = max(0, l.tours[tour].trips[k].departure + d) }
        }
        tt.drafts = [:]
    }
}

private struct TripRow: View {
    var trip: TTTrip
    var key: String
    var choices: [String]
    var info: TTTripInfo?
    var edit: ((inout TTTrip) -> Void) -> Void
    var remove: () -> Void
    private var tt: TimetableEditor { TimetableEditor.shared }

    var body: some View {
        let draft = tt.drafts[key]
        let time = Binding<String>(get: { draft ?? ttTime(trip.departure) }, set: { v in
            if let m = ttParse(v) { edit { $0.departure = m } } else { edit { _ in } }
            tt.drafts[key] = v
        })
        let profiles: [(value: String, label: String)] = info.map { i in i.profiles.enumerated().map { (value: "\($0.offset)", label: $0.element) } }.flatMap { $0.isEmpty ? nil : $0 } ?? [(value: "\(trip.profile)", label: "\(trip.profile)")]
        let profile = profiles.contains { $0.value == "\(trip.profile)" } ? "\(trip.profile)" : profiles[0].value
        let field = InputField(prompt: "h:mm", text: time, mono: true)
            .overlay(RoundedRectangle(cornerRadius: Theme.radiusSm).stroke(draft != nil && ttParse(draft!) == nil ? Theme.danger : .clear))
        let tripSelect = SelectField(options: choices.map { (value: $0, label: $0) }, selection: choices.first { ttSame($0, trip.trip) } ?? trip.trip, onChange: { v in edit { $0.trip = v; $0.profile = 0 } })
        let profileSelect = SelectField(options: profiles, selection: profile, onChange: { v in edit { $0.profile = Int(v) ?? 0 } })
        let to = Text(info?.terminus ?? "").font(.inter(13)).foregroundStyle(Theme.text2).lineLimit(1).help(info?.terminus ?? "")
        let x = Button(action: remove) { Image(systemName: "xmark") }.buttonStyle(.launcher(.ghost, small: true)).help("Remove this trip")
        ViewThatFits(in: .horizontal) {
            HStack(spacing: 8) {
                field.frame(width: 90)
                tripSelect.frame(minWidth: 170)
                profileSelect.frame(minWidth: 120)
                to.frame(minWidth: 90, maxWidth: .infinity, alignment: .leading)
                x
            }
            VStack(alignment: .leading, spacing: 6) {
                HStack(spacing: 8) {
                    field.frame(width: 90)
                    tripSelect
                    x
                }
                HStack(spacing: 8) {
                    profileSelect
                    to.frame(maxWidth: .infinity, alignment: .leading)
                }
            }
            .padding(.bottom, 6)
        }
    }
}
