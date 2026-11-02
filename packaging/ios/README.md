# Horizon iOS development shell (vertical slice in progress)

This app is the iOS port of the **Horizon Rust engine**, not Pomme/Client.
The SwiftUI home screen is still a shell; it **does not launch playable
Minecraft**. The iOS static library now links the shared `horizon-client`
engine, checks the native resource manifest and initializes the persistent
local player name in the application-support sandbox. A WGPU/Metal integration
probe clears the background blue behind the SwiftUI panel.

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
  inside the app container and checks for a ready GPU (`state=2`) **and** an
  initialized game profile (engine state 1 or 2, nonempty player name). A valid
  screenshot alone cannot hide a failed Metal or profile initialization.

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

The IPA currently contains the Horizon shell, Metal integration probe,
shared client profile, private resource importer and opt-in block registry
loading stage, **not playable Minecraft**. Minecraft engine loading and gameplay will be added in
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
- The SwiftUI shell now offers **Import prepared resources ZIP**. Select a ZIP
  created from the player's legally obtained Minecraft Java Edition files
  using the Horizon resource preparation pipeline (the ZIP must contain
  `horizon-resources.json` directly at its root). The Files picker grants
  temporary access; Swift's `NSFileCoordinator` copies the selected archive to
  a unique temporary file, and a Rust worker extracts it using the existing
  path/symlink/size checks. The source ZIP is never modified. The installer
  rejects duplicates rather than replacing existing installed resources;
  imported files live inside `Application Support/Horizon/.local/horizon-client/<version>/`.
  Importing a manifest does **not** validate all Minecraft assets or start a
  game world. ZIP files are never bundled into the public GitHub IPA.
- **Core game CPU data** now has a separate opt-in loading stage. After
  importing prepared resources, the SwiftUI **Load game block registry**
  action invokes `horizon_ios_load_core_assets()` on a worker. The exact
  `horizon_client::game_startup::load_core_registry()` function is now used
  by both iOS and desktop/Android `game.rs::load_core`, sharing the same pinned
  resource-version, `facts.json` and block-tag validations. It returns an
  explicit success/error status and keeps the loaded registry in the process
  for subsequent game startup. Invalid/missing game files do not count as a
  successful load, even when `horizon-resources.json` exists. The load is
  single-flight; failed attempts may be retried, successful loads cannot
  repeatedly leak registries. **This only prepares CPU game metadata**:
  creating a world and rendering gameplay still requires integration with
  the existing `loading_app` Winit event loop.
- Winit now forwards native UIKit touch phases into the portable Rust
  multitouch recorder. It tracks up to ten active fingers, clamps physical
  coordinates, and cancels outstanding touches when suspended. SwiftUI has
  a **Touch input bridge** diagnostic pad that exercises the same Rust recorder
  from a tap or drag, with active finger and accepted event counters. This is
  **not yet** Minecraft movement, camera look, block interaction or a full
  touch-control overlay. Real Winit touch events are passed through the Rust
  callback, but gameplay bindings require the `loading_app` integration.
- The shared `horizon-client` library passed an iOS Simulator type-check on
  the macOS engine-compatibility workflow and is now linked into the iOS app's
  static library. On UIKit's main thread, Swift supplies the absolute
  `Application Support/Horizon` sandbox home path to Rust, which uses
  `horizon_client::offline_name::load_or_create` to persist the local profile
  and `horizon_client::resource_install::ready` to detect a prepared game
  resource manifest. The event loop and game renderer are **not** started.
- `horizon_ios_engine_state`: 0 = not checked, 1 = game assets missing,
  2 = resource manifest found, 3 = preflight failed. States 1 and 2 mean the
  profile/bootstrap completed, **never** that gameplay is running.
- A manifest alone is not a proof of complete, valid Minecraft assets.
  The native game loader's resource validation remains future work.
- The bridge exposes separate shell, Metal and resource-preparation states;
  none asserts that a Minecraft game world is running.
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

The main Simulator workflow links the shared Rust client library but still
runs only the iOS bootstrap and Metal drawing probe, not gameplay. The separate **Horizon iOS Engine Compatibility** workflow
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

The WGPU/Metal one-frame clear and iOS Simulator run are verified, and the
shared client library now provides native profile/resource preflight. Next:
extract the native game's `loading_app` startup interface from its desktop
binary, then adapt input,
world/renderer and lifecycle events without creating a second UIKit event
loop. Exercise rotation and background recovery on an actual iPhone/iPad.
The Android crate and the `android` branch remain untouched.

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
