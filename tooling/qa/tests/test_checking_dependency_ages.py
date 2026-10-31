"""Transitive checker updates must not bypass the seven-day hold."""

from __future__ import annotations

import copy
import unittest
from datetime import datetime, UTC
from pathlib import Path
from unittest import mock

from tooling.qa.runtime import checking_dependency_ages as owner
from tooling.qa.runtime.toolchain import check_age, ToolchainError


class CheckingDependencyAgeTests(unittest.TestCase):
    def test_transitive_young_missing_and_foreign_publication_evidence_is_rejected(self):
        versions = {"scriptTools": {}, "updatePolicy": {"minimumReleaseAgeDays": 7}}
        package = ("npm", "fixture", "1.0.0")
        item = {
            "ecosystem": "npm",
            "name": "fixture",
            "version": "1.0.0",
            "source": "https://registry.npmjs.org/fixture",
            "publishedAt": "2020-01-01T00:00:00Z",
        }
        with mock.patch.object(owner, "locked_packages", return_value=[package]):
            owner.check(Path("."), versions, {}, [item], check_age)
            for mutation in ("young", "missing", "foreign", "duplicate"):
                records = [copy.deepcopy(item)]
                if mutation == "young":
                    records[0]["publishedAt"] = datetime.now(UTC).isoformat()
                elif mutation == "missing":
                    records = []
                elif mutation == "foreign":
                    records[0]["source"] = "https://untrusted.invalid/metadata"
                else:
                    records.append(records[0])
                with self.subTest(mutation=mutation), self.assertRaises((ValueError, ToolchainError)):
                    owner.check(Path("."), versions, {}, records, check_age)


if __name__ == "__main__":
    unittest.main()
