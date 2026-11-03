#!/usr/bin/env bash
# Shared environment for the pinned script tools; sourced by their wrappers.
script_tools_env() {
  local root="$1"
  local python_version node_version
  read -r python_version node_version < <(
    python3 - "$root" <<'PY'
import json
import re
import sys
from pathlib import Path
value = json.loads((Path(sys.argv[1]) / 'tooling/configs/qa/tool-versions.json').read_text())
versions = [value['python']['version'], value['browserAutomation']['node']]
if any(not isinstance(version, str) or re.fullmatch(r'[0-9]+\.[0-9]+\.[0-9]+', version) is None for version in versions):
    raise SystemExit('Invalid cataloged runtime version')
print(*versions)
PY
  )
  if [[ -x "$root/.tmp/qa-tools/python-$python_version/bin/python3" ]]; then
    export PATH="$root/.tmp/qa-tools/python-$python_version/bin:$PATH"
    export LD_LIBRARY_PATH="$root/.tmp/qa-tools/python-$python_version/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
  fi
  if [[ "$(python3 -c 'import platform; print(platform.python_version())')" != "$python_version" ]]; then
    printf 'Install the cataloged Python version before script checks: %s\n' "$python_version" >&2
    return 1
  fi
  if [[ -x "$root/.tmp/qa-tools/node-$node_version/bin/node" ]]; then
    export PATH="$root/.tmp/qa-tools/node-$node_version/bin:$PATH"
  fi
  if [[ "${2:-}" == --require-node ]] && [[ "$(node --version)" != "v$node_version" ]]; then
    printf 'Install the cataloged Node.js version before script checks: %s\n' "$node_version" >&2
    return 1
  fi
}
