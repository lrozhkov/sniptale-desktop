#!/usr/bin/env python3
"""Unified CLI for the Sniptale isolated desktop E2E lab."""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import signal
import sys
import time
from pathlib import Path
from typing import Any

from artifacts import artifact_manifest, create_source_bundle, evidence_manifest, write_manifest
from config import LANES, LabConfig
from errors import IntegrityError, LabError, RunFailed, UsageError
from evidence import verify_run_evidence
from golden_storage import cleanup_source_images, compact_existing_goldens
from host import bootstrap, doctor
from paths import LabPaths
from progress import emit as emit_progress
from provision import Provisioner
from receipt import create_run_dir, redact, redact_data, write_receipt
from results import GuestResult, TestCase, write_junit
from runner import LaneRunner
from state import LabLock, read_json
from system import run, sha256_file
from vm import VmManager
from work_sessions import WorkSessions, load_session, state_path as session_state_path

RUN_ID = re.compile(r"^\d{8}T\d{6}Z(?:-\d{3})?$")


def _interrupt(_signum: int, _frame: object) -> None:
    raise KeyboardInterrupt


def parser() -> argparse.ArgumentParser:
    root = argparse.ArgumentParser(prog="tooling/desktop/e2e-lab")
    sub = root.add_subparsers(dest="command", required=True)
    session_parser = sub.add_parser("session")
    session_sub = session_parser.add_subparsers(dest="session_command", required=True)
    session_open = session_sub.add_parser("open")
    session_open.add_argument("--ttl-seconds", type=int, default=7200)
    session_sub.add_parser("status")
    session_close = session_sub.add_parser("close")
    session_close.add_argument("--save", action="store_true")
    session_sub.add_parser("recover")
    session_collect = session_sub.add_parser("collect")
    session_collect.add_argument("session_id")
    session_collect.add_argument("iteration")
    session_build = session_sub.add_parser("build")
    session_build.add_argument("--profile", choices=("dev", "release"), default="dev")
    session_test = session_sub.add_parser("test")
    session_test.add_argument("--cases", default="")
    session_test.add_argument("--extension-artifact", type=Path)
    session_test.add_argument("--extension-source", type=Path)
    doctor_parser = sub.add_parser("doctor")
    doctor_parser.add_argument("--json", action="store_true")
    sub.add_parser("bootstrap")
    provision_parser = sub.add_parser("provision")
    provision_parser.add_argument("lane", choices=(*LANES, "all"))
    start_parser = sub.add_parser("start")
    start_parser.add_argument("lane", choices=LANES)
    build_parser = sub.add_parser("build")
    build_parser.add_argument("lane", choices=LANES)
    build_parser.add_argument("--profile", choices=("dev", "release"), required=True)
    test_parser = sub.add_parser("test")
    test_parser.add_argument("lane", choices=LANES)
    test_parser.add_argument("--artifact", type=Path, required=True)
    test_parser.add_argument("--cases", default="")
    run_parser = sub.add_parser("run")
    run_parser.add_argument("--lane", choices=(*LANES, "all"), required=True)
    run_parser.add_argument("--build", choices=("current",), required=True)
    run_parser.add_argument("--profile", choices=("dev", "release"), default="release")
    status_parser = sub.add_parser("status")
    status_parser.add_argument("--json", action="store_true")
    collect_parser = sub.add_parser("collect")
    collect_parser.add_argument("run_id")
    rerun_parser = sub.add_parser("rerun-failed")
    rerun_parser.add_argument("run_id")
    reset_parser = sub.add_parser("reset")
    reset_parser.add_argument("lane", choices=LANES)
    stop_parser = sub.add_parser("stop")
    stop_group = stop_parser.add_mutually_exclusive_group(required=True)
    stop_group.add_argument("--all", action="store_true")
    stop_group.add_argument("--lane", choices=LANES)
    sub.add_parser("gc")
    destroy_parser = sub.add_parser("destroy")
    destroy_parser.add_argument("lane", choices=LANES)
    destroy_parser.add_argument("--yes", action="store_true")
    return root


def print_json(value: object) -> None:
    print(json.dumps(value, sort_keys=True, indent=2))


def command_doctor(paths: LabPaths, config: LabConfig, json_output: bool) -> None:
    checks = doctor(paths, config)
    payload = {
        "schema_version": 1,
        "passed": all(item.passed or not item.required for item in checks),
        "checks": [item.as_dict() for item in checks],
    }
    if json_output:
        print_json(payload)
    else:
        for item in checks:
            print(f"{'OK' if item.passed else 'FAIL'} {item.name}: {item.detail}")
    if not payload["passed"]:
        raise LabError("doctor found blocking host prerequisites")


def status(paths: LabPaths, config: LabConfig) -> dict[str, object]:
    manager = VmManager(paths, config)
    lanes: dict[str, object] = {}
    for name in LANES:
        state_error = None
        try:
            vm = manager.current(name, clean_stale=False)
        except IntegrityError as exc:
            vm = None
            state_error = str(exc)
        try:
            golden_dir = paths.golden / name
            if golden_dir.is_symlink() or golden_dir.resolve(strict=False) != golden_dir:
                raise IntegrityError(f"golden image directory is redirected for {name}")
            identity = read_json(golden_dir / "identity.json")
        except IntegrityError as exc:
            identity = {"error": str(exc)}
        orphan_pids = manager.orphan_pids(name)
        lanes[name] = {
            "running": vm is not None or bool(orphan_pids),
            "pid": vm.pid if vm else (orphan_pids[0] if orphan_pids else None),
            "golden": identity,
            "ssh": f"127.0.0.1:{config.lanes[name].ssh_port}",
            "vnc": f"127.0.0.1:{5900 + config.lanes[name].vnc_display}",
            "state_error": state_error,
        }
    runs = (
        sorted(path.name for path in paths.lab.iterdir() if path.is_dir() and RUN_ID.fullmatch(path.name))
        if paths.lab.exists()
        else []
    )
    return {"schema_version": 1, "lanes": lanes, "runs": runs}


def snapshot_identities(paths: LabPaths, lanes: list[str]) -> dict[str, object]:
    identities: dict[str, object] = {}
    for lane in lanes:
        try:
            identities[lane] = read_json(paths.golden / lane / "identity.json")
        except IntegrityError as exc:
            identities[lane] = {"error": redact(str(exc))}
    return identities


def execute_run(
    paths: LabPaths,
    config: LabConfig,
    lanes: list[str],
    profile: str,
    supplied_artifact: Path | None,
    requested_cases: set[str] | None = None,
    retry_of: str | None = None,
) -> str:
    if requested_cases is not None and lanes != ["windows-11"]:
        raise UsageError("focused E2E cases are supported only for the windows-11 lane")
    if supplied_artifact is None:
        gc(paths, config)
    run_id, run_dir = create_run_dir(paths.runs)
    run_mode = (
        "focused-rerun"
        if retry_of is not None
        else "focused-test"
        if requested_cases is not None
        else "alpha-release-acceptance"
        if supplied_artifact is not None and (supplied_artifact.parent / "provenance.json").is_file()
        else "full-acceptance"
    )
    emit_progress(
        "run-start",
        runId=run_id,
        mode=run_mode,
        lanes=lanes,
        requestedCases=sorted(requested_cases or []),
        retryOf=retry_of,
    )
    source_archive = run_dir / ".source.tar.gz"
    emit_progress("source-bundle", runId=run_id, status="started")
    source = create_source_bundle(paths.root, source_archive)
    emit_progress(
        "source-bundle",
        runId=run_id,
        status="completed",
        sourceSha256=source.sha256,
        fileCount=source.file_count,
    )
    lane_runner = LaneRunner(paths, config)
    results: list[GuestResult] = []
    artifacts: list[Path] = []
    durations: dict[str, int] = {}
    controller_error: LabError | None = None
    started_run = time.monotonic()
    for lane_index, lane in enumerate(lanes):
        started = time.monotonic()
        try:
            if supplied_artifact is None:
                emit_progress("lane-build", runId=run_id, lane=lane, status="started")
                built = lane_runner.build(lane, profile, source, run_dir)
                emit_progress("lane-build", runId=run_id, lane=lane, status="completed")
                artifacts.extend(built)
                suffix = ".msi" if lane == "windows-11" else ".deb"
                candidates = [path for path in built if path.suffix.lower() == suffix]
                candidate = next((path for path in candidates if "-0.1.0-" in path.name), None) or next(
                    iter(candidates), None
                )
                if candidate is None:
                    raise LabError(f"no testable artifact was built for {lane}")
            else:
                candidate = supplied_artifact.resolve(strict=True)
                artifacts.append(candidate)
            emit_progress(
                "lane-test",
                runId=run_id,
                lane=lane,
                status="started",
                timeoutSeconds=getattr(getattr(config, "timeouts", None), "test", 0),
            )
            if requested_cases is None:
                results.append(lane_runner.test(lane, candidate, run_dir))
            else:
                results.append(lane_runner.test(lane, candidate, run_dir, requested_cases))
            emit_progress("lane-test", runId=run_id, lane=lane, status="completed")
            artifacts.extend(lane_runner.consumed_artifacts)
            lane_runner.consumed_artifacts.clear()
        except (LabError, OSError, KeyboardInterrupt) as exc:
            message = redact(str(exc))
            if isinstance(exc, KeyboardInterrupt):
                message = "controller interrupted"
            results.append(GuestResult(lane, (TestCase("controller", "fail", 0, message),), {}, ()))
            controller_error = exc if isinstance(exc, LabError) else LabError(message)
            emit_progress("lane-test", runId=run_id, lane=lane, status="failed", detail=message)
            for unattempted in lanes[lane_index + 1 :]:
                results.append(
                    GuestResult(
                        unattempted,
                        (TestCase("controller", "fail", 0, "not attempted because a prior requested lane failed"),),
                        {},
                        (),
                    )
                )
                durations[unattempted] = 0
                lane_runner.teardowns.setdefault(
                    unattempted,
                    {
                        "pre_run_forced": False,
                        "post_run_forced": False,
                    },
                )
            break
        finally:
            durations[lane] = int((time.monotonic() - started) * 1000)
            lane_runner.teardowns.setdefault(
                lane,
                {
                    "pre_run_forced": False,
                    "post_run_forced": False,
                },
            )
    manifest_value = artifact_manifest(dict.fromkeys(artifacts))
    manifest_path = run_dir / "artifact-manifest.json"
    write_manifest(manifest_path, manifest_value)
    environment = {
        "schema_version": 1,
        "host": {
            "kernel": os.uname().release,
            "machine": os.uname().machine,
            "acceleration": "kvm" if os.access("/dev/kvm", os.R_OK | os.W_OK) else "tcg",
        },
        "lanes": snapshot_identities(paths, lanes),
        "guests": {result.lane: redact_data(result.environment) for result in results},
    }
    (run_dir / "environment.json").write_text(
        json.dumps(environment, sort_keys=True, indent=2) + "\n", encoding="utf-8"
    )
    write_junit(run_dir / "junit.xml", results)
    evidence_value = evidence_manifest(run_dir)
    evidence_manifest_path = run_dir / "evidence-manifest.json"
    write_manifest(evidence_manifest_path, evidence_value)
    clean_teardown = all(not any(values.values()) for values in lane_runner.teardowns.values())
    passed = len(results) == len(lanes) and all(result.passed for result in results) and clean_teardown
    matrix = [
        {
            "lane": result.lane,
            "tests": [
                {
                    "name": case.name,
                    "status": case.status,
                    "duration_ms": case.duration_ms,
                    "detail": redact(case.detail),
                }
                for case in result.cases
            ],
        }
        for result in results
    ]
    receipt = {
        "schema_version": 1,
        "run_id": run_id,
        "source": {"sha256": source.sha256, "file_count": source.file_count},
        "commit_sha": source.commit_sha,
        "dirty_state_sha256": source.dirty_sha256,
        "artifact_manifest_sha256": sha256_file(manifest_path),
        "evidence_manifest_sha256": sha256_file(evidence_manifest_path),
        "artifacts": manifest_value["artifacts"],
        "lanes": lanes,
        "snapshots": environment["lanes"],
        "test_matrix": matrix,
        "durations_ms": {**durations, "total": int((time.monotonic() - started_run) * 1000)},
        "teardown": lane_runner.teardowns,
        "build_cache": getattr(lane_runner, "build_caches", {}),
        "result": "pass" if passed else "fail",
        "run_mode": run_mode,
        "requested_cases": sorted(requested_cases or []),
        "retry_of": retry_of,
    }
    write_receipt(run_dir / "receipt.json", receipt)
    if passed and run_mode == "alpha-release-acceptance":
        try:
            verify_run_evidence(run_dir)
        except LabError as exc:
            controller_error = exc
            passed = False
            receipt["result"] = "fail"
            write_receipt(run_dir / "receipt.json", receipt)
    emit_progress(
        "run-finish",
        runId=run_id,
        mode=run_mode,
        status=receipt["result"],
        cleanTeardown=clean_teardown,
    )
    source_archive.unlink(missing_ok=True)
    # Supplied artifacts can live in an older run, so pre-run GC must not delete their source.
    # Once evidence is finalized the artifact is no longer in use and every execution, including
    # focused reruns and failures, can enforce the same bounded temporary-run retention.
    gc(paths, config)
    if controller_error is not None:
        raise RunFailed(run_id, f"run {run_id} failed: {controller_error}", run_mode)
    if not passed:
        raise RunFailed(
            run_id,
            f"run {run_id} contains failed guest tests or forced teardown",
            run_mode,
        )
    return run_id


def execute_build(paths: LabPaths, config: LabConfig, lane: str, profile: str) -> tuple[str, list[Path]]:
    gc(paths, config)
    run_id, run_dir = create_run_dir(paths.runs)
    emit_progress("build-start", runId=run_id, lane=lane, profile=profile)
    source_archive = run_dir / ".source.tar.gz"
    source = create_source_bundle(paths.root, source_archive)
    started = time.monotonic()
    lane_runner = LaneRunner(paths, config)
    built: list[Path] = []
    build_error: LabError | None = None
    try:
        built = lane_runner.build(lane, profile, source, run_dir)
    except (LabError, OSError, KeyboardInterrupt) as exc:
        if isinstance(exc, KeyboardInterrupt):
            exc = LabError("controller interrupted")
        build_error = exc if isinstance(exc, LabError) else LabError(redact(str(exc)))
    duration = int((time.monotonic() - started) * 1000)
    manifest_path = run_dir / "artifact-manifest.json"
    manifest_value = artifact_manifest(built)
    write_manifest(manifest_path, manifest_value)
    environment = {
        "schema_version": 1,
        "host": {
            "kernel": os.uname().release,
            "machine": os.uname().machine,
            "acceleration": "kvm" if os.access("/dev/kvm", os.R_OK | os.W_OK) else "tcg",
        },
        "lanes": snapshot_identities(paths, [lane]),
        "guests": {},
    }
    (run_dir / "environment.json").write_text(
        json.dumps(environment, sort_keys=True, indent=2) + "\n", encoding="utf-8"
    )
    clean_teardown = all(not any(values.values()) for values in lane_runner.teardowns.values())
    if build_error is None and not clean_teardown:
        build_error = LabError("build VM required forced teardown")
    status_value = "fail" if build_error else "pass"
    detail = redact(str(build_error)) if build_error else "artifact build completed"
    result = GuestResult(
        lane, (TestCase(f"build-{profile}", status_value, duration, detail),), {}, tuple(path.name for path in built)
    )
    write_junit(run_dir / "junit.xml", [result])
    evidence_value = evidence_manifest(run_dir)
    evidence_manifest_path = run_dir / "evidence-manifest.json"
    write_manifest(evidence_manifest_path, evidence_value)
    write_receipt(
        run_dir / "receipt.json",
        {
            "schema_version": 1,
            "run_id": run_id,
            "source": {"sha256": source.sha256, "file_count": source.file_count},
            "commit_sha": source.commit_sha,
            "dirty_state_sha256": source.dirty_sha256,
            "artifact_manifest_sha256": sha256_file(manifest_path),
            "evidence_manifest_sha256": sha256_file(evidence_manifest_path),
            "artifacts": manifest_value["artifacts"],
            "lanes": [lane],
            "snapshots": environment["lanes"],
            "test_matrix": [
                {
                    "lane": lane,
                    "tests": [
                        {"name": f"build-{profile}", "status": status_value, "duration_ms": duration, "detail": detail}
                    ],
                }
            ],
            "durations_ms": {lane: duration, "total": duration},
            "teardown": lane_runner.teardowns,
            "result": status_value,
        },
    )
    source_archive.unlink(missing_ok=True)
    # Build overlays and old evidence are transient too. Enforce retention after every exit so a
    # build-only workflow cannot leave max_runs + 1 directories until the next test or manual GC.
    gc(paths, config)
    if build_error is not None:
        raise LabError(f"build {run_id} failed: {build_error}")
    emit_progress("build-finish", runId=run_id, lane=lane, profile=profile, status="pass")
    return run_id, built


def parse_case_selection(value: str) -> set[str] | None:
    if not value:
        return None
    values = {item.strip() for item in value.split(",") if item.strip()}
    if not values:
        raise UsageError("--cases requires at least one case name")
    return values


def failed_windows_rerun(
    paths: LabPaths,
    config: LabConfig,
    run_id: str,
) -> str:
    if not RUN_ID.fullmatch(run_id):
        raise UsageError("invalid run id")
    previous = paths.scoped(paths.lab / run_id)
    verify_run_evidence(previous)
    receipt = json.loads((previous / "receipt.json").read_text(encoding="utf-8"))
    failed = {
        str(test.get("name"))
        for lane in receipt.get("test_matrix", [])
        if isinstance(lane, dict) and lane.get("lane") == "windows-11"
        for test in lane.get("tests", [])
        if isinstance(test, dict) and test.get("status") == "fail" and test.get("name") != "controller"
    }
    if not failed:
        raise UsageError("selected run has no failed Windows guest cases")
    candidate = _resolve_rerun_windows_msi(paths, previous, receipt)
    return execute_run(
        paths,
        config,
        ["windows-11"],
        "release",
        candidate,
        failed,
        run_id,
    )


def _resolve_rerun_windows_msi(
    paths: LabPaths,
    previous: Path,
    receipt: dict[str, Any],
) -> Path:
    declared = [
        item
        for item in receipt.get("artifacts", [])
        if isinstance(item, dict)
        and isinstance(item.get("name"), str)
        and "-0.1.0-" in item["name"]
        and item["name"].lower().endswith(".msi")
        and isinstance(item.get("sha256"), str)
    ]
    if len(declared) != 1:
        raise LabError("selected run does not declare exactly one current Windows MSI")
    expected = declared[0]
    name = expected["name"]
    digest = expected["sha256"]
    search_roots = (previous / "artifacts/windows-11", paths.cache / "builds/windows-11")
    candidates = {
        path.resolve()
        for root in search_roots
        if root.is_dir()
        for path in root.rglob(name)
        if path.is_file() and sha256_file(path) == digest
    }
    if not candidates:
        raise LabError("receipt-bound current Windows MSI is unavailable from run evidence and build cache")
    return max(candidates, key=lambda path: (path.stat().st_mtime_ns, path.as_posix()))


def _build_run_key(path: Path) -> tuple[str, str] | None:
    try:
        receipt = read_json(path / "receipt.json")
    except IntegrityError:
        return None
    if receipt is None:
        return None
    matrix = receipt.get("test_matrix")
    if not isinstance(matrix, list) or len(matrix) != 1 or not isinstance(matrix[0], dict):
        return None
    lane = matrix[0].get("lane")
    tests = matrix[0].get("tests")
    if not isinstance(lane, str) or not isinstance(tests, list) or len(tests) != 1:
        return None
    test = tests[0]
    name = test.get("name") if isinstance(test, dict) else None
    if name not in {"build-dev", "build-release"}:
        return None
    return lane, name


def gc(paths: LabPaths, config: LabConfig) -> dict[str, int]:
    now = time.time()
    run_dirs = (
        sorted(
            (path for path in paths.lab.iterdir() if path.is_dir() and RUN_ID.fullmatch(path.name)),
            key=lambda path: path.stat().st_mtime,
            reverse=True,
        )
        if paths.lab.exists()
        else []
    )
    removed = 0
    incomplete = 0
    redundant_builds = 0
    complete_seen = 0
    retained_build_keys: set[tuple[str, str]] = set()
    for path in run_dirs:
        too_old = now - path.stat().st_mtime > config.limits.max_run_age_days * 86400
        abandoned = not (path / "receipt.json").is_file()
        too_many = complete_seen >= config.limits.max_runs
        build_key = None if abandoned else _build_run_key(path)
        redundant_build = build_key is not None and build_key in retained_build_keys
        if too_many or too_old or abandoned or redundant_build:
            shutil.rmtree(paths.scoped(path))
            removed += 1
            incomplete += int(abandoned)
            redundant_builds += int(redundant_build)
        else:
            complete_seen += 1
            if build_key is not None:
                retained_build_keys.add(build_key)
    partials = 0
    for path in paths.lab.rglob("*.partial"):
        if now - path.stat().st_mtime > 86400:
            paths.scoped(path).unlink()
            partials += 1
    package_archives = 0
    apt_archives = paths.cache / "apt-archives"
    if apt_archives.is_dir():
        for path in apt_archives.glob("*.deb"):
            paths.scoped(path).unlink()
            package_archives += 1
    build_caches = (
        sorted(
            (path for path in (paths.cache / "builds").glob("*/*/*") if path.is_dir() and not path.is_symlink()),
            key=lambda path: path.stat().st_mtime,
            reverse=True,
        )
        if (paths.cache / "builds").is_dir()
        else []
    )
    build_caches_removed = 0
    for index, path in enumerate(build_caches):
        too_old = now - path.stat().st_mtime > config.limits.max_run_age_days * 86400
        if index >= 8 or too_old:
            shutil.rmtree(paths.scoped(path))
            build_caches_removed += 1
    storage = compact_existing_goldens(paths, config, VmManager(paths, config))
    from work_sessions import gc_sessions

    sessions_removed = gc_sessions(paths, config)
    source_images = cleanup_source_images(paths)
    return {
        "runs_removed": removed,
        "sessions_removed": sessions_removed,
        "incomplete_runs_removed": incomplete,
        "redundant_build_runs_removed": redundant_builds,
        "partials_removed": partials,
        "package_archives_removed": package_archives,
        "build_caches_removed": build_caches_removed,
        **storage,
        **source_images,
    }


def requires_fast_sources(args: argparse.Namespace) -> bool:
    return args.command in {"provision", "start", "build", "test", "run", "rerun-failed"} or (
        args.command == "session" and args.session_command in {"open", "build", "test"}
    )


def source_stamp(paths: LabPaths) -> str:
    value = run(["python3", "-m", "tooling.qa.runtime.cli", "fingerprint"], cwd=paths.root, timeout=30).stdout.strip()
    if re.fullmatch(r"[0-9a-f]{64}", value) is None:
        raise IntegrityError("invalid fast source fingerprint")
    return value


def main(argv: list[str] | None = None) -> int:
    signal.signal(signal.SIGTERM, _interrupt)
    args = parser().parse_args(argv)
    paths = LabPaths.discover()
    config = LabConfig.load(paths.root)
    try:
        if args.command == "doctor":
            command_doctor(paths, config, args.json)
            return 0
        if args.command == "status":
            value = status(paths, config)
            print_json(value) if args.json else print_json(value)
            return 0
        if args.command == "collect":
            if not RUN_ID.fullmatch(args.run_id):
                raise UsageError("invalid run id")
            run_dir = paths.scoped(paths.lab / args.run_id)
            summary = verify_run_evidence(run_dir)
            summary["path"] = run_dir.relative_to(paths.root).as_posix()
            print_json(summary)
            return 0
        admission = None
        if requires_fast_sources(args):
            admission = source_stamp(paths)
            # Acquire QA admission before the controller lock; cleanup commands remain available.
            run([str(paths.root / "tooling/qa/wrappers/fast.sh")], cwd=paths.root, timeout=600, stream=True)
        paths.ensure()
        with LabLock(paths.locks / "controller.lock", args.command):
            if admission is not None and source_stamp(paths) != admission:
                raise IntegrityError("desktop sources changed while acquiring the controller lock; rerun the command")
            manager = VmManager(paths, config)
            if args.command == "session":
                sessions = WorkSessions(paths, config)
                if args.session_command == "open":
                    print_json(sessions.open(args.ttl_seconds))
                elif args.session_command == "status":
                    print_json(load_session(paths))
                elif args.session_command == "close":
                    print_json(sessions.close(save=args.save))
                elif args.session_command == "recover":
                    print_json(sessions.recover())
                elif args.session_command == "collect":
                    from session_evidence import collect as collect_session

                    print_json(collect_session(paths, args.session_id, args.iteration))
                else:
                    print_json(
                        sessions.iterate(
                            args.session_command,
                            getattr(args, "profile", "dev"),
                            parse_case_selection(getattr(args, "cases", "")),
                            getattr(args, "extension_artifact", None),
                            getattr(args, "extension_source", None),
                        )
                    )
                return 0
            resets_windows = (args.command == "reset" and args.lane == "windows-11") or (
                args.command == "stop" and (args.all or args.lane == "windows-11")
            )
            if resets_windows:
                # Explicit recovery remains available even when the lease is corrupted.
                manager.reset("windows-11")
                session_state_path(paths).unlink(missing_ok=True)
            elif load_session(paths) is not None:
                raise UsageError("close the development session before clean lab operations")
            if args.command == "bootstrap":
                print_json(bootstrap(paths))
            elif args.command == "provision":
                provisioner = Provisioner(paths, config)
                for lane in config.expand_lane(args.lane):
                    print_json(provisioner.provision(lane))
            elif args.command == "start":
                vm = manager.start(args.lane)
                print_json({"lane": vm.lane, "pid": vm.pid})
            elif args.command == "reset":
                print_json({"lane": args.lane, "forced": manager.reset(args.lane)})
            elif args.command == "stop":
                lanes = list(LANES) if args.all else [args.lane]
                print_json({lane: {"forced": manager.stop(lane)} for lane in lanes})
            elif args.command == "destroy":
                if not args.yes:
                    raise UsageError("destroy requires --yes")
                manager.reset(args.lane)
                golden = paths.scoped(paths.golden / args.lane)
                if golden.exists():
                    shutil.rmtree(golden)
                print_json({"destroyed": args.lane})
            elif args.command == "gc":
                print_json(gc(paths, config))
            elif args.command == "build":
                run_id, built = execute_build(paths, config, args.lane, args.profile)
                print_json({"run_id": run_id, "artifacts": [str(path) for path in built]})
            elif args.command == "test":
                run_id = execute_run(
                    paths,
                    config,
                    [args.lane],
                    "release",
                    args.artifact,
                    parse_case_selection(args.cases),
                )
                print_json({"run_id": run_id, "result": "pass"})
            elif args.command == "rerun-failed":
                run_id = failed_windows_rerun(paths, config, args.run_id)
                print_json({"run_id": run_id, "result": "pass", "mode": "focused-rerun"})
            elif args.command == "run":
                run_id = execute_run(paths, config, config.expand_lane(args.lane), args.profile, None)
                print_json({"run_id": run_id, "result": "pass"})
            else:
                raise UsageError(f"unsupported command: {args.command}")
        return 0
    except RunFailed as exc:
        print_json({"run_id": exc.run_id, "result": "fail", "mode": exc.mode})
        print(f"ERROR: {redact(str(exc))}", file=sys.stderr)
        return exc.exit_code
    except LabError as exc:
        print(f"ERROR: {redact(str(exc))}", file=sys.stderr)
        return exc.exit_code


if __name__ == "__main__":
    raise SystemExit(main())
