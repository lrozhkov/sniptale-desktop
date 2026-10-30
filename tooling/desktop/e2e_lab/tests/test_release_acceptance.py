from __future__ import annotations

import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from errors import LabError
from release_acceptance import CASES, test_release


class ReleaseAcceptanceTests(unittest.TestCase):
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
        self.assertIn('[Windows.Automation.ValuePattern]$pattern).Current.Value -eq $Leaf', script)
        self.assertIn('Wait-For { Tray @("Video","Pause") -ProbeOnly } 90', script)
        self.assertIn('$videoStreams.Count -ne 1 -or $audioStreams.Count -ne 1', script)
        self.assertIn('[double]::IsNaN($videoDuration)', script)
        self.assertIn('[double]::IsInfinity($audioDuration)', script)
        self.assertIn('[Math]::Abs($videoDuration - $audioDuration) -gt 1', script)
        self.assertIn('    Assert-RecordedAvTimeline $metadata.streams', script)
        self.assertIn('Test-RecordedAvTimelinePolicy\n\nAdd-Test "network-isolation"', script)
        self.assertIn('@(50.97,30.19)', script)


if __name__ == "__main__":
    unittest.main()
