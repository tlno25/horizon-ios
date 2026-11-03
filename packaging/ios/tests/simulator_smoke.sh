#!/usr/bin/env bash
set -euo pipefail

# macOS only: build artifact must already exist in the DerivedData directory.
: "${DERIVED_DATA:?Pass an Xcode -derivedDataPath directory as DERIVED_DATA}"
app="${DERIVED_DATA}/Build/Products/Debug-iphonesimulator/HorizonIOS.app"
if [[ ! -d "$app" ]]; then
    echo "Simulator app was not produced: $app" >&2
    exit 1
fi

udid="${HORIZON_IOS_SIMULATOR_UDID:-}"
if [[ -z "$udid" ]]; then
    udid="$(xcrun simctl list devices available --json | python3 -c '
import json,sys
inventory = json.load(sys.stdin)
phones = [device for runtime, devices in inventory["devices"].items()
          if ".iOS-" in runtime or "SimRuntime.iOS" in runtime
          for device in devices
          if device.get("isAvailable", True) and "iPhone" in device["name"]]
if not phones:
    raise SystemExit("No available iPhone Simulator; install a simulator runtime")
print(phones[0]["udid"])
')"
fi

# The opt-in GPU probe runs immediately afterwards on the same Simulator.
# Shutting down and booting again can stall simctl bootstatus for tens of
# minutes on hosted macOS, wasting the runner budget.
if [[ "${HORIZON_IOS_SIMULATOR_KEEP_BOOTED:-0}" != "1" ]]; then
    trap 'xcrun simctl shutdown "$udid" >/dev/null 2>&1 || true' EXIT
fi
xcrun simctl boot "$udid" || true
xcrun simctl bootstatus "$udid" -b
xcrun simctl install "$udid" "$app"
xcrun simctl launch "$udid" com.example.horizonios
# Allow one UIKit layout/display cycle before capturing the host + Metal probe.
sleep 3
xcrun simctl io "$udid" screenshot "${RUNNER_TEMP:-/tmp}/horizon-ios-smoke.png"
# Capture native diagnostic logs for problems which do not prevent launch.
# The SwiftUI overlay can show GPU failures while the app process stays
# alive, so a successful simctl launch alone is insufficient rendering evidence.
log_file="${RUNNER_TEMP:-/tmp}/horizon-ios-simulator-native.log"
xcrun simctl spawn "$udid" log show --style compact --last 5m \
    --predicate 'process == "HorizonIOS" AND eventMessage CONTAINS[c] "Horizon iOS graphics"' \
    > "$log_file" 2>&1 || true
if [[ -s "$log_file" ]]; then
    echo 'Horizon iOS native GPU diagnostics:'
    tail -35 "$log_file"
fi
# UIKit can be alive with an unavailable Metal backend; treat that as a
# failed renderer smoke even though the app's launch and screenshot succeeded.
app_data="$(xcrun simctl get_app_container "$udid" com.example.horizonios data)"
status_file="$app_data/Library/Caches/horizon-ios-graphics.json"
python3 "$(dirname "$0")/verify_graphics_status.py" "$status_file"
echo "Simulator launch and Metal smoke test passed ($udid); screenshot captured"
