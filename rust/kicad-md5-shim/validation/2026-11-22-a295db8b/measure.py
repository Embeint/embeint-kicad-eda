#!/usr/bin/env python3
import hashlib
import json
from pathlib import Path
import subprocess
import time

work = Path.home() / 'kicad-port/pilot-md5'
unit = 'libs.kimath.src.md5_hash.application.pure.p001'
cases = work / f'repo/port/cases/{unit}.jsonl'
results = {}
for name, binary in [('oracle', work / 'oracle-build/kicad-oracle'),
                     ('rust', work / 'repo/rust/target/debug/kicad-md5-shim')]:
    with (cases.open('rb') as source,
          (work / f'{name}-actual.jsonl').open('wb') as output,
          (work / f'{name}-runtime.stderr').open('wb') as errors):
        started = time.perf_counter()
        result = subprocess.run([str(binary)], stdin=source, stdout=output, stderr=errors, timeout=300)
        elapsed = time.perf_counter() - started
    results[name] = {'seconds': elapsed, 'exit_code': result.returncode,
                     'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
                     'output_sha256': hashlib.sha256((work / f'{name}-actual.jsonl').read_bytes()).hexdigest()}
    (work / 'runtimes.json').write_text(json.dumps(results, indent=2) + '\n')
    print(name, results[name], flush=True)
    if result.returncode:
        raise SystemExit(f'{name} failed before comparison')
if any(result['seconds'] >= 60 for result in results.values()):
    raise SystemExit('Measured program exceeds the unchanged 60-second runner budget; report timing for decision')
