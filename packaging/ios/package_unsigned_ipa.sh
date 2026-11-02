#!/usr/bin/env bash
set -euo pipefail

# Package an *iPhoneOS* (not Simulator) .app as a standards-shaped unsigned IPA.
# Signing is deliberately disabled in Xcode; an installer must sign this later.
: "${DERIVED_DATA:?Set DERIVED_DATA to Xcode -derivedDataPath}"
: "${RUNNER_TEMP:?Set RUNNER_TEMP to the GitHub runner temp directory}"
app="$DERIVED_DATA/Build/Products/Debug-iphoneos/HorizonIOS.app"
if [[ ! -f "$app/HorizonIOS" || ! -f "$app/Info.plist" ]]; then
    echo "Cannot package unsigned IPA: device-target app missing from $app" >&2
    exit 1
fi
if [[ ! -x "$app/HorizonIOS" ]]; then
    echo 'Cannot package unsigned IPA: the app executable is not executable' >&2
    exit 1
fi

# Ensure this is really an arm64 device build; Simulator binaries cannot be
# sideloaded on an actual iPhone, even when repackaged as Payload/*.app.
if ! xcrun lipo -archs "$app/HorizonIOS" | tr ' ' '\n' | grep -qx arm64; then
    echo 'Cannot package unsigned IPA: the device executable lacks arm64' >&2
    exit 1
fi
if codesign --display "$app" >/dev/null 2>&1; then
    echo 'Cannot label a code-signed app as an unsigned IPA' >&2
    exit 1
fi

output_dir="$RUNNER_TEMP/horizon-ios-artifacts"
mkdir -p "$output_dir"
output="$output_dir/horizon-ios-unsigned.ipa"
stage="$(mktemp -d "$RUNNER_TEMP/horizon-ios-payload.XXXXXXXX")"
trap 'rm -rf "$stage"' EXIT
mkdir -p "$stage/Payload"
ditto "$app" "$stage/Payload/HorizonIOS.app"
ditto -c -k --keepParent "$stage/Payload" "$output"
python3 "$(dirname "$0")/tests/verify_unsigned_ipa.py" "$output"
printf 'Created unsigned device IPA: %s\n' "$output"
