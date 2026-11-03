from __future__ import annotations

import copy
import hashlib
import tempfile
import unittest
from datetime import date
from pathlib import Path

from tooling.qa.runtime.finding_registry import classify, finding, validate
from tooling.qa.runtime.static_analysis import codeql_findings, duplication_findings, location, source_policy_errors


class FindingRegistryTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        (self.root / "source.py").write_text("first\nsecond\n")
        self.item = finding("codeql", "py/test-rule", [location(self.root, "source.py", 1, 1)], "Unsafe value", True)
        self.record = {
            "id": "noise.test",
            "classification": "tool-noise",
            "owner": "qa",
            "rationale": "Synthetic fixture only.",
            "risk": "A broader entry could hide real findings.",
            "reviewBy": "2027-01-01",
            "removalCondition": "Remove when the rule rejects this fixture.",
            "targetAction": "Review the exact fixture.",
            "source": {key: self.item[key] for key in ("tool", "rule", "fingerprint")},
            "exactScope": {
                "exactPaths": ["source.py"],
                "locations": [
                    {
                        "path": "source.py",
                        "startLine": 1,
                        "endLine": 1,
                        "sourceSha256": hashlib.sha256(b"first").hexdigest(),
                    }
                ],
            },
            "proofReferences": ["source.py"],
        }

    def test_exact_noise_and_stale_or_changed_findings(self) -> None:
        self.assertEqual(validate([self.record], self.root, date(2026, 11, 1)), [])
        self.assertEqual(classify([copy.deepcopy(self.item)], [self.record]), ([], []))
        changed = finding("codeql", "py/test-rule", [location(self.root, "source.py", 2, 2)], "Unsafe value", True)
        remaining, stale = classify([changed], [self.record])
        self.assertEqual(remaining, [changed])
        self.assertTrue(stale)
        self.assertTrue(classify([], [self.record])[1])
        (self.root / "source.py").write_text("changed\nsecond\n")
        self.assertTrue(validate([self.record], self.root, date(2026, 11, 1)))

    def test_debt_never_suppresses_a_blocking_codeql_finding(self) -> None:
        self.record["classification"] = "temporary-debt"
        remaining, errors = classify([self.item], [self.record])
        self.assertEqual(remaining, [self.item])
        self.assertEqual(errors, [])

    def test_expiry_duplicates_broad_scopes_and_missing_paths_fail(self) -> None:
        self.assertTrue(validate([self.record], self.root, date(2027, 1, 2)))
        self.assertTrue(validate([self.record, self.record], self.root, date(2026, 11, 1)))
        for paths in [["**/*.py"], ["../outside.py"], ["missing.py"], [], [42]]:
            with self.subTest(paths=paths):
                record = copy.deepcopy(self.record)
                record["exactScope"]["exactPaths"] = paths
                self.assertTrue(validate([record], self.root, date(2026, 11, 1)))
        for field in ["owner", "rationale", "risk", "removalCondition", "targetAction", "proofReferences"]:
            record = copy.deepcopy(self.record)
            record.pop(field)
            self.assertTrue(validate([record], self.root, date(2026, 11, 1)))

    def test_inline_suppression_cannot_hide_a_finding(self) -> None:
        (self.root / "source.py").write_text("# lgtm[py/test-rule]\nvalue = 1\n")
        self.assertTrue(source_policy_errors(self.root, ["source.py"]))
        (self.root / "source.py").write_text("value = 1\n")
        self.assertEqual(source_policy_errors(self.root, ["source.py"]), [])

    def test_fingerprint_tracks_content_and_source_bounds(self) -> None:
        (self.root / "source.py").write_text("prefix\nfirst\nsecond\n")
        shifted = finding("codeql", "py/test-rule", [location(self.root, "source.py", 2, 2)], "Unsafe value", True)
        self.assertNotEqual(shifted["fingerprint"], self.item["fingerprint"])
        with self.assertRaises(ValueError):
            location(self.root, "../outside.py", 1, 1)


class AnalyzerReportTests(unittest.TestCase):
    def report(self) -> dict:
        return {
            "version": "2.1.0",
            "runs": [
                {
                    "tool": {"driver": {"name": "CodeQL", "rules": [{"id": "py/test"}]}},
                    "invocations": [{"executionSuccessful": True}],
                    "results": [],
                }
            ],
        }

    def test_complete_zero_finding_report_passes_and_incomplete_reports_fail(self) -> None:
        self.assertEqual(codeql_findings(self.report()), [])
        for value in [{}, {"version": "2.1.0", "runs": []}]:
            with self.assertRaises(ValueError):
                codeql_findings(value)
        for key in ["invocations", "results", "tool"]:
            value = self.report()
            del value["runs"][0][key]
            with self.assertRaises(ValueError):
                codeql_findings(value)
        value = self.report()
        value["runs"][0]["invocations"][0]["executionSuccessful"] = False
        with self.assertRaises(ValueError):
            codeql_findings(value)

    def test_empty_duplication_scan_is_not_clean_evidence(self) -> None:
        for value in [{}, {"statistics": {"total": {"sources": 0}}, "duplicates": []}]:
            with self.assertRaises(ValueError):
                duplication_findings(value)
        self.assertEqual(duplication_findings({"statistics": {"total": {"sources": 1}}, "duplicates": []}), [])
