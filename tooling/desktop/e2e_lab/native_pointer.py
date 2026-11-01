"""Fixed physical HID input for the isolated native-recording fixture."""

from __future__ import annotations

import json
import time
from collections.abc import Callable

from errors import IntegrityError


class NativePointerFixture:
    def __init__(
        self,
        send: Callable[[dict[str, object]], None],
        pause: Callable[[float], None] = time.sleep,
        clock: Callable[[], float] = time.time,
    ):
        self.send = send
        self.pause = pause
        self.seen: set[str] = set()
        self.failed = False
        self.clock = clock

    def observe(self, line: str) -> None:
        prefix = "[guest-progress] "
        if not line.startswith(prefix):
            return
        try:
            value = json.loads(line[len(prefix) :])
        except ValueError:
            return
        if not isinstance(value, dict):
            return
        phases = {f"real-extension-{browser}-pointer-recording": browser for browser in ("chrome", "edge")}
        scenario = value.get("scenario")
        if not isinstance(scenario, str) or scenario not in phases or value.get("status") != "started":
            return
        expected = {"schemaVersion", "scenario", "status", "attempt", "maxAttempts", "deadlineSeconds", "epoch"}
        if (
            set(value) != expected
            or type(value["schemaVersion"]) is not int
            or value["schemaVersion"] != 1
            or any(
                type(value[key]) is not int or value[key] != 0 for key in ("attempt", "maxAttempts", "deadlineSeconds")
            )
            or type(value["epoch"]) is not int
            or abs(self.clock() - value["epoch"]) > 5
            or scenario in self.seen
        ):
            self.failed = True
            raise IntegrityError("invalid or repeated physical pointer fixture request")
        # Absolute USB-tablet input produces ordinary guest HID events. Native hooks reject
        # injected Win32 mouse_event input, which cannot prove captured cursor telemetry.
        try:
            for x in (14745, 18022, 21298):
                self.send(
                    {
                        "events": [
                            {"type": "abs", "data": {"axis": "x", "value": x}},
                            {"type": "abs", "data": {"axis": "y", "value": 20479}},
                        ]
                    }
                )
                self.pause(0.2)
                try:
                    self.send({"events": [{"type": "btn", "data": {"button": "left", "down": True}}]})
                    self.pause(0.1)
                finally:
                    self.send({"events": [{"type": "btn", "data": {"button": "left", "down": False}}]})
                self.pause(0.2)
        except Exception:
            self.failed = True
            raise
        self.seen.add(scenario)

    def require_complete(self) -> None:
        if self.failed or self.seen != {
            f"real-extension-{browser}-pointer-recording" for browser in ("chrome", "edge")
        }:
            raise IntegrityError("physical pointer fixture did not run for both browsers")
