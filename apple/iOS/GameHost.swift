import CoreMotion
import SwiftUI
import UIKit
import UniformTypeIdentifiers

/// What the game asks of the app (`crates/omsi-app/src/apple.rs`): its window on the scene,
/// the motion sensor for tilt steering, the haptic engine, the clipboard and the browser.
@MainActor
final class GameHost {
    static let shared = GameHost()

    let store = LauncherStore()
    weak var scene: UIWindowScene?
    weak var launcherWindow: UIWindow?
    /// The game's window while it is on the scene.
    weak var gameWindow: UIWindow?

    private let motion = CMMotionManager()
    private var tiltValue: Float = 0
    private var tiltSeen = false

    func attach(_ view: UIView) {
        guard let w = view.window else { return }
        if let scene { w.windowScene = scene; w.frame = scene.coordinateSpace.bounds }
        w.windowLevel = .normal + 1
        w.backgroundColor = .black
        w.isHidden = false
        w.makeKeyAndVisible()
        gameWindow = w
        launcherWindow?.isHidden = true
        store.starting = false
        UIApplication.shared.isIdleTimerDisabled = true
    }

    func detach(_ view: UIView) {
        if let w = view.window {
            w.isHidden = true
            w.windowScene = nil
        }
        gameWindow = nil
        showLauncher()
    }

    func showLauncher() {
        UIApplication.shared.isIdleTimerDisabled = false
        launcherWindow?.isHidden = false
        launcherWindow?.makeKeyAndVisible()
        if !store.ready { store.open() }
        store.gameEnded()
    }

    // --- tilt steering ---------------------------------------------------------------------

    func tilt(_ on: Bool) {
        if on {
            guard motion.isDeviceMotionAvailable, !motion.isDeviceMotionActive else { return }
            motion.deviceMotionUpdateInterval = 1.0 / 60.0
            var smooth: Float = 0
            motion.startDeviceMotionUpdates(to: .main) { [weak self] m, _ in
                guard let self, let g = m?.gravity else { return }
                // held across (landscape either way round): gravity lies along the device's
                // x axis, and turning it like a wheel moves it into y (as on Android)
                let (ax, ay) = (Float(g.x), Float(g.y))
                let angle = atan2(ay * (ax < 0 ? -1 : 1), abs(ax)) * 180 / .pi
                let dead: Float = 2.5
                let a = abs(angle) < dead ? 0 : angle - dead * (angle < 0 ? -1 : 1)
                let turn = max(-1, min(1, a / 45))
                smooth += (turn - smooth) * 0.35
                self.tiltValue = smooth
                self.tiltSeen = true
            }
        } else {
            motion.stopDeviceMotionUpdates()
            tiltSeen = false
        }
    }

    func tiltValue(_ out: UnsafeMutablePointer<Float>) -> Bool {
        guard tiltSeen else { return false }
        out.pointee = tiltValue
        return true
    }
}

// --- the functions the game calls (C names, see apple.rs) ------------------------------------

@_cdecl("openbus_swift_attach_window")
func openbusSwiftAttachWindow(_ view: UnsafeMutableRawPointer) {
    let v = Unmanaged<UIView>.fromOpaque(view).takeUnretainedValue()
    MainActor.assumeIsolated { GameHost.shared.attach(v) }
}

@_cdecl("openbus_swift_detach_window")
func openbusSwiftDetachWindow(_ view: UnsafeMutableRawPointer) {
    let v = Unmanaged<UIView>.fromOpaque(view).takeUnretainedValue()
    MainActor.assumeIsolated { GameHost.shared.detach(v) }
}

@_cdecl("openbus_swift_game_started")
func openbusSwiftGameStarted() {
    MainActor.assumeIsolated { GameHost.shared.store.starting = false }
}

@_cdecl("openbus_swift_show_launcher")
func openbusSwiftShowLauncher() {
    MainActor.assumeIsolated { GameHost.shared.showLauncher() }
}

@_cdecl("openbus_swift_tilt")
func openbusSwiftTilt(_ on: Int32) {
    DispatchQueue.main.async { GameHost.shared.tilt(on != 0) }
}

@_cdecl("openbus_swift_tilt_value")
func openbusSwiftTiltValue(_ out: UnsafeMutablePointer<Float>) -> Int32 {
    // (asked from the game's frame, on the main thread)
    MainActor.assumeIsolated { GameHost.shared.tiltValue(out) ? 1 : 0 }
}

@_cdecl("openbus_swift_haptic")
func openbusSwiftHaptic(_ ms: UInt32) {
    DispatchQueue.main.async {
        UIImpactFeedbackGenerator(style: ms > 30 ? .medium : .light).impactOccurred()
    }
}

@_cdecl("openbus_swift_open_url")
func openbusSwiftOpenUrl(_ url: UnsafePointer<CChar>) {
    let s = String(cString: url)
    DispatchQueue.main.async { Platform.openURL(s) }
}

@_cdecl("openbus_swift_safe_insets")
func openbusSwiftSafeInsets(_ out: UnsafeMutablePointer<Float>) {
    MainActor.assumeIsolated {
        guard let w = GameHost.shared.gameWindow else { return }
        let s = Float(w.screen.scale)
        let i = w.safeAreaInsets
        // (the home bar is hidden while driving: only the sides, where the island and the
        // corners are)
        out[0] = Float(i.left) * s
        out[1] = 0
        out[2] = Float(i.right) * s
        out[3] = 0
    }
}

@_cdecl("openbus_swift_is_phone")
func openbusSwiftIsPhone() -> Int32 {
    MainActor.assumeIsolated { UIDevice.current.userInterfaceIdiom == .phone ? 1 : 0 }
}

@_cdecl("openbus_swift_set_clipboard")
func openbusSwiftSetClipboard(_ text: UnsafePointer<CChar>) -> Int32 {
    UIPasteboard.general.string = String(cString: text)
    return 1
}

// --- what the launcher asks of the system -------------------------------------------------------

enum Platform {
    static let isPhone = true

    /// The Files app at the folder (the app's own folders show as "openBus" there).
    static func reveal(_ path: String) {
        var c = URLComponents(url: URL(fileURLWithPath: path), resolvingAgainstBaseURL: false)
        c?.scheme = "shareddocuments"
        if let u = c?.url { UIApplication.shared.open(u) }
    }

    static func openURL(_ s: String) {
        if let u = URL(string: s) { UIApplication.shared.open(u) }
    }

    @MainActor
    static func updateLauncherOrientation(turn: Bool) {
        guard UIDevice.current.userInterfaceIdiom == .phone, let host = GameHost.shared.launcherWindow?.rootViewController else { return }
        host.setNeedsUpdateOfSupportedInterfaceOrientations()
        if turn { GameHost.shared.scene?.requestGeometryUpdate(.iOS(interfaceOrientations: host.supportedInterfaceOrientations)) }
    }

    /// A folder or an archive chosen in the system's document picker.
    @MainActor
    static func pick(folder: Bool, types: [String] = [], completion: @escaping (URL?) -> Void) {
        let utis: [UTType] = folder ? [.folder] : (types.isEmpty ? [.item] : types.compactMap { UTType(filenameExtension: $0) })
        let picker = UIDocumentPickerViewController(forOpeningContentTypes: utis, asCopy: false)
        picker.allowsMultipleSelection = false
        let delegate = PickerDelegate(completion: completion)
        picker.delegate = delegate
        objc_setAssociatedObject(picker, &PickerDelegate.key, delegate, .OBJC_ASSOCIATION_RETAIN_NONATOMIC)
        var top = GameHost.shared.launcherWindow?.rootViewController
        while let p = top?.presentedViewController { top = p }
        top?.present(picker, animated: true)
    }

    /// A mod chosen in the picker: read where it lies, through the access the picker gave.
    static func importForInstall(_ url: URL) -> String {
        _ = url.startAccessingSecurityScopedResource()
        return url.path
    }

    /// The OMSI 2 folder chosen in the picker (iCloud Drive, a USB drive, another app's
    /// folder): the access is kept for the next starts as a bookmark.
    static func keepAccess(_ url: URL) -> String {
        _ = url.startAccessingSecurityScopedResource()
        if let b = try? url.bookmarkData(options: [], includingResourceValuesForKeys: nil, relativeTo: nil) {
            UserDefaults.standard.set(b, forKey: "rootBookmark")
        }
        return url.path
    }

    static func restoreAccess() {
        guard let b = UserDefaults.standard.data(forKey: "rootBookmark") else { return }
        var stale = false
        if let url = try? URL(resolvingBookmarkData: b, options: [], relativeTo: nil, bookmarkDataIsStale: &stale) {
            _ = url.startAccessingSecurityScopedResource()
            if stale { _ = keepAccess(url) }
        }
    }
}

final class PickerDelegate: NSObject, UIDocumentPickerDelegate {
    nonisolated(unsafe) static var key = 0
    let completion: (URL?) -> Void
    init(completion: @escaping (URL?) -> Void) { self.completion = completion }

    func documentPicker(_ controller: UIDocumentPickerViewController, didPickDocumentsAt urls: [URL]) {
        completion(urls.first)
    }

    func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) {
        completion(nil)
    }
}
