#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Launch the development KiCad build (10.99) under WSLg with the IPC API server enabled.
# Uses an isolated configuration directory so it never touches an installed KiCad's settings.
#
#   run_kicad_wsl.sh [file.kicad_sch | project.kicad_pro]
#       default: creates ~/kicad-live/projects/live-demo and opens its schematic
#
# Environment overrides:
#   KICAD_LIVE_ROOT     build root (default ~/kicad-live, expects install/ and deps/ inside)
#   KICAD_LIBRARY_ROOT  KiCad library share dir (default: the Windows KiCad 10.0 install via /mnt/c)

set -euo pipefail

ROOT="${KICAD_LIVE_ROOT:-$HOME/kicad-live}"
INSTALL="$ROOT/install"
LIBS="${KICAD_LIBRARY_ROOT:-/mnt/c/Program Files/KiCad/10.0/share/kicad}"
CONFIG_HOME="$ROOT/config"
CONFIG_DIR="$CONFIG_HOME/10.99"

if [[ ! -x "$INSTALL/bin/kicad" ]]; then
    echo "Development KiCad not installed at $INSTALL (run: ninja -C $ROOT/build install)" >&2
    exit 1
fi

mkdir -p "$CONFIG_DIR"

# Enable the API server and acknowledge the first-run privacy prompts; otherwise the modal setup
# wizard blocks startup and the API answers AS_NOT_READY. Merged so other preferences are kept.
python3 - "$CONFIG_DIR/kicad_common.json" <<'PY'
import json, sys
path = sys.argv[1]
try:
    with open(path) as f:
        data = json.load(f)
except (OSError, ValueError):
    data = {}
data.setdefault("api", {})["enable_server"] = True
prompts = data.setdefault("do_not_show_again", {})
prompts["data_collection_prompt"] = True
prompts["update_check_prompt"] = True
with open(path, "w") as f:
    json.dump(data, f, indent=2)
PY

# Global library tables referencing the installed KiCad 10 libraries (versioned variables).
# All three must exist and parse, or the setup wizard runs.
for table in sym-lib-table fp-lib-table; do
    if [[ ! -f "$CONFIG_DIR/$table" && -f "$LIBS/template/$table" ]]; then
        install -m 644 "$LIBS/template/$table" "$CONFIG_DIR/$table"
    fi
done
if [[ ! -f "$CONFIG_DIR/design-block-lib-table" ]]; then
    printf '(design_block_lib_table\n  (version 7)\n)\n' > "$CONFIG_DIR/design-block-lib-table"
fi

export KICAD_CONFIG_HOME="$CONFIG_HOME"
for ver in 10 11; do
    export "KICAD${ver}_SYMBOL_DIR=$LIBS/symbols"
    export "KICAD${ver}_FOOTPRINT_DIR=$LIBS/footprints"
    export "KICAD${ver}_3DMODEL_DIR=$LIBS/3dmodels"
    export "KICAD${ver}_TEMPLATE_DIR=$LIBS/template"
done

# poppler-glib may be unpacked locally (no root needed) instead of installed system-wide.
if [[ -d "$ROOT/deps/usr/lib/x86_64-linux-gnu" ]]; then
    export LD_LIBRARY_PATH="$ROOT/deps/usr/lib/x86_64-linux-gnu:$INSTALL/lib:${LD_LIBRARY_PATH:-}"
else
    export LD_LIBRARY_PATH="$INSTALL/lib:${LD_LIBRARY_PATH:-}"
fi

TARGET="${1:-}"
if [[ -z "$TARGET" ]]; then
    DEMO="$ROOT/projects/live-demo"
    PROJECT="$DEMO/live-demo.kicad_pro"
    TARGET="$DEMO/live-demo.kicad_sch"
    if [[ ! -f "$PROJECT" ]]; then
        mkdir -p "$DEMO"
        printf '{\n  "meta": { "filename": "live-demo.kicad_pro", "version": 3 }\n}\n' > "$PROJECT"
        cat > "$DEMO/live-demo.kicad_sch" <<EOF
(kicad_sch (version 20260306) (generator "eeschema") (generator_version "10.0")
  (uuid $(cat /proc/sys/kernel/random/uuid))
  (paper "A4")
  (lib_symbols)
  (sheet_instances (path "/" (page "1")))
)
EOF
    fi
fi

# A schematic opens straight in the schematic editor (which hosts its own API server);
# a project opens the project manager.
case "$TARGET" in
    *.kicad_sch) exec "$INSTALL/bin/eeschema" "$TARGET" ;;
    *)           exec "$INSTALL/bin/kicad" "$TARGET" ;;
esac
