import Foundation
import Observation
import SwiftUI

/// The launcher's pages, in the desktop launcher's order (`launcher/ui/js/app.js`): the
/// main ones, then after a gap the running games, the tutorials and the setup.
enum Page: String, CaseIterable, Identifiable {
    case drive, depot, multiplayer, profile, mods, timetable, settings, controls, games, tutorials, setup
    var id: String { rawValue }

    static let main: [Page] = [.drive, .depot, .multiplayer, .profile, .mods, .timetable, .settings, .controls]
    static let more: [Page] = [.games, .tutorials, .setup]

    var title: LocalizedStringKey {
        switch self {
        case .drive: "Drive"
        case .depot: "Depot"
        case .multiplayer: "Multiplayer"
        case .profile: "Profile"
        case .mods: "Mods"
        case .timetable: "Timetable"
        case .settings: "Settings"
        case .controls: "Controls"
        case .games: "Running games"
        case .tutorials: "Tutorials"
        case .setup: "Setup"
        }
    }

    /// SF Symbols (filled) in place of the Material Symbols the desktop launcher draws.
    var icon: String {
        switch self {
        case .drive: "bus.fill"
        case .depot: "door.garage.closed"
        case .multiplayer: "person.2.fill"
        case .profile: "person.fill"
        case .mods: "puzzlepiece.extension.fill"
        case .timetable: "clock.fill"
        case .settings: "slider.horizontal.3"
        case .controls: "keyboard.fill"
        case .games: "gamecontroller.fill"
        case .tutorials: "book.fill"
        case .setup: "folder.fill"
        }
    }
}

/// The pickers opened from the Drive page's cards.
enum DrivePicker: String, Identifiable {
    case map, bus
    var id: String { rawValue }
}

/// What the launcher knows (the game's launcher state, through the bridge) and what the
/// pages have open. Ticked four times a second; the lists and lines are fetched again only
/// when the bridge says they changed.
@MainActor
@Observable
final class LauncherStore {
    var snapshot = Snapshot()
    var lists = Lists()
    var lines: [LineInfo] = []
    var overview: MapOverview?
    var overviewLoading = false
    var tutorials: [Tutorial] = []
    var version = ""
    var build = ""
    var platform = ""
    var inProcess = false
    var storeBuild = false
    var page: Page = .drive
    /// The phone launcher's tab (drive, multiplayer, profile, mods, settings, more), the page
    /// opened from More, and the full-screen sheet open over Drive.
    var phoneTab = "drive"
    var phoneMore: Page?
    var phoneSheet: PhoneSheet?
    /// The map or bus picker open over the Drive page.
    var picker: DrivePicker?
    /// A message of the Swift side itself (an action that failed), shown like the status.
    var notice: (text: String, error: Bool, at: Date)?
    /// Start was pressed: the game is on its way (an iPhone: until its window is in front).
    var starting = false
    var ready = false
    var onboarding = false
    var onboardingStep = 0

    private var timer: Timer?
    private var lastTick = Date()
    private var overviewKey = ""

    init() {}

    func open() {
        guard !ready else { return }
        do {
            let info = try JSONSerialization.jsonObject(with: Bridge.call("init")) as? [String: Any] ?? [:]
            version = info["version"] as? String ?? ""
            build = info["build"] as? String ?? ""
            platform = info["platform"] as? String ?? ""
            inProcess = info["in_process"] as? Bool ?? false
            storeBuild = info["store"] as? Bool ?? false
            ready = true
        } catch {
            say(error.localizedDescription, error: true)
        }
        tick()
        timer = Timer.scheduledTimer(withTimeInterval: 0.25, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated { self?.tick() }
        }
        if snapshot.root_ok == false { page = .setup }
        #if os(iOS)
        onboarding = ready && !snapshot.root_ok && snapshot.config.root.isEmpty
        #endif
        debugScript()
    }

    /// Looking at the launcher without a person at it (the Simulator, the CI):
    /// `OMSI_APPLE_PAGE=settings`, `games` (on a phone under
    /// More), `tab:more`, `picker:bus`, `sheet:trip` (a phone's sheet); `OMSI_APPLE_START=<seconds>`
    /// presses Start then.
    private func debugScript() {
        let env = ProcessInfo.processInfo.environment
        if let p = env["OMSI_APPLE_PAGE"] {
            let parts = p.split(separator: ":").map(String.init)
            if parts.first != "onboarding" { onboarding = false }
            switch parts.first {
            case "tab": phoneTab = parts.count > 1 ? parts[1] : "drive"
            case "picker": picker = parts.count > 1 ? DrivePicker(rawValue: parts[1]) : nil
            case "sheet": phoneSheet = parts.count > 1 ? PhoneSheet(rawValue: parts[1]) : nil
            case "onboarding":
                onboarding = true
                onboardingStep = parts.count > 1 ? Int(parts[1]) ?? 0 : 0
            default:
                if let pg = Page(rawValue: parts[0]) {
                    page = pg
                    let tabs = ["drive", "multiplayer", "profile", "mods", "settings"]
                    phoneTab = parts.count > 1 ? parts[1] : (tabs.contains(pg.rawValue) ? pg.rawValue : "more")
                    if phoneTab == "more" { phoneMore = pg }
                }
            }
        }
        if let s = env["OMSI_APPLE_START"], let t = Double(s) {
            Task { @MainActor in
                try? await Task.sleep(nanoseconds: UInt64(t * 1e9))
                self.start()
            }
        }
    }

    func tick() {
        guard ready else { return }
        let dt = min(1, Date().timeIntervalSince(lastTick))
        lastTick = Date()
        guard let s = try? Bridge.get(Snapshot.self, "tick", ["dt": dt]) else { return }
        let listsChanged = s.lists_rev != snapshot.lists_rev || lists.rev == 0
        let linesChanged = s.lines_rev != snapshot.lines_rev
        snapshot = s
        if listsChanged, let l = try? Bridge.get(Lists.self, "lists") {
            lists = l
        }
        if linesChanged || (lines.isEmpty && s.counts.lines > 0), let l = try? Bridge.get(LinesReply.self, "lines") {
            lines = l.lines
        }
        if !busOffered, let v = offeredVehicles.first { Bridge.act("select_bus", ["file": v.file]) }
        if starting && !s.in_game && s.status.error { starting = false }
        refreshOverviewIfNeeded()
    }

    // --- what the pages read -------------------------------------------------------------

    var choice: Choice { snapshot.choice }
    var bus: VehicleInfo? { lists.vehicles.first { $0.file == choice.bus } }
    var map: MapInfo? { lists.maps.first { $0.file == choice.map } }
    var line: LineInfo? { lines.first { $0.name == choice.line } }
    var tour: TourInfo? { line?.tours.first { $0.number == choice.tour } }
    var firstTrip: TripInfo? {
        guard let t = tour else { return nil }
        let i = snapshot.first_trip ?? 0
        return t.trips.indices.contains(i) ? t.trips[i] : t.trips.first
    }
    var weather: WeatherInfo? { lists.weathers.first { $0.file == choice.weather } }

    func setting(_ key: String) -> JSON? { snapshot.settings[key] }
    func flag(_ key: String, _ fallback: Bool = false) -> Bool { snapshot.settings[key]?.bool ?? fallback }

    var status: (text: String, error: Bool)? {
        if let n = notice, Date().timeIntervalSince(n.at) < 8 { return (n.text, n.error) }
        let s = snapshot.status
        // (a message shows a few seconds, as the desktop launcher's toasts; an error stays)
        return s.text.isEmpty || (!s.error && s.age > 6) ? nil : (s.text, s.error)
    }

    var busTitle: String {
        guard let b = bus else { return choice.bus.isEmpty ? "No bus chosen" : choice.bus }
        let paint = choice.paint.isEmpty ? (b.default_paint?.isEmpty == false ? b.default_paint! : String(localized: "Default paint")) : choice.paint
        return "\(b.name) | \(paint)"
    }

    var dutyTitle: String {
        if choice.free { return String(localized: "Free drive") }
        guard let l = choice.line else { return String(localized: "Choose a line") }
        if let t = choice.tour { return tr("Line {line}, tour {tour}", ["line": l, "tour": "\(t)"]) }
        return tr("Line {name}", ["name": l])
    }

    var weatherTitle: String {
        let w = choice.weather
        if w.isEmpty { return weather?.name ?? String(localized: "Natural weather") }
        if w == "cycle" { return String(localized: "Weather cycle") }
        if w == "tomorrow" { return "Tomorrow.io" }
        if w.hasPrefix("metar:") { return "METAR \(w.dropFirst(6))" }
        if w.hasPrefix("custom:") { return String(localized: "Custom weather") }
        return weather?.name ?? (w as NSString).lastPathComponent.replacingOccurrences(of: ".owt", with: "")
    }

    var dateTitle: String {
        let f = DateFormatter()
        f.dateFormat = "yyyy-MM-dd"
        guard let d = f.date(from: choice.date) else { return choice.date }
        f.dateStyle = .medium
        return f.string(from: d)
    }

    /// The trip a start takes: the one picked, else the one under way or the next at the
    /// start time (as the desktop launcher's `tripAt`).
    var startTrip: TripInfo? {
        guard let t = tour, !choice.free else { return nil }
        if let p = snapshot.picked_trip, let x = t.trips.first(where: { $0.index == p }) { return x }
        return firstTrip
    }

    /// The map's picture (`picture.jpg` beside its global.cfg), in the OMSI 2 folder or the
    /// content folder.
    func mapPicture(_ file: String) -> URL? {
        if let hit = pictureCache[file] { return hit }
        let dir = (file.replacingOccurrences(of: "\\", with: "/") as NSString).deletingLastPathComponent
        var bases = [snapshot.config.root]
        if let c = snapshot.content_dir { bases.append(c) }
        if let c = snapshot.mods?.content_dir { bases.append(c) }
        var found: URL?
        for b in bases where !b.isEmpty {
            for name in ["picture.jpg", "Picture.jpg", "picture.JPG"] {
                let u = URL(fileURLWithPath: b).appendingPathComponent(dir).appendingPathComponent(name)
                if FileManager.default.fileExists(atPath: u.path) { found = u; break }
            }
            if found != nil { break }
        }
        pictureCache[file] = .some(found)
        return found
    }

    @ObservationIgnored private var pictureCache: [String: URL?] = [:]

    // --- actions ---------------------------------------------------------------------------

    func act(_ what: String, _ arg: [String: Any] = [:]) {
        if let e = Bridge.act(what, arg) { say(e, error: true) }
        tick()
    }

    func set(_ key: String, _ value: Any) {
        act("setting", ["key": key, "value": value])
    }

    func say(_ text: String, error: Bool) {
        notice = (text, error, Date())
    }

    func start() {
        starting = true
        act("launch")
        Haptics.tap()
    }

    func continueLast(pick: Int) {
        starting = true
        act("launch_last", ["pick": pick])
    }

    func tutorial(_ n: Int) {
        starting = true
        act("launch_tutorial", ["n": n])
    }

    func loadTutorials() {
        if tutorials.isEmpty, let t = try? Bridge.get([Tutorial].self, "tutorials") { tutorials = t }
    }

    /// The map of the duty chosen, read again when the map, the date or the trip changed.
    func refreshOverviewIfNeeded() {
        let key = "\(choice.map)|\(choice.date)|\(firstTrip?.name ?? "")"
        guard key != overviewKey, !choice.map.isEmpty, !overviewLoading, !snapshot.loading_content else { return }
        overviewKey = key
        overviewLoading = true
        Task.detached(priority: .utility) {
            let o = try? Bridge.get(MapOverview.self, "map_overview")
            await MainActor.run {
                self.overview = o
                self.overviewLoading = false
            }
        }
    }

    func closeOnboarding() {
        onboarding = false
        onboardingStep = 0
    }

    enum GameLook {
        case found, waiting, missing([String]), failed(String)
    }

    nonisolated static func findGame(at path: String?) -> GameLook {
        do {
            let data = try Bridge.call("find_root", path.map { ["path": $0] } ?? [:])
            let r = try JSONSerialization.jsonObject(with: data) as? [String: Any] ?? [:]
            if r["found"] as? Bool == true { return .found }
            guard let m = r["missing"] as? [String], !m.isEmpty else { return .waiting }
            return .missing(m)
        } catch {
            return .failed(error.localizedDescription)
        }
    }

    func lookForGame(at path: String? = nil) async -> GameLook {
        let r = await Task.detached(priority: .utility) { LauncherStore.findGame(at: path) }.value
        switch r {
        case .found: tick()
        case .failed(let e): say(e, error: true)
        default: break
        }
        return r
    }

    /// The game ended (an iPhone, an iPad): the launcher is in front again.
    func gameEnded() {
        starting = false
        tick()
    }
}
