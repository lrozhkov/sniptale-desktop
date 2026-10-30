#!/usr/bin/env python3
from __future__ import annotations

import json
import os
import re
import subprocess
import sys
from datetime import date
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[3]
REGISTRY = ROOT / "tooling/configs/qa/exception-registry.json"
WRAPPERS = ROOT / "tooling/configs/qa/wrappers.json"
GIT_ATTRIBUTES = ROOT / ".gitattributes"
GITLEAKS_CONFIG = ROOT / "tooling/configs/qa/gitleaks.toml"
GITLEAKS_BASELINE = ROOT / "tooling/configs/qa/gitleaks-baseline.json"
TOOL_VERSIONS = ROOT / "tooling/configs/qa/tool-versions.json"
HISTORICAL_REFERENCE_FILES = {"tooling/configs/qa/gitleaks-baseline.json"}
POLICY_DECLARATION_FILES = {"tooling/configs/qa/docs.json"}
REQUIRED_EXCEPTION_FIELDS = {
    "id",
    "ruleId",
    "classification",
    "owner",
    "exactScope",
    "rationale",
    "risk",
    "proofReferences",
}
VALID_CLASSIFICATIONS = {"architectural-boundary", "tool-noise", "temporary-debt"}
APPROVED_EXECUTABLE_PREFIXES = (
    "tooling/qa/installers/",
    "tooling/qa/wrappers/",
    "tooling/qa/runtime/",
    "tooling/qa/rules/",
    "tooling/qa/hooks/",
    "tooling/desktop/e2e_lab/",
    "tooling/release/windows/",
)
APPROVED_EXECUTABLE_FILES = {"tooling/desktop/e2e-lab"}


def load_object(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"{path.relative_to(ROOT)} must contain a JSON object")
    return value


def git(*args: str) -> str:
    completed = subprocess.run(
        ["git", *args], cwd=ROOT, check=False, text=True, capture_output=True
    )
    if completed.returncode not in {0, 1}:
        raise RuntimeError(completed.stderr.strip())
    return completed.stdout


def retired_product_roots() -> tuple[str, ...]:
    return (
        "scr" + "eenyx",
        "smart" + "capture",
        "ns" + "mp",
        "s" + "cx",
    )


def retired_identity_matches(value: str) -> tuple[str, ...]:
    folded = value.casefold()
    return tuple(root for root in retired_product_roots() if root in folded)


def validate_clean_break_identity(errors: list[str]) -> None:
    completed = subprocess.run(
        ["git", "ls-files", "-co", "--exclude-standard", "-z"],
        cwd=ROOT,
        check=False,
        capture_output=True,
    )
    if completed.returncode != 0:
        errors.append("clean-break identity guard could not enumerate repository files")
        return
    for raw_relative in completed.stdout.split(b"\0"):
        if not raw_relative:
            continue
        try:
            relative = raw_relative.decode("utf-8")
        except UnicodeDecodeError:
            errors.append("clean-break identity guard found a non-UTF-8 repository path")
            continue
        path = ROOT / relative
        if not path.is_file():
            continue
        if retired_identity_matches(relative):
            errors.append(f"clean-break identity guard found a retired root in path: {relative}")
        payload = path.read_bytes()
        try:
            text = payload.decode("utf-8")
        except UnicodeDecodeError:
            continue
        for line_number, line in enumerate(text.splitlines(), 1):
            if retired_identity_matches(line):
                errors.append(
                    f"clean-break identity guard found a retired root in UTF-8 content: {relative}:{line_number}"
                )


def validate_exact_scope(entry: dict[str, Any], errors: list[str]) -> None:
    scope = entry.get("exactScope")
    if not isinstance(scope, dict) or not scope:
        errors.append(f"{entry.get('id')} requires non-empty exactScope")
        return
    forbidden = {"baseline", "ceiling", "count", "maximum"} & set(scope)
    if forbidden:
        errors.append(f"{entry.get('id')} uses count-only scope: {sorted(forbidden)}")
    if "exactPaths" in scope:
        paths = scope["exactPaths"]
        if not isinstance(paths, list) or not paths or len(paths) != len(set(paths)):
            errors.append(f"{entry.get('id')} exactPaths must be a unique non-empty list")
        else:
            for relative in paths:
                if not isinstance(relative, str) or not (ROOT / relative).is_file():
                    errors.append(f"{entry.get('id')} stale exact path: {relative}")
    elif "pathTerms" in scope:
        values = scope["pathTerms"]
        if not isinstance(values, dict) or not values:
            errors.append(f"{entry.get('id')} pathTerms must be a non-empty object")
        else:
            for relative, terms in values.items():
                path = ROOT / relative
                if not path.is_file():
                    errors.append(f"{entry.get('id')} stale pathTerms path: {relative}")
                    continue
                text = path.read_text(encoding="utf-8")
                for term in terms:
                    if term not in text:
                        errors.append(f"{entry.get('id')} stale term {term} in {relative}")
    elif "exactEdges" in scope:
        edges = scope["exactEdges"]
        if not isinstance(edges, list) or not edges:
            errors.append(f"{entry.get('id')} exactEdges must be a non-empty list")
        else:
            for edge in edges:
                relative = edge.get("path")
                imported = edge.get("imports")
                path = ROOT / str(relative)
                if not path.is_file() or not re.search(
                    rf"\b(?:crate\s*::\s*)?{re.escape(str(imported))}\s*::", path.read_text(encoding="utf-8")
                ):
                    errors.append(f"{entry.get('id')} stale exact edge: {relative}->{imported}")
    elif "catalog" in scope:
        catalog_path = ROOT / str(scope["catalog"])
        if not catalog_path.is_file():
            errors.append(f"{entry.get('id')} missing catalog: {scope['catalog']}")
            return
        catalog = load_object(catalog_path)
        values = catalog.get(str(scope.get("array")))
        if not isinstance(values, list) or not values:
            errors.append(f"{entry.get('id')} catalog array is empty")
            return
        field = str(scope.get("field"))
        observed: set[str] = set()
        for item in values:
            relative = item.get(field) if isinstance(item, dict) else None
            if not isinstance(relative, str) or relative in observed or not (ROOT / relative).is_file():
                errors.append(f"{entry.get('id')} invalid or stale catalog path: {relative}")
            else:
                observed.add(relative)
    else:
        errors.append(f"{entry.get('id')} uses unsupported exactScope shape")


def validate_registry(errors: list[str]) -> None:
    registry = load_object(REGISTRY)
    if registry.get("schemaVersion") != 1:
        errors.append("exception registry schemaVersion must be 1")
    debt = registry.get("technicalDebt")
    if debt != []:
        errors.append("technicalDebt must stay empty in this tooling wave")
    entries = registry.get("exceptions")
    if not isinstance(entries, list):
        errors.append("exception registry exceptions must be a list")
        return
    ids: set[str] = set()
    rules: set[str] = set()
    for entry in entries:
        if not isinstance(entry, dict):
            errors.append("exception registry entry must be an object")
            continue
        missing = REQUIRED_EXCEPTION_FIELDS - set(entry)
        if missing:
            errors.append(f"exception entry missing fields: {sorted(missing)}")
            continue
        if entry["id"] in ids or entry["ruleId"] in rules:
            errors.append(f"duplicate exception id or ruleId: {entry['id']}")
        ids.add(entry["id"])
        rules.add(entry["ruleId"])
        classification = entry["classification"]
        for field in ("id", "ruleId", "owner", "rationale", "risk"):
            if not isinstance(entry[field], str) or not entry[field].strip():
                errors.append(f"{entry.get('id')} requires non-empty string field {field}")
        if not isinstance(entry["proofReferences"], list) or not entry["proofReferences"]:
            errors.append(f"{entry.get('id')} requires non-empty proofReferences")
            continue
        if classification not in VALID_CLASSIFICATIONS:
            errors.append(f"{entry['id']} has invalid classification {classification}")
        if classification == "temporary-debt":
            if "reviewBy" not in entry or not isinstance(entry.get("removalCondition"), str) or not entry["removalCondition"].strip():
                errors.append(
                    f"temporary debt requires reviewBy and removalCondition: {entry['id']}"
                )
            errors.append(f"temporary debt is forbidden in this wave: {entry['id']}")
        if classification == "tool-noise" and "reviewBy" not in entry:
            errors.append(f"tool-noise entry requires reviewBy: {entry['id']}")
        if "reviewBy" in entry:
            try:
                if date.fromisoformat(entry["reviewBy"]) < date.today():
                    errors.append(f"expired exception entry: {entry['id']}")
            except (TypeError, ValueError):
                errors.append(f"invalid reviewBy date: {entry['id']}")
        for reference in entry["proofReferences"]:
            if not isinstance(reference, str) or not (ROOT / reference.split("#", 1)[0]).is_file():
                errors.append(f"{entry['id']} has missing proof reference: {reference}")
        validate_exact_scope(entry, errors)


def validate_wrapper_topology(errors: list[str]) -> None:
    manifest = load_object(WRAPPERS)
    entries = manifest.get("wrappers")
    if manifest.get("schemaVersion") != 1 or not isinstance(entries, list):
        errors.append("wrapper registry must use schemaVersion 1 and a wrappers list")
        return
    registered = {entry.get("entrypoint") for entry in entries if isinstance(entry, dict)}
    actual = {
        path.relative_to(ROOT).as_posix()
        for path in (ROOT / "tooling/qa/wrappers").glob("*.sh")
    }
    if registered != actual:
        errors.append(
            "wrapper registry drift: missing="
            + repr(sorted(actual - registered))
            + " stale="
            + repr(sorted(registered - actual))
        )
    ids = [entry.get("id") for entry in entries if isinstance(entry, dict)]
    if len(ids) != len(set(ids)):
        errors.append("wrapper registry contains duplicate ids")
    for entrypoint in sorted(registered):
        path = ROOT / str(entrypoint)
        if not path.is_file() or not path.read_bytes().startswith(b"#!/usr/bin/env bash"):
            errors.append(f"invalid canonical wrapper: {entrypoint}")
    for entry in entries:
        if not isinstance(entry, dict):
            continue
        entrypoint = entry.get("entrypoint")
        wrapper_id = entry.get("id")
        blocking = entry.get("blocking")
        if not isinstance(wrapper_id, str) or not isinstance(blocking, bool):
            errors.append(f"wrapper registry entry has invalid id/blocking: {entrypoint}")
            continue
        path = ROOT / str(entrypoint)
        if not path.is_file():
            continue
        text = path.read_text(encoding="utf-8")
        match = re.search(r'qa_init\s+"([^"]+)"\s+(blocking|non-blocking)', text)
        if match is None:
            target_match = re.search(r'exec\s+"\$ROOT/([^"]+)"', text)
            target = ROOT / target_match.group(1) if target_match else None
            if target is not None and target.is_file():
                match = re.search(
                    r'qa_init\s+"([^"]+)"\s+(blocking|non-blocking)',
                    target.read_text(encoding="utf-8"),
                )
        expected_mode = "blocking" if blocking else "non-blocking"
        if match is None or match.group(1) != wrapper_id or match.group(2) != expected_mode:
            errors.append(
                f"wrapper registry lifecycle drift: {entrypoint} expected {wrapper_id}/{expected_mode}"
            )


def validate_executable_topology(errors: list[str]) -> None:
    repository_files = {
        relative
        for relative in git("ls-files", "-co", "--exclude-standard").splitlines()
        if (ROOT / relative).is_file()
    }
    approved_topology = {"configs", "desktop", "qa", "release"}
    for relative in sorted(path for path in repository_files if path.startswith("tooling/")):
        parts = Path(relative).parts
        if relative != "tooling/__init__.py" and len(parts) > 1 and parts[1] not in approved_topology:
            errors.append(f"tooling file has no approved owner: {relative}")
        path = ROOT / relative
        if os.access(path, os.X_OK) and not (
            relative in APPROVED_EXECUTABLE_FILES
            or relative.startswith(APPROVED_EXECUTABLE_PREFIXES)
        ):
            errors.append(f"executable repository tooling is outside an approved owner: {relative}")
    for relative in sorted(repository_files):
        path = ROOT / relative
        if (
            os.access(path, os.X_OK)
            and not relative.startswith("tooling/")
            and not relative.startswith(".git/")
        ):
            errors.append(f"tracked executable must live under tooling/**: {relative}")


def validate_retired_scripts(errors: list[str]) -> None:
    tracked = [path for path in git("ls-files", "scripts").splitlines() if (ROOT / path).exists()]
    if tracked:
        errors.append("tracked scripts/** is retired: " + ", ".join(tracked))
    forbidden = "scripts" + "/"
    references = git("grep", "-n", forbidden)
    for line in references.splitlines():
        relative = line.split(":", 1)[0]
        if (
            relative in HISTORICAL_REFERENCE_FILES
            or relative in POLICY_DECLARATION_FILES
            or relative == "tooling/qa/rules/verify_tooling.py"
        ):
            continue
        errors.append(f"operational retired-path reference: {line}")


def validate_zero_baselines(errors: list[str]) -> None:
    expectations = {
        "tooling/qa/rules/verify_boundaries.py": [
            "FILE_LEVEL_DEAD_CODE_BASELINE = 0",
            "PLATFORM_CFG_DEAD_CODE_BASELINE = 0",
        ],
        "tooling/qa/rules/verify_architecture.py": [
            "AUD11_MEDIA_NOOP_WORKER_BASELINE = set()",
            "AUD11_DIRECT_APP_MEDIA_EXECUTION_BASELINE = {}",
        ],
        "tooling/qa/rules/verify_security.py": ["AUD11_PLACEHOLDER_RECENT_ERRORS_BASELINE = {}"],
    }
    for relative, markers in expectations.items():
        text = (ROOT / relative).read_text(encoding="utf-8")
        for marker in markers:
            if marker not in text:
                errors.append(f"zero suppressive baseline drift: {relative} missing {marker}")


def validate_hosted_qa_policy(errors: list[str]) -> None:
    # JSON is a YAML subset. Parsing the committed workflows as JSON avoids an extra QA dependency.
    workflow_root = ROOT / ".github/workflows"
    expected = {"checks.yml", "alpha.yml", "alpha-publish.yml"}
    found = {path.name for path in workflow_root.glob("*") if path.is_file()}
    if found != expected:
        errors.append("hosted workflow allowlist mismatch")
        return
    try:
        lock = load_object(ROOT / "tooling/configs/qa/toolchain-lock.json")
    except (OSError, ValueError):
        errors.append("hosted toolchain lock missing or invalid")
        return
    pinned = {repo + "@" + item["commit"] for repo, item in lock["actions"].items()}
    pinned.add("actions/cache/save@" + lock["actions"]["actions/cache"]["commit"])
    commands = {
        "python3 tooling/qa/runtime/toolchain.py --install-rust",
        "tooling/qa/installers/install-release-tools.sh",
        "tooling/qa/wrappers/alpha-proof.sh",
        "tooling/qa/wrappers/environment-audit.sh",
        "python3 -m tooling.qa.runtime.compilation_cache",
        'test -n "$ImageVersion" && echo "version=$ImageVersion" >> "$GITHUB_OUTPUT"',
        "& tooling/release/windows/alpha-package.ps1 -QaProof .tmp/release/alpha-proof -Tag $env:ALPHA_TAG",
        'if (-not $env:ImageVersion) { throw "runner image identity missing" }; "version=$env:ImageVersion" >> $env:GITHUB_OUTPUT',
        "python3 tooling/release/windows/alpha_publish.py",
    }
    try:
        for name in sorted(found):
            workflow = load_object(workflow_root / name)
            triggers = set(workflow["on"])
            wanted = {"pull_request", "workflow_dispatch", "workflow_call"} if name == "checks.yml" else {"workflow_dispatch"}
            if triggers != wanted or workflow.get("permissions") != ({"contents": "read", "actions": "read"} if name == "alpha.yml" else {"contents": "read"}):
                errors.append("hosted trigger or default permission drift: " + name)
            for job_name, job in workflow["jobs"].items():
                publisher = name == "alpha-publish.yml" and job_name == "publish"
                if "uses" in job and job["uses"] != "./.github/workflows/checks.yml":
                    errors.append("unapproved reusable workflow")
                if "permissions" in job and (not publisher or job["permissions"] != {"contents": "write", "actions": "read"}):
                    errors.append("hosted job permission drift")
                if publisher and job.get("permissions") != {"contents": "write", "actions": "read"}:
                    errors.append("publication permissions missing")
                if (publisher or job_name == "package") and job.get("if") != "github.ref == 'refs/heads/main'":
                    errors.append("release jobs require reviewed main")
                for step in job.get("steps", []):
                    if "uses" in step and step["uses"] not in pinned:
                        errors.append("hosted Action must use approved immutable SHA")
                    if step.get("uses", "").startswith("actions/checkout@") and step.get("with") != {"fetch-depth": 0, "persist-credentials": False}:
                        errors.append("checkout must use full current source without persisted credentials")
                    if "run" in step and (step["run"] not in commands or "${{" in step["run"]):
                        errors.append("unapproved hosted command")
                    if publisher and "run" in step and step["run"] != "python3 tooling/release/windows/alpha_publish.py":
                        errors.append("publication must not rebuild")
                    action = step.get("uses", "")
                    options = step.get("with", {})
                    if name == "checks.yml" and step.get("name") == "Cache compiled Linux dependencies":
                        expected_key = "linux-ubuntu24-cargo-v1-${{ runner.arch }}-${{ steps.build-image.outputs.version }}-${{ hashFiles('Cargo.lock', 'Cargo.toml', 'build.rs', 'rust-toolchain.toml', '.cargo/**', 'resources/**', 'tooling/configs/qa/tool-versions.json', 'tooling/configs/qa/toolchain-lock.json', 'tooling/qa/runtime/compilation_cache.py', 'tooling/qa/runtime/lib.sh', 'tooling/qa/wrappers/coverage.sh', '.github/workflows/checks.yml') }}"
                        if action != "actions/cache@" + lock["actions"]["actions/cache"]["commit"] or options != {"path": "target", "key": expected_key}:
                            errors.append("Linux compilation cache scope or identity drift")
                    if name == "alpha.yml" and action.startswith("actions/cache"):
                        expected_key = "windows-msvc-static-v1-${{ steps.build-image.outputs.version }}-${{ hashFiles('Cargo.lock', 'Cargo.toml', 'build.rs', 'rust-toolchain.toml', '.cargo/**', 'resources/**', 'tooling/configs/qa/tool-versions.json', 'tooling/configs/qa/toolchain-lock.json', 'tooling/release/windows/toolchain.ps1', 'tooling/release/windows/alpha-package.ps1') }}"
                        if name != "alpha.yml" or job_name != "package" or options != {"path": "target", "key": expected_key}:
                            errors.append("compiled dependency cache scope or identity drift")
                    if action.startswith("actions/setup-python@") and options.get("python-version") != lock["python"]["version"]:
                        errors.append("hosted Python version differs from canonical toolchain")
                    if action.startswith("actions/upload-artifact@"):
                        kind = "alpha" if name == "alpha.yml" else "qa"
                        path = ".tmp/release/alpha" if kind == "alpha" else ".tmp/release/alpha-proof"
                        if (publisher or options.get("path") != path or options.get("if-no-files-found") != "error"
                                or options.get("name") != kind + "-${{ github.sha }}-${{ github.run_id }}-${{ github.run_attempt }}"):
                            errors.append("artifact output path or identity drift")
                    if action.startswith("actions/download-artifact@") and (name != "alpha.yml" or options != {
                        "name": "qa-${{ github.sha }}-${{ github.run_id }}-${{ github.run_attempt }}",
                        "path": ".tmp/release/alpha-proof",
                    }):
                        errors.append("QA handoff path or identity drift")
            if name == "alpha.yml" and workflow["jobs"]["package"].get("needs") != "qa":
                errors.append("alpha package must require canonical QA")
            if name == "checks.yml":
                fast = workflow["jobs"].get("fast", {})
                if workflow["jobs"]["qa"].get("needs") != "fast":
                    errors.append("compilation must wait for fast environment and script checks")
                audits = [step for step in fast.get("steps", []) if step.get("run") == "tooling/qa/wrappers/environment-audit.sh"]
                if len(audits) != 1 or any(step.get("continue-on-error") or "if" in step for step in audits):
                    errors.append("fast tool environment audit must be unconditional and blocking")
                steps = workflow["jobs"]["qa"]["steps"]
                cache = [i for i, step in enumerate(steps) if step.get("name") == "Cache compiled Linux dependencies"]
                images = [i for i, step in enumerate(steps) if step.get("id") == "build-image"
                          and step.get("run") == 'test -n "$ImageVersion" && echo "version=$ImageVersion" >> "$GITHUB_OUTPUT"']
                invalidate = [i for i, step in enumerate(steps) if step.get("run") == "python3 -m tooling.qa.runtime.compilation_cache"]
                proof = [i for i, step in enumerate(steps) if step.get("run") == "tooling/qa/wrappers/alpha-proof.sh"]
                if len(images) != 1 or len(cache) != 1 or images[0] >= cache[0]:
                    errors.append("Linux compilation cache requires an exact runner image identity")
                if len(cache) != 1 or len(invalidate) != 2 or len(proof) != 1 or not cache[0] < invalidate[0] < proof[0] < invalidate[1]:
                    errors.append("Linux compilation cache requires application invalidation before full QA")
                if any("if" in steps[i] or steps[i].get("continue-on-error", False) for i in invalidate + proof):
                    errors.append("Linux cache invalidation and full QA must be unconditional and blocking")
    except (ValueError, KeyError, TypeError) as exc:
        errors.append("malformed hosted workflow policy: " + type(exc).__name__)
    package_script = ROOT / "tooling/release/windows/alpha-package.ps1"
    if package_script.is_file():
        source = package_script.read_text(encoding="utf-8")
        clean = "& cargo clean --locked --package sniptale-native-agent --release --target x86_64-pc-windows-msvc --target-dir target"
        build = "& cargo build --locked --release --target x86_64-pc-windows-msvc --bin sniptale-native-agent --all-features --timings --target-dir target"
        if clean not in source or build not in source or source.index(clean) > source.index(build):
            errors.append("alpha compilation must invalidate current package in the exact release target")
    else:
        errors.append("alpha packaging source missing")



def validate_git_attributes(errors: list[str]) -> None:
    if not GIT_ATTRIBUTES.is_file():
        errors.append(".gitattributes is required for deterministic text bytes")
        return
    lines = {line.strip() for line in GIT_ATTRIBUTES.read_text(encoding="utf-8").splitlines() if line.strip()}
    required = {
        "* text=auto eol=lf",
        "*.md text eol=lf",
        "*.json text eol=lf",
        "*.txt text eol=lf",
        "LICENSE text eol=lf",
        "NOTICE text eol=lf",
        "LICENSES/third-party/*.txt -text",
        "THIRD_PARTY_NOTICES.md -text",
        "*.ico binary",
        "*.png binary",
    }
    missing = sorted(required - lines)
    if missing:
        errors.append(".gitattributes is missing deterministic text/binary rules: " + ", ".join(missing))
    probes = ["LICENSE", "NOTICE", "THIRD_PARTY_DEPENDENCIES.json", "README.md"]
    attributes = git("check-attr", "eol", "--", *probes)
    for relative in probes:
        if f"{relative}: eol: lf" not in attributes:
            errors.append(f"Git LF policy is not active for {relative}")
    exact_byte_probes = ["THIRD_PARTY_NOTICES.md"]
    license_probes = sorted((ROOT / "LICENSES/third-party").glob("*.txt"))
    if license_probes:
        exact_byte_probes.append(license_probes[0].relative_to(ROOT).as_posix())
    exact_attributes = git("check-attr", "text", "--", *exact_byte_probes)
    for relative in exact_byte_probes:
        if f"{relative}: text: unset" not in exact_attributes:
            errors.append(f"Git exact-byte policy is not active for {relative}")


def validate_secret_audit(errors: list[str]) -> None:
    if not GITLEAKS_CONFIG.is_file() or not GITLEAKS_BASELINE.is_file():
        errors.append("Gitleaks config and baseline are required")
        return
    config = GITLEAKS_CONFIG.read_text(encoding="utf-8")
    if "useDefault = true" not in config or "^\\.tmp/" not in config or "^target/" not in config:
        errors.append("Gitleaks config must extend defaults and exclude generated trees")
    baseline = json.loads(GITLEAKS_BASELINE.read_text(encoding="utf-8"))
    if not isinstance(baseline, list):
        errors.append("Gitleaks baseline must be a JSON array")
    else:
        fingerprints: set[str] = set()
        for entry in baseline:
            if not isinstance(entry, dict):
                errors.append("Gitleaks baseline entries must be objects")
                continue
            required = {"RuleID", "StartLine", "File", "Commit", "Fingerprint", "Secret", "SniptaleScope", "SniptaleEnabled", "SniptaleRationale"}
            missing = required - set(entry)
            if missing:
                errors.append("Gitleaks baseline entry is missing fields: " + ", ".join(sorted(missing)))
                continue
            extra = set(entry) - required
            if extra:
                errors.append("Gitleaks baseline entry contains forbidden fields: " + ", ".join(sorted(extra)))
            if entry["Secret"] != "REDACTED":
                errors.append("Gitleaks baseline must not contain raw secret values")
            if entry["SniptaleScope"] not in {"worktree", "history", "both"} or not str(entry["SniptaleRationale"]).strip():
                errors.append("Gitleaks baseline requires an exact scope and rationale")
            if not isinstance(entry["SniptaleEnabled"], bool):
                errors.append("Gitleaks baseline requires an explicit boolean enabled state")
            if entry["Fingerprint"] in fingerprints:
                errors.append(f"duplicate Gitleaks baseline fingerprint: {entry['Fingerprint']}")
            fingerprints.add(entry["Fingerprint"])
            if entry["SniptaleScope"] == "history" and not re.fullmatch(r"[0-9a-f]{40}", str(entry["Commit"])):
                errors.append("Gitleaks history baseline requires an exact commit")
            if entry["SniptaleScope"] in {"worktree", "both"} and entry["Commit"] != "":
                errors.append("Gitleaks worktree/both baseline must not pin a commit")
    tools = load_object(TOOL_VERSIONS)
    gitleaks = tools.get("externalTools", {}).get("gitleaks", {})
    version = gitleaks.get("version")
    linux = gitleaks.get("linuxX64", {})
    if not isinstance(version, str) or not re.fullmatch(r"\d+\.\d+\.\d+", version):
        errors.append("Gitleaks version must be exact")
    if not isinstance(linux.get("url"), str) or f"/v{version}/gitleaks_{version}_linux_x64.tar.gz" not in linux.get("url", ""):
        errors.append("Gitleaks Linux URL must match the exact version")
    if not isinstance(linux.get("sha256"), str) or not re.fullmatch(r"[0-9a-f]{64}", linux.get("sha256", "")):
        errors.append("Gitleaks Linux archive requires an exact SHA-256")
    wrapper = ROOT / "tooling/qa/wrappers/audit.sh"
    release = ROOT / "tooling/qa/wrappers/release.sh"
    if not wrapper.is_file() or not release.is_file():
        errors.append("Gitleaks audit and release wrappers are required")
        return
    wrapper_text = wrapper.read_text(encoding="utf-8")
    for marker in ("elif [[ -x \"$managed_bin\" ]]", "scan_scope dir", "scan_scope git", "--redact=100", "--exit-code 0", "verify_gitleaks.py", "--is-shallow-repository"):
        if marker not in wrapper_text:
            errors.append(f"Gitleaks audit wrapper is missing {marker}")
    if 'qa_step "secret audit" tooling/qa/wrappers/audit.sh' not in release.read_text(encoding="utf-8"):
        errors.append("release wrapper must run the secret audit")


def validate_retired_smoke_queue(errors: list[str]) -> None:
    retired_script = "windows-smoke-" + "watch.ps1"
    bridge_name = "windows-smoke-" + "bridge"
    if (ROOT / "tooling/release/windows/smoke" / retired_script).exists():
        errors.append("retired Windows smoke queue script still exists")
    for fragment in (retired_script, bridge_name):
        references = git("grep", "-n", fragment)
        for line in references.splitlines():
            if line.startswith("tooling/qa/rules/verify_tooling.py:"):
                continue
            errors.append(f"retired Windows smoke queue reference: {line}")


def main() -> int:
    errors: list[str] = []
    validate_registry(errors)
    validate_wrapper_topology(errors)
    validate_executable_topology(errors)
    validate_retired_scripts(errors)
    validate_zero_baselines(errors)
    validate_hosted_qa_policy(errors)
    sys.path.insert(0, str(ROOT / "tooling/qa/runtime"))
    import toolchain
    try:
        toolchain.check(ROOT)
    except (toolchain.ToolchainError, OSError, ValueError, KeyError, TypeError) as exc:
        errors.append(f"canonical toolchain policy: {exc}")
    validate_git_attributes(errors)
    validate_secret_audit(errors)
    validate_retired_smoke_queue(errors)
    validate_clean_break_identity(errors)
    if errors:
        for error in errors:
            print(f"FAIL {error}", file=sys.stderr)
        return 1
    print("OK tooling topology and exception registry")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
