#!/usr/bin/env python3
"""Wrap Horizon's actual Rust game binary in an unsigned Intel iOS Simulator .app.

Builds a minimally sufficient UIKit app without embedding Minecraft assets.
This is a diagnostic/debug bundle, NOT an iPhone-installable device IPA.
"""

import argparse
import plistlib
import shutil
import struct
from pathlib import Path


def package_simulator_app(binary: Path, template: Path, bundle: Path) -> None:
    binary, template, bundle = Path(binary), Path(template), Path(bundle)
    with binary.open("rb") as source:
        header = source.read(28)
    if (len(header) != 28 or
            struct.unpack("<III", header[:12])[:2] != (0xFEEDFACF, 0x01000007) or
            struct.unpack("<I", header[12:16])[0] != 2):
        raise ValueError("Expected an x86_64 Mach-O executable for the iOS Simulator")
    if not binary.stat().st_mode & 0o111:
        raise ValueError("Simulator game binary must be executable")

    with template.open("rb") as file:
        info = plistlib.load(file)
    if (info.get("CFBundleExecutable") != "HorizonGame" or
            info.get("CFBundlePackageType") != "APPL" or
            not info.get("CFBundleIdentifier") or
            info.get("MinimumOSVersion") != "16.0"):
        raise ValueError("Invalid iOS game bundle metadata")
    info["CFBundleSupportedPlatforms"] = ["iPhoneSimulator"]
    info["DTPlatformName"] = "iphonesimulator"

    if bundle.exists():
        raise FileExistsError(f"Simulator app already exists: {bundle}")
    bundle.mkdir(parents=True, exist_ok=False)
    try:
        shutil.copy2(binary, bundle / "HorizonGame")
        with (bundle / "Info.plist").open("wb") as file:
            plistlib.dump(info, file)
        (bundle / "PkgInfo").write_bytes(b"APPL????")
    except BaseException:
        shutil.rmtree(bundle)
        raise


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output-app", type=Path, required=True)
    parser.add_argument("--plist", type=Path, default=Path(__file__).with_name("GameInfo.plist"))
    options = parser.parse_args()
    package_simulator_app(options.binary, options.plist, options.output_app)
    print(f"Created Horizon native Simulator app: {options.output_app}")


if __name__ == "__main__":
    main()
