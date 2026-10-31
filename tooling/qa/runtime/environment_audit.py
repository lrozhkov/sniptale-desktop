"""Audit cataloged build tools without compiling the application or starting a VM."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime, UTC
from pathlib import Path
from typing import Any

from .advisories import AdvisoryError, Fetch, cpython_query, decode, nvd_query, osv_queries, request_json
from .repository import ROOT

POLICY = ROOT / "tooling/configs/qa/environment-audit.json"
CATALOG = ROOT / "tooling/configs/qa/tool-versions.json"


def catalog_tools(value: dict[str, Any]) -> dict[str, str]:
    tools = {
        "python": value["python"]["version"],
        "rust": value["rust"]["channel"],
        "node": value["browserAutomation"]["node"],
        "playwright-core": value["browserAutomation"]["playwrightCore"],
        "wix": value["windows"]["wix"],
        "windows-sdk": value["windows"]["sdkPackage"],
    }
    inventories = [
        value["cargoSubcommands"],
        {name: item["version"] for name, item in value["externalTools"].items()},
        value["githubActions"],
        {name: item["version"] for name, item in value.get("scriptTools", {}).items()},
    ]
    for inventory in inventories:
        if set(inventory) & set(tools):
            raise AdvisoryError("catalog tool names collide across inventories")
        tools.update(inventory)
    if any(
        not isinstance(version, str) or re.fullmatch(r"v?[0-9]+(?:\.[0-9]+){2,3}", version) is None
        for version in tools.values()
    ):
        raise AdvisoryError("tool audit requires exact stable catalog versions")
    return tools


def audit_tool(name: str, version: str, policy: dict[str, Any], fetch: Fetch) -> dict[str, Any]:
    provider = policy.get("provider")
    result: dict[str, Any] = {"tool": name, "version": version, "provider": provider}
    if provider == "unavailable":
        if (
            set(policy) != {"provider", "reason", "reviewSource"}
            or not isinstance(policy["reason"], str)
            or not policy["reason"].strip()
        ):
            raise AdvisoryError("uncovered tool requires an explicit coverage reason")
        return {**result, "coverage": "manual-review-required", "reason": policy["reason"], "advisories": []}
    if provider == "cpython":
        if set(policy) != {"provider"}:
            raise AdvisoryError("invalid CPython advisory policy")
        findings = cpython_query(version, fetch)
    elif provider == "osv":
        if set(policy) != {"provider", "ecosystem", "package"}:
            raise AdvisoryError("invalid OSV tool policy")
        selected = "v" + version.lstrip("v") if policy["ecosystem"] == "Go" else version.lstrip("v")
        query = {
            "package": {"ecosystem": policy["ecosystem"], "name": policy["package"]},
            "version": selected,
        }
        findings = osv_queries([query], fetch)[0]
    elif provider == "nvd":
        if set(policy) != {"provider", "vendor", "product"}:
            raise AdvisoryError("invalid NVD tool policy")
        findings = nvd_query(policy["vendor"], policy["product"], version, fetch)
    else:
        raise AdvisoryError("tool has no declared advisory coverage")
    return {**result, "coverage": "direct-version-query", "advisories": findings}


def audit(catalog: dict[str, Any], policy: dict[str, Any], fetch: Fetch = request_json) -> dict[str, Any]:
    tools = catalog_tools(catalog)
    if policy.get("schemaVersion") != 1 or set(policy) != {"schemaVersion", "coverageLimit", "tools", "platformGaps"}:
        raise AdvisoryError("invalid environment audit policy")
    if set(tools) != set(policy["tools"]):
        raise AdvisoryError("every cataloged tool requires exactly one coverage declaration")
    # NVD's unauthenticated service permits five requests per thirty seconds.
    nvd = [(name, version) for name, version in tools.items() if policy["tools"][name].get("provider") == "nvd"]
    other = [(name, version) for name, version in tools.items() if policy["tools"][name].get("provider") != "nvd"]

    def check(item: tuple[str, str]) -> dict[str, Any]:
        name, version = item
        try:
            return audit_tool(name, version, policy["tools"][name], fetch)
        except AdvisoryError as exc:
            raise AdvisoryError(f"{name}: {exc}") from exc

    with ThreadPoolExecutor(max_workers=4) as pool:
        pending = [pool.submit(check, item) for item in other]
        results = [check(item) for item in nvd]
        results.extend(task.result() for task in pending)
    results.sort(key=lambda item: item["tool"])
    return {
        "schemaVersion": 1,
        "checkedAt": datetime.now(UTC).isoformat(),
        "result": "fail" if any(item["advisories"] for item in results) else "pass",
        "coverageLimit": policy["coverageLimit"],
        "tools": results,
        "platformGaps": policy["platformGaps"],
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / ".tmp/qa/environment-audit.json")
    args = parser.parse_args()
    try:
        catalog_bytes = CATALOG.read_bytes()
        report = audit(decode(catalog_bytes), decode(POLICY.read_bytes()))
        from . import tool_dependencies

        report["dependencies"] = tool_dependencies.audit()
        if any(item["advisories"] for item in report["dependencies"]):
            report["result"] = "fail"
        report["catalogSha256"] = hashlib.sha256(catalog_bytes).hexdigest()
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2) + "\n")
        for item in report["tools"]:
            state = (
                "UNCOVERED"
                if item["coverage"] == "manual-review-required"
                else "VULNERABLE"
                if item["advisories"]
                else "CHECKED"
            )
            print(f"{state} {item['tool']} {item['version']} " + ",".join(item["advisories"]))
        for dependency in report["dependencies"]:
            if dependency["advisories"]:
                print(
                    "VULNERABLE dependency "
                    + dependency["package"]
                    + " "
                    + dependency["version"]
                    + " "
                    + ",".join(dependency["advisories"])
                )
        print(f"CHECKED {len(report['dependencies'])} locked checking dependencies")
        for gap in report["platformGaps"]:
            print("PLATFORM GAP " + gap)
        return 0 if report["result"] == "pass" else 1
    except (AdvisoryError, OSError, ValueError, KeyError, TypeError) as exc:
        print("FAIL environment audit: " + str(exc), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
