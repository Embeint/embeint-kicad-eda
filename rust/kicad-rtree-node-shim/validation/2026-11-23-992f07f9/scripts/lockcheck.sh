#!/usr/bin/env bash
# Locked metadata succeeds and an unlocked offline resolution leaves Cargo.lock unchanged.
set -euo pipefail
source "$HOME/.cargo/env"
bash /mnt/c/kb/rtree-p003-scripts/sync.sh >/dev/null
cd "$HOME/kicad-port/rtree-p003/repo/rust"
sha256sum Cargo.lock
cargo +1.99.1 metadata --locked --format-version 1 > /tmp/rtree-meta.json
python3 -c "import json; m=json.load(open('/tmp/rtree-meta.json')); names=sorted(p['name'] for p in m['packages'] if p['id'] in m['workspace_members']); print(len(names), 'workspace members:', names)"
cp Cargo.lock /tmp/rtree-Cargo.lock.before
cargo +1.99.1 metadata --offline --format-version 1 > /dev/null
cmp Cargo.lock /tmp/rtree-Cargo.lock.before && echo "Cargo.lock unchanged by unlocked offline resolution"
