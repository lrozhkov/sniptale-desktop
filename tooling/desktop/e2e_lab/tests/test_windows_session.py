from __future__ import annotations

import base64
import sys
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from errors import LabError, TimeoutExpired
from windows_session import prepare_console


class WindowsSessionTests(unittest.TestCase):
    def test_reboot_requires_new_boot_and_console_after_transport_interruption(self):
        ssh = mock.Mock()
        ssh.command.side_effect = [
            SimpleNamespace(stdout="639000000000000000\n"), SimpleNamespace(stdout=""),
            SimpleNamespace(stdout="console-waiting"), LabError("transport restarting"),
            SimpleNamespace(stdout="console-ready"),
        ]
        with mock.patch("windows_session.time.sleep"):
            prepare_console(ssh, 60)
        probe = base64.b64decode(ssh.command.call_args.args[0][-1]).decode("utf-16-le")
        self.assertIn("$boot -gt 639000000000000000", probe)
        self.assertIn("Get-Process explorer", probe)
        self.assertEqual(ssh.command.call_count, 5)
        for call in ssh.command.call_args_list:
            command = base64.b64decode(call.args[0][-1]).decode("utf-16-le")
            self.assertIn("$ProgressPreference='SilentlyContinue'", command)

    def test_invalid_boot_identity_cannot_schedule_restart(self):
        for boot in ("", "0", "1;exit", "1" * 20, "1\n2", "１２３"):
            with self.subTest(boot=boot):
                ssh = mock.Mock()
                ssh.command.return_value.stdout = boot
                with self.assertRaisesRegex(LabError, "boot identity"):
                    prepare_console(ssh, 60)
                self.assertEqual(ssh.command.call_count, 1)

    def test_old_boot_or_missing_console_cannot_pass(self):
        ssh = mock.Mock()
        ssh.command.side_effect = [
            SimpleNamespace(stdout="639000000000000000"), SimpleNamespace(stdout=""),
            SimpleNamespace(stdout="console-waiting"),
        ]
        with mock.patch("windows_session.time.monotonic", side_effect=[0, 0, 0, 1, 1, 2]), mock.patch(
            "windows_session.time.sleep"
        ):
            with self.assertRaises(TimeoutExpired):
                prepare_console(ssh, 1)


if __name__ == "__main__":
    unittest.main()
