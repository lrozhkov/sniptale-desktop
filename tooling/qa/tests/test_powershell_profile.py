"""Collected Framework APIs must supplement the baseline without changing identity."""

from __future__ import annotations

import copy
import unittest
from unittest import mock

from tooling.qa.runtime import powershell_profile as owner


class PowerShellProfileTests(unittest.TestCase):
    def test_real_member_data_is_retained_and_conflicting_assembly_is_rejected(self):
        import json
        from pathlib import Path
        import tempfile

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            config = root / "tooling/configs/qa"
            config.mkdir(parents=True)
            (config / "tool-versions.json").write_text(
                json.dumps({"scriptTools": {"PSScriptAnalyzer": {"version": "1.25.0"}}})
            )
            identity = {"Name": "TestAssembly", "Version": "4.0.0.0", "PublicKeyToken": "token"}
            addon = {
                "types": {
                    "System.Test.Type": {
                        "assembly": identity,
                        "data": {
                            "Static": {
                                "Methods": {"Available": {"ReturnType": "System.Void", "OverloadParameters": [[]]}}
                            }
                        },
                    }
                }
            }
            (config / "windows51-type-profile.json").write_text(json.dumps(addon))
            directory_path = root / ".tmp/qa-tools/script-tools/PSScriptAnalyzer-1.25.0/compatibility_profiles"
            directory_path.mkdir(parents=True)
            baseline = directory_path / "win-48_x64_10.0.17763.0_5.1.17763.316_x64_4.0.30319.42000_framework.json"
            body = {"Id": "upstream", "Runtime": {"Types": {"Assemblies": {}}}}
            baseline.write_text(json.dumps(body), encoding="utf-8-sig")
            with mock.patch.object(owner, "ROOT", root):
                owner.prepare()
                result = json.loads((root / ".tmp/qa-reports/windows51-profile.json").read_text())
                self.assertEqual(
                    result["Runtime"]["Types"]["Assemblies"]["TestAssembly"]["Types"]["System.Test"]["Type"],
                    addon["types"]["System.Test.Type"]["data"],
                )
                bad = copy.deepcopy(identity)
                bad["Version"] = "8.0.0.0"
                body["Runtime"]["Types"]["Assemblies"]["TestAssembly"] = {"AssemblyName": bad, "Types": {}}
                baseline.write_text(json.dumps(body))
                with self.assertRaisesRegex(ValueError, "identity mismatch"):
                    owner.prepare()


if __name__ == "__main__":
    unittest.main()
