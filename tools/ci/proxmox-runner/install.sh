#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Run as root on an EXISTING Linux x64 build runner, never a deployment host.
# Build first, then pipe a repository registration token to this script on stdin.
set -euo pipefail
[[ $(id -u) == 0 && $(uname -m) == x86_64 ]]
image=${1:?usage: install.sh LOCAL_IMAGE (registration token on stdin)}
name=kicad-ci
network=kicad-ci
if docker container inspect "$name" >/dev/null 2>&1; then
    echo 'Existing KiCad container retained; inspect it before changing its installation.' >&2
    exit 1
fi
script_dir=$(cd -- "$(dirname -- "$0")" && pwd)
bash "$script_dir/firewall.sh"
# Docker restores containers before an operator can restore these rules after
# reboot, so fail closed: no restart policy. Start only through start.sh.
docker create --name "$name" --network "$network" --dns 1.1.1.1 --dns 1.0.0.1 \
    --cpus 2 --memory 2560m --memory-swap 2560m --pids-limit 512 \
    --cap-drop ALL --security-opt no-new-privileges \
    --volume kicad-ci-runner:/runner --volume kicad-ci-home:/home/runner \
    "$image" >/dev/null
# Registration never enters an image layer, host file, log, or command argument.
# Config is supplied as a process environment variable only inside the container.
IFS= read -r token
printf '%s\n' "$token" | docker run --rm -i --network "$network" \
    --cap-drop ALL --security-opt no-new-privileges \
    --volume kicad-ci-runner:/runner --volume kicad-ci-home:/home/runner \
    --entrypoint /bin/bash "$image" -c '
        set -euo pipefail
        IFS= read -r registration_token
        export ACTIONS_RUNNER_INPUT_TOKEN="$registration_token"
        ./config.sh --unattended --url https://github.com/Embeint/embeint-kicad-eda \
          --name proxmox-kicad-isolated-01 --no-default-labels \
          --labels kicad-ci-isolated-linux-x64 --work _work --disableupdate
    '
unset token
echo "Registered. Install kicad-ci.service, then start it to apply the firewall before listening."
