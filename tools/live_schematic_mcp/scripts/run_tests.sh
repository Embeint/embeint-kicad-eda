#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
#   scripts/run_tests.sh           unit tests (no KiCad needed)
#   scripts/run_tests.sh --live    also the end-to-end tests against a running development KiCad
#                                  (start it with run_kicad_wsl.sh).  The SolidCraft handoff test
#                                  runs when SOLIDCRAFT_CLI points at solidcraft-cli.
#
# The live tests never save the open schematic; library tests use scratch libraries in
# $KICAD_LIVE_ROOT/libs and remove them.  They run library tests first on purpose: adding and
# removing global libraries must not break placing stock symbols afterwards.

set -uo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"
ensure_protos
cd "$MCP"

"$PYTHON" -m pytest -q -p no:cacheprovider test_client.py || exit 1
[[ "${1:-}" == "--live" ]] || exit 0

"$SCRIPTS/wait_ready.sh" >/dev/null || exit 1
out="${KICAD_LIVE_TEST_OUTPUT:-$ROOT/test-output}"
mkdir -p "$out"
tests=(e2e_symbols e2e_footprints)
[[ -n "${SOLIDCRAFT_CLI:-}" ]] && tests+=(e2e_model_handoff)
tests+=(e2e_smoke e2e_features)
failed=0
for test in "${tests[@]}"; do
    echo "--- $test"
    if ! "$PYTHON" "$test.py" "$out" > "$out/$test.log" 2>&1; then
        failed=1
    fi
    tail -3 "$out/$test.log"
done
[[ -z "${SOLIDCRAFT_CLI:-}" ]] && echo "(skipped e2e_model_handoff: set SOLIDCRAFT_CLI)"
echo "images and logs: $out"
exit "$failed"
