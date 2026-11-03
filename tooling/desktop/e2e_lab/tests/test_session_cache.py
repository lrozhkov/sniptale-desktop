from __future__ import annotations
import json
import shutil
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path
from types import SimpleNamespace
import unittest.mock as mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from paths import LabPaths
from session_cache import cache_root, restore
from state import write_json
from system import sha256_file


class SessionCacheTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.paths = LabPaths.discover(Path(self.temp.name))
        self.paths.ensure()
        self.root = cache_root(self.paths)
        self.root.mkdir()
        self.disk = self.root / "disk.qcow2"
        self.disk.write_bytes(b"synthetic qcow2 fixture")
        self.golden = self.paths.golden / "windows-11/disk.qcow2"
        self.overlay = self.paths.runtime / "overlay.qcow2"
        self.stamp = dict(
            schema_version=2,
            mode="development-session",
            release_admissible=False,
            golden_sha256="a" * 64,
            image_sha256=sha256_file(self.disk),
            created_epoch=int(time.time()) - 10,
            expires_epoch=int(time.time()) + 600,
            source_session_id="20261030T160000Z",
        )
        write_json(self.root / "image.json", self.stamp)

    def tearDown(self):
        self.temp.cleanup()

    def test_restore_creates_small_cow_layer_over_verified_cache_without_copying_it(self):
        info = {"format": "qcow2", "backing-filename": str(self.golden), "virtual-size": 64 * 1024**3}
        with mock.patch("session_cache.run", return_value=SimpleNamespace(stdout=json.dumps(info))) as execute:
            result = restore(self.paths, "qemu-img", self.golden, "a" * 64, self.overlay, 64)
        self.assertEqual(result, self.stamp)
        self.assertIs(result["release_admissible"], False)
        self.assertEqual(execute.call_count, 2)
        conversion = execute.call_args.args[0]
        self.assertEqual(conversion[1], "create")
        self.assertNotIn("convert", conversion)
        self.assertEqual(conversion[conversion.index("-b") + 1], str(self.disk))
        self.assertEqual(self.disk.stat().st_mode & 0o222, 0)
        self.assertEqual(sha256_file(self.disk), self.stamp["image_sha256"])

    def test_corrupt_stale_or_admissible_cache_never_reaches_image_tool(self):
        for changes in (
            {"image_sha256": "b" * 64},
            {"expires_epoch": int(time.time()) - 1},
            {"golden_sha256": "c" * 64},
            {"release_admissible": True},
            {"schema_version": True},
        ):
            with self.subTest(changes=changes), mock.patch("session_cache.run") as execute:
                write_json(self.root / "image.json", {**self.stamp, **changes})
                self.assertIsNone(restore(self.paths, "qemu-img", self.golden, "a" * 64, self.overlay, 64))
                execute.assert_not_called()

    def test_redirected_or_incompatible_backing_never_converts(self):
        for info in (
            {"format": "raw"},
            {"format": "qcow2", "backing-filename": "/outside"},
            {
                "format": "qcow2",
                "backing-filename": str(self.golden),
                "virtual-size": 64 * 1024**3,
                "format-specific": {"data": {"data-file": "/outside"}},
            },
        ):
            with (
                self.subTest(info=info),
                mock.patch("session_cache.run", return_value=SimpleNamespace(stdout=json.dumps(info))) as execute,
            ):
                self.assertIsNone(restore(self.paths, "qemu-img", self.golden, "a" * 64, self.overlay, 64))
                self.assertEqual(execute.call_count, 1)

    def test_compressed_cache_is_checked_before_replacing_previous_image(self):
        from session_cache import store
        from errors import LabError

        previous = self.disk.read_bytes()

        def execute(command, **kwargs):
            if command[1] == "create":
                Path(command[-1]).write_bytes(b"new compressed image")
            elif command[1] == "check":
                raise LabError("injected qcow2 validation failure")

        with (
            mock.patch("session_cache.run", side_effect=execute),
            self.assertRaisesRegex(LabError, "validation failure"),
        ):
            store(self.paths, "qemu-img", self.golden, self.overlay, "a" * 64, "20261030T160001Z", 12)
        self.assertEqual(self.disk.read_bytes(), previous)
        self.assertFalse((self.root / "pending.qcow2").exists())

    def test_cache_budget_failure_preserves_previous_image_and_cleans_candidate(self):
        from session_cache import store
        from errors import ResourceError

        original = self.disk.read_bytes()

        def convert(command, **kwargs):
            if command[1] == "create":
                Path(command[-1]).write_bytes(b"new converted image")

        with mock.patch("session_cache.run", side_effect=convert), self.assertRaises(ResourceError):
            store(self.paths, "qemu-img", self.golden, self.overlay, "a" * 64, "20261030T160001Z", 0)
        self.assertEqual(self.disk.read_bytes(), original)
        self.assertFalse((self.root / "pending.qcow2").exists())
        with mock.patch("session_cache.run", side_effect=convert):
            result = store(self.paths, "qemu-img", self.golden, self.overlay, "a" * 64, "20261030T160001Z", 8)
        self.assertEqual(result["image_sha256"], sha256_file(self.disk))
        self.assertIs(result["release_admissible"], False)

    def test_saved_cache_is_flat_against_original_golden_and_read_only(self):
        from session_cache import store

        commands = []

        def execute(command, **kwargs):
            commands.append(command)
            if command[1] == "create":
                Path(command[-1]).write_bytes(b"new bounded cache")

        with mock.patch("session_cache.run", side_effect=execute):
            store(self.paths, "qemu-img", self.golden, self.overlay, "a" * 64, "20261030T160001Z", 12)
        creation, rebase, check, compare = commands
        self.assertEqual(creation[creation.index("-b") + 1], str(self.overlay))
        self.assertIn("compression_type=zstd", creation)
        self.assertEqual(rebase[1], "rebase")
        self.assertIn("-c", rebase)
        self.assertNotIn("-u", rebase)
        self.assertEqual(rebase[rebase.index("-b") + 1], str(self.golden))
        self.assertEqual(check[1], "check")
        self.assertEqual(compare[1], "compare")
        self.assertEqual(compare[-2:], [str(self.overlay), str(self.root / "pending.qcow2")])
        self.assertEqual(self.disk.stat().st_mode & 0o222, 0)

    def test_candidate_growth_limit_stops_rebase_and_preserves_previous_cache(self):
        from session_cache import store
        from errors import ResourceError

        original = self.disk.read_bytes()

        def execute(command, **kwargs):
            if command[1] == "create":
                Path(command[-1]).write_bytes(b"candidate")
            elif command[1] == "rebase":
                with (self.root / "pending.qcow2").open("r+b") as candidate:
                    candidate.truncate(1025)
                kwargs["health_check"]()

        with (
            mock.patch("session_cache.MAX_BYTES", 1024),
            mock.patch("session_cache.run", side_effect=execute),
            self.assertRaisesRegex(ResourceError, "cache budget"),
        ):
            store(self.paths, "qemu-img", self.golden, self.overlay, "a" * 64, "20261030T160001Z", 12)
        self.assertEqual(self.disk.read_bytes(), original)
        self.assertFalse((self.root / "pending.qcow2").exists())

    def test_data_mismatch_preserves_previous_cache(self):
        from session_cache import store
        from errors import LabError

        original = self.disk.read_bytes()

        def execute(command, **kwargs):
            if command[1] == "create":
                Path(command[-1]).write_bytes(b"incorrect guest contents")
            if command[1] == "compare":
                raise LabError("guest-visible data mismatch")

        with mock.patch("session_cache.run", side_effect=execute), self.assertRaisesRegex(LabError, "data mismatch"):
            store(self.paths, "qemu-img", self.golden, self.overlay, "a" * 64, "20261030T160001Z", 12)
        self.assertEqual(self.disk.read_bytes(), original)
        self.assertFalse((self.root / "pending.qcow2").exists())

    def test_old_cache_schema_is_not_reused(self):
        write_json(self.root / "image.json", {**self.stamp, "schema_version": 1})
        with mock.patch("session_cache.run") as execute:
            self.assertIsNone(restore(self.paths, "qemu-img", self.golden, "a" * 64, self.overlay, 64))
        execute.assert_not_called()

    def test_real_qcow_chain_retains_inherited_blocks_across_two_saves(self):
        from session_cache import store

        binary = shutil.which("qemu-img")
        if not binary:
            managed = Path(__file__).resolve().parents[4] / ".artifacts/desktop-e2e-lab/cache/host-tools/bin/qemu-img"
            if managed.is_file():
                binary = str(managed)
        if not binary:
            self.skipTest("QEMU image tool is required for the real backing-chain regression")

        def image(*arguments):
            subprocess.run([binary, *map(str, arguments)], check=True, capture_output=True, timeout=60)

        self.golden.parent.mkdir(parents=True, exist_ok=True)
        image("create", "-q", "-f", "qcow2", self.golden, "16M")
        raw = Path(self.temp.name) / "expected.raw"
        expected = bytearray(16 * 1024**2)
        expected[65536:131072] = bytes([0x5A]) * 65536
        expected[524288:589824] = bytes([0xC3]) * 65536
        raw.write_bytes(expected)
        self.disk.unlink()
        image("convert", "-q", "-f", "raw", "-O", "qcow2", "-B", self.golden, "-F", "qcow2", raw, self.disk)
        self.overlay.parent.mkdir(parents=True, exist_ok=True)
        for index in range(2):
            image("create", "-q", "-f", "qcow2", "-F", "qcow2", "-b", self.disk, self.overlay)
            store(self.paths, binary, self.golden, self.overlay, "a" * 64, f"20261030T16000{index}Z", 12)
            image("compare", "-q", "-f", "raw", "-F", "qcow2", raw, self.disk)
            info = json.loads(
                subprocess.run(
                    [binary, "info", "--output=json", str(self.disk)], check=True, capture_output=True, text=True
                ).stdout
            )
            self.assertEqual(info["backing-filename"], str(self.golden))
            self.overlay.unlink()
