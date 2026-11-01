"""Apply the repository STE checker to authored English Markdown."""

from __future__ import annotations

import re
from pathlib import Path

from tooling.qa.rules import ste_lint


ADVISORY_RULES = {"synonym-rotation", "passive-voice", "present-perfect"}
# This notice is generator-owned and byte-checked by the legal gate.
GENERATED_NOTICE = "THIRD_PARTY_NOTICES.md"


def prose_input(text: str) -> str:
    """Preserve source line numbers while masking non-prose Markdown."""
    result = []
    lines = text.splitlines()
    frontmatter = bool(lines and lines[0] == "---")
    for index, line in enumerate(lines):
        if frontmatter:
            result.append("")
            if index and line == "---":
                frontmatter = False
            continue
        # Preserve visible labels. URLs and inline code are technical syntax.
        line = re.sub(r"(!?\[[^\]]*\])\([^)]*\)", r"\1", line)
        result.append(line)
    return "\n".join(result)


def english_errors(path: Path, relative: str) -> list[str]:
    if relative == GENERATED_NOTICE:
        return []
    findings, _ = ste_lint.lint(prose_input(path.read_text(encoding="utf-8")), relative)
    errors = []
    for finding in findings:
        label = f"{relative}:{finding['line']}: {finding['rule']}: {finding['message']}"
        if finding["level"] == "advisory" or finding["rule"] in ADVISORY_RULES:
            print(f"ADVICE English: {label}")
        else:
            errors.append(label)
    return errors
