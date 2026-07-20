"""Strict declarative lab configuration."""

from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path

from errors import IntegrityError, UsageError

LANES = ("linux-wayland", "linux-x11", "windows-11")


@dataclass(frozen=True)
class Timeouts:
    boot: int
    test: int
    shutdown: int
    provision: int


@dataclass(frozen=True)
class Lane:
    name: str
    os: str
    display_server: str
    vcpus: int
    memory_mib: int
    disk_gib: int
    ssh_port: int
    vnc_display: int


@dataclass(frozen=True)
class Limits:
    min_free_gib: int
    min_available_memory_mib: int
    max_runs: int
    max_run_age_days: int
    max_log_mib: int
    max_screenshot_mib: int
    max_recording_mib: int
    max_cache_gib: int
    max_temporary_gib: int
    max_persistent_gib: int


@dataclass(frozen=True)
class LabConfig:
    lanes: dict[str, Lane]
    timeouts: Timeouts
    limits: Limits

    @classmethod
    def load(cls, root: Path) -> "LabConfig":
        path = root / "tooling/desktop/e2e_lab/lab-config.json"
        try:
            raw = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as exc:
            raise IntegrityError(f"invalid lab configuration: {exc}") from exc
        if set(raw.get("lanes", {})) != set(LANES):
            raise IntegrityError("lab configuration must define exactly the three supported lanes")
        lanes: dict[str, Lane] = {}
        ports: set[int] = set()
        displays: set[int] = set()
        for name in LANES:
            item = raw["lanes"][name]
            lane = Lane(name=name, **item)
            if lane.vcpus < 1 or lane.vcpus > 4:
                raise IntegrityError(f"{name}: vCPU limit must be 1..4")
            ceiling = 8192 if lane.os == "windows" else 6144
            if lane.memory_mib < 2048 or lane.memory_mib > ceiling:
                raise IntegrityError(f"{name}: memory exceeds lane policy")
            if lane.ssh_port in ports or lane.vnc_display in displays:
                raise IntegrityError("lane management endpoints must be unique")
            ports.add(lane.ssh_port)
            displays.add(lane.vnc_display)
            lanes[name] = lane
        return cls(
            lanes=lanes,
            timeouts=Timeouts(**raw["timeouts_seconds"]),
            limits=Limits(**raw["limits"]),
        )

    def expand_lane(self, requested: str) -> list[str]:
        if requested == "all":
            return list(LANES)
        if requested not in self.lanes:
            raise UsageError(f"unknown lane: {requested}")
        return [requested]
