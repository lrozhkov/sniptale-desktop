#!/usr/bin/env python3
from __future__ import annotations

import json
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]
INDEX = ROOT / "contracts/native/contract-index.json"
FIXTURE_REQUIREMENTS = ROOT / "contracts/native/fixture-requirements.json"
POSITIVE_FIXTURES = ROOT / "contracts/native/fixtures/positive"
NEGATIVE_FIXTURES = ROOT / "contracts/native/fixtures/negative"
POSITIVE_FIXTURE_BUNDLE = POSITIVE_FIXTURES / "contract-fixtures.json"
NEGATIVE_FIXTURE_BUNDLE = NEGATIVE_FIXTURES / "contract-fixtures.json"
OPERATION_ERROR_CATALOG = ROOT / "contracts/native/operation-error-catalog.json"

REQUIRED_CONTRACT_IDS = [
    "NativeProtocolConstants",
    "NativePlatform",
    "NativeAutostartState",
    "NativeInstallState",
    "ExtensionHelloMessage",
    "AppHelloMessage",
    "NativeAppCapabilities",
    "NativeCapabilityUnavailableReason",
    "ExtensionControllerAcquireMessage",
    "AppControllerLeaseMessage",
    "ExtensionSettingsSyncMessage",
    "AppSettingsAcceptedMessage",
    "ExtensionPingMessage",
    "AppPongMessage",
    "NativeTrayActionRegistry",
    "AppTrayActionRequestedMessage",
    "ExtensionTrayActionResultMessage",
    "AppOpenSettingsRequestedMessage",
    "ExtensionScreenshotCaptureCommandMessage",
    "ExtensionRecordingStartCommandMessage",
    "ExtensionRecordingControlCommandMessage",
    "AppCommandAcceptedMessage",
    "AppScreenshotStartMessage",
    "AppScreenshotChunkMessage",
    "AppScreenshotCommitMessage",
    "ExtensionScreenshotChunkRequestMessage",
    "ExtensionScreenshotAckMessage",
    "ExtensionScreenshotRejectMessage",
    "AppRecordingStartedMessage",
    "AppRecordingProgressMessage",
    "AppRecordingStoppedMessage",
    "ExtensionRecordingChunkRequestMessage",
    "AppRecordingChunkMessage",
    "ExtensionRecordingAckMessage",
    "ExtensionRecordingRejectMessage",
    "NativeRecordingTimebase",
    "NativeRecordingTelemetrySnapshot",
    "AppOperationFailedMessage",
]

REQUIRED_LIMITS = {
    "hostToExtensionMaxMessageBytes": 1_048_576,
    "extensionToHostMaxMessageBytes": 1_048_576,
    "extensionToHostControlMaxMessageBytes": 1_048_576,
    "extensionToHostChromiumMaxMessageBytes": 64 * 1024 * 1024,
    "targetJsonUtf8Bytes": 900 * 1024,
    "chunkRawBytes": 512 * 1024,
    "chunkMaxBase64Chars": 700_000,
    "screenshotMaxTotalBytes": 256 * 1024 * 1024,
    "recordingMaxTotalBytes": 8 * 1024 * 1024 * 1024,
}

REQUIRED_CASES = [
    "positive-fixtures",
    "negative-fixtures",
    "malformed-json",
    "unsupported-protocol-version",
    "unsupported-settings-version",
    "oversized-extension-to-host-json",
    "oversized-host-to-extension-json",
    "stale-or-missing-controller-lease",
    "duplicate-or-replay-command",
    "duplicate-or-replay-transfer",
    "replayed-chunk-request",
    "out-of-bounds-chunk-index",
    "malformed-identifiers",
    "malformed-filenames",
    "settings-accepted",
    "operation-failure",
    "operation-error-catalog",
]

REQUIRED_COVERAGE_MARKERS = {
    "positive-fixtures": "src/protocol/tests/contract/fixtures.rs::positive_wire_contract_fixtures_parse",
    "negative-fixtures": "src/protocol/tests/contract/fixtures.rs::negative_wire_contract_fixtures_are_rejected",
    "malformed-json": "src/protocol/tests/contract/fixtures.rs::malformed_json_is_rejected",
    "unsupported-protocol-version": "src/protocol/tests/contract/fixtures.rs::unsupported_protocol_version_is_rejected",
    "unsupported-settings-version": (
        "src/app/responses/settings_sync/tests/mod.rs::"
        "settings_sync_rejects_unsupported_schema_version_without_commit"
    ),
    "oversized-extension-to-host-json": (
        "src/protocol/tests/contract/fixtures.rs::oversized_message_is_rejected_before_json_parse"
    ),
    "oversized-host-to-extension-json": (
        "src/protocol/tests/contract/media_limits.rs::chunk_json_size_cap_is_rejected"
    ),
    "stale-or-missing-controller-lease": (
        "src/app/tests/command_flow.rs::native_host_rejects_command_after_controller_lease_expiry"
    ),
    "duplicate-or-replay-command": (
        "src/app/tests/command_flow.rs::native_host_replays_previous_command_result_for_duplicate_id"
    ),
    "duplicate-or-replay-transfer": (
        "src/app/tests/command_flow.rs::native_host_rejects_unknown_screenshot_ack_with_typed_failure"
    ),
    "replayed-chunk-request": (
        "src/app/tests/command_flow.rs::native_host_rejects_replayed_recording_chunk_request_with_typed_failure"
    ),
    "out-of-bounds-chunk-index": (
        "src/capture/tests/transfer_service.rs::screenshot_transfer_message_rejects_out_of_bounds_chunk"
    ),
    "malformed-identifiers": (
        "src/protocol/tests/contract/validation.rs::invalid_identifier_is_rejected"
    ),
    "malformed-filenames": (
        "src/protocol/tests/contract/media_limits.rs::invalid_filename_is_rejected"
    ),
    "settings-accepted": (
        "src/protocol/tests/contract/validation.rs::settings_accepted_effective_settings_requires_settings_shape"
    ),
    "operation-failure": "src/app/tests/command_flow.rs::native_host_reports_command_dispatch_failure",
    "operation-error-catalog": (
        "src/protocol/tests/contract/metadata.rs::operation_error_catalog_matches_protocol_taxonomy"
    ),
}


def fail(messages: list[str]) -> None:
    print("FAIL contract guardrails")
    for message in messages:
        print(f"- {message}")
    raise SystemExit(1)


def contract_guardrail_errors(root: Path) -> list[str]:
    errors: list[str] = []
    index = root / "contracts/native/contract-index.json"
    fixture_requirements = root / "contracts/native/fixture-requirements.json"
    positive_fixtures = root / "contracts/native/fixtures/positive"
    negative_fixtures = root / "contracts/native/fixtures/negative"
    positive_fixture_bundle = positive_fixtures / "contract-fixtures.json"
    negative_fixture_bundle = negative_fixtures / "contract-fixtures.json"
    operation_error_catalog = root / "contracts/native/operation-error-catalog.json"

    if not index.is_file():
        return ["missing contracts/native/contract-index.json"]
    if not fixture_requirements.is_file():
        return ["missing contracts/native/fixture-requirements.json"]

    data = json.loads(index.read_text(encoding="utf-8"))
    ids = data.get("contractIds")
    if not isinstance(ids, list):
        errors.append("contractIds must be a list")
    else:
        missing = [contract_id for contract_id in REQUIRED_CONTRACT_IDS if contract_id not in ids]
        extra = [contract_id for contract_id in ids if contract_id not in REQUIRED_CONTRACT_IDS]
        duplicates = sorted({contract_id for contract_id in ids if ids.count(contract_id) > 1})
        if missing:
            errors.append("missing Contract IDs: " + ", ".join(missing))
        if extra:
            errors.append("unknown Contract IDs: " + ", ".join(extra))
        if duplicates:
            errors.append("duplicate Contract IDs: " + ", ".join(duplicates))

    limits = data.get("nativeMessagingLimits")
    if not isinstance(limits, dict):
        errors.append("nativeMessagingLimits must be an object")
    else:
        for key, expected in REQUIRED_LIMITS.items():
            actual = limits.get(key)
            if actual != expected:
                errors.append(f"{key} must be {expected}, got {actual}")

    if data.get("protocolVersion") != 1:
        errors.append("protocolVersion must be 1")
    if data.get("settingsSchemaVersion") != 1:
        errors.append("settingsSchemaVersion must be 1")
    if data.get("cacheSchemaVersion") != 1:
        errors.append("cacheSchemaVersion must be 1")

    requirements = json.loads(fixture_requirements.read_text(encoding="utf-8"))
    required_cases = requirements.get("requiredCases")
    if required_cases != REQUIRED_CASES:
        errors.append("fixture-requirements.json requiredCases are incomplete or out of order")

    coverage_markers = requirements.get("coverageMarkers")
    if coverage_markers != REQUIRED_COVERAGE_MARKERS:
        errors.append("fixture-requirements.json coverageMarkers are incomplete or out of date")
    elif isinstance(coverage_markers, dict):
        for scenario, marker in coverage_markers.items():
            relative, _, test_name = marker.partition("::")
            if not relative or not test_name:
                errors.append(f"{scenario} coverage marker must be path::test_name")
                continue
            path = root / relative
            if not path.is_file():
                errors.append(f"{scenario} coverage marker file is missing: {relative}")
                continue
            text = path.read_text(encoding="utf-8")
            if f"fn {test_name}" not in text:
                errors.append(f"{scenario} coverage marker test is missing: {marker}")

    if not operation_error_catalog.is_file():
        errors.append("missing contracts/native/operation-error-catalog.json")
    else:
        catalog = json.loads(operation_error_catalog.read_text(encoding="utf-8"))
        if catalog.get("schemaVersion") != 1:
            errors.append("operation-error-catalog.json schemaVersion must be 1")
        public_codes = catalog.get("publicCodes")
        if not isinstance(public_codes, list) or not public_codes:
            errors.append("operation-error-catalog.json publicCodes must be a non-empty list")
        elif len(public_codes) != len(set(public_codes)):
            errors.append("operation-error-catalog.json publicCodes must be unique")
        operation_failures = catalog.get("operationFailures")
        if not isinstance(operation_failures, dict) or not operation_failures:
            errors.append("operation-error-catalog.json operationFailures must be a non-empty object")
        elif isinstance(public_codes, list):
            public_code_set = set(public_codes)
            for key, entry in operation_failures.items():
                if not isinstance(entry, dict):
                    errors.append(f"operation error {key} must be an object")
                    continue
                if entry.get("publicCode") not in public_code_set:
                    errors.append(f"operation error {key} publicCode is not registered")
                if not isinstance(entry.get("defaultPhase"), str) or not entry["defaultPhase"]:
                    errors.append(f"operation error {key} defaultPhase must be a non-empty string")
                if not isinstance(entry.get("recoverable"), bool):
                    errors.append(f"operation error {key} recoverable must be boolean")

    for path in [positive_fixtures, negative_fixtures]:
        if not path.is_dir():
            errors.append(f"missing fixture directory {path.relative_to(root).as_posix()}")

    def fixture_ids(path: Path) -> set[str]:
        if not path.is_file():
            errors.append(f"missing fixture bundle {path.relative_to(root).as_posix()}")
            return set()
        data = json.loads(path.read_text(encoding="utf-8"))
        if not isinstance(data, dict):
            errors.append(f"{path.relative_to(root).as_posix()} must be a Contract ID object")
            return set()
        return set(data.keys())

    protocol_parse = root / "src/protocol/parse.rs"
    if not protocol_parse.is_file():
        errors.append("missing src/protocol/parse.rs")
        parse_lines = []
    else:
        parse_lines = [
            line.strip()
            for line in protocol_parse.read_text(encoding="utf-8").splitlines()
            if line.strip() and not line.strip().startswith("//!")
        ]
    parser_implemented = bool(parse_lines)
    if parser_implemented and isinstance(ids, list):
        for name, present in [
            ("positive", fixture_ids(positive_fixture_bundle)),
            ("negative", fixture_ids(negative_fixture_bundle)),
        ]:
            missing = [contract_id for contract_id in ids if contract_id not in present]
            extra = [contract_id for contract_id in present if contract_id not in ids]
            if missing:
                errors.append(f"implemented parser requires {name} fixtures for: " + ", ".join(missing))
            if extra:
                errors.append(f"{name} fixture bundle has unknown Contract IDs: " + ", ".join(extra))

    return errors


def write_selftest_file(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


def write_valid_selftest_repo(root: Path) -> None:
    write_selftest_file(
        root / "contracts/native/contract-index.json",
        json.dumps(
            {
                "schemaVersion": 1,
                "protocolVersion": 1,
                "settingsSchemaVersion": 1,
                "cacheSchemaVersion": 1,
                "contractIds": REQUIRED_CONTRACT_IDS,
                "nativeMessagingLimits": REQUIRED_LIMITS,
            },
            indent=2,
        ),
    )
    write_selftest_file(
        root / "contracts/native/fixture-requirements.json",
        json.dumps(
            {
                "schemaVersion": 1,
                "requiredCases": REQUIRED_CASES,
                "coverageMarkers": REQUIRED_COVERAGE_MARKERS,
                "activationRule": "self-test",
            },
            indent=2,
        ),
    )
    write_selftest_file(
        root / "contracts/native/operation-error-catalog.json",
        json.dumps(
            {
                "schemaVersion": 1,
                "publicCodes": ["malformed-message"],
                "operationFailures": {
                    "malformed-message": {
                        "publicCode": "malformed-message",
                        "defaultPhase": "parse",
                        "recoverable": True,
                    }
                },
            },
            indent=2,
        ),
    )
    fixture_bundle = {contract_id: {} for contract_id in REQUIRED_CONTRACT_IDS}
    for kind in ["positive", "negative"]:
        write_selftest_file(
            root / f"contracts/native/fixtures/{kind}/contract-fixtures.json",
            json.dumps(fixture_bundle, indent=2),
        )
    write_selftest_file(root / "src/protocol/parse.rs", "pub(crate) fn parse() {}\n")

    tests_by_path: dict[str, list[str]] = {}
    for marker in REQUIRED_COVERAGE_MARKERS.values():
        relative, _, test_name = marker.partition("::")
        tests_by_path.setdefault(relative, []).append(test_name)
    for relative, test_names in tests_by_path.items():
        write_selftest_file(
            root / relative,
            "\n".join(f"fn {test_name}() {{}}" for test_name in sorted(test_names)) + "\n",
        )


def assert_contains(values: list[str], needle: str) -> None:
    if not any(needle in value for value in values):
        raise AssertionError(f"expected {needle!r} in {values!r}")


def run_self_tests() -> int:
    with tempfile.TemporaryDirectory(prefix="sniptale-contracts-") as temp_dir:
        root = Path(temp_dir)
        write_valid_selftest_repo(root)
        errors = contract_guardrail_errors(root)
        if errors:
            raise AssertionError(f"valid contract self-test repo failed: {errors!r}")

        requirements_path = root / "contracts/native/fixture-requirements.json"
        requirements = json.loads(requirements_path.read_text(encoding="utf-8"))
        requirements["requiredCases"] = REQUIRED_CASES[:-1]
        requirements_path.write_text(json.dumps(requirements, indent=2), encoding="utf-8")
        errors = contract_guardrail_errors(root)
        assert_contains(errors, "requiredCases")

        write_valid_selftest_repo(root)
        marker = REQUIRED_COVERAGE_MARKERS["unsupported-protocol-version"]
        relative, _, _test_name = marker.partition("::")
        marker_path = root / relative
        marker_path.write_text("", encoding="utf-8")
        errors = contract_guardrail_errors(root)
        assert_contains(errors, "coverage marker test is missing")

        write_valid_selftest_repo(root)
        positive_path = root / "contracts/native/fixtures/positive/contract-fixtures.json"
        fixtures = json.loads(positive_path.read_text(encoding="utf-8"))
        fixtures.pop("AppOperationFailedMessage")
        positive_path.write_text(json.dumps(fixtures, indent=2), encoding="utf-8")
        errors = contract_guardrail_errors(root)
        assert_contains(errors, "implemented parser requires positive fixtures")

    print("OK contract guardrail self-tests")
    return 0


def main() -> int:
    if sys.argv[1:] == ["--self-test"]:
        return run_self_tests()
    if sys.argv[1:]:
        fail(["usage: verify_contracts.py [--self-test]"])

    errors = contract_guardrail_errors(ROOT)
    if errors:
        fail(errors)
    print("OK contract guardrails")
    return 0


if __name__ == "__main__":
    sys.exit(main())
