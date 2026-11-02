#!/usr/bin/env python3
"""Fail a simulator smoke test unless the native Metal renderer is usable."""

import json
import sys
from pathlib import Path


def validate(payload: object) -> str:
    if not isinstance(payload, dict):
        raise ValueError("Graphics status file must be a JSON object")
    state = payload.get("state")
    error = payload.get("error")
    if type(state) is not int or not isinstance(error, str):
        raise ValueError("Graphics status file needs numeric 'state' and string 'error'")
    if state != 2:
        raise ValueError(error or f"Metal graphics not ready (state={state})")
    return "Metal ready"


def main() -> int:
    if len(sys.argv) != 2:
        print("Usage: verify_graphics_status.py <status.json>", file=sys.stderr)
        return 2
    try:
        result = validate(json.loads(Path(sys.argv[1]).read_text(encoding="utf-8")))
    except (OSError, ValueError) as exc:
        print(f"Horizon iOS graphics smoke test failed: {exc}", file=sys.stderr)
        return 1
    print(f"Horizon iOS graphics smoke test passed: {result}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
