"""Negative proof for configuration parsing and mandatory tool inventory."""

from __future__ import annotations

import subprocess
import unittest

from tooling.qa.runtime import script_lints
from tooling.qa.runtime.advisories import AdvisoryError, decode


class ScriptInventoryTests(unittest.TestCase):
    def test_every_script_language_has_a_lane(self):
        files = script_lints.inventory()
        for language in ("python", "shell", "powershell", "javascript", "yaml", "toml", "json"):
            self.assertTrue(files[language], language)
        self.assertIn("tooling/desktop/e2e-lab", files["shell"])
        self.assertIn("tooling/qa/runtime/script-tools-env.sh", files["shell"])

    def test_duplicate_nested_json_key_is_rejected(self):
        with self.assertRaises(AdvisoryError):
            decode(b'{"rules":{"enabled":true,"enabled":false}}')

    def test_history_fixture_record_variants_remain_closed(self):
        result = subprocess.run(
            [str(script_lints.CACHE / "python/bin/python"), "-m", "tooling.qa.runtime.json_contracts", "--self-test"],
            cwd=script_lints.ROOT,
            capture_output=True,
            text=True,
            check=False,
            timeout=30,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_windows_smoke_rejects_invalid_origin_after_pure_framing_proof(self):
        versions = decode((script_lints.ROOT / "tooling/configs/qa/tool-versions.json").read_bytes())
        version = versions["scriptTools"]["pwsh"]["version"]
        result = subprocess.run(
            [
                str(script_lints.CACHE / f"pwsh-{version}/pwsh"),
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-File",
                "tooling/release/windows/smoke/windows-smoke.ps1",
                "-ExtensionOrigin",
                "invalid",
            ],
            cwd=script_lints.ROOT,
            capture_output=True,
            text=True,
            check=False,
            timeout=20,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("OK native-host probe framing self-tests", result.stdout)
        self.assertIn("Extension origin must be exact", result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
