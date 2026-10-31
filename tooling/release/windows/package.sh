#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
# shellcheck source=../../qa/runtime/lib.sh
source "$ROOT/tooling/qa/runtime/lib.sh"
ensure_rust_env
cd "$ROOT"

channel="dev"
extension_origin="${SNIPTALE_EXTENSION_ORIGIN:-}"
out_dir=""
product_version=""
wix_cmd="${WIX:-wix}"
signing_mode=""
signing_cert_thumbprint="${SNIPTALE_SIGNING_CERT_THUMBPRINT:-}"
timestamp_url="${SNIPTALE_TIMESTAMP_URL:-}"
manual_qa_evidence="${SNIPTALE_WINDOWS_RELEASE_EVIDENCE:-}"
qa_init "qa:package-windows" blocking default \
  "$signing_cert_thumbprint" "$timestamp_url" "$manual_qa_evidence"

usage() {
  cat <<'EOF'
Usage: tooling/qa/wrappers/package-windows.sh --extension-origin <chrome-extension://32-letter-id/> [options]

Builds the Windows installable artifacts for one Sniptale native-agent channel:
the x86_64-pc-windows-msvc executable, rendered Native Messaging manifest, and MSI.

Options:
  --channel stable|beta|dev          Channel to package (default: dev)
  --extension-origin <origin>        Exact allowed Chrome/Edge extension origin
  --out-dir <path>                   Output directory (default: .tmp/release/windows-<channel>)
  --product-version <version>        MSI product version (default: Cargo.toml package version)
  --signing-mode unsigned|production Signing policy (default: production for stable, unsigned otherwise)
  --signing-cert-thumbprint <sha1>    CurrentUser\My code-signing certificate thumbprint
  --timestamp-url <url>               Authenticode timestamp server URL
  --manual-qa-evidence <path>         Filled performance/accessibility evidence JSON
  --wix <path-or-command>            WiX v4 CLI command (default: WIX env or wix)
  -h, --help                         Show this help

The wrapper requires fresh structured state from tooling/qa/wrappers/checkpoint.sh. Stable packaging requires production
signing. Beta/dev packaging may be unsigned only under their channel-scoped host names and origins.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --channel)
      channel="${2:-}"
      shift 2
      ;;
    --channel=*)
      channel="${1#*=}"
      shift
      ;;
    --extension-origin)
      extension_origin="${2:-}"
      shift 2
      ;;
    --extension-origin=*)
      extension_origin="${1#*=}"
      shift
      ;;
    --out-dir)
      out_dir="${2:-}"
      shift 2
      ;;
    --out-dir=*)
      out_dir="${1#*=}"
      shift
      ;;
    --product-version)
      product_version="${2:-}"
      shift 2
      ;;
    --product-version=*)
      product_version="${1#*=}"
      shift
      ;;
    --signing-mode)
      signing_mode="${2:-}"
      shift 2
      ;;
    --signing-mode=*)
      signing_mode="${1#*=}"
      shift
      ;;
    --signing-cert-thumbprint)
      signing_cert_thumbprint="${2:-}"
      shift 2
      ;;
    --signing-cert-thumbprint=*)
      signing_cert_thumbprint="${1#*=}"
      shift
      ;;
    --timestamp-url)
      timestamp_url="${2:-}"
      shift 2
      ;;
    --timestamp-url=*)
      timestamp_url="${1#*=}"
      shift
      ;;
    --manual-qa-evidence)
      manual_qa_evidence="${2:-}"
      shift 2
      ;;
    --manual-qa-evidence=*)
      manual_qa_evidence="${1#*=}"
      shift
      ;;
    --wix)
      wix_cmd="${2:-}"
      shift 2
      ;;
    --wix=*)
      wix_cmd="${1#*=}"
      shift
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      printf 'Unknown argument: %s\n' "$1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

QA_SENSITIVE_VALUES+=("$signing_cert_thumbprint" "$timestamp_url" "$manual_qa_evidence")

case "$channel" in
  stable)
    manifest_template="installer/native-host/stable.json.template"
    native_host_name="com.sniptale.native_host"
    origin_token="{{SNIPTALE_STABLE_EXTENSION_ORIGIN}}"
    ;;
  beta)
    manifest_template="installer/native-host/beta.json.template"
    native_host_name="com.sniptale.beta.native_host"
    origin_token="{{SNIPTALE_BETA_EXTENSION_ORIGIN}}"
    ;;
  dev)
    manifest_template="installer/native-host/dev.json.template"
    native_host_name="com.sniptale.dev.native_host"
    origin_token="{{SNIPTALE_DEV_EXTENSION_ORIGIN}}"
    ;;
  *)
    printf 'Unknown package channel: %s\n' "$channel" >&2
    usage >&2
    exit 2
    ;;
esac

if [[ -z "$signing_mode" ]]; then
  if [[ "$channel" == "stable" ]]; then
    signing_mode="production"
  else
    signing_mode="unsigned"
  fi
fi

case "$signing_mode" in
  unsigned | production)
    ;;
  *)
    printf 'Unknown signing mode: %s\n' "$signing_mode" >&2
    usage >&2
    exit 2
    ;;
esac

if [[ "$channel" == "stable" && "$signing_mode" != "production" ]]; then
  printf 'Stable Windows packaging requires --signing-mode production.\n' >&2
  exit 2
fi

if [[ "$signing_mode" == "production" ]]; then
  if [[ -z "$signing_cert_thumbprint" ]]; then
    printf 'Production signing requires --signing-cert-thumbprint or SNIPTALE_SIGNING_CERT_THUMBPRINT.\n' >&2
    exit 2
  fi
  if [[ -z "$timestamp_url" ]]; then
    printf 'Production signing requires --timestamp-url or SNIPTALE_TIMESTAMP_URL.\n' >&2
    exit 2
  fi
  if [[ -z "$manual_qa_evidence" ]]; then
    printf 'Production signing requires --manual-qa-evidence or SNIPTALE_WINDOWS_RELEASE_EVIDENCE.\n' >&2
    exit 2
  fi
fi

if [[ -z "$extension_origin" ]]; then
  printf 'Missing required --extension-origin <chrome-extension://32-letter-id/>.\n' >&2
  exit 2
fi

extension_origin_regex='^chrome-extension://[a-p]{32}/$'
if [[ ! "$extension_origin" =~ $extension_origin_regex ]]; then
  printf 'Extension origin must be exact chrome-extension://<32-letter-id>/, got: %s\n' "$extension_origin" >&2
  exit 2
fi

if [[ -z "$product_version" ]]; then
  product_version="$(awk -F\" '/^version = / { print $2; exit }' Cargo.toml)"
fi

if [[ -z "$product_version" ]]; then
  printf 'Could not determine package version from Cargo.toml.\n' >&2
  exit 1
fi

if [[ -z "$out_dir" ]]; then
  out_dir=".tmp/release/windows-$channel"
fi
msi_file_name="SniptaleNativeAgent-$channel-$product_version.msi"
manual_qa_evidence_bundle_path="$out_dir/windows-release-qa-evidence.json"
project_license_path="$out_dir/LICENSE.txt"
project_notice_path="$out_dir/NOTICE.txt"
third_party_notices_path="$out_dir/THIRD_PARTY_NOTICES.md"
third_party_dependencies_path="$out_dir/THIRD_PARTY_DEPENDENCIES.json"
third_party_license_dir="$out_dir/LICENSES/third-party"

require_command cargo
require_command python3
if ! command -v "$wix_cmd" >/dev/null 2>&1; then
  printf 'Missing required WiX v4 CLI command: %s\n' "$wix_cmd" >&2
  printf 'Install WiX v4 or pass --wix /path/to/wix.\n' >&2
  exit 1
fi

powershell_cmd=""
if command -v powershell.exe >/dev/null 2>&1; then
  powershell_cmd="powershell.exe"
elif command -v powershell >/dev/null 2>&1; then
  powershell_cmd="powershell"
elif command -v pwsh >/dev/null 2>&1; then
  powershell_cmd="pwsh"
else
  printf 'Windows packaging requires PowerShell for MSI database verification.\n' >&2
  exit 1
fi

qa_require_harness
qa_assert_state checkpoint
mkdir -p "$out_dir"

bundle_legal_payload() {
  mkdir -p "$third_party_license_dir"
  find "$third_party_license_dir" -type f -delete
  cp LICENSE "$project_license_path"
  cp NOTICE "$project_notice_path"
  cp THIRD_PARTY_NOTICES.md "$third_party_notices_path"
  cp THIRD_PARTY_DEPENDENCIES.json "$third_party_dependencies_path"
  cp LICENSES/third-party/* "$third_party_license_dir/"
}

verify_msi_identity() {
  local artifact="$1"
  local expected_version="$2"
  local verifier="tooling/release/windows/verify_msi_identity.ps1"
  if [[ "$powershell_cmd" == *.exe ]] && command -v wslpath >/dev/null 2>&1; then
    verifier="$(wslpath -w "$verifier")"
    artifact="$(wslpath -w "$artifact")"
  fi
  "$powershell_cmd" -NoProfile -ExecutionPolicy Bypass -File "$verifier" \
    -Msi "$artifact" -ExpectedProductVersion "$expected_version"
}

validate_manual_qa_evidence() {
  if [[ -z "$manual_qa_evidence" ]]; then
    return 0
  fi
  if [[ ! -s "$manual_qa_evidence" ]]; then
    printf 'Manual QA release evidence is missing or empty: %s\n' "$manual_qa_evidence" >&2
    exit 1
  fi
  python3 tooling/release/windows/verify-windows-release-evidence.py "$manual_qa_evidence" \
    --expected-channel "$channel" \
    --expected-msi-file "$msi_file_name" \
    --expected-release-evidence-file "release-evidence.json" \
    --expected-sha256-file "SHA256SUMS" \
    --expected-sbom-file "sbom.spdx.json" \
    --expected-provenance-file "provenance.json"
}

bundle_manual_qa_evidence() {
  if [[ -z "$manual_qa_evidence" ]]; then
    return 0
  fi
  if [[ "$(realpath "$manual_qa_evidence")" != "$(realpath -m "$manual_qa_evidence_bundle_path")" ]]; then
    cp "$manual_qa_evidence" "$manual_qa_evidence_bundle_path"
  fi
}

ensure_wix_ui_extension() {
  wix_version="$("$wix_cmd" --version | sed -E 's/^([0-9]+\.[0-9]+\.[0-9]+).*/\1/')"
  if [[ -z "$wix_version" || ! "$wix_version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    printf 'Could not determine WiX version from %s --version.\n' "$wix_cmd" >&2
    exit 1
  fi
  local expected_wix
  expected_wix="$(python3 -c 'import json; print(json.load(open("tooling/configs/qa/tool-versions.json"))["windows"]["wix"])')"
  if [[ "$wix_version" != "$expected_wix" ]]; then
    printf 'WiX version differs from canonical tool policy; prepare the pinned Windows toolchain.\n' >&2
    exit 1
  fi
  if "$wix_cmd" extension list 2>/dev/null | grep -Eq "WixToolset\\.UI\\.wixext[[:space:]]+$wix_version"; then
    return 0
  fi
  qa_step "install wix ui extension" "$wix_cmd" extension add -g "WixToolset.UI.wixext/$wix_version"
}

sign_authenticode_artifact() {
  local artifact="$1"
  # PowerShell expands its variables after Bash passes this literal command.
  # shellcheck disable=SC2016
  "$powershell_cmd" -NoProfile -ExecutionPolicy Bypass -Command \
    '& {
      param([string]$Path, [string]$Thumbprint, [string]$TimestampUrl)
      $ErrorActionPreference = "Stop"
      $normalized = ($Thumbprint -replace "\s", "").ToUpperInvariant()
      $cert = Get-ChildItem -Path Cert:\CurrentUser\My | Where-Object {
        $_.Thumbprint.ToUpperInvariant() -eq $normalized
      } | Select-Object -First 1
      if (-not $cert) {
        throw "Code-signing certificate not found in Cert:\CurrentUser\My: $normalized"
      }
      $signature = Set-AuthenticodeSignature -FilePath $Path -Certificate $cert -HashAlgorithm SHA256 -TimestampServer $TimestampUrl
      if ($signature.Status -ne "Valid") {
        throw "Authenticode signing failed for ${Path}: $($signature.Status) $($signature.StatusMessage)"
      }
    }' "$artifact" "$signing_cert_thumbprint" "$timestamp_url"
}

verify_authenticode_artifact() {
  local artifact="$1"
  local evidence_path="$2"
  # PowerShell expands its variables after Bash passes this literal command.
  # shellcheck disable=SC2016
  "$powershell_cmd" -NoProfile -ExecutionPolicy Bypass -Command \
    '& {
      param([string]$Path, [string]$Thumbprint, [string]$EvidencePath)
      $ErrorActionPreference = "Stop"
      $expected = ($Thumbprint -replace "\s", "").ToUpperInvariant()
      $signature = Get-AuthenticodeSignature -FilePath $Path
      if ($signature.Status -ne "Valid") {
        throw "Authenticode verification failed for ${Path}: $($signature.Status) $($signature.StatusMessage)"
      }
      if (-not $signature.SignerCertificate) {
        throw "Authenticode signer certificate is missing for ${Path}"
      }
      $actual = $signature.SignerCertificate.Thumbprint.ToUpperInvariant()
      if ($actual -ne $expected) {
        throw "Authenticode signer mismatch for ${Path}: expected $expected, got $actual"
      }
      if (-not $signature.TimeStamperCertificate) {
        throw "Authenticode timestamp is missing for ${Path}"
      }
      $chain = [System.Security.Cryptography.X509Certificates.X509Chain]::new()
      $chain.ChainPolicy.RevocationMode = [System.Security.Cryptography.X509Certificates.X509RevocationMode]::Online
      $chain.ChainPolicy.RevocationFlag = [System.Security.Cryptography.X509Certificates.X509RevocationFlag]::ExcludeRoot
      $chain.ChainPolicy.VerificationFlags = [System.Security.Cryptography.X509Certificates.X509VerificationFlags]::NoFlag
      $chain.ChainPolicy.UrlRetrievalTimeout = [TimeSpan]::FromSeconds(30)
      $revocationChecked = $true
      $chainValid = $chain.Build($signature.SignerCertificate)
      $chainStatus = @($chain.ChainStatus | ForEach-Object {
        [ordered]@{
          status = [string]$_.Status
          statusInformation = ($_.StatusInformation -replace "\s+", " ").Trim()
        }
      })
      if (-not $chainValid) {
        $statusText = ($chainStatus | ForEach-Object { $_.status }) -join ","
        throw "Authenticode revocation/chain verification failed for ${Path}: ${statusText}"
      }
      $timestamp = $signature.TimeStamperCertificate
      $payload = [ordered]@{
        fileName = [System.IO.Path]::GetFileName($Path)
        status = [string]$signature.Status
        signerThumbprint = $actual
        expectedSignerThumbprint = $expected
        signerSubject = $signature.SignerCertificate.Subject
        timestampPresent = $true
        timestampCertificateThumbprint = $timestamp.Thumbprint
        timestampCertificateSubject = $timestamp.Subject
        timestampCertificateNotBefore = $timestamp.NotBefore.ToUniversalTime().ToString("o")
        timestampCertificateNotAfter = $timestamp.NotAfter.ToUniversalTime().ToString("o")
        revocationChecked = $revocationChecked
        revocationMode = "Online"
        revocationFlag = "ExcludeRoot"
        revocationStatus = "Valid"
        chainStatus = $chainStatus
      }
      $json = $payload | ConvertTo-Json -Depth 3
      [System.IO.File]::WriteAllText($EvidencePath, $json, [System.Text.UTF8Encoding]::new($false))
    }' "$artifact" "$signing_cert_thumbprint" "$evidence_path"
}

write_named_sha256() {
  local artifact="$1"
  local name="$2"
  local digest
  digest="$(sha256sum "$artifact" | awk '{print $1}')"
  printf '%s  %s\n' "$digest" "$name"
}

write_release_evidence() {
  local evidence_path="$out_dir/release-evidence.json"
  local sha_path="$out_dir/SHA256SUMS"
  local sbom_source_path=".tmp/release/sbom.spdx.json"
  local sbom_path="$out_dir/sbom.spdx.json"
  local sbom_stamp_path=".tmp/release/sbom.dependency.stamp"
  local cargo_audit_source_path=".tmp/release/supply-chain/cargo-audit.json"
  local cargo_deny_source_path=".tmp/release/supply-chain/cargo-deny.txt"
  local cargo_audit_path="$out_dir/cargo-audit.json"
  local cargo_deny_path="$out_dir/cargo-deny.txt"
  local supply_chain_stamp_path=".tmp/qa/supply-chain.stamp"
  local provenance_path="$out_dir/provenance.json"
  local manual_evidence_path=""
  local current_dependency_fingerprint
  local checkpoint_fingerprint
  current_dependency_fingerprint="$(dependency_fingerprint)"
  checkpoint_fingerprint="$(python3 -c 'import json; print(json.load(open(".tmp/qa/state/checkpoint.json", encoding="utf-8"))["fingerprint"])')"
  if [[ ! -s "$sbom_source_path" ]]; then
    if [[ "$signing_mode" == "production" ]]; then
      printf 'Missing release SBOM: .tmp/release/sbom.spdx.json. Run tooling/qa/wrappers/release.sh first.\n' >&2
      exit 1
    fi
    sbom_path=""
  elif [[ "$signing_mode" == "production" ]]; then
    if [[ ! -s "$sbom_stamp_path" ]] || [[ "$(cat "$sbom_stamp_path")" != "$current_dependency_fingerprint" ]]; then
      printf 'Stale release SBOM: run tooling/qa/wrappers/release.sh after dependency changes.\n' >&2
      exit 1
    fi
  fi
  if [[ ! -s "$cargo_audit_source_path" ]]; then
    if [[ "$signing_mode" == "production" ]]; then
      printf 'Missing cargo audit release report: .tmp/release/supply-chain/cargo-audit.json. Run tooling/qa/wrappers/release.sh first.\n' >&2
      exit 1
    fi
    cargo_audit_path=""
  fi
  if [[ ! -s "$cargo_deny_source_path" ]]; then
    if [[ "$signing_mode" == "production" ]]; then
      printf 'Missing cargo deny release report: .tmp/release/supply-chain/cargo-deny.txt. Run tooling/qa/wrappers/release.sh first.\n' >&2
      exit 1
    fi
    cargo_deny_path=""
  fi
  if [[ "$signing_mode" == "production" ]]; then
    if [[ ! -s "$supply_chain_stamp_path" ]] || [[ "$(cat "$supply_chain_stamp_path")" != "$current_dependency_fingerprint" ]]; then
      printf 'Stale supply-chain reports: run tooling/qa/wrappers/release.sh after dependency changes.\n' >&2
      exit 1
    fi
  fi
  if [[ -n "$sbom_path" ]]; then
    cp "$sbom_source_path" "$sbom_path"
  fi
  if [[ -n "$cargo_audit_path" ]]; then
    cp "$cargo_audit_source_path" "$cargo_audit_path"
  fi
  if [[ -n "$cargo_deny_path" ]]; then
    cp "$cargo_deny_source_path" "$cargo_deny_path"
  fi
  if [[ -n "$manual_qa_evidence" ]]; then
    manual_evidence_path="$manual_qa_evidence_bundle_path"
  fi
  qa_step "write release provenance" \
    python3 - "$provenance_path" "$channel" "$signing_mode" "$current_dependency_fingerprint" "$checkpoint_fingerprint" "$native_agent_exe" "$manifest_path" "$msi_path" "$project_license_path" "$project_notice_path" "$third_party_notices_path" "$third_party_dependencies_path" "$manual_evidence_path" <<'PY'
import datetime as dt
import json
import subprocess
import sys
from pathlib import Path


def command_output(*args: str) -> str:
    result = subprocess.run(args, check=False, capture_output=True, text=True)
    if result.returncode != 0:
        return "unavailable"
    return result.stdout.strip()


def sha256sum(path: Path) -> str:
    result = subprocess.run(["sha256sum", str(path)], check=True, capture_output=True, text=True)
    return result.stdout.split()[0]


provenance_path = Path(sys.argv[1])
channel = sys.argv[2]
signing_mode = sys.argv[3]
dependency_fingerprint = sys.argv[4]
checkpoint_fingerprint = sys.argv[5]
artifact_paths = [Path(value) for value in sys.argv[6:13]]
manual_evidence_path = Path(sys.argv[13]) if sys.argv[13] else None
git_status = command_output("git", "status", "--short")
payload = {
    "formatVersion": 1,
    "kind": "sniptale-windows-release-provenance",
    "generatedAtUtc": dt.datetime.now(dt.UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
    "source": {
        "gitCommit": command_output("git", "rev-parse", "--verify", "HEAD"),
        "worktreeClean": git_status == "",
        "worktreeStatus": git_status.splitlines(),
        "worktreeFingerprint": command_output("bash", "-lc", "source tooling/qa/runtime/lib.sh && fingerprint_worktree"),
    },
    "build": {
        "packageWrapper": "tooling/qa/wrappers/package-windows.sh",
        "checkpointFingerprint": checkpoint_fingerprint,
        "cargoVersion": command_output("cargo", "--version"),
        "rustcVersion": command_output("rustc", "--version"),
        "target": "x86_64-pc-windows-msvc",
        "profile": "release",
        "allFeatures": True,
    },
    "policy": {
        "channel": channel,
        "signingMode": signing_mode,
        "productionSigned": signing_mode == "production",
        "timestampRequired": signing_mode == "production",
        "revocationRequiredWhenSupported": signing_mode == "production",
    },
    "dependencyFingerprint": dependency_fingerprint,
    "artifacts": [
        {
            "fileName": path.name,
            "role": role,
            "sha256": sha256sum(path),
        }
        for path, role in zip(artifact_paths, ["native-agent-exe", "native-host-manifest", "windows-msi", "project-license", "project-notice", "third-party-notices", "third-party-dependency-manifest"])
    ],
    "manualQaEvidence": {
        "fileName": manual_evidence_path.name if manual_evidence_path else None,
        "sha256": sha256sum(manual_evidence_path) if manual_evidence_path else None,
        "required": signing_mode == "production",
    },
}
provenance_path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
PY
  {
    write_named_sha256 "$native_agent_exe" "$(basename "$native_agent_exe")"
    write_named_sha256 "$manifest_path" "$(basename "$manifest_path")"
    write_named_sha256 "$msi_path" "$(basename "$msi_path")"
    write_named_sha256 "$project_license_path" "$(basename "$project_license_path")"
    write_named_sha256 "$project_notice_path" "$(basename "$project_notice_path")"
    write_named_sha256 "$third_party_notices_path" "$(basename "$third_party_notices_path")"
    write_named_sha256 "$third_party_dependencies_path" "$(basename "$third_party_dependencies_path")"
    while IFS= read -r -d '' license_file; do
      write_named_sha256 "$license_file" "${license_file#"$out_dir/"}"
    done < <(find "$third_party_license_dir" -type f -print0 | sort -z)
    write_named_sha256 "$provenance_path" "$(basename "$provenance_path")"
    if [[ -n "$sbom_path" ]]; then
      write_named_sha256 "$sbom_path" "$(basename "$sbom_path")"
    fi
    if [[ -n "$cargo_audit_path" ]]; then
      write_named_sha256 "$cargo_audit_path" "$(basename "$cargo_audit_path")"
    fi
    if [[ -n "$cargo_deny_path" ]]; then
      write_named_sha256 "$cargo_deny_path" "$(basename "$cargo_deny_path")"
    fi
    if [[ "$signing_mode" == "production" ]]; then
      write_named_sha256 "$out_dir/signature-exe.json" "signature-exe.json"
      write_named_sha256 "$out_dir/signature-msi.json" "signature-msi.json"
    fi
    if [[ -n "$manual_qa_evidence" ]]; then
      write_named_sha256 "$manual_evidence_path" "$(basename "$manual_evidence_path")"
    fi
  } >"$sha_path"
  qa_step "write release evidence" \
    python3 - "$evidence_path" "$channel" "$signing_mode" "$timestamp_url" "$signing_cert_thumbprint" "$current_dependency_fingerprint" "$native_agent_exe" "$manifest_path" "$msi_path" "$provenance_path" "$sha_path" "$sbom_path" "$cargo_audit_path" "$cargo_deny_path" "$manual_evidence_path" "$out_dir/signature-exe.json" "$out_dir/signature-msi.json" "$project_license_path" "$project_notice_path" "$third_party_notices_path" "$third_party_dependencies_path" <<'PY'
import json
import sys
from pathlib import Path

evidence_path = Path(sys.argv[1])
channel = sys.argv[2]
signing_mode = sys.argv[3]
timestamp_url = sys.argv[4]
expected_signer_thumbprint = sys.argv[5]
dependency_fingerprint = sys.argv[6]
artifact_paths = [Path(value) for value in sys.argv[7:10]]
provenance_path = Path(sys.argv[10])
sha_path = Path(sys.argv[11])
sbom_path = Path(sys.argv[12]) if sys.argv[12] else None
cargo_audit_path = Path(sys.argv[13]) if sys.argv[13] else None
cargo_deny_path = Path(sys.argv[14]) if sys.argv[14] else None
manual_evidence_path = Path(sys.argv[15]) if sys.argv[15] else None
signature_paths = [Path(value) for value in sys.argv[16:18] if value and Path(value).is_file()]
legal_paths = [Path(value) for value in sys.argv[18:22]]
hashes = {}
for line in sha_path.read_text(encoding="utf-8").splitlines():
    digest, path = line.split(maxsplit=1)
    hashes[Path(path).name] = digest
signature_evidence = []
for path in signature_paths:
    signature_evidence.append(json.loads(path.read_text(encoding="utf-8")))
payload = {
    "formatVersion": 1,
    "channel": channel,
    "signingMode": signing_mode,
    "productionSigned": signing_mode == "production",
    "timestampRequired": signing_mode == "production",
    "timestampUrl": timestamp_url if signing_mode == "production" else None,
    "expectedSignerThumbprint": expected_signer_thumbprint if signing_mode == "production" else None,
    "dependencyFingerprint": dependency_fingerprint,
    "sha256SumsFileName": sha_path.name,
    "artifacts": [
        {
            "fileName": path.name,
            "pathCategory": "windows-release-artifact",
            "sha256": hashes[path.name],
        }
        for path in artifact_paths
    ],
    "provenance": {
        "pathCategory": "release-provenance",
        "required": True,
        "fileName": provenance_path.name,
        "sha256": hashes[provenance_path.name],
    },
    "sbom": {
        "pathCategory": "release-sbom",
        "required": signing_mode == "production",
        "fileName": sbom_path.name if sbom_path else None,
        "sha256": hashes.get(sbom_path.name) if sbom_path else None,
        "format": "SPDX-2.3",
    },
    "supplyChainReports": {
        "pathCategory": "release-supply-chain",
        "required": signing_mode == "production",
        "cargoAudit": cargo_audit_path.name if cargo_audit_path else None,
        "cargoAuditSha256": hashes.get(cargo_audit_path.name) if cargo_audit_path else None,
        "cargoDeny": cargo_deny_path.name if cargo_deny_path else None,
        "cargoDenySha256": hashes.get(cargo_deny_path.name) if cargo_deny_path else None,
    },
    "legalPayload": [
        {
            "fileName": path.name,
            "pathCategory": "release-legal-payload",
            "sha256": hashes[path.name],
        }
        for path in legal_paths
    ],
    "manualQaEvidence": {
        "pathCategory": "manual-windows-release-evidence",
        "required": signing_mode == "production",
        "fileName": manual_evidence_path.name if manual_evidence_path else None,
        "sha256": hashes.get(manual_evidence_path.name) if manual_evidence_path else None,
    },
    "signatureEvidence": signature_evidence,
}
evidence_path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
PY
  write_named_sha256 "$evidence_path" "$(basename "$evidence_path")" >>"$sha_path"
}

qa_step "validate manual windows release evidence" validate_manual_qa_evidence
qa_step "bundle manual windows release evidence" bundle_manual_qa_evidence
qa_step "verify dependency license payload" tooling/qa/wrappers/licenses.sh --check
qa_step "bundle legal payload" bundle_legal_payload
ensure_wix_ui_extension

qa_step "build windows-msvc release executable" \
  env -u SNIPTALE_DEV_LOCAL_BROWSER_TRUST \
  SNIPTALE_INSTALL_CHANNEL="$channel" SNIPTALE_ALLOWED_EXTENSION_ORIGIN="$extension_origin" SNIPTALE_WINDOWS_RESOURCES=require \
  cargo build --locked --release --target x86_64-pc-windows-msvc --bin sniptale-native-agent --all-features
native_agent_exe="target/x86_64-pc-windows-msvc/release/sniptale-native-agent.exe"

if [[ ! -s "$native_agent_exe" ]]; then
  printf 'Native agent executable is missing or empty: %s\n' "$native_agent_exe" >&2
  exit 1
fi

qa_step "verify Windows executable resources" \
  python3 tooling/release/windows/verify_windows_pe_resources.py "$native_agent_exe"

if [[ "$signing_mode" == "production" ]]; then
  qa_step "sign windows executable" sign_authenticode_artifact "$native_agent_exe"
  qa_step "verify windows executable signature" verify_authenticode_artifact "$native_agent_exe" "$out_dir/signature-exe.json"
fi

manifest_path="$out_dir/native-host.json"
msi_path="$out_dir/$msi_file_name"

qa_step "render native host manifest" \
  python3 - "$manifest_template" "$manifest_path" "sniptale-native-agent.exe" "$origin_token" "$extension_origin" <<'PY'
import json
import sys
from pathlib import Path

template_path = Path(sys.argv[1])
output_path = Path(sys.argv[2])
exe_path = sys.argv[3]
origin_token = sys.argv[4]
extension_origin = sys.argv[5]

rendered = template_path.read_text(encoding="utf-8")
rendered = rendered.replace("{{SNIPTALE_NATIVE_AGENT_EXE}}", exe_path)
rendered = rendered.replace(origin_token, extension_origin)
data = json.loads(rendered)
output_path.parent.mkdir(parents=True, exist_ok=True)
output_path.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
PY

qa_step "build windows msi" \
  "$wix_cmd" build installer/wix/SniptaleNativeAgent.wxs \
  -ext WixToolset.UI.wixext \
  -d "NativeAgentExe=$native_agent_exe" \
  -d "NativeHostManifest=$manifest_path" \
  -d "ProjectLicense=$project_license_path" \
  -d "ProjectNotice=$project_notice_path" \
  -d "ThirdPartyNotices=$third_party_notices_path" \
  -d "ThirdPartyDependencies=$third_party_dependencies_path" \
  -d "AppIcon=resources/windows/icons/sniptale.ico" \
  -d "NativeHostName=$native_host_name" \
  -d "ProductVersion=$product_version" \
  -o "$msi_path"

if [[ ! -s "$msi_path" ]]; then
  printf 'MSI is missing or empty: %s\n' "$msi_path" >&2
  exit 1
fi

qa_step "verify Windows MSI identity" verify_msi_identity "$msi_path" "$product_version"

if [[ "$signing_mode" == "production" ]]; then
  qa_step "sign windows msi" sign_authenticode_artifact "$msi_path"
  qa_step "verify windows msi signature" verify_authenticode_artifact "$msi_path" "$out_dir/signature-msi.json"
fi

cp "$native_agent_exe" "$out_dir/sniptale-native-agent.exe"
native_agent_exe="$out_dir/sniptale-native-agent.exe"
write_release_evidence

qa_fail_if_any "windows package summary"
printf 'windows package OK\n'
printf 'executable: %s\n' "$native_agent_exe"
printf 'manifest:   %s\n' "$manifest_path"
printf 'msi:        %s\n' "$msi_path"
printf 'evidence:   %s\n' "$out_dir/release-evidence.json"
