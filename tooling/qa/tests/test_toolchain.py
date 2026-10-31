from datetime import datetime, timedelta, UTC
import unittest
from tooling.qa.runtime.toolchain import ToolchainError, check_age


class ToolchainAgeTests(unittest.TestCase):
    def setUp(self):
        self.now = datetime(2026, 10, 30, 12, tzinfo=UTC)
        self.policy = {"minimumReleaseAgeDays": 7}

    def test_seven_full_days_required(self):
        check_age((self.now - timedelta(days=7)).isoformat(), self.policy, now=self.now)
        with self.assertRaises(ToolchainError):
            check_age((self.now - timedelta(days=7) + timedelta(microseconds=1)).isoformat(), self.policy, now=self.now)

    def test_future_or_ambiguous_dates_fail(self):
        for value in ["2026-10-31T00:00:00Z", "2026-10-01", None, "invalid"]:
            with self.subTest(value=value), self.assertRaises(ToolchainError):
                check_age(value, self.policy, now=self.now)

    def test_generic_bypass_or_nonsecurity_reason_fails(self):
        for exception in [
            True,
            {"allowYoung": True},
            {"advisory": "https://example.com/release", "reason": "faster", "fixedVersion": "1.2.3"},
        ]:
            with self.subTest(exception=exception), self.assertRaises(ToolchainError):
                check_age(self.now.isoformat(), self.policy, exception, now=self.now)

    def test_advisory_bound_security_exception(self):
        exception = {
            "advisory": "https://rustsec.org/advisories/RUSTSEC-2026-0001.html",
            "reason": "Fix confirmed vulnerability in the selected tool",
            "fixedVersion": "1.2.3",
        }
        check_age(self.now.isoformat(), self.policy, exception, now=self.now)

    def test_policy_cannot_be_lowered_or_boolean(self):
        for days in [0, 6, True, "7", None]:
            with self.subTest(days=days), self.assertRaises(ToolchainError):
                check_age("2020-01-01T00:00:00Z", {"minimumReleaseAgeDays": days}, now=self.now)


class ToolchainLockTests(unittest.TestCase):
    def test_version_strings_are_narrowed_before_shell_generation(self):
        import copy
        import json
        from tooling.qa.runtime.toolchain import ROOT, VERSIONS, validate_versions

        versions = json.loads((ROOT / VERSIONS).read_text())
        for section, key, bad in (
            ("windows", "wix", "5.0.2';exit"),
            ("windows", "sdkPackage", "latest"),
            ("python", "version", True),
            ("rust", "channel", "stable"),
            ("githubActions", "actions/cache", "main"),
        ):
            with self.subTest(section=section, key=key):
                candidate = copy.deepcopy(versions)
                candidate[section][key] = bad
                with self.assertRaises(ToolchainError):
                    validate_versions(candidate)

    def test_missing_stale_or_mutated_inputs_fail(self):
        import json
        import shutil
        import tempfile
        from pathlib import Path
        from tooling.qa.runtime.toolchain import ROOT, VERSIONS, LOCK, check

        for mutation in (
            "missing-tool",
            "stale-version",
            "package-url",
            "package-hash",
            "compiler-component",
            "exception-version",
            "generated-rust",
            "missing-crate",
            "young-crate",
            "custom-registry",
            "python-package-hash",
            "python-package-source",
            "python-linux-source",
            "python-linux-hash",
        ):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                (root / VERSIONS).parent.mkdir(parents=True)
                for relative in (VERSIONS, LOCK, Path("rust-toolchain.toml"), Path("Cargo.lock"), Path("Cargo.toml")):
                    shutil.copyfile(ROOT / relative, root / relative)
                lock = json.loads((root / LOCK).read_text())
                versions = json.loads((root / VERSIONS).read_text())
                if mutation == "missing-tool":
                    del lock["tools"]["cargo-nextest"]
                elif mutation == "stale-version":
                    lock["tools"]["cargo-nextest"]["version"] = "0.0.1"
                elif mutation == "package-url":
                    lock["nuget"]["wix"]["url"] = "https://example.com/package"
                elif mutation == "package-hash":
                    lock["nuget"]["wix"]["sha256"] = "invalid"
                elif mutation == "compiler-component":
                    lock["rust"]["components"][0]["url"] = "https://example.com/rust"
                elif mutation == "exception-version":
                    versions["securityExceptions"]["python"]["fixedVersion"] = "3.15.1"
                elif mutation == "python-package-hash":
                    lock["python"]["windows"]["sha256"] = "f" * 64
                elif mutation == "python-package-source":
                    lock["python"]["windows"]["url"] = "https://example.com/python.zip"
                elif mutation == "python-linux-source":
                    lock["python"]["linuxX64"]["url"] = "https://example.com/python.tar.gz"
                elif mutation == "python-linux-hash":
                    lock["python"]["linuxX64"]["sha256"] = "invalid"
                elif mutation == "custom-registry":
                    cargo = root / "Cargo.lock"
                    cargo.write_text(
                        cargo.read_text().replace(
                            "registry+https://github.com/rust-lang/crates.io-index",
                            "registry+https://example.com/index",
                            1,
                        )
                    )
                elif mutation == "missing-crate":
                    lock["cargo"].pop()
                elif mutation == "young-crate":
                    lock["cargo"][0]["publishedAt"] = datetime.now(UTC).isoformat()
                else:
                    (root / "rust-toolchain.toml").write_text('[toolchain]\nchannel="stable"\n')
                (root / LOCK).write_text(json.dumps(lock))
                (root / VERSIONS).write_text(json.dumps(versions))
                with self.assertRaises(ToolchainError):
                    check(root)


class BrowserToolchainTests(unittest.TestCase):
    def test_pins_age_hash_and_official_sources_are_required(self):
        import copy
        import json
        from tooling.qa.runtime import browser_toolchain
        from tooling.qa.runtime.toolchain import ROOT, VERSIONS, LOCK

        versions = json.loads((ROOT / VERSIONS).read_text())
        lock = json.loads((ROOT / LOCK).read_text())["browserAutomation"]
        browser_toolchain.check(versions, lock, check_age)
        for field, value in [
            ("url", "https://untrusted.invalid/node.exe"),
            ("sha256", "bad"),
            ("version", "0.0.0"),
            ("source", "https://untrusted.invalid/release"),
            ("publishedAt", datetime.now(UTC).isoformat()),
        ]:
            changed = copy.deepcopy(lock)
            changed["node"][field] = value
            with self.subTest(field=field), self.assertRaises((ValueError, ToolchainError)):
                browser_toolchain.check(versions, changed, check_age)
        for value in ["latest", "1.2.3';exit", True]:
            changed = copy.deepcopy(versions)
            changed["browserAutomation"]["node"] = value
            with self.assertRaises(ValueError):
                browser_toolchain.urls(changed)
