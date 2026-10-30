import io
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from browser_automation import runtime_inventory
from errors import IntegrityError


class BrowserAutomationTests(unittest.TestCase):
    def test_package_paths_links_aliases_and_expansion_fail_closed(self):
        for name, kind in [('package/runtime.js', tarfile.REGTYPE), ('../escape', tarfile.REGTYPE),
                           ('package/../escape', tarfile.REGTYPE), ('package/link', tarfile.SYMTYPE),
                           ('package/alias', tarfile.LNKTYPE), ('package/file:stream', tarfile.REGTYPE),
                           ('package/file.', tarfile.REGTYPE), ('package/CON.txt', tarfile.REGTYPE),
                           ('package/./alias.js', tarfile.REGTYPE), ('package//alias.js', tarfile.REGTYPE)]:
            with self.subTest(name=name, kind=kind), tempfile.TemporaryDirectory() as directory:
                package = Path(directory) / 'client.tgz'
                with tarfile.open(package, 'w:gz') as archive:
                    item = tarfile.TarInfo(name); item.type = kind
                    item.size = 1 if kind == tarfile.REGTYPE else 0
                    archive.addfile(item, io.BytesIO(b'x') if item.size else None)
                if name == 'package/runtime.js':
                    self.assertEqual(list(runtime_inventory(package)), [name])
                else:
                    with self.assertRaises(IntegrityError): runtime_inventory(package)
