"""Labels and nested configuration must not waive update timing policy."""

from __future__ import annotations

import copy
import json
import unittest
from pathlib import Path

from tooling.qa.runtime.renovate_policy import check

ROOT = Path(__file__).resolve().parents[3]


class RenovatePolicyTests(unittest.TestCase):
    def setUp(self):
        self.source = (ROOT / "tooling/configs/qa/tool-versions.json").read_text()
        self.catalog = json.loads(self.source)
        self.config = json.loads((ROOT / "renovate.json").read_text())

    def test_exact_catalog_is_covered(self):
        check(self.config, self.catalog, self.source)

    def test_ordinary_fresh_update_and_hidden_age_override_are_rejected(self):
        for mutation in ("age", "missing-timestamp", "nested-age", "automerge", "manager", "preset", "script"):
            value = copy.deepcopy(self.config)
            if mutation == "age":
                value["minimumReleaseAge"] = "6 days"
            elif mutation == "missing-timestamp":
                value["minimumReleaseAgeBehaviour"] = "timestamp-optional"
            elif mutation == "nested-age":
                value["packageRules"].append({"minimumReleaseAge": "0 days"})
            elif mutation == "automerge":
                value["automerge"] = True
            elif mutation == "manager":
                value["customManagers"].pop()
            elif mutation == "preset":
                value["extends"] = ["config:recommended"]
            else:
                value["postUpgradeTasks"] = {"commands": ["untrusted"]}
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                check(value, self.catalog, self.source)


if __name__ == "__main__":
    unittest.main()
