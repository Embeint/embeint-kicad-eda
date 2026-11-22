#!/usr/bin/env bash
set -eu
ROOT=/mnt/c/Users/KarlVonRichter/.t3/worktrees/embeint-kicad-eda/t3-230f667d-d7c5b42cde1e6a30
RUN="$HOME/kicad-port/corrected-review-v5"
PY="$HOME/kicad-port-extractor-venv/bin/python"
driver=$(cat "$RUN/driver.pid")
while kill -0 "$driver" 2>/dev/null; do
  state=$(ps -p "$driver" -o stat= || true)
  case "$state" in Z*|'') break;; esac
  sleep 10
done
test -f "$RUN/completed.txt"
while ! mkdir "$HOME/kicad-port.build-lock" 2>/dev/null; do sleep 10; done
trap 'rmdir "$HOME/kicad-port.build-lock"' EXIT
FINAL="$RUN/final"
OUT="$FINAL/port/extractor/generated"
mkdir -p "$OUT/corrected-extraction" "$OUT/evidence/corrected"
cp "$RUN/output/graph.json.gz" "$OUT/corrected-extraction/graph.json.gz"
cp "$RUN/output/inventory.json" "$OUT/corrected-extraction/inventory.json"
cp "$RUN/output/graph.json.gz" "$OUT/graph.json.gz"
cp "$RUN/output/inventory.json" "$OUT/inventory.json"
cp "$HOME/kicad-port/source-manifest.json" "$OUT/source-manifest.json"
cp "$RUN/inputs/generated-sources.json" "$RUN/inputs/generated-recipes.json" "$OUT/"
rsync -a "$RUN/inputs/sources/" "$OUT/sources/"
cd "$RUN/runner"
"$PY" port/tools/split_units.py --graph "$OUT/graph.json" --inventory "$OUT/inventory.json" --units-dir "$FINAL/port/units" --report "$OUT/units.json" --source-root "$HOME/kicad-port/extract-source" > "$RUN/split.log" 2>&1
"$PY" port/tools/validate_units.py --root "$FINAL" --source-root "$HOME/kicad-port/extract-source" --json > "$OUT/evidence/corrected/validate-snapshot.json"
set +e
"$PY" port/tools/validate_units.py --root "$FINAL" --source-root "$HOME/kicad-port/extract-source" --json --require-complete > "$OUT/evidence/corrected/validate-strict.json"
strict=$?
set -e
test "$strict" -eq 1
"$PY" port/tools/extraction_report.py --raw-output "$HOME/kicad-port/raw-output" --reconciled-output "$HOME/kicad-port/reconciled-output" --corrected-output "$RUN/output" --reconciliation "$HOME/kicad-port/reconciliation/reconciliation.json" --units-dir "$FINAL/port/units" --units-report "$OUT/units.json" --output "$OUT/measurement.json"
"$PY" - <<'PY'
import gzip,hashlib,json,sys
from pathlib import Path
sys.path.insert(0,str(Path.cwd()/'port'))
from extractor.generated_sources import verify
run=Path.home()/'kicad-port/corrected-review-v5';out=run/'final/port/extractor/generated'
manifest=json.loads((out/'generated-sources.json').read_text());verify(manifest,Path(manifest['build_root']),out)
before={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in (run/'final/port/units').glob('*.toml')}
(run/'repeat-before.json').write_text(json.dumps({'units':before,'graph':hashlib.sha256((out/'graph.json.gz').read_bytes()).hexdigest(),'report':hashlib.sha256((out/'units.json').read_bytes()).hexdigest()},sort_keys=True))
artifacts={}
for name,path in [('extraction.log',run/'extraction.log'),('driver.log',run/'driver.log'),('interrupted-extraction.log',Path.home()/'kicad-port/corrected-review/extraction.log'),('interrupted-driver.log',Path.home()/'kicad-port/corrected-review/driver.log')]:
 data=path.read_bytes();encoded=bytearray(gzip.compress(data,mtime=0));encoded[9]=255
 (out/'evidence/corrected'/f'{name}.gz').write_bytes(encoded)
 artifacts[name]={'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest()}
(out/'evidence/corrected/artifacts.json').write_text(json.dumps(artifacts,sort_keys=True,indent=2)+'\n')
for name in ['historical-preservation.json','corrected-tests.log','completed.txt']:
 (out/'evidence/corrected'/name).write_bytes((run/name).read_bytes())
PY
"$PY" port/tools/split_units.py --graph "$OUT/graph.json" --inventory "$OUT/inventory.json" --units-dir "$FINAL/port/units" --report "$OUT/units.json" --source-root "$HOME/kicad-port/extract-source" > "$RUN/repeat-split.log" 2>&1
"$PY" - <<'PY'
import hashlib,json
from pathlib import Path
run=Path.home()/'kicad-port/corrected-review-v5';out=run/'final/port/extractor/generated';before=json.loads((run/'repeat-before.json').read_text())
after={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in (run/'final/port/units').glob('*.toml')}
assert before['units']==after
assert before['graph']==hashlib.sha256((out/'graph.json.gz').read_bytes()).hexdigest()
assert before['report']==hashlib.sha256((out/'units.json').read_bytes()).hexdigest()
(out/'evidence/corrected/repeat-generation.json').write_text(json.dumps({'all_unit_bytes_identical':True,'unit_count':len(after),'graph_and_report_identical':True,'frozen_generated_inputs_verified':len(json.loads((out/'generated-sources.json').read_text())['files'])},sort_keys=True,indent=2)+'\n')
PY
date -u > "$RUN/finalization-completed.txt"
echo 'Corrected private generation/validation/repeat completed; owner must audit, adopt, refresh Windows drift/Size/PILOTS, commit and push'
