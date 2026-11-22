"""Adopt the guarded recapture (Windows). D2 generator-refresh evidence stays byte-identical."""
import gzip, hashlib, json, os, shutil, subprocess, sys
from pathlib import Path
root = Path(r'C:\Users\KarlVonRichter\.t3\worktrees\embeint-kicad-eda\t3-7461f604-ff4777aca1333cd1')
os.chdir(root)
N = Path(r'\\wsl.localhost\Ubuntu\home\karlvonrichter\kicad-port\generator-refresh-ce9681f')
S = Path(r'C:\Users\KarlVonRichter\.t3\phase0-source-guard')
G = root / 'port/extractor/generated'
D2 = G / 'evidence/generator-refresh'
E = G / 'evidence/source-guard'
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
assert (N / 'completed.txt').exists(), 'guarded workflow has not completed'
d2_before = {p.relative_to(D2).as_posix(): sha(p) for p in D2.rglob('*') if p.is_file()}


def write_json(path, value):
    path.write_text(json.dumps(value, sort_keys=True, indent=2) + '\n', encoding='utf-8', newline='\n')


def deterministic_gzip(data):
    out = bytearray(gzip.compress(data, compresslevel=9, mtime=0)); out[9] = 255
    return bytes(out)


assert sha(E / 'base-generation-input-audit.json') == 'bf7a1689587de011b7f2b8879e7d6284827a451ee43155ad72b8963fe30a6de6'
F = N / 'final/port/extractor/generated'
for name in ('graph.json.gz', 'inventory.json', 'generated-sources.json', 'measurement.json'):
    shutil.copyfile(F / name, G / name)
for name in ('generated-recipes.json.gz', 'units.json', 'source-manifest.json'):
    assert sha(F / name) == sha(G / name), name
for name in ('code-and-input-identities.json', 'd2-equivalence.json', 'assembly.json', 'actual-controls.json', 'repro-controls.json'):
    shutil.copyfile(N / 'evidence' / name, E / name)
shutil.copyfile(N / 'refreshed/refresh-report.json', E / 'refresh-report.json')
shutil.copyfile(N / 'capture/generator-dependency-capture.json', E / 'generator-dependency-capture.json')
(E / 'logs').mkdir(exist_ok=True)
logs = {}
for path in sorted((N / 'logs').glob('*.log')):
    data = path.read_bytes()
    (E / 'logs' / (path.name + '.gz')).write_bytes(deterministic_gzip(data))
    logs[path.name] = {'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()}
write_json(E / 'artifacts.json', logs)
(E / 'scripts').mkdir(exist_ok=True)
for path in sorted(S.glob('*')):
    if path.suffix in ('.sh', '.py') and path.name not in ('launch.sh', 'wait.sh'):
        (E / 'scripts' / path.name).write_bytes(path.read_bytes().replace(b'\r\n', b'\n'))

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
drift = {}
for name in ('drift-all.json', 'drift-ported.json'):
    previous = json.loads(subprocess.check_output(['git', 'show', 'd2bdddb904:port/extractor/generated/' + name]))
    current = json.loads((G / name).read_text())
    targets = (previous.pop('target'), current.pop('target'))
    assert previous == current, name
    drift[name] = {'equal_to_d2_except_target': True, 'd2_target': targets[0], 'target': targets[1]}
drift['target_policy'] = ('drift_report.py records the commit it compared (HEAD when generated); a committed report names the '
                          'parent of its publishing commit')
write_json(E / 'drift-comparison.json', drift)
assert {p.relative_to(D2).as_posix(): sha(p) for p in D2.rglob('*') if p.is_file()} == d2_before
print(json.dumps({'results': results, 'drift': drift, 'd2_evidence_unchanged': len(d2_before)}, indent=2))
