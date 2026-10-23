import SwiftUI

func tripAt(_ tour: TourInfo, _ time: Double) -> Int {
    var now = time
    let n = tour.trips.count
    if n > 0 && now < 86400 && tour.trips[0].departure > now && tour.trips[n - 1].departure >= now + 86400 - 120 { now += 86400 }
    if let i = tour.trips.firstIndex(where: { $0.departure >= now - 120 }) { return i }
    return n - 1
}

struct TourHit {
    var tour: TourInfo
    var ended: Bool
    var at: Int
    var via: String?
    var trip: TripInfo? { tour.trips.indices.contains(at) ? tour.trips[at] : nil }
}

func tourTrip(_ tour: TourInfo, _ now: Double, _ q: String) -> TourHit {
    let ended = tour.runs && tour.trips.allSatisfy { $0.departure < now - 120 }
    let first = tour.trips.isEmpty ? -1 : tripAt(tour, now)
    let has = { (s: String) in s.lowercased().contains(q) }
    if q.isEmpty || has(tour.number) { return TourHit(tour: tour, ended: ended, at: first) }
    let from = ended ? 0 : max(first, 0)
    let at = tour.trips.indices.first { i in
        let x = tour.trips[i]
        return i >= from && (has(x.name) || has(x.line) || has(x.from) || has(x.terminus) || x.stops.contains { has($0.name) })
    } ?? -1
    var via: String?
    if at >= 0 {
        let hit = tour.trips[at]
        if !has(hit.line) && !has(hit.from) && !has(hit.terminus) { via = hit.stops.first { has($0.name) }?.name }
    }
    return TourHit(tour: tour, ended: ended, at: at, via: via)
}

func findTours(_ line: LineInfo?, now: Double, q: String, showEnded: Bool, keep: String?) -> (shown: [TourHit], ended: Int) {
    let all = (line?.tours ?? []).map { tourTrip($0, now, q) }.filter { $0.at >= 0 || ($0.tour.trips.isEmpty && (q.isEmpty || $0.tour.number.lowercased().contains(q))) }
    let dep = { (x: TourHit) in x.trip?.departure ?? .infinity }
    let shown = all
        .filter { showEnded || !$0.ended || (q.isEmpty && $0.tour.number == keep) }
        .sorted { a, b in
            if a.tour.runs != b.tour.runs { return a.tour.runs }
            if a.ended != b.ended { return !a.ended }
            if !q.isEmpty && dep(a) != dep(b) { return dep(a) < dep(b) }
            return a.tour.number.localizedStandardCompare(b.tour.number) == .orderedAscending
        }
    return (shown, all.filter(\.ended).count)
}

func tourRoute(_ tour: TourInfo) -> String {
    let service = tour.trips.filter { !isDepotRun($0) }
    let from = (service.first ?? tour.trips.first)?.from ?? ""
    let to = (service.last ?? tour.trips.last)?.terminus ?? ""
    return from.isEmpty && to.isEmpty ? "" : "\(from) → \(to)"
}

func tripLength(_ tour: TourInfo) -> String {
    let runs = tour.trips.filter { !isDepotRun($0) }.map { $0.arrival - $0.departure }.sorted()
    guard !runs.isEmpty else { return "" }
    let m = max(1, Int((runs[runs.count / 2] / 60).rounded()))
    return Duration.seconds(m * 60).formatted(.units(allowed: [.hours, .minutes], width: .abbreviated))
}

struct DutyPickRow<Content: View>: View {
    var on: Bool
    var dim = false
    var big = false
    var action: () -> Void
    @ViewBuilder var content: Content

    var body: some View {
        Button { Haptics.tap(); action() } label: {
            HStack(spacing: 10) {
                content
                if big && on { Image(systemName: "checkmark").font(.system(size: 15, weight: .bold)).foregroundStyle(Theme.accent) }
            }
            .padding(.horizontal, big ? 14 : 10).padding(.vertical, big ? 8 : 7)
            .frame(maxWidth: .infinity, minHeight: big ? 56 : 0, alignment: .leading)
            .background(on ? Theme.accentSoft : (big ? Theme.surface : .clear), in: RoundedRectangle(cornerRadius: big ? 12 : Theme.radiusSm, style: .continuous))
            .opacity(dim ? 0.6 : 1)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

private struct PickList<Content: View>: View {
    var big: Bool
    var chosen: String?
    @ViewBuilder var content: Content

    var body: some View {
        if big {
            VStack(spacing: 6) { content }
        } else {
            ScrollViewReader { proxy in
                Scroll {
                    LazyVStack(spacing: 2) { content }.padding(4)
                }
                .frame(maxHeight: 250)
                .fieldBackground()
                .clipShape(RoundedRectangle(cornerRadius: Theme.radiusSm, style: .continuous))
                .onAppear { if let chosen { proxy.scrollTo(chosen, anchor: .center) } }
            }
        }
    }
}

struct LineList: View {
    @Environment(LauncherStore.self) private var store
    var lines: [LineInfo]
    var big = false
    var onPick: () -> Void = {}
    @State private var q = ""

    var body: some View {
        let lq = q.trimmingCharacters(in: .whitespaces).lowercased()
        let found = lines.filter { lq.isEmpty || "\($0.name) \($0.termini.joined(separator: " "))".lowercased().contains(lq) }
        VStack(alignment: .leading, spacing: 8) {
            if !big {
                HStack {
                    Text("Line").font(.inter(12.5, .medium)).foregroundStyle(Theme.text2)
                    Spacer()
                    Text(lq.isEmpty ? "\(lines.count)" : "\(found.count) / \(lines.count)").font(.inter(12.5).monospacedDigit()).foregroundStyle(Theme.muted)
                }
            }
            if lines.count > 8 { SearchField(prompt: "Filter lines...", text: $q) }
            if found.isEmpty { Text("No line matches.").font(.inter(13)).foregroundStyle(Theme.muted).padding(8) }
            PickList(big: big, chosen: store.choice.line) {
                ForEach(found) { l in
                    DutyPickRow(on: l.name == store.choice.line, big: big, action: { choose(l) }) {
                        LineBadge(line: l.name)
                        Text(l.termini.joined(separator: " - ")).font(.inter(big ? 14 : 13.5)).foregroundStyle(Theme.text2).lineLimit(1)
                            .frame(maxWidth: .infinity, alignment: .leading)
                        HStack(spacing: 3) {
                            Image(systemName: "calendar").font(.system(size: 11))
                            Text("\(l.tours.filter(\.runs).count)").monospacedDigit()
                        }
                        .font(.inter(12.5)).foregroundStyle(Theme.muted)
                        .help("Tours of this line on the chosen day")
                    }
                    .id(l.name)
                }
            }
        }
        .onChange(of: store.choice.map) { q = "" }
    }

    private func choose(_ l: LineInfo) {
        if l.name != store.choice.line {
            store.act("line", ["line": l.name])
            let now = Double(store.choice.time * 60)
            if let t = findTours(l, now: now, q: "", showEnded: false, keep: nil).shown.first?.tour ?? l.tours.first(where: \.runs) ?? l.tours.first, t.runs {
                store.act("tour", ["tour": t.number])
            }
        }
        onPick()
    }
}

struct TourList: View {
    @Environment(LauncherStore.self) private var store
    var line: LineInfo
    var big = false
    var onPick: () -> Void = {}
    @State private var q = ""
    @State private var showEnded = false

    var body: some View {
        let tq = q.trimmingCharacters(in: .whitespaces).lowercased()
        let r = findTours(line, now: Double(store.choice.time * 60), q: tq, showEnded: showEnded, keep: store.choice.tour)
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                if !big { Text("Tour").font(.inter(12.5, .medium)).foregroundStyle(Theme.text2) }
                Spacer()
                if r.ended > 0 {
                    ToggleRow(label: LocalizedStringKey(tr("Show ended tours ({n})", ["n": "\(r.ended)"])), isOn: showEnded) { showEnded = $0 }
                        .fixedSize()
                }
            }
            SearchField(prompt: "Filter tours: number, route, stop...", text: $q)
            if r.shown.isEmpty {
                Text(LocalizedStringKey(tq.isEmpty ? "No tour left at this time: show the ended tours, or start earlier." : "No tour has a trip still to come that matches."))
                    .font(.inter(13)).foregroundStyle(Theme.muted).padding(8).fixedSize(horizontal: false, vertical: true)
            }
            PickList(big: big, chosen: store.choice.tour) {
                ForEach(r.shown, id: \.tour.number) { h in row(h, tq).id(h.tour.number) }
            }
        }
        .onChange(of: store.choice.line) { q = "" }
    }

    private func row(_ h: TourHit, _ tq: String) -> some View {
        let x = h.tour
        let trip = h.trip
        let live = x.runs && !h.ended
        let hit = !tq.isEmpty && trip != nil && !x.number.lowercased().contains(tq)
        let days = !x.runs ? (x.next_run.map { tr("runs {date}", ["date": longDate($0)]) } ?? String(localized: "never within a year")) : h.ended ? String(localized: "ended") : x.days
        let length = tripLength(x)
        var sub = tourRoute(x)
        if hit, let trip {
            sub = "\(trip.line.isEmpty ? "" : "\(trip.line) | ")\(trip.from) → \(trip.terminus)\(h.via.map { " | \(tr("via {stop}", ["stop": $0]))" } ?? "")"
        }
        let facts = "\(tr("{n} trips", ["n": "\(x.trips.count)"])) - \(days)\(length.isEmpty ? "" : " - \(tr("{time} per trip", ["time": length]))")"
        return DutyPickRow(on: x.number == store.choice.tour, dim: !live, big: big, action: { choose(x, hit ? trip : nil) }) {
            VStack(alignment: .leading, spacing: 2) {
                HStack {
                    Text(x.number).font(.inter(14.5, .semibold)).foregroundStyle(Theme.text).lineLimit(1)
                    Spacer(minLength: 6)
                    if let trip {
                        Text("\(hhmm(trip.departure)) - \(hhmm(trip.arrival))").font(.inter(12.5, .semibold).monospacedDigit())
                            .foregroundStyle(live ? Theme.accent : Theme.muted)
                    }
                }
                if !sub.isEmpty { Text(sub).font(.inter(12.5)).foregroundStyle(Theme.text2).lineLimit(1) }
                Text(facts).font(.inter(12)).foregroundStyle(Theme.muted).lineLimit(1)
            }
        }
    }

    private func choose(_ x: TourInfo, _ trip: TripInfo?) {
        if x.number != store.choice.tour || trip != nil {
            store.act("tour", ["tour": x.number])
            if let trip { store.act("pick_trip", ["index": trip.index, "departure": trip.departure]) }
        }
        onPick()
    }
}

struct TourBuses: View {
    @Environment(LauncherStore.self) private var store
    var tour: TourInfo
    @State private var found: [Found] = []

    struct Found: Decodable, Hashable { var file: String; var paint: String; var number: String }

    var body: some View {
        let c = store.choice
        let buses = found.compactMap { f -> (v: VehicleInfo, paint: String, number: String)? in
            guard let v = store.offeredVehicles.first(where: { busFile($0.file) == busFile(f.file) }) else { return nil }
            return (v, v.paints.contains(f.paint) ? f.paint : "", (v.numbers ?? []).contains { $0.first == f.number } ? f.number : "")
        }.prefix(10)
        Group {
            if !buses.isEmpty {
                VStack(alignment: .leading, spacing: 6) {
                    Text("Buses the timetable gives this tour:").font(.inter(12.5)).foregroundStyle(Theme.muted)
                    Scroll(.horizontal) {
                        HStack(spacing: 6) {
                            ForEach(Array(buses.enumerated()), id: \.offset) { _, b in
                                let on = b.v.file == c.bus && c.paint == b.paint
                                Button { take(b.v, b.paint, b.number) } label: {
                                    Label([b.v.name, b.paint].filter { !$0.isEmpty }.joined(separator: " - "), systemImage: on ? "checkmark" : "bus.fill").lineLimit(1)
                                }
                                .buttonStyle(.launcher(on ? .primary : .plain, small: true))
                            }
                        }
                    }
                    .scrollIndicators(.never)
                }
            }
        }
        .task(id: "\(c.map)|\(c.date)|\(tour.ai_group)") {
            found = []
            guard !tour.ai_group.isEmpty, !c.map.isEmpty else { return }
            let args: [String: Any] = ["map": c.map, "group": tour.ai_group, "date": c.date]
            let r = await Task.detached(priority: .utility) { try? Bridge.api([Found].self, "tour_buses", args) }.value
            found = r ?? []
        }
    }

    private func take(_ v: VehicleInfo, _ paint: String, _ number: String) {
        if v.file != store.choice.bus { store.act("select_bus", ["file": v.file]) }
        store.act("paint", ["paint": paint])
        store.act("number", ["number": number])
        if store.choice.hof_manual { store.act("hof", ["hof": ""]) }
    }
}

struct NextDepartures: View {
    @Environment(LauncherStore.self) private var store
    var lines: [LineInfo]
    var big = false
    var onPick: () -> Void = {}
    @State private var open = false

    var body: some View {
        let c = store.choice
        let now = Double(c.time * 60)
        let upcoming = lines.flatMap { l in
            l.tours.compactMap { x -> (l: LineInfo, x: TourInfo, trip: TripInfo)? in
                guard x.runs else { return nil }
                let i = tripAt(x, now)
                guard x.trips.indices.contains(i) else { return nil }
                let trip = x.trips[i]
                return trip.departure >= now - 120 && !isDepotRun(trip) ? (l, x, trip) : nil
            }
        }
        .sorted { $0.trip.departure < $1.trip.departure }
        .prefix(15)
        if !upcoming.isEmpty {
            DisclosureGroup(isExpanded: $open) {
                VStack(spacing: big ? 6 : 2) {
                    ForEach(Array(upcoming.enumerated()), id: \.offset) { _, u in
                        DutyPickRow(on: u.l.name == c.line && u.x.number == c.tour, big: big, action: { choose(u.l, u.x, u.trip) }) {
                            Text(hhmm(u.trip.departure)).font(.inter(14, .bold).monospacedDigit()).frame(width: 48, alignment: .leading)
                            LineBadge(line: u.trip.line.isEmpty ? u.l.name : u.trip.line)
                            VStack(alignment: .leading, spacing: 1) {
                                Text("\(u.trip.from) → \(u.trip.terminus)").font(.inter(13.5)).foregroundStyle(Theme.text).lineLimit(1)
                                Text(tr("Tour {n}", ["n": "\(u.x.number)"])).font(.inter(12)).foregroundStyle(Theme.muted).lineLimit(1)
                            }
                            .frame(maxWidth: .infinity, alignment: .leading)
                        }
                    }
                }
                .padding(.top, 8)
            } label: {
                Text(tr("Next departures from {time}", ["time": c.hhmm])).font(.inter(12.5, .medium)).foregroundStyle(Theme.text2)
            }
            .tint(Theme.muted)
        }
    }

    private func choose(_ l: LineInfo, _ x: TourInfo, _ trip: TripInfo) {
        if l.name != store.choice.line { store.act("line", ["line": l.name]) }
        store.act("tour", ["tour": x.number])
        store.act("pick_trip", ["index": trip.index, "departure": trip.departure])
        onPick()
    }
}
