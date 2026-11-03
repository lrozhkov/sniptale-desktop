from __future__ import annotations

import hashlib
import json
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]


class DevPackageTests(unittest.TestCase):
    def test_unsigned_smoke_refuses_production_channels_and_implicit_trust(self) -> None:
        version = json.loads((ROOT / "tooling/configs/qa/tool-versions.json").read_text())["scriptTools"]["pwsh"][
            "version"
        ]
        pwsh = ROOT / f".tmp/qa-tools/script-tools/pwsh-{version}/pwsh"
        for args in (
            ["-Channel", "stable", "-UnsignedDev", "-DevLocalTrust"],
            ["-Channel", "beta", "-UnsignedDev", "-DevLocalTrust"],
            ["-Channel", "dev", "-UnsignedDev"],
            ["-BuildProfile", "dev"],
        ):
            with self.subTest(args=args):
                result = subprocess.run(
                    [
                        str(pwsh),
                        "-NoLogo",
                        "-NoProfile",
                        "-NonInteractive",
                        "-File",
                        str(ROOT / "tooling/release/windows/smoke/windows-smoke.ps1"),
                        *args,
                    ],
                    capture_output=True,
                    text=True,
                    timeout=20,
                )
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("Unsigned dev builds require", result.stderr)

    def test_bootstrap_rejects_bad_inventory_before_any_installer_action(self) -> None:
        version = json.loads((ROOT / "tooling/configs/qa/tool-versions.json").read_text())["scriptTools"]["pwsh"][
            "version"
        ]
        pwsh = ROOT / f".tmp/qa-tools/script-tools/pwsh-{version}/pwsh"
        self.assertTrue(pwsh.is_file(), "Install the pinned script tools before harness tests")
        for mutation in ("missing", "empty", "omitted-msi", "duplicate", "hash", "origin"):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                shutil.copyfile(ROOT / "tooling/release/windows/install-dev.ps1", root / "install-dev.ps1")
                for name in ("sniptale-native-agent.exe", "SniptaleNativeAgent-dev-0.1.1.msi"):
                    (root / name).write_bytes(b"dev-fixture")
                origin = "chrome-extension://" + "a" * 32 + "/"
                (root / "native-host.json").write_text(
                    json.dumps({"name": "com.sniptale.dev.native_host", "allowed_origins": [origin]})
                )
                entries = [
                    {"path": p.name, "sha256": hashlib.sha256(p.read_bytes()).hexdigest()} for p in root.iterdir()
                ]
                bundle = {
                    "kind": "unsigned-dev-local",
                    "releaseAdmissible": False,
                    "host": "com.sniptale.dev.native_host",
                    "extensionOrigin": origin,
                    "files": entries,
                }
                if mutation == "missing":
                    del bundle["files"]
                elif mutation == "empty":
                    bundle["files"] = []
                elif mutation == "omitted-msi":
                    bundle["files"] = [e for e in entries if not e["path"].endswith(".msi")]
                elif mutation == "duplicate":
                    bundle["files"] = [*entries, entries[0]]
                elif mutation == "hash":
                    entries[0]["sha256"] = "0" * 64
                else:
                    bundle["extensionOrigin"] = "chrome-extension://" + "b" * 32 + "/"
                (root / "dev-bundle.json").write_text(json.dumps(bundle))
                result = subprocess.run(
                    [
                        str(pwsh),
                        "-NoLogo",
                        "-NoProfile",
                        "-NonInteractive",
                        "-File",
                        str(root / "install-dev.ps1"),
                        "-NoLaunch",
                    ],
                    capture_output=True,
                    text=True,
                    timeout=20,
                )
                self.assertNotEqual(result.returncode, 0)
                expected = "identity" if mutation == "origin" else "verification" if mutation == "hash" else "inventory"
                self.assertIn(expected, result.stderr.lower(), result.stderr)
                self.assertNotIn("Get-AuthenticodeSignature", result.stderr)
                self.assertFalse((root / "dev-local-browser-trust.json").exists())


if __name__ == "__main__":
    unittest.main()
