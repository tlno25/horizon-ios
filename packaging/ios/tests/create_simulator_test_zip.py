#!/usr/bin/env python3
"""Create a tiny fake Horizon ZIP for iOS Simulator importer smoke only.

This archive has no Minecraft game assets and MUST NOT be treated as playable
resources or bundled with the distributed app. The test never starts gameplay.
"""

import argparse
import pathlib
import zipfile


CONTENTS = {
    "horizon-resources.json": b'{"schema":7,"minecraft":"26.3"}',
    "facts.json": b'{}',
    "block-tags.json": b'{}',
    "registries.json": b'{}',
    "commands.json": b'{}',
    "assets/minecraft/font/fixture": b'CI-only synthetic font placeholder',
    "assets/minecraft/textures/fixture": b'CI-only synthetic texture placeholder',
}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("path", type=pathlib.Path)
    args = parser.parse_args()
    if args.path.exists():
        parser.error(f"Refusing to overwrite an existing archive: {args.path}")
    args.path.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(args.path, "x", zipfile.ZIP_DEFLATED) as archive:
        for name, data in CONTENTS.items():
            archive.writestr(name, data)


if __name__ == "__main__":
    main()
