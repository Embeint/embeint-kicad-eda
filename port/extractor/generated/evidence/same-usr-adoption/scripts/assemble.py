"""Assemble the refreshed canonical tree, prove regeneration is byte-stable, validate and measure."""
import gzip, hashlib, json, pathlib, shutil, subprocess, sys
N = pathlib.Path(sys.argv[1])
K = pathlib.Path.home() / 'kicad-port'
V5 = K / 'corrected-review-v5'
PY = str(pathlib.Path.home() / 'kicad-port-extractor-venv/bin/python')
H = N / 'head/port/extractor/generated'
run = N / 'runner'
F = N / 'final'
G = F / 'port/extractor/generated'
P = N / 'pre/port/extractor/generated'
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
if F.exists():
    shutil.rmtree(F)
G.mkdir(parents=True)
shutil.copytree(N / 'pre/port/units', F / 'port/units')
pre_units = {p.name: sha(p) for p in (F / 'port/units').glob('*.toml')}
for name in ('graph.json.gz', 'inventory.json', 'generated-sources.json'):
    shutil.copyfile(N / 'refreshed' / name, G / name)
shutil.copyfile(P / 'units.json', G / 'units.json')
shutil.copyfile(K / 'source-manifest.json', G / 'source-manifest.json')
shutil.copytree(V5 / 'inputs/sources', G / 'sources')
raw = (N / 'capture/generated-recipes.json').read_bytes()
compressed = bytearray(gzip.compress(raw, compresslevel=9, mtime=0))
compressed[9] = 255
(G / 'generated-recipes.json.gz').write_bytes(compressed)
assert (G / 'generated-recipes.json.gz').read_bytes() == (H / 'generated-recipes.json.gz').read_bytes()
assert hashlib.sha256(raw).hexdigest() == json.loads((G / 'generated-sources.json').read_text())['recipes_sha256']
del raw, compressed

tracked = lambda: {str(p.relative_to(F)): sha(p) for p in sorted((F / 'port/units').glob('*.toml'))} | \
    {str((G / n).relative_to(F)): sha(G / n) for n in ('graph.json.gz', 'inventory.json', 'units.json')}
before = tracked()
split = [PY, str(run / 'port/tools/split_units.py'), '--graph', str(G / 'graph.json'), '--inventory', str(G / 'inventory.json'),
         '--units-dir', str(F / 'port/units'), '--report', str(G / 'units.json'), '--source-root', str(K / 'extract-source')]
iterations = []
for _ in range(2):
    out = subprocess.run(split, check=True, capture_output=True, text=True)
    assert tracked() == before, 'canonical regeneration changed bytes'
    iterations.append(out.stdout)
assert {p.name: sha(p) for p in (F / 'port/units').glob('*.toml')} == pre_units
validation = {}
for label, extra, expected in (('default', [], 0), ('strict', ['--require-complete'], 1)):
    out = subprocess.run([PY, str(run / 'port/tools/validate_units.py'), '--root', str(F), '--source-root',
                          str(K / 'extract-source'), '--json', *extra], capture_output=True, text=True)
    validation[label] = {'exit': out.returncode, 'result': json.loads(out.stdout)}
    assert out.returncode == expected, validation
assert validation['strict']['result']['errors'] == ['WSL extraction diagnostic incomplete; inspect provenance coverage and parse failures'], validation
measurement = G / 'measurement.json'
out = subprocess.run([PY, str(run / 'port/tools/extraction_report.py'), '--raw-output', str(K / 'raw-output'),
                      '--reconciled-output', str(K / 'reconciled-output'), '--corrected-output', str(N / 'replay'),
                      '--reconciliation', str(K / 'reconciliation/reconciliation.json'), '--units-dir', str(F / 'port/units'),
                      '--units-report', str(G / 'units.json'), '--output', str(measurement)], check=True, capture_output=True, text=True)
result = {'regeneration_iterations': 2, 'byte_identical_artifacts': len(before), 'unit_files': len(pre_units),
          'split_stdout': iterations, 'validation': validation, 'artifact_sha256': before,
          'measurement_stdout': out.stdout, 'measurement_sha256': sha(measurement),
          'refreshed_manifest_sha256': sha(G / 'generated-sources.json'),
          'recipes_gzip_sha256': sha(G / 'generated-recipes.json.gz'),
          'replay_graph_sha256': sha(N / 'replay/graph.json.gz'), 'replay_inventory_sha256': sha(N / 'replay/inventory.json'),
          'pre_refresh_canonical_graph_sha256': sha(P / 'graph.json.gz')}
(N / 'evidence/assembly.json').write_text(json.dumps(result, sort_keys=True, indent=2) + '\n')
print(json.dumps({k: v for k, v in result.items() if k not in ('artifact_sha256', 'validation')}, indent=2))
