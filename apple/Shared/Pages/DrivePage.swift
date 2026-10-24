import ImageIO
import SwiftUI

/// The Drive page, as the desktop launcher lays it out (`launcher/ui/js/pages/drive.js`): the
/// duty put together in cards on the left - the map, the bus, the duty (a timetable's line and
/// tour, or a free drive), the time and the weather, the road - and on the right the summary
/// with the destination sign and Start, and the situations to continue. (A phone has a Drive
/// page of its own: PhoneLauncher.swift.)
struct DrivePage: View {
    @Environment(LauncherStore.self) private var store

    var body: some View {
        GeometryReader { geo in
            let width = geo.size.width - 64
            let twoColumns = width >= 760
            let side: CGFloat = width >= 1080 ? 340 : 300
            VStack(alignment: .leading, spacing: 16) {
                PageHeader(title: "Drive", subtitle: "Choose where and what you drive, then start.")
                    .padding(.horizontal, 32)
                    .padding(.top, 28)
                if twoColumns {
                    HStack(alignment: .top, spacing: 20) {
                        Scroll {
                            cards.padding(.bottom, 40)
                        }
                        .scrollIndicators(.automatic)
                        Scroll {
                            VStack(spacing: 16) {
                                Summary(showStart: true)
                                ContinueCard()
                            }
                            .padding(.bottom, 40)
                        }
                        .scrollIndicators(.never)
                        .frame(width: side)
                    }
                    .padding(.horizontal, 32)
                } else {
                    Scroll {
                        VStack(spacing: 16) {
                            cards
                            Summary(showStart: true)
                            ContinueCard()
                        }
                        .padding(.bottom, 40)
                    }
                    .padding(.horizontal, 32)
                }
            }
        }
    }

    private var cards: some View {
        VStack(spacing: 16) {
            MultiplayerDutyNotice()
            MapCard()
            BusCard()
            DutyCard()
            ViewThatFits(in: .horizontal) {
                HStack(alignment: .top, spacing: 16) {
                    TimeCard().frame(minWidth: 300)
                    ConditionsCard().frame(minWidth: 300)
                }
                VStack(spacing: 16) {
                    TimeCard()
                    ConditionsCard()
                }
            }
        }
    }
}

// --- the map -----------------------------------------------------------------------------------

struct MapCard: View {
    @Environment(LauncherStore.self) private var store
    @Environment(\.compactLayout) private var compact

    var body: some View {
        Card("Map", actions: AnyView(
            Button { store.picker = .map } label: { Label("Change map", systemImage: "map.fill") }
                .buttonStyle(.launcher(.plain, small: true))
                .disabled(store.lists.maps.isEmpty)
        )) {
            if store.lists.maps.isEmpty && store.snapshot.loading_content {
                LoadingRow(text: "Reading the maps…")
            } else if let m = store.map {
                HStack(alignment: .center, spacing: 16) {
                    MapPictureView(url: store.mapPicture(m.file))
                        .frame(width: compact ? 120 : 168, height: compact ? 68 : 94)
                    VStack(alignment: .leading, spacing: 3) {
                        Text(m.title).font(.inter(17, .semibold)).lineLimit(1)
                        if !m.description.isEmpty {
                            Text(m.description).font(.inter(13)).foregroundStyle(Theme.muted).lineLimit(2)
                        }
                        HStack(spacing: 8) {
                            if m.installed == true { Tag(text: "Mod") }
                            Text(tr("{n} entry points", ["n": "\(m.entry_points.count)"])).font(.inter(12.5)).foregroundStyle(Theme.muted)
                        }
                        .padding(.top, 5)
                    }
                    Spacer(minLength: 0)
                }
                if store.choice.lan_mode != "join" { NotesBox(kind: "map", file: m.file) }
            } else {
                Notice(text: store.lists.maps.isEmpty ? String(localized: "No map is installed.") : String(localized: "Choose a map"), kind: .warn)
            }
        }
    }
}

/// A map's picture (or a map symbol while there is none), filling its frame.
struct MapPictureView: View {
    var url: URL?
    var icon = "map"
    var radius: CGFloat = Theme.radiusSm
    @State private var image: CGImage?

    var body: some View {
        // (the picture fills the frame it is given, never more)
        Theme.surface3
            .overlay {
                if let image = image ?? (Theme.renderFlat ? PictureCache.loadNow(url) : nil) {
                    Image(decorative: image, scale: 1).resizable().aspectRatio(contentMode: .fill)
                } else {
                    Image(systemName: icon).font(.system(size: 26)).foregroundStyle(Theme.muted.opacity(0.7))
                }
            }
            .clipped()
            .clipShape(RoundedRectangle(cornerRadius: radius, style: .continuous))
        .task(id: url) { image = await PictureCache.load(url) }
    }
}

/// The maps' pictures, read once and kept small.
enum PictureCache {
    nonisolated(unsafe) private static let cache = NSCache<NSURL, CGImage>()

    static func load(_ url: URL?) async -> CGImage? {
        guard let url else { return nil }
        if let hit = cache.object(forKey: url as NSURL) { return hit }
        let img = await Task.detached(priority: .utility) { read(url) }.value
        if let img { cache.setObject(img, forKey: url as NSURL) }
        return img
    }

    /// At once, on this thread (the screenshot tool, whose renderer runs no tasks).
    static func loadNow(_ url: URL?) -> CGImage? {
        guard let url else { return nil }
        if let hit = cache.object(forKey: url as NSURL) { return hit }
        let img = read(url)
        if let img { cache.setObject(img, forKey: url as NSURL) }
        return img
    }

    nonisolated private static func read(_ url: URL) -> CGImage? {
        guard let src = CGImageSourceCreateWithURL(url as CFURL, nil) else { return nil }
        let opts: [CFString: Any] = [kCGImageSourceCreateThumbnailFromImageAlways: true, kCGImageSourceThumbnailMaxPixelSize: 900, kCGImageSourceCreateThumbnailWithTransform: true]
        return CGImageSourceCreateThumbnailAtIndex(src, 0, opts as CFDictionary)
    }
}

struct LoadingRow: View {
    var text: LocalizedStringKey
    var body: some View {
        HStack(spacing: 10) {
            ProgressView().controlSize(.small).tint(Theme.accent)
            Text(text).font(.inter(14)).foregroundStyle(Theme.muted)
        }
    }
}

// --- the bus -----------------------------------------------------------------------------------

struct BusCard: View {
    @Environment(LauncherStore.self) private var store
    @Environment(\.compactLayout) private var compact

    var body: some View {
        Card("Bus", actions: AnyView(
            Button { store.picker = .bus } label: { Label("Change bus", systemImage: "bus.fill") }
                .buttonStyle(.launcher(.plain, small: true))
                .disabled(store.offeredVehicles.isEmpty)
        )) {
            if store.lists.vehicles.isEmpty && store.snapshot.loading_content {
                LoadingRow(text: "Reading the buses…")
            } else if store.noServerBus {
                Notice(text: String(localized: "The server offers none of the vehicles installed here"), kind: .danger)
            } else if let b = store.bus {
                if store.picker != .bus {
                    BusPreview()
                        .frame(height: compact ? 200 : 250)
                        .clipShape(RoundedRectangle(cornerRadius: Theme.radiusSm, style: .continuous))
                }
                HStack(alignment: .center) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(b.name).font(.inter(17, .semibold)).lineLimit(1)
                        Text([b.manufacturer, b.type_name].filter { !$0.isEmpty }.joined(separator: " - ")).font(.inter(12.5)).foregroundStyle(Theme.muted).lineLimit(1)
                    }
                    Spacer()
                    if b.installed == true { Tag(text: "Mod") }
                }
                if let m = b.missing_packs, !m.isEmpty {
                    Notice(text: tr("Parts of this bus come from packs that are not installed: {packs}", ["packs": m.joined(separator: ", ")]), kind: .warn)
                }
                VehicleFields(bus: b)
            } else {
                Notice(text: store.lists.vehicles.isEmpty ? String(localized: "No bus is installed.") : String(localized: "Choose a bus"), kind: .warn)
            }
        }
    }
}

/// The paint, the depot file, the fleet number and the plate (the vehicle dialog of OMSI).
struct VehicleFields: View {
    @Environment(LauncherStore.self) private var store
    @Environment(\.compactLayout) private var compact
    var bus: VehicleInfo
    @State private var plate = ""
    @State private var number = ""

    private func custom(_ n: String, _ numbers: [[String]]) -> String {
        numbers.contains { $0.first == n } ? "" : n
    }

    var body: some View {
        let c = store.choice
        let columns = compact ? [GridItem(.flexible())] : [GridItem(.flexible(), spacing: 16), GridItem(.flexible(), spacing: 16)]
        LazyVGrid(columns: columns, alignment: .leading, spacing: 14) {
            Field("Paint") { PaintSelect(bus: bus) }
            Field("Depot file") {
                SelectField(options: [(value: "", label: "\(String(localized: "Automatic"))\(store.snapshot.default_hof.isEmpty ? "" : " (\(store.snapshot.default_hof))")")] + bus.hofs.map { (value: $0, label: $0) },
                            selection: c.hof_manual ? c.hof : "", onChange: { store.act("hof", ["hof": $0]) })
            }
            if let numbers = bus.numbers, !numbers.isEmpty {
                Field("Fleet number") {
                    VStack(spacing: 6) {
                        SelectField(options: [(value: "", label: "\(String(localized: "Automatic")) (\(numbers.first?.first ?? ""))")] + numbers.compactMap { n in n.first.map { (value: $0, label: n.count > 1 && !n[1].isEmpty ? "\($0) - \(n[1])" : $0) } },
                                    selection: c.number, onChange: { store.act("number", ["number": $0]) })
                        InputField(prompt: "Custom", text: $number) { store.act("number", ["number": number.trimmingCharacters(in: .whitespaces)]) }
                            .onAppear { number = custom(c.number, numbers) }
                            .onChange(of: c.number) { _, v in number = custom(v, numbers) }
                            .onChange(of: number) { _, v in
                                let n = v.trimmingCharacters(in: .whitespaces)
                                if n != custom(store.choice.number, numbers) { store.act("number", ["number": n]) }
                            }
                    }
                }
            }
            Field("Number plate") {
                InputField(prompt: "Automatic", text: $plate) { store.act("plate", ["plate": plate]) }
                    .onAppear { plate = c.plate }
                    .onChange(of: bus.file) { _, _ in plate = store.choice.plate }
            }
        }
    }
}

/// The paint of the bus: a select, with the previous and the next one a tap away.
struct PaintSelect: View {
    @Environment(LauncherStore.self) private var store
    var bus: VehicleInfo

    var body: some View {
        let standard = bus.default_paint?.isEmpty == false ? "\(String(localized: "Standard")) (\(bus.default_paint!))" : String(localized: "Standard")
        let options = [(value: "", label: standard)] + bus.paints.map { (value: $0, label: $0) }
        let i = options.firstIndex { $0.value == store.choice.paint } ?? 0
        HStack(spacing: 6) {
            SelectField(options: options, selection: store.choice.paint, onChange: { store.act("paint", ["paint": $0]) })
            if options.count > 1 {
                Button { store.act("paint", ["paint": options[(i - 1 + options.count) % options.count].value]) } label: { Image(systemName: "chevron.left") }
                    .buttonStyle(.secondary).help("Previous paint")
                Button { store.act("paint", ["paint": options[(i + 1) % options.count].value]) } label: { Image(systemName: "chevron.right") }
                    .buttonStyle(.secondary).help("Next paint")
                Text("\(i + 1) / \(options.count)").font(.inter(12.5).monospacedDigit()).foregroundStyle(Theme.muted).fixedSize()
            }
        }
    }
}

// --- the duty ----------------------------------------------------------------------------------

/// A trip without passengers (to or from the depot): its line is empty or it says so.
func isDepotRun(_ x: TripInfo) -> Bool {
    if x.line.trimmingCharacters(in: .whitespaces).isEmpty { return true }
    let t = x.terminus.lowercased()
    return ["betriebsfahrt", "leerfahrt", "dienstfahrt", "not in service", "out of service", "hors service", "zjazd do zajezdni"].contains { t.contains($0) }
}

struct DutyCard: View {
    @Environment(LauncherStore.self) private var store
    @Environment(\.compactLayout) private var compact

    var body: some View {
        let c = store.choice
        Card("Duty", actions: AnyView(
            Segmented(items: [("line", "Timetable"), ("free", "Free drive")], selection: c.free ? "free" : "line") { v in
                store.act("free", ["on": v == "free"])
            }
        )) {
            if c.free {
                Text("Free driving: the bus starts where you chose, with the traffic and the timetable's buses around it, but no line of your own.")
                    .font(.inter(14)).foregroundStyle(Theme.muted).fixedSize(horizontal: false, vertical: true)
                startAt
                RouteMapView()
            } else if store.snapshot.loading_lines && store.lines.isEmpty {
                LoadingRow(text: "Reading the timetable…")
            } else if lines.isEmpty {
                Notice(text: String(localized: "This map has no lines to drive on this date. Choose a free drive, or another date."))
                startAt
            } else {
                timetable
            }
        }
    }

    private var lines: [LineInfo] { store.lines.filter { $0.user_allowed && !$0.tours.isEmpty } }
    private var now: Double { Double(store.choice.time * 60) }

    @ViewBuilder
    private var timetable: some View {
        if compact {
            LineList(lines: lines)
            if let l = store.line { TourList(line: l) }
        } else {
            HStack(alignment: .top, spacing: 16) {
                LineList(lines: lines).frame(maxWidth: .infinity)
                Group { if let l = store.line { TourList(line: l) } else { Color.clear } }.frame(maxWidth: .infinity)
            }
        }
        if let t = store.tour { TourBuses(tour: t) }
        NextDepartures(lines: lines)
        if let t = store.tour, !t.runs {
            Notice(text: t.next_run.map { tr("This tour does not run on the chosen date; it runs next on {date}.", ["date": longDate($0)]) } ?? String(localized: "This tour does not run within a year of the chosen date."), kind: .warn)
        }
        if let t = store.tour {
            VStack(alignment: .leading, spacing: 8) {
                HStack {
                    Text("Trips of the tour").font(.inter(12.5, .medium)).foregroundStyle(Theme.text2)
                    Spacer()
                    Text(LocalizedStringKey(compact ? "Tap a trip to start with it" : "Click a trip to start with it")).font(.inter(12.5)).foregroundStyle(Theme.muted)
                }
                TripsTable(tour: t, current: store.startTrip?.index, now: now)
            }
        }
        startAt
        RouteMapView()
        if let trip = store.startTrip, !trip.stops.isEmpty {
            StopsList(trip: trip)
        }
        IbisBox(trip: store.startTrip)
    }

    private var startAt: some View {
        let c = store.choice
        let m = store.map
        let auto = c.free ? String(localized: "Automatic (the map's first)") : String(localized: "Automatic (nearest to the first stop)")
        return Field("Start at") {
            SelectField(options: [(value: "-1", label: auto)] + (m?.entry_points ?? []).map { (value: String($0.index), label: $0.name) },
                        selection: String(c.entry), onChange: { store.act("entry", ["entry": Int($0) ?? -1]) })
        }
    }
}

/// The trips of the tour: departure (and how long the trip takes), line, from, to, arrival; the
/// one a start takes is marked, the ones gone by before the start time are dimmed; a click starts
/// the tour there.
struct TripsTable: View {
    @Environment(LauncherStore.self) private var store
    @Environment(\.compactLayout) private var compact
    var tour: TourInfo
    var current: Int?
    var now: Double

    var body: some View {
        ScrollViewReader { proxy in
            Scroll {
                LazyVStack(spacing: 0, pinnedViews: [.sectionHeaders]) {
                    Section {
                        ForEach(Array(tour.trips.enumerated()), id: \.element.index) { i, x in
                            row(x, on: x.index == current, past: x.arrival < now - 60 && x.index != current, odd: i % 2 == 1)
                                .id(x.index)
                        }
                    } header: {
                        header
                    }
                }
            }
            .frame(maxHeight: 300)
            .fieldBackground()
            .clipShape(RoundedRectangle(cornerRadius: Theme.radiusSm, style: .continuous))
            .onAppear { if let c = current { proxy.scrollTo(c, anchor: .center) } }
            .onChange(of: "\(tour.id)|\(current ?? -1)") { _, _ in
                if let c = current { withAnimation { proxy.scrollTo(c, anchor: .center) } }
            }
        }
    }

    private var header: some View {
        HStack(spacing: 10) {
            Text("Departs").frame(width: 58, alignment: .leading)
            Text("Line").frame(width: 70, alignment: .leading)
            if !compact { Text("From").frame(maxWidth: .infinity, alignment: .leading) }
            Text("To").frame(maxWidth: .infinity, alignment: .leading)
            Text("Arrives").frame(width: 54, alignment: .leading)
        }
        .font(.inter(12, .semibold)).foregroundStyle(Theme.muted)
        .padding(.horizontal, 10).padding(.vertical, 8)
        .background(Theme.surface2)
    }

    private func row(_ x: TripInfo, on: Bool, past: Bool, odd: Bool) -> some View {
        Button { store.act("pick_trip", ["index": x.index, "departure": x.departure]) } label: {
            HStack(spacing: 10) {
                VStack(alignment: .leading, spacing: 1) {
                    Text(hhmm(x.departure)).font(.inter(14, .bold).monospacedDigit())
                    Text(tripTime(x)).font(.inter(11.5)).foregroundStyle(Theme.muted).lineLimit(2)
                }
                .frame(width: 58, alignment: .leading)
                Group {
                    if isDepotRun(x) { Text("Depot run").font(.inter(12.5)).foregroundStyle(Theme.muted).lineLimit(1) } else { LineBadge(line: x.line) }
                }
                .frame(width: 70, alignment: .leading)
                if !compact { Text(x.from).lineLimit(1).frame(maxWidth: .infinity, alignment: .leading) }
                Text(x.terminus).lineLimit(1).frame(maxWidth: .infinity, alignment: .leading)
                Text(hhmm(x.arrival)).font(.inter(14).monospacedDigit()).foregroundStyle(Theme.muted).frame(width: 54, alignment: .leading)
            }
            .font(.inter(14))
            .foregroundStyle(past ? Theme.muted : Theme.text)
            .padding(.horizontal, 10).padding(.vertical, 8)
            .background(on ? Theme.accentSoft : (odd ? Color.white.opacity(0.018) : .clear))
            .overlay(alignment: .leading) { if on { Rectangle().fill(Theme.accent).frame(width: 3) } }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

/// The stops of the trip a start takes, with their times.
struct StopsList: View {
    var trip: TripInfo
    @State private var open = false

    var body: some View {
        DisclosureGroup(isExpanded: $open) {
            VStack(alignment: .leading, spacing: 4) {
                ForEach(Array(trip.stops.enumerated()), id: \.offset) { _, s in
                    HStack(spacing: 12) {
                        Text(hhmm(s.dep > 0 ? s.dep : s.arr)).font(.inter(13, .semibold).monospacedDigit()).foregroundStyle(Theme.text2).frame(width: 46, alignment: .leading)
                        Text(s.name).font(.inter(13)).lineLimit(1)
                    }
                }
            }
            .padding(.top, 8)
        } label: {
            Text(tr("Stops of the trip ({n})", ["n": "\(trip.stops.count)"])).font(.inter(12.5, .medium)).foregroundStyle(Theme.text2)
        }
        .tint(Theme.muted)
    }
}

/// What to type into the IBIS for the duty.
struct IbisBox: View {
    @Environment(LauncherStore.self) private var store
    var trip: TripInfo?
    @State private var reply: (key: String, info: IbisInfo?, error: String?)?

    var body: some View {
        let c = store.choice
        let hof = c.hof.range(of: #"\.(bus|ovh)"#, options: [.regularExpression, .caseInsensitive]) != nil ? "" : c.hof
        let stops = trip?.stops.map(\.name) ?? []
        let key = c.free || c.bus.isEmpty || (c.line ?? "").isEmpty ? "" : "\(c.bus)|\(hof)|\(c.line ?? "")|\(trip?.terminus ?? "")|\(stops.joined(separator: "|"))"
        Group {
            if let r = reply, r.key == key, !key.isEmpty {
                VStack(alignment: .leading, spacing: 6) {
                    Text("IBIS").font(.inter(12.5, .medium)).foregroundStyle(Theme.text2)
                    if let e = r.error {
                        Notice(text: e, kind: .warn)
                    } else if let i = r.info {
                        content(i, line: c.line ?? "")
                    }
                }
                .padding(.horizontal, 12).padding(.vertical, 10)
                .fieldBackground()
            }
        }
        .task(id: key) {
            guard !key.isEmpty else { return }
            let args: [String: Any] = ["bus": c.bus, "hof": hof, "line": c.line ?? "", "terminus": trip?.terminus ?? "", "stops": stops]
            let r = await Task.detached(priority: .utility) { () -> (IbisInfo?, String?) in
                do { return (try Bridge.api(IbisInfo.self, "ibis", args), nil) } catch { return (nil, error.localizedDescription) }
            }.value
            reply = (key, r.0, r.1)
        }
    }

    @ViewBuilder
    private func content(_ i: IbisInfo, line: String) -> some View {
        if i.routes.isEmpty {
            Text(tr("Depot file {hof} has no entries for this line. Type line {line} and the terminus code by hand, or press Shift+U.", ["hof": i.hof, "line": i.line_code.isEmpty ? line : i.line_code]))
                .font(.inter(13)).foregroundStyle(Theme.muted).fixedSize(horizontal: false, vertical: true)
        } else {
            let routes = i.routes.sorted { ($0.code == i.trip_route ? 0 : 1) < ($1.code == i.trip_route ? 0 : 1) }.prefix(3)
            Grid(alignment: .leading, horizontalSpacing: 12, verticalSpacing: 6) {
                GridRow {
                    Text(verbatim: "")
                    Text("Line")
                    Text("Route code")
                }
                .font(.inter(12, .semibold)).foregroundStyle(Theme.muted)
                ForEach(Array(routes.enumerated()), id: \.offset) { _, r in
                    let codes = r.code.count > 2 ? (String(r.code.dropLast(2)), String(r.code.suffix(2))) : (i.line_code.isEmpty ? line : i.line_code, r.code)
                    let on = r.code == i.trip_route
                    GridRow {
                        Text((r.name.isEmpty ? r.terminus : r.name).trimmingCharacters(in: .whitespaces)).font(.inter(13, on ? .bold : .semibold)).foregroundStyle(on ? Theme.accent : Theme.text).lineLimit(1)
                            .frame(maxWidth: .infinity, alignment: .leading)
                        LineBadge(line: codes.0)
                        LineBadge(line: codes.1)
                    }
                }
            }
            Text(tr("Depot file {hof} | Shift+U in the game types it for you", ["hof": i.hof])).font(.inter(12)).foregroundStyle(Theme.muted)
        }
    }
}

/// The map of the duty in its card.
struct RouteMapView: View {
    @Environment(\.compactLayout) private var compact
    var body: some View {
        MapCanvas()
            .frame(height: compact ? 240 : 300)
    }
}

// --- the time, the weather, the road -----------------------------------------------------------

struct TimeCard: View {
    @Environment(LauncherStore.self) private var store
    @State private var time = ""
    @State private var date = ""
    @State private var editing: String?
    @State private var picking = false

    var body: some View {
        let c = store.choice
        let realTime = store.snapshot.real_time || store.flag("use_real_time")
        let realDate = store.flag("use_real_date")
        Card("Time and weather") {
            HStack(alignment: .top, spacing: 12) {
                Field("Start time", help: realTime ? String(localized: "Follows the clock (Settings)") : nil) {
                    InputField(prompt: "09:00", text: $time, mono: true) { commitTime() }
                        .disabled(realTime)
                        .onAppear { time = c.hhmm }
                        .onChange(of: c.time) { _, _ in time = store.choice.hhmm }
                }
                Field("Date", help: realDate ? String(localized: "Today (Settings)") : nil) {
                    HStack(spacing: 6) {
                        InputField(prompt: "1989-05-30", text: $date, mono: true) { commitDate() }
                            .disabled(realDate)
                            .onAppear { date = c.date }
                            .onChange(of: c.date) { _, v in date = v }
                        Button { picking = true } label: { Image(systemName: "calendar") }
                            .buttonStyle(.launcher(.ghost)).disabled(realDate).help("Choose a date")
                            .popover(isPresented: $picking) { DateChooser { picking = false }.launcherSheet(store) }
                    }
                }
            }
            HStack(spacing: 6) {
                Button { store.act("now", ["time": true]) } label: { Label("Current time", systemImage: "clock") }
                    .buttonStyle(.launcher(.ghost, small: true)).disabled(realTime)
                Button { store.act("now", ["time": false, "date": true]) } label: { Label("Today", systemImage: "calendar") }
                    .buttonStyle(.launcher(.ghost, small: true)).disabled(realDate)
            }
            .padding(.top, -6)
            Field("Season") {
                SelectField(options: [("auto", "By date"), ("spring", "Spring"), ("summer", "Summer"), ("autumn", "Autumn"), ("winter", "Winter")].map { (value: $0.0, label: NSLocalizedString($0.1, comment: "")) },
                            selection: c.season, onChange: { store.act("season", ["season": $0]) })
            }
            if c.season != "auto" {
                Segmented(items: [("early", "Early"), ("mid", "Mid"), ("late", "Late")], selection: c.phase, full: true) { store.act("phase", ["phase": $0]) }
            }
            Field("Weather", help: weatherHelp) {
                SelectField(options: weatherOptions, selection: weatherValue, onChange: pickWeather)
            }
            CabinTemperatureOption()
            if c.weather.hasPrefix("metar:") {
                Field("Airport") { AirportField(code: WeatherChoice.metar(c)) }
            }
            if WeatherChoice.editable(c.weather), !WeatherChoice.isCustom(c.weather) {
                Button { editing = c.weather } label: { Label("Edit selected weather as custom", systemImage: "slider.horizontal.3") }
                    .buttonStyle(.launcher(.ghost, small: true))
            }
        }
        .sheet(isPresented: Binding(get: { editing != nil }, set: { if !$0 { editing = nil } })) {
            WeatherEditor(from: editing ?? "custom:").launcherSheet(store)
        }
    }

    private var weatherValue: String {
        let w = store.choice.weather
        return w.hasPrefix("metar:") ? "metar:" : WeatherChoice.isCustom(w) ? "custom:" : w
    }

    private func pickWeather(_ v: String) {
        let w = store.choice.weather
        switch v {
        case "custom:": editing = WeatherChoice.isCustom(w) ? w : "custom:"
        case "metar:": store.act("weather", ["weather": "metar:\(WeatherChoice.metar(store.choice))"])
        default: store.act("weather", ["weather": v])
        }
    }

    private var weatherOptions: [(value: String, label: String)] {
        let w = store.choice.weather
        var o: [(value: String, label: String)] = [
            ("", String(localized: "Natural weather")),
            ("custom:", String(localized: "Custom weather")),
        ]
        if w.hasPrefix("metar:") || !WeatherChoice.metar(store.choice).isEmpty { o.append(("metar:", String(localized: "Current weather"))) }
        o += [("cycle", String(localized: "Weather cycle")), ("tomorrow", "Tomorrow.io")]
        o += store.lists.weathers.filter(fits).map { (value: $0.file, label: $0.name) }
        if !w.isEmpty, !w.hasPrefix("metar:"), !WeatherChoice.isCustom(w), !o.contains(where: { $0.value == w }) { o.append((value: w, label: store.weatherTitle)) }
        return o
    }

    private var weatherHelp: String {
        let w = store.choice.weather
        if w.isEmpty { return String(localized: "Develops by itself through the day and the season") }
        if WeatherChoice.isCustom(w) { return String(localized: "Set visibility, wind, clouds, rain, temperature and road state") }
        if w.hasPrefix("metar:") { return WeatherChoice.metarText(WeatherChoice.metar(store.choice)) }
        if w == "cycle" { return String(localized: "Changes every 25-60 minutes, as the month allows") }
        if w == "tomorrow" { return String(localized: "Current weather") }
        if let x = store.weather {
            return x.description.isEmpty ? "\(Int(x.temp)) °C - \(precipText(x.precip)) - \(x.fog_m >= 10000 ? String(localized: "clear air") : "\(Int(x.fog_m)) m")" : x.description
        }
        return ""
    }

    /// The presets that fit the season (`State::weather_fits`).
    private func fits(_ w: WeatherInfo) -> Bool {
        let rain = w.precip.hasPrefix("rain")
        switch store.snapshot.season {
        case "winter": return w.snow || (rain ? w.temp <= 10 : w.temp <= 16)
        case let s:
            if w.snow || w.temp <= 0 { return false }
            return s != "summer" || w.temp >= 8
        }
    }

    /// HH:MM typed in 24 hours (the timetables' clock).
    private func commitTime() {
        let parts = time.split(whereSeparator: { ":. ".contains($0) }).compactMap { Int($0) }
        let digits = time.filter(\.isNumber)
        var h = parts.first ?? -1, m = parts.count > 1 ? parts[1] : 0
        if parts.count == 1, digits.count == 4, let n = Int(digits) { h = n / 100; m = n % 100 }
        if (0 ..< 24).contains(h), (0 ..< 60).contains(m) { store.act("time", ["minutes": h * 60 + m]) } else { time = store.choice.hhmm }
    }

    /// A date typed as the timetables write it (YYYY-MM-DD).
    private func commitDate() {
        let f = DateFormatter()
        f.dateFormat = "yyyy-MM-dd"
        f.locale = Locale(identifier: "en_US_POSIX")
        f.isLenient = false
        if let d = f.date(from: date.trimmingCharacters(in: .whitespaces)) { store.act("date", ["date": f.string(from: d)]) } else { date = store.choice.date }
    }
}

struct CabinTemperatureOption: View {
    @Environment(LauncherStore.self) private var store

    var body: some View {
        let temperature = store.setting("units")?.string == "us" ? "72°F" : "22°C"
        VStack(alignment: .leading, spacing: 4) {
            ToggleRow(label: "Start with a comfortable cabin temperature", help: LocalizedStringKey(tr("Sets the cabin to {temperature} when starting a drive.", ["temperature": temperature])), isOn: store.choice.comfortable_cabin_on_start) {
                store.act("flag", ["name": "comfortable_cabin_on_start", "on": $0])
            }
        }
    }
}

struct ConditionsCard: View {
    @Environment(LauncherStore.self) private var store

    var body: some View {
        let c = store.choice
        Card("On the road") {
            VStack(alignment: .leading, spacing: 6) {
                SliderField(label: "Cars around", value: c.traffic, range: 0 ... 120, step: 1, format: { $0 < 0.5 ? String(localized: "None") : "\(Int($0))" }) {
                    store.act("traffic", ["value": $0])
                }
                ToggleRow(label: "Passengers", help: "People wait at the stops and ride along", isOn: c.passengers) { flag("passengers", $0) }
                ToggleRow(label: "Timetable buses", help: "Other buses of the timetable drive their lines", isOn: c.schedule) { flag("schedule", $0) }
                ToggleRow(label: "Bus ready to go", help: "Engine running, doors and lights set (Shift+U)", isOn: c.autostart) { flag("autostart", $0) }
                ToggleRow(label: "Start on foot", help: "Walk to the bus, or place one later from the menu", isOn: c.on_foot) { flag("on_foot", $0) }
            }
        }
    }

    private func flag(_ name: String, _ on: Bool) {
        store.act("flag", ["name": name, "on": on])
    }
}

// --- the summary -------------------------------------------------------------------------------

struct Summary: View {
    @Environment(LauncherStore.self) private var store
    @Environment(\.compactLayout) private var compact
    var showStart: Bool
    @State private var showArgs = false

    var body: some View {
        let c = store.choice
        let trip = store.startTrip
        VStack(alignment: .leading, spacing: 0) {
            ZStack(alignment: .bottomLeading) {
                MapPictureView(url: store.map.map { store.mapPicture($0.file) } ?? nil, radius: 0)
                if let m = store.map {
                    LinearGradient(colors: [.black.opacity(0.72), .clear], startPoint: .bottom, endPoint: .top)
                        .frame(height: 96)
                    VStack(alignment: .leading, spacing: 1) {
                        Text(m.title).font(.inter(18, .bold)).lineLimit(1)
                        Text("\(store.dateTitle) - \(c.hhmm)").font(.inter(12.5).monospacedDigit()).opacity(0.85)
                    }
                    .foregroundStyle(.white)
                    .padding(.horizontal, 16).padding(.bottom, 12)
                }
            }
            .frame(height: compact ? 120 : 170)
            .clipped()
            VStack(alignment: .leading, spacing: 14) {
                if let trip, !isDepotRun(trip) {
                    DestinationSign(number: trip.line, to: trip.terminus)
                } else if c.free {
                    DestinationSign(number: nil, icon: "point.topleft.down.to.point.bottomright.curvepath.fill", to: String(localized: "Free drive"))
                }
                if compact && showStart { StartButton(big: true) }
                Facts(rows: facts)
                if c.lan_mode == "join" {
                    HStack(spacing: 10) {
                        Image(systemName: "person.2.fill").foregroundStyle(Theme.muted)
                        Text(tr("Joining {server}", ["server": c.lan_addr.isEmpty ? store.snapshot.joined_server ?? "" : c.lan_addr])).font(.inter(13.5)).lineLimit(1)
                        Spacer()
                        Button("Leave") { store.act("server_leave"); store.act("lan", ["mode": "off"]) }.buttonStyle(.launcher(.ghost, small: true))
                    }
                    .padding(.horizontal, 12).padding(.vertical, 8)
                    .fieldBackground()
                } else if c.lan_mode == "host" {
                    Notice(text: String(localized: "The next drive is hosted for others to join."), icon: "person.2.fill")
                }
                if showStart && !compact {
                    StartButton(big: true)
                }
                let running = store.snapshot.instances.filter { $0.running != false }.count
                if running > 0 {
                    HStack(spacing: 4) {
                        Text(tr("{n} game(s) running", ["n": "\(running)"]) + " -").foregroundStyle(Theme.muted)
                        Button("Show") { store.page = .games; store.phoneTab = "more"; store.phoneMore = .games }.buttonStyle(.plain).foregroundStyle(Theme.accent)
                    }
                    .font(.inter(12.5))
                    .frame(maxWidth: .infinity)
                }
                if !store.snapshot.cmdline.isEmpty {
                    Button(showArgs ? "Hide the command line" : "Show the command line") { showArgs.toggle() }
                        .buttonStyle(.plain).font(.inter(12.5)).foregroundStyle(Theme.muted)
                    if showArgs { LogBlock(text: store.snapshot.cmdline) }
                }
            }
            .padding(.horizontal, 20).padding(.vertical, 18)
        }
        .surface()
        .clipShape(RoundedRectangle(cornerRadius: Theme.radius, style: .continuous))
    }

    private var facts: [(String, String)] {
        let c = store.choice
        var r: [(String, String)] = [("Bus", store.bus?.name ?? "-")]
        if !c.free {
            r.append(("Line", store.line?.name ?? "-"))
            r.append(("Tour", store.tour?.number ?? "-"))
        }
        if let t = store.startTrip { r.append(("First trip", "\(hhmm(t.departure)) - \(hhmm(t.arrival)) \(t.from)\(t.km > 0 ? String(format: " | %.1f km", t.km) : "")")) }
        if c.entry >= 0, let e = store.map?.entry_points.first(where: { $0.index == c.entry }) { r.append(("Start at", entryName(e))) }
        r.append(("Weather", store.weatherTitle))
        r.append(("Driver", store.snapshot.config.profile.isEmpty ? "-" : store.snapshot.config.profile))
        return r
    }
}

/// Start the duty (or the free drive, or join the session).
struct StartButton: View {
    @Environment(LauncherStore.self) private var store
    var big = true

    var body: some View {
        let c = store.choice
        let ready = store.map != nil && store.bus != nil && (c.free || (store.line != nil && store.tour != nil)) && !store.noServerBus && store.busOffered
        Button { store.start() } label: {
            Label(store.starting ? String(localized: "Starting…") : c.lan_mode == "join" ? String(localized: "Join and drive") : c.free ? String(localized: "Start driving") : String(localized: "Start the duty"),
                  systemImage: "play.fill")
        }
        .buttonStyle(.launcher(.primary, big: big, wide: true))
        .disabled(!ready || store.snapshot.in_game || store.starting)
        .keyboardShortcut(.defaultAction)
    }
}

/// The situations saved on this map: the last one, and the save slots.
struct ContinueCard: View {
    @Environment(LauncherStore.self) private var store
    @State private var deleting: SavedSituation?

    var body: some View {
        let saved = store.snapshot.saved
        let c = store.choice
        if !saved.isEmpty {
            Card("Continue") {
                VStack(spacing: 0) {
                    ForEach(Array(saved.prefix(5).enumerated()), id: \.offset) { i, s in
                        HStack(spacing: 2) {
                            ListItem(title: s.name, subtitle: Date(timeIntervalSince1970: TimeInterval(s.saved)).formatted(date: .abbreviated, time: .shortened), icon: "clock.arrow.circlepath", selected: false) {
                                if !store.starting { store.continueLast(pick: i) }
                            } trailing: {
                                Image(systemName: "play.fill").font(.system(size: 12)).foregroundStyle(Theme.muted)
                            }
                            if c.lan_mode != "join" { DeleteSaveButton { deleting = s } }
                        }
                    }
                }
                .padding(.horizontal, -8)
                .padding(.vertical, -6)
            }
            .modifier(SaveDelete(item: $deleting))
        }
    }
}

// --- the pickers -------------------------------------------------------------------------------

/// A picker's frame: its title and close button, the content, a foot with what it counts.
struct PickerFrame<Content: View, Foot: View>: View {
    var title: LocalizedStringKey
    @ViewBuilder var content: Content
    @ViewBuilder var foot: Foot
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text(title).font(.inter(17, .semibold))
                Spacer()
                Button { dismiss() } label: { Image(systemName: "xmark") }.buttonStyle(.ghost).help("Close")
            }
            .padding(.horizontal, 20).padding(.top, 16).padding(.bottom, 10)
            content.frame(maxWidth: .infinity, maxHeight: .infinity)
            HStack(spacing: 8) { foot }
                .padding(.horizontal, 20).padding(.vertical, 14)
        }
        .background(Theme.surface.ignoresSafeArea())
        #if os(macOS)
        .frame(minWidth: 860, idealWidth: 1080, minHeight: 560, idealHeight: 720)
        #endif
    }
}

/// The map picker: every installed map as a picture with its name.
struct MapPicker: View {
    @Environment(LauncherStore.self) private var store
    @Environment(\.dismiss) private var dismiss
    @State private var search = ""
    @State private var which = "all"

    var body: some View {
        let all = store.lists.maps
        let mods = all.filter { $0.installed == true }.count
        let maps = all
            .filter { which == "all" || (which == "mods") == ($0.installed == true) }
            .filter { search.isEmpty || "\($0.friendly) \($0.name) \($0.file)".localizedCaseInsensitiveContains(search) }
            .sorted { $0.title.localizedStandardCompare($1.title) == .orderedAscending }
        PickerFrame(title: "Choose a map") {
            VStack(alignment: .leading, spacing: 16) {
                HStack(spacing: 10) {
                    SearchField(prompt: "Search maps", text: $search).frame(maxWidth: 360)
                    if mods > 0 {
                        Segmented(items: [("all", "All"), ("stock", "OMSI 2"), ("mods", "Mods")], selection: which) { which = $0 }
                    }
                }
                .padding(.horizontal, 20)
                if maps.isEmpty { EmptyState(icon: "map", title: "No map found") }
                Scroll {
                    LazyVGrid(columns: [GridItem(.adaptive(minimum: 220), spacing: 14)], spacing: 14) {
                        ForEach(maps) { m in
                            let on = m.file == store.choice.map
                            Button {
                                store.act("select_map", ["file": m.file])
                                dismiss()
                            } label: {
                                VStack(alignment: .leading, spacing: 0) {
                                    MapPictureView(url: store.mapPicture(m.file), radius: 0).frame(height: 124)
                                    HStack(alignment: .top) {
                                        VStack(alignment: .leading, spacing: 2) {
                                            Text(m.title).font(.inter(14, .semibold)).lineLimit(1)
                                            Text("\(m.installed == true ? String(localized: "Mod") : "OMSI 2") - " + tr("{n} entry points", ["n": "\(m.entry_points.count)"])).font(.inter(12.5)).foregroundStyle(Theme.muted)
                                        }
                                        Spacer(minLength: 4)
                                        if on { Image(systemName: "checkmark.circle.fill").foregroundStyle(Theme.accent) }
                                    }
                                    .padding(.horizontal, 12).padding(.top, 10).padding(.bottom, 12)
                                }
                                .background(on ? Theme.accentSoft : Theme.surface2)
                                .clipShape(RoundedRectangle(cornerRadius: Theme.radius, style: .continuous))
                                .contentShape(Rectangle())
                            }
                            .buttonStyle(.plain)
                        }
                    }
                    .padding(.horizontal, 20)
                    .padding(.bottom, 8)
                }
            }
        } foot: {
            Text(tr("{n} maps", ["n": "\(maps.count)"])).font(.inter(12.5)).foregroundStyle(Theme.muted)
            Spacer()
            Button("Close") { dismiss() }.buttonStyle(.secondary)
        }
    }
}

/// The bus picker: the buses in a list (search, makers), the chosen one turning beside it with
/// its paints and description. A tap takes the bus at once (the preview is of the bus chosen).
struct BusPicker: View {
    @Environment(LauncherStore.self) private var store
    @Environment(\.dismiss) private var dismiss
    @State private var search = ""
    @State private var maker = ""
    @State private var show = "all"
    @State private var data = DriveData.shared

    var body: some View {
        let all = store.offeredVehicles
        let makers = Array(Set(all.map(\.manufacturer).filter { !$0.isEmpty })).sorted()
        let list = filterBuses(all, search: search, show: show, maker: maker, starred: data.starred)
        PickerFrame(title: "Choose a bus") {
            GeometryReader { geo in
                if geo.size.width >= 700 {
                    HStack(spacing: 0) {
                        listColumn(list, makers).frame(width: 360).background(Theme.sidebar)
                        Scroll { detail.padding(.bottom, 12) }
                    }
                } else {
                    VStack(spacing: 0) {
                        if store.bus != nil {
                            BusPreview().frame(height: 200)
                        }
                        listColumn(list, makers)
                    }
                }
            }
        } foot: {
            Text(tr("{n} buses", ["n": "\(list.count)"])).font(.inter(12.5)).foregroundStyle(Theme.muted)
            Spacer()
            Button { dismiss() } label: { Label("Take this bus", systemImage: "checkmark") }.buttonStyle(.primary).disabled(store.bus == nil)
        }
    }

    private func listColumn(_ list: [VehicleInfo], _ makers: [String]) -> some View {
        VStack(spacing: 10) {
            SearchField(prompt: "Search buses", text: $search)
            HStack(spacing: 8) {
                SelectField(options: [(value: "", label: String(localized: "All makers"))] + makers.map { (value: $0, label: $0) }, selection: maker, onChange: { maker = $0 })
                Segmented(items: [("all", "All"), ("fav", "Starred"), ("mods", "Mods")], selection: show) { show = $0 }
            }
            if store.noServerBus {
                Notice(text: String(localized: "The server offers none of the vehicles installed here"), kind: .danger)
            } else if list.isEmpty {
                if show == "fav" { EmptyState(icon: "star", title: "No starred bus", text: "Star a bus to find it here.") } else { EmptyState(icon: "bus", title: "No bus found") }
            }
            Scroll {
                BusTree(list: list, searching: !search.isEmpty) { store.act("select_bus", ["file": $0.file]) }
            }
        }
        .padding(14)
    }

    @ViewBuilder
    private var detail: some View {
        if let v = store.bus {
            VStack(alignment: .leading, spacing: 0) {
                BusPreview().frame(height: 340)
                VStack(alignment: .leading, spacing: 14) {
                    HStack {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(v.name).font(.inter(20, .semibold))
                            Text([v.manufacturer, v.type_name].filter { !$0.isEmpty }.joined(separator: " - ")).foregroundStyle(Theme.muted)
                        }
                        Spacer()
                        if v.installed == true { Tag(text: "Mod") }
                    }
                    if let m = v.missing_packs, !m.isEmpty {
                        Notice(text: tr("Parts of this bus come from packs that are not installed: {packs}", ["packs": m.joined(separator: ", ")]), kind: .warn)
                    }
                    Field("Paint") { PaintSelect(bus: v) }
                    if !v.description.isEmpty {
                        Text(v.description).font(.inter(14)).foregroundStyle(Theme.text2).textSelection(.enabled)
                    }
                    NotesBox(kind: "bus", file: v.file)
                    Text(v.file).font(.inter(12.5)).foregroundStyle(Theme.muted).textSelection(.enabled)
                }
                .padding(.horizontal, 20).padding(.vertical, 18)
            }
        } else {
            EmptyState(icon: "bus", title: "Choose a bus on the left")
        }
    }
}

func precipText(_ p: String) -> String {
    let n = String(p.drop(while: { !$0.isNumber }).dropLast())
    if p.hasPrefix("rain ") { return tr("rain {n}%", ["n": n]) }
    if p.hasPrefix("snow ") { return tr("snow {n}%", ["n": n]) }
    return String(localized: "dry")
}
