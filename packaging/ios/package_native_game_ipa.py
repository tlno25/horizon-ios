#!/usr/bin/env python3
"""Package Horizon's *actual game executable* as an unsigned device IPA.

This deliberately builds no asset archive: the player must separately supply
their legitimately prepared Java Edition resources in the app's Documents.
"""
import argparse
import os
import plistlib
import stat
import struct
import tempfile
import zipfile
from pathlib import Path


def package(binary: Path, info_plist: Path, output: Path) -> None:
    binary, info_plist, output = Path(binary), Path(info_plist), Path(output)
    with binary.open("rb") as source:
        header = source.read(28)
    # Reject host/simulator binaries, dylibs, fat binaries and invalid headers.
    if (len(header) != 28 or struct.unpack("<III", header[:12])[:2] !=
            (0xFEEDFACF, 0x0100000C) or struct.unpack("<I", header[12:16])[0] != 2):
        raise ValueError("Expected an iPhoneOS arm64 Mach-O executable")
    if not (binary.stat().st_mode & stat.S_IXUSR):
        raise ValueError("Horizon game binary is not executable")

    with info_plist.open("rb") as file:
        info_bytes = file.read()
    info = plistlib.loads(info_bytes)
    if (info.get("CFBundleExecutable") != "HorizonGame" or
            info.get("CFBundlePackageType") != "APPL" or
            not info.get("CFBundleIdentifier") or
            info.get("MinimumOSVersion") != "16.0" or
            info.get("UIDeviceFamily") != [1, 2] or
            info.get("UIFileSharingEnabled") is not True):
        raise ValueError("Invalid iOS game bundle metadata")

    output.parent.mkdir(parents=True, exist_ok=True)
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(
            dir=output.parent, prefix=".horizon-game-ipa-", suffix=".zip",
            delete=False
        ) as file:
            temporary = Path(file.name)
        prefix = "Payload/HorizonGame.app/"
        with zipfile.ZipFile(temporary, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=6) as ipa:
            # Preserve the Unix executable mode so iOS can launch the program.
            executable = zipfile.ZipInfo(prefix + "HorizonGame")
            executable.compress_type = zipfile.ZIP_DEFLATED
            executable.create_system = 3
            executable.external_attr = (0o100755 << 16)
            with binary.open("rb") as source, ipa.open(executable, "w", force_zip64=True) as dest:
                while data := source.read(1024 * 1024):
                    dest.write(data)
            ipa.writestr(prefix + "Info.plist", info_bytes)
            ipa.writestr(prefix + "PkgInfo", b"APPL????")
        os.replace(temporary, output)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--plist", type=Path, default=Path(__file__).with_name("GameInfo.plist"))
    args = parser.parse_args()
    package(args.binary, args.plist, args.output)
    print(f"Created unsigned native-game IPA: {args.output}")


if __name__ == "__main__":
    main()
