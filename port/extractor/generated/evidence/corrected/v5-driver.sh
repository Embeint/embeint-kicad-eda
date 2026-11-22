#!/usr/bin/env bash
set -eu
# New run only: never resume or relabel historical AST caches.
lock_wait_reported=0
while ! mkdir "$HOME/kicad-port.build-lock" 2>/dev/null; do
  if test "$lock_wait_reported" -eq 0; then echo 'Waiting for shared build lock; no clang jobs started'; lock_wait_reported=1; fi
  sleep 10
done
trap 'rmdir "$HOME/kicad-port.build-lock"' EXIT
echo 'Acquired shared build lock for corrected v5'
ROOT=/mnt/c/Users/KarlVonRichter/.t3/worktrees/embeint-kicad-eda/t3-230f667d-d7c5b42cde1e6a30
RUN="$HOME/kicad-port/corrected-review-v5"
PY="$HOME/kicad-port-extractor-venv/bin/python"
mkdir -p "$RUN/runner/port/extractor" "$RUN/runner/port/tools"
rsync -a --exclude generated --exclude __pycache__ "$ROOT/port/extractor/" "$RUN/runner/port/extractor/"
rsync -a --exclude __pycache__ "$ROOT/port/tools/" "$RUN/runner/port/tools/"
cd "$RUN/runner"
"$PY" -m unittest discover -s port/extractor -p 'test_*.py' -v > "$RUN/corrected-tests.log" 2>&1
"$PY" port/extractor/generated_sources.py --build-root "$HOME/kicad-port/extract-build" --reconciliation "$HOME/kicad-port/reconciliation" --output "$RUN/inputs"
"$PY" - <<'PY'
import hashlib,json
from pathlib import Path
home=Path.home()/'kicad-port';run=home/'corrected-review-v5'
old=json.loads((home/'reconciliation/private-generated-input-fingerprints.json').read_text())
fresh=json.loads((run/'inputs/generated-sources.json').read_text())
files={e['path'].removeprefix('@build/'):e['bytes_sha256'] for e in fresh['files']}
assert all(files[e['path']]==e['sha256'] for e in old['files'])
baseline={}
for name in ['raw-output/graph.json.gz','raw-output/inventory.json','raw-output/compile_commands.json','reconciled-output/graph.json.gz','reconciled-output/inventory.json','reconciled-output/compile_commands.json']:
 p=home/name;baseline[name]={'bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()}
baseline['cache_policy']='No historical AST cache reused; corrected cache keys include new parser/model/generated-source implementation, inventory, generated-manifest and libclang byte identities'
baseline['original_frozen_inputs_verified']=len(old['files'])
(run/'historical-preservation.json').write_text(json.dumps(baseline,sort_keys=True,indent=2)+'\n')
PY
set +e
"$PY" port/tools/extract_graph.py --compile-commands "$HOME/kicad-port/reconciled-output/compile_commands.json" --source-root "$HOME/kicad-port/extract-source" --original-source-root "$HOME/kicad-port/extract-source" --manifest "$HOME/kicad-port/source-manifest.json" --generated-manifest "$RUN/inputs/generated-sources.json" --cache "$RUN/cache" --output "$RUN/output" --jobs 8 > "$RUN/extraction.log" 2>&1
code=$?
set -e
echo "$code" > "$RUN/extraction-exit.txt"
test "$code" -eq 1
"$PY" - <<'PY'
import json,hashlib,sys
from pathlib import Path
sys.path.insert(0,str(Path.cwd()/'port'))
from extractor.model import read_json
from extractor.generated_sources import verify
home=Path.home()/'kicad-port';run=home/'corrected-review-v5'
g=read_json(run/'output/graph.json');p=g['provenance']
assert p['commands_attempted']==p['compile_commands_count']==2511,p
failures=p['parse_failures']
assert len(failures)==4,failures
assert all('/fmt/' in f['file'] and f['status']=='parse-error' for f in failures),failures
verify(read_json(run/'inputs/generated-sources.json'),home/'extract-build',run/'inputs')
baseline=read_json(run/'historical-preservation.json')
for name,info in baseline.items():
 if not isinstance(info,dict):continue
 path=home/name;assert hashlib.sha256(path.read_bytes()).hexdigest()==info['sha256'],name
(run/'completed.txt').write_text(json.dumps({'extractor_sha256':p['extractor_sha256'],'functions':len(g['functions']),'commands':2511,'failures':len(failures)},sort_keys=True)+'\n')
print((run/'completed.txt').read_text())
PY
