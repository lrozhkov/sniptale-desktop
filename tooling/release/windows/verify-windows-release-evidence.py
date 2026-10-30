#!/usr/bin/env python3
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import json
import math
from pathlib import Path


REQUIRED_PATHS = [
    ("releaseCandidate", "nativeVersion"),
    ("releaseCandidate", "extensionVersion"),
    ("releaseCandidate", "channel"),
    ("releaseCandidate", "windowsBuild"),
    ("releaseCandidate", "browserBuilds", "chrome"),
    ("releaseCandidate", "browserBuilds", "edge"),
    ("releaseCandidate", "installerEvidence", "msiFileName"),
    ("releaseCandidate", "installerEvidence", "releaseEvidenceFileName"),
    ("releaseCandidate", "installerEvidence", "sha256SumsFileName"),
    ("releaseCandidate", "installerEvidence", "sbomFileName"),
    ("releaseCandidate", "installerEvidence", "provenanceFileName"),
    ("performanceBaseline", "coldStartMs"),
    ("performanceBaseline", "screenshotCaptureMs"),
    ("performanceBaseline", "recordingFiveMinute", "droppedFrames"),
    ("performanceBaseline", "recordingFiveMinute", "dropRatePct"),
    ("performanceBaseline", "recordingFiveMinute", "queuePressureMax"),
    ("performanceBaseline", "recordingFiveMinute", "encodeFinalizeMs"),
    ("performanceBaseline", "recordingFiveMinute", "tempMediaBytes"),
    ("performanceBaseline", "reconnectDuringRecording", "stopFinalizeMs"),
    ("accessibilityBaseline", "accessibilityInsightsRunId"),
    ("evidenceFiles", "wprTraceFileName"),
    ("evidenceFiles", "accessibilityReportFileName"),
    ("evidenceFiles", "manualQaNotesFileName"),
    ("review", "tester"),
    ("review", "completedAtUtc"),
]

REQUIRED_TRUE_FLAGS = [
    ("performanceBaseline", "recordingFiveMinute", "cleanupVerified"),
    ("performanceBaseline", "reconnectDuringRecording", "completed"),
    ("performanceBaseline", "reconnectDuringRecording", "finalizedMediaRecovered"),
    ("accessibilityBaseline", "trayKeyboardOnly"),
    ("accessibilityBaseline", "trayUiaNamesAndRoles"),
    ("accessibilityBaseline", "overlayKeyboardCancel"),
    ("accessibilityBaseline", "overlayFocusReturn"),
    ("accessibilityBaseline", "overlayHighContrastUsable"),
    ("windowsHardeningReview", "wdacAppControlGuidanceReviewed"),
    ("windowsHardeningReview", "appLockerSecondaryGuidanceReviewed"),
    ("windowsHardeningReview", "cfgProcessMitigationGuidanceReviewed"),
    ("windowsHardeningReview", "bitLockerDefenseInDepthAcknowledged"),
    ("windowsHardeningReview", "noProductRuntimePolicyAdded"),
    ("review", "approvedForProductionCandidate"),
]

STRING_PATHS = [
    ("releaseCandidate", "nativeVersion"),
    ("releaseCandidate", "extensionVersion"),
    ("releaseCandidate", "windowsBuild"),
    ("releaseCandidate", "browserBuilds", "chrome"),
    ("releaseCandidate", "browserBuilds", "edge"),
    ("accessibilityBaseline", "accessibilityInsightsRunId"),
    ("review", "tester"),
]

FILE_NAME_PATHS = [
    ("releaseCandidate", "installerEvidence", "msiFileName"),
    ("releaseCandidate", "installerEvidence", "releaseEvidenceFileName"),
    ("releaseCandidate", "installerEvidence", "sha256SumsFileName"),
    ("releaseCandidate", "installerEvidence", "sbomFileName"),
    ("releaseCandidate", "installerEvidence", "provenanceFileName"),
    ("evidenceFiles", "wprTraceFileName"),
    ("evidenceFiles", "accessibilityReportFileName"),
    ("evidenceFiles", "manualQaNotesFileName"),
]

NON_NEGATIVE_NUMBER_PATHS = [
    ("performanceBaseline", "coldStartMs"),
    ("performanceBaseline", "screenshotCaptureMs"),
    ("performanceBaseline", "recordingFiveMinute", "dropRatePct"),
    ("performanceBaseline", "recordingFiveMinute", "queuePressureMax"),
    ("performanceBaseline", "recordingFiveMinute", "encodeFinalizeMs"),
    ("performanceBaseline", "recordingFiveMinute", "tempMediaBytes"),
    ("performanceBaseline", "reconnectDuringRecording", "stopFinalizeMs"),
]

NON_NEGATIVE_INTEGER_PATHS = [
    ("performanceBaseline", "recordingFiveMinute", "droppedFrames"),
    ("performanceBaseline", "recordingFiveMinute", "durationMs"),
]

PERCENT_PATHS = [
    ("performanceBaseline", "recordingFiveMinute", "dropRatePct"),
    ("performanceBaseline", "recordingFiveMinute", "queuePressureMax"),
]

EXPECTED_DURATION_MS = 300_000
VALID_CHANNELS = {"stable", "beta", "dev"}


def get_path(payload: dict, path: tuple[str, ...]):
    value = payload
    for part in path:
        if not isinstance(value, dict) or part not in value:
            raise KeyError(".".join(path))
        value = value[part]
    return value


def dotted(path: tuple[str, ...]) -> str:
    return ".".join(path)


def is_number(value) -> bool:
    return isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value)


def is_integer(value) -> bool:
    return isinstance(value, int) and not isinstance(value, bool)


def validate_leaf_file_name(errors: list[str], path: tuple[str, ...], value) -> None:
    if not isinstance(value, str) or not value.strip():
        errors.append("empty " + dotted(path))
        return
    if value in {".", ".."} or "/" in value or "\\" in value:
        errors.append("not a leaf file name " + dotted(path))


def validate_completed_at_utc(errors: list[str], value) -> None:
    if not isinstance(value, str) or not value:
        errors.append("empty review.completedAtUtc")
        return
    normalized = value.replace("Z", "+00:00")
    try:
        parsed = datetime.fromisoformat(normalized)
    except ValueError:
        errors.append("review.completedAtUtc must be ISO-8601 UTC")
        return
    if parsed.tzinfo is None or parsed.utcoffset() != timezone.utc.utcoffset(parsed):
        errors.append("review.completedAtUtc must be UTC")


def validate(
    payload: dict,
    *,
    allow_template: bool,
    expected_channel: str | None = None,
    expected_msi_file: str | None = None,
    expected_release_evidence_file: str | None = None,
    expected_sha256_file: str | None = None,
    expected_sbom_file: str | None = None,
    expected_provenance_file: str | None = None,
) -> list[str]:
    errors: list[str] = []
    if not isinstance(payload, dict):
        return ["evidence root must be an object"]
    if not is_integer(payload.get("formatVersion")) or payload.get("formatVersion") != 1:
        errors.append("formatVersion must be 1")
    for path in REQUIRED_PATHS:
        try:
            value = get_path(payload, path)
        except KeyError:
            errors.append("missing " + ".".join(path))
            continue
        if allow_template:
            continue
        if value in ("", None):
            errors.append("empty " + dotted(path))
    for path in REQUIRED_TRUE_FLAGS:
        try:
            value = get_path(payload, path)
        except KeyError:
            errors.append("missing " + dotted(path))
            continue
        if not allow_template and value is not True:
            errors.append("not approved " + dotted(path))
    if allow_template:
        try:
            duration = get_path(payload, ("performanceBaseline", "recordingFiveMinute", "durationMs"))
            if duration != EXPECTED_DURATION_MS:
                errors.append("performanceBaseline.recordingFiveMinute.durationMs must be 300000")
        except KeyError:
            errors.append("missing performanceBaseline.recordingFiveMinute.durationMs")
        return errors

    for path in STRING_PATHS:
        try:
            value = get_path(payload, path)
        except KeyError:
            continue
        if not isinstance(value, str) or not value.strip():
            errors.append("empty " + dotted(path))

    for path in FILE_NAME_PATHS:
        try:
            validate_leaf_file_name(errors, path, get_path(payload, path))
        except KeyError:
            continue

    try:
        channel = get_path(payload, ("releaseCandidate", "channel"))
        if channel not in VALID_CHANNELS:
            errors.append("releaseCandidate.channel must be stable, beta, or dev")
        if expected_channel is not None and channel != expected_channel:
            errors.append(f"releaseCandidate.channel must be {expected_channel}")
    except KeyError:
        pass

    for path in NON_NEGATIVE_NUMBER_PATHS:
        try:
            value = get_path(payload, path)
        except KeyError:
            continue
        if not is_number(value) or value < 0:
            errors.append(dotted(path) + " must be a non-negative number")

    for path in NON_NEGATIVE_INTEGER_PATHS:
        try:
            value = get_path(payload, path)
        except KeyError:
            continue
        if not is_integer(value) or value < 0:
            errors.append(dotted(path) + " must be a non-negative integer")

    for path in PERCENT_PATHS:
        try:
            value = get_path(payload, path)
        except KeyError:
            continue
        if is_number(value) and not 0 <= value <= 100:
            errors.append(dotted(path) + " must be between 0 and 100")

    try:
        if get_path(payload, ("performanceBaseline", "recordingFiveMinute", "durationMs")) != EXPECTED_DURATION_MS:
            errors.append("performanceBaseline.recordingFiveMinute.durationMs must be 300000")
    except KeyError:
        pass

    try:
        validate_completed_at_utc(errors, get_path(payload, ("review", "completedAtUtc")))
    except KeyError:
        pass

    expected_files = [
        (expected_msi_file, ("releaseCandidate", "installerEvidence", "msiFileName")),
        (
            expected_release_evidence_file,
            ("releaseCandidate", "installerEvidence", "releaseEvidenceFileName"),
        ),
        (expected_sha256_file, ("releaseCandidate", "installerEvidence", "sha256SumsFileName")),
        (expected_sbom_file, ("releaseCandidate", "installerEvidence", "sbomFileName")),
        (expected_provenance_file, ("releaseCandidate", "installerEvidence", "provenanceFileName")),
    ]
    for expected, path in expected_files:
        if expected is None:
            continue
        try:
            actual = get_path(payload, path)
        except KeyError:
            continue
        if actual != expected:
            errors.append(f"{dotted(path)} must be {expected}")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description="Validate Windows release perf/accessibility evidence")
    parser.add_argument("path", help="Evidence JSON path")
    parser.add_argument(
        "--allow-template",
        action="store_true",
        help="Validate schema shape without requiring filled values",
    )
    parser.add_argument("--expected-channel", choices=sorted(VALID_CHANNELS))
    parser.add_argument("--expected-msi-file")
    parser.add_argument("--expected-release-evidence-file")
    parser.add_argument("--expected-sha256-file")
    parser.add_argument("--expected-sbom-file")
    parser.add_argument("--expected-provenance-file")
    args = parser.parse_args()
    path = Path(args.path)

    def reject_invalid_constant(value: str):
        raise ValueError(f"invalid JSON constant: {value}")

    try:
        payload = json.loads(path.read_text(encoding="utf-8"), parse_constant=reject_invalid_constant)
    except ValueError as error:
        print("FAIL Windows release evidence")
        print(f"- invalid JSON: {error}")
        return 1
    errors = validate(
        payload,
        allow_template=args.allow_template,
        expected_channel=args.expected_channel,
        expected_msi_file=args.expected_msi_file,
        expected_release_evidence_file=args.expected_release_evidence_file,
        expected_sha256_file=args.expected_sha256_file,
        expected_sbom_file=args.expected_sbom_file,
        expected_provenance_file=args.expected_provenance_file,
    )
    if errors:
        print("FAIL Windows release evidence")
        for error in errors:
            print(f"- {error}")
        return 1
    print("OK Windows release evidence")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
