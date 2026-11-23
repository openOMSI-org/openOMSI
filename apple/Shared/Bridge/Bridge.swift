import Foundation

/// The game library's launcher: commands with a JSON argument and a JSON reply (see
/// `crates/omsi-app/src/apple_bridge.rs`). Every call is synchronous and quick except
/// `map_overview` (a map's tiles are read), which the store makes off the main thread.
enum Bridge {
    struct Failure: Error, LocalizedError {
        let message: String
        var errorDescription: String? { message }
    }

    /// The raw reply's `value`, as JSON data.
    static func call(_ cmd: String, _ arg: [String: Any] = [:]) throws -> Data {
        let json = (try? JSONSerialization.data(withJSONObject: arg)) ?? Data("{}".utf8)
        let text = String(decoding: json, as: UTF8.self)
        guard let reply = openbus_bridge(cmd, text) else { throw Failure(message: "no reply to \(cmd)") }
        defer { openbus_bridge_free(reply) }
        let data = Data(bytes: reply, count: strlen(reply))
        guard let obj = try JSONSerialization.jsonObject(with: data, options: [.fragmentsAllowed]) as? [String: Any] else {
            throw Failure(message: "a reply to \(cmd) that is no JSON object")
        }
        if (obj["ok"] as? Bool) != true {
            throw Failure(message: obj["error"] as? String ?? "\(cmd) failed")
        }
        return try JSONSerialization.data(withJSONObject: obj["value"] ?? NSNull(), options: [.fragmentsAllowed])
    }

    /// The reply decoded as `T`.
    static func get<T: Decodable>(_ type: T.Type, _ cmd: String, _ arg: [String: Any] = [:]) throws -> T {
        try JSONDecoder().decode(T.self, from: call(cmd, arg))
    }

    static func api(_ cmd: String, _ args: [String: Any] = [:]) throws -> Data {
        try call("api", ["cmd": cmd, "args": args])
    }

    static func api<T: Decodable>(_ type: T.Type, _ cmd: String, _ args: [String: Any] = [:]) throws -> T {
        try JSONDecoder().decode(T.self, from: api(cmd, args))
    }

    /// An action of the launcher (`act {do: ...}`); its error, if any, as text.
    @discardableResult
    static func act(_ what: String, _ arg: [String: Any] = [:]) -> String? {
        var a = arg
        a["do"] = what
        do {
            _ = try call("act", a)
            return nil
        } catch {
            return error.localizedDescription
        }
    }
}

/// Any JSON value (the settings, and the parts of a reply that are kept as they come).
enum JSON: Codable, Equatable, Hashable {
    case null
    case bool(Bool)
    case number(Double)
    case string(String)
    case array([JSON])
    case object([String: JSON])

    init(from decoder: Decoder) throws {
        let c = try decoder.singleValueContainer()
        if c.decodeNil() { self = .null }
        else if let b = try? c.decode(Bool.self) { self = .bool(b) }
        else if let n = try? c.decode(Double.self) { self = .number(n) }
        else if let s = try? c.decode(String.self) { self = .string(s) }
        else if let a = try? c.decode([JSON].self) { self = .array(a) }
        else { self = .object(try c.decode([String: JSON].self)) }
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.singleValueContainer()
        switch self {
        case .null: try c.encodeNil()
        case .bool(let b): try c.encode(b)
        case .number(let n): try c.encode(n)
        case .string(let s): try c.encode(s)
        case .array(let a): try c.encode(a)
        case .object(let o): try c.encode(o)
        }
    }

    subscript(key: String) -> JSON? {
        if case .object(let o) = self { return o[key] }
        return nil
    }

    var bool: Bool? {
        switch self {
        case .bool(let b): return b
        case .number(let n): return n != 0
        case .string(let s): return s == "1" || s.lowercased() == "true"
        default: return nil
        }
    }

    var double: Double? {
        switch self {
        case .number(let n): return n
        case .string(let s): return Double(s.trimmingCharacters(in: .whitespaces))
        case .bool(let b): return b ? 1 : 0
        default: return nil
        }
    }

    var string: String? {
        switch self {
        case .string(let s): return s
        case .number(let n): return n == n.rounded() && abs(n) < 1e15 ? String(Int64(n)) : String(n)
        case .bool(let b): return b ? "true" : "false"
        default: return nil
        }
    }

    /// As Foundation sees it, for an argument of a call.
    var any: Any {
        switch self {
        case .null: return NSNull()
        case .bool(let b): return b
        case .number(let n): return n
        case .string(let s): return s
        case .array(let a): return a.map(\.any)
        case .object(let o): return o.mapValues(\.any)
        }
    }
}
