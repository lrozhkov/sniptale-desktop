"""Reject duplicate JSON keys and validate every checked-in JSON contract."""

from __future__ import annotations

import argparse
import hashlib
import json
from typing import Any

import jsonschema
from jsonschema import validators

from .advisories import decode, unique_object
from .repository import ROOT
from .script_lints import inventory

REGISTRY = ROOT / "tooling/configs/qa/json-contracts.json"


def validate(value: Any, schema: dict[str, Any]) -> None:
    def references(node: Any) -> None:
        if isinstance(node, dict):
            for key, child in node.items():
                if key in {"$ref", "$dynamicRef"} and (not isinstance(child, str) or not child.startswith("#")):
                    raise ValueError("configuration schemas must not retrieve external references")
                references(child)
        elif isinstance(node, list):
            for child in node:
                references(child)

    references(schema)
    validator = validators.validator_for(schema)
    validator.check_schema(schema)
    validator(schema).validate(value)


def self_test() -> None:
    schema = {
        "type": "object",
        "required": ["enabled"],
        "additionalProperties": False,
        "properties": {"enabled": {"type": "boolean"}},
    }
    validate({"enabled": True}, schema)
    for value in ({"enabled": "yes"}, {"unknown": True}, {}):
        try:
            validate(value, schema)
        except jsonschema.ValidationError:
            continue
        raise ValueError("JSON schema accepted an invalid configuration")
    try:
        validate({}, {"$ref": "https://example.invalid/schema.json"})
    except ValueError:
        pass
    else:
        raise ValueError("JSON schemas must not retrieve external references")
    try:
        json.loads('{"nested":{"key":1,"key":2}}', object_pairs_hook=unique_object)
    except ValueError:
        print("OK JSON duplicate-key and schema rejection proof")
        return
    raise ValueError("JSON duplicate keys were accepted")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return 0
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
            contract = contracts[name]
            if contract == {"schemaTool": "renovate"} and name == "renovate.json":
                lock = decode((ROOT / "tooling/configs/qa/toolchain-lock.json").read_bytes())["scriptTools"]["renovate"]
                schema_path = (
                    ROOT
                    / ".tmp/qa-tools/script-tools"
                    / ("renovate-schema-" + lock["version"])
                    / "renovate-schema.json"
                )
                schema_bytes = schema_path.read_bytes()
                if hashlib.sha256(schema_bytes).hexdigest() != lock["schemaAsset"]["schemaSha256"]:
                    raise ValueError("pinned Renovate schema checksum mismatch")
                schema = decode(schema_bytes)
                if schema.get("x-renovate-version") != lock["version"]:
                    raise ValueError("pinned Renovate schema version mismatch")
                validate(value, schema)
            else:
                if contract.get("type") not in {"object", "array"}:
                    raise ValueError("configuration requires a non-vacuous root schema")
                validate(value, contract)
        except (ValueError, jsonschema.ValidationError, jsonschema.SchemaError) as exc:
            print("FAIL JSON contract " + name + ": " + type(exc).__name__)
            errors += 1
    print(f"JSON contracts: {len(files)} checked, {errors} failed")
    return int(errors > 0)


if __name__ == "__main__":
    raise SystemExit(main())
