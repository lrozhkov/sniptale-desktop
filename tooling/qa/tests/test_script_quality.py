"""Negative proof for configuration parsing and mandatory tool inventory."""

from __future__ import annotations

import unittest

from tooling.qa.runtime.advisories import AdvisoryError, decode
from tooling.qa.runtime import script_lints


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


if __name__ == "__main__":
    unittest.main()
