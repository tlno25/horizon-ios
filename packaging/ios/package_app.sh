#!/usr/bin/env bash
set -euo pipefail

# Preserve the .app bundle's executable permissions and symlinks for Actions
# artifacts. Neither variant is signed; device builds cannot be installed as-is.
variant="${1:?Usage: package_app.sh simulator|device}"
: "${DERIVED_DATA:?Set DERIVED_DATA to the Xcode -derivedDataPath directory}"
: "${RUNNER_TEMP:?Set RUNNER_TEMP to the GitHub runner temp directory}"

case "$variant" in
    simulator) product_folder="Debug-iphonesimulator" ;;
    device) product_folder="Debug-iphoneos" ;;
    *) echo "Unsupported iOS app variant: $variant" >&2; exit 2 ;;
esac

app="$DERIVED_DATA/Build/Products/$product_folder/HorizonIOS.app"
if [[ ! -f "$app/HorizonIOS" || ! -f "$app/Info.plist" ]]; then
    echo "Missing built HorizonIOS.app in $product_folder" >&2
    exit 1
fi

output_dir="$RUNNER_TEMP/horizon-ios-artifacts"
mkdir -p "$output_dir"
output="$output_dir/HorizonIOS-$variant-unsigned.zip"
ditto -c -k --sequesterRsrc --keepParent "$app" "$output"
echo "Created $output"
