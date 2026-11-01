"""Release ZIP runtime bytes and archive admission are independently verified."""

from __future__ import annotations

import hashlib
import json
import stat
import subprocess
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from extension_artifact import extract_release, package_extension
from errors import IntegrityError


class ReleaseExtensionTests(unittest.TestCase):
    def test_browser_runtime_contracts_use_the_pinned_node_before_guest_work(self):
        root = Path(__file__).resolve().parents[4]
        versions = json.loads((root / "tooling/configs/qa/tool-versions.json").read_text())
        version = versions["browserAutomation"]["node"]
        node = root / f".tmp/qa-tools/node-{version}/bin/node"
        self.assertTrue(node.is_file(), "install the cataloged Node runtime before the release harness")
        for contract, runtime in (
            ("native-probe-contracts.cjs", "../browser-extension/probe.js"),
            ("real-extension-contracts.cjs", "real-extension.js"),
            ("browser-playwright-contracts.cjs", "browser-playwright.cjs"),
        ):
            result = subprocess.run(
                [
                    str(node),
                    str(root / "tooling/desktop/e2e_lab/tests" / contract),
                    str(root / "tooling/desktop/e2e_lab/guest/windows" / runtime),
                ],
                cwd=root,
                capture_output=True,
                text=True,
                check=False,
                timeout=30,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_release_input_is_not_rebuilt_and_only_manifest_is_adapted(self):
        root = Path(__file__).resolve().parents[4]
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            source, output = base / "source", base / "output"
            source.mkdir()
            (source / ".git").mkdir()
            output.mkdir()
            bundle = base / "release.zip"
            manifest = {
                "manifest_version": 3,
                "name": "Sniptale",
                "version": "0.5.2",
                "background": {"service_worker": "background.js"},
                "permissions": ["storage"],
                "optional_permissions": ["nativeMessaging"],
            }
            files = {
                "manifest.json": json.dumps(manifest).encode(),
                "background.js": b"/* release runtime */",
                "bundle.map": b"original source map",
                "settings/index.html": b"<html>release</html>",
            }
            with zipfile.ZipFile(bundle, "w") as archive:
                for name, data in files.items():
                    archive.writestr(name, data)
            with (
                mock.patch("extension_artifact.source_tree_fingerprint", return_value={"commit": "test"}),
                mock.patch("extension_artifact.run") as run,
            ):
                target, receipt = package_extension(SimpleNamespace(root=root), bundle, source, output)
                self.assertEqual(run.call_count, 2)
                for call in run.call_args_list:
                    self.assertEqual(call.args[0][0], "node")
                    self.assertNotIn("qa:e2e:build", call.args[0])
            self.assertEqual(receipt["input_kind"], "release-zip")
            self.assertEqual(receipt["input_bundle_sha256"], hashlib.sha256(bundle.read_bytes()).hexdigest())
            self.assertEqual(
                {entry["name"]: entry["sha256"] for entry in receipt["original_artifacts"]},
                {name: hashlib.sha256(data).hexdigest() for name, data in files.items()},
            )
            with zipfile.ZipFile(target) as archive:
                self.assertEqual(set(archive.namelist()), set(files))
                for name, data in files.items():
                    if name != "manifest.json":
                        self.assertEqual(archive.read(name), data)
                adapted = json.loads(archive.read("manifest.json"))
            changes = {key for key in set(manifest) | set(adapted) if manifest.get(key) != adapted.get(key)}
            self.assertEqual(changes, set(receipt["fixture_overrides"]))
            self.assertEqual(changes, {"key", "version_name", "permissions", "optional_permissions"})
            self.assertEqual(adapted["permissions"], ["nativeMessaging", "storage"])
            self.assertEqual(adapted["optional_permissions"], [])

    def test_zip_rejects_traversal_links_windows_aliases_and_duplicates(self):
        for names in (
            ("../escape.js",),
            ("/absolute.js",),
            ("a/../b.js",),
            ("a\\b.js",),
            ("bad.js:stream",),
            ("name.js.",),
            ("CON.js",),
            ("File.js", "file.js"),
            ("dup.js", "dup.js"),
            ("Dir/one.js", "dir/two.js"),
        ):
            with self.subTest(names=names), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                bundle = root / "input.zip"
                with zipfile.ZipFile(bundle, "w") as archive:
                    for name in names:
                        archive.writestr(name, b"synthetic")
                with self.assertRaises(IntegrityError):
                    extract_release(bundle, root / "extracted")
                self.assertFalse((root / "escape.js").exists())
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            bundle = root / "link.zip"
            entry = zipfile.ZipInfo("link.js")
            entry.create_system = 3
            entry.external_attr = (stat.S_IFLNK | 0o777) << 16
            with zipfile.ZipFile(bundle, "w") as archive:
                archive.writestr(entry, b"outside.js")
            with self.assertRaises(IntegrityError):
                extract_release(bundle, root / "extracted")
