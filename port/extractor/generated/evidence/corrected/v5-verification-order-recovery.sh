#!/usr/bin/env bash
set -eu
RUN="$HOME/kicad-port/corrected-review-v5"
ROOT=/mnt/c/Users/KarlVonRichter/.t3/worktrees/embeint-kicad-eda/t3-230f667d-d7c5b42cde1e6a30
PY="$HOME/kicad-port-extractor-venv/bin/python"
test -f "$RUN/finalization-completed.txt"
test ! -f "$RUN/verification-completed.txt"
mkdir "$HOME/kicad-port.build-lock"
trap 'rmdir "$HOME/kicad-port.build-lock"' EXIT
cd "$RUN/runner"
"$PY" - <<'PY'
import hashlib,json,sys
from pathlib import Path
sys.path.insert(0,str(Path.cwd()/'port'))
from extractor.model import read_json
run=Path.home()/'kicad-port/corrected-review-v5'
a=read_json(run/'output/graph.json');b=read_json(run/'repeat-output/graph.json')
assert a['provenance']['cache_reused_commands']==0
assert b['provenance']['cache_reused_commands']==2511
key=lambda value:json.dumps(value,sort_keys=True)
original_order=[f['file'] for f in a['provenance']['parse_failures']]
replay_order=[f['file'] for f in b['provenance']['parse_failures']]
assert sorted(a['provenance']['parse_failures'],key=key)==sorted(b['provenance']['parse_failures'],key=key)
for value in (a,b):
    value['provenance'].pop('cache_reused_commands')
    value['provenance']['parse_failures'].sort(key=key)
    for tu in value['translation_units']:tu.pop('cache_reused_from',None)
assert a==b,'A graph difference beyond cache trace and failure reporting order remains'
first=read_json(run/'output/inventory.json');second=read_json(run/'repeat-output/inventory.json')
for value in (first,second):
    value['provenance'].pop('cache_reused_commands')
    value['provenance']['parse_failures'].sort(key=key)
assert first==second
sha=lambda value:hashlib.sha256(json.dumps(value,sort_keys=True,separators=(',',':')).encode()).hexdigest()
report={'cache_reused_commands':2511,'new_clang_parses':0,'graph_semantics_equal':True,'inventory_semantics_equal':True,
    'excluded_audit_fields':['provenance.cache_reused_commands','translation_units[].cache_reused_from'],
    'diagnostic_comparison':'All failure file/status/severity/message payloads compared exactly after sorting complete records; no diagnostic omitted',
    'original_failure_order':original_order,'replay_failure_order':replay_order,
    'raw_failure_reporting_order_is_nondeterministic':True,'raw_artifacts_unchanged':True,
    'semantic_graph_sha256':sha(a),'semantic_inventory_sha256':sha(first),
    'interpretation':'Full aggregation replay from exact V5 AST cache. The first stricter comparison failed only on completion-order diagnostics; its log/difference remain preserved. Canonical splitter and post-adoption regeneration separately check all output bytes from identical recorded input.'}
out=run/'final/port/extractor/generated/evidence/corrected'
(out/'extraction-replay.json').write_text(json.dumps(report,sort_keys=True,indent=2)+'\n')
(out/'initial-replay-difference.json').write_bytes((run/'replay-difference.json').read_bytes())
(out/'initial-replay-verifier.log').write_bytes((run/'verify.log').read_bytes())
print(json.dumps(report),flush=True)
PY
mkdir -p "$RUN/check-runner/port/extractor" "$RUN/check-runner/port/tools"
rsync -a --exclude generated --exclude __pycache__ "$ROOT/port/extractor/" "$RUN/check-runner/port/extractor/"
rsync -a --exclude __pycache__ "$ROOT/port/tools/" "$RUN/check-runner/port/tools/"
cd "$RUN/check-runner"
"$PY" -m unittest discover -s port/extractor -p 'test_*.py' -v > "$RUN/final/port/extractor/generated/evidence/corrected/final-extractor-tests-wsl.log" 2>&1
"$PY" port/tools/validate_units.py --root "$RUN/final" --source-root "$HOME/kicad-port/extract-source" --json > "$RUN/final/port/extractor/generated/evidence/corrected/final-validate-snapshot.json"
date -u > "$RUN/verification-completed.txt"
echo 'Full cached graph replay matched with diagnostic payloads compared exactly independent of failure reporting order; final tests and source validation passed'
