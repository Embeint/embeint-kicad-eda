#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Live (development KiCad) halves of the KiCad 10 compatibility checks.  Each step starts the
# development KiCad on a disposable COPY, runs the check over IPC and stops only the KiCad it started.
# Hold the shared build lock first if other agents use $KICAD_LIVE_ROOT.
#
#   compat/run_live.sh fixture   OUT STEP_FILE      author a new compat-fixture project in OUT (MCP)
#   compat/run_live.sh exports   OUT [--update]     every MCP export of the committed fixture vs golden
#   compat/run_live.sh roundtrip OUT PROJECT_DIR    open/save a reference-authored project, restart, verify
#   compat/run_live.sh cli-libs  OUT PROJECT_DIR    rewrite its symbol/footprint libraries with the fork CLI
#
# OUT must not exist.  Reports are written below OUT; inputs are never modified.

set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/../scripts/common.sh"
COMPAT="$MCP/compat"
INSTALL="$ROOT/install"
step="${1:?step}"
out="${2:?output folder}"
mkdir -p "$(dirname "$out")"
mkdir "$out"
out="$(cd "$out" && pwd)"
ensure_protos
export PYTHONPATH="$MCP"
kicad_pid=""

# One development KiCad at a time: name its socket so discovery never picks a stale one
export KICAD_API_SOCKET="${KICAD_API_SOCKET:-ipc:///tmp/kicad/api.sock}"

start_kicad() {  # project.kicad_pro log
    "$MCP/run_kicad_wsl.sh" "$1" > "$2" 2>&1 &
    kicad_pid=$!
    # run_kicad_wsl.sh first refreshes the local library copies, which can take minutes
    for _ in $(seq 300); do
        kill -0 "$kicad_pid" 2>/dev/null || { echo "KiCad exited: $2" >&2; exit 1; }
        if (cd "$MCP" && "$PYTHON" -c 'from client import LiveSchematic, base
LiveSchematic(timeout_ms=2000).send(base.GetVersion(), base.GetVersionResponse)') >/dev/null 2>&1; then
            return 0
        fi
        sleep 2
    done
    stop_kicad
    echo "KiCad did not answer: $2" >&2
    exit 1
}

stop_kicad() {
    if [[ -n "$kicad_pid" ]] && kill -0 "$kicad_pid" 2>/dev/null; then
        kill "$kicad_pid"
        for _ in $(seq 30); do kill -0 "$kicad_pid" 2>/dev/null || break; sleep 1; done
        kill -9 "$kicad_pid" 2>/dev/null || true
    fi
    wait "$kicad_pid" 2>/dev/null || true
    kicad_pid=""
}
trap stop_kicad EXIT

copy_project() {  # source destination: project files only (no backups, locks or caches)
    rsync -a --exclude '*-backups/' --exclude '*.lck' --exclude '~*' --exclude 'fp-info-cache' \
        --exclude '_autosave-*' "$1/" "$2/"
}

case "$step" in
    fixture)
        project="$out/compat-fixture"
        mkdir -p "$project"
        printf '{\n  "meta": { "filename": "compat-fixture.kicad_pro", "version": 3 }\n}\n' \
            > "$project/compat-fixture.kicad_pro"
        # Minimal empty root sheet; the fork's writer replaces it on the explicit save
        printf '(kicad_sch (version 20250114) (generator "eeschema")\n (uuid "%s") (paper "A4")\n (lib_symbols) (sheet_instances (path "/" (page "1"))))\n' \
            "$(cat /proc/sys/kernel/random/uuid)" > "$project/compat-fixture.kicad_sch"
        start_kicad "$project/compat-fixture.kicad_pro" "$out/kicad.log"
        KICAD_LIVE_TEST_PROJECT=compat-fixture "$PYTHON" "$COMPAT/build_fixture.py" "${3:?STEP file}" \
            "$out/build-report.json" 2>&1 | tee "$out/build.log"
        ;;
    exports)
        copy_project "$COMPAT/fixture/compat-fixture" "$out/compat-fixture"
        start_kicad "$out/compat-fixture/compat-fixture.kicad_pro" "$out/kicad.log"
        KICAD_LIVE_TEST_PROJECT=compat-fixture "$PYTHON" "$COMPAT/check_compat.py" fork-exports \
            --out "$out/exports" ${3:-} 2>&1 | tee "$out/exports.log"
        ;;
    roundtrip)
        source_dir="${3:?reference-authored project folder}"
        name="$(basename "$source_dir")"
        copy_project "$source_dir" "$out/$name"
        pro="$(ls "$out/$name"/*.kicad_pro)"
        export KICAD_LIVE_TEST_PROJECT="$(basename "$pro" .kicad_pro)"
        start_kicad "$pro" "$out/kicad-save.log"
        "$PYTHON" "$COMPAT/check_compat.py" fork-roundtrip save --state "$out/roundtrip.json" 2>&1 | tee "$out/save.log"
        stop_kicad
        start_kicad "$pro" "$out/kicad-verify.log"
        "$PYTHON" "$COMPAT/check_compat.py" fork-roundtrip verify --state "$out/roundtrip.json" 2>&1 | tee "$out/verify.log"
        ;;
    cli-libs)
        source_dir="${3:?project folder}"
        export LD_LIBRARY_PATH="$ROOT/deps/usr/lib/x86_64-linux-gnu:$INSTALL/lib:${LD_LIBRARY_PATH:-}"
        export KICAD_CONFIG_HOME="$ROOT/config"
        "$INSTALL/bin/kicad-cli" version --format about > "$out/fork-version.txt" 2>&1 || true
        for library in "$source_dir"/*.kicad_sym; do
            "$INSTALL/bin/kicad-cli" sym upgrade --force -o "$out/$(basename "$library")" "$library" \
                >> "$out/cli.log" 2>&1; echo "sym $(basename "$library") rc=$?" | tee -a "$out/cli.log"
        done
        for library in "$source_dir"/*.pretty; do
            "$INSTALL/bin/kicad-cli" fp upgrade --force -o "$out/$(basename "$library")" "$library" \
                >> "$out/cli.log" 2>&1; echo "fp $(basename "$library") rc=$?" | tee -a "$out/cli.log"
        done
        "$PYTHON" "$COMPAT/check_compat.py" versions "$out" > "$out/versions.json"
        ;;
    *)
        echo "unknown step $step" >&2
        exit 2
        ;;
esac
