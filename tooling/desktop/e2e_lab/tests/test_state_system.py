from __future__ import annotations

import json
import io
import os
import subprocess
import sys
import tempfile
import unittest
from contextlib import redirect_stderr
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from errors import IntegrityError, LabError, ResourceError, TimeoutExpired
from state import LabLock, read_json, write_json
from system import require_sha256, run, validate_resources, wait_for_resources
from config import Limits
from limits import enforce_run_limits
from paths import LabPaths
from ssh import GuestSsh, powershell_encoded


class StateSystemTests(unittest.TestCase):
    def test_guest_commands_close_stdin_without_changing_scp_options(self):
        ssh = GuestSsh(LabPaths.discover(Path("/tmp/synthetic-lab")), 22423, "lab")
        with mock.patch("ssh.run") as execute:
            ssh.command(["cmd.exe", "/d", "/c", "exit", "0"], timeout=30)
        argv = execute.call_args.args[0]
        self.assertEqual(argv[:2], ["ssh", "-n"])
        self.assertNotIn("-n", ssh.options())

    def test_batch_upload_rejects_escaping_names_and_links_before_transport(self):
        with tempfile.TemporaryDirectory() as directory:
            paths = LabPaths.discover(Path(directory))
            paths.ensure()
            ssh = GuestSsh(paths, 22423, "lab")
            source = paths.lab / "fixture.ps1"
            source.write_text("synthetic")
            link = paths.lab / "link.ps1"
            link.symlink_to(source)
            with mock.patch("ssh.run") as execute:
                for files in ({"../escape.ps1": source}, {"-option": source}, {"fixture.ps1": link}):
                    with self.assertRaises(LabError):
                        ssh.upload_many(files, timeout=10)
                execute.assert_not_called()
                ssh.upload_many({"first.ps1": source, "second.ps1": source}, timeout=10)
                execute.assert_called_once()
                self.assertEqual(execute.call_args.args[0][0], "scp")
                self.assertNotIn("-n", execute.call_args.args[0])
            self.assertEqual(source.read_text(), "synthetic")
            self.assertEqual(list(paths.lab.glob("upload-*")), [])

    def test_ssh_wait_uses_platform_probe(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            ssh = GuestSsh(LabPaths.discover(Path(directory)), 22423, "lab")
            probe = ["cmd.exe", "/d", "/c", "exit", "0"]
            with mock.patch.object(ssh, "command") as command:
                ssh.wait(1, probe=probe)
            command.assert_called_once_with(probe, timeout=15)

    def test_ssh_wait_propagates_vm_failure_without_transport_probe_or_sleep(self):
        ssh = GuestSsh(LabPaths.discover(Path("/tmp/synthetic-lab")), 22423, "lab")
        health = mock.Mock(side_effect=LabError("guest VM is not running"))
        with mock.patch.object(ssh, "command") as command, mock.patch("ssh.time.sleep") as sleep:
            with self.assertRaisesRegex(LabError, "VM is not running"):
                ssh.wait(360, health_check=health)
        command.assert_not_called()
        sleep.assert_not_called()

    def test_powershell_commands_are_utf16_encoded(self) -> None:
        import base64

        script = "$items | Where-Object { $_.Length -gt 0 }"
        command = powershell_encoded(script)
        self.assertEqual(command[-2], "-EncodedCommand")
        self.assertEqual(base64.b64decode(command[-1]).decode("utf-16-le"), script)
        self.assertNotIn("Where-Object", " ".join(command[:-1]))

    def test_atomic_state_round_trip_and_corruption(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "state.json"
            write_json(path, {"pid": 42})
            self.assertEqual(read_json(path), {"pid": 42})
            path.write_text("{broken", encoding="utf-8")
            with self.assertRaisesRegex(IntegrityError, "corrupted state"):
                read_json(path)

    def test_state_reader_rejects_symlink_and_oversize_input(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            outside = root / "outside.json"
            outside.write_text("{}", encoding="utf-8")
            linked = root / "state.json"
            linked.symlink_to(outside)
            with self.assertRaisesRegex(IntegrityError, "corrupted state"):
                read_json(linked)
            linked.unlink()
            linked.write_bytes(b" " * (1024 * 1024 + 1))
            with self.assertRaisesRegex(IntegrityError, "type or size"):
                read_json(linked)

    def test_exclusive_lock_blocks_concurrent_controller(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "controller.lock"
            with LabLock(path, "outer"):
                with self.assertRaisesRegex(ResourceError, "lab is busy"):
                    with LabLock(path, "inner"):
                        self.fail("nested lock unexpectedly acquired")

    def test_file_execution_policy_is_an_explicit_process_only_opt_in(self):
        plain = powershell_encoded("Write-Output 'read-only'")
        trusted = powershell_encoded(". ./lab-signing.ps1", allow_lab_scripts=True)
        self.assertNotIn("-ExecutionPolicy", plain)
        self.assertEqual(trusted[trusted.index("-ExecutionPolicy") + 1], "Bypass")
        with self.assertRaises(TypeError):
            powershell_encoded("read", allow_lab_scripts="Bypass")

    def test_lock_rejects_symlink_without_touching_target(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            outside = root / "outside.txt"
            outside.write_text("preserve", encoding="utf-8")
            (root / "controller.lock").symlink_to(outside)
            with self.assertRaisesRegex(IntegrityError, "lock is redirected"):
                with LabLock(root / "controller.lock", "test"):
                    self.fail("symlink lock unexpectedly acquired")
            self.assertEqual(outside.read_text(encoding="utf-8"), "preserve")

    def test_health_failure_reaps_started_command_and_keeps_log(self):
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "command.log"
            started = []
            real_popen = subprocess.Popen

            def launch(*args, **kwargs):
                process = real_popen(*args, **kwargs)
                started.append(process)
                return process

            health = mock.Mock(side_effect=LabError("guest VM is not running"))
            with mock.patch("system.subprocess.Popen", side_effect=launch):
                with self.assertRaisesRegex(LabError, "VM is not running"):
                    run([sys.executable, "-c", "import time; time.sleep(30)"], timeout=60, log=log, health_check=health)
            self.assertIsNotNone(started[0].poll())
            self.assertTrue(log.exists())

    def test_timeout_preserves_bounded_redacted_partial_log(self):
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "command.log"
            with self.assertRaises(TimeoutExpired):
                run(
                    [
                        sys.executable,
                        "-u",
                        "-c",
                        "import time; print('phase-started token=super-secret-token'); time.sleep(30)",
                    ],
                    timeout=1,
                    log=log,
                )
            self.assertIn("phase-started", log.read_text())
            self.assertNotIn("super-secret-token", log.read_text())

    def test_timeout_is_typed(self) -> None:
        with self.assertRaises(TimeoutExpired):
            run([sys.executable, "-c", "import time; time.sleep(2)"], timeout=1)

    def test_command_output_is_bounded(self) -> None:
        with self.assertRaisesRegex(ResourceError, "output exceeded"):
            run([sys.executable, "-c", "print('x' * 10000)"], timeout=5, max_output_bytes=1024)

    def test_streamed_command_output_is_redacted_and_returned(self) -> None:
        output = io.StringIO()
        with redirect_stderr(output):
            completed = run(
                [sys.executable, "-c", "print('token=private-value'); print('phase=build')"],
                timeout=5,
                stream=True,
            )
        self.assertNotIn("private-value", output.getvalue())
        self.assertIn("phase=build", output.getvalue())
        self.assertIn("private-value", completed.stdout)

    def test_streamed_overlong_line_is_replaced_not_partially_exposed(self) -> None:
        output = io.StringIO()
        with redirect_stderr(output):
            run(
                [sys.executable, "-c", "print('token=' + ('private' * 10000), end='')"],
                timeout=5,
                stream=True,
            )
        self.assertIn("overlong guest output line redacted", output.getvalue())
        self.assertNotIn("private", output.getvalue())

    def test_checksum_failure_is_closed(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "installer.bin"
            path.write_bytes(b"official bytes")
            with self.assertRaisesRegex(IntegrityError, "SHA-256 mismatch"):
                require_sha256(path, "0" * 64)

    def test_resource_limits_fail_before_vm_start(self) -> None:
        usage = os.statvfs("/")
        with (
            mock.patch("system.shutil.disk_usage") as disk,
            mock.patch("system.available_memory_mib", return_value=1024),
        ):
            disk.return_value = mock.Mock(free=100 * 1024**3)
            with self.assertRaisesRegex(ResourceError, "available RAM"):
                validate_resources(Path("/"), min_free_gib=40, min_memory_mib=2048)

    def test_resource_wait_retries_without_lowering_limits(self) -> None:
        expected = {"free_disk_gib": 100, "available_memory_mib": 12288}
        with (
            mock.patch(
                "system.validate_resources",
                side_effect=[ResourceError("memory is still draining"), expected],
            ) as validate,
            mock.patch("system.time.sleep"),
        ):
            actual = wait_for_resources(
                Path("/"),
                min_free_gib=40,
                min_memory_mib=10240,
                timeout=90,
            )
        self.assertEqual(actual, expected)
        self.assertEqual(validate.call_count, 2)

    def test_evidence_limits_reject_oversize_recording(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "recordings").mkdir()
            (root / "recordings/large.raw").write_bytes(b"x" * 2048)
            limits = Limits(1, 1, 1, 1, 1, 1, 0, 1, 1, 1)
            with self.assertRaisesRegex(ResourceError, "recordings evidence"):
                enforce_run_limits(root, limits)


if __name__ == "__main__":
    unittest.main()
