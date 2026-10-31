#!/usr/bin/env python3
from __future__ import annotations

import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]
CARGO = ROOT / "Cargo.toml"

REQUIRED_DEV_DEPENDENCIES = {
    "assert_cmd": "CLI/mode and doctor/version command proof",
    "predicates": "CLI/output assertions",
    "tempfile": "cache, transfer, diagnostics, and installer filesystem proof",
    "proptest": "boundary parser, ID, filename, and chunk property tests",
    "insta": "stable JSON fixture and diagnostics snapshot review",
}

RUNTIME_DEPENDENCY_JUSTIFICATIONS = {
    "base64": "transfer-owned media chunk encoding/decoding boundary",
    "chacha20poly1305": "transfer-owned temp-media key wrapping and authenticated encryption",
    "crc32fast": "bounded non-cryptographic integrity checks where owner contracts require CRC32",
    "getrandom": "OS RNG for install, IPC, transfer, and identifier entropy",
    "hmac": "IPC pipe authentication MAC",
    "serde": "typed protocol/settings/install/diagnostics parsing",
    "serde_json": "typed JSON parser/serializer at owner boundaries",
    "sha2": "IPC, transfer, release, and artifact hash contracts",
    "tracing": "structured logging with redaction guardrails",
    "tracing-subscriber": "logging runtime setup and JSON formatting",
    "windows": "owner-local Windows FFI adapters; features are explicitly enumerated",
}

HIGH_RISK_DEPENDENCIES = {
    "base64": "media encoding boundary",
    "chacha20poly1305": "crypto",
    "getrandom": "entropy",
    "hmac": "IPC authentication crypto",
    "sha2": "hashing",
    "windows": "Windows FFI",
}

DISALLOWED_RUNTIME_DEPENDENCIES = {
    "tokio": "no general async runtime in v1",
    "async-std": "no general async runtime in v1",
    "smol": "no general async runtime in v1",
}

REQUIRED_TOOL_MANIFEST = ROOT / "tooling/configs/qa/tool-versions.json"


def fail(messages: list[str]) -> None:
    print("FAIL dev dependency guardrails")
    for message in messages:
        print(f"- {message}")
    raise SystemExit(1)


def dev_dependency_block(text: str) -> str:
    match = re.search(r"(?ms)^\[dev-dependencies\]\n(.*?)(?:^\[|\Z)", text)
    return match.group(1) if match else ""


def dependency_block(text: str) -> str:
    match = re.search(r"(?ms)^\[dependencies\]\n(.*?)(?:^\[|\Z)", text)
    return match.group(1) if match else ""


def target_windows_dependency_block(text: str) -> str:
    match = re.search(r"(?ms)^\[target\.'cfg\(windows\)'\.dependencies\]\n(.*?)(?:^\[|\Z)", text)
    return match.group(1) if match else ""


def dependency_names(block: str) -> set[str]:
    return set(re.findall(r"(?m)^([A-Za-z0-9_-]+)\s*=", block))


def main() -> int:
    text = CARGO.read_text(encoding="utf-8")
    block = dev_dependency_block(text)
    errors: list[str] = []
    if not block:
        errors.append("Cargo.toml must define [dev-dependencies] for deterministic owner QA")
    for dep, reason in REQUIRED_DEV_DEPENDENCIES.items():
        if not re.search(rf"(?m)^{re.escape(dep)}\s*=", block):
            errors.append(f"missing dev-dependency {dep}: {reason}")
    dev_deps = dependency_names(block)
    extra_dev_deps = sorted(set(dev_deps) - set(REQUIRED_DEV_DEPENDENCIES))
    for dep in extra_dev_deps:
        errors.append(f"untracked dev-dependency {dep}: add a deterministic QA justification before using it")
    deps = dependency_block(text)
    runtime_deps = dependency_names(deps) | dependency_names(target_windows_dependency_block(text))
    for dep, reason in DISALLOWED_RUNTIME_DEPENDENCIES.items():
        if dep in runtime_deps:
            errors.append(f"runtime dependency {dep} is disallowed: {reason}")
    undocumented_runtime_deps = sorted(runtime_deps - set(RUNTIME_DEPENDENCY_JUSTIFICATIONS))
    for dep in undocumented_runtime_deps:
        errors.append(f"runtime dependency {dep} requires a tracked owner/security justification")
    for dep, category in HIGH_RISK_DEPENDENCIES.items():
        if dep in runtime_deps and dep not in RUNTIME_DEPENDENCY_JUSTIFICATIONS:
            errors.append(f"high-risk dependency {dep} ({category}) lacks documented justification")
    if not REQUIRED_TOOL_MANIFEST.is_file():
        errors.append("missing tooling/configs/qa/tool-versions.json")
    if errors:
        fail(errors)
    print("OK dev dependency guardrails")
    return 0


if __name__ == "__main__":
    sys.exit(main())
