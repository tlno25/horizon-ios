import SwiftUI
import UIKit

/// Called on the Winit main thread after the game's single UIWindow exists.
/// SwiftUI is a modal over the game's own controller, not another application.
@_cdecl("horizon_launcher_present")
public func horizonLauncherPresent(_ submit: HorizonSubmit?) -> Bool {
    guard Thread.isMainThread else { return false }
    Bridge.submit = submit
    DispatchQueue.main.async { presentLauncher(tries: 25) }
    return true
}

@MainActor
private func presentLauncher(tries: Int) {
    let windows = UIApplication.shared.connectedScenes
        .compactMap { $0 as? UIWindowScene }
        .flatMap(\.windows)
    guard let root = (windows.first(where: \.isKeyWindow) ?? windows.first)?.rootViewController,
          root.view.window != nil else {
        if tries > 0 {
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) {
                presentLauncher(tries: tries - 1)
            }
        } else {
            // Report the startup failure instead of silently starting the game
            // without files or leaving the loader waiting indefinitely.
            Task { _ = await Bridge.send("launcher-failed") }
        }
        return
    }
    if root.presentedViewController != nil {
        if tries > 0 {
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) {
                presentLauncher(tries: tries - 1)
            }
        } else {
            Task { _ = await Bridge.send("launcher-failed") }
        }
        return
    }
    let host = GameLauncherHostingController(rootView: GameLauncherView(onPlay: { [weak root] in
        guard let root else { return }
        let scene = root.view.window?.windowScene
        root.dismiss(animated: true) {
            updateOrientation(in: scene, to: .landscape)
        }
    }))
    host.view.backgroundColor = .systemBackground
    host.modalPresentationStyle = .fullScreen
    root.present(host, animated: false) {
        updateOrientation(in: host.view.window?.windowScene,
                          to: UIDevice.current.userInterfaceIdiom == .pad ? .all : .portrait)
        // Only Simulator smoke jobs request a small readiness marker. This
        // proves UIKit actually presented SwiftUI, not merely that Rust
        // loaded the framework; no game data or account info is written.
        if ProcessInfo.processInfo.environment["HORIZON_IOS_SIMULATOR_SMOKE"] == "1",
           let caches = FileManager.default.urls(for: .cachesDirectory,
                                                  in: .userDomainMask).first {
            let marker = caches.appendingPathComponent("horizon-ios-launcher-presented")
            try? "presented".write(to: marker, atomically: true, encoding: .utf8)
        }
        NSLog("Horizon iOS SwiftUI launcher presented")
    }
}

@MainActor
private func updateOrientation(in scene: UIWindowScene?, to orientations: UIInterfaceOrientationMask) {
    guard let scene else { return }
    for window in scene.windows {
        window.rootViewController?.setNeedsUpdateOfSupportedInterfaceOrientations()
        window.rootViewController?.presentedViewController?.setNeedsUpdateOfSupportedInterfaceOrientations()
    }
    scene.requestGeometryUpdate(.iOS(interfaceOrientations: orientations)) { error in
        NSLog("Horizon orientation change failed: %@", error.localizedDescription)
    }
}

private final class GameLauncherHostingController: UIHostingController<GameLauncherView> {
    override var supportedInterfaceOrientations: UIInterfaceOrientationMask {
        UIDevice.current.userInterfaceIdiom == .pad ? .all : .portrait
    }
    override var preferredInterfaceOrientationForPresentation: UIInterfaceOrientation {
        .portrait
    }
}
