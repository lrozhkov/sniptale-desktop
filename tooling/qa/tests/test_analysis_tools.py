from __future__ import annotations

import io
import json
import tarfile
import tempfile
import unittest
import unittest.mock as mock
from pathlib import Path

from tooling.qa.runtime import analysis_tools


class AnalysisToolIntegrityTests(unittest.TestCase):
    def test_reused_tool_rejects_changed_contents_without_network_access(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            config = root / "tooling/configs/qa/tool-versions.json"
            config.parent.mkdir(parents=True)
            config.write_text(
                json.dumps(
                    {
                        "externalTools": {
                            "jscpd": {
                                "version": "5.4.0",
                                "linuxX64": {
                                    "url": "https://github.com/kucherenko/jscpd/releases/download/v5.4.0/jscpd-linux-x64-gnu.tar.gz",
                                    "sha256": "a" * 64,
                                },
                            }
                        }
                    }
                )
            )
            base = root / ".tmp/qa-tools/static-analysis"
            destination = base / "jscpd-5.4.0"
            destination.mkdir(parents=True)
            binary = destination / "jscpd"
            binary.write_bytes(b"verified binary")
            (base / "jscpd-5.4.0.json").write_text(
                json.dumps({"archiveSha256": "a" * 64, "treeSha256": analysis_tools.tree_digest(destination)})
            )
            with mock.patch.object(analysis_tools.urllib.request, "urlopen") as network:
                self.assertEqual(analysis_tools.install("jscpd", root), binary)
                binary.write_bytes(b"changed binary")
                with self.assertRaisesRegex(ValueError, "identity changed"):
                    analysis_tools.install("jscpd", root)
                network.assert_not_called()

    def test_codeql_cache_rejects_application_paths_and_broad_permissions(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            parent = Path(directory)
            root = parent / "repo"
            root.mkdir()
            with (
                mock.patch.object(analysis_tools.tempfile, "gettempdir", return_value=str(root)),
                self.assertRaisesRegex(ValueError, "outside the source root"),
            ):
                analysis_tools.installation_base("codeql", root)
            with mock.patch.object(analysis_tools.tempfile, "gettempdir", return_value=str(parent)):
                base = analysis_tools.installation_base("codeql", root)
                base.chmod(0o755)
                with self.assertRaisesRegex(ValueError, "private owned directory"):
                    analysis_tools.installation_base("codeql", root)

    def test_archive_rejects_traversal_external_links_and_devices(self) -> None:
        for kind in ["traversal", "symlink", "device"]:
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                archive = root / "tool.tar.gz"
                member = tarfile.TarInfo("../escaped" if kind == "traversal" else "entry")
                if kind == "symlink":
                    member.type, member.linkname = tarfile.SYMTYPE, "../../outside"
                elif kind == "device":
                    member.type = tarfile.CHRTYPE
                else:
                    member.size = 1
                with tarfile.open(archive, "w:gz") as stream:
                    stream.addfile(member, io.BytesIO(b"x") if member.isfile() else None)
                with self.assertRaises((ValueError, tarfile.FilterError)):
                    analysis_tools.extract_archive(archive, root / "unpacked")
                self.assertFalse((root / "escaped").exists())

    def test_archive_expansion_budget_is_not_a_compressed_size_budget(self) -> None:
        member = tarfile.TarInfo("large")
        member.size = analysis_tools.LIMIT + 1
        archive = mock.Mock()
        archive.getmembers.return_value = [member]
        with self.assertRaisesRegex(ValueError, "extraction budget"):
            analysis_tools.archive_members(archive)
