#!/usr/bin/env python3
from __future__ import annotations

import copy
import importlib.util
import json
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ET
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]
WIX_SOURCE = ROOT / "installer/wix/SniptaleNativeAgent.wxs"
PACKAGE_WINDOWS_WRAPPER = ROOT / "tooling/release/windows/package.sh"
WINDOWS_SMOKE_SCRIPT = ROOT / "tooling/release/windows/smoke/windows-smoke.ps1"
WINDOWS_SMOKE_SHELL = ROOT / "tooling/release/windows/smoke/windows-smoke.sh"
WINDOWS_SMOKE_FROM_WSL = ROOT / "tooling/release/windows/smoke/windows-smoke-from-wsl.sh"
WINDOWS_TRAY_QUICK = ROOT / "tooling/release/windows/smoke/windows-tray-quick.ps1"
INSTALLER_README = ROOT / "installer/README.md"
MANUAL_QA_CHECKLIST = ROOT / "docs/oss/release.md"
MANUAL_QA_EVIDENCE_TEMPLATE = ROOT / "tooling/release/windows/windows-release-evidence-template.json"
LOGGING_RUNBOOK = ROOT / "docs/operations/diagnostics.md"
DEVELOPER_RUNBOOK = ROOT / "docs/tooling/qa.md"
WINDOWS_RELEASE_EVIDENCE_VALIDATOR = ROOT / "tooling/release/windows/verify-windows-release-evidence.py"
WINDOWS_PE_RESOURCE_VERIFIER = ROOT / "tooling/release/windows/verify_windows_pe_resources.py"
WINDOWS_MSI_IDENTITY_VERIFIER = ROOT / "tooling/release/windows/verify_msi_identity.ps1"
REGISTRY_SOURCE = ROOT / "src/install/registry.rs"
REGISTRY_MANIFEST_SOURCE = ROOT / "src/install/registry/manifest.rs"
IDENTITY_SOURCE = ROOT / "src/install/identity/mod.rs"
REGISTRATION_FACTS_SOURCE = ROOT / "src/install/registration_facts.rs"
MANIFEST_TEMPLATES = {
    "stable": ("com.sniptale.native_host", "{{SNIPTALE_STABLE_EXTENSION_ORIGIN}}"),
    "beta": ("com.sniptale.beta.native_host", "{{SNIPTALE_BETA_EXTENSION_ORIGIN}}"),
    "dev": ("com.sniptale.dev.native_host", "{{SNIPTALE_DEV_EXTENSION_ORIGIN}}"),
}
WIX_NS = {"w": "http://wixtoolset.org/schemas/v4/wxs"}
EXPECTED_UPGRADE_CODE = "{BC80D869-3328-47B1-BC08-CC5E0FA05961}"
EXPECTED_COMPONENT_GUIDS = {
    "NativeAgentExecutable": "{03183BD5-775C-4EB4-981F-EFB47CCBE72C}",
    "NativeHostManifestFile": "{8595CD6F-6457-4543-8C45-90F8AD696405}",
    "ProjectLicenseFile": "{C8C2911C-844C-4A1F-8A47-57C9EE9B7C48}",
    "ProjectNoticeFile": "{D65E7844-4811-4133-8948-E8BCEF60B7F0}",
    "ThirdPartyNoticesMarkdownFile": "{AD1936C4-9944-43AC-A6F0-17726ED05B19}",
    "ThirdPartyDependenciesFile": "{C8367801-AE20-42A7-B4CC-A68F74D662E1}",
    "AutostartRegistration": "{25DE16B5-3992-4D10-A465-8649BAF30582}",
    "ChromeNativeHostRegistration": "{4524E8EA-998C-4B26-A0D6-0070423736D7}",
    "EdgeNativeHostRegistration": "{221B8686-241A-4EB9-8E4D-85549F3679B9}",
    "ChromiumNativeHostRegistration": "{F2641735-F503-4072-BBC5-C8B57ABAC13E}",
}


def fail(messages: list[str]) -> None:
    print("FAIL release artifact guardrails")
    for message in messages:
        print(f"- {message}")
    raise SystemExit(1)


def rel(path: Path) -> str:
    return path.relative_to(ROOT).as_posix()


def require_files(errors: list[str]) -> None:
    required = [
        WIX_SOURCE,
        PACKAGE_WINDOWS_WRAPPER,
        WINDOWS_SMOKE_SCRIPT,
        WINDOWS_SMOKE_SHELL,
        WINDOWS_SMOKE_FROM_WSL,
        WINDOWS_TRAY_QUICK,
        INSTALLER_README,
        MANUAL_QA_CHECKLIST,
        MANUAL_QA_EVIDENCE_TEMPLATE,
        LOGGING_RUNBOOK,
        DEVELOPER_RUNBOOK,
        WINDOWS_RELEASE_EVIDENCE_VALIDATOR,
        WINDOWS_PE_RESOURCE_VERIFIER,
        WINDOWS_MSI_IDENTITY_VERIFIER,
        REGISTRY_SOURCE,
        REGISTRY_MANIFEST_SOURCE,
        IDENTITY_SOURCE,
        REGISTRATION_FACTS_SOURCE,
    ]
    required.extend(
        ROOT / f"installer/native-host/{channel}.json.template"
        for channel in MANIFEST_TEMPLATES
    )
    missing = [rel(path) for path in required if not path.is_file()]
    if missing:
        errors.append("missing release artifact files: " + ", ".join(missing))


def check_wix(errors: list[str]) -> None:
    if not WIX_SOURCE.is_file():
        return
    root = ET.parse(WIX_SOURCE).getroot()
    package = root.find("w:Package", WIX_NS)
    if package is None:
        errors.append("WiX source must define a Package")
        return
    if package.attrib.get("Scope") != "perUser":
        errors.append("WiX Package must be per-user scoped")
    if package.attrib.get("Version") != "$(var.ProductVersion)":
        errors.append("WiX Package version must use ProductVersion variable")
    if package.attrib.get("Name") != "Sniptale Desktop" or package.attrib.get("Manufacturer") != "Sniptale":
        errors.append("WiX Package must use the exact Sniptale product and manufacturer")
    if package.attrib.get("UpgradeCode") != EXPECTED_UPGRADE_CODE:
        errors.append("WiX Package must use the pinned Sniptale UpgradeCode")
    wix_ui = root.find("w:Package/ui:WixUI", {**WIX_NS, "ui": "http://wixtoolset.org/schemas/v4/wxs/ui"})
    if wix_ui is None:
        errors.append("WiX source must use WixToolset.UI for valid interactive installer UI")
    elif wix_ui.attrib.get("Id") != "WixUI_InstallDir":
        errors.append("WiX source must use WixUI_InstallDir for interactive installer UI")
    error_progress = root.find("w:Package/w:UIRef[@Id='WixUI_ErrorProgressText']", WIX_NS)
    if error_progress is None:
        errors.append("WiX source must include WixUI_ErrorProgressText")
    major_upgrade = root.find("w:Package/w:MajorUpgrade", WIX_NS)
    if major_upgrade is None:
        errors.append("WiX Package must define MajorUpgrade")
    elif major_upgrade.attrib.get("AllowSameVersionUpgrades") != "yes":
        errors.append(
            "WiX MajorUpgrade must allow same-version upgrades for rebuilt package payloads"
        )

    files = {file.attrib.get("Id"): file.attrib for file in root.findall(".//w:File", WIX_NS)}
    if files.get("NativeAgentExe", {}).get("Source") != "$(var.NativeAgentExe)":
        errors.append("WiX must install NativeAgentExe from NativeAgentExe variable")
    if files.get("NativeHostManifest", {}).get("Source") != "$(var.NativeHostManifest)":
        errors.append("WiX must install native-host.json from NativeHostManifest variable")
    legal_files = {
        "ProjectLicense": ("$(var.ProjectLicense)", "LICENSE.txt"),
        "ProjectNotice": ("$(var.ProjectNotice)", "NOTICE.txt"),
        "ThirdPartyNotices": ("$(var.ThirdPartyNotices)", "THIRD_PARTY_NOTICES.md"),
        "ThirdPartyDependencies": ("$(var.ThirdPartyDependencies)", "THIRD_PARTY_DEPENDENCIES.json"),
    }
    for file_id, (source, name) in legal_files.items():
        if files.get(file_id, {}).get("Source") != source or files.get(file_id, {}).get("Name") != name:
            errors.append(f"WiX must install {name} from {source}")
    components = {
        component.attrib.get("Id"): component.attrib.get("Guid")
        for component in root.findall(".//w:Component", WIX_NS)
    }
    if components != EXPECTED_COMPONENT_GUIDS:
        errors.append("WiX component identities must match all ten pinned Sniptale GUIDs")
    if root.findall(".//w:RemoveFile", WIX_NS):
        errors.append("WiX clean-break package must not remove files from another product layout")
    icon = root.find("w:Package/w:Icon[@Id='SniptaleAppIcon']", WIX_NS)
    if icon is None or icon.attrib.get("SourceFile") != "$(var.AppIcon)":
        errors.append("WiX must use the tracked Sniptale app icon input")
    arp_icon = root.find("w:Package/w:Property[@Id='ARPPRODUCTICON']", WIX_NS)
    if arp_icon is None or arp_icon.attrib.get("Value") != "SniptaleAppIcon":
        errors.append("WiX must expose SniptaleAppIcon as ARPPRODUCTICON")

    registry_values = [value.attrib for value in root.findall(".//w:RegistryValue", WIX_NS)]
    autostart = [
        value
        for value in registry_values
        if value.get("Root") == "HKCU"
        and value.get("Key") == r"Software\Microsoft\Windows\CurrentVersion\Run"
        and value.get("Name") == "SniptaleNativeAgent"
    ]
    if not autostart:
        errors.append("WiX must register HKCU Run autostart for resident agent")
    elif "--native-host" in autostart[0].get("Value", ""):
        errors.append("WiX autostart must not launch native-host mode")
    properties = {prop.attrib.get("Id"): prop.attrib for prop in root.findall(".//w:Property", WIX_NS)}
    if properties.get("SNIPTALE_START_WITH_WINDOWS", {}).get("Value") != "1":
        errors.append("WiX must expose enabled-by-default SNIPTALE_START_WITH_WINDOWS property")
    checkbox = root.find(".//w:Control[@Id='StartWithWindowsCheckBox']", WIX_NS)
    if checkbox is None or checkbox.attrib.get("Property") != "SNIPTALE_START_WITH_WINDOWS":
        errors.append("WiX must expose a StartWithWindowsCheckBox bound to SNIPTALE_START_WITH_WINDOWS")
    autostart_component = root.find(".//w:Component[@Id='AutostartRegistration']", WIX_NS)
    autostart_condition = (
        autostart_component.attrib.get("Condition", "")
        if autostart_component is not None
        else ""
    )
    if 'SNIPTALE_START_WITH_WINDOWS = "1"' not in autostart_condition:
        errors.append("WiX autostart component must be conditioned on SNIPTALE_START_WITH_WINDOWS")
    if REGISTRATION_FACTS_SOURCE.is_file():
        facts_text = REGISTRATION_FACTS_SOURCE.read_text(encoding="utf-8")
        if 'AUTOSTART_VALUE_NAME: &str = "SniptaleNativeAgent"' not in facts_text:
            errors.append("runtime autostart value name must match WiX SniptaleNativeAgent")

    required_browser_keys = {
        r"Software\Google\Chrome\NativeMessagingHosts\$(var.NativeHostName)",
        r"Software\Microsoft\Edge\NativeMessagingHosts\$(var.NativeHostName)",
        r"Software\Chromium\NativeMessagingHosts\$(var.NativeHostName)",
    }
    present_keys = {
        value.get("Key")
        for value in registry_values
        if value.get("Root") == "HKCU" and value.get("Value") == "[INSTALLFOLDER]native-host.json"
    }
    missing_keys = sorted(required_browser_keys - present_keys)
    if missing_keys:
        errors.append("WiX missing browser Native Messaging keys: " + ", ".join(missing_keys))


def check_manifest_templates(errors: list[str]) -> None:
    for channel, (host_name, origin_token) in MANIFEST_TEMPLATES.items():
        path = ROOT / f"installer/native-host/{channel}.json.template"
        if not path.is_file():
            continue
        data = json.loads(path.read_text(encoding="utf-8"))
        if data.get("name") != host_name:
            errors.append(f"{rel(path)} has wrong host name")
        if data.get("type") != "stdio":
            errors.append(f"{rel(path)} must use stdio manifest type")
        if data.get("sniptale_native_host_manifest_version") != "0.1.0":
            errors.append(f"{rel(path)} has wrong manifest version")
        if data.get("path") != "{{SNIPTALE_NATIVE_AGENT_EXE}}":
            errors.append(f"{rel(path)} path must use executable token")
        origins = data.get("allowed_origins")
        if origins != [origin_token]:
            errors.append(f"{rel(path)} must use its channel-specific origin token")
        if any("*" in str(origin) or "unpacked" in str(origin).lower() for origin in origins or []):
            errors.append(f"{rel(path)} must not allow wildcard or unpacked origins")


def check_clean_break_identity_guards(errors: list[str]) -> None:
    tooling_guard = ROOT / "tooling/qa/rules/verify_tooling.py"
    if tooling_guard.is_file():
        text = tooling_guard.read_text(encoding="utf-8")
        for term in (
            "retired_product_roots",
            "retired_identity_matches",
            'git", "ls-files", "-co", "--exclude-standard", "-z"',
            "validate_clean_break_identity(errors)",
        ):
            if term not in text:
                errors.append(f"repository clean-break identity guard is missing {term}")

    if WINDOWS_PE_RESOURCE_VERIFIER.is_file():
        spec = importlib.util.spec_from_file_location(
            "verify_windows_pe_resources_identity_test", WINDOWS_PE_RESOURCE_VERIFIER
        )
        if spec is None or spec.loader is None:
            errors.append("could not load Windows PE identity verifier")
        else:
            module = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(module)
            roots = module.retired_product_roots()
            if len(roots) != 4:
                errors.append("Windows PE identity verifier must construct four retired roots")
            for root in roots:
                payload = f"metadata-prefix-{root.upper()}-suffix".encode("ascii")
                if not module.retired_identity_strings(Path("Sniptale.exe"), payload):
                    errors.append("Windows PE identity verifier did not reject a retired metadata root")
            if module.retired_identity_strings(Path("Sniptale.exe"), b"Sniptale Desktop"):
                errors.append("Windows PE identity verifier rejected the current product identity")

    if WINDOWS_MSI_IDENTITY_VERIFIER.is_file():
        text = WINDOWS_MSI_IDENTITY_VERIFIER.read_text(encoding="utf-8")
        for term in (
            "Get-RetiredProductRoots",
            "Assert-CleanIdentityValue",
            "SELECT * FROM",
            'SELECT `UpgradeCode` FROM `Upgrade`',
            EXPECTED_UPGRADE_CODE,
            "SniptaleNativeAgent-(stable|beta|dev)",
        ):
            if term not in text:
                errors.append(f"Windows MSI identity verifier is missing {term}")


def check_package_wrapper(errors: list[str]) -> None:
    if not PACKAGE_WINDOWS_WRAPPER.is_file():
        return
    text = PACKAGE_WINDOWS_WRAPPER.read_text(encoding="utf-8")
    required_terms = [
        "tooling/qa/wrappers/checkpoint.sh",
        "qa_assert_state checkpoint",
        "cargo build --locked --release --target x86_64-pc-windows-msvc",
        "sniptale-native-agent.exe",
        "installer/native-host/stable.json.template",
        "installer/native-host/beta.json.template",
        "installer/native-host/dev.json.template",
        "extension_origin_regex='^chrome-extension://[a-p]{32}/$'",
        'SNIPTALE_INSTALL_CHANNEL="$channel"',
        'SNIPTALE_ALLOWED_EXTENSION_ORIGIN="$extension_origin"',
        "NativeAgentExe=",
        "verify_windows_pe_resources.py",
        "verify Windows executable resources",
        "verify_msi_identity.ps1",
        "verify Windows MSI identity",
        "NativeHostManifest=",
        "ProjectLicense=",
        "ProjectNotice=",
        "ThirdPartyNotices=",
        "ThirdPartyDependencies=",
        "tooling/qa/wrappers/licenses.sh --check",
        "bundle legal payload",
        "NativeHostName=",
        "ProductVersion=",
        "wix",
        "extension list",
        "extension add -g",
        "WixToolset.UI.wixext/$wix_version",
        "-ext WixToolset.UI.wixext",
        "SniptaleNativeAgent-$channel-$product_version.msi",
        "--signing-mode unsigned|production",
        "--signing-cert-thumbprint",
        "--timestamp-url",
        "Stable Windows packaging requires --signing-mode production",
        "Set-AuthenticodeSignature",
        "Get-AuthenticodeSignature",
        "release-evidence.json",
        "SHA256SUMS",
        "provenance.json",
        "Missing release SBOM",
        "Missing cargo audit release report",
        "Missing cargo deny release report",
        "Stale release SBOM",
        "Stale supply-chain reports",
        "signature-exe.json",
        "signature-msi.json",
        "timestampPresent",
        "revocationChecked",
        "revocationStatus",
        "revocationRequiredWhenSupported",
        "--manual-qa-evidence",
        "SNIPTALE_WINDOWS_RELEASE_EVIDENCE",
        "verify-windows-release-evidence.py",
        "windows-release-qa-evidence.json",
        "manualQaEvidence",
        "manual-windows-release-evidence",
        "expectedSignerThumbprint",
        "dependencyFingerprint",
        "checkpointFingerprint",
        "worktreeFingerprint",
        "release-provenance",
        "cargoAuditSha256",
        "cargoDenySha256",
        "legalPayload",
        "productionSigned",
        "write release provenance",
        'write_named_sha256 "$evidence_path" "$(basename "$evidence_path")"',
        "sbom.spdx.json",
        "cargo-audit.json",
        "cargo-deny.txt",
    ]
    missing = [term for term in required_terms if term not in text]
    if missing:
        errors.append("package-windows wrapper missing terms: " + ", ".join(missing))

    forbidden_terms = [
        "--native-agent-exe",
        "native_agent_exe=\"\"",
        "[[ -z \"$native_agent_exe\" ]]",
        "[[ ! -f \"$native_agent_exe\" ]]",
        "existing executable",
    ]
    present_forbidden = [term for term in forbidden_terms if term in text]
    if present_forbidden:
        errors.append(
            "package-windows wrapper must not allow prebuilt executable bypass: "
            + ", ".join(present_forbidden)
        )

    if REGISTRY_SOURCE.is_file():
        registry_text = REGISTRY_SOURCE.read_text(encoding="utf-8")
        required_registry_terms = [
            'option_env!("SNIPTALE_INSTALL_CHANNEL")',
            'option_env!("SNIPTALE_ALLOWED_EXTENSION_ORIGIN")',
            "expected_install_channel",
            "expected_allowed_extension_origins",
        ]
        missing_registry_terms = [
            term for term in required_registry_terms if term not in registry_text
        ]
        if missing_registry_terms:
            errors.append(
                "install registry runtime config missing terms: "
                + ", ".join(missing_registry_terms)
            )

    if REGISTRY_MANIFEST_SOURCE.is_file():
        manifest_text = REGISTRY_MANIFEST_SOURCE.read_text(encoding="utf-8")
        required_manifest_terms = [
            "path == expected_executable_file_name(expected_path)",
            "(b'a'..=b'p').contains(&byte)",
        ]
        missing_manifest_terms = [
            term for term in required_manifest_terms if term not in manifest_text
        ]
        if missing_manifest_terms:
            errors.append(
                "install registry manifest validation missing terms: "
                + ", ".join(missing_manifest_terms)
            )

    if "env -u SNIPTALE_DEV_LOCAL_BROWSER_TRUST" not in text:
        errors.append("package-windows wrapper must explicitly unset dev-local browser trust")

    if IDENTITY_SOURCE.is_file():
        identity_text = IDENTITY_SOURCE.read_text(encoding="utf-8")
        required_identity_terms = [
            "InstallRuntimeIdentity",
            "expected_install_identity",
            "expected_install_channel()",
            "expected_allowed_extension_origins()",
            "expected_install_id(channel)",
            "expected_host_name(channel)",
        ]
        missing_identity_terms = [
            term for term in required_identity_terms if term not in identity_text
        ]
        if missing_identity_terms:
            errors.append(
                "install identity runtime config missing terms: "
                + ", ".join(missing_identity_terms)
            )

    invalid_origins = [
        "chrome-extension://*/",
        "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaq/",
    ]
    for origin in invalid_origins:
        result = subprocess.run(
            [str(PACKAGE_WINDOWS_WRAPPER), "--channel", "dev", "--extension-origin", origin],
            cwd=ROOT,
            check=False,
            capture_output=True,
            text=True,
        )
        output = result.stdout + result.stderr
        if result.returncode == 0 or "Extension origin must be exact" not in output:
            errors.append(f"package-windows wrapper must reject invalid origin: {origin}")

    result = subprocess.run(
        [
            str(PACKAGE_WINDOWS_WRAPPER),
            "--channel",
            "stable",
            "--signing-mode",
            "unsigned",
            "--extension-origin",
            "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/",
        ],
        cwd=ROOT,
        check=False,
        capture_output=True,
        text=True,
    )
    stable_unsigned_output = result.stdout + result.stderr
    if result.returncode == 0 or "Stable Windows packaging requires" not in stable_unsigned_output:
        errors.append("package-windows wrapper must reject unsigned stable packages")

    result = subprocess.run(
        [
            str(PACKAGE_WINDOWS_WRAPPER),
            "--channel",
            "stable",
            "--extension-origin",
            "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/",
        ],
        cwd=ROOT,
        check=False,
        capture_output=True,
        text=True,
    )
    stable_missing_signing_output = result.stdout + result.stderr
    if (
        result.returncode == 0
        or "Production signing requires --signing-cert-thumbprint" not in stable_missing_signing_output
    ):
        errors.append("package-windows wrapper must require production signing inputs for stable packages")

    result = subprocess.run(
        [
            str(PACKAGE_WINDOWS_WRAPPER),
            "--channel",
            "stable",
            "--extension-origin",
            "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/",
            "--signing-cert-thumbprint",
            "00112233445566778899AABBCCDDEEFF00112233",
            "--timestamp-url",
            "https://timestamp.example.invalid",
        ],
        cwd=ROOT,
        check=False,
        capture_output=True,
        text=True,
    )
    stable_missing_qa_output = result.stdout + result.stderr
    if (
        result.returncode == 0
        or "Production signing requires --manual-qa-evidence" not in stable_missing_qa_output
    ):
        errors.append("package-windows wrapper must require manual QA evidence for production packages")


def check_windows_smoke_wrappers(errors: list[str]) -> None:
    bash_root = 'ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"'
    for path in (WINDOWS_SMOKE_SHELL, WINDOWS_SMOKE_FROM_WSL):
        if path.is_file() and bash_root not in path.read_text(encoding="utf-8"):
            errors.append(f"{rel(path)} must resolve repository root from four parent levels")
    powershell_root = 'Join-Path $PSScriptRoot "..\\..\\..\\.."'
    for path in (WINDOWS_SMOKE_SCRIPT, WINDOWS_TRAY_QUICK):
        if path.is_file() and powershell_root not in path.read_text(encoding="utf-8"):
            errors.append(f"{rel(path)} must resolve repository root from four parent levels")
    if WINDOWS_SMOKE_SCRIPT.is_file():
        text = WINDOWS_SMOKE_SCRIPT.read_text(encoding="utf-8")
        required_terms = [
            "Ensure-WixUiExtension",
            'Join-Path $PSScriptRoot "..\\..\\..\\.."',
            "extension list",
            "extension add -g",
            "WixToolset.UI.wixext/$wixVersion",
            "wixext4\\WixToolset.UI.wixext.dll",
            '-ext "$wixUiExtension"',
            '"/qn"',
            '"REBOOT=ReallySuppress"',
            "Invoke-NativeAgentJsonMode",
            "Assert-SupportModeOutputs",
            "Assert-RuntimeDirectoryAccessFailClosed",
            "Assert-PipeSecretProvisioned",
            "Invoke-TopologyFailureProbes",
            "Assert-NativeHostOriginGate",
            "Assert-NativeHostSessionProbes",
            "installed project license",
            "installed project notice",
            "installed third-party notices",
            "installed third-party dependency manifest",
            "ProjectLicense=",
            "ProjectNotice=",
            "ThirdPartyNotices=",
            "ThirdPartyDependencies=",
            "Invoke-NativeHostProbe",
            "New-NativeMessageFrame",
            "Read-NativeMessageFrames",
            "installed repeated native-host session probes",
            "app.hello/controller probe",
            "Assert-CollectDiagnosticsBundle",
            "Write-SmokeEvidenceArtifact",
            "windows-smoke-evidence.json",
            "verify Windows resource inputs",
            "verify_windows_resources.py",
            "verify Windows executable resources",
            "verify installed executable resources",
            "verify_windows_pe_resources.py",
            "verify_msi_identity.ps1",
            "verify Windows MSI identity",
            '-d "AppIcon=resources\\windows\\icons\\sniptale.ico"',
            "CARGO_INCREMENTAL",
            "/inheritance:r",
            "worker-startup-shutdown",
            "browser-eof-active-operation",
            "resident-restart-pending-transfer",
            "stale-challenge-replay",
            "queue-backpressure-category",
            "malformed-authenticated-frame",
            "support-bundle-recent-errors",
            '"--version-json"',
            '"--doctor-json"',
            '"--collect-diagnostics"',
            "installed support mode probes",
            "windows topology failure probes",
            "installed native-host origin gate probe",
            "authAlgorithm",
            "runtime-directory-access",
            "temp-media",
            "categories",
            "broad-write-access",
            "broad-read-access",
            "S-1-5-32-545",
        ]
        missing = [term for term in required_terms if term not in text]
        if missing:
            errors.append("windows-smoke wrapper missing terms: " + ", ".join(missing))

def check_docs(errors: list[str]) -> None:
    if not INSTALLER_README.is_file():
        return
    text = INSTALLER_README.read_text(encoding="utf-8")
    required_terms = [
        "tooling/qa/wrappers/package-windows.sh",
        "Sniptale Desktop",
        "NativeAgentExe",
        "NativeHostManifest",
        "NativeHostName",
        "ProjectLicense",
        "ThirdPartyNotices",
        "sniptale-native-agent.exe",
        "native-host.json",
        "release-evidence.json",
        "SHA256SUMS",
        "provenance.json",
        "SPDX",
        "Authenticode",
        "revocation",
        "Beta/dev manifest templates",
    ]
    missing = [term for term in required_terms if term not in text]
    if missing:
        errors.append("installer README missing terms: " + ", ".join(missing))

    deliverables = {
        MANUAL_QA_CHECKLIST: [
            "package-windows.sh",
            "SniptaleNativeAgent-<channel>-<version>.msi",
            "native-host.json",
            "Windows 10/11",
            "Chrome/Edge",
            "mixed DPI",
            "mixed-audio",
            "SBOM",
            "provenance.json",
            "revocation",
            "windows-release-evidence-template.json",
            "cold start",
            "Accessibility Insights",
            "keyboard-only",
            "UIA report",
            "high contrast",
        ],
        LOGGING_RUNBOOK: [
            "%LOCALAPPDATA%\\Sniptale\\NativeAgent",
            "SNIPTALE_NATIVE_LOG",
            "--version-json",
            "--doctor-json",
            "--collect-diagnostics",
            "redaction",
            "Data handling",
        ],
        DEVELOPER_RUNBOOK: [
            "checkpoint.sh",
            "closeout.sh",
            "release.sh",
            "package-windows.sh",
            "sbom.spdx.json",
            "Windows package",
        ],
    }
    for path, terms in deliverables.items():
        if not path.is_file():
            continue
        text = path.read_text(encoding="utf-8").lower()
        missing_terms = [term for term in terms if term.lower() not in text]
        if missing_terms:
            errors.append(f"{rel(path)} missing required release-deliverable terms: " + ", ".join(missing_terms))

    if MANUAL_QA_EVIDENCE_TEMPLATE.is_file() and WINDOWS_RELEASE_EVIDENCE_VALIDATOR.is_file():
        result = subprocess.run(
            [
                sys.executable,
                str(WINDOWS_RELEASE_EVIDENCE_VALIDATOR),
                str(MANUAL_QA_EVIDENCE_TEMPLATE),
                "--allow-template",
            ],
            cwd=ROOT,
            check=False,
            capture_output=True,
            text=True,
        )
        if result.returncode != 0:
            errors.append(
                "windows release evidence template must satisfy validator: "
                + (result.stdout + result.stderr)
            )


def set_nested(payload: dict, path: tuple[str, ...], value) -> None:
    current = payload
    for part in path[:-1]:
        current = current[part]
    current[path[-1]] = value


def filled_release_evidence_sample() -> dict:
    return {
        "formatVersion": 1,
        "releaseCandidate": {
            "nativeVersion": "0.1.0",
            "extensionVersion": "0.1.0",
            "channel": "stable",
            "windowsBuild": "Windows 11 24H2 26100",
            "browserBuilds": {
                "chrome": "Chrome 126.0.0.0",
                "edge": "Edge 126.0.0.0",
            },
            "installerEvidence": {
                "msiFileName": "SniptaleNativeAgent-stable-0.1.0.msi",
                "releaseEvidenceFileName": "release-evidence.json",
                "sha256SumsFileName": "SHA256SUMS",
                "sbomFileName": "sbom.spdx.json",
                "provenanceFileName": "provenance.json",
            },
        },
        "performanceBaseline": {
            "coldStartMs": 1200,
            "screenshotCaptureMs": 180,
            "recordingFiveMinute": {
                "durationMs": 300000,
                "droppedFrames": 0,
                "dropRatePct": 0.0,
                "queuePressureMax": 42.0,
                "encodeFinalizeMs": 900,
                "tempMediaBytes": 0,
                "cleanupVerified": True,
            },
            "reconnectDuringRecording": {
                "completed": True,
                "finalizedMediaRecovered": True,
                "stopFinalizeMs": 700,
            },
        },
        "accessibilityBaseline": {
            "trayKeyboardOnly": True,
            "trayUiaNamesAndRoles": True,
            "overlayKeyboardCancel": True,
            "overlayFocusReturn": True,
            "overlayHighContrastUsable": True,
            "accessibilityInsightsRunId": "ai-win-20260703",
        },
        "windowsHardeningReview": {
            "wdacAppControlGuidanceReviewed": True,
            "appLockerSecondaryGuidanceReviewed": True,
            "cfgProcessMitigationGuidanceReviewed": True,
            "bitLockerDefenseInDepthAcknowledged": True,
            "noProductRuntimePolicyAdded": True,
        },
        "evidenceFiles": {
            "wprTraceFileName": "wpr-trace.etl",
            "accessibilityReportFileName": "accessibility-insights.html",
            "manualQaNotesFileName": "manual-qa-notes.md",
        },
        "review": {
            "tester": "release-owner",
            "completedAtUtc": "2026-07-03T00:00:00Z",
            "approvedForProductionCandidate": True,
        },
    }


def run_evidence_validator(path: Path, *extra_args: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        [sys.executable, str(WINDOWS_RELEASE_EVIDENCE_VALIDATOR), str(path), *extra_args],
        cwd=ROOT,
        check=False,
        capture_output=True,
        text=True,
    )


def check_windows_release_evidence_validator(errors: list[str]) -> None:
    if not WINDOWS_RELEASE_EVIDENCE_VALIDATOR.is_file():
        return
    with tempfile.TemporaryDirectory() as temp_dir:
        temp_path = Path(temp_dir)
        valid_path = temp_path / "valid.json"
        valid_payload = filled_release_evidence_sample()
        valid_path.write_text(json.dumps(valid_payload, indent=2) + "\n", encoding="utf-8")
        valid_result = run_evidence_validator(
            valid_path,
            "--expected-channel",
            "stable",
            "--expected-msi-file",
            "SniptaleNativeAgent-stable-0.1.0.msi",
            "--expected-release-evidence-file",
            "release-evidence.json",
            "--expected-sha256-file",
            "SHA256SUMS",
            "--expected-sbom-file",
            "sbom.spdx.json",
            "--expected-provenance-file",
            "provenance.json",
        )
        if valid_result.returncode != 0:
            errors.append("filled windows release evidence sample must pass: " + valid_result.stdout + valid_result.stderr)

        invalid_cases = [
            (("formatVersion",), True, "boolean format version"),
            (("performanceBaseline", "recordingFiveMinute", "dropRatePct"), "0", "string drop rate"),
            (("performanceBaseline", "recordingFiveMinute", "durationMs"), 299999, "short recording duration"),
            (("releaseCandidate", "browserBuilds", "chrome"), "", "missing browser build"),
            (("evidenceFiles", "wprTraceFileName"), "   ", "blank artifact file name"),
            (("evidenceFiles", "wprTraceFileName"), r"C:\trace.etl", "non-leaf artifact path"),
            (("releaseCandidate", "channel"), "qa", "invalid channel"),
            (("review", "completedAtUtc"), "2026-07-03T00:00:00+03:00", "non-UTC review timestamp"),
            (
                ("releaseCandidate", "installerEvidence", "msiFileName"),
                "SniptaleNativeAgent-stable-0.2.0.msi",
                "unexpected MSI file name",
            ),
        ]
        for index, (path, value, label) in enumerate(invalid_cases):
            payload = copy.deepcopy(valid_payload)
            set_nested(payload, path, value)
            invalid_path = temp_path / f"invalid-{index}.json"
            invalid_path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
            result = run_evidence_validator(
                invalid_path,
                "--expected-channel",
                "stable",
                "--expected-msi-file",
                "SniptaleNativeAgent-stable-0.1.0.msi",
            )
            if result.returncode == 0:
                errors.append(f"windows release evidence validator accepted {label}")

        invalid_json_path = temp_path / "invalid-json.json"
        invalid_json_path.write_text(
            valid_path.read_text(encoding="utf-8").replace('"coldStartMs": 1200', '"coldStartMs": NaN'),
            encoding="utf-8",
        )
        result = run_evidence_validator(invalid_json_path)
        if result.returncode == 0:
            errors.append("windows release evidence validator accepted NaN performance evidence")


def main() -> int:
    errors: list[str] = []
    require_files(errors)
    check_wix(errors)
    check_manifest_templates(errors)
    check_clean_break_identity_guards(errors)
    check_package_wrapper(errors)
    check_windows_smoke_wrappers(errors)
    check_docs(errors)
    check_windows_release_evidence_validator(errors)
    if errors:
        fail(errors)
    print("OK release artifact guardrails")
    return 0


if __name__ == "__main__":
    sys.exit(main())
