#!/usr/bin/env bash
set -euo pipefail

# Called by Xcode on macOS. The current slice supports arm64 device and sim.
cd "$(dirname "$0")/../.."
export PATH="$HOME/.cargo/bin:$PATH"
case "${PLATFORM_NAME:-}" in
    iphonesimulator) target=aarch64-apple-ios-sim ;;
    iphoneos) target=aarch64-apple-ios ;;
    *) echo 'HorizonIOS: build requires an iOS SDK' >&2; exit 1 ;;
esac

if [[ "${ARCHS:-arm64}" != *arm64* ]]; then
    echo 'HorizonIOS currently requires an arm64 build destination' >&2
    exit 1
fi

cargo build --locked --release --package horizon-ios --target "$target"
