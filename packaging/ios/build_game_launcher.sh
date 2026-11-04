#!/usr/bin/env bash
# Native game and SwiftUI framework are built for the SAME SDK/architecture.
set -euo pipefail
sdk="${1:?iphoneos or iphonesimulator}"
arch="${2:?arm64 or x86_64}"
derived="${3:?derived data directory}"
case "$sdk/$arch" in
    iphoneos/arm64|iphonesimulator/x86_64|iphonesimulator/arm64) ;;
    *) echo "Unsupported iOS launcher target: $sdk/$arch" >&2; exit 2 ;;
esac
if ! command -v xcodegen >/dev/null 2>&1; then
    command -v brew >/dev/null 2>&1 || { echo 'XcodeGen is required' >&2; exit 2; }
    brew install xcodegen
fi
project="$(cd "$(dirname "$0")/GameLauncher" && pwd)"
(cd "$project" && xcodegen generate)
if [ "$sdk" = iphoneos ]; then destination='generic/platform=iOS'; else destination='generic/platform=iOS Simulator'; fi
xcodebuild -project "$project/HorizonGameLauncher.xcodeproj" \
    -scheme HorizonLauncherKit -configuration Release \
    -sdk "$sdk" -destination "$destination" -derivedDataPath "$derived" \
    ARCHS="$arch" CODE_SIGNING_ALLOWED=NO build
framework="$derived/Build/Products/Release-$sdk/HorizonLauncherKit.framework"
test -s "$framework/HorizonLauncherKit" || { echo 'SwiftUI framework missing from Xcode output' >&2; exit 1; }
echo "SwiftUI framework built: $framework"
