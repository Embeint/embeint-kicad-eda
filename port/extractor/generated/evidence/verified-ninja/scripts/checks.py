"""Windows checks after the guarded recapture: suites, identities, CLI replays and pilot protection."""
import gzip, hashlib, io, json, os, re, runpy, shutil, subprocess, sys, tarfile, tempfile, tomllib
from pathlib import Path
root = Path(r'C:\Users\KarlVonRichter\.t3\worktrees\embeint-kicad-eda\t3-04614140-10140941291c27ae')
os.chdir(root)
W = Path(r'\\wsl.localhost\Ubuntu\home\karlvonrichter\kicad-port\generator-refresh-57e')
G = root / 'port/extractor/generated'
C = G / 'corrected-extraction'
E = G / 'evidence/verified-ninja'
D2 = G / 'evidence/generator-refresh'
BASE = '19035296e129cf8640e89331bc9e8ad84f0fe38c'
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
lf = lambda b: hashlib.sha256(b.replace(b'\r\n', b'\n')).hexdigest()
blob = lambda rev, path: subprocess.check_output(['git', 'cat-file', 'blob', f'{rev}:{path}'])
results = {}

# 1. Suites on Windows; WSL suite result from the workflow log.
for name, command in [
        ('extractor-windows', [sys.executable, '-m', 'unittest', 'discover', '-s', 'port/extractor', '-p', 'test_*.py', '-v']),
        ('progress-tests', [sys.executable, '-m', 'unittest', 'discover', '-s', 'port/site', '-p', 'test_*.py', '-v']),
        ('bootstrap-tests', [sys.executable, '-m', 'unittest', 'discover', '-s', 'port/tools', '-p', 'test_*.py', '-v']),
        ('policy-tests', [sys.executable, '-m', 'unittest', 'discover', '-s', 'rust/kicad-difftest/tests', '-p', 'test_policy.py', '-v']),
        ('progress-build', [sys.executable, 'port/tools/progress.py', '--out', r'C:\Users\KarlVonRichter\.t3\phase0-verified-ninja-site']),
        ('golden-changes', [sys.executable, 'port/tools/check_goldens_policy.py', '--base', 'origin/rust'])]:
    with (E / f'{name}.log').open('w', encoding='utf-8', newline='\n') as log:
        code = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT).returncode
    text = (E / f'{name}.log').read_text(encoding='utf-8')
    results[name] = {'command': command[1:], 'exit_code': code}
    count, skipped = re.search(r'Ran (\d+) tests?', text), re.search(r'OK \(skipped=(\d+)\)', text)
    if count:
        results[name].update(tests=int(count[1]), skipped=int(skipped[1]) if skipped else 0)
    print(name, results[name], flush=True)
    assert code == 0, (name, code)
wsl = gzip.decompress((E / 'logs/extractor-tests-wsl.log.gz').read_bytes()).decode()
count = re.search(r'Ran (\d+) tests', wsl)
assert count and wsl.rstrip().endswith('OK') and 'skipped' not in wsl
results['extractor-wsl'] = {'tests': int(count[1]), 'skipped': 0, 'log': 'logs/extractor-tests-wsl.log.gz'}

# 2. Identities: parse identities preserved, refresh identities recorded, V5 inputs coherent.
graph = json.loads(gzip.decompress((G / 'graph.json.gz').read_bytes()))
p = graph['provenance']
r = p['generator_dependency_refresh']
inventory = json.loads((G / 'inventory.json').read_text())
manifest = json.loads((G / 'generated-sources.json').read_text())
assert inventory['provenance'] == p
assert p['extractor_sha256'] == r['parse_extractor_sha256'] == 'e1548c1fe920a05d882e3f2de1c14843ca0e7b3b04ca9441d4132e16de77b88d'
assert r['parse_generated_sources_manifest_sha256'] == 'a2466270bdf4aec52b7b8267140ac4b18c8295520c5b94e1ecb3dc8498dacb34'
assert r['parse_recipes_sha256'] == 'dff5b79afdbe58e2e4c7a4f0e4d8b093fd34a94a0302d38cddd6483660496951'
assert r['parse_source_inventory_sha256'] == '3d52afc68446cc7e00344409c2fc1ac6908ed5aa554f55d2501126885e5ee175'
assert r['input_graph_sha256'] == '146e2ccac0e8d0ba6d37ceadb26ae5a84d0223609dd55abebb0b5451bfe45735'
assert r['input_inventory_sha256'] == 'b9e6986b36529fdf4041712dfb258af050c2ff61bb3971875394eb16e2a47682'
code = b''.join((root / 'port/extractor' / n).read_bytes() for n in ('refresh_generator_dependencies.py', 'generated_sources.py'))
assert lf(code) == r['refresh_code_sha256'] == r['capture_code_sha256']
assert lf((G / 'generated-sources.json').read_bytes()) == p['generated_sources_manifest_sha256']
assert lf((E / 'generator-dependency-capture.json').read_bytes()) == r['capture_sha256']
assert lf(gzip.decompress((G / 'generated-recipes.json.gz').read_bytes())) == manifest['recipes_sha256'] == r['recipes_sha256']
v5_inventory = json.loads((C / 'inventory.json').read_text())
assert lf((C / 'generated-sources.json').read_bytes()) == v5_inventory['provenance']['generated_sources_manifest_sha256']
assert lf(gzip.decompress((C / 'generated-recipes.json.gz').read_bytes())) == json.loads((C / 'generated-sources.json').read_text())['recipes_sha256']
assert sha(C / 'graph.json.gz') == '40861e9ca678464cedc513561e6e559cbcb5f902bb5b221fe2b6c378fb803e18'
assert sha(C / 'inventory.json') == 'b9e6986b36529fdf4041712dfb258af050c2ff61bb3971875394eb16e2a47682'
for relative in ('port/extractor/generated/corrected-extraction/graph.json.gz', 'port/extractor/generated/corrected-extraction/inventory.json',
                 'port/extractor/generated/units.json', 'port/extractor/generated/source-manifest.json'):
    assert hashlib.sha256(blob(BASE, relative)).hexdigest() == sha(root / relative), relative
for old, new in (('port/extractor/generated/generated-sources.json', C / 'generated-sources.json'),
                 ('port/extractor/generated/generated-recipes.json.gz', C / 'generated-recipes.json.gz')):
    assert hashlib.sha256(blob(BASE, old)).hexdigest() == sha(new), old
identity = r['source_identity']
assert identity['source_files_sha256'] == '3c0d96516f2c20ca5e7883aa7a549ed0e412a190b27af60012a4edfb0621122e'
assert identity['verified_source_files'] == 9655 and identity['source_root'] == '/home/karlvonrichter/kicad-port/extract-source'
assert identity['generation_inputs_sha256'] == lf((E / 'base-generation-input-audit.json').read_bytes())
assert identity['generation_inputs_sha256'] == lf(blob('d2bdddb904', 'port/extractor/generated/evidence/generator-refresh/generation-input-audit.json'))
assert identity['inventory_sha256'] == lf((C / 'inventory.json').read_bytes())
assert identity['tracked_source_inventory_sha256'] == p['tracked_source_inventory_sha256']
assert identity['source_commit'] == p['source_commit'] == '3cd2c9afc6d2749b7d3a79fd1deaa86ac51cb3a7'
captured = json.loads((E / 'generator-dependency-capture.json').read_text())
assert {k: v for k, v in captured['source_identity'].items() if k != 'files'} == identity
d2_capture = json.loads(blob('d2bdddb904', 'port/extractor/generated/evidence/generator-refresh/generator-dependency-capture.json'))
assert captured['generator_inputs'] == d2_capture['generator_inputs']
assert captured['source_generator_dependencies'] == d2_capture['source_generator_dependencies']
d2_files = subprocess.check_output(['git', 'ls-tree', '-r', '--name-only', 'd2bdddb904', '--', 'port/extractor/generated/evidence/generator-refresh'], text=True).split()
for relative in d2_files:
    assert hashlib.sha256(blob('d2bdddb904', relative)).hexdigest() == sha(root / relative), relative
assert len(d2_files) == sum(1 for x in D2.rglob('*') if x.is_file())
results['d2_generator_refresh_evidence_unchanged'] = len(d2_files)
results['identities'] = {'parse_extractor_sha256': p['extractor_sha256'], 'refresh': r,
                         'canonical_graph_sha256': sha(G / 'graph.json.gz'), 'canonical_inventory_sha256': sha(G / 'inventory.json'),
                         'refreshed_manifest_sha256': p['generated_sources_manifest_sha256'],
                         'recipes_gzip_sha256': sha(G / 'generated-recipes.json.gz'),
                         'v5_manifest_and_recipes_preserved_in_corrected_extraction': True}

# 3. CLI postprocess replay on Windows from the pre-refresh canonical blobs.
with tempfile.TemporaryDirectory() as tmp:
    tmp = Path(tmp)
    (tmp / 'graph.json.gz').write_bytes(blob(BASE, 'port/extractor/generated/graph.json.gz'))
    (tmp / 'inventory.json').write_bytes(blob(BASE, 'port/extractor/generated/inventory.json'))
    out = subprocess.run([sys.executable, 'port/extractor/refresh_generator_dependencies.py', 'apply', '--graph', str(tmp / 'graph.json.gz'),
                          '--inventory', str(tmp / 'inventory.json'), '--base-manifest', str(C / 'generated-sources.json'),
                          '--capture', str(E / 'generator-dependency-capture.json'), '--recipes', str(G / 'generated-recipes.json.gz'),
                          '--generation-inputs', str(E / 'base-generation-input-audit.json'),
                          '--output', str(tmp / 'out')], capture_output=True, text=True)
    assert out.returncode == 0, out.stderr
    # Pure apply must not need the WSL source tree: it is not reachable as a POSIX path here.
    assert not Path(identity['source_root']).is_dir()
    replay = {name: sha(tmp / 'out' / name) == sha(G / name) for name in ('graph.json.gz', 'inventory.json', 'generated-sources.json')}
    replay['refresh-report.json'] = sha(tmp / 'out/refresh-report.json') == sha(E / 'refresh-report.json')
    if not replay['graph.json.gz']:
        replay['graph_payload_equal'] = gzip.decompress((tmp / 'out/graph.json.gz').read_bytes()) == gzip.decompress((G / 'graph.json.gz').read_bytes())
    assert all(replay.values()), replay
    results['windows_refresh_cli_replay'] = replay
    # Canonical split regeneration with committed LF unit bytes (the worktree may use CRLF).
    units_tar = tmp / 'units.tar'
    units_tar.write_bytes(subprocess.check_output(['git', '-c', 'core.autocrlf=false', '-c', 'core.eol=lf', 'archive', '--format=tar', BASE, 'port/units']))
    regen = tmp / 'regen'
    with tarfile.open(units_tar) as tar:
        tar.extractall(regen, filter='data')
    gd = regen / 'port/extractor/generated'
    gd.mkdir(parents=True)
    for name in ('graph.json.gz', 'inventory.json', 'units.json'):
        shutil.copyfile(G / name, gd / name)
    shutil.copytree(G / 'sources', gd / 'sources')
    files = lambda: {str(x.relative_to(regen)): sha(x) for x in sorted(regen.joinpath('port/units').glob('*.toml'))} | \
        {n: sha(gd / n) for n in ('graph.json.gz', 'inventory.json', 'units.json')}
    before = files()
    for _ in range(2):
        subprocess.run([sys.executable, 'port/tools/split_units.py', '--graph', str(gd / 'graph.json'), '--inventory', str(gd / 'inventory.json'),
                        '--units-dir', str(regen / 'port/units'), '--report', str(gd / 'units.json'), '--source-root', str(root)],
                       check=True, capture_output=True)
        assert files() == before
    results['windows_canonical_regeneration'] = {'iterations': 2, 'byte_identical_artifacts': len(before)}

# 4. Unit records, pilots and solidcore loader.
units = {path.stem: tomllib.loads(path.read_text(encoding='utf-8')) for path in (root / 'port/units').glob('*.toml')}
changed_units = subprocess.check_output(['git', 'diff', '--name-only', BASE, '--', 'port/units'], text=True).split()
assert not changed_units and len(units) == 1801
expected = {'libs.kimath.src.md5_hash.application.pure.p001': (289, 15, 1),
            'pcb_calculator.application.pure.p001': (1126, 40, 7),
            'pcb_calculator.calculator_panels.application.pure.p001': (1943, 28, 9)}
pilots = {}
for uid, (lines, defs, files_) in expected.items():
    u = units[uid]
    assert (u['est_lines'], len(u['functions']), len(u['upstream'])) == (lines, defs, files_), uid
    assert u['layer'] == 0 and u['wx_class'] == 'pure' and u['status'] == 'todo' and not u['depends_on'] and not u['soft_depends_on'] and not u['coverage_gaps']
    raw = blob(BASE, f'port/units/{uid}.toml')
    assert tomllib.loads(raw.decode()) == u
    pilots[uid] = {'est_lines': lines, 'definitions': defs, 'files': files_, 'toml_blob_sha256': hashlib.sha256(raw).hexdigest(), 'unchanged_from_base': True}
getter = [(usr, fn) for usr, fn in graph['functions'].items() if 'PCB_CALCULATOR_DATAFILE_PARSER::GetReader' in fn['name']]
owner = [uid for uid, u in units.items() if any(usr in u['functions'] for usr, _ in getter)]
assert len(getter) == 1 and owner == ['pcb_calculator.application.thin.replacement.p001'], owner
assert (getter[0][1]['file'], getter[0][1]['start'], getter[0][1]['end']) == ('pcb_calculator/datafile_read_write.h', 55, 55)
assert units[owner[0]]['layer'] == 0 and units[owner[0]]['wx_class'] == 'thin-replacement'
pilots['GetReader'] = {'owner': owner[0], 'location': 'pcb_calculator/datafile_read_write.h:55:55', 'layer': 0, 'wx_class': 'thin-replacement'}
assert all(u['status'] in ('todo', 'blocked') and not u['tests'] and not u['claimed_by'] for u in units.values())
reference = Path(r'C:\Users\KarlVonRichter\.t3\phase0-solidcore-reference')
assert subprocess.check_output(['git', '-C', str(reference), 'rev-parse', 'HEAD'], text=True).strip() == '7a588fa284765a83823293ae20b84da8356ab479'
loader = runpy.run_path(str(reference / 'harness/plan/units.py'))
for uid, unit in units.items():
    assert set(loader['FIELDS']) <= set(unit) and loader['parse_unit'](loader['dump_unit'](unit)) == unit
results['units'] = {'count': len(units), 'changed_from_base': 0, 'statuses': {s: sum(u['status'] == s for u in units.values()) for s in ('todo', 'blocked')},
                    'ported_or_claimed': 0, 'solidcore_roundtrip_units': len(units), 'pilots': pilots}

# 5. Measurement, protected metrics and blob limits.
sys.path.insert(0, str(root / 'port/tools'))
from extraction_report import measure_units
measurement = json.loads((G / 'measurement.json').read_text())
assert measure_units(list(units.values()), json.loads((G / 'units.json').read_text())) == measurement['units']
m = measurement['units']
protected = {'unit_count': 1801, 'accounted_lines': 2380947, 'layers': 392, 'hard_edge_count': 46894, 'back_edge_count': 9548,
             'back_edge_weight': 106538, 'wx': {'pure': (553, 666688), 'thin-replacement': (101, 53714), 'UI': (1147, 1660545)}}
assert (m['unit_count'], m['accounted_lines'], len(m['layers']), m['hard_edge_count'], m['back_edge_count'], m['back_edge_weight']) == \
    (1801, 2380947, 392, 46894, 9548, 106538)
assert {k: (v['units'], v['lines']) for k, v in m['wx_classes'].items()} == protected['wx']
corrected = measurement['runs']['corrected']
assert (corrected['functions'], corrected['build_generated_definitions'], corrected['parsed_ok'], corrected['commands']) == (107254, 37838, 2507, 2511)
assert (measurement['runs']['raw']['parsed_ok'], measurement['runs']['raw']['commands']) == (2462, 2471)
assert len(corrected['uncompiled_application_sources']) == 63 and m['statuses'] == {'blocked': 64, 'todo': 1737}
for record in measurement['canonical']['artifacts']:
    assert sha(G / record['filename']) == record['sha256'] and (G / record['filename']).stat().st_size == record['bytes']
for path in G.rglob('*'):
    if path.is_file():
        assert path.stat().st_size < 100_000_000, path
results['metrics'] = {'protected_unit_metrics_unchanged': protected, 'definitions': 107254, 'generated_definitions': 37838,
                      'raw': '2462/2471', 'corrected': '2507/2511', 'uncompiled_sources': 63, 'blocked_units': 64,
                      'largest_generated_blob_bytes': max(x.stat().st_size for x in G.rglob('*') if x.is_file())}
results['head'] = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
(E / 'checks.json').write_text(json.dumps(results, sort_keys=True, indent=2) + '\n', encoding='utf-8', newline='\n')
print(json.dumps({k: v for k, v in results.items() if k not in ('identities',)}, indent=1)[:5000])
