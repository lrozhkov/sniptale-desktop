"""Keep tool archive admission bounded before extraction."""

from __future__ import annotations

import io
import tarfile
import unittest
import copy
import hashlib
import json
import tempfile
from pathlib import Path
from unittest import mock
from tooling.qa.runtime import install_script_tools as owner

from tooling.qa.runtime.python_toolchain import validate as validate_python

from tooling.qa.runtime.install_script_tools import bounded_members


class ScriptInstallationTests(unittest.TestCase):
    def test_archive_content_must_fit_byte_budget(self):
        stream = io.BytesIO()
        with tarfile.open(fileobj=stream, mode="w") as archive:
            member = tarfile.TarInfo("tool")
            member.size = 4
            archive.addfile(member, io.BytesIO(b"tool"))
        for budget, accepted in ((3, False), (4, True)):
            stream.seek(0)
            with self.subTest(budget=budget), tarfile.open(fileobj=stream) as archive:
                if accepted:
                    self.assertEqual([item.name for item in bounded_members(archive, budget)], ["tool"])
                else:
                    with self.assertRaisesRegex(ValueError, "budget"):
                        bounded_members(archive, budget)

    def test_python_distribution_is_bound_to_publisher_version_platform_and_hash(self):
        version = "3.15.2"
        tag = version + "-1234"
        item = {
            "tag": tag,
            "source": "https://api.github.com/repos/actions/python-versions/releases/tags/" + tag,
            "url": f"https://github.com/actions/python-versions/releases/download/{tag}/python-{version}-linux-24.04-x64.tar.gz",
            "sha256": "a" * 64,
        }
        validate_python(version, item)
        for key in ("tag", "source", "url", "sha256"):
            bad = copy.deepcopy(item)
            bad[key] = "invalid"
            with self.subTest(key=key), self.assertRaisesRegex(ValueError, "identity"):
                validate_python(version, bad)

    def test_stale_installed_graph_is_rejected_when_source_locks_change(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            cache = root / "cache"
            config = root / "tooling/configs/qa"
            config.mkdir(parents=True)
            inputs = [
                config / "script-python-requirements.txt",
                config / "script-python-build-requirements.txt",
                config / "script-node/package-lock.json",
            ]
            for path in inputs:
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("original")
            schema = {
                "manager": "npm",
                "version": "1.0.0",
                "schemaAsset": {"schemaSha256": hashlib.sha256(b"{}").hexdigest()},
            }
            (config / "toolchain-lock.json").write_text(json.dumps({"scriptTools": {"renovate": schema}}))
            for group, names in (
                ("python/bin", ("python", "ruff", "yamllint", "zizmor")),
                ("npm/node_modules/.bin", ("eslint", "pyright")),
            ):
                for name in names:
                    path = cache / group / name
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.touch()
            (cache / "python/.identity").write_text(owner.python_identity(root))
            (cache / "npm/.identity").write_text(hashlib.sha256(inputs[2].read_bytes()).hexdigest())
            schema_path = cache / "renovate-schema-1.0.0/renovate-schema.json"
            schema_path.parent.mkdir()
            schema_path.write_text("{}")
            with mock.patch.object(owner, "ROOT", root), mock.patch.object(owner, "CACHE", cache):
                owner.verify_installed()
                for path in inputs:
                    path.write_text("changed")
                    with self.subTest(input=path.name), self.assertRaisesRegex(ValueError, "inputs differ"):
                        owner.verify_installed()
                    path.write_text("original")


if __name__ == "__main__":
    unittest.main()
