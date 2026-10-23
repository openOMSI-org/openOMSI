import SwiftUI

struct DepotReply: Decodable {
    var buses: [DepotBus]
    var groups: [String]
}

struct DepotBus: Decodable, Identifiable {
    struct Fault: Decodable, Hashable { var id: String; var label: String }
    struct Run: Decodable, Hashable {
        var time: UInt64
        var map: String
        var line: String
        var tour: String
        var metres: Double
        var seconds: Double
        var stops: Int
        var tickets: Int
        var crashes: Int
    }

    var id: String
    var name: String
    var bus: String
    var paint: String
    var plate: String
    var number: String
    var last_used: UInt64
    var sessions: Int
    var metres: Double
    var seconds: Double
    var stops: Int
    var tickets: Int
    var takings: Double
    var crashes: Int
    var fuel_used: Double
    var history: [Run]
    var group: String
    var odometer: Double?
    var fuel: Double?
    var dirt: Double?
    var faults: [Fault]
}

func sameBusFile(_ a: String, _ b: String) -> Bool {
    func norm(_ f: String) -> String {
        let n = f.replacingOccurrences(of: "\\", with: "/").lowercased()
        guard let r = n.range(of: "vehicles/", options: .backwards), r.lowerBound > n.startIndex else { return n }
        return String(n[r.lowerBound...])
    }
    return norm(a) == norm(b)
}

func depotMapName(_ m: String) -> String {
    m.replacingOccurrences(of: "\\", with: "/").replacingOccurrences(of: "/global.cfg", with: "").replacingOccurrences(of: "maps/", with: "")
}

struct DepotPage: View {
    @Environment(LauncherStore.self) private var store
    @Environment(\.compactLayout) private var compact
    @State private var buses: [DepotBus] = []
    @State private var groups: [String] = []
    @State private var error: String?
    @State private var loaded = false
    @AppStorage("depot-group") private var group = ""
    @State private var selected: String?
    @State private var opened = false
    @State private var adding = false
    @State private var naming: Naming?
    @State private var name = ""
    @State private var confirmGroup = false

    private static let none = String(UnicodeScalar(1))

    enum Naming: Identifiable {
        case new, rename
        var id: Int { self == .new ? 0 : 1 }
    }

    var body: some View {
        let g = group == Self.none || groups.contains(group) ? group : ""
        let list = g.isEmpty ? buses : buses.filter { g == Self.none ? $0.group.isEmpty : $0.group == g }
        let b = list.first { $0.id == selected } ?? list.first
        Group {
        if compact, opened, let b {
            VStack(spacing: 0) {
                HStack(spacing: 6) {
                    Button { opened = false } label: {
                        Image(systemName: "chevron.left").font(.system(size: 20, weight: .semibold)).foregroundStyle(Theme.text)
                            .frame(width: 44, height: 44).contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    Text("Depot").font(.inter(15)).foregroundStyle(Theme.muted)
                    Spacer()
                }
                .padding(.horizontal, 8)
                Scroll {
                    DepotDetail(b: b, groups: groups, reload: load) { selected = nil; opened = false }
                        .id(b.id)
                        .padding(.horizontal, 16).padding(.bottom, 24)
                }
            }
        } else {
            PageScroll("Depot", subtitle: "Your own buses: each keeps its number plate, its kilometres and the state you left it in.", maxWidth: 1200) {
                Button { adding = true } label: { Label("Add a bus", systemImage: "plus") }
                    .buttonStyle(.primary).disabled(store.lists.vehicles.isEmpty)
            } content: {
                if let error { Notice(text: error, kind: .danger) }
                if buses.isEmpty && groups.isEmpty {
                    Card { EmptyState(icon: "door.garage.closed", text: loaded ? "Your buses show up here once added." : nil) }
                } else if compact {
                    VStack(alignment: .leading, spacing: 12) {
                        groupBar(g)
                        listCard(list, b) { selected = $0; opened = true }
                    }
                } else {
                    HStack(alignment: .top, spacing: 20) {
                        VStack(alignment: .leading, spacing: 12) {
                            groupBar(g)
                            listCard(list, b) { selected = $0 }
                        }
                        .frame(width: 320)
                        if let b {
                            DepotDetail(b: b, groups: groups, reload: load) { selected = nil }.id(b.id)
                        }
                    }
                }
            }
        }
        }
        .task { load() }
        .sheet(isPresented: $adding) {
            DepotBusPicker { bus, paint in add(bus, paint, g) }
                .environment(store)
                .font(.inter(14)).foregroundStyle(Theme.text).tint(Theme.accent)
                .preferredColorScheme(.dark)
        }
        .alert(naming == .rename ? "Rename the depot" : "New depot", isPresented: Binding(get: { naming != nil }, set: { if !$0 { naming = nil } })) {
            TextField("Grand Paris-Moulon", text: $name)
            Button("Cancel", role: .cancel) { naming = nil }
            Button("Save") { saveName(g) }
                .disabled(name.trimmingCharacters(in: .whitespaces).isEmpty || (naming == .rename && name.trimmingCharacters(in: .whitespaces) == g))
        }
        .confirmationDialog("Delete the depot", isPresented: $confirmGroup) {
            Button("Delete", role: .destructive) {
                groupCall("depot_group_remove", ["name": g]) { group = "" }
            }
        } message: {
            Text(tr("The buses of {name} stay in the list, in no depot.", ["name": g]))
        }
    }

    private func groupBar(_ g: String) -> some View {
        let loose = !groups.isEmpty && buses.contains { $0.group.isEmpty }
        let options = [(value: "", label: "\(String(localized: "All buses")) (\(buses.count))")]
            + groups.map { n in (value: n, label: "\(n) (\(buses.filter { $0.group == n }.count))") }
            + (loose ? [(value: Self.none, label: String(localized: "No depot"))] : [])
        return HStack(spacing: 6) {
            SelectField(options: options, selection: g, onChange: { group = $0 })
            Button { name = ""; naming = .new } label: { Image(systemName: "plus") }.buttonStyle(.secondary).help("New depot")
            if groups.contains(g) {
                Button { name = g; naming = .rename } label: { Image(systemName: "pencil") }.buttonStyle(.secondary).help("Rename the depot")
                Button { confirmGroup = true } label: { Image(systemName: "trash") }.buttonStyle(.destructive).help("Delete the depot")
            }
        }
    }

    private func listCard(_ list: [DepotBus], _ b: DepotBus?, open: @escaping (String) -> Void) -> some View {
        Card(padded: false) {
            if list.isEmpty {
                EmptyState(icon: "door.garage.closed", text: "Your buses show up here once added.")
            }
            LazyVStack(spacing: 0) {
                ForEach(list) { x in
                    let sub = [x.plate, x.number.isEmpty ? "" : "#\(x.number)", x.odometer.map { "\(Int($0.rounded()).formatted()) km" } ?? ""].filter { !$0.isEmpty }.joined(separator: " - ")
                    ListItem(title: x.name, subtitle: sub, icon: "bus.fill", selected: !compact && x.id == b?.id) { open(x.id) } trailing: {
                        if !x.faults.isEmpty {
                            Image(systemName: "exclamationmark.triangle.fill").font(.system(size: 13)).foregroundStyle(Theme.warn)
                                .help(x.faults.map { NSLocalizedString($0.label, comment: "") }.joined(separator: ", "))
                        }
                        if compact { Image(systemName: "chevron.right").font(.system(size: 13, weight: .semibold)).foregroundStyle(Theme.muted) }
                    }
                }
            }
            .padding(6)
        }
    }

    private func load() {
        do {
            let d = try Bridge.api(DepotReply.self, "depot_get")
            buses = d.buses
            groups = d.groups
            error = nil
        } catch {
            buses = []
            self.error = error.localizedDescription
        }
        loaded = true
    }

    private func groupCall(_ cmd: String, _ args: [String: Any], then: () -> Void) {
        do {
            groups = try Bridge.api([String].self, cmd, args)
            then()
            naming = nil
            load()
        } catch {
            store.say(error.localizedDescription, error: true)
        }
    }

    private func saveName(_ g: String) {
        let n = name.trimmingCharacters(in: .whitespaces)
        guard !n.isEmpty else { return }
        if naming == .rename {
            groupCall("depot_group_rename", ["name": g, "to": n]) { group = n }
        } else {
            groupCall("depot_group_add", ["name": n]) { group = n }
        }
    }

    private func add(_ bus: String, _ paint: String, _ g: String) {
        let v = store.lists.vehicles.first { $0.file == bus }
        let number = v?.numbers?.randomElement()?.first ?? ""
        var args: [String: Any] = ["bus": bus, "name": v?.name ?? bus.stem]
        if !paint.isEmpty { args["paint"] = paint }
        if !number.isEmpty { args["number"] = number }
        if groups.contains(g) { args["group"] = g }
        do {
            let id = try Bridge.api(String.self, "depot_add", args)
            load()
            selected = id
            store.say(String(localized: "Bus added to your depot."), error: false)
        } catch {
            store.say(error.localizedDescription, error: true)
        }
    }
}

struct DepotDetail: View {
    @Environment(LauncherStore.self) private var store
    @Environment(\.compactLayout) private var compact
    var b: DepotBus
    var groups: [String]
    var reload: () -> Void
    var removed: () -> Void
    @State private var name = ""
    @State private var plate = ""
    @State private var confirmRemove = false

    var body: some View {
        let v = store.lists.vehicles.first { sameBusFile($0.file, b.bus) }
        let busName = v?.name ?? b.bus.stem
        let columns = compact ? [GridItem(.flexible())] : [GridItem(.flexible(), spacing: 16), GridItem(.flexible(), spacing: 16)]
        VStack(alignment: .leading, spacing: 16) {
            Card {
                HStack(alignment: .top, spacing: 10) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(b.name).font(.inter(20, .semibold))
                        Text(b.last_used > 0 ? "\(busName) - \(Date(timeIntervalSince1970: TimeInterval(b.last_used)).formatted(.relative(presentation: .named)))" : busName)
                            .font(.inter(13)).foregroundStyle(Theme.muted)
                    }
                    Spacer(minLength: 8)
                    Button { confirmRemove = true } label: { Image(systemName: "trash") }.buttonStyle(.destructive).help("Remove from the depot")
                    Button { drive(v) } label: { Label("Drive this bus", systemImage: "play.fill") }.buttonStyle(.primary).disabled(v == nil)
                }
                if v == nil { Notice(text: tr("{name}: not installed", ["name": busName]), kind: .warn) }
                LazyVGrid(columns: columns, alignment: .leading, spacing: 14) {
                    Field("Name") {
                        InputField(prompt: "Name (e.g. my Citaro)", text: $name) { saveName() }
                    }
                    Field("Number plate") {
                        HStack(spacing: 6) {
                            InputField(prompt: "AB-123-CD", text: $plate) { savePlate() }
                            Button { reroll() } label: { Image(systemName: "arrow.clockwise") }.buttonStyle(.secondary).help("Another plate")
                        }
                    }
                    Field("Livery") {
                        let standard = v?.default_paint?.isEmpty == false ? "\(String(localized: "Standard")) (\(v!.default_paint!))" : String(localized: "Standard")
                        SelectField(options: [(value: "", label: standard)] + (v?.paints ?? []).map { (value: $0, label: $0) }, selection: b.paint, onChange: { update(["paint": $0]) })
                            .disabled(v == nil)
                    }
                    if !groups.isEmpty {
                        Field("Depot") {
                            SelectField(options: [(value: "", label: String(localized: "No depot"))] + groups.map { (value: $0, label: $0) }, selection: b.group, onChange: { update(["group": $0]) })
                        }
                    }
                    Field("Fleet number") {
                        if let numbers = v?.numbers, !numbers.isEmpty {
                            SelectField(options: numbers.compactMap { $0.first.map { (value: $0, label: $0) } }, selection: b.number, onChange: { update(["number": $0]) })
                        } else {
                            Text("None").font(.inter(14)).foregroundStyle(Theme.muted)
                                .padding(.horizontal, 11).frame(maxWidth: .infinity, minHeight: 36, alignment: .leading).fieldBackground()
                        }
                    }
                }
                HStack(spacing: 16) {
                    if let f = b.fuel { Label("\(Int(f.rounded())) L", systemImage: "fuelpump.fill") }
                    Label(tr("Dirt {n} %", ["n": "\(Int(((b.dirt ?? 0) * 100).rounded()))"]), systemImage: "sun.max.fill")
                }
                .font(.inter(13.5)).foregroundStyle(Theme.text2)
                VStack(alignment: .leading, spacing: 8) {
                    if b.faults.isEmpty {
                        Label("No fault", systemImage: "checkmark").font(.inter(13)).foregroundStyle(Theme.muted)
                    }
                    ForEach(b.faults, id: \.self) { f in
                        HStack(spacing: 10) {
                            Image(systemName: "exclamationmark.triangle.fill").font(.system(size: 13)).foregroundStyle(Theme.warn)
                            Text(LocalizedStringKey(f.label)).font(.inter(14)).frame(maxWidth: .infinity, alignment: .leading)
                            Button { service("repair:\(f.id)") } label: { Label("Repair", systemImage: "wrench.and.screwdriver.fill") }.buttonStyle(.launcher(.plain, small: true))
                        }
                    }
                    if (store.setting("maintenance")?.string ?? "0") == "0" {
                        Text("Parts never wear out with Maintenance set to Infinite: choose Normal under Settings so that bulbs blow and the doors wear as in OMSI.")
                            .font(.inter(12)).foregroundStyle(Theme.muted).fixedSize(horizontal: false, vertical: true)
                    }
                }
                HStack(spacing: 8) {
                    Button { service("refuel") } label: { Label("Refuel", systemImage: "fuelpump.fill") }.buttonStyle(.launcher(.plain, small: true)).disabled(b.fuel == nil)
                    Button { service("wash") } label: { Label("Wash", systemImage: "drop.fill") }.buttonStyle(.launcher(.plain, small: true)).disabled((b.dirt ?? 0) == 0)
                    Button { service("repair") } label: { Label("Repair all", systemImage: "wrench.and.screwdriver.fill") }.buttonStyle(.launcher(.plain, small: true)).disabled(b.faults.isEmpty)
                }
            }
            LazyVGrid(columns: [GridItem(.adaptive(minimum: compact ? 140 : 150), spacing: 12)], spacing: 12) {
                StatTile(icon: "gauge.with.dots.needle.67percent", value: b.odometer.map { "\(Int($0.rounded()).formatted()) km" } ?? String(localized: "never"), label: "Odometer")
                StatTile(icon: "point.topleft.down.to.point.bottomright.curvepath.fill", value: String(format: "%.1f km", b.metres / 1000), label: "distance")
                StatTile(icon: "clock.fill", value: tr("{h} h {m} min", ["h": "\(Int(b.seconds) / 3600)", "m": String(format: "%02d", (Int(b.seconds) % 3600) / 60)]), label: "hours driven")
                StatTile(icon: "gamecontroller.fill", value: "\(b.sessions)", label: "runs")
                StatTile(icon: "mappin.circle.fill", value: "\(b.stops)", label: "stops served")
                StatTile(icon: "ticket.fill", value: "\(b.tickets)", label: "tickets sold")
                StatTile(icon: "banknote.fill", value: String(format: "%.2f", b.takings), label: "takings")
                StatTile(icon: "fuelpump.fill", value: String(format: "%.1f L", b.fuel_used), label: "fuel used")
                StatTile(icon: "exclamationmark.triangle.fill", value: "\(b.crashes)", label: "crashes")
            }
            Card("Recent runs") {
                if b.history.isEmpty {
                    EmptyState(icon: "clock.arrow.circlepath", text: "No runs yet. Drive a duty and it shows up here.")
                }
                ForEach(Array(b.history.enumerated()), id: \.offset) { _, x in
                    HStack(spacing: 10) {
                        if x.line.isEmpty { Text("Free drive").font(.inter(14, .medium)) } else {
                            LineBadge(line: x.line)
                            if !x.tour.isEmpty { Text("/ \(x.tour)").font(.inter(14)) }
                        }
                        VStack(alignment: .leading, spacing: 2) {
                            Text(depotMapName(x.map)).font(.inter(13.5)).lineLimit(1)
                            Text("\(Date(timeIntervalSince1970: TimeInterval(x.time)).formatted(.relative(presentation: .named))) - \(String(format: "%d:%02d", Int(x.seconds) / 3600, (Int(x.seconds) % 3600) / 60)) - \(String(format: "%.1f km", x.metres / 1000)) - \(x.stops) / \(x.tickets) / \(x.crashes)")
                                .font(.inter(12.5).monospacedDigit()).foregroundStyle(Theme.muted).lineLimit(1)
                        }
                        Spacer(minLength: 0)
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .onAppear { name = b.name; plate = b.plate }
        .onDisappear { saveName(); savePlate() }
        .confirmationDialog("Remove from the depot", isPresented: $confirmRemove) {
            Button("Remove", role: .destructive) { remove() }
        } message: {
            Text(tr("{name} leaves the depot with its kilometres and its wear.", ["name": b.name]))
        }
    }

    private func update(_ patch: [String: String]) {
        do {
            _ = try Bridge.api("depot_update", ["id": b.id, "patch": patch])
        } catch {
            store.say(error.localizedDescription, error: true)
        }
        reload()
    }

    private func saveName() {
        let n = name.trimmingCharacters(in: .whitespaces)
        if !n.isEmpty && n != b.name { update(["name": n]) }
    }

    private func savePlate() {
        let p = plate.trimmingCharacters(in: .whitespaces).uppercased()
        if !p.isEmpty && p != b.plate { update(["plate": p]) }
    }

    private func reroll() {
        guard let p = try? Bridge.api(String.self, "depot_plate"), !p.isEmpty else { return }
        plate = p
        update(["plate": p])
    }

    private func service(_ kind: String) {
        do {
            _ = try Bridge.api("depot_service", ["id": b.id, "kind": kind])
        } catch {
            store.say(error.localizedDescription, error: true)
        }
        reload()
    }

    private func remove() {
        do {
            _ = try Bridge.api("depot_remove", ["id": b.id])
        } catch {
            store.say(error.localizedDescription, error: true)
        }
        removed()
        reload()
    }

    private func drive(_ v: VehicleInfo?) {
        guard let v else { return }
        if let s = store.serverBuses, !s.contains(busFile(v.file)) {
            store.say(tr("The server does not offer {name}", ["name": b.name]), error: true)
            return
        }
        store.act("select_bus", ["file": v.file])
        store.act("paint", ["paint": b.paint])
        store.act("hof", ["hof": ""])
        store.act("number", ["number": b.number])
        store.act("plate", ["plate": b.plate])
        store.page = .drive
        store.phoneMore = nil
        store.phoneTab = "drive"
        store.say(tr("{name} is ready under Drive: choose the time and the duty.", ["name": b.name]), error: false)
    }
}

struct DepotBusPicker: View {
    @Environment(LauncherStore.self) private var store
    @Environment(\.dismiss) private var dismiss
    var take: (String, String) -> Void
    @State private var search = ""
    @State private var bus = ""
    @State private var paint = ""

    var body: some View {
        let list = store.lists.vehicles
            .filter { search.isEmpty || "\($0.name) \($0.manufacturer) \($0.type_name) \($0.folder)".localizedCaseInsensitiveContains(search) }
            .sorted { ($0.manufacturer, $0.name) < ($1.manufacturer, $1.name) }
        let v = store.lists.vehicles.first { $0.file == bus }
        PickerFrame(title: "Add a bus") {
            VStack(spacing: 10) {
                SearchField(prompt: "Search buses", text: $search)
                if list.isEmpty { EmptyState(icon: "bus", title: "No bus found") }
                Scroll {
                    LazyVStack(spacing: 0) {
                        ForEach(list) { x in
                            ListItem(title: x.name, subtitle: x.manufacturer, selected: x.file == bus) { bus = x.file; paint = "" }
                        }
                    }
                }
                if let v {
                    let standard = v.default_paint?.isEmpty == false ? "\(String(localized: "Standard")) (\(v.default_paint!))" : String(localized: "Standard")
                    Field("Livery") {
                        SelectField(options: [(value: "", label: standard)] + v.paints.map { (value: $0, label: $0) }, selection: paint, onChange: { paint = $0 })
                    }
                }
            }
            .padding(.horizontal, 20)
        } foot: {
            Spacer()
            Button("Cancel") { dismiss() }.buttonStyle(.secondary)
            Button {
                take(bus, paint)
                dismiss()
            } label: { Label("Add to the depot", systemImage: "checkmark") }.buttonStyle(.primary).disabled(v == nil)
        }
    }
}
