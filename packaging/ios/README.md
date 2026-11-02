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
  The simulator job writes `Library/Caches/horizon-ios-graphics.json`
  inside the app container and checks for a ready GPU (`state=2`). A valid
  screenshot alone cannot make a failed Metal initialization pass CI.

**An unsigned real-device IPA is now built by default** on pushes to the `ios`
branch and on manual workflow runs (the `build_device` checkbox is enabled by
default). Pull requests still build and smoke-test the Simulator app, without
producing a device IPA. The `device-build` job compiles for `iphoneos` arm64
with code signing disabled, then packages `Payload/HorizonIOS.app` into the
exact file **`horizon-ios-unsigned.ipa`**. It checks the arm64 executable,
unsigned status, bundle structure, permissions, and ZIP integrity before
uploading the artifact `horizon-ios-unsigned-ipa`.

To get it: open [Horizon iOS Build](https://github.com/tlno25/horizon-ios/actions/workflows/ios-simulator.yml),
open a successful run, then download **`horizon-ios-unsigned-ipa`** from
**Artifacts**. Extract GitHub's outer artifact ZIP; the contained file is
`horizon-ios-unsigned.ipa`. The IPA is for actual iPhone/iPad hardware,
**not the Simulator**. It is intentionally **unsigned**, so it cannot be
installed as-is. A separate signing process (for example using your own
Apple ID in an appropriate sideloading tool) is required. No Apple signing
credentials are stored in this repository or Actions workflow.

The IPA currently contains the Horizon shell/Metal integration probe, **not
playable Minecraft**. Minecraft engine loading and gameplay will be added in
later iterations. Neither GitHub nor your Windows/Linux computer needs a
local Xcode installation to produce the unsigned IPA through Actions.

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
- The shared `horizon-client` library passed an iOS Simulator type-check on
  the macOS engine-compatibility workflow. Its startup/render loop is not yet
  linked into the Horizon iOS application; the Linux cross-compile attempt
  could not run Apple's native build tools (`xcrun`).
- The bridge exposes the ABI version, Minecraft version and *shell* status
  only. State `ready` means the shell exists, **not** that the game is running.
- On suspend it drops the GPU probe and updates shell state; on resume it
  recreates the surface and device. Renderer output is not game content.
- The GPU state is `Metal ready` only **after** a WGPU frame is successfully
  submitted and presented; merely creating the Metal device does not mark it
  ready. Resize, suspension, and surface reconfiguration invalidate readiness
  until the next successful presentation.

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

## Shared game engine portability check

The main Simulator workflow builds only the native iOS shell, not the full
Minecraft client. The separate **Horizon iOS Engine Compatibility** workflow
is a manually triggered macOS job that type-checks `horizon-client --lib`
with no optional audio/controllers against the actual iOS SDK. It reports the
next Rust/iOS source incompatibilities without blocking the already working
app build; its compiler log is available as an Actions artifact. The game
remains unintegrated until this check passes **and** its startup/rendering
interface is adapted to the Winit/UIKit shell.

The shared client's iOS dependency graph now selects only Metal. Android,
Windows and Linux retain their original WGPU backends. The native iOS shell
is unaffected by the additional shared-engine portability check.

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
iPhone simulator and uploads a screenshot. Separately it builds and uploads
`horizon-ios-unsigned.ipa` for real iOS devices on `ios` pushes and by default
on manual runs.
The screenshot and a successful `simctl launch` are **smoke evidence**;
CI also checks the native GPU state file and fails if Metal does not initialize
and present. Neither proves correct pixels, background recovery, or iPad behavior. Inspect
the screenshot and manually exercise rotation/background transitions before
accepting the vertical slice.
