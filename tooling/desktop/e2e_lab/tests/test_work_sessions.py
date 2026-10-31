from __future__ import annotations

import sys
import tempfile
import time
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from config import LabConfig
from errors import IntegrityError, LabError, UsageError
from paths import LabPaths
from vm import VmManager
from state import write_json
from work_sessions import SessionState, WorkSessions, load_session, state_path


class WorkSessionTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.paths = LabPaths.discover(Path(self.temp.name))
        self.paths.ensure()
        root = Path(__file__).resolve().parents[4]
        self.config = LabConfig.load(root)
        license_probe = mock.patch("work_sessions.require_license")
        license_probe.start()
        self.addCleanup(license_probe.stop)
        self.sessions = WorkSessions(self.paths, self.config)
        self.state = SessionState(
            schema_version=1,
            id="20261030T140000Z",
            pid=55,
            started_epoch=int(time.time()),
            expires_epoch=int(time.time()) + 600,
            golden_sha256="a" * 64,
            boot_id="639000000000000000",
            iterations=0,
            vendor_lock="b" * 64,
            last_artifact=None,
            health="ready",
        )
        self.vm = SimpleNamespace(pid=55, started_epoch=self.state["started_epoch"], golden_sha256="a" * 64)
        self.vms = mock.Mock(spec=VmManager)
        self.sessions.vms = self.vms
        self.vms.current.return_value = self.vm
        self.vms.state_path.return_value = self.paths.runtime / "windows-11/state.json"
        self.boot_id = mock.Mock(return_value=self.state["boot_id"])
        self.sessions.boot_id = self.boot_id
        write_json(state_path(self.paths), self.state)

    def loaded_state(self) -> SessionState:
        value = load_session(self.paths)
        assert value is not None, "expected a persisted session fixture"
        return value

    def tearDown(self):
        self.temp.cleanup()

    def test_watchdog_records_why_its_owned_session_ends(self):
        import session_watchdog
        from state import read_json

        for expired in (False, True):
            with self.subTest(expired=expired):
                maximum = self.config.limits.max_temporary_gib * 1024**3
                now = self.state["expires_epoch"] + 1 if expired else self.state["started_epoch"]
                usage = 0 if expired else maximum + 1
                with (
                    mock.patch("session_watchdog.sys.argv", ["watchdog", str(self.paths.root), self.state["id"]]),
                    mock.patch("session_watchdog.LabPaths.discover", return_value=self.paths),
                    mock.patch("session_watchdog.LabConfig.load", return_value=self.config),
                    mock.patch("session_watchdog.load_session", return_value=self.state),
                    mock.patch("session_watchdog.time.time", return_value=now),
                    mock.patch("session_watchdog.directory_allocated_bytes", return_value=usage),
                    mock.patch("session_watchdog.WorkSessions") as sessions,
                ):
                    session_watchdog.main()
                sessions.return_value.close.assert_called_once()
                record = read_json(self.paths.lab / "sessions" / self.state["id"] / "termination.json")
                assert record is not None
                self.assertEqual(record["reason"], "lifetime-expired" if expired else "temporary-storage-budget")
                self.assertEqual(record["temporary_bytes"], usage)

    def test_failed_cache_save_preserves_current_disk_and_lease(self):
        from state import read_json

        with (
            mock.patch.object(self.sessions, "save_work_image", side_effect=LabError("save failed")),
            self.assertRaisesRegex(LabError, "save failed"),
        ):
            self.sessions.close(save=True)
        self.vms.reset.assert_not_called()
        self.assertEqual(read_json(state_path(self.paths)), self.state)

    def test_offline_save_retry_requires_unchanged_disk_and_stopped_generation(self):
        from state import read_json

        overlay = self.paths.runtime / "windows-11/overlay.qcow2"
        overlay.parent.mkdir(parents=True, exist_ok=True)
        overlay.write_bytes(b"clean shutdown fixture")
        write_json(
            self.vms.state_path.return_value,
            {
                "pid": self.state["pid"],
                "started_epoch": self.state["started_epoch"],
                "golden_sha256": self.state["golden_sha256"],
            },
        )
        marker = self.paths.runtime / "windows-11/work-image-save.json"
        write_json(marker, self.sessions.save_stamp(self.state, overlay))
        self.vms.orphan_pids.return_value = []
        with (
            mock.patch("work_sessions.process_matches", return_value=False),
            mock.patch.object(self.sessions, "store_work_image") as store,
        ):
            original = overlay.read_bytes()
            overlay.write_bytes(b"changed disk")
            with self.assertRaisesRegex(IntegrityError, "identity changed"):
                self.sessions.close(save=True)
            store.assert_not_called()
            self.vms.reset.assert_not_called()
            self.assertEqual(read_json(state_path(self.paths)), self.state)
            overlay.write_bytes(original)
            self.vms.orphan_pids.return_value = [55]
            with self.assertRaisesRegex(IntegrityError, "VM is running"):
                self.sessions.close(save=True)
            store.assert_not_called()
            self.vms.orphan_pids.return_value = []
            self.assertTrue(self.sessions.close(save=True)["cache_saved"])
            store.assert_called_once_with(self.state, overlay)
            self.vms.reset.assert_called_once_with("windows-11")
            self.vms.current.assert_not_called()
            self.assertIsNone(load_session(self.paths))

    def test_generation_and_boot_must_match(self):
        self.assertEqual(self.sessions.require(), self.state)
        self.boot_id.return_value = "639000000000000001"
        with self.assertRaisesRegex(IntegrityError, "rebooted"):
            self.sessions.require()
        self.vms.current.return_value = SimpleNamespace(
            pid=66, started_epoch=self.vm.started_epoch, golden_sha256="a" * 64
        )
        with self.assertRaisesRegex(IntegrityError, "generation"):
            self.sessions.require()

    def test_expired_session_is_closed_and_cannot_test(self):
        self.state.update(started_epoch=int(time.time()) - 1200, expires_epoch=int(time.time()) - 600)
        self.vm.started_epoch = self.state["started_epoch"]
        write_json(state_path(self.paths), self.state)
        with self.assertRaisesRegex(UsageError, "expired"):
            self.sessions.require()
        self.vms.reset.assert_called_once_with("windows-11")
        self.assertIsNone(load_session(self.paths))

    def test_old_lease_cannot_stop_new_vm(self):
        self.vm.pid = 66
        with self.assertRaisesRegex(IntegrityError, "another VM"):
            self.sessions.close()
        self.vms.reset.assert_not_called()

    def test_failed_or_interrupted_session_is_not_reusable(self):
        self.state["health"] = "failed"
        write_json(state_path(self.paths), self.state)
        with self.assertRaisesRegex(UsageError, "failed session"):
            self.sessions.require()

    def test_recovery_retains_boot_and_returns_failed_session_to_ready(self):
        self.state["health"] = "failed"
        write_json(state_path(self.paths), self.state)
        self.sessions.ssh = mock.Mock()
        result = self.sessions.recover()
        self.assertTrue(result["recovered"])
        self.assertEqual(self.loaded_state()["boot_id"], self.state["boot_id"])
        self.assertEqual(self.loaded_state()["health"], "ready")
        self.vms.reset.assert_not_called()

    def test_recovery_failure_keeps_session_invalid(self):
        self.state["health"] = "failed"
        write_json(state_path(self.paths), self.state)
        self.sessions.ssh = mock.Mock()
        self.sessions.ssh.command.side_effect = LabError("installer busy")
        with self.assertRaisesRegex(LabError, "installer busy"):
            self.sessions.recover()
        self.assertEqual(self.loaded_state()["health"], "failed")

    def test_untrusted_state_fails_closed(self):
        for key, value in (
            ("pid", True),
            ("iterations", -1),
            ("iterations", 51),
            ("boot_id", "1;exit"),
            ("golden_sha256", "bad"),
            ("last_artifact", "../../outside.msi"),
            ("expires_epoch", self.state["started_epoch"] + 14401),
        ):
            with self.subTest(key=key):
                write_json(state_path(self.paths), {**self.state, key: value})
                with self.assertRaises(IntegrityError):
                    load_session(self.paths)

    def test_dependency_change_requires_build_without_invalidating_ready_guest(self):
        (self.paths.root / "Cargo.lock").write_text("changed dependency lock")
        self.sessions.validate_toolchain = mock.Mock()
        with self.assertRaisesRegex(UsageError, "build before testing"):
            self.sessions.iterate("test")
        self.assertEqual(self.loaded_state()["health"], "ready")

    def test_interrupted_dependency_build_invalidates_vendor_before_guest_mutation(self):
        (self.paths.root / "Cargo.lock").write_text("changed dependency lock")
        self.sessions.validate_toolchain = mock.Mock()

        def source(root, archive):
            archive.write_bytes(b"source")
            return SimpleNamespace(archive=archive, as_dict=lambda: {"sha256": "c" * 64})

        def vendor(command, **kwargs):
            destination = Path(command[-1])
            destination.mkdir()
            (destination / "fixture").write_text("verified dependency")

        def build(*args):
            self.assertEqual(self.loaded_state()["vendor_lock"], "0" * 64)
            self.assertEqual(self.loaded_state()["health"], "failed")
            raise LabError("injected build interruption")

        with (
            mock.patch("work_sessions.create_source_bundle", side_effect=source),
            mock.patch("work_sessions.run", side_effect=vendor),
            mock.patch("work_sessions.LaneRunner") as runner,
        ):
            runner.return_value._build_windows.side_effect = build
            with self.assertRaisesRegex(LabError, "injected"):
                self.sessions.iterate("build")
        self.assertEqual(self.loaded_state()["vendor_lock"], "0" * 64)
        self.assertEqual(self.loaded_state()["health"], "failed")
        self.vms.reset.assert_not_called()

    def test_halted_vm_fails_before_remote_probes(self):
        self.vms.assert_running.side_effect = LabError("VM halted")
        with self.assertRaisesRegex(LabError, "halted"):
            self.sessions.require()
        self.boot_id.assert_not_called()


if __name__ == "__main__":
    unittest.main()
