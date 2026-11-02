#!/usr/bin/env bash
set -euo pipefail

# Opt-in diagnostic launch in an isolated iPhone Simulator. Does not alter the
# standard Metal smoke or require any private Minecraft resource ZIP.
: "${DERIVED_DATA:?Set DERIVED_DATA to the Xcode derived data directory}"
app="$DERIVED_DATA/Build/Products/Debug-iphonesimulator/HorizonIOS.app"
[[ -d "$app" ]] || { echo 'No built iOS Simulator app' >&2; exit 1; }

audid="${HORIZON_IOS_SIMULATOR_UDID:-}"
if [[ -z "$audid" ]]; then
    audid="$(xcrun simctl list devices available --json | python3 -c '
import json,sys
inventory=json.load(sys.stdin)
for runtime,devices in inventory["devices"].items():
    if "iOS" in runtime:
        for device in devices:
            if device.get("isAvailable",True) and "iPhone" in device["name"]:
                print(device["udid"])
                raise SystemExit(0)
raise SystemExit("No available iPhone Simulator")
')"
fi
trap 'xcrun simctl shutdown "$audid" >/dev/null 2>&1 || true' EXIT
xcrun simctl boot "$audid" || true
xcrun simctl bootstatus "$audid" -b
xcrun simctl install "$audid" "$app"
SIMCTL_CHILD_HORIZON_IOS_RENDERER_SMOKE=1 xcrun simctl launch "$audid" com.example.horizonios

app_data="$(xcrun simctl get_app_container "$audid" com.example.horizonios data)"
source_status="$app_data/Library/Caches/horizon-ios-graphics.json"
output="${RUNNER_TEMP:-/tmp}/horizon-ios-boot-renderer-status.json"
# Full Horizon pipelines can take longer than the simple Metal clear. The
# preview reports both success and failed-but-recovered states into JSON.
for ((try=0; try<90; try++)); do
    if [[ -f "$source_status" ]]; then
        cp "$source_status" "$output"
        state="$(python3 - "$output" <<'PY'
import json,sys
try:
    print(json.load(open(sys.argv[1], encoding='utf-8')).get('renderer_preview'))
except (OSError,ValueError):
    print('invalid')
PY
)"
        if [[ "$state" == 3 || "$state" == 4 ]]; then
            break
        fi
    fi
    sleep 1
done

if [[ -f "$output" ]]; then
    cat "$output"
else
    echo 'Horizon did not write an iOS renderer status file' >&2
    exit 1
fi
python3 "$(dirname "$0")/verify_renderer_preview.py" "$output"
echo 'The real Horizon boot renderer presented a frame on the iOS Simulator'
