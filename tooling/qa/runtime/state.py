from __future__ import annotations

import os
import platform
import shutil
from pathlib import Path

from .repository import check_plan, current_targets, fingerprint, harness_targets, revision
from .storage import atomic_write_json, iso_now, read_json


STATE_VERSION = "sniptale-qa-state-v2"


def proof_context(root: Path, name: str) -> dict:
    plan = check_plan(full=name == "checkpoint-full")
    tools = ["python3"]
    flags = {"NODE_OPTIONS": os.environ.get("NODE_OPTIONS", "")}
    if "ui" in plan or "scripts" in plan:
        tools.append("node")
    if "rust" in plan:
        tools.extend(["cargo", "rustc"])
        flags.update(
            {
                k: v
                for k, v in os.environ.items()
                if k.startswith(("RUST", "CARGO_PROFILE_"))
                or k in {"CARGO_BUILD_TARGET", "CARGO_ENCODED_RUSTFLAGS", "CARGO_TARGET_DIR"}
            }
        )
    result = {
        "plan": plan,
        "platform": platform.platform(),
        "tools": {t: shutil.which(t) for t in tools},
        "flags": flags,
    }
    if name.startswith(("checkpoint", "build")) and "ui" in plan:
        outputs = sorted(
            p.relative_to(root).as_posix() for p in (root / "ui/dist").rglob("*") if p.is_file() or p.is_symlink()
        )
        result["frontend"] = fingerprint(outputs)
    return result


def _targets(name: str) -> list[str]:
    targets = current_targets()
    return harness_targets(targets) if name == "release-harness" else targets


def state_path(root: Path, name: str) -> Path:
    return root / ".tmp/qa/state" / f"{name}.json"


def write_state(root: Path, name: str, run_id: str) -> None:
    targets = _targets(name)
    atomic_write_json(
        state_path(root, name),
        {
            "schemaVersion": 1,
            "contractVersion": STATE_VERSION,
            "name": name,
            "status": "passed",
            "producerRunId": run_id,
            "revision": revision(),
            "targetFiles": targets,
            "fingerprint": fingerprint(targets),
            "createdAt": iso_now(),
            "context": proof_context(root, name),
        },
    )


def assert_state(root: Path, name: str) -> None:
    path = state_path(root, name)
    if not path.is_file():
        raise RuntimeError(f"fresh {name} state required: {path.relative_to(root)} is missing")
    state = read_json(path)
    targets = _targets(name)
    expected = {
        "contractVersion": STATE_VERSION,
        "name": name,
        "status": "passed",
        "revision": revision(),
        "targetFiles": targets,
        "fingerprint": fingerprint(targets),
        "context": proof_context(root, name),
    }
    mismatches = [key for key, value in expected.items() if state.get(key) != value]
    if not isinstance(state.get("producerRunId"), str) or not state.get("producerRunId"):
        mismatches.append("producerRunId")
    if mismatches:
        raise RuntimeError(f"stale {name} state ({', '.join(sorted(set(mismatches)))})")


def require_harness_if_needed(root: Path) -> None:
    if harness_targets():
        assert_state(root, "release-harness")


def assert_run_inputs(record_path: Path) -> None:
    source = read_json(record_path).get("repository", {})
    targets = current_targets()
    expected = {"revision": revision(), "targetFiles": targets, "fingerprint": fingerprint(targets)}
    if not isinstance(source, dict) or any(source.get(key) != value for key, value in expected.items()):
        raise RuntimeError("tracked worktree changed during proof; rerun checkpoint")
