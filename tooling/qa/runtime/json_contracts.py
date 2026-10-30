"""Reject duplicate JSON keys and validate every checked-in JSON contract."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import jsonschema

from .advisories import decode, unique_object
from .repository import ROOT
from .script_lints import inventory

REGISTRY = ROOT / "tooling/configs/qa/json-contracts.json"


def validate(value: Any, schema: dict[str, Any]) -> None:
    jsonschema.Draft202012Validator.check_schema(schema)
    jsonschema.Draft202012Validator(schema).validate(value)


def main() -> int:
    files = inventory()["json"]
    registry = decode(REGISTRY.read_bytes())
    contracts = registry["contracts"]
    if set(files) != set(contracts):
        raise ValueError("every JSON file requires one declared schema contract")
    errors = 0
    for name in files:
        try:
            source = (ROOT / name).read_bytes()
            if len(source) > 8 * 1024 * 1024:
                raise ValueError("JSON contract exceeds byte budget")
            value = json.loads(source, object_pairs_hook=unique_object)
            validate(value, contracts[name])
        except (ValueError, jsonschema.ValidationError, jsonschema.SchemaError) as exc:
            print("FAIL JSON contract " + name + ": " + type(exc).__name__)
            errors += 1
    print(f"JSON contracts: {len(files)} checked, {errors} failed")
    return int(errors > 0)


if __name__ == "__main__":
    raise SystemExit(main())
