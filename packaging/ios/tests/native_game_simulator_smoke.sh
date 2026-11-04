#!/usr/bin/env bash
set -euo pipefail

# Human-reviewable runtime smoke for the actual Rust Horizon game executable.
# This does NOT assert that gameplay works or that proprietary assets exist.
: "${HORIZON_NATIVE_SIMULATOR_APP:?Set HORIZON_NATIVE_SIMULATOR_APP to a built .app}"
app="${HORIZON_NATIVE_SIMULATOR_APP}"
[[ -d "$app" ]] || { echo "No native Horizon Simulator app: $app" >&2; exit 1; }
output_dir="${RUNNER_TEMP:-/tmp}"
udid="${HORIZON_IOS_SIMULATOR_UDID:-}"
if [[ -z "$udid" ]]; then
    udid="$(xcrun simctl list devices available --json | python3 -c '
import json, sys
devices = json.load(sys.stdin)["devices"]
for runtime, entries in devices.items():
    if "iOS" in runtime:
        for device in entries:
            if device.get("isAvailable", True) and "iPhone" in device["name"]:
                print(device["udid"])
                raise SystemExit(0)
raise SystemExit("No available iPhone Simulator")
')"
fi

capture() {
    xcrun simctl io "$udid" screenshot "$output_dir/horizon-ios-game-simulator-screen.png"         >/dev/null 2>&1 || true
    xcrun simctl spawn "$udid" log show --style compact --last 8m \
        --predicate 'process == "HorizonGame" AND (eventMessage CONTAINS[c] "Horizon" OR eventMessage CONTAINS[c] "error" OR eventMessage CONTAINS[c] "panic" OR eventMessage CONTAINS[c] "wgpu")' \
        > "$output_dir/horizon-ios-game-simulator-console.log" 2>&1 || true
    # Simulator log show captures UIKit, not Rust's file logger. Fetch the
    # actual game's Documents/logs/latest.log before destroying the simulator.
    # Absence is not a test failure: the process may have crashed before logging.
    container="$(xcrun simctl get_app_container "$udid" com.example.horizonios.game data 2>/dev/null || true)"
    if [[ -n "$container" && -f "$container/Documents/logs/latest.log" ]]; then
        cp "$container/Documents/logs/latest.log" "$output_dir/horizon-ios-game-simulator-game.log" || true
    fi
    xcrun simctl shutdown "$udid" >/dev/null 2>&1 || true
}
trap capture EXIT

xcrun simctl boot "$udid" || true
xcrun simctl bootstatus "$udid" -b
xcrun simctl install "$udid" "$app"
# The combined native app intentionally waits at the SwiftUI launcher. No
# Metal frame is presented before the player downloads game files and taps
# Play. Check UIKit presentation instead of timing out on the old frame signal.
launcher_bundle="$app/Frameworks/HorizonLauncherKit.framework"
launcher_marker="Library/Caches/horizon-ios-launcher-presented"
bridge_marker="Library/Caches/horizon-ios-launcher-bridge-ready"
import_marker="Library/Caches/horizon-ios-launcher-import-ready"
if [[ -d "$launcher_bundle" ]]; then
    container="$(xcrun simctl get_app_container "$udid" com.example.horizonios.game data 2>/dev/null || true)"
    if [[ -n "$container" ]]; then
        rm -f "$container/$launcher_marker" "$container/$bridge_marker" "$container/$import_marker"
        # A synthetic resource tree exercises SwiftUI -> Rust's checked ZIP
        # importer. It contains no copyrighted Minecraft resources and will
        # never be packaged into an iOS app.
        fixture="$container/Documents/horizon-download-CIRESOURCE.zip"
        rm -f "$fixture"
        script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
        python3 "$script_dir/create_simulator_test_zip.py" "$fixture"
    fi
fi
# Disable telemetry for a CI run, and never synthesize proprietary game files.
SIMCTL_CHILD_HORIZON_TELEMETRY_URL=off \
SIMCTL_CHILD_HORIZON_IOS_SIMULATOR_SMOKE=1 \
    xcrun simctl launch "$udid" com.example.horizonios.game
# Shader compilation took 43 seconds in a real Simulator run, so a fixed
# 7/20-second delay captured a black screen before the first present. Poll the
# native game log instead of assuming startup has finished. Keep collecting
# diagnostics (via EXIT trap) even when the renderer never reaches a frame.
ready_timeout="${HORIZON_IOS_GAME_SIMULATOR_READY_TIMEOUT_SECONDS:-120}"
if ! [[ "$ready_timeout" =~ ^[0-9]+$ ]]; then
    echo "Invalid first-frame timeout: $ready_timeout" >&2
    exit 2
fi
deadline=$((SECONDS + ready_timeout))
while true; do
    container="$(xcrun simctl get_app_container "$udid" com.example.horizonios.game data 2>/dev/null || true)"
    if [[ -d "$launcher_bundle" ]]; then
        if [[ -n "$container" && -f "$container/$launcher_marker" ]] \
            && grep -Fxq 'presented' "$container/$launcher_marker"; then
            if [[ -f "$container/$bridge_marker" ]] \
                && grep -Fxq 'ready' "$container/$bridge_marker"; then
                if [[ -f "$container/$import_marker" ]] \
                    && grep -Fxq 'ready' "$container/$import_marker"; then
                    sleep "${HORIZON_IOS_GAME_SIMULATOR_SETTLE_SECONDS:-2}"
                    echo "Native Horizon SwiftUI launcher, Rust bridge and checked ZIP importer verified; capturing screenshot for review."
                    break
                fi
                if (( SECONDS >= deadline )); then
                    echo "Native Horizon SwiftUI bridge responded, but the synthetic ZIP import did not succeed within ${ready_timeout}s; capturing diagnostics." >&2
                    exit 1
                fi
            elif (( SECONDS >= deadline )); then
                echo "Native Horizon SwiftUI launcher appeared, but its Rust bridge did not respond within ${ready_timeout}s; capturing diagnostics." >&2
                exit 1
            fi
        elif (( SECONDS >= deadline )); then
            echo "Native Horizon SwiftUI launcher did not appear within ${ready_timeout}s; capturing diagnostics." >&2
            exit 1
        fi
        sleep 1
        continue
    fi
    if [[ -n "$container" && -f "$container/Documents/logs/latest.log" ]] \
        && grep -Fq 'Horizon renderer: presenting the first frame' "$container/Documents/logs/latest.log"; then
        # The renderer writes this marker immediately before queue.present().
        # Let the Simulator compositor display the completed frame as well.
        sleep "${HORIZON_IOS_GAME_SIMULATOR_SETTLE_SECONDS:-2}"
        echo "Native Horizon game renderer reached its first frame; capturing screenshot for review."
        break
    fi
    if (( SECONDS >= deadline )); then
        echo "Native Horizon game did not present its first frame within ${ready_timeout}s; capturing diagnostics." >&2
        exit 1
    fi
    sleep 1
done
