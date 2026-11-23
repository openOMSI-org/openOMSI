#if os(iOS)
import SwiftUI
import UIKit

struct Onboarding: View {
    @Environment(LauncherStore.self) private var store
    @State private var step = 0
    @State private var method: String?
    @State private var missing: [String] = []
    @State private var pickMissing: [String]?
    @State private var freeSpace: String?
    @State private var looking = false

    private var phone: Bool { UIDevice.current.userInterfaceIdiom == .phone }

    var body: some View {
        GeometryReader { geo in
            ZStack {
                Theme.bg.ignoresSafeArea()
                ScrollView {
                    VStack(spacing: 0) {
                        dots.padding(.top, 20).padding(.bottom, 28)
                        Group {
                            switch step {
                            case 0: welcome
                            case 1: copy
                            default: ready
                            }
                        }
                        .frame(maxWidth: 520)
                    }
                    .padding(.horizontal, 20)
                    .padding(.bottom, 28)
                    .frame(maxWidth: .infinity, minHeight: geo.size.height)
                }
                .scrollBounceBehavior(.basedOnSize)
            }
            .onChange(of: geo.size.width > geo.size.height) { _, landscape in
                if landscape && step == 2 && phone { finish() }
            }
        }
        .font(.inter(14))
        .foregroundStyle(Theme.text)
        .onAppear {
            step = store.onboardingStep
            if store.snapshot.root_ok && step < 2 { step = 2 }
            orient()
        }
        .onChange(of: step) { _, _ in orient() }
        .onDisappear { Platform.updateLauncherOrientation(turn: true) }
        .onChange(of: store.snapshot.root_ok) { _, ok in
            if ok && step < 2 {
                Haptics.tap()
                withAnimation(.easeOut(duration: 0.25)) { step = 2 }
            }
        }
        .onReceive(NotificationCenter.default.publisher(for: UIApplication.didBecomeActiveNotification)) { _ in Task { await look() } }
        .task(id: step) {
            guard step == 1 else { return }
            freeSpace = Self.freeSpace()
            while !Task.isCancelled {
                await look()
                try? await Task.sleep(nanoseconds: 2_000_000_000)
            }
        }
    }

    private var dots: some View {
        HStack(spacing: 6) {
            ForEach(0..<3) { i in
                Capsule().fill(i == step ? Theme.accent : Theme.surface3).frame(width: i == step ? 22 : 7, height: 7)
            }
        }
        .animation(.easeOut(duration: 0.2), value: step)
    }

    private var welcome: some View {
        VStack(spacing: 22) {
            Image("AppLogo")
                .resizable()
                .interpolation(.high)
                .frame(width: 120, height: 120)
                .clipShape(RoundedRectangle(cornerRadius: 27, style: .continuous))
                .shadow(color: .black.opacity(0.4), radius: 16, y: 8)
                .accessibilityLabel("openBus")
                .padding(.top, 24)
            VStack(spacing: 10) {
                Text("Welcome to openBus").font(.inter(28, .bold)).multilineTextAlignment(.center)
                Text("Drive the buses of OMSI 2 wherever you are: its maps, its buses and its lines, at your fingertips.")
                    .font(.inter(15)).foregroundStyle(Theme.text2).multilineTextAlignment(.center)
            }
            Notice(text: String(localized: "openBus plays the content of your own copy of OMSI 2 (Steam or another copy): the game itself is not included."), kind: .info)
            Spacer(minLength: 12)
            VStack(spacing: 10) {
                Button { next(1) } label: { Label("Get started", systemImage: "arrow.right") }
                    .buttonStyle(.launcher(.primary, big: true, wide: true))
                Button("I already have it: choose the folder") { choose() }
                    .buttonStyle(.ghost)
            }
        }
    }

    private var copy: some View {
        VStack(alignment: .leading, spacing: 18) {
            VStack(alignment: .leading, spacing: 8) {
                Text("Copy OMSI 2 onto this device").font(.inter(24, .bold))
                Text("openBus needs the whole OMSI 2 folder of your computer: the one with Omsi.exe, maps and Vehicles in it.")
                    .font(.inter(14.5)).foregroundStyle(Theme.text2).fixedSize(horizontal: false, vertical: true)
            }
            Notice(text: String(localized: "On Steam: right-click OMSI 2, then Manage, then Browse local files."), kind: .info, icon: "lightbulb.fill")
            if let pickMissing {
                VStack(alignment: .leading, spacing: 6) {
                    Notice(text: String(localized: "This is not a complete OMSI 2 installation."), kind: .danger)
                    Text(pickMissing.joined(separator: ", ")).font(.inter(12)).foregroundStyle(Theme.muted).fixedSize(horizontal: false, vertical: true)
                }
            }
            if let free = freeSpace {
                Facts(rows: [("Free space", free)])
                    .padding(.horizontal, 16).padding(.vertical, 12)
                    .surface()
            }
            Text("How to copy it").font(.inter(12, .semibold)).foregroundStyle(Theme.muted).textCase(.uppercase).tracking(0.4)
                .padding(.top, 4)
            VStack(spacing: 10) {
                way("mac", icon: "laptopcomputer", title: "From a Mac", steps: [
                    "Connect the iPhone or iPad to the Mac with a cable.",
                    "In the Finder, select it in the sidebar, then Files.",
                    "Drag the OMSI 2 folder onto openBus.",
                ])
                way("pc", icon: "pc", title: "From a Windows PC", steps: [
                    "Install Apple Devices from the Microsoft Store (or iTunes).",
                    "Connect the iPhone or iPad with a cable, then open Files (File Sharing in iTunes).",
                    "Drag the OMSI 2 folder onto openBus.",
                ])
                way("usb", icon: "externaldrive.fill", title: "From a USB drive", steps: [
                    "Copy the OMSI 2 folder onto a USB drive and plug it into the iPhone or iPad.",
                    "Choose the folder on the drive.",
                ]) {
                    VStack(alignment: .leading, spacing: 10) {
                        Button { choose() } label: { Label("Choose the folder", systemImage: "folder.badge.plus") }
                            .buttonStyle(.secondary)
                        Text("Or copy it into openBus in the Files app: the drive is then no longer needed.")
                            .font(.inter(13)).foregroundStyle(Theme.muted).fixedSize(horizontal: false, vertical: true)
                    }
                }
            }
            if let c = store.snapshot.content_dir {
                Button { Platform.reveal(c) } label: { Label("Open openBus in Files", systemImage: "folder.fill") }
                    .buttonStyle(.launcher(.plain, wide: true))
            }
            if !missing.isEmpty {
                VStack(alignment: .leading, spacing: 6) {
                    Notice(text: String(localized: "OMSI 2 is there but not complete yet: is the copy still running?"), kind: .warn)
                    Text(missing.joined(separator: ", ")).font(.inter(12)).foregroundStyle(Theme.muted).fixedSize(horizontal: false, vertical: true)
                }
            } else {
                HStack(spacing: 10) {
                    ProgressView().tint(Theme.muted)
                    Text("Looking for OMSI 2: it is found by itself once the copy is done.")
                        .font(.inter(13)).foregroundStyle(Theme.muted).fixedSize(horizontal: false, vertical: true)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            HStack {
                Button { next(0) } label: { Label("Back", systemImage: "chevron.left") }.buttonStyle(.ghost)
                Spacer()
                Button("Not now") { finish() }.buttonStyle(.ghost)
            }
        }
    }

    private func way(_ id: String, icon: String, title: LocalizedStringKey, steps: [LocalizedStringKey]) -> some View {
        way(id, icon: icon, title: title, steps: steps) { EmptyView() }
    }

    private func way<Extra: View>(_ id: String, icon: String, title: LocalizedStringKey, steps: [LocalizedStringKey], @ViewBuilder extra: () -> Extra) -> some View {
        let open = method == id
        return VStack(alignment: .leading, spacing: 0) {
            Button {
                withAnimation(.easeOut(duration: 0.2)) { method = open ? nil : id }
                Haptics.tap()
            } label: {
                HStack(spacing: 14) {
                    Image(systemName: icon).font(.system(size: 18, weight: .medium)).foregroundStyle(open ? Theme.accent : Theme.text2)
                        .frame(width: 38, height: 38)
                        .background(open ? Theme.accentSoft : Theme.surface2, in: RoundedRectangle(cornerRadius: Theme.radiusSm, style: .continuous))
                    Text(title).font(.inter(15, .semibold))
                    Spacer()
                    Image(systemName: "chevron.down").font(.system(size: 13, weight: .semibold)).foregroundStyle(Theme.muted)
                        .rotationEffect(.degrees(open ? 180 : 0))
                }
                .padding(.horizontal, 14).padding(.vertical, 12)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            if open {
                VStack(alignment: .leading, spacing: 12) {
                    ForEach(steps.indices, id: \.self) { i in
                        HStack(alignment: .top, spacing: 12) {
                            Text(String(i + 1)).font(.inter(12, .bold).monospacedDigit()).foregroundStyle(Theme.accentText)
                                .frame(width: 22, height: 22)
                                .background(Theme.accent, in: Circle())
                            Text(steps[i]).font(.inter(14)).foregroundStyle(Theme.text).fixedSize(horizontal: false, vertical: true)
                                .frame(maxWidth: .infinity, alignment: .leading)
                        }
                    }
                    extra()
                }
                .padding(.horizontal, 14).padding(.bottom, 16).padding(.top, 2)
                .transition(.opacity)
            }
        }
        .surface()
    }

    private var ready: some View {
        VStack(spacing: 22) {
            Image(systemName: "checkmark")
                .font(.system(size: 38, weight: .bold))
                .foregroundStyle(Theme.bg)
                .frame(width: 84, height: 84)
                .background(Theme.ok, in: Circle())
                .padding(.top, 24)
            VStack(spacing: 10) {
                Text("All set!").font(.inter(28, .bold))
                Text("OMSI 2 is in place.").font(.inter(15)).foregroundStyle(Theme.text2)
            }
            HStack(spacing: 10) {
                StatTile(icon: "map.fill", value: count(store.snapshot.counts.maps), label: "Maps")
                StatTile(icon: "bus.fill", value: count(store.snapshot.counts.vehicles), label: "Buses")
            }
            Spacer(minLength: 12)
            if phone {
                TurnPhone()
                Text("Turn your phone, and let's go!").font(.inter(20, .bold)).multilineTextAlignment(.center)
                Button("Let's go") { finish() }.buttonStyle(.ghost)
            } else {
                Button { finish() } label: { Label("Let's go", systemImage: "arrow.right") }
                    .buttonStyle(.launcher(.primary, big: true, wide: true))
            }
        }
    }

    private func count(_ n: Int) -> String {
        store.snapshot.loading_content && n == 0 ? "…" : String(n)
    }

    private static func freeSpace() -> String? {
        let home = URL(fileURLWithPath: NSHomeDirectory())
        guard let v = try? home.resourceValues(forKeys: [.volumeAvailableCapacityForImportantUsageKey]).volumeAvailableCapacityForImportantUsage, v > 0 else { return nil }
        return ByteCountFormatter.string(fromByteCount: v, countStyle: .file)
    }

    private func next(_ s: Int) {
        if s == 0 { pickMissing = nil }
        Haptics.tap()
        withAnimation(.easeOut(duration: 0.25)) { step = s }
    }

    private func choose() {
        Platform.pick(folder: true) { url in
            guard let url else { return }
            Task { @MainActor in
                let path = Platform.importForInstall(url)
                let inside = store.snapshot.content_dir.map { docs in
                    URL(fileURLWithPath: path).resolvingSymlinksInPath().path.hasPrefix(URL(fileURLWithPath: docs).resolvingSymlinksInPath().path)
                } ?? false
                if inside {
                    url.stopAccessingSecurityScopedResource()
                    pickMissing = nil
                    next(1)
                    return
                }
                switch await store.lookForGame(at: path) {
                case .found:
                    _ = Platform.keepAccess(url)
                    pickMissing = nil
                case .missing(let m):
                    url.stopAccessingSecurityScopedResource()
                    pickMissing = m
                    next(1)
                default:
                    url.stopAccessingSecurityScopedResource()
                    next(1)
                }
            }
        }
    }

    private func look() async {
        guard !store.snapshot.root_ok, !looking else { return }
        looking = true
        defer { looking = false }
        switch await store.lookForGame() {
        case .missing(let m):
            missing = m
            pickMissing = nil
        case .found:
            missing = []
            pickMissing = nil
        default:
            missing = []
        }
    }

    private func orient() {
        store.onboardingStep = step
        Platform.updateLauncherOrientation(turn: step < 2)
    }

    private func finish() {
        store.closeOnboarding()
        Platform.updateLauncherOrientation(turn: true)
    }
}

private struct TurnPhone: View {
    @State private var turned = false

    var body: some View {
        Image(systemName: "iphone")
            .font(.system(size: 54, weight: .light))
            .foregroundStyle(Theme.accent)
            .rotationEffect(.degrees(turned ? -90 : 0))
            .frame(width: 96, height: 96)
            .task {
                while !Task.isCancelled {
                    try? await Task.sleep(nanoseconds: 700_000_000)
                    withAnimation(.easeInOut(duration: 0.6)) { turned.toggle() }
                }
            }
    }
}
#endif
