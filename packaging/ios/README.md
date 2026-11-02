# Horizon iOS development shell (vertical slice in progress)

This app is the iOS port of the **Horizon Rust engine**, not Pomme/Client.
The current SwiftUI home screen is only a shell. It **does not launch
Minecraft**. A small experimental WGPU/Metal integration probe clears the
background blue, behind the translucent SwiftUI panel.

## Building entirely on GitHub Actions (no Mac or Xcode needed locally)

GitHub-hosted `macos-15` runs on Apple Silicon and includes Xcode. Once
`.github/workflows/ios-simulator.yml` has been committed and pushed to the
**default `ios` branch**, open the GitHub repository's **Actions** tab,
choose **Horizon iOS Build**, and select **Run workflow** / branch **ios**.
The workflow also runs automatically for matching changes pushed to `ios` and
on pull requests that change the iOS project.

The `simulator-build` job runs Xcode and attempts to launch the app in an iPhone
Simulator. Regardless of whether the simulator smoke step passes, the build
artifact is uploaded immediately after a **successful build**. On the workflow
run page, open the **Artifacts** section and download:

- `horizon-ios-simulator-app`: ZIP containing `HorizonIOS-simulator-unsigned.zip`
  (an unsigned `HorizonIOS.app` for **arm64 iOS Simulator only**, not a real
  iPhone/iPad). Extract the nested ZIP to inspect/use the app bundle on a Mac.
- `horizon-ios-simulator-preview`: optional screenshot from the Simulator run.
  A successful screenshot does not prove that the Metal output is correct.

To also compile a native iPhone/iPad build, manually run the workflow with
**Also build an unsigned iPhone/iPad app** enabled. It uploads
`horizon-ios-device-unsigned-app`. This ZIP contains the device-target `.app`,
**not an installable IPA**. To install on a physical iPhone or distribute it,
Apple code signing (certificate, provisioning profile and an appropriate
account) must be configured separately. Do not commit signing credentials.

Since the workflow exists locally but is not yet pushed, GitHub Actions cannot
run it until it is published to the remote repository. Neither GitHub nor the
local Windows/Linux computer needs Xcode installed for the Actions build.

## Build on macOS

- Xcode with the iOS 16+ SDK and an installed iOS Simulator runtime.
- Rust toolchain 1.95.0 (`rust-toolchain.toml`) and the matching iOS target:
  `rustup target add aarch64-apple-ios-sim aarch64-apple-ios`.
- Open `packaging/ios/HorizonIOS.xcodeproj` and select an arm64 iPhone/iPad
  simulator. Build using the `HorizonIOS` scheme. Xcode first builds
  `crates/horizon-ios` as a Rust static library and links it into the app.
- For the CLI build, use:

```sh
xcodebuild -project packaging/ios/HorizonIOS.xcodeproj \
  -scheme HorizonIOS -destination 'generic/platform=iOS Simulator' \
  CODE_SIGNING_ALLOWED=NO ARCHS=arm64 build
```

The app's bundle ID (`com.example.horizonios`) is a placeholder for local
Simulator builds. Replace it and configure code signing before device testing.

## Ownership and status

- `main.m` enters `horizon_ios_run()` on the main thread. **Do not add a
  SwiftUI `@main`**: Winit 0.30.13 calls `UIApplicationMain` itself.
- On Winit's `resumed` callback, Rust creates one UIKit window and passes
  its *borrowed* view-controller pointer to `horizon_ios_mount_home()`.
- Swift embeds `HomeView` in Winit's controller using `UIHostingController`.
  The Metal layer is created before the SwiftUI overlay is mounted so the
  opaque GPU sublayer cannot cover the native UI. On resume the overlay is
  brought to the front again.
- The shared `horizon-client-input` clipboard uses iOS UIKit callbacks via a
  bounded C ABI buffer. UI-thread calls use `UIPasteboard`; callers off the
  main thread use the existing in-memory fallback, not unsafe UIKit access.
  Pasteboard behavior and privacy prompts still require Simulator/device tests.
- The shared input crate is now cross-checkable for iOS: desktop `arboard`
  is not resolved, and iOS uses the same physical-key fallback as Android
  because Winit does not expose `key_without_modifiers` on either platform.
- A full `horizon-client` iOS cross-compile is **not** yet verified: it was
  attempted on Linux and stopped in `ring`'s build script, which requires
  Apple's SDK and `xcrun`. Validate that target in the macOS engine-porting
  phase rather than treating a Linux compiler failure as a source incompatibility.
- The bridge exposes the ABI version, Minecraft version and *shell* status
  only. State `ready` means the shell exists, **not** that the game is running.
- On suspend it drops the GPU probe and updates shell state; on resume it
  recreates the surface and device. Renderer output is not game content.

The lifecycle/window ownership and Swift callback must be exercised in a real
Simulator before they are considered validated. The Linux iOS cross-target
`cargo check` checks Rust types only; it does **not** link or execute an app.

## Linux validation

```sh
cargo test --locked -p horizon-ios
cargo check --locked -p horizon-ios --target aarch64-apple-ios-sim
packaging/ios/tests/run_ffi_smoke.sh
packaging/ios/tests/check_portability.sh
```

## Next milestone

Run the WGPU/Metal surface creation, one-frame clear, resize and
foreground/background recovery probe in the iOS Simulator. Then confirm that
the Winit GPU view and SwiftUI host can coexist on iPhone and iPad without
duplicate UIKit lifecycles. Only after validation should the shared Rust game
start interface be extracted. Android crates and the `android` branch remain
untouched by this shell work.

### Continuous integration

The GitHub workflow `ios-simulator.yml` builds on a macOS Apple Silicon
runner, uploads the Simulator `.app.zip`, then launches `HorizonIOS` in an
iPhone simulator and uploads a screenshot. Optionally it also builds an
unsigned iOS device-target `.app.zip` on manual dispatch.
The screenshot and a successful `simctl launch` are **smoke evidence**, not
proof of correct Metal pixels, background recovery, or iPad behavior. Inspect
the screenshot and manually exercise rotation/background transitions before
accepting the vertical slice.
