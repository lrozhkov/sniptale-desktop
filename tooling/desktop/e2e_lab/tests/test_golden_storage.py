from __future__ import annotations

import os
import hashlib
import stat
import sys
import tempfile
import time
import unittest
from pathlib import Path
import unittest.mock as mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from golden_storage import cleanup_source_images, compact_existing_goldens
from host import _write_wrapper, bootstrap
from paths import LabPaths
from state import write_json


class GoldenStorageTests(unittest.TestCase):
    def test_source_images_are_removed_after_sealed_golden_or_stale_day(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            paths = LabPaths.discover(Path(directory) / "repo")
            paths.ensure()
            referenced = "a" * 64
            identity = paths.golden / "windows-11/identity.json"
            write_json(identity, {"source_sha256": referenced})
            current = paths.images / f"windows-{referenced}.iso"
            stale = paths.images / f"ubuntu-{'b' * 64}.img"
            recent = paths.images / f"pending-{'c' * 64}.iso"
            for path in (current, stale, recent):
                path.write_bytes(b"image")
            old = time.time() - 90000
            os.utime(stale, (old, old))

            result = cleanup_source_images(paths)

            self.assertEqual(result["source_images_removed"], 2)
            self.assertFalse(current.exists())
            self.assertFalse(stale.exists())
            self.assertTrue(recent.exists())

    def test_compaction_is_deferred_while_any_lane_is_running(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            paths = LabPaths.discover(Path(directory) / "repo")
            paths.ensure()
            manager = mock.Mock()
            manager.current.side_effect = lambda lane, clean_stale=False: object() if lane == "windows-11" else None
            manager.orphan_pids.return_value = []

            result = compact_existing_goldens(paths, mock.Mock(), manager)

            self.assertEqual(result["goldens_skipped_running"], 1)
            self.assertEqual(result["goldens_compacted"], 0)

    def test_compaction_keeps_disk_and_identity_private_and_readable_by_owner(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            paths = LabPaths.discover(Path(directory) / "repo")
            paths.ensure()
            golden = paths.golden / "windows-11"
            golden.mkdir(parents=True)
            disk = golden / "disk.qcow2"
            disk.write_bytes(b"original golden")
            write_json(golden / "identity.json", {"disk_sha256": hashlib.sha256(disk.read_bytes()).hexdigest()})
            manager = mock.Mock()
            manager.current.return_value = None
            manager.orphan_pids.return_value = []

            def convert(command, **_options):
                if "convert" in command:
                    Path(command[-1]).write_bytes(b"compressed golden")

            with (
                mock.patch("golden_storage._qemu_img", return_value="qemu-img"),
                mock.patch("golden_storage.run", side_effect=convert),
            ):
                result = compact_existing_goldens(paths, mock.Mock(), manager)
            self.assertEqual(result["goldens_compacted"], 1)
            self.assertEqual(disk.read_bytes(), b"compressed golden")
            for path in [disk, golden / "identity.json"]:
                self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o400)
                self.assertTrue(path.read_bytes())

    def test_bundled_host_tool_wrapper_is_relocatable(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            prefix = Path(directory) / "host-tools"
            target = prefix / "usr/bin/qemu-img"
            target.parent.mkdir(parents=True)
            target.write_bytes(b"binary")

            _write_wrapper(prefix, "qemu-img")

            wrapper = (prefix / "bin/qemu-img").read_text(encoding="utf-8")
            self.assertIn("${BASH_SOURCE[0]}", wrapper)
            self.assertNotIn(str(prefix), wrapper)

    def test_bootstrap_repairs_relocated_qemu_data_symlink(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            paths = LabPaths.discover(Path(directory) / "repo")
            paths.ensure()
            prefix = paths.cache / "host-tools"
            firmware = prefix / "usr/share/seabios/efi-e1000e.rom"
            firmware.parent.mkdir(parents=True)
            firmware.write_bytes(b"rom")
            qemu_data = prefix / "usr/share/qemu"
            qemu_data.mkdir(parents=True)
            link = qemu_data / firmware.name
            link.symlink_to("/old/temporary/prefix/efi-e1000e.rom")

            with (
                mock.patch("host._host_tool", return_value="/usr/bin/tool"),
                mock.patch("host._installed", return_value=True),
            ):
                bootstrap(paths)

            self.assertEqual(link.resolve(strict=False), firmware.resolve(strict=False))


if __name__ == "__main__":
    unittest.main()
