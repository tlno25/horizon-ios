# Horizon iOS development shell (vertical slice in progress)

This app is the iOS port of the **Horizon Rust engine**, not Pomme/Client.
The SwiftUI home screen is still a shell; it **does not launch playable
Minecraft**. The iOS static library now links the shared `horizon-client`
engine, checks the native resource manifest and initializes the persistent
local player name in the application-support sandbox. A WGPU/Metal integration
probe clears the background blue behind the SwiftUI panel.

## Building entirely on GitHub Actions (no Mac or Xcode needed locally)

The **Horizon iOS Quick Checks** workflow runs Rust, Python,
C ABI and portability tests automatically on Ubuntu for relevant `ios` pushes.
It does **not** build a macOS/iOS app or use a macOS runner.

To produce an iPhone/iPad app, open
[Horizon iOS Manual Build](https://github.com/tlno25/horizon-ios/actions/workflows/ios-simulator.yml),
select **Run workflow**, choose branch **ios**, and select `build_target`:

- **device** (default): build an unsigned arm64 iPhone/iPad IPA with Xcode.
  Download the `horizon-ios-unsigned-ipa` artifact, extract GitHub's outer ZIP,
  and use the enclosed **`horizon-ios-unsigned.ipa`**. The IPA is **unsigned**;
  signing with your own credentials is required before installing it.
- **simulator**: build x86_64 `HorizonIOS.app`, run the iPhone Simulator,
  verify that Metal presented a frame, and upload a screenshot and diagnostics.
  Download the `horizon-ios-simulator-app` artifact containing the Simulator
  `.app.zip`. A Simulator binary cannot run on a physical iPhone.
- **both**: build both artifacts. This consumes more macOS runner minutes and
  should be used only when both are needed.

Enable `renderer_probe` only when investigating the experimental **real Horizon
GPU boot renderer**. The Simulator then checks the shared Horizon pipeline
and fails if it cannot present a frame. This diagnostic does **not** load a
Minecraft world. The ordinary Metal smoke test remains available without it.

The manual workflow builds using `macos-15-intel` and cross-compiles to the
selected iOS architecture. Xcode Debug uses Cargo's `fast` profile to reduce
link and compilation time. Packages and screenshots are retained for 7 days.
**No macOS build is triggered automatically by a push or pull request.**
The separate **Horizon iOS Engine Compatibility** check is also manual-only.

The IPA currently contains the Horizon shell, Metal probe, shared-client
profile, private resource importer, block-registry loader and an optional
shared Horizon renderer preview. After the block registry is loaded, the
preview draws the **existing native Horizon title screen** with its prepared
GUI atlas, buttons and panorama using the shared GPU renderer, paced to about
30 fps while foregrounded. The controls are *visual only* at this stage;
iOS real Winit/UIKit touches can now activate **Singleplayer** and the
**Back/Cancel** buttons through the existing native button hit-testing.
In the Select World screen, tapping a row focuses it without joining even on
repeated taps or the row icon. A vertical swipe scrolls the world list using
the existing native scroll bounds.
The native **Play Selected World** button now initiates a **read-only launch
preflight**, not gameplay. A background Rust worker validates the selected
save folder, rejects path traversal, symlinked worlds and incompatible or
upgrade-required saves, and reads metadata through the shared `Level` parser.
The result appears in the SwiftUI diagnostics (automatically shown after the
tap). Read-only inspection now also stages the saved player feet position,
rotation and corresponding initial chunk coordinate (including negative
coordinates) when the saved data contains valid, finite coordinates. For the
vanilla overworld, it now opens the corresponding existing Anvil `.mca` file
read-only and decodes **one saved chunk NBT record**. It now uses Horizon's
existing `ChunkData` storage decoder to validate the saved section layout and
sample the palette-resolved block directly beneath the saved player's feet
(if that section is present). The diagnostic also reports the saved data
version and section count. Linked region files and linked external chunk
payloads are rejected. After the core block registry is loaded, the optional resident preflight
also uses Horizon's existing `save::decode_with` to build the full in-memory
block/biome column for this one saved chunk. It reports the registry's numeric
block-state ID below the player, numeric biome ID, resident section count and
approximate section-memory usage. After validation, the Winit shell now
retains **at most one decoded initial column** in Rust memory for the next
terrain stage. The column is cleared on a new world inspection, title
navigation, or suspension. It is not yet meshed, sent to Metal, or modified
on disk. This keeps native-Rust ownership of the game's data, as seen in the
separate [heyskylo/horizon-ios](https://github.com/heyskylo/horizon-ios)
experiment, without copying its app entry point or implementation.
Custom world biome packs are not yet resolved during this preflight, so IDs
are based on the bundled vanilla biome registry; custom-biome mappings must
be integrated before actual gameplay. This does **not** generate missing
chunks, take a session lock, modify `level.dat` or start world simulation.
Other dimensions and worlds without a saved position await a different
initial loading path. Remaining gameplay actions stay disabled.
Use **Show game** to hide the full-screen diagnostics panel and touch the
renderer; the small **Diagnostics** button restores the panel. Taps are
rejected after dragging, cancellation or backgrounding. The resource-free CI
probe continues to render the lightweight loading frame without private
Minecraft files. This remains **not playable Minecraft**. Windows/Linux
computers do not need local Xcode to download the unsigned IPA.

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
- **Responsive Horizon renderer preparation**: the opt-in preview now
  compiles the real Horizon boot renderer's WGPU/Metal pipelines **offscreen on
  a worker**, keeping UIKit and the existing Metal probe responsive instead
  of synchronously blocking the iOS event loop. The worker notifies the one
  Winit loop with a distinct `RendererCompleted` event; only that loop may
  detach the probe, validate the prepared surface format and attach the live
  window surface. Backgrounded/stale results are discarded. Preparation and
  surface/first-frame failures are recorded, with Metal-probe fallback.
  Rust panic payloads from GPU validation and iOS-specific calls are now
  retained in the `error` JSON property (rather than reduced to a generic
  "GPU worker panicked" string). The last actual GPU stage remains available
  in `renderer_stage`, and the RendererCompleted callback does not discard a
  notification just because the worker is returning from its function.
  This is still a GPU boot-screen diagnostic, **not a running game world**.
- **Horizon renderer preview** is another opt-in, after successful game
  registry loading. The desktop/Android boot-renderer constructor and empty
  loading frame now live in `horizon_client::game_startup`, and the iOS shell
  can replace its working blue Metal probe with these **real Horizon GPU
  pipelines on the same Winit window and event loop**. The switch is
  single-flight and only releases the probe's surface after offscreen
  pipeline preparation has completed. If creation or rendering fails, it reports the error and attempts
  to restore the probe. Background/suspend also drops the renderer and returns
  to the probe on resume. No game world, engine ticks, or interactive title
  have been attached; this is a renderer integration diagnostic, not gameplay.
  The existing Simulator and device IPA workflows keep using the stable
  Metal probe by default.
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

### CI cost controls and manual builds

There are now two independent iOS workflows to avoid consuming macOS runner
minutes after every small change:

- **Horizon iOS Quick Checks** (`ios-quick-checks.yml`) runs the Rust bridge,
  portability, regression, C ABI and Python packaging checks on **Ubuntu**
  for relevant pull requests and `ios` pushes. It cancels an older in-progress
  quick check when newer changes arrive on the same PR/branch. A single Rust
  dependency cache is warmed on trusted `ios` pushes or manual runs.
- **Horizon iOS Manual Build** (`ios-simulator.yml`) runs **only** from the
  GitHub Actions **Run workflow** button, with a branch and target selected.
  Its `build_target` input is `device` (default), `simulator`, or `both`.
  Select `device` to produce the artifact `horizon-ios-unsigned-ipa` containing
  `horizon-ios-unsigned.ipa` for a real arm64 iPhone or iPad. Select
  `simulator` to build, launch, screenshot and check the Metal probe in the
  x86_64 Simulator. The `renderer_probe` checkbox additionally attempts the
  experimental **real Horizon GPU boot renderer** (Simulator only). When
  requested, failure **fails the job** and captures diagnostic status/logs.
  A successful normal Metal probe does **not** imply that real Horizon's GPU
  pipelines work. Neither diagnostic runs Minecraft gameplay.

The macOS build uses `macos-15-intel` for both targets, cross-compiling arm64
for device. Caches retain compiled Rust dependencies per job/architecture;
GitHub's Actions cache quota and evictions still apply. The Xcode **Debug**
configuration now uses the repository's Cargo `fast` profile (release-derived,
without thin LTO and with parallel code generation) and links its `fast`
static library. Xcode **Release** still uses the fully optimized Cargo
`release` profile. Build success and total runner time should be measured
on the next macOS run; cached builds are not guaranteed to be faster.

The Simulator job now reuses the already booted iPhone simulator for the
experimental GPU probe instead of shutting it down and booting it again. The
regular Metal test has a 6-minute step limit, the optional GPU probe has a
separate 5-minute limit, and the overall Simulator job has a 35-minute cap.
This prevents a stalled `simctl bootstatus` from consuming the previous
45-minute job timeout. Experimental probe failures remain visible as failures;
no green build is inferred from the ordinary Metal screenshot.

Both macOS jobs have explicit timeouts and upload artifacts for **7 days**.
Do not dispatch `both` unless both packages are needed. Manual builds in the
same branch are serialized instead of cancelling one that already spent runner
minutes. The separate **Horizon iOS Engine Compatibility** macOS workflow
remains manual-only.

Simulator diagnostics use a one-shot
`Library/Caches/horizon-ios-renderer-smoke.flag` inside the simulator's app
container and the `SIMCTL_CHILD_HORIZON_IOS_RENDERER_SMOKE=1` environment
variable to opt in. The app records `smoke_mode`, `renderer_stage`, errors and
`renderer_preview` in JSON. `renderer_preview=3` proves a real Horizon boot
frame was presented; `4` indicates a failed attempt. See the uploaded
`horizon-ios-boot-renderer-status` artifact for the captured evidence. The
normal screenshot and successful `simctl launch` alone do **not** prove
correct game pixels, multitouch gameplay, background recovery or iPad support.
