"""Assemble the guarded canonical tree; regeneration, validation and measurement vs D2."""
import gzip, hashlib, json, pathlib, shutil, subprocess, sys, tarfile
N = pathlib.Path(sys.argv[1])
home = pathlib.Path.home() / 'kicad-port'
V5 = home / 'corrected-review-v5'
ROOT = pathlib.Path('/mnt/c/Users/KarlVonRichter/.t3/worktrees/embeint-kicad-eda/t3-7461f604-ff4777aca1333cd1')
PY = str(pathlib.Path.home() / 'kicad-port-extractor-venv/bin/python')
cur = N / 'current'
F = N / 'final'
G = F / 'port/extractor/generated'
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
if F.exists():
    shutil.rmtree(F)
G.mkdir(parents=True)
with tarfile.open(N / 'units.tar') as tar:
    tar.extractall(F, filter='data')
committed_units = {p.name: sha(p) for p in (F / 'port/units').glob('*.toml')}
for name in ('graph.json.gz', 'inventory.json', 'generated-sources.json'):
    shutil.copyfile(N / 'refreshed' / name, G / name)
shutil.copyfile(N / 'canonical-old/units.json', G / 'units.json')
shutil.copyfile(home / 'source-manifest.json', G / 'source-manifest.json')
shutil.copytree(V5 / 'inputs/sources', G / 'sources')
raw = (N / 'capture/generated-recipes.json').read_bytes()
compressed = bytearray(gzip.compress(raw, compresslevel=9, mtime=0))
compressed[9] = 255
(G / 'generated-recipes.json.gz').write_bytes(compressed)
assert gzip.decompress(bytes(compressed)) == raw
storage = {'storage': 'generated-recipes.json.gz', 'raw_bytes': len(raw), 'raw_sha256': hashlib.sha256(raw).hexdigest(),
           'gzip_bytes': len(compressed), 'gzip_sha256': hashlib.sha256(compressed).hexdigest()}
assert storage['gzip_sha256'] == 'f5a91a1c6f5e2357aeb1db4f6499be3ba7bca7cc3b46fea793c74de42257064e', storage
assert (G / 'generated-recipes.json.gz').read_bytes() == (ROOT / 'port/extractor/generated/generated-recipes.json.gz').read_bytes()
assert storage['raw_sha256'] == json.loads((G / 'generated-sources.json').read_text())['recipes_sha256']
del raw, compressed

tracked = lambda: {str(p.relative_to(F)): sha(p) for p in sorted((F / 'port/units').glob('*.toml'))} | \
    {str((G / n).relative_to(F)): sha(G / n) for n in ('graph.json.gz', 'inventory.json', 'units.json')}
before = tracked()
split = [PY, str(cur / 'port/tools/split_units.py'), '--graph', str(G / 'graph.json'), '--inventory', str(G / 'inventory.json'),
         '--units-dir', str(F / 'port/units'), '--report', str(G / 'units.json'), '--source-root', str(home / 'extract-source')]
iterations = []
for _ in range(2):
    out = subprocess.run(split, check=True, capture_output=True, text=True)
    assert tracked() == before, 'canonical regeneration changed bytes'
    iterations.append(out.stdout)
assert {p.name: sha(p) for p in (F / 'port/units').glob('*.toml')} == committed_units
validation = {}
for label, extra, expected in (('default', [], 0), ('strict', ['--require-complete'], 1)):
    out = subprocess.run([PY, str(cur / 'port/tools/validate_units.py'), '--root', str(F), '--source-root',
                          str(home / 'extract-source'), '--json', *extra], capture_output=True, text=True)
    validation[label] = {'exit': out.returncode, 'result': json.loads(out.stdout)}
    assert out.returncode == expected, validation
assert validation['strict']['result']['errors'] == ['WSL extraction diagnostic incomplete; inspect provenance coverage and parse failures'], validation
measurement = G / 'measurement.json'
subprocess.run([PY, str(cur / 'port/tools/extraction_report.py'), '--raw-output', str(home / 'raw-output'),
                '--reconciled-output', str(home / 'reconciled-output'), '--corrected-output', str(V5 / 'output'),
                '--reconciliation', str(home / 'reconciliation/reconciliation.json'), '--units-dir', str(F / 'port/units'),
                '--units-report', str(G / 'units.json'), '--output', str(measurement)], check=True, capture_output=True, text=True)
old = json.loads((ROOT / 'port/extractor/generated/measurement.json').read_text())
new = json.loads(measurement.read_text())
old_artifacts, new_artifacts = old['canonical'].pop('artifacts'), new['canonical'].pop('artifacts')
assert old == new, 'measurement changed beyond canonical artifact fingerprints'
changed = [a['filename'] for a, b in zip(old_artifacts, new_artifacts) if a != b]
assert changed == ['graph.json.gz', 'inventory.json'], changed
result = {'regeneration_iterations': 2, 'byte_identical_artifacts': len(before), 'unit_files': len(committed_units),
          'unit_files_equal_committed': True, 'split_stdout': iterations, 'validation': validation, 'artifact_sha256': before,
          'measurement_changes_vs_d2': {'changed_canonical_artifacts': changed, 'before': old_artifacts, 'after': new_artifacts,
                                        'all_other_fields_equal': True},
          'recipe_storage': dict(storage, byte_identical_to_d2=True)}
(N / 'evidence/assembly.json').write_text(json.dumps(result, sort_keys=True, indent=2) + '\n')
print(json.dumps({k: result[k] for k in ('regeneration_iterations', 'byte_identical_artifacts', 'measurement_changes_vs_d2')}, indent=2))
