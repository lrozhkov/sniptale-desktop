"""Prove real checker rejection with isolated, deliberately invalid fixtures."""

from __future__ import annotations

import json
import subprocess
import tempfile
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

from .repository import ROOT
from .powershell_profile import prepare
from .script_lints import CACHE
from .storage import sanitize_text


def rejection(label: str, command: list[str], marker: str | tuple[str, ...] | None) -> None:
    result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, check=False, timeout=120)
    output = result.stdout + result.stderr
    markers = (marker,) if isinstance(marker, str) else marker or ()
    if (
        result.returncode == 0
        or (marker is None and result.returncode != 1)
        or any(item not in output for item in markers)
    ):
        report = ROOT / ".tmp/qa-reports" / ("negative-" + label.replace(" ", "-").replace("/", "-") + ".log")
        report.write_text(sanitize_text(output, ROOT))
        raise ValueError(f"negative proof did not detect {label}")
    print("OK negative proof: " + label, flush=True)


def main() -> int:
    prepare()
    versions = json.loads((ROOT / "tooling/configs/qa/tool-versions.json").read_text())["scriptTools"]
    config = ROOT / "tooling/configs/qa"
    python = CACHE / "python/bin"
    npm = CACHE / "npm/node_modules/.bin"

    def binary(name: str) -> str:
        return str(CACHE / (name + "-" + versions[name]["version"]) / name)

    reports = ROOT / ".tmp/qa-reports"
    reports.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=reports, prefix="lint-negative-") as directory:
        work = Path(directory)

        def fixture(name: str, source: str) -> str:
            path = work / name
            path.write_text(source)
            return str(path)

        bad_python = fixture("errors.py", "answer: int = 'wrong type'\nprint(undefined_name)\n")
        bad_format = fixture("format.py", "answer=  1\n")
        bad_shell = fixture("quoting.sh", "#!/bin/sh\nvalue=\"$1\"\nprintf '%s\\n' $value\n")
        shell_format = fixture("format.sh", "#!/bin/sh\nif true;then echo value;fi\n")
        ps_bad = fixture("bad.ps1", "function Broken {\n")
        ps_rules = fixture(
            "rules.ps1",
            "try { Get-Item -LiteralPath missing } catch { }\n$value = $null ?? 'fallback'\nGet-Error\n[System.IO.File]::ReadAllBytesAsync('fixture')\n",
        )
        ps_inventory = fixture("ps-inventory.json", json.dumps([str(Path(ps_bad).relative_to(ROOT))]))
        rules_inventory = fixture("rules-inventory.json", json.dumps([str(Path(ps_rules).relative_to(ROOT))]))
        ps_command = [
            binary("pwsh"),
            "-NoProfile",
            "-NonInteractive",
            "-File",
            "tooling/qa/runtime/lint-powershell.ps1",
            "-Repository",
            str(ROOT),
            "-ModulePath",
            str(CACHE / ("PSScriptAnalyzer-" + versions["PSScriptAnalyzer"]["version"]) / "PSScriptAnalyzer.psd1"),
        ]
        javascript = fixture("floating.cjs", "async function work() { throw new Error('fixture'); }\nwork();\n")
        project = fixture(
            "tsconfig.json",
            json.dumps(
                {
                    "compilerOptions": {"allowJs": True, "checkJs": True, "noEmit": True, "target": "ES2022"},
                    "files": [javascript],
                }
            ),
        )
        # Keep every production rule. Only the fixture's type-project location changes.
        eslint_config = fixture(
            "eslint.config.cjs",
            "const configs = require("
            + json.dumps(str(config / "eslint.config.cjs"))
            + ");\nmodule.exports = configs.map(config => ({...config, languageOptions: {...config.languageOptions, parserOptions: {project: "
            + json.dumps(project)
            + ", tsconfigRootDir: "
            + json.dumps(str(ROOT))
            + "}}}));\n",
        )
        workflow = fixture(
            "expression.yml",
            json.dumps(
                {
                    "name": "Negative expression",
                    "on": {"push": {}},
                    "permissions": {"contents": "read"},
                    "jobs": {"bad": {"runs-on": "${{ missing.value }}", "steps": [{"run": "echo valid"}]}},
                }
            ),
        )
        dangerous = fixture(
            "danger.yml",
            json.dumps(
                {
                    "name": "Negative injection",
                    "on": {"pull_request_target": {}},
                    "permissions": {"contents": "read"},
                    "jobs": {
                        "bad": {
                            "runs-on": "ubuntu-latest",
                            "steps": [{"run": "echo '${{ github.event.pull_request.title }}'"}],
                        }
                    },
                }
            ),
        )
        yaml_duplicate = fixture("duplicate.yaml", "enabled: true\nenabled: false\n")
        toml_invalid = fixture("invalid.toml", "values = [\n")
        toml_format = fixture("format.toml", "value=1\n")
        toml_valid = fixture("valid.toml", "value = 1\n")
        subprocess.run(
            [binary("taplo"), "format", "--check", "--config", str(config / "taplo.toml"), toml_valid],
            check=True,
            timeout=30,
        )
        checks = [
            (
                "undefined Python name",
                [str(python / "ruff"), "check", "--config", str(config / "ruff.toml"), bad_python],
                "F821",
            ),
            (
                "Python formatting",
                [str(python / "ruff"), "format", "--check", "--config", str(config / "ruff.toml"), bad_format],
                "would be reformatted",
            ),
            (
                "Python type mismatch",
                [str(npm / "pyright"), "--project", str(config / "pyrightconfig.json"), bad_python],
                "reportAssignmentType",
            ),
            ("shell quoting", [binary("shellcheck"), bad_shell], "SC2086"),
            ("shell formatting", [binary("shfmt"), "-d", "-i", "2", "-ci", shell_format], "---"),
            ("PowerShell syntax", [*ps_command, "-Inventory", ps_inventory], "PowerShellSyntax"),
            (
                "PowerShell error handling and 5.1 syntax",
                [*ps_command, "-Inventory", rules_inventory],
                (
                    "PSAvoidUsingEmptyCatchBlock",
                    "PSUseCompatibleSyntax",
                    "PSUseCompatibleCommands",
                    "PSUseCompatibleTypes",
                ),
            ),
            (
                "unhandled JavaScript Promise",
                [str(npm / "eslint"), "--no-ignore", "--config", eslint_config, javascript],
                "@typescript-eslint/no-floating-promises",
            ),
            ("workflow expression", [binary("actionlint"), workflow], "[expression]"),
            (
                "workflow trust boundary",
                [
                    str(python / "zizmor"),
                    "--offline",
                    "--no-progress",
                    "--config",
                    str(config / "zizmor.yaml"),
                    dangerous,
                ],
                "template-injection",
            ),
            (
                "duplicate YAML key",
                [str(python / "yamllint"), "--strict", "--config-file", str(config / "yamllint.yaml"), yaml_duplicate],
                "key-duplicates",
            ),
            (
                "TOML syntax",
                [binary("taplo"), "format", "--check", "--config", str(config / "taplo.toml"), toml_invalid],
                "error",
            ),
            (
                "TOML formatting",
                [binary("taplo"), "format", "--check", "--config", str(config / "taplo.toml"), toml_format],
                None,
            ),
        ]
        with ThreadPoolExecutor(max_workers=4) as pool:
            pending = [pool.submit(rejection, *item) for item in checks]
            failures = []
            for task in pending:
                try:
                    task.result()
                except ValueError as exc:
                    failures.append(str(exc))
            if failures:
                raise ValueError("; ".join(failures))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
