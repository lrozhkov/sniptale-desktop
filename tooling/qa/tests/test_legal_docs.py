from __future__ import annotations

import importlib.util
import json
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]


def load_module(name: str, relative: str):
    spec = importlib.util.spec_from_file_location(name, ROOT / relative)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load {relative}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


docs = load_module("verify_docs_test", "tooling/qa/rules/verify_docs.py")
legal_generator = load_module("legal_generator_test", "tooling/release/legal/generate_dependency_licenses.py")
legal_verifier = load_module("legal_verifier_test", "tooling/qa/rules/verify_legal.py")
sbom_generator = load_module("sbom_generator_test", "tooling/release/windows/generate-sbom.py")


class DocumentationPolicyTests(unittest.TestCase):
    def test_natural_paragraphs_accept_long_lines_and_reject_wrapping(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "probe.md"
            path.write_text("# Heading\n\n" + "long " * 100 + "\n", encoding="utf-8")
            self.assertEqual(docs.prose_errors(path), [])
            path.write_text("# Heading\n\nfirst physical line\nsecond physical line\n", encoding="utf-8")
            self.assertTrue(docs.prose_errors(path))

    def test_markdown_policy_requires_unique_disjoint_paths_and_exact_indexing(self) -> None:
        policy = {"activeDocs": ["docs/README.md", "docs/a.md", "docs/a.md"], "repositoryMarkdown": ["docs/a.md"]}
        errors = docs.policy_shape_errors(policy, "[A](a.md)\n[A again](a.md)\n")
        self.assertTrue(any("duplicate" in error for error in errors))
        self.assertTrue(any("overlap" in error for error in errors))
        self.assertTrue(any("exactly once" in error for error in errors))


class DependencyLicenseGeneratorTests(unittest.TestCase):
    def test_canonical_apache_appendix_is_preserved_but_modified_placeholders_fail(self) -> None:
        # Source: https://www.apache.org/licenses/LICENSE-2.0.txt, retrieved 2026-11-02.
        text = (ROOT / "tooling/qa/fixtures/licenses/apache-2.0.txt").read_text()
        package = {
            "name": "dep",
            "version": "1.0.0",
            "id": "registry#dep@1.0.0",
            "source": "registry",
            "repository": None,
        }
        raw = {
            "crates": [{"package": package, "license": "Apache-2.0"}],
            "licenses": [{"id": "Apache-2.0", "text": text, "used_by": [{"crate": package}]}],
        }
        _, files = legal_generator.build_outputs(raw)
        self.assertEqual(list(files.values()), [text.encode()])
        self.assertFalse(legal_verifier.unresolved_license_placeholders(text, ["Apache-2.0"]))
        for ids in [None, [], ["MIT"], ["Apache-2.0", "MIT"]]:
            self.assertTrue(legal_verifier.unresolved_license_placeholders(text, ids))
        for changed in [
            "Copyright [yyyy] [name of copyright owner]",
            text + "Copyright <year>\n",
            text.replace("Version 2.0", "Version 9.9", 1),
        ]:
            with self.subTest(changed=changed[:40]), self.assertRaises(ValueError):
                legal_generator.reject_placeholder_license(changed, "Apache-2.0")
            self.assertTrue(legal_verifier.unresolved_license_placeholders(changed, ["Apache-2.0"]))
        with self.assertRaises(ValueError):
            legal_generator.reject_placeholder_license(text, "MIT")

    def test_apache_standard_text_allows_upstream_whitespace_without_rewriting_bytes(self):
        canonical = (ROOT / "tooling/qa/fixtures/licenses/apache-2.0.txt").read_text()
        compact = "\n".join(line.strip() for line in canonical.splitlines()) + "\n"
        legal_generator.reject_placeholder_license(compact, "Apache-2.0")
        self.assertFalse(legal_verifier.unresolved_license_placeholders(compact, ["Apache-2.0"]))
        raw = {
            "crates": [
                {
                    "package": {
                        "id": "registry#dunce@1.0.5",
                        "name": "dunce",
                        "version": "1.0.5",
                        "license": "Apache-2.0",
                    }
                }
            ],
            "licenses": [{"id": "Apache-2.0", "text": compact, "used_by": [{"crate": {"id": "registry#dunce@1.0.5"}}]}],
        }
        _, files = legal_generator.build_outputs(raw)
        self.assertEqual(list(files.values()), [compact.encode()])
        with self.assertRaises(ValueError):
            legal_generator.reject_placeholder_license(
                compact.replace("Copyright", "DifferentCopyright", 1), "Apache-2.0"
            )

    def test_generator_writes_readable_package_owned_files(self) -> None:
        first = {
            "name": "dep-a",
            "version": "1.0.0",
            "id": "registry#dep-a@1.0.0",
            "source": "registry",
            "repository": None,
        }
        second = {
            "name": "dep-b",
            "version": "2.0.0",
            "id": "registry#dep-b@2.0.0",
            "source": "registry",
            "repository": None,
        }
        raw = {
            "crates": [{"package": first, "license": "MIT"}, {"package": second, "license": "MIT"}],
            "licenses": [{"id": "MIT", "text": "license text", "used_by": [{"crate": first}, {"crate": second}]}],
        }
        manifest, files = legal_generator.build_outputs(raw)
        self.assertEqual(len(manifest["packages"]), 2)
        self.assertEqual(len(manifest["licenseFiles"]), 2)
        self.assertEqual(
            set(files),
            {"LICENSES/third-party/dep-a-1.0.0.txt", "LICENSES/third-party/dep-b-2.0.0.txt"},
        )

    def test_generator_uses_spdx_suffix_for_multiple_selected_texts(self) -> None:
        package = {
            "name": "dep",
            "version": "1.0.0",
            "id": "registry#dep@1.0.0",
            "source": "registry",
            "repository": None,
        }
        raw = {
            "crates": [{"package": package, "license": "MIT AND Unicode-3.0"}],
            "licenses": [
                {"id": "MIT", "text": "MIT text", "used_by": [{"crate": package}]},
                {"id": "Unicode-3.0", "text": "Unicode text", "used_by": [{"crate": package}]},
            ],
        }
        _, files = legal_generator.build_outputs(raw)
        self.assertEqual(
            set(files),
            {"LICENSES/third-party/dep-1.0.0-MIT.txt", "LICENSES/third-party/dep-1.0.0-Unicode-3.0.txt"},
        )

    def test_generator_preserves_distinct_texts_with_the_same_spdx(self) -> None:
        package = {
            "name": "dep",
            "version": "1.0.0",
            "id": "registry#dep@1.0.0",
            "source": "registry",
            "repository": None,
        }
        texts = ["Copyright first\nMIT terms\n", "Copyright second\nMIT terms\n"]
        raw = {
            "crates": [{"package": package, "license": "MIT"}],
            "licenses": [{"id": "MIT", "text": text, "used_by": [{"crate": package}]} for text in [*texts, texts[0]]],
        }
        manifest, files = legal_generator.build_outputs(raw)
        self.assertEqual(set(files.values()), {text.encode() for text in texts})
        self.assertEqual(len(files), 2)
        self.assertEqual(legal_verifier.readable_license_path_errors(manifest), [])
        for name in files:
            self.assertRegex(name, r"dep-1\.0\.0-MIT-[0-9a-f]{12}\.txt$")

    def test_verifier_rejects_hash_only_license_filename(self) -> None:
        manifest = {
            "packages": [
                {
                    "name": "dep",
                    "version": "1.0.0",
                    "selectedLicenses": [{"spdxId": "MIT", "file": "LICENSES/third-party/abc123.txt"}],
                }
            ],
            "licenseFiles": [{"file": "LICENSES/third-party/abc123.txt"}],
        }
        self.assertTrue(legal_verifier.readable_license_path_errors(manifest))

    def test_generator_rejects_placeholder_license_text(self) -> None:
        package = {
            "name": "dep",
            "version": "1.0.0",
            "id": "registry#dep@1.0.0",
            "source": "registry",
            "repository": None,
        }
        raw = {
            "crates": [{"package": package, "license": "MIT"}],
            "licenses": [
                {"id": "MIT", "text": "Copyright (c) <year> <copyright holders>", "used_by": [{"crate": package}]}
            ],
        }
        with self.assertRaises(ValueError):
            legal_generator.build_outputs(raw)

    def test_generator_preserves_upstream_declaration_when_selection_is_clarified(self) -> None:
        package = {
            "name": "dep",
            "version": "1.0.0",
            "id": "registry#dep@1.0.0",
            "source": "registry",
            "repository": None,
            "license": "MIT OR Apache-2.0",
        }
        raw = {
            "crates": [{"package": package, "license": "MIT"}],
            "licenses": [{"id": "MIT", "text": "Copyright (c) Example\nMIT terms", "used_by": [{"crate": package}]}],
        }
        manifest, _ = legal_generator.build_outputs(raw)
        self.assertEqual(manifest["packages"][0]["declaredLicense"], "MIT OR Apache-2.0")

    def test_license_write_keeps_files_when_candidate_generation_fails(self):
        import os
        import shutil
        import subprocess

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for relative in (
                "tooling/qa/wrappers/licenses.sh",
                "tooling/release/legal/generate_dependency_licenses.py",
            ):
                destination = root / relative
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(ROOT / relative, destination)
            runtime = root / "tooling/qa/runtime"
            runtime.mkdir(parents=True)
            (runtime / "lib.sh").write_text("""ensure_rust_env() { :; }
require_command() { command -v "$1" >/dev/null; }
qa_init() { SNIPTALE_QA_RUN_ID=fixture; QA_FAILED_STEPS=(); }
qa_step() { local label="$1"; shift; if "$@"; then :; else QA_FAILED_STEPS+=("$label"); fi; }
qa_fail_if_any() { if [[ ${#QA_FAILED_STEPS[@]} -gt 0 ]]; then exit 1; fi; }
""")
            binaries = root / "bin"
            binaries.mkdir()
            about = binaries / "cargo-about"
            about.write_text(
                "#!/usr/bin/env python3\nimport pathlib,sys\npathlib.Path(sys.argv[sys.argv.index('--output-file')+1]).write_text('{bad')\n"
            )
            about.chmod(0o700)
            preserved = root / "LICENSES/third-party/existing.txt"
            preserved.parent.mkdir(parents=True)
            preserved.write_bytes(b"exact upstream bytes\r\n")
            result = subprocess.run(
                ["bash", str(root / "tooling/qa/wrappers/licenses.sh"), "--write"],
                env={**os.environ, "PATH": str(binaries) + os.pathsep + os.environ["PATH"]},
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(preserved.read_bytes(), b"exact upstream bytes\r\n")

    def test_contribution_policy_requires_machine_false(self) -> None:
        errors = legal_verifier.contribution_policy_errors(
            {"project": {"contributionsAccepted": True}},
            "Sniptale Desktop does not currently accept external code contributions.",
        )
        self.assertTrue(errors)

    def test_sbom_uses_exact_redistribution_closure(self) -> None:
        root = {"id": "root", "name": "sniptale-native-agent", "version": "0.1.0", "source": None, "license": "MIT"}
        direct = {"id": "direct", "name": "direct-dep", "version": "2.0.0", "source": "registry", "license": "MIT"}
        transitive = {
            "id": "transitive",
            "name": "transitive-dep",
            "version": "2.1.0",
            "source": "registry",
            "license": "MIT",
        }
        dev = {"id": "dev", "name": "dev-dep", "version": "3.0.0", "source": "registry", "license": "MIT"}
        metadata = {
            "workspace_members": ["root"],
            "packages": [root, direct, transitive, dev],
            "resolve": {
                "nodes": [
                    {"id": "root", "deps": [{"pkg": "direct"}]},
                    {"id": "direct", "deps": [{"pkg": "transitive"}]},
                    {"id": "transitive", "deps": []},
                    {"id": "dev", "deps": []},
                ]
            },
        }
        manifest = {
            "packages": [
                {"name": "direct-dep", "version": "2.0.0", "source": "registry", "declaredLicense": "MIT"},
                {"name": "transitive-dep", "version": "2.1.0", "source": "registry", "declaredLicense": "MIT"},
            ]
        }
        sbom = sbom_generator.build_sbom(metadata, manifest, "digest", "revision", "2026-01-01T00:00:00Z")
        self.assertEqual(
            [package["name"] for package in sbom["packages"]], ["sniptale-native-agent", "direct-dep", "transitive-dep"]
        )
        ids = {package["name"]: package["SPDXID"] for package in sbom["packages"]}
        edges = {
            (item["spdxElementId"], item["relatedSpdxElement"])
            for item in sbom["relationships"]
            if item["relationshipType"] == "DEPENDS_ON"
        }
        self.assertIn((ids["sniptale-native-agent"], ids["direct-dep"]), edges)
        self.assertIn((ids["direct-dep"], ids["transitive-dep"]), edges)
        self.assertNotIn((ids["sniptale-native-agent"], ids["transitive-dep"]), edges)
        self.assertIn("/revision/digest", sbom["documentNamespace"])

    def test_sbom_rejects_manifest_package_outside_metadata(self) -> None:
        root = {"id": "root", "name": "sniptale-native-agent", "version": "0.1.0", "source": None, "license": "MIT"}
        metadata = {
            "workspace_members": ["root"],
            "packages": [root],
            "resolve": {"nodes": [{"id": "root", "deps": []}]},
        }
        manifest = {
            "packages": [{"name": "missing", "version": "1.0.0", "source": "registry", "declaredLicense": "MIT"}]
        }
        with self.assertRaises(ValueError):
            sbom_generator.build_sbom(metadata, manifest, "digest", "revision", "2026-01-01T00:00:00Z")


class WebLicenseTests(unittest.TestCase):
    def fixture(self, root: Path) -> Path:
        ui = root / "ui"
        package = ui / "node_modules/probe"
        package.mkdir(parents=True)
        (ui / "package.json").write_text(json.dumps({"dependencies": {"probe": "1.0.0"}}))
        lock = {
            "lockfileVersion": 3,
            "packages": {
                "": {"dependencies": {"probe": "1.0.0"}},
                "node_modules/probe": {
                    "version": "1.0.0",
                    "resolved": "https://registry.npmjs.org/probe/-/probe-1.0.0.tgz",
                    "integrity": "sha512-fixture",
                },
                "node_modules/dev-only": {"dev": True},
            },
        }
        (ui / "package-lock.json").write_text(json.dumps(lock))
        (package / "package.json").write_text(json.dumps({"name": "probe", "version": "1.0.0", "license": "MIT"}))
        (package / "LICENSE").write_bytes(b"Copyright (c) Probe\r\nPermission is granted\r\n")
        font = ui / "src/styles/fonts/golos-ui"
        font.mkdir(parents=True)
        (font / "golos-ui-vf.woff2").write_bytes(b"font fixture")
        (font / "LICENSE.txt").write_bytes(b"Font copyright\r\nSIL terms\r\n")
        return package

    def test_git_staging_preserves_the_upstream_font_license_bytes(self):
        path = "ui/src/styles/fonts/golos-ui/LICENSE.txt"
        filtered = subprocess.check_output(["git", "hash-object", "--path=" + path, path], cwd=ROOT)
        original = subprocess.check_output(["git", "hash-object", "--no-filters", path], cwd=ROOT)
        self.assertEqual(filtered, original, "Git text conversion would change the upstream font license")

    def test_production_notices_preserve_exact_bytes_and_exclude_dev_packages(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            package = self.fixture(root)
            web = legal_generator.web_license_inputs(root)
            self.assertEqual([entry["package"]["name"] for entry in web["crates"]], ["probe", "golos-ui"])
            _, files = legal_generator.build_outputs(web)
            self.assertEqual(files["LICENSES/third-party/probe-1.0.0.txt"], (package / "LICENSE").read_bytes())
            notices = legal_generator.render_notices(web)
            self.assertIn((package / "LICENSE").read_bytes(), notices)
            self.assertIn(b"Font copyright\r\nSIL terms\r\n", notices)
            web["licenses"][0]["text"] = "Copyright (c) <year> <owner>"
            with self.assertRaises(ValueError):
                legal_generator.render_notices(web)

    def test_changed_identity_missing_text_and_package_escape_fail_closed(self):
        for defect in ("identity", "missing-license", "escaped-license", "manifest", "unsupported-license"):
            with self.subTest(defect=defect), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                package = self.fixture(root)
                metadata = json.loads((package / "package.json").read_text())
                if defect == "identity":
                    metadata["version"] = "2.0.0"
                elif defect == "unsupported-license":
                    metadata["license"] = "Unreviewed-License"
                elif defect in ("missing-license", "escaped-license"):
                    (package / "LICENSE").unlink()
                    if defect == "escaped-license":
                        (root / "outside.txt").write_text("outside license")
                        (package / "LICENSE").symlink_to(root / "outside.txt")
                else:
                    (root / "ui/package.json").write_text(json.dumps({"dependencies": {"probe": "2.0.0"}}))
                (package / "package.json").write_text(json.dumps(metadata))
                with self.assertRaises(ValueError):
                    legal_generator.web_license_inputs(root)

    def test_sbom_includes_web_and_font_without_weakening_cargo_resolution(self):
        root = {"id": "root", "name": "sniptale-native-agent", "version": "0.1.1", "source": None, "license": "MIT"}
        metadata = {
            "workspace_members": ["root"],
            "packages": [root],
            "resolve": {"nodes": [{"id": "root", "deps": []}]},
        }
        manifest = {
            "packages": [
                {
                    "name": "react",
                    "version": "19.3.1",
                    "source": "https://registry.npmjs.org/react/-/react-19.3.1.tgz",
                    "declaredLicense": "MIT",
                },
                {
                    "name": "golos-ui",
                    "version": "font-hash",
                    "source": "bundled-font:font-hash",
                    "declaredLicense": "OFL-1.1",
                },
            ]
        }
        sbom = sbom_generator.build_sbom(metadata, manifest, "digest", "revision", "2026-01-01T00:00:00Z")
        self.assertEqual(
            [package["name"] for package in sbom["packages"]], ["sniptale-native-agent", "react", "golos-ui"]
        )
        self.assertEqual(sbom["packages"][-1]["downloadLocation"], "NOASSERTION")
        self.assertEqual(sum(item["relationshipType"] == "CONTAINS" for item in sbom["relationships"]), 2)
        manifest["packages"].append(
            {"name": "missing-rust", "version": "1.0.0", "source": "registry", "declaredLicense": "MIT"}
        )
        with self.assertRaises(ValueError):
            sbom_generator.build_sbom(metadata, manifest, "digest", "revision", "2026-01-01T00:00:00Z")


if __name__ == "__main__":
    unittest.main()
