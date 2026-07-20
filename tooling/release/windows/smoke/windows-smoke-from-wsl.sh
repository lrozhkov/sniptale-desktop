#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
cd "$ROOT"

usage() {
  cat <<'EOF'
Usage: tooling/qa/wrappers/windows-smoke-from-wsl.sh [options]

Runs the Windows install/probe smoke lane from WSL by invoking tooling/release/windows/smoke/windows-smoke.ps1.
Requires working WSL PowerShell interop and fails immediately when interop is unavailable.
The PowerShell lane writes windows-smoke-evidence.json under .tmp/release/windows-<channel>/.

Options:
  --channel stable|beta|dev          Package channel (default: stable)
  --extension-origin <origin>        Exact chrome-extension://<32-letter-id>/ origin
  --extension-version <version>      Extension version used in extension.hello
  --product-version <version>        MSI product version override
  --wix <path-or-command>            WiX v4 CLI command on Windows (default: wix)
  --probe-count <n>                  Repeated framed app.hello + controller-lease probes (default: 3)
  --timeout-ms <n>                   Per-read probe timeout in milliseconds (default: 30000)
  --skip-checkpoint                  Do not run tooling/qa/wrappers/checkpoint.sh through WSL first
  --dev-local-trust                  Sign with/trust a CurrentUser dev certificate and reprovision pipe secret
  --keep-logs                        Do not archive old native-agent logs before smoke
  -h, --help                         Show this help

PowerShell-style arguments may also be passed directly to windows-smoke.ps1.
EOF
}

ps_args=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --channel)
      ps_args+=("-Channel" "${2:-}")
      shift 2
      ;;
    --channel=*)
      ps_args+=("-Channel" "${1#*=}")
      shift
      ;;
    --extension-origin)
      ps_args+=("-ExtensionOrigin" "${2:-}")
      shift 2
      ;;
    --extension-origin=*)
      ps_args+=("-ExtensionOrigin" "${1#*=}")
      shift
      ;;
    --extension-version)
      ps_args+=("-ExtensionVersion" "${2:-}")
      shift 2
      ;;
    --extension-version=*)
      ps_args+=("-ExtensionVersion" "${1#*=}")
      shift
      ;;
    --product-version)
      ps_args+=("-ProductVersion" "${2:-}")
      shift 2
      ;;
    --product-version=*)
      ps_args+=("-ProductVersion" "${1#*=}")
      shift
      ;;
    --wix)
      ps_args+=("-Wix" "${2:-}")
      shift 2
      ;;
    --wix=*)
      ps_args+=("-Wix" "${1#*=}")
      shift
      ;;
    --probe-count)
      ps_args+=("-ProbeCount" "${2:-}")
      shift 2
      ;;
    --probe-count=*)
      ps_args+=("-ProbeCount" "${1#*=}")
      shift
      ;;
    --timeout-ms)
      ps_args+=("-TimeoutMs" "${2:-}")
      shift 2
      ;;
    --timeout-ms=*)
      ps_args+=("-TimeoutMs" "${1#*=}")
      shift
      ;;
    --skip-checkpoint)
      ps_args+=("-SkipCheckpoint")
      shift
      ;;
    --dev-local-trust)
      ps_args+=("-DevLocalTrust")
      shift
      ;;
    --keep-logs)
      ps_args+=("-KeepLogs")
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      ps_args+=("$1")
      shift
      ;;
  esac
done

if ! command -v wslpath >/dev/null 2>&1; then
  printf 'wslpath is required to locate the Windows repo path.\n' >&2
  exit 1
fi

if command -v powershell.exe >/dev/null 2>&1; then
  powershell_cmd=(powershell.exe -NoProfile -ExecutionPolicy Bypass)
elif command -v pwsh.exe >/dev/null 2>&1; then
  powershell_cmd=(pwsh.exe -NoProfile)
elif [[ -x /mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe ]]; then
  powershell_cmd=(/mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe -NoProfile -ExecutionPolicy Bypass)
else
  printf 'powershell.exe or pwsh.exe is required via WSL interop.\n' >&2
  exit 1
fi

repo_win="$(wslpath -w "$ROOT")"
script_win="$(wslpath -w "$ROOT/tooling/release/windows/smoke/windows-smoke.ps1")"

if ! "${powershell_cmd[@]}" -Command "exit 0" >/dev/null 2>&1; then
  printf 'WSL PowerShell interop is unavailable; run windows-smoke.ps1 directly from Windows PowerShell.\n' >&2
  exit 1
fi
"${powershell_cmd[@]}" -File "$script_win" -Repo "$repo_win" "${ps_args[@]}"
