# Horizon iOS upstream synchronization — 2026-11-05

The iOS port is maintained on the **existing `ios` branch**. Avoid creating
one-off agent branches or replacing this branch with another platform's tree.
The local `android` branch tracks `origin/android` (the owner's fork);
`upstream/android` tracks Vasysik's Android implementation, and
`horizon-rust/main` tracks the main Rust project.

## Android upstream: fully merged locally

- Source: https://github.com/Vasysik/horizon-android, `android` at
  `150a414b09a18c900489d9462a7fc035dae744b7` (2026-11-04).
- Local `ios` merge: `09281776d` (Android upstream is an actual second parent).
- Local `android` fast-forwarded to the same source commit.
- The sole conflict was the removed `game.rs::loading_frame`: on iOS, its
  implementation already lives in `game_startup.rs`. Keep that shared loader,
  adding the new `Frame.distant_terrain` field there rather than duplicating
  the function in `game.rs`.

## Horizon Rust upstream: partial, selected ports only

- Source: https://github.com/horizon-rust/horizon, `main` at
  `63eccfc3559807d338e93ca2fec1e1fdc077a721` (2026-11-04).
- This branch diverges substantially from the mobile fork. A trial full merge
  against the post-Android iOS branch reported **425 conflicts** (including
  `Cargo.lock`, renderers, the runtime and many plugins). **Do not claim that
  all `horizon-rust/main` code is merged.** Blind merging, overwrite strategies,
  or fabricated ancestry would jeopardize the iOS app.
- Portable improvements applied on the `ios` branch with original commit
  attribution (`git cherry-pick -x`):
  - `16a4b6010` — bound the recursive SNBT parser (local `4a2f283b6`).
  - `6bf332f5c` — bound recursive `/execute ... run` command parsing
    (local `ba27098c2`; retained the mobile fork's existing command tests).
  - `b5c41f49d` — display offending mod IDs first in native mixin errors
    (local `31c8bf403`).
- The newer 256-frame SNBT limit from `3f529b7d8` was **not** applied;
  the safer initial 128-frame cap is retained pending iOS stack validation.

## Branch cleanup and publication

Removed integrated remote feature branches:
`fix/ios-indirect-execution-fallback` (merged PR #1),
`perf/sunrise-sector-selection` (the work exists in Android upstream history),
and `agent/ios-preview-crouch` (merged PR #2). PR #2 was merged as an
**experimental** native iOS foundation, not a validated stable release.
`ios`, `android` and `main` are retained.

**Published 2026-11-05:** `origin/ios` was updated with both the PR #2 merge
commit `427e4c17` and the subsequent local upstream/authentication work via
`e0d87eb04`. `origin/android` was fast-forwarded to Vasysik's
`150a414b0`. GitHub Actions and physical iOS runtime validation remain
necessary for the merged changes. No real-device gameplay or Microsoft
sign-in verification is claimed.

## Verification completed

- `cargo check --locked --no-default-features -p horizon-client --bin horizon-client`
- iOS Python contracts: 97 passed; Android Python checks: 15 passed.
- iOS native launcher Rust test: passed; native game startup: 19 passed;
  renderer: 36 passed; Horizon command parser: 32 passed.
- `cargo fmt --all -- --check` passed.
- Launcher GUI native-mod diagnostic test was not runnable in the Linux
  environment without the `fontconfig` development package. A separate
  default-feature Rust test also required `libudev`; reduced-feature tests
  succeeded. Neither represents verified iOS-device gameplay.

For future updates, fetch both upstreams, compare merged ancestry and APIs,
review portable commits individually, then test. Preserve the unpublished
local iOS implementation and private Minecraft resources.
