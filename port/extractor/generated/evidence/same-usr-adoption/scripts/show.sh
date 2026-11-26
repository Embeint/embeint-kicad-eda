#!/usr/bin/env bash
N="$HOME/kicad-port/canonical-adopt-4738a9c2"
ls -ld "$HOME/kicad-port.build-lock" 2>&1
cat "$N/logs/compare-replay.log"
cat "$N/logs/retained-controls.log"
cat "$N/logs/capture.log"
cat "$N/logs/capture-compare.log" | head -40
cat "$N/logs/apply-refreshed.log"
cat "$N/logs/assemble.log"
head -c 9000 "$N/logs/diffs.log"
grep -E "Maximum resident|Elapsed" "$N/logs/replay.err"
cat "$N/evidence/cache-before.sha256" "$N/evidence/cache-after.sha256"
