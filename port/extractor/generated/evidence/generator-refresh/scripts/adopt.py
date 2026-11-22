"""Adopt the verified generator-dependency refresh into the worktree (Windows)."""
import gzip, hashlib, json, os, shutil, subprocess, sys
from pathlib import Path
root = Path(r'C:\Users\KarlVonRichter\.t3\worktrees\embeint-kicad-eda\t3-7461f604-ff4777aca1333cd1')
os.chdir(root)
W = Path(r'\\wsl.localhost\Ubuntu\home\karlvonrichter\kicad-port\generator-refresh-57e')
S = Path(r'C:\Users\KarlVonRichter\.t3\phase0-gen-refresh')
G = root / 'port/extractor/generated'
C = G / 'corrected-extraction'
E = G / 'evidence/generator-refresh'
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
lf = lambda p: hashlib.sha256(p.read_bytes().replace(b'\r\n', b'\n')).hexdigest()
assert (W / 'completed.txt').exists(), 'WSL refresh workflow has not completed'


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, sort_keys=True, indent=2) + '\n', encoding='utf-8', newline='\n')


def deterministic_gzip(data):
    out = bytearray(gzip.compress(data, compresslevel=9, mtime=0))
    out[9] = 255
    return bytes(out)


# 1. Keep the V5 parse manifest and recipes beside the V5 AST they produced.
if not (C / 'generated-sources.json').exists():
    assert lf(G / 'generated-sources.json') == 'a2466270bdf4aec52b7b8267140ac4b18c8295520c5b94e1ecb3dc8498dacb34'
    assert sha(G / 'generated-recipes.json.gz') == 'e0d935a1588f0951462e35aaa49f36a1ba58787043ad2a266dc1412ae1705d2e'
    subprocess.run(['git', 'mv', 'port/extractor/generated/generated-sources.json', 'port/extractor/generated/corrected-extraction/generated-sources.json'], check=True)
    subprocess.run(['git', 'mv', 'port/extractor/generated/generated-recipes.json.gz', 'port/extractor/generated/corrected-extraction/generated-recipes.json.gz'], check=True)
assert lf(C / 'generated-sources.json') == json.loads((C / 'inventory.json').read_text())['provenance']['generated_sources_manifest_sha256']

# 2. Adopt the refreshed canonical artifacts; unchanged ones must already match.
F = W / 'final/port/extractor/generated'
for name in ('graph.json.gz', 'inventory.json', 'generated-sources.json', 'generated-recipes.json.gz', 'measurement.json'):
    shutil.copyfile(F / name, G / name)
for name in ('units.json', 'source-manifest.json'):
    assert sha(F / name) == sha(G / name), name

# 3. Evidence, logs and the exact workflow scripts.
E.mkdir(parents=True, exist_ok=True)
for name in ('code-identities.json', 'v5-replay.json', 'canonical-from-v5.json', 'assembly.json', 'recipe-storage.json',
             'relations.json', 'reviewed-reproducers.json', 'generation-input-candidates.json'):
    shutil.copyfile(W / 'evidence' / name, E / name)
shutil.copyfile(W / 'refreshed/refresh-report.json', E / 'refresh-report.json')
shutil.copyfile(W / 'capture/generator-dependency-capture.json', E / 'generator-dependency-capture.json')
catalog = (W / 'capture/ninja-targets.txt').read_bytes()
(E / 'ninja-targets.txt.gz').write_bytes(deterministic_gzip(catalog))
(E / 'logs').mkdir(exist_ok=True)
logs = {}
for path in sorted((W / 'logs').glob('*.log')):
    data = path.read_bytes()
    (E / 'logs' / (path.name + '.gz')).write_bytes(deterministic_gzip(data))
    logs[path.name] = {'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()}
logs['ninja-targets.txt'] = {'bytes': len(catalog), 'sha256': hashlib.sha256(catalog).hexdigest()}
write_json(E / 'artifacts.json', logs)
(E / 'scripts').mkdir(exist_ok=True)
for path in sorted(S.glob('*')):
    if path.suffix in ('.sh', '.py') and path.name not in ('quicktest.sh', 'status.sh', 'peek.sh', 'st.sh', 'wait.sh', 'launch.sh'):
        (E / 'scripts' / path.name).write_bytes(path.read_bytes().replace(b'\r\n', b'\n'))

# 4. Every tracked generator/configure input equals its pinned Git blob.
data = json.loads((E / 'generation-input-candidates.json').read_text())
pin = data['source_commit']
batch = subprocess.run(['git', 'cat-file', '--batch-check'], input=''.join(f"{pin}:{e['path']}\n" for e in data['inputs']).encode(),
                       capture_output=True, check=True).stdout.decode().splitlines()
assert len(batch) == len(data['inputs'])
for entry, line in zip(data['inputs'], batch):
    parts = line.split()
    assert len(parts) == 3 and parts[1] == 'blob', (entry['path'], line)
    if parts[0] not in (entry['raw_blob'], entry['normalized_blob']):
        expected = subprocess.check_output(['git', 'show', f"{pin}:{entry['path']}"])
        assert hashlib.sha256(expected.replace(b'\r\n', b'\n')).hexdigest() == entry['normalized_sha256'], entry['path']
    entry['pinned_git_blob'] = parts[0]
data['verified_count'] = len(data['inputs'])
data['policy'] = ('Tracked generator/configure inputs reached by the refreshed dependency map, its source-tree outputs and '
                  'every recorded Ninja query/command equal pinned Git blobs (raw or CRLF-normalized)')
write_json(E / 'generation-input-audit.json', data)
(E / 'generation-input-candidates.json').unlink()

# 5. Windows validation and drift from the adopted tree.
results = {}
for name, args in [('validate-windows', ['port/tools/validate_units.py', '--json']),
                   ('validate-strict-windows', ['port/tools/validate_units.py', '--json', '--require-complete']),
                   ('drift-ported', ['port/tools/drift_report.py', '--output', str(G / 'drift-ported.json')]),
                   ('drift-all', ['port/tools/drift_report.py', '--include-todo', '--output', str(G / 'drift-all.json')])]:
    with (E / f'{name}.log').open('w', encoding='utf-8', newline='\n') as log:
        results[name] = subprocess.run([sys.executable, *args], stdout=log, stderr=subprocess.STDOUT).returncode
assert results == {'validate-windows': 0, 'validate-strict-windows': 1, 'drift-ported': 0, 'drift-all': 0}, results
for name, target in [('validate-windows', 'validate'), ('validate-strict-windows', 'validate-strict'),
                     ('drift-ported', 'drift-ported'), ('drift-all', 'drift-all')]:
    shutil.copyfile(E / f'{name}.log', G / f'{target}.log')
previous = json.loads(subprocess.check_output(['git', 'show', 'HEAD:port/extractor/generated/drift-all.json']))
current = json.loads((G / 'drift-all.json').read_text())
summary = lambda r: {'target': r['target'], 'changed_files': len(r['changed_files']), 'direct_findings': len(r['affected']),
                     'affected_units': len(r['affected_units']), 'dependency_reviews': len(r['dependency_reviews'])}
before_reviews = {d['unit']: d['changed_inputs'] for d in previous['dependency_reviews']}
after_reviews = {d['unit']: d['changed_inputs'] for d in current['dependency_reviews']}
drift = {'before': summary(previous), 'after': summary(current),
         'units_added_to_dependency_review': sorted(set(after_reviews) - set(before_reviews)),
         'units_removed_from_dependency_review': sorted(set(before_reviews) - set(after_reviews)),
         'units_with_changed_inputs': sorted(u for u in set(after_reviews) & set(before_reviews) if after_reviews[u] != before_reviews[u]),
         'direct_findings_equal': previous['affected'] == current['affected'],
         'target_policy': ('drift_report.py records the commit it compared (HEAD when generated). A committed report cannot name '
                           'the commit that contains it, so its target is the parent of the publishing commit; regenerate to '
                           'compare a later head.')}
write_json(E / 'drift-comparison.json', drift)
print(json.dumps({'results': results, 'drift': {k: v for k, v in drift.items() if not isinstance(v, list)}}, indent=2))
