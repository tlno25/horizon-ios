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
import sys
from pathlib import Path


def package(binary: Path, info_plist: Path, output: Path,
            launcher_framework: Path | None = None, engine_data: Path | None = None) -> None:
    binary, info_plist, output = Path(binary), Path(info_plist), Path(output)
    # The game and its SwiftUI launcher must ship in the *same* app bundle.
    # Source assets are never copied here, even when a manifest URL is set.
    if launcher_framework is None:
        # A bare game binary silently falls back to the misleading first-run
        # "missing game files" screen. Never ship that as an iPhone game IPA.
        raise ValueError("The native iOS game IPA requires a SwiftUI launcher framework")
    launcher_framework = Path(launcher_framework)
    if launcher_framework.name != "HorizonLauncherKit.framework" or not launcher_framework.is_dir():
        raise ValueError("Invalid launcher framework path")
    executable = launcher_framework / "HorizonLauncherKit"
    info_file = launcher_framework / "Info.plist"
    if not executable.is_file() or not info_file.is_file():
        raise ValueError("Missing launcher framework binary or Info.plist")
    with info_file.open("rb") as source:
        info = plistlib.load(source)
    if info.get("CFBundleExecutable") != "HorizonLauncherKit" or info.get("CFBundlePackageType") != "FMWK":
        raise ValueError("Invalid launcher framework bundle metadata")
    if not executable.stat().st_mode & stat.S_IXUSR:
        raise ValueError("Launcher framework binary is not executable")
    # A signed IPA containing a simulator/x86 or non-dylib framework can
    # install but fails to present SwiftUI. Validate the actual Apple target.
    with executable.open("rb") as source:
        framework_header = source.read(28)
    if (len(framework_header) != 28 or
            struct.unpack("<II", framework_header[:8]) != (0xFEEDFACF, 0x0100000C) or
            struct.unpack("<I", framework_header[12:16])[0] != 6):
        raise ValueError("Expected an arm64 iPhoneOS SwiftUI launcher Mach-O dylib")
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

    if engine_data is not None:
        # Only derived engine tables are permitted. Reject the original Mojang
        # assets, JARs and sounds before writing any IPA.
        sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "tools"))
        from android_build import generated_engine_file
        engine_data = Path(engine_data)
        with zipfile.ZipFile(engine_data) as source:
            names = source.namelist()
            if (not names or len(names) != len(set(names)) or
                    not all(generated_engine_file(name) and not source.getinfo(name).is_dir()
                            for name in names) or
                    not all(name in names for name in
                            ("horizon-resources.json", "facts.json", "registries.json", "commands.json"))):
                raise ValueError("iOS engine tables must be generated-only; no vanilla game files")
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
            if engine_data is not None:
                ipa.write(engine_data, prefix + "engine-data.zip")
            for file in launcher_framework.rglob("*"):
                if file.is_symlink():
                    raise ValueError("Launcher framework must not contain symlinks")
                if not file.is_file():
                    continue
                if file.name == "horizon-resources.json":
                    raise ValueError("Cannot ship private Minecraft game resources")
                subpath = file.relative_to(launcher_framework).as_posix()
                ipa.write(file, prefix + "Frameworks/HorizonLauncherKit.framework/" + subpath)
        os.replace(temporary, output)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--plist", type=Path, default=Path(__file__).with_name("GameInfo.plist"))
    parser.add_argument("--launcher-framework", type=Path, required=True,
                        help="Required: HorizonLauncherKit.framework compiled by Xcode")
    parser.add_argument("--engine-data", type=Path,
                        help="Optional generated-only engine-data ZIP from your authorized preparation")
    args = parser.parse_args()
    package(args.binary, args.plist, args.output, args.launcher_framework, args.engine_data)
    print(f"Created unsigned native-game IPA: {args.output}")


if __name__ == "__main__":
    main()
