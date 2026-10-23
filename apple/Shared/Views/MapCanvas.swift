import SwiftUI

/// The route map, drawn as the desktop and phone launchers draw it (`launcher/ui/js/routemap.js`):
/// the map's roads as the game's own city map draws them - every road's casing first, then
/// every road's surface, at the road's own width in metres but never thinner than 2.4 / 1.4
/// points, main roads brighter, in the city map's colours - the trip's route in the map's red
/// on the very lanes the game drives, its stops on the route with "HH:MM Name" labels, and the
/// entry points in amber, the chosen one ringed white. Dragged to move, pinched (or the wheel)
/// to zoom; a tap on an entry point chooses where the bus starts.
struct MapCanvas: View {
    @Environment(LauncherStore.self) private var store
    @State private var center: CGPoint?
    @State private var mpp: Double?
    @State private var dragFrom: CGPoint?
    @State private var zoomFrom: Double?
    @State private var fittedFor = ""
    @State private var snapped: (key: String, stops: [CGPoint])?

    var body: some View {
        GeometryReader { geo in
            ZStack(alignment: .bottomLeading) {
                Color(hex: 0x0E0E0E)
                if let o = store.overview {
                    let (c, m) = view(o, geo.size)
                    let stops = stopPoints(o)
                    Canvas { ctx, size in
                        draw(o, stops: stops, ctx: ctx, size: size, center: c, mpp: m)
                    }
                    .contentShape(Rectangle())
                    .gesture(pan(c, m))
                    .simultaneousGesture(zoom(c, m))
                    .onTapGesture(coordinateSpace: .local) { p in tap(o, p, geo.size, c, m) }
                    .onAppear { fit(o) }
                    .onChange(of: o.map + o.trip) { _, _ in fit(o) }
                    Text(tr("{roads} roads | {stops} stops | {entries} entry points", ["roads": "\(o.roads.count)", "stops": "\(o.stops.count)", "entries": "\(o.entries.count)"]))
                        .font(.inter(11.5))
                        .foregroundStyle(Theme.text2)
                        .padding(.horizontal, 8).padding(.vertical, 3)
                        .background(Color(hex: 0x0E0E0E).opacity(0.8), in: RoundedRectangle(cornerRadius: 4, style: .continuous))
                        .padding(.leading, 10).padding(.bottom, 8)
                        .allowsHitTesting(false)
                } else {
                    HStack(spacing: 10) {
                        if store.overviewLoading { ProgressView().controlSize(.small).tint(Theme.accent) }
                        Text(LocalizedStringKey(store.overviewLoading ? "Reading the map…" : "No map chosen")).font(.inter(14)).foregroundStyle(Theme.muted)
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                }
            }
        }
        .clipShape(RoundedRectangle(cornerRadius: Theme.radiusSm, style: .continuous))
    }

    // --- where the map looks -------------------------------------------------------------

    /// The middle (map metres) and metres a point: the trip's own route when there is one,
    /// else the whole map, with room for the labels round it.
    private func view(_ o: MapOverview, _ size: CGSize) -> (CGPoint, Double) {
        if let c = center, let m = mpp { return (c, m) }
        var lo = CGPoint(x: Double.infinity, y: Double.infinity)
        var hi = CGPoint(x: -Double.infinity, y: -Double.infinity)
        for r in o.route {
            var i = 0
            while i + 1 < r.count {
                lo.x = min(lo.x, r[i]); lo.y = min(lo.y, r[i + 1]); hi.x = max(hi.x, r[i]); hi.y = max(hi.y, r[i + 1])
                i += 2
            }
        }
        if !lo.x.isFinite, o.lo.count == 2, o.hi.count == 2 {
            lo = CGPoint(x: o.lo[0], y: o.lo[1]); hi = CGPoint(x: o.hi[0], y: o.hi[1])
        }
        guard lo.x.isFinite else { return (.zero, 1) }
        let m = max((hi.x - lo.x) / max(1, size.width - 80), (hi.y - lo.y) / max(1, size.height - 60), 0.2)
        return (CGPoint(x: (lo.x + hi.x) / 2, y: (lo.y + hi.y) / 2), m)
    }

    private func fit(_ o: MapOverview) {
        let key = o.map + "|" + o.trip
        guard key != fittedFor else { return }
        fittedFor = key
        center = nil
        mpp = nil
    }

    private func screen(_ x: Double, _ y: Double, _ size: CGSize, _ c: CGPoint, _ m: Double) -> CGPoint {
        CGPoint(x: size.width / 2 + (x - c.x) / m, y: size.height / 2 - (y - c.y) / m)
    }

    /// The stop objects stand beside the road; their dots go onto the route itself: the nearest
    /// point of the route's lanes within 40 m (a stop the route misses stays where it is).
    private func stopPoints(_ o: MapOverview) -> [CGPoint] {
        let key = "\(o.map)|\(o.trip)|\(o.stops.count)|\(o.route.count)"
        if let s = snapped, s.key == key { return s.stops }
        let pts = MapCanvas.snap(o)
        DispatchQueue.main.async { snapped = (key, pts) }
        return pts
    }

    static func snap(_ o: MapOverview) -> [CGPoint] {
        o.stops.map { s in
            var best: CGPoint?
            var bd = 40.0 * 40.0
            for p in o.route {
                var i = 0
                while i + 3 < p.count {
                    let (ax, ay, bx, by) = (p[i], p[i + 1], p[i + 2], p[i + 3])
                    let vx = bx - ax, vy = by - ay
                    let l = vx * vx + vy * vy == 0 ? 1 : vx * vx + vy * vy
                    let k = max(0, min(1, ((s.x - ax) * vx + (s.y - ay) * vy) / l))
                    let x = ax + vx * k, y = ay + vy * k
                    let dd = (x - s.x) * (x - s.x) + (y - s.y) * (y - s.y)
                    if dd < bd { bd = dd; best = CGPoint(x: x, y: y) }
                    i += 2
                }
            }
            return best ?? CGPoint(x: s.x, y: s.y)
        }
    }

    // --- drawing ---------------------------------------------------------------------------

    /// The stops' times and names: the trip the map was read for.
    private var trip: TripInfo? {
        if let o = store.overview, let t = store.tour?.trips.first(where: { $0.name == o.trip && $0.index == store.firstTrip?.index }) { return t }
        return store.firstTrip
    }

    private func draw(_ o: MapOverview, stops: [CGPoint], ctx: GraphicsContext, size: CGSize, center c: CGPoint, mpp m: Double) {
        func path(_ p: [Double]) -> Path {
            var path = Path()
            var i = 0
            while i + 1 < p.count {
                let q = screen(p[i], p[i + 1], size, c, m)
                if i == 0 { path.move(to: q) } else { path.addLine(to: q) }
                i += 2
            }
            return path
        }
        let colors = o.palette
        let style = { (w: Double) in StrokeStyle(lineWidth: w, lineCap: .round, lineJoin: .round) }
        // every road's casing first, then every road's surface (a crossing is not cut by a
        // neighbour's casing), as the game's city map draws them
        for pass in 0 ..< 2 {
            for r in o.roads {
                let w = max((r.w + (pass == 1 ? 0 : 2)) / m, pass == 1 ? 1.4 : 2.4)
                ctx.stroke(path(r.p), with: .color(pass == 1 ? (r.main ? colors.main : colors.road) : colors.casing), style: style(w))
            }
        }
        for r in o.route {
            ctx.stroke(path(r), with: .color(colors.route), style: style(4.5))
        }
        // the stops, with their times and names: right of the stop, or left when that would
        // cover a label already written, else none
        let t = trip
        var taken: [CGRect] = []
        for (k, s) in o.stops.enumerated() {
            let at = k < stops.count ? stops[k] : CGPoint(x: s.x, y: s.y)
            let q = screen(at.x, at.y, size, c, m)
            ctx.fill(Path(ellipseIn: CGRect(x: q.x - 3.2, y: q.y - 3.2, width: 6.4, height: 6.4)), with: .color(Color(hex: 0xF0F0F0)))
            guard let t, t.stops.indices.contains(s.i) else { continue }
            let st = t.stops[s.i]
            let label = ctx.resolve((
                Text(hhmm(st.dep > 0 ? st.dep : st.arr)).foregroundStyle(Theme.accent)
                    + Text("  ") + Text(st.name).foregroundStyle(Color(hex: 0xECECEC))
            ).font(.inter(11.5, .semibold)))
            let w = label.measure(in: CGSize(width: 1000, height: 40)).width + 10
            let candidates = [q.x + 8, q.x - 8 - w]
            guard let lx = candidates.first(where: { x in !taken.contains { $0.intersects(CGRect(x: x, y: q.y - 10, width: w, height: 20)) } }) else { continue }
            let box = CGRect(x: lx, y: q.y - 10, width: w, height: 20)
            taken.append(box)
            ctx.fill(Path(roundedRect: box, cornerRadius: 4), with: .color(Color(hex: 0x0E0E0E).opacity(0.86)))
            ctx.draw(label, at: CGPoint(x: lx + 5, y: q.y), anchor: .leading)
        }
        // entry points: amber, the chosen one ringed white
        let chosen = store.choice.entry
        for e in o.entries {
            let q = screen(e.x, e.y, size, c, m)
            ctx.fill(Path(ellipseIn: CGRect(x: q.x - 4, y: q.y - 4, width: 8, height: 8)), with: .color(Theme.accent))
            if e.index == chosen {
                ctx.stroke(Path(ellipseIn: CGRect(x: q.x - 10, y: q.y - 10, width: 20, height: 20)), with: .color(.white), lineWidth: 2)
            }
        }
    }

    // --- the fingers and the mouse -----------------------------------------------------------

    private func pan(_ c: CGPoint, _ m: Double) -> some Gesture {
        DragGesture(minimumDistance: 3)
            .onChanged { v in
                let from = dragFrom ?? c
                if dragFrom == nil { dragFrom = c; mpp = m }
                center = CGPoint(x: from.x - v.translation.width * m, y: from.y + v.translation.height * m)
            }
            .onEnded { _ in dragFrom = nil }
    }

    private func zoom(_ c: CGPoint, _ m: Double) -> some Gesture {
        MagnifyGesture()
            .onChanged { v in
                let from = zoomFrom ?? m
                if zoomFrom == nil { zoomFrom = m; center = c }
                mpp = min(200, max(0.15, from / v.magnification))
            }
            .onEnded { _ in zoomFrom = nil }
    }

    private func tap(_ o: MapOverview, _ p: CGPoint, _ size: CGSize, _ c: CGPoint, _ m: Double) {
        let d = { (e: MapOverview.Entry) in hypot(screen(e.x, e.y, size, c, m).x - p.x, screen(e.x, e.y, size, c, m).y - p.y) }
        if let e = o.entries.min(by: { d($0) < d($1) }), d(e) < 18 {
            store.act("entry", ["entry": e.index])
            store.say(tr("Starting point: {name}", ["name": e.name]), error: false)
        }
    }
}

extension MapOverview {
    /// The city map's colours (casing, road, main road, route) as the data names them, else the
    /// game's own.
    var palette: (casing: Color, road: Color, main: Color, route: Color) {
        let parsed = (colors ?? []).compactMap(Color.init(css:))
        if parsed.count == 4 { return (parsed[0], parsed[1], parsed[2], parsed[3]) }
        return (Color(white: 30 / 255).opacity(0.9), Color(white: 92 / 255), Color(white: 112 / 255), Color(red: 214 / 255, green: 48 / 255, blue: 40 / 255))
    }
}

extension Color {
    /// `rgba(r,g,b,a)`, `rgb(r,g,b)` or `#rrggbb`.
    init?(css: String) {
        let s = css.trimmingCharacters(in: .whitespaces)
        if s.hasPrefix("#"), s.count == 7, let v = UInt32(s.dropFirst(), radix: 16) {
            self.init(hex: v)
            return
        }
        guard let open = s.firstIndex(of: "("), let close = s.lastIndex(of: ")") else { return nil }
        let n = s[s.index(after: open) ..< close].split(separator: ",").compactMap { Double($0.trimmingCharacters(in: .whitespaces)) }
        guard n.count >= 3 else { return nil }
        self.init(.sRGB, red: n[0] / 255, green: n[1] / 255, blue: n[2] / 255, opacity: n.count > 3 ? n[3] : 1)
    }
}
