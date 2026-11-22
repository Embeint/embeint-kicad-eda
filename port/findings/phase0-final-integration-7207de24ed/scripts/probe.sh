#!/usr/bin/env bash
# Read-only probe of the shared WSL host before the continued integration proof.
free -g
nproc
ls -ld "$HOME/kicad-port.build-lock" 2>&1 || true
ls -d "$HOME"/kicad-port/phase0-final-* 2>&1
ls -d "$HOME/kicad-port/md5-corrected-graph/repo/.port-deps/solidcore" "$HOME/kicad-port/md5-corrected-graph/repo/rust/target" 2>&1
ls "$HOME/kicad-port-extractor-venv/bin/python" 2>&1 && "$HOME/kicad-port-extractor-venv/bin/python" -c 'import clang.cindex, sys; print(sys.version)'
which ninja cmake gcc node; gcc --version | head -1
source "$HOME/.cargo/env"; rustc --version
ps -eo pid,etime,args | grep -E 'cargo|ninja|cc1plus|rustc|build.sh' | grep -v grep | head -20
df -h "$HOME" | tail -1
