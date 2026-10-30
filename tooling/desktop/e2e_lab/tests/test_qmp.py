from __future__ import annotations

import sys
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from errors import LabError
from qmp import QmpReader
from vm import VmManager


class QmpTests(unittest.TestCase):
    def reader(self, *blocks):
        client = mock.Mock()
        client.recv.side_effect = blocks
        return QmpReader(client)

    def test_fragmented_and_coalesced_frames_with_events(self):
        reader = self.reader(b'{"QMP":', b'{}}\r\n{"event":"RESET"}\r\n{"return":',
                             b'{"running":true,"status":"running"}}\r\n')
        self.assertEqual(reader.message(), {"QMP": {}})
        self.assertEqual(reader.reply(), {"running": True, "status": "running"})

    def test_malformed_oversized_closed_and_event_flood_fail(self):
        for blocks in ((b'[]\n',), (b'{broken}\n',), (b'\xff\n',), (b'x' * 65537,),
                       (b'',), (b'{"error":{}}\n',), (b'{"unknown":0}\n',),
                       (b'{"event":"RESET"}\n' * 32,)):
            with self.subTest(blocks=tuple(len(block) for block in blocks)):
                with self.assertRaises(LabError):
                    self.reader(*blocks).reply()

    def test_deadline_cannot_be_extended_by_partial_reply(self):
        with mock.patch('qmp.time.monotonic', side_effect=[0, 11]):
            with self.assertRaisesRegex(LabError, 'deadline'):
                self.reader().message()

    def test_halted_missing_and_malformed_status_cannot_pass_health_check(self):
        manager = VmManager(mock.Mock(), mock.Mock())
        with mock.patch.object(manager, 'current', return_value=mock.Mock()), mock.patch.object(manager, 'qmp') as query:
            for status in ({'running': False, 'status': 'internal-error'},
                           {'running': False, 'status': 'paused'}, {'running': 1, 'status': 'running'},
                           {'running': True, 'status': 'shutdown'}, [], {}):
                with self.subTest(status=status):
                    query.return_value = status
                    with self.assertRaises(LabError):
                        manager.assert_running('windows-11')
            query.return_value = {'running': True, 'status': 'running'}
            manager.assert_running('windows-11')
        with mock.patch.object(manager, 'current', return_value=None):
            with self.assertRaisesRegex(LabError, 'unavailable'):
                manager.assert_running('windows-11')
