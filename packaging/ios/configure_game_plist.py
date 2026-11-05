#!/usr/bin/env python3
"""Set publisher-provided HTTPS resource manifest URL in a *generated* plist.

Never put URLs containing credentials in the built app or GitHub Actions logs.
The game's main bundle resolves this manifest at runtime after the user taps
Download; no links, Java assets or game data are compiled into the repository.
"""
import argparse
import plistlib
import re
import urllib.parse
from pathlib import Path


def prepare(source: Path, output: Path, manifest_url: str,
            microsoft_client_id: str = '', build_number: str = '') -> None:
    manifest_url = manifest_url.strip()
    if manifest_url:
        parsed = urllib.parse.urlsplit(manifest_url)
        if (parsed.scheme != "https" or not parsed.hostname or parsed.username is not None or
                parsed.password is not None or parsed.fragment or len(manifest_url) > 2048):
            raise ValueError("HORIZON_IOS_RESOURCES_MANIFEST_URL must be a public HTTPS URL without credentials")
    microsoft_client_id = microsoft_client_id.strip()
    if microsoft_client_id and not re.fullmatch(
        r"[0-9a-fA-F]{8}(?:-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12}", microsoft_client_id
    ):
        raise ValueError("HORIZON_MSA_CLIENT_ID must be a valid public-client GUID (no secrets)")
    # A static CFBundleVersion=1 across all CI builds can make signed updates
    # indistinguishable from older, launcher-less installations. GitHub's run
    # number is a monotonically increasing numeric app build identifier.
    if build_number and (not re.fullmatch(r"[1-9][0-9]{0,8}", build_number)):
        raise ValueError("iOS app build number must be 1-9 positive decimal digits")
    with Path(source).open("rb") as stream:
        info = plistlib.load(stream)
    if build_number:
        info["CFBundleVersion"] = build_number
    info["HorizonResourcesManifestURL"] = manifest_url
    info["HorizonMicrosoftClientID"] = microsoft_client_id
    Path(output).parent.mkdir(parents=True, exist_ok=True)
    with Path(output).open("wb") as stream:
        plistlib.dump(info, stream)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--manifest-url', default='')
    parser.add_argument('--microsoft-client-id', default='')
    parser.add_argument('--build-number', default='',
                        help='Monotonic CI run number for CFBundleVersion')
    args = parser.parse_args()
    prepare(args.source, args.output, args.manifest_url,
            args.microsoft_client_id, args.build_number)
