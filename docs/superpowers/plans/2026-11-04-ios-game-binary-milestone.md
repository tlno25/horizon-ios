# Horizon iOS — native game executable milestone (2026-11-04)

The current iOS staticlib/SwiftUI shell can render a bounded read-only preview, but it does not run the playable `horizon-client` application handler. Do not label it playable merely because the Metal probe or unsigned IPA builds.

## Chosen near-term route

Keep the existing SwiftUI/Winit shell and its device workflow unchanged. Add a **separately named experimental native-game IPA** built from the actual Rust `horizon-client` executable, with its own app bundle and sandbox, following the tested feasibility of heyskylo/horizon-ios without copying their runtime. This is a stepping stone toward integrating real gameplay into one app; it is not a claim of completed touch controls, assets, account authentication, or world persistence.

## Tasks

1. Add a portable, tested helper for resolving the iOS app's writable Documents game directory, and enter it before the ordinary client bootstrap. Never write into `/` or the app bundle.
2. Add a deterministic, tested IPA packager validating arm64 Mach-O executable and generated bundle metadata; never package proprietary Minecraft resources.
3. Add a **manual-only** GitHub Actions build target for the actual game, using the macOS Apple SDK and uploading a distinguishable unsigned game IPA. Keep the existing shell IPA pathway intact.
4. Validate unit tests and Rust portability locally; push the feature branch; dispatch the macOS build and inspect any failures. Address compiler/toolchain issues from real logs.
5. Continue mobile input, resource import, lifecycle, rendering and game-loop integration only when evidence proves earlier milestones. Real-device gameplay and signed installation remain separate acceptance gates.

## Acceptance

- Existing Ubuntu quick checks and device-shell workflow remain usable.
- The new workflow produces a `Horizon-game-unsigned.ipa` containing a real iPhoneOS arm64 `horizon-client` executable and valid `Info.plist`, without vanilla assets.
- Document installation, resource preparation and missing touch/input limitations accurately.
