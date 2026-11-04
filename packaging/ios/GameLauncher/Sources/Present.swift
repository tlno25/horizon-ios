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
    let host = UIHostingController(rootView: GameLauncherView(onPlay: {
        root.dismiss(animated: true)
    }))
    host.view.backgroundColor = .systemBackground
    host.modalPresentationStyle = .fullScreen
    root.present(host, animated: false)
}
