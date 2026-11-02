#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../../.."
export PATH="$HOME/.cargo/bin:$PATH"
cargo build --locked -p horizon-ios --release
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
cc -std=c11 -Wall -Wextra -Werror \
   -I packaging/ios/HorizonIOS \
   packaging/ios/tests/ffi_smoke.c \
   target/release/libhorizon_ios.a \
   -ldl -lpthread -lm -o "$scratch/bridge-smoke"
"$scratch/bridge-smoke"
echo 'C ABI staticlib smoke test passed'
