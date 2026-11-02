#!/usr/bin/env python3
"""Check that an iPhone device .app is packaged as a valid IPA ZIP payload."""

import sys
import zipfile
from pathlib import Path

APP = 'Payload/HorizonIOS.app/'
REQUIRED = (APP + 'HorizonIOS', APP + 'Info.plist')


def validate(archive: Path) -> int:
    try:
        with zipfile.ZipFile(archive) as package:
            entries = package.infolist()
            names = [entry.filename for entry in entries]
            if len(names) != len(set(names)):
                raise ValueError('Duplicate files in IPA')
            for name in names:
                parts = name.split('/')
                if (name.startswith('/') or '\\' in name or
                        any(part in ('.', '..') for part in parts)):
                    raise ValueError(f'Unsafe IPA path: {name}')
                if not name.startswith(APP) and name != 'Payload/':
                    raise ValueError(f'Unexpected file outside the app bundle: {name}')
            missing = [name for name in REQUIRED if name not in names]
            if missing:
                raise ValueError(f'Missing required IPA payload: {missing}')
            for name in REQUIRED:
                if package.getinfo(name).file_size == 0:
                    raise ValueError(f'Empty required payload: {name}')
            binary = package.getinfo(REQUIRED[0])
            if binary.create_system == 3 and not (binary.external_attr >> 16 & 0o111):
                raise ValueError('App executable lost its executable permissions')
            corrupted = package.testzip()
            if corrupted:
                raise ValueError(f'Corrupt ZIP member: {corrupted}')
            return len(REQUIRED)
    except (OSError, zipfile.BadZipFile) as exc:
        raise ValueError(f'Invalid IPA archive: {exc}') from exc


def main() -> int:
    if len(sys.argv) != 2:
        print('Usage: verify_unsigned_ipa.py horizon-ios-unsigned.ipa', file=sys.stderr)
        return 2
    try:
        count = validate(Path(sys.argv[1]))
    except ValueError as exc:
        print(f'Invalid Horizon unsigned IPA: {exc}', file=sys.stderr)
        return 1
    print(f'Valid IPA: Payload/HorizonIOS.app ({count} required files present)')
    return 0


if __name__ == '__main__':
    sys.exit(main())
