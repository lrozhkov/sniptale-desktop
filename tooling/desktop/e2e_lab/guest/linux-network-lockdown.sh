#!/usr/bin/env bash
set -euo pipefail

iptables -P OUTPUT ACCEPT
iptables -F OUTPUT
iptables -A OUTPUT -o lo -j ACCEPT
iptables -A OUTPUT -m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT
iptables -P OUTPUT DROP
policy="$(iptables -S OUTPUT)"
grep -Fx -- '-P OUTPUT DROP' <<<"$policy" >/dev/null
iptables -C OUTPUT -m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT
printf '%s\n' output-drop-v1 >/run/sniptale-e2e-network-policy
