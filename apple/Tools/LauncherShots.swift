// The Swift launcher's pages drawn into PNG files without a window on the screen (for
// checking a change and for the CI): `launcher-shots <out dir> <W>x<H> <seconds> [page[:step]] ...`
// with the pages of `Page` (drive, settings, mods ...; `drive:bus` / `drive:map` with a picker
// open). Built by scripts/build-apple.sh shots; reads the launcher's data of $HOME as the app
// does, Inter from $OPENBUS_FONTS.
import AppKit
import SwiftUI

let args = CommandLine.arguments
guard args.count >= 4 else {
    print("usage: launcher-shots <out dir> <W>x<H> <seconds to read the content> [page[:step]] ...")
    exit(2)
}
let out = URL(fileURLWithPath: args[1])
let dims = args[2].split(separator: "x").compactMap { Double($0) }
let (w, h) = (dims[0], dims[1])
let wait = Double(args[3]) ?? 8
let pages = args.count > 4 ? Array(args[4...]) : ["drive", "settings", "profile", "mods"]
try? FileManager.default.createDirectory(at: out, withIntermediateDirectories: true)

let app = NSApplication.shared
app.setActivationPolicy(.prohibited)
Theme.registerFonts()
Theme.renderFlat = true

@MainActor
func shot(_ store: LauncherStore, _ name: String) {
    let view = LauncherRoot().environment(store).preferredColorScheme(.dark).environment(\.colorScheme, .dark).frame(width: w, height: h)
    // (SwiftUI's own renderer: no window at all; what AppKit draws itself - menus, text
    // fields - comes out as SwiftUI draws it, the glass as its fallback)
    let renderer = ImageRenderer(content: view)
    renderer.scale = 2
    RunLoop.main.run(until: Date().addingTimeInterval(1.0))
    guard let cg = renderer.cgImage else { return }
    let rep = NSBitmapImageRep(cgImage: cg)
    let file = out.appendingPathComponent("\(name.replacingOccurrences(of: ":", with: "_")).png")
    try? rep.representation(using: .png, properties: [:])?.write(to: file)
    print(file.path)
}

MainActor.assumeIsolated {
    let store = LauncherStore()
    store.open()
    let deadline = Date().addingTimeInterval(wait)
    while Date() < deadline { RunLoop.main.run(until: Date().addingTimeInterval(0.25)) }
    for p in pages {
        let parts = p.split(separator: ":")
        guard let page = Page(rawValue: String(parts[0])) else { continue }
        store.page = page
        store.picker = parts.count > 1 ? DrivePicker(rawValue: String(parts[1])) : nil
        for _ in 0 ..< 8 { RunLoop.main.run(until: Date().addingTimeInterval(0.25)) }
        shot(store, p)
    }
}
