# HorizonLauncherKit

One SwiftUI overlay, bundled inside the native Rust game app. `Present.swift`
exposes `horizon_launcher_present` to `horizon-ios-launcher`; `Bridge.swift`
passes asynchronous JSON requests to the game's Winit event loop. The same
Documents sandbox stores downloads, extracted resources, saves and options.

The publisher sets `HorizonResourcesManifestURL` in `GameInfo.plist` to an HTTPS
manifest whose `minecraft`, `url`, `sha256` and `size` fields describe the
**privately authorized** prepared Horizon game ZIP. Users only press Download.
No unverified third-party mirror or Minecraft copyrighted assets are bundled.
