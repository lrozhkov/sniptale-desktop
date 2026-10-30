#!/usr/bin/env bash
set -euo pipefail

uid="$(id -u)"
runtime="/run/user/$uid"
export XDG_RUNTIME_DIR="$runtime"
export DBUS_SESSION_BUS_ADDRESS="unix:path=$runtime/bus"
session_id=""
while read -r candidate _; do
  if [[ "$(loginctl show-session "$candidate" -p User --value)" == "$uid" ]] &&
    [[ "$(loginctl show-session "$candidate" -p Seat --value)" == "seat0" ]]; then
    session_id="$candidate"
    break
  fi
done < <(loginctl list-sessions --no-legend)
if [[ -z "$session_id" ]]; then
  echo "local console session not found" >&2
  exit 1
fi
session_type="$(loginctl show-session "$session_id" -p Type --value)"
display="$(loginctl show-session "$session_id" -p Display --value)"
export XDG_SESSION_TYPE="$session_type"
while IFS='=' read -r name value; do
  case "$name" in
    DISPLAY | WAYLAND_DISPLAY | XAUTHORITY | XDG_CURRENT_DESKTOP | XDG_SESSION_DESKTOP | DESKTOP_SESSION)
      export "$name=$value"
      ;;
  esac
done < <(systemctl --user show-environment)
if [[ "$session_type" == wayland ]]; then
  export WAYLAND_DISPLAY="${WAYLAND_DISPLAY:-wayland-0}"
  export DISPLAY="${DISPLAY:-${display:-:0}}"
else
  export DISPLAY="${DISPLAY:-${display:-:0}}"
fi
if [[ -z "${XAUTHORITY:-}" ]]; then
  if [[ -f "$runtime/gdm/Xauthority" ]]; then
    export XAUTHORITY="$runtime/gdm/Xauthority"
  else
    authority="$(find "$runtime" -maxdepth 1 -type f -name '.mutter-Xwaylandauth.*' -print -quit)"
    [[ -z "$authority" ]] || export XAUTHORITY="$authority"
  fi
fi
exec "$@"
