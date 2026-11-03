"""Validate update timing policy and complete extraction of the common catalog."""

from __future__ import annotations

import json
import re
from typing import Any

from .environment_audit import catalog_tools
from .repository import ROOT


def check(config: dict[str, Any], catalog: dict[str, Any], source: str) -> None:
    if (
        config.get("minimumReleaseAge") != "7 days"
        or config.get("minimumReleaseAgeBehaviour") != "timestamp-required"
        or config.get("internalChecksFilter") != "strict"
        or config.get("prCreation") != "not-pending"
        or config.get("automerge") is not False
        or config.get("autoReplaceGlobalMatch") is not False
    ):
        raise ValueError("Renovate must wait seven full days and must not merge or replace unrelated values")
    alerts = config.get("vulnerabilityAlerts", {})
    if alerts != {"enabled": True, "automerge": False, "minimumReleaseAge": "0 days", "addLabels": ["security-update"]}:
        raise ValueError("only confirmed vulnerability alerts may propose younger releases")

    def overrides(node: Any, path: tuple[str, ...] = ()) -> None:
        if isinstance(node, dict):
            for name, value in node.items():
                current = (*path, name)
                if name == "minimumReleaseAge" and current not in {
                    ("minimumReleaseAge",),
                    ("vulnerabilityAlerts", "minimumReleaseAge"),
                }:
                    raise ValueError("Renovate cannot bypass age through package rules or presets")
                if name in {"extends", "ignoreTests", "postUpgradeTasks", "allowScripts"}:
                    raise ValueError("Renovate update policy cannot import presets or execute repository code")
                overrides(value, current)
        elif isinstance(node, list):
            for index, value in enumerate(node):
                overrides(value, (*path, str(index)))

    overrides(config)
    if config.get("enabledManagers") != ["cargo", "custom.regex"]:
        raise ValueError("generated tool pins must have one update owner")
    extracted: dict[str, str] = {}
    for manager in config["customManagers"]:
        if manager.get("customType") != "regex" or manager.get("managerFilePatterns") != [
            "/^tooling/configs/qa/tool-versions\\.json$/"
        ]:
            raise ValueError("Renovate custom managers must target only the common catalog")
        for expression in manager["matchStrings"]:
            # These expressions use the RE2 subset: no lookaround, backreferences or executable replacements.
            if re.search(r"\(\?[=!]|\(\?<[=!]|\\[1-9]", expression):
                raise ValueError("Renovate catalog matching must use the RE2-compatible subset")
            pattern = re.sub(r"\(\?<([A-Za-z][A-Za-z0-9]*)>", r"(?P<\1>", expression)
            for match in re.finditer(pattern, source):
                groups = match.groupdict()
                name = groups.get("depName") or manager.get("depNameTemplate")
                if not isinstance(name, str) or name in extracted:
                    raise ValueError("Renovate catalog extraction is ambiguous")
                extracted[name] = groups["currentValue"]
    if extracted != catalog_tools(catalog):
        raise ValueError("Renovate must extract every exact catalog pin once")
    if not any(
        rule.get("matchPackageNames") == ["wix"] and rule.get("allowedVersions") == "<6"
        for rule in config["packageRules"]
    ):
        raise ValueError("WiX license transition requires a separate decision")


def main() -> int:
    source = (ROOT / "tooling/configs/qa/tool-versions.json").read_text()
    check(json.loads((ROOT / "renovate.json").read_text()), json.loads(source), source)
    print("OK Renovate seven-day policy, security alert separation and complete catalog extraction")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
