"""Reject duplicate JSON keys and validate every checked-in JSON contract."""

from __future__ import annotations

import argparse
import copy
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
    history_record_shapes_self_test()
    keyboard_hold_schema_self_test()
    desktop_capability_schema_self_test()
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


def desktop_capability_schema_self_test() -> None:
    contracts = decode(REGISTRY.read_bytes())["contracts"]
    for name in ("capture-panel", "application-settings"):
        path = f"capabilities/{name}.json"
        value = decode((ROOT / path).read_bytes())
        validate(value, contracts[path])
        candidates = []
        for key, replacement in (
            ("local", False),
            ("windows", ["*"]),
            ("remote", {"urls": ["https://*"]}),
            ("permissions", ["core:default"]),
            (
                "permissions",
                ["allow-desktop-preferences-save"] if name == "capture-panel" else ["allow-desktop-capture-command"],
            ),
        ):
            mutated = copy.deepcopy(value)
            mutated[key] = replacement
            candidates.append(mutated)
        for mutated in candidates:
            try:
                validate(mutated, contracts[path])
            except jsonschema.ValidationError:
                continue
            raise ValueError("desktop capability schema accepted expanded authority")
    path = "tauri.conf.json"
    value = decode((ROOT / path).read_bytes())
    validate(value, contracts[path])
    for section, key, replacement in (
        ("build", "frontendDist", "https://example.invalid"),
        ("app", "withGlobalTauri", True),
        ("bundle", "active", True),
    ):
        mutated = copy.deepcopy(value)
        mutated[section][key] = replacement
        try:
            validate(mutated, contracts[path])
        except jsonschema.ValidationError:
            continue
        raise ValueError("desktop configuration schema accepted a forbidden integration mode")


def history_record_shapes_self_test() -> None:
    contracts = decode(REGISTRY.read_bytes())["contracts"]
    for family in ("positive", "negative"):
        name = f"contracts/native/fixtures/{family}/contract-fixtures.json"
        fixtures = decode((ROOT / name).read_bytes())
        for contract in ("CursorTrajectoryBlock", "RecordingEventsBlock"):
            schema = contracts[name]["properties"][contract]
            block = fixtures[contract]
            validate(block, schema)
            for index in range(len(block["records"])):
                mutated = copy.deepcopy(block)
                mutated["records"][index]["unexpectedPrivateField"] = "forbidden"
                mutations = [mutated]
                record = block["records"][index]
                if record["kind"] == "action-update" and "gestureType" in record["data"]:
                    for field in ("ctrlKey", "altKey", "shiftKey", "metaKey"):
                        mutated = copy.deepcopy(block)
                        del mutated["records"][index]["data"][field]
                        mutations.append(mutated)
                    mutated = copy.deepcopy(block)
                    mutated["records"][index]["data"]["clickCount"] = 2
                    mutations.append(mutated)
                for mutated in mutations:
                    try:
                        validate(mutated, schema)
                    except jsonschema.ValidationError:
                        continue
                    raise ValueError(f"history record schema accepted invalid fields: {contract}/{index}")


def keyboard_hold_schema_self_test() -> None:
    contracts = decode(REGISTRY.read_bytes())["contracts"]
    schema = contracts["contracts/native/fixtures/positive/contract-fixtures.json"]["properties"][
        "RecordingEventsBlock"
    ]
    fixture = decode((ROOT / "contracts/native/fixtures/recording-events-keyboard-hold-v1.json").read_bytes())
    accepted = [decode(case["utf8"].encode()) for case in fixture["cases"] if case["accepted"]]
    for block in accepted:
        validate(block, schema)
    block = accepted[0]
    for field in block["records"][1]["signal"]["data"]:
        mutated = copy.deepcopy(block)
        del mutated["records"][1]["signal"]["data"][field]
        try:
            validate(mutated, schema)
        except jsonschema.ValidationError:
            continue
        raise ValueError(f"keyboard-hold schema accepted a missing field: {field}")
    for field in ("label", "key", "groupId", "targetId"):
        mutated = copy.deepcopy(block)
        mutated["records"][1]["signal"]["data"][field] = "forbidden"
        try:
            validate(mutated, schema)
        except jsonschema.ValidationError:
            continue
        raise ValueError(f"keyboard-hold schema accepted a forbidden field: {field}")


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
