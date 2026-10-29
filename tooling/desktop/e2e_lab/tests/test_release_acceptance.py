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

    def test_release_guest_contract_has_all_cases_and_no_development_trust(self):
        script = (Path(__file__).resolve().parents[1] / "guest/windows/release-run.ps1").read_text()
        for name in CASES:
            self.assertIn(f'Add-Test "{name}"', script)
        self.assertNotIn("Provision-DevRuntimeTrust", script)
        self.assertNotIn("protectedSecretHex", script)
        self.assertNotIn('WriteAllText("$directory/local-profile.json"', script)
        self.assertIn('Tray @("Mode","Manual")', script)


if __name__ == "__main__":
    unittest.main()
