"""Run complete pinned duplication and CodeQL release analysis."""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import os
import re
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from urllib.parse import unquote, urlparse

from .analysis_tools import install, sha256
from .finding_registry import classify, finding, validate
from .repository import ROOT
from .storage import atomic_write_json
from .toolchain import check


LANGUAGES = ("rust", "python", "javascript", "actions")
EXTENSIONS = {".rs", ".py", ".sh", ".ps1", ".js", ".cjs", ".mjs", ".yml", ".yaml", ".ts", ".tsx"}


def read_report(path: Path) -> dict:
    if path.is_symlink() or not path.is_file() or path.stat().st_size > 32 * 1024**2:
        raise ValueError("missing, redirected, or oversized analyzer report")
    value = json.loads(path.read_text())
    if not isinstance(value, dict):
        raise ValueError("analyzer report must be an object")
    return value


def location(root: Path, name: str, start: int, end: int) -> dict:
    parsed = urlparse(unquote(name))
    if parsed.scheme not in {"", "file"} or parsed.netloc:
        raise ValueError("analyzer finding has an external location")
    path = Path(parsed.path)
    path = path if path.is_absolute() else root / path
    resolved = path.resolve()
    if not resolved.is_relative_to(root.resolve()) or not resolved.is_file():
        raise ValueError("analyzer finding is outside the source tree")
    lines = resolved.read_text().splitlines()
    if not isinstance(start, int) or not isinstance(end, int) or start < 1 or end < start or end > len(lines):
        raise ValueError("analyzer finding has invalid source bounds")
    return {
        "path": resolved.relative_to(root).as_posix(),
        "startLine": start,
        "endLine": end,
        "snippet": "\n".join(lines[start - 1 : end]),
    }


def duplication_findings(report: dict, root: Path = ROOT) -> list[dict]:
    if report.get("statistics", {}).get("total", {}).get("sources", 0) <= 0:
        raise ValueError("duplication scan analyzed no files")
    if not isinstance(report.get("duplicates"), list):
        raise ValueError("duplication report has no result inventory")
    findings = []
    for clone in report["duplicates"]:
        places = [
            location(
                root,
                clone[key]["name"],
                clone[key]["startLoc"]["line"],
                clone[key]["endLoc"]["line"],
            )
            for key in ("firstFile", "secondFile")
        ]
        findings.append(finding("jscpd", "duplicate-code", places, "Repeated source tokens", False))
    return findings


def codeql_findings(report: dict, root: Path = ROOT) -> list[dict]:
    if report.get("version") != "2.1.0" or not report.get("runs"):
        raise ValueError("invalid or empty CodeQL SARIF report")
    findings = []
    for run in report["runs"]:
        driver = run.get("tool", {}).get("driver", {})
        if driver.get("name") != "CodeQL" or not driver.get("rules"):
            raise ValueError("CodeQL report lacks executed rules")
        invocations = run.get("invocations", [])
        if not invocations or any(item.get("executionSuccessful") is not True for item in invocations):
            raise ValueError("CodeQL invocation did not complete successfully")
        if any(
            note.get("level") == "error" for item in invocations for note in item.get("toolExecutionNotifications", [])
        ):
            raise ValueError("CodeQL reported an execution error")
        rules = {rule["id"]: rule for rule in driver["rules"]}
        if not isinstance(run.get("results"), list):
            raise ValueError("CodeQL report lacks its result inventory")
        for result in run["results"]:
            rule = result["ruleId"]
            if rule not in rules or not result.get("locations"):
                raise ValueError("CodeQL result has an unknown rule or no source location")
            places = []
            for item in result["locations"]:
                physical = item["physicalLocation"]
                region = physical["region"]
                places.append(
                    location(
                        root,
                        physical["artifactLocation"]["uri"],
                        region["startLine"],
                        region.get("endLine", region["startLine"]),
                    )
                )
            level = result.get(
                "level",
                rules[rule].get("defaultConfiguration", {}).get("level", "warning"),
            )
            if level not in {"note", "warning", "error"}:
                raise ValueError("CodeQL result has an unknown severity")
            security_severity = float(rules[rule].get("properties", {}).get("security-severity", "0"))
            findings.append(
                finding(
                    "codeql",
                    rule,
                    places,
                    result["message"]["text"],
                    level != "note" or security_severity > 0,
                )
            )
    return findings


def command(args: list[str], output: Path, timeout: int, metrics: list[dict]) -> None:
    started = time.monotonic()
    with output.open("w") as log:
        subprocess.run(
            args,
            cwd=ROOT,
            env={**os.environ, "SNIPTALE_WINDOWS_RESOURCES": "check-skip", "PYTHONDONTWRITEBYTECODE": "1"},
            stdout=log,
            stderr=subprocess.STDOUT,
            timeout=timeout,
            check=True,
        )
    metrics.append({"command": output.stem, "wallSeconds": round(time.monotonic() - started, 3)})
    print(f"OK {output.stem}: {metrics[-1]['wallSeconds']}s", flush=True)


def source_inventory(files: list[str], root: Path = ROOT) -> list[str]:
    sources = sorted(
        name
        for name in files
        if Path(name).suffix in EXTENSIONS
        and (name.startswith(("src/", "tooling/", "ui/")) or name == "build.rs")
        and not name.startswith(("ui/node_modules/", "ui/dist/"))
        and (root / name).is_file()
    )
    if not sources or any((root / name).is_symlink() or (root / name).stat().st_size > 8 * 1024**2 for name in sources):
        raise ValueError("invalid or empty duplication source inventory")
    return sources


def source_policy_errors(root: Path, sources: list[str]) -> list[str]:
    errors = []
    suppression = re.compile(r"(?im)^\s*(?://|#|/\*)\s*(?:(?:codeql|lgtm)\s*\[|(?:jscpd|cpd)\s*:\s*ignore)")
    for name in sources:
        if suppression.search((root / name).read_text()):
            errors.append(f"{name}: use the exact finding registry instead of an inline analyzer suppression")
    return errors


RAW_REPORTS = {
    "jscpd-report.json",
    "jscpd-report.sarif",
    *[f"codeql-{language}.sarif" for language in LANGUAGES],
}


def scan_inputs(root: Path = ROOT) -> dict:
    paths = (
        subprocess.check_output(["git", "ls-files", "-co", "--exclude-standard", "-z"], cwd=root).decode().split("\0")
    )
    values = {}
    for name in sorted(set(paths) - {"", "tooling/configs/qa/exception-registry.json"}):
        if Path(name).suffix == ".md":
            continue
        path = root / name
        values[name] = os.readlink(path) if path.is_symlink() else sha256(path) if path.is_file() else "missing"
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()
    overrides = {
        name: hashlib.sha256(os.environ[name].encode()).hexdigest() if name in os.environ else None
        for name in set(
            (
                "RUSTFLAGS",
                "CARGO_ENCODED_RUSTFLAGS",
                "RUSTUP_TOOLCHAIN",
                "CARGO_BUILD_TARGET",
                "CARGO_INCREMENTAL",
                "SNIPTALE_INSTALL_CHANNEL",
                "SNIPTALE_ALLOWED_EXTENSION_ORIGIN",
                "SNIPTALE_RELEASE_FLAVOR",
                "SNIPTALE_DEV_LOCAL_BROWSER_TRUST",
            )
        )
        | {name for name in os.environ if name.startswith(("CODEQL_", "CARGO_", "RUST", "CC_", "CXX_"))}
        | {"CC", "CXX", "AR", "CFLAGS", "CXXFLAGS", "CPPFLAGS", "LDFLAGS"}
    }
    runtime = {
        name: subprocess.check_output(command, cwd=root, text=True).strip()
        for name, command in {
            "rustc": ["rustc", "--version"],
            "cargo": ["cargo", "--version"],
        }.items()
    }
    return {
        "revision": revision,
        "files": values,
        "buildOverrides": overrides,
        "runtime": {**runtime, "python": sys.version},
    }


def verify_raw(directory: Path, inputs: dict) -> dict:
    manifest = read_report(directory / "scan.json")
    if (
        manifest.get("schemaVersion") != 1
        or manifest.get("kind") != "sniptale-analysis-scan"
        or manifest.get("scanInputs") != inputs
    ):
        raise ValueError("raw analysis source, dependency, or tool inputs changed")
    if not isinstance(manifest.get("reports"), dict) or set(manifest["reports"]) != RAW_REPORTS:
        raise ValueError("raw analysis report inventory is incomplete")
    for name, digest in manifest["reports"].items():
        path = directory / name
        if path.is_symlink() or not path.is_file() or path.stat().st_size > 32 * 1024**2 or sha256(path) != digest:
            raise ValueError("raw analysis report is missing or changed")
    return manifest


def report_findings(directory: Path) -> list[dict]:
    findings = duplication_findings(read_report(directory / "jscpd-report.json"))
    for language in LANGUAGES:
        findings.extend(codeql_findings(read_report(directory / f"codeql-{language}.sarif")))
    return findings


def run_scan(output: Path, sources: list[str], metrics: list[dict]) -> None:
    scanner = install("jscpd")
    command(
        [
            str(scanner),
            *[str(ROOT / name) for name in sources],
            "--absolute",
            "--no-gitignore",
            "--mode",
            "weak",
            "--min-lines",
            "15",
            "--min-tokens",
            "100",
            "--max-size",
            "8mb",
            "--workers",
            "2",
            "--reporters",
            "json,sarif",
            "--output",
            str(output),
            "--fail-on-empty",
        ],
        output / "jscpd.log",
        120,
        metrics,
    )
    codeql = install("codeql")
    with tempfile.TemporaryDirectory(prefix="codeql-", dir=output) as scratch:
        database = Path(scratch) / "databases"
        command(
            [
                str(codeql),
                "database",
                "create",
                str(database),
                "--db-cluster",
                "--language=" + ",".join(LANGUAGES),
                "--build-mode=none",
                "--source-root=" + str(ROOT),
                "--codescanning-config=tooling/configs/qa/codeql.json",
                "--extractor-option=rust.cargo_target=x86_64-pc-windows-msvc",
                "--threads=2",
                "--ram=6144",
            ],
            output / "codeql-create.log",
            1200,
            metrics,
        )
        extracted = set(read_report(database / "rust/baseline-info.json")["languages"]["rust"]["files"])
        expected_rust = {name for name in sources if Path(name).suffix == ".rs"}
        if expected_rust - extracted:
            raise ValueError("CodeQL omitted Rust source files: " + ", ".join(sorted(expected_rust - extracted)))
        for language in LANGUAGES:
            report = output / f"codeql-{language}.sarif"
            command(
                [
                    str(codeql),
                    "database",
                    "analyze",
                    str(database / language),
                    f"codeql/{language}-queries:codeql-suites/{language}-security-and-quality.qls",
                    "--format=sarif-latest",
                    "--output=" + str(report),
                    "--threads=2",
                    "--ram=6144",
                    "--no-print-metrics-summary",
                ],
                output / f"codeql-{language}.log",
                1800,
                metrics,
            )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, allow_abbrev=False)
    parser.add_argument(
        "--review-run",
        help="Reclassify complete reports for unchanged analyzer inputs.",
    )
    args = parser.parse_args()
    check(ROOT)
    inputs = scan_inputs()
    run_id = os.environ["SNIPTALE_QA_RUN_ID"]
    base = ROOT / ".tmp/qa/static-analysis"
    output = base / run_id
    output.mkdir(parents=True, exist_ok=False)
    sources = source_inventory(list(inputs["files"]), ROOT)
    errors = source_policy_errors(ROOT, sources)
    if errors:
        raise ValueError("; ".join(errors))
    metrics: list[dict] = []
    if args.review_run:
        if re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.-]{1,127}", args.review_run) is None:
            raise ValueError("invalid analysis run ID")
        previous = base / args.review_run
        if not previous.resolve().is_relative_to(base.resolve()):
            raise ValueError("analysis run directory is redirected")
        manifest = verify_raw(previous, inputs)
        for name in RAW_REPORTS:
            shutil.copyfile(previous / name, output / name)
        metrics = manifest["metrics"]
        print(f"Reclassify verified raw reports from {args.review_run}; analyzer inputs are unchanged.")
    else:
        run_scan(output, sources, metrics)
    if scan_inputs() != inputs:
        raise ValueError("analyzer inputs changed during analysis")
    manifest = {
        "schemaVersion": 1,
        "kind": "sniptale-analysis-scan",
        "scanInputs": inputs,
        "reports": {name: sha256(output / name) for name in sorted(RAW_REPORTS)},
        "metrics": metrics,
    }
    atomic_write_json(output / "scan.json", manifest)
    findings = report_findings(output)
    entries = read_report(ROOT / "tooling/configs/qa/exception-registry.json")["technicalDebt"]
    errors = validate(entries, ROOT)
    outstanding, stale = (findings, errors) if errors else classify(findings, entries)
    atomic_write_json(
        output / "findings.json",
        {
            "findings": findings,
            "metrics": metrics,
            "errors": stale,
            "rawScanRun": args.review_run or run_id,
        },
    )
    for item in outstanding:
        print(
            f"{'FAIL' if item['blocking'] else 'WARN'} {item['tool']} {item['rule']}: {item['locations'][0]['path']} [{item['fingerprint']}]"
        )
    for error in stale:
        print(f"FAIL finding registry: {error}")
    print(f"Analysis report: {output.relative_to(ROOT)}/findings.json")
    return int(bool(stale) or any(item["blocking"] for item in outstanding))


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (KeyError, OSError, ValueError, subprocess.SubprocessError) as error:
        print(f"FAIL static analysis: {type(error).__name__}: {error}", file=sys.stderr)
        raise SystemExit(1) from error
