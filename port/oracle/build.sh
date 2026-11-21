#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
set -euo pipefail

if [[ $# -lt 2 || $# -gt 3 ]]; then
    echo 'usage: bash build.sh STAGE_DIR ORACLE_OVERLAY [WORK_ROOT]' >&2
    exit 2
fi
stage=$(realpath "$1")
overlay=$(realpath "$2")
work=$(realpath -m "${3:-$HOME/kicad-port}")
live=$(realpath -m "$HOME/kicad-live")
case "$work/" in "$live/"*) echo 'Refusing the shared kicad-live tree' >&2; exit 2 ;; esac
case "$work" in /|"$HOME"|/mnt/*) echo 'Use a dedicated Linux work root' >&2; exit 2 ;; esac
jobs=${ORACLE_JOBS:-8}
if [[ ! "$jobs" =~ ^[1-8]$ ]]; then
    echo 'ORACLE_JOBS must be between 1 and 8' >&2
    exit 2
fi

# A busy lock is an explicit retry, never removed on another worker's behalf.
lock="$HOME/kicad-port.build-lock"
if ! mkdir "$lock"; then
    echo "Build lock busy: $lock; retry after its owner finishes" >&2
    exit 3
fi
trap 'rmdir "$lock"' EXIT
mkdir -p "$work/oracle-src/reference" "$work/oracle-src/overlay" "$work/oracle-build"
python3 "$overlay/verify_archive.py" "$stage"
tar -xf "$stage/reference.tar" -C "$work/oracle-src/reference"
cp "$stage/source-manifest.json" "$work/oracle-src/reference/source-manifest.json"
rsync -a --delete --exclude=__pycache__ "$overlay/" "$work/oracle-src/overlay/"
cmake -S "$work/oracle-src/overlay" -B "$work/oracle-build" -G Ninja \
    -DCMAKE_BUILD_TYPE=Release -DCMAKE_C_COMPILER="${CC:-gcc}" \
    -DCMAKE_CXX_COMPILER="${CXX:-g++}" \
    -DKICAD_REFERENCE_SOURCE="$work/oracle-src/reference"
cmake --build "$work/oracle-build" --target kicad-oracle oracle-dispatch-test -- -j"$jobs"
ctest --test-dir "$work/oracle-build" --output-on-failure
sha256sum "$work/oracle-build/kicad-oracle" > "$work/oracle-build/binary.sha256"
echo "Oracle: $work/oracle-build/kicad-oracle"
echo "Provenance: $work/oracle-build/oracle-provenance.json"
