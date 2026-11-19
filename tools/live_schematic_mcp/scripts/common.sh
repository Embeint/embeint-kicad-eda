# SPDX-License-Identifier: GPL-3.0-or-later
# Shared paths for the live MCP scripts.  Source it; do not run it.
#
#   KICAD_LIVE_ROOT  where the build lives (default ~/kicad-live): src/ build/ install/ deps/ venv/ config/
#   KICAD_BUILD_JOBS parallel compile jobs (default 8).  Each KiCad compile job can take about 1 GB,
#                    so an uncapped build can exhaust WSL memory and kill other work on the machine.

ROOT="${KICAD_LIVE_ROOT:-$HOME/kicad-live}"
SCRIPTS="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MCP="$(cd "$SCRIPTS/.." && pwd)"
REPO="$(cd "$MCP/../.." && pwd)"
PYTHON="${KICAD_LIVE_PYTHON:-$ROOT/venv/bin/python}"
POPPLER="$ROOT/deps/usr"
JOBS="${KICAD_BUILD_JOBS:-8}"

# Regenerate the Python protocol code whenever a .proto is newer than the last generation
ensure_protos() {
    local stamp="$MCP/_generated/.stamp"
    if [[ ! -f "$stamp" ]] || [[ -n "$(find "$REPO/api/proto" -name '*.proto' -newer "$stamp" -print -quit)" ]]; then
        "$PYTHON" "$MCP/generate_proto.py" >&2 && touch "$stamp"
    fi
}
