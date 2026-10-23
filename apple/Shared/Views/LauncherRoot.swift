import SwiftUI

/// The launcher: the desktop launcher's sidebar and pages on a Mac, an iPad and an unfolded
/// phone; on a phone the same sections behind the system's tab bar. Which one is the window's
/// size's to say, so a foldable phone or an iPad in Split View changes it as it changes.
struct LauncherRoot: View {
    @Environment(LauncherStore.self) private var store

    var body: some View {
        GeometryReader { geo in
            let phone = geo.size.width < 700 || geo.size.height < 500
            ZStack {
                Theme.bg.ignoresSafeArea()
                if phone {
                    PhoneLauncher()
                        .environment(\.compactLayout, true)
                } else {
                    DesktopLauncher(railOnly: geo.size.width < 1040)
                }
            }
            .accessibilityHidden(store.onboarding)
        }
        .font(.inter(14))
        .foregroundStyle(Theme.text)
        .tint(Theme.accent)
        .overlay { Dialogs() }
        .overlay {
            if store.starting {
                StartingOverlay()
            }
        }
        .animation(.easeOut(duration: 0.15), value: store.starting)
        #if os(iOS)
        .overlay {
            if store.onboarding {
                Onboarding().transition(.opacity)
            }
        }
        .animation(.easeOut(duration: 0.25), value: store.onboarding)
        #endif
        .sheet(item: Bindable(store).picker) { p in
            Group {
                switch p {
                case .map: MapPicker()
                case .bus: BusPicker()
                }
            }
            .environment(store)
            .font(.inter(14))
            .foregroundStyle(Theme.text)
            .tint(Theme.accent)
            .preferredColorScheme(.dark)
            .presentationSizing(.page)
        }
    }
}

struct StartingOverlay: View {
    @Environment(LauncherStore.self) private var store

    var body: some View {
        ZStack {
            Color(red: 8 / 255, green: 9 / 255, blue: 11 / 255).opacity(0.6).ignoresSafeArea()
            VStack(spacing: 14) {
                ProgressView().controlSize(.large).tint(Theme.accent)
                Text("Starting the game...").font(.inter(15, .semibold))
                if let s = store.status, s.error {
                    Text(s.text).font(.inter(13)).foregroundStyle(Theme.danger).multilineTextAlignment(.center).frame(maxWidth: 420)
                    Button("Close") { store.starting = false }.buttonStyle(.secondary)
                }
            }
            .padding(28)
            .surface(radius: 10)
            .shadow(color: .black.opacity(0.35), radius: 15, y: 10)
        }
        .task {
            // (a computer's game runs beside the launcher: the overlay goes once it runs)
            try? await Task.sleep(nanoseconds: store.inProcess ? 30_000_000_000 : 4_000_000_000)
            store.starting = false
        }
    }
}

// --- the desktop layout ----------------------------------------------------------------------

struct DesktopLauncher: View {
    @Environment(LauncherStore.self) private var store
    /// A narrow window: the sidebar keeps only its icons (as the desktop launcher below 1040).
    var railOnly = false

    var body: some View {
        HStack(spacing: 0) {
            Sidebar(railOnly: railOnly)
            PageView(page: store.page)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                .id(store.page)
        }
        .overlay(alignment: .bottomTrailing) {
            StatusToast().padding(20)
        }
        .animation(.easeOut(duration: 0.16), value: store.status?.text)
    }
}

/// A page by its name.
struct PageView: View {
    var page: Page
    var body: some View {
        switch page {
        case .drive: DrivePage()
        case .depot: DepotPage()
        case .multiplayer: MultiplayerPage()
        case .profile: ProfilePage()
        case .mods: ModsPage()
        case .timetable: TimetablePage()
        case .settings: SettingsPage()
        case .controls: ControlsPage()
        case .games: GamesPage()
        case .tutorials: TutorialsPage()
        case .setup: SetupPage()
        }
    }
}

/// The sidebar: the name and version on top, the pages, the driver at the foot.
struct Sidebar: View {
    @Environment(LauncherStore.self) private var store
    var railOnly = false

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(spacing: 10) {
                Image(systemName: "bus.doubledecker.fill").font(.system(size: 18, weight: .semibold)).foregroundStyle(Theme.accentText)
                    .frame(width: 28, height: 28)
                    .background(Theme.accent, in: RoundedRectangle(cornerRadius: 6, style: .continuous))
                if !railOnly {
                    VStack(alignment: .leading, spacing: 0) {
                        Text("openOMSI").font(.inter(17, .bold)).tracking(-0.3)
                        Text(String(localized: "Launcher") + " \(store.version)").font(.inter(11.5, .medium)).foregroundStyle(Theme.muted)
                    }
                }
            }
            .frame(maxWidth: .infinity, alignment: railOnly ? .center : .leading)
            .padding(.horizontal, railOnly ? 0 : 8)
            .padding(.bottom, 18)
            ViewThatFits(in: .vertical) {
                pages
                ScrollView { pages }.scrollIndicators(.never)
            }
            Spacer()
            DriverChip(railOnly: railOnly)
        }
        .padding(.top, 18)
        .padding(.horizontal, 12)
        .padding(.bottom, 12)
        .frame(width: railOnly ? 72 : Theme.sidebarWidth)
        .frame(maxHeight: .infinity)
        .background(Theme.sidebar.ignoresSafeArea())
    }

    private var pages: some View {
        VStack(alignment: .leading, spacing: 2) {
            ForEach(Page.main) { p in SidebarItem(page: p, railOnly: railOnly) }
            Spacer().frame(height: 10)
            ForEach(Page.more) { p in SidebarItem(page: p, railOnly: railOnly) }
        }
    }
}

struct SidebarItem: View {
    @Environment(LauncherStore.self) private var store
    var page: Page
    var railOnly = false

    var body: some View {
        let on = store.page == page
        let count = badge
        Button { store.page = page } label: {
            HStack(spacing: 12) {
                Image(systemName: page.icon).font(.system(size: 16, weight: .medium)).frame(width: 20)
                    .foregroundStyle(on ? Theme.accent : Theme.text2)
                if !railOnly {
                    Text(page.title).font(.inter(14, .medium)).foregroundStyle(on ? Theme.text : Theme.text2).lineLimit(1)
                    Spacer(minLength: 4)
                    if count > 0 {
                        Text("\(count)").font(.inter(11.5, .bold).monospacedDigit()).foregroundStyle(Theme.accentText)
                            .padding(.horizontal, 6).frame(minWidth: 20, minHeight: 20)
                            .background(Theme.accent, in: Capsule())
                    } else if page == .setup && !store.snapshot.root_ok {
                        Circle().fill(Theme.danger).frame(width: 8, height: 8)
                    }
                }
            }
            .padding(.horizontal, railOnly ? 0 : 10)
            .frame(maxWidth: .infinity, minHeight: 38, alignment: railOnly ? .center : .leading)
            .background(on ? Theme.surface3 : .clear, in: RoundedRectangle(cornerRadius: Theme.radiusSm, style: .continuous))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(Text(page.title))
    }

    /// Running games, installs under way.
    private var badge: Int {
        switch page {
        case .games: store.snapshot.instances.filter { $0.running != false }.count
        case .mods: store.snapshot.jobs.filter { !["done", "failed", "cancelled"].contains($0.state) }.count
        default: 0
        }
    }
}

/// The driver at the sidebar's foot: their initial, name and level; opens the Profile.
struct DriverChip: View {
    @Environment(LauncherStore.self) private var store
    var railOnly = false
    var action: (() -> Void)? = nil

    var body: some View {
        let name = store.snapshot.config.profile
        Button { if let action { action() } else { store.page = .profile } } label: {
            HStack(spacing: 10) {
                Avatar(name: name)
                if !railOnly {
                    VStack(alignment: .leading, spacing: 1) {
                        Text(name.isEmpty ? String(localized: "No driver") : name).font(.inter(14, .semibold)).lineLimit(1)
                        Text(store.snapshot.profile.map { tr("Level {n}", ["n": "\($0.level)"]) } ?? String(localized: "Choose a driver"))
                            .font(.inter(12)).foregroundStyle(Theme.muted)
                    }
                    Spacer(minLength: 0)
                }
            }
            .padding(railOnly ? 6 : 10)
            .frame(maxWidth: .infinity)
            .surface()
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

/// A crash of the last game, a server that sent the player away.
struct Dialogs: View {
    @Environment(LauncherStore.self) private var store
    @State private var bootCrash: CrashReport?

    var body: some View {
        Group {
            if let c = store.snapshot.crash {
                crashDialog(CrashReport(what: c.why, tail: c.log)) { store.act("dismiss_crash") }
            } else if let c = bootCrash {
                crashDialog(c) { bootCrash = nil }
            } else if let d = store.snapshot.disconnected {
                dialog(title: "Disconnected from the server", text: d, detail: nil) {
                    store.act("dismiss_disconnected")
                }
            }
        }
        .task(id: store.platform) {
            guard store.platform == "ios", bootCrash == nil else { return }
            bootCrash = (try? await offMain { try Bridge.api(BootCrash.self, "bootstrap") })?.crash
        }
    }

    private func crashDialog(_ c: CrashReport, close: @escaping () -> Void) -> some View {
        frame(title: c.silent ? "The game was closed by the system" : "The game closed on an error") {
            CrashContent(crash: c)
        } buttons: {
            CrashButtons(crash: c, close: close)
            Button("Close", action: close).buttonStyle(.secondary)
        }
    }

    private func dialog(title: LocalizedStringKey, text: String, detail: String?, close: @escaping () -> Void) -> some View {
        frame(title: title) {
            Text(text).font(.inter(14)).foregroundStyle(Theme.text2)
            if let detail, !detail.isEmpty { LogBlock(text: detail).frame(maxHeight: 220) }
        } buttons: {
            Button("Close", action: close).buttonStyle(.primary)
        }
    }

    private func frame<Content: View, Buttons: View>(title: LocalizedStringKey, @ViewBuilder content: () -> Content, @ViewBuilder buttons: () -> Buttons) -> some View {
        ZStack {
            Color(red: 8 / 255, green: 9 / 255, blue: 11 / 255).opacity(0.6).ignoresSafeArea()
            VStack(alignment: .leading, spacing: 0) {
                Text(title).font(.inter(17, .semibold)).padding(.horizontal, 20).padding(.top, 18).padding(.bottom, 10)
                VStack(alignment: .leading, spacing: 12) { content() }
                    .padding(.horizontal, 20).padding(.vertical, 8)
                ViewThatFits(in: .horizontal) {
                    HStack(spacing: 8) { Spacer(); buttons() }
                    VStack(alignment: .trailing, spacing: 8) { buttons() }.frame(maxWidth: .infinity, alignment: .trailing)
                }
                .padding(.horizontal, 20).padding(.top, 12).padding(.bottom, 18)
            }
            .frame(maxWidth: 560)
            .surface(radius: 10)
            .shadow(color: .black.opacity(0.35), radius: 15, y: 10)
            .padding(32)
        }
    }
}
