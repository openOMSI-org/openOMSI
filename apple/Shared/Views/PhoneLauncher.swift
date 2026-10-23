import SwiftUI

/// The launcher on a phone, as the phone launcher of the other systems lays it out
/// (`launcher/ui/js/mobile/mobile.js`): a layout of its own for a landscape phone and a
/// finger, not the desktop page made small. A rail of icons on the left; Drive is the bus
/// turning on the left with the Start button over it and the duty as grouped rows on the
/// right, each opening a full-screen sheet to choose from.
struct PhoneLauncher: View {
    @Environment(LauncherStore.self) private var store

    static let tabs: [(id: String, label: LocalizedStringKey, icon: String)] = [
        ("drive", "Drive", Page.drive.icon),
        ("multiplayer", "Multiplayer", Page.multiplayer.icon),
        ("profile", "Profile", Page.profile.icon),
        ("mods", "Mods", Page.mods.icon),
        ("settings", "Settings", Page.settings.icon),
        ("more", "More", "ellipsis.circle.fill"),
    ]

    var body: some View {
        HStack(spacing: 0) {
            rail
            Group {
                switch store.phoneTab {
                case "multiplayer": MultiplayerPage()
                case "profile": ProfilePage()
                case "mods": ModsPage()
                case "settings": SettingsPage()
                case "more": PhoneMore()
                default: PhoneDrive()
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(Theme.bg.ignoresSafeArea())
        }
        .overlay {
            if let s = store.phoneSheet {
                PhoneSheetView(sheet: s)
                    .transition(.move(edge: .trailing).combined(with: .opacity))
            }
        }
        .animation(.easeOut(duration: 0.18), value: store.phoneSheet)
        .onAppear {
            if !store.snapshot.root_ok {
                store.phoneTab = "more"
                store.phoneMore = .setup
            }
        }
    }

    private var rail: some View {
        VStack(spacing: 0) {
            ForEach(Self.tabs, id: \.id) { t in
                let on = store.phoneTab == t.id
                Button {
                    store.phoneTab = t.id
                    if t.id == "more" && on { store.phoneMore = nil }
                    Haptics.tap()
                } label: {
                    VStack(spacing: 3) {
                        Image(systemName: t.icon).font(.system(size: 20, weight: .medium))
                            .foregroundStyle(on ? Theme.accent : Theme.muted)
                            .frame(width: 52, height: 28)
                            .background(on ? Theme.accentSoft : .clear, in: Capsule())
                        Text(t.label).font(.inter(10.5, .semibold)).foregroundStyle(on ? Theme.text : Theme.muted).lineLimit(1).minimumScaleFactor(0.8)
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .overlay(alignment: .topTrailing) {
                        if t.id == "more" && (running > 0 || !store.snapshot.root_ok) {
                            Circle().fill(store.snapshot.root_ok ? Theme.accent : Theme.danger).frame(width: 8, height: 8).padding(.top, 12).padding(.trailing, 14)
                        }
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
        .padding(.vertical, 6)
        .frame(width: 76)
        .background(Theme.sidebar.ignoresSafeArea())
    }

    private var running: Int { store.snapshot.instances.filter { $0.running != false }.count }
}

// --- the rows ------------------------------------------------------------------------------------

/// A group of rows on a card of their own.
struct PhoneGroup<Content: View>: View {
    @ViewBuilder var content: Content
    var body: some View {
        VStack(spacing: 0) { content }
            .background(Theme.surface, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
            .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
    }
}

/// A row: its icon on a tile, the label, the value on the right, a chevron.
struct PhoneRow: View {
    var icon: String
    var label: LocalizedStringKey
    var value: String = ""
    var sub: String? = nil
    var trailing = "chevron.right"
    var divider = false
    var action: () -> Void

    var body: some View {
        Button {
            Haptics.tap()
            action()
        } label: {
            HStack(spacing: 12) {
                Image(systemName: icon).font(.system(size: 15, weight: .medium)).foregroundStyle(Theme.text2)
                    .frame(width: 30, height: 30)
                    .background(Theme.surface3, in: RoundedRectangle(cornerRadius: 8, style: .continuous))
                VStack(alignment: .leading, spacing: 1) {
                    Text(label).font(.inter(15, .medium)).foregroundStyle(Theme.text).lineLimit(1)
                    if let sub { Text(sub).font(.inter(11.5)).foregroundStyle(Theme.muted).lineLimit(1) }
                }
                .fixedSize()
                Text(value).font(.inter(15)).foregroundStyle(Theme.text2).lineLimit(1)
                    .frame(maxWidth: .infinity, alignment: .trailing)
                Image(systemName: trailing).font(.system(size: 13, weight: .semibold)).foregroundStyle(Theme.muted)
            }
            .padding(.leading, 14).padding(.trailing, 12)
            .frame(minHeight: 52)
            .overlay(alignment: .top) { if divider { Rectangle().fill(Color.white.opacity(0.04)).frame(height: 1) } }
            .contentShape(Rectangle())
        }
        .buttonStyle(PhonePressStyle())
    }
}

/// A pressed row darkens a little (a finger has no hover).
struct PhonePressStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label.background(configuration.isPressed ? Theme.surface2 : .clear)
    }
}

/// Two or more big choices side by side (Timetable | Free drive, the seasons).
struct PhoneSegmented: View {
    var items: [(String, LocalizedStringKey)]
    var selection: String
    var onChange: (String) -> Void

    var body: some View {
        HStack(spacing: 4) {
            ForEach(items, id: \.0) { item in
                let on = item.0 == selection
                Button { onChange(item.0); Haptics.tap() } label: {
                    Text(item.1).font(.inter(14.5, .semibold)).foregroundStyle(on ? Theme.text : Theme.text2).lineLimit(1).minimumScaleFactor(0.8)
                        .frame(maxWidth: .infinity, minHeight: 40)
                        .background(on ? Theme.surface3 : .clear, in: RoundedRectangle(cornerRadius: 9, style: .continuous))
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
        .padding(4)
        .background(Theme.surface, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
    }
}

// --- Drive -----------------------------------------------------------------------------------

struct PhoneDrive: View {
    @Environment(LauncherStore.self) private var store
    @State private var deleting: SavedSituation?

    var body: some View {
        GeometryReader { geo in
            HStack(spacing: 0) {
                stage.frame(width: geo.size.width * 0.46)
                list
            }
        }
    }

    // the bus, the chips over it, the sign and Start at its foot
    private var stage: some View {
        let c = store.choice
        return ZStack {
            Color(hex: 0x1A1A1A)
            if store.bus != nil && store.phoneSheet == nil {
                BusPreview(buttonsTop: true)
            }
        }
        .overlay(alignment: .topLeading) {
            HStack(spacing: 8) {
                chip("map.fill", store.map?.title ?? "-") { store.phoneSheet = .map }
                chip("clock.fill", c.hhmm) { store.phoneSheet = .time }
            }
            .padding(10)
        }
        .overlay(alignment: .bottom) {
            VStack(spacing: 8) {
                if let t = store.startTrip, !isDepotRun(t) {
                    DestinationSign(number: t.line, to: t.terminus)
                } else if c.free {
                    DestinationSign(number: nil, icon: "point.topleft.down.to.point.bottomright.curvepath.fill", to: String(localized: "Free drive"))
                }
                PhoneStartButton()
            }
            .padding(12)
        }
        .clipped()
        .ignoresSafeArea(edges: .bottom)
    }

    private func chip(_ icon: String, _ text: String, _ action: @escaping () -> Void) -> some View {
        Button(action: action) {
            HStack(spacing: 6) {
                Image(systemName: icon).font(.system(size: 13, weight: .semibold))
                Text(text).font(.inter(13, .semibold).monospacedDigit()).lineLimit(1)
            }
            .foregroundStyle(Theme.text)
            .padding(.horizontal, 12).frame(height: 34)
            .background(Theme.bg.opacity(0.78), in: Capsule())
        }
        .buttonStyle(.plain)
    }

    private var list: some View {
        let c = store.choice
        let open = { (s: PhoneSheet) in { store.phoneSheet = s } }
        return Scroll {
            VStack(spacing: 12) {
                MultiplayerDutyNotice()
                if store.noServerBus {
                    Notice(text: String(localized: "The server offers none of the vehicles installed here"), kind: .danger)
                }
                PhoneGroup {
                    PhoneRow(icon: "map.fill", label: "Map", value: store.map?.title ?? "-", action: open(.map))
                    PhoneRow(icon: "bus.fill", label: "Bus", value: store.bus?.name ?? "-", divider: true, action: open(.bus))
                    if let b = store.bus {
                        PhoneRow(icon: "paintbrush.fill", label: "Paint", value: c.paint.isEmpty ? (b.default_paint?.isEmpty == false ? b.default_paint! : String(localized: "Standard")) : c.paint, divider: true, action: open(.paint))
                    }
                }
                PhoneSegmented(items: [("line", "Timetable"), ("free", "Free drive")], selection: c.free ? "free" : "line") { store.act("free", ["on": $0 == "free"]) }
                PhoneGroup {
                    if !c.free {
                        PhoneRow(icon: "point.topleft.down.to.point.bottomright.curvepath.fill", label: "Line", value: store.line?.name ?? (store.snapshot.loading_lines ? "…" : "-"), action: open(.line))
                        PhoneRow(icon: "calendar", label: "Tour", value: store.tour.map { "\($0.number) - \($0.days)" } ?? "-", divider: true, action: open(.tour))
                        PhoneRow(icon: "clock.arrow.circlepath", label: "First trip", value: store.startTrip.map { "\(hhmm($0.departure)) \($0.from)" } ?? "-", sub: store.startTrip.flatMap { $0.km > 0 ? String(format: "%.1f km", $0.km) : nil }, divider: true, action: open(.trip))
                    }
                    PhoneRow(icon: "mappin.and.ellipse", label: "Start at", value: c.entry < 0 ? String(localized: "Automatic") : (store.map?.entry_points.first { $0.index == c.entry }?.name ?? "-"), divider: !c.free, action: open(.entry))
                }
                PhoneGroup {
                    PhoneRow(icon: "clock.fill", label: "Time and date", value: "\(c.hhmm) - \(c.date)", sub: realClock ? String(localized: "Follows the clock (Settings)") : nil, action: open(.time))
                    PhoneRow(icon: "cloud.sun.fill", label: "Weather", value: store.weatherTitle, divider: true, action: open(.weather))
                }
                VStack(alignment: .leading, spacing: 4) {
                    SliderField(label: "Cars around", value: c.traffic, range: 0 ... 120, step: 1, format: { $0 < 0.5 ? String(localized: "None") : "\(Int($0))" }) { store.act("traffic", ["value": $0]) }
                    toggle("Passengers", "passengers", c.passengers)
                    toggle("Timetable buses", "schedule", c.schedule)
                    toggle("Bus ready to go", "autostart", c.autostart)
                    CabinTemperatureOption()
                    toggle("Start on foot", "on_foot", c.on_foot)
                }
                .padding(.horizontal, 14).padding(.vertical, 10)
                .background(Theme.surface, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                if !store.snapshot.saved.isEmpty {
                    PhoneGroup {
                        ForEach(Array(store.snapshot.saved.prefix(3).enumerated()), id: \.offset) { i, s in
                            HStack(spacing: 0) {
                                PhoneRow(icon: "clock.arrow.circlepath", label: LocalizedStringKey(s.name), value: Date(timeIntervalSince1970: TimeInterval(s.saved)).formatted(date: .abbreviated, time: .shortened), trailing: "play.fill") {
                                    if !store.starting { store.continueLast(pick: i) }
                                }
                                if c.lan_mode != "join" { DeleteSaveButton { deleting = s }.padding(.trailing, 6) }
                            }
                            .overlay(alignment: .top) { if i > 0 { Rectangle().fill(Color.white.opacity(0.04)).frame(height: 1) } }
                        }
                    }
                    .modifier(SaveDelete(item: $deleting))
                }
            }
            .padding(.horizontal, 14)
            .padding(.top, 12)
            .padding(.bottom, 16)
        }
    }

    private var realClock: Bool { store.snapshot.real_time || store.flag("use_real_time") || store.flag("use_real_date") }

    private func toggle(_ label: LocalizedStringKey, _ name: String, _ on: Bool) -> some View {
        Toggle(isOn: Binding(get: { on }, set: { store.act("flag", ["name": name, "on": $0]) })) {
            Text(label).font(.inter(15, .medium)).foregroundStyle(Theme.text).frame(maxWidth: .infinity, alignment: .leading)
        }
        .toggleStyle(TrailingSwitchStyle())
        .padding(.vertical, 8)
    }
}

/// The phone's switch rows: the name on the left, the switch on the right.
struct TrailingSwitchStyle: ToggleStyle {
    func makeBody(configuration: Configuration) -> some View {
        Button { configuration.isOn.toggle(); Haptics.tap() } label: {
            HStack(spacing: 12) {
                configuration.label
                ZStack(alignment: configuration.isOn ? .trailing : .leading) {
                    Capsule().fill(configuration.isOn ? Theme.accent : Theme.surface4).frame(width: 44, height: 26)
                    Circle().fill(configuration.isOn ? Theme.accentText : Theme.text2).frame(width: 20, height: 20).padding(.horizontal, 3)
                }
                .animation(.easeOut(duration: 0.12), value: configuration.isOn)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

/// The big amber Start button over the bus.
struct PhoneStartButton: View {
    @Environment(LauncherStore.self) private var store

    var body: some View {
        let c = store.choice
        let ready = store.map != nil && store.bus != nil && (c.free || (store.line != nil && store.tour != nil))
        let enabled = ready && !store.snapshot.in_game && !store.starting && !store.noServerBus && store.busOffered
        Button { store.start() } label: {
            HStack(spacing: 10) {
                Image(systemName: "play.fill").font(.system(size: 20, weight: .bold))
                Text(store.starting ? String(localized: "Starting…") : c.lan_mode == "join" ? String(localized: "Join and drive") : c.free ? String(localized: "Start driving") : String(localized: "Start the duty"))
                    .font(.inter(17, .bold))
            }
            .foregroundStyle(Theme.accentText)
            .frame(maxWidth: .infinity, minHeight: 56)
            .background(Theme.accent, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
            .shadow(color: .black.opacity(0.4), radius: 12, y: 8)
            .opacity(enabled ? 1 : 0.5)
        }
        .buttonStyle(.plain)
        .disabled(!enabled)
    }
}

// --- the sheets ------------------------------------------------------------------------------

enum PhoneSheet: String, Identifiable {
    case map, bus, paint, line, tour, trip, entry, time, weather
    var id: String { rawValue }

    var title: LocalizedStringKey {
        switch self {
        case .map: "Choose a map"
        case .bus: "Choose a bus"
        case .paint: "Paint"
        case .line: "Line"
        case .tour: "Tour"
        case .trip: "Trips of the tour"
        case .entry: "Start at"
        case .time: "Time and date"
        case .weather: "Weather"
        }
    }
}

/// A full-screen sheet: a title bar with Back, and what to choose from.
struct PhoneSheetView: View {
    @Environment(LauncherStore.self) private var store
    var sheet: PhoneSheet
    @State private var search = ""
    @State private var editing: String?
    @State private var show = "all"
    @State private var data = DriveData.shared

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 6) {
                Button { close() } label: {
                    Image(systemName: "chevron.left").font(.system(size: 20, weight: .semibold)).foregroundStyle(Theme.text)
                        .frame(width: 44, height: 44).contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                Text(sheet.title).font(.inter(17, .semibold))
                Spacer()
            }
            .padding(.horizontal, 8)
            .frame(height: 56)
            .background(Theme.sidebar.ignoresSafeArea(edges: [.top, .horizontal]))
            content
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        }
        .background(Theme.bg.ignoresSafeArea())
    }

    private func close() { store.phoneSheet = nil }

    @ViewBuilder
    private var content: some View {
        switch sheet {
        case .map: list { mapTiles }
        case .bus: list { busList }
        case .paint: list { paints }
        case .line: list { lines }
        case .tour: list { tours }
        case .trip: split { trips } map: { RouteMapPanel() }
        case .entry: split { entries } map: { RouteMapPanel() }
        case .time: list { PhoneTimeSheet() }
        case .weather:
            list { weathers }
                .sheet(isPresented: Binding(get: { editing != nil }, set: { if !$0 { editing = nil } })) {
                    WeatherEditor(from: editing ?? "custom:").launcherSheet(store)
                }
        }
    }

    private func list<C: View>(@ViewBuilder _ c: () -> C) -> some View {
        Scroll {
            VStack(spacing: 6) { c() }
                .padding(.horizontal, 16).padding(.top, 12).padding(.bottom, 16)
        }
    }

    private func split<L: View, M: View>(@ViewBuilder _ l: () -> L, @ViewBuilder map: () -> M) -> some View {
        let rows = l()
        return HStack(alignment: .top, spacing: 12) {
            ScrollViewReader { proxy in
                Scroll {
                    VStack(spacing: 6) { rows }.padding(.bottom, 16)
                }
                .onAppear { proxy.scrollTo("on", anchor: .center) }
            }
            map().clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous)).padding(.bottom, 12)
        }
        .padding(.horizontal, 16).padding(.top, 12)
    }

    /// A choice: a big row on the surface, the chosen one on the accent's soft ground with a check.
    private func pick<C: View>(_ on: Bool, dim: Bool = false, _ action: @escaping () -> Void, @ViewBuilder _ c: () -> C) -> some View {
        Button { Haptics.tap(); action() } label: {
            HStack(spacing: 12) {
                c()
                Spacer(minLength: 4)
                if on { Image(systemName: "checkmark").font(.system(size: 15, weight: .bold)).foregroundStyle(Theme.accent) }
            }
            .padding(.horizontal, 14).padding(.vertical, 8)
            .frame(maxWidth: .infinity, minHeight: 56, alignment: .leading)
            .background(on ? Theme.accentSoft : Theme.surface, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
            .opacity(dim ? 0.6 : 1)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .id(on ? "on" : nil)
    }

    private func text(_ title: String, _ sub: String? = nil) -> some View {
        VStack(alignment: .leading, spacing: 1) {
            Text(title).font(.inter(15, .semibold)).foregroundStyle(Theme.text).lineLimit(1)
            if let sub, !sub.isEmpty { Text(sub).font(.inter(12.5)).foregroundStyle(Theme.muted).lineLimit(1) }
        }
    }

    // maps as picture tiles
    @ViewBuilder
    private var mapTiles: some View {
        if let m = store.map, store.choice.lan_mode != "join" { NotesBox(kind: "map", file: m.file).padding(.bottom, 6) }
        LazyVGrid(columns: [GridItem(.adaptive(minimum: 200), spacing: 12)], spacing: 12) {
            ForEach(store.lists.maps) { m in
                let on = m.file == store.choice.map
                Button {
                    store.act("select_map", ["file": m.file]); close()
                } label: {
                    VStack(alignment: .leading, spacing: 2) {
                        MapPictureView(url: store.mapPicture(m.file), radius: 0).frame(height: 110).padding(.bottom, 6)
                        HStack {
                            Text(m.title).font(.inter(15, .semibold)).lineLimit(1)
                            Spacer()
                            if on { Image(systemName: "checkmark").font(.system(size: 14, weight: .bold)).foregroundStyle(Theme.accent) }
                        }
                        .padding(.horizontal, 12)
                        Text(tr("{n} entry points", ["n": "\(m.entry_points.count)"])).font(.inter(12.5)).foregroundStyle(Theme.muted).padding(.horizontal, 12).padding(.bottom, 10)
                    }
                    .background(on ? Theme.accentSoft : Theme.surface)
                    .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
    }

    @ViewBuilder
    private var busList: some View {
        HStack(spacing: 8) {
            Image(systemName: "magnifyingglass").foregroundStyle(Theme.muted)
            TextField("", text: $search, prompt: Text("Search buses").foregroundStyle(Theme.muted))
                .textFieldStyle(.plain).font(.inter(15))
                #if os(iOS)
                .textInputAutocapitalization(.never).autocorrectionDisabled()
                #endif
        }
        .padding(.horizontal, 14).frame(height: 46)
        .background(Theme.surface2, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        .padding(.bottom, 4)
        PhoneSegmented(items: [("all", "All"), ("fav", "Starred"), ("mods", "Mods")], selection: show) { show = $0 }
            .padding(.bottom, 4)
        let buses = filterBuses(store.offeredVehicles, search: search, show: show, starred: data.starred)
        if store.noServerBus {
            Notice(text: String(localized: "The server offers none of the vehicles installed here"), kind: .danger)
        } else if buses.isEmpty {
            if show == "fav" { EmptyState(icon: "star", title: "No starred bus", text: "Star a bus to find it here.") } else { EmptyState(icon: "bus", title: "No bus found") }
        }
        BusTree(list: buses, searching: !search.isEmpty, big: true) { v in
            store.act("select_bus", ["file": v.file])
            close()
        }
    }

    @ViewBuilder
    private var paints: some View {
        if let v = store.bus {
            let standard = v.default_paint?.isEmpty == false ? "\(String(localized: "Standard")) (\(v.default_paint!))" : String(localized: "Standard")
            let all = [("", standard)] + v.paints.map { ($0, $0) }
            ForEach(Array(all.enumerated()), id: \.element.0) { i, p in
                pick(store.choice.paint == p.0, { store.act("paint", ["paint": p.0]); close() }) {
                    text(p.1)
                    Spacer(minLength: 4)
                    Text("\(i + 1) / \(all.count)").font(.inter(12.5).monospacedDigit()).foregroundStyle(Theme.muted)
                }
            }
            NotesBox(kind: "bus", file: v.file).padding(.top, 6)
        }
    }

    @ViewBuilder
    private var lines: some View {
        let all = store.lines.filter { $0.user_allowed && !$0.tours.isEmpty }
        NextDepartures(lines: all, big: true) { close() }
            .padding(.bottom, 6)
        LineList(lines: all, big: true) { close() }
    }

    @ViewBuilder
    private var tours: some View {
        if let line = store.line {
            if let t = store.tour { TourBuses(tour: t).padding(.bottom, 6) }
            TourList(line: line, big: true) { close() }
        }
    }

    @ViewBuilder
    private var trips: some View {
        if let t = store.tour {
            let cur = store.startTrip?.index
            ForEach(t.trips) { x in
                pick(x.index == cur, { store.act("pick_trip", ["index": x.index, "departure": x.departure]) }) {
                    Text(hhmm(x.departure)).font(.inter(15, .bold).monospacedDigit()).frame(width: 48, alignment: .leading)
                    Group {
                        if isDepotRun(x) { Text("Depot run").font(.inter(12.5)).foregroundStyle(Theme.muted).lineLimit(1).minimumScaleFactor(0.8) } else { LineBadge(line: x.line) }
                    }
                    .frame(width: 58, alignment: .leading)
                    text(x.terminus, tr("From {stop} - {time}", ["stop": x.from, "time": hhmm(x.arrival)]) + " - \(tripTime(x))")
                }
            }
        }
    }

    @ViewBuilder
    private var entries: some View {
        let c = store.choice
        pick(c.entry < 0, { store.act("entry", ["entry": -1]); close() }) {
            text(String(localized: "Automatic"), c.free ? String(localized: "Automatic (the map's first)") : String(localized: "Automatic (nearest to the first stop)"))
        }
        ForEach(store.map?.entry_points ?? [], id: \.index) { e in
            pick(c.entry == e.index, { store.act("entry", ["entry": e.index]); close() }) { text(entryName(e)) }
        }
    }

    @ViewBuilder
    private var weathers: some View {
        let w = store.choice.weather
        pick(w.isEmpty, { store.act("weather", ["weather": ""]); close() }) { text(String(localized: "Natural weather"), String(localized: "Develops by itself through the day and the season")) }
        let custom = WeatherChoice.isCustom(w)
        let metar = WeatherChoice.metar(store.choice)
        pick(custom, { editing = custom ? w : "custom:" }) { text(String(localized: "Custom weather"), String(localized: "Set visibility, wind, clouds, rain, temperature and road state")) }
        if !metar.isEmpty || w.hasPrefix("metar:") {
            pick(w.hasPrefix("metar:"), { store.act("weather", ["weather": "metar:\(metar)"]) }) { text(String(localized: "Current weather"), WeatherChoice.metarText(metar)) }
        }
        if w.hasPrefix("metar:") {
            Field("Airport") { AirportField(code: metar) }.padding(.horizontal, 14).padding(.vertical, 4)
        }
        pick(w == "cycle", { store.act("weather", ["weather": "cycle"]); close() }) { text(String(localized: "Weather cycle"), String(localized: "Changes every 25-60 minutes, as the month allows")) }
        pick(w == "tomorrow", { store.act("weather", ["weather": "tomorrow"]); close() }) { text("Tomorrow.io", String(localized: "Current weather")) }
        if WeatherChoice.editable(w), !custom {
            Button { editing = w } label: { Label("Edit selected weather as custom", systemImage: "slider.horizontal.3") }
                .buttonStyle(.launcher(.ghost)).frame(maxWidth: .infinity, alignment: .leading)
        }
        ForEach(store.lists.weathers) { x in
            pick(w == x.file, { store.act("weather", ["weather": x.file]); close() }) {
                text(x.name, "\(Int(x.temp.rounded())) °C - \(precipText(x.precip)) - \(x.fog_m >= 10000 ? String(localized: "clear air") : "\(Int(x.fog_m)) m")")
            }
        }
    }
}

/// The route map beside a sheet's list (the trips, the start places).
struct RouteMapPanel: View {
    var body: some View {
        MapCanvas().frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

/// The time and the date by steps (a finger needs no keyboard), and the season.
struct PhoneTimeSheet: View {
    @Environment(LauncherStore.self) private var store
    @State private var picking = false

    var body: some View {
        let c = store.choice
        let rt = store.snapshot.real_time || store.flag("use_real_time")
        let rd = store.flag("use_real_date")
        VStack(spacing: 14) {
            dial(["-1 h": -60, "-5": -5], ["+5": 5, "+1 h": 60], value: c.hhmm, disabled: rt) { m in
                store.act("time", ["minutes": (c.time + m + 1440) % 1440])
            }
            dial(["-7": -7, "-1": -1], ["+1": 1, "+7": 7], value: store.dateTitle, disabled: rd) { d in
                store.act("date", ["date": shifted(c.date, d)])
            }
            PhoneSegmented(items: [("auto", "By date"), ("spring", "Spring"), ("summer", "Summer"), ("autumn", "Autumn"), ("winter", "Winter")], selection: c.season) { store.act("season", ["season": $0]) }
            if c.season != "auto" {
                PhoneSegmented(items: [("early", "Early"), ("mid", "Mid"), ("late", "Late")], selection: c.phase) { store.act("phase", ["phase": $0]) }
            }
            HStack(spacing: 8) {
                Button { store.act("now", ["time": true]) } label: { Label("Current time", systemImage: "clock") }.buttonStyle(.launcher(.ghost)).disabled(rt)
                Button { store.act("now", ["time": false, "date": true]) } label: { Label("Today", systemImage: "calendar") }.buttonStyle(.launcher(.ghost)).disabled(rd)
                Button { picking = true } label: { Label("Choose a date", systemImage: "calendar.badge.plus") }.buttonStyle(.launcher(.ghost)).disabled(rd)
            }
            .sheet(isPresented: $picking) {
                Scroll { DateChooser { picking = false } }.background(Theme.surface.ignoresSafeArea()).launcherSheet(store)
            }
            if rt || rd {
                Text("Follows the clock (Settings)").font(.inter(13)).foregroundStyle(Theme.muted)
            }
        }
        .frame(maxWidth: 620)
        .frame(maxWidth: .infinity)
    }

    /// Two step buttons on each side of the value.
    private func dial(_ left: KeyValuePairs<String, Int>, _ right: KeyValuePairs<String, Int>, value: String, disabled: Bool, step: @escaping (Int) -> Void) -> some View {
        HStack(spacing: 8) {
            ForEach(Array(left.enumerated()), id: \.offset) { _, p in stepButton(p.key, disabled) { step(p.value) } }
            Text(value).font(.inter(22, .bold).monospacedDigit()).lineLimit(1).minimumScaleFactor(0.7).frame(maxWidth: .infinity)
            ForEach(Array(right.enumerated()), id: \.offset) { _, p in stepButton(p.key, disabled) { step(p.value) } }
        }
        .padding(10)
        .background(Theme.surface, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
    }

    private func stepButton(_ label: String, _ disabled: Bool, _ action: @escaping () -> Void) -> some View {
        Button { Haptics.tap(); action() } label: {
            Text(label).font(.inter(15, .semibold).monospacedDigit()).foregroundStyle(Theme.text)
                .frame(minWidth: 56, minHeight: 44)
                .background(Theme.surface3, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                .opacity(disabled ? 0.4 : 1)
        }
        .buttonStyle(.plain)
        .disabled(disabled)
    }

    private func shifted(_ date: String, _ days: Int) -> String {
        let f = DateFormatter()
        f.dateFormat = "yyyy-MM-dd"
        f.locale = Locale(identifier: "en_US_POSIX")
        f.timeZone = TimeZone(identifier: "UTC")
        guard let d = f.date(from: date) else { return date }
        return f.string(from: d.addingTimeInterval(Double(days) * 86400))
    }
}

// --- More ------------------------------------------------------------------------------------

/// More: grouped rows to the pages the rail has no room for; a page opens with Back.
struct PhoneMore: View {
    @Environment(LauncherStore.self) private var store

    var body: some View {
        if let p = store.phoneMore {
            VStack(spacing: 0) {
                HStack(spacing: 6) {
                    Button { store.phoneMore = nil } label: {
                        Image(systemName: "chevron.left").font(.system(size: 20, weight: .semibold)).foregroundStyle(Theme.text)
                            .frame(width: 44, height: 44).contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    Text("More").font(.inter(15)).foregroundStyle(Theme.muted)
                    Spacer()
                }
                .padding(.horizontal, 8)
                PageView(page: p)
            }
        } else {
            let running = store.snapshot.instances.filter { $0.running != false }.count
            Scroll {
                VStack(alignment: .leading, spacing: 12) {
                    Text("More").font(.inter(22, .bold)).padding(.horizontal, 4).padding(.bottom, 2)
                    DriverChip { store.phoneTab = "profile" }
                    PhoneGroup {
                        PhoneRow(icon: Page.games.icon, label: "Running games", value: running > 0 ? "\(running)" : "") { store.phoneMore = .games }
                        PhoneRow(icon: Page.tutorials.icon, label: "Tutorials", divider: true) { store.phoneMore = .tutorials }
                        PhoneRow(icon: Page.setup.icon, label: "Setup", value: store.snapshot.root_ok ? "" : String(localized: "OMSI 2 not found"), divider: true) { store.phoneMore = .setup }
                    }
                    PhoneGroup {
                        PhoneRow(icon: Page.depot.icon, label: "Depot") { store.phoneMore = .depot }
                        PhoneRow(icon: Page.timetable.icon, label: "Timetable", divider: true) { store.phoneMore = .timetable }
                        PhoneRow(icon: Page.controls.icon, label: "Controls", divider: true) { store.phoneMore = .controls }
                        PhoneRow(icon: Page.profile.icon, label: "Profile", value: store.snapshot.config.profile, divider: true) { store.phoneTab = "profile" }
                    }
                    Text("openOMSI \(store.version)").font(.inter(12)).foregroundStyle(Theme.muted).padding(.horizontal, 4).padding(.top, 4)
                }
                .frame(maxWidth: 640, alignment: .leading)
                .padding(.horizontal, 16).padding(.top, 14).padding(.bottom, 24)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
    }
}
