#!/usr/bin/env python3
"""Reject iPhone game binaries still containing the obsolete Metal first-run screen.

The native iOS client always uses its bundled SwiftUI launcher. An old or
incorrect build may still display the 'Horizon on iPhone needs game files'
screen, so assert the unwanted code wasn't linked into an iPhoneOS executable.
"""
import argparse
from pathlib import Path

BANNED = (
    b"Horizon on iPhone needs game files",
    b"Horizon launcher is missing",
    b"Missing: Documents/resources/horizon-resources.json",
    b"Start Horizon from the launcher",
)


def assert_no_legacy_screen(binary: Path) -> None:
    binary = Path(binary)
    contents = binary.read_bytes()
    for marker in BANNED:
        if marker in contents:
            raise ValueError(
                f"iOS game still contains an obsolete first-run screen: {marker.decode()}"
            )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    args = parser.parse_args()
    assert_no_legacy_screen(args.binary)
    print(f"Native iOS launcher-only executable verified: {args.binary}")


if __name__ == "__main__":
    main()
