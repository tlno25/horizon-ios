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

trap 'xcrun simctl shutdown "$udid" >/dev/null 2>&1 || true' EXIT
xcrun simctl boot "$udid" || true
xcrun simctl bootstatus "$udid" -b
xcrun simctl install "$udid" "$app"
xcrun simctl launch "$udid" com.example.horizonios
# Allow one UIKit layout/display cycle before capturing the host + Metal probe.
sleep 3
xcrun simctl io "$udid" screenshot "${RUNNER_TEMP:-/tmp}/horizon-ios-smoke.png"
echo "Simulator launch smoke test passed ($udid); screenshot captured"
