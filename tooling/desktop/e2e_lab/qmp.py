"""Bounded QMP framing; asynchronous events never replace command replies."""

from __future__ import annotations

import json
import time

from errors import LabError


class QmpReader:
    def __init__(self, client) -> None:
        self.client = client
        self.buffer = b""
        self.deadline = time.monotonic() + 10

    def message(self) -> dict[str, object]:
        while b"\n" not in self.buffer:
            remaining = self.deadline - time.monotonic()
            if remaining <= 0:
                raise LabError("QMP response deadline exceeded")
            self.client.settimeout(remaining)
            block = self.client.recv(65537 - len(self.buffer))
            if not block:
                raise LabError("QMP closed before a complete response")
            self.buffer += block
            if len(self.buffer) > 65536:
                raise LabError("QMP response exceeds size limit")
        line, self.buffer = self.buffer.split(b"\n", 1)
        try:
            message = json.loads(line)
        except (ValueError, UnicodeError) as exc:
            raise LabError("QMP returned invalid JSON") from exc
        if not isinstance(message, dict):
            raise LabError("QMP response must be an object")
        return message

    def reply(self):
        for _ in range(32):
            message = self.message()
            if "error" in message:
                raise LabError("QMP rejected command")
            if "return" in message:
                return message["return"]
            if not isinstance(message.get("event"), str):
                raise LabError("QMP response has no command result")
        raise LabError("QMP event limit exceeded")
