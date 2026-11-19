#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Rebuild and install after editing the C++ or .proto files.  Stops a running development
# eeschema first (it is using the installed libraries), copies changed sources and builds.
# Prints the first compile errors on failure; the full log is $KICAD_LIVE_ROOT/build.log.

set -uo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

pkill -x eeschema 2>/dev/null && sleep 1
rsync -rc --exclude .git --exclude tools/live_schematic_mcp --exclude rust/target --out-format='synced %n' "$REPO/" "$ROOT/src/" \
    | grep -v "non-regular" | grep -v "_help_\|help_md"
ninja -j "$JOBS" -C "$ROOT/build" > "$ROOT/build.log" 2>&1
status=$?
if (( status )); then
    grep -E "^FAILED|error:" "$ROOT/build.log" | head -20
    echo "build failed: see $ROOT/build.log" >&2
    exit "$status"
fi
tail -1 "$ROOT/build.log"
ninja -j "$JOBS" -C "$ROOT/build" install > "$ROOT/install.log" 2>&1 && echo installed
ensure_protos
