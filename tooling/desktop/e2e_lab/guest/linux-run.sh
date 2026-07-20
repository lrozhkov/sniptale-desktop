#!/usr/bin/env bash
set -euo pipefail

lane="${1:?lane required}"
package="${2:?package required}"
expected_sha256="${3:?sha256 required}"
artifact_kind="${4:?artifact kind required}"
package_name="${5:?package name required}"
executable="${6:?executable required}"
output="$HOME/e2e-out"
actual_sha256="$(sha256sum "$package" | awk '{print $1}')"
if [[ "$actual_sha256" != "$expected_sha256" ]]; then
  echo "artifact checksum mismatch" >&2
  exit 3
fi

rm -rf "$output"
mkdir -p "$output/evidence"
case "$artifact_kind" in
  deb-v1)
    sudo dpkg --purge "$package_name" >/dev/null 2>&1 || true
    sudo dpkg -i "$package"
    ;;
  appimage-v1)
    chmod 0755 "$package"
    ;;
  *)
    echo "unsupported Linux artifact adapter" >&2
    exit 4
    ;;
esac

probe_status=0
probe_command=("$executable")
if [[ "$artifact_kind" == appimage-v1 ]]; then
  probe_command=(env APPIMAGE_EXTRACT_AND_RUN=1 "$executable")
fi
sniptale-session-exec "${probe_command[@]}" --e2e-probe \
  --lane "$lane" \
  --output "$output/result.json" \
  --evidence "$output/evidence" || probe_status=$?

if [[ "$artifact_kind" == deb-v1 ]]; then
  sudo dpkg --purge "$package_name"
  if dpkg-query -W "$package_name" >/dev/null 2>&1; then
    lifecycle_status=fail
    lifecycle_detail="package remains installed after purge"
  else
    lifecycle_status=pass
    lifecycle_detail="dpkg install and purge completed"
  fi
else
  rm -f -- "$package"
  if [[ -e "$package" ]]; then
    lifecycle_status=fail
    lifecycle_detail="portable artifact remains after cleanup"
  else
    lifecycle_status=pass
    lifecycle_detail="portable launch and cleanup completed"
  fi
fi

python3 - "$output/result.json" "$lifecycle_status" "$lifecycle_detail" <<'PY'
import json
import sys
from pathlib import Path

path = Path(sys.argv[1])
value = json.loads(path.read_text(encoding="utf-8"))
value["tests"].append({"name": "package-lifecycle", "status": sys.argv[2], "duration_ms": 0, "detail": sys.argv[3]})
path.write_text(json.dumps(value, sort_keys=True, indent=2) + "\n", encoding="utf-8")
PY

if [[ "$probe_status" -ne 0 || "$lifecycle_status" != pass ]]; then
  exit 1
fi
