#!/usr/bin/env python3
from __future__ import annotations

import ast
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]

REQUIRED_TEST_DOC_TERMS = [
    "Contract tests",
    "Boundary parser tests",
    "Property regression tests",
    "State-machine tests",
    "Transfer integrity tests",
    "Diagnostics redaction tests",
    "Coverage floors",
]

REQUIRED_WRAPPER_DOC_TERMS = [
    "Wrapper routing",
    "release-harness.sh",
    "checkpoint.sh",
    "coverage.sh",
    "licenses.sh --check",
    "supply-chain.sh",
    "closeout.sh",
    "release.sh",
    "package-windows.sh",
    "windows-smoke-from-wsl.sh",
    "Desktop E2E receipts",
]

OWNER_MODULES = [
    "app",
    "protocol",
    "ipc",
    "settings",
    "assets",
    "tray",
    "hotkeys",
    "capture",
    "recording",
    "audio",
    "telemetry",
    "transfer",
    "install",
    "diagnostics",
]

RELEASE_WRAPPER = ROOT / "tooling/qa/wrappers/release.sh"
BUILD_WRAPPER = ROOT / "tooling/qa/wrappers/build.sh"
CLOSEOUT_WRAPPER = ROOT / "tooling/qa/wrappers/closeout.sh"
CHECKPOINT_WRAPPER = ROOT / "tooling/qa/wrappers/checkpoint.sh"
QA_LIB = ROOT / "tooling/qa/runtime/lib.sh"
QA_REPOSITORY_RUNTIME = ROOT / "tooling/qa/runtime/repository.py"
WINDOWS_SMOKE_WRAPPER = ROOT / "tooling/qa/wrappers/windows-smoke.sh"
WINDOWS_SMOKE_IMPLEMENTATION = ROOT / "tooling/release/windows/smoke/windows-smoke.sh"
COVERAGE_WRAPPER = ROOT / "tooling/qa/wrappers/coverage.sh"
PRE_COMMIT_HOOK = ROOT / "tooling/qa/hooks/pre-commit"
MAX_TEST_FILE_LINES = 1400
SCENARIO_SPLIT_PARENTS = [
    ROOT / "src/app/tests/mod.rs",
    ROOT / "src/audio/tests/mod.rs",
    ROOT / "src/recording/tests.rs",
    ROOT / "src/capture/tests.rs",
    ROOT / "src/protocol/tests/contract.rs",
    ROOT / "src/telemetry/tests/mod.rs",
    ROOT / "src/transfer/tests/mod.rs",
    ROOT / "src/tray/tests/mod.rs",
]
REQUIRED_SCENARIO_TEST_FILES = [
    ROOT / "src/app/tests/command_flow.rs",
    ROOT / "src/app/tests/host_modes.rs",
    ROOT / "src/app/tests/native_session.rs",
    ROOT / "src/app/tests/resident_bridge.rs",
    ROOT / "src/audio/tests/adapters.rs",
    ROOT / "src/audio/tests/devices.rs",
    ROOT / "src/audio/tests/mix_plan.rs",
    ROOT / "src/audio/tests/pcm_mixer.rs",
    ROOT / "src/audio/tests/pipeline.rs",
    ROOT / "src/recording/tests/encoder.rs",
    ROOT / "src/recording/tests/platform.rs",
    ROOT / "src/recording/tests/state_service.rs",
    ROOT / "src/recording/tests/temp_media.rs",
    ROOT / "src/capture/tests/geometry.rs",
    ROOT / "src/capture/tests/source_plan.rs",
    ROOT / "src/capture/tests/transfer_service.rs",
    ROOT / "src/protocol/tests/contract/fixtures.rs",
    ROOT / "src/protocol/tests/contract/media_limits.rs",
    ROOT / "src/protocol/tests/contract/metadata.rs",
    ROOT / "src/protocol/tests/contract/validation.rs",
    ROOT / "src/telemetry/tests/collector.rs",
    ROOT / "src/telemetry/tests/hook_policy.rs",
    ROOT / "src/telemetry/tests/redaction_output_surface.rs",
    ROOT / "src/telemetry/tests/runtime.rs",
    ROOT / "src/telemetry/tests/sanitizer.rs",
    ROOT / "src/telemetry/tests/snapshot.rs",
    ROOT / "src/telemetry/tests/timebase.rs",
    ROOT / "src/transfer/tests/chunking.rs",
    ROOT / "src/transfer/tests/hashing.rs",
    ROOT / "src/transfer/tests/identity.rs",
    ROOT / "src/transfer/tests/lifecycle.rs",
    ROOT / "src/transfer/tests/messages.rs",
    ROOT / "src/transfer/tests/pending.rs",
    ROOT / "src/transfer/tests/protected_file.rs",
    ROOT / "src/transfer/tests/protected_key.rs",
    ROOT / "src/tray/tests/actions.rs",
    ROOT / "src/tray/tests/browser.rs",
    ROOT / "src/tray/tests/consent.rs",
    ROOT / "src/tray/tests/icon.rs",
    ROOT / "src/tray/tests/menu.rs",
]
REQUIRED_PROPERTY_TEST_MARKERS = {
    ROOT / "src/diagnostics/tests/redaction.rs": [
        "proptest!",
        "redaction_corpus_required_outputs_cover_fuzz_sensitive_categories",
    ],
    ROOT / "src/ipc/tests/controller_lease_auth.rs": [
        "proptest!",
        "controller_lease_rejects_stale_or_replayed_lease_ids",
    ],
    ROOT / "src/ipc/tests/native_messaging.rs": [
        "proptest!",
        "native_message_declared_lengths_above_limit_never_read_body",
    ],
    ROOT / "src/ipc/tests/pipe_auth.rs": [
        "proptest!",
        "pipe_auth_rejects_tampered_handshake_fields",
    ],
    ROOT / "src/protocol/tests/contract/validation.rs": [
        "proptest!",
        "controller_lease_identifier_rejects_extension_incompatible_ascii",
    ],
    ROOT / "src/settings/tests.rs": [
        "proptest!",
        "effective_settings_policy_bounds_manual_bitrate_and_fps",
    ],
    ROOT / "src/transfer/tests/chunking.rs": [
        "proptest!",
        "encode_chunk_tracks_offsets_hashes_and_budgets",
    ],
    ROOT / "src/transfer/tests/identity.rs": [
        "proptest!",
        "transfer_identity_rejects_pathlike_or_reserved_filename_fragments",
    ],
    ROOT / "src/transfer/tests/pending.rs": [
        "proptest!",
        "pending_transfer_accepts_in_order_state_progression",
    ],
    ROOT / "src/transfer/tests/protected_key.rs": [
        "proptest!",
        "protected_media_key_rejects_malformed_hex_metadata",
    ],
}

REQUIRED_PERF_PROOF_MARKERS = {
    ROOT / "src/app/resident/capture_worker/tests.rs": [
        "capture_worker_queue_backpressure_burst_stays_bounded",
    ],
    ROOT / "src/app/resident/recording_worker/tests.rs": [
        "recording_worker_queue_backpressure_burst_stays_bounded",
    ],
    ROOT / "src/app/tests/resident_bridge.rs": [
        "pipe_bridge_startup_latency_budget_is_attempt_bounded",
    ],
    ROOT / "src/capture/tests/transfer_service.rs": [
        "screenshot_finalize_chunk_read_path_handles_bounded_multi_chunk_media",
        "screenshot_transfer_cleanup_scan_classifies_bounded_mixed_records",
    ],
    ROOT / "src/recording/tests/temp_media.rs": [
        "recording_finalize_chunk_read_path_handles_bounded_multi_chunk_media",
        "recording_transfer_cleanup_scan_classifies_bounded_mixed_records",
    ],
    ROOT / "src/telemetry/tests/hook_policy.rs": [
        "telemetry_hook_drop_counters_accumulate_saturated_burst_without_growth",
    ],
    ROOT / "src/transfer/tests/lifecycle.rs": [
        "transfer_cleanup_scan_budget_classifies_batch_without_payload_access",
    ],
}


def fail(message: str) -> None:
    print(f"FAIL {message}")
    raise SystemExit(1)


def non_comment_rust_lines(path: Path) -> list[str]:
    lines: list[str] = []
    for raw in path.read_text(encoding="utf-8").splitlines():
        stripped = raw.strip()
        if not stripped or stripped.startswith("//") or stripped.startswith("#!["):
            continue
        lines.append(stripped)
    return lines


def check_test_policy_doc() -> None:
    path = ROOT / "docs/engineering/testing-policy.md"
    if not path.is_file():
        fail("missing docs/engineering/testing-policy.md")
    text = path.read_text(encoding="utf-8")
    missing = [term for term in REQUIRED_TEST_DOC_TERMS if term not in text]
    if missing:
        fail("testing policy is missing required terms: " + ", ".join(missing))


def check_tooling_guardrail_inventory_doc() -> None:
    path = ROOT / "docs/tooling/qa.md"
    if not path.is_file():
        fail("missing docs/tooling/qa.md")
    text = path.read_text(encoding="utf-8")
    missing = [term for term in REQUIRED_WRAPPER_DOC_TERMS if term not in text]
    if missing:
        fail("tooling wrapper docs are missing guardrail inventory terms: " + ", ".join(missing))


def check_owner_test_roots() -> None:
    missing = []
    for module in OWNER_MODULES:
        module_path = ROOT / "src" / module
        if not module_path.exists():
            continue
        test_path = module_path / "tests"
        if not test_path.exists():
            missing.append(f"src/{module}/tests/.gitkeep")
    if missing:
        fail("missing owner-local test roots: " + ", ".join(missing))


def check_non_scaffold_modules_have_tests() -> None:
    offenders = []
    for module in OWNER_MODULES:
        module_path = ROOT / "src" / module
        if not module_path.exists():
            continue
        rust_files = [path for path in module_path.rglob("*.rs") if "/tests/" not in path.as_posix()]
        code_lines: list[str] = []
        for path in rust_files:
            code_lines.extend(non_comment_rust_lines(path))
        scaffold_only = all(
            line.startswith("//!")
            or line in {"pub(crate) fn run() {", "}"}
            or line.startswith("mod ")
            for line in code_lines
        )
        if scaffold_only:
            continue
        has_tests = any((module_path / "tests").rglob("*.rs")) or any(
            "#[cfg(test)]" in path.read_text(encoding="utf-8") for path in rust_files
        )
        if not has_tests:
            offenders.append(f"src/{module}")
    if offenders:
        fail("non-scaffold owner modules require owner-local tests: " + ", ".join(offenders))


def check_test_topology_is_scenario_sliced() -> None:
    missing = [
        path.relative_to(ROOT).as_posix()
        for path in REQUIRED_SCENARIO_TEST_FILES
        if not path.is_file()
    ]
    if missing:
        fail("missing scenario-local test files: " + ", ".join(missing))

    unsplit_parents = [
        path.relative_to(ROOT).as_posix()
        for path in SCENARIO_SPLIT_PARENTS
        if path.is_file() and "#[test]" in path.read_text(encoding="utf-8")
    ]
    if unsplit_parents:
        fail("scenario-split parent test modules must not contain #[test]: " + ", ".join(unsplit_parents))

    oversized = []
    for path in (ROOT / "src").rglob("*.rs"):
        relative = path.relative_to(ROOT).as_posix()
        if "/tests" not in relative:
            continue
        lines = len(path.read_text(encoding="utf-8").splitlines())
        if lines > MAX_TEST_FILE_LINES:
            oversized.append(f"{relative} has {lines} lines")
    if oversized:
        fail(
            f"owner test files must stay under {MAX_TEST_FILE_LINES} lines: "
            + "; ".join(sorted(oversized))
        )


def check_property_harnesses_are_tracked() -> None:
    missing = []
    for path, markers in REQUIRED_PROPERTY_TEST_MARKERS.items():
        if not path.is_file():
            missing.append(path.relative_to(ROOT).as_posix())
            continue
        text = path.read_text(encoding="utf-8")
        for marker in markers:
            if marker not in text:
                missing.append(f"{path.relative_to(ROOT).as_posix()}::{marker}")
    if missing:
        fail("missing property regression test markers: " + ", ".join(missing))


def check_perf_proof_harnesses_are_tracked() -> None:
    missing = []
    for path, markers in REQUIRED_PERF_PROOF_MARKERS.items():
        if not path.is_file():
            missing.append(path.relative_to(ROOT).as_posix())
            continue
        text = path.read_text(encoding="utf-8")
        for marker in markers:
            if marker not in text:
                missing.append(f"{path.relative_to(ROOT).as_posix()}::{marker}")
    if missing:
        fail("missing perf/latency proof test markers: " + ", ".join(missing))


def check_release_wrapper_contract() -> None:
    missing = [
        path.relative_to(ROOT).as_posix()
        for path in (
            RELEASE_WRAPPER,
            BUILD_WRAPPER,
            CLOSEOUT_WRAPPER,
            CHECKPOINT_WRAPPER,
            COVERAGE_WRAPPER,
            QA_LIB,
            WINDOWS_SMOKE_WRAPPER,
            PRE_COMMIT_HOOK,
        )
        if not path.is_file()
    ]
    if missing:
        fail("missing QA wrapper files: " + ", ".join(missing))

    build = BUILD_WRAPPER.read_text(encoding="utf-8")
    closeout = CLOSEOUT_WRAPPER.read_text(encoding="utf-8")
    release = RELEASE_WRAPPER.read_text(encoding="utf-8")
    checkpoint = CHECKPOINT_WRAPPER.read_text(encoding="utf-8")
    coverage = COVERAGE_WRAPPER.read_text(encoding="utf-8")
    qa_lib = QA_LIB.read_text(encoding="utf-8")
    qa_repository = QA_REPOSITORY_RUNTIME.read_text(encoding="utf-8")
    windows_smoke = WINDOWS_SMOKE_IMPLEMENTATION.read_text(encoding="utf-8")
    pre_commit = PRE_COMMIT_HOOK.read_text(encoding="utf-8")

    wrapper_texts = {
        "build.sh": build,
        "closeout.sh": closeout,
        "release.sh": release,
        "checkpoint.sh": checkpoint,
        "coverage.sh": coverage,
        "lib.sh": qa_lib,
        "windows-smoke.sh": windows_smoke,
        "pre-commit": pre_commit,
    }
    stale = [name for name, text in wrapper_texts.items() if "seamgate" in text]
    if stale:
        fail("QA wrappers must use checkpoint terminology: " + ", ".join(stale))

    required_build_terms = [
        "--profile dev|release",
        "qa_assert_state checkpoint",
        "cargo build --release",
        "cargo check --release --target x86_64-pc-windows-msvc",
    ]
    missing_build_terms = [term for term in required_build_terms if term not in build]
    if missing_build_terms:
        fail("build wrapper is missing profile contract terms: " + ", ".join(missing_build_terms))

    if "tooling/qa/wrappers/checkpoint.sh" not in closeout:
        fail("closeout wrapper must call checkpoint.sh")
    if (
        'SNIPTALE_QA_BUILD_WRAPPER:-tooling/qa/wrappers/build.sh' not in closeout
        or 'qa_step "build" "$build_wrapper" --profile dev' not in closeout
    ):
        fail("closeout wrapper must call build.sh with the dev profile")
    required_coverage_terms = [
        "cargo llvm-cov",
        "--fail-under-lines",
        ".tmp/qa/coverage/lcov.info",
        ".tmp/qa/coverage/summary.txt",
        ".tmp/qa/coverage/metadata.json",
        ".tmp/qa/coverage/owner-floors.json",
        "--check-lcov",
    ]
    missing_coverage_terms = [term for term in required_coverage_terms if term not in coverage]
    if missing_coverage_terms:
        fail(
            "coverage wrapper is missing required gate/artifact terms: "
            + ", ".join(missing_coverage_terms)
        )
    if 'qa_step "checkpoint static proof" python3 -m tooling.qa.runtime.checkpoint_checks' not in checkpoint:
        fail("checkpoint wrapper must call the independent proof catalog")
    try:
        catalog = ast.parse((ROOT / "tooling/qa/runtime/checkpoint_checks.py").read_text())
        checks = next(ast.literal_eval(node.value) for node in catalog.body
                      if isinstance(node, ast.Assign) and any(isinstance(target, ast.Name) and target.id == "CHECKS" for target in node.targets))
    except (OSError, ValueError, SyntaxError, StopIteration):
        fail("checkpoint proof catalog must be a literal command list")
    if ("release artifact guardrails", ["python3", "tooling/release/windows/verify_release_artifacts.py"]) not in checks:
        fail("checkpoint catalog must call release artifact guardrails")
    if "cargo check --release --target x86_64-pc-windows-msvc --bin sniptale-native-agent" not in checkpoint:
        fail("checkpoint wrapper must check the Windows MSVC release executable target")
    if "tooling/qa/wrappers/checkpoint.sh" not in pre_commit:
        fail("pre-commit hook must call checkpoint.sh")
    if '"rev-parse", "--verify", "HEAD"' not in qa_repository:
        fail("checkpoint fingerprint must include the current HEAD")
    if "src/install" not in qa_lib:
        fail("high-risk wrapper predicate must include src/install")

    required_windows_smoke_terms = [
        "cargo build --all-targets --all-features",
        "cargo test --all-targets --all-features",
        "Windows host",
    ]
    missing_windows_smoke_terms = [
        term for term in required_windows_smoke_terms if term not in windows_smoke
    ]
    if missing_windows_smoke_terms:
        fail(
            "windows-smoke wrapper is missing runtime proof terms: "
            + ", ".join(missing_windows_smoke_terms)
        )

    required_release_terms = [
        "tooling/qa/wrappers/checkpoint.sh",
        "tooling/qa/wrappers/coverage.sh",
        "tooling/qa/wrappers/supply-chain.sh",
        "tooling/qa/wrappers/build.sh --profile release",
    ]
    missing_release_terms = [term for term in required_release_terms if term not in release]
    if missing_release_terms:
        fail("release wrapper is missing full-gate steps: " + ", ".join(missing_release_terms))

    forbidden_release_terms = ["high_risk_files_changed", "dependency_files_changed"]
    present_forbidden_terms = [term for term in forbidden_release_terms if term in release]
    if present_forbidden_terms:
        fail("release wrapper must not use diff-gated checks: " + ", ".join(present_forbidden_terms))


def main() -> int:
    check_test_policy_doc()
    check_tooling_guardrail_inventory_doc()
    check_owner_test_roots()
    check_non_scaffold_modules_have_tests()
    check_test_topology_is_scenario_sliced()
    check_property_harnesses_are_tracked()
    check_perf_proof_harnesses_are_tracked()
    check_release_wrapper_contract()
    print("OK test policy")
    return 0


if __name__ == "__main__":
    sys.exit(main())
