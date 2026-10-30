"""Run independent proof commands with bounded concurrency and isolated logs."""
from __future__ import annotations

import tempfile
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

from .repository import ROOT
from .run import run_step
from .storage import atomic_write_json, append_bounded_log, ensure_private_directory, read_json


def run_group(checks: list[tuple[str, list[str]]], record: Path, log: Path, workers: int) -> int:
    if not 1 <= workers <= 4 or not checks or len(checks) > 32:
        raise ValueError("parallel proof requires 1..4 workers and 1..32 checks")
    if len({label for label, _ in checks}) != len(checks):
        raise ValueError("parallel proof labels must be unique")
    reports = ROOT / ".tmp/qa-reports"
    ensure_private_directory(reports)
    # Workers never update a shared record or log. Only the coordinator merges results.
    with tempfile.TemporaryDirectory(prefix="parallel-", dir=reports) as directory:
        work = Path(directory)

        def execute(item):
            index, (label, command) = item
            step_id = "qa.step." + "".join(
                char if char.isascii() and (char.isalnum() or char in "._-") else "-"
                for char in label.lower()
            )
            child_record, child_log = work / f"{index}.json", work / f"{index}.log"
            atomic_write_json(child_record, {"steps": [], "log": {"path": str(child_log.relative_to(ROOT))}})
            code = run_step(child_record, child_log, step_id, label, command)
            return code, read_json(child_record)["steps"][0], child_log

        with ThreadPoolExecutor(max_workers=workers) as executor:
            # map preserves catalog order; failures do not skip remaining checks.
            results = list(executor.map(execute, enumerate(checks)))
        parent = read_json(record)
        for code, step, child_log in results:
            step["parallelGroup"] = "checkpoint static proof"
            parent["steps"].append(step)
            append_bounded_log(log, child_log.read_text(encoding="utf-8"), ROOT)
            print(f"{'OK' if code == 0 else 'FAIL'} {step['label']} ({step['durationMs']}ms)", flush=True)
        atomic_write_json(record, parent)
        return next((code for code, _, _ in results if code), 0)
