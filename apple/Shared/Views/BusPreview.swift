import SwiftUI

/// The bus of the duty, as the game draws it (the launcher's showroom on a device of its
/// own, see `apple_bridge`): asked for while a preview is on screen, at its size in pixels;
/// a drag turns it, a pinch or the wheel zooms. One for the whole app: the showroom hands a
/// picture over once, so a preview that came back (a sheet closed over it, a page left and
/// opened again) starts from the last picture rather than waiting for one that never comes.
@MainActor
@Observable
final class PreviewModel {
    static let shared = PreviewModel()
    var image: CGImage?
    private var size: (w: Int, h: Int) = (0, 0)
    private var running = false
    private var task: Task<Void, Never>?
    private var users: Set<UUID> = []

    func run(_ user: UUID, width: Int, height: Int) {
        users.insert(user)
        size = (max(16, width), max(16, height))
        guard !running else { return }
        running = true
        task = Task.detached(priority: .userInitiated) { [weak self] in
            var buffer = [UInt8]()
            while let me = self, await me.running {
                let (w, h) = await me.size
                if buffer.count < w * h * 4 { buffer = [UInt8](repeating: 0, count: w * h * 4) }
                var ow: UInt32 = 0, oh: UInt32 = 0
                let got = buffer.withUnsafeMutableBufferPointer { b in
                    openbus_preview_frame(UInt32(w), UInt32(h), b.baseAddress, b.count, &ow, &oh)
                }
                if got == 1, let img = Self.image(buffer, Int(ow), Int(oh)) {
                    await MainActor.run { me.image = img }
                }
                try? await Task.sleep(nanoseconds: 33_000_000)
            }
        }
    }

    func stop(_ user: UUID) {
        users.remove(user)
        guard users.isEmpty else { return }
        running = false
        task = nil
    }

    nonisolated private static func image(_ px: [UInt8], _ w: Int, _ h: Int) -> CGImage? {
        guard w > 0, h > 0, px.count >= w * h * 4 else { return nil }
        let data = Data(px[0 ..< w * h * 4]) as CFData
        guard let provider = CGDataProvider(data: data) else { return nil }
        return CGImage(width: w, height: h, bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: w * 4,
                       space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.noneSkipLast.rawValue),
                       provider: provider, decode: nil, shouldInterpolate: true, intent: .defaultIntent)
    }
}

struct BusPreview: View {
    @Environment(LauncherStore.self) private var store
    @State private var model = PreviewModel.shared
    @State private var id = UUID()
    @State private var lastDrag: CGSize = .zero
    @State private var lastZoom: CGFloat = 1
    @Environment(\.displayScale) private var scale
    var buttonsTop = false

    static let views: [(id: String, label: LocalizedStringKey, icon: String)] = [
        ("outside", "Outside view", "bus.fill"),
        ("passenger", "Passenger view", "person.2.fill"),
        ("driver", "Driver's view", "steeringwheel"),
    ]

    var body: some View {
        GeometryReader { geo in
            ZStack {
                // (the viewer's ground, until the showroom's first picture)
                Color(hex: 0x1A1A1A)
                if let img = model.image, store.snapshot.preview.error == nil {
                    Image(decorative: img, scale: scale)
                        .resizable()
                        .interpolation(.high)
                        .aspectRatio(contentMode: .fill)
                        .frame(width: geo.size.width, height: geo.size.height)
                        .clipped()
                } else if let e = store.snapshot.preview.error {
                    // (why there is no picture, rather than a spinner for ever)
                    VStack(spacing: 8) {
                        Image(systemName: "bus").font(.system(size: 28)).foregroundStyle(Theme.muted)
                        Text(e).font(.inter(12.5)).foregroundStyle(Theme.muted).multilineTextAlignment(.center).lineLimit(4)
                    }
                    .padding(16)
                } else {
                    HStack(spacing: 10) {
                        ProgressView().controlSize(.small).tint(Theme.accent)
                        Text("Loading the bus…").font(.inter(13)).foregroundStyle(Theme.muted)
                    }
                }
            }
            .contentShape(Rectangle())
            .gesture(
                DragGesture(minimumDistance: 2)
                    .onChanged { v in
                        let d = CGSize(width: v.translation.width - lastDrag.width, height: v.translation.height - lastDrag.height)
                        lastDrag = v.translation
                        openbus_preview_orbit(Float(d.width), Float(d.height))
                    }
                    .onEnded { _ in lastDrag = .zero }
            )
            .simultaneousGesture(
                MagnifyGesture()
                    .onChanged { v in
                        let k = lastZoom / v.magnification
                        lastZoom = v.magnification
                        openbus_preview_zoom(Float(k))
                    }
                    .onEnded { _ in lastZoom = 1 }
            )
            .overlay(alignment: buttonsTop ? .topTrailing : .bottomTrailing) { viewButtons.padding(.top, buttonsTop ? 44 : 0) }
            .onAppear { model.run(id, width: Int(geo.size.width * scale), height: Int(geo.size.height * scale)) }
            .onChange(of: geo.size) { _, s in model.run(id, width: Int(s.width * scale), height: Int(s.height * scale)) }
            .onChange(of: store.snapshot.preview.error, initial: true) { _, error in
                if error != nil { model.image = nil }
            }
            .onDisappear { model.stop(id) }
        }
    }

    @ViewBuilder
    private var viewButtons: some View {
        let p = store.snapshot.preview
        let can = p.views ?? []
        if p.error == nil && model.image != nil && can.count > 1 {
            HStack(spacing: 4) {
                ForEach(Array(Self.views.enumerated()), id: \.offset) { i, v in
                    let on = v.id == (p.view ?? "outside")
                    Button { openbus_preview_view(UInt32(i)); Haptics.tap() } label: {
                        Image(systemName: v.icon).font(.system(size: 13, weight: .semibold))
                            .foregroundStyle(on ? Theme.accentText : Theme.text)
                            .frame(width: 36, height: 30)
                            .background(on ? Theme.accent : Theme.bg.opacity(0.72), in: RoundedRectangle(cornerRadius: 8, style: .continuous))
                            .opacity(can.contains(v.id) ? 1 : 0.4)
                    }
                    .buttonStyle(.plain)
                    .disabled(!can.contains(v.id))
                    .help(v.label)
                    .accessibilityLabel(Text(v.label))
                }
            }
            .padding(8)
        }
    }
}
