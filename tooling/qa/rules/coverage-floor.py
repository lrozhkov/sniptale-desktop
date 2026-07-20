#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]
AGGREGATE_HIGH_RISK_LINE_FLOOR = 84
HIGH_RISK_OWNER_LINE_FLOORS = {
    "src/protocol": 88,
    "src/ipc": 84,
    "src/transfer": 96,
    "src/settings": 95,
    "src/assets": 95,
    "src/diagnostics": 93,
    "src/recording": 92,
    "src/capture": 91,
    "src/tray": 90,
}


def has_non_scaffold_code(path: Path) -> bool:
    if not path.exists():
        return False
    for rust_file in path.rglob("*.rs"):
        if "/tests/" in rust_file.as_posix():
            continue
        for raw in rust_file.read_text(encoding="utf-8").splitlines():
            stripped = raw.strip()
            if not stripped:
                continue
            if stripped.startswith("//") or stripped.startswith("mod "):
                continue
            return True
    return False


def aggregate_line_floor() -> int:
    if any(has_non_scaffold_code(ROOT / owner) for owner in HIGH_RISK_OWNER_LINE_FLOORS):
        return AGGREGATE_HIGH_RISK_LINE_FLOOR
    return 0


def owner_line_totals(lcov_path: Path) -> dict[str, tuple[int, int]]:
    totals = {owner: (0, 0) for owner in HIGH_RISK_OWNER_LINE_FLOORS}
    current_source: str | None = None
    for raw in lcov_path.read_text(encoding="utf-8").splitlines():
        if raw.startswith("SF:"):
            current_source = raw.removeprefix("SF:").replace("\\", "/")
            continue
        if current_source is None or not raw.startswith("DA:"):
            continue
        for owner in HIGH_RISK_OWNER_LINE_FLOORS:
            if f"/{owner}/" not in current_source and not current_source.endswith(f"/{owner}.rs"):
                continue
            covered, total = totals[owner]
            count = int(raw.split(":", 1)[1].split(",", 2)[1])
            totals[owner] = (covered + int(count > 0), total + 1)
            break
    return totals


def check_owner_line_floors(lcov_path: Path) -> list[str]:
    if not lcov_path.is_file():
        return [f"missing coverage lcov artifact: {lcov_path}"]
    errors: list[str] = []
    for owner, floor in HIGH_RISK_OWNER_LINE_FLOORS.items():
        if not has_non_scaffold_code(ROOT / owner):
            continue
        covered, total = owner_line_totals(lcov_path)[owner]
        if total == 0:
            errors.append(f"{owner} has no coverage records")
            continue
        percent = covered * 100 / total
        if percent < floor:
            errors.append(f"{owner} line coverage {percent:.2f}% is below floor {floor}%")
    return errors


def owner_floors_json() -> str:
    payload = {
        "aggregateLineFloor": aggregate_line_floor(),
        "ownerLineFloors": HIGH_RISK_OWNER_LINE_FLOORS,
    }
    return json.dumps(payload, indent=2, sort_keys=True)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check-lcov", type=Path)
    parser.add_argument("--owner-floors-json", action="store_true")
    args = parser.parse_args()

    if args.owner_floors_json:
        print(owner_floors_json())
        return 0
    if args.check_lcov is not None:
        errors = check_owner_line_floors(args.check_lcov)
        if errors:
            print("FAIL coverage owner floors")
            for error in errors:
                print(f"- {error}")
            return 1
        print("OK coverage owner floors")
        return 0

    print(aggregate_line_floor())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
