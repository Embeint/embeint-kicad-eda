#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
set -euo pipefail
[[ $(id -u) == 0 ]]
network=kicad-ci
bridge=br-kicad-ci
# This bridge and these rules apply only to the new container. Never flush
# shared firewall tables or change another runner/service.
if ! docker network inspect "$network" >/dev/null 2>&1; then
    docker network create --driver bridge --subnet 172.30.240.0/24 \
        --opt com.docker.network.bridge.name="$bridge" \
        --opt com.docker.network.bridge.enable_icc=false "$network" >/dev/null
fi
# Refuse a colliding network name rather than attach to an unverified bridge.
test "$(docker network inspect --format '{{.EnableIPv6}}' "$network")" = false
test "$(docker network inspect --format '{{index .Options "com.docker.network.bridge.name"}}' "$network")" = "$bridge"
test "$(docker network inspect --format '{{(index .IPAM.Config 0).Subnet}}' "$network")" = 172.30.240.0/24
iptables -N KICAD-CI-EGRESS 2>/dev/null || iptables -S KICAD-CI-EGRESS >/dev/null
for subnet in 0.0.0.0/8 10.0.0.0/8 100.64.0.0/10 127.0.0.0/8 169.254.0.0/16 172.16.0.0/12 192.168.0.0/16 224.0.0.0/4 240.0.0.0/4; do
    iptables -C KICAD-CI-EGRESS -d "$subnet" -j REJECT 2>/dev/null ||
        iptables -A KICAD-CI-EGRESS -d "$subnet" -j REJECT
done
iptables -C INPUT -i "$bridge" -j REJECT 2>/dev/null ||
    iptables -I INPUT 1 -i "$bridge" -j REJECT
# Tailscale's FORWARD accept rule may precede DOCKER-USER. Enforce the
# bridge boundary ahead of it, rather than relying on DOCKER-USER ordering.
if iptables -C FORWARD -i "$bridge" -j KICAD-CI-EGRESS 2>/dev/null; then
    iptables -D FORWARD -i "$bridge" -j KICAD-CI-EGRESS
fi
iptables -I FORWARD 1 -i "$bridge" -j KICAD-CI-EGRESS
