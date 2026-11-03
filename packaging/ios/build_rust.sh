#!/usr/bin/env bash
set -euo pipefail

# Called by Xcode on macOS. The current slice supports arm64 device and sim.
cd "$(dirname "$0")/../.."
export PATH="$HOME/.cargo/bin:$PATH"
# Xcode's ARCHS is the architecture of the app being linked, not that of
# the macOS build host. Intel Simulator builds and arm64 physical iPhones are
# separate target triples and must link the matching staticlib slice.
case "${PLATFORM_NAME:-}:${ARCHS:-}" in
    iphonesimulator:*x86_64*) target=x86_64-apple-ios ;;
    iphonesimulator:*arm64*) target=aarch64-apple-ios-sim ;;
    iphoneos:*arm64*) target=aarch64-apple-ios ;;
    *) echo "HorizonIOS: unsupported platform/architecture: ${PLATFORM_NAME:-unset} / ${ARCHS:-unset}" >&2; exit 1 ;;
esac

# Development/CI shells use the repository's fast release-derived profile:
# no thin LTO and parallel codegen. Production Release keeps full optimization.
if [[ "${CONFIGURATION:-Release}" == "Debug" ]]; then
    cargo build --locked --profile fast --package horizon-ios --target "$target"
else
    cargo build --locked --release --package horizon-ios --target "$target"
fi
