"""Reject incomplete dependency audits and non-reproducible checking inputs."""

from __future__ import annotations

import unittest

from tooling.qa.runtime.advisories import AdvisoryError, osv_batch
from tooling.qa.runtime.tool_dependencies import npm_packages, python_packages
from tooling.qa.rules.verify_script_quality import suppression_errors


class ToolDependencyTests(unittest.TestCase):
    def test_npm_lock_requires_registry_identity_and_integrity(self):
        item = {
            "version": "1.2.3",
            "resolved": "https://registry.npmjs.org/example/-/example-1.2.3.tgz",
            "integrity": "sha512-" + "a" * 86 + "==",
        }
        lock = {"lockfileVersion": 3, "packages": {"": {}, "node_modules/example": item}}
        self.assertEqual(npm_packages(lock), [("npm", "example", "1.2.3")])
        for field, value in (("resolved", "file:local"), ("integrity", "missing"), ("version", "^1.2.3")):
            with self.subTest(field=field), self.assertRaises(AdvisoryError):
                npm_packages({"lockfileVersion": 3, "packages": {"node_modules/example": {**item, field: value}}})

    def test_python_lock_rejects_unpinned_or_unhashed_requirements(self):
        self.assertEqual(
            python_packages("Some_Name==1.2.3 --hash=sha256:" + "a" * 64), [("PyPI", "some-name", "1.2.3")]
        )
        for value in ("example>=1.2.3", "example==1.2.3", "--extra-index-url https://unapproved.invalid"):
            with self.subTest(value=value), self.assertRaises(AdvisoryError):
                python_packages(value)

    def test_batch_cannot_drop_or_reorder_query_results(self):
        queries = [{"package": {"ecosystem": "npm", "name": name}, "version": "1.2.3"} for name in ("first", "second")]
        result = osv_batch(queries, lambda *_: {"results": [{}, {"vulns": [{"id": "GHSA-aaaa-bbbb-cccc"}]}]})
        self.assertEqual(result, [[], ["GHSA-aaaa-bbbb-cccc"]])
        with self.assertRaisesRegex(AdvisoryError, "incomplete"):
            osv_batch(queries, lambda *_: {"results": [{}]})
        with self.assertRaisesRegex(AdvisoryError, "identifiers"):
            osv_batch(queries, lambda *_: {"results": [{"vulns": [None]}, {}]})

    def test_broad_or_unexplained_suppressions_are_rejected(self):
        fixtures = [
            ("bad.py", "value = missing  # noqa\n"),
            ("bad.py", "# pyright: ignore\n"),
            ("bad.sh", "# shellcheck disable=SC2086\necho $value\n"),
            ("bad.cjs", "// eslint-disable no-eval\n"),
        ]
        for name, source in fixtures:
            with self.subTest(name=name):
                self.assertTrue(suppression_errors(name, source))
        self.assertFalse(suppression_errors("valid.py", 'value = "# noqa"\n'))
        self.assertFalse(
            suppression_errors(
                "valid.cjs",
                "// eslint-disable-next-line no-eval -- This exact fixture exercises a rejected dynamic expression.\n",
            )
        )


if __name__ == "__main__":
    unittest.main()
