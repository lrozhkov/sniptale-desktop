from __future__ import annotations

import contextlib
import io
import tempfile
import unittest
from pathlib import Path

from tooling.qa.rules import english_docs


class EnglishDocumentationTests(unittest.TestCase):
    def check(self, text: str, relative: str = "AGENTS.md") -> tuple[list[str], str]:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "probe.md"
            path.write_text(text, encoding="utf-8")
            output = io.StringIO()
            with contextlib.redirect_stdout(output):
                errors = english_docs.english_errors(path, relative)
            return errors, output.getvalue()

    def test_sentence_boundary_and_semicolon_block(self) -> None:
        self.assertEqual(self.check(" ".join(["word"] * 25) + ".")[0], [])
        self.assertTrue(self.check(" ".join(["word"] * 26) + ".")[0])
        self.assertTrue(self.check("Read the file; inspect its contents.")[0])

    def test_advice_does_not_fail_or_disappear(self) -> None:
        errors, output = self.check("Check the file. Validate the signature. The file is deleted.")
        self.assertEqual(errors, [])
        self.assertIn("ADVICE English:", output)
        self.assertIn("synonym-rotation", output)
        self.assertIn("passive-voice", output)

    def test_code_frontmatter_and_link_targets_preserve_diagnostic_lines(self) -> None:
        text = (
            "---\ndescription: a; b\n---\n\n```sh\na;b\n```\n`a;b` [source](https://example.test/a;b)\nRead this; stop."
        )
        errors, _ = self.check(text)
        self.assertEqual(len(errors), 1)
        self.assertTrue(errors[0].startswith("AGENTS.md:9: semicolon"))

    def test_only_exact_generated_notice_is_excluded(self) -> None:
        self.assertEqual(self.check("Legal; text.", "THIRD_PARTY_NOTICES.md")[0], [])
        self.assertTrue(self.check("Legal; text.", "docs/THIRD_PARTY_NOTICES.md")[0])
        self.assertTrue(self.check("Legal; text.", ".agents/skills/example/SKILL.md")[0])

    def test_uncertainty_survives_without_a_warning(self) -> None:
        self.assertEqual(self.check("The request may have failed. It could be a timeout."), ([], ""))
