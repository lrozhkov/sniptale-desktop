#!/usr/bin/env bash
set -euo pipefail

lane="${1:?lane required}"
case "$lane" in
  linux-wayland) session_type=wayland ;;
  linux-x11) session_type=x11 ;;
  *) exit 2 ;;
esac

export DEBIAN_FRONTEND=noninteractive
sed -i \
  -e 's|http://archive.ubuntu.com/ubuntu|https://archive.ubuntu.com/ubuntu|g' \
  -e 's|http://security.ubuntu.com/ubuntu|https://security.ubuntu.com/ubuntu|g' \
  /etc/apt/sources.list /etc/apt/sources.list.d/*.sources 2>/dev/null || true
apt-get update
apt-get install -y --no-install-recommends ca-certificates curl gpg

install -d -m 0755 /etc/apt/keyrings
mozilla_key=/etc/apt/keyrings/packages.mozilla.org.asc
mozilla_url="$({
  curl --fail --silent --show-error --location \
    --proto '=https' --proto-redir '=https' --tlsv1.2 \
    --output "$mozilla_key" --write-out '%{url_effective}' \
    https://packages.mozilla.org/apt/repo-signing-key.gpg
} 2>/dev/null)"
if [[ "$mozilla_url" != https://packages.mozilla.org/* ]]; then
  echo "Mozilla signing key redirected outside packages.mozilla.org" >&2
  exit 1
fi
mozilla_fingerprint="$(gpg --batch --show-keys --with-colons "$mozilla_key" | awk -F: '$1 == "fpr" { print $10; exit }')"
if [[ "$mozilla_fingerprint" != 35BAA0B33E9EB396F59CA838C0BA5CE6DC6315A3 ]]; then
  echo "Mozilla signing key fingerprint mismatch" >&2
  exit 1
fi
cat >/etc/apt/sources.list.d/mozilla.list <<'EOF'
deb [signed-by=/etc/apt/keyrings/packages.mozilla.org.asc] https://packages.mozilla.org/apt mozilla main
EOF
cat >/etc/apt/preferences.d/mozilla <<'EOF'
Package: firefox*
Pin: origin packages.mozilla.org
Pin-Priority: 1000
EOF

google_key=/etc/apt/keyrings/google-linux-signing-key.pub
google_url="$({
  curl --fail --silent --show-error --location \
    --proto '=https' --proto-redir '=https' --tlsv1.2 \
    --output "$google_key" --write-out '%{url_effective}' \
    https://dl.google.com/linux/linux_signing_key.pub
} 2>/dev/null)"
if [[ "$google_url" != https://dl.google.com/* ]]; then
  echo "Google signing key redirected outside dl.google.com" >&2
  exit 1
fi
google_fingerprint="$(gpg --batch --show-keys --with-colons "$google_key" | awk -F: '$1 == "fpr" { print $10; exit }')"
if [[ "$google_fingerprint" != EB4C1BFD4F042F6DDDCCEC917721F63BD38B4796 ]]; then
  echo "Google Linux signing key fingerprint mismatch" >&2
  exit 1
fi
gpg --batch --yes --dearmor --output /etc/apt/keyrings/google-linux-signing-key.gpg "$google_key"
cat >/etc/apt/sources.list.d/google-chrome.list <<'EOF'
deb [arch=amd64 signed-by=/etc/apt/keyrings/google-linux-signing-key.gpg] https://dl.google.com/linux/chrome/deb/ stable main
EOF
apt-get update
apt-get install -y --no-install-recommends \
  ubuntu-desktop-minimal gdm3 gnome-shell gnome-session gnome-terminal \
  gnome-shell-extension-appindicator xdg-desktop-portal xdg-desktop-portal-gnome \
  pipewire pipewire-pulse wireplumber pulseaudio-utils alsa-utils \
  xserver-xorg xserver-xorg-video-qxl dbus-x11 at-spi2-core python3-gi \
  gir1.2-gtk-3.0 gir1.2-appindicator3-0.1 python3-pyatspi \
  gnome-screenshot imagemagick openssh-server jq \
  google-chrome-stable firefox python3-selenium wmctrl xdotool

if dpkg-query -W -f='${Version}\n' firefox | grep -qi snap; then
  echo "Firefox resolved to the Ubuntu snap transition package" >&2
  exit 1
fi

install -d -m 0755 /etc/gdm3
if [[ "$session_type" == x11 ]]; then
  wayland=false
  session=ubuntu-xorg
else
  wayland=true
  session=ubuntu
fi
cat >/etc/gdm3/custom.conf <<EOF
[daemon]
AutomaticLoginEnable=true
AutomaticLogin=lab
WaylandEnable=$wayland
DefaultSession=$session.desktop

[security]

[xdmcp]

[chooser]

[debug]
Enable=false
EOF

install -d -o lab -g lab -m 0700 /home/lab/.config
touch /home/lab/.config/gnome-initial-setup-done
chown lab:lab /home/lab/.config/gnome-initial-setup-done
cat >/home/lab/.dmrc <<EOF
[Desktop]
Session=$session
EOF
chown lab:lab /home/lab/.dmrc

install -d -m 0755 /etc/netplan
install -d -m 0755 /etc/cloud/cloud.cfg.d
cat >/etc/cloud/cloud.cfg.d/99-sniptale-disable-network-config.cfg <<'EOF'
network: {config: disabled}
EOF
rm -f /etc/netplan/50-cloud-init.yaml
cat >/etc/netplan/01-sniptale-lab.yaml <<'EOF'
network:
  version: 2
  renderer: networkd
  ethernets:
    lab0:
      match:
        macaddress: "52:54:00:12:34:56"
      set-name: enp0s1
      addresses: [10.0.2.15/24]
      routes:
        - to: default
          via: 10.0.2.2
      nameservers:
        addresses: [10.0.2.3]
      dhcp6: false
      optional: true
EOF
chmod 0600 /etc/netplan/01-sniptale-lab.yaml
netplan generate

install -d -m 0755 /etc/systemd/system/getty@tty1.service.d
cat >/etc/systemd/system/getty@tty1.service.d/autologin.conf <<'EOF'
[Service]
ExecStart=
ExecStart=-/sbin/agetty --autologin lab --noclear %I $TERM
EOF

systemctl enable ssh gdm3
loginctl enable-linger lab
install -d -m 0755 /var/lib/sniptale-e2e
printf '%s\n' "$lane" >/var/lib/sniptale-e2e/lane
printf 'provisioned\n' >/var/lib/sniptale-e2e/provisioned
sync
systemctl poweroff
