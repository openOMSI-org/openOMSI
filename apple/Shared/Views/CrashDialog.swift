import SwiftUI

struct CrashReport: Decodable, Equatable {
    var what: String
    var tail: String

    var lost: Bool { what.contains("graphics device was lost") }
    var silent: Bool { what.contains("closed without a word") }

    func report(version: String, os: String) -> String {
        "openBus \(version) (\(os))\n\(what)\n\n\(tail)"
    }

    func issueURL(version: String, os: String) -> String {
        let gap = "\n...\n"
        let (machine, end) = tail.range(of: gap).map { (String(tail[..<$0.lowerBound]), String(tail[$0.upperBound...])) } ?? ("", tail)
        let about = machine.isEmpty ? "" : "The computer:\n```\n\(machine)\n```\n\n"
        let body = { (lines: [String]) in "openBus \(version) on \(os)\n\n```\n\(what)\n```\n\n\(about)The end of the log:\n```\n\(lines.joined(separator: "\n"))\n```\n" }
        let lines = end.components(separatedBy: "\n")
        var shown = 0
        while shown < lines.count, encode(body(Array(lines.suffix(shown + 1)))).count <= 6500 { shown += 1 }
        let title = "Crash: \(String(what.prefix(80)))"
        return "https://github.com/rafaelkaiz/openBus/issues/new?title=\(encode(title))&body=\(encode(body(Array(lines.suffix(shown)))))"
    }

    private func encode(_ s: String) -> String {
        s.addingPercentEncoding(withAllowedCharacters: CharacterSet(charactersIn: "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_.!~*'()")) ?? s
    }
}

struct CrashContent: View {
    @Environment(LauncherStore.self) private var store
    var crash: CrashReport

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(crash.what).font(.inter(14)).foregroundStyle(Theme.text).textSelection(.enabled)
            Text(hint).font(.inter(13.5)).foregroundStyle(Theme.text2).fixedSize(horizontal: false, vertical: true)
            if !crash.tail.isEmpty { LogBlock(text: crash.tail).frame(maxHeight: 220) }
        }
    }

    private var hint: LocalizedStringKey {
        if crash.lost && store.platform == "macos" { return "The graphics driver stopped the game. Updating the graphics driver usually helps; Settings → Graphics API can switch to OpenGL." }
        if crash.silent { return "The system closed the game while it was running, typically because the device ran out of memory (RAM). Lowering texture resolution or reducing AI traffic in Settings helps prevent memory exhaustion." }
        return "Copy the report (the end of the game's log), or open a GitHub issue with it: it tells what went wrong on this computer."
    }
}

struct CrashButtons: View {
    @Environment(LauncherStore.self) private var store
    var crash: CrashReport
    var close: () -> Void

    var body: some View {
        if crash.silent {
            Button { close(); store.page = .settings; store.phoneTab = "settings" } label: { Label("Settings", systemImage: "slider.horizontal.3") }.buttonStyle(.ghost)
        } else {
            Button { copyText(report); Platform.openURL(crash.issueURL(version: store.version, os: store.platform)) } label: { Label("Report on GitHub", systemImage: "arrow.up.forward.square") }.buttonStyle(.ghost)
        }
        Button { copyText(report); store.say(String(localized: "The report is copied: paste it into a GitHub issue or a message."), error: false) } label: { Label("Copy report", systemImage: "doc.on.doc") }
            .buttonStyle(.primary)
    }

    private var report: String { crash.report(version: store.version, os: store.platform) }
}

struct BootCrash: Decodable {
    var crash: CrashReport?
}
