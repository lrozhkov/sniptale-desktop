"""Inventory and run independent script checks without application or VM work."""

from __future__ import annotations

import argparse
import json
import os
import subprocess
from pathlib import Path

from .repository import ROOT
from .powershell_profile import prepare

CACHE = ROOT / ".tmp/qa-tools/script-tools"
LABELS = (
    "Ruff errors and code smells",
    "Ruff formatting",
    "Pyright types",
    "ShellCheck",
    "shfmt formatting",
    "PowerShell parser and analyzer",
    "ESLint promises and safety",
    "actionlint",
    "zizmor",
    "yamllint",
    "Taplo syntax and formatting",
    "JSON syntax duplicates and schemas",
)


def inventory() -> dict[str, list[str]]:
    result: dict[str, list[str]] = {
        key: [] for key in ("python", "shell", "powershell", "javascript", "yaml", "toml", "json")
    }
    deleted = set(
        subprocess.run(["git", "ls-files", "--deleted", "-z"], cwd=ROOT, capture_output=True, check=True, timeout=20)
        .stdout.decode()
        .split("\0")
    )
    files = (
        subprocess.run(
            ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"],
            cwd=ROOT,
            capture_output=True,
            check=True,
            timeout=20,
        )
        .stdout.decode()
        .split("\0")
    )
    suffixes = {
        ".py": "python",
        ".pyi": "python",
        ".sh": "shell",
        ".ps1": "powershell",
        ".psm1": "powershell",
        ".psd1": "powershell",
        ".js": "javascript",
        ".cjs": "javascript",
        ".mjs": "javascript",
        ".yml": "yaml",
        ".yaml": "yaml",
        ".toml": "toml",
        ".json": "json",
    }
    for name in sorted(set(files) - {""} - deleted):
        path = ROOT / name
        if not path.is_file() or path.is_symlink():
            raise ValueError("script inventory contains missing or redirected source")
        category = suffixes.get(path.suffix)
        if path.suffix in {".ts", ".tsx", ".mts", ".cts"}:
            raise ValueError("TypeScript source requires an explicit typed lint lane")
        if category is None:
            with path.open("rb") as stream:
                first = stream.readline(256)
            if first.startswith(b"#!") and not first.startswith(b"#!["):
                if b"bash" in first or b"/sh" in first:
                    category = "shell"
                elif b"python" in first:
                    category = "python"
                elif b"node" in first:
                    category = "javascript"
                elif b"pwsh" in first or b"powershell" in first:
                    category = "powershell"
                else:
                    raise ValueError("script interpreter requires an explicit lint lane")
        if category:
            result[category].append(name)
    return result


def checks(files: dict[str, list[str]]) -> list[tuple[str, list[str]]]:
    prepare()
    versions = json.loads((ROOT / "tooling/configs/qa/tool-versions.json").read_text())["scriptTools"]

    def binary(name: str) -> str:
        return str(CACHE / (name + "-" + versions[name]["version"]) / name)

    python = CACHE / "python/bin"
    node = CACHE / "npm/node_modules/.bin"
    config = "tooling/configs/qa/"
    ps_inventory = ROOT / ".tmp/qa-reports/powershell-inventory.json"
    ps_inventory.parent.mkdir(parents=True, exist_ok=True)
    ps_inventory.write_text(json.dumps(files["powershell"]))
    result = [
        (
            "Ruff errors and code smells",
            [str(python / "ruff"), "check", "--config", config + "ruff.toml", *files["python"]],
        ),
        (
            "Ruff formatting",
            [str(python / "ruff"), "format", "--check", "--config", config + "ruff.toml", *files["python"]],
        ),
        ("Pyright types", [str(node / "pyright"), "--project", config + "pyrightconfig.json", *files["python"]]),
        (
            "ShellCheck",
            [
                binary("shellcheck"),
                "--external-sources",
                "--source-path=SCRIPTDIR",
                "--severity=style",
                *files["shell"],
            ],
        ),
        ("shfmt formatting", [binary("shfmt"), "-d", "-i", "2", "-ci", *files["shell"]]),
        (
            "PowerShell parser and analyzer",
            [
                binary("pwsh"),
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-File",
                "tooling/qa/runtime/lint-powershell.ps1",
                "-Repository",
                str(ROOT),
                "-ModulePath",
                str(CACHE / ("PSScriptAnalyzer-" + versions["PSScriptAnalyzer"]["version"]) / "PSScriptAnalyzer.psd1"),
                "-Inventory",
                str(ps_inventory),
            ],
        ),
        (
            "ESLint promises and safety",
            [
                str(node / "eslint"),
                "--no-warn-ignored",
                "--no-ignore",
                "--config",
                config + "eslint.config.cjs",
                *files["javascript"],
            ],
        ),
        (
            "actionlint",
            [
                binary("actionlint"),
                "-shellcheck=" + binary("shellcheck"),
                *[name for name in files["yaml"] if name.startswith(".github/workflows/")],
            ],
        ),
        (
            "zizmor",
            [
                str(python / "zizmor"),
                "--offline",
                "--no-progress",
                "--config",
                config + "zizmor.yaml",
                ".github/workflows",
            ],
        ),
        ("yamllint", [str(python / "yamllint"), "--strict", "--config-file", config + "yamllint.yaml", *files["yaml"]]),
        (
            "Taplo syntax and formatting",
            [binary("taplo"), "format", "--check", "--config", config + "taplo.toml", *files["toml"]],
        ),
        ("JSON syntax duplicates and schemas", [str(python / "python"), "-m", "tooling.qa.runtime.json_contracts"]),
    ]
    if tuple(label for label, _ in result) != LABELS:
        raise ValueError("lint proof catalog labels drifted")
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--lane")
    parser.add_argument("--inventory", action="store_true")
    parser.add_argument("--powershell-inventory", action="store_true")
    args = parser.parse_args()
    files = inventory()
    if args.powershell_inventory:
        path = ROOT / ".tmp/qa-reports/powershell-inventory.json"
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(files["powershell"]))
        print(f"Inventoried {len(files['powershell'])} PowerShell sources")
        return 0
    if args.inventory:
        print(json.dumps(files, indent=2))
        return 0
    lanes = checks(files)
    if args.lane:
        match = [command for label, command in lanes if label == args.lane]
        if len(match) != 1:
            raise ValueError("unknown script lint lane")
        return subprocess.run(match[0], cwd=ROOT, check=False, timeout=300).returncode
    from .parallel import run_group

    return run_group(lanes, Path(os.environ["SNIPTALE_QA_RUN_RECORD"]), Path(os.environ["SNIPTALE_QA_RUN_LOG"]), 4)


if __name__ == "__main__":
    raise SystemExit(main())
