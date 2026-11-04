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
# Disable telemetry for a CI run, and never synthesize proprietary game files.
SIMCTL_CHILD_HORIZON_TELEMETRY_URL=off \
    xcrun simctl launch "$udid" com.example.horizonios.game
# Allow first-run Metal and font initialization time to paint the screen.
# Tests can set zero seconds; the real simulator defaults to twenty.
sleep "${HORIZON_IOS_GAME_SIMULATOR_WAIT_SECONDS:-20}"
echo "Native Horizon game process launched in Simulator; screenshot is for visual review."
