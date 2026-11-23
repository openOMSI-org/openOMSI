import Foundation

/// The launcher speaks the language of the game's settings (`language=` in settings.cfg), not
/// the system's: the string catalog's language is chosen for the app as it starts (a change
/// in the settings is heard from the next start; the game's own texts change at once).
enum AppLanguage {
    /// The interface tables' codes (the game's language list) → Apple's language identifiers.
    static let apple: [String: String] = ["pt": "pt-BR", "pt-pt": "pt-PT", "zh": "zh-Hans", "zh-tw": "zh-Hant", "tl": "fil"]

    /// `home` is the folder the game keeps `.openbus` in (the app's data folder on an iPhone).
    static func apply(home: URL) {
        let file = home.appendingPathComponent(".openbus/settings.cfg")
        guard let text = try? String(contentsOf: file, encoding: .utf8) else { return }
        let code = text.split(whereSeparator: \.isNewline)
            .first { $0.hasPrefix("language=") }
            .map { String($0.dropFirst("language=".count)).trimmingCharacters(in: .whitespaces) }
        guard let code,
              let list = try? JSONSerialization.jsonObject(with: Bridge.call("languages")) as? [[String: Any]],
              let iso = list.first(where: { ($0["code"] as? String)?.caseInsensitiveCompare(code) == .orderedSame })?["iso"] as? String
        else { return }
        UserDefaults.standard.set([apple[iso] ?? iso], forKey: "AppleLanguages")
    }
}
