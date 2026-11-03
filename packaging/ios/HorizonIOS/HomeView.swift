import Foundation
import SwiftUI
import UIKit
import UniformTypeIdentifiers

private extension Notification.Name {
    static let horizonGraphicsChanged = Notification.Name("HorizonGraphicsChanged")
    static let horizonResourceImportChanged = Notification.Name("HorizonResourceImportChanged")
    static let horizonTouchChanged = Notification.Name("HorizonTouchChanged")
    static let horizonCoreLoadChanged = Notification.Name("HorizonCoreLoadChanged")
}

// Main-thread-only: Rust reports an error synchronously before the SwiftUI
// view may exist; it remains available to the first HomeView instance.
private var mostRecentGraphicsError = ""
private var mostRecentRendererStage = ""
private var mostRecentResourceMessage = ""
private var mostRecentCoreLoadMessage = ""

/// The iOS sandbox is owned by Swift/UIKit, not inferred by a desktop-style
/// current working directory in the Rust game. Preflight is idempotent.
private func prepareSharedEngine() {
    guard Thread.isMainThread else { return }
    guard let support = FileManager.default.urls(for: .applicationSupportDirectory,
                                                  in: .userDomainMask).first else {
        NSLog("Horizon iOS: cannot locate application-support directory")
        return
    }
    let home = support.appendingPathComponent("Horizon", isDirectory: true)
    let bytes = Array(home.path.utf8)
    let result = bytes.withUnsafeBufferPointer { buffer in
        horizon_ios_prepare_engine(buffer.baseAddress, buffer.count)
    }
    if result != 0 {
        NSLog("Horizon iOS: shared engine profile preflight failed")
    }
}

/// `simctl launch` environment variables can be missing on some simulator
/// restarts. An explicit sandbox marker makes the renderer diagnostic reliable
/// without changing regular launches or shipping proprietary game data.
private func enableRequestedRendererSmoke() {
    guard Thread.isMainThread,
          let caches = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask).first else { return }
    let marker = caches.appendingPathComponent("horizon-ios-renderer-smoke.flag")
    guard FileManager.default.fileExists(atPath: marker.path) else { return }
    do {
        let contents = try String(contentsOf: marker, encoding: .utf8)
        guard contents.trimmingCharacters(in: .whitespacesAndNewlines) == "1" else { return }
        horizon_ios_enable_renderer_smoke()
        NSLog("Horizon iOS: explicit boot renderer smoke requested")
        try FileManager.default.removeItem(at: marker)
    } catch {
        NSLog("Horizon iOS: cannot enable boot renderer smoke: %@", error.localizedDescription)
    }
}

/// Leave a machine-readable signal for CI. simctl launch alone is not proof
/// that the renderer initialized or presented a valid Metal surface.
private func saveGraphicsStatus() {
    guard Thread.isMainThread else { return }
    guard let caches = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask).first else { return }
    let status: [String: Any] = [
        "state": Int(horizon_ios_graphics_state()),
        "error": mostRecentGraphicsError,
        "engine": Int(horizon_ios_engine_state()),
        "core_load": Int(horizon_ios_core_load_state()),
        "renderer_preview": Int(horizon_ios_renderer_preview_state()),
        "renderer_stage": mostRecentRendererStage,
        "smoke_mode": Int(horizon_ios_renderer_smoke_enabled()),
        "player": horizon_ios_player_name().map { String(cString: $0) } ?? ""
    ]
    do {
        try FileManager.default.createDirectory(at: caches, withIntermediateDirectories: true)
        let data = try JSONSerialization.data(withJSONObject: status, options: [.sortedKeys])
        try data.write(to: caches.appendingPathComponent("horizon-ios-graphics.json"), options: .atomic)
    } catch {
        NSLog("Horizon iOS: cannot save graphics status: %@", error.localizedDescription)
    }
}

private struct HomeView: View {
    @State private var bridgeState = horizon_ios_state()
    @State private var graphicsState = horizon_ios_graphics_state()
    @State private var engineState = horizon_ios_engine_state()
    @State private var coreLoadState = horizon_ios_core_load_state()
    @State private var rendererPreviewState = horizon_ios_renderer_preview_state()
    @State private var rendererStage = mostRecentRendererStage
    @State private var coreLoadMessage = mostRecentCoreLoadMessage
    @State private var loadingCore = false
    @State private var playerName = horizon_ios_player_name().map { String(cString: $0) } ?? ""
    @State private var graphicsError = mostRecentGraphicsError
    @State private var showResourceImporter = false
    @State private var importingResources = false
    @State private var resourceMessage = mostRecentResourceMessage
    @State private var activeTouchCount = horizon_ios_touch_count()
    @State private var touchEventCount = horizon_ios_touch_events()
    @State private var diagnosticFingerDown = false

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

    private var engineLabel: String {
        switch engineState {
        case 1: return "Waiting for game resources"
        case 2: return "Resource manifest found"
        case 3: return "Profile initialization failed"
        default: return "Not checked"
        }
    }

    private var coreLoadLabel: String {
        switch coreLoadState {
        case 1: return "Loading game block registry"
        case 2: return "Game block registry loaded"
        case 3: return "Game data load failed"
        default: return "Not started"
        }
    }

    /// The shared native client's first CPU loading stage: block states,
    /// metadata and tags, not the WGPU renderer or world simulation.
    private func loadGameRegistry() {
        guard engineState == 2, !loadingCore, coreLoadState != 2 else { return }
        loadingCore = true
        coreLoadMessage = "Reading prepared game data…"
        DispatchQueue.global(qos: .userInitiated).async {
            _ = horizon_ios_load_core_assets()
            DispatchQueue.main.async {
                loadingCore = false
                refreshStatus()
            }
        }
    }

    private var rendererPreviewLabel: String {
        switch rendererPreviewState {
        case 1: return "Handoff requested"
        case 2: return "Initializing Horizon renderer"
        case 3: return "Horizon renderer presented a frame"
        case 4: return "Failed; using Metal probe"
        default: return "Metal probe"
        }
    }

    private func previewGameRenderer() {
        let result = horizon_ios_preview_game_renderer()
        if result != 0 {
            mostRecentGraphicsError = "Cannot start Horizon renderer preview (code \(result))"
        }
        refreshStatus()
    }

    private func refreshStatus() {
        bridgeState = horizon_ios_state()
        graphicsState = horizon_ios_graphics_state()
        engineState = horizon_ios_engine_state()
        coreLoadState = horizon_ios_core_load_state()
        rendererPreviewState = horizon_ios_renderer_preview_state()
        rendererStage = mostRecentRendererStage
        playerName = horizon_ios_player_name().map { String(cString: $0) } ?? ""
        graphicsError = mostRecentGraphicsError
        activeTouchCount = horizon_ios_touch_count()
        touchEventCount = horizon_ios_touch_events()
    }

    /// Forward diagnostic SwiftUI drag input to the same Rust input recorder
    /// used by real Winit/UIKit touch events; gameplay bindings come later.
    private func touchPadEvent(_ phase: UInt32, _ point: CGPoint, _ size: CGSize) {
        let width = UInt32(max(size.width.rounded(), 1))
        let height = UInt32(max(size.height.rounded(), 1))
        _ = horizon_ios_debug_touch(phase, Float(point.x), Float(point.y), width, height)
        activeTouchCount = horizon_ios_touch_count()
        touchEventCount = horizon_ios_touch_events()
    }

    /// Import the player's private Horizon resource ZIP through iOS Files.
    /// Coordinate and copy the security-scoped file into our own sandbox,
    /// then run Rust's existing bounded ZIP extractor off the UIKit thread.
    private func selectResourceArchive(_ result: Result<URL, Error>) {
        guard !importingResources, engineState != 2 else { return }
        switch result {
        case .failure(let error):
            resourceMessage = "Cannot open archive: \(error.localizedDescription)"
        case .success(let url):
            guard url.startAccessingSecurityScopedResource() else {
                resourceMessage = "Files could not grant access to the archive"
                return
            }
            importingResources = true
            resourceMessage = "Importing prepared resources…"
            DispatchQueue.global(qos: .userInitiated).async {
                defer { url.stopAccessingSecurityScopedResource() }
                let staged = FileManager.default.temporaryDirectory
                    .appendingPathComponent("horizon-\(UUID().uuidString).zip")
                defer { try? FileManager.default.removeItem(at: staged) }

                var coordinationError: NSError?
                var copyError: Error?
                let coordinator = NSFileCoordinator(filePresenter: nil)
                coordinator.coordinate(readingItemAt: url, options: [], error: &coordinationError) { accessible in
                    do {
                        try FileManager.default.copyItem(at: accessible, to: staged)
                    } catch {
                        copyError = error
                    }
                }
                if let error = coordinationError ?? (copyError as NSError?) {
                    DispatchQueue.main.async {
                        resourceMessage = "Cannot read ZIP: \(error.localizedDescription)"
                        importingResources = false
                    }
                    return
                }
                let bytes = Array(staged.path.utf8)
                _ = bytes.withUnsafeBufferPointer { buffer in
                    horizon_ios_import_resources(buffer.baseAddress, buffer.count)
                }
                DispatchQueue.main.async {
                    importingResources = false
                    refreshStatus()
                }
            }
        }
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
                    LabeledContent("Game assets", value: engineLabel)
                    LabeledContent("Game CPU data", value: coreLoadLabel)
                    LabeledContent("Renderer", value: rendererPreviewLabel)
                    if !rendererStage.isEmpty {
                        Text(rendererStage)
                            .font(.footnote)
                            .foregroundStyle(.secondary)
                            .textSelection(.enabled)
                    }
                    if !playerName.isEmpty {
                        LabeledContent("Local player", value: playerName)
                    }
                    if (graphicsState == 3 || rendererPreviewState == 4) && !graphicsError.isEmpty {
                        Text(graphicsError)
                            .font(.footnote.monospaced())
                            .foregroundStyle(.red)
                            .textSelection(.enabled)
                    }
                    Button("Refresh status", action: refreshStatus)
                        .buttonStyle(.bordered)
                    if engineState == 1 || engineState == 3 {
                        Button("Import prepared resources ZIP") {
                            showResourceImporter = true
                        }
                        .buttonStyle(.borderedProminent)
                        .disabled(importingResources)
                    }
                    Divider()
                    Text("Touch input bridge")
                        .font(.headline)
                    LabeledContent("Active fingers", value: String(activeTouchCount))
                    LabeledContent("Input events", value: String(touchEventCount))
                    GeometryReader { geometry in
                        RoundedRectangle(cornerRadius: 9)
                            .fill(Color.primary.opacity(0.06))
                            .overlay {
                                Text("Drag or tap to test touch input")
                                    .font(.footnote)
                                    .foregroundStyle(.secondary)
                            }
                            .contentShape(Rectangle())
                            .gesture(DragGesture(minimumDistance: 0)
                                .onChanged { gesture in
                                    touchPadEvent(diagnosticFingerDown ? 1 : 0,
                                                  gesture.location, geometry.size)
                                    diagnosticFingerDown = true
                                }
                                .onEnded { gesture in
                                    touchPadEvent(2, gesture.location, geometry.size)
                                    diagnosticFingerDown = false
                                })
                    }
                    .frame(height: 76)
                    if engineState == 2 && coreLoadState != 2 {
                        Button("Load game block registry", action: loadGameRegistry)
                            .buttonStyle(.borderedProminent)
                            .disabled(loadingCore)
                    }
                    if coreLoadState == 2 && graphicsState == 2 &&
                        (rendererPreviewState == 0 || rendererPreviewState == 4) {
                        Button("Preview Horizon renderer", action: previewGameRenderer)
                            .buttonStyle(.bordered)
                    }
                    if rendererPreviewState == 1 || rendererPreviewState == 2 {
                        ProgressView("Initializing the Horizon boot renderer")
                    }
                    if loadingCore {
                        ProgressView("Loading game block registry")
                    }
                    if !coreLoadMessage.isEmpty {
                        Text(coreLoadMessage)
                            .font(.footnote)
                            .foregroundColor(coreLoadState == 3 ? .red : .secondary)
                            .textSelection(.enabled)
                    }
                    if importingResources {
                        ProgressView("Importing game resources")
                    }
                    if !resourceMessage.isEmpty {
                        Text(resourceMessage)
                            .font(.footnote)
                            .foregroundStyle(.secondary)
                            .textSelection(.enabled)
                    }

                    Text("The Metal probe renders the blue background. After loading game data, Preview Horizon renderer shows Horizon's native title and animated panorama. Touch controls and launching worlds are not connected yet.")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }
                .padding(20)
                .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 16))
                .padding(16)
            }
        }
        .fileImporter(isPresented: $showResourceImporter, allowedContentTypes: [.zip], onCompletion: selectResourceArchive)
        .onReceive(NotificationCenter.default.publisher(for: .horizonCoreLoadChanged)) { _ in
            coreLoadMessage = mostRecentCoreLoadMessage
            refreshStatus()
        }
        .onReceive(NotificationCenter.default.publisher(for: .horizonTouchChanged)) { _ in
            refreshStatus()
        }
        .onReceive(NotificationCenter.default.publisher(for: .horizonResourceImportChanged)) { _ in
            resourceMessage = mostRecentResourceMessage
            refreshStatus()
        }
        .onReceive(NotificationCenter.default.publisher(for: UIApplication.didBecomeActiveNotification)) { _ in
            refreshStatus()
        }
        .onReceive(NotificationCenter.default.publisher(for: UIApplication.didEnterBackgroundNotification)) { _ in
            if diagnosticFingerDown {
                touchPadEvent(3, .zero, CGSize(width: 1, height: 1))
                diagnosticFingerDown = false
            }
            refreshStatus()
        }
        .onReceive(NotificationCenter.default.publisher(for: .horizonGraphicsChanged)) { _ in
            refreshStatus()
        }
    }
}

/// Exposes the native game's existing GPU startup checkpoints. Persisting
/// synchronously is essential: a stalled main thread cannot execute an
/// asynchronous status update before the Simulator snapshot is collected.
@_cdecl("horizon_ios_renderer_stage_changed")
public func horizon_ios_renderer_stage_changed(_ bytes: UnsafePointer<UInt8>?, _ length: Int) {
    guard let bytes, length > 0, length <= 1024 else { return }
    let message = String(decoding: UnsafeBufferPointer(start: bytes, count: length), as: UTF8.self)
    if Thread.isMainThread {
        mostRecentRendererStage = message
        saveGraphicsStatus()
    } else {
        DispatchQueue.main.async {
            mostRecentRendererStage = message
            saveGraphicsStatus()
        }
    }
}

/// Synchronous checkpoint for the expensive native renderer handoff.
/// We must save the current stage before `loading_renderer` can block UIKit;
/// dispatching that write would make a stalled initialization look idle.
@_cdecl("horizon_ios_renderer_status_changed")
public func horizon_ios_renderer_status_changed() {
    guard Thread.isMainThread else { return }
    saveGraphicsStatus()
    DispatchQueue.main.async {
        NotificationCenter.default.post(name: .horizonGraphicsChanged, object: nil)
    }
}

/// Called by Rust on Winit's main thread whenever GPU status changes.
/// Notification delivery is deferred to avoid re-entering SwiftUI during Winit callbacks.
@_cdecl("horizon_ios_graphics_changed")
public func horizon_ios_graphics_changed(_ state: UInt32) {
    DispatchQueue.main.async {
        saveGraphicsStatus()
        NotificationCenter.default.post(name: .horizonGraphicsChanged, object: nil)
    }
}

/// Called by the worker that loaded Horizon's actual block registry.
/// Rust borrows this string; copy it before dispatching UI notifications.
@_cdecl("horizon_ios_core_load_result")
public func horizon_ios_core_load_result(_ bytes: UnsafePointer<UInt8>?,
                                        _ length: Int, _ succeeded: Int32) {
    guard let bytes, length > 0, length <= 4096 else { return }
    let message = String(decoding: UnsafeBufferPointer(start: bytes, count: length), as: UTF8.self)
    DispatchQueue.main.async {
        mostRecentCoreLoadMessage = succeeded == 1 ? message : "Game data load failed: \(message)"
        saveGraphicsStatus()
        NotificationCenter.default.post(name: .horizonCoreLoadChanged, object: nil)
    }
}

/// Rust calls this after recording a Winit or diagnostic Swift touch event.
/// Re-dispatch to avoid mutating SwiftUI inside an event-loop callback.
@_cdecl("horizon_ios_touch_changed")
public func horizon_ios_touch_changed() {
    DispatchQueue.main.async {
        NotificationCenter.default.post(name: .horizonTouchChanged, object: nil)
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
        saveGraphicsStatus()
        NotificationCenter.default.post(name: .horizonGraphicsChanged, object: nil)
    }
}

/// May be invoked on a worker. Copy Rust's temporary bytes before hopping
/// back to the main actor to notify SwiftUI of the import result.
@_cdecl("horizon_ios_resource_import_result")
public func horizon_ios_resource_import_result(_ bytes: UnsafePointer<UInt8>?,
                                               _ length: Int, _ succeeded: Int32) {
    guard let bytes, length > 0, length <= 4096 else { return }
    let message = String(decoding: UnsafeBufferPointer(start: bytes, count: length), as: UTF8.self)
    DispatchQueue.main.async {
        mostRecentResourceMessage = succeeded == 1 ? message : "Import failed: \(message)"
        saveGraphicsStatus()
        NotificationCenter.default.post(name: .horizonResourceImportChanged, object: nil)
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

    prepareSharedEngine()
    enableRequestedRendererSmoke()
    saveGraphicsStatus()
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
