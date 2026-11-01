from __future__ import annotations

import io
import json
import subprocess
import sys
import unittest
from contextlib import redirect_stderr
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from errors import IntegrityError, LabError
from native_pointer import NativePointerFixture
from system import run


def phase(browser: str = "chrome", /, **patch: object):
    value = {
        "schemaVersion": 1,
        "scenario": f"real-extension-{browser}-pointer-recording",
        "status": "started",
        "attempt": 0,
        "maxAttempts": 0,
        "deadlineSeconds": 0,
        "epoch": 1000,
    }
    value.update(patch)
    return "[guest-progress] " + json.dumps(value)


class NativePointerTests(unittest.TestCase):
    def test_fixed_hid_sequence_requires_both_browsers_and_cannot_repeat(self):
        events = []
        sleeps = []
        fixture = NativePointerFixture(events.append, sleeps.append, lambda: 1000)
        for line in ("ordinary output", "[guest-progress] broken", phase("other"), phase(status="pass")):
            fixture.observe(line)
        self.assertEqual(events, [])
        fixture.observe(phase())
        self.assertEqual(len(events), 9)
        moves = [event["events"] for event in events[::3]]
        self.assertEqual([move[0]["data"]["value"] for move in moves], [14745, 18022, 21298])
        self.assertTrue(all(move[1]["data"] == {"axis": "y", "value": 20479} for move in moves))
        self.assertEqual(
            [event["events"][0]["data"]["down"] for event in events if event["events"][0]["type"] == "btn"],
            [True, False] * 3,
        )
        with self.assertRaises(IntegrityError):
            fixture.require_complete()
        fixture.observe(phase("edge"))
        fixture.require_complete()
        self.assertEqual(len(events), 18)
        self.assertAlmostEqual(sum(sleeps), 3)
        with self.assertRaises(IntegrityError):
            fixture.observe(phase())
        self.assertEqual(len(events), 18)
        with self.assertRaises(IntegrityError):
            fixture.require_complete()

    def test_malformed_or_stale_known_phase_never_sends_input(self):
        for patch in (
            {"epoch": True},
            {"epoch": 994},
            {"epoch": 1006},
            {"schemaVersion": True},
            {"attempt": 1},
            {"deadlineSeconds": 1},
            {"extra": "untrusted"},
        ):
            with self.subTest(patch=patch):
                send = mock.Mock()
                fixture = NativePointerFixture(send, lambda _seconds: None, lambda: 1000)
                with self.assertRaises(IntegrityError):
                    fixture.observe(phase(**patch))
                send.assert_not_called()
                with self.assertRaises(IntegrityError):
                    fixture.require_complete()

    def test_qmp_failure_cannot_become_a_completed_fixture(self):
        send = mock.Mock(side_effect=LabError("QMP failure"))
        fixture = NativePointerFixture(send, lambda _seconds: None, lambda: 1000)
        with self.assertRaises(LabError):
            fixture.observe(phase())
        self.assertEqual(send.call_count, 1)
        with self.assertRaises(IntegrityError):
            fixture.require_complete()

    def test_stream_callback_is_bounded_and_receives_a_final_unterminated_line(self):
        lines = []
        with redirect_stderr(io.StringIO()):
            run(
                [sys.executable, "-c", "print('ready'); print('x'*70000); print('last',end='')"],
                timeout=5,
                stream=True,
                line_callback=lines.append,
            )
        self.assertEqual(lines, ["ready", "last"])
        with self.assertRaises(ValueError):
            run([sys.executable, "-c", "pass"], timeout=5, line_callback=lines.append)

    def test_button_release_is_attempted_when_press_or_hold_fails(self):
        for failure in ("press", "hold"):
            events = []

            def send(event, events=events, failure=failure):
                events.append(event)
                if failure == "press" and len(events) == 2:
                    raise LabError("press response failed")

            def pause(seconds, failure=failure):
                if failure == "hold" and seconds == 0.1:
                    raise LabError("hold failed")

            fixture = NativePointerFixture(send, pause, lambda: 1000)
            with self.assertRaises(LabError):
                fixture.observe(phase())
            self.assertEqual(len(events), 3)
            self.assertEqual(events[-1], {"events": [{"type": "btn", "data": {"button": "left", "down": False}}]})
            with self.assertRaises(IntegrityError):
                fixture.require_complete()

    def test_callback_failure_stops_and_reaps_the_running_process(self):
        processes = []
        start = subprocess.Popen

        def spawn(*args, **kwargs):
            process = start(*args, **kwargs)
            processes.append(process)
            return process

        def reject(_line):
            raise IntegrityError("invalid fixture request")

        with (
            mock.patch("system.subprocess.Popen", side_effect=spawn),
            redirect_stderr(io.StringIO()),
            self.assertRaises(IntegrityError),
        ):
            run(
                [sys.executable, "-c", "import time; print('ready',flush=True); time.sleep(30)"],
                timeout=5,
                stream=True,
                line_callback=reject,
            )
        self.assertEqual(len(processes), 1)
        self.assertIsNotNone(processes[0].poll())
