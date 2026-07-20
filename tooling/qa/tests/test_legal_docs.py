from __future__ import annotations

import importlib.util
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
    def test_generator_writes_readable_package_owned_files(self) -> None:
        first = {"name": "dep-a", "version": "1.0.0", "id": "registry#dep-a@1.0.0", "source": "registry", "repository": None}
        second = {"name": "dep-b", "version": "2.0.0", "id": "registry#dep-b@2.0.0", "source": "registry", "repository": None}
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
        package = {"name": "dep", "version": "1.0.0", "id": "registry#dep@1.0.0", "source": "registry", "repository": None}
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

    def test_verifier_rejects_hash_only_license_filename(self) -> None:
        manifest = {
            "packages": [{"name": "dep", "version": "1.0.0", "selectedLicenses": [{"spdxId": "MIT", "file": "LICENSES/third-party/abc123.txt"}]}],
            "licenseFiles": [{"file": "LICENSES/third-party/abc123.txt"}],
        }
        self.assertTrue(legal_verifier.readable_license_path_errors(manifest))

    def test_generator_rejects_placeholder_license_text(self) -> None:
        package = {"name": "dep", "version": "1.0.0", "id": "registry#dep@1.0.0", "source": "registry", "repository": None}
        raw = {"crates": [{"package": package, "license": "MIT"}], "licenses": [{"id": "MIT", "text": "Copyright (c) <year> <copyright holders>", "used_by": [{"crate": package}]}]}
        with self.assertRaises(ValueError):
            legal_generator.build_outputs(raw)

    def test_generator_preserves_upstream_declaration_when_selection_is_clarified(self) -> None:
        package = {"name": "dep", "version": "1.0.0", "id": "registry#dep@1.0.0", "source": "registry", "repository": None, "license": "MIT OR Apache-2.0"}
        raw = {"crates": [{"package": package, "license": "MIT"}], "licenses": [{"id": "MIT", "text": "Copyright (c) Example\nMIT terms", "used_by": [{"crate": package}]}]}
        manifest, _ = legal_generator.build_outputs(raw)
        self.assertEqual(manifest["packages"][0]["declaredLicense"], "MIT OR Apache-2.0")

    def test_contribution_policy_requires_machine_false(self) -> None:
        errors = legal_verifier.contribution_policy_errors(
            {"project": {"contributionsAccepted": True}},
            "Sniptale Desktop does not currently accept external code contributions.",
        )
        self.assertTrue(errors)

    def test_sbom_uses_exact_redistribution_closure(self) -> None:
        root = {"id": "root", "name": "sniptale-native-agent", "version": "0.1.0", "source": None, "license": "MIT"}
        direct = {"id": "direct", "name": "direct-dep", "version": "2.0.0", "source": "registry", "license": "MIT"}
        transitive = {"id": "transitive", "name": "transitive-dep", "version": "2.1.0", "source": "registry", "license": "MIT"}
        dev = {"id": "dev", "name": "dev-dep", "version": "3.0.0", "source": "registry", "license": "MIT"}
        metadata = {
            "workspace_members": ["root"],
            "packages": [root, direct, transitive, dev],
            "resolve": {"nodes": [
                {"id": "root", "deps": [{"pkg": "direct"}]},
                {"id": "direct", "deps": [{"pkg": "transitive"}]},
                {"id": "transitive", "deps": []},
                {"id": "dev", "deps": []},
            ]},
        }
        manifest = {"packages": [
            {"name": "direct-dep", "version": "2.0.0", "source": "registry", "declaredLicense": "MIT"},
            {"name": "transitive-dep", "version": "2.1.0", "source": "registry", "declaredLicense": "MIT"},
        ]}
        sbom = sbom_generator.build_sbom(metadata, manifest, "digest", "revision", "2026-01-01T00:00:00Z")
        self.assertEqual([package["name"] for package in sbom["packages"]], ["sniptale-native-agent", "direct-dep", "transitive-dep"])
        ids = {package["name"]: package["SPDXID"] for package in sbom["packages"]}
        edges = {(item["spdxElementId"], item["relatedSpdxElement"]) for item in sbom["relationships"] if item["relationshipType"] == "DEPENDS_ON"}
        self.assertIn((ids["sniptale-native-agent"], ids["direct-dep"]), edges)
        self.assertIn((ids["direct-dep"], ids["transitive-dep"]), edges)
        self.assertNotIn((ids["sniptale-native-agent"], ids["transitive-dep"]), edges)
        self.assertIn("/revision/digest", sbom["documentNamespace"])

    def test_sbom_rejects_manifest_package_outside_metadata(self) -> None:
        root = {"id": "root", "name": "sniptale-native-agent", "version": "0.1.0", "source": None, "license": "MIT"}
        metadata = {"workspace_members": ["root"], "packages": [root], "resolve": {"nodes": [{"id": "root", "deps": []}]}}
        manifest = {"packages": [{"name": "missing", "version": "1.0.0", "source": "registry", "declaredLicense": "MIT"}]}
        with self.assertRaises(ValueError):
            sbom_generator.build_sbom(metadata, manifest, "digest", "revision", "2026-01-01T00:00:00Z")


if __name__ == "__main__":
    unittest.main()
