from __future__ import annotations

import sys
import json
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from errors import IntegrityError, LabError, ResourceError
from paths import LabPaths
from release_acceptance import CASES, collect_release_result, test_release
from results import GuestResult, TestCase
from runner import LaneRunner
import main as lab_main


class ReleaseAcceptanceTests(unittest.TestCase):
    def _guest_fixture(self, root, inventory=None, recording_bytes=b"synthetic mp4"):
        output = root / "guest/windows-11"
        for category in ("logs", "screenshots", "recordings"):
            (output / "e2e-out" / category).mkdir(parents=True)
            (root / category).mkdir()
        (output / "e2e-out/logs/guest.log").write_text("synthetic status\n")
        (output / "e2e-out/screenshots/product.png").write_bytes(b"synthetic png")
        (output / "e2e-out/recordings/product.mp4").write_bytes(recording_bytes)
        value = {
            "schema_version": 1, "lane": "windows-11", "environment": {},
            "tests": [{"name": name, "status": "pass", "duration_ms": 1, "detail": "ok"} for name in sorted(CASES)],
            "artifacts": inventory if inventory is not None else ["guest.log", "product.png", "product.mp4"],
        }
        (output / "e2e-out/result.json").write_text(json.dumps(value))
        runner = SimpleNamespace(
            _validate_guest_tree=LaneRunner._validate_guest_tree,
            _sanitize_guest_text_tree=LaneRunner._sanitize_guest_text_tree,
            _verify_declared_artifacts=LaneRunner._verify_declared_artifacts,
            _classify_evidence=LaneRunner._classify_evidence,
            config=SimpleNamespace(limits=SimpleNamespace(max_log_mib=1, max_screenshot_mib=1, max_recording_mib=1)),
        )
        return runner, output

    def test_alpha_media_is_classified_before_receipt_manifest_creation(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            runner, output = self._guest_fixture(root)
            result = collect_release_result(runner, "windows-11", output, root)
            self.assertTrue(result.passed)
            self.assertEqual((root / "recordings/windows-11-e2e-out-recordings-product.mp4").read_bytes(), b"synthetic mp4")
            self.assertEqual((root / "screenshots/windows-11-e2e-out-screenshots-product.png").read_bytes(), b"synthetic png")

    def test_alpha_collection_rejects_missing_extra_and_duplicate_inventory(self):
        for inventory in ([], ["guest.log", "product.png"], ["guest.log", "product.png", "product.mp4", "ghost.log"], ["guest.log", "product.png", "product.png", "product.mp4"]):
            with self.subTest(inventory=inventory), tempfile.TemporaryDirectory() as name:
                root = Path(name)
                runner, output = self._guest_fixture(root, inventory)
                with self.assertRaisesRegex(IntegrityError, "artifact inventory"):
                    collect_release_result(runner, "windows-11", output, root)
                self.assertEqual(list((root / "recordings").iterdir()), [])

    def test_alpha_classification_enforces_recording_budget(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            runner, output = self._guest_fixture(root, recording_bytes=b"x" * (1024 * 1024 + 1))
            with self.assertRaisesRegex(ResourceError, "recordings evidence"):
                collect_release_result(runner, "windows-11", output, root)

    def test_alpha_controller_cannot_report_success_without_verified_media(self):
        for include_recording in (True, False):
            with self.subTest(include_recording=include_recording), tempfile.TemporaryDirectory() as name:
                root = Path(name).resolve()
                paths = LabPaths.discover(root)
                paths.ensure()
                artifact = root / "candidate.msi"
                artifact.write_bytes(b"msi")
                (root / "provenance.json").write_text("{}")
                source = SimpleNamespace(sha256="a" * 64, file_count=1, commit_sha="b" * 40, dirty_sha256="c" * 64)

                class FakeRunner:
                    def __init__(self, _paths, _config):
                        self.teardowns = {}
                        self.consumed_artifacts = []

                    def test(self, lane, _artifact, run_dir):
                        (run_dir / "logs/guest.log").write_text("synthetic status")
                        (run_dir / "screenshots/product.png").write_bytes(b"synthetic png")
                        if include_recording:
                            (run_dir / "recordings/product.mp4").write_bytes(b"synthetic mp4")
                        return GuestResult(lane, tuple(TestCase(case, "pass", 1, "ok") for case in sorted(CASES)), {}, ())

                with mock.patch.object(lab_main, "create_source_bundle", return_value=source), mock.patch.object(
                    lab_main, "LaneRunner", FakeRunner
                ), mock.patch.object(lab_main, "snapshot_identities", return_value={"windows-11": {"disk_sha256": "d" * 64}}), mock.patch.object(lab_main, "gc"):
                    if include_recording:
                        lab_main.execute_run(paths, SimpleNamespace(), ["windows-11"], "release", artifact)
                    else:
                        with self.assertRaisesRegex(LabError, "recordings evidence"):
                            lab_main.execute_run(paths, SimpleNamespace(), ["windows-11"], "release", artifact)
                run = next(path for path in paths.runs.iterdir() if path.name[:8].isdigit())
                self.assertEqual(json.loads((run / "receipt.json").read_text())["result"], "pass" if include_recording else "fail")

    def test_invalid_provenance_cannot_start_a_guest(self):
        for payload in ("{", "[]", '{"tag": 12}'):
            with self.subTest(payload=payload), tempfile.TemporaryDirectory() as name:
                directory = Path(name)
                artifact = directory / "candidate.msi"
                artifact.write_bytes(b"msi")
                (directory / "provenance.json").write_text(payload)
                runner = SimpleNamespace(vms=mock.Mock())
                with self.assertRaises(LabError):
                    test_release(runner, "windows-11", artifact, directory)
                runner.vms.start.assert_not_called()

    def test_failed_capture_backend_stops_acceptance_before_guest_commands(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            artifact = root / "SniptaleNativeAgent-dev-0.1.0.msi"
            artifact.write_bytes(b"msi")
            (root / "provenance.json").write_text('{"tag":"v0.1.0-alpha.1","productVersion":"0.1.0"}')
            runner = SimpleNamespace(
                paths=SimpleNamespace(root=root), consumed_artifacts=[], vms=mock.Mock(),
                _assert_windows_audio_backend=mock.Mock(side_effect=LabError("capture backend unavailable")),
                _collect_qemu_log=mock.Mock(), _finish_vm=mock.Mock(),
            )
            runner.vms.reset.return_value = False
            with mock.patch("release_acceptance.subprocess.check_output", return_value="a" * 40), mock.patch(
                "release_acceptance.subprocess.run", return_value=SimpleNamespace(returncode=0)
            ), mock.patch("release_acceptance.GuestSsh") as ssh:
                with self.assertRaisesRegex(LabError, "capture backend unavailable"):
                    test_release(runner, "windows-11", artifact, root)
            runner.vms.start.assert_called_once_with("windows-11", windows_audio_capture=True, host_reserve_mib=1024)
            runner._assert_windows_audio_backend.assert_called_once_with("windows-11", require_capture=True)
            ssh.assert_not_called()
            runner._finish_vm.assert_called_once_with("windows-11", "test", False)

    def test_release_guest_contract_has_all_cases_and_no_development_trust(self):
        script = (Path(__file__).resolve().parents[1] / "guest/windows/release-run.ps1").read_text()
        for name in CASES:
            self.assertIn(f'Add-Test "{name}"', script)
        self.assertNotIn("Provision-DevRuntimeTrust", script)
        self.assertNotIn("protectedSecretHex", script)
        self.assertNotIn('WriteAllText("$directory/local-profile.json"', script)
        self.assertIn('Tray @("Mode","Manual")', script)
        self.assertIn('Wait-For { Folder-Selected $picker ([IO.Path]::GetFileName($exports)) }', script)
        self.assertIn('$invoke.Invoke()', script)
        self.assertIn('Wait-For { Window-Hidden $settings }', script)
        self.assertIn('Wait-For { Window-Hidden $restartedSettings }', script)
        self.assertIn('restart or Escape changed user settings', script)
        self.assertIn('[Windows.Automation.ValuePattern]$pattern).Current.Value -eq $Leaf', script)
        self.assertIn('Wait-For { Tray @("Video","Pause") -ProbeOnly } 90', script)
        self.assertIn('$videoStreams.Count -ne 1 -or $audioStreams.Count -ne 1', script)
        self.assertIn('[double]::IsNaN($videoDuration)', script)
        self.assertIn('[double]::IsInfinity($audioDuration)', script)
        self.assertIn('[Math]::Abs($videoDuration - $audioDuration) -gt 1', script)
        self.assertIn('    Assert-RecordedAvTimeline $metadata.streams', script)
        self.assertIn('Test-RecordedAvTimelinePolicy\n\nAdd-Test "network-isolation"', script)
        self.assertIn('@(50.97,30.19)', script)
        self.assertIn('IsWindowVisible(w) && GetWindowRect(w,out r)', script)
        self.assertIn('$entry = Wait-TrayMenuItem $label', script)
        self.assertIn('if ($ProbeOnly) { Close-TrayMenus; return (-not $disabled) }', script)
        self.assertIn('Test-TrayMenuPolicy\nTest-RecordedAvTimelinePolicy', script)
        self.assertNotIn('Start-Sleep -Milliseconds 800', script)
        self.assertNotIn('Start-Sleep -Milliseconds 700', script)


if __name__ == "__main__":
    unittest.main()
