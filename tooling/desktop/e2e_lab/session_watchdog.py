"""Expire only the development session generation that launched this watchdog."""
from __future__ import annotations

import sys
import time
from pathlib import Path

from config import LabConfig
from errors import LabError, ResourceError
from paths import LabPaths
from limits import directory_allocated_bytes
from state import LabLock, write_json
from work_sessions import WorkSessions, load_session


def main():
    paths = LabPaths.discover(Path(sys.argv[1]))
    config = LabConfig.load(paths.root)
    identity = sys.argv[2]
    while True:
        try:
            with LabLock(paths.locks / "controller.lock", "session-expiration"):
                config = LabConfig.load(paths.root)
                state = load_session(paths)
                if state is None or state["id"] != identity:
                    return
                now = int(time.time())
                usage = directory_allocated_bytes(paths.lab)
                maximum = config.limits.max_temporary_gib * 1024 ** 3
                if now >= state["expires_epoch"] or usage > maximum:
                    write_json(paths.scoped(paths.lab / "sessions" / identity / "termination.json"), {
                        "schema_version": 1, "reason": "lifetime-expired" if now >= state["expires_epoch"] else "temporary-storage-budget",
                        "terminated_epoch": now, "temporary_bytes": usage, "maximum_temporary_bytes": maximum,
                    })
                    WorkSessions(paths, config).close()
                    return
        except ResourceError:
            pass  # An in-flight command has its own bounded timeout.
        except LabError:
            return  # Refuse to act on malformed state or a different VM generation.
        time.sleep(15)


if __name__ == "__main__":
    main()
