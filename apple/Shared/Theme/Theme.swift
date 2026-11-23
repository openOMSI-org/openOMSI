import CoreText
import SwiftUI

/// The launcher's look, as the desktop launcher has it (`launcher/ui/styles.css`): calm,
/// neutral graphite surfaces without outlines, one amber accent (the amber of a destination
/// sign) for the main action, the current choice and the active page, Inter for every text,
/// tabular figures wherever times and numbers line up. No glass, no glow, no gradients but the
/// dimming under pictures; the system's own bars (the tab bar) stay as the system draws them.
enum Theme {
    static let accent = Color(hex: 0xE8A030)
    static let accentHover = Color(hex: 0xF0AD45)
    static let accentText = Color(hex: 0x1A1206)
    static let accentSoft = Color(hex: 0xE8A030).opacity(0.14)
    static let ok = Color(hex: 0x4CB782)
    static let warn = Color(hex: 0xE0A43A)
    static let danger = Color(hex: 0xE5534B)
    static let dangerSoft = Color(hex: 0xE5534B).opacity(0.14)

    static let bg = Color(hex: 0x121212)
    static let sidebar = Color(hex: 0x171717)
    static let surface = Color(hex: 0x1C1C1C)
    static let surface2 = Color(hex: 0x252525)
    static let surface3 = Color(hex: 0x2F2F2F)
    /// The "on" of a segmented choice and a switch's off track.
    static let surface4 = Color(hex: 0x3A3A3A)

    static let text = Color(hex: 0xEDEDED)
    static let text2 = Color(hex: 0xB8B8B8)
    static let muted = Color(hex: 0x8A8A8A)

    /// The destination sign: amber figures on near-black.
    static let signBg = Color(hex: 0x0E0F11)
    static let signText = Color(hex: 0xFFB02E)

    /// Corners: cards, and the controls inside them.
    static let radius: CGFloat = 8
    static let radiusSm: CGFloat = 6
    static let sidebarWidth: CGFloat = 232
    /// The pages drawn into pictures (the screenshot tool): scroll views as plain stacks.
    nonisolated(unsafe) static var renderFlat = false

    // --- Inter -----------------------------------------------------------------------------

    /// Inter (the variable font, bundled with its licence) for the whole process: from the
    /// app's resources, or from `OPENBUS_FONTS` (the screenshot tool, which has no bundle).
    static func registerFonts() {
        var urls: [URL] = []
        if let u = Bundle.main.url(forResource: "InterVariable", withExtension: "ttf") { urls.append(u) }
        if let dir = ProcessInfo.processInfo.environment["OPENBUS_FONTS"] {
            urls.append(URL(fileURLWithPath: dir).appendingPathComponent("InterVariable.ttf"))
        }
        for u in urls where FileManager.default.fileExists(atPath: u.path) {
            CTFontManagerRegisterFontsForURL(u as CFURL, .process, nil)
            return
        }
    }

    /// The named instance of the variable font for a weight.
    static func interName(_ weight: Font.Weight) -> String {
        switch weight {
        case .ultraLight, .thin: "InterVariable-ExtraLight"
        case .light: "InterVariable-Light"
        case .medium: "InterVariable-Medium"
        case .semibold: "InterVariable-SemiBold"
        case .bold: "InterVariable-Bold"
        case .heavy: "InterVariable-ExtraBold"
        case .black: "InterVariable-Black"
        default: "InterVariable"
        }
    }

    #if os(iOS)
    static func uiFont(_ size: CGFloat, _ weight: Font.Weight = .regular) -> UIFont {
        UIFont(name: interName(weight), size: size) ?? .systemFont(ofSize: size)
    }
    #endif
}

extension Font {
    /// Inter at a size and weight (the launcher's one typeface).
    static func inter(_ size: CGFloat, _ weight: Font.Weight = .regular) -> Font {
        .custom(Theme.interName(weight), fixedSize: size)
    }
}

extension Color {
    init(hex: UInt32) {
        self.init(red: Double((hex >> 16) & 0xFF) / 255, green: Double((hex >> 8) & 0xFF) / 255, blue: Double(hex & 0xFF) / 255)
    }
}

extension View {
    /// A card's ground: the surface colour in the card's corners, no outline.
    func surface(_ color: Color = Theme.surface, radius: CGFloat = Theme.radius) -> some View {
        background(color, in: RoundedRectangle(cornerRadius: radius, style: .continuous))
    }

    /// A control's ground (a select, a text field, a segmented choice).
    func fieldBackground(_ color: Color = Theme.surface2) -> some View {
        background(color, in: RoundedRectangle(cornerRadius: Theme.radiusSm, style: .continuous))
    }
}

/// The buttons: the amber one that starts things, the plain graphite one, the ghost (no ground
/// until pressed) and the danger one (red text).
struct LauncherButtonStyle: ButtonStyle {
    enum Kind { case primary, plain, ghost, danger }
    var kind: Kind = .plain
    var big = false
    var small = false
    var wide = false
    @Environment(\.isEnabled) private var enabled

    func makeBody(configuration: Configuration) -> some View {
        let pressed = configuration.isPressed
        let ground: Color = switch kind {
        case .primary: pressed ? Theme.accentHover : Theme.accent
        case .plain: pressed ? Theme.surface3 : Theme.surface2
        case .ghost: pressed ? Theme.surface2 : .clear
        case .danger: pressed ? Theme.dangerSoft : Theme.surface2
        }
        let fg: Color = switch kind {
        case .primary: Theme.accentText
        case .plain: Theme.text
        case .ghost: Theme.text2
        case .danger: Theme.danger
        }
        return configuration.label
            .font(.inter(big ? 15 : (small ? 13 : 14), kind == .primary ? .semibold : .medium))
            .labelStyle(TightLabelStyle())
            .lineLimit(1)
            .foregroundStyle(fg)
            .padding(.horizontal, big ? 22 : (small ? 10 : 14))
            .frame(minWidth: small ? 30 : 36, maxWidth: wide ? .infinity : nil, minHeight: big ? 46 : (small ? 30 : 36))
            .background(ground, in: RoundedRectangle(cornerRadius: Theme.radiusSm, style: .continuous))
            .contentShape(RoundedRectangle(cornerRadius: Theme.radiusSm, style: .continuous))
            .opacity(enabled ? 1 : 0.5)
    }
}

/// An icon and its text 8 points apart, as the desktop launcher's buttons have them.
struct TightLabelStyle: LabelStyle {
    func makeBody(configuration: Configuration) -> some View {
        HStack(spacing: 8) {
            configuration.icon.font(.system(size: 14, weight: .semibold)).imageScale(.medium)
            configuration.title
        }
    }
}

extension ButtonStyle where Self == LauncherButtonStyle {
    static var primary: LauncherButtonStyle { LauncherButtonStyle(kind: .primary) }
    static var secondary: LauncherButtonStyle { LauncherButtonStyle(kind: .plain) }
    static var ghost: LauncherButtonStyle { LauncherButtonStyle(kind: .ghost) }
    static var destructive: LauncherButtonStyle { LauncherButtonStyle(kind: .danger) }
    static func launcher(_ kind: LauncherButtonStyle.Kind, big: Bool = false, small: Bool = false, wide: Bool = false) -> LauncherButtonStyle {
        LauncherButtonStyle(kind: kind, big: big, small: small, wide: wide)
    }
}

/// A small press feedback on a phone (nothing on a Mac).
enum Haptics {
    static func tap() {
        #if os(iOS)
        UIImpactFeedbackGenerator(style: .light).impactOccurred()
        #endif
    }
}
