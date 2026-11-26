"""Adopt the final-head same-USR canonical artifacts into the Windows worktree (explicit owned paths).

Earlier evidence directories (V5, D2, source-guard, verified-ninja, freeze-symlink,
corrected-extraction and the same-USR finding) are checked byte-identical before
and after.
"""
import gzip, hashlib, json, os, shutil, subprocess, sys
from pathlib import Path
root = Path(r'C:\Users\KarlVonRichter\.t3\worktrees\embeint-kicad-eda\t3-290c6870-81dcfa65b1526978')
os.chdir(root)
N = Path(r'\\wsl.localhost\Ubuntu\home\karlvonrichter\kicad-port\canonical-adopt-4738a9c2')
S = Path(r'C:\Users\KarlVonRichter\.t3\canon-adopt\scripts')
G = root / 'port/extractor/generated'
E = G / 'evidence/same-usr-adoption'
R = G / 'replay-extraction'
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
assert (N / 'completed.txt').exists(), 'WSL adoption workflow has not completed'
assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip() == '4738a9c23f8c6068784d26ba991a690044343892'
immutable = [G / 'evidence' / d for d in ('corrected', 'generator-refresh', 'source-guard', 'verified-ninja', 'freeze-symlink')] + \
    [G / 'corrected-extraction', G / 'raw', G / 'reconciled-extraction', G / 'history', G / 'sources',
     root / 'port/findings/extractor-same-usr-definition-endpoints']
snapshot = lambda: {p.relative_to(root).as_posix(): sha(p) for d in immutable for p in d.rglob('*') if p.is_file()} | \
    {'port/findings/extractor-same-usr-definition-endpoints.md': sha(root / 'port/findings/extractor-same-usr-definition-endpoints.md')}
before = snapshot()


def write_json(path, value):
    path.write_text(json.dumps(value, sort_keys=True, indent=2) + '\n', encoding='utf-8', newline='\n')


def deterministic_gzip(data):
    out = bytearray(gzip.compress(data, compresslevel=9, mtime=0)); out[9] = 255
    return bytes(out)


F = N / 'final/port/extractor/generated'
for name in ('graph.json.gz', 'inventory.json', 'generated-sources.json', 'units.json', 'measurement.json'):
    shutil.copyfile(F / name, G / name)
for name in ('generated-recipes.json.gz', 'source-manifest.json'):
    assert sha(F / name) == sha(G / name), name
units_dir = root / 'port/units'
final_units = {p.name: p for p in (N / 'final/port/units').glob('*.toml')}
assert set(final_units) == {p.name for p in units_dir.glob('*.toml')}
for name, path in final_units.items():
    data = path.read_bytes()
    if data != (units_dir / name).read_bytes().replace(b'\r\n', b'\n'):
        (units_dir / name).write_bytes(data)
R.mkdir(exist_ok=False)
for name in ('graph.json.gz', 'inventory.json'):
    shutil.copyfile(N / 'replay' / name, R / name)

E.mkdir(exist_ok=False)
for name in ('identities.json', 'replay-comparison.json', 'retained-capture-controls.json', 'capture-comparison.json',
             'assembly.json', 'canonical-graph-changes.json', 'measurement-changes.json', 'replay-exit.txt',
             'cache-before.sha256', 'cache-after.sha256'):
    shutil.copyfile(N / 'evidence' / name, E / name)
(E / 'unit-record-changes.json.gz').write_bytes(deterministic_gzip((N / 'evidence/unit-record-changes.json').read_bytes()))
for name in ('cache-before-listing.txt', 'cache-before-content.txt', 'cache-after-listing.txt', 'cache-after-content.txt'):
    (E / (name + '.gz')).write_bytes(deterministic_gzip((N / 'evidence' / name).read_bytes()))
shutil.copyfile(N / 'capture/generator-dependency-capture.json', E / 'generator-dependency-capture.json')
shutil.copyfile(N / 'refreshed/refresh-report.json', E / 'refresh-report.json')
(E / 'logs').mkdir()
logs = {}
for path in sorted((N / 'logs').glob('*')):
    data = path.read_bytes()
    (E / 'logs' / (path.name + '.gz')).write_bytes(deterministic_gzip(data))
    logs[path.name] = {'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()}
write_json(E / 'artifacts.json', logs)
(E / 'scripts').mkdir()
for path in sorted(S.glob('*')):
    if path.suffix in ('.sh', '.py', '.ps1') and path.name != 'survey.sh':
        (E / 'scripts' / path.name).write_bytes(path.read_bytes().replace(b'\r\n', b'\n'))

results = {}
for name, args in [('validate-windows', ['port/tools/validate_units.py', '--json']),
                   ('validate-strict-windows', ['port/tools/validate_units.py', '--json', '--require-complete']),
                   ('drift-ported', ['port/tools/drift_report.py', '--output', str(G / 'drift-ported.json')]),
                   ('drift-all', ['port/tools/drift_report.py', '--include-todo', '--output', str(G / 'drift-all.json')])]:
    with (E / f'{name}.log').open('w', encoding='utf-8', newline='\n') as log:
        results[name] = subprocess.run([sys.executable, *args], stdout=log, stderr=subprocess.STDOUT).returncode
print(json.dumps(results))
assert results == {'validate-windows': 0, 'validate-strict-windows': 1, 'drift-ported': 0, 'drift-all': 0}, results
for name, target in [('validate-windows', 'validate'), ('validate-strict-windows', 'validate-strict'),
                     ('drift-ported', 'drift-ported'), ('drift-all', 'drift-all')]:
    shutil.copyfile(E / f'{name}.log', G / f'{target}.log')
drift = {}
for name in ('drift-all.json', 'drift-ported.json'):
    previous = json.loads(subprocess.check_output(['git', 'show', 'HEAD:port/extractor/generated/' + name]))
    current = json.loads((G / name).read_text())
    targets = (previous.pop('target'), current.pop('target'))
    drift[name] = {'equal_to_committed_except_target': previous == current, 'committed_target': targets[0], 'target': targets[1]}
    if previous != current:
        drift[name]['changed_keys'] = sorted(k for k in set(previous) | set(current) if previous.get(k) != current.get(k))
write_json(E / 'drift-comparison.json', drift)
assert snapshot() == before, 'immutable evidence changed'
print(json.dumps({'results': results, 'drift': drift, 'immutable_files_unchanged': len(before)}, indent=2))
