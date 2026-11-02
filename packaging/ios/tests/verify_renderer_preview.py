#!/usr/bin/env python3
"""Require a *real* Horizon boot-renderer frame, not merely a Metal probe."""
import json
import sys
from pathlib import Path

from verify_graphics_status import validate as verify_metal


def validate(payload: object) -> str:
    verify_metal(payload)
    assert isinstance(payload, dict)  # already checked by verify_metal
    mode = payload.get('smoke_mode')
    if type(mode) is not int or mode != 1:
        raise ValueError('iOS did not enable Horizon renderer diagnostic mode')
    state = payload.get('renderer_preview')
    if type(state) is not int or state != 3:
        raise ValueError(
            payload.get('error') or
            f'Horizon renderer did not present a frame (renderer_preview={state})'
        )
    return 'Horizon boot renderer presented a frame'


def main() -> int:
    if len(sys.argv) != 2:
        print('Usage: verify_renderer_preview.py <status.json>', file=sys.stderr)
        return 2
    try:
        message = validate(json.loads(Path(sys.argv[1]).read_text(encoding='utf-8')))
    except (ValueError, OSError) as exc:
        print(f'Horizon renderer preview failed: {exc}', file=sys.stderr)
        return 1
    print(message)
    return 0


if __name__ == '__main__':
    sys.exit(main())
