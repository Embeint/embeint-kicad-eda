#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Run inside the qualification container; /qualification/src is a source snapshot.
set -euo pipefail

root=/qualification
source_dir="$root/src"
revision="${KICAD_SOURCE_REVISION:?Set KICAD_SOURCE_REVISION to the snapshot commit}"
jobs="${KICAD_BUILD_JOBS:-1}"
optimization="${KICAD_BUILD_OPTIMIZATION:--O1}"
mkdir -p "$root/build" "$root/evidence" /opt/embeint-kicad

printf '%s\n' "$revision" > "$root/evidence/source-revision.txt"
dpkg-query -W > "$root/evidence/build-packages.txt"
uname -a > "$root/evidence/build-platform.txt"

cmake -S "$source_dir" -B "$root/build" -G Ninja \
    -DCMAKE_BUILD_TYPE=Release -DCMAKE_CXX_FLAGS_RELEASE="$optimization -DNDEBUG" \
    -DCMAKE_EXE_LINKER_FLAGS=-fuse-ld=lld -DCMAKE_SHARED_LINKER_FLAGS=-fuse-ld=lld \
    -DCMAKE_INSTALL_PREFIX=/opt/embeint-kicad -DCMAKE_INSTALL_LIBDIR=lib \
    -DKICAD_BUILD_QA_TESTS=OFF -DKICAD_USE_SENTRY=OFF \
    -DKICAD_INSTALL_DEMOS=OFF -DKICAD_BUILD_I18N=OFF \
    2>&1 | tee "$root/evidence/configure.log"
cmake --build "$root/build" --parallel "$jobs" 2>&1 | tee "$root/evidence/build.log"
cmake --install "$root/build" 2>&1 | tee "$root/evidence/install.log"

python3 -m venv "$root/venv"
"$root/venv/bin/python" -m pip install -r "$source_dir/tools/live_schematic_mcp/requirements.txt" \
    2>&1 | tee "$root/evidence/python-install.log"
"$root/venv/bin/python" -m pip freeze > "$root/evidence/python-packages.txt"
"$root/venv/bin/python" "$source_dir/tools/live_schematic_mcp/generate_proto.py"
"$root/venv/bin/python" -m pytest -q -p no:cacheprovider "$source_dir/tools/live_schematic_mcp" \
    2>&1 | tee "$root/evidence/offline-tests.log"
