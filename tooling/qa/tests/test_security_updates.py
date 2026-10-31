"""A security label or advisory URL alone must not bypass release age."""

from __future__ import annotations

import json
import unittest
from datetime import datetime, timedelta, UTC
from pathlib import Path

from tooling.qa.runtime.advisories import AdvisoryError
from tooling.qa.runtime.security_updates import cpython_affected, verify

ROOT = Path(__file__).resolve().parents[3]


class SecurityUpdateTests(unittest.TestCase):
    def setUp(self):
        config = ROOT / "tooling/configs/qa"
        self.catalog = json.loads((config / "tool-versions.json").read_text())
        self.policy = json.loads((config / "environment-audit.json").read_text())
        self.proofs = json.loads((config / "security-update-proofs.json").read_text())
        self.python = {
            "cveMetadata": {"cveId": "CVE-2026-115557", "state": "PUBLISHED"},
            "containers": {
                "cna": {
                    "affected": [
                        {
                            "product": "CPython",
                            "repo": "https://github.com/python/cpython",
                            "defaultStatus": "unaffected",
                            "versions": [
                                {
                                    "version": "3.15.0",
                                    "lessThan": "3.15.2",
                                    "status": "affected",
                                    "versionType": "python",
                                },
                                {
                                    "version": "3.16.0a1",
                                    "lessThan": "3.16.0",
                                    "status": "affected",
                                    "versionType": "python",
                                },
                            ],
                        }
                    ]
                }
            },
        }
        self.nextest = {
            "id": "RUSTSEC-2026-0412",
            "affected": [
                {
                    "package": {"name": "cargo-nextest", "ecosystem": "crates.io"},
                    "ranges": [{"type": "SEMVER", "events": [{"introduced": "0.9.125"}, {"fixed": "0.9.150"}]}],
                }
            ],
        }

        self.minimatch = {
            "id": "GHSA-g3hv-75wg-m249",
            "affected": [
                {
                    "package": {"name": "minimatch", "ecosystem": "npm"},
                    "ranges": [{"type": "SEMVER", "events": [{"introduced": "0"}, {"fixed": "11.0.1"}]}],
                }
            ],
        }

        self.parent_metadata = {
            "name": "minimatch",
            "version": "11.0.1",
            "dependencies": {"@isaacs/nonrecursive-glob-match": "^1.0.1"},
        }
        self.child_metadata = {
            "name": "@isaacs/nonrecursive-glob-match",
            "versions": {"1.0.1": {}},
            "time": {"1.0.1": (datetime.now(UTC) - timedelta(days=4)).isoformat()},
        }

    def source(self, url, payload):
        self.assertIsNone(payload)
        if url.endswith("CVE-2026-115557"):
            return self.python
        if url.endswith("RUSTSEC-2026-0412"):
            return self.nextest
        if url.endswith("GHSA-g3hv-75wg-m249"):
            return self.minimatch
        if url == "https://registry.npmjs.org/minimatch/11.0.1":
            return self.parent_metadata
        if url == "https://registry.npmjs.org/@isaacs/nonrecursive-glob-match":
            return self.child_metadata
        raise AssertionError("unexpected advisory identity")

    def test_actual_fixed_versions_have_both_affected_and_fixed_evidence(self):
        self.assertEqual(
            set(verify(self.catalog, self.policy, self.proofs, self.source)),
            {"python", "cargo-nextest", "minimatch", "@isaacs/nonrecursive-glob-match"},
        )

    def test_vulnerable_target_cannot_use_a_security_exception(self):
        self.catalog["python"]["version"] = "3.15.1"
        self.catalog["securityExceptions"]["python"]["fixedVersion"] = "3.15.1"
        with self.assertRaisesRegex(AdvisoryError, "does not prove"):
            verify(self.catalog, self.policy, self.proofs, self.source)

    def test_old_advisory_cannot_bypass_age_for_an_already_fixed_baseline(self):
        self.catalog["python"]["version"] = "3.15.3"
        self.catalog["securityExceptions"]["python"]["fixedVersion"] = "3.15.3"
        with self.assertRaisesRegex(AdvisoryError, "actual update baseline"):
            verify(self.catalog, self.policy, self.proofs, self.source, baseline={"python": "3.15.2"})

    def test_false_affected_baseline_cannot_manufacture_an_exception(self):
        self.proofs["exceptions"]["cargo-nextest"]["affectedVersion"] = "0.9.124"
        with self.assertRaisesRegex(AdvisoryError, "does not prove"):
            verify(self.catalog, self.policy, self.proofs, self.source)

    def test_missing_proof_wrong_advisory_and_wrong_package_fail_closed(self):
        for mutation in ("proof", "url", "package", "withdrawn"):
            with self.subTest(mutation=mutation):
                self.setUp()
                if mutation == "proof":
                    del self.proofs["exceptions"]["python"]
                elif mutation == "url":
                    self.catalog["securityExceptions"]["python"]["advisory"] = (
                        "https://www.cve.org/CVERecord?id=CVE-2026-0000"
                    )
                elif mutation == "package":
                    self.nextest["affected"][0]["package"]["name"] = "another-crate"
                else:
                    self.nextest["withdrawn"] = "2026-10-30T00:00:00Z"
                with self.assertRaises(AdvisoryError):
                    verify(self.catalog, self.policy, self.proofs, self.source)

    def test_fresh_dependency_must_be_necessary_for_the_verified_fixed_parent(self):
        for mutation in ("range", "parent", "advisory", "aged-alternative"):
            self.setUp()
            name = "@isaacs/nonrecursive-glob-match"
            if mutation == "range":
                self.parent_metadata["dependencies"][name] = "^1.0.0"
            elif mutation == "parent":
                self.proofs["exceptions"][name]["requiredBy"] = "unverified"
            elif mutation == "advisory":
                self.proofs["exceptions"][name]["advisoryId"] = "GHSA-unrelated"
            else:
                self.child_metadata["versions"]["1.0.2"] = {}
                self.child_metadata["time"]["1.0.2"] = (datetime.now(UTC) - timedelta(days=10)).isoformat()
            with self.subTest(mutation=mutation), self.assertRaises(AdvisoryError):
                verify(self.catalog, self.policy, self.proofs, self.source)

    def test_python_prerelease_on_another_branch_does_not_hide_current_fix(self):
        self.assertTrue(cpython_affected(self.python, "CVE-2026-115557", "3.15.1"))
        self.assertFalse(cpython_affected(self.python, "CVE-2026-115557", "3.15.2"))


if __name__ == "__main__":
    unittest.main()
