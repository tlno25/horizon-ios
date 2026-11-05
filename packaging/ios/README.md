# Horizon iOS development shell (vertical slice in progress)

## iPhone shows "Horizon on iPhone needs game files" instead of SwiftUI

That black screen is from **an older or incomplete native game build** that
cannot find the `HorizonLauncherKit.framework` launcher. A modern bundled
game must show its SwiftUI launcher **before** any Minecraft files have been
installed. Copying `horizon-resources.json` into Documents is **not the fix**
for a missing launcher. As of the launcher-only native iOS builds, that
entire Metal fallback screen is excluded from the iOS binary; a missing or
broken framework causes an explicit logged launch failure instead of taking
the user to the old screen. An *already installed* older app must still be
replaced with the freshly signed IPA to receive this code change.

1. Open [Horizon iOS Manual Build](https://github.com/tlno25/horizon-ios/actions/workflows/ios-simulator.yml).
   Select branch `ios`, target **`game`** (now the default), and use a
   successful run. Or use an existing successful `game` run with the latest
   code you want to install.
2. Download the artifact **`horizon-ios-native-game-ipa`**. GitHub gives an
   outer ZIP; extract **`Horizon-game-unsigned.ipa`** from it. Do **not**
   install `horizon-ios-unsigned.ipa` / the legacy diagnostic-shell artifact.
3. Sign the **entire** app with an authorized signing method. The nested
   `Frameworks/HorizonLauncherKit.framework` **must remain present, be signed
   correctly, and retain its iPhoneOS arm64 binary**. An unsigned IPA cannot
   be installed directly on a standard iPhone. Reinstall the signed game app.
   Starting with version **0.2.0**, the home-screen name is **Horizon**
   (`CFBundleDisplayName`); the bundle identifier stays
   `com.example.horizonios.game` so an existing game install can be updated.
   Each CI build also uses an increasing `CFBundleVersion` (Actions run
   number). The IPA archive filename still contains `game` to distinguish it
   from the separate legacy diagnostics IPA.
4. The expected app is named **Horizon** and has bundle ID
   `com.example.horizonios.game`. It should open the SwiftUI native launcher
   first, including when no personal game data exists. If you need to delete
   an older installation, back up any existing Documents/game saves first;
   deleting an app can remove its sandbox data.

**Important:** showing the launcher is not the same as starting Minecraft.
The repository has no configured `HORIZON_MSA_CLIENT_ID` or
`HORIZON_IOS_RESOURCES_MANIFEST_URL`; real Microsoft sign-in, lawful game-file
preparation and actual iPhone gameplay remain incomplete. For debugging, the
latest native-game Simulator run tests the SwiftUI bridge and resource importer
using synthetic files, not proprietary Minecraft files.

## SwiftUI launcher (existing development shell)

The `device` build now opens a **SwiftUI launcher** in the original HorizonIOS
shell. It can import an archive from Files or download a privately prepared
resource ZIP over HTTPS, with live byte progress, cancellation, a 2 GiB transfer
limit, mandatory SHA-256 verification, and the existing Rust ZIP extractor's
path/symlink/unpacked-size checks. The native importer now checks required
files and Minecraft version **before** installing a downloaded or Files ZIP;
a partial archive does not block retry. Redirects to insecure HTTP are refused.

The download address is intentionally **not** a public Mojang asset mirror. For
this milestone the user prepares resources on Windows using their own licensed
Minecraft Java Edition (see below), creates a ZIP with the manifest at its root,
keeps it in private HTTPS storage, and provides its exact URL and SHA-256:

```powershell
Get-FileHash -Algorithm SHA256 .\Horizon-iOS-prepared.zip
```

The SwiftUI settings sheet writes Horizon's actual `options.txt` format via the
shared Rust parser, rather than inventing a second options format. It currently
exposes render and simulation distance, FPS cap, FOV, VSync, auto-jump, master
volume and offline name. Edits are validated and existing unknown lines are
preserved. Restart the shell to load changes.

**Important limitation:** the SwiftUI launcher lives in the diagnostic shell's
app sandbox. The native-game `game` build is currently another app with a
**different sandbox and executable**. Installing resources or changing settings
in the shell does **not** make the separate HorizonGame IPA ready to play.
A single bundled launcher + real-game event loop, Microsoft sign-in / ownership
verification, automatic private resource provisioning, and device gameplay
are subsequent integration tasks. The launcher doesn't claim to start Minecraft
or distribute Mojang files. The game IPA remains unsigned.

## Single-app SwiftUI launcher and one-tap resource install (experimental)

The `build_target=game` **native game IPA** now embeds
`HorizonLauncherKit.framework` next to the Rust `HorizonGame` executable.
Unlike the older diagnostic shell, this framework uses the **game's own**
Documents sandbox, so an installation and game startup share the same files.
The Rust Winit event loop presents the SwiftUI screen before initializing
Metal or loading Minecraft resources. After pressing Download, the launcher:

1. Fetches the app publisher's HTTPS manifest, validates the required
   Minecraft version, ZIP size (up to 2 GiB), and SHA-256 checksum.
2. Downloads the prepared ZIP with progress and cancellation, checks its
   complete byte size and SHA-256, then asks the Rust game to import it.
3. Rust extracts the ZIP into a staging directory, verifies core resources,
   and atomically installs it before the SwiftUI launcher requests game start.
4. The launcher asks the native game loader to start in the **same UIWindow
   and sandbox**. No second install or folder manipulation is required from
   the end user in a publisher-configured build. This runtime handoff still
   needs iPhone and Simulator validation.

### User-hosted HTTPS ZIP fallback and interrupted-download recovery

As in the heyskylo/horizon-ios reference launcher, the native SwiftUI game
launcher now also exposes an **optional personal HTTPS ZIP link** with an
optional SHA-256 field. When a publisher has not set a default manifest URL,
this link form opens automatically instead of leaving a broken Download
button. A player can retrieve an archive they legally prepared and host
privately, without importing or moving files in iOS Files.app.

Both configured one-tap downloads and private-link downloads use the same
URLSessionDownloadTask, the 2 GiB limit, and Rust's checked installer.
A publisher-configured manifest requires an exact SHA-256 and byte count;
a personal link may omit the checksum but then HTTPS and the Rust ZIP
structure/version/symlink/path checks are the only integrity safeguards.
Do not install ZIPs from sources you do not trust.

When a server supports HTTP byte ranges and ETag/Last-Modified, iOS can
return download resume data on an interrupted or canceled transfer. The
launcher saves this in its private cache, keyed to the exact URL,
checksum and expected length, and uses it on the next attempt.
A file that has been validated and installed clears that cache.
The transfer requests bounded iOS background execution time and saves
resume information if the background time expires. This is **not** a claim
that a transfer will continue indefinitely while the app is suspended,
and servers without Range support may require a fresh download.

The third-party URL found in the reference implementation is intentionally
**not embedded or trusted as a default Minecraft asset provider**. Public
redistribution of proprietary game files requires appropriate rights.
A publisher-configured build still offers the cleanest one-button
experience without any user link input.

**Publisher setup is still required:** Set the Actions repository variable
`HORIZON_IOS_RESOURCES_MANIFEST_URL` to an HTTPS JSON file you are authorized
to distribute. The release workflow writes it into the generated game's
`Info.plist` as `HorizonResourcesManifestURL`. It must contain:

```json
{
  "minecraft": "26.3",
  "url": "https://your-authorized-server.example/horizon-26.3.zip",
  "sha256": "<64-hex-character archive SHA-256>",
  "size": 123456789
}
```

For an **authorized** publisher archive, `packaging/ios/create_resource_download_manifest.py`
can generate these values automatically, checking the game's preparation
manifest and the required file structure. Example (URLs and file names are
illustrative, not an existing public asset server):

```bash
python3 packaging/ios/create_resource_download_manifest.py \
  --archive /path/to/authorized-prepared-resources.zip \
  --minecraft-version 26.3 \
  --url https://your-authorized-server.example/horizon-26.3.zip \
  --output game-files.json
```

The publisher must host that exact archive and `game-files.json` over HTTPS,
then set `HORIZON_IOS_RESOURCES_MANIFEST_URL` in GitHub Actions repository
variables to the **public HTTPS address of `game-files.json`**, and build the
`game` target. No folder selection, URL entry or checksum entry is required of
the player. This helper never uploads files or supplies a third-party mirror.

Use a **genuine hash and byte size** from the ZIP actually served. The ZIP
needs the core files that `horizon-prepare` generates (root
`horizon-resources.json` and the supporting files). Do not upload or publicly
redistribute Mojang's proprietary assets unless you have authorization.
There is **no verified publisher manifest configured in this repository**;
without one, Download reports the missing configuration instead of guessing
an untrusted mirror. That is a distributor setup issue, **not a step required
of the player** after a correctly configured app is installed.

For fast compiler feedback on Windows/Linux development, manually select
`build_target=game-launcher` in the `Horizon iOS Manual Build` workflow. This
compiles the SwiftUI framework alone on the macOS runner and checks the C ABI
entrypoint, without rebuilding the expensive Rust client. The `game` and
`game-simulator` targets remain the actual integrated binary checks.

The native Simulator smoke test verifies more than SwiftUI presentation: in
`HORIZON_IOS_SIMULATOR_SMOKE=1` mode, it asks Rust for the status, game settings
and offline player name, confirms it refuses to start with no valid game files,
then saves and reads back render distance 8. The resulting cache marker is
CI-only and contains no player information or game assets. This catches broken
Swift-to-Rust callbacks even if the launcher looks correct in a screenshot.
The Simulator smoke also generates a tiny **synthetic, non-playable ZIP** in
the simulator app's `Documents/` directory; SwiftUI calls
`launcher-import horizon-download-CIRESOURCE.zip` and Rust verifies the
structure/version and installs it. A second marker is written only if the
installed resources become `ready`. The smoke then sends a second
`launcher-start` **without any Minecraft account**, and requires Rust to
reject it with the ownership prompt while leaving the native launcher in its
waiting state. A third CI-only marker confirms the result; the smoke fails if
that ownership gate is bypassed or unresponsive. Neither test ZIP nor marker
is embedded in IPA artifacts, and the test intentionally does not start
gameplay using fake Minecraft data. Actual licensed assets and gameplay
remain unverified.

The SwiftUI screen now displays in portrait on iPhone (all orientations on
iPad), then requests landscape when Play begins. The underlying Winit window
changes its supported orientation mask to match. Settings expose the game's
options.txt format and the persistent 3–16 character offline player name; this
name is **not** a Microsoft login and does not prove game ownership.

The native game currently has no iOS text-to-speech narrator backend, so it
reports the narrator as **unavailable** rather than trying to spawn an
unsupported platform backend and writing an ERROR at every successful game
startup. This does not disable sound effects, music or the iOS `AVAudioSession`
used for game audio. Narration is still a future accessibility task.

The SwiftUI screen offers a limited subset of Horizon's game settings. Full
account sign-in, entitlement verification, legal provisioning of Java Edition
game data, all gameplay features and on-device performance are **not yet
verified**. The signed, combined IPA still needs a legitimate Apple signing
process to install on an actual iPhone. The `device` target continues to
produce the separate diagnostic shell; use `game` for this integrated app.

## Experimental native game executable (separate IPA)

The manual GitHub Actions **Horizon iOS Manual Build** workflow now also offers
`build_target=game`. It compiles the **actual Horizon Rust game binary**
(`horizon-client`) for iPhoneOS arm64 and uploads **horizon-ios-native-game-ipa**
containing `Horizon-game-unsigned.ipa`. This is separate from the existing
`device` option, which continues to output the original SwiftUI/Metal
diagnostic shell `horizon-ios-unsigned.ipa`. The `both` option retains its
existing meaning (device shell + Simulator shell). None of the expensive macOS
workflows run automatically after each push.

The same manual workflow also offers `build_target=game-simulator`.
This separately compiles **the real game executable** for an Apple Silicon
iPhone Simulator (`aarch64-apple-ios-sim`) on macOS arm64, uploads
`horizon-ios-game-simulator-app`, then attempts a boot/install/launch with
no prepared Minecraft resources. It uploads an image and native logs in
`horizon-ios-game-simulator-diagnostics` for inspection. A successful launch
only demonstrates a Simulator app startup, **not** playable worlds or a
correct Metal frame; examine the screenshot and logs before claiming more.
This intentionally costs another manual macOS build and is not part of
`build_target=both`.

The native game uses Horizon's existing `loading_app` and gameplay code
rather than a second, rewritten SwiftUI simulation. It enters its **own**
writable app sandbox `Documents/` before loading resources, settings or saves.
Use iOS file sharing to copy the **legally prepared** Horizon resource tree
into the game's **Documents/resources/** with `horizon-resources.json` directly
inside that `resources` directory. An older import into
`Documents/.local/horizon-client/<game-version>/` is accepted as fallback.
The resources imported into the diagnostic SwiftUI shell's Application Support
folder are **not shared** with this distinct bundle ID. The build never
includes Minecraft assets; they must come from the player's own Java Edition.
Native-game runtime logs and crash reports are written by default under
`Documents/logs/latest.log` and `Documents/crash-reports/`, accessible from
Files or Finder/iOS file sharing for troubleshooting; explicitly configured
`--home` and `HORIZON_HOME` still take precedence. Treat these files as
private, since crash diagnostics may contain server or local file details.

### Preparing your own resources on Windows and transferring to iPhone

On iPhone, the native game performs a lightweight **preflight before the
resource loader starts**. It checks that `horizon-resources.json` is readable,
contains a preparation schema for the expected Minecraft version, and that
the four core JSON files (`facts.json`, `block-tags.json`, `registries.json`,
`commands.json`) and nonempty font/texture directories are present. If the
folder is only partly transferred, the first-run screen names the missing
file instead of trying to load a broken world. This does **not** check hashes,
verify every resource, or guarantee playable worlds: run
`horizon-prepare --check` on Windows before copying the archive.

The native game IPA cannot include the proprietary Minecraft game files.
From Windows 11, install the official Minecraft: Java Edition **26.3** with
its launcher and run it once. You also need Rust, JDK **25** and the matching
26.3 server JAR obtained from your own licensed Minecraft installation.
In PowerShell, from a local clone of this project:

```powershell
cargo run --release -p horizon-prepare -- --prepare --server-jar "$env:USERPROFILE\Downloads\server.jar"
cargo run --release -p horizon-prepare -- --check
```

Replace the server JAR path with your actual copy. The importer produces
`.local\horizon-client\26.3\` on your PC. To transfer it as a single file:

```powershell
tar -a -cf Horizon-iOS-prepared.zip -C ".local\horizon-client\26.3" .
```

**Keep this ZIP private**; it contains game files prepared from your
own Minecraft copy, not open-source Horizon code. Transfer it through the
iPhone Files app, iCloud Drive or iOS file sharing; extract it in Files
and make the resulting folder **`On My iPhone > Horizon > resources`**.
The `horizon-resources.json` file must be immediately
inside `resources/`, not inside another nested ZIP folder. Completely quit
and reopen the iOS app. This transfer path has not yet been validated on a
physical iPhone. The separate SwiftUI shell has its own ZIP importer.

**Do not mistake this build target for a finished, playable mobile client.**
The modern game app shows its bundled SwiftUI launcher when prepared resources
are missing. The old native first-run screen is excluded from the iOS game
binary. The loading/title stage requests the native UIKit keyboard when a text
field receives focus; committed Unicode input uses Horizon's existing title
edit path. Exact keyboard and orientation behavior still need physical iPhone
verification.

The native game and the loading window now request the device's own
landscape viewport instead of a desktop-sized (1280×720 point) UIKit window.
The **initial loading/title menu** uses Horizon's existing touch gesture
recognizer for tap-on-release and scroll, preventing drag-to-open errors.
The game window now also routes native iOS touches through the **existing
Horizon touch pad** (movement, look, jump, sneak, attack/use, hotbar and
pause/inventory), drawing its existing HUD and retaining the user's touch
control layout. App suspension releases held inputs and suspends its GPU
surface; resume recreates the surface. These iOS paths still require
on-device verification, especially menu text entry and multiplayer. Account
sign-in, advanced native text-entry UX, audio output and actual world gameplay
remain incomplete or unverified. This native game IPA is a feasibility/compile
milestone, not a claim that worlds can be played on iPhone. The IPA is also
**unsigned** and must be signed with a valid development/sideloading identity
before installation. The bundle identifier `com.example.horizonios.game` is
a development placeholder. The original shell and the native game can be
installed side by side (distinct bundle IDs).

This app is the iOS port of the **Horizon Rust engine**, not Pomme/Client.
The SwiftUI home screen is still a shell; it **does not launch playable
Minecraft**. The iOS static library now links the shared `horizon-client`
engine, checks the native resource manifest and initializes the persistent
local player name in the application-support sandbox. A WGPU/Metal integration
probe clears the background blue behind the SwiftUI panel.

## Building entirely on GitHub Actions (no Mac or Xcode needed locally)

The **Horizon iOS Quick Checks** workflow runs Rust, Python,
C ABI and portability tests automatically on Ubuntu for relevant `ios` pushes.

### Native controllers and game audio (experimental)

The native-game target now reads Apple **GameController** snapshots on iPhone
and iPad even when built with `--no-default-features --features audio` (the
existing iPhoneOS game CI flags). Xbox, DualSense and compatible controllers
feed their face buttons, shoulders, D-pad, sticks and triggers into Horizon's
existing `PadState`, `keybinds` and movement logic; there is no second input
engine. Touch and controller input merge through the same pad processing path.
No native haptics, rumble or adaptive triggers are claimed on iOS. Physical
controller pairing, input latency and button layouts still require device tests.

The SwiftUI game overlay also activates an iOS `AVAudioSession` when Play
succeeds, using `.playback` and `.mixWithOthers` so the device's silent switch
does not mute game audio and other audio is not forcibly interrupted. It
handles interruption-ended notifications and deactivates the session when the
launcher is presented again. This is session policy, not proof that every
CoreAudio output device or Bluetooth routing scenario has been validated.
Both improvements adapt the MIT-licensed reference implementation from
[heyskylo/horizon-ios](https://github.com/heyskylo/horizon-ios).

Swift audio/controller integration is **not** verified by the Linux Python or
Rust tests alone. Rebuild the `game`/`game-simulator` manual workflows on macOS
and test a signed build on an iPhone before calling it supported.

It does **not** build a macOS/iOS app or use a macOS runner.

To produce an iPhone/iPad app, open
[Horizon iOS Manual Build](https://github.com/tlno25/horizon-ios/actions/workflows/ios-simulator.yml),
select **Run workflow**, choose the **ios** branch, and select
`build_target`. The experimental native-game build targets were merged into
`ios` in PR #2. To validate the current game, choose `game` for iPhoneOS or
`game-simulator` for the Apple Silicon Simulator. Neither target bundles
proprietary Minecraft data, and an unsigned IPA cannot be installed without
signing.


- **game** (default): build the real Rust game and embedded SwiftUI launcher.
  Download `horizon-ios-native-game-ipa`, extract `Horizon-game-unsigned.ipa`,
  and sign the app and nested framework before installing.
- **device** (legacy diagnostics only): build the separate iOS shell with Xcode.
  This produces `horizon-ios-diagnostic-shell-ipa` containing
  `horizon-ios-unsigned.ipa`. It is **not** the native Minecraft game app.
- **simulator**: build and upload an x86_64 `HorizonIOS.app` without booting
  Simulator by default. This **compile-only** option validates Xcode, Swift
  and iOS-target Rust while avoiding costly `simctl bootstatus` stalls. Enable
  `simulator_smoke` to boot iPhone Simulator, verify Metal presentation and
  capture a screenshot. `renderer_probe` also enables the basic smoke and then
  tests Horizon's experimental native boot renderer. Download the
  `horizon-ios-simulator-app` `.app.zip` artifact; it cannot run on a real iPhone.
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
navigation, or suspension. On a player-triggered preflight with prepared
resources, the same background worker now:

1. Loads and caches the block-model atlas, visibility/meshing tables, and
   **native collision shape registry** **on demand** (not at title startup).
2. Reads up to eight existing neighbouring Anvil columns read-only, without
   generating any missing chunks, to preserve native face occlusion.
3. Runs Horizon's shared `Mesher` over non-air sections of **up to nine saved
   columns** (centre, cardinal neighbours, diagonals). Every section has its
   own `(chunkX, sectionY, chunkZ)` GPU upload key, and the **combined**
   1,048,576-vertex safety budget (about 24 MiB of vertices) is respected.
   The saved centre column is prioritized; if a further column does not fit,
   that column and subsequent neighbours are skipped rather than partially
   displayed. No missing columns are generated.
4. Builds a separate **read-only native `World`**, backed by the same Arc
   chunks. It uses Horizon's canonical collision shapes and native movement
   resolver against the standard standing-player body (0.6×1.8 blocks) to
   constrain the temporary preview camera. Missing chunks remain absent and
   no world tick, block mutation, lock or save writer starts.
5. Reports quad counts, visible sections, included columns, columns skipped
   by the memory cap and native collision snapshot size. The CPU vertices and
   resident chunks are released on selection changes, navigation or suspension.

The **Preview saved terrain** action in SwiftUI now makes this an explicit
opt-in GPU preview instead of only a CPU diagnostic. After the saved world
preflight finishes, use Diagnostics to start the preview. The background worker
builds a second native Horizon renderer with the full block sprite atlas,
**sharing the existing title renderer's GPU device**; UIKit/Winit continues to
draw the title until preparation finishes. UIKit then transfers the one live
surface to the textured renderer and uploads the prepared section meshes with
`Renderer::upload`. `terrain_preview_frame` points its camera at the saved
player position. The preview automatically hides the diagnostics overlay so
the player can see the scene. **Drag one finger** to look around the static
area. **Drag two fingers** together to pan horizontally, entering only nearby
columns for which the CPU mesher actually produced geometry. **Pinch two fingers**
to raise or lower the preview camera within 16 blocks of the saved height;
three or more fingers cancel the gesture. A bounded 3×3 saved region is the
largest preview; missing or truncated columns are not traversable.
With **Enable physics preview**, the same saved snapshot now accepts a
**native UIKit thumbstick** in the lower left and a Jump button in the lower
right, visible only when diagnostics are hidden. The stick sends normalized
camera-relative left/forward axes to Winit, which samples them on its native
20 TPS `FixedStep` ticks. The dry-ground movement impulse comes from Horizon's
shared `living_air::input_vector`, with bounded acceleration and diagonal
normalization. The preview now loads the prepared `movement-properties.nbt`
once on world selection, samples the saved support block and applies the
native friction-dependent ground speed and drag (e.g. ice versus stone).
Grounded movement uses the existing `SceneCollisions::resolve` step-up
selection with a **0.6-block maximum**: a half slab can be stepped onto,
while full block walls and airborne movement cannot step. The next native
`Entity.move` phase also applies `movement_support::block_speed_factor` to
horizontal velocity (e.g. soul sand), and jumping delegates to the native
block-specific `block_jump_factor` (e.g. honey block). An optional **Sprint**
toggle in Diagnostics or above Jump in the game view enables the vanilla
30% movement-speed modifier only for sufficiently forward joystick input;
`living_motion::jump_from_ground` adds the vanilla forward sprint impulse.
Sprint is transient and automatically cleared on leaving the preview or
backgrounding. Collision sampling, physics ticks, and GPU camera movement
never mutate world blocks.
Its thumb snaps back and sends zero input when released, cancelled, when the
diagnostics overlay opens, or when the app backgrounds. Unmeshed chunk edges
clamp the camera rather than ending the physics preview; the saved pose is
never modified. This does **not** load the larger `PlayerMotion` gameplay
resources; fluid travel, status effects, native sprint eligibility/exhaustion,
complex supporting blocks and full entity step/landing effects still require
the complete game runtime. Sprint here is an explicit opt-in toggle rather
than vanilla's full double-tap / hunger-aware input state machine.

A separate opt-in **Crouch** toggle now uses the same native collision
resolver with a 0.6×1.5 player body, 1.27-block eye height, and the
standard 30% crouched movement-input scale. Crouching excludes sprint.
**Stand** checks the real saved block geometry for sufficient headroom
before restoring the 0.6×1.8 standing body. This does not yet implement
full gameplay sneak-edge backoff or fluid/ability rules; it only affects
the temporary read-only viewer and never changes save data.

The transient view camera moves through the read-only native block collision
world (full voxel shapes, not an invented approximate physics implementation),
while the saved player's pose never changes. By default this remains a
**static collision-only preview**. After opening Diagnostics, the user may
explicitly select **Enable physics preview** to run a temporary standing
camera body at **20 TPS**, using Horizon's existing `FixedStep`, native
`living_motion::jump_from_ground` / `apply_gravity`, and full voxel collision
resolver. This enables falling, landing and jumping without executing World
block/entity ticks. Tap **Jump** in Diagnostics or its small floating button
when diagnostics are hidden. Stop physics at any time to return to static
camera movement. The opt-in clock and temporary velocity are discarded on
world replacement, backgrounding and title return; **Reset camera** restores
only the transient viewpoint and motion, never the saved pose.
Three or more fingers still cancel the touch gesture.
**Diagnostics → Reset camera** restores the original saved viewpoint, while
**Back to title** restores the original renderer. The preview redraws on
touch/resize rather than animating an unchanged image at title-frame rate. If GPU initialization, upload or drawing fails, the iOS
shell attempts to restore the title renderer and exposes a retry action.
Backgrounding drops the GPU view and invalidates in-flight preparation. There
is no second UIApplicationMain, no automatic Mac build and no gameplay loop.

This remains a **read-only render of at most nine saved columns**. The
optional walk/gravity/jump/sprint preview is deliberately **not a playable
Minecraft session**: it does not run full `PlayerMotion` (fluid locomotion,
full sprint eligibility, status effects, health and block effects), server/world
ticks, terrain generation, multiplayer,
inventory or saving. Missing neighbours, light and face occlusion beyond the
saved region still require the full world streamer and game runtime. Neither the preview nor the title joins a game session or
changes `level.dat`; no session lock is acquired. Real-device/Simulator GPU
behavior is not guaranteed until the next explicit macOS verification. This keeps native-Rust ownership of
the game's data, as seen in the separate
[heyskylo/horizon-ios](https://github.com/heyskylo/horizon-ios) experiment,
without copying its app entry point or implementation.
Custom world biome packs are not yet resolved during this preflight, so IDs
are based on the bundled vanilla biome registry; custom-biome mappings must
be integrated before actual gameplay. This does **not** generate missing
chunks, take a session lock, modify `level.dat` or start world simulation.
Other dimensions and worlds without a saved position await a different
initial loading path. The terrain preview requires a prepared vanilla
registry, a saved overworld chunk, a valid player camera and an active native
title renderer. Remaining gameplay actions stay disabled.
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
  Its `build_target` input defaults to **`game`**, which builds the actual
  iPhoneOS native game + SwiftUI launcher (`horizon-ios-native-game-ipa`,
  containing `Horizon-game-unsigned.ipa`). `device` is only the legacy iOS
  diagnostics shell (`horizon-ios-diagnostic-shell-ipa`). Select
  `simulator` to build the x86_64 diagnostics app **without booting a Simulator** by default.
  Check `simulator_smoke` only when an actual launch, screenshot and Metal
  health check are needed. The `renderer_probe` checkbox additionally enables
  the smoke step and attempts the
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

The Simulator job skips runtime boot entirely unless `simulator_smoke` or
`renderer_probe` is selected. When the latter is selected, it reuses the
already booted iPhone simulator rather than restarting it. The
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
