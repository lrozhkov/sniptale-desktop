from __future__ import annotations

import json
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[3]
REGISTRY_PATH = ROOT / "tooling/configs/qa/exception-registry.json"


def registry() -> dict[str, Any]:
    value = json.loads(REGISTRY_PATH.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError("exception registry must be an object")
    return value


def exception(rule_id: str) -> dict[str, Any]:
    matches = [entry for entry in registry().get("exceptions", []) if entry.get("ruleId") == rule_id]
    if len(matches) != 1:
        raise ValueError(f"expected exactly one exception entry for {rule_id}")
    return matches[0]


def exact_paths(rule_id: str) -> set[str]:
    scope = exception(rule_id)["exactScope"]
    if "exactPaths" in scope:
        return set(scope["exactPaths"])
    catalog_path = ROOT / scope["catalog"]
    catalog = json.loads(catalog_path.read_text(encoding="utf-8"))
    values = catalog[scope["array"]]
    return {entry[scope["field"]] for entry in values}


def path_terms(rule_id: str) -> dict[str, set[str]]:
    values = exception(rule_id)["exactScope"]["pathTerms"]
    return {path: set(terms) for path, terms in values.items()}


def exact_edges(rule_id: str) -> dict[tuple[str, str, str], str]:
    entry = exception(rule_id)
    return {
        (edge["path"], edge["owner"], edge["imports"]): entry["rationale"] for edge in entry["exactScope"]["exactEdges"]
    }
