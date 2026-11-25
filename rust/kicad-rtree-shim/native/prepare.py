#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Export genuine pinned headers, never copy the moving worktree's reading source."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
ROOT = Path(__file__).resolve().parents[3]
PIN = '4cf939778df04d2b747bb2c4714fc5014a7565c7'
SOURCES = ['libs/kimath/include/geometry/rtree/dynamic_rtree.h',
           'libs/kimath/include/geometry/rtree/rtree_node.h',
           'thirdparty/nlohmann_json/nlohmann/json.hpp']
p = argparse.ArgumentParser()
p.add_argument('--out', type=Path, required=True)
a = p.parse_args()
a.out.mkdir(parents=True, exist_ok=True)
manifest = {'pin': PIN, 'source_sha256': {}, 'overlay_sha256': {}}
for name in SOURCES:
    data = subprocess.check_output(['git', '-C', str(ROOT), 'show', f'{PIN}:{name}'])
    path = a.out / name
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    manifest['source_sha256'][name] = hashlib.sha256(data).hexdigest()
for name in ['port/oracle/shims/dynamic_rtree.cpp', 'port/oracle/protocol.cpp',
             'port/oracle/protocol.h', 'port/oracle/registry.h',
             'rust/kicad-rtree-shim/native/main.cpp']:
    data = (ROOT / name).read_bytes()
    path = a.out / Path(name).name
    path.write_bytes(data)
    manifest['overlay_sha256'][name] = hashlib.sha256(data).hexdigest()
manifest['revision'] = subprocess.check_output(['git', '-C', str(ROOT), 'rev-parse', 'HEAD'], text=True).strip()
manifest['dirty'] = bool(subprocess.check_output(['git', '-C', str(ROOT), 'status', '--porcelain']))
(a.out / 'source-manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
print(json.dumps(manifest, indent=2))
