#!/usr/bin/env python3
"""Set publisher-provided HTTPS resource manifest URL in a *generated* plist.

Never put URLs containing credentials in the built app or GitHub Actions logs.
The game's main bundle resolves this manifest at runtime after the user taps
Download; no links, Java assets or game data are compiled into the repository.
"""
import argparse
import plistlib
import urllib.parse
from pathlib import Path


def prepare(source: Path, output: Path, manifest_url: str) -> None:
    manifest_url = manifest_url.strip()
    if manifest_url:
        parsed = urllib.parse.urlsplit(manifest_url)
        if (parsed.scheme != "https" or not parsed.hostname or parsed.username is not None or
                parsed.password is not None or parsed.fragment or len(manifest_url) > 2048):
            raise ValueError("HORIZON_IOS_RESOURCES_MANIFEST_URL must be a public HTTPS URL without credentials")
    with Path(source).open("rb") as stream:
        info = plistlib.load(stream)
    info["HorizonResourcesManifestURL"] = manifest_url
    Path(output).parent.mkdir(parents=True, exist_ok=True)
    with Path(output).open("wb") as stream:
        plistlib.dump(info, stream)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--manifest-url', default='')
    args = parser.parse_args()
    prepare(args.source, args.output, args.manifest_url)
