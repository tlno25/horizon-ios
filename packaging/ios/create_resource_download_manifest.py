#!/usr/bin/env python3
"""Create the HTTPS manifest used by Horizon iOS's one-button installer.

Run only on game files you legally prepared and are permitted to distribute.
The script never downloads, uploads, or copies Minecraft resources into Git.
"""

import argparse
import hashlib
import json
import stat
from pathlib import Path, PurePosixPath
from urllib.parse import urlsplit
import zipfile

MAX_ZIP_BYTES = 2 * 1024 * 1024 * 1024
REQUIRED_FILES = (
    'horizon-resources.json', 'facts.json', 'block-tags.json',
    'registries.json', 'commands.json',
)
REQUIRED_PREFIXES = ('assets/minecraft/font/', 'assets/minecraft/textures/')


def generate_manifest(archive: Path, output: Path, url: str,
                      expected_version: str | None = None) -> dict:
    archive, output = Path(archive), Path(output)
    parsed = urlsplit(url)
    if (parsed.scheme != 'https' or not parsed.hostname or parsed.username is not None or
            parsed.password is not None or parsed.fragment or len(url) > 2048):
        raise ValueError('Download URL must be a public HTTPS URL without credentials or fragments')
    size = archive.stat().st_size
    if not 0 < size <= MAX_ZIP_BYTES:
        raise ValueError('ZIP size must be between 1 byte and 2 GiB')
    try:
        with zipfile.ZipFile(archive) as zip:
            info = {entry.filename: entry for entry in zip.infolist()}
            if len(info) != len(zip.infolist()):
                raise ValueError('ZIP contains duplicate entry names')
            expanded = 0
            for entry in info.values():
                if stat.S_ISLNK(entry.external_attr >> 16):
                    raise ValueError('ZIP contains a symlink')
                expanded += entry.file_size
                if expanded > 8 * 1024 * 1024 * 1024:
                    raise ValueError('ZIP would expand beyond 8 GiB')
            for name in info:
                path = PurePosixPath(name)
                if path.is_absolute() or '..' in path.parts or '\\' in name:
                    raise ValueError('ZIP contains an unsafe path')
            for name in REQUIRED_FILES:
                if name not in info or info[name].is_dir():
                    raise ValueError(f'ZIP is missing {name}')
            for prefix in REQUIRED_PREFIXES:
                if not any(name.startswith(prefix) and not entry.is_dir()
                           for name, entry in info.items()):
                    raise ValueError(f'ZIP is missing required files under {prefix}')
            if info['horizon-resources.json'].file_size > 1024 * 1024:
                raise ValueError('ZIP contains an oversized resources manifest')
            contents = json.loads(zip.read('horizon-resources.json'))
            version = contents.get('minecraft')
            if not isinstance(version, str) or not 0 < len(version) < 64:
                raise ValueError('ZIP has no valid Minecraft version')
            if not isinstance(contents.get('schema'), int) or contents['schema'] <= 0:
                raise ValueError('ZIP has no valid Horizon preparation schema')
            if expected_version is not None and version != expected_version:
                raise ValueError(f'ZIP has Minecraft {version}, expected {expected_version}')
    except (zipfile.BadZipFile, KeyError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ValueError(f'ZIP is incomplete, corrupt or missing valid Horizon data: {error}') from error

    digest = hashlib.sha256()
    with archive.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(chunk)
    result = {'minecraft': version, 'url': url, 'sha256': digest.hexdigest(), 'size': size}
    if output.exists():
        raise FileExistsError(f'Refusing to overwrite existing manifest: {output}')
    output.parent.mkdir(parents=True, exist_ok=True)
    # Only the 4-field JSON manifest is written; the prepared ZIP is never copied.
    output.write_text(json.dumps(result, indent=2, ensure_ascii=False) + '\n', encoding='utf-8')
    return result


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--archive', type=Path, required=True, help='authorized, already-prepared ZIP')
    parser.add_argument('--url', required=True, help='HTTPS URL where the ZIP will be published')
    parser.add_argument('--output', type=Path, required=True, help='new JSON manifest file')
    parser.add_argument('--minecraft-version', help='verify the archive targets this Minecraft version')
    args = parser.parse_args()
    result = generate_manifest(args.archive, args.output, args.url, args.minecraft_version)
    print(f"Manifest created: {args.output} (Minecraft {result['minecraft']}; {result['size']} bytes)")


if __name__ == '__main__':
    main()
