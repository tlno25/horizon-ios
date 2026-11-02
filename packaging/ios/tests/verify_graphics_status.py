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
    engine = payload.get("engine")
    player = payload.get("player")
    if type(engine) is not int or engine not in (1, 2):
        raise ValueError(f"Shared game engine profile not initialized (state={engine})")
    if not isinstance(player, str) or not player or len(player) > 16:
        raise ValueError("Shared game engine profile has no valid player name")
    core = payload.get("core_load", 0)
    if type(core) is not int or core not in (0, 1, 2, 3):
        raise ValueError(f"Invalid core game data state (core_load={core})")
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
