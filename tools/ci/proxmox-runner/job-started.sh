#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
set -euo pipefail
# Defense in depth against accidental public-PR routing. Container/network
# isolation remains required: workflow authors with write access are trusted.
if [[ ${GITHUB_REPOSITORY:-} != Embeint/embeint-kicad-eda ]] ||
   [[ ${GITHUB_EVENT_NAME:-} != push && ${GITHUB_EVENT_NAME:-} != workflow_dispatch ]]; then
    echo '::error::This runner accepts only Embeint KiCad push/manual jobs; PR jobs belong on hosted runners.'
    exit 1
fi
