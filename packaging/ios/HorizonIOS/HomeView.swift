import Foundation
import SwiftUI
import UIKit

private extension Notification.Name {
    static let horizonGraphicsChanged = Notification.Name("HorizonGraphicsChanged")
}

// Main-thread-only: Rust reports an error synchronously before the SwiftUI
// view may exist; it remains available to the first HomeView instance.
private var mostRecentGraphicsError = ""

private struct HomeView: View {
    @State private var bridgeState = horizon_ios_state()
    @State private var graphicsState = horizon_ios_graphics_state()
    @State private var graphicsError = mostRecentGraphicsError

    private var stateLabel: String {
        switch bridgeState {
        case 1: return "Shell ready"
        case 2: return "Suspended"
        default: return "Not initialized"
        }
    }

    private var graphicsLabel: String {
        switch graphicsState {
        case 1: return "Initializing Metal"
        case 2: return "Metal ready"
        case 3: return "Metal initialization / presentation failed"
        case 4: return "Paused in background"
        default: return "Not started"
        }
    }

    private func refreshStatus() {
        bridgeState = horizon_ios_state()
        graphicsState = horizon_ios_graphics_state()
        graphicsError = mostRecentGraphicsError
    }

    var body: some View {
        ZStack(alignment: .topLeading) {
            // Transparent outside the card, so the blue Metal clear is visible.
            Color.clear.ignoresSafeArea()
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    Text("Horizon")
                        .font(.largeTitle.bold())
                    Text("iOS integration preview")
                        .foregroundStyle(.secondary)

                    Divider()
                    LabeledContent("Game version", value: String(cString: horizon_ios_game_version()))
                    LabeledContent("Rust ABI", value: String(horizon_ios_abi_version()))
                    LabeledContent("App shell", value: stateLabel)
                    LabeledContent("GPU", value: graphicsLabel)
                    if graphicsState == 3 && !graphicsError.isEmpty {
                        Text(graphicsError)
                            .font(.footnote.monospaced())
                            .foregroundStyle(.red)
                            .textSelection(.enabled)
                    }
                    Button("Refresh status", action: refreshStatus)
                        .buttonStyle(.bordered)

                    Text("The blue background is rendered by WGPU/Metal. The Minecraft engine is not running yet.")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }
                .padding(20)
                .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 16))
                .padding(16)
            }
        }
        .onReceive(NotificationCenter.default.publisher(for: UIApplication.didBecomeActiveNotification)) { _ in
            refreshStatus()
        }
        .onReceive(NotificationCenter.default.publisher(for: UIApplication.didEnterBackgroundNotification)) { _ in
            refreshStatus()
        }
        .onReceive(NotificationCenter.default.publisher(for: .horizonGraphicsChanged)) { _ in
            refreshStatus()
        }
    }
}

/// Called by Rust on Winit's main thread whenever GPU status changes.
/// Notification delivery is deferred to avoid re-entering SwiftUI during Winit callbacks.
@_cdecl("horizon_ios_graphics_changed")
public func horizon_ios_graphics_changed(_ state: UInt32) {
    DispatchQueue.main.async {
        NotificationCenter.default.post(name: .horizonGraphicsChanged, object: nil)
    }
}

/// Called on Winit's main thread. Only the copied Swift string escapes the
/// callback; the borrowed Rust pointer must not be retained.
@_cdecl("horizon_ios_report_graphics_error")
public func horizon_ios_report_graphics_error(_ bytes: UnsafePointer<UInt8>?, _ length: Int) {
    guard Thread.isMainThread, let bytes, length > 0, length < 4_096 else { return }
    let text = String(decoding: UnsafeBufferPointer(start: bytes, count: length), as: UTF8.self)
    mostRecentGraphicsError = text
    NSLog("Horizon iOS graphics: %@", text)
    DispatchQueue.main.async {
        NotificationCenter.default.post(name: .horizonGraphicsChanged, object: nil)
    }
}

/// Invoked by Rust only after Winit created its UIKit UIViewController.
/// The Rust pointer is borrowed, not transferred; Winit owns its lifetime.
@_cdecl("horizon_ios_mount_home")
public func horizon_ios_mount_home(_ parentPointer: UnsafeMutableRawPointer?) {
    guard let parentPointer else { return }
    guard Thread.isMainThread else {
        NSLog("Horizon iOS: refusing off-main-thread UIKit access")
        return
    }

    let parent = Unmanaged<UIViewController>.fromOpaque(parentPointer).takeUnretainedValue()
    // The Metal surface may have recreated its CAMetalLayer after backgrounding.
    // Reorder the existing overlay above it rather than installing another one.
    if let existing = parent.children.first(where: { $0 is UIHostingController<HomeView> }) {
        parent.view.bringSubviewToFront(existing.view)
        return
    }
    let hosting = UIHostingController(rootView: HomeView())
    hosting.view.backgroundColor = .clear
    hosting.view.isOpaque = false
    parent.addChild(hosting)
    hosting.view.frame = parent.view.bounds
    hosting.view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
    parent.view.addSubview(hosting.view)
    hosting.didMove(toParent: parent)
}

// These callbacks are used only by the shared Horizon clipboard hooks after
// an explicit Copy/Paste action. The Swift side owns UIKit; Rust owns the buffer.
// Off-main callers are rejected so the Rust clipboard can use its local copy.
@_cdecl("horizon_ios_clipboard_read")
public func horizon_ios_clipboard_read(_ destination: UnsafeMutablePointer<UInt8>?,
                                      _ capacity: Int) -> Int {
    guard Thread.isMainThread, capacity >= 0 else { return Int.max }
    let utf8 = Array((UIPasteboard.general.string ?? "").utf8)
    guard utf8.count <= 1_048_576 else { return Int.max }
    if let destination, capacity >= utf8.count {
        for i in utf8.indices {
            destination[i] = utf8[i]
        }
    }
    return utf8.count
}

@_cdecl("horizon_ios_clipboard_write")
public func horizon_ios_clipboard_write(_ bytes: UnsafePointer<UInt8>?, _ length: Int) {
    guard Thread.isMainThread, length > 0, length <= 1_048_576, let bytes else { return }
    let buffer = UnsafeBufferPointer(start: bytes, count: length)
    guard let text = String(bytes: buffer, encoding: .utf8) else { return }
    UIPasteboard.general.string = text
}
