# OSS provenance

## Project material

`LICENSE` contains the MIT text copied from the Open Source Initiative template and then updated with the project copyright. `NOTICE` is the project attribution owner: Sniptale Desktop, Copyright (c) 2026 Lev Rozhkov. `Cargo.toml` declares `MIT`. `tooling/configs/qa/open-source.json` pins the source URL, template digest, final legal-file digests, project identity, and contribution policy.

Source, documentation, Windows resources, installer definitions, and repository tooling are treated as project-authored unless a checked-in provenance entry states otherwise. Git history supports authorship but does not replace the legal files.

## Dependency closure

`cargo-deny` audits each configured platform graph separately for advisories, license expressions, sources, and bans. Separate graphs prevent impossible dependency paths between Windows-only Tauri and Linux-only GTK. No advisory is ignored. Only Tauri may introduce its internal Tokio runtime. Product owners cannot depend on Tokio directly. `cargo-audit` audits RustSec advisories. `cargo-about` builds the non-dev Windows redistribution notice closure from `Cargo.lock`, local crate sources, `about.toml`, and the tracked template without ClearlyDefined or another mutable license catalog.

The legal generator combines the Rust notice closure with production packages from `ui/package-lock.json`. It checks installed package identities before copying their license text. Exact Fluent npm clarifications use the pinned canonical Apache text because those three archives omit it. The Golos UI font uses its content digest as its version identity. Its upstream SIL license remains unchanged.

`THIRD_PARTY_DEPENDENCIES.json`, `THIRD_PARTY_NOTICES.md`, and package-owned `LICENSES/third-party/` texts are generator-owned and tracked.

Exact crate clarifications preserve upstream license files that automatic detection misses. Missing archive files use the published VCS revision and a SHA-256 digest. Generation permits network retrieval with locked dependencies. The manifest, package-owned texts, and notice bundle use the same validated cargo-about JSON. The generator adds the locked web and font records before rendering. Unresolved templates fail generation. The complete canonical Apache-2.0 appendix is recognized by its full text digest after whitespace normalization. Generated files retain every upstream byte.

A dependency with multiple selected license texts adds the SPDX identifier to each filename. Distinct texts with the same identifier also add their content-hash prefix. Identical text occurrences share one exact-byte file for that package. `tooling/qa/wrappers/licenses.sh --write` is their only update path. Normal checks regenerate into `.tmp/**` and require byte equality.

SHA-256 digests remain in the manifest. `.gitattributes` disables text conversion for exact upstream license texts and the generated notice bundle. Repository formatters do not rewrite them.

The release SPDX SBOM and raw cargo-about JSON are ignored evidence. They are regenerated from the reviewed lockfiles and do not replace Git policy.

The alpha also links the MSVC CRT supplied by the build toolchain. Its compiler version and static-link flags are recorded in `build-environment.json`. This compiler runtime is outside the Cargo notice/SBOM graph and remains subject to [Microsoft's toolchain redistribution terms](https://learn.microsoft.com/en-us/visualstudio/releases/2022/redistribution). The [threat model](../security/threat-model.md) owns its update trade-off.

## Publication scope

This repository contains the Windows desktop/native-host component only. QA and release proof use the same repository commands locally and in allowlisted GitHub Actions workflows. The local proof wrappers do not publish. The trusted alpha publication workflow admits an existing package and Windows acceptance, verifies draft asset hashes, and publishes a GitHub prerelease. [Release procedure](release.md) owns that operation. External code contributions are disabled by `CONTRIBUTING.md` and machine policy.

The alpha QA handoff includes the built web entrypoint, script, stylesheet, and referenced fonts. Its existing commit identity and file hashes cover these assets. Admission rejects missing, unexpected, changed, or redirected files. The Windows builder copies verified assets before Cargo embeds them. It does not rebuild the interface with a different toolchain.

## Repository skills

The repository adapts architecture, security, topology, and audit review practices from the companion browser repository. Desktop owner names, Rust proof, Windows trust, and local wrappers govern these adaptations. The copied workflow does not authorize an external review service.

The asd-ste100 adaptation includes its original [MIT license](../../.agents/skills/asd-ste100/LICENSE) without byte changes. Its source skill version is 0.4.0, by Dustin Yuchen Teng. The bundled checker retains upstream structural heuristics with local formatting and a type-narrowing guard. The repository owns the integration, concise instructions, and paraphrased references. It does not redistribute ASD's official dictionary or claim certified compliance.


## Analysis tools

jscpd and CodeQL are installed as verified QA tools. They do not enter the product Cargo dependency or redistribution closure. Their installed bundles retain upstream legal files. CodeQL use follows the [GitHub CodeQL terms](https://github.com/github/codeql-cli-binaries/blob/main/LICENSE.md). The common tool catalog owns versions and archive hashes. The [QA guide](../tooling/qa.md) owns release execution and the finding registry.
