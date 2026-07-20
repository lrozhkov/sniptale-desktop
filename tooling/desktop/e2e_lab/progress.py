"""Safe, line-oriented controller progress for humans and wrapper logs."""

from __future__ import annotations

import json
import sys
import time
from typing import Any

from receipt import redact_data


def emit(phase: str, **fields: Any) -> None:
    payload = {
        "schemaVersion": 1,
        "epoch": int(time.time()),
        "phase": phase,
        **redact_data(fields),
    }
    print(
        "[desktop-e2e] " + json.dumps(payload, sort_keys=True, separators=(",", ":")),
        file=sys.stderr,
        flush=True,
    )
