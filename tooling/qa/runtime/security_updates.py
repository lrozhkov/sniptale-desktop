"""Verify that each release-age exception actually closes its named advisory."""

from __future__ import annotations

import re
import subprocess
from datetime import datetime, timedelta, UTC
from typing import Any

from .advisories import AdvisoryError, Fetch, decode, request_json
from .environment_audit import catalog_tools
from .repository import ROOT


def version_key(value: str) -> tuple[int, int, int, int, int]:
    match = re.fullmatch(r"v?(\d+)(?:\.(\d+))?(?:\.(\d+))?(?:(a|b|rc)(\d+))?", value)
    if match is None:
        raise AdvisoryError("unsupported advisory version syntax")
    major, minor, patch, prerelease, serial = match.groups()
    return (
        int(major),
        int(minor or 0),
        int(patch or 0),
        {None: 0, "a": -3, "b": -2, "rc": -1}[prerelease],
        int(serial or 0),
    )


def osv_affected(record: dict[str, Any], package: str, ecosystem: str, version: str) -> bool:
    selected = version_key(version)
    entries = record.get("affected")
    if record.get("withdrawn") or not isinstance(entries, list) or not entries:
        raise AdvisoryError("security exception requires an active affected-version record")
    matched = False
    affected = False
    for entry in entries:
        if (
            not isinstance(entry, dict)
            or entry.get("package", {}).get("name") != package
            or entry["package"].get("ecosystem") != ecosystem
        ):
            continue
        matched = True
        ranges = entry.get("ranges", [])
        if not isinstance(ranges, list):
            raise AdvisoryError("invalid advisory ranges")
        for span in ranges:
            if span.get("type") not in {"SEMVER", "ECOSYSTEM"} or not isinstance(span.get("events"), list):
                raise AdvisoryError("unsupported security-exception range")
            active = False
            for event in span["events"]:
                if not isinstance(event, dict) or len(event) != 1:
                    raise AdvisoryError("invalid advisory range event")
                boundary, value = next(iter(event.items()))
                key = version_key(value)
                if boundary == "introduced":
                    active = selected >= key
                elif boundary in {"fixed", "limit"}:
                    if active and selected < key:
                        affected = True
                    active = False
                elif boundary == "last_affected":
                    if active and selected <= key:
                        affected = True
                    active = False
                else:
                    raise AdvisoryError("unknown advisory range event")
            affected |= active
        versions = entry.get("versions", [])
        if not isinstance(versions, list):
            raise AdvisoryError("invalid explicit advisory versions")
        affected |= version in versions
    if not matched:
        raise AdvisoryError("security exception advisory names another package")
    return affected


def cpython_affected(record: dict[str, Any], advisory_id: str, version: str) -> bool:
    metadata = record.get("cveMetadata", {})
    if metadata.get("cveId") != advisory_id or metadata.get("state") != "PUBLISHED":
        raise AdvisoryError("security exception CVE identity or publication state changed")
    entries = record.get("containers", {}).get("cna", {}).get("affected")
    if not isinstance(entries, list) or not entries:
        raise AdvisoryError("CVE affected-version evidence missing")
    matched = False
    affected = False
    selected = version_key(version)
    for entry in entries:
        if entry.get("repo") != "https://github.com/python/cpython" or entry.get("product") != "CPython":
            continue
        matched = True
        if entry.get("defaultStatus", "unaffected") != "unaffected":
            raise AdvisoryError("unsupported default CVE version status")
        for span in entry.get("versions", []):
            if span.get("versionType") not in {"python", "semver"} or span.get("changes"):
                raise AdvisoryError("unsupported CVE version range")
            lower = version_key(span["version"])
            if "lessThan" in span:
                included = lower <= selected < version_key(span["lessThan"])
            elif "lessThanOrEqual" in span:
                included = lower <= selected <= version_key(span["lessThanOrEqual"])
            else:
                included = selected == lower
            if span.get("status") not in {"affected", "unaffected"}:
                raise AdvisoryError("unknown CVE version status")
            affected |= included and span["status"] == "affected"
    if not matched:
        raise AdvisoryError("CVE does not describe CPython")
    return affected


def required_dependency(
    catalog: dict[str, Any],
    policy: dict[str, Any],
    proofs: dict[str, Any],
    name: str,
    verified: list[str],
    fetch: Fetch,
) -> None:
    proof = proofs["exceptions"][name]
    if set(proof) != {"advisoryId", "affectedVersion", "requiredBy"}:
        raise AdvisoryError("invalid required security dependency proof")
    parent = proof["requiredBy"]
    if parent not in verified or "requiredBy" in proofs["exceptions"][parent]:
        raise AdvisoryError("security dependency requires an independently verified fixed parent")
    parent_proof = proofs["exceptions"][parent]
    exceptions = catalog["securityExceptions"]
    if (
        proof["advisoryId"] != parent_proof["advisoryId"]
        or proof["affectedVersion"] != parent_proof["affectedVersion"]
        or exceptions[name]["advisory"] != exceptions[parent]["advisory"]
    ):
        raise AdvisoryError("security dependency must bind the same confirmed parent advisory")
    child_policy, parent_policy = policy["tools"][name], policy["tools"][parent]
    if any(item.get("provider") != "osv" or item.get("ecosystem") != "npm" for item in (child_policy, parent_policy)):
        raise AdvisoryError("required dependency proof supports npm parents only")
    current = catalog_tools(catalog)[name]
    parent_version = catalog_tools(catalog)[parent]
    metadata = fetch("https://registry.npmjs.org/" + parent_policy["package"] + "/" + parent_version, None)
    if (
        metadata.get("name") != parent_policy["package"]
        or metadata.get("version") != parent_version
        or metadata.get("dependencies", {}).get(child_policy["package"]) != "^" + current
        or version_key(current)[0] == 0
    ):
        raise AdvisoryError("fresh dependency must be the published minimum required by the verified fix")
    releases = fetch("https://registry.npmjs.org/" + child_policy["package"], None)
    if releases.get("name") != child_policy["package"] or current not in releases.get("versions", {}):
        raise AdvisoryError("required dependency publisher identity mismatch")
    now = datetime.now(UTC)

    def aged(version: str) -> bool:
        publication = datetime.fromisoformat(releases["time"][version].replace("Z", "+00:00"))
        if publication.tzinfo is None or publication > now:
            raise AdvisoryError("invalid required dependency publication timestamp")
        return now - publication >= timedelta(days=7)

    if not aged(current):
        for version in releases["versions"]:
            if (
                re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", version)
                and version_key(current) <= version_key(version)
                and version_key(version)[0] == version_key(current)[0]
                and aged(version)
            ):
                raise AdvisoryError("an aged compatible dependency exists; a fresh exception is unnecessary")


def verify(
    catalog: dict[str, Any],
    policy: dict[str, Any],
    proofs: dict[str, Any],
    fetch: Fetch = request_json,
    *,
    baseline: dict[str, str] | None = None,
) -> list[str]:
    exceptions = catalog.get("securityExceptions", {})
    if (
        set(proofs) != {"schemaVersion", "exceptions"}
        or proofs["schemaVersion"] != 1
        or set(proofs["exceptions"]) != set(exceptions)
    ):
        raise AdvisoryError("each age exception requires exactly one advisory-bound proof")
    selected = catalog_tools(catalog)
    verified: list[str] = []
    for name in sorted(exceptions, key=lambda item: "requiredBy" in proofs["exceptions"][item]):
        exception = exceptions[name]
        proof = proofs["exceptions"][name]
        current = selected.get(name)
        if current is None or exception.get("fixedVersion") != current or not exception.get("reason", "").strip():
            raise AdvisoryError("security exception does not bind the selected tool version")
        if "requiredBy" in proof:
            required_dependency(catalog, policy, proofs, name, verified, fetch)
            verified.append(name)
            continue
        if set(proof) != {"advisoryId", "affectedVersion"}:
            raise AdvisoryError("invalid security-exception proof fields")
        if (
            baseline is not None
            and name in baseline
            and baseline[name] != current
            and proof["affectedVersion"] != baseline[name]
        ):
            raise AdvisoryError("security exception affected version differs from the actual update baseline")
        advisory_id = proof["advisoryId"]
        coverage = policy["tools"][name]
        if coverage["provider"] == "cpython" and re.fullmatch(r"CVE-\d{4}-\d{4,}", advisory_id):
            if exception["advisory"] != "https://www.cve.org/CVERecord?id=" + advisory_id:
                raise AdvisoryError("exception URL and advisory ID disagree")
            record = fetch("https://cveawg.mitre.org/api/cve/" + advisory_id, None)
            was_affected = cpython_affected(record, advisory_id, proof["affectedVersion"])
            remains_affected = cpython_affected(record, advisory_id, current)
        elif coverage["provider"] == "osv" and re.fullmatch(r"RUSTSEC-\d{4}-\d{4}|GHSA-[a-z0-9-]+", advisory_id):
            if advisory_id not in exception["advisory"]:
                raise AdvisoryError("exception URL and advisory ID disagree")
            record = fetch("https://api.osv.dev/v1/vulns/" + advisory_id, None)
            if record.get("id") != advisory_id:
                raise AdvisoryError("exception advisory source identity changed")
            was_affected = osv_affected(record, coverage["package"], coverage["ecosystem"], proof["affectedVersion"])
            remains_affected = osv_affected(record, coverage["package"], coverage["ecosystem"], current)
        else:
            raise AdvisoryError("security exception lacks supported vendor affected-version proof")
        if not was_affected or remains_affected or version_key(current) <= version_key(proof["affectedVersion"]):
            raise AdvisoryError("selected security update does not prove the named vulnerability is fixed")
        verified.append(name)
    return verified


def main() -> int:
    try:
        paths = ["tool-versions.json", "environment-audit.json", "security-update-proofs.json"]
        values = [decode((ROOT / "tooling/configs/qa" / name).read_bytes()) for name in paths]
        from .security_update_baseline import versions as baseline_versions

        verified = verify(values[0], values[1], values[2], baseline=baseline_versions())
        print("OK advisory-bound security updates: " + ", ".join(verified))
        return 0
    except (AdvisoryError, OSError, KeyError, TypeError, ValueError, subprocess.CalledProcessError) as exc:
        print("FAIL security update policy: " + str(exc))
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
