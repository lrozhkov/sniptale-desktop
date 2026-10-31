"""Keep all supported sources covered and lint suppressions narrow and explained."""

from __future__ import annotations

import io
import json
import re
import tokenize
import tomllib
from pathlib import Path

from tooling.qa.runtime.repository import ROOT
from tooling.qa.runtime.script_lints import inventory
from tooling.qa.runtime.tool_dependencies import python_packages


def suppression_errors(name: str, source: str) -> list[str]:
    path = Path(name)
    if path.suffix == ".py":
        comments = [
            (token.start[0], token.string)
            for token in tokenize.generate_tokens(io.StringIO(source).readline)
            if token.type == tokenize.COMMENT
        ]
    else:
        comments = [
            (index, line.strip())
            for index, line in enumerate(source.splitlines(), 1)
            if line.lstrip().startswith(("#", "//", "/*"))
        ]
    errors = []
    lines = source.splitlines()
    for number, comment in comments:
        if (re.search(r"\bnoqa\b", comment)) and (
            not re.search(r"noqa:\s*[A-Z]+[0-9]+(?:\s*,\s*[A-Z]+[0-9]+)*\s+--\s+.{20,}", comment)
        ):
            errors.append(f"{name}:{number}: noqa requires explicit codes and a reason")
        if ("pyright:" in comment and "ignore" in comment) and (
            not re.search(r"pyright:\s*ignore\[report[A-Za-z]+(?:\s*,\s*report[A-Za-z]+)*\]\s+#\s+.{20,}", comment)
        ):
            errors.append(f"{name}:{number}: Pyright suppression requires explicit diagnostics and a reason")
        if (re.search(r"type:\s*ignore", comment)) and (
            not re.search(r"type:\s*ignore\[[A-Za-z0-9_, -]+\]\s+#\s+.{20,}", comment)
        ):
            errors.append(f"{name}:{number}: type suppression requires explicit diagnostics and a reason")
        if ("eslint-disable" in comment) and (
            not re.search(
                r"eslint-disable-(?:next-line|line)\s+[@A-Za-z0-9_/-]+(?:\s*,\s*[@A-Za-z0-9_/-]+)*\s+--\s+.{20,}",
                comment,
            )
        ):
            errors.append(f"{name}:{number}: ESLint suppression must be local, named and explained")
        if "shellcheck disable=" in comment:
            reason = lines[number - 2].strip() if number > 1 else ""
            if (
                re.fullmatch(r"# shellcheck disable=SC[0-9]{4}(?:,SC[0-9]{4})*", comment) is None
                or not reason.startswith("# ")
                or len(reason) < 24
                or "shellcheck" in reason
            ):
                errors.append(f"{name}:{number}: ShellCheck suppression requires named rules and a preceding reason")
    return errors


def check() -> list[str]:
    files = inventory()
    errors = []
    for category in ("python", "shell", "powershell", "javascript"):
        for name in files[category]:
            errors.extend(suppression_errors(name, (ROOT / name).read_text()))
    config = ROOT / "tooling/configs/qa"
    ruff = tomllib.loads((config / "ruff.toml").read_text())
    if any(key in ruff for key in ("exclude", "extend-exclude", "force-exclude")) or any(
        key in ruff.get("lint", {})
        for key in ("ignore", "extend-ignore", "per-file-ignores", "extend-per-file-ignores")
    ):
        errors.append("Ruff policy must not use broad exclusions or ignored rules")
    pyright = json.loads((config / "pyrightconfig.json").read_text())
    if (
        pyright.get("typeCheckingMode") != "standard"
        or pyright.get("exclude")
        or pyright.get("ignore")
        or any(key.startswith("report") and value in (False, "none") for key, value in pyright.items())
    ):
        errors.append("Pyright must check all inventoried Python sources without disabled diagnostics")
    ps = (config / "PSScriptAnalyzer.psd1").read_text()
    if re.search(r"\b(?:ExcludeRules|IgnoreTypes|IgnoreCommands)\b", ps) or "TargetVersions = @('5.1')" not in ps:
        errors.append("PowerShell policy must preserve explicit rules and Windows PowerShell 5.1 compatibility")
    eslint = (config / "eslint.config.cjs").read_text()
    if "reportUnusedDisableDirectives: 'error'" not in eslint or re.search(
        r"['\"](?:off|warn)['\"]|\bignores\s*:", eslint
    ):
        errors.append("ESLint must reject unused suppressions and must not downgrade rules or ignore files")
    catalog = json.loads((config / "tool-versions.json").read_text())
    npm = json.loads((config / "script-node/package.json").read_text())
    expected_npm = {
        item["package"]: item["version"]
        for name, item in catalog["scriptTools"].items()
        if item["manager"] == "npm" and name != "renovate"
    }
    if npm["dependencies"] != expected_npm or npm.get("overrides") != {
        "minimatch": "$minimatch",
        "@isaacs/nonrecursive-glob-match": "$@isaacs/nonrecursive-glob-match",
        "@types/estree": "$@types/estree",
    }:
        errors.append("npm checker pins and reviewed security override must match the common catalog")
    python = {
        name: version for _, name, version in python_packages((config / "script-python-requirements.txt").read_text())
    }
    for item in catalog["scriptTools"].values():
        if item["manager"] == "pypi":
            name = re.sub(r"[-_.]+", "-", item["package"]).lower()
            if python.get(name) != item["version"]:
                errors.append("Python checker pins must match the common catalog")
    return errors


def main() -> int:
    errors = check()
    for error in errors:
        print("FAIL " + error)
    if not errors:
        print("OK script coverage, strict configuration and explained local suppressions")
    return int(bool(errors))


if __name__ == "__main__":
    raise SystemExit(main())
