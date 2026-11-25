#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
set -euo pipefail
script_dir=$(cd -- "$(dirname -- "$0")" && pwd)
bash "$script_dir/firewall.sh"
exec docker start --attach kicad-ci
