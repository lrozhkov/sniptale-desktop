"""Repository and disposable lab path ownership."""

from __future__ import annotations

import os
from dataclasses import dataclass
from pathlib import Path

from errors import IntegrityError


@dataclass(frozen=True)
class LabPaths:
    root: Path
    lab: Path
    artifacts: Path
    cache: Path
    images: Path
    golden: Path
    runtime: Path
    runs: Path
    locks: Path

    @classmethod
    def discover(cls, root: Path | None = None) -> "LabPaths":
        repo = (root or Path(__file__).resolve().parents[3]).resolve()
        lab = repo / ".tmp/qa/desktop-e2e-lab"
        artifacts = repo / ".artifacts/desktop-e2e-lab"
        return cls(
            root=repo,
            lab=lab,
            artifacts=artifacts,
            cache=artifacts / "cache",
            images=artifacts / "images",
            golden=artifacts / "golden",
            runtime=lab / "runtime",
            runs=lab,
            locks=lab / "locks",
        )

    def ensure(self) -> None:
        self._assert_roots_not_redirected()
        self.lab.mkdir(parents=True, exist_ok=True, mode=0o700)
        self.artifacts.mkdir(parents=True, exist_ok=True, mode=0o700)
        self._migrate_legacy_persistent_state()
        for path in (self.cache, self.images, self.golden, self.runtime, self.runs, self.locks):
            if path.is_symlink():
                raise IntegrityError(f"desktop E2E lab directory must not be a symlink: {path.name}")
            path.mkdir(parents=True, exist_ok=True, mode=0o700)
            if not path.is_dir() or path.resolve() != path:
                raise IntegrityError(f"desktop E2E lab directory is not owner-local: {path.name}")

    def scoped(self, path: Path, *, allow_root: bool = False) -> Path:
        self._assert_roots_not_redirected()
        candidate = path.resolve(strict=False)
        for owner in (self.lab.resolve(strict=False), self.artifacts.resolve(strict=False)):
            if candidate == owner and not allow_root:
                raise IntegrityError("refusing destructive operation on a lab root")
            if candidate.is_relative_to(owner):
                return candidate
        raise IntegrityError("refusing operation outside disposable lab scope or persistent artifact scope")

    def _assert_roots_not_redirected(self) -> None:
        if self.lab.resolve(strict=False) != self.lab:
            raise IntegrityError("desktop E2E temporary root must not be redirected by a symlink")
        if self.artifacts.resolve(strict=False) != self.artifacts:
            raise IntegrityError("desktop E2E persistent root must not be redirected by a symlink")

    def _migrate_legacy_persistent_state(self) -> None:
        for name, destination in (
            ("cache", self.cache), ("images", self.images), ("golden", self.golden),
        ):
            legacy = self.lab / name
            if not legacy.exists():
                continue
            if legacy.is_symlink() or not legacy.is_dir():
                raise IntegrityError(f"legacy desktop E2E {name} root is invalid")
            if destination.exists():
                raise IntegrityError(f"both legacy and persistent desktop E2E {name} roots exist")
            os.replace(legacy, destination)
