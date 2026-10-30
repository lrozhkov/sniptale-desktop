from __future__ import annotations

import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from errors import IntegrityError
from extension_artifact import artifact_files
from paths import LabPaths
from session_evidence import collect, inventory
from state import write_json


class SessionEvidenceTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.paths = LabPaths.discover(Path(self.temp.name))
        self.paths.ensure()
        self.identity = "20261030T140000Z"
        self.iteration = "20261030T140001Z"
        self.directory = self.paths.lab / "sessions" / self.identity / "iterations" / self.iteration
        self.directory.mkdir(parents=True)
        (self.directory / "proof.json").write_text('{"passed":true}')
        write_json(self.directory / "development-receipt.json", dict(
            mode="development-session", release_admissible=False, session_id=self.identity,
            iteration=self.iteration, result="pass", operation="test", timings_ms={"test":10},
            evidence=inventory(self.directory)))

    def tearDown(self):
        self.temp.cleanup()

    def test_collection_verifies_files_and_remains_non_admissible(self):
        result = collect(self.paths, self.identity, self.iteration)
        self.assertTrue(result["verified"])
        self.assertIs(result["release_admissible"], False)
        (self.directory / "proof.json").write_text('{"passed":false}')
        with self.assertRaisesRegex(IntegrityError, "inventory"):
            collect(self.paths, self.identity, self.iteration)

    def test_missing_extra_and_redirected_evidence_fail(self):
        proof = self.directory / "proof.json"
        proof.unlink()
        with self.assertRaises(IntegrityError):
            collect(self.paths, self.identity, self.iteration)
        proof.symlink_to(self.paths.root / "outside")
        with self.assertRaises(IntegrityError):
            inventory(self.directory)
        with self.assertRaises(IntegrityError):
            collect(self.paths, "../../outside", self.iteration)

    def test_collection_rejects_invalid_result_and_phase_metadata(self):
        import json
        path = self.directory / "development-receipt.json"
        original = json.loads(path.read_text())
        for key, value in (("result", "unknown"), ("operation", "release"), ("timings_ms", {"test": True}), ("timings_ms", {"test": -1})):
            with self.subTest(key=key, value=value):
                write_json(path, {**original, key: value})
                with self.assertRaises(IntegrityError):
                    collect(self.paths, self.identity, self.iteration)

    def test_extension_inventory_rejects_links_and_fingerprints_runtime_bytes(self):
        runtime = self.directory / "runtime.js"
        runtime.write_text("original runtime")
        first = artifact_files(self.directory)
        runtime.write_text("changed runtime")
        self.assertNotEqual(first, artifact_files(self.directory))
        (self.directory / "escape.js").symlink_to(runtime)
        with self.assertRaisesRegex(IntegrityError, "link"):
            artifact_files(self.directory)


if __name__ == "__main__":
    unittest.main()
