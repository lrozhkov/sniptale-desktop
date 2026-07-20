#!/usr/bin/env python3
from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path
from urllib.parse import unquote


ROOT = Path(__file__).resolve().parents[3]
POLICY = ROOT / "tooling/configs/qa/docs.json"
LINK = re.compile(r"!?(?:\[[^\]]*\])\(([^)]+)\)")
LIST_ITEM = re.compile(r"^\s*(?:[-+*]\s+|\d+[.)]\s+)")
TABLE_SEPARATOR = re.compile(r"^\s*\|?(?:\s*:?-+:?\s*\|)+\s*:?-+:?\s*\|?\s*$")


def git_paths(*args: str) -> list[str]:
    result = subprocess.run(["git", *args], cwd=ROOT, check=False, capture_output=True, text=True)
    if result.returncode not in {0, 1}:
        raise RuntimeError(result.stderr.strip())
    return result.stdout.splitlines()


def visible_markdown() -> set[str]:
    return {
        path
        for path in git_paths("ls-files", "-co", "--exclude-standard", "*.md")
        if (ROOT / path).is_file()
    }


def prose_errors(path: Path) -> list[str]:
    errors: list[str] = []
    try:
        label = path.relative_to(ROOT).as_posix()
    except ValueError:
        label = path.as_posix()
    lines = path.read_text(encoding="utf-8").splitlines()
    in_fence = False
    in_frontmatter = bool(lines and lines[0].strip() == "---")
    previous_kind: str | None = None
    for number, raw in enumerate(lines, start=1):
        stripped = raw.strip()
        if in_frontmatter:
            if number > 1 and stripped == "---":
                in_frontmatter = False
            continue
        if stripped.startswith("```") or stripped.startswith("~~~"):
            in_fence = not in_fence
            previous_kind = None
            continue
        if in_fence:
            continue
        if not stripped:
            previous_kind = None
            continue
        if stripped.startswith("#"):
            kind = "heading"
        elif LIST_ITEM.match(raw):
            kind = "list"
        elif raw.startswith("    ") or stripped.startswith((">", "<", "::", "[//]:", "<!--")):
            kind = "structural"
        elif "|" in stripped and (stripped.startswith("|") or TABLE_SEPARATOR.match(raw)):
            kind = "table"
        elif stripped in {"---", "***", "___"}:
            kind = "structural"
        else:
            kind = "prose"
        if kind == "prose" and previous_kind in {"prose", "list"}:
            errors.append(f"{label}:{number} continues a prose/list paragraph on a new physical line")
        if kind == "list" and previous_kind == "prose":
            errors.append(f"{label}:{number} starts a list without a blank line")
        if kind == "heading" and previous_kind is not None:
            errors.append(f"{label}:{number} starts a heading without a blank line")
        if previous_kind == "heading":
            errors.append(f"{label}:{number} follows a heading without a blank line")
        previous_kind = kind
    return errors


def local_link_errors(path: Path) -> list[str]:
    errors: list[str] = []
    in_fence = False
    for number, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        stripped = raw.strip()
        if stripped.startswith("```") or stripped.startswith("~~~"):
            in_fence = not in_fence
            continue
        if in_fence:
            continue
        for match in LINK.finditer(raw):
            target = unquote(match.group(1).split("#", 1)[0].strip().strip("<>"))
            if not target or re.match(r"^[a-z][a-z0-9+.-]*:", target, flags=re.I):
                continue
            resolved = (path.parent / target).resolve()
            if not resolved.is_relative_to(ROOT) or not resolved.exists():
                errors.append(f"{path.relative_to(ROOT)}:{number} has missing local link {target}")
    return errors


def policy_shape_errors(policy: dict, index: str) -> list[str]:
    errors: list[str] = []
    active_values = policy["activeDocs"]
    repository_values = policy["repositoryMarkdown"]
    if len(active_values) != len(set(active_values)):
        errors.append("activeDocs contains duplicate paths")
    if len(repository_values) != len(set(repository_values)):
        errors.append("repositoryMarkdown contains duplicate paths")
    overlap = set(active_values) & set(repository_values)
    if overlap:
        errors.append(f"Markdown classifications overlap: {sorted(overlap)}")
    for relative in sorted(set(active_values) - {"docs/README.md"}):
        target = Path(relative).relative_to("docs").as_posix()
        count = index.count(f"({target})")
        if count != 1:
            errors.append(f"docs/README.md must index {relative} exactly once, found {count}")
    return errors


def validate() -> list[str]:
    errors: list[str] = []
    policy = json.loads(POLICY.read_text(encoding="utf-8"))
    index = (ROOT / "docs/README.md").read_text(encoding="utf-8")
    errors.extend(policy_shape_errors(policy, index))
    active = set(policy["activeDocs"])
    expected = active | set(policy["repositoryMarkdown"])
    actual = visible_markdown()
    if expected != actual:
        errors.append(f"Markdown topology drift: unclassified={sorted(actual - expected)} stale={sorted(expected - actual)}")
    for relative in sorted(expected):
        path = ROOT / relative
        errors.extend(prose_errors(path))
        errors.extend(local_link_errors(path))
        text = path.read_text(encoding="utf-8")
        for fragment in policy["forbiddenFragments"]:
            if fragment in text:
                errors.append(f"{relative} contains forbidden fragment {fragment}")
        ignored = subprocess.run(["git", "check-ignore", "-q", relative], cwd=ROOT, check=False).returncode == 0
        if ignored:
            errors.append(f"active Markdown is ignored by .gitignore: {relative}")
    for relative in policy["legalText"]:
        path = ROOT / relative
        if not path.is_file():
            errors.append(f"missing legal text: {relative}")
            continue
        lines = path.read_text(encoding="utf-8").splitlines()
        for number in range(1, len(lines)):
            if lines[number - 1].strip() and lines[number].strip():
                errors.append(f"{relative}:{number + 1} continues prose on a new physical line")
    for probe in (".tmp/docs-policy/probe.md", ".artifacts/docs-policy/probe.md"):
        if subprocess.run(["git", "check-ignore", "-q", probe], cwd=ROOT, check=False).returncode != 0:
            errors.append(f"generated/local documentation probe is not ignored: {probe}")
    return errors


def main() -> int:
    errors = validate()
    if errors:
        for error in errors:
            print(f"FAIL {error}", file=sys.stderr)
        return 1
    print("OK documentation topology, links, ignore policy, and natural paragraphs")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
