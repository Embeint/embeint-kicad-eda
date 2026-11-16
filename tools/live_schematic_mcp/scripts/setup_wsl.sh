#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# One-time setup of the development KiCad and the live MCP under WSL (Ubuntu 24.04+), without root
# beyond the packages it lists.  Idempotent: rerun after a pull.
#
#   scripts/setup_wsl.sh            check packages, unpack poppler-glib locally, configure, build,
#                                   install, create the venv and generate the protocol code
#
# The source is copied to $KICAD_LIVE_ROOT/src and built there: building straight from /mnt/c is
# several times slower.

set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

PACKAGES=(build-essential cmake ninja-build gettext swig python3-dev python3-venv libwxgtk3.2-dev
          libwxgtk-webview3.2-dev libglm-dev libglew-dev libcairo2-dev libboost-all-dev unixodbc-dev
          libharfbuzz-dev libfontconfig-dev libwayland-dev libgtk-3-dev libssl-dev libpixman-1-dev
          libxml2-dev libsqlite3-dev libeigen3-dev libgit2-dev libsecret-1-dev libzstd-dev
          protobuf-compiler libprotobuf-dev libabsl-dev libcurl4-openssl-dev libspnav-dev
          libngspice0-dev libocct-data-exchange-dev libocct-visualization-dev rsync)

missing=()
for package in "${PACKAGES[@]}"; do
    dpkg -s "$package" >/dev/null 2>&1 || missing+=("$package")
done
if (( ${#missing[@]} )); then
    echo "Install these first:  sudo apt-get install ${missing[*]}" >&2
    exit 1
fi

mkdir -p "$ROOT"/{src,build,install,deps,debs,config}

# poppler-glib headers are often absent; unpack the .debs locally instead of asking for root
if ! dpkg -s libpoppler-glib-dev >/dev/null 2>&1 && [[ ! -f "$POPPLER/include/poppler/glib/poppler.h" ]]; then
    (cd "$ROOT/debs" && apt-get download libpoppler-glib-dev libpoppler-glib8t64)
    for deb in "$ROOT"/debs/*.deb; do dpkg -x "$deb" "$ROOT/deps"; done
fi

rsync -a --delete --exclude .git --exclude tools/live_schematic_mcp/_generated "$REPO/" "$ROOT/src/"

if [[ ! -f "$ROOT/build/build.ninja" ]]; then
    extra=()
    if [[ -f "$POPPLER/include/poppler/glib/poppler.h" ]]; then
        extra=(-DPoppler_Glib_INCLUDE_DIR="$POPPLER/include"
               -DPoppler_Glib_LIBRARY="$POPPLER/lib/x86_64-linux-gnu/libpoppler-glib.so")
    fi
    cmake -S "$ROOT/src" -B "$ROOT/build" -G Ninja -DCMAKE_BUILD_TYPE=Release -DKICAD_BUILD_QA_TESTS=OFF \
          -DKICAD_USE_SENTRY=OFF -DCMAKE_INSTALL_PREFIX="$ROOT/install" "${extra[@]}"
fi

ninja -C "$ROOT/build"
ninja -C "$ROOT/build" install

if [[ ! -x "$PYTHON" ]]; then
    python3 -m venv "$ROOT/venv"
fi
"$PYTHON" -m pip install -q -r "$MCP/requirements.txt"
ensure_protos

echo "Ready.  Start KiCad:  $MCP/run_kicad_wsl.sh    Register the MCP:  see $MCP/README.md"
