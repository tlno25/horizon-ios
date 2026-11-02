#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../../.."
export PATH="$HOME/.cargo/bin:$PATH"

# Native iOS builds must never transitively pull desktop clipboard platforms.
# --no-default-features avoids audio/controller integrations, which are added
# independently after the engine's portable core has been validated.
tree="$(cargo tree --locked --target aarch64-apple-ios-sim \
    --package horizon-client --no-default-features --edges normal)"
if grep -Eq '(^|[[:space:]])arboard v' <<< "$tree"; then
    echo 'ERROR: desktop arboard is being resolved for iOS; use platform clipboard hooks' >&2
    grep -n 'arboard v' <<< "$tree" | head -10 >&2
    exit 1
fi
echo 'iOS client dependency tree excludes desktop arboard'
