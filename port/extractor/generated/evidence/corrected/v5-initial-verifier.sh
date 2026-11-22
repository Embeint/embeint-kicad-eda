#!/usr/bin/env bash
set -eu
RUN="$HOME/kicad-port/corrected-review-v5"
ROOT=/mnt/c/Users/KarlVonRichter/.t3/worktrees/embeint-kicad-eda/t3-230f667d-d7c5b42cde1e6a30
PY="$HOME/kicad-port-extractor-venv/bin/python"
pid=$(cat "$RUN/finalizer.pid")
while kill -0 "$pid" 2>/dev/null; do
  state=$(ps -p "$pid" -o stat= || true)
  case "$state" in Z*|'') break;; esac
  sleep 10
done
test -f "$RUN/finalization-completed.txt"
while ! mkdir "$HOME/kicad-port.build-lock" 2>/dev/null; do sleep 10; done
trap 'rmdir "$HOME/kicad-port.build-lock"' EXIT
cd "$RUN/runner"
set +e
"$PY" port/tools/extract_graph.py --compile-commands "$HOME/kicad-port/reconciled-output/compile_commands.json" --source-root "$HOME/kicad-port/extract-source" --original-source-root "$HOME/kicad-port/extract-source" --manifest "$HOME/kicad-port/source-manifest.json" --generated-manifest "$RUN/inputs/generated-sources.json" --cache "$RUN/cache" --output "$RUN/repeat-output" --jobs 8 > "$RUN/replay-extraction.log" 2>&1
code=$?
set -e
echo "$code" > "$RUN/replay-extraction-exit.txt"
test "$code" -eq 1
"$PY" - <<'PY'
import copy,hashlib,json,sys
from pathlib import Path
sys.path.insert(0,str(Path.cwd()/'port'))
from extractor.model import read_json
run=Path.home()/'kicad-port/corrected-review-v5'
original=read_json(run/'output/graph.json');replay=read_json(run/'repeat-output/graph.json')
assert replay['provenance']['cache_reused_commands']==2511,replay['provenance']['cache_reused_commands']
assert original['provenance']['cache_reused_commands']==0
for value in (original,replay):
    value['provenance'].pop('cache_reused_commands')
    for tu in value['translation_units']:tu.pop('cache_reused_from',None)
assert original==replay,'Cache replay changed semantic graph beyond explicitly removed cache trace fields'
def sha(value):return hashlib.sha256(json.dumps(value,sort_keys=True,separators=(',',':')).encode()).hexdigest()
first=read_json(run/'output/inventory.json');second=read_json(run/'repeat-output/inventory.json')
for value in (first,second):value['provenance'].pop('cache_reused_commands')
assert first==second
report={'cache_reused_commands':2511,'new_clang_parses':0,'graph_semantics_equal':True,'inventory_semantics_equal':True,
        'excluded_audit_fields':['provenance.cache_reused_commands','translation_units[].cache_reused_from'],
        'semantic_graph_sha256':sha(original),'semantic_inventory_sha256':sha(first),
        'interpretation':'Full aggregation replay from exact V5 AST cache; original cache trace retained in original graph. Separate splitter repeat checks preserve every canonical graph/report/unit byte.'}
(run/'final/port/extractor/generated/evidence/corrected/extraction-replay.json').write_text(json.dumps(report,sort_keys=True,indent=2)+'\n')
print(json.dumps(report))
PY
mkdir -p "$RUN/check-runner/port/extractor" "$RUN/check-runner/port/tools"
rsync -a --exclude generated --exclude __pycache__ "$ROOT/port/extractor/" "$RUN/check-runner/port/extractor/"
rsync -a --exclude __pycache__ "$ROOT/port/tools/" "$RUN/check-runner/port/tools/"
cd "$RUN/check-runner"
"$PY" -m unittest discover -s port/extractor -p 'test_*.py' -v > "$RUN/final/port/extractor/generated/evidence/corrected/final-extractor-tests-wsl.log" 2>&1
"$PY" port/tools/validate_units.py --root "$RUN/final" --source-root "$HOME/kicad-port/extract-source" --json > "$RUN/final/port/extractor/generated/evidence/corrected/final-validate-snapshot.json"
date -u > "$RUN/verification-completed.txt"
echo 'V5 full cached replay and final WSL tests/source validation complete; no adoption or commit performed'
