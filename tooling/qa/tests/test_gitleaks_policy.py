from __future__ import annotations

import importlib.util
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]
SPEC = importlib.util.spec_from_file_location("verify_gitleaks_test", ROOT / "tooling/qa/rules/verify_gitleaks.py")
assert SPEC is not None and SPEC.loader is not None
VERIFY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VERIFY)


class GitleaksPolicyTests(unittest.TestCase):
    def test_accepts_exact_redacted_fingerprint(self) -> None:
        finding = {
            "RuleID": "generic-api-key",
            "File": "fixture.json",
            "StartLine": 1,
            "Fingerprint": "fixture:rule:1",
            "Secret": "REDACTED",
            "Commit": "",
        }
        baseline = [
            {**finding, "SniptaleScope": "worktree", "SniptaleEnabled": True, "SniptaleRationale": "public fixture"}
        ]
        self.assertEqual(VERIFY.validate([finding], baseline, "worktree"), [])

    def test_accepts_commit_independent_both_scope_fingerprint(self) -> None:
        commit = "a" * 40
        finding = {
            "RuleID": "generic-api-key",
            "File": "fixture.json",
            "StartLine": 1,
            "Fingerprint": f"{commit}:fixture:rule:1",
            "Secret": "REDACTED",
            "Commit": commit,
        }
        baseline = [
            {
                "RuleID": "generic-api-key",
                "File": "fixture.json",
                "StartLine": 1,
                "Fingerprint": "fixture:rule:1",
                "Secret": "REDACTED",
                "Commit": "",
                "SniptaleScope": "both",
                "SniptaleEnabled": True,
                "SniptaleRationale": "public fixture",
            }
        ]
        self.assertEqual(VERIFY.validate([finding], baseline, "history"), [])

    def test_disabled_history_fingerprint_is_retained_but_does_not_suppress(self) -> None:
        commit = "a" * 40
        finding = {
            "RuleID": "generic-api-key",
            "File": "old/fixture.json",
            "StartLine": 1,
            "Fingerprint": f"{commit}:old/fixture.json:generic-api-key:1",
            "Secret": "REDACTED",
            "Commit": commit,
        }
        baseline = [
            {
                **finding,
                "SniptaleScope": "history",
                "SniptaleEnabled": False,
                "SniptaleRationale": "retained disabled proof",
            }
        ]
        self.assertEqual(VERIFY.validate([], baseline, "history"), [])
        self.assertTrue(any("unreviewed" in error for error in VERIFY.validate([finding], baseline, "history")))

    def test_rejects_new_stale_unredacted_and_history_without_commit(self) -> None:
        finding = {
            "RuleID": "generic-api-key",
            "File": "fixture.json",
            "StartLine": 1,
            "Fingerprint": "new",
            "Secret": "raw",
            "Commit": "",
        }
        baseline = [{"Fingerprint": "stale", "SniptaleScope": "history", "SniptaleEnabled": True}]
        errors = VERIFY.validate([finding], baseline, "history")
        self.assertTrue(any("not fully redacted" in error for error in errors))
        self.assertTrue(any("lacks an exact commit" in error for error in errors))
        self.assertTrue(any("unreviewed" in error for error in errors))
        self.assertTrue(any("stale" in error for error in errors))

    def test_sanitized_evidence_omits_author_message_and_match(self) -> None:
        finding = {
            "RuleID": "rule",
            "File": "file",
            "StartLine": 1,
            "Commit": "a" * 40,
            "Fingerprint": "fingerprint",
            "Secret": "raw",
            "Author": "person",
            "Email": "private@example.invalid",
            "Message": "private",
            "Match": "raw",
        }
        sanitized = VERIFY.sanitize([finding], "history")
        self.assertEqual(
            sanitized,
            [
                {
                    "RuleID": "rule",
                    "File": "file",
                    "StartLine": 1,
                    "Commit": "a" * 40,
                    "Fingerprint": "fingerprint",
                    "Secret": "REDACTED",
                    "SniptaleScope": "history",
                }
            ],
        )


if __name__ == "__main__":
    unittest.main()
