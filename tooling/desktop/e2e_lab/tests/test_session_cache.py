from __future__ import annotations
import json
import sys
import tempfile
import time
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

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
        self.disk = self.root / 'disk.qcow2'
        self.disk.write_bytes(b'synthetic qcow2 fixture')
        self.golden = self.paths.golden / 'windows-11/disk.qcow2'
        self.overlay = self.paths.runtime / 'overlay.qcow2'
        self.stamp = dict(schema_version=1, mode='development-session', release_admissible=False,
                          golden_sha256='a' * 64, image_sha256=sha256_file(self.disk),
                          created_epoch=int(time.time()) - 10, expires_epoch=int(time.time()) + 600,
                          source_session_id='20261030T160000Z')
        write_json(self.root / 'image.json', self.stamp)

    def tearDown(self):
        self.temp.cleanup()

    def test_restore_retains_golden_backing_and_has_no_release_admissibility(self):
        info = {'format': 'qcow2', 'backing-filename': str(self.golden), 'virtual-size': 64 * 1024 ** 3}
        with mock.patch('session_cache.run', return_value=SimpleNamespace(stdout=json.dumps(info))) as execute:
            result = restore(self.paths, 'qemu-img', self.golden, 'a' * 64, self.overlay, 64)
        self.assertEqual(result, self.stamp)
        self.assertIs(result['release_admissible'], False)
        self.assertEqual(execute.call_count, 2)
        conversion = execute.call_args.args[0]
        self.assertIn('-B', conversion)
        self.assertEqual(conversion[conversion.index('-B') + 1], str(self.golden))

    def test_corrupt_stale_or_admissible_cache_never_reaches_image_tool(self):
        for changes in ({'image_sha256': 'b' * 64}, {'expires_epoch': int(time.time()) - 1},
                        {'golden_sha256': 'c' * 64}, {'release_admissible': True}, {'schema_version': True}):
            with self.subTest(changes=changes), mock.patch('session_cache.run') as execute:
                write_json(self.root / 'image.json', {**self.stamp, **changes})
                self.assertIsNone(restore(self.paths, 'qemu-img', self.golden, 'a' * 64, self.overlay, 64))
                execute.assert_not_called()

    def test_redirected_or_incompatible_backing_never_converts(self):
        for info in ({'format': 'raw'}, {'format': 'qcow2', 'backing-filename': '/outside'},
                     {'format': 'qcow2', 'backing-filename': str(self.golden), 'virtual-size': 64 * 1024 ** 3,
                      'format-specific': {'data': {'data-file': '/outside'}}}):
            with self.subTest(info=info), mock.patch('session_cache.run', return_value=SimpleNamespace(stdout=json.dumps(info))) as execute:
                self.assertIsNone(restore(self.paths, 'qemu-img', self.golden, 'a' * 64, self.overlay, 64))
                self.assertEqual(execute.call_count, 1)

    def test_compressed_cache_is_checked_before_replacing_previous_image(self):
        from session_cache import store
        from errors import LabError
        previous = self.disk.read_bytes()
        def execute(command, **kwargs):
            if command[1] == 'convert':
                self.assertIn('compression_type=zstd', command)
                self.assertIn('-c', command)
                Path(command[-1]).write_bytes(b'new compressed image')
            else:
                raise LabError('injected qcow2 validation failure')
        with mock.patch('session_cache.run', side_effect=execute):
            with self.assertRaisesRegex(LabError, 'validation failure'):
                store(self.paths, 'qemu-img', self.golden, self.overlay, 'a' * 64, '20261030T160001Z', 12)
        self.assertEqual(self.disk.read_bytes(), previous)
        self.assertFalse((self.root / 'pending.qcow2').exists())

    def test_cache_budget_failure_preserves_previous_image_and_cleans_candidate(self):
        from session_cache import store
        from errors import ResourceError
        original = self.disk.read_bytes()
        def convert(command, **kwargs):
            if command[1] == 'convert':
                Path(command[-1]).write_bytes(b'new converted image')
        with mock.patch('session_cache.run', side_effect=convert):
            with self.assertRaises(ResourceError):
                store(self.paths, 'qemu-img', self.golden, self.overlay, 'a' * 64, '20261030T160001Z', 0)
        self.assertEqual(self.disk.read_bytes(), original)
        self.assertFalse((self.root / 'pending.qcow2').exists())
        with mock.patch('session_cache.run', side_effect=convert):
            result = store(self.paths, 'qemu-img', self.golden, self.overlay, 'a' * 64, '20261030T160001Z', 8)
        self.assertEqual(result['image_sha256'], sha256_file(self.disk))
        self.assertIs(result['release_admissible'], False)
