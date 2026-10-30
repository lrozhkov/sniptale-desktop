"""Negative proof for tool identity, vulnerable Python, and incomplete advisory sources."""
from __future__ import annotations

import copy
import json
import unittest
from pathlib import Path

from tooling.qa.runtime.advisories import AdvisoryError, cpython_query, decode, nvd_query, osv_queries
from tooling.qa.runtime.environment_audit import audit

ROOT = Path(__file__).resolve().parents[3]
VULNERABLE_PYTHON_3121 = "2305ca51448552542b2414186252123a8dc87db7"


class EnvironmentAuditTests(unittest.TestCase):
    def setUp(self):
        self.catalog = json.loads((ROOT / "tooling/configs/qa/tool-versions.json").read_text())
        self.policy = json.loads((ROOT / "tooling/configs/qa/environment-audit.json").read_text())
        self.calls = []

    def source(self, url, payload):
        self.calls.append((url, payload))
        if "git/ref/tags/v3.12.1" in url:
            return {"object": {"type": "commit", "sha": VULNERABLE_PYTHON_3121}}
        if "git/ref/tags/" in url:
            return {"object": {"type": "commit", "sha": "a" * 40}}
        if "api.osv.dev" in url:
            if payload == {"commit": VULNERABLE_PYTHON_3121}:
                # PSF's upstream record: advisories/python/PSF-2024-2.json,
                # aliases CVE-2024-0450, CPython 3.12.1 is before its fix.
                return {"vulns": [{"id": "PSF-2024-2", "aliases": ["CVE-2024-0450"]}]}
            return {}
        if "services.nvd.nist.gov" in url:
            return {"totalResults": 0, "startIndex": 0, "vulnerabilities": []}
        raise AssertionError("unexpected source")

    def test_known_vulnerable_python_fails_the_whole_environment_audit(self):
        self.catalog["python"]["version"] = "3.12.1"
        report = audit(self.catalog, self.policy, self.source)
        self.assertEqual(report["result"], "fail")
        python = next(tool for tool in report["tools"] if tool["tool"] == "python")
        self.assertEqual(python["advisories"], ["PSF-2024-2"])
        self.assertTrue(any(payload == {"commit": VULNERABLE_PYTHON_3121} for _, payload in self.calls))
        self.assertFalse(any(payload and payload.get("package", {}).get("name") == "python" for _, payload in self.calls))

    def test_clean_results_cover_every_exact_catalog_pin(self):
        report = audit(self.catalog, self.policy, self.source)
        self.assertEqual(report["result"], "pass")
        self.assertEqual(len(report["tools"]), len(self.policy["tools"]))
        self.assertTrue(report["platformGaps"])
        self.assertIn("not all bundled", report["coverageLimit"])

    def test_missing_tool_mapping_is_not_silently_uncovered(self):
        del self.policy["tools"]["wix"]
        with self.assertRaisesRegex(AdvisoryError, "coverage declaration"):
            audit(self.catalog, self.policy, self.source)

    def test_source_failure_cannot_be_treated_as_a_clean_run(self):
        def unavailable(url, payload):
            raise AdvisoryError("source unavailable")
        with self.assertRaisesRegex(AdvisoryError, "unavailable"):
            audit(self.catalog, self.policy, unavailable)

    def test_explicit_coverage_gap_is_reported_with_reason(self):
        self.policy["tools"]["wix"] = {"provider": "unavailable", "reason": "fixture unsupported ecosystem", "reviewSource": "vendor"}
        report = audit(self.catalog, self.policy, self.source)
        tool = next(tool for tool in report["tools"] if tool["tool"] == "wix")
        self.assertEqual(tool["coverage"], "manual-review-required")
        self.assertEqual(tool["reason"], "fixture unsupported ecosystem")

    def test_duplicate_json_and_incomplete_source_pages_are_rejected(self):
        with self.assertRaisesRegex(AdvisoryError, "duplicate"):
            decode(b'{"result":"pass","result":"fail"}')
        with self.assertRaisesRegex(AdvisoryError, "incomplete NVD"):
            nvd_query("python", "python", "3.12.1", lambda *_: {"totalResults": 2, "startIndex": 0, "vulnerabilities": []})
        with self.assertRaisesRegex(AdvisoryError, "pagination"):
            osv_queries([{"commit": "a" * 40}], lambda *_: {"next_page_token": "repeated"})

    def test_release_tag_cannot_redirect_commit_identity(self):
        with self.assertRaisesRegex(AdvisoryError, "reference"):
            cpython_query("3.12.1", lambda *_: {"object": {"type": "commit", "sha": "not-a-commit"}})


if __name__ == "__main__":
    unittest.main()
