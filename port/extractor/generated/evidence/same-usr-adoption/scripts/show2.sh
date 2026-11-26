#!/usr/bin/env bash
N="$HOME/kicad-port/canonical-adopt-4738a9c2"
cat "$N/run-exited.txt"; ls -la --time-style=full-iso "$HOME/kicad-port.build-lock" "$HOME" | grep -i lock
ps -eo pid,lstart,args | grep -v grep | grep -iE "extract|ninja|cargo|refresh|split_units" | head
tail -c 6000 "$N/logs/diffs.log"
/home/karlvonrichter/kicad-port-extractor-venv/bin/python - <<'PY'
import json
c=json.load(open('/home/karlvonrichter/kicad-port/canonical-adopt-4738a9c2/evidence/canonical-graph-changes.json'))
print(c['vrml_body_callers']); print(c['vrml_body_callers_committed'][:40], len(c['vrml_body_callers_committed']))
r=json.load(open('/home/karlvonrichter/kicad-port/canonical-adopt-4738a9c2/evidence/replay-comparison.json'))['versus_v5_original']
print(r['shape_destructor_remaining_vrml_body_callers'])
PY
