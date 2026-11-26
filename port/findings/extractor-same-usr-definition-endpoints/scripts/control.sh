#!/usr/bin/env bash
# Negative control: restore the old location-blind aggregation in a disposable
# copy and confirm the new regressions detect the collision.
set -eu
RUN="$HOME/kicad-port/shape-collision"
rm -rf "$RUN/control"
cp -a "$RUN/runner" "$RUN/control"
cd "$RUN/control"
"$HOME/kicad-port-extractor-venv/bin/python" - <<'PY'
from pathlib import Path
m = Path('port/extractor/model.py'); s = m.read_text()
old = "        if location is None:\n            return choices.get(usr, [usr])\n"
assert old in s
m.write_text(s.replace(old, "        if True:\n            return choices.get(usr, [usr])\n"))
PY
set +e
"$HOME/kicad-port-extractor-venv/bin/python" -m unittest discover -s port/extractor -p 'test_*.py' -k separate > "$RUN/control.log" 2>&1
echo "control exit $?"
grep -E "^(FAIL|ERROR):|AssertionError|^Ran|^FAILED|^OK" "$RUN/control.log"
