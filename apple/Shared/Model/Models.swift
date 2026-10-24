import Foundation

// What the launcher's bridge sends, as Swift reads it. The field names are the Rust ones
// (`omsi_launcher_lib`, `launcher::state`), so a reply decodes without a key table; fields a
// newer or older game may not send are optional.

struct Snapshot: Decodable {
    var lists_rev: UInt64 = 0
    var lines_rev: UInt64 = 0
    var status: Status = Status()
    var loading_content = false
    var loading_lines = false
    var counts: Counts = Counts()
    var choice: Choice = Choice()
    var default_hof = ""
    var season = "summer"
    var first_trip: Int?
    var picked_trip: Int?
    var real_time = false
    var config: Config = Config()
    var root_ok = false
    var root_problem = ""
    var content_dir: String?
    var data_dir = ""
    var profiles: [String] = []
    var profile: Profile?
    var settings: [String: JSON] = [:]
    var mods: ModsStatus?
    var jobs: [InstallJob] = []
    var mod_busy: String?
    var mod_info: ModInfoReply?
    var instances: [Instance] = []
    var in_game = false
    var crash: Crash?
    var disconnected: String?
    var servers: [Server] = []
    var joined_server: String?
    var join: Join = Join()
    var saved: [SavedSituation] = []
    var save_pick = 0
    var ibis: IbisReply?
    var cmdline = ""
    var fresh: [String] = []
    /// The bus preview: a bus being read, or why there is no picture.
    var preview: PreviewStatus = PreviewStatus()
    var host_vehicles: [String]?

    struct Status: Decodable { var text = ""; var error = false; var age: Double = 0 }
    struct Counts: Decodable { var maps = 0; var vehicles = 0; var weathers = 0; var lines = 0 }
    struct Config: Decodable { var root = ""; var profile = ""; var game = "" }
    struct Crash: Decodable { var why = ""; var log = "" }
    struct Join: Decodable { var ok = true; var text = "" }
    struct PreviewStatus: Decodable { var busy = false; var error: String?; var view: String?; var views: [String]? }

    init() {}

    // (every field may be missing: a snapshot of an older game decodes with the rest)
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: AnyKey.self)
        func v<T: Decodable>(_ k: String, _ d: T) -> T { (try? c.decodeIfPresent(T.self, forKey: AnyKey(k))) ?? d }
        lists_rev = v("lists_rev", 0); lines_rev = v("lines_rev", 0)
        status = v("status", Status()); loading_content = v("loading_content", false); loading_lines = v("loading_lines", false)
        counts = v("counts", Counts()); choice = v("choice", Choice()); default_hof = v("default_hof", "")
        season = v("season", "summer"); first_trip = v("first_trip", nil as Int?); picked_trip = v("picked_trip", nil as Int?)
        real_time = v("real_time", false); config = v("config", Config()); root_ok = v("root_ok", false)
        root_problem = v("root_problem", ""); content_dir = v("content_dir", nil as String?); data_dir = v("data_dir", ""); profiles = v("profiles", [])
        profile = v("profile", nil as Profile?); settings = v("settings", [:]); mods = v("mods", nil as ModsStatus?)
        jobs = v("jobs", []); mod_busy = v("mod_busy", nil as String?); mod_info = v("mod_info", nil as ModInfoReply?); instances = v("instances", [])
        in_game = v("in_game", false); crash = v("crash", nil as Crash?); disconnected = v("disconnected", nil as String?)
        servers = v("servers", []); joined_server = v("joined_server", nil as String?); join = v("join", Join())
        saved = v("saved", []); save_pick = v("save_pick", 0); ibis = v("ibis", nil as IbisReply?)
        cmdline = v("cmdline", ""); fresh = v("fresh", []); preview = v("preview", PreviewStatus())
        host_vehicles = v("host_vehicles", nil as [String]?)
    }
}

struct AnyKey: CodingKey {
    var stringValue: String
    var intValue: Int? { nil }
    init(_ s: String) { stringValue = s }
    init?(stringValue: String) { self.stringValue = stringValue }
    init?(intValue: Int) { nil }
}

/// The duty chosen (`launcher-duty.json`).
struct Choice: Decodable, Equatable {
    var bus = ""
    var paint = ""
    var plate = ""
    var number = ""
    var hof = ""
    var hof_manual = false
    var map = ""
    var entry = -1
    var line: String?
    var tour: String?
    var free = false
    var time = 540
    var date = "1989-05-30"
    var season = "auto"
    var phase = "mid"
    var weather = ""
    var traffic: Double = 30
    var passengers = true
    var schedule = true
    var comfortable_cabin_on_start = false
    var autostart = false
    var on_foot = false
    var lan_mode = "off"
    var lan_addr = ""

    init() {}

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: AnyKey.self)
        func v<T: Decodable>(_ k: String, _ d: T) -> T { (try? c.decodeIfPresent(T.self, forKey: AnyKey(k))) ?? d }
        bus = v("bus", ""); paint = v("paint", ""); plate = v("plate", ""); number = v("number", "")
        hof = v("hof", ""); hof_manual = v("hof_manual", false); map = v("map", ""); entry = v("entry", -1)
        line = v("line", nil as String?); tour = v("tour", nil as String?); free = v("free", false)
        time = v("time", 540); date = v("date", "1989-05-30"); season = v("season", "auto"); phase = v("phase", "mid")
        weather = v("weather", ""); traffic = v("traffic", 30.0); passengers = v("passengers", true)
        comfortable_cabin_on_start = v("comfortable_cabin_on_start", false)
        schedule = v("schedule", true); autostart = v("autostart", false); on_foot = v("on_foot", false)
        lan_mode = v("lan_mode", "off"); lan_addr = v("lan_addr", "")
    }

    var hhmm: String { String(format: "%02d:%02d", time / 60, time % 60) }
}

struct Lists: Decodable {
    var maps: [MapInfo] = []
    var vehicles: [VehicleInfo] = []
    var weathers: [WeatherInfo] = []
    var rev: UInt64 = 0
}

struct MapInfo: Decodable, Identifiable, Hashable {
    var name: String
    var friendly: String
    var file: String
    var description: String
    var entry_points: [EntryInfo]
    var hof: String
    var installed: Bool?
    var id: String { file }
    var title: String { friendly.isEmpty ? name : friendly }
}

struct EntryInfo: Decodable, Hashable {
    var index: Int
    var name: String
}

struct VehicleInfo: Decodable, Identifiable, Hashable {
    var name: String
    var manufacturer: String
    var type_name: String
    var file: String
    var folder: String
    var description: String
    var default_paint: String?
    var paints: [String]
    var hofs: [String]
    var installed: Bool?
    var missing_packs: [String]?
    var numbers: [[String]]?
    var id: String { file }
}

struct WeatherInfo: Decodable, Identifiable, Hashable {
    var name: String
    var file: String
    var description: String
    var fog_m: Double
    var temp: Double
    var clouds: String
    var precip: String
    var snow: Bool
    var installed: Bool?
    var id: String { file }
}

struct LinesReply: Decodable {
    var lines: [LineInfo]
    var rev: UInt64
}

struct LineInfo: Decodable, Identifiable, Hashable {
    var name: String
    var user_allowed: Bool
    var termini: [String]
    var tours: [TourInfo]
    var id: String { name }
}

struct TourInfo: Decodable, Identifiable, Hashable {
    var id: String
    var number: String
    var ai_group: String
    var first: Double
    var last: Double
    var days: String
    var runs: Bool
    var next_run: String?
    var trips: [TripInfo]
}

struct TripInfo: Decodable, Identifiable, Hashable {
    var name: String
    var index: Int
    var line: String
    var from: String
    var terminus: String
    var departure: Double
    var arrival: Double
    var stops: [StopInfo]
    var km: Double
    var id: Int { index }
}

struct StopInfo: Decodable, Hashable {
    var name: String
    var arr: Double
    var dep: Double
}

struct Profile: Decodable {
    var name: String
    var hours: Double
    var km: Double
    var xp: Int
    var level: Int
    var next_level_xp: Int
    var stops: Int
    var early: Int
    var late: Int
    var tickets: Double
    var cash: Double
    var crashes: Int
    var hurt: Int
    var rating_driving: Double
    var rating_comfort: Double
    var rating_tickets: Double
    var sessions: [Session]
    var exists: Bool
}

struct Session: Decodable, Hashable {
    var time: UInt64
    var driver: String
    var map: String
    var bus: String
    var line: String?
    var tour: String?
    var seconds: Double
    var metres: Double
    var stops: Int
    var tickets: Int
    var cash: Double
    var crashes: Int
}

struct ModsStatus: Decodable {
    var content_dir: String
    var inbox: String
    var inbox_items: [String]
    var waiting: [String]
    var free_bytes: UInt64
    var installed: [ModEntry]
    var archives: [[JSON]]?
    var hof_warnings: [String]?
}

struct ModInfoReply: Decodable {
    var ok: Bool
    var info: ModSourceInfo?
    var error: String?
}

struct ModSourceInfo: Decodable {
    var is_archive: Bool
    var files: UInt64
    var unpacked_bytes: UInt64
    var archive_bytes: UInt64
    var needed_bytes: UInt64
    var free_bytes: UInt64
    var fits: Bool
    var in_place: String
    var in_place_ok: Bool
}

struct ModEntry: Decodable, Identifiable, Hashable {
    var id: String
    var name: String
    var paths: [String]
    var enabled: Bool
    var installed: UInt64?
    var bytes: UInt64?
    var kind: String?
    var noted: Bool?
}

struct InstallJob: Decodable, Identifiable, Hashable {
    var id: UInt64
    var source: String
    var name: String
    var state: String
    var files_done: UInt64
    var files_total: UInt64
    var bytes_done: UInt64
    var bytes_total: UInt64
    var message: String
    var warnings: [String]
    var installed: [String]
    var finished: UInt64?
}

struct Instance: Decodable, Identifiable, Hashable {
    var pid: UInt32
    var log: String
    var started: UInt64
    var map: String
    var bus: String
    var line: String?
    var tour: String?
    var profile: String
    var lan: String
    var running: Bool?
    var ended: UInt64?
    var exit_code: Int?
    var stopping: UInt64?
    var lan_status: JSON?
    var last_line: String?
    var id: UInt32 { pid }
}

struct Server: Decodable, Identifiable, Hashable {
    var name: String
    var address: String
    var info: ServerInfo?
    var official: Bool?
    var `protocol`: String?
    var id: String { address }
}

struct ServerInfo: Decodable, Hashable {
    var ok: Bool
    var name: String?
    var motd: String?
    var map: String?
    var players: Int?
    var max_players: Int?
    var version: String?
    var time: String?
    var weather: String?
    var password: Bool?
    var error: String?
}

struct SavedSituation: Decodable, Hashable {
    var file: String
    var name: String
    var saved: UInt64
}

struct IbisReply: Decodable {
    var ok: Bool
    var info: IbisInfo?
    var error: String?
}

struct IbisInfo: Decodable {
    var hof: String
    var line_code: String
    var routes: [IbisRoute]
    var trip_route: String?
}

struct IbisRoute: Decodable, Hashable {
    var code: String
    var route: String
    var name: String
    var terminus: String
}

/// The Drive page's map: roads, the chosen trip's route, its stops and the entry points, in
/// metres from the map's middle (x east, y north).
struct MapOverview: Decodable {
    var map: String
    var trip: String
    var lo: [Double]
    var hi: [Double]
    var roads: [Road]
    var route: [[Double]]
    var stops: [Point]
    var entries: [Entry]
    var colors: [String]?

    struct Road: Decodable { var w: Double; var main: Bool; var p: [Double] }
    struct Point: Decodable { var x: Double; var y: Double; var i: Int }
    struct Entry: Decodable { var x: Double; var y: Double; var name: String; var index: Int }
}

struct Tutorial: Decodable, Identifiable {
    var n: Int
    var title: String
    var text: String
    var id: Int { n }
}

/// "HH:MM" of seconds since midnight.
func hhmm(_ seconds: Double) -> String {
    let s = Int(max(0, seconds.rounded()))
    return String(format: "%02d:%02d", (s / 3600) % 24, (s % 3600) / 60)
}

/// How long a trip takes, from its departure to its arrival, short: "22 min", "1 hr, 5 min"
/// (#2813). The arrival of a trip over midnight runs on past 24:00, so the difference is its length.
func tripTime(_ x: TripInfo) -> String {
    let minutes = max(1, Int(((x.arrival - x.departure) / 60).rounded()))
    return Duration.seconds(minutes * 60).formatted(.units(allowed: [.hours, .minutes], width: .abbreviated))
}

func formatBytes(_ b: UInt64) -> String {
    let d = Double(b)
    if d >= 1_073_741_824 { return String(format: "%.1f GB", d / 1_073_741_824) }
    if d >= 1_048_576 { return String(format: "%.0f MB", d / 1_048_576) }
    return String(format: "%.0f KB", d / 1024)
}
