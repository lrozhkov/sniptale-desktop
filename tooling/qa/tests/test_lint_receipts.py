"""A receipt must prove the exact source and every mandatory lint lane."""

from __future__ import annotations

import copy
import unittest
from unittest import mock
from datetime import UTC, datetime, timedelta

from tooling.qa.runtime.lint_receipts import REQUIRED, validate
from tooling.qa.runtime import lint_receipts as owner


class LintReceiptTests(unittest.TestCase):
    def setUp(self):
        self.now = datetime(2026, 10, 31, tzinfo=UTC)
        self.source = {"revision": "a" * 40, "files": [], "fingerprint": "b" * 64}
        self.record = {
            "wrapperId": "qa:script-lints",
            "status": "all-passed",
            "exitCode": 0,
            "repository": {"revision": self.source["revision"]},
            "lintScope": self.source,
            "finishedAt": self.now.isoformat(),
            "steps": [
                {"label": label, "outcome": "passed", "exitCode": 0, "problemIds": []} for label in sorted(REQUIRED)
            ],
        }

    def test_complete_identical_proof_is_accepted(self):
        validate(self.record, self.source)

    def test_identical_static_proof_does_not_expire_with_time(self):
        self.record["finishedAt"] = (self.now - timedelta(days=30)).isoformat()
        validate(self.record, self.source)

    def test_missing_failed_skipped_duplicate_and_changed_checks_are_rejected(self):
        for mutation in ("missing", "failed", "skipped", "duplicate", "scope", "producer", "source"):
            record = copy.deepcopy(self.record)
            if mutation == "missing":
                record["steps"].pop()
            elif mutation == "failed":
                record["steps"][0]["exitCode"] = 1
            elif mutation == "skipped":
                record["steps"][0]["outcome"] = "skipped"
            elif mutation == "duplicate":
                record["steps"].append(record["steps"][0])
            elif mutation == "scope":
                record["lintScope"]["fingerprint"] = "c" * 64
            elif mutation == "producer":
                record["wrapperId"] = "qa:checkpoint"
            else:
                record["repository"]["revision"] = "c" * 40
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                validate(record, self.source)

    def test_ci_proof_rejects_another_run_attempt_or_commit(self):
        identity = {
            "GITHUB_REPOSITORY": "owner/repository",
            "GITHUB_RUN_ID": "42",
            "GITHUB_RUN_ATTEMPT": "1",
            "GITHUB_SHA": self.source["revision"],
        }
        envelope = {"schemaVersion": 1, "context": identity, "record": self.record}
        environment = {
            **identity,
            "GITHUB_ACTIONS": "true",
            "SNIPTALE_QA_LINT_PROOF": ".tmp/qa-artifacts/script-lint-proof.json",
        }
        with (
            mock.patch.dict(owner.os.environ, environment, clear=True),
            mock.patch.object(owner, "read_json", return_value=envelope),
            mock.patch.object(owner, "current_source", return_value=self.source),
        ):
            owner.reuse()
            for key in identity:
                with (
                    self.subTest(key=key),
                    mock.patch.dict(owner.os.environ, {key: "different"}),
                    self.assertRaisesRegex(ValueError, "another workflow"),
                ):
                    owner.reuse()

    def test_requirement_changes_invalidate_proof_but_rust_only_edits_do_not(self):
        import tempfile
        from pathlib import Path
        from tooling.qa.runtime import repository

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            requirement = root / "tooling/configs/qa/script-python-requirements.txt"
            requirement.parent.mkdir(parents=True)
            requirement.write_text("dependency==1.0.0")
            with (
                mock.patch.object(owner, "inventory", return_value={}),
                mock.patch.object(owner, "revision", return_value="a" * 40),
                mock.patch.object(repository, "ROOT", root),
                mock.patch.object(repository, "revision", return_value="a" * 40),
            ):
                before = owner.current_source()
                (root / "feature.rs").write_text("fn feature() {}")
                self.assertEqual(owner.current_source(), before)
                requirement.write_text("dependency==2.0.0")
                self.assertNotEqual(owner.current_source(), before)


if __name__ == "__main__":
    unittest.main()
