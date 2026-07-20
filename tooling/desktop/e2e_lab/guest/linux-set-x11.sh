#!/usr/bin/env bash
set -euo pipefail

sed -i \
  -e 's/^WaylandEnable=.*/WaylandEnable=false/' \
  -e 's/^DefaultSession=.*/DefaultSession=ubuntu-xorg.desktop/' \
  /etc/gdm3/custom.conf
cat >/home/lab/.dmrc <<'EOF'
[Desktop]
Session=ubuntu-xorg
EOF
chown lab:lab /home/lab/.dmrc
printf '%s\n' linux-x11 >/var/lib/sniptale-e2e/lane
sync
systemd-run --unit=sniptale-lab-poweroff --on-active=2s /usr/bin/systemctl poweroff >/dev/null
