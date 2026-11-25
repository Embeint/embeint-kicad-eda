#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
set -euo pipefail
stage="$1"
out="$2"
mkdir -p "$out"
compiler="${CXX:-g++}"
"$compiler" --version > "$out/compiler.txt"
# One compiler process; no GUI or full KiCad build. Never change the exported headers.
"$compiler" -std=c++20 -O0 -g0 --coverage -fno-fast-math -ffp-contract=off \
  -fsanitize=undefined,float-cast-overflow -fno-sanitize-recover=all \
  -I"$stage" -I"$stage/libs/kimath/include" -I"$stage/thirdparty/nlohmann_json" \
  "$stage/main.cpp" "$stage/protocol.cpp" "$stage/dynamic_rtree.cpp" \
  -o "$out/rtree-oracle"

python3 - "$stage" "$out" "$compiler" <<'PYCODE'
import hashlib, json, platform, sys
from pathlib import Path
stage,out,compiler=Path(sys.argv[1]),Path(sys.argv[2]),sys.argv[3]
manifest=json.loads((stage/'source-manifest.json').read_text())
manifest.update(platform=platform.platform(),machine=platform.machine(),compiler=compiler,
                compiler_version=(out/'compiler.txt').read_text(),
                flags=['-std=c++20','-O0','-g0','--coverage','-fno-fast-math','-ffp-contract=off',
                       '-fsanitize=undefined,float-cast-overflow','-fno-sanitize-recover=all'],
                binary_sha256=hashlib.sha256((out/'rtree-oracle').read_bytes()).hexdigest())
(out/'provenance.json').write_text(json.dumps(manifest,indent=2)+'\n')
PYCODE
