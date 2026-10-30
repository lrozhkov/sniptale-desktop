#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from collections import Counter, defaultdict
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[3]
SRC = ROOT / "src"
DEFAULT_OUTPUT = ROOT / ".tmp/audit/deep-research-report-11-architecture-snapshot.json"
OWNERS = {
    "app",
    "protocol",
    "ipc",
    "settings",
    "tray",
    "hotkeys",
    "capture",
    "recording",
    "audio",
    "telemetry",
    "transfer",
    "install",
    "diagnostics",
}
MEDIA_WORKER_EXEMPTIONS = {"TransferWorker"}
HOT_FILE_THRESHOLD = 250
APP_MEDIA_WORKER_EXECUTION_FILES = {
    "src/app/resident/capture_worker.rs",
    "src/app/resident/recording_worker.rs",
    "src/app/resident/telemetry_worker.rs",
}
DEFERRED_EVALUATION_ALLOWED_FILES = {
    "src/protocol/validation/warnings.rs",
    "src/settings/effective.rs",
}
APPROVED_DPAPI_OWNER_FILES = {
    "src/install/windows/dpapi.rs",
    "src/transfer/windows_dpapi.rs",
}


def rel(path: Path) -> str:
    return path.relative_to(ROOT).as_posix()


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def git_output(*args: str) -> str | None:
    try:
        return subprocess.check_output(
            ["git", *args],
            cwd=ROOT,
            stderr=subprocess.DEVNULL,
            text=True,
        ).strip()
    except (subprocess.CalledProcessError, FileNotFoundError):
        return None


def rust_files() -> list[Path]:
    if not SRC.exists():
        return []
    return sorted(path for path in SRC.rglob("*.rs") if path.is_file())


def is_test_file(path: Path) -> bool:
    relative = path.as_posix()
    return "/tests/" in relative or path.name == "tests.rs"


def production_rust_files() -> list[Path]:
    return [path for path in rust_files() if not is_test_file(path)]


def owner_for(path: Path) -> str | None:
    try:
        parts = path.relative_to(SRC).parts
    except ValueError:
        return None
    if not parts:
        return None
    if parts[0] == "main.rs":
        return "main"
    if parts[0] in OWNERS:
        return parts[0]
    return None


def line_count(path: Path) -> int:
    return len(read(path).splitlines())


def owner_file_counts(files: list[Path]) -> dict[str, dict[str, int]]:
    counts: dict[str, dict[str, int]] = {
        owner: {
            "directRustFiles": 0,
            "productionRustFiles": 0,
            "testRustFiles": 0,
            "totalRustFiles": 0,
        }
        for owner in sorted(OWNERS)
    }
    for path in files:
        owner = owner_for(path)
        if owner is None or owner == "main":
            continue
        entry = counts[owner]
        entry["totalRustFiles"] += 1
        if path.parent == SRC / owner and path.name != "mod.rs":
            entry["directRustFiles"] += 1
        if is_test_file(path):
            entry["testRustFiles"] += 1
        else:
            entry["productionRustFiles"] += 1
    return counts


def owner_imports(path: Path) -> list[str]:
    text = read(path)
    imports: list[str] = []
    for owner in sorted(OWNERS):
        patterns = [
            rf"\buse\s+crate::{owner}\b",
            rf"\bcrate::{owner}::",
            rf"\bsuper::super::{owner}::",
            rf"\buse\s+crate::\{{[^}}]*\b{owner}\b",
            rf"\buse\s+crate::\{{[^}}]*\{{[^}}]*\b{owner}\b",
        ]
        if any(re.search(pattern, text) for pattern in patterns):
            imports.append(owner)
    return imports


def direct_owner_import_snapshot(files: list[Path]) -> list[dict[str, Any]]:
    entries: list[dict[str, Any]] = []
    for path in files:
        owner = owner_for(path)
        if owner is None or owner == "main":
            continue
        imports = [imported for imported in owner_imports(path) if imported != owner]
        if imports:
            entries.append(
                {
                    "path": rel(path),
                    "owner": owner,
                    "imports": imports,
                }
            )
    return entries


def agent_components() -> set[str]:
    path = SRC / "app/resident/startup.rs"
    if not path.is_file():
        return set()
    match = re.search(r"enum\s+AgentComponent\s*\{(?P<body>.*?)\n\s*\}", read(path), re.S)
    if not match:
        return set()
    return set(re.findall(r"^\s*([A-Z][A-Za-z0-9_]*)\s*,", match.group("body"), re.M))


def handled_workers() -> set[str]:
    texts = []
    for relative in [
        "app/resident/workers.rs",
        "app/resident/worker_start.rs",
    ]:
        path = SRC / relative
        if path.is_file():
            texts.append(read(path))
    text = "\n".join(texts)
    handled = set(re.findall(r"component\s*==\s*AgentComponent::([A-Z][A-Za-z0-9_]*)", text))
    handled.update(re.findall(r"AgentComponent::([A-Z][A-Za-z0-9_]*)\s*=>", text))
    return handled


def media_noop_workers() -> list[str]:
    candidates = {
        component
        for component in agent_components()
        if component.endswith("Worker") and component not in MEDIA_WORKER_EXEMPTIONS
    }
    return sorted(candidates - handled_workers())


def direct_app_media_execution() -> list[dict[str, Any]]:
    occurrences: list[dict[str, Any]] = []
    app = SRC / "app"
    if not app.exists():
        return occurrences
    for path in sorted(app.rglob("*.rs")):
        if is_test_file(path):
            continue
        relative = rel(path)
        if relative in APP_MEDIA_WORKER_EXECUTION_FILES:
            continue
        for line_number, line in enumerate(read(path).splitlines(), start=1):
            if ".capture_screenshot(" in line:
                occurrences.append({"path": relative, "line": line_number, "method": "capture_screenshot"})
            if (
                ".start_recording(" in line
                and "effects.start_recording(" not in line
                and "self.recording.start_recording(" not in line
            ):
                occurrences.append({"path": relative, "line": line_number, "method": "start_recording"})
    return occurrences


def regex_occurrences(pattern: str, files: list[Path]) -> list[dict[str, Any]]:
    regex = re.compile(pattern)
    occurrences: list[dict[str, Any]] = []
    for path in files:
        for line_number, line in enumerate(read(path).splitlines(), start=1):
            if regex.search(line):
                occurrences.append({"path": rel(path), "line": line_number})
    return occurrences


def known_scaffold_markers(files: list[Path]) -> dict[str, Any]:
    production = production_rust_files()
    dpapi_files = [
        rel(path)
        for path in production
        if re.search(r"\bCrypt(?:Protect|Unprotect)Data\b|\bCRYPT_INTEGER_BLOB\b", read(path))
    ]
    unapproved_dpapi_files = [path for path in dpapi_files if path not in APPROVED_DPAPI_OWNER_FILES]
    deferred = [
        occurrence
        for occurrence in regex_occurrences(r'"deferred-evaluation"', production)
        if "/tests/" not in occurrence["path"] and occurrence["path"] not in DEFERRED_EVALUATION_ALLOWED_FILES
    ]
    return {
        "mediaNoopWorkers": {
            "ratchetStatus": "zero baseline for media workers; TransferWorker is the only documented non-media worker exemption.",
            "targetWave": "Wave 3 removed no-op media workers; Wave 7 lowers the baseline.",
            "components": media_noop_workers(),
        },
        "directAppMediaExecution": {
            "ratchetStatus": "zero baseline outside resident media worker owner files.",
            "targetWave": "Wave 3 replaced direct app media execution with worker enqueue.",
            "occurrences": direct_app_media_execution(),
        },
        "duplicatedDpapiAdapters": {
            "approvedOwnerFiles": sorted(APPROVED_DPAPI_OWNER_FILES),
            "ratchetStatus": "zero baseline for raw DPAPI mechanics outside approved owner-local adapters.",
            "targetWave": "Wave 2 decided DPAPI ownership; Wave 7 lowers the raw-mechanics baseline.",
            "unapprovedRawMechanicsFiles": sorted(unapproved_dpapi_files),
        },
        "placeholderRecentErrors": {
            "ratchetStatus": "zero baseline.",
            "targetWave": "Wave 5 replaced placeholder diagnostics; Wave 7 lowers the baseline.",
            "occurrences": regex_occurrences(r"recentErrors\\?\"\s*:\s*\\?\[\s*\\?\]", production),
        },
        "deferredEvaluationEffectiveSettings": {
            "allowedPolicyFiles": sorted(DEFERRED_EVALUATION_ALLOWED_FILES),
            "ratchetStatus": "zero baseline for deferred-evaluation placeholders outside settings policy and protocol warning registry.",
            "targetWave": "Wave 1 implemented effective settings policy; Wave 7 lowers the placeholder baseline.",
            "occurrences": deferred,
        },
        "inputFileCount": len(files),
    }


def test_file_sizes(files: list[Path]) -> list[dict[str, Any]]:
    return [{"path": rel(path), "lines": line_count(path)} for path in files if is_test_file(path)]


def hot_files(files: list[Path]) -> list[dict[str, Any]]:
    result: list[dict[str, Any]] = []
    for path in files:
        lines = line_count(path)
        if lines >= HOT_FILE_THRESHOLD:
            result.append({"path": rel(path), "lines": lines})
    return result


def snapshot() -> dict[str, Any]:
    files = rust_files()
    production = production_rust_files()
    return {
        "schemaVersion": 1,
        "audit": "deep-research-report-11",
        "task": "AUD11-W0-T1",
        "sourceRevision": git_output("rev-parse", "--verify", "HEAD"),
        "sourceBranch": git_output("branch", "--show-current"),
        "rebuildCommand": (
            "python3 tooling/qa/rules/audit11_architecture_snapshot.py "
            "--write .tmp/audit/deep-research-report-11-architecture-snapshot.json"
        ),
        "ownerFileCounts": owner_file_counts(files),
        "hotFilesNear300Loc": hot_files(production),
        "testFileSizes": test_file_sizes(files),
        "directOwnerImports": direct_owner_import_snapshot(production),
        "knownScaffoldMarkers": known_scaffold_markers(files),
        "summary": {
            "rustFiles": len(files),
            "productionRustFiles": len(production),
            "testRustFiles": len(files) - len(production),
            "owners": len(OWNERS),
        },
    }


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Generate the AUD11 machine-readable architecture ratchet snapshot.")
    parser.add_argument(
        "--write",
        nargs="?",
        const=str(DEFAULT_OUTPUT),
        help="write JSON to the optional path, defaulting to .tmp/audit/deep-research-report-11-architecture-snapshot.json",
    )
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    payload = snapshot()
    rendered = json.dumps(payload, indent=2, sort_keys=True) + "\n"
    if args.write:
        output = Path(args.write)
        if not output.is_absolute():
            output = ROOT / output
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(rendered, encoding="utf-8")
        print(f"wrote {output.relative_to(ROOT).as_posix()}")
    else:
        sys.stdout.write(rendered)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
